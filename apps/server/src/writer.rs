//! The one thread every change runs on, and every switch is made on.

use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{mpsc, Arc, Mutex, PoisonError};
use std::thread;
use std::time::Instant;

use serde_json::json;
use tokio::sync::oneshot;

use crate::code::Condition;
use crate::events::{Event, Events};
use crate::log::{Log, Planned};
use crate::transcript::{Ran, Transcript};
use crate::underway::{Cancelling, Underway};
use crate::watcher::Watcher;
use crate::{Backend, Refusal};

/// What came of a task: what it ran, or why the backend ran none of it;
/// and, around that, why the thread could not say, when it could not.
type Done = Result<Result<Ran, Refusal>, Failed>;

/// What came of a switch: what the backend acts on now, as it says it, or
/// why it switched to nothing; and, around that, why the thread could not
/// say, when it could not.
type Switched = Result<Result<serde_json::Value, Refusal>, Failed>;

/// Why the thread could not say how a job went: the condition, a 500 of the
/// server's own, and the words for it, which are fixed, since what failed
/// left nothing to say of itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Failed {
    pub(crate) condition: Condition,
    pub(crate) message: &'static str,
}

/// What waits its turn on the thread, when it was queued, and where to say
/// how it went: a task to run, or a switch of what the backend acts on,
/// which takes its turn as a task does, so it never lands while one runs.
enum Job<B: Backend> {
    Run {
        task: B::Task,
        sent: Instant,
        reply: oneshot::Sender<Done>,
    },
    Switch {
        to: B::Switch,
        sent: Instant,
        reply: oneshot::Sender<Switched>,
    },
}

/// The queue in front of the thread that runs every task.
///
/// One thread fed by a channel, not tokio's blocking pool behind a lock. A
/// channel is first in, first out, so tasks run in the order they arrived; a
/// lock promises no order to whoever waits on it, and the panel shows a queue
/// of lines that must run as shown. One thread also means a write never runs
/// beside another, which is what the commands underneath were written for:
/// they come from a CLI, where one command at a time is all there ever is.
/// And a long write holds that thread, not one of the pool's, so queries go
/// on being answered while it runs.
pub(crate) struct Writer<B: Backend> {
    queue: mpsc::Sender<Job<B>>,
    /// The run on the thread now, which a cancel reaches.
    current: Current,
    /// Told that the thread is busy whenever a job is queued while a run
    /// is under way.
    events: Events,
}

impl<B: Backend> Writer<B> {
    /// Start the thread, recording every run in `transcript`, telling
    /// `events` as each starts, goes and finishes and as a switch is taken,
    /// keeping the run under way in `current` for a cancel to reach,
    /// having `watcher` watch what the backend names after a switch, and
    /// saying in `log`, when there is one, how long each job waited its
    /// turn and took. It lives as long as somebody can still queue a task,
    /// and ends when the last router holding this writer is dropped.
    pub(crate) fn start(
        backend: Arc<B>,
        transcript: Transcript,
        events: Events,
        current: Current,
        watcher: Arc<Watcher>,
        log: Option<Arc<Log>>,
    ) -> io::Result<Self> {
        let (queue, jobs) = mpsc::channel::<Job<B>>();
        let writer = Self {
            queue,
            current: current.clone(),
            events: events.clone(),
        };
        thread::Builder::new()
            .name(String::from("mindweaver-writes"))
            .spawn(move || {
                // Nobody may be waiting any more: a client that gives up
                // does not take its task back, which has already run.
                let log = log.as_deref();
                // How many runs started, the one starting now counted: its
                // number, from 1.
                let mut started = 0;
                for job in jobs {
                    match job {
                        Job::Run { task, sent, reply } => {
                            let took = Took { sent, log };
                            let on = On {
                                transcript: &transcript,
                                events: &events,
                                current: &current,
                                started: &mut started,
                            };
                            let _ = reply.send(perform(&*backend, task, on, took));
                        }
                        Job::Switch { to, sent, reply } => {
                            let took = Took { sent, log };
                            let _ = reply.send(switch(&*backend, to, &watcher, &events, took));
                        }
                    }
                }
            })?;
        Ok(writer)
    }

    /// Queue a task behind every one that arrived before it, and wait for it
    /// to have run, or to have been refused.
    pub(crate) async fn run(&self, task: B::Task) -> Done {
        let (reply, done) = oneshot::channel();
        let sent = Instant::now();
        self.events.busy();
        self.queue
            .send(Job::Run { task, sent, reply })
            .map_err(|_| stopped())?;
        done.await.map_err(|_| stopped())?
    }

    /// Queue a switch behind every task that arrived before it, and wait
    /// for it to have been taken, or refused.
    pub(crate) async fn switch(&self, to: B::Switch) -> Switched {
        let (reply, done) = oneshot::channel();
        let sent = Instant::now();
        self.events.busy();
        self.queue
            .send(Job::Switch { to, sent, reply })
            .map_err(|_| stopped())?;
        done.await.map_err(|_| stopped())?
    }

    /// Ask run `run` to stop, if it is the one under way and takes a
    /// cancel, and say which run that was; or why not. Off the thread,
    /// which the run holds: the backend's hook is called where it is
    /// asked, on tokio's blocking pool, as every call of the backend's
    /// is off the runtime.
    pub(crate) async fn cancel(&self, run: u64) -> Result<Cancelling, Uncancelled> {
        let current = self.current.clone();
        let events = self.events.clone();
        tokio::task::spawn_blocking(move || current.cancel(run, &events))
            .await
            .unwrap_or(Err(Uncancelled::Failed))
    }
}

/// What the thread works with as a run goes: where it is recorded, told
/// and kept for a cancel, and how many started before it.
struct On<'a> {
    transcript: &'a Transcript,
    events: &'a Events,
    current: &'a Current,
    started: &'a mut u64,
}

/// The run under way on the writer thread, if one is: shared with every
/// request that asks to cancel it, and with a server stopping, which asks
/// it to stop too. Cloned, it is the same.
#[derive(Clone, Default)]
pub(crate) struct Current(Arc<Mutex<Option<Running>>>);

/// One run under way: its number, its lines, whether its preview said it
/// takes a cancel, whether one was asked, and the run as the backend holds
/// it, which the cancel reaches.
struct Running {
    run: u64,
    lines: Vec<String>,
    cancellable: bool,
    cancelling: bool,
    underway: Underway,
}

/// Why a cancel was not passed on to a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Uncancelled {
    /// The run it names is not under way: over, not started, or none, and
    /// the run that is under way if one is, which the words say.
    NotRunning { under_way: Option<u64> },
    /// It is under way, and takes no cancel.
    Uncancellable,
    /// What the backend said a cancel does panicked.
    Failed,
}

impl Current {
    /// Ask run `run` to stop, as [`Writer::cancel`] says.
    ///
    /// Checked, and told to every subscriber as `cancelling` the first time
    /// it is asked, under the lock the run is taken off under once it is
    /// over, so a cancel is never told of a run already told finished; the
    /// backend's hook is called once the lock is let go, so a hook that
    /// takes its time holds up nothing but the request that asked.
    fn cancel(&self, run: u64, events: &Events) -> Result<Cancelling, Uncancelled> {
        let (underway, cancelling) = {
            let mut current = self.lock();
            let under_way = current.as_ref().map(|running| running.run);
            let running = match current.as_mut() {
                Some(running) if running.run == run => running,
                _ => return Err(Uncancelled::NotRunning { under_way }),
            };
            if !running.cancellable {
                return Err(Uncancelled::Uncancellable);
            }
            if !running.cancelling {
                running.cancelling = true;
                events.publish(Event::Cancelling { run });
            }
            let cancelling = Cancelling {
                run,
                lines: running.lines.clone(),
            };
            (running.underway.clone(), cancelling)
        };
        if underway.cancel_caught() {
            Ok(cancelling)
        } else {
            Err(Uncancelled::Failed)
        }
    }

    /// Ask the run under way to stop, if one is and it takes a cancel: what
    /// a server stopping does, as leaving the TUI stops its gather, so a
    /// stop does not wait as long as the network does. Told as any cancel
    /// is.
    pub(crate) fn cancel_any(&self, events: &Events) {
        let run = self.lock().as_ref().map(|running| running.run);
        if let Some(run) = run {
            let _ = self.cancel(run, events);
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Running>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The thread has stopped, so nothing queued on it is answered.
fn stopped() -> Failed {
    Failed {
        condition: Condition::WriterStopped,
        message: "the thread that runs tasks has stopped",
    }
}

/// When a job was queued, and the log to say how long it waited and took
/// in, if the front end gave one.
///
/// Said around the backend's calls rather than worked out from the reply:
/// a request line says only how long the whole answer took, and a run that
/// took seconds could have waited behind another, been slow to work out,
/// or been slow to run, which a log says apart (#240).
#[derive(Clone, Copy)]
struct Took<'a> {
    sent: Instant,
    log: Option<&'a Log>,
}

/// The backend panicked, as `message` says where, so there is no answer
/// to give.
fn panicked(message: &'static str) -> Failed {
    Failed {
        condition: Condition::BackendFailed,
        message,
    }
}

/// Run one task: its lines first, worked out on this thread, so they are the
/// lines of the disk the run itself meets, after every write before it.
/// A task whose lines the backend refuses is never handed to it to run, and
/// nobody is told it started, since nothing did.
///
/// One that runs is told to every subscriber as it starts and as it
/// finishes, however it finishes, a refusal or a panic included, so a panel
/// showing it running never waits for an end that is not coming. Finished
/// is told once the run is in the transcript, as the reply is sent, so
/// whoever asks for the transcript on hearing it finds the run there.
///
/// Between the two it is the run under way: numbered, one more than the
/// run before it, kept where a cancel reaches it, and handed to the backend
/// as an [`Underway`] whose every report is told as `progress`, until the
/// backend's call returns, after which nothing it reports is told. It stops
/// being the run under way before its finish is told, so a cancel that
/// crosses the finish is refused as of a run over, never told as
/// `cancelling` after it.
///
/// Said in the log, when there is one, once its lines are worked out, with
/// how long it waited its turn and how long they took to work out, and
/// again once they have run, however that went, with how long they took
/// ([`Took`]): the first before any line runs, so a run that never ends
/// is said to have started.
///
/// A backend that panics has a bug, but the thread outlives it so the
/// tasks queued behind can still run. An outcome that will not serialize is
/// still a run that happened, so it is recorded as an error beside the
/// lines that ran rather than raised: raised, it would leave the transcript
/// a run short, and the client a bare 500 for lines that may well have
/// changed the disk.
fn perform<B: Backend>(backend: &B, task: B::Task, on: On<'_>, took: Took<'_>) -> Done {
    let On {
        transcript,
        events,
        current,
        started,
    } = on;
    let failed = || panicked("the backend panicked while running the task");
    let queued = took.sent.elapsed();
    let working = Instant::now();
    let planned = panic::catch_unwind(AssertUnwindSafe(|| backend.preview(&task)));
    if let Some(log) = took.log {
        let planned = match &planned {
            Ok(Ok(preview)) => Planned::Lines(preview.lines.len()),
            Ok(Err(_)) => Planned::Refused,
            Err(_) => Planned::Panicked,
        };
        log.planned(queued, working.elapsed(), planned);
    }
    let (lines, cancellable) = match planned {
        Ok(Ok(preview)) => (preview.lines, preview.cancellable),
        // A refused task ran nothing, so it is no run to replay.
        Ok(Err(refusal)) => return Ok(Err(refusal)),
        Err(_) => return Err(failed()),
    };
    *started += 1;
    let run = *started;
    let underway = Underway::new(run, {
        let events = events.clone();
        move |progress| events.publish(Event::Progress { run, progress })
    });
    // Kept before it is told, so a cancel sent on hearing it started
    // finds it.
    *current.lock() = Some(Running {
        run,
        lines: lines.clone(),
        cancellable,
        cancelling: false,
        underway: underway.clone(),
    });
    events.publish(Event::CommandStarted {
        run,
        lines: lines.clone(),
        cancellable,
    });
    let running = Instant::now();
    let ran = panic::catch_unwind(AssertUnwindSafe(|| backend.run(task, &underway)));
    underway.end();
    if let Some(log) = took.log {
        log.ran(running.elapsed(), lines.len());
    }
    let done = match ran {
        Ok(Ok(outcome)) => {
            let outcome = serde_json::to_value(outcome).unwrap_or_else(
                |error| json!({ "error": format!("the backend's outcome is not JSON: {error}") }),
            );
            let ran = Ran {
                lines: lines.clone(),
                outcome,
            };
            transcript.record(ran.clone());
            Ok(Ok(ran))
        }
        Ok(Err(refusal)) => Ok(Err(refusal)),
        Err(_) => Err(failed()),
    };
    *current.lock() = None;
    events.publish(Event::CommandFinished { run, lines });
    done
}

/// Switch what the backend acts on, and, once it is taken, have the watcher
/// watch what it names now, then tell every subscriber, in that order: so
/// whatever the panel reads on hearing it, every change to it is watched
/// for.
///
/// Told for a backend that panics too, which may have switched before it
/// did: reading everything again is how the panel finds out, and costs
/// nothing when nothing moved. Not for a refusal, which switched nothing.
fn switch<B: Backend>(
    backend: &B,
    to: B::Switch,
    watcher: &Watcher,
    events: &Events,
    took: Took<'_>,
) -> Switched {
    let queued = took.sent.elapsed();
    let switching = Instant::now();
    let switched = panic::catch_unwind(AssertUnwindSafe(move || backend.switch(to)));
    if !matches!(switched, Ok(Err(_))) {
        watcher.again();
        events.switched();
    }
    if let Some(log) = took.log {
        log.switched(queued, switching.elapsed());
    }
    switched.map_err(|_| panicked("the backend panicked while switching"))
}
