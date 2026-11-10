//! Following a run while it goes, and stopping it: a gather, or a backup.
//!
//! Either can take as long as the network does, so somebody may want to know
//! how it is going, and to change their mind. The front end hands it a
//! [`Watch`]: a callback it is told each step through, and a flag that
//! cancels it. Both are core's own types rather than a library's progress
//! tree or a process handle, so a front end, the server that streams them
//! included, depends on neither `gix`'s version nor how a backup reaches
//! `git`.
//!
//! What a step is, and until which one a cancel is honoured, is each run's
//! to say ([`gather::Progress`], [`backup::Progress`]): one watch, generic
//! over its step, rather than one type of watch for each, so a front end
//! that follows several kinds of run holds one flag and one sink, and hands
//! each run the watch it [relays](Watch::relaying) through.
//!
//! [`gather::Progress`]: crate::gather::Progress
//! [`backup::Progress`]: crate::backup::Progress

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// How a front end follows a run and stops it: where each step `P` goes, and
/// the flag that cancels it.
///
/// A plain callback rather than a channel, so the front end chooses how the
/// news travels: the CLI writes a line, the TUI sends it to the thread that
/// draws, and a server can stream it. It is called from the thread the run
/// runs on and, while `gix` fetches, from one the gather starts to read
/// `gix`'s counts, which is why it must be `Send` and `Sync`.
///
/// Clones share the flag, so a front end keeps one to [`cancel`](Self::cancel)
/// from wherever its keys are read, and so does every watch
/// [relaying](Self::relaying) to it. A run says when a cancel is still
/// honoured: a gather until it starts putting things on the shelf, and a
/// backup until it starts pushing.
pub struct Watch<P> {
    sink: Option<Arc<dyn Fn(P) + Send + Sync>>,
    cancelled: Arc<AtomicBool>,
}

impl<P> Watch<P> {
    /// A watch that reports nothing and that nothing has cancelled yet: what a
    /// run nobody follows is given.
    pub fn new() -> Self {
        Self {
            sink: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Report each step of the run to `sink`.
    pub fn reporting_to(mut self, sink: impl Fn(P) + Send + Sync + 'static) -> Self {
        self.sink = Some(Arc::new(sink));
        self
    }

    /// Ask the run to stop. Safe from any thread, and as often as wanted.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Whether [`cancel`](Self::cancel) has been called on this watch, a
    /// clone of it, or a watch it relays to or that relays to it.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// The flag itself, for `gix` to stop at.
    pub(crate) fn flag(&self) -> &AtomicBool {
        &self.cancelled
    }

    /// Tell the sink, if there is one.
    ///
    /// Public so that whatever stands in for a run reports the way the run
    /// does: a front end's test of how it draws a slow one, or a source a
    /// front end fetches itself.
    pub fn report(&self, step: P) {
        if let Some(sink) = &self.sink {
            sink(step);
        }
    }
}

impl<P: 'static> Watch<P> {
    /// A watch of another run's steps, `Q`, each told to this one's sink as
    /// `into` makes it a `P`, and sharing this one's flag both ways.
    ///
    /// How a front end that follows more than one kind of run holds one
    /// watch for all of them: the CLI's steps are a gather's or a backup's,
    /// and it hands a gather `watch.relaying(Step::Gather)`, so a cancel of
    /// the one it holds reaches whichever is running, and each step reaches
    /// the one sink in its own words.
    pub fn relaying<Q>(&self, into: impl Fn(Q) -> P + Send + Sync + 'static) -> Watch<Q> {
        let sink = self
            .sink
            .clone()
            .map(|sink| Arc::new(move |step: Q| sink(into(step))) as Arc<dyn Fn(Q) + Send + Sync>);
        Watch {
            sink,
            cancelled: Arc::clone(&self.cancelled),
        }
    }
}

// By hand rather than derived: a derive would ask `P` to be `Clone` and
// `Default`, which a watch of it never needs, a step being only handed on.
impl<P> Clone for Watch<P> {
    fn clone(&self) -> Self {
        Self {
            sink: self.sink.clone(),
            cancelled: Arc::clone(&self.cancelled),
        }
    }
}

impl<P> Default for Watch<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> fmt::Debug for Watch<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Watch")
            .field("reporting", &self.sink.is_some())
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[cfg(test)]
#[path = "follow_test.rs"]
mod tests;
