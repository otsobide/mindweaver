//! Putting an artifact from the workspace shelf into a mind project, and
//! taking it back out.
//!
//! Three places offer what can be installed: the workspace's own artifacts,
//! the own artifacts of every workspace it loads (read where they are), and
//! the shelf gathering fills. This is the other half. It writes into a mind
//! project, the only thing in Mindweaver that does, and the rule it works by
//! is that **it only manages what it installed**. An artifact somebody wrote by
//! hand is neither overwritten nor deleted, whatever a caller asks for, and it
//! is told so rather than being obeyed quietly. Which of the two a file is, is
//! what [`crate::ledger`] remembers.
//!
//! Where in the project is its agents' to say: a copy in each folder one of
//! them reads ([`targets`]), written together and recorded as one
//! installation, so that every agent the project names runs the same skill.
//! A rule is no folder: it reaches a project marked by its settings as a
//! section of the block in its `AGENTS.md`, rendered with every other rule
//! the project has, and one a `mind.toml` marks as a copy of its file
//! ([`rules`]). Nor is an MCP server, which reaches each agent as one entry
//! in its own config file, recorded by the fingerprint of what was written
//! there ([`mcp`]).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::{Serialize, Serializer};
use thiserror::Error;

use crate::agent::{Agent, AgentHomes};
use crate::agents_md::{AgentsMd, AgentsMdError};
use crate::artifact::{self, issue_list, name_issues, Artifact, ArtifactError, ValidationIssue};
use crate::catalog::{Catalog, DiscoveryFailure, Reference, QUALIFIER};
use crate::claude_md::ClaudeMdError;
use crate::claude_rules::ClaudeRulesError;
use crate::code::codes;
use crate::copy;
use crate::exclude::{self, ExcludeError, Excluded};
use crate::gather::Scrub;
use crate::hash;
use crate::kind::{Kind, Layout};
use crate::ledger::{
    Action, Carried, Forgotten, Gathered, InstallRecord, Ledger, LedgerError, Outcome, Target,
};
use crate::paths;
use crate::settings::Share;
use crate::url::redacted;
use crate::value::{ArtifactName, ContentHash, Origin, SourceName};
use crate::workspace::{same_directory, Loaded, MindProject, WeaverWorkspace, WorkspaceError};

pub(crate) mod entries;
pub mod mcp;
mod plan;
pub mod rules;
mod standing;

pub use entries::{entry_standings, ConfigUnread, ForAgent};
pub use plan::{holding, plan, Asked, Cell, Change, Clash, Conflict, Locked, Plan, PlanError};
pub use standing::Standing;
pub(crate) use standing::{Found, Surveyed};

/// Whether an install or an uninstall may write over, or take out, a copy
/// edited since it was installed (D-04): what `--force` says.
///
/// An edit is somebody's work, so by default it is refused as a copy
/// nothing records is: never written over or deleted, and the caller told
/// which copies, rather than obeyed quietly. Forcing reaches only a copy the
/// ledger records; somebody's that nothing records stays out of reach
/// whatever is asked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Edits {
    /// Left as they are, and the whole change refused.
    #[default]
    Kept,
    /// Written over or taken out as any copy Mindweaver installed.
    Forced,
}

/// Every copy of the installation of `name` into `project` edited since it
/// was installed, where the ledger has it: none when nothing is recorded.
///
/// A copy whose record has no hash is held to where it came from, so the
/// offers are looked for only then.
fn edited(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
) -> Result<Vec<PathBuf>, InstallError> {
    let entry = project_entry(workspace, project);
    let Some(record) = ledger.installation(&entry, kind, name)? else {
        return Ok(Vec::new());
    };
    let unhashed = record.content_hash.is_none()
        || record
            .targets
            .iter()
            .any(|target| kind == Kind::Rule && target.content_hash.is_none());
    let offers = if unhashed {
        offers(workspace, ledger, kind)?
    } else {
        Vec::new()
    };
    let offered = offered_for(&record, &offers);
    // Matched on the kind, so a third one has to say how its copies are told.
    let looked = match kind {
        Kind::Skill => standing::skill_copies(workspace, project, &record, &offered),
        Kind::Rule => standing::rule_copies(
            workspace,
            project,
            &record,
            rules::block_file(project.root()).as_ref(),
            &offered,
        ),
        Kind::Mcp => entries::told(workspace, project, &record, &offered).0,
    };
    Ok(looked
        .into_iter()
        .filter(|copy| copy.found == standing::Found::Edited)
        .map(|copy| copy.path)
        .collect())
}

/// What `origin` says for the workspace's own artifacts.
pub const WORKSPACE_ORIGIN: &str = "workspace";

/// Which of the three places offers something to install: what `gather
/// list --json` says of an offer as its `place`, and a snapshot of each it
/// lists ([`crate::view::Offered`]).
///
/// Declaration order is the order [`survey`] offers them in: what the
/// workspace keeps for its projects before what it found elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// One of the workspace's own.
    Workspace,
    /// One of the own of a workspace it loads.
    Loaded,
    /// Gathered onto its shelf.
    Shelf,
}

impl Place {
    /// The word it is written as.
    pub const fn slug(self) -> &'static str {
        match self {
            Place::Workspace => "workspace",
            Place::Loaded => "loaded",
            Place::Shelf => "shelf",
        }
    }
}

/// By its slug, for the reason a kind is: one spelling, which a renamed
/// variant cannot change.
impl Serialize for Place {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// Which of the three places offers something to install.
    Place by slug { Workspace, Loaded, Shelf }
}

/// Where something that can be installed would be copied from.
#[derive(Debug, Clone)]
pub enum Offer {
    /// Gathered from a source onto the shelf.
    Shelf(Gathered),
    /// One of the workspace's own artifacts, which it keeps to share with the
    /// projects it manages.
    Workspace {
        kind: Kind,
        name: ArtifactName,
        summary: Option<String>,
        /// The directory it is, wherever the workspace keeps it.
        path: PathBuf,
    },
    /// One of the own artifacts of a workspace this one loads, read where
    /// that workspace keeps it rather than from a copy.
    Loaded {
        kind: Kind,
        name: ArtifactName,
        summary: Option<String>,
        /// The directory it is, inside the loaded workspace.
        path: PathBuf,
        /// The `loaded` entry it came through, as the marker spells it.
        entry: String,
        /// The loaded workspace's declared name.
        workspace: String,
    },
}

/// As an object holding the place that offers it (`shelf`, `workspace` or
/// `loaded`), what that place holds of it, each field by the name its
/// variant gives it, and its origin as `--from` takes it.
///
/// By hand, for the place: a derive would tag each variant with its Rust
/// name, where these are words a front end compares and a rename must not
/// change. `path` is what the variant holds: the shelf's route from the
/// workspace, `/` separated, as the ledger keeps it, and the folder itself
/// for the other two. The origin goes out through [`redacted`], as every
/// URL does, so an offer built with a credential in its source prints none.
impl Serialize for Offer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Placed<T> {
            place: Place,
            #[serde(flatten)]
            held: T,
            origin: Origin,
        }
        #[derive(Serialize)]
        struct Own<'a> {
            kind: Kind,
            name: &'a str,
            summary: &'a Option<String>,
            #[serde(serialize_with = "paths::lossy")]
            path: &'a Path,
        }
        #[derive(Serialize)]
        struct Load<'a> {
            kind: Kind,
            name: &'a str,
            summary: &'a Option<String>,
            #[serde(serialize_with = "paths::lossy")]
            path: &'a Path,
            entry: &'a str,
            workspace: &'a str,
        }

        let origin = Origin::from(redacted(&self.origin()));
        match self {
            Offer::Shelf(gathered) => Placed {
                place: Place::Shelf,
                held: gathered,
                origin,
            }
            .serialize(serializer),
            Offer::Workspace {
                kind,
                name,
                summary,
                path,
            } => Placed {
                place: Place::Workspace,
                held: Own {
                    kind: *kind,
                    name,
                    summary,
                    path,
                },
                origin,
            }
            .serialize(serializer),
            Offer::Loaded {
                kind,
                name,
                summary,
                path,
                entry,
                workspace,
            } => Placed {
                place: Place::Loaded,
                held: Load {
                    kind: *kind,
                    name,
                    summary,
                    path,
                    entry,
                    workspace,
                },
                origin,
            }
            .serialize(serializer),
        }
    }
}

impl Offer {
    /// Which place offers it.
    pub fn place(&self) -> Place {
        match self {
            Offer::Workspace { .. } => Place::Workspace,
            Offer::Loaded { .. } => Place::Loaded,
            Offer::Shelf(_) => Place::Shelf,
        }
    }

    pub fn name(&self) -> &ArtifactName {
        match self {
            Offer::Shelf(gathered) => &gathered.name,
            Offer::Workspace { name, .. } | Offer::Loaded { name, .. } => name,
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Offer::Shelf(gathered) => gathered.kind,
            Offer::Workspace { kind, .. } | Offer::Loaded { kind, .. } => *kind,
        }
    }

    /// The one line a listing shows.
    pub fn summary(&self) -> Option<&str> {
        match self {
            Offer::Shelf(gathered) => gathered.summary.as_deref(),
            Offer::Workspace { summary, .. } | Offer::Loaded { summary, .. } => summary.as_deref(),
        }
    }

    /// Where it came from, as one string: [`WORKSPACE_ORIGIN`] for the
    /// workspace's own; for a gathered one its source's address, with the
    /// branch when one was asked for; for a loaded one its entry after
    /// [`LOADED_PREFIX`] (`load:../team`), so it can be neither the word the
    /// workspace's own go by nor the address of something gathered, even from
    /// that same directory.
    ///
    /// One string for every front end: what a listing prints is what
    /// `--from` accepts. A source's address carries no credential, because
    /// the ledger hands none back.
    pub fn origin(&self) -> Origin {
        match self {
            Offer::Shelf(gathered) => gathered.source.origin(),
            Offer::Workspace { .. } => Origin::from(WORKSPACE_ORIGIN),
            Offer::Loaded { entry, .. } => loaded_origin(entry),
        }
    }

    /// The word a listing names where it came from by, which `--from`
    /// takes: a shelf source's name (D-13), and the origin of anything else,
    /// which has no name of its own.
    ///
    /// Beside [`origin`](Self::origin) rather than in its place: the origin
    /// is what an install records and an update finds its offer by, and the
    /// envelope's and the snapshot's `origin` hold it as they always did.
    pub fn listed_from(&self) -> String {
        match self {
            Offer::Shelf(gathered) => gathered.source.name.to_string(),
            Offer::Workspace { .. } | Offer::Loaded { .. } => self.origin().into_string(),
        }
    }

    /// Whether `from`, as somebody typed it, names where this came from
    /// ([`named_by`](Self::named_by)).
    pub fn comes_from(&self, from: &str) -> bool {
        self.named_by(from).is_some()
    }

    /// How `from`, as somebody typed it, names where this came from, if it
    /// does: as printed, the origin; by a name, a shelf source's (D-13) or a
    /// loaded workspace's, unless that is the word the workspace's own go
    /// by; loosely, a source's address without its branch; or by another
    /// spelling of a source's URL, the same repository's
    /// ([`crate::url::canonical`]) at the same branch, which `--from` still
    /// takes and says ([`Respelled`]). The closest a `from` names any offer
    /// by wins over the rest ([`offered`]), so an origin names its one offer
    /// wherever it is printed, the install screen's lines included.
    /// A path to a loaded workspace is [`resolve_from`]'s to turn into its
    /// origin, and only when nothing matches it as typed, so a path never takes
    /// a shelf source's address or the workspace's own away. An address typed
    /// with credentials names the source they were taken out of.
    pub(crate) fn named_by(&self, from: &str) -> Option<Naming> {
        let from = redacted(from);
        let from = from.as_ref();
        if self.origin() == from {
            return Some(Naming::Printed);
        }
        match self {
            Offer::Shelf(gathered) if gathered.source.name == from => Some(Naming::Named),
            Offer::Shelf(gathered) if gathered.source.url == from => Some(Naming::Loose),
            Offer::Shelf(gathered) => {
                let (url, reference) = match from.rsplit_once('#') {
                    Some((url, reference)) => (url, Some(reference)),
                    None => (from, None),
                };
                (reference == gathered.source.reference.as_deref()
                    && crate::url::canonical(url) == crate::url::canonical(&gathered.source.url))
                .then_some(Naming::Respelled)
            }
            Offer::Loaded { workspace, .. } if from != WORKSPACE_ORIGIN && workspace == from => {
                Some(Naming::Named)
            }
            Offer::Loaded { .. } | Offer::Workspace { .. } => None,
        }
    }

    /// The directory it is copied from.
    pub(crate) fn source(&self, workspace: &WeaverWorkspace) -> PathBuf {
        match self {
            Offer::Shelf(gathered) => workspace.root().join(&gathered.path),
            Offer::Workspace { path, .. } | Offer::Loaded { path, .. } => path.clone(),
        }
    }
}

/// How a `--from` names an offer ([`Offer::named_by`]), the closest last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Naming {
    /// By another spelling of its source's URL.
    Respelled,
    /// By its source's address without the branch its origin has.
    Loose,
    /// By a name: a shelf source's, or a loaded workspace's. The two are as
    /// close, so one word that is both names two places, refused as offered
    /// twice, rather than taken for the source's: `--from team` named a
    /// loaded workspace called `team` before sources had names, and a
    /// script saying it would otherwise be given a source's skill unsaid.
    Named,
    /// By its origin, as every listing's JSON and every record holds it.
    Printed,
}

/// A `--from` that named a shelf source by another spelling of its URL than
/// the one it is installed from: still taken, and said, since what a listing
/// shows and `--from` takes is a source's name (D-13).
///
/// What a script written before the step to schema 7 made two spellings one
/// source says, `https://github.com/acme/skills` for the source kept as
/// `git@github.com:acme/skills.git`, or what somebody types for a
/// repository from memory. Refusing it as not offered would break the
/// first for a change it did not ask for; taking it unsaid would leave
/// nothing pointing at the name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "--from {from} names the source `{name}` by another spelling of its URL, which --from takes \
     for now: name it `{name}`"
)]
pub struct Respelled {
    /// What was typed, without the credentials [`redacted`] takes out.
    pub from: String,
    /// What the source is called, which `--from` takes.
    pub name: SourceName,
}

codes!(Respelled => "from_spelling_deprecated");

/// Whether `from` named `candidate` by another spelling of its source's URL
/// ([`Respelled`]): what a front end says beside the install, which still
/// goes ahead.
pub fn respelled(candidate: &Candidate, from: &str) -> Option<Respelled> {
    match &candidate.offer {
        Offer::Shelf(gathered) if candidate.offer.named_by(from) == Some(Naming::Respelled) => {
            Some(Respelled {
                from: redacted(from).into_owned(),
                name: gathered.source.name.clone(),
            })
        }
        _ => None,
    }
}

/// What a loaded workspace's origin starts with.
pub const LOADED_PREFIX: &str = "load:";

/// The origin of what the workspace loaded as `entry` offers, the entry as
/// its marker spells it: written once, for an offer and for a listing of
/// the loads alike ([`crate::view::LoadedWorkspace`]), so what a listing
/// says `--from` takes is what an install from that load records.
pub fn loaded_origin(entry: &str) -> Origin {
    Origin::from(format!("{LOADED_PREFIX}{entry}"))
}

/// `from` as `--from` should match it: when it is a path, typed relative to
/// `base` (bare or after `load:`) to a workspace `workspace` loads, that
/// workspace's origin; otherwise `from` as it is, and always for the word
/// the workspace's own go by. So the path given to `mindweaver load` works as
/// `--from` from wherever it was typed, not only spelled as the marker
/// stores it.
pub fn resolve_from(workspace: &WeaverWorkspace, base: &Path, from: &str) -> String {
    if from == WORKSPACE_ORIGIN {
        return from.to_owned();
    }
    // `load:` with any spelling of the path after it means the same load.
    let typed = from.strip_prefix(LOADED_PREFIX).unwrap_or(from);
    let path = paths::normalize(&base.join(typed));
    workspace
        .loads()
        .into_iter()
        .find(|loaded| paths::normalize(&loaded.root) == path || same_place(&loaded.root, &path))
        .and_then(|loaded| paths::to_config_string(&loaded.entry))
        .map_or_else(
            || from.to_owned(),
            |entry| loaded_origin(&entry).into_string(),
        )
}

/// Whether a declared name is one plain folder name, and so safe to join onto
/// a project's folder. A name is read from somebody's `SKILL.md`; `..`, an
/// absolute path or `a/b` would put the copy (and the deletion that
/// replacing it starts with) somewhere else entirely.
pub fn is_folder_name(name: &str) -> bool {
    let mut components = Path::new(name).components();
    matches!(
        (components.next(), components.next()),
        (Some(Component::Normal(_)), None)
    ) && !name.contains(['/', '\\'])
}

/// Whether `name` is one an install of `kind` can file a copy under, and so
/// one that is offered, installed and removed: one folder name for a skill
/// ([`is_folder_name`]); for a rule, a route held to the rules every name is
/// ([`name_issues`]), lowercase words joined by single hyphens and slashes,
/// which becomes folders and a file, and the marker of a section in
/// `AGENTS.md`, and never climbs out of the folder it is filed in.
///
/// Matched on the kind, so a third one has to say what it is filed under.
pub fn is_installable_name(kind: Kind, name: &str) -> bool {
    match kind {
        Kind::Skill => is_folder_name(name),
        Kind::Rule => name_issues(name).is_empty(),
        // A definition's file is named by it, as a skill's folder is, and an
        // entry will be keyed by it: the keys every agent takes, each one
        // file's name too.
        Kind::Mcp => crate::mcp::name::is_key(name),
    }
}

/// Refuse `name` when an install of `kind` cannot file a copy under it.
fn installable_name(kind: Kind, name: &str) -> Result<(), InstallError> {
    if is_installable_name(kind, name) {
        return Ok(());
    }
    Err(match kind {
        Kind::Skill | Kind::Mcp => InstallError::UnsafeName {
            name: name.to_owned(),
        },
        Kind::Rule => InstallError::UnsafeRoute {
            problem: issue_list(&name_issues(name)),
            route: name.to_owned(),
        },
    })
}

/// What an install leaves out of every copy it writes, from the top of the
/// artifact, and why: a `.claude-plugin` folder, by which Claude would load
/// the skill as a plugin, with whatever hooks and MCP servers it declares
/// (docs-only, in `docs/agents.md`, so it is left out whatever Claude does
/// with it).
///
/// A skill is instructions an agent reads when it chooses to; a plugin's
/// hooks run commands on their own and its servers are started for it, which
/// installing a skill must not turn on behind anybody's back. Left out of
/// every install, whichever agents the project names: a folder a project's
/// `targets` name may be Claude's too. Only at the top, where Claude
/// looks for one: a skill that teaches plugin writing can ship one further
/// down as an example, and it is the skill's like any other file.
const LEAVE_OUT: [(&str, &str); 1] = [(
    ".claude-plugin",
    "Claude would load the skill as a plugin, with the hooks and MCP servers it declares",
)];

/// The names [`LEAVE_OUT`] leaves out, as the copy takes them.
pub(crate) fn left_out_names() -> [&'static str; LEAVE_OUT.len()] {
    LEAVE_OUT.map(|(name, _)| name)
}

/// Something an offer holds that an install left out of every copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    /// Where it is in the offer.
    pub path: PathBuf,
    /// Why no copy holds it, as a clause a front end can say after its name.
    pub reason: &'static str,
}

/// What the artifact at `from` holds that an install leaves out of every
/// copy ([`LEAVE_OUT`]): what an install says it did not copy, and what
/// adopting a copy refuses, since no install could have written it
/// ([`crate::adopt`]).
pub(crate) fn left_out(from: &Path) -> Vec<LeftOut> {
    LEAVE_OUT
        .iter()
        .map(|(name, reason)| LeftOut {
            path: from.join(name),
            reason,
        })
        .filter(|left| fs::symlink_metadata(&left.path).is_ok())
        .collect()
}

/// One folder of a project's that an install of a kind puts a copy in, under
/// the artifact's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallTarget {
    /// The folder, under the project's root, whether or not it is there yet.
    pub folder: PathBuf,
    /// The agents the copy is for, in report order: none for the folder a
    /// `mind.toml` names, which is no one agent's.
    pub agents: Vec<Agent>,
}

impl InstallTarget {
    /// The agent the ledger says a copy here is for ([`agent_of`]).
    fn agent(&self) -> Option<String> {
        agent_of(&self.agents)
    }
}

/// The agent the ledger says a copy for `agents` is for: the one agent it is
/// for, or none for a copy two agents share, or no agent owns.
pub(crate) fn agent_of(agents: &[Agent]) -> Option<String> {
    match agents {
        [agent] => Some(agent.name().to_owned()),
        _ => None,
    }
}

/// Where an install of `kind` into `project` puts a copy: each folder the
/// target table sends one of the agents its settings name to
/// ([`MindProject::targets`], D-07), or, for a project a `mind.toml` marks,
/// which names no agents, the one folder that marker names for it.
///
/// Not a project's own folder from its settings' `directories`, which is
/// what it authors in (D-06): what the workspace offers goes where the agents
/// read, and what the project writes itself stays where it writes it.
///
/// Two targets that are one folder on disk, `.claude/skills` linked to
/// `.agents/skills` so that Claude and Codex share it, are one target, under
/// the first spelling and for the agents of both, as discovery reads them
/// once: two copies put in place in one folder would find each other there.
///
/// Settings that name `targets` for the kind put a copy in each folder they
/// name an install may use, for every agent they name, and nowhere else: a
/// folder they name that leads out of the project, or into `.mind`, is
/// passed over ([`MindProject::misplaced_targets`] says which).
///
/// Refused when there is nowhere to put one: a settings file naming no agent
/// that is sent `kind` (`NoTarget`), or naming only folders for it that no
/// install may use (`NoUsableTarget`), or a `mind.toml` naming no folder for
/// it (`NoDirectory`).
pub fn targets(project: &MindProject, kind: Kind) -> Result<Vec<InstallTarget>, InstallError> {
    // Refused here, where every install, adoption and survey of a project
    // asks where a kind goes, so none of them reads a folder for one no
    // install writes, or says a project has none.
    if !installs(kind) {
        return Err(InstallError::NotInstallable { kind });
    }
    // A server is an entry in each agent's own config file, written by its
    // renderer ([`mcp`]), never a copy in a folder: matched on the kind, so
    // a kind added says which it is before it compiles.
    match kind {
        Kind::Skill | Kind::Rule => {}
        Kind::Mcp => return Err(InstallError::NotCopied { kind }),
    }
    if project.settings().is_none() {
        return Ok(vec![InstallTarget {
            folder: project.required_directory(kind)?,
            agents: Vec::new(),
        }]);
    }
    let mut targets: Vec<InstallTarget> = Vec::new();
    for target in project.targets(kind) {
        let folder = project.root().join(&target.path);
        match targets
            .iter_mut()
            .find(|seen| same_directory(&seen.folder, &folder))
        {
            Some(seen) => {
                seen.agents.extend(target.agents);
                seen.agents.sort();
                seen.agents.dedup();
            }
            None => targets.push(InstallTarget {
                folder,
                agents: target.agents,
            }),
        }
    }
    if targets.is_empty() {
        let overridden = project.settings().is_some_and(|settings| {
            !settings.agents.is_empty() && settings.targets.get(kind).is_some()
        });
        let project = project.name().to_owned();
        return Err(if overridden {
            InstallError::NoUsableTarget { project, kind }
        } else {
            InstallError::NoTarget { project, kind }
        });
    }
    Ok(targets)
}

/// One thing that can be installed, seen from one project.
///
/// What it serializes to is a contract, as a status report's is: each field
/// by its name.
#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub offer: Offer,
    pub standing: Standing,
}

impl Candidate {
    pub fn name(&self) -> &ArtifactName {
        self.offer.name()
    }

    pub fn kind(&self) -> Kind {
        self.offer.kind()
    }

    /// The one line a listing shows.
    pub fn summary(&self) -> Option<&str> {
        self.offer.summary()
    }

    /// See [`Offer::origin`].
    pub fn origin(&self) -> Origin {
        self.offer.origin()
    }

    /// See [`Offer::comes_from`].
    pub fn comes_from(&self, from: &str) -> bool {
        self.offer.comes_from(from)
    }

    /// Whether installing it waits for a yes ([`asks_first`]).
    pub fn asks_first(&self) -> bool {
        asks_first(self.kind(), &self.origin())
    }

    /// The directory it is copied from.
    fn source(&self, workspace: &WeaverWorkspace) -> PathBuf {
        self.offer.source(workspace)
    }
}

/// What the workspaces a workspace loads offer, and what stops them offering
/// more: [`offered_by_loads`].
#[derive(Debug, Default)]
pub struct Loads {
    pub offers: Vec<Offer>,
    pub failures: Vec<LoadFailure>,
}

/// The own artifacts of `kind` of every workspace `workspace` loads, in the
/// order its marker lists them. What could not be offered is dropped here;
/// [`offered_by_loads`] has it, for a front end to warn with.
pub fn loaded_offers(workspace: &WeaverWorkspace, kind: Kind) -> Vec<Offer> {
    from_loads(workspace, &[kind]).offers
}

/// Everything the workspaces `workspace` loads offer, of every kind, and
/// every reason something was not offered: a loaded workspace that cannot
/// be opened, an artifact in one that cannot be read, a name no copy can be
/// filed under. Failures collected, not raised, like every discovery.
///
/// Every kind, as [`survey`] offers every kind to install, so what a load
/// is said to offer, in `mindweaver load`, `mindweaver gather list` and the TUI, is
/// what the command line and the install screen let somebody install.
pub fn offered_by_loads(workspace: &WeaverWorkspace) -> Loads {
    from_loads(workspace, &Kind::ALL)
}

/// The two above. No ledger: listing what a load offers is reading, and must
/// not create the database a workspace that never gathered does not have.
/// Only a loaded workspace's own, never what it loads in turn; all of its own,
/// even kinds it keeps in one folder with a project of its, since that project
/// is not what is being offered; and each loaded directory once, however many
/// spellings of it the marker has.
fn from_loads(workspace: &WeaverWorkspace, kinds: &[Kind]) -> Loads {
    let mut loads = Loads::default();
    let mut seen: Vec<PathBuf> = Vec::new();
    for loaded in workspace.loads() {
        if seen
            .iter()
            .any(|root| *root == loaded.root || same_place(root, &loaded.root))
        {
            continue;
        }
        seen.push(loaded.root.clone());
        let source = match loaded.workspace {
            Ok(source) => source,
            Err(error) => {
                loads.failures.push(LoadFailure::Workspace(error));
                continue;
            }
        };
        if kinds.is_empty() {
            continue;
        }
        let entry = paths::to_config_string(&loaded.entry)
            .unwrap_or_else(|| loaded.entry.display().to_string());
        let own = source.own();
        let (artifacts, failures) =
            Catalog::discover_kinds(std::slice::from_ref(&own), kinds).into_parts();
        loads
            .failures
            .extend(failures.into_iter().map(LoadFailure::Artifact));
        for artifact in artifacts {
            if let Err(refused) = installable_name(artifact.kind(), artifact.name()) {
                let path = artifact.path().to_path_buf();
                loads.failures.push(match refused {
                    InstallError::UnsafeRoute { route, problem } => LoadFailure::UnsafeRoute {
                        path,
                        route,
                        problem,
                    },
                    _ => LoadFailure::UnsafeName {
                        path,
                        name: artifact.name().to_string(),
                    },
                });
                continue;
            }
            loads.offers.push(Offer::Loaded {
                kind: artifact.kind(),
                name: artifact.name().to_owned(),
                summary: artifact.summary().map(str::to_owned),
                path: artifact.path().to_path_buf(),
                entry: entry.clone(),
                workspace: source.name().to_owned(),
            });
        }
    }
    loads
}

/// Why something a loaded workspace has is not offered.
#[derive(Debug, Error)]
pub enum LoadFailure {
    #[error(transparent)]
    Workspace(WorkspaceError),
    #[error(transparent)]
    Artifact(DiscoveryFailure),
    #[error(
        "{path}: declares the name `{name}`, which is not one folder name, so it is not offered"
    )]
    UnsafeName { path: PathBuf, name: String },
    #[error("{path}: the rule `{route}` cannot be installed under that name ({problem}), so it is not offered")]
    UnsafeRoute {
        path: PathBuf,
        route: String,
        problem: String,
    },
}

codes!(LoadFailure {
    Self::UnsafeName { .. } => "load_name_unsafe",
    Self::UnsafeRoute { .. } => "load_route_unsafe",
} carried {
    Self::Workspace(error) => error,
    Self::Artifact(error) => error,
});

/// Everything of `kind` that can be installed, and how each stands with
/// `project`: the workspace's own first, then what the workspaces it loads
/// offer, then the shelf. A loaded workspace that cannot be opened offers
/// nothing here; [`offered_by_loads`] says why, for a front end to warn with.
/// Nothing whose name is not one folder name is offered, from anywhere.
///
/// That is the list of what can be installed, so an artifact a project holds
/// that is in neither does not appear: it is not a thing this command can
/// offer to do anything about.
///
/// A project with nowhere to put `kind` ([`targets`]) is offered nothing,
/// rather than refused: nothing can be put there, and the screen surveys
/// every project a workspace links, where a refusal for one would hide the
/// rest. Asking for one by name ([`offered`]) is refused.
pub fn survey(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
) -> Result<Vec<Candidate>, InstallError> {
    // Nothing is read of what is offered for a project that can take none
    // of it.
    if receives(project, kind).is_err() {
        return Ok(Vec::new());
    }
    survey_among(
        workspace,
        ledger,
        project,
        kind,
        &offers(workspace, ledger, kind)?,
    )
}

/// [`survey`], against `offers`, every offer of `kind` as [`offers`] reads
/// them: for a reader surveying every project a workspace links, which reads
/// what is offered once rather than once a project, as a snapshot does.
pub(crate) fn survey_among(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    offers: &[Offer],
) -> Result<Vec<Candidate>, InstallError> {
    // A server reaches each agent as an entry in its config file, so how
    // one stands is told by the ledger and those files ([`mcp::survey`]).
    if kind == Kind::Mcp {
        return mcp::survey(workspace, ledger, project, offers);
    }
    // A rule reaches a project marked by its settings through the block of
    // its `AGENTS.md` rather than as a copy in a folder (D-03), so how one
    // stands there is told by the ledger and that block.
    if kind == Kind::Rule && project.settings().is_some() {
        if rules::receives(project).is_err() {
            return Ok(Vec::new());
        }
        return rules::survey(workspace, ledger, project, offers);
    }
    let Ok(targets) = targets(project, kind) else {
        return Ok(Vec::new());
    };
    let entry = project_entry(workspace, project);
    let recorded = ledger.copies(&entry, kind)?;
    let records = ledger.records(&entry, kind)?;
    // Once a name: every offer of one name stands as the one installation
    // the project has of it, whichever of them it came from.
    let mut said: Vec<(String, Standing)> = Vec::new();
    let mut candidates = Vec::with_capacity(offers.len());
    for offer in offers {
        let standing = match said.iter().find(|(name, _)| name == offer.name()) {
            Some((_, standing)) => *standing,
            None => {
                let record = records.iter().find(|record| record.name == offer.name());
                let standing = standing(
                    workspace,
                    project,
                    &targets,
                    kind,
                    offer.name(),
                    &recorded,
                    record.map(|record| (record, offers)),
                );
                said.push((offer.name().to_string(), standing));
                standing
            }
        };
        candidates.push(Candidate {
            offer: offer.clone(),
            standing,
        });
    }
    Ok(candidates)
}

/// Every installation of `kind` the ledger records in `project`, by name in
/// name order, and how each stands ([`Standing`]): as [`survey`] says of
/// each offer of its name, and said here too of one nothing offers any more,
/// which no offer carries.
///
/// A rule in a project marked by its settings is told by its section in the
/// block of `AGENTS.md` and its file for Claude, as for a survey.
///
/// Only the ledger can fail it, read for the records and for what the
/// shelf offers, so that is what it says: a report reads that ledger too,
/// and says a failure of it once, as discovery's.
pub fn standings(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
) -> Result<Vec<(String, Standing)>, LedgerError> {
    Ok(surveyed(workspace, ledger, project, kind)?
        .into_iter()
        .map(|surveyed| (surveyed.record.name.into_string(), surveyed.standing))
        .collect())
}

/// Every installation of `kind` the ledger records in `project`, by name in
/// name order, as [`standings`] tells them, with what told each: where it
/// is offered now and how each of its copies is found.
pub(crate) fn surveyed(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
) -> Result<Vec<Surveyed>, LedgerError> {
    let records = ledger.records(&project_entry(workspace, project), kind)?;
    if records.is_empty() {
        return Ok(Vec::new());
    }
    let offers = offers(workspace, ledger, kind)?;
    let block = block_of(project, kind);
    Ok(records
        .into_iter()
        .map(|record| told(workspace, project, record, &offers, block.as_ref()))
        .collect())
}

/// The installation of `name`, of `kind`, the ledger records in `project`,
/// told as [`surveyed`] tells each, against `offers`, every offer of its
/// kind ([`offers`]): none when the ledger records none.
///
/// One name's, for a caller asking after one artifact in every project, as
/// removing one everywhere does ([`crate::everywhere`]): surveying each
/// project whole would hash the copies of everything else installed there.
pub(crate) fn surveyed_one(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
    offers: &[Offer],
) -> Result<Option<Surveyed>, LedgerError> {
    let entry = project_entry(workspace, project);
    let Some(record) = ledger.installation(&entry, kind, name)? else {
        return Ok(None);
    };
    let block = block_of(project, kind);
    Ok(Some(told(
        workspace,
        project,
        record,
        offers,
        block.as_ref(),
    )))
}

/// Every installation of `kind` the ledger records in `project` that
/// nothing offers any more where it came from, by name in name order: what
/// `uninstall --orphaned` takes out. Orphaned, or edited and orphaned,
/// which reads as edited, an edit being said first; an update leaves both
/// alike, with nothing to write either from (`update::Step::Orphaned`), and
/// taking out the edited one is the uninstall's to refuse unless forced.
///
/// Not one from a loaded workspace that cannot be opened now: what that
/// offers cannot be told, so its copies read as orphaned only while it is
/// away, and taking them out on that would take out what a drive mounted
/// again tomorrow offers still. A record from before origins were kept may
/// have come from any loaded workspace, so it waits while any is away.
///
/// Nor one whose place where it came from could not be read, in the
/// workspace's own ([`own_failures`]) or a loaded workspace's
/// ([`LoadFailure::Artifact`]): front matter that no longer parses, or a
/// folder that cannot be listed, offers nothing there, and is still
/// somebody's artifact to mend, not a reason to take its copies out. By
/// place ([`places`]), since what cannot be read cannot say its name.
pub(crate) fn orphaned(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
) -> Result<Vec<String>, LedgerError> {
    let loads = workspace.loads();
    let away: Vec<Origin> = loads
        .iter()
        .filter(|loaded| loaded.workspace.is_err())
        .map(load_origin)
        .collect();
    let mut unread: Vec<PathBuf> = own_failures(workspace, &[kind])
        .iter()
        .map(|failure| failure.path().to_path_buf())
        .collect();
    unread.extend(from_loads(workspace, &[kind]).failures.iter().filter_map(
        |failure| match failure {
            LoadFailure::Artifact(failure) => Some(failure.path().to_path_buf()),
            _ => None,
        },
    ));
    Ok(surveyed(workspace, ledger, project, kind)?
        .into_iter()
        .filter(|surveyed| surveyed.offered.is_empty())
        .filter(
            |surveyed| match (&surveyed.record.origin, surveyed.record.source_id) {
                (Some(origin), _) => !away.contains(origin),
                (None, Some(_)) => true,
                (None, None) => away.is_empty(),
            },
        )
        .filter(|surveyed| {
            // A failure about the artifact's own file, or about a folder it
            // sits in.
            !places(workspace, &loads, &surveyed.record)
                .iter()
                .any(|place| {
                    unread
                        .iter()
                        .any(|path| path.starts_with(place) || place.starts_with(path))
                })
        })
        .map(|surveyed| surveyed.record.name.into_string())
        .collect())
}

/// What the workspace's own could not offer of `kinds`: an artifact of it
/// that cannot be read, or a folder of them that cannot be listed. None of
/// a kind it does not keep ([`WeaverWorkspace::own_kinds`]), which it
/// offers none of. Failures collected, as every discovery's, for a front
/// end to warn with, as [`offered_by_loads`] has a load's.
pub fn own_failures(workspace: &WeaverWorkspace, kinds: &[Kind]) -> Vec<DiscoveryFailure> {
    let kept = workspace.own_kinds();
    let kinds: Vec<Kind> = kinds
        .iter()
        .copied()
        .filter(|kind| kept.contains(kind))
        .collect();
    if kinds.is_empty() {
        return Vec::new();
    }
    let own = workspace.own();
    let (_, failures) = Catalog::discover_kinds(std::slice::from_ref(&own), &kinds).into_parts();
    failures
}

/// Where what `record` installed would sit now in each place its origin
/// names that offers by discovery: the workspace's own, or a loaded
/// workspace's that can be opened. A record from before origins were kept
/// may have come from any of them. None for one from the shelf, whose
/// offers are the ledger's rows, not a read that can fail.
fn places(workspace: &WeaverWorkspace, loads: &[Loaded], record: &InstallRecord) -> Vec<PathBuf> {
    let any = record.origin.is_none() && record.source_id.is_none();
    let names = |origin: &str| any || record.origin.as_deref() == Some(origin);
    let mut places = Vec::new();
    if names(WORKSPACE_ORIGIN) {
        places.extend(crate::lifecycle::place(
            &workspace.own(),
            record.kind,
            &record.name,
        ));
    }
    for loaded in loads {
        if let Ok(source) = &loaded.workspace {
            if names(&load_origin(loaded)) {
                places.extend(crate::lifecycle::place(
                    &source.own(),
                    record.kind,
                    &record.name,
                ));
            }
        }
    }
    places
}

/// The origin a record names for what it installed from `loaded`, as
/// [`Offer::origin`] spells it.
fn load_origin(loaded: &Loaded) -> Origin {
    let entry = paths::to_config_string(&loaded.entry)
        .unwrap_or_else(|| loaded.entry.display().to_string());
    loaded_origin(&entry)
}

/// The block of `project`'s `AGENTS.md` that a rule's copies are told by;
/// none for a skill, whose copies are folders.
fn block_of(project: &MindProject, kind: Kind) -> Option<AgentsMd> {
    // Matched on the kind, so a third one has to say how its copies are told.
    match kind {
        Kind::Skill | Kind::Mcp => None,
        Kind::Rule => rules::block_file(project.root()),
    }
}

/// How the installation `record` in `project` is found, against `offers`,
/// every offer of its kind, and `block`, the project's `AGENTS.md` as it
/// reads now for a rule: what [`surveyed`] says of each.
fn told(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    record: InstallRecord,
    offers: &[Offer],
    block: Option<&AgentsMd>,
) -> Surveyed {
    let offered = offered_for(&record, offers);
    // Matched on the kind, so a third one has to say how its copies are told.
    let (copies, standing) = match record.kind {
        Kind::Skill => {
            let copies = standing::skill_copies(workspace, project, &record, &offered);
            let standing = standing::skill_standing(workspace, &copies, &offered);
            (copies, standing)
        }
        Kind::Rule => {
            let copies = standing::rule_copies(workspace, project, &record, block, &offered);
            let standing = standing::rule_standing(workspace, &record, &copies, &offered);
            (copies, standing)
        }
        Kind::Mcp => entries::told(workspace, project, &record, &offered),
    };
    let offered = offered.into_iter().cloned().collect();
    Surveyed {
        record,
        offered,
        copies,
        standing,
    }
}

/// Everything of `kind` offered to install, from wherever it is offered, in
/// the order [`survey`] gives it: the workspace's own, what it loads, the
/// shelf. Nothing whose name no copy can be filed under
/// ([`is_installable_name`]).
pub(crate) fn offers(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    kind: Kind,
) -> Result<Vec<Offer>, LedgerError> {
    // The workspace's own come first: what it keeps for its projects before
    // what it gathered from elsewhere.
    let mut offers = Vec::new();
    if workspace.own_kinds().contains(&kind) {
        let own = workspace.own();
        let catalog = Catalog::discover_kinds(std::slice::from_ref(&own), &[kind]);
        offers.extend(catalog.artifacts().iter().map(|artifact| Offer::Workspace {
            kind,
            name: artifact.name().to_owned(),
            summary: artifact.summary().map(str::to_owned),
            path: artifact.path().to_path_buf(),
        }));
    }
    offers.extend(loaded_offers(workspace, kind));
    offers.extend(
        ledger
            .gathered()?
            .into_iter()
            .filter(|gathered| gathered.kind == kind)
            .map(Offer::Shelf),
    );

    offers.retain(|offer| is_installable_name(kind, offer.name()));
    Ok(offers)
}

/// Where what `record` installed is offered now, among `offers`: what its
/// copies are held to when it has no hash, and what tells whether the place
/// it came from offers something else by now (D-04).
///
/// By the origin it records; a record from before origins were kept names a
/// shelf source by its id, or else came from the workspace's own or a
/// loaded workspace's, any of which may be the one, since the record does
/// not say.
pub(crate) fn offered_for<'a>(record: &InstallRecord, offers: &'a [Offer]) -> Vec<&'a Offer> {
    offers
        .iter()
        .filter(|offer| offer.kind() == record.kind && offer.name() == record.name)
        .filter(|offer| match (&record.origin, record.source_id) {
            (Some(origin), _) => offer.origin() == *origin,
            (None, Some(id)) => matches!(offer, Offer::Shelf(gathered) if gathered.source.id == id),
            (None, None) => !matches!(offer, Offer::Shelf(_)),
        })
        .collect()
}

/// Whether the installation `record` came from the library copy, `library`,
/// as far as the ledger tells: what it records of where it came from
/// matches that ([`offered_for`]), and nothing offered from `elsewhere`.
///
/// A record names its origin, and one naming the workspace's own came from
/// the library. One from before origins were kept may have come from the
/// workspace's own or from any workspace it loads, and is taken for the
/// library's only while no loaded workspace offers its name: while one
/// does, removing the library copy leaves it offered, and taking a copy out
/// that may be that one's would be a guess. One rule for removing one of
/// the workspace's own everywhere ([`crate::everywhere`]) and for the
/// projects a snapshot says hold a copy of it ([`crate::view::LibraryHolder`]),
/// so the two never name different projects.
pub(crate) fn from_the_library(
    record: &InstallRecord,
    library: &Offer,
    elsewhere: &[Offer],
) -> bool {
    !offered_for(record, std::slice::from_ref(library)).is_empty()
        && offered_for(record, elsewhere).is_empty()
}

/// The one candidate called `name` that `from` names, among what `project`
/// can be offered.
///
/// `from` is needed only when two places offer the same name: a project has
/// one directory of that name in each folder it is installed in, and which
/// of the two goes there is not something to guess.
///
/// An offer `from` names as printed, by its origin, is the one it names,
/// though a looser reading names another too ([`Offer::named_by`]): one
/// address gathered at its default branch and again at `main` is offered as
/// `<url>` and `<url>#main`, and `<url>` is also the second's address
/// without its branch. Refusing that as offered twice would leave the first
/// with no `--from` of its own, though one is printed beside it, and so
/// leave the line the install screen runs for its box refused. A name comes
/// next, a source's and a loaded workspace's alike, so a word both are
/// called is refused as offered twice, each still named by its origin. The
/// looser readings stand when nothing is printed or named as `from`, which
/// is what they are for, and another spelling of a source's URL only when
/// nothing is named more closely ([`respelled`] says it).
///
/// A project with nowhere to put `kind` ([`targets`]) is refused here, where
/// [`survey`] offers it nothing: "not offered" would send somebody to gather
/// what could never be installed there anyway.
pub fn offered(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
    from: Option<&str>,
) -> Result<Candidate, InstallError> {
    receives(project, kind)?;
    let candidates = survey(workspace, ledger, project, kind)?;
    let mut matching: Vec<Candidate> = candidates
        .into_iter()
        .filter(|candidate| candidate.name() == name)
        .filter(|candidate| from.is_none_or(|from| candidate.comes_from(from)))
        .collect();
    if let Some(from) = from {
        let closest = matching
            .iter()
            .filter_map(|candidate| candidate.offer.named_by(from))
            .max();
        matching.retain(|candidate| candidate.offer.named_by(from) == closest);
    }
    match matching.len() {
        0 => Err(InstallError::NotOffered {
            name: name.to_owned(),
            from: from.map(|from| redacted(from).into_owned()),
        }),
        1 => Ok(matching.remove(0)),
        _ => Err(InstallError::OfferedTwice {
            name: name.to_owned(),
            origins: matching.iter().map(Candidate::origin).collect(),
        }),
    }
}

/// The one candidate `reference` names among what `project` can be offered:
/// of the kind it is qualified with (`rule:git/commits`), or, typed bare, of
/// the one kind that offers its name, as [`offered`] finds one of a kind.
///
/// A bare name that more than one kind offers is refused
/// ([`InstallError::OfferedAsKinds`]), for the qualifier to say which, as
/// `mind show` refuses one: a skill and a rule of one name are two things,
/// and which of them goes in is not something to guess. A kind offering
/// the name from two places counts as offering it, and says so itself
/// ([`InstallError::OfferedTwice`]) when no other kind does.
///
/// Only the kinds a copy could be filed under that name are asked
/// ([`is_installable_name`]), so `git/commits`, which no skill can be
/// called, is a rule's however it is typed. When no kind offers it, the
/// first refusal that says more than that nothing is offered is the one
/// given: a project with nowhere to put a skill is told so, as before
/// rules could be installed, rather than sent to gather one. Only where it
/// is worth saying, though: a kind the project has nowhere to put is said
/// over another kind offering nothing of the name only when the name is
/// offered as that kind. Otherwise a typo in a skill's name, in a project
/// a `mind.toml` marks, which names no agent to give a server, would be
/// answered with why no MCP server can go there. A ledger that cannot be
/// read is said first, from whichever kind found it.
///
/// `from` is taken as typed first, so a shelf source's address or
/// `workspace` means what it says, and only when that offers a kind nothing,
/// as a path to a loaded workspace typed from `base` ([`resolve_from`]).
/// Kind by kind: a kind that cannot go into the project at all refuses
/// for that, which keeps no other from being found by the path. A shelf
/// source's name that is also such a path names two places, refused as
/// offered twice, as one a loaded workspace is called too is
/// ([`Naming::Named`]): `--from team` named the workspace loaded from
/// `./team` before sources had names, and is not to name a source unsaid.
pub fn offered_typed(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    reference: &Reference,
    from: Option<&str>,
    base: &Path,
) -> Result<Candidate, InstallError> {
    let name = reference.name();
    let resolved = from.map(|from| resolve_from(workspace, base, from));
    let path = resolved
        .as_deref()
        .filter(|resolved| Some(*resolved) != from);
    let ask = |kind: Kind| match offered(workspace, ledger, project, kind, name, from) {
        Err(InstallError::NotOffered { .. }) if path.is_some() => {
            offered(workspace, ledger, project, kind, name, path)
        }
        Ok(found)
            if path.is_some()
                && matches!(found.offer, Offer::Shelf(_))
                && from.and_then(|from| found.offer.named_by(from)) == Some(Naming::Named) =>
        {
            match offered(workspace, ledger, project, kind, name, path) {
                Ok(loaded) if loaded.origin() != found.origin() => {
                    Err(InstallError::OfferedTwice {
                        name: name.to_owned(),
                        origins: vec![loaded.origin(), found.origin()],
                    })
                }
                _ => Ok(found),
            }
        }
        asked => asked,
    };
    if let Some(kind) = reference.kind() {
        return ask(kind);
    }
    let mut asked: Vec<(Kind, Result<Candidate, InstallError>)> = filed_as(name)
        .into_iter()
        .map(|kind| (kind, ask(kind)))
        .collect();
    if let Some(index) = asked
        .iter()
        .position(|(_, asked)| matches!(asked, Err(InstallError::Ledger(_))))
    {
        return asked.swap_remove(index).1;
    }
    let offering = |asked: &Result<Candidate, InstallError>| {
        matches!(asked, Ok(_) | Err(InstallError::OfferedTwice { .. }))
    };
    let kinds: Vec<Kind> = asked
        .iter()
        .filter(|(_, asked)| offering(asked))
        .map(|(kind, _)| *kind)
        .collect();
    if kinds.len() > 1 {
        return Err(InstallError::OfferedAsKinds {
            name: name.to_owned(),
            kinds,
        });
    }
    // Some kind could go into the project, and offers nothing of the name.
    let received = asked
        .iter()
        .any(|(_, asked)| matches!(asked, Err(InstallError::NotOffered { .. })));
    let offered_as = |kind: Kind| {
        offers(workspace, ledger, kind)
            .is_ok_and(|offers| offers.iter().any(|offer| offer.name() == name))
    };
    let index = asked
        .iter()
        .position(|(_, asked)| offering(asked))
        .or_else(|| {
            asked.iter().position(|(kind, asked)| match asked {
                Err(InstallError::NotOffered { .. }) => false,
                Err(InstallError::NoTarget { .. } | InstallError::NoUsableTarget { .. }) => {
                    !received || offered_as(*kind)
                }
                _ => true,
            })
        })
        .unwrap_or(0);
    asked.swap_remove(index).1
}

/// Take what `reference` names back out of `project`, and say which kind it
/// was: the kind it is qualified with, or, typed bare, the one kind the
/// ledger records an installation of that name of, as [`uninstall`] takes
/// one of a kind out.
///
/// A bare name the ledger records as more than one kind is refused
/// ([`InstallError::InstalledAsKinds`]), for the qualifier to say which:
/// taking both out would be twice what was asked. One it records as none
/// changes nothing whichever kind it is taken for, so each kind it could be
/// filed as is asked what is there, and somebody's of either is said
/// ([`Removed::Foreign`]) before the absence the first kind finds: the name
/// was typed because something by it is somewhere.
pub fn uninstall_typed(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    reference: &Reference,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<(Kind, Removed), InstallError> {
    let name = reference.name();
    let kind = match reference.kind() {
        Some(kind) => kind,
        None => {
            let entry = project_entry(workspace, project);
            let kinds = filed_as(name);
            let mut recorded = Vec::new();
            for &kind in &kinds {
                if ledger.installation(&entry, kind, name)?.is_some() {
                    recorded.push(kind);
                }
            }
            match recorded[..] {
                [kind] => kind,
                [] => {
                    let mut absent = None;
                    for kind in kinds {
                        let removed =
                            uninstall_with(workspace, ledger, project, kind, name, homes, edits)?;
                        if matches!(removed, Removed::Foreign { .. }) {
                            return Ok((kind, removed));
                        }
                        absent.get_or_insert((kind, removed));
                    }
                    return Ok(absent.expect("a name is asked of one kind at least"));
                }
                _ => {
                    return Err(InstallError::InstalledAsKinds {
                        name: name.to_owned(),
                        kinds: recorded,
                    })
                }
            }
        }
    };
    uninstall_with(workspace, ledger, project, kind, name, homes, edits)
        .map(|removed| (kind, removed))
}

/// The kinds `reference` could name: the one it is qualified with, or,
/// typed bare, those a copy could be filed as under its name, which
/// [`offered_typed`] and [`uninstall_typed`] ask of it. For a front end to
/// say what is true of each before it is known which one is meant, without
/// deciding for itself what a name can be: a route like `git/commits` is a
/// rule's whatever the folders for skills hold.
pub fn kinds_named(reference: &Reference) -> Vec<Kind> {
    reference
        .kind()
        .map_or_else(|| filed_as(reference.name()), |kind| vec![kind])
}

/// The kinds a copy could be filed as under `name` ([`is_installable_name`]),
/// or every kind when none could, so that what is asked of them refuses the
/// name, saying why: of the kinds an install writes ([`installs`]), since a
/// name typed bare would otherwise be refused for a kind nobody offered it
/// as, where nothing offers it at all.
fn filed_as(name: &str) -> Vec<Kind> {
    let installed: Vec<Kind> = Kind::ALL
        .into_iter()
        .filter(|&kind| installs(kind))
        .collect();
    let kinds: Vec<Kind> = installed
        .iter()
        .copied()
        .filter(|&kind| is_installable_name(kind, name))
        .collect();
    if kinds.is_empty() {
        installed
    } else {
        kinds
    }
}

/// Whether a front end steps through `kind` to install, and a name typed
/// bare is asked of it: a skill, copied into a folder, a rule, a section of
/// a block or a file, and an MCP server, an entry in each agent's own
/// config file (D-10, [`mcp`]), which the commands say what it will run of
/// before they write it, and write from elsewhere than the library only
/// once somebody says yes ([`asks_first`], #95). Matched on the kind, so a
/// kind added says whether it is stepped through before it compiles.
pub const fn installs(kind: Kind) -> bool {
    match kind {
        Kind::Skill | Kind::Rule | Kind::Mcp => true,
    }
}

/// Whether an install of `kind` from `origin`, as `--from` takes it, waits
/// for somebody to say yes before it writes: an MCP server whose definition
/// does not come from the workspace's own library, a loaded workspace's or
/// the shelf's (#95). A server is a command every agent in the project
/// runs, or an address each sends what it is given to, and one written
/// somewhere else is somebody else's to have written; the library's own is
/// the workspace's, written by whoever keeps it, and installed as a skill
/// is. Matched on the kind, so a kind added says whether it waits.
pub fn asks_first(kind: Kind, origin: &str) -> bool {
    match kind {
        Kind::Skill | Kind::Rule => false,
        Kind::Mcp => origin != WORKSPACE_ORIGIN,
    }
}

/// Whether somebody has said yes to an install that waits for one
/// ([`asks_first`]): the CLI's `--yes`. Without it, such an install writes
/// nothing and says what it would give each agent ([`mcp::preview`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confirmed {
    Yes,
    No,
}

/// Refuse `project` when an install of `kind` has nowhere to go in it: no
/// [`targets`] for a kind copied into a folder, for a rule into a project
/// marked by its settings, which goes into its `AGENTS.md`, no agent to
/// read it ([`rules::receives`]), and for an MCP server no agent to give an
/// entry ([`mcp::receives`]). What a front end says in place of the
/// offers of `kind`, which [`survey`] leaves empty for such a project.
pub fn receives(project: &MindProject, kind: Kind) -> Result<(), InstallError> {
    match kind {
        Kind::Rule if project.settings().is_some() => rules::receives(project),
        Kind::Mcp => mcp::receives(project),
        Kind::Skill | Kind::Rule => targets(project, kind).map(drop),
    }
}

/// Copy one shelf entry into a project: a copy in each of its [`targets`],
/// recorded as one installation with a target for each.
///
/// The copies are built in the project's scratch folder and put in place
/// together, all of them or none (`copy::Stage::place`), so an agent that
/// reads two of them never finds two versions of one skill. What an
/// interrupted install left there, or beside a copy, is cleared first and
/// said ([`Installation::swept`]).
///
/// `homes` are the folders the agents read for every project, which the
/// front end decides: a copy that would land inside one of them is refused
/// ([`InstallError::UserScope`]).
///
/// A copy edited since it was installed refuses the whole install, with
/// nothing written in any target ([`Installed::Edited`]): it is somebody's
/// work, as a copy nothing records is, until [`install_with`] is told to
/// write over it.
///
/// A rule is rendered with the project's others ([`rules`]), and an MCP
/// server is no copy but an entry in the config file of each agent the
/// project names, all of them written or none ([`mcp`]).
pub fn install(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
) -> Result<Installation, InstallError> {
    install_with(workspace, ledger, project, candidate, homes, Edits::Kept)
}

/// [`install`], writing over a copy edited since it was installed when
/// `edits` says so, as `--force` does. Never over a copy nothing records,
/// which is somebody's whatever is asked.
pub fn install_with(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    let kind = candidate.kind();
    let name = candidate.name().to_owned();
    installable_name(kind, &name)?;
    // Matched on the kind, so a third one has to say how it is installed.
    match kind {
        Kind::Skill => {}
        Kind::Rule => return rules::install(workspace, ledger, project, candidate, homes, edits),
        Kind::Mcp => return mcp::install(workspace, ledger, project, candidate, homes, edits),
    }
    let entry = project_entry(workspace, project);
    // The folder is named after the artifact, not after the folder it had on
    // the shelf: inside a project a skill's directory has to match its
    // declared name, which is what `validate` checks and what an agent uses to
    // find it.
    let targets = targets(project, kind)?;
    let copies: Vec<PathBuf> = targets
        .iter()
        .map(|target| copy_at(&target.folder, kind, &name))
        .collect();
    // A project at the user's home has its agents' own folders for targets:
    // `.claude/skills` there is Claude's skills for every project, and
    // `.agents/skills` is the one Codex and OpenCode read everywhere. That is
    // the user-scope install D-22 leaves for later, behind a `--user` of its
    // own, so it is refused here rather than done by accident. The targets
    // cannot tell, being relative to the project, and core has no home to
    // compare them with: the front end hands it the agents' folders.
    if let Some(error) = user_scope(homes, &copies) {
        return Err(error);
    }
    // Somebody's work in any of them refuses the whole install: one copy
    // written beside it would leave the agents reading both folders two
    // different skills of one name. Looked at before the lock, so a refusal
    // waits for nobody, and again under it.
    if candidate.standing == Standing::Foreign {
        let recorded = ledger.copies(&entry, kind)?;
        if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
            return Ok(Installed::Foreign { name, path }.into());
        }
    }
    // Held until the record is written, so that what the ledger says was
    // installed is the copy that is there: two installs of one name at once
    // would otherwise each record theirs, and the copies left would be
    // whichever moved into place last.
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    install_held(workspace, ledger, project, candidate, homes, edits)
}

/// [`install_with`], with the workspace's lock held already: for a writer
/// that installs several things under one hold of it, as an update does
/// ([`crate::update`]), since the lock is not re-entrant. Everything an
/// install refuses is looked at again here, under the lock, whatever was
/// looked at before it.
pub(crate) fn install_held(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Installation, InstallError> {
    let kind = candidate.kind();
    let name = candidate.name().to_owned();
    installable_name(kind, &name)?;
    // Matched on the kind, so a third one has to say how it is installed.
    match kind {
        Kind::Skill => {}
        Kind::Rule => {
            return rules::install_held(workspace, ledger, project, candidate, homes, edits)
        }
        Kind::Mcp => return mcp::install_held(workspace, ledger, project, candidate, homes, edits),
    }
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
    if !from.is_dir() {
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

    // A loaded workspace can be the project itself, keeping its own in a
    // folder the install writes in: then what would be copied is already
    // there. Nothing is written, and above all nothing is recorded: a record
    // would let `uninstall` delete the loaded workspace's own files.
    if let Some(to) = copies.iter().find(|to| same_place(&from, to)) {
        return Ok(Installed::Unchanged {
            name,
            paths: vec![to.clone()],
            unhashed: None,
        }
        .into());
    }
    for to in &copies {
        // One inside the other is worse: replacing `to` starts by deleting
        // it, and with it `from`, or copies `from` into itself.
        if overlaps(&from, to) {
            return Err(InstallError::Overlaps {
                from,
                to: to.clone(),
            });
        }
        // And never over what a loaded workspace keeps as its own: a project
        // can be the workspace it loads, sharing a folder, and a record from
        // before the load would otherwise let this overwrite it.
        if let Some(loaded) = kept_by_a_load(workspace, kind, to) {
            return Err(InstallError::KeptByALoad {
                path: to.clone(),
                loaded,
            });
        }
    }

    // Strict, where gathering was lenient: the copy is what the project's
    // agents load, so one they would skip is not put there.
    let refused = unfit(kind, &from, &name)?;
    if !refused.is_empty() {
        return Err(InstallError::Unfit {
            name: name.into_string(),
            issues: refused,
        });
    }

    // Looked at again under the lock, before anything is written. The
    // candidate's standing is from a survey made before it was held, minutes
    // before on the install screen, and since then somebody may have written
    // a folder of that name by hand, or edited a copy. Only a turn to Foreign
    // or to Edited changes what happens here, and that one must: it is
    // somebody's work, never written over unless forced.
    let recorded = ledger.copies(&entry, kind)?;
    if let Some(path) = unrecorded(project.root(), kind, &copies, &recorded) {
        return Ok(Installed::Foreign { name, path }.into());
    }
    // And a copy somebody edited since it was installed is theirs as much,
    // unless the install is forced: wherever the record has it, since an
    // install records its copies afresh, and one it did not write over
    // would be left with no record.
    let edited = edited(workspace, ledger, project, kind, &name)?;
    if edits == Edits::Kept && !edited.is_empty() {
        return Ok(Installed::Edited {
            name,
            paths: edited,
        }
        .into());
    }
    // Forced, each of them is written over even where it reads as holding
    // what is offered: a link to the same bytes does to `copy::same`, which
    // follows it, but it is no copy an install writes, and left in place it
    // would read as edited still, recorded with no hash.
    let over: Vec<PathBuf> = copies
        .iter()
        .filter(|copy| edited.iter().any(|path| same_place(copy, path)))
        .cloned()
        .collect();

    // Links are followed only inside where the artifact comes from: the
    // workspace that keeps it, or the shelf folder it was gathered into.
    let within = match &candidate.offer {
        Offer::Loaded { entry, .. } => paths::normalize(&workspace.root().join(entry)),
        Offer::Workspace { .. } => workspace.root().to_path_buf(),
        Offer::Shelf(_) => from.clone(),
    };
    // Out of git before anything is written, so a copy in a local project
    // never shows in `git status`, not even for a moment, as its settings
    // never do; and a block this cannot edit refuses the install with
    // nothing written, rather than putting copies one machine has in every
    // teammate's way. Taken back out if the copies then fail, from where
    // nothing is left: a line naming an empty place would keep whatever
    // somebody writes there later out of git without a word. Only the
    // copies git tracks nothing in, as an adopted one often does.
    let excluded = match local(project) {
        Some(key) => Some(exclude::add(
            project.root(),
            key,
            &entries(project, &untracked(project, &copies)),
        )?),
        None => None,
    };
    let undo = || {
        if let Some(key) = local(project) {
            let gone: Vec<PathBuf> = copies
                .iter()
                .filter(|copy| !holds(copy, kind))
                .cloned()
                .collect();
            let _ = exclude::remove(project.root(), key, &entries(project, &gone));
        }
    };
    // Built in the project's scratch folder rather than beside a copy, in a
    // folder agents read: neither a copy half made nor one an interrupted
    // run left behind sits where an agent would load it.
    let scratch = project.scratch_dir();
    let mut stage = match copy::Stage::open(&scratch) {
        Ok(stage) => stage,
        Err(source) => {
            undo();
            return Err(InstallError::Write {
                path: scratch,
                source,
                swept: Vec::new(),
            });
        }
    };
    // Said, since what lands is then not all of what was offered.
    let left_out = left_out(&from);
    let changes = match stage.place_over(&from, &copies, &within, &left_out_names(), &over) {
        Ok(changes) => changes,
        // What was cleared on the way goes out with the error rather than
        // with the stage: it was cleared whether or not this copy lands.
        Err(copy::PlaceError { target, source }) => {
            undo();
            return Err(InstallError::Write {
                // The target it failed at, since the error may not say: or
                // the first, as with one target, when it failed building the
                // copy, which is no target's doing.
                path: target.unwrap_or_else(|| copies[0].clone()),
                source,
                swept: stage.swept().to_vec(),
            });
        }
    };
    let swept = stage.swept().to_vec();
    // Cleared before the ledger is written rather than after, so the scratch
    // folder is held for no longer than the copy takes.
    drop(stage);
    // Of the copies as they now stand, not of where they came from: what was
    // written is what a later look at a copy is compared with, and the
    // source may hold links the copy resolved. A copy that cannot be read
    // back is recorded all the same, with no hash, since without a record it
    // would be Foreign for good, and D-04 reads a record with no hash as it
    // reads one from before hashes were kept. That is said, in what this
    // returns and in the log: no copy left as it was holds what the hash
    // refuses, a link being an edit, written over when forced, but one can
    // change between landing and being read back.
    let (content_hash, unhashed) = match one_hash(&copies) {
        Ok(hash) => (Some(hash), None),
        Err(reason) => (None, Some(reason)),
    };

    // A source to point at only for what was gathered: the workspace's own
    // came from nowhere but the workspace. The origin says where every offer
    // came from, those two included, in the one spelling `--from` takes.
    let source_id = match &candidate.offer {
        Offer::Shelf(gathered) => Some(gathered.source.id),
        Offer::Workspace { .. } | Offer::Loaded { .. } => None,
    };
    ledger.installed(
        &entry,
        &InstallRecord {
            kind,
            name: name.clone(),
            source_id,
            origin: Some(candidate.origin()),
            content_hash,
            targets: targets
                .iter()
                .zip(&copies)
                .map(|(target, to)| Target {
                    agent: target.agent(),
                    path: route(project.root(), to),
                    // Every copy holds the tree `content_hash` names.
                    content_hash: None,
                })
                .collect(),
            entries: Vec::new(),
        },
    )?;
    let mut detail = format!("{name} into {}", project.name());
    for left in &left_out {
        if let Some(name) = left.path.file_name() {
            detail.push_str(&format!(", without {}", name.to_string_lossy()));
        }
    }
    if let Some(reason) = &unhashed {
        detail.push_str(&format!(", recorded without a hash: {reason}"));
    }
    ledger.log(Action::Install, Some(&entry), Outcome::Ok, Some(&detail))?;

    // One change for the installation, as it is recorded once: added when
    // every copy is new, unchanged when none is, and updated otherwise, a
    // copy put back in one folder beside one that was current included.
    let paths = copies;
    let installed = if changes
        .iter()
        .all(|change| *change == copy::Change::Unchanged)
    {
        Installed::Unchanged {
            name,
            paths,
            unhashed,
        }
    } else if changes.iter().all(|change| *change == copy::Change::Added) {
        Installed::Added {
            name,
            paths,
            unhashed,
        }
    } else {
        Installed::Updated {
            name,
            paths,
            unhashed,
        }
    };
    Ok(Installation {
        installed,
        swept,
        left_out,
        excluded,
        rendered: None,
        entries: None,
    })
}

/// The refusal for the first of `copies` inside one of the agents' own
/// folders, if one is.
pub(crate) fn user_scope(homes: &AgentHomes, copies: &[PathBuf]) -> Option<InstallError> {
    let (path, home, reader) = in_agent_home(homes, copies)?;
    Some(InstallError::UserScope { path, home, reader })
}

/// The first of `paths` inside one of the agents' own folders, if one is,
/// with that folder and who reads it: for each writer that refuses one to
/// say in its own words, an install's copy here and a block of rules in
/// [`crate::agents_md`].
///
/// Resolved as far as each path exists, so a home reached through a link,
/// or a project reached through one, is still the folder it is.
pub(crate) fn in_agent_home(
    homes: &AgentHomes,
    paths: &[PathBuf],
) -> Option<(PathBuf, PathBuf, &'static str)> {
    let folders: Vec<(&str, &Path, PathBuf)> = homes
        .folders()
        .into_iter()
        .filter_map(|(reader, folder)| Some((reader, folder, resolved(folder)?)))
        .collect();
    paths.iter().find_map(|path| {
        let at = resolved(path)?;
        folders
            .iter()
            .find(|(_, _, home)| at.starts_with(home))
            .map(|(reader, folder, _)| (path.clone(), folder.to_path_buf(), *reader))
    })
}

/// The key of a project's group in the exclude block, when what is installed
/// into it stays out of git: a local project's (D-02), named by its id, which
/// stays when the project is renamed or moved. None for a shared one, whose
/// copies are the team's to commit, and for one a `mind.toml` marks, since a
/// project from before settings was committed by design.
///
/// Asked by whatever else Mindweaver writes into a project, as Claude's rule
/// files ([`crate::claude_rules`]), so all of it is kept out of git alike.
pub(crate) fn local(project: &MindProject) -> Option<&str> {
    project
        .settings()
        .filter(|settings| settings.share == Share::Local)
        .map(|settings| settings.id.as_str())
}

/// One exclude entry for each of `copies`, a folder each, as the project
/// spells it.
pub(crate) fn entries(project: &MindProject, copies: &[PathBuf]) -> Vec<exclude::Entry> {
    copies
        .iter()
        .filter_map(|copy| paths::relative_to(copy, project.root()))
        .map(exclude::Entry::directory)
        .collect()
}

/// Those of `copies` git tracks nothing inside, which is what a local
/// project keeps out of git: a copy not there yet among them.
///
/// A line for a copy git tracks would keep none of its files out, since an
/// ignore rule leaves alone what is in the index already, and would hide
/// every file added to it later from `git status`, where a teammate would
/// never see it to commit: one a team committed, or one a project that was
/// shared kept when it turned local.
/// Mindweaver runs no git command and leaves the index as it is, so such a
/// copy stays the team's in git, written over by an update as a tracked
/// file is, for the change to show. An index that cannot be read takes
/// nothing for tracked: the exclude edit that follows reads the same
/// repository, and is refused, as it was, if that cannot be read either.
pub(crate) fn untracked(project: &MindProject, copies: &[PathBuf]) -> Vec<PathBuf> {
    copies
        .iter()
        .filter(|copy| {
            paths::relative_to(copy, project.root())
                .is_none_or(|inside| !exclude::tracks(project.root(), &inside).unwrap_or(false))
        })
        .cloned()
        .collect()
}

/// The one hash every copy holds ([`hash::tree`]), or why there is none:
/// a copy that cannot be hashed, or, should one change while the others are
/// read, copies that do not hold the same.
pub(crate) fn one_hash(copies: &[PathBuf]) -> Result<ContentHash, String> {
    let mut found: Option<(ContentHash, &Path)> = None;
    for copy in copies {
        let hash = hash::tree(copy).map_err(|error| error.to_string())?;
        match &found {
            Some((first, at)) if *first != hash => {
                return Err(format!(
                    "{} does not hold what {} does",
                    copy.display(),
                    at.display()
                ))
            }
            Some(_) => {}
            None => found = Some((hash, copy)),
        }
    }
    found
        .map(|(hash, _)| hash)
        .ok_or_else(|| String::from("no copy was written"))
}

/// What stops the artifact at `from` from being installed as `name`:
/// whichever strict rules it breaks once filed under that name
/// ([`artifact::strict_issues`]).
///
/// Read from the files about to be copied rather than from what the ledger
/// noted when it was gathered, because the shelf is a folder anybody can
/// edit, and the workspace's own are edited as a matter of course. And held
/// to `name` rather than to the folder they sit in, because `name` is the
/// folder the copy will be: a shelf folder keeps the source's spelling, and
/// `name` is what the ledger recorded, which the files stop declaring once
/// they are edited, or once the source renames the skill and the record of
/// the old name stays behind.
fn unfit(kind: Kind, from: &Path, name: &str) -> Result<Vec<ValidationIssue>, InstallError> {
    // Matched on the kind, so a third one has to say what it is held to.
    let artifact = match kind {
        Kind::Skill => Artifact::skill(from, from)?,
        // A rule is held to its route, which `installable_name` asks, and
        // its front matter says nothing an agent would pass it over for;
        // nor does an install of one come here ([`rules::install`]), nor of
        // a definition, which is refused before.
        Kind::Rule | Kind::Mcp => return Ok(Vec::new()),
    };
    Ok(artifact
        .manifest()
        .map_or_else(Vec::new, |manifest| artifact::strict_issues(manifest, name)))
}

/// Take one artifact back out of a project: every copy its installation
/// recorded, wherever the project would install it now.
///
/// Only if the ledger says Mindweaver put it there, and only the copies it
/// says Mindweaver wrote: a folder of that name in a target the record does
/// not name is somebody's, and stays. Anything else is reported and left
/// where it is.
///
/// `homes` are the agents' own folders, which the front end decides as it
/// does for [`install`]: taking one thing out can change how the rest reach
/// the agents.
///
/// A copy edited since it was installed refuses the whole uninstall, with
/// every copy left where it is and the record kept ([`Removed::Edited`]),
/// until [`uninstall_with`] is told to take it out.
pub fn uninstall(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
    homes: &AgentHomes,
) -> Result<Removed, InstallError> {
    uninstall_with(workspace, ledger, project, kind, name, homes, Edits::Kept)
}

/// [`uninstall`], taking out a copy edited since it was installed when
/// `edits` says so, as `--force` does.
pub fn uninstall_with(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
    homes: &AgentHomes,
    edits: Edits,
) -> Result<Removed, InstallError> {
    installable_name(kind, name)?;
    if kind == Kind::Rule && project.settings().is_some() {
        return rules::uninstall(workspace, ledger, project, name, homes, edits);
    }
    if kind == Kind::Mcp {
        return mcp::uninstall(workspace, ledger, project, name, edits);
    }
    // Held from before the record is read until it is dropped, for the
    // reason `install` holds it: an install of the same name at the same
    // moment comes wholly before this or wholly after it. Between the copies
    // going and their record going, it would leave a copy with no record,
    // which Mindweaver would never touch again.
    let _lock = workspace.lock().map_err(WorkspaceError::from)?;
    let entry = project_entry(workspace, project);
    let Some(record) = ledger.installation(&entry, kind, name)? else {
        return Ok(match held_where(project, kind, name) {
            Some(path) => Removed::Foreign {
                name: ArtifactName::from(name),
                path,
            },
            None => Removed::Absent {
                name: ArtifactName::from(name),
            },
        });
    };

    let mut paths = Vec::with_capacity(record.targets.len());
    for target in &record.targets {
        let path = recorded_copy(project, kind, name, &target.path)?;
        if let Some(loaded) = kept_by_a_load(workspace, kind, &path) {
            return Err(InstallError::KeptByALoad { path, loaded });
        }
        paths.push(path);
    }
    // Every copy or none: one left behind with no record would be nobody's
    // to take out again, and its record kept beside copies gone would say
    // they are missing.
    if edits == Edits::Kept {
        let edited = edited(workspace, ledger, project, kind, name)?;
        if !edited.is_empty() {
            return Ok(Removed::Edited {
                name: ArtifactName::from(name),
                paths: edited,
            });
        }
    }
    for path in &paths {
        let removed = match kind.layout() {
            Layout::Directory { .. } if path.exists() => fs::remove_dir_all(path),
            // A copy of a rule is the file itself, never a link to one, so
            // whatever is at its place is taken away rather than followed.
            Layout::Files { .. } if fs::symlink_metadata(path).is_ok() => fs::remove_file(path),
            _ => Ok(()),
        };
        removed.map_err(|source| InstallError::Write {
            path: path.clone(),
            source,
            swept: Vec::new(),
        })?;
        // The folders its route was filed in, which the install made on the
        // way, go too while they are empty, as a removal of a rule leaves
        // none (`lifecycle::prune`); not the folder of rules itself.
        if let Layout::Files { .. } = kind.layout() {
            let folder = path.ancestors().nth(name.split('/').count());
            if let Some(folder) = folder {
                crate::lifecycle::prune(folder, path.parent());
            }
        }
    }
    ledger.uninstalled(&entry, kind, name)?;
    let detail = format!("{name} from {}", project.name());
    ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;

    // After the copies, so none shows in `git status` on its way out; and
    // said rather than raised when the block cannot be edited, since they
    // are gone and so is their record.
    let still_excluded = local(project)
        .and_then(|key| exclude::remove(project.root(), key, &entries(project, &paths)).err())
        .map(|error| error.to_string());
    Ok(Removed::Removed {
        name: ArtifactName::from(name),
        paths,
        still_excluded,
        rendered: None,
    })
}

/// Where somebody's `name`, of `kind`, is in `project` while the ledger
/// records no installation of it there: what an uninstall leaves, saying so
/// ([`Removed::Foreign`]), and an install will not write over. None when
/// the ledger records one, or nothing of that name is where an install of
/// it would go.
///
/// A rule in a project marked by its settings is somebody's in its block or
/// in Claude's folder of Mindweaver's rules ([`rules`]), as its uninstall
/// tells.
pub(crate) fn somebodys(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    kind: Kind,
    name: &str,
) -> Result<Option<PathBuf>, InstallError> {
    if kind == Kind::Rule && project.settings().is_some() {
        return rules::somebodys(workspace, ledger, project, name);
    }
    let entry = project_entry(workspace, project);
    if ledger.installation(&entry, kind, name)?.is_some() {
        return Ok(None);
    }
    // Matched on the kind, so a third one says where somebody's is.
    Ok(match kind {
        Kind::Skill | Kind::Rule => held_where(project, kind, name),
        Kind::Mcp => mcp::somebodys(project, name),
    })
}

/// The first target of `project` holding something called `name`, of
/// `kind`, where an install of it would write.
///
/// Nowhere to look is nothing there: a project with no targets now holds
/// nothing an install would have written.
fn held_where(project: &MindProject, kind: Kind, name: &str) -> Option<PathBuf> {
    targets(project, kind)
        .unwrap_or_default()
        .into_iter()
        .map(|target| copy_at(&target.folder, kind, name))
        .find(|copy| holds(copy, kind))
}

/// Where in `project` a target of `name`'s installation recorded at `route`
/// is, as long as that is a folder of the project named after it, or for a
/// rule, the file its route names ([`copy_at`]) in a folder of the project.
///
/// The ledger is the workspace's own, but a removal deletes whatever this
/// says, so a route that climbs out of the project, starts over at a root, or
/// ends in another name is refused rather than followed: before records had
/// targets, a removal was only ever `name` joined onto a folder of the
/// project's.
pub(crate) fn recorded_copy(
    project: &MindProject,
    kind: Kind,
    name: &str,
    route: &str,
) -> Result<PathBuf, InstallError> {
    let path = Path::new(route);
    let inside = path
        .components()
        .all(|component| matches!(component, Component::Normal(_)));
    let named = match kind.layout() {
        Layout::Directory { .. } => path.file_name() == Some(std::ffi::OsStr::new(name)),
        Layout::Files { .. } | Layout::Flat { .. } => {
            path.ends_with(copy_at(Path::new(""), kind, name))
        }
    };
    if !inside || !named {
        return Err(InstallError::Recorded {
            name: name.to_owned(),
            route: route.to_owned(),
        });
    }
    Ok(project.root().join(path))
}

/// Drop the record that Mindweaver installed `name` into `project`, when it
/// names a copy at `at`, saying whether there was one.
///
/// For when what is in the project stops being a copy that was installed:
/// it was removed, which uninstalls it, or something new was written under a
/// name an old record still claims, which outlived its copy. The record is
/// what gives an install leave to overwrite and an uninstall leave to
/// delete, so it must not outlive the copy it was about: whatever somebody
/// writes there next would inherit it. A copy edited or renamed is still
/// there, and keeps its record ([`carry`]), which tells the edit apart
/// (D-04).
///
/// Only a record of a copy at `at`, the place the change reached, and only
/// that copy's part of it ([`Ledger::uninstalled_copy`]): a project marked by
/// its settings authors in a folder of its own, beside the copies installed
/// for its agents, and a change there leaves those copies what was installed.
/// Dropping their record by name would make them Foreign, never updated or
/// removed again, over a change to something else. So would dropping the
/// whole installation when the folder it authors in is also a target, which
/// its settings may make it (`"skills": ".claude/skills"`), and the change
/// reached the copy there: the one in `.agents/skills` is still what was
/// installed. The installation goes with its last copy. A record from before
/// installations had targets names none, and is dropped as it was; a rule's
/// naming none reaches no agent the project names, and stays, since the
/// change reached nothing it installed, and so does an MCP server's, whose
/// entries are in its agents' config files, where no change reaches (D-10).
///
/// In a local project, `at` stops being kept out of git with its record:
/// what is there now, or is written there next, is its author's, and a line
/// left in the exclude block would hide it from `git status` without a word.
///
/// Only the changes in [`crate::lifecycle`] call it, each when its change
/// calls for it. Public, it would let a front end drop a record on a rule of
/// its own. Keeping it here cannot make a front end drop one; changing things
/// through [`crate::lifecycle`] does.
///
/// `workspace` is the one the front end resolved for the command, given
/// rather than found by walking up from the project: a walk misses every
/// workspace that links the project from somewhere that is not above it, and
/// each of those would keep its stale record. Another workspace that links
/// the same project still keeps its own, since nothing in the project says
/// which workspaces link it.
///
/// The ledger is opened to write, as by every command that writes to it, so
/// what that scrubbed comes back too, for the change to hand on: the changes
/// in [`crate::lifecycle`] reach the ledger only through here.
pub(crate) fn disown(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    kind: Kind,
    name: &str,
    at: &Path,
) -> Result<Disowned, InstallError> {
    // Only a workspace that manages the project keeps records about it, and
    // one that has never installed anything has no ledger: this must not
    // create one just to look.
    if !workspace.is_linked(project.root()) || !workspace.ledger_path().is_file() {
        return Ok(Disowned::default());
    }
    // However long the scrub waits for the workspace, which it does only
    // when an older build left a credential in a clone: the change this
    // follows is made, so giving up would leave its record behind, and
    // trying the change again would find nothing to change.
    let (ledger, scrub) = workspace.ledger_to_write_waiting(Duration::MAX)?;
    let entry = project_entry(workspace, project);
    let reached = route(project.root(), at);
    let detail = match ledger.uninstalled_copy(&entry, kind, name, &reached)? {
        Forgotten::Nothing => {
            return Ok(Disowned {
                scrub,
                ..Disowned::default()
            })
        }
        Forgotten::Copy => format!(
            "{name}'s copy at {reached} from {}, which no longer holds what was installed there",
            project.name()
        ),
        Forgotten::Installation => format!(
            "{name} from {}, which no longer holds the copy that was installed",
            project.name()
        ),
    };
    ledger.log(Action::Uninstall, Some(&entry), Outcome::Ok, Some(&detail))?;
    // After the record, as an uninstall takes its entries out after the
    // copies; and handed back rather than raised, since the record is gone.
    let still_excluded = local(project).and_then(|key| {
        exclude::remove(project.root(), key, &entries(project, &[at.to_path_buf()]))
            .err()
            .map(|source| InstallError::StillExcluded {
                path: at.to_path_buf(),
                source,
            })
    });
    Ok(Disowned {
        dropped: true,
        scrub,
        still_excluded,
    })
}

/// Carry the record of the copy of `name` at `at` in `project`, which a
/// rename moved to `to_at` and called `to_name`, along with it
/// ([`Ledger::renamed_copy`]), saying what became of it.
///
/// For a rename of an installed copy: it is what was installed, renamed, so
/// it stays Mindweaver's, as an edit leaves a copy, told by the hash of what
/// was written (D-04), and never left out of a listing of what the project
/// was given. A skill's rename rewrites the `name` line of its manifest, an
/// edit in all but name, so it reads as edited: never written over or
/// deleted unless forced. A rule's moves its file and changes not a byte,
/// so it reads as it did: edited only if it was, and otherwise as what was
/// installed, orphaned while nothing offers it by its new route, which an
/// uninstall takes out unforced. Which of the two it is comes back
/// ([`Carry::edited`]), for the front end to say. Dropping its record, as a
/// rename once did, made it somebody's that nobody can take out again.
/// Where another installation holds `to_name` already, the copy cannot join
/// it, and its record goes instead, as [`disown`] would drop it.
///
/// In a local project its place in the exclude block moves with it while it
/// is recorded, and goes while it is not: a line naming the place it left
/// would keep whatever is written there next out of git.
///
/// The workspace and the ledger are found and opened as for [`disown`], and
/// only by the changes in [`crate::lifecycle`].
pub(crate) fn carry(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    kind: Kind,
    name: &str,
    at: &Path,
    (to_name, to_at): (&str, &Path),
) -> Result<Carry, InstallError> {
    if !workspace.is_linked(project.root()) || !workspace.ledger_path().is_file() {
        return Ok(Carry::default());
    }
    let (ledger, scrub) = workspace.ledger_to_write_waiting(Duration::MAX)?;
    let entry = project_entry(workspace, project);
    let (from, to) = (route(project.root(), at), route(project.root(), to_at));
    let carried = ledger.renamed_copy(&entry, kind, name, &from, to_name, &to)?;
    let (action, detail) = match carried {
        Carried::Nothing => {
            return Ok(Carry {
                scrub,
                ..Carry::default()
            })
        }
        Carried::Moved => (
            Action::Install,
            format!(
                "{name}'s copy at {from} in {}, renamed {to_name} at {to}",
                project.name()
            ),
        ),
        Carried::Forgotten => (
            Action::Uninstall,
            format!(
                "{name}'s copy at {from} from {}, renamed onto {to_name}, which is installed already",
                project.name()
            ),
        ),
    };
    ledger.log(action, Some(&entry), Outcome::Ok, Some(&detail))?;
    // Told as any change of it is told, so what the front end says of it is
    // what an install and an uninstall will find.
    let edited = carried == Carried::Moved
        && !edited(workspace, &ledger, project, kind, to_name)?.is_empty();
    // After the record, as an install keeps its copies out of git before
    // they land and an uninstall takes them back in after they go: the copy
    // has moved already. Said rather than raised, since the record has too.
    let left = entries(project, &[at.to_path_buf()]);
    let unexcluded = local(project).and_then(|key| {
        if carried == Carried::Moved {
            if let Err(source) = exclude::add(
                project.root(),
                key,
                &entries(project, &[to_at.to_path_buf()]),
            ) {
                return Some(InstallError::NotExcluded {
                    path: to_at.to_path_buf(),
                    source,
                });
            }
        }
        exclude::remove(project.root(), key, &left)
            .err()
            .map(|source| InstallError::StillExcluded {
                path: at.to_path_buf(),
                source,
            })
    });
    Ok(Carry {
        carried,
        edited,
        scrub,
        unexcluded,
    })
}

/// What [`carry`] did.
#[derive(Debug)]
pub(crate) struct Carry {
    pub carried: Carried,
    /// Whether the copy carried, under its new name, holds something other
    /// than was installed, and so is left alone unless forced: a renamed
    /// skill always, a renamed rule only when it was edited before.
    pub edited: bool,
    /// What opening the ledger to write took out of it, for the front end to
    /// say; nothing when there was no ledger to open.
    pub scrub: Scrub,
    /// Why the exclude block of a local project does not say what is
    /// Mindweaver's now, when it could not be edited.
    pub unexcluded: Option<InstallError>,
}

impl Default for Carry {
    fn default() -> Self {
        Carry {
            carried: Carried::Nothing,
            edited: false,
            scrub: Scrub::default(),
            unexcluded: None,
        }
    }
}

/// Whether `workspace`'s ledger records a copy of `name` at `at` in
/// `project`: read, never written, for a change that leaves the record as it
/// is and says so. False for a workspace that does not manage the project,
/// and one with no ledger, which is not made to look.
pub(crate) fn recorded_at(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    kind: Kind,
    name: &str,
    at: &Path,
) -> Result<bool, InstallError> {
    if !workspace.is_linked(project.root()) || !workspace.ledger_path().is_file() {
        return Ok(false);
    }
    let ledger = workspace.ledger_for_reading()?;
    let reached = route(project.root(), at);
    Ok(ledger
        .installation(&project_entry(workspace, project), kind, name)?
        .is_some_and(|record| record.targets.iter().any(|target| target.path == reached)))
}

/// What [`disown`] did.
#[derive(Debug, Default)]
pub(crate) struct Disowned {
    /// Whether there was a record of the copy, which is now gone.
    pub dropped: bool,
    /// What opening the ledger to write took out of it, for the front end to
    /// say; nothing when there was no ledger to open.
    pub scrub: Scrub,
    /// Why the exclude block of a local project still keeps the copy's place
    /// out of git, when it could not be edited.
    pub still_excluded: Option<InstallError>,
}

/// What installing one artifact did.
///
/// Each that leaves copies names every one of them (`paths`), one for each
/// target, in the order [`targets`] gives them, and says whether they could
/// be hashed (`unhashed`): copies that could not are recorded all the same,
/// and said rather than recorded quietly, since a record with no hash is
/// read the way one from before hashes were kept is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installed {
    Added {
        name: ArtifactName,
        paths: Vec<PathBuf>,
        unhashed: Option<String>,
    },
    Updated {
        name: ArtifactName,
        paths: Vec<PathBuf>,
        unhashed: Option<String>,
    },
    Unchanged {
        name: ArtifactName,
        paths: Vec<PathBuf>,
        unhashed: Option<String>,
    },
    /// Something of that name is already in one of the targets, at `path`,
    /// and is not ours to replace, so nothing was written in any of them.
    Foreign { name: ArtifactName, path: PathBuf },
    /// A copy Mindweaver installed was changed since, at each of `paths`,
    /// and the install was not forced, so nothing was written in any target.
    Edited {
        name: ArtifactName,
        paths: Vec<PathBuf>,
    },
    /// An MCP server that does not come from the workspace's own library,
    /// asked for with no yes ([`asks_first`]): nothing written, and what it
    /// would write into each of `paths` said beside it
    /// ([`Installation::entries`], [`mcp::preview`]). `recorded` when the
    /// ledger records it in the project, so that writing it would update
    /// what an earlier install wrote rather than add it.
    Unconfirmed {
        name: ArtifactName,
        paths: Vec<PathBuf>,
        recorded: bool,
    },
}

impl Installed {
    /// Why the copy could not be hashed, when it could not, so the ledger
    /// records it with no hash.
    pub fn unhashed(&self) -> Option<&str> {
        match self {
            Installed::Added { unhashed, .. }
            | Installed::Updated { unhashed, .. }
            | Installed::Unchanged { unhashed, .. } => unhashed.as_deref(),
            Installed::Foreign { .. }
            | Installed::Edited { .. }
            | Installed::Unconfirmed { .. } => None,
        }
    }

    /// Why nothing was written, when the install was refused rather than
    /// done: what a front end says it with, by its code.
    pub fn refused(&self) -> Option<Refused> {
        match self {
            Installed::Added { .. } | Installed::Updated { .. } | Installed::Unchanged { .. } => {
                None
            }
            Installed::Foreign { .. } => Some(Refused::Foreign),
            Installed::Edited { .. } => Some(Refused::Edited),
            // Not refused but asked about: nothing is wrong with it.
            Installed::Unconfirmed { .. } => None,
        }
    }
}

/// What [`install`] did: to the artifact, and to the project's scratch folder
/// on the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub installed: Installed,
    /// What installs that were interrupted had left, cleared before this one
    /// built its copy: in the project's scratch folder, and beside the copy,
    /// where earlier versions staged. Said rather than cleared quietly,
    /// because each is an install that did not finish.
    pub swept: Vec<PathBuf>,
    /// What the offer holds that no copy does (`LEAVE_OUT`): said, because
    /// the copies are then not all that was offered.
    pub left_out: Vec<LeftOut>,
    /// What keeping the copies out of git came to, for a local project; none
    /// for a project whose copies are committed. [`Excluded::warning`] says
    /// when nothing keeps them out, outside a git work tree.
    pub excluded: Option<Excluded>,
    /// What rendering the project's rules did, for a rule installed into a
    /// project marked by its settings ([`rules::Rendered`]): every rule it
    /// has is rendered with the one installed, and what there is to say of
    /// any of them is said here. None for a skill, and for a rule copied
    /// into a project a `mind.toml` marks.
    pub rendered: Option<rules::Rendered>,
    /// What installing an MCP server wrote into its agents' config files,
    /// an entry an agent, and which of the agents the project names it
    /// does not reach, and why ([`mcp::Rendered`]). None for a skill and a
    /// rule.
    pub entries: Option<mcp::Rendered>,
}

impl From<Installed> for Installation {
    /// An installation that never reached the scratch folder, and so cleared
    /// nothing in it and copied nothing.
    fn from(installed: Installed) -> Self {
        Installation {
            installed,
            swept: Vec::new(),
            left_out: Vec::new(),
            excluded: None,
            rendered: None,
            entries: None,
        }
    }
}

/// What uninstalling one artifact did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Removed {
    /// Every copy its installation recorded is gone, and so is the record.
    Removed {
        name: ArtifactName,
        paths: Vec<PathBuf>,
        /// Why the exclude block of a local project still names them, when it
        /// could not be edited: a line left there keeps whatever somebody
        /// writes in that place later out of git, which is worth saying.
        still_excluded: Option<String>,
        /// What rendering the project's rules without it did, for a rule
        /// taken out of a project marked by its settings, as
        /// [`Installation::rendered`] says for one installed. Boxed, being
        /// far larger than what the other two say.
        rendered: Option<Box<rules::Rendered>>,
    },
    /// Present, at `path`, but nobody here put it there.
    Foreign { name: ArtifactName, path: PathBuf },
    /// Installed, but a copy was changed since, at each of `paths`, and the
    /// uninstall was not forced, so every copy is where it was, and so is
    /// the record.
    Edited {
        name: ArtifactName,
        paths: Vec<PathBuf>,
    },
    /// Nothing of that name to remove.
    Absent { name: ArtifactName },
}

impl Removed {
    /// Why nothing was taken out, when the uninstall was refused rather
    /// than done: what a front end says it with, by its code.
    pub fn refused(&self) -> Option<Refused> {
        match self {
            Removed::Removed { .. } => None,
            Removed::Foreign { .. } => Some(Refused::Foreign),
            Removed::Edited { .. } => Some(Refused::Edited),
            Removed::Absent { .. } => Some(Refused::NotInstalled),
        }
    }
}

/// Why an install or an uninstall did not do what it was asked, though
/// nothing went wrong: what [`Installed::refused`] and [`Removed::refused`]
/// say, with a code from the list every error's comes from (`code`).
///
/// An outcome rather than an error, as what it comes from is: the refusal
/// is the answer, and a front end says it and goes on with the rest. Still
/// a code, since the panel tells somebody's copy from one changed since the
/// way it tells one error from another. One type for both changes, since a
/// copy that is somebody's refuses an install and an uninstall alike, and a
/// code names one condition wherever it is met: a code on each outcome
/// would give `foreign` twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Something of that name is in a target, and the ledger records none
    /// of it.
    Foreign,
    /// A copy Mindweaver installed was changed since, and the change was not
    /// forced.
    Edited,
    /// Nothing of that name is installed to take out: an uninstall's only.
    NotInstalled,
}

codes!(Refused {
    Self::Foreign => "foreign",
    Self::Edited => "edited",
    Self::NotInstalled => "not_installed",
});

/// How `name` stands in `project`, which installs into `targets`, given
/// every copy the ledger records Mindweaver writing there (`recorded`, as
/// [`Ledger::copies`] spells them), and the record of its installation with
/// every offer of `kind`, when there is one.
///
/// Foreign when any target holds something of that name the ledger does not
/// record, whatever the others hold: installing would write beside it, and
/// uninstalling would take away the copies the agents reading both folders
/// share with it. Otherwise, while the ledger records it, as each copy its
/// record names is found against what was installed there and what the
/// place it came from offers now ([`Standing`]), a copy gone since included,
/// which the next install puts back.
fn standing(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    targets: &[InstallTarget],
    kind: Kind,
    name: &str,
    recorded: &[String],
    record: Option<(&InstallRecord, &[Offer])>,
) -> Standing {
    let copies: Vec<PathBuf> = targets
        .iter()
        .map(|target| copy_at(&target.folder, kind, name))
        .collect();
    if unrecorded(project.root(), kind, &copies, recorded).is_some() {
        return Standing::Foreign;
    }
    let Some((record, offers)) = record else {
        return Standing::Absent;
    };
    let offered = offered_for(record, offers);
    // Matched on the kind, so a third one has to say how its copies are told.
    match kind {
        Kind::Skill => standing::of_skill(workspace, project, record, &offered),
        // Only into a project a `mind.toml` marks, as a copy of its file: one
        // marked by its settings has its rules told by `rules::survey`.
        Kind::Rule => standing::of_rule(workspace, project, record, None, &offered),
        Kind::Mcp => entries::told(workspace, project, record, &offered).1,
    }
}

/// The first of `copies` that is there and that `recorded` does not name,
/// if one is: somebody's work.
///
/// By the copy's path, as a listing marks installed copies
/// (`Catalog::with_records`), not by its name: an installation of a name
/// into one folder does not make what somebody keeps under that name in
/// another folder Mindweaver's to write over.
fn unrecorded(root: &Path, kind: Kind, copies: &[PathBuf], recorded: &[String]) -> Option<PathBuf> {
    copies
        .iter()
        .find(|copy| holds(copy, kind) && !recorded.contains(&route(root, copy)))
        .cloned()
}

/// Where a copy of `name` goes in `folder`: a folder of that name for a kind
/// that is one; for a kind that is files, the file its route names, folders
/// and all, as discovery names one by its route ([`crate::catalog`]).
///
/// Joined a word of the route at a time, so it is the same path on every
/// platform.
pub(crate) fn copy_at(folder: &Path, kind: Kind, name: &str) -> PathBuf {
    match kind.layout() {
        Layout::Directory { .. } => folder.join(name),
        Layout::Flat { extension } => folder.join(format!("{name}.{extension}")),
        Layout::Files { extension } => {
            let mut path = folder.to_path_buf();
            path.extend(name.split('/'));
            let last = name.rsplit('/').next().unwrap_or(name);
            path.set_file_name(format!("{last}.{extension}"));
            path
        }
    }
}

/// Whether something is at `copy`, where an install would write.
pub(crate) fn holds(copy: &Path, kind: Kind) -> bool {
    match kind.layout() {
        // Anything by that name, not only a finished artifact: a folder of
        // notes somebody started by hand is theirs, and installing over it
        // would delete them.
        Layout::Directory { .. } => copy.exists(),
        Layout::Files { .. } | Layout::Flat { .. } => copy.is_file(),
    }
}

/// How the ledger names a project: relative to the workspace, the way a
/// registered project is stored, so moving the two together keeps it true.
///
/// The route to where the workspace's own entry leads
/// ([`WeaverWorkspace::spelling_of`]), not to however the caller spelled the
/// project: `mind` run inside it and `mindweaver -p` must read and write one
/// record, whichever symlinks either went through.
fn project_entry(workspace: &WeaverWorkspace, project: &MindProject) -> String {
    entry_for_root(workspace, project.root())
}

/// [`project_entry`], for a project known only by its root, as an artifact
/// knows the one it was found in.
pub(crate) fn entry_for_root(workspace: &WeaverWorkspace, root: &Path) -> String {
    route(workspace.root(), &workspace.spelling_of(root))
}

/// A path below another, `/` separated, or the path itself when there is no
/// route between them.
pub(crate) fn route(base: &Path, target: &Path) -> String {
    paths::relative_to(target, base)
        .as_deref()
        .and_then(paths::to_config_string)
        .unwrap_or_else(|| target.display().to_string())
}

/// Whether two paths are one directory on disk.
fn same_place(a: &Path, b: &Path) -> bool {
    matches!((fs::canonicalize(a), fs::canonicalize(b)), (Ok(a), Ok(b)) if a == b)
}

/// Whether one of two paths is inside the other. `to` need not exist yet, so
/// it is resolved through the nearest ancestor that does.
fn overlaps(from: &Path, to: &Path) -> bool {
    match (fs::canonicalize(from), resolved(to)) {
        (Ok(from), Some(to)) => from.starts_with(&to) || to.starts_with(&from),
        _ => false,
    }
}

/// A path made canonical as far as it exists, with the rest appended.
pub(crate) fn resolved(path: &Path) -> Option<PathBuf> {
    let mut missing = Vec::new();
    let mut existing = path;
    loop {
        if let Ok(real) = fs::canonicalize(existing) {
            return Some(
                missing
                    .iter()
                    .rev()
                    .fold(real, |path, part| path.join(part)),
            );
        }
        missing.push(existing.file_name()?.to_owned());
        existing = existing.parent()?;
    }
}

/// The loaded workspace that keeps its own of `kind` in a folder `path` is
/// in, or around, if one does (spelled as its root, which `mindweaver unload`
/// accepts from anywhere).
///
/// By the folder, not by what is in it yet: a project that is also a loaded
/// workspace keeping both in one folder has nothing in it Mindweaver may
/// write or remove, from the first install on; otherwise an install today
/// is the loaded workspace's own tomorrow, and can never be updated again.
pub(crate) fn kept_by_a_load(
    workspace: &WeaverWorkspace,
    kind: Kind,
    path: &Path,
) -> Option<PathBuf> {
    let at = resolved(path)?;
    workspace.loads().into_iter().find_map(|loaded| {
        let source = loaded.workspace.ok()?;
        let kept = fs::canonicalize(source.own().directory_for(kind)?).ok()?;
        (at.starts_with(&kept) || kept.starts_with(&at)).then_some(loaded.root)
    })
}

/// Whether `candidate` is the one `project` holds of its name. For telling
/// which of two offers of one name is the one installed.
///
/// A skill is the copy there when it has the same names and bytes as the
/// folder, but for what an install leaves out (`LEAVE_OUT`): a record from
/// before schema 3 does not say where what came from a workspace rather than
/// the shelf came from, and an edit since may have made it neither. It is
/// compared with the first copy the project holds of it: an install writes
/// every copy from one tree, so any of them is what was installed, and one
/// edited since is no longer any offer's.
///
/// A rule is the one the ledger records it was installed from. Every record
/// of one says where it came from, and nothing in a project marked by its
/// settings holds a rule's bytes alone to compare: its section is rendered,
/// and Claude may have no file of it.
pub fn is_current(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    candidate: &Candidate,
) -> bool {
    // Matched on the kind, so a third one has to say how it is told.
    match candidate.kind() {
        Kind::Skill => {}
        // Each of a server's records says where it came from, as a rule's
        // does, and its entries hold no bytes of the definition to compare.
        Kind::Rule | Kind::Mcp => {
            let entry = project_entry(workspace, project);
            return ledger
                .installation(&entry, candidate.kind(), candidate.name())
                .ok()
                .flatten()
                .and_then(|record| record.origin)
                .is_some_and(|origin| origin == candidate.origin());
        }
    }
    let Ok(targets) = targets(project, candidate.kind()) else {
        return false;
    };
    targets
        .iter()
        .map(|target| target.folder.join(candidate.name()))
        .find(|at| at.is_dir())
        .is_some_and(|at| {
            copy::same_leaving(&candidate.source(workspace), &at, &left_out_names())
                .unwrap_or(false)
        })
}

/// Why an install or an uninstall could not happen at all.
#[derive(Debug, Error)]
pub enum InstallError {
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Workspace(#[from] WorkspaceError),
    #[error("{name}: the ledger has it at {path}, but nothing is there; gather {workspace} again")]
    Missing {
        name: String,
        path: PathBuf,
        workspace: PathBuf,
    },
    #[error("{name}: {path} is not there any more, because the workspace loaded from {workspace} has changed; `mindweaver unload {workspace}` if it has gone")]
    Gone {
        name: String,
        path: PathBuf,
        workspace: PathBuf,
    },
    #[error("`{name}` is not one folder name, so it cannot be installed or removed")]
    UnsafeName { name: String },
    #[error("`{route}` is no name a rule can be installed under ({problem}), so it cannot be installed or removed")]
    UnsafeRoute { route: String, problem: String },
    #[error("{path} leads to {target}, outside {within}, where it is offered from, so it is not installed: what it leads to is not that workspace's to give")]
    Outside {
        path: PathBuf,
        target: PathBuf,
        within: PathBuf,
    },
    #[error("rule `{name}` was installed from {}, which no longer offers it, so the project's rules are not rendered: put it back there, or uninstall it", .origin.as_deref().unwrap_or("a place not recorded"))]
    NoLongerOffered {
        name: String,
        origin: Option<Origin>,
    },
    #[error("{file}: its block holds rule `{route}`, which this workspace did not install there, so the project's rules are not rendered: uninstall it from the workspace that did, or take its section out by hand")]
    NotRecorded { file: PathBuf, route: String },
    #[error("{file}: the section of rule `{route}` was changed since it was rendered and cannot be read back, so the project's rules are not rendered: put it right by hand, or install the rule again with --force")]
    Unread { file: PathBuf, route: String },
    #[error("{file}: holds rule `{route}` in Mindweaver's block, which cannot be edited ({why}), so the rule is not uninstalled: share the project or set edit_tracked, or take its section out by hand")]
    Withheld {
        file: PathBuf,
        route: String,
        why: rules::Withheld,
    },
    #[error("{file}: holds MCP server `{name}`, which cannot be taken out of it ({why}), so the server is not uninstalled: share the project or set edit_tracked, or take its entry out by hand")]
    EntryWithheld {
        file: PathBuf,
        name: String,
        why: rules::Withheld,
    },
    #[error(transparent)]
    AgentsMd(#[from] AgentsMdError),
    #[error(transparent)]
    ClaudeMd(#[from] ClaudeMdError),
    #[error(transparent)]
    ClaudeRules(#[from] ClaudeRulesError),
    #[error("{} cannot be installed into a project yet", .kind.plural())]
    NotInstallable { kind: Kind },
    /// Where a copy of `kind` would go, asked of one installed as an entry
    /// in each agent's config file instead ([`targets`]).
    #[error("{} are installed as an entry in each agent's config file, never copied into a folder", .kind.plural())]
    NotCopied { kind: Kind },
    /// An agent's config file an MCP server's entry is written into, or
    /// taken out of, that its renderer refuses or cannot write: what the
    /// renderer says, by its code. Boxed, being far larger than the rest.
    #[error(transparent)]
    Config(Box<ConfigUnread>),
    /// An MCP server that reaches none of the agents the project names,
    /// each of them refused it or left out for why it says ([`mcp`]).
    #[error("MCP server `{name}` reaches none of the agents {project} names, so nothing was installed: {}", unreached(.why))]
    Unreached {
        name: String,
        project: String,
        why: Vec<mcp::Unreached>,
    },
    #[error("`{name}` cannot be installed: {}", issue_list(.issues))]
    Unfit {
        name: String,
        issues: Vec<ValidationIssue>,
    },
    #[error(transparent)]
    Artifact(#[from] ArtifactError),
    #[error("{from} and {to} are one inside the other, so installing one onto the other would delete it")]
    Overlaps { from: PathBuf, to: PathBuf },
    #[error("{path} is where the workspace loaded from {loaded} keeps its own, so Mindweaver neither installs nor removes there; `mindweaver unload {loaded}` first to manage it as a project")]
    KeptByALoad { path: PathBuf, loaded: PathBuf },
    #[error("{path}: cannot be written: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
        /// What interrupted installs had left that this one cleared before
        /// the write failed ([`InstallError::swept`]).
        swept: Vec<PathBuf>,
    },
    #[error("nothing called `{name}` can be installed{}: gather it, load a workspace that has it with `mindweaver load`, or add it to the workspace with `mindweaver add`", from.as_deref().map(|from| format!(" from {from}")).unwrap_or_default())]
    NotOffered { name: String, from: Option<String> },
    #[error("`{name}` is offered by more than one place ({}), so say which with --from", .origins.join(", "))]
    OfferedTwice { name: String, origins: Vec<Origin> },
    #[error("`{name}` is offered as {}, so say which: {}", as_kinds(.kinds), qualified(.name, .kinds))]
    OfferedAsKinds { name: String, kinds: Vec<Kind> },
    #[error("`{name}` is installed as {}, so say which: {}", as_kinds(.kinds), qualified(.name, .kinds))]
    InstalledAsKinds { name: String, kinds: Vec<Kind> },
    #[error("{project} names no agent in its settings that {kinds} are installed for, so none can be installed into it", kinds = .kind.plural())]
    NoTarget { project: String, kind: Kind },
    #[error("none of the folders {project}'s settings name for its {kinds} is one an install may use, so none can be installed into it", kinds = .kind.plural())]
    NoUsableTarget { project: String, kind: Kind },
    #[error("{name}: the ledger records a copy at {route}, which is not a folder of the project named after it, so nothing was removed")]
    Recorded { name: String, route: String },
    #[error("{0}, and nothing was installed, since its copies could not be kept out of git")]
    Exclude(#[from] ExcludeError),
    #[error("{path}: its record of being installed was dropped, but the exclude file still keeps it out of git: {source}")]
    StillExcluded {
        path: PathBuf,
        #[source]
        source: ExcludeError,
    },
    #[error("{path}: renamed, and still recorded as installed, but the exclude file cannot keep it out of git: {source}")]
    NotExcluded {
        path: PathBuf,
        #[source]
        source: ExcludeError,
    },
    #[error("{path} is inside {home}, which {reader} reads for every project, so nothing was installed: Mindweaver installs into one project at a time")]
    UserScope {
        path: PathBuf,
        home: PathBuf,
        reader: &'static str,
    },
}

codes!(InstallError {
    Self::Missing { .. } => "shelf_copy_missing",
    Self::Gone { .. } => "loaded_copy_gone",
    Self::UnsafeName { .. } => "name_unsafe",
    Self::UnsafeRoute { .. } => "route_unsafe",
    Self::Outside { .. } => "offer_leads_outside",
    Self::NoLongerOffered { .. } => "rule_no_longer_offered",
    Self::NotRecorded { .. } => "rule_not_recorded",
    Self::Unread { .. } => "rule_section_unread",
    Self::Withheld { .. } => "rules_block_unedited",
    Self::EntryWithheld { .. } => "mcp_entry_unedited",
    Self::NotInstallable { .. } => "not_installable",
    Self::NotCopied { .. } => "not_copied",
    Self::Unreached { .. } => "mcp_unreached",
    Self::Unfit { .. } => "install_unfit",
    Self::Overlaps { .. } => "install_overlaps",
    Self::KeptByALoad { .. } => "kept_by_a_load",
    Self::Write { .. } => "install_write_failed",
    Self::NotOffered { .. } => "not_offered",
    Self::OfferedTwice { .. } => "offered_twice",
    Self::OfferedAsKinds { .. } => "offered_as_kinds",
    Self::InstalledAsKinds { .. } => "installed_as_kinds",
    Self::NoTarget { .. } => "no_target",
    Self::NoUsableTarget { .. } => "no_usable_target",
    Self::Recorded { .. } => "recorded_outside",
    Self::StillExcluded { .. } => "still_excluded",
    Self::NotExcluded { .. } => "not_excluded",
    Self::UserScope { .. } => "install_user_scope",
} carried {
    Self::Ledger(error) => error,
    Self::Workspace(error) => error,
    Self::AgentsMd(error) => error,
    Self::ClaudeMd(error) => error,
    Self::ClaudeRules(error) => error,
    Self::Artifact(error) => error,
    Self::Exclude(error) => error,
    Self::Config(error) => error,
});

impl InstallError {
    /// What an install that failed had cleared first, as
    /// [`Installation::swept`] says for one that did not: for a front end to
    /// say with the error, because each is an install that did not finish,
    /// whether or not this one did.
    ///
    /// Only a copy that fails carries any. A ledger that fails after the copy
    /// stops everything, as it does wherever it fails, and what was swept
    /// goes unsaid with the rest.
    pub fn swept(&self) -> &[PathBuf] {
        match self {
            InstallError::Write { swept, .. } => swept,
            _ => &[],
        }
    }
}

/// "claude: why; codex: why": each agent an MCP server does not reach, as
/// [`InstallError::Unreached`] says them.
fn unreached(why: &[mcp::Unreached]) -> String {
    let each: Vec<String> = why
        .iter()
        .map(|unreached| format!("{}: {}", unreached.agent.name(), unreached.why.message))
        .collect();
    each.join("; ")
}

/// "a skill and as a rule": the kinds a name is, as a refusal says them.
fn as_kinds(kinds: &[Kind]) -> String {
    let each: Vec<String> = kinds.iter().map(|kind| format!("a {kind}")).collect();
    match each.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} and as {last}", rest.join(", as ")),
        None => String::new(),
    }
}

/// "`skill:deploy` or `rule:deploy`": what to type for each, with the
/// qualifier a [`Reference`] reads.
fn qualified(name: &str, kinds: &[Kind]) -> String {
    let each: Vec<String> = kinds
        .iter()
        .map(|kind| format!("`{}{QUALIFIER}{name}`", kind.slug()))
        .collect();
    match each.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => String::new(),
    }
}
