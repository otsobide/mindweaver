//! Everything a workspace holds, read in one go for a front end to draw: the
//! snapshot every front end reads, the TUI first and the web panel next.
//!
//! One read rather than each front end putting its view together from the
//! reports, as the TUI did: which failure belongs to which artifact, which
//! install nothing of is found, how many of its own a loaded workspace
//! offers, are answers, and two front ends working them out would come to
//! two of them. Core builds and orders it, from the same types the CLI
//! prints, and a front end only chooses how to show it. A [`Listing`] is a
//! part of it, its holders read for the kinds asked: what `list --json`
//! prints, so a script reads a project as a front end draws it.
//!
//! - **A read, and nothing more.** No file is made or changed: the ledger is
//!   opened only through [`WeaverWorkspace::ledger_for_reading`], the
//!   default workspace is never made for it, and no scrub runs. A ledger
//!   from an older schema is migrated, as every open migrates it, which is
//!   upkeep of Mindweaver's own file rather than a change.
//! - **No scan.** What could be linked comes from [`crate::scan`], a query
//!   of its own run when somebody asks, since a walk of the disk is no price
//!   to pay for every live update of a screen.
//! - **Its names are a contract**, as a report's are: every field by its
//!   name, a kind, an agent and a state by the word a command line takes,
//!   and every URL through [`redacted`](crate::url::redacted), where it is
//!   written. [`FORMAT`] moves when a field goes or changes what it holds.
//! - **Failures are collected**, beside what they are about: a project's in
//!   the project, the library's in the library, and what belongs to neither
//!   in [`Snapshot::warnings`]. A ledger that cannot be read is about every
//!   project's records and the shelf alike, so each of them says it.

use std::error::Error as _;
use std::path::{Path, PathBuf};
use std::slice;

use serde::{Serialize, Serializer};

use crate::agent::{Agent, Starts};
use crate::artifact::{Artifact, ValidationIssue};
use crate::catalog::{Catalog, DiscoveryFailure};
use crate::code::Said;
use crate::doctor::{Examined, Machine};
use crate::install::{self, LoadFailure, Offer, Place};
use crate::kind::Kind;
use crate::known::{self, KnownWorkspace};
use crate::ledger::{Entry, Gathered, InstallRecord, Ledger, LedgerError, Source};
use crate::lifecycle;
use crate::mcp::{self, McpDefinition, Running};
use crate::paths;
use crate::relink;
use crate::resolve::{self, Inputs, Intent, Reason, ResolveError, Resolved};
use crate::settings::{ProjectId, Settings, Share};
use crate::status::{ArtifactStatus, EntryStatus, Missed, ProjectStatus, State, Unreached};
use crate::template::{self, Template};
use crate::value::{ArtifactName, Origin, Redacted};
use crate::workspace::{
    same_directory, MarkerFile, Member, MindProject, WeaverWorkspace, WorkspaceError,
};
use crate::{claude_mcp, codex_mcp, opencode_mcp};

/// The version of what a snapshot holds, which it carries as `format`.
///
/// Its own rather than the CLI's envelope version: the snapshot is no
/// command's report, and a front end reading it holds it to this number.
/// It moves, as the envelope's does, when a field goes or changes what it
/// holds, and not when one is added. A [`Listing`] is printed in the CLI's
/// envelope, so a change that moves this moves that version too when it
/// is to a holder or what it holds.
pub const FORMAT: u32 = 1;

/// How many of the latest actions a snapshot carries, newest first: enough
/// to say what happened lately, and a bound on what every update carries.
pub const HISTORY: usize = 20;

/// Everything a workspace holds, as every front end shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Snapshot {
    /// [`FORMAT`].
    pub format: u32,
    /// The workspace it is of, and why that one.
    pub workspace: Workspace,
    /// Every kind this build knows, in report order: what a front end lays
    /// its columns out by, so a kind added to core reaches it without a
    /// change of its own.
    pub kinds: Vec<Kind>,
    /// Every agent this build knows, in report order, for the same reason.
    pub agents: Vec<Agent>,
    /// Every state an artifact can stand in, in report order.
    pub states: Vec<State>,
    /// Every state a box of an offer ([`Snapshot::boxes`]) can stand in, in
    /// report order ([`State::of_boxes`]): `states` but `authored`, which no
    /// offer is. What a legend of the boxes lists, so a front end leaves
    /// out no state by its name.
    pub box_states: Vec<State>,
    /// Every health a project can be in, in report order: what a front end
    /// sorts and filters its projects by, so it orders none of its own.
    pub healths: Vec<Health>,
    /// What it holds, counted: what an overview shows of it.
    pub summary: Summary,
    /// The workspace's own artifacts.
    pub library: Library,
    /// Every entry in its registry, in the order its marker lists them, a
    /// second spelling of one directory included: each is what `unlink` is
    /// given.
    pub projects: Vec<Project>,
    /// Every workspace it loads, in the order its marker lists them.
    pub loaded: Vec<LoadedWorkspace>,
    /// Everything gathered onto its shelf, by kind, then name, then source.
    pub shelf: Vec<Shelved>,
    /// Every source the shelf is filled from, by URL.
    pub sources: Vec<ShelfSource>,
    /// Everything there is to install, from wherever it is offered: what
    /// a front end that assigns lays its offers out by.
    pub offers: Vec<Offered>,
    /// How each entry in the registry stands with every offer, in the
    /// order `projects` lists them: what each of its boxes shows.
    pub boxes: Vec<Boxes>,
    /// What doctor finds of each entry in the registry, in the order
    /// `projects` lists them: what keeps its agents from what it gives
    /// them, as `mindweaver doctor` says it ([`crate::doctor`]), with the
    /// agent homes and the names of the variables set the front end told
    /// the snapshot ([`Machine`]).
    pub doctor: Vec<Examined>,
    /// Every workspace this machine knows ([`crate::known`]), this one
    /// among them, each with whether this one loads it, and could.
    pub known: Vec<KnownHere>,
    /// The latest actions, newest first, at most [`HISTORY`].
    pub history: Vec<Entry>,
    /// What could not be read and belongs to no project or library: two
    /// projects holding one id, what a loaded workspace cannot offer, a
    /// ledger whose shelf or history would not read.
    pub warnings: Vec<Said>,
}

/// What a workspace holds, counted: the projects it links, those that open
/// none, and the artifacts its projects, its library and its shelf hold.
///
/// Counted here rather than by a front end from the lists beside it, for
/// the reason a [`Tally`] is, and because a count of projects is no list's
/// length: two entries can spell one directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Summary {
    /// How many directories its registry names that open a project: each
    /// once, however many entries spell it, as [`KnownWorkspace::projects`]
    /// counts them.
    pub projects: usize,
    /// How many it names that open none, each once.
    pub stale: usize,
    /// What those projects hold together, each project once.
    pub held: Tally,
    /// What its library holds: its own, which stand in no state.
    pub library: Tally,
    /// What its shelf holds, which stands in no state either.
    pub shelf: Tally,
}

impl Summary {
    /// `projects`, every entry of a registry, each directory counted once,
    /// with `library` and `shelf`.
    fn of(projects: &[Project], library: &Library, shelf: &[Shelved]) -> Summary {
        let mut once: Vec<&Project> = Vec::new();
        for project in projects {
            if !once
                .iter()
                .any(|counted| same_directory(&counted.path, &project.path))
            {
                once.push(project);
            }
        }
        let (stale, opened): (Vec<&Project>, Vec<&Project>) = once
            .into_iter()
            .partition(|project| project.stale.is_some());
        Summary {
            projects: opened.len(),
            stale: stale.len(),
            held: Tally::of(
                opened.iter().flat_map(|project| {
                    project.artifacts.iter().map(|held| (held.kind, held.state))
                }),
                &Kind::ALL,
            ),
            library: Tally::of_held(&library.artifacts, &Kind::ALL),
            shelf: Tally::of(
                shelf.iter().map(|shelved| (shelved.gathered.kind, None)),
                &Kind::ALL,
            ),
        }
    }
}

/// The workspace a snapshot is of.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Workspace {
    pub name: String,
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// The rule that chose it ([`resolve`](crate::resolve)).
    pub reason: Reason,
    /// Whether it is on disk: only the default one is ever read otherwise,
    /// as it will be once the first command that writes makes it.
    pub made: bool,
}

/// The workspace's own artifacts, held as a project holds its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Library {
    /// What `--from` takes for each of its own, and what an install of one
    /// records as its origin: so a front end names it in a command line by
    /// core's word rather than by one of its own that core could spell
    /// differently.
    pub origin: &'static str,
    /// Where it keeps each kind, as a route from the workspace's root.
    pub directories: Vec<Directory>,
    /// Each of its own, by kind and then name: no project's, so none
    /// stands in a state, and nothing is installed into them.
    pub artifacts: Vec<Held>,
    /// Each project its registry links that holds a copy of one of its own,
    /// in the order of `artifacts`, then of the registry.
    pub holders: Vec<LibraryHolder>,
    /// What `mindweaver add --template` can start from.
    pub templates: Vec<Template>,
    /// What could not be read in it and is no artifact's: a folder that
    /// cannot be listed, its templates.
    pub failures: Vec<Said>,
    /// Each of its own MCP servers whose definition reads, in the order
    /// of `artifacts`, as each agent would be given it: what a page of
    /// servers shows beside each one's row of `artifacts`.
    pub servers: Vec<LibraryServer>,
}

/// One of the workspace's own MCP servers, as `add mcp` would be told to
/// write it and as each agent would be given it.
///
/// Worked out here, from each agent's renderer, rather than by a front end
/// from the definition: what an agent runs is what its own entry spells
/// ([`mcp::Runs`]), an input reaching Claude as `${VAR}` and OpenCode as
/// `{env:VAR}`, and what it cannot be given is core's report of it
/// ([`McpDefinition::expressibility`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LibraryServer {
    pub name: ArtifactName,
    /// How it is started or reached, as `add mcp`'s options would say it
    /// ([`mcp::Given::of`]), for a form changing it to start from: none
    /// when the definition holds what no option says, which is changed as
    /// text.
    pub given: Option<mcp::Given>,
    /// How each agent would be given it, in report order.
    pub agents: Vec<GivenTo>,
}

/// How one agent would be given an MCP server: what it cannot be given of
/// it, what its entry would run or reach, and what it asks before it
/// starts one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct GivenTo {
    pub agent: Agent,
    /// Why it would be given none of it, each by its code and in its
    /// words: what it cannot be given at all, as `show` says it, or else
    /// why its renderer writes it no entry, an input read from no
    /// variable. Empty, and it is given an entry.
    pub refused: Vec<Said>,
    /// What its entry would be written without, as `show` says it.
    pub dropped: Vec<Said>,
    /// What its entry would run or reach, as the entry spells it; none
    /// when it is refused.
    pub runs: Option<Running>,
    /// What it asks before it starts a server a project gives it.
    pub starts: Starts,
}

impl LibraryServer {
    /// `definition`, as each agent would be given it.
    fn of(definition: &McpDefinition) -> LibraryServer {
        let agents = definition
            .expressibility()
            .into_iter()
            .map(|reported| {
                // Matched on the agent with no wildcard, so an agent added
                // says how its entry is made before it compiles. OpenCode's
                // in the shape Mindweaver writes (D-12), which runs what the
                // other does.
                let entry = match reported.agent {
                    Agent::Claude => claude_mcp::entry(definition)
                        .map(|entry| entry.runs())
                        .map_err(|error| Said::of(error.code(), &error)),
                    Agent::Codex => codex_mcp::entry(definition)
                        .map(|entry| entry.runs())
                        .map_err(|error| Said::of(error.code(), &error)),
                    Agent::OpenCode => opencode_mcp::entry(definition, opencode_mcp::Shape::V2)
                        .map(|entry| entry.runs())
                        .map_err(|error| Said::of(error.code(), &error)),
                };
                let mut refused = reported.refused;
                let runs = match entry {
                    Ok(runs) if refused.is_empty() => Some(runs.shown()),
                    Ok(_) => None,
                    Err(why) => {
                        if refused.is_empty() {
                            refused.push(why);
                        }
                        None
                    }
                };
                GivenTo {
                    agent: reported.agent,
                    refused,
                    dropped: reported.dropped,
                    runs,
                    starts: reported.agent.starts_a_server(),
                }
            })
            .collect();
        LibraryServer {
            name: definition.name.clone(),
            given: mcp::Given::of(definition),
            agents,
        }
    }
}

/// A project holding a copy of one of the workspace's own: one whose install
/// the ledger tells came from it ([`install::from_the_library`]), the rule
/// `mindweaver remove --everywhere` takes the copies it removes by.
///
/// Told here rather than by a front end comparing a copy's
/// [`origin`](Held::origin) with [`Library::origin`], which would be a
/// rule of its own, and a different one: a record from before origins were
/// kept names none, and is a copy of one of the workspace's own all the
/// same while no workspace it loads offers that name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LibraryHolder {
    /// The one of the workspace's own it holds a copy of.
    pub kind: Kind,
    pub name: ArtifactName,
    /// The project, as its registry entry spells it ([`Project::entry`]):
    /// its artifact of that kind and name is the copy. Each entry that
    /// spells one directory is a holder of its own, as each is a project
    /// of the snapshot's.
    pub entry: String,
}

/// One entry in a workspace's registry, and the project it opens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Project {
    /// The entry as the marker spells it, which `-p` and `unlink` take; none
    /// for a project no workspace links.
    pub entry: Option<String>,
    /// Where it points.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// What its marker calls it; none for an entry that opens no project.
    pub name: Option<String>,
    /// The file its marker is read from, which says an older one `mind
    /// migrate` moves.
    pub marker: Option<MarkerFile>,
    /// Whether `mind migrate` has it to move: an older marker marks it, a
    /// `mind.toml` or a `settings.json`, whose settings nothing but that
    /// move changes ([`MindProject::changeable`]); false for an entry that
    /// opens no project. Told here rather than by a front end comparing
    /// `marker` with the file it knows to be the newest, which would be the
    /// rule written again, and wrong once a newer one is.
    pub migrates: bool,
    /// The id its settings name it by (D-09), which a move is followed by.
    pub id: Option<ProjectId>,
    /// The agents its settings name, in report order: a slot each for every
    /// artifact's own, so they line up.
    pub agents: Vec<Agent>,
    /// What its settings say, which a change of them starts from; none for
    /// a project a `mind.toml` marks, which has none to change.
    pub settings: Option<ProjectSettings>,
    /// Why its entry opens no project, when it does not: listed rather than
    /// left out, since it is the one somebody has to find to unlink it.
    pub stale: Option<Said>,
    /// How it stands as a whole, from everything below. A [`Listing`] read
    /// for some kinds tells it from what it read of them, as it counts the
    /// tally, and from `misplaced`, which no listing narrows: a skill that
    /// does not read leaves `ok` a project listed for its rules. Reading
    /// every kind for the health alone was the other way, which would weigh
    /// a failure the listing says nowhere.
    pub health: Health,
    /// Where it authors each kind, as a route from it.
    pub directories: Vec<Directory>,
    /// Everything it holds, authored, received or somebody's, by kind and
    /// then name, the rules its `AGENTS.md` block holds among them (D-23).
    pub artifacts: Vec<Held>,
    /// Each entry of an MCP server its agents' config files hold, and each
    /// one the ledger records writing, as `status` lists them: by agent,
    /// then name, a foreign one among them (D-23).
    pub entries: Vec<EntryStatus>,
    /// How many of them are of each kind, and stand in each state.
    pub tally: Tally,
    /// What `mind add --template` can start from in it.
    pub templates: Vec<Template>,
    /// Each kind it installs only where some of its agents do not read, as
    /// `status` says it.
    pub unreached: Vec<Unreached>,
    /// Each rule it received that some of its agents read nowhere, and
    /// each MCP server it received that some of them are given no entry
    /// of, as `status` says them.
    pub missed: Vec<Missed>,
    /// Each folder its settings name in `targets` that no install uses.
    pub misplaced: Vec<Said>,
    /// What could not be read in it and is no artifact's: a folder that
    /// cannot be listed, the ledger, its templates.
    pub failures: Vec<Said>,
}

/// What a project's settings say, as a change of them takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ProjectSettings {
    /// Its agents, in the order its settings name them, which a change
    /// keeps for the ones it keeps: not report order, which
    /// [`Project::agents`] is in.
    pub agents: Vec<Agent>,
    pub share: Share,
    pub edit_tracked: bool,
    /// Each kind it names `targets` for, in report order: none when every
    /// kind goes where its agents read.
    pub targets: Vec<KindTargets>,
}

/// The folders a project's settings install one kind in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct KindTargets {
    pub kind: Kind,
    /// Each folder as the settings spell it, `/` separated.
    pub folders: Vec<String>,
}

/// Where a holder keeps one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Directory {
    pub kind: Kind,
    /// A route from the holder, `/` separated, or the path itself when no
    /// route leads there.
    pub path: String,
}

/// One artifact a holder holds, and how it stands there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Held {
    pub kind: Kind,
    pub name: ArtifactName,
    /// The one line a listing shows; none for one whose file does not read,
    /// and for an install nothing of which is found.
    pub summary: Option<String>,
    /// How its name stands in its project, as `status` says; none for the
    /// library's own, which are no project's. Only a copy that reads has a
    /// standing, so beside an [`unread`](Self::unread) one it is that of a
    /// copy of the name elsewhere, in an agent's folder, and none when
    /// there is no such copy.
    pub state: Option<State>,
    /// The agents among its project's that read a copy of it, in report
    /// order.
    pub agents: Vec<Agent>,
    /// Where what was installed came from, as `--from` takes it.
    pub origin: Option<Origin>,
    /// Where it is held, as routes from its holder, `/` separated, in path
    /// order: each copy discovery found, the one whose file does not read
    /// among them, and each place the ledger records putting one, found or
    /// not.
    pub targets: Vec<String>,
    /// Everything `validate` says of it, warnings included, each saying
    /// whether it is one.
    pub issues: Vec<ValidationIssue>,
    /// The copy whose file does not read, when one does not, and why.
    /// Listed all the same, by the name its place gives it, because a file
    /// that does not read is the one most in need of opening.
    pub unread: Option<Unread>,
}

/// How many artifacts a holder holds of each kind, and how many of them
/// stand in each state.
///
/// Counted here, from the rows it counts, rather than by each front end from
/// the rows it draws: a count is an answer, as which row is which artifact's
/// is, and the panel counting its own would be a second set of them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Tally {
    /// How many, every kind together.
    pub total: usize,
    /// How many of each kind it was read for, in report order, one it holds
    /// none of among them: what a front end lays a column a kind out by.
    pub kinds: Vec<KindCount>,
    /// How many stand in each state, every state in report order, one none
    /// stands in among them. A row that stands in none, the library's own
    /// or one whose only copy does not read, is in no count here.
    pub states: Vec<StateCount>,
}

/// How many of one kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct KindCount {
    pub kind: Kind,
    pub count: usize,
}

/// How many stand in one state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StateCount {
    pub state: State,
    pub count: usize,
}

impl Tally {
    /// `rows`, each a kind and the state it stands in, counted for `kinds`:
    /// every one of them listed, in report order, whether any row is of it.
    fn of(rows: impl IntoIterator<Item = (Kind, Option<State>)>, kinds: &[Kind]) -> Tally {
        let mut tally = Tally {
            total: 0,
            kinds: Kind::ALL
                .into_iter()
                .filter(|kind| kinds.contains(kind))
                .map(|kind| KindCount { kind, count: 0 })
                .collect(),
            states: State::ALL
                .into_iter()
                .map(|state| StateCount { state, count: 0 })
                .collect(),
        };
        for (kind, state) in rows {
            tally.total += 1;
            if let Some(counted) = tally.kinds.iter_mut().find(|each| each.kind == kind) {
                counted.count += 1;
            }
            if let Some(counted) = tally
                .states
                .iter_mut()
                .find(|each| Some(each.state) == state)
            {
                counted.count += 1;
            }
        }
        tally
    }

    /// What `held` holds, for `kinds`.
    fn of_held(held: &[Held], kinds: &[Kind]) -> Tally {
        Tally::of(held.iter().map(|held| (held.kind, held.state)), kinds)
    }
}

/// How a project stands as a whole: the one word a front end sorts and
/// filters its projects by, told here, so that no front end weighs what it
/// draws on its own.
///
/// Declaration order is report order, from nothing to do to nothing to
/// open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Health {
    /// Nothing in it asks for anything: what was installed is where it was
    /// put, as it was put there and as its origin still offers it, every
    /// agent reads what it was given, and everything reads and passes
    /// `validate` with no warning.
    Ok,
    /// Everything in it reads, and something asks for a command: an
    /// install outdated, edited, missing or orphaned, a kind or a rule some
    /// of its agents read nowhere, a folder its targets name that no
    /// install uses, copies of one name that do not hold the same, or
    /// something `validate` warns of.
    Attention,
    /// Something in it does not read, or does not pass `validate`: a file,
    /// a folder that cannot be listed, the ledger, its templates.
    Broken,
    /// Its entry opens no project.
    Stale,
}

impl Health {
    /// Every health, in report order.
    pub const ALL: [Health; 4] = [Health::Ok, Health::Attention, Health::Broken, Health::Stale];

    /// How it is written in JSON.
    pub const fn slug(self) -> &'static str {
        match self {
            Health::Ok => "ok",
            Health::Attention => "attention",
            Health::Broken => "broken",
            Health::Stale => "stale",
        }
    }

    /// How `project`, one that opened, stands, from what the snapshot says
    /// of it, and whether any of its `failures` is of something that does
    /// not read (`unreadable`), which a [`Said`] no longer tells.
    ///
    /// Something that does not read is said over something to do, since
    /// what to do cannot be told of a file nobody can read. A copy that is
    /// somebody's (`foreign`) asks for nothing: it is never Mindweaver's to
    /// change, and only an install of its name, which refuses, meets it.
    fn of(project: &Project, unreadable: bool) -> Health {
        let broken = unreadable
            || project.artifacts.iter().any(|held| {
                held.unread.is_some() || held.issues.iter().any(|issue| !issue.is_warning())
            });
        if broken {
            return Health::Broken;
        }
        // A failure left is of copies that read and do not agree.
        let attention = !project.failures.is_empty()
            || !project.unreached.is_empty()
            || !project.missed.is_empty()
            || !project.misplaced.is_empty()
            || project.artifacts.iter().any(|held| {
                held.state.is_some_and(State::asks_for_a_command) || !held.issues.is_empty()
            });
        if attention {
            Health::Attention
        } else {
            Health::Ok
        }
    }
}

/// By its slug, for the reason a kind is: one spelling, which a renamed
/// variant cannot change.
impl Serialize for Health {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// How a project stands as a whole.
    Health by slug { Ok, Attention, Broken, Stale }
}

/// A copy of an artifact whose file does not read, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Unread {
    /// Where the copy is, as a route from its holder, `/` separated: its
    /// place in the folder the holder keeps its kind in, the one place a
    /// file that does not read is named by ([`lifecycle::identify`]). Said
    /// on its own, as well as among [`Held::targets`], since a copy that
    /// reads can sit beside it under the same name, elsewhere.
    pub path: String,
    pub code: &'static str,
    /// The reason alone, without the path, which [`path`](Self::path)
    /// says: written through [`redacted`](crate::url::redacted), as a
    /// [`Said`]'s message is.
    pub message: Redacted,
}

/// A workspace another one loads, and how much of its own it offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LoadedWorkspace {
    /// The entry as the marker spells it: what `mindweaver unload` is given.
    pub entry: String,
    /// Where it points: what `mind unload` is given, from a project.
    #[serde(serialize_with = "paths::lossy")]
    pub path: PathBuf,
    /// Its name; none for one that would not open.
    pub name: Option<String>,
    /// How many of its own it offers to install, counted from what the
    /// install machinery offers, so the number is what an install shows. A
    /// second spelling of one workspace offers what the first does.
    pub offers: usize,
    /// What `--from` takes for what it offers, and what an install of one
    /// of them records as its origin: the first spelling's, for a second
    /// spelling of one workspace, as its offers are.
    pub origin: Origin,
    /// Why it would not open, when it would not.
    pub error: Option<Said>,
}

/// A workspace this machine knows, as `mindweaver workspaces` lists it, with
/// how the workspace a snapshot is of stands with it: whether it loads it,
/// and whether it could.
///
/// Beside the row rather than in it, as a shelf row's origin is
/// ([`Shelved`]): the row is the machine's list, the same whichever
/// workspace asks, and what one of them loads is nothing that list says.
/// Told here rather than by a front end from `in_use`, `stale` and
/// [`Snapshot::loaded`], whose paths it would compare itself: two spellings
/// of one folder are one workspace, which only a comparison of where each
/// leads tells.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct KnownHere {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub workspace: KnownWorkspace,
    /// The entry this workspace loads it by, when it loads it, as
    /// [`LoadedWorkspace::entry`] spells it: what `mindweaver unload` is given.
    /// The first, when it loads it by more than one spelling.
    pub loaded_as: Option<String>,
    /// Whether this workspace can load it: it opens, it is not this one,
    /// this one loads it by no spelling yet, and its path is text a command
    /// line can hold. What a front end offers to load, by its `path`, which
    /// `mindweaver load` is given as it is.
    pub loadable: bool,
}

impl KnownHere {
    /// `known`, as `workspace`, which loads `loaded`, stands with it.
    fn of(known: KnownWorkspace, workspace: &WeaverWorkspace, loaded: &[LoadedWorkspace]) -> Self {
        let loaded_as = loaded
            .iter()
            .find(|load| same_directory(&load.path, &known.path))
            .map(|load| load.entry.clone());
        // `mindweaver load` of this one is refused (`loads_itself`), and of one
        // loaded already changes nothing; one that would not open is no
        // workspace to load. A path that is not UTF-8 is no word of a
        // command line, which the path policy refuses rather than spell it
        // as another folder.
        let loadable = known.stale.is_none()
            && loaded_as.is_none()
            && !same_directory(&known.path, workspace.root())
            && known.path.to_str().is_some();
        KnownHere {
            workspace: known,
            loaded_as,
            loadable,
        }
    }
}

/// One artifact gathered onto the shelf, with what `--from` takes for it.
///
/// The ledger's row with its origin beside it, as `gather list --json`
/// prints an offer from the shelf: the same names, so a script reading
/// either reads one shape. Beside the row rather than in it, since an offer
/// flattens the row beside an origin of its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Shelved {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub gathered: Gathered,
    /// Where it came from, as `--from` takes it and an install of it
    /// records it ([`Source::origin`]).
    pub origin: Origin,
}

impl Shelved {
    fn of(gathered: Gathered) -> Shelved {
        Shelved {
            origin: gathered.source.origin(),
            gathered,
        }
    }
}

/// A source the shelf is filled from, with what `--from` takes for what it
/// supplied, how much of it the shelf holds and since when: a row of
/// `mindweaver gather sources` (#100), as the snapshot lists it too.
///
/// Counted and dated here, from the shelf's rows, rather than by a front
/// end from [`Snapshot::shelf`], as every count a view shows is. `shelved`
/// and `gathered_at` were added after the rest, which moved no version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ShelfSource {
    #[serde(flatten)]
    #[cfg_attr(feature = "ts", ts(flatten))]
    pub source: Source,
    /// [`Source::origin`], which every artifact it supplied is installed
    /// from.
    pub origin: Origin,
    /// How many of its artifacts the shelf holds, of each kind it holds
    /// any of, in report order: none for a source no gather has put
    /// anything on the shelf from.
    pub shelved: Vec<KindCount>,
    /// When its last gather put on the shelf what it holds there, in
    /// seconds since the Unix epoch: the latest of their `gathered_at`, a
    /// gather recording every artifact it takes, changed or not. `None`
    /// while the shelf holds nothing of it.
    pub gathered_at: Option<i64>,
}

impl ShelfSource {
    /// Every source the workspace's ledger knows, in the order it lists
    /// them (by URL, then branch, then folder), each with what the shelf
    /// holds of it: what `mindweaver gather sources` lists, and the
    /// snapshot's `sources`.
    pub fn read(ledger: &Ledger) -> Result<Vec<ShelfSource>, LedgerError> {
        let gathered = ledger.gathered()?;
        Ok(Self::all(ledger.sources()?, &gathered))
    }

    /// Each of `sources`, with what `shelf` holds of it.
    fn all(sources: Vec<Source>, shelf: &[Gathered]) -> Vec<ShelfSource> {
        sources
            .into_iter()
            .map(|source| {
                let held: Vec<&Gathered> = shelf
                    .iter()
                    .filter(|gathered| gathered.source.id == source.id)
                    .collect();
                let shelved = Kind::ALL
                    .into_iter()
                    .map(|kind| KindCount {
                        kind,
                        count: held.iter().filter(|gathered| gathered.kind == kind).count(),
                    })
                    .filter(|counted| counted.count > 0)
                    .collect();
                ShelfSource {
                    origin: source.origin(),
                    shelved,
                    gathered_at: held.iter().map(|gathered| gathered.gathered_at).max(),
                    source,
                }
            })
            .collect()
    }
}

/// One thing there is to install, from one place: a row of the install
/// screen turned round by artifact, and of the panel's matrix (#79).
///
/// Told by core's own list of what an install offers ([`install::survey`]),
/// rather than put together by a front end from the library, the shelf and
/// the loads: only that list leaves out a name no copy can be filed under,
/// and reads what each loaded workspace offers, which nothing else in a
/// snapshot lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Offered {
    pub kind: Kind,
    pub name: ArtifactName,
    /// The one line a listing shows.
    pub summary: Option<String>,
    /// Which place offers it.
    pub place: Place,
    /// Where it is offered from, as `--from` takes it and an install of it
    /// records it: what a box of it names its offer by.
    pub origin: Origin,
}

impl Offered {
    fn of(offer: &Offer) -> Offered {
        Offered {
            kind: offer.kind(),
            name: offer.name().to_owned(),
            summary: offer.summary().map(str::to_owned),
            place: offer.place(),
            origin: offer.origin(),
        }
    }
}

/// How one entry in the registry stands with every offer ([`Offered`]):
/// what each of its boxes shows, told as the install screen tells it, by
/// [`install::survey`] and [`install::holding`], so the panel ticks the box
/// the screen ticks.
///
/// A box neither listed in `held` nor of a kind in `closed` is empty: the
/// project holds nothing of its name, and records no installation of it.
/// Only the boxes that are not are listed, a project holding a few of the
/// hundreds offered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Boxes {
    /// The entry, as [`Project::entry`] spells it.
    pub entry: String,
    /// Each kind none of its boxes can be asked about, and why, by the code
    /// a plan locks one by: every kind of an entry that opens no project,
    /// and each a project has nowhere to put ([`install::receives`]), or
    /// could not be surveyed for.
    pub closed: Vec<Closed>,
    /// Each box of a name it holds something of or records, kind by kind,
    /// in the order of [`Snapshot::offers`].
    pub held: Vec<HeldBox>,
}

/// A kind none of a project's boxes of can be asked about, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Closed {
    pub kind: Kind,
    pub why: Said,
}

/// A box of an offer whose name a project holds something of, or records
/// an installation of: how the name stands, and whether the box is ticked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HeldBox {
    pub kind: Kind,
    pub name: ArtifactName,
    /// The offer's, as [`Offered::origin`] says it.
    pub origin: Origin,
    /// How the name stands in the project, as every offer of it stands
    /// ([`State::of_standing`]): never `authored`, which no offer is.
    pub state: State,
    /// Whether the box is ticked ([`install::holding`]): the offer the
    /// installation came from, every offer of a name somebody's copy holds,
    /// and none of one whose every copy has gone, for ticking it to put
    /// them back.
    pub ticked: bool,
}

/// What `workspace` offers, read once, and how each of `members` stands with
/// it: [`Snapshot::offers`] and [`Snapshot::boxes`].
///
/// A ledger that will not open, or a shelf that will not read, is said among
/// the snapshot's warnings already, so it offers nothing here, rather than
/// saying it again in every project.
fn boxes(workspace: &WeaverWorkspace, members: &[Member]) -> (Vec<Offered>, Vec<Boxes>) {
    let ledger = workspace.ledger_for_reading().ok();
    let offers: Vec<(Kind, Vec<Offer>)> = Kind::ALL
        .into_iter()
        .map(|kind| {
            let offers = ledger
                .as_ref()
                .and_then(|ledger| install::offers(workspace, ledger, kind).ok())
                .unwrap_or_default();
            (kind, offers)
        })
        .collect();
    let offered = offers
        .iter()
        .flat_map(|(_, offers)| offers.iter().map(Offered::of))
        .collect();
    let boxes = members
        .iter()
        .map(|member| {
            let entry = paths::as_route(&member.entry);
            match (&member.project, &ledger) {
                (Ok(project), Some(ledger)) => {
                    Boxes::of(entry, workspace, ledger, project, &offers)
                }
                (Err(error), _) => Boxes {
                    entry,
                    closed: Kind::ALL
                        .into_iter()
                        .map(|kind| Closed {
                            kind,
                            why: Said::of(error.code(), error),
                        })
                        .collect(),
                    held: Vec::new(),
                },
                (Ok(_), None) => Boxes {
                    entry,
                    closed: Vec::new(),
                    held: Vec::new(),
                },
            }
        })
        .collect();
    (offered, boxes)
}

impl Boxes {
    /// How `project`, linked as `entry`, stands with `offers`, every offer
    /// of each kind, read against `ledger`.
    fn of(
        entry: String,
        workspace: &WeaverWorkspace,
        ledger: &Ledger,
        project: &MindProject,
        offers: &[(Kind, Vec<Offer>)],
    ) -> Boxes {
        let mut boxes = Boxes {
            entry,
            closed: Vec::new(),
            held: Vec::new(),
        };
        for (kind, offers) in offers {
            let kind = *kind;
            let read = install::receives(project, kind)
                .and_then(|()| install::survey_among(workspace, ledger, project, kind, offers))
                .and_then(|candidates| {
                    let ticked = install::holding(workspace, ledger, project, &candidates)
                        .map_err(install::InstallError::from)?;
                    Ok((candidates, ticked))
                });
            let (candidates, ticked) = match read {
                Ok(read) => read,
                Err(error) => {
                    boxes.closed.push(Closed {
                        kind,
                        why: Said::of(error.code(), &error),
                    });
                    continue;
                }
            };
            for (candidate, ticked) in candidates.iter().zip(ticked) {
                let Some(state) = State::of_standing(candidate.standing) else {
                    continue;
                };
                boxes.held.push(HeldBox {
                    kind,
                    name: candidate.name().to_owned(),
                    origin: candidate.origin(),
                    state,
                    ticked,
                });
            }
        }
        boxes
    }
}

/// The holders a listing reads, each as a snapshot holds it, read for the
/// kinds the listing asks for: what `list --json` prints.
///
/// A part of the snapshot rather than a shape of its own, so what a script
/// prints of a project is what a front end draws of it, every name the
/// same, and a test comparing the two compares like with like. Read for the
/// kinds asked, as the listing reads them, rather than read whole and cut
/// down after: a folder of a kind not asked for is not listed, so it is no
/// failure of the reply's either.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Listing {
    /// The workspace's own, when the listing reads them: none for one
    /// project's listing.
    pub library: Option<Library>,
    /// Every entry in the workspace's registry, as the snapshot lists them,
    /// or the one project a listing is of.
    pub projects: Vec<Project>,
}

impl Listing {
    /// What `workspace` holds of `kinds`: its own, and every entry in its
    /// registry, against its ledger.
    pub fn of_workspace(workspace: &WeaverWorkspace, kinds: &[Kind]) -> Listing {
        Listing {
            library: Some(Library::of(workspace, kinds)),
            projects: workspace
                .members()
                .iter()
                .map(|member| Project::of_member(member, workspace, kinds))
                .collect(),
        }
    }

    /// What `project` holds of `kinds`, read as [`Project::read`] reads it,
    /// against what `workspace` records when that one links it.
    pub fn of_project(
        project: &MindProject,
        workspace: Option<&WeaverWorkspace>,
        kinds: &[Kind],
    ) -> Listing {
        Listing {
            library: None,
            projects: vec![Project::read_kinds(project, workspace, kinds)],
        }
    }
}

impl Snapshot {
    /// The workspace `inputs` resolve to, as a read resolves it whatever
    /// they intend, and everything it holds; none when no workspace
    /// resolves.
    ///
    /// A read: the default workspace is the one in home as it is, or as it
    /// would be, never made; a workspace named or activated that is not one
    /// is refused, as every command refuses it.
    ///
    /// `machine` is what doctor is told of the agent homes and the
    /// variables set, as the front end decided them ([`Machine`]).
    pub fn read(inputs: &Inputs<'_>, machine: &Machine) -> Result<Option<Snapshot>, ResolveError> {
        let inputs = Inputs {
            intent: Intent::Read,
            ..*inputs
        };
        Ok(
            resolve::resolve(&inputs)?
                .map(|resolved| Snapshot::of(&resolved, inputs.home, machine)),
        )
    }

    /// Everything `resolved` holds, with the workspaces the default one in
    /// `home` knows and what doctor finds told `machine`: for a front end
    /// that resolved the workspace itself, to scan it too
    /// ([`crate::scan`]), which no snapshot does.
    pub fn of(resolved: &Resolved, home: Option<&Path>, machine: &Machine) -> Snapshot {
        let workspace = &resolved.workspace;
        let mut warnings: Vec<Said> = Vec::new();

        let members = workspace.members();
        let ids: Vec<(ProjectId, PathBuf)> = members
            .iter()
            .filter_map(|member| {
                let settings = member.project.as_ref().ok()?.settings()?;
                Some((settings.id.clone(), member.root.clone()))
            })
            .collect();
        let (offers, boxes) = boxes(workspace, &members);
        let mut projects: Vec<Project> = Vec::new();
        let mut doctor: Vec<Examined> = Vec::new();
        for member in &members {
            let project = Project::of_member(member, workspace, &Kind::ALL);
            // Told what the project's agents miss, read for it already.
            let told = (project.unreached.as_slice(), project.missed.as_slice());
            doctor.push(Examined::of_member(member, workspace, machine, Some(told)));
            projects.push(project);
        }
        warnings.extend(
            relink::duplicates(&ids)
                .iter()
                .map(|duplicate| Said::of(duplicate.code(), duplicate)),
        );

        let loads = install::offered_by_loads(workspace);
        let loaded = loaded(workspace, &loads.offers);
        // A loaded workspace that would not open says why as its own
        // `error`; the rest is no entry's.
        warnings.extend(
            loads
                .failures
                .iter()
                .filter(|failure| !matches!(failure, LoadFailure::Workspace(_)))
                .map(|failure| Said::of(failure.code(), failure)),
        );

        // A ledger that will not open is said once, with nothing from it;
        // one that opens says each thing it could not answer, and answers
        // the rest, so a shelf is counted whatever the log says.
        let (shelf, sources, history): (Vec<Shelved>, Vec<ShelfSource>, Vec<Entry>) =
            match workspace.ledger_for_reading() {
                Ok(ledger) => {
                    let gathered = answered(ledger.gathered(), &mut warnings);
                    let sources =
                        ShelfSource::all(answered(ledger.sources(), &mut warnings), &gathered);
                    (
                        gathered.into_iter().map(Shelved::of).collect(),
                        sources,
                        answered(ledger.history(HISTORY), &mut warnings),
                    )
                }
                Err(error) => {
                    warnings.push(Said::of(error.code(), &error));
                    Default::default()
                }
            };

        // What could not be read of a workspace this machine knows is said
        // in its row, as `stale`.
        let (known, _) = known::known(home, Some(workspace));
        let known = known
            .workspaces
            .into_iter()
            .map(|known| KnownHere::of(known, workspace, &loaded))
            .collect();
        let library = Library::of(workspace, &Kind::ALL);

        Snapshot {
            format: FORMAT,
            workspace: Workspace {
                name: workspace.name().to_owned(),
                path: workspace.root().to_path_buf(),
                reason: resolved.reason,
                made: workspace.is_made(),
            },
            kinds: Kind::ALL.to_vec(),
            agents: Agent::ALL.to_vec(),
            states: State::ALL.to_vec(),
            box_states: State::of_boxes(),
            healths: Health::ALL.to_vec(),
            summary: Summary::of(&projects, &library, &shelf),
            library,
            projects,
            loaded,
            shelf,
            sources,
            offers,
            boxes,
            doctor,
            known,
            history,
            warnings,
        }
    }

    /// The entry of its registry spelled `entry`, as
    /// [`projects`](Self::projects) lists it, with the project it opens or
    /// why it opens none: what a front end shows of one project, cut from
    /// the snapshot rather than read another way, so it says of it exactly
    /// what the snapshot says.
    ///
    /// By the entry's spelling and nothing else, as
    /// [`policy::linked`](crate::policy::linked) takes one, and refused as
    /// that refuses one no registry holds (`not_registered`): a project's
    /// name, which two projects can declare, would be a second way to name
    /// one.
    pub fn project(self, entry: &str) -> Result<Project, WorkspaceError> {
        let workspace = self.workspace.path;
        self.projects
            .into_iter()
            .find(|project| project.entry.as_deref() == Some(entry))
            .ok_or_else(|| WorkspaceError::NotRegistered {
                path: PathBuf::from(entry),
                workspace,
            })
    }
}

impl Library {
    /// `workspace`'s own, of the kinds among `kinds` it keeps apart from a
    /// project at its root ([`WeaverWorkspace::own_kinds`]), with its
    /// templates and its folders of those kinds.
    fn of(workspace: &WeaverWorkspace, kinds: &[Kind]) -> Library {
        let own = workspace.own();
        let own_kinds: Vec<Kind> = workspace
            .own_kinds()
            .into_iter()
            .filter(|kind| kinds.contains(kind))
            .collect();
        let catalog = Catalog::discover_kinds(slice::from_ref(&own), &own_kinds);
        let (artifacts, mut failures) = holdings(&own, &catalog, None);
        let templates = templates(Some(workspace), &own, kinds, &mut failures);
        let servers = catalog
            .artifacts()
            .iter()
            .filter_map(Artifact::definition)
            .map(LibraryServer::of)
            .collect();
        Library {
            origin: install::WORKSPACE_ORIGIN,
            directories: directories(&own, kinds),
            holders: holders(workspace, &own, &artifacts),
            artifacts,
            templates,
            failures,
            servers,
        }
    }
}

impl Project {
    /// `project` as a snapshot shows one, against what `workspace`'s ledger
    /// records when that one links it, with `workspace`'s templates: for a
    /// front end showing a project no workspace links, `workspace` being the
    /// one above it, if any, as `mind` would consult it.
    pub fn read(project: &MindProject, workspace: Option<&WeaverWorkspace>) -> Project {
        Project::read_kinds(project, workspace, &Kind::ALL)
    }

    /// [`read`](Self::read), with what doctor finds of it told `machine`,
    /// as a snapshot holds both of every project it lists: for a front end
    /// showing a project no workspace links, which takes both in one read
    /// from core rather than a report beside the view, and what `status`
    /// says its agents miss read once for the two.
    pub fn read_examined(
        project: &MindProject,
        workspace: Option<&WeaverWorkspace>,
        machine: &Machine,
    ) -> (Project, Examined) {
        let read = Project::read(project, workspace);
        let told = (read.unreached.as_slice(), read.missed.as_slice());
        let examined = Examined::of(project, read.entry.clone(), workspace, machine, Some(told));
        (read, examined)
    }

    /// [`read`](Self::read), for `kinds` alone.
    fn read_kinds(
        project: &MindProject,
        workspace: Option<&WeaverWorkspace>,
        kinds: &[Kind],
    ) -> Project {
        let entry = workspace
            .and_then(|workspace| workspace.linked_as(project.root()))
            .map(paths::as_route);
        Project::opened(entry, project, workspace, kinds)
    }

    /// One entry of `workspace`'s registry, named as that entry spells it
    /// rather than as the first entry pointing at the same directory does,
    /// read for `kinds`.
    fn of_member(member: &Member, workspace: &WeaverWorkspace, kinds: &[Kind]) -> Project {
        let entry = paths::as_route(&member.entry);
        match &member.project {
            Ok(project) => Project::opened(Some(entry), project, Some(workspace), kinds),
            Err(error) => Project::stale(entry, member.root.clone(), error, kinds),
        }
    }

    /// `project` read for `kinds`: what it holds of them, the folders and
    /// the templates it has of them, and what of them its agents miss.
    fn opened(
        entry: Option<String>,
        project: &MindProject,
        workspace: Option<&WeaverWorkspace>,
        kinds: &[Kind],
    ) -> Project {
        let (status, catalog, placed) = ProjectStatus::read_placed(project, kinds, workspace);
        let (artifacts, mut failures) = holdings(project, &catalog, Some((&status, &placed)));
        // Told apart here, where the failures still say what they are: a
        // file or a folder that does not read, or copies that do not agree.
        let read = failures.len();
        let templates = templates(workspace, project, kinds, &mut failures);
        let unreadable = failures.len() > read
            || catalog
                .failures()
                .iter()
                .any(DiscoveryFailure::is_unreadable);
        let settings = project.settings();
        let project = Project {
            entry,
            path: project.root().to_path_buf(),
            name: Some(project.name().to_owned()),
            marker: Some(project.marker_file()),
            migrates: project.changeable().is_err(),
            id: settings.map(|settings| settings.id.clone()),
            agents: status.agents,
            settings: settings.map(ProjectSettings::of),
            stale: None,
            health: Health::Ok,
            directories: directories(project, kinds),
            tally: Tally::of_held(&artifacts, kinds),
            artifacts,
            entries: status.entries,
            templates,
            unreached: status.unreached,
            missed: status.missed,
            misplaced: project
                .misplaced_targets()
                .iter()
                .map(|misplaced| Said::of(misplaced.code(), misplaced))
                .collect(),
            failures,
        };
        Project {
            health: Health::of(&project, unreadable),
            ..project
        }
    }

    /// An entry that opens no project, which holds none of `kinds`.
    fn stale(entry: String, root: PathBuf, error: &WorkspaceError, kinds: &[Kind]) -> Project {
        Project {
            entry: Some(entry),
            path: root,
            name: None,
            marker: None,
            migrates: false,
            id: None,
            agents: Vec::new(),
            settings: None,
            stale: Some(Said::of(error.code(), error)),
            health: Health::Stale,
            directories: Vec::new(),
            artifacts: Vec::new(),
            entries: Vec::new(),
            tally: Tally::of([], kinds),
            templates: Vec::new(),
            unreached: Vec::new(),
            missed: Vec::new(),
            misplaced: Vec::new(),
            failures: Vec::new(),
        }
    }
}

impl ProjectSettings {
    fn of(settings: &Settings) -> ProjectSettings {
        ProjectSettings {
            agents: settings.agents.clone(),
            share: settings.share,
            edit_tracked: settings.edit_tracked,
            targets: Kind::ALL
                .into_iter()
                .filter_map(|kind| {
                    Some(KindTargets {
                        kind,
                        folders: settings
                            .targets
                            .get(kind)?
                            .iter()
                            .map(|folder| paths::as_route(folder))
                            .collect(),
                    })
                })
                .collect(),
        }
    }
}

/// What `project` holds, read in `catalog`, with how each stands where
/// `status` says, and the failures no artifact can be named by.
///
/// A file that does not read is listed where it would have been, by the
/// name its place gives it ([`lifecycle::identify`]), so it can be opened
/// and fixed from a front end; and in place of a copy of that name in the
/// project's agents' folders, since a row is named by its name alone and a
/// change to that name reaches the project's own copy, this one. The row
/// keeps the report's standing of the name, which only a copy that reads
/// has, and adds this copy's place to where it is held, naming it in
/// `unread` too, so that no reader takes the other copy for it. Only what
/// cannot be pinned to an artifact, a folder that cannot be listed, is left
/// a failure on its own. An install nothing of which is found is listed as
/// the report lists it, from which it can be put back.
fn holdings(
    project: &MindProject,
    catalog: &Catalog,
    status: Option<(&ProjectStatus, &[Vec<String>])>,
) -> (Vec<Held>, Vec<Said>) {
    let root = project.root();
    let row = |kind: Kind, name: &str| -> Option<(&ArtifactStatus, &Vec<String>)> {
        let (status, placed) = status?;
        let index = status
            .artifacts
            .iter()
            .position(|held| held.kind == kind && held.name == name)?;
        Some((&status.artifacts[index], &placed[index]))
    };
    let mut held: Vec<Held> = catalog
        .artifacts()
        .iter()
        .map(|artifact| {
            let row = row(artifact.kind(), artifact.name());
            Held {
                kind: artifact.kind(),
                name: artifact.name().to_owned(),
                summary: artifact.summary().map(str::to_owned),
                state: row.map(|(row, _)| row.state),
                agents: row.map(|(row, _)| row.agents.clone()).unwrap_or_default(),
                origin: row.and_then(|(row, _)| row.origin.clone()),
                targets: row.map_or_else(
                    || {
                        artifact
                            .copies()
                            .iter()
                            .map(|copy| install::route(root, copy.path()))
                            .collect()
                    },
                    |(_, placed)| placed.clone(),
                ),
                issues: artifact.validate(),
                unread: None,
            }
        })
        .collect();

    let mut failures = Vec::new();
    for failure in catalog.failures() {
        let Some((kind, name)) = lifecycle::identify(project, failure.path()) else {
            failures.push(Said::of(failure.code(), failure));
            continue;
        };
        let row = row(kind, &name);
        // Identified, so the project keeps a folder for the kind, and the
        // file is in it.
        let path = lifecycle::place(project, kind, &name).map_or_else(
            || install::route(root, failure.path()),
            |place| install::route(root, &place),
        );
        let mut targets = row.map(|(_, placed)| placed.clone()).unwrap_or_default();
        targets.push(path.clone());
        targets.sort();
        targets.dedup();
        let unread = Held {
            summary: None,
            state: row.map(|(row, _)| row.state),
            agents: row.map(|(row, _)| row.agents.clone()).unwrap_or_default(),
            origin: row.and_then(|(row, _)| row.origin.clone()),
            targets,
            issues: Vec::new(),
            unread: Some(Unread {
                path,
                code: failure.code(),
                message: Redacted::new(
                    &failure
                        .source()
                        .map_or_else(|| failure.to_string(), ToString::to_string),
                ),
            }),
            kind,
            name: ArtifactName::from(name),
        };
        match held
            .iter_mut()
            .find(|other| other.kind == unread.kind && other.name == unread.name)
        {
            Some(other) => *other = unread,
            None => held.push(unread),
        }
    }

    if let Some((status, placed)) = status {
        for (row, placed) in status.artifacts.iter().zip(placed) {
            if held
                .iter()
                .any(|other| other.kind == row.kind && other.name == row.name)
            {
                continue;
            }
            held.push(Held {
                kind: row.kind,
                name: row.name.clone(),
                summary: None,
                state: Some(row.state),
                agents: row.agents.clone(),
                origin: row.origin.clone(),
                targets: placed.clone(),
                issues: Vec::new(),
                unread: None,
            });
        }
    }
    held.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    (held, failures)
}

/// What `mind add --template` can start from in `project`, with
/// `workspace`'s, of `kinds`; one that cannot be read among `failures`.
fn templates(
    workspace: Option<&WeaverWorkspace>,
    project: &MindProject,
    kinds: &[Kind],
    failures: &mut Vec<Said>,
) -> Vec<Template> {
    let mut templates = template::templates(workspace, project).unwrap_or_else(|error| {
        failures.push(Said::of(error.code(), &error));
        Vec::new()
    });
    templates.retain(|template| kinds.contains(&template.kind));
    templates
}

/// Where `project` keeps each of `kinds`, as a route from it, in report
/// order; a kind it keeps no folder for is left out.
fn directories(project: &MindProject, kinds: &[Kind]) -> Vec<Directory> {
    Kind::ALL
        .into_iter()
        .filter(|kind| kinds.contains(kind))
        .filter_map(|kind| {
            let directory = project.directory_for(kind)?;
            Some(Directory {
                kind,
                path: install::route(project.root(), &directory),
            })
        })
        .collect()
}

/// Each entry of `workspace`'s registry holding a copy of one of
/// `artifacts`, its own, held in `own`: in the order of `artifacts`, then
/// of the registry.
///
/// Each of its own is held to its place in `own` rather than to what is
/// offered, as `mindweaver remove --everywhere` holds it: one whose file no
/// longer reads is offered by nothing, and what was installed from it is a
/// copy of it all the same. What cannot be read leaves a holder out and is
/// said where it belongs, not again here: a ledger that does not open or
/// answer, by each project linked, and an entry that opens no project, as
/// its `stale`.
fn holders(
    workspace: &WeaverWorkspace,
    own: &MindProject,
    artifacts: &[Held],
) -> Vec<LibraryHolder> {
    if artifacts.is_empty() {
        return Vec::new();
    }
    let Ok(ledger) = workspace.ledger_for_reading() else {
        return Vec::new();
    };
    let members: Vec<(String, MindProject)> = workspace
        .members()
        .into_iter()
        .filter_map(|member| Some((paths::as_route(&member.entry), member.project.ok()?)))
        .collect();
    let mut holders = Vec::new();
    for kind in Kind::ALL {
        let own_of_kind: Vec<&Held> = artifacts.iter().filter(|held| held.kind == kind).collect();
        if own_of_kind.is_empty() {
            continue;
        }
        let Ok(offers) = install::offers(workspace, &ledger, kind) else {
            continue;
        };
        let elsewhere: Vec<Offer> = offers
            .into_iter()
            .filter(|offer| !matches!(offer, Offer::Workspace { .. }))
            .collect();
        // Each project's records of the kind, read once for all its own.
        let records: Vec<(&str, Vec<InstallRecord>)> = members
            .iter()
            .filter_map(|(entry, project)| {
                let named = install::entry_for_root(workspace, project.root());
                Some((entry.as_str(), ledger.records(&named, kind).ok()?))
            })
            .collect();
        for held in own_of_kind {
            let Some(path) = lifecycle::place(own, kind, &held.name) else {
                continue;
            };
            let library = Offer::Workspace {
                kind,
                name: held.name.clone(),
                summary: None,
                path,
            };
            holders.extend(
                records
                    .iter()
                    .filter(|(_, records)| {
                        records
                            .iter()
                            .any(|record| install::from_the_library(record, &library, &elsewhere))
                    })
                    .map(|(entry, _)| LibraryHolder {
                        kind,
                        name: held.name.clone(),
                        entry: (*entry).to_owned(),
                    }),
            );
        }
    }
    holders
}

/// What `read` answered, or nothing, with why among `warnings`.
fn answered<T: Default>(read: Result<T, LedgerError>, warnings: &mut Vec<Said>) -> T {
    read.unwrap_or_else(|error| {
        warnings.push(Said::of(error.code(), &error));
        T::default()
    })
}

impl LoadedWorkspace {
    /// Every workspace `workspace` loads, as a snapshot lists them: for a
    /// front end showing a project no workspace links, what `mind load` has
    /// put into the one above it.
    pub fn of(workspace: &WeaverWorkspace) -> Vec<LoadedWorkspace> {
        loaded(workspace, &install::offered_by_loads(workspace).offers)
    }
}

/// Every workspace `workspace` loads, with how many of `offers`, what the
/// loads offer, each comes with.
fn loaded(workspace: &WeaverWorkspace, offers: &[Offer]) -> Vec<LoadedWorkspace> {
    let loads = workspace.loads();
    loads
        .iter()
        .enumerate()
        .map(|(index, loaded)| {
            let entry = paths::as_route(&loaded.entry);
            // A second spelling of one workspace offers what the first does,
            // which the offers name.
            let first = loads[..index]
                .iter()
                .find(|earlier| same_directory(&earlier.root, &loaded.root))
                .map_or_else(|| entry.clone(), |earlier| paths::as_route(&earlier.entry));
            match &loaded.workspace {
                Ok(source) => LoadedWorkspace {
                    entry,
                    path: loaded.root.clone(),
                    name: Some(source.name().to_owned()),
                    offers: offers
                        .iter()
                        .filter(|offer| {
                            matches!(offer, Offer::Loaded { entry: from, .. } if *from == first)
                        })
                        .count(),
                    origin: install::loaded_origin(&first),
                    error: None,
                },
                Err(error) => LoadedWorkspace {
                    entry,
                    path: loaded.root.clone(),
                    name: None,
                    offers: 0,
                    origin: install::loaded_origin(&first),
                    error: Some(Said::of(error.code(), error)),
                },
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "view_test.rs"]
mod tests;
