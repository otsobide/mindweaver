//! Installing a rule into a project, and taking it back out.
//!
//! A project marked by its settings is given its rules as sections of a
//! block in its `AGENTS.md` ([`crate::agents_md`]), the one file Claude Code,
//! Codex and OpenCode all read (D-03), and Claude reads that block natively,
//! through an import in the project's own `CLAUDE.md`
//! ([`crate::claude_md`]), or not at all, when its own rule files carry every
//! rule instead ([`crate::claude_rules`]). The block is one file, written
//! whole, so a project's rules are rendered together: installing one, or
//! taking one out, renders every rule the project has, each from where it was
//! installed from, as it is there now. A project a `mind.toml` marks has no
//! block: a rule is copied into the folder its marker names, filed by its
//! route, as a skill is copied into its folder.
//!
//! What is Mindweaver's is what the ledger records, as for a skill: one
//! installation a rule, with a target for its section, named by the file it
//! is in, and one for its file for Claude, each with the hash of what it
//! holds, and whether Mindweaver made the `AGENTS.md`, so that the file goes
//! with the last rule only when it was Mindweaver's to begin with. A
//! section in the block that the ledger does not record is somebody's, as a
//! copy nothing records is, and no render writes over it.
//!
//! In a local project (D-02), an `AGENTS.md` Mindweaver made is kept out of
//! git, and one git tracks is written only with `edit_tracked`: otherwise no
//! block is written, Claude gets every rule from its own rule files, and the
//! agents that read rules only in the block are said to miss them
//! ([`Warning::Withheld`]).

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use thiserror::Error;

use super::{
    copy_at, kept_by_a_load, local, project_entry, route, same_place, standing, targets,
    unrecorded, user_scope, Candidate, Edits, InstallError, Installation, Installed, Offer,
    Removed, Standing,
};
use crate::agent::{Agent, AgentHomes};
use crate::agents_md::{self, AgentsMd, AgentsMdError, Section, Written as BlockWritten};
use crate::artifact::Artifact;
use crate::claude_md;
use crate::claude_rules::{self, Mode, Recorded};
use crate::code::{codes, Said};
use crate::copy;
use crate::exclude::{self, Excluded};
use crate::hash;
use crate::kind::Kind;
use crate::ledger::{Action, InstallRecord, Ledger, Outcome, Target};
use crate::names::Names;
use crate::paths;
use crate::settings::Share;
use crate::value::{ArtifactName, ContentHash, Origin};
use crate::workspace::{replace_bytes_unresolved, MindProject, WeaverWorkspace, WorkspaceError};

/// How the ledger names a project's `AGENTS.md`: its route from the project,
/// as it names every copy.
const AGENTS_MD: &str = agents_md::FILE;

/// What rendering a project's rules did: to the block, to the import, and to
/// Claude's rule files, and what there is to say of any of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// What rendering the block of `AGENTS.md` did, or `None` when it was
    /// withheld from the file ([`Warning::Withheld`]).
    pub block: Option<agents_md::Rendered>,
    /// How Claude reaches the block ([`claude_md::Rendered::reach`]), and
    /// what that did to the project's own `CLAUDE.md` files.
    pub import: claude_md::Rendered,
    /// What Claude's rule files came to: every rule's in fallback mode, only
    /// those about some files otherwise, and none for a project that does not
    /// name Claude.
    pub files: claude_rules::Rendered,
    /// What keeping an `AGENTS.md` Mindweaver made out of git did, in a local
    /// project; `None` where nothing had to be. [`Excluded::warning`] says
    /// when nothing keeps it out, outside a git work tree.
    pub excluded: Option<Excluded>,
    /// Why the exclude block still names a file that stopped being
    /// Mindweaver's, an `AGENTS.md` or a rule's file for Claude, when it
    /// could not be edited: a line left there keeps whatever is in that place
    /// out of git.
    pub still_excluded: Option<String>,
    /// What there is to say about the rules as a whole, beside what each
    /// renderer says ([`agents_md::Rendered::warnings`],
    /// [`claude_md::Rendered::warnings`], [`claude_rules::Rendered::warnings`]).
    pub warnings: Vec<Warning>,
}

impl Rendered {
    /// Everything a render made for a change of the rule `changed` has to
    /// warn of, each as a front end says it after `warning: `: the rules as
    /// a whole first ([`Warning`]), which say which agents read none of
    /// them, then the block's, the import's and Claude's files' in the order
    /// they were rendered. Every one, for a render that changed no rule.
    ///
    /// Each with its code beside its message ([`Said`]), which a front end
    /// carries beside the line it says: the warnings are of four types, and
    /// a caller handed text alone could not tell the panel which is which.
    ///
    /// Gathered here rather than by each front end, so none says one
    /// renderer's warnings and forgets another's. Not what is true of
    /// another rule whatever the render: that it reaches Claude alone, or no
    /// agent. That was said when it went in, `status` says it whenever
    /// asked, and saying it again would make an install of one rule warn,
    /// and fail, for another. Nor `still_excluded`, which an uninstall says
    /// of the rule it took out ([`Removed::Removed`]), and an install of the
    /// project.
    pub fn warnings_of(&self, changed: Option<&str>) -> Vec<Said> {
        self.warnings_about(|route| changed.is_none_or(|changed| changed == route), true)
    }

    /// What a render that brought the rules `updated` up to date has to
    /// warn of, as [`Rendered::warnings_of`] says it of one rule changed:
    /// what is true of each of them, and of the rules as a whole. Not an
    /// edit it kept, in a section or a file for Claude, which the update
    /// says itself of every copy it leaves alone, as it says a skill's
    /// ([`crate::update::Step::Leave`]).
    pub fn warnings_of_update(&self, updated: &[&str]) -> Vec<Said> {
        self.warnings_about(|route| updated.contains(&route), false)
    }

    /// Every warning of the render, those about one rule only while it is
    /// `about` that rule, and those about an edit it kept only with `kept`.
    fn warnings_about(&self, about: impl Fn(&str) -> bool, kept: bool) -> Vec<Said> {
        let rules = self.warnings.iter().filter(|warning| match warning {
            Warning::Unreached { route } => about(route),
            Warning::Kept { .. } => kept,
            Warning::Withheld { .. }
            | Warning::Left { .. }
            | Warning::Edited { .. }
            | Warning::Unremoved { .. } => true,
        });
        let block = self.block.iter().flat_map(|block| &block.warnings);
        let files = self.files.warnings.iter().filter(|warning| match warning {
            claude_rules::Warning::ClaudeOnly { route, .. } => about(route),
            claude_rules::Warning::Edited { .. } => kept,
            claude_rules::Warning::Foreign { .. } | claude_rules::Warning::Failed { .. } => true,
        });
        rules
            .map(|warning| Said::of(warning.code(), warning))
            .chain(block.map(|warning| Said::of(warning.code(), warning)))
            .chain(
                self.import
                    .warnings
                    .iter()
                    .map(|warning| Said::of(warning.code(), warning)),
            )
            .chain(files.map(|warning| Said::of(warning.code(), warning)))
            .collect()
    }

    /// What nothing went wrong with but is worth knowing, each as a front
    /// end says it after `note: `: that what the render kept out of git is
    /// kept out of nothing, outside a git work tree ([`Excluded::warning`]).
    /// Once, though the block and Claude's files each find it.
    pub fn notes(&self) -> Vec<String> {
        let mut notes: Vec<String> = Vec::new();
        for note in [&self.excluded, &self.files.excluded]
            .into_iter()
            .flatten()
            .filter_map(Excluded::warning)
        {
            if !notes.contains(&note) {
                notes.push(note);
            }
        }
        notes
    }
}

/// Something a render of a project's rules has to say, which kept nothing
/// from being written.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Warning {
    /// No block was written into `file`, for `why`, so `missed`, the agents
    /// the project names that read rules only there, read none of them.
    /// Claude, when it is named, has every rule from its own rule files.
    #[error("{file}: {why}, so no rules are written there{}", missing(.missed))]
    Withheld {
        file: PathBuf,
        why: Withheld,
        missed: Vec<Agent>,
    },
    /// `file` holds a block of rules written before, which is no longer
    /// rendered, for `why`, and is left as it is: the agents that read the
    /// file read the rules it held then.
    #[error("{file}: holds Mindweaver's block of rules, no longer rendered, and is left as it is: {why}")]
    Left { file: PathBuf, why: Withheld },
    /// The rule reaches none of the agents the project names: one about some
    /// files only in a project that does not name Claude, the one agent that
    /// reads such a rule, or one whose block was withheld there.
    #[error("rule `{route}` reaches none of the agents the project names")]
    Unreached { route: String },
    /// The rule's copy from before the project had settings, in the folder
    /// it authors in ([`copy_in`]), holds something other than what was
    /// copied there: left as it is, the project's own from now on, and no
    /// longer recorded, as a copy changed before a change of agents is.
    #[error("{path}: changed since rule `{route}` was copied there, before the project had settings, so it is left as it is, the project's own")]
    Edited { route: String, path: PathBuf },
    /// The rule's section in `file` holds something other than what was
    /// rendered there: somebody's edit, kept as the block holds it, and its
    /// record with it, so it reads as edited until it is undone, or the rule
    /// is installed again with `--force` (D-04).
    #[error("{file}: the section of rule `{route}` was changed since it was rendered, so it is left as it is")]
    Kept { route: String, file: PathBuf },
    /// Such a copy, still what was copied, that could not be taken out: it
    /// stays recorded, for the next render to take out.
    #[error("{path}: rule `{route}` was copied there before the project had settings, and could not be taken out: {error}")]
    Unremoved {
        route: String,
        path: PathBuf,
        error: String,
    },
}

codes!(Warning {
    Self::Withheld { .. } => "rules_block_withheld",
    Self::Left { .. } => "rules_block_left",
    Self::Unreached { .. } => "rule_unreached",
    Self::Edited { .. } => "rule_copy_edited",
    Self::Kept { .. } => "rule_section_kept",
    Self::Unremoved { .. } => "rule_copy_unremoved",
});

/// Why no block was written into a project's `AGENTS.md`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Withheld {
    /// Git tracks it, in a local project that does not set `edit_tracked`
    /// (D-02).
    #[error("git tracks it, and the project is local without edit_tracked")]
    Tracked,
    /// Whether git tracks it cannot be told, so it is left as if it did.
    #[error("whether git tracks it cannot be told ({0})")]
    Untold(String),
}

codes!(Withheld {
    Self::Tracked => "withheld_tracked",
    Self::Untold(_) => "withheld_untold",
});

/// ", and codex and opencode read none of them", or nothing for no agent.
fn missing(missed: &[Agent]) -> String {
    let names: Vec<&str> = missed.iter().map(|agent| agent.name()).collect();
    match names.split_last() {
        None => String::new(),
        Some((last, [])) => format!(", and {last} reads none of them"),
        Some((last, rest)) => format!(", and {} and {last} read none of them", rest.join(", ")),
    }
}

/// Refuse a project marked by its settings that names no agent: the block
/// reaches each agent it names, and there is none to reach.
pub(crate) fn receives(project: &MindProject) -> Result<(), InstallError> {
    match project.settings() {
        Some(settings) if settings.agents.is_empty() => Err(InstallError::NoTarget {
            project: project.name().to_owned(),
            kind: Kind::Rule,
        }),
        _ => Ok(()),
    }
}

/// `offers`, each with how it stands in `project`, which its settings mark:
/// [`Held::standing`].
pub(crate) fn survey(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    offers: &[Offer],
) -> Result<Vec<Candidate>, InstallError> {
    let held = Held::read(workspace, ledger, project)?;
    Ok(offers
        .iter()
        .map(|offer| Candidate {
            standing: held.standing(workspace, project, offer.name(), offers).0,
            offer: offer.clone(),
        })
        .collect())
}

/// What a project marked by its settings holds of rules: what the ledger
/// records, and the block of its `AGENTS.md`.
struct Held {
    recorded: Vec<InstallRecord>,
    /// The file as it reads now: none when it is not there, or cannot be
    /// read as one, which a render then refuses, saying why.
    file: Option<AgentsMd>,
}

impl Held {
    fn read(
        workspace: &WeaverWorkspace,
        ledger: &Ledger,
        project: &MindProject,
    ) -> Result<Held, InstallError> {
        Ok(Held {
            recorded: ledger.records(&project_entry(workspace, project), Kind::Rule)?,
            file: block_file(project.root()),
        })
    }

    /// How the rule at `route` stands in `project`, given every rule offered
    /// (`offers`), and where somebody's is when it is somebody's.
    ///
    /// While the ledger records it, as its section and its file for Claude
    /// are found against the hash recorded of each, and as the place it came
    /// from offers it now ([`Standing`]): the next render puts back what has
    /// gone, as for a skill. Otherwise somebody's when the block holds a
    /// section for it, which this workspace did not write (another workspace
    /// linking the project, or a hand that copied a marker), or a file is at
    /// its place in Claude's folder of Mindweaver's rules: written over by
    /// nothing, as a copy nothing records is not.
    fn standing(
        &self,
        workspace: &WeaverWorkspace,
        project: &MindProject,
        route: &str,
        offers: &[Offer],
    ) -> (Standing, Option<PathBuf>) {
        if let Some(record) = self.recorded.iter().find(|record| record.name == route) {
            let offered = super::offered_for(record, offers);
            let standing =
                standing::of_rule(workspace, project, record, self.file.as_ref(), &offered);
            return (standing, None);
        }
        match self.foreign(project.root(), route) {
            Some(path) => (Standing::Foreign, Some(path)),
            None => (Standing::Absent, None),
        }
    }

    /// Where somebody's rule at `route` is in the project at `root`, when
    /// the ledger records none there and something is: [`Held::standing`]'s
    /// foreign, without looking at what a recorded one holds.
    fn foreign(&self, root: &Path, route: &str) -> Option<PathBuf> {
        if self.recorded.iter().any(|record| record.name == route) {
            return None;
        }
        if self
            .file
            .as_ref()
            .is_some_and(|file| file.routes().contains(&route))
        {
            return Some(root.join(AGENTS_MD));
        }
        // At its place in either folder, a Mindflayer build's among them,
        // as a render tells somebody's file.
        Names::ALL
            .into_iter()
            .map(|names| root.join(claude_rules::file_under(names, route)))
            .find(|file| fs::symlink_metadata(file).is_ok())
    }
}

/// Where somebody's rule at `route` is in `project`, which its settings
/// mark, while the ledger records none there: in the block of its
/// `AGENTS.md`, or at its place in Claude's folder of Mindweaver's rules, as
/// [`uninstall`] tells it.
pub(crate) fn somebodys(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Result<Option<PathBuf>, InstallError> {
    Ok(Held::read(workspace, ledger, project)?.foreign(project.root(), route))
}

/// The routes the block in the `AGENTS.md` at `root` holds: none when it has
/// no block, or cannot be read as one, which a render then refuses, saying
/// why.
pub(crate) fn block_routes(root: &Path) -> Vec<String> {
    block_file(root)
        .map(|text| text.routes().into_iter().map(str::to_owned).collect())
        .unwrap_or_default()
}

/// The `AGENTS.md` at `root` as it reads now: none when it is not there, or
/// cannot be read as a file holding Mindweaver's block, which a render then
/// refuses, saying why.
pub(crate) fn block_file(root: &Path) -> Option<AgentsMd> {
    let bytes = fs::read(root.join(agents_md::FILE)).ok()?;
    AgentsMd::parse(&bytes).ok()
}

/// The route the ledger names the rule at `route`'s file for Claude by,
/// once a render makes it now: in Mindweaver's folder.
pub(crate) fn claude_file(route: &str) -> String {
    claude_file_under(Names::Mindweaver, route)
}

/// The route the ledger names the rule at `route`'s file for Claude by, in
/// the folder of `names`: `.claude/rules/mindflayer/` for one a Mindflayer
/// build recorded.
pub(crate) fn claude_file_under(names: Names, route: &str) -> String {
    let file = claude_rules::file_under(names, route);
    paths::to_config_string(&file).unwrap_or_else(|| file.display().to_string())
}

/// Whose folder `path`, a target a record names, is the rule at `route`'s
/// file for Claude in: `None` when it is no such file. Under either names,
/// so a record a Mindflayer build made is read where it says the file is.
pub(crate) fn claude_file_names(path: &str, route: &str) -> Option<Names> {
    Names::ALL
        .into_iter()
        .find(|names| path == claude_file_under(*names, route))
}

/// Whether `path`, a target a record names, is the rule at `route`'s file
/// for Claude, in either folder.
pub(crate) fn is_claude_file(path: &str, route: &str) -> bool {
    claude_file_names(path, route).is_some()
}

/// Install `candidate`, a rule, into `project`: rendered with every other
/// rule the project has into a project marked by its settings, copied into
/// one a `mind.toml` marks.
pub(crate) fn install(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    if project.settings().is_none() {
        return copy_in(workspace, ledger, project, candidate, homes, edits);
    }
    receives(project)?;
    let name = candidate.name().to_owned();
    // Looked at before the lock, so a refusal waits for nobody, and again
    // under it, as an install of a skill looks.
    if candidate.standing == Standing::Foreign {
        if let Some(path) = Held::read(workspace, ledger, project)?.foreign(project.root(), &name) {
            return Ok(Installed::Foreign { name, path }.into());
        }
    }
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    install_held(workspace, ledger, project, candidate, homes, edits)
}

/// [`install`], with the workspace's lock held already, as
/// [`super::install_held`] is for a skill.
pub(crate) fn install_held(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    if project.settings().is_none() {
        return copy_in_held(workspace, ledger, project, candidate, homes, edits);
    }
    receives(project)?;
    let root = project.root();
    let name = candidate.name().to_owned();
    if let Some(path) = Held::read(workspace, ledger, project)?.foreign(root, &name) {
        return Ok(Installed::Foreign { name, path }.into());
    }
    // A section or a file of it somebody changed since it was rendered is
    // theirs, as a skill's copy is, unless the install is forced.
    if edits == Edits::Kept {
        let paths = super::edited(workspace, ledger, project, Kind::Rule, &name)?;
        if !paths.is_empty() {
            return Ok(Installed::Edited { name, paths }.into());
        }
    }
    let entry = project_entry(workspace, project);
    let before = ledger.installation(&entry, Kind::Rule, &name)?;
    let rendered = render_held(
        workspace,
        ledger,
        project,
        homes,
        Change::Install(candidate),
        edits,
    )?;
    let after = ledger.installation(&entry, Kind::Rule, &name)?;
    let detail = format!("{name} into {}", project.name());
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;

    let targets = after.map(|record| record.targets).unwrap_or_default();
    let paths = targets
        .iter()
        .map(|target| root.join(&target.path))
        .collect();
    let installed = match before {
        None => Installed::Added {
            name,
            paths,
            unhashed: None,
        },
        // What each of its copies holds, by the hash recorded of each.
        Some(before) if before.targets == targets => Installed::Unchanged {
            name,
            paths,
            unhashed: None,
        },
        Some(_) => Installed::Updated {
            name,
            paths,
            unhashed: None,
        },
    };
    Ok(Installation {
        rendered: Some(rendered),
        ..Installation::from(installed)
    })
}

/// Take the rule `name` back out of `project`, which its settings mark:
/// every other rule it has rendered again without it, and its record
/// forgotten. One of them no longer offered where it was installed from is
/// kept as the project holds it, rather than refusing, so each such rule
/// can be taken out in turn.
///
/// Only one the ledger records: a section in the block nothing records, or
/// a file at its place in Claude's folder, is somebody's, and left. Refused
/// while its section is in a block no render may edit
/// ([`InstallError::Withheld`]), which would keep the section with no record
/// of it.
pub(crate) fn uninstall(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    name: &str,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Removed, InstallError> {
    let root = project.root();
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    let entry = project_entry(workspace, project);
    let Some(record) = ledger.installation(&entry, Kind::Rule, name)? else {
        return Ok(
            match Held::read(workspace, ledger, project)?.foreign(root, name) {
                Some(path) => Removed::Foreign {
                    name: ArtifactName::from(name),
                    path,
                },
                None => Removed::Absent {
                    name: ArtifactName::from(name),
                },
            },
        );
    };
    if edits == Edits::Kept {
        let paths = super::edited(workspace, ledger, project, Kind::Rule, name)?;
        if !paths.is_empty() {
            return Ok(Removed::Edited {
                name: ArtifactName::from(name),
                paths,
            });
        }
    }
    let rendered = render_held(
        workspace,
        ledger,
        project,
        homes,
        Change::Uninstall(name),
        edits,
    )?;
    let detail = format!("{name} from {}", project.name());
    ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;
    Ok(Removed::Removed {
        name: ArtifactName::from(name),
        paths: record
            .targets
            .iter()
            .map(|target| root.join(&target.path))
            .collect(),
        still_excluded: rendered.still_excluded.clone(),
        rendered: Some(Box::new(rendered)),
    })
}

/// Render `project`'s rules again, each from where it was installed from, as
/// it is there now, for a front end to bring the block, the import and
/// Claude's rule files in line with the project as it is: its agents, how
/// it shares, the `CLAUDE.md` files in and above it. `None` for a project a
/// `mind.toml` marks, whose rules are copies that nothing renders.
///
/// `homes` are the agents' own folders, which the front end decides, as for
/// [`super::install`]. Holds the workspace's lock throughout, as an install
/// does.
pub fn render(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    homes: &AgentHomes,
) -> Result<Option<Rendered>, InstallError> {
    if project.settings().is_none() {
        return Ok(None);
    }
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    render_held(
        workspace,
        ledger,
        project,
        homes,
        Change::Again,
        Edits::Kept,
    )
    .map(Some)
}

/// Render `project`'s rules again to bring each up to date with where it
/// was installed from, as an update does ([`crate::update`]), with the
/// workspace's lock held: as [`render`] renders them, but writing over an
/// edit of each rule `forced` names, section and file for Claude, as
/// `--force` asks, and keeping a rule no longer offered as the project
/// holds it rather than refusing the render, as an uninstall of another
/// rule keeps one.
///
/// Each rule `left` names, one somebody edited that the update leaves, is
/// kept whole as the project holds it, its record as it was: every copy of
/// it, not only the one edited. A render keeps an edited section or file
/// for Claude and writes the rule's other copy from the offer, which is
/// what an install or an uninstall of another rule wants; an update says it
/// leaves the rule alone, and writing its other copy would have the agents
/// reading the block and those reading Claude's folder read two versions of
/// one rule.
pub(crate) fn update_held(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    homes: &AgentHomes,
    forced: &[String],
    left: &[String],
) -> Result<Rendered, InstallError> {
    let edits = if forced.is_empty() {
        Edits::Kept
    } else {
        Edits::Forced
    };
    let rendered = render_held(
        workspace,
        ledger,
        project,
        homes,
        Change::Update { forced, left },
        edits,
    )?;
    let entry = project_entry(workspace, project);
    let detail = format!("the rules of {}, brought up to date", project.name());
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;
    Ok(rendered)
}

/// What a render is for.
#[derive(Clone, Copy)]
enum Change<'a> {
    /// Giving the project this rule too, or again.
    Install(&'a Candidate),
    /// Taking this one out.
    Uninstall(&'a str),
    /// Bringing every rule up to date, writing over the edits of those
    /// `forced` names and keeping those `left` names whole.
    Update {
        forced: &'a [String],
        left: &'a [String],
    },
    /// Neither: rendering what the project has, as it is now.
    Again,
}

impl Change<'_> {
    /// Whether the rule `name`, recorded before, is the one this changes,
    /// and so is not rendered from its record.
    fn changes(&self, name: &str) -> bool {
        match self {
            Change::Install(candidate) => candidate.name() == name,
            Change::Uninstall(going) => *going == name,
            Change::Update { .. } | Change::Again => false,
        }
    }

    /// Whether the rule `name` is kept whole as the project holds it, every
    /// copy and its record, rather than rendered from its offer: one an
    /// update leaves for an edit somebody made in any copy of it.
    fn leaves(&self, name: &str) -> bool {
        match self {
            Change::Update { left, .. } => left.iter().any(|route| route == name),
            Change::Install(_) | Change::Uninstall(_) | Change::Again => false,
        }
    }

    /// Whether an edit of the rule `name`, in its section or its file for
    /// Claude, is written over or taken away rather than kept, when the
    /// render is forced: the one rule an install or an uninstall changes,
    /// whose edit refused it unless forced, or each an update was forced
    /// to write over.
    fn forces(&self, name: &str) -> bool {
        match self {
            Change::Install(_) | Change::Uninstall(_) => self.changes(name),
            Change::Update { forced, .. } => forced.iter().any(|route| route == name),
            Change::Again => false,
        }
    }
}

/// One rule a render gives the project: what it is rendered from, and what
/// was recorded of it before.
struct Wanted {
    artifact: Artifact,
    /// The rule's file as written, which its file for Claude holds whole.
    bytes: Vec<u8>,
    origin: Origin,
    before: Option<InstallRecord>,
}

impl Wanted {
    /// The rule `offer` stands for, read where it is offered from.
    fn of(
        workspace: &WeaverWorkspace,
        offer: &Offer,
        before: Option<InstallRecord>,
    ) -> Result<Wanted, InstallError> {
        let path = offer.source(workspace);
        let within = within(workspace, offer, &path);
        inside(&path, &within)?;
        let artifact = Artifact::rule(&path, offer.name().to_string(), &within)?;
        let bytes = fs::read(artifact.file()).map_err(|source| {
            InstallError::Artifact(crate::artifact::ArtifactError::Read {
                path: artifact.file(),
                source,
            })
        })?;
        Ok(Wanted {
            artifact,
            bytes,
            origin: offer.origin(),
            before,
        })
    }

    fn name(&self) -> &str {
        self.artifact.name()
    }

    /// What is recorded of it before anything is written: what was recorded
    /// already, and a file for Claude the render may make, by the hash of
    /// what it would hold, while nothing is at its place.
    ///
    /// So a render killed after it made the file, and before it recorded
    /// what it made, leaves a file the next one knows for Mindweaver's,
    /// rather than one it takes for somebody's and leaves where Claude reads
    /// it. A record of a file that is not made after all says nothing wrong:
    /// the next render makes it, or forgets it, as it does any recorded file
    /// found gone.
    fn pending(&self, root: &Path, claude: bool) -> InstallRecord {
        let mut targets = self
            .before
            .as_ref()
            .map(|record| record.targets.clone())
            .unwrap_or_default();
        let file = claude_file(self.name());
        if claude
            && !targets
                .iter()
                .any(|target| is_claude_file(&target.path, self.name()))
            && fs::symlink_metadata(root.join(&file)).is_err()
        {
            targets.push(Target {
                agent: Some(Agent::Claude.name().to_owned()),
                path: file,
                content_hash: Some(hash::bytes(&self.bytes)),
            });
        }
        InstallRecord {
            kind: Kind::Rule,
            name: ArtifactName::from(self.name()),
            source_id: None,
            origin: Some(self.origin.clone()),
            content_hash: self
                .before
                .as_ref()
                .and_then(|record| record.content_hash.clone())
                .or_else(|| Some(hash::bytes(&self.bytes))),
            targets,
            entries: Vec::new(),
        }
    }
}

/// Where links in the rule `offer` offers, at `path`, may lead: the
/// workspace that keeps it, as for a skill's copy.
pub(super) fn within(workspace: &WeaverWorkspace, offer: &Offer, path: &Path) -> PathBuf {
    match offer {
        Offer::Loaded { entry, .. } => paths::normalize(&workspace.root().join(entry)),
        Offer::Workspace { .. } => workspace.root().to_path_buf(),
        Offer::Shelf(_) => path.to_path_buf(),
    }
}

/// Refuse the rule at `path` when it is a link, or reached through one, that
/// leads out of `within`, where it is offered from: what it leads to would
/// go into every agent's instructions in the project, and a credential is
/// how such a file ends up there, as a skill's copy refuses one.
pub(super) fn inside(path: &Path, within: &Path) -> Result<(), InstallError> {
    let (Ok(real), Ok(root)) = (crate::paths::canonical(path), crate::paths::canonical(within)) else {
        // Nothing to follow: reading it says what is wrong.
        return Ok(());
    };
    if real.starts_with(&root) {
        Ok(())
    } else {
        Err(InstallError::Outside {
            path: path.to_path_buf(),
            target: real,
            within: within.to_path_buf(),
        })
    }
}

/// Whether a local project may write a block into its `AGENTS.md`: always
/// in a shared one or with `edit_tracked`, and otherwise while git does not
/// track the file (D-02). `None` when it may, the reason when not.
fn withholding(project: &MindProject) -> Option<Withheld> {
    withheld_from(project, Path::new(AGENTS_MD))
}

/// [`withholding`] for the file at `route` from `project`'s root, a team's
/// file Mindweaver edits only as D-02 lets it: what a move of a block's
/// markers to Mindweaver's names asks of each file it would edit
/// (`migrate::names`), as the render of the block and of the import in a
/// `CLAUDE.md` ask it.
pub(crate) fn withheld_from(project: &MindProject, route: &Path) -> Option<Withheld> {
    let settings = project.settings()?;
    if settings.share == Share::Shared || settings.edit_tracked {
        return None;
    }
    match exclude::tracks_file(project.root(), route) {
        Ok(false) => None,
        Ok(true) => Some(Withheld::Tracked),
        Err(error) => Some(Withheld::Untold(error.to_string())),
    }
}

/// Render the rules of `project`, which its settings mark, as `change` asks,
/// with the workspace's lock held: the block, the import, Claude's rule
/// files, and the record of each.
///
/// Every rule the ledger records is rendered from the offer it was installed
/// from, as it is now; one no longer offered there refuses the render, since
/// leaving its section out would take it from every agent, as the block
/// refuses a rule it cannot read. An uninstall of another rule keeps it
/// instead, as the project holds it: its section as the block holds it, its
/// file for Claude where it is, its record as it was; and so does an
/// update. What can refuse is
/// asked before anything is written: a block that cannot be read or holds a
/// section nothing records, a rule taken out whose section is in a block
/// that cannot be edited, a file or a folder where no rule is written, a
/// `CLAUDE.md` that cannot be read. What is recorded before the first write
/// is what a render killed part way leaves for the next one
/// ([`Wanted::pending`]); a block that cannot be written puts that record
/// back as it was.
///
/// After the block is written, a failure of the import or of Claude's rule
/// files records what was written, and is then raised: the block is in
/// place, and what Claude reads is as it was until the next render.
///
/// A section of another rule somebody changed since it was rendered is kept
/// as the block holds it, with the hash recorded of what was rendered, so it
/// reads as edited still (D-04), and said ([`Warning::Kept`]); so is a file
/// for Claude, by [`claude_rules::render`]. The rule `change` installs or
/// takes out was refused before this if it was edited, unless `edits`
/// forces it, and then its file for Claude is written over or taken away as
/// its section is; so is each rule an update is forced to write over. Each
/// rule an update leaves for an edit is kept whole instead, the copy not
/// edited too, with its record as it was ([`update_held`]).
fn render_held(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    homes: &AgentHomes,
    change: Change<'_>,
    edits: Edits,
) -> Result<Rendered, InstallError> {
    let root = project.root();
    let agents = project
        .settings()
        .map(|settings| settings.agents.clone())
        .unwrap_or_default();
    let claude = agents.contains(&Agent::Claude);
    let entry = project_entry(workspace, project);
    let before = ledger.records(&entry, Kind::Rule)?;

    let offers = super::offers(workspace, ledger, Kind::Rule)?;
    let mut wanted = Vec::new();
    // The rules no longer offered that an uninstall of another keeps.
    let mut unoffered: Vec<&InstallRecord> = Vec::new();
    // The rules an update leaves for an edit, each kept whole.
    let mut left: Vec<&InstallRecord> = Vec::new();
    for record in &before {
        if change.changes(&record.name) {
            continue;
        }
        if change.leaves(&record.name) {
            left.push(record);
            continue;
        }
        let offer = record.origin.as_deref().and_then(|origin| {
            offers
                .iter()
                .find(|offer| offer.name() == record.name && offer.origin() == origin)
        });
        match (offer, change) {
            (Some(offer), _) => wanted.push(Wanted::of(workspace, offer, Some(record.clone()))?),
            // Taking one rule out is no reason to take another out with it,
            // and refusing would leave two rules no longer offered each
            // keeping the other from being taken out, with nothing else that
            // drops a rule's record: the one kept stays as the project holds
            // it, for its own uninstall to take out. Nor is bringing the
            // rest up to date, which an update does whatever is orphaned
            // beside them (D-04): it is said, not refused.
            (None, Change::Uninstall(_) | Change::Update { .. }) => unoffered.push(record),
            (None, _) => {
                return Err(InstallError::NoLongerOffered {
                    name: record.name.to_string(),
                    origin: record.origin.clone(),
                })
            }
        }
    }
    if let Change::Install(candidate) = change {
        let record = before
            .iter()
            .find(|record| record.name == candidate.name())
            .cloned();
        wanted.push(Wanted::of(workspace, &candidate.offer, record)?);
    }

    // What the block will hold, each section's hash by its route, worked out
    // as the block's render works it out, before anything is written.
    let mut sections: BTreeMap<String, ContentHash> = BTreeMap::new();
    for rule in &wanted {
        if let Some(section) = Section::of(&rule.artifact)? {
            sections.insert(section.route().to_owned(), section.hash());
        }
    }
    let file = root.join(AGENTS_MD);
    let withheld = withholding(project);
    // A section in a block no render can edit stays there whatever an
    // uninstall does, and forgetting its record would leave the next render
    // that can edit the file taking this workspace's own section for
    // somebody's ([`InstallError::NotRecorded`]).
    if let (Change::Uninstall(going), Some(why)) = (change, &withheld) {
        if block_routes(root).iter().any(|route| route == going) {
            return Err(InstallError::Withheld {
                file,
                route: going.to_owned(),
                why: why.clone(),
            });
        }
    }
    // Each kept rule's section, as the block holds it.
    let mut held = Vec::new();
    // The rules among them kept for an edit somebody made in their section.
    let mut edited: Vec<String> = Vec::new();
    if withheld.is_none() {
        agents_md::linked_inside(root, &file)?;
        if let Some(bytes) = read(&file)? {
            let text = AgentsMd::parse(&bytes).map_err(|problem| AgentsMdError::Malformed {
                file: file.clone(),
                problem,
            })?;
            // A section nothing here records is somebody's: another
            // workspace that links the project rendered it, or a hand put it
            // there. Rendering the block whole would take it out.
            if let Some(route) = text
                .routes()
                .into_iter()
                .find(|route| !before.iter().any(|record| record.name == *route))
            {
                return Err(InstallError::NotRecorded {
                    file,
                    route: route.to_owned(),
                });
            }
            for record in &unoffered {
                if !text.routes().contains(&record.name.as_str()) {
                    continue;
                }
                // One that cannot be read back would go with the render, so
                // it refuses the render, as a rule not kept does.
                let section =
                    text.section(&record.name)
                        .ok_or_else(|| InstallError::NoLongerOffered {
                            name: record.name.to_string(),
                            origin: record.origin.clone(),
                        })?;
                let recorded = record
                    .targets
                    .iter()
                    .find(|target| target.path == AGENTS_MD)
                    .and_then(|target| target.content_hash.clone());
                sections.insert(
                    record.name.to_string(),
                    recorded.unwrap_or_else(|| section.hash()),
                );
                held.push(section);
            }
            // A rule left for an edit keeps its section as the block holds
            // it, edited or not, so it is never rendered from an offer
            // while another copy of it holds somebody's edit.
            for record in &left {
                if !text.routes().contains(&record.name.as_str()) {
                    continue;
                }
                // One that cannot be read back would go with the render.
                let section = text
                    .section(&record.name)
                    .ok_or_else(|| InstallError::Unread {
                        file: file.clone(),
                        route: record.name.to_string(),
                    })?;
                let recorded = record
                    .targets
                    .iter()
                    .find(|target| target.path == AGENTS_MD)
                    .and_then(|target| target.content_hash.clone());
                sections.insert(
                    record.name.to_string(),
                    recorded.unwrap_or_else(|| section.hash()),
                );
                held.push(section);
            }
            // A rule rendered from its offer keeps its section as the block
            // holds it while somebody's edit is in it: by the hash recorded,
            // so one never recorded with one is rendered as it was before
            // hashes were kept. One gone is put back.
            for rule in &wanted {
                let name = rule.name();
                let recorded = rule.before.as_ref().and_then(|record| {
                    record
                        .targets
                        .iter()
                        .find(|target| target.path == AGENTS_MD)?
                        .content_hash
                        .clone()
                });
                let Some(recorded) = recorded else {
                    continue;
                };
                if change.forces(name) || !text.routes().contains(&name) {
                    continue;
                }
                // One that cannot be read back would go with the render.
                let section = text.section(name).ok_or_else(|| InstallError::Unread {
                    file: file.clone(),
                    route: name.to_owned(),
                })?;
                if section.hash() == recorded {
                    continue;
                }
                sections.insert(name.to_owned(), recorded);
                edited.push(name.to_owned());
                held.push(section);
            }
        }
    }
    let block = withheld.is_none() && !sections.is_empty();
    if block {
        if let Some(refused) = agents_md::misplaced(workspace, homes, root, &file) {
            return Err(AgentsMdError::from(refused).into());
        }
    }
    if claude && !wanted.is_empty() {
        let folder = root.join(claude_rules::folder());
        if let Some(refused) = agents_md::misplaced(workspace, homes, root, &folder) {
            return Err(claude_rules::ClaudeRulesError::from(refused).into());
        }
    }
    if claude {
        // Reads each `CLAUDE.md` the render would, and refuses one it
        // could not write the import into, before the block is written.
        claude_md::reach(project, homes, block)?;
    }

    // Copies from before the project had settings: a rule copied into the
    // folder its `mind.toml` named ([`copy_in`]), whose record a move to
    // settings carries as it was, for this render to take out
    // (`migrate::project`). Not a kept rule's, which stays as it is.
    let going = match change {
        Change::Uninstall(going) => before.iter().find(|record| record.name == going),
        _ => None,
    };
    let mut copied = Vec::new();
    for record in wanted
        .iter()
        .filter_map(|rule| rule.before.as_ref())
        .chain(going)
    {
        for target in &record.targets {
            if target.path == AGENTS_MD || is_claude_file(&target.path, &record.name) {
                continue;
            }
            copied.push(Copied {
                path: super::recorded_copy(project, Kind::Rule, &record.name, &target.path)?,
                name: record.name.to_string(),
                target: target.clone(),
            });
        }
    }

    let created = ledger.created(&entry, AGENTS_MD)?;
    let there = fs::symlink_metadata(&file).is_ok();
    let making = block && !there;
    let mut pending: Vec<InstallRecord> = wanted
        .iter()
        .map(|rule| rule.pending(root, claude))
        .collect();
    pending.extend(unoffered.iter().map(|&record| record.clone()));
    pending.extend(left.iter().map(|&record| record.clone()));
    if let Change::Uninstall(going) = change {
        // Kept until the render is done, so one killed part way leaves the
        // rule recorded, for the uninstall to be run again.
        pending.extend(before.iter().find(|record| record.name == going).cloned());
    }
    ledger.rendered(
        &entry,
        Kind::Rule,
        &pending,
        &[(AGENTS_MD, created || making)],
    )?;
    let undo = || ledger.rendered(&entry, Kind::Rule, &before, &[(AGENTS_MD, created)]);

    // An `AGENTS.md` Mindweaver makes is kept out of git before it is made,
    // as an install's copies are; one somebody had is theirs to commit.
    let key = local(project);
    let excluding = withheld.is_none() && (making || (created && there));
    let agents_entry = [exclude::Entry::file(AGENTS_MD)];
    let excluded = match key.filter(|_| excluding) {
        Some(key) => match exclude::add(root, key, &agents_entry) {
            Ok(excluded) => Some(excluded),
            Err(error) => {
                undo()?;
                return Err(error.into());
            }
        },
        None => None,
    };
    let written = if withheld.is_none() {
        let artifacts = wanted
            .iter()
            .filter(|rule| !edited.iter().any(|name| name == rule.name()))
            .map(|rule| &rule.artifact);
        match agents_md::render_keeping(workspace, project, artifacts, held, created, homes) {
            Ok(written) => Some(written),
            Err(error) => {
                if let Some(key) = key.filter(|_| making && excluding) {
                    let _ = exclude::remove(root, key, &agents_entry);
                }
                undo()?;
                return Err(error.into());
            }
        }
    } else {
        None
    };
    let rendered_routes: Vec<String> = written
        .as_ref()
        .map(|written| written.routes.clone())
        .unwrap_or_default();

    // The import, and then Claude's rule files, in the mode the import says:
    // every rule there while Claude reads no block, only those about some
    // files while it does. A project that does not name Claude has its
    // import and its files taken out.
    let import = claude_md::render(project, homes, claude && !rendered_routes.is_empty());
    let files = import.as_ref().ok().map(|import| {
        let mode = if claude {
            import.reach.mode()
        } else {
            Mode::Scoped
        };
        let asked: Vec<&Artifact> = if claude {
            wanted.iter().map(|rule| &rule.artifact).collect()
        } else {
            Vec::new()
        };
        // Not a kept rule's, nor a left one's, which no render here writes
        // or takes away. The rule a forced change is about counts as
        // holding what its file holds now, so an edit there is written over
        // or taken away as the file Mindweaver wrote would be; only a file,
        // never a link or a folder somebody put in its place.
        let recorded: Vec<Recorded> = pending
            .iter()
            .filter(|record| {
                !unoffered
                    .iter()
                    .chain(&left)
                    .any(|kept| kept.name == record.name)
            })
            .filter_map(|record| {
                let (target, names) = record.targets.iter().find_map(|target| {
                    claude_file_names(&target.path, &record.name).map(|names| (target, names))
                })?;
                let forced = (edits == Edits::Forced && change.forces(&record.name))
                    .then(|| held_file(&root.join(&target.path)))
                    .flatten();
                Some(Recorded {
                    route: record.name.to_string(),
                    hash: forced.or_else(|| target.content_hash.clone())?,
                    names,
                })
            })
            .collect();
        claude_rules::render(workspace, project, asked, mode, &recorded, homes)
    });

    // What is recorded now: each rule with the section the block holds and
    // the file Claude's folder keeps of it, each by the hash of what it
    // holds, and the rule taken out forgotten. While the import or the files
    // failed, each rule's file is recorded as it was.
    let kept: Option<BTreeMap<String, (ContentHash, Names)>> = match &files {
        Some(Ok(files)) => Some(
            files
                .recorded()
                .into_iter()
                .map(|kept| (kept.route, (kept.hash, kept.names)))
                .collect(),
        ),
        _ => None,
    };
    // The rules whose file for Claude was left as somebody edited it.
    let left_files: Vec<&str> = match &files {
        Some(Ok(files)) => files
            .files
            .iter()
            .filter(|file| file.written == claude_rules::Written::Edited)
            .map(|file| file.route.as_str())
            .collect(),
        _ => Vec::new(),
    };
    let mut records: Vec<InstallRecord> = wanted
        .iter()
        .zip(&pending)
        .map(|(rule, pending)| {
            let name = rule.name();
            let mut targets = Vec::new();
            if let (true, Some(hash)) = (
                rendered_routes.iter().any(|route| route == name),
                sections.get(name),
            ) {
                targets.push(Target {
                    agent: None,
                    path: AGENTS_MD.to_owned(),
                    content_hash: Some(hash.clone()),
                });
            }
            // Where the render left it, a Mindflayer build's folder among
            // them, or where it was recorded while the render did not run.
            let given = match &kept {
                Some(kept) => kept
                    .get(name)
                    .map(|(hash, names)| (hash.clone(), claude_file_under(*names, name))),
                None => pending
                    .targets
                    .iter()
                    .find(|target| is_claude_file(&target.path, name))
                    .and_then(|target| Some((target.content_hash.clone()?, target.path.clone()))),
            };
            if let Some((hash, path)) = given {
                targets.push(Target {
                    agent: Some(Agent::Claude.name().to_owned()),
                    path,
                    content_hash: Some(hash),
                });
            }
            // What the rule was rendered from, which an edit left in place
            // was not rendered from this time: that copy holds what an
            // earlier render wrote, and once the edit is undone it has to read
            // as behind the offer, not as current with it.
            let left = edited.iter().any(|route| route == name) || left_files.contains(&name);
            let content_hash = match (&rule.before, left) {
                (Some(before), true) => before.content_hash.clone(),
                _ => Some(hash::bytes(&rule.bytes)),
            };
            InstallRecord {
                kind: Kind::Rule,
                name: ArtifactName::from(name),
                source_id: None,
                origin: Some(rule.origin.clone()),
                content_hash,
                targets,
                entries: Vec::new(),
            }
        })
        .collect();
    // A rule being taken out keeps its file for Claude recorded while
    // Claude's files were not rendered, so the uninstall run again takes
    // the file away rather than finding it somebody's.
    if let (Change::Uninstall(going), None) = (change, &kept) {
        if let Some(record) = pending.iter().find(|record| record.name == going) {
            let targets: Vec<Target> = record
                .targets
                .iter()
                .filter(|target| is_claude_file(&target.path, going))
                .cloned()
                .collect();
            if !targets.is_empty() {
                records.push(InstallRecord {
                    targets,
                    ..record.clone()
                });
            }
        }
    }
    // A rule kept as the project holds it keeps its record as it was, but
    // for a section the block no longer holds.
    for record in &unoffered {
        let mut targets: Vec<Target> = record
            .targets
            .iter()
            .filter(|target| target.path != AGENTS_MD)
            .cloned()
            .collect();
        if let (true, Some(hash)) = (
            rendered_routes.iter().any(|route| *route == record.name),
            sections.get(record.name.as_str()),
        ) {
            targets.push(Target {
                agent: None,
                path: AGENTS_MD.to_owned(),
                content_hash: Some(hash.clone()),
            });
        }
        records.push(InstallRecord {
            targets,
            ..(*record).clone()
        });
    }
    // A rule left for an edit keeps its record whole, a copy gone from it
    // included: it still says what was installed, and where, for a forced
    // update to write over or the edit undone to read as it was.
    records.extend(left.iter().map(|&record| record.clone()));
    // Each copy from before the project had settings goes now that the rule
    // reaches the project's agents as any rule does, while it holds what was
    // copied: out of the folder the project authors in, as a change of agents
    // takes an untouched copy out of a folder no agent is sent copies to.
    // One changed since is left, its author's, and its record goes; one that
    // could not be taken out stays recorded, for the next render.
    let mut stayed = Vec::new();
    for copied in &copied {
        let error = match copied.take_out() {
            Ok(()) => continue,
            Err(Stays::Edited) => {
                stayed.push(Warning::Edited {
                    route: copied.name.clone(),
                    path: copied.path.clone(),
                });
                continue;
            }
            Err(Stays::Failed(error)) => error,
        };
        stayed.push(Warning::Unremoved {
            route: copied.name.clone(),
            path: copied.path.clone(),
            error: error.to_string(),
        });
        match records.iter_mut().find(|record| record.name == copied.name) {
            Some(record) => record.targets.push(copied.target.clone()),
            None => records.extend(before.iter().find(|record| record.name == copied.name).map(
                |record| InstallRecord {
                    targets: vec![copied.target.clone()],
                    ..record.clone()
                },
            )),
        }
    }
    let made = match &written {
        Some(written) => match written.written {
            BlockWritten::Created => true,
            BlockWritten::Removed => false,
            // A file left with no block holds only what somebody wrote in
            // it, and is theirs from then on.
            BlockWritten::Changed | BlockWritten::Unchanged => {
                created && !written.routes.is_empty()
            }
        },
        // Tracked by git: somebody committed it, so it is the team's.
        None => created && !matches!(withheld, Some(Withheld::Tracked)),
    };
    ledger.rendered(&entry, Kind::Rule, &records, &[(AGENTS_MD, made)])?;

    // Out of git last, what stopped being Mindweaver's: an `AGENTS.md` it
    // no longer counts as its own, and the file for Claude of a rule taken
    // out that was left where it is (changed since, or not to be removed),
    // which is its author's now, as a copy edited since is.
    let mut gone: Vec<exclude::Entry> = Vec::new();
    if !made && (created || excluding) {
        gone.extend(agents_entry.iter().cloned());
    }
    if let (Change::Uninstall(going), Some(kept)) = (change, &kept) {
        if let Some((_, names)) = kept.get(going) {
            gone.push(exclude::Entry::file(claude_rules::file_under(
                *names, going,
            )));
        }
    }
    let unexcluded = key
        .filter(|_| !gone.is_empty())
        .and_then(|key| exclude::remove(root, key, &gone).err())
        .map(|error| error.to_string());
    let still: Vec<String> = match &files {
        Some(Ok(files)) => files.still_excluded.clone(),
        _ => None,
    }
    .into_iter()
    .chain(unexcluded)
    .collect();
    let still_excluded = (!still.is_empty()).then(|| still.join("; "));

    let import = import?;
    let mut files = files.expect("rendered once the import was")?;
    // A file recorded only because the render might make it, and not made,
    // is nothing to report as gone: it was never there.
    files.files.retain(|file| {
        file.written != claude_rules::Written::Gone
            || before.iter().any(|record| {
                record.name == file.route
                    && record
                        .targets
                        .iter()
                        .any(|target| is_claude_file(&target.path, &record.name))
            })
    });

    let mut warnings = Vec::new();
    if let Some(why) = withheld {
        // A block written before the file could no longer be edited, a
        // project turned local after a team committed it say, is left with
        // the rules it held, as an import left in a `CLAUDE.md` is.
        if !block_routes(root).is_empty() {
            warnings.push(Warning::Left {
                file: file.clone(),
                why: why.clone(),
            });
        }
        let missed: Vec<Agent> = agents
            .iter()
            .copied()
            .filter(|agent| *agent != Agent::Claude)
            .collect();
        // Said only of a render that holds a rule the block would have held:
        // one with no rule left misses nothing, and one whose rules are all
        // about some files says of each that it reaches Claude alone. A rule
        // kept as the project holds it counts, since what it is about cannot
        // be read where it is offered no longer.
        let missing = !sections.is_empty() || !unoffered.is_empty() || !left.is_empty();
        if missing && !missed.is_empty() {
            warnings.push(Warning::Withheld {
                file: file.clone(),
                why,
                missed,
            });
        }
    }
    // A rule reaches an agent through the block, Claude among them while it
    // reads the block, or through its file for Claude.
    let reads_block = claude && import.reach.mode() == Mode::Scoped;
    for record in &records {
        let in_block = record.targets.iter().any(|target| target.path == AGENTS_MD);
        let in_file = record
            .targets
            .iter()
            .any(|target| is_claude_file(&target.path, &record.name));
        let reached = agents.iter().any(|agent| match agent {
            Agent::Claude => in_file || (in_block && reads_block),
            Agent::Codex | Agent::OpenCode => in_block,
        });
        if !reached {
            warnings.push(Warning::Unreached {
                route: record.name.to_string(),
            });
        }
    }
    warnings.extend(stayed);
    if written.is_some() {
        warnings.extend(edited.into_iter().map(|route| Warning::Kept {
            route,
            file: file.clone(),
        }));
    }

    Ok(Rendered {
        block: written,
        import,
        files,
        excluded,
        still_excluded,
        warnings,
    })
}

/// A rule's copy from before its project had settings, at `path`, recorded
/// as `target` of the installation of `name`.
struct Copied {
    name: String,
    target: Target,
    path: PathBuf,
}

/// Why a copy from before a project had settings stays where it is.
enum Stays {
    /// It holds something other than what was copied, or is not a file.
    Edited,
    /// It could not be looked at or taken out.
    Failed(io::Error),
}

impl Copied {
    /// Take the copy out while it holds what was copied, a file and not a
    /// link, by the hash recorded of it, and the folders its route was filed
    /// in that this leaves empty, as an uninstall of the copy does; one gone
    /// already is nothing to take out.
    fn take_out(&self) -> Result<(), Stays> {
        let held = match fs::symlink_metadata(&self.path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(Stays::Failed(error)),
            Ok(metadata) if !metadata.is_file() => return Err(Stays::Edited),
            Ok(_) => fs::read(&self.path).map_err(Stays::Failed)?,
        };
        if self.target.content_hash.as_ref() != Some(&hash::bytes(&held)) {
            return Err(Stays::Edited);
        }
        fs::remove_file(&self.path).map_err(Stays::Failed)?;
        if let Some(folder) = self.path.ancestors().nth(self.name.split('/').count()) {
            crate::lifecycle::prune(folder, self.path.parent());
        }
        Ok(())
    }
}

/// The hash of what the plain file at `path` holds, `None` for anything
/// else: nothing there, a link or a folder, or a file that cannot be read.
fn held_file(path: &Path) -> Option<ContentHash> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }
    fs::read(path).ok().map(|bytes| hash::bytes(&bytes))
}

/// The file's bytes, `None` when nothing is there to read.
fn read(file: &Path) -> Result<Option<Vec<u8>>, InstallError> {
    match fs::read(file) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(AgentsMdError::Read {
            path: file.to_path_buf(),
            source,
        }
        .into()),
    }
}

/// Copy `candidate`, a rule, into `project`, which a `mind.toml` marks: the
/// file as written, filed by its route in the folder the marker names, its
/// folders made on the way, and recorded as a skill's copy is.
///
/// Such a project names no agents and has no block: it was given its rules
/// as files before rules had one, and a copy is what it reads them as. An
/// unrecorded file at that place is somebody's, and refuses the install.
fn copy_in(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    let kind = Kind::Rule;
    let name = candidate.name().to_owned();
    let entry = project_entry(workspace, project);
    let targets = targets(project, kind)?;
    let copies: Vec<PathBuf> = targets
        .iter()
        .map(|target| copy_at(&target.folder, kind, &name))
        .collect();
    if let Some(error) = user_scope(homes, &copies) {
        return Err(error);
    }
    if candidate.standing == Standing::Foreign {
        let recorded = ledger.copies(&entry, kind)?;
        if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
            return Ok(Installed::Foreign { name, path }.into());
        }
    }
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    copy_in_held(workspace, ledger, project, candidate, homes, edits)
}

/// [`copy_in`], with the workspace's lock held already.
fn copy_in_held(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    let kind = Kind::Rule;
    let name = candidate.name().to_owned();
    let entry = project_entry(workspace, project);
    let targets = targets(project, kind)?;
    let copies: Vec<PathBuf> = targets
        .iter()
        .map(|target| copy_at(&target.folder, kind, &name))
        .collect();
    if let Some(error) = user_scope(homes, &copies) {
        return Err(error);
    }

    let from = candidate.source(workspace);
    if !from.is_file() {
        return Err(match &candidate.offer {
            Offer::Loaded { entry, .. } => InstallError::Gone {
                name: name.into_string(),
                path: from,
                workspace: paths::normalize(&workspace.root().join(entry)),
            },
            _ => InstallError::Missing {
                name: name.into_string(),
                path: from,
                workspace: workspace.root().to_path_buf(),
            },
        });
    }
    // A loaded workspace can be the project itself, keeping its own rules
    // where the copy would go: nothing is written, and nothing recorded,
    // which would let an uninstall take its own file away.
    if let Some(to) = copies.iter().find(|to| same_place(&from, to)) {
        return Ok(Installed::Unchanged {
            name,
            paths: vec![to.clone()],
            unhashed: None,
        }
        .into());
    }
    for to in &copies {
        if let Some(loaded) = kept_by_a_load(workspace, kind, to) {
            return Err(InstallError::KeptByALoad {
                path: to.clone(),
                loaded,
            });
        }
    }
    let recorded = ledger.copies(&entry, kind)?;
    if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
        return Ok(Installed::Foreign { name, path }.into());
    }
    if edits == Edits::Kept {
        let paths = super::edited(workspace, ledger, project, kind, &name)?;
        if !paths.is_empty() {
            return Ok(Installed::Edited { name, paths }.into());
        }
    }

    let rule = Wanted::of(workspace, &candidate.offer, None)?;
    let mut changes = Vec::with_capacity(copies.len());
    for to in &copies {
        changes.push(put(to, &rule.bytes)?);
    }
    let held = hash::bytes(&rule.bytes);
    ledger.installed(
        &entry,
        &InstallRecord {
            kind,
            name: name.clone(),
            source_id: None,
            origin: Some(rule.origin.clone()),
            content_hash: Some(held.clone()),
            targets: targets
                .iter()
                .zip(&copies)
                .map(|(target, to)| Target {
                    agent: target.agent(),
                    path: route(project.root(), to),
                    content_hash: Some(held.clone()),
                })
                .collect(),
            entries: Vec::new(),
        },
    )?;
    let detail = format!("{name} into {}", project.name());
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;

    let paths = copies;
    Ok(if changes
        .iter()
        .all(|change| *change == copy::Change::Unchanged)
    {
        Installed::Unchanged {
            name,
            paths,
            unhashed: None,
        }
    } else if changes.iter().all(|change| *change == copy::Change::Added) {
        Installed::Added {
            name,
            paths,
            unhashed: None,
        }
    } else {
        Installed::Updated {
            name,
            paths,
            unhashed: None,
        }
    }
    .into())
}

/// Put `bytes` at `to`, a rule's copy: made with its folders where nothing
/// is there, so a file somebody made meanwhile is never written over;
/// replaced through a temporary and a rename where a recorded copy is, never
/// through a link; left alone where it already holds them.
fn put(to: &Path, bytes: &[u8]) -> Result<copy::Change, InstallError> {
    let failed = |source| InstallError::Write {
        path: to.to_path_buf(),
        source,
        swept: Vec::new(),
    };
    match fs::symlink_metadata(to) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            claude_rules::create(to, bytes).map_err(failed)?;
            Ok(copy::Change::Added)
        }
        Err(error) => Err(failed(error)),
        Ok(metadata) if metadata.is_file() && fs::read(to).map_err(failed)? == bytes => {
            Ok(copy::Change::Unchanged)
        }
        Ok(_) => {
            replace_bytes_unresolved(to, bytes).map_err(|(_, source)| failed(source))?;
            Ok(copy::Change::Updated)
        }
    }
}

#[cfg(test)]
#[path = "rules_test.rs"]
mod tests;
