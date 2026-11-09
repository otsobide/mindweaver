//! The run under way, as the backend carrying it out reaches whoever
//! follows it: what it reports of how it goes, and what asking it to stop
//! does (#112).
//!
//! A run is one task's lines, carried out on the writer thread, and most
//! are the local disk, over before anybody could read a count. A gather is
//! the network, and so will a backup be: long enough that somebody wants to
//! know how it goes, and to change their mind. What it reports and what a
//! cancel does are the backend's own, since only the front end knows what a
//! step of one of its commands is and how one stops; the server carries a
//! report to the panel unread, as JSON, and hands a cancel over to whatever
//! the backend said one does. So the server names no step of a gather's,
//! and a backup's phases, or a child process killed, plug in with nothing
//! here changed.

use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What a cancel does, as the backend said it.
type Hook = Arc<dyn Fn() + Send + Sync>;

/// One run under way, handed to [`Backend::run`](crate::Backend::run) beside
/// its task: where to report how it goes ([`report`](Self::report)), and
/// where to say what asking it to stop does ([`on_cancel`](Self::on_cancel)).
///
/// Cloned, it is the same run: the writer thread keeps one, which a cancel
/// reaches from the request that asks for it, while the backend holds the
/// other. A struct of the server's own rather than a trait the backend
/// implements, so a backend run outside a server, as a test drives one,
/// makes one with [`new`](Self::new) or [`unfollowed`](Self::unfollowed).
#[derive(Clone)]
pub struct Underway {
    run: u64,
    report: Arc<dyn Fn(Value) + Send + Sync>,
    stop: Arc<Mutex<Stop>>,
    /// Set once the backend's call has returned, after which nothing it
    /// reports reaches anybody: a step told after the run's end would say
    /// it is still going.
    over: Arc<AtomicBool>,
}

/// Whether the run was asked to stop, and what that does.
#[derive(Default)]
struct Stop {
    asked: bool,
    hook: Option<Hook>,
}

impl Underway {
    /// Run number `run`, each report handed to `report`.
    pub fn new(run: u64, report: impl Fn(Value) + Send + Sync + 'static) -> Self {
        Underway {
            run,
            report: Arc::new(report),
            stop: Arc::default(),
            over: Arc::default(),
        }
    }

    /// A run nobody follows, numbered 0: what it reports goes nowhere, and
    /// only a clone of it asks it to stop.
    pub fn unfollowed() -> Self {
        Self::new(0, |_| {})
    }

    /// Its number: counted from 1 by the server that runs it, in the order
    /// the runs started, as `command_started` tells it.
    pub fn run(&self) -> u64 {
        self.run
    }

    /// Tell whoever follows the run how it goes, in the backend's own JSON,
    /// which the server carries unread (`progress`); nothing once the
    /// backend's call has returned.
    ///
    /// Each report is one event to every subscriber, and the bus keeps
    /// [`CAPACITY`](crate::CAPACITY) of them for one that has not read
    /// them: a backend reports when a step changes, and no faster than a
    /// person reads, or a burst tells every subscriber to read everything
    /// again in place of what it missed.
    pub fn report(&self, progress: Value) {
        if !self.over.load(Ordering::Acquire) {
            (self.report)(progress);
        }
    }

    /// What asking the run to stop does, from now on: called once a cancel
    /// is asked, and at once when one was asked already, so a cancel that
    /// arrives before the backend says what one does is not lost.
    ///
    /// The backend's to say, since only it knows how one of its commands
    /// stops: a flag a gather checks between its steps, or a child process
    /// killed. Called on whatever thread asked, never the run's own, so it
    /// is to return at once, and leave the stopping to the run.
    pub fn on_cancel(&self, hook: impl Fn() + Send + Sync + 'static) {
        let hook: Hook = Arc::new(hook);
        let asked = {
            let mut stop = self.lock();
            stop.hook = Some(Arc::clone(&hook));
            stop.asked
        };
        if asked {
            hook();
        }
    }

    /// Ask the run to stop: what `POST /api/v1/tasks/cancel` does, and what
    /// a server stopping does. Whether it stops, and where, is the run's;
    /// how it finishes says. Safe from any thread, and as often as wanted:
    /// what a cancel does is done each time.
    pub fn cancel(&self) {
        let hook = {
            let mut stop = self.lock();
            stop.asked = true;
            stop.hook.clone()
        };
        if let Some(hook) = hook {
            hook();
        }
    }

    /// Whether it was asked to stop, for a backend that checks rather than
    /// say what a cancel does.
    pub fn is_cancelled(&self) -> bool {
        self.lock().asked
    }

    /// The backend's call has returned: nothing it reports from now on
    /// reaches anybody.
    pub(crate) fn end(&self) {
        self.over.store(true, Ordering::Release);
    }

    /// Ask the run to stop, with a panic of the backend's hook caught:
    /// whether it was caught, so the request that asked says the backend
    /// failed rather than the server's thread going with it.
    pub(crate) fn cancel_caught(&self) -> bool {
        panic::catch_unwind(AssertUnwindSafe(|| self.cancel())).is_ok()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Stop> {
        self.stop.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl std::fmt::Debug for Underway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Underway")
            .field("run", &self.run)
            .field("cancelled", &self.is_cancelled())
            .field("over", &self.over.load(Ordering::Acquire))
            .finish()
    }
}

/// What `POST /api/v1/tasks/cancel` is sent: the run to stop, by its number
/// as `command_started` told it, `{"run": 3}`.
///
/// By its number rather than "whatever runs now", so a cancel that crosses
/// the end of the run it was meant for never stops the one queued behind
/// it: by then that one has a number of its own. A field it does not have
/// is refused, as a task's is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Cancel {
    /// The run's number, as the events tell it.
    pub run: u64,
}

/// What a cancel is answered with: the run asked to stop and its lines,
/// `{"run": 3, "lines": [...]}`. Whether it stopped is for how it finishes
/// to say: a run past the point where it can stop runs to its end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Cancelling {
    /// The run's number.
    pub run: u64,
    /// Its lines, each as somebody would type it.
    pub lines: Vec<String>,
}

#[cfg(test)]
#[path = "underway_test.rs"]
mod tests;
