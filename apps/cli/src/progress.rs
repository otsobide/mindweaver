//! What a run a front end follows is doing, in words, and drawn on the
//! terminal while it runs: a gather, or a backup.
//!
//! Core says which step each is at ([`gather::Progress`],
//! [`backup::Progress`]), and until which a cancel is honoured; how that
//! reads is the front end's to decide, and every one of this crate's says it
//! with [`said`]: the line `mindweaver gather` and `mindweaver backup` keep
//! rewriting on stderr, the box the TUI draws while one runs, and each step
//! the panel's backend reports to the server ([`Step`], [`reported`]).
//!
//! Every line a front end runs is handed one [`Watch`] of either run's step
//! ([`Progress`]), whichever command the line is, and the command hands its
//! run the watch [relaying](mindweaver_core::follow::Watch::relaying) to it:
//! one flag a front end cancels, and one sink it reads, whatever is running.

use std::io::{self, IsTerminal as _, Write};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use mindweaver_core::{backup, follow, gather};
use serde::Serialize;

/// A step of a run a front end follows ([`crate::task::Followed`]), as core
/// says it of the run: one variant a run, each holding core's own step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// A step of `mindweaver gather git`.
    Gather(gather::Progress),
    /// A step of `mindweaver backup`.
    Backup(backup::Progress),
}

impl Progress {
    /// Whether a cancel asked now still stops the run, as core says of the
    /// run's step: a gather until it places, a backup until it pushes.
    pub fn cancellable(self) -> bool {
        match self {
            Progress::Gather(progress) => progress.cancellable(),
            Progress::Backup(progress) => progress.cancellable(),
        }
    }
}

/// How a front end follows the lines it runs, and stops them: one watch of
/// either run's step, handed to every line.
pub type Watch = follow::Watch<Progress>;

/// One step of a run, as a person reads it: what is being done or counted,
/// and how far it has got.
pub(crate) fn said(progress: Progress) -> String {
    let progress = match progress {
        Progress::Gather(progress) => progress,
        Progress::Backup(progress) => {
            return String::from(match progress {
                backup::Progress::CheckingOut => "checking the repository out",
                backup::Progress::Exporting => "exporting the library",
                backup::Progress::Committing => "committing",
                backup::Progress::Pushing => "pushing",
            })
        }
    };
    let (what, done, total) = match progress {
        gather::Progress::Connecting => return String::from("connecting"),
        gather::Progress::Receiving { done, total } => ("receiving objects", done, total),
        gather::Progress::Resolving { done, total } => ("resolving deltas", done, total),
        gather::Progress::CheckingOut { done, total } => ("checking out files", done, total),
        gather::Progress::Harvesting { done, total } => ("reading folders", done, Some(total)),
        gather::Progress::Placing { done, total } => ("placing on the shelf", done, Some(total)),
    };
    match total {
        Some(total) => format!("{what} {done}/{total}"),
        // A count with nothing to measure it against says little until it
        // has started.
        None if done == 0 => what.to_owned(),
        None => format!("{what} {done}"),
    }
}

/// One step of a run under way, as the panel is told it: what the run is
/// doing, how far it has got, the same in the words a terminal draws it
/// in, and whether a cancel asked now still stops it.
///
/// What the panel's backend reports through the server (`progress`), as
/// the server carries it unread: its words and its verdict worked out
/// here, from core's step, so the panel draws a bar and offers a cancel
/// without spelling a step, nor the point past which a cancel comes too
/// late, in TypeScript. Of no one run's shape but its stages: a gather's
/// and a backup's checking out, exporting, committing and pushing are
/// said in the same five fields, a backup's with no count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Step {
    /// What it is doing, as one word.
    pub stage: Stage,
    /// How much of it is done, when the stage counts anything.
    pub done: Option<usize>,
    /// Of how much, once that is known.
    pub total: Option<usize>,
    /// The step in words, as `mindweaver gather` draws it on a terminal
    /// and the TUI in its box ([`said`]): `receiving objects 120/340`.
    pub said: String,
    /// Whether a cancel asked now still stops the run, as core says of
    /// the step (`Progress::cancellable`): true until a gather starts
    /// putting what it read on the shelf, and until a backup starts
    /// pushing.
    pub cancellable: bool,
}

/// What a run under way is doing, as one word: each step of a gather, in
/// the order it takes them, then a backup's that are not a gather's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    /// Reaching the source, before it has sent anything.
    Connecting,
    /// Receiving the objects it sends.
    Receiving,
    /// Resolving the deltas among them.
    Resolving,
    /// Writing the fetched revision's files into the clone; for a backup,
    /// checking the repository out, which counts nothing.
    CheckingOut,
    /// Reading the folders that hold a manifest.
    Harvesting,
    /// Putting what was read on the shelf.
    Placing,
    /// A backup's export of the library into its check-out.
    Exporting,
    /// A backup's commit of what it exported.
    Committing,
    /// A backup's push of its commit, past which a cancel is too late.
    Pushing,
}

impl Step {
    /// `progress`, as the panel is told it.
    pub fn of(progress: Progress) -> Step {
        use gather::Progress as Gather;
        let (stage, done, total) = match progress {
            Progress::Gather(Gather::Connecting) => (Stage::Connecting, None, None),
            Progress::Gather(Gather::Receiving { done, total }) => {
                (Stage::Receiving, Some(done), total)
            }
            Progress::Gather(Gather::Resolving { done, total }) => {
                (Stage::Resolving, Some(done), total)
            }
            Progress::Gather(Gather::CheckingOut { done, total }) => {
                (Stage::CheckingOut, Some(done), total)
            }
            Progress::Gather(Gather::Harvesting { done, total }) => {
                (Stage::Harvesting, Some(done), Some(total))
            }
            Progress::Gather(Gather::Placing { done, total }) => {
                (Stage::Placing, Some(done), Some(total))
            }
            Progress::Backup(progress) => {
                let stage = match progress {
                    backup::Progress::CheckingOut => Stage::CheckingOut,
                    backup::Progress::Exporting => Stage::Exporting,
                    backup::Progress::Committing => Stage::Committing,
                    backup::Progress::Pushing => Stage::Pushing,
                };
                (stage, None, None)
            }
        };
        Step {
            stage,
            done,
            total,
            said: said(progress),
            cancellable: progress.cancellable(),
        }
    }

    /// Whether it is the last of its count: as far as the stage goes.
    fn ends_its_stage(&self) -> bool {
        matches!((self.done, self.total), (Some(done), Some(total)) if done == total)
    }
}

/// How often a step of one stage is handed on, at most: ten times a second,
/// as often as `gix`'s counts are read, and as often as anybody reads a
/// count.
pub(crate) const PACE: Duration = Duration::from_millis(100);

/// A watch that hands each step of a run to `report`, as a [`Step`]:
/// the first of each stage and the last of each count, so no stage goes
/// unsaid and none is left short of its end, and in between no more than
/// one every [`PACE`].
///
/// Paced here rather than by the server, which carries each report to
/// every subscriber as an event: a gather reads and places a folder at a
/// time, as fast as the disk goes, and a few hundred of those at once would
/// leave every subscriber behind the bus's capacity, and so told to read
/// everything again. Here, where a stage is known, so the pace never drops
/// one that says something new.
pub(crate) fn reported(report: impl Fn(Step) + Send + Sync + 'static) -> Watch {
    paced(report, PACE)
}

/// [`reported`], one step of a stage at most every `pace`.
fn paced(report: impl Fn(Step) + Send + Sync + 'static, pace: Duration) -> Watch {
    let last: Mutex<Option<(Stage, Instant)>> = Mutex::new(None);
    Watch::new().reporting_to(move |progress| {
        let step = Step::of(progress);
        let now = Instant::now();
        let due = {
            let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
            let due = match *last {
                None => true,
                Some((stage, at)) => {
                    stage != step.stage || step.ends_its_stage() || now.duration_since(at) >= pace
                }
            };
            if due {
                *last = Some((step.stage, now));
            }
            due
        };
        if due {
            report(step);
        }
    })
}

/// A run's progress, a gather's or a backup's, drawn on one line of stderr
/// and rewritten in place as it changes, then wiped before the command says
/// what it did.
///
/// Only when stderr is a terminal. Piped, or read by a test, a line rewritten
/// with carriage returns is noise in a log, and the report that follows says
/// what happened anyway; so is anything a command run that way prints, which
/// is why it is not printed at all rather than printed differently.
pub struct ProgressLine {
    drawn: Option<Arc<Mutex<Drawn>>>,
}

impl ProgressLine {
    /// A line on stderr, drawn if stderr is a terminal and never otherwise.
    pub fn on_stderr() -> Self {
        if io::stderr().is_terminal() {
            Self::to(io::stderr())
        } else {
            Self { drawn: None }
        }
    }

    /// A line drawn into `out`, whatever it is.
    fn to(out: impl Write + Send + 'static) -> Self {
        Self {
            drawn: Some(Arc::new(Mutex::new(Drawn {
                out: Box::new(out),
                width: 0,
            }))),
        }
    }

    /// The watch to run a command with, so how a gather or a backup goes
    /// is drawn here.
    pub fn watch(&self) -> Watch {
        let Some(drawn) = &self.drawn else {
            return Watch::new();
        };
        let drawn = Arc::clone(drawn);
        Watch::new().reporting_to(move |progress| {
            if let Ok(mut drawn) = drawn.lock() {
                drawn.draw(&said(progress));
            }
        })
    }

    /// Wipe the line, so what the command says next starts at the beginning
    /// of an empty one rather than after the last step it reached.
    pub fn clear(&self) {
        if let Some(Ok(mut drawn)) = self.drawn.as_ref().map(|drawn| drawn.lock()) {
            drawn.wipe();
        }
    }
}

/// Where the line goes, and how much of it is drawn there now.
struct Drawn {
    out: Box<dyn Write + Send>,
    /// In characters, which is what a terminal advances by for these words.
    width: usize,
}

impl Drawn {
    /// Draw `text` over what is there. A carriage return and spaces over the
    /// rest, rather than an escape that erases the line, which a terminal
    /// that does not read escapes would print as it is.
    fn draw(&mut self, text: &str) {
        let width = text.chars().count();
        let rest = self.width.saturating_sub(width);
        // A terminal that has gone away is no reason to stop a run.
        let _ = write!(self.out, "\r{text}{:rest$}", "");
        let _ = self.out.flush();
        self.width = width;
    }

    fn wipe(&mut self) {
        if self.width == 0 {
            return;
        }
        let _ = write!(self.out, "\r{:width$}\r", "", width = self.width);
        let _ = self.out.flush();
        self.width = 0;
    }
}

#[cfg(test)]
#[path = "progress_test.rs"]
mod tests;
