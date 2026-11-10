//! Following a gather while it runs, and stopping it.
//!
//! A gather is the network, so it can take long enough that somebody wants to
//! know how it is going, and to change their mind. The front end hands it a
//! [`Watch`]: a callback it is told each [`Progress`] through, and a flag that
//! cancels it. Both are core's own types rather than `gix`'s progress tree and
//! interrupt, so a front end, the server that streams them included, depends
//! on neither gix's version nor the names it gives its steps.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// What a gather is doing, said while it does it.
///
/// One variant per step, in the order a gather takes them. The counts during
/// the fetch are `gix`'s, read off its progress as it goes; the rest are the
/// harvest's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Reaching the source, before it has sent anything.
    Connecting,
    /// Receiving the objects it sends: `done` so far, of `total` once the
    /// pack has said how many.
    Receiving { done: usize, total: Option<usize> },
    /// Resolving the deltas among them.
    Resolving { done: usize, total: Option<usize> },
    /// Writing the fetched revision's files into the clone.
    CheckingOut { done: usize, total: Option<usize> },
    /// Reading the folders in the source that hold a manifest: `done` of
    /// `total` read.
    Harvesting { done: usize, total: usize },
    /// Putting what was read on the shelf: `done` of `total` placed.
    Placing { done: usize, total: usize },
}

impl Progress {
    /// Whether a cancel at this step still stops the gather: at every step
    /// until it starts placing, and at none after.
    ///
    /// Core's to say rather than each front end's, so the TUI that stops
    /// offering a cancel and the panel's backend that reports these steps
    /// (#112) cannot disagree with the gather about when one comes too
    /// late.
    pub fn cancellable(self) -> bool {
        !matches!(self, Self::Placing { .. })
    }
}

/// How a front end follows a gather and stops it: where its [`Progress`]
/// goes, and the flag that cancels it.
///
/// A plain callback rather than a channel, so the front end chooses how the
/// news travels: the CLI writes a line, the TUI sends it to the thread that
/// draws, and a server can stream it. It is called from the thread the gather
/// runs on and, while `gix` fetches, from one the gather starts to read
/// `gix`'s counts, which is why it must be `Send` and `Sync`.
///
/// Clones share the flag, so a front end keeps one to [`cancel`](Self::cancel)
/// from wherever its keys are read. A cancel is honoured until the gather
/// starts putting things on the shelf, and not after: a half-placed shelf
/// would be worse than a finished one.
#[derive(Clone, Default)]
pub struct Watch {
    sink: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
    cancelled: Arc<AtomicBool>,
}

impl Watch {
    /// A watch that reports nothing and that nothing has cancelled yet: what a
    /// gather nobody follows is given.
    pub fn new() -> Self {
        Self::default()
    }

    /// Report each step of the gather to `sink`.
    pub fn reporting_to(mut self, sink: impl Fn(Progress) + Send + Sync + 'static) -> Self {
        self.sink = Some(Arc::new(sink));
        self
    }

    /// Ask the gather to stop. Safe from any thread, and as often as wanted.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// Whether [`cancel`](Self::cancel) has been called on this watch or a
    /// clone of it.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// The flag itself, for `gix` to stop at.
    pub(crate) fn flag(&self) -> &AtomicBool {
        &self.cancelled
    }

    /// Tell the sink, if there is one.
    ///
    /// Public so that whatever stands in for a gather reports the way a
    /// gather does: a front end's test of how it draws a slow one, or a
    /// source a front end fetches itself.
    pub fn report(&self, progress: Progress) {
        if let Some(sink) = &self.sink {
            sink(progress);
        }
    }
}

impl fmt::Debug for Watch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Watch")
            .field("reporting", &self.sink.is_some())
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[cfg(test)]
#[path = "watch_test.rs"]
mod tests;
