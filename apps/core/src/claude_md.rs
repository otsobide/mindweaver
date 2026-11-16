//! Whether Claude Code reads a project's `AGENTS.md`, and the import that
//! makes it read the file where a `CLAUDE.md` is in the way.
//!
//! The block of rules in `AGENTS.md` ([`crate::agents_md`]) reaches Codex
//! and OpenCode wherever it is, but Claude reads that file natively only
//! when no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` is in the
//! folder it starts in or any folder above it (docs/agents.md, verified on
//! 2.1.309 but for `.claude/CLAUDE.md`, seen only among the names it looks
//! for). One predicate decides it, [`reads_agents_md`], so whatever gives
//! Claude its rules asks the same question and never reads the block as
//! reaching Claude where it does not (D-03).
//!
//! Where the project's own `CLAUDE.md` is what is in the way, one line in
//! it brings `AGENTS.md` in, between the markers the block of `AGENTS.md`
//! has, at the top of a file that is otherwise somebody else's:
//!
//! ```text
//! <!-- mindweaver:begin -->
//! @AGENTS.md
//! <!-- mindweaver:end -->
//!
//! # Working here
//! ```
//!
//! The layout the probe verified, with the file loaded once (docs/agents.md).
//! Everything outside the block comes back byte for byte, but for the empty
//! line just below the end marker, which is the block's ([`ClaudeMd`]), and a
//! block that is not exactly one well-formed one is refused, never repaired,
//! as in `AGENTS.md`: the markers are read by the same search
//! (`agents_md::locate`), under either names, and an import a Mindflayer
//! build wrote is kept under its own markers until it is moved
//! ([`ClaudeMd::renamed`]).
//!
//! [`reach`] decides how Claude gets the block, from the predicate, the
//! project's settings and whether the block was written: natively, through
//! an import of `AGENTS.md` the project's files already hold, Mindweaver's
//! or somebody's, through the import added, or not at all, when Claude's
//! own rules folder is to hold every rule instead ([`Reach::mode`],
//! [`crate::claude_rules`]). Never both a block Claude reads and a rule
//! file for the same rule, so Claude reads each rule once. [`render`] adds
//! the import where the reach says, and takes it out everywhere else.
//! Which rules a project is given, and when to render, are the caller's
//! ([`crate::install::rules`]), as for the block itself.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::agent::AgentHomes;
use crate::agents_md::{self, Malformed, BEGIN, END};
use crate::claude_rules::{self, Mode};
use crate::code::codes;
use crate::exclude;
use crate::lines::{self, Line};
use crate::names::Names;
use crate::settings::Share;
use crate::workspace::{replace_bytes_unresolved, MindProject};

/// The file Claude reads as a folder's instructions, and its user memory
/// file's name in its own folder.
pub const CLAUDE_MD: &str = "CLAUDE.md";
/// The one a person keeps to themselves, beside `CLAUDE.md`. Mindweaver
/// never makes one: a `CLAUDE.local.md` alone stops Claude reading
/// `AGENTS.md`, without a word to anybody (D-03).
pub const CLAUDE_LOCAL_MD: &str = "CLAUDE.local.md";

/// Each name that keeps Claude from reading `AGENTS.md` when it is in a
/// folder Claude looks in, as a route of folder names from that folder, in
/// the order Claude's check lists them.
const NAMES: [&[&str]; 3] = [&[CLAUDE_MD], &[".claude", CLAUDE_MD], &[CLAUDE_LOCAL_MD]];

/// The project's own files an import may go in, in the order one is chosen,
/// each with the line that brings in the `AGENTS.md` at the project's root
/// from where the file is.
///
/// `CLAUDE.md` first, the one the probe verified the import in. From
/// `.claude`, `@../AGENTS.md`: Claude takes an import's path from the file
/// it is in, not from where it starts (docs/agents.md, docs-only). Never
/// `CLAUDE.local.md`, which is a person's own and kept to themselves, so an
/// import there would reach nobody else, and would leave the project with
/// rules that live in a file no teammate has.
const OWN: [(&[&str], &str); 2] = [
    (&[CLAUDE_MD], "@AGENTS.md"),
    (&[".claude", CLAUDE_MD], "@../AGENTS.md"),
];

/// A person's own file, which Mindweaver never writes in but reads for an
/// import of `AGENTS.md` they wrote themselves, and that import's line.
const LOCAL: (&[&str], &str) = (&[CLAUDE_LOCAL_MD], "@AGENTS.md");

/// Whether Claude Code, started in `folder`, reads the `AGENTS.md` there:
/// whether no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` is in
/// `folder` or in a folder above it ([`searched`]).
///
/// The user memory file, `CLAUDE.md` in Claude's own folder, which `homes`
/// names, never counts, even where the walk up from a project under the
/// user's home finds it as that home's `.claude/CLAUDE.md`: Claude reads it
/// for every project and goes on reading `AGENTS.md` beside it (verified).
/// Told by the file it is, both resolved, so a spelling through a link is
/// still the memory file. With no Claude folder in `homes`, as a test hands
/// over, nothing is left out, and such a file counts like any other.
///
/// Folders above the git root count too (verified: a `CLAUDE.md` in the
/// folder a person keeps their repositories in stops the read), and so does
/// anything at one of those names, a link followed to whatever it names; a
/// link to nothing is nothing there.
pub fn reads_agents_md(folder: &Path, homes: &AgentHomes) -> Result<bool, ClaudeMdError> {
    Ok(stopping(folder, homes)?.is_empty())
}

/// Every file that keeps Claude from reading `AGENTS.md` when it starts in
/// `folder`, the nearest folder's first, each folder's in the order of
/// [`NAMES`]: spelled from `folder` resolved, as Claude, started there, has
/// it from the operating system.
fn stopping(folder: &Path, homes: &AgentHomes) -> Result<Vec<PathBuf>, ClaudeMdError> {
    let start = crate::paths::canonical(folder).map_err(|source| ClaudeMdError::Read {
        path: folder.to_path_buf(),
        source,
    })?;
    let memory = homes
        .claude_config_dir
        .as_deref()
        .and_then(|folder| crate::paths::canonical(folder.join(CLAUDE_MD)).ok());
    let mut found = Vec::new();
    for folder in searched(&start) {
        for name in NAMES {
            let path = joined(folder, name);
            match fs::metadata(&path) {
                Ok(_) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
                    ) =>
                {
                    continue
                }
                Err(source) => return Err(ClaudeMdError::Read { path, source }),
            }
            if memory.is_some() && crate::paths::canonical(&path).ok() == memory {
                continue;
            }
            found.push(path);
        }
    }
    Ok(found)
}

/// Every path [`reads_agents_md`] looks at for Claude started in `folder`,
/// there or not, the nearest folder's first: what a front end watches
/// (`crate::watch`), so a `CLAUDE.md` made above a project since, which
/// keeps Claude from its `AGENTS.md`, is a change to what doctor says of
/// it. Spelled from `folder` resolved, as the look is, or as given when it
/// does not resolve; the user memory file among them, which is watched all
/// the same.
pub(crate) fn looked_at(folder: &Path) -> Vec<PathBuf> {
    let start = crate::paths::canonical(folder).unwrap_or_else(|_| folder.to_path_buf());
    searched(&start)
        .flat_map(|folder| NAMES.iter().map(move |name| joined(folder, name)))
        .collect()
}

/// The folders Claude looks in, started in `start`: `start` and every folder
/// above it, nearest first, but the filesystem's root.
///
/// Not the root, because Claude's own check leaves it out: its debug log
/// counted four folders searched from a start four below `/`, and five from
/// one five below, each one short of the folders up to and including `/`
/// (docs/agents.md). Counting `/` would have this say Claude reads no
/// `AGENTS.md` where it does, with a `CLAUDE.md` at `/` and none nearer, and
/// give Claude every rule about every file twice, in the block and in its
/// rules folder. Leaving it out errs the other way only if Claude does read
/// that far after all, and a `CLAUDE.md` at `/` is one only the machine's
/// administrator can put there.
fn searched(start: &Path) -> impl Iterator<Item = &Path> {
    start
        .ancestors()
        .take_while(|folder| folder.parent().is_some())
}

/// How Claude Code reaches the block of rules in a project's `AGENTS.md`,
/// as [`reach`] decides it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// Claude reads `AGENTS.md` itself: nothing in the project or above it
    /// keeps it from doing so ([`reads_agents_md`]).
    Native,
    /// Claude reads the block as one of its own files, the project's
    /// `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` here, which is
    /// its `AGENTS.md`, one file through a link, as some teams keep the two.
    /// The block is in that one file, whichever name is the link
    /// ([`crate::agents_md::render`] writes through one), so no import goes
    /// in: it would bring the file into itself, between the markers the
    /// block already holds.
    Same(PathBuf),
    /// Claude reads the block through an import of `AGENTS.md` somebody
    /// wrote in this file, outside Mindweaver's markers, as Claude's docs
    /// show a `CLAUDE.md` sharing `AGENTS.md`: the project's own
    /// `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md`. Nothing is
    /// written, and Mindweaver's import, wherever it is, is no longer wanted.
    Imported(PathBuf),
    /// Claude reads the block through Mindweaver's import in this file, the
    /// project's own `CLAUDE.md` or `.claude/CLAUDE.md`, there already or for
    /// [`render`] to add.
    Import(PathBuf),
    /// Claude reads no block, and is to have every rule from its own rules
    /// folder ([`Mode::Fallback`]), for this reason.
    Fallback(Fallback),
}

impl Reach {
    /// Which rules Claude's own rules folder is to hold
    /// ([`crate::claude_rules::render`]): those about some files only while
    /// Claude reads the block, and every rule while it does not, so Claude
    /// reads each rule once and misses none.
    pub fn mode(&self) -> Mode {
        match self {
            Reach::Native | Reach::Same(_) | Reach::Imported(_) | Reach::Import(_) => Mode::Scoped,
            Reach::Fallback(_) => Mode::Fallback,
        }
    }
}

/// Why Claude reads no block of rules in a project, and gets them from its
/// own rules folder instead: said, and no failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Fallback {
    /// No block was written into `AGENTS.md`: no rule about every file was
    /// asked for, or the block was not rendered.
    #[error("no block of rules in AGENTS.md for Claude Code to read")]
    NoBlock,
    /// What keeps Claude from reading `AGENTS.md` in the project is its
    /// `CLAUDE.local.md`, and no `CLAUDE.md` of its own is there to import
    /// it from.
    #[error(
        "{0}: Claude Code reads it in place of AGENTS.md, and it is a person's own, so no \
         import is added there; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    LocalOnly(PathBuf),
    /// What keeps Claude from reading `AGENTS.md` is above the project, the
    /// nearest such file given, and the project has no `CLAUDE.md` of its own
    /// to import it from. Never edited: it is not the project's, and Claude
    /// reads it in every project below.
    #[error(
        "{0}: above the project, where Claude Code reads it in place of AGENTS.md, and \
         Mindweaver edits nothing above a project; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Above(PathBuf),
    /// The project's own `CLAUDE.md`, which git tracks in a local project
    /// that does not set `edit_tracked` (D-02).
    #[error(
        "{0}: git tracks it, and the project is local without edit_tracked, so no import of \
         AGENTS.md is added there; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Tracked(PathBuf),
    /// The project's own `CLAUDE.md`, reached through a link to `target`.
    #[error(
        "{file}: a link to {target}, so no import of AGENTS.md is added through it; Claude \
         Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Linked { file: PathBuf, target: PathBuf },
    /// The project's own `CLAUDE.md`, of which whether git tracks it cannot
    /// be told.
    #[error(
        "{file}: whether git tracks it cannot be told ({detail}), so no import of AGENTS.md \
         is added there; Claude Code gets its rules from {folder}",
        folder = claude_rules::folder().display()
    )]
    Untold { file: PathBuf, detail: String },
}

codes!(Fallback {
    Self::NoBlock => "claude_fallback_no_block",
    Self::LocalOnly(_) => "claude_fallback_local_only",
    Self::Above(_) => "claude_fallback_above",
    Self::Tracked(_) => "claude_fallback_tracked",
    Self::Linked { .. } => "claude_fallback_linked",
    Self::Untold { .. } => "claude_fallback_untold",
});

/// How Claude Code, started at `project`'s root, is to reach the block of
/// rules in its `AGENTS.md`, given whether the block is there
/// (`block_written`, what [`crate::agents_md::render`] said).
///
/// - [`Reach::Native`] when Claude reads `AGENTS.md` itself
///   ([`reads_agents_md`], with `homes` naming the user memory file it
///   leaves out).
/// - [`Reach::Same`] when one of Claude's own files in the project is that
///   `AGENTS.md`, through a link.
/// - [`Reach::Imported`] when the project's own `CLAUDE.md` or
///   `.claude/CLAUDE.md` already imports `AGENTS.md` with a line of its own
///   ([`ClaudeMd::imports_outside`]), whether or not it may be edited:
///   Claude reads the block through it, and the fallback beside it would
///   give Claude every rule about every file twice. Chosen before
///   Mindweaver's own import, which beside it would only name the same file
///   a second time, and which [`render`] then takes out.
/// - [`Reach::Import`] when the project's own `CLAUDE.md` or
///   `.claude/CLAUDE.md` is there and may be edited: in a shared project,
///   in one a `mind.toml` marks, which was committed by design (D-02), and
///   in a local one while git does not track the file or its settings say
///   `edit_tracked`. Chosen in the order of [`OWN`], but one already
///   holding the import keeps it, so the import never moves from file to
///   file, and is kept even where it may no longer be edited, since Claude
///   reads it there all the same.
/// - [`Reach::Imported`] again when no file of the project's own can take
///   the import but a `CLAUDE.local.md` imports `AGENTS.md`, as its owner
///   may have written it to keep reading that file: last, since it reaches
///   only them, and a teammate cloning a shared project needs the import
///   in the `CLAUDE.md` they share.
/// - [`Reach::Fallback`] otherwise: no block, only a `CLAUDE.local.md`, a
///   `CLAUDE.md` only above the project, a project's own one git tracks
///   without `edit_tracked`, or one reached through a link, which is never
///   followed to write, as its import would then mean whatever the link
///   leads to.
///
/// Reads, and writes nothing: [`render`] carries it out. A `CLAUDE.md` of
/// the project's own that holds a malformed block is refused, as writing
/// the import into it would be, since what Claude reads there cannot be
/// told. Each is read before any is chosen, so whichever file holds an
/// import, a malformed one refuses the reach as it refuses the render.
pub fn reach(
    project: &MindProject,
    homes: &AgentHomes,
    block_written: bool,
) -> Result<Reach, ClaudeMdError> {
    if !block_written {
        return Ok(Reach::Fallback(Fallback::NoBlock));
    }
    decide(project, &Survey::of(project.root(), homes)?)
}

/// Whether Claude Code, started at `project`'s root, reads the block of its
/// `AGENTS.md` as the files stand now: `None` when it does, natively, as one
/// of its own files, or through an import a file holds already,
/// Mindweaver's or somebody's; otherwise the file nearest the project that
/// keeps it from reading `AGENTS.md`.
///
/// [`reach`] says how Claude is to reach the block once a render carries it
/// out, an import it is yet to add among it; this is what Claude does until
/// then, which doctor asks of a project rendered while nothing kept Claude
/// from the file, before a `CLAUDE.local.md` came, say (D-03). Read as
/// [`reach`] reads, so the two never disagree about what Claude finds.
pub(crate) fn unread_by(
    project: &MindProject,
    homes: &AgentHomes,
) -> Result<Option<PathBuf>, ClaudeMdError> {
    let survey = Survey::of(project.root(), homes)?;
    let Some(nearest) = survey.stopping.first().cloned() else {
        return Ok(None);
    };
    Ok(match decide(project, &survey)? {
        Reach::Native | Reach::Same(_) | Reach::Imported(_) => None,
        Reach::Import(file) => {
            let import = OWN
                .iter()
                .find(|(name, _)| joined(&survey.root, name) == file)
                .map(|(_, import)| *import);
            let held = match import {
                Some(import) => read(&file)?.imports(import),
                None => false,
            };
            (!held).then_some(nearest)
        }
        Reach::Fallback(_) => Some(nearest),
    })
}

/// What [`reach`] and [`render`] look at in a project and above it.
struct Survey {
    root: PathBuf,
    /// The root resolved.
    real: PathBuf,
    /// Every file that keeps Claude from reading `AGENTS.md` there.
    stopping: Vec<PathBuf>,
    /// The project's `AGENTS.md` resolved, when it is there.
    agents: Option<PathBuf>,
}

impl Survey {
    fn of(root: &Path, homes: &AgentHomes) -> Result<Self, ClaudeMdError> {
        Ok(Survey {
            root: root.to_path_buf(),
            real: resolved(root)?,
            stopping: stopping(root, homes)?,
            agents: crate::paths::canonical(root.join(agents_md::FILE)).ok(),
        })
    }

    /// The project's file at `name`, as the project spells it, while it is
    /// one that keeps Claude from reading `AGENTS.md`: never the user memory
    /// file, which a project at the user's home holds as `.claude/CLAUDE.md`.
    fn own(&self, name: &[&str]) -> Option<PathBuf> {
        self.stopping
            .contains(&joined(&self.real, name))
            .then(|| joined(&self.root, name))
    }

    /// Whether `file` is the project's `AGENTS.md`, through a link one way or
    /// the other.
    fn is_agents_md(&self, file: &Path) -> bool {
        self.agents.is_some() && crate::paths::canonical(file).ok() == self.agents
    }

    /// Whether the project's `file` at `name` is a file in its place, no
    /// link leading anywhere on the way to it.
    fn in_place(&self, file: &Path, name: &[&str]) -> bool {
        crate::paths::canonical(file).is_ok_and(|target| target == joined(&self.real, name))
    }
}

/// [`reach`] for a written block, from what `survey` found.
fn decide(project: &MindProject, survey: &Survey) -> Result<Reach, ClaudeMdError> {
    let Some(nearest) = survey.stopping.first() else {
        return Ok(Reach::Native);
    };
    for name in NAMES {
        if let Some(file) = survey.own(name) {
            if survey.is_agents_md(&file) {
                return Ok(Reach::Same(file));
            }
        }
    }

    let mut own = Vec::new();
    for (name, import) in OWN {
        let Some(file) = survey.own(name) else {
            continue;
        };
        let edit = edit(project, &survey.real, &file, name)?;
        // A link's import would mean whatever the link leads to, so only a
        // file in its place is asked whether it imports.
        let text = match edit {
            Edit::Refused(Fallback::Linked { .. }) => None,
            _ => Some(read(&file)?),
        };
        own.push((file, import, edit, text));
    }
    let holding = |holds: fn(&ClaudeMd, &str) -> bool| {
        own.iter()
            .find(|(_, import, _, text)| text.as_ref().is_some_and(|text| holds(text, import)))
            .map(|(file, ..)| file.clone())
    };
    if let Some(file) = holding(ClaudeMd::imports_outside) {
        return Ok(Reach::Imported(file));
    }
    if let Some(file) = holding(ClaudeMd::imports) {
        return Ok(Reach::Import(file));
    }

    let mut chosen = None;
    let mut refused = None;
    for (file, _, edit, _) in own {
        match edit {
            Edit::Allowed => {
                chosen.get_or_insert(file);
            }
            Edit::Refused(reason) => {
                refused.get_or_insert(reason);
            }
        }
    }
    if let Some(file) = chosen {
        return Ok(Reach::Import(file));
    }
    let (name, import) = LOCAL;
    if let Some(file) = survey.own(name) {
        if survey.in_place(&file, name) && imported_by(&read_lines(&file)?, import) {
            return Ok(Reach::Imported(file));
        }
    }
    if let Some(reason) = refused {
        return Ok(Reach::Fallback(reason));
    }
    // No file of the project's own: what is in the way is a person's own
    // `CLAUDE.local.md`, or something above the project.
    let local = joined(&survey.real, &[CLAUDE_LOCAL_MD]);
    Ok(Reach::Fallback(if *nearest == local {
        Fallback::LocalOnly(survey.root.join(CLAUDE_LOCAL_MD))
    } else {
        Fallback::Above(nearest.clone())
    }))
}

/// Carry out how Claude is to reach `project`'s block, which [`reach`]
/// decides from `block_written` and `homes`: the import in the file it
/// names, and in none of the project's other `CLAUDE.md` files.
///
/// Only the project's own `CLAUDE.md` and `.claude/CLAUDE.md` are touched,
/// each only while it is there: never made, as no `CLAUDE.local.md` is
/// ever made, since a `CLAUDE.md` Mindweaver made would itself keep Claude
/// from reading `AGENTS.md` natively. Never one above the project either,
/// which the reach says instead ([`Fallback::Above`]): Claude reads it in
/// every project below, and it is nobody's to edit for one of them. Never a
/// link, the user memory file, or the project's `AGENTS.md` itself.
///
/// The import is added once, and changed nowhere it already is; with no
/// block, or a reach that needs no import, it is taken out of whichever file
/// holds it, so it goes with the last rule, and the file is left exactly as
/// it was before. One that may not be edited any more, tracked by git in a
/// local project without `edit_tracked`, keeps it, and that is said
/// ([`Warning`]), rather than edited behind the setting that keeps
/// Mindweaver out of it. A file whose text a change leaves as it was is not
/// written at all.
///
/// A file is replaced whole through a temporary and a rename
/// (`replace_bytes_unresolved`), never through a link. Every file is read
/// first, and a malformed block in any of them refuses the render with
/// nothing written.
///
/// Takes no lock: the caller holds the workspace's, as one calling
/// [`crate::agents_md::render`] does, and the lock is not re-entrant.
pub fn render(
    project: &MindProject,
    homes: &AgentHomes,
    block_written: bool,
) -> Result<Rendered, ClaudeMdError> {
    let survey = Survey::of(project.root(), homes)?;
    let reach = if block_written {
        decide(project, &survey)?
    } else {
        Reach::Fallback(Fallback::NoBlock)
    };

    let mut found = Vec::new();
    for (name, import) in OWN {
        let Some(file) = survey.own(name) else {
            continue;
        };
        if !survey.in_place(&file, name) || survey.is_agents_md(&file) {
            continue;
        }
        let text = read(&file)?;
        found.push((name, import, file, text));
    }

    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for (name, import, file, mut text) in found {
        let wanted = reach == Reach::Import(file.clone());
        let written = if wanted {
            if text.imports(import) {
                Written::Unchanged
            } else {
                let added = !text.has_block();
                text.set_import(import);
                write(&file, &text)?;
                if added {
                    Written::Added
                } else {
                    Written::Changed
                }
            }
        } else if !text.has_block() {
            Written::Unchanged
        } else {
            match edit(project, &survey.real, &file, name)? {
                Edit::Allowed => {
                    text.drop_import();
                    write(&file, &text)?;
                    Written::Removed
                }
                Edit::Refused(Fallback::Tracked(file)) => {
                    warnings.push(Warning::Tracked(file));
                    Written::Unchanged
                }
                Edit::Refused(Fallback::Untold { file, detail }) => {
                    warnings.push(Warning::Untold { file, detail });
                    Written::Unchanged
                }
                // A link is never read here (`Survey::in_place`), and nothing
                // else is refused.
                Edit::Refused(_) => Written::Unchanged,
            }
        };
        files.push(ImportFile {
            path: file,
            written,
        });
    }
    Ok(Rendered {
        reach,
        files,
        warnings,
    })
}

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// How Claude reaches the block now: what Claude's rule files are to be
    /// rendered for ([`Reach::mode`]), and why, when it reads no block.
    pub reach: Reach,
    /// The project's own `CLAUDE.md` and `.claude/CLAUDE.md`, each that is
    /// there and could hold the import, in that order, and what the render
    /// did to it.
    pub files: Vec<ImportFile>,
    /// What there is to say about an import left in place.
    pub warnings: Vec<Warning>,
}

/// One of the project's own `CLAUDE.md` files, and what [`render`] did to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportFile {
    pub path: PathBuf,
    pub written: Written,
}

/// What [`render`] did to one file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// It held no block, and now holds the import, at its top.
    Added,
    /// It held a block that was not the import, and now holds the import.
    Changed,
    /// It held a block no longer wanted, taken out with the empty line
    /// below it.
    Removed,
    /// It was left as it was.
    Unchanged,
}

/// Something [`render`] has to say, which kept nothing from being written.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// The import is in a file git tracks, in a local project without
    /// `edit_tracked`, where it is no longer wanted, and is left there.
    #[error(
        "{0}: holds Mindweaver's import of AGENTS.md, no longer wanted, and is left as it is: \
         git tracks it, and the project is local without edit_tracked"
    )]
    Tracked(PathBuf),
    /// The import is in a file of which whether git tracks it cannot be
    /// told, where it is no longer wanted, and is left there.
    #[error(
        "{file}: holds Mindweaver's import of AGENTS.md, no longer wanted, and is left as it \
         is: whether git tracks it cannot be told ({detail})"
    )]
    Untold { file: PathBuf, detail: String },
}

codes!(Warning {
    Self::Tracked(_) => "claude_import_left_tracked",
    Self::Untold { .. } => "claude_import_left_untold",
});

/// Write `text` over the file at `file`, which is there.
fn write(file: &Path, text: &ClaudeMd) -> Result<(), ClaudeMdError> {
    replace_bytes_unresolved(file, &text.to_bytes())
        .map_err(|(path, source)| ClaudeMdError::Write { path, source })
}

/// Whether the project's own `file`, at `name` from its root, may be edited
/// to hold the import, the root being `real` resolved.
enum Edit {
    Allowed,
    Refused(Fallback),
}

fn edit(
    project: &MindProject,
    real: &Path,
    file: &Path,
    name: &[&str],
) -> Result<Edit, ClaudeMdError> {
    let target = resolved(file)?;
    if target != joined(real, name) {
        return Ok(Edit::Refused(Fallback::Linked {
            file: file.to_path_buf(),
            target,
        }));
    }
    // A project a `mind.toml` marks has no settings, and was committed by
    // design, as a shared one is.
    let Some(settings) = project.settings() else {
        return Ok(Edit::Allowed);
    };
    if settings.share == Share::Shared || settings.edit_tracked {
        return Ok(Edit::Allowed);
    }
    let route: PathBuf = name.iter().collect();
    Ok(match exclude::tracks_file(project.root(), &route) {
        Ok(false) => Edit::Allowed,
        Ok(true) => Edit::Refused(Fallback::Tracked(file.to_path_buf())),
        Err(error) => Edit::Refused(Fallback::Untold {
            file: file.to_path_buf(),
            detail: error.to_string(),
        }),
    })
}

/// `folder` with the route `name` below it.
fn joined(folder: &Path, name: &[&str]) -> PathBuf {
    let mut path = folder.to_path_buf();
    path.extend(name);
    path
}

fn resolved(path: &Path) -> Result<PathBuf, ClaudeMdError> {
    crate::paths::canonical(path).map_err(|source| ClaudeMdError::Read {
        path: path.to_path_buf(),
        source,
    })
}

/// The `CLAUDE.md` at `file`, parsed.
fn read(file: &Path) -> Result<ClaudeMd, ClaudeMdError> {
    ClaudeMd::parse(&read_bytes(file)?).map_err(|problem| ClaudeMdError::Malformed {
        file: file.to_path_buf(),
        problem,
    })
}

/// The lines of the file at `file`, a byte order mark left out: one
/// Mindweaver never writes in, so no block of its own there is looked for,
/// and none is refused.
fn read_lines(file: &Path) -> Result<Vec<Line>, ClaudeMdError> {
    let bytes = read_bytes(file)?;
    Ok(lines::split(bytes.strip_prefix(BOM).unwrap_or(&bytes)))
}

fn read_bytes(file: &Path) -> Result<Vec<u8>, ClaudeMdError> {
    fs::read(file).map_err(|source| ClaudeMdError::Read {
        path: file.to_path_buf(),
        source,
    })
}

/// Whether one of `lines` is `import` as Claude follows it: the whole
/// line, trailing whitespace forgiven, the shape Claude's docs show and the
/// probe verified outside the markers (docs/agents.md, `claude-import` run
/// 1). Not a line in a fenced code block, which Claude's import parsing
/// skips (docs-only), nor one in an HTML comment over several lines, which
/// Claude strips, before or after following imports, which is not known.
///
/// Narrow on purpose, and line by line rather than as markdown, as
/// `AGENTS.md` is read: a line taken for an import Claude does not follow
/// would keep the rules about every file from Claude, which would read them
/// only in the block, while an import missed has Claude read them twice at
/// worst, in the block and in its rules folder, as before the import was
/// looked for. So a fence is taken as opened by any run of three backticks
/// or tildes, and closed only by one that CommonMark closes it with, and an
/// import in the middle of a sentence, which Claude follows too, is not
/// looked for.
fn imported_by<'a>(lines: impl IntoIterator<Item = &'a Line>, import: &str) -> bool {
    enum In {
        Text,
        Fence { mark: u8, length: usize },
        Comment,
    }
    let mut state = In::Text;
    for line in lines {
        let text = &line.text[..];
        state = match state {
            In::Fence { mark, length } => match fence(text) {
                Some((closing, run, rest))
                    if closing == mark && run >= length && rest.trim_ascii().is_empty() =>
                {
                    In::Text
                }
                _ => In::Fence { mark, length },
            },
            In::Comment => match find(text, b"-->") {
                Some(at) if !opens_comment(&text[at..]) => In::Text,
                _ => In::Comment,
            },
            In::Text => {
                if let Some((mark, length, _)) = fence(text) {
                    In::Fence { mark, length }
                } else if opens_comment(text) {
                    In::Comment
                } else if text.trim_ascii_end() == import.as_bytes() {
                    return true;
                } else {
                    In::Text
                }
            }
        };
    }
    false
}

/// The run of three or more backticks or tildes `text` starts with, after
/// at most three spaces, as its character, its length and what follows it.
fn fence(text: &[u8]) -> Option<(u8, usize, &[u8])> {
    let indent = text.iter().take_while(|&&byte| byte == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &text[indent..];
    let mark = *rest.first()?;
    if mark != b'`' && mark != b'~' {
        return None;
    }
    let length = rest.iter().take_while(|&&byte| byte == mark).count();
    (length >= 3).then(|| (mark, length, &rest[length..]))
}

/// Whether `text` leaves an HTML comment open: its last `<!--` has no
/// `-->` after it.
fn opens_comment(text: &[u8]) -> bool {
    text.windows(4)
        .rposition(|window| window == b"<!--")
        .is_some_and(|at| find(&text[at + 4..], b"-->").is_none())
}

fn find(text: &[u8], needle: &[u8]) -> Option<usize> {
    text.windows(needle.len())
        .position(|window| window == needle)
}

/// What a file Windows wrote may open with: kept ahead of the block, which
/// goes at the top, where it would otherwise put the mark in the middle of
/// the file and leave its begin marker one that never reads back as itself.
const BOM: &[u8] = b"\xEF\xBB\xBF";

/// A `CLAUDE.md`'s text, split around Mindweaver's import.
///
/// Kept as lines with their own endings ([`crate::lines`]), as `AGENTS.md`
/// is, so what is outside the block is written back exactly as read, and
/// the block takes whichever ending the file already uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaudeMd {
    /// Whether the file opens with a byte order mark, kept ahead of all.
    bom: bool,
    before: Vec<Line>,
    block: Option<Block>,
    after: Vec<Line>,
    ending: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    /// Whose markers it is spelled with: those it was found under, and
    /// Mindweaver's for one made anew.
    names: Names,
    begin: Line,
    /// Every line between the markers, as read or as written.
    lines: Vec<Line>,
    end: Line,
    /// The empty line just below the end marker, when there is one.
    ///
    /// A block added to a file with anything in it is set apart from what
    /// follows by an empty line, the layout the probe verified. Taken for
    /// the block's when read, as the one above `AGENTS.md`'s block is, since
    /// nothing tells it from one written by hand, so dropping the block gives
    /// back the file as it was before the block was added.
    spacer: Option<Line>,
}

impl ClaudeMd {
    /// Read a `CLAUDE.md`'s bytes.
    ///
    /// Refused as `AGENTS.md` is ([`Malformed`]), and also when the block
    /// holds a rule's marker, which Mindweaver writes only in `AGENTS.md`: a
    /// `CLAUDE.md` holding one is a copy of that file, kept beside it, and
    /// replacing its rules with the import would take them from whoever
    /// reads that copy, while importing beside them would give them twice.
    pub fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let (bom, bytes) = match bytes.strip_prefix(BOM) {
            Some(rest) => (true, rest),
            None => (false, bytes),
        };
        let lines = lines::split(bytes);
        let ending = lines::ending(&lines);
        let Some(span) = agents_md::locate(&lines)? else {
            return Ok(ClaudeMd {
                bom,
                before: lines,
                block: None,
                after: Vec::new(),
                ending,
            });
        };
        if let Some((index, _)) = span.rules.first() {
            return Err(Malformed::Ruled { line: index + 1 });
        }

        let mut lines = lines.into_iter();
        let before: Vec<Line> = lines.by_ref().take(span.begin).collect();
        let begin = lines.next().expect("the begin marker was found");
        let inside: Vec<Line> = lines.by_ref().take(span.end - span.begin - 1).collect();
        let end = lines.next().expect("the end marker was found");
        let mut after: Vec<Line> = lines.collect();
        let spacer = after
            .first()
            .is_some_and(|line| line.text.is_empty())
            .then(|| after.remove(0));
        Ok(ClaudeMd {
            bom,
            before,
            block: Some(Block {
                names: span.names,
                begin,
                lines: inside,
                end,
                spacer,
            }),
            after,
            ending,
        })
    }

    /// Whether the file holds Mindweaver's block.
    pub fn has_block(&self) -> bool {
        self.block.is_some()
    }

    /// Whose markers the block is spelled with: `None` when there is none.
    pub fn names(&self) -> Option<Names> {
        self.block.as_ref().map(|block| block.names)
    }

    /// Spell the block's two markers with `names`', and whether that
    /// changed anything: what `migrate --names` does to an import a
    /// Mindflayer build wrote, every other byte kept, the markers' line
    /// endings among them.
    pub fn renamed(&mut self, names: Names) -> bool {
        let Some(block) = &mut self.block else {
            return false;
        };
        if block.names == names {
            return false;
        }
        let (begin, end, _) = agents_md::markers(names);
        block.begin = Line::new(begin, block.begin.end);
        block.end = Line::new(end, block.end.end);
        block.names = names;
        true
    }

    /// Whether the block holds `import` and nothing else: whether Claude,
    /// reading the file, is brought to `AGENTS.md` by it. Trailing
    /// whitespace is forgiven, as on a marker.
    pub fn imports(&self, import: &str) -> bool {
        self.block.as_ref().is_some_and(|block| {
            matches!(&block.lines[..], [line] if line.text.trim_ascii_end() == import.as_bytes())
        })
    }

    /// Whether a line outside the block is `import`, as somebody wrote it
    /// to share `AGENTS.md` with Claude: Claude follows it as it follows
    /// Mindweaver's, which beside it would only name the file a second time.
    /// What counts as that line is narrow ([`imported_by`]).
    pub fn imports_outside(&self, import: &str) -> bool {
        imported_by(self.before.iter().chain(&self.after), import)
    }

    /// Make the block hold `import` alone, and whether that changed
    /// anything.
    ///
    /// Everything between the markers is Mindweaver's, so it is replaced
    /// whole, as in `AGENTS.md`. A file with no block gains one at the top,
    /// on lines of its own and an empty line apart from what follows; one
    /// somebody moved is changed where it is.
    pub fn set_import(&mut self, import: &str) -> bool {
        let ending = self.ending;
        let lines = vec![Line::new(import, ending)];
        if let Some(block) = &mut self.block {
            if block.lines == lines {
                return false;
            }
            block.lines = lines;
            return true;
        }
        // At the top rather than after what the file holds, where the block
        // of `AGENTS.md` goes: the layout the probe verified, and the import
        // stays where it is whatever somebody writes below it.
        self.after = std::mem::take(&mut self.before);
        let spacer = (!self.after.is_empty()).then(|| Line::new("", ending));
        self.block = Some(Block {
            names: Names::Mindweaver,
            begin: Line::new(BEGIN, ending),
            lines,
            end: Line::new(END, ending),
            spacer,
        });
        true
    }

    /// Take the block out, markers, import and the empty line below them,
    /// and whether there was one.
    pub fn drop_import(&mut self) -> bool {
        let Some(block) = self.block.take() else {
            return false;
        };
        // An end marker without a newline is the file's last line, so the
        // file ended without one and still does once the block is gone.
        if block.end.end.is_empty() {
            if let Some(last) = self.before.last_mut() {
                last.end = b"";
            }
        }
        let after = std::mem::take(&mut self.after);
        self.before.extend(after);
        true
    }

    /// The file as it is to be written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.bom {
            out.extend_from_slice(BOM);
        }
        let mut emit = |line: &Line| {
            out.extend_from_slice(&line.text);
            out.extend_from_slice(line.end);
        };
        self.before.iter().for_each(&mut emit);
        if let Some(block) = &self.block {
            emit(&block.begin);
            block.lines.iter().for_each(&mut emit);
            emit(&block.end);
            block.spacer.iter().for_each(&mut emit);
        }
        self.after.iter().for_each(&mut emit);
        out
    }
}

/// Why how Claude reads a project's `AGENTS.md` could not be told, or the
/// import could not be written.
#[derive(Debug, Error)]
pub enum ClaudeMdError {
    #[error("{path}: cannot be read: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{file}: {problem}, so nothing was written")]
    Malformed { file: PathBuf, problem: Malformed },
}

codes!(ClaudeMdError {
    Self::Read { .. } => "claude_md_read_failed",
    Self::Write { .. } => "claude_md_write_failed",
} carried {
    Self::Malformed { problem, .. } => problem,
});

#[cfg(test)]
#[path = "claude_md_test.rs"]
mod tests;
