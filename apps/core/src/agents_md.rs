//! A project's rules, rendered into a block of its `AGENTS.md`.
//!
//! The root `AGENTS.md` is the one file of a project that Claude Code, Codex
//! and OpenCode all read, and Codex has no way to include another (D-03, on
//! the facts in docs/agents.md). So a rule reaches a project's agents as a
//! section of a block Mindweaver owns in that file, between two markers, in a
//! file that is otherwise somebody else's:
//!
//! ```text
//! <!-- mindweaver:begin -->
//! <!-- mindweaver:rule git/no-force-push -->
//! Never force-push a branch somebody else may have pulled.
//!
//! <!-- mindweaver:rule review/tone -->
//! Say what a change does before what is wrong with it.
//! <!-- mindweaver:end -->
//! ```
//!
//! HTML comments, because they are the one mark markdown has that is not
//! text: Claude strips a block-level comment before the file reaches the
//! model and keeps what lies between two of them (verified on 2.1.309), and
//! to Codex and OpenCode, which read the file as it is, a comment is a line
//! that says nothing. A heading of Mindweaver's own was the alternative, and
//! every agent would read it as one more instruction.
//!
//! A block a Mindflayer build wrote says `mindflayer:` where this one says
//! `mindweaver:` (D-26, [`Names`]). It is read as a block all the same, and
//! a render replaces it under its own markers, a section added to it
//! included, since a block spelled two ways would be read whole by neither
//! build: only a block made anew is given Mindweaver's, and only
//! `migrate --names` moves an old one to them ([`AgentsMd::renamed`]).
//!
//! Everything outside the block is somebody else's and comes back byte for
//! byte, but for the empty line just above the begin marker, which is the
//! block's (`Block::spacer`). A block that is not exactly one well-formed
//! block is refused rather than repaired, as in git's exclude file ([`crate::exclude`]): guessing
//! where a missing marker belongs could swallow somebody's text into the
//! block, and the next render would replace it.
//!
//! Which rules a project is given, and when, is the caller's to decide, and
//! so is keeping a record of what was written ([`crate::install::rules`]);
//! whether Claude reads the file at all, natively or through an import, is
//! [`crate::claude_md`]'s. This only renders what it is handed.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::agent::AgentHomes;
use crate::artifact::{issue_list, name_issues, Artifact, ArtifactError, Declared};
use crate::catalog::{report_order, spelled};
use crate::code::codes;
use crate::exclude;
use crate::hash;
use crate::install;
use crate::lines::{self, Line};
use crate::names::Names;
use crate::paths;
use crate::value::ContentHash;
use crate::workspace::{replace_bytes, same_directory, MindProject, WeaverWorkspace};

/// The line that opens Mindweaver's block.
pub const BEGIN: &str = "<!-- mindweaver:begin -->";
/// The line that closes it.
pub const END: &str = "<!-- mindweaver:end -->";
/// What opens the marker heading one rule's section, before the rule's
/// route; [`CLOSE`] follows the route.
pub const RULE: &str = "<!-- mindweaver:rule ";
/// What closes a rule's marker, after its route.
pub const CLOSE: &str = " -->";
/// [`BEGIN`] as a Mindflayer build wrote it, read and kept in a block it
/// wrote ([`Names::Mindflayer`]).
pub const FLAYER_BEGIN: &str = "<!-- mindflayer:begin -->";
/// [`END`] as a Mindflayer build wrote it.
pub const FLAYER_END: &str = "<!-- mindflayer:end -->";
/// [`RULE`] as a Mindflayer build wrote it.
pub const FLAYER_RULE: &str = "<!-- mindflayer:rule ";

/// The file the block is rendered into, at a project's root.
pub const FILE: &str = "AGENTS.md";
/// What Codex reads in a folder in place of its `AGENTS.md`.
pub const OVERRIDE: &str = "AGENTS.override.md";

/// How many bytes of the `AGENTS.md` files from the git root down Codex
/// reads, unless its config says otherwise (`project_doc_max_bytes`): it
/// cuts off whatever comes past them (docs/agents.md, docs-only).
pub const CODEX_BUDGET: u64 = 32 * 1024;
/// Where those files are near enough to [`CODEX_BUDGET`] to say so: an
/// eighth short of it, room for a rule or two more of a page each, so the
/// warning comes a render before Codex starts cutting rather than after.
pub const CODEX_NEAR: u64 = CODEX_BUDGET / 8 * 7;

/// What each marker's comment opens with, under either names. A comment
/// that opens with one and is none of the markers is one written wrong, and
/// refused.
const MARKS: [&[u8]; 2] = [b"mindweaver:", b"mindflayer:"];

/// The markers a block is spelled with under `names`: its begin, its end,
/// and what opens a rule's.
pub(crate) const fn markers(names: Names) -> (&'static str, &'static str, &'static str) {
    match names {
        Names::Mindweaver => (BEGIN, END, RULE),
        Names::Mindflayer => (FLAYER_BEGIN, FLAYER_END, FLAYER_RULE),
    }
}

/// An `AGENTS.md`'s text, split around Mindweaver's block.
///
/// Kept as lines with their own endings ([`crate::lines`]), as git's exclude
/// file is, so what is outside the block is written back exactly as read, and
/// the block takes whichever ending the file already uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentsMd {
    before: Vec<Line>,
    block: Option<Block>,
    after: Vec<Line>,
    ending: &'static [u8],
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Block {
    /// The empty line just above the begin marker, when there is one.
    ///
    /// A block added after what a file holds is set apart from it by an empty
    /// line, so that once Claude strips the markers the file's last paragraph
    /// and the first rule's are still two, as the blank line between two
    /// sections keeps two rules two; it is also the layout the probe verified
    /// on Claude (docs/agents.md). Taken for the block's when read, since
    /// nothing tells it from one written by hand, so that dropping the block
    /// gives back the file as it was before the block was added. One somebody
    /// put there goes with the block too: the price of that round trip,
    /// which leaving it would break for every file Mindweaver added to.
    spacer: Option<Line>,
    /// Whose markers it is spelled with: those it was found under, and
    /// Mindweaver's for one made anew.
    names: Names,
    begin: Line,
    /// Every line between the markers, as read or as rendered.
    lines: Vec<Line>,
    /// The route each section's marker names, in the order they come.
    routes: Vec<String>,
    end: Line,
}

/// One rule as the block holds it: its route, and the lines of its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    route: String,
    /// The body's lines without their endings, less the blank lines it opens
    /// and closes with.
    lines: Vec<String>,
}

impl Section {
    /// The section for `rule`, or `None` for a rule about some files only.
    ///
    /// A rule whose front matter states `paths`
    /// ([`RuleManifest::is_scoped`](crate::rule::RuleManifest::is_scoped))
    /// goes to Claude's rules folder alone (D-03, [`crate::claude_rules`]),
    /// where Claude loads it once one of those files is read: in `AGENTS.md`
    /// every agent would read it for every file. A `paths` written in a shape
    /// that is not read says nothing, so that rule has its section. The body
    /// is what an agent is given, [`Artifact::contents`], read from the file
    /// as it is now.
    pub fn of(rule: &Artifact) -> Result<Option<Self>, AgentsMdError> {
        let Declared::Rule(manifest) = rule.declared() else {
            return Err(AgentsMdError::NotARule {
                name: rule.qualified_name(),
            });
        };
        if manifest.is_scoped() {
            return Ok(None);
        }
        let body = rule.contents()?;
        Section::new(rule.name().as_str(), &body).map(Some)
    }

    /// The section for the rule at `route`, whose body, its front matter
    /// already taken off, is `body`.
    ///
    /// The route is held to the rules every name is ([`name_issues`]), which
    /// is also what keeps its marker one that reads back as itself: a route
    /// of lowercase words joined by single hyphens and slashes holds no space
    /// and no `--`, so it can neither end the comment early nor read as two
    /// words. A body line that would read back as one of the markers is
    /// refused too, since the block it was written into would end, or open a
    /// section, where the rule meant neither.
    ///
    /// The blank lines a body opens and closes with are left out, so how a
    /// rule's file spaces its front matter, or ends, does not change how the
    /// block is laid out: one blank line between two sections, whatever each
    /// file holds. Every other line is kept as written, trailing spaces
    /// included, since two of them are a line break in markdown.
    pub fn new(route: impl Into<String>, body: &str) -> Result<Self, AgentsMdError> {
        let route = route.into();
        let issues = name_issues(&route);
        if !issues.is_empty() {
            return Err(AgentsMdError::Route {
                problem: issue_list(&issues),
                route,
            });
        }
        // `str::lines` takes a `\r` off only before a `\n`; one left at the
        // end of a line would be read back as that line's ending, and the
        // block would differ from its own rendering on every render.
        let mut lines: Vec<String> = body
            .lines()
            .map(|line| line.trim_end_matches('\r').to_owned())
            .collect();
        let blank = |line: &String| line.trim().is_empty();
        while lines.last().is_some_and(blank) {
            lines.pop();
        }
        let opening = lines.iter().take_while(|line| blank(line)).count();
        lines.drain(..opening);
        if let Some(line) = lines
            .iter()
            .find(|line| !matches!(marker(line.as_bytes()), Marker::Text))
        {
            return Err(AgentsMdError::Marked {
                line: line.clone(),
                route,
            });
        }
        Ok(Section { route, lines })
    }

    /// The rule's route, which names it in its marker.
    pub fn route(&self) -> &str {
        &self.route
    }

    /// What the section says, as a hash ([`hash::bytes`]) of its body's
    /// lines, each ended with a newline: what its installation records of
    /// the copy in this file (D-04), beside the hash of the rule's own file.
    ///
    /// Of the lines rather than of the bytes the file holds, so a section
    /// reads alike whatever ending the file uses: an editor or git turning
    /// the file's newlines into another kind changes no rule, and is no edit
    /// of one. Without its marker, which says only whose it is.
    pub fn hash(&self) -> ContentHash {
        let mut said = String::new();
        for line in &self.lines {
            said.push_str(line);
            said.push('\n');
        }
        hash::bytes(said.as_bytes())
    }
}

/// Render `rules` into the block of `project`'s `AGENTS.md`, each that has a
/// [section](Section::of) once, in the order a listing gives them
/// ([`report_order`]).
///
/// The block holds those sections and nothing else afterwards. A file that
/// is not there is made with `create_new`, so one somebody made meanwhile is
/// never written over, and only then is [`Written::Created`] said: the
/// caller keeps that ([`crate::install::rules`], in the ledger) and hands
/// it back as `created` on a later render, so that when no rule is left
/// the file goes too, unless somebody has written something in it since.
/// Mindweaver takes away only a file it made.
/// A file that is there is replaced whole through a temporary and a rename
/// ([`replace_bytes`]), every byte outside the block as it was, and one that
/// already holds what was asked is not written at all, modification time
/// included.
///
/// A block that cannot be read, a rule that cannot be, and two rules of one
/// name are refused before anything is written: a section left out because
/// its rule could not be read would take that rule away from every agent
/// until the next render. The caller collects the refusal, as it does any
/// other one project's failure.
///
/// So is a block left in a folder whose `AGENTS.md` the agents of other
/// projects read too ([`misplaced`]): `workspace` is the one whose links say
/// where those are, and `homes` the agents' own folders, which the front end
/// decides as it does for an install. A render that leaves no block goes
/// ahead there, since it takes rules away from those agents rather than
/// giving them any. A file that is a link is written through to what it
/// points at, as a marker shared by a link is, but only inside the project
/// ([`linked_inside`]).
///
/// What the file then holds is checked against what Codex reads of it, and
/// anything that keeps the rules from Codex is said ([`Warning`]): only
/// while there is a block, since without one nothing of Mindweaver's is
/// there to miss.
///
/// Takes no lock: the caller holds the workspace's, as one calling
/// [`crate::config_entry::ConfigFile`] does, and the lock is not
/// re-entrant.
pub fn render<'a>(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    rules: impl IntoIterator<Item = &'a Artifact>,
    created: bool,
    homes: &AgentHomes,
) -> Result<Rendered, AgentsMdError> {
    render_keeping(workspace, project, rules, Vec::new(), created, homes)
}

/// [`render`], with `kept` among the sections, each as it is handed in: the
/// section the block holds of a rule its caller can no longer read from
/// where it was installed from, and keeps as the project holds it
/// ([`AgentsMd::section`]) rather than take it away with a change to some
/// other rule. In the order of their routes with the rest, which is the
/// order a listing gives the rules ([`report_order`]).
pub(crate) fn render_keeping<'a>(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    rules: impl IntoIterator<Item = &'a Artifact>,
    kept: Vec<Section>,
    created: bool,
    homes: &AgentHomes,
) -> Result<Rendered, AgentsMdError> {
    let file = project.root().join(FILE);
    linked_inside(project.root(), &file)?;
    let mut rules: Vec<&Artifact> = rules.into_iter().collect();
    rules.sort_by(|a, b| report_order(a, b));
    let mut sections = Vec::new();
    for rule in rules {
        sections.extend(Section::of(rule)?);
    }
    if !kept.is_empty() {
        sections.extend(kept);
        sections.sort_by(|a, b| a.route.cmp(&b.route));
    }
    // Only a render that leaves a block is held to where it is: one that
    // drops it is how a block put there before a project was linked below,
    // or before the one at the workspace's root was unlinked, comes out
    // again, and refusing it would keep the very leak the refusal is for.
    if !sections.is_empty() {
        if let Some(refused) = misplaced(workspace, homes, project.root(), &file) {
            return Err(refused.into());
        }
    }

    let read = read(&file)?;
    let mut text = AgentsMd::parse(read.as_deref().unwrap_or_default()).map_err(|problem| {
        AgentsMdError::Malformed {
            file: file.clone(),
            problem,
        }
    })?;
    text.set_block(&sections)?;
    let bytes = text.to_bytes();

    let left_empty = !text.has_block() && text.is_blank_outside();
    let written = match &read {
        None if !text.has_block() => Written::Unchanged,
        None => {
            create(&file, &bytes)?;
            Written::Created
        }
        // A link somebody put in place of the file is theirs, and so is
        // what it points at: emptied, never taken away.
        Some(_) if created && left_empty && is_plain_file(&file) => {
            fs::remove_file(&file).map_err(|source| AgentsMdError::Write {
                path: file.clone(),
                source,
            })?;
            Written::Removed
        }
        Some(old) if *old == bytes => Written::Unchanged,
        Some(_) => {
            replace_bytes(&file, &bytes)
                .map_err(|(path, source)| AgentsMdError::Write { path, source })?;
            Written::Changed
        }
    };
    let warnings = if text.has_block() {
        codex_warnings(project.root())
    } else {
        Vec::new()
    };
    Ok(Rendered {
        routes: text.routes().into_iter().map(str::to_owned).collect(),
        file,
        written,
        warnings,
    })
}

/// What [`render`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// The project's `AGENTS.md`, whether or not it is there now.
    pub file: PathBuf,
    pub written: Written,
    /// The route of each rule the block holds, in its order: none when there
    /// is no block. What the caller records as written
    /// ([`crate::install::rules`]), since a rule declaring `paths` is handed
    /// in and given no section.
    pub routes: Vec<String>,
    /// What keeps the block from reaching Codex, or will, in the order
    /// [`Warning`] lists them.
    pub warnings: Vec<Warning>,
}

impl Rendered {
    /// Whether the file was not there, and this render made it.
    pub fn created(&self) -> bool {
        self.written == Written::Created
    }
}

/// What [`render`] did to the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Written {
    /// It was not there, and now holds the block: made with `create_new`.
    Created,
    /// It was replaced whole, through a temporary and a rename.
    Changed,
    /// It was taken away: Mindweaver made it, and the block was all that was
    /// left in it.
    Removed,
    /// It already held what was asked, or was not there and was asked to
    /// hold nothing, and was left as it was.
    Unchanged,
}

/// Something about where a rendered `AGENTS.md` sits that keeps its rules
/// from Codex, in part or whole: said, and no reason not to render.
///
/// Codex reads one file a folder, `AGENTS.override.md` before `AGENTS.md`,
/// from the git root down to where it starts, root first, and cuts the whole
/// off at a budget (docs/agents.md, docs-only). Each is worked out for Codex
/// started at the project's root, where the file is. Claude and OpenCode read
/// past all three, so none is said for them.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// An `AGENTS.override.md` beside the file, which Codex reads in its
    /// place.
    #[error("{file}: Codex reads it in place of the AGENTS.md beside it, so the rules rendered there do not reach Codex")]
    Overridden { file: PathBuf },
    /// No git work tree holds the project, so Codex, finding no root to read
    /// down from, reads only the folder it starts in.
    #[error("{project}: in no git work tree, so Codex reads its AGENTS.md only when started in that folder itself")]
    NoGitRoot { project: PathBuf },
    /// What Codex reads from the git root down comes to [`CODEX_NEAR`] or
    /// more, and not past [`CODEX_BUDGET`].
    #[error("{project}: the AGENTS.md files Codex reads there come to {bytes} bytes, close to the {CODEX_BUDGET} it reads before cutting off the rest")]
    NearBudget {
        project: PathBuf,
        bytes: u64,
        /// The file Codex reads in each folder, root first.
        files: Vec<PathBuf>,
    },
    /// What Codex reads from the git root down comes to more than
    /// [`CODEX_BUDGET`], so it cuts off the end, where the project's own
    /// file is.
    #[error("{project}: the AGENTS.md files Codex reads there come to {bytes} bytes, past the {CODEX_BUDGET} it reads, so it cuts off the rest")]
    OverBudget {
        project: PathBuf,
        bytes: u64,
        /// The file Codex reads in each folder, root first.
        files: Vec<PathBuf>,
    },
    /// The repository, or a file Codex would read, could not be looked at,
    /// so how much Codex reads is not known.
    #[error("{project}: how much of its AGENTS.md Codex reads cannot be told: {detail}")]
    Unmeasured { project: PathBuf, detail: String },
}

codes!(Warning {
    Self::Overridden { .. } => "codex_overridden",
    Self::NoGitRoot { .. } => "codex_no_git_root",
    Self::NearBudget { .. } => "codex_near_budget",
    Self::OverBudget { .. } => "codex_over_budget",
    Self::Unmeasured { .. } => "codex_unmeasured",
});

/// What [`Warning`]s the `AGENTS.md` at `project`'s root, as it is on disk,
/// calls for: what a render says, and what doctor finds of the project as
/// it stands (`crate::doctor`), one reading of what Codex reads for both.
pub(crate) fn codex_warnings(project: &Path) -> Vec<Warning> {
    let mut warnings = Vec::new();
    let beside = project.join(OVERRIDE);
    if beside.is_file() {
        warnings.push(Warning::Overridden { file: beside });
    }
    let unmeasured = |detail: String| Warning::Unmeasured {
        project: project.to_path_buf(),
        detail,
    };
    // Every folder from the git root down to the project, root first.
    let folders = match exclude::work_tree(project) {
        Ok(Some((root, route))) => down_to(root, &route),
        Ok(None) => {
            warnings.push(Warning::NoGitRoot {
                project: project.to_path_buf(),
            });
            vec![project.to_path_buf()]
        }
        Err(error) => {
            warnings.push(unmeasured(error.to_string()));
            return warnings;
        }
    };
    let mut bytes = 0;
    let mut files = Vec::new();
    for folder in folders {
        for name in [OVERRIDE, FILE] {
            let path = folder.join(name);
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_file() => {
                    bytes += metadata.len();
                    files.push(path);
                    break;
                }
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => {
                    warnings.push(unmeasured(format!("{}: {error}", path.display())));
                    return warnings;
                }
            }
        }
    }
    let project = project.to_path_buf();
    if bytes > CODEX_BUDGET {
        warnings.push(Warning::OverBudget {
            project,
            bytes,
            files,
        });
    } else if bytes >= CODEX_NEAR {
        warnings.push(Warning::NearBudget {
            project,
            bytes,
            files,
        });
    }
    warnings
}

/// `root` and every folder below it on `route`, root first: the folders
/// Codex reads an `AGENTS.md` in, from the git root down to a project.
fn down_to(root: PathBuf, route: &Path) -> Vec<PathBuf> {
    let mut folder = root;
    let mut folders = vec![folder.clone()];
    for component in route.components() {
        if let Component::Normal(name) = component {
            folder.push(name);
            folders.push(folder.clone());
        }
    }
    folders
}

/// Every path [`codex_warnings`] reads for `project`, there or not, for a
/// front end to watch (`crate::watch`): each [`OVERRIDE`] and [`FILE`] from
/// the git root down to it. In no work tree, its own two, and a `.git` in
/// it and in each folder above that is not there, whose making gives Codex
/// a root to read down from: only one not there, since one that is
/// changes with every git command and holds nothing Codex reads.
pub(crate) fn codex_looked_at(project: &Path) -> Vec<PathBuf> {
    let (folders, rootless) = match exclude::work_tree(project) {
        Ok(Some((root, route))) => (down_to(root, &route), false),
        Ok(None) => (vec![project.to_path_buf()], true),
        // Nothing past the project's own is read of a repository that does
        // not open.
        Err(_) => (vec![project.to_path_buf()], false),
    };
    let mut paths: Vec<PathBuf> = folders
        .iter()
        .flat_map(|folder| [folder.join(OVERRIDE), folder.join(FILE)])
        .collect();
    if rootless {
        paths.extend(
            project
                .ancestors()
                .map(|folder| folder.join(".git"))
                .filter(|git| fs::symlink_metadata(git).is_err()),
        );
    }
    paths
}

/// Why `file` is no place for rules rendered for the project at `folder`,
/// when agents beyond that project's would read them there.
///
/// Claude reads every `AGENTS.md` from where it starts up to the root, and
/// OpenCode V2 up to the home directory, so one in a folder above a project
/// is read in that project too (docs/agents.md, docs-only for reading more
/// than one): rules rendered into a monorepo's root would reach every
/// package linked below it, beside the package's own. So a folder above a
/// project the workspace links is refused, a stale entry's included, since
/// its project may only have moved for a moment. So is the workspace's own
/// root, unless a project it links is there: `mindweaver init` and the
/// workspace's own artifacts put nothing in it ([`WeaverWorkspace::own`] is
/// a holder at that root, not a project), while a project at the root is a
/// project like any other, and its `AGENTS.md` its own (D-03).
///
/// So is a `file` inside one of the agents' own folders, as an install's
/// copy is ([`crate::install::InstallError::UserScope`]): a project at
/// `$CODEX_HOME` would write the `AGENTS.md` Codex reads in every project,
/// and one at `~/.claude` the `.claude/AGENTS.md` Claude finds above every
/// project in the user's home. Inside rather than equal, for the reason an
/// install gives: nothing in one of those folders is any one project's.
///
/// Every renderer of a project's rules asks this of the file it would
/// write, so none of them leaves rules where the others refuse to.
pub(crate) fn misplaced(
    workspace: &WeaverWorkspace,
    homes: &AgentHomes,
    folder: &Path,
    file: &Path,
) -> Option<Misplaced> {
    if let Some((file, home, reader)) = install::in_agent_home(homes, &[file.to_path_buf()]) {
        return Some(Misplaced::UserScope { file, home, reader });
    }
    if same_directory(folder, workspace.root()) && !workspace.is_linked(folder) {
        return Some(Misplaced::WorkspaceRoot {
            folder: folder.to_path_buf(),
        });
    }
    let below: Vec<PathBuf> = workspace
        .members()
        .into_iter()
        .map(|member| member.root)
        .filter(|root| is_below(root, folder))
        .collect();
    if !below.is_empty() {
        return Some(Misplaced::AboveProjects {
            folder: folder.to_path_buf(),
            projects: below,
        });
    }
    None
}

/// Why rules are not written where a project would have them: what
/// [`misplaced`] finds, for each renderer to refuse in words of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Misplaced {
    /// The file is inside the folder `reader` reads for every project.
    UserScope {
        file: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    /// The project is at the workspace's root, which links no project there.
    WorkspaceRoot { folder: PathBuf },
    /// The project is above these, which the workspace links.
    AboveProjects {
        folder: PathBuf,
        projects: Vec<PathBuf>,
    },
}

impl From<Misplaced> for AgentsMdError {
    fn from(misplaced: Misplaced) -> Self {
        match misplaced {
            Misplaced::UserScope { file, home, reader } => {
                AgentsMdError::UserScope { file, home, reader }
            }
            Misplaced::WorkspaceRoot { folder } => AgentsMdError::WorkspaceRoot { folder },
            Misplaced::AboveProjects { folder, projects } => {
                AgentsMdError::AboveProjects { folder, projects }
            }
        }
    }
}

/// Whether `path` is inside `folder` and not `folder` itself: by the route
/// from one to the other, then through the filesystem, so a spelling through
/// a link is still below what it leads into.
fn is_below(path: &Path, folder: &Path) -> bool {
    let down = |route: PathBuf| {
        route
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    };
    if paths::relative_to(path, folder).is_some_and(down) {
        return true;
    }
    match (crate::paths::canonical(path), crate::paths::canonical(folder)) {
        (Ok(path), Ok(folder)) => path
            .strip_prefix(&folder)
            .is_ok_and(|route| route.components().next().is_some()),
        _ => false,
    }
}

/// Refuse an `AGENTS.md` that is a link to a file outside `project`.
///
/// One that is a link to a file in the project, `AGENTS.md` to `CLAUDE.md`
/// as some teams keep the two, is written through, as [`replace_bytes`]
/// follows a link to the file it names rather than replacing the link:
/// Claude then reads the block as that `CLAUDE.md`, and no import goes in
/// beside it ([`crate::claude_md::Reach::Same`]). One
/// leading out is not: a repository can hold a link to the user's own
/// `~/.codex/AGENTS.md`, which every project would then be given this one's
/// rules through. Refused rather than replaced by a file of the project's
/// own, which would undo what somebody linked on purpose. A link to nothing
/// is let by: there is nothing to read, and `create_new` makes nothing
/// through it.
pub(crate) fn linked_inside(project: &Path, file: &Path) -> Result<(), AgentsMdError> {
    let is_link = fs::symlink_metadata(file).is_ok_and(|metadata| metadata.is_symlink());
    if !is_link {
        return Ok(());
    }
    let Ok(target) = crate::paths::canonical(file) else {
        return Ok(());
    };
    let root = crate::paths::canonical(project).map_err(|source| AgentsMdError::Read {
        path: project.to_path_buf(),
        source,
    })?;
    if target.starts_with(&root) {
        Ok(())
    } else {
        Err(AgentsMdError::LinkedOut {
            file: file.to_path_buf(),
            target,
        })
    }
}

/// The file's bytes, `None` when it is not there.
fn read(path: &Path) -> Result<Option<Vec<u8>>, AgentsMdError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(AgentsMdError::Read {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Write a new file whole, refusing one that is there by now.
///
/// `create_new` rather than a temporary renamed into place, which would put
/// this file over one somebody made after the read found none. A write that
/// fails takes away what it made, so no half of a block is left to be refused
/// as malformed later.
fn create(path: &Path, contents: &[u8]) -> Result<(), AgentsMdError> {
    let write_error = |source| AgentsMdError::Write {
        path: path.to_path_buf(),
        source,
    };
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(write_error)?;
    if let Err(source) = file.write_all(contents) {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(write_error(source));
    }
    Ok(())
}

/// Whether `path` is a file itself, not a link to one.
fn is_plain_file(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
}

impl AgentsMd {
    /// Read an `AGENTS.md`'s bytes. An absent file is an empty one.
    ///
    /// Line by line, not as markdown: a marker quoted in somebody's code
    /// fence is a marker here, and refused unless it makes one well-formed
    /// block. Telling a fence apart was the alternative, at the price of a
    /// markdown parser for a mistake that is refused, never acted on.
    pub fn parse(bytes: &[u8]) -> Result<Self, Malformed> {
        let lines = lines::split(bytes);
        let ending = lines::ending(&lines);
        let Some(span) = locate(&lines)? else {
            return Ok(AgentsMd {
                before: lines,
                block: None,
                after: Vec::new(),
                ending,
            });
        };

        let mut lines = lines.into_iter();
        let mut before: Vec<Line> = lines.by_ref().take(span.begin).collect();
        let spacer = before.pop_if(|line| line.text.is_empty());
        let begin_line = lines.next().expect("the begin marker was found");
        let inside: Vec<Line> = lines.by_ref().take(span.end - span.begin - 1).collect();
        let end_line = lines.next().expect("the end marker was found");
        Ok(AgentsMd {
            before,
            block: Some(Block {
                spacer,
                names: span.names,
                begin: begin_line,
                lines: inside,
                routes: span.rules.into_iter().map(|(_, route)| route).collect(),
                end: end_line,
            }),
            after: lines.collect(),
            ending,
        })
    }

    /// The route of each rule the block holds, in the order it holds them:
    /// none when there is no block.
    pub fn routes(&self) -> Vec<&str> {
        self.block
            .iter()
            .flat_map(|block| &block.routes)
            .map(String::as_str)
            .collect()
    }

    /// The section the block holds for the rule at `route`, as written
    /// there, an edit included: `None` when the block holds none, or a line
    /// of it that is not UTF-8, which no section is rendered from.
    ///
    /// Its lines run from its marker to the next one, or to the end of the
    /// block, less the blank lines [`Section::new`] leaves out, the one
    /// between two sections among them, so a section read back is the one
    /// that was rendered.
    pub fn section(&self, route: &str) -> Option<Section> {
        let block = self.block.as_ref()?;
        let mut lines = block
            .lines
            .iter()
            .skip_while(|line| marker(&line.text) != Marker::Rule(block.names, route));
        lines.next()?;
        let body: Vec<&str> = lines
            .take_while(|line| marker(&line.text) == Marker::Text)
            .map(|line| std::str::from_utf8(&line.text).ok())
            .collect::<Option<_>>()?;
        Section::new(route, &body.join("\n")).ok()
    }

    /// Whether the file holds Mindweaver's block.
    pub fn has_block(&self) -> bool {
        self.block.is_some()
    }

    /// Whose markers the block is spelled with: `None` when there is none.
    pub fn names(&self) -> Option<Names> {
        self.block.as_ref().map(|block| block.names)
    }

    /// Spell every marker of the block with `names`' markers, and whether
    /// that changed anything: what `migrate --names` does to a block a
    /// Mindflayer build wrote, every byte but the markers kept as it was,
    /// each line ending among them, so no section's hash
    /// ([`Section::hash`]), which is of its body alone, changes.
    pub fn renamed(&mut self, names: Names) -> bool {
        let Some(block) = &mut self.block else {
            return false;
        };
        if block.names == names {
            return false;
        }
        let (begin, end, rule) = markers(names);
        block.begin = Line::new(begin, block.begin.end);
        block.end = Line::new(end, block.end.end);
        for line in &mut block.lines {
            if let Marker::Rule(_, route) = marker(&line.text) {
                *line = Line::new(&format!("{rule}{route}{CLOSE}"), line.end);
            }
        }
        block.names = names;
        true
    }

    /// Whether nothing but blank lines is left outside the block: what a
    /// file Mindweaver made holds once somebody has written nothing in it.
    pub fn is_blank_outside(&self) -> bool {
        self.before.iter().chain(&self.after).all(Line::is_blank)
    }

    /// Make the block hold `sections`, in that order and nothing else, and
    /// whether that changed anything.
    ///
    /// Everything between the markers is Mindweaver's, so it is replaced
    /// whole, a line somebody wrote there included; telling an edited section
    /// from the one rendered is for the ledger's record of what was written
    /// ([`Section::hash`], D-04). A file with no block gains one after what
    /// it holds, on lines of its own and an empty line apart from it
    /// (`Block::spacer`).
    /// No section drops the block, that empty line included, giving back the
    /// file as it was before the block was written.
    pub fn set_block(&mut self, sections: &[Section]) -> Result<bool, AgentsMdError> {
        for (index, section) in sections.iter().enumerate() {
            if sections[..index]
                .iter()
                .any(|other| other.route == section.route)
            {
                return Err(AgentsMdError::Twice {
                    route: section.route.clone(),
                });
            }
        }
        if sections.is_empty() {
            return Ok(self.drop_block());
        }

        let ending = self.ending;
        let routes = sections
            .iter()
            .map(|section| section.route.clone())
            .collect();
        if let Some(block) = &mut self.block {
            // Under the markers it was found under, which a render never
            // moves.
            let lines = rendered(sections, ending, block.names);
            if block.lines == lines {
                return Ok(false);
            }
            block.lines = lines;
            block.routes = routes;
            return Ok(true);
        }
        // The new block goes on lines of its own, so a last line without a
        // newline gets one, and the end marker goes without one instead: the
        // file still ends the way it did, and dropping the block takes the
        // newline back. The empty line above it comes after anything at all,
        // a blank last line included: one left out there would read back as
        // a spacer, and dropping the block would take the file's own.
        let mut close = ending;
        if let Some(last) = self.before.last_mut() {
            if last.end.is_empty() {
                last.end = ending;
                close = b"";
            }
        }
        let spacer = (!self.before.is_empty()).then(|| Line::new("", ending));
        self.block = Some(Block {
            spacer,
            names: Names::Mindweaver,
            begin: Line::new(BEGIN, ending),
            lines: rendered(sections, ending, Names::Mindweaver),
            routes,
            end: Line::new(END, close),
        });
        Ok(true)
    }

    /// Take the block out, markers and all, and whether there was one.
    fn drop_block(&mut self) -> bool {
        let Some(block) = self.block.take() else {
            return false;
        };
        // An end marker without a newline is the file's last line, so the
        // file ended without one and still does once the block is gone.
        let after = std::mem::take(&mut self.after);
        self.before.extend(after);
        if block.end.end.is_empty() {
            if let Some(last) = self.before.last_mut() {
                last.end = b"";
            }
        }
        true
    }

    /// The file as it is to be written.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut emit = |line: &Line| {
            out.extend_from_slice(&line.text);
            out.extend_from_slice(line.end);
        };
        self.before.iter().for_each(&mut emit);
        if let Some(block) = &self.block {
            block.spacer.iter().for_each(&mut emit);
            emit(&block.begin);
            block.lines.iter().for_each(&mut emit);
            emit(&block.end);
        }
        self.after.iter().for_each(&mut emit);
        out
    }
}

/// Where Mindweaver's block lies among a file's lines, as [`locate`] finds
/// it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Span {
    /// The begin marker's index among the lines.
    pub(crate) begin: usize,
    /// The end marker's.
    pub(crate) end: usize,
    /// Whose markers the block is spelled with, every one of them alike.
    pub(crate) names: Names,
    /// Each rule's marker between the two, in order: its index among the
    /// lines, and the route it names.
    pub(crate) rules: Vec<(usize, String)>,
}

/// Where Mindweaver's block is among `lines`: `None` when they hold no
/// marker, refused unless they hold exactly one well-formed block.
///
/// One search for every markdown file Mindweaver keeps a block in, this
/// file and the `CLAUDE.md` that imports it ([`crate::claude_md`]), so a
/// marker reads the same in each and one written wrong is refused alike.
///
/// Under either names, the block's every marker spelled as its begin marker
/// is: one spelled the other way inside it is refused, as a second block is
/// beside it, rather than read as either build would.
pub(crate) fn locate(lines: &[Line]) -> Result<Option<Span>, Malformed> {
    let mut begin: Option<(usize, Names)> = None;
    let mut end = None;
    let mut rules: Vec<(usize, String)> = Vec::new();
    let mixed = |line: usize, found: Names, opened: Option<(usize, Names)>| match opened {
        Some((_, block)) if block != found => Some(Malformed::Mixed { line, found, block }),
        _ => None,
    };
    for (index, line) in lines.iter().enumerate() {
        let number = index + 1;
        match marker(&line.text) {
            Marker::Text => {}
            Marker::Begin(names) => {
                if begin.is_some() {
                    return Err(Malformed::Reopened {
                        line: number,
                        names,
                    });
                }
                begin = Some((index, names));
            }
            Marker::End(names) => {
                if begin.is_none() || end.is_some() {
                    return Err(Malformed::Unopened {
                        line: number,
                        names,
                    });
                }
                if let Some(mixed) = mixed(number, names, begin) {
                    return Err(mixed);
                }
                end = Some(index);
            }
            Marker::Rule(names, route) => {
                if begin.is_none() || end.is_some() {
                    return Err(Malformed::Stray { line: number });
                }
                if let Some(mixed) = mixed(number, names, begin) {
                    return Err(mixed);
                }
                if rules.iter().any(|(_, seen)| seen == route) {
                    return Err(Malformed::Repeated {
                        line: number,
                        route: route.to_owned(),
                    });
                }
                rules.push((index, route.to_owned()));
            }
            Marker::Miswritten => return Err(Malformed::Miswritten { line: number }),
        }
    }
    match (begin, end) {
        (None, _) => Ok(None),
        (Some((begin, names)), None) => Err(Malformed::Unclosed {
            line: begin + 1,
            names,
        }),
        (Some((begin, names)), Some(end)) => Ok(Some(Span {
            begin,
            end,
            names,
            rules,
        })),
    }
}

/// The lines between the markers for `sections`: each rule's marker, spelled
/// under `names`, then its body, with a blank line between one section and
/// the next.
///
/// The blank line keeps two rules two paragraphs once Claude has stripped the
/// markers, which would otherwise leave the last line of one against the
/// first of the next.
fn rendered(sections: &[Section], ending: &'static [u8], names: Names) -> Vec<Line> {
    let (_, _, rule) = markers(names);
    let mut lines = Vec::new();
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            lines.push(Line::new("", ending));
        }
        lines.push(Line::new(
            &format!("{rule}{}{CLOSE}", section.route),
            ending,
        ));
        lines.extend(section.lines.iter().map(|line| Line::new(line, ending)));
    }
    lines
}

/// What a line of the file is to the block, each marker with whose names it
/// is spelled under.
#[derive(Debug, PartialEq, Eq)]
enum Marker<'a> {
    /// Anything that is not a marker.
    Text,
    Begin(Names),
    End(Names),
    /// A rule's marker, and the route it names.
    Rule(Names, &'a str),
    /// A comment that opens as a marker does and is none of them: indented,
    /// spaced otherwise, naming a mark there is not, or a route that is no
    /// name.
    Miswritten,
}

/// What `text`, one line without its ending, is to the block.
///
/// A marker is the line exactly as Mindweaver writes it, trailing whitespace
/// forgiven as an editor may add some. Anything else that opens as a comment
/// naming a mark is a marker written wrong, and refused rather than read as
/// text: read as text, a hand-indented begin marker would leave the next
/// render to write a second block below the first.
fn marker(text: &[u8]) -> Marker<'_> {
    let line = text.trim_ascii_end();
    for names in Names::ALL {
        let (begin, end, rule) = markers(names);
        if line == begin.as_bytes() {
            return Marker::Begin(names);
        }
        if line == end.as_bytes() {
            return Marker::End(names);
        }
        if let Some(route) = line
            .strip_prefix(rule.as_bytes())
            .and_then(|rest| rest.strip_suffix(CLOSE.as_bytes()))
        {
            return match std::str::from_utf8(route) {
                Ok(route) if name_issues(route).is_empty() => Marker::Rule(names, route),
                _ => Marker::Miswritten,
            };
        }
    }
    let comment = text
        .trim_ascii()
        .strip_prefix(b"<!--")
        .map(<[u8]>::trim_ascii_start);
    let opens_a_mark = comment.is_some_and(|comment| {
        MARKS.iter().any(|mark| {
            comment
                .get(..mark.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(mark))
        })
    });
    if opens_a_mark {
        Marker::Miswritten
    } else {
        Marker::Text
    }
}

/// Why the block in an `AGENTS.md` is not one Mindweaver can replace. Lines
/// are counted from 1.
///
/// Each that quotes a marker quotes it as the file spells it, under `names`:
/// a block a Mindflayer build wrote is refused in its own words.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Malformed {
    #[error("line {line}: `{}` is never closed by `{}`", markers(*.names).0, markers(*.names).1)]
    Unclosed { line: usize, names: Names },
    #[error("line {line}: `{}` closes no `{}`", markers(*.names).1, markers(*.names).0)]
    Unopened { line: usize, names: Names },
    #[error("line {line}: a second `{}`, where there can be one block", markers(*.names).0)]
    Reopened { line: usize, names: Names },
    /// A marker spelled under other names than the block's begin marker:
    /// one a Mindflayer build wrote in a block this one opened, or the
    /// other way round.
    #[error(
        "line {line}: a marker spelled as {found} spells them, in a block {block}'s \
         markers open, where every marker of a block is spelled alike"
    )]
    Mixed {
        line: usize,
        found: Names,
        block: Names,
    },
    #[error("line {line}: a rule's marker outside the block")]
    Stray { line: usize },
    #[error("line {line}: a second section for the rule `{route}` in the block")]
    Repeated { line: usize, route: String },
    #[error(
        "line {line}: written like one of Mindweaver's markers but none of them, \
         which are each a line of their own, as Mindweaver writes them"
    )]
    Miswritten { line: usize },
    /// A rule's marker in a file whose block holds no rules: the import in
    /// a `CLAUDE.md` ([`crate::claude_md`]).
    #[error(
        "line {line}: a rule's marker, where Mindweaver's block holds only its import of AGENTS.md"
    )]
    Ruled { line: usize },
}

codes!(Malformed {
    Self::Unclosed { .. } => "markdown_block_unclosed",
    Self::Unopened { .. } => "markdown_block_unopened",
    Self::Reopened { .. } => "markdown_block_reopened",
    Self::Mixed { .. } => "markdown_markers_mixed",
    Self::Stray { .. } => "markdown_marker_stray",
    Self::Repeated { .. } => "markdown_section_repeated",
    Self::Miswritten { .. } => "markdown_marker_miswritten",
    Self::Ruled { .. } => "markdown_rule_in_import",
});

/// Why a project's rules could not be rendered into its `AGENTS.md`.
#[derive(Debug, Error)]
pub enum AgentsMdError {
    #[error("rule `{route}` cannot be named in AGENTS.md: {problem}")]
    Route { route: String, problem: String },
    #[error(
        "rule `{route}` holds a line that would be read back as one of Mindweaver's markers, \
         so it is not written into AGENTS.md: {line}"
    )]
    Marked { route: String, line: String },
    #[error("two rules are named `{route}`, and AGENTS.md holds one section a name")]
    Twice { route: String },
    #[error("`{name}` is not a rule, so it has no section in AGENTS.md")]
    NotARule { name: String },
    #[error(
        "{folder}: the workspace's own folder, where it links no project, so no AGENTS.md \
         is written there: the agents of every project below would read it"
    )]
    WorkspaceRoot { folder: PathBuf },
    #[error(
        "{folder}: above {}, which the workspace links, so no AGENTS.md is written there: \
         their agents would read it too",
        spelled(.projects)
    )]
    AboveProjects {
        folder: PathBuf,
        projects: Vec<PathBuf>,
    },
    #[error(
        "{file} is inside {home}, which {reader} reads for every project, so no rules are \
         written into it: Mindweaver gives rules to one project at a time"
    )]
    UserScope {
        file: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
    #[error(
        "{file}: a link to {target}, outside the project, so it is not written through: \
         what it points at is not the project's to change"
    )]
    LinkedOut { file: PathBuf, target: PathBuf },
    #[error(transparent)]
    Rule(#[from] ArtifactError),
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

codes!(AgentsMdError {
    Self::Route { .. } => "agents_md_route_unusable",
    Self::Marked { .. } => "agents_md_marker_in_rule",
    Self::Twice { .. } => "agents_md_route_twice",
    Self::NotARule { .. } => "agents_md_not_a_rule",
    Self::WorkspaceRoot { .. } => "agents_md_workspace_root",
    Self::AboveProjects { .. } => "agents_md_above_projects",
    Self::UserScope { .. } => "agents_md_user_scope",
    Self::LinkedOut { .. } => "agents_md_linked_out",
    Self::Read { .. } => "agents_md_read_failed",
    Self::Write { .. } => "agents_md_write_failed",
} carried {
    Self::Rule(error) => error,
    Self::Malformed { problem, .. } => problem,
});

#[cfg(test)]
#[path = "agents_md_test.rs"]
mod tests;
