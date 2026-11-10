//! What a gather is doing while it runs, and the watch that follows it.
//!
//! A gather is the network, so it can take long enough that somebody wants to
//! know how it is going, and to change their mind. The front end hands it a
//! [`Watch`] ([`crate::follow`]), told each [`Progress`]: core's own steps
//! rather than `gix`'s progress tree and interrupt, so a front end, the
//! server that streams them included, depends on neither gix's version nor
//! the names it gives its steps.

use crate::follow;

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

/// How a front end follows a gather and stops it: a [`follow::Watch`] of
/// its [`Progress`], cancelled until the gather starts placing.
pub type Watch = follow::Watch<Progress>;
