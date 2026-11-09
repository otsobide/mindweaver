//! What the panel is told as it happens.
//!
//! A bus every subscriber hears all of: a tokio broadcast channel, which
//! hands each event to every receiver and keeps the latest few for one that
//! has not read them. Free of axum, as the watcher is, so a desktop shell
//! answering the panel without HTTP (D-17) takes its events from the same
//! bus; the stream at `/api/v1/events` is only one way of carrying them.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use serde::Serialize;
use tokio::sync::broadcast::{self, error::RecvError};

/// How many events wait for a subscriber that has not read them yet before
/// it is told it fell behind ([`Event::Resync`]).
///
/// Enough for the bursts there are, a command's start and end, a change or
/// two and the steps a run reports as it goes, many times over; a
/// subscriber further behind than this has stopped reading, and what it
/// needs then is one word to ask for everything again, not the backlog.
pub const CAPACITY: usize = 64;

/// One thing the panel is told.
///
/// Tagged by its name in snake_case beside its fields, as a task and a
/// query are: `{"type": "changed", "scopes": [...]}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Something a backend watches changed on disk, so what the panel shows
    /// of each scope may be out of date: one event for every change the
    /// watcher took in together, each scope once, in the backend's words.
    Changed { scopes: Vec<serde_json::Value> },
    /// A task's lines started to run on the writer thread, as run `run`:
    /// counted from 1 in the order runs start, the number a cancel names
    /// it by. With whether it takes a cancel, as the backend's preview
    /// said ([`Preview::cancellable`]), so a panel offers one for a run
    /// another tab or a script sent too.
    ///
    /// [`Preview::cancellable`]: crate::Preview::cancellable
    CommandStarted {
        run: u64,
        lines: Vec<String>,
        cancellable: bool,
    },
    /// How run `run` is going, as the backend reported it
    /// ([`Underway::report`]): its own JSON, carried unread, as a changed
    /// path's scope is.
    ///
    /// [`Underway::report`]: crate::Underway::report
    Progress {
        run: u64,
        progress: serde_json::Value,
    },
    /// Run `run` was asked to stop (`POST /api/v1/tasks/cancel`), from
    /// this tab, another or a script: whether it stopped, and where, its
    /// finish and its outcome say. Told once, however often it is asked.
    Cancelling { run: u64 },
    /// The writer thread is busy with run `run`, so whatever is sent now
    /// waits its turn behind it: told when a run or a switch is queued
    /// while a run is under way, and to a subscriber as it subscribes
    /// while one is, with the run's latest [`Progress`](Event::Progress)
    /// after it. With its lines, whether it takes a cancel and whether it
    /// was asked to stop, as its start and any cancel said.
    Busy {
        run: u64,
        lines: Vec<String>,
        cancellable: bool,
        cancelling: bool,
    },
    /// Run `run` stopped, however that went: the reply to the run says how.
    CommandFinished { run: u64, lines: Vec<String> },
    /// The backend acts on something else since ([`Backend::switch`]),
    /// and what it names is watched: everything is to be asked for again,
    /// and nothing shown before kept, since it was of what it acted on
    /// before. Told on the writer thread, once the switch is taken, to
    /// every subscriber, the tab that asked for it among them
    /// ([`Events::switched`]), with how many switches were taken since the
    /// bus was made, this one counted.
    ///
    /// [`Backend::switch`]: crate::Backend::switch
    Switched { switches: u64 },
    /// The subscriber may have missed something: it has just connected, or
    /// it fell behind. Everything is to be asked for again.
    ///
    /// With how many switches were taken since the bus was made, as the
    /// latest [`Switched`](Event::Switched) said: a subscriber that missed
    /// one, a tab whose stream was let go while it was hidden or one that
    /// fell behind, learns it from a count other than the one it heard last,
    /// and lets go of what it shows of what the backend acted on before as
    /// it would on hearing `Switched`. A count rather than a word that a
    /// switch happened, which only a bus that knew what each subscriber had
    /// heard could say.
    Resync { switches: u64 },
    /// Every path the backend named is watched, so a change to one arrives
    /// as [`Changed`](Event::Changed).
    Watching,
    /// Not every one is, so a change there arrives as nothing, and what the
    /// panel shows is refreshed by hand: the system's limit on watches was
    /// reached, or a watch could not be set up at all.
    Unwatched { reason: String },
}

impl Event {
    /// Whether it says how things are watched, which a subscriber is told
    /// on its own as soon as it subscribes.
    fn is_watch(&self) -> bool {
        matches!(self, Event::Watching | Event::Unwatched { .. })
    }
}

/// What goes through the channel: an event, or the word that it is closed.
#[derive(Clone)]
enum Message {
    Event(Event),
    Closed,
}

/// The bus: publish to it, subscribe to it. Cloned, it is the same bus.
#[derive(Clone)]
pub struct Events {
    inner: Arc<Inner>,
}

struct Inner {
    sender: broadcast::Sender<Message>,
    /// Behind one lock with the send, so a subscriber is never told how
    /// things are watched twice over or not at all.
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The latest [`Event::Watching`] or [`Event::Unwatched`]; none until
    /// something watches.
    watch: Option<Event>,
    /// How many switches were told, as the latest [`Event::Switched`] says:
    /// what every [`Event::Resync`] carries.
    switches: u64,
    /// The run under way, as its start, its progress and any cancel told
    /// it, until its finish is told; none between runs.
    running: Option<Running>,
    closed: bool,
}

/// A run under way, as the events about it told it: what [`Event::Busy`]
/// says of it, and the latest step it reported.
#[derive(Clone)]
struct Running {
    run: u64,
    lines: Vec<String>,
    cancellable: bool,
    cancelling: bool,
    progress: Option<serde_json::Value>,
}

impl Running {
    /// That the writer is busy with it.
    fn busy(&self) -> Event {
        Event::Busy {
            run: self.run,
            lines: self.lines.clone(),
            cancellable: self.cancellable,
            cancelling: self.cancelling,
        }
    }

    /// What a subscriber is told of it on its own: that the writer is busy
    /// with it, then the latest step it reported, if it reported one.
    fn told(&self) -> Vec<Event> {
        let progress = self.progress.clone().map(|progress| Event::Progress {
            run: self.run,
            progress,
        });
        std::iter::once(self.busy()).chain(progress).collect()
    }
}

impl State {
    /// Send `event`, keeping what a subscriber is told on its own beside
    /// it; nothing once the bus is closed.
    fn send(&mut self, sender: &broadcast::Sender<Message>, event: Event) {
        if self.closed {
            return;
        }
        if event.is_watch() {
            self.watch = Some(event.clone());
        }
        self.follow(&event);
        // An error only says nobody is listening, which is no failure.
        let _ = sender.send(Message::Event(event));
    }

    /// Keep what `event` says of the switches and of the run under way.
    /// What it says of a run other than the one under way, a step reported
    /// too late or a cancel of one already over, changes nothing.
    fn follow(&mut self, event: &Event) {
        let under_way = self.running.as_ref().map(|running| running.run);
        match event {
            Event::Switched { switches } => self.switches = *switches,
            Event::CommandStarted {
                run,
                lines,
                cancellable,
            } => {
                self.running = Some(Running {
                    run: *run,
                    lines: lines.clone(),
                    cancellable: *cancellable,
                    cancelling: false,
                    progress: None,
                });
            }
            Event::Progress { run, progress } if under_way == Some(*run) => {
                if let Some(running) = &mut self.running {
                    running.progress = Some(progress.clone());
                }
            }
            Event::Cancelling { run } if under_way == Some(*run) => {
                if let Some(running) = &mut self.running {
                    running.cancelling = true;
                }
            }
            Event::CommandFinished { run, .. } if under_way == Some(*run) => self.running = None,
            Event::Changed { .. }
            | Event::Progress { .. }
            | Event::Cancelling { .. }
            | Event::Busy { .. }
            | Event::CommandFinished { .. }
            | Event::Resync { .. }
            | Event::Watching
            | Event::Unwatched { .. } => {}
        }
    }
}

impl Default for Events {
    fn default() -> Self {
        let (sender, _) = broadcast::channel(CAPACITY);
        Events {
            inner: Arc::new(Inner {
                sender,
                state: Mutex::default(),
            }),
        }
    }
}

impl Events {
    pub fn new() -> Self {
        Self::default()
    }

    /// Tell every subscriber, if there is one; nothing once the bus is
    /// closed. How things are watched is kept besides, for whoever
    /// subscribes next, and so is the count a switch carries.
    pub fn publish(&self, event: Event) {
        self.lock().send(&self.inner.sender, event);
    }

    /// Tell every subscriber the backend acts on something else since
    /// ([`Event::Switched`]), counted one more than the switch before it,
    /// under the lock a subscription is taken under, so a subscriber hears
    /// the switch or is told its count by [`Event::Resync`], never neither.
    pub fn switched(&self) {
        let mut state = self.lock();
        let switches = state.switches + 1;
        state.send(&self.inner.sender, Event::Switched { switches });
    }

    /// Tell every subscriber the writer thread is busy with the run under
    /// way ([`Event::Busy`]), if one is: what queuing a run or a switch
    /// does, so whoever sent it learns what it waits behind, rather than
    /// take a wait of minutes for a server that hangs.
    ///
    /// Of the run as the events told it, under the lock its finish is told
    /// under, so a run is never said to keep the writer busy once it is
    /// told finished.
    pub fn busy(&self) {
        let mut state = self.lock();
        if let Some(busy) = state.running.as_ref().map(Running::busy) {
            state.send(&self.inner.sender, busy);
        }
    }

    /// Hear every event from now on, after [`Event::Resync`], since
    /// whatever happened before was missed, how things are watched, once
    /// something has said so, and the run under way, if one is
    /// ([`Event::Busy`], then its latest step).
    pub fn subscribe(&self) -> Subscription {
        let state = self.lock();
        let receiver = self.inner.sender.subscribe();
        let mut pending = VecDeque::new();
        if !state.closed {
            pending.push_back(Event::Resync {
                switches: state.switches,
            });
            pending.extend(state.watch.clone());
            pending.extend(state.running.iter().flat_map(Running::told));
        }
        Subscription {
            receiver,
            pending,
            events: self.clone(),
            done: state.closed,
        }
    }

    /// End every subscription, and every one taken from now on: for a
    /// server that is stopping, whose streams would otherwise hold it open
    /// for as long as a browser keeps the page.
    pub fn close(&self) {
        let mut state = self.lock();
        state.closed = true;
        let _ = self.inner.sender.send(Message::Closed);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.inner
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// How things are watched now, if anything has said, how many
    /// switches were told, and what a subscriber is told of the run under
    /// way, if one is.
    fn now(&self) -> (Option<Event>, u64, Vec<Event>) {
        let state = self.lock();
        let running = state.running.iter().flat_map(Running::told).collect();
        (state.watch.clone(), state.switches, running)
    }
}

/// One subscriber's hearing of the bus.
pub struct Subscription {
    receiver: broadcast::Receiver<Message>,
    /// What it is told before the channel's next event.
    pending: VecDeque<Event>,
    events: Events,
    done: bool,
}

impl Subscription {
    /// The next event, waiting for one; none once the bus is closed.
    ///
    /// One that fell behind by more than [`CAPACITY`] is told
    /// [`Event::Resync`] in place of what it missed, with the count of
    /// switches as it stands, how things are watched and the run under way
    /// again, in case any was among it; then the events the channel still
    /// holds, a switch among them carrying a count no higher than that one.
    pub async fn next(&mut self) -> Option<Event> {
        if let Some(event) = self.pending.pop_front() {
            return Some(event);
        }
        if self.done {
            return None;
        }
        match self.receiver.recv().await {
            Ok(Message::Event(event)) => Some(event),
            Ok(Message::Closed) | Err(RecvError::Closed) => {
                self.done = true;
                None
            }
            Err(RecvError::Lagged(_)) => {
                let (watch, switches, running) = self.events.now();
                self.pending.extend(watch);
                self.pending.extend(running);
                Some(Event::Resync { switches })
            }
        }
    }
}

#[cfg(test)]
#[path = "events_test.rs"]
mod tests;
