//! The panel's backend: what the local server hands each request from the
//! browser to (#67).
//!
//! The server is transport, generic over a [`Backend`] and depending on no
//! mindweaver crate (D-17); this is the CLI's, with its own [`Query`], the
//! [`Task`] the TUI runs, and an [`Outcome`] of its own. It is a front end
//! as the TUI is, and works as the TUI does (D-18): a read is a query core
//! answers, the snapshot or a file's contents, and a change is a task turned
//! into the command lines it is, each shown before it runs, with what of it
//! the panel has somebody confirm first ([`Confirm`]), then run through the
//! real parsers ([`run_line`]) on the server's one writer thread.
//!
//! What it adds is distrust, since every request comes from a browser:
//!
//! - **It acts on one workspace at a time**, the one the panel was launched
//!   on, resolved once ([`PanelBackend::launch`]), until it is switched to
//!   another ([`Backend::switch`]), and only to one core lists
//!   ([`Query::Workspaces`]): a workspace this machine knows, or the
//!   launch's. Each line it runs names it with `-W`, so a change made
//!   meanwhile elsewhere on the disk cannot send a line to another, and a
//!   line copied from the panel reaches the same one; but for a root that
//!   is not UTF-8, which no word can hold, and which every line reaches all
//!   the same through what it runs with.
//! - **The browser supplies no `-C`, `--home`, `-W` or agent home.** A task
//!   has no field for them, no value it holds is read as an option, and
//!   every line runs with the launch's home and agent homes, from the
//!   workspace's root, as the TUI's do.
//! - **A task names only what the path policy allows** ([`Admission`]): a
//!   project by its entry, which becomes the folder the registry resolves,
//!   and a folder not linked yet by an absolute, normalized path outside
//!   every `.mindweaver`, shown verbatim.
//! - **A task no front end without a terminal runs is refused**
//!   ([`Task::refusal`]): a screen, an editor, a shell's code, and `show`,
//!   which prints an artifact whole through any link.
//! - **A gather is followed and can be stopped** ([`task::Followed`]): its
//!   preview says it takes a cancel, each step it takes is reported to the
//!   server as a [`progress::Step`], and the server's cancel is handed to its watch,
//!   which stops it before it places anything. Its source is held to the
//!   path policy as a folder to link is when it names a folder on this
//!   machine ([`policy::source`]); core's gather holds an address to its
//!   own rules, refusing one that carries a credential, and never prompts.
//! - **A scan walks only the folders the browser names**, each held to the
//!   path policy as a folder to link is, and is only read: it is asked for
//!   by hand ([`Query::Scan`]), never with the snapshot, which a live update
//!   reads again on every change.
//! - **A folder is listed only where the browser may name one**, held to
//!   the path policy as a folder to link is, and is only read: what is
//!   directly inside it, for the panel's explorer to pick one from
//!   ([`Query::Folders`]), which runs nothing.
//! - **A plan names only linked projects**, each box's by its entry, held
//!   to the path policy as a task's projects are, and is only read: what
//!   the boxes ticked amount to is core's to say ([`Query::Plan`]), and
//!   nothing runs until its tasks are sent back.
//! - **What an update would do is only read** ([`Query::Updates`]): the
//!   dry run of `mindweaver update --all`, core's, naming nothing a browser
//!   sent; an update runs only as the task the panel sends for it.
//! - **A draft is only judged** ([`Query::Validate`]): what core says of an
//!   artifact a form or the editor is about to send, by the checks the
//!   command makes and in its words, with what `validate` would say of it;
//!   nothing is written until the task that writes it is sent.
//! - **A file reaches the browser only as text inside JSON**, held to its
//!   holder once its links are followed, and never larger than core's cap
//!   ([`Contents`]): the one way a file's contents leave, since `show` is
//!   refused. What an artifact holds is listed the same way ([`Files`]),
//!   each file by the route a file is then asked for by.
//!
//! Every refusal has a code, as every error does, and nothing of a refused
//! task runs. A read whose parameters name what is not there, an entry, an
//! artifact, a file or a folder, is refused as absent, a 404; every other
//! refusal, a task's, a switch's and whatever a body sends among them, as
//! unfit, a 422 ([`Absence`]).

use std::fmt::Display;
use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use mindweaver_core::browse::{BrowseError, Folders};
use mindweaver_core::code::Said;
use mindweaver_core::contents::{Contents, ContentsError, Files};
use mindweaver_core::draft;
use mindweaver_core::install::{self, Cell, Change, Conflict, Locked};
use mindweaver_core::known::{self, Launch};
use mindweaver_core::lock::{self, Waited};
use mindweaver_core::mcp;
use mindweaver_core::policy::PolicyError;
use mindweaver_core::resolve::{resolve, Intent, Reason, Resolved};
use mindweaver_core::scan::{self, Order, DEFAULT_DEPTH};
use mindweaver_core::update::Updates;
use mindweaver_core::value::Redacted;
use mindweaver_core::value::{ArtifactName, Origin};
use mindweaver_core::view::Snapshot;
use mindweaver_core::{paths, policy, watch};
use mindweaver_core::{
    AgentHomes, Found, Kind, LifecycleError, MindProject, Reference, Scan, WeaverWorkspace,
    WorkspaceError,
};
use mindweaver_server::{Backend, Grounds, Preview, Refusal, Route, Underway, Watched};
use serde::{Deserialize, Serialize};

use crate::admit::Admission;
use crate::commands::project::{scan_warned, worth_linking};
use crate::commands::{run_line, Ran};
use crate::progress;
use crate::routes;
use crate::task::{self, cautioned, followed, shell, Caution, Holder, MindsChange, Task};
use crate::tui::state::{transcript, Done};
use crate::{CliError, Lookup, Warning};

/// A read the panel asks for.
///
/// Tagged as a task is, by its name in snake_case beside its fields:
/// `{"type": "snapshot"}`, `{"type": "projects"}`, `{"type": "library"}`,
/// `{"type": "project", "entry": "alpha"}`, `{"type": "files", "holder": {"type":
/// "workspace"}, "reference": "skill:deploy"}`, `{"type": "contents",
/// "holder": {"type": "workspace"}, "reference": "skill:deploy", "file":
/// "SKILL.md"}`, `{"type": "scan", "roots": ["/home/you/Work"], "depth":
/// null}`, `{"type": "folders", "path": "/home/you/Work", "hidden":
/// false}`, `{"type": "plan", "selection": [{"project": "alpha",
/// "reference": "skill:deploy", "origin": "workspace", "wanted": true}]}`,
/// `{"type": "updates"}`, `{"type": "validate", "holder": {"type":
/// "workspace"}, "draft": {"type": "edit", "reference": "skill:deploy",
/// "text": "..."}}`, `{"type": "workspaces"}`. A field a query does not
/// have is refused, as a task's is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    /// Everything the workspace holds ([`Snapshot`]).
    ///
    /// A variant with no fields rather than a unit one, which serde would
    /// read from an object holding any field at all: with braces there is
    /// something for `deny_unknown_fields` to hold the object to.
    Snapshot {},
    /// Every entry of the workspace's registry and the project it opens,
    /// the snapshot's own list of them ([`Snapshot::projects`]).
    ///
    /// Cut from the snapshot rather than read another way, for a script
    /// that wants only them: each is what the snapshot says of it, its
    /// health and its tally included, and nothing a second reading could
    /// say otherwise.
    Projects {},
    /// The workspace's own artifacts, the snapshot's own part of them
    /// ([`Snapshot::library`]), cut from it as the projects are.
    Library {},
    /// One entry of the registry and the project it opens, by the entry as
    /// the snapshot lists it ([`Snapshot::project`]): the snapshot's own row
    /// of it, and an entry no registry holds refused as an unlink of it is.
    Project { entry: String },
    /// Every file and folder of one artifact ([`Files`]), named as for
    /// [`Contents`](Self::Contents), whose `file` each route it lists is.
    Files { holder: Holder, reference: String },
    /// One file of one artifact, as text ([`Contents`]): `reference` names
    /// the artifact in `holder`, the workspace's own or a project by its
    /// entry, and `file` is its route from the artifact's folder, or a
    /// rule's file name.
    Contents {
        holder: Holder,
        reference: String,
        file: String,
    },
    /// What could be linked under `roots`, `depth` levels down, as `mindweaver
    /// scan --suggest` finds it ([`Scanned`]): each root an absolute,
    /// normalized path outside every `.mindweaver` ([`policy::path`]), as a
    /// folder to link is, which is only read; none is the workspace's root,
    /// and no depth core's own (`scan::DEFAULT_DEPTH`), as for the command.
    ///
    /// A query of its own, asked for only when somebody asks, rather than a
    /// part of the snapshot: a scan walks the disk far past what a snapshot
    /// reads, and a live update reads the snapshot again on every change.
    Scan {
        roots: Vec<String>,
        depth: Option<usize>,
    },
    /// The folders directly inside `path`, for the panel's explorer to pick
    /// one from ([`Folders`]): an absolute, normalized path outside every
    /// `.mindweaver` ([`policy::path`]), as a folder to link or a root to
    /// scan is, held to it before anything is read; none is the workspace's
    /// root. Hidden folders only when `hidden` asks for them. Answered with
    /// the launch's home beside them, for a way back to it.
    ///
    /// A read of one folder, asked for each time the explorer opens one,
    /// and no task: choosing a folder only fills a field, and the line it
    /// goes to is previewed and run as a typed one is. No command line has
    /// one, since a terminal has `ls` and completion.
    Folders { path: Option<String>, hidden: bool },
    /// What `selection`, boxes of projects and offers ticked or not,
    /// amounts to ([`Planned`]): core's plan of it ([`install::plan`]), the
    /// one place the rules of a batch live, each change as the task that
    /// runs it, beside what clashes and what is locked.
    ///
    /// A read, as marking a box on the install screen is: nothing runs
    /// until the panel sends the tasks back, to be previewed and run as any
    /// task is.
    /// Each project a box names is an entry the workspace links, held to
    /// the path policy as a task's projects are
    /// ([`Admission::selection`]).
    Plan { selection: Vec<Cell> },
    /// What updating every project the workspace links would do ([`Updates`]):
    /// each installation an update says something of, how it stands, what
    /// an update does with it, and why it leaves one it leaves, every copy
    /// edited since left alone, as `mindweaver update --all` says without
    /// `--yes`.
    ///
    /// A query of its own, asked for by the page of updates, rather than a
    /// part of the snapshot: telling what an update does reads where each
    /// installation came from again, past what the snapshot reads, and a
    /// live update reads the snapshot again on every change. Nothing in it
    /// is sent by the browser, so nothing is held to the path policy.
    Updates {},
    /// What core says of `draft`, an artifact a command is about to write
    /// in `holder` ([`draft::Judged`]): why the command would write nothing, in its
    /// words, and what `validate` would say of it once written, in its own.
    ///
    /// How the panel's forms and editor judge what is typed as it is typed,
    /// by core's rules and never a copy of them in TypeScript: only a read,
    /// as a plan is, which writes nothing, and a draft judged fit is still
    /// held to the same rules by the line that writes it. The holder is held
    /// to the path policy as a read of an artifact's is.
    Validate { holder: Holder, draft: Draft },
    /// Every workspace the panel may switch to ([`known::switchable`]):
    /// those this machine knows and the one it was launched on, each with
    /// whether it is the one in use, the launch's, the rule it is chosen by
    /// once switched to, and whether a switch to it is offered; what
    /// [`Backend::switch`] is held to.
    ///
    /// A query of its own rather than a part of the snapshot: which one the
    /// panel was launched on is the launch's, and a snapshot says only what
    /// a workspace holds, the same whichever front end reads it. Nothing in
    /// it is sent by the browser.
    Workspaces {},
}

/// An artifact a command is about to write, as the panel asks core to judge
/// it ([`Query::Validate`]): tagged as a query is, `{"type": "add", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Draft {
    /// What `add` would write, by the fields an `add` task carries: a
    /// template by its name, as `--template` takes it, or for an MCP
    /// server how it is started or reached, as `add mcp`'s options say it.
    Add {
        kind: Kind,
        name: String,
        description: String,
        template: Option<String>,
        #[serde(default)]
        server: Option<mcp::Given>,
    },
    /// What `edit --contents` would write into the artifact `reference`
    /// names, `kind:name`, given `text` whole: its `SKILL.md`, or a rule's
    /// file.
    Edit { reference: String, text: String },
    /// What `edit` would write into the MCP server `name` told `server`,
    /// how it is started or reached, by the fields an `edit_server` task
    /// carries.
    EditServer { name: String, server: mcp::Given },
}

/// What a scan the panel asked for found ([`Query::Scan`]): what `mindweaver
/// scan --suggest --json` prints, each directory with what the panel needs
/// to offer it, and what the command warns of beside it.
///
/// A shape of the backend's rather than the envelope the command prints,
/// since what it adds to each directory is a front end's: the words the
/// command says why in, what marking it asks for, and the entry a relink
/// names. Each worked out here, in Rust, where the minds screen works out
/// the same, rather than by the panel from the scan's fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Scanned {
    /// Where it looked, each once, in the order given: the workspace's root
    /// when it was given none.
    pub roots: Vec<String>,
    /// How many levels below each root it looked.
    pub depth: usize,
    /// Every directory it found, in the order `mindweaver scan --suggest` ranks
    /// them: what is not linked first, most worth linking first, then what
    /// is.
    pub found: Vec<Suggestion>,
    /// Each entry where no project is any more whose id the ledger never
    /// learned, as the scan spells it: what `mindweaver relink <entry> <path>`
    /// follows, which only somebody who knows where it went can type.
    pub untraced: Vec<String>,
    /// Whether it stopped at core's bound before looking everywhere, which
    /// `warnings` says too.
    pub truncated: bool,
    /// What it could not read, and that it stopped early, as `mindweaver scan`
    /// warns of them, each with its code.
    pub warnings: Vec<Warning>,
}

/// One directory a scan found, and what the panel offers for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Suggestion {
    /// What `mindweaver scan --json` prints of it.
    pub found: Found,
    /// Why it is worth linking, in the words `mindweaver scan --suggest` and the
    /// minds screen say it in ([`worth_linking`]); none for one linked.
    pub why: Option<String>,
    /// What marking it asks for ([`MindsChange::of_found`]): linking it, or
    /// making it a project first. None for one linked, and for one found
    /// moved, which wants a relink rather than a second entry beside the one
    /// it left stale.
    pub offers: Option<MindsChange>,
    /// The entry it moved from, as the marker spells it, which a relink
    /// names: matched by core's `scan::moved_from`, as the minds screen
    /// matches it, since the scan spells `moved` as `mindweaver link` would
    /// take it. None when it did not move.
    pub moved_from: Option<String>,
}

impl Scanned {
    /// What `scan` found in `workspace`, as the panel is answered with it.
    fn of(workspace: &WeaverWorkspace, scan: &Scan) -> Scanned {
        let members = workspace.members();
        let left = |moved: &str| {
            members
                .iter()
                .find(|member| scan::moved_from(moved, workspace.root(), &member.root))
                .and_then(|member| paths::to_config_string(&member.entry))
        };
        let found = scan
            .found
            .iter()
            .map(|found| {
                let moved_from = found.moved.as_deref().and_then(left);
                Suggestion {
                    why: (!found.linked).then(|| {
                        worth_linking(
                            &found.route,
                            found.moved.as_deref(),
                            found.reason,
                            &found.agent_files,
                        )
                    }),
                    offers: (!found.linked && moved_from.is_none())
                        .then(|| MindsChange::of_found(found.mind)),
                    moved_from,
                    found: found.clone(),
                }
            })
            .collect();
        Scanned {
            roots: scan
                .roots
                .iter()
                .map(|root| root.to_string_lossy().into_owned())
                .collect(),
            depth: scan.depth,
            found,
            untraced: scan.untraced.clone(),
            truncated: scan.truncated,
            warnings: scan_warned(scan),
        }
    }
}

/// What a selection the panel asked to plan amounts to ([`Query::Plan`]):
/// core's plan of it ([`install::Plan`]), each change in it the task that
/// runs it.
///
/// Tasks rather than core's changes, so the panel sends back what it was
/// handed, never a task it puts together itself: the `install` and
/// `uninstall` tasks, each a project by the entry its box named it by, which
/// the backend turns into the folder the registry resolves as it does any
/// task's. Sent as one `apply_install`, as the install screen applies its
/// marks, each line runs whatever came of the ones before.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Planned {
    /// Each `install` and `uninstall` that carries the selection out, in
    /// the order to run them. None carries a yes: an MCP server from
    /// elsewhere than the library is only previewed by its line, until its
    /// confirmation (`asks`) is sent in its place.
    pub tasks: Vec<Task>,
    /// Boxes of one name in one project that cannot all be as asked, none
    /// of which is planned.
    pub conflicts: Vec<Conflict>,
    /// Boxes whose change is held back, each by the code the command would
    /// refuse it with, or why the project could not be asked about it.
    pub locked: Vec<Locked>,
    /// Each of `tasks` that waits for a yes ([`install::asks_first`]), with
    /// what it would give each agent of each project and the task carrying
    /// the yes, in the order of `tasks`.
    pub asks: Vec<Asking>,
}

/// An install of a plan that waits for somebody to say yes, an MCP server
/// from elsewhere than the library, as the install screen confirms one:
/// what it would give each agent of each project, laid out as its line
/// would say it ([`crate::install::foreseen`]), then the task that writes
/// it once that has been shown.
///
/// The task carrying the yes is handed over here, beside the lines it was
/// shown with, rather than the panel setting one on the task it holds:
/// which install waits for one is core's, and the yes is the
/// confirmation's alone. The panel sends it in the place of the task it
/// confirms, `task` by its place among the plan's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Asking {
    /// Which of the plan's tasks it confirms, by its place among them.
    pub task: usize,
    /// For each project it names, a line naming the install, then what
    /// each agent there would run or reach, and each warning, as the
    /// install screen's confirmation shows them.
    pub lines: Vec<String>,
    /// That task, saying yes: its line with `--yes`.
    pub confirmed: Task,
}

impl Planned {
    /// `plan`, each change the task that runs it, beside the confirmation
    /// of each that waits for a yes, previewed in `workspace` for the agent
    /// homes `homes`.
    fn of(workspace: &WeaverWorkspace, plan: install::Plan, homes: &AgentHomes) -> Planned {
        let mut asks = Vec::new();
        let mut tasks = Vec::new();
        for (at, change) in plan.changes.into_iter().enumerate() {
            if let Change::Install {
                projects,
                kind,
                name,
                from,
            } = &change
            {
                if install::asks_first(*kind, from) {
                    asks.push(Asking {
                        task: at,
                        lines: foreseen_in(workspace, projects, *kind, name, from, homes),
                        confirmed: Task::confirmed(change.clone()),
                    });
                }
            }
            tasks.push(Task::from(change));
        }
        Planned {
            tasks,
            conflicts: plan.conflicts,
            locked: plan.locked,
            asks,
        }
    }
}

/// What installing `kind` `name` from `from` would give each agent of
/// each of `projects`, by their entries, as the install screen's
/// confirmation says it: a heading for each project, then its lines. A
/// project or an offer that cannot be read says why under its heading, as
/// its line would refuse it.
fn foreseen_in(
    workspace: &WeaverWorkspace,
    projects: &[String],
    kind: Kind,
    name: &ArtifactName,
    from: &Origin,
    homes: &AgentHomes,
) -> Vec<String> {
    let ledger = match workspace.ledger_for_reading() {
        Ok(ledger) => ledger,
        Err(error) => return vec![format!("  error: {error}")],
    };
    let reference = Reference::parse(&crate::task::qualified(kind, name));
    let mut lines = Vec::new();
    for entry in projects {
        let project = policy::linked(workspace, entry)
            .map_err(|error| error.to_string())
            .and_then(|member| member.project.map_err(|error| error.to_string()));
        let shown = project
            .as_ref()
            .map_or(entry.as_str(), |project| project.name());
        lines.push(crate::install::heading(kind, name, shown, from));
        let candidate = project.and_then(|project| {
            install::offered_typed(
                workspace,
                &ledger,
                &project,
                &reference,
                Some(from),
                workspace.root(),
            )
            .map(|candidate| (project, candidate))
            .map_err(|error| error.to_string())
        });
        match candidate {
            Ok((project, candidate)) => lines.extend(crate::install::foreseen(
                workspace, &ledger, &project, &candidate, homes,
            )),
            Err(error) => lines.push(format!("  error: {error}")),
        }
    }
    lines
}

/// What running a task's lines produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Outcome {
    /// Whether every line ran and found everything in order.
    pub ok: bool,
    /// Each line that ran, in order: up to the first that failed, for a task
    /// whose lines lean on each other ([`Task::chained`]).
    pub runs: Vec<Run>,
}

/// What one command line produced, as a terminal would show it, with the
/// code of everything that went wrong beside its words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Run {
    /// The line, as it would be typed into a shell ([`shell`]).
    pub command: String,
    pub stdout: String,
    /// Each line it said on stderr, with its code; none for a hint or a
    /// note, which no code is given to.
    pub stderr: Vec<Warning>,
    /// Why it could not run, when it could not.
    pub error: Option<Said>,
    /// Whether it ran and found everything in order: what its exit status
    /// would have said.
    pub ok: bool,
}

/// One thing a line of a task takes away, which the panel has somebody
/// confirm before it asks for the run: an item of a preview's `confirm`,
/// one for each [`Caution`] of each line ([`cautioned`]).
///
/// The words beside the word, so the panel says what a line takes in the
/// CLI's words rather than its own, and draws by the word alone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Confirm {
    /// Which of the preview's lines, by its place among them, from 0.
    pub line: usize,
    /// What it takes away.
    pub caution: Caution,
    /// The same, said for whoever is asked.
    pub message: String,
}

/// What the panel asks to switch to: a workspace by its path, as the
/// answer to [`Query::Workspaces`] lists it, `{"path": "/home/you/work"}`.
///
/// A path matched against that list, read again when the switch arrives,
/// and never opened as sent ([`policy::switched`]): a browser goes to no
/// workspace that list does not offer. A field it does not have is
/// refused, as a task's is.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Switch {
    pub path: String,
}

/// The workspace a backend acts on, and the rule it is chosen by.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pin {
    /// Its root: where every line runs from, and what each names.
    root: PathBuf,
    /// Why it is the one, which the snapshot says.
    reason: Reason,
}

impl Pin {
    /// The workspace, as the disk has it now: the default one as it will
    /// be, while nothing has made it yet.
    ///
    /// Only the default one: any other was there when it was pinned, and
    /// one gone since is refused rather than taken for one to make, which
    /// the first change would do to it.
    fn workspace(&self) -> Result<WeaverWorkspace, Refusal> {
        let made = WeaverWorkspace::is_marked(&self.root);
        if made || self.reason != Reason::Home {
            WeaverWorkspace::open(&self.root)
        } else {
            WeaverWorkspace::unmade(&self.root)
        }
        .map_err(|error| refused(error.code(), error))
    }

    /// The workspace, or, when it will not open now, gone or broken since
    /// it was pinned, as it would be once made again: what is watched, and
    /// what the panel may switch away from, either way.
    fn opened_or_not(&self) -> Result<WeaverWorkspace, Refusal> {
        self.workspace()
            .or_else(|refusal| WeaverWorkspace::unmade(&self.root).map_err(|_| refusal))
    }
}

/// Where a backend says how long each line it ran took, a line of text at a
/// time: the front end's, as the server's log is, which decides where it
/// goes ([`PanelBackend::logging`]).
pub type Log = Arc<dyn Fn(&str) + Send + Sync>;

/// The backend over the workspace a panel was launched on, or switched to
/// since.
#[derive(Clone)]
pub struct PanelBackend {
    /// The workspace the launch resolved, and how: what the panel may
    /// always switch back to, by the rule that chose it.
    launch: Pin,
    /// The workspace it acts on now: the launch's, until a switch
    /// ([`Backend::switch`]). Shared by its clones, and read once by
    /// each query, preview and run, so none of them meets two.
    pin: Arc<RwLock<Pin>>,
    /// Where to look for workspaces, as the launch decided it: its home and
    /// agent homes, which every line runs with, and the home whose default
    /// workspace knows what may be switched to.
    lookup: Lookup,
    /// Every line it ran for a task that changes something, in the order
    /// they ran ([`transcript`](Self::transcript)). Shared by its clones.
    changed: Arc<Mutex<Vec<Run>>>,
    /// Where it says how long each line took, if anywhere.
    log: Option<Log>,
}

/// By hand, for the log, which is a function: whether there is one.
impl std::fmt::Debug for PanelBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanelBackend")
            .field("launch", &self.launch)
            .field("pin", &self.pin)
            .field("lookup", &self.lookup)
            .field("changed", &self.changed)
            .field("log", &self.log.is_some())
            .finish()
    }
}

impl PanelBackend {
    /// The backend for the workspace `lookup` resolves from `directory`,
    /// resolved as a read resolves it, here and once: the default one in
    /// home as it will be, when nothing has made it yet, which the first
    /// change makes, as from the TUI.
    pub fn launch(directory: &Path, lookup: &Lookup) -> Result<PanelBackend, CliError> {
        let resolved = resolve(&lookup.inputs(directory, Intent::Read))?
            .ok_or_else(|| CliError::NotInWorkspace(directory.to_path_buf()))?;
        let launch = Pin {
            root: resolved.workspace.root().to_path_buf(),
            reason: resolved.reason,
        };
        Ok(PanelBackend {
            pin: Arc::new(RwLock::new(launch.clone())),
            launch,
            lookup: lookup.clone(),
            changed: Arc::default(),
            log: None,
        })
    }

    /// Say in `log`, once each line it runs has run, how long it took and
    /// how much of that it waited for a workspace's lock another writer
    /// held, and which process held it: `run: line 1 of 1 took 5402ms, 0ms
    /// of it waiting for a workspace's lock`.
    ///
    /// What the server's own log cannot say, since it knows a task and
    /// not its lines, nor what core waited for running one: a run that
    /// took seconds is told apart there from one that waited its turn on
    /// the writer thread, and here from one that waited for another writer
    /// (#240). By its place among the task's lines rather than its words,
    /// which are the request's, as the server logs no body.
    pub fn logging(mut self, log: Log) -> Self {
        self.log = Some(log);
        self
    }

    /// Every line run so far for a task that changes something, with what
    /// each said, as a shell session would show it: what `mindweaver ui` prints
    /// once it stops, as the TUI prints its own when it closes, so what the
    /// panel did outlives the server and can be done again without it.
    ///
    /// The TUI's transcript, laid out by the TUI's function, and kept by the
    /// TUI's rule ([`Task::writes`]): a read changed nothing there is to do
    /// again. Kept by every clone of the backend alike, so one kept back
    /// while the server holds another reads what the server's ran.
    pub fn transcript(&self) -> String {
        let changed = self.changed.lock().unwrap_or_else(PoisonError::into_inner);
        transcript(&changed.iter().map(Done::from).collect::<Vec<_>>())
    }

    /// The root of the workspace it acts on now.
    pub fn root(&self) -> PathBuf {
        self.pin().root
    }

    /// The workspace it acts on now, and why.
    fn pin(&self) -> Pin {
        self.pin
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Where it was launched, as core's list of what it may switch to
    /// takes it.
    fn launched(&self) -> Launch<'_> {
        Launch {
            root: &self.launch.root,
            reason: self.launch.reason,
        }
    }

    /// What every line runs with: the launch's home and agent homes, and the
    /// workspace named, so each finds it and nothing but it, once it is
    /// made. Before then the default is named by nothing, and found as the
    /// TUI's lines find it, walking up from its root; its first change makes
    /// it first ([`plan`](Self::plan)).
    fn pinned(&self, workspace: &WeaverWorkspace) -> Lookup {
        Lookup {
            flag: Some(if workspace.is_made() {
                workspace.root().to_path_buf()
            } else {
                PathBuf::new()
            }),
            activated: None,
            ..self.lookup.clone()
        }
    }

    /// The lines `task` is in the workspace `pin` names, as they would run
    /// against the disk as it is now, with what they run with and whether
    /// each leans on the one before; or why none of them runs.
    fn plan(&self, task: &Task, pin: &Pin) -> Result<Plan, Refusal> {
        if let Some((line, refusal)) = task.refused_line() {
            return Err(refused(
                refusal.code(),
                format!("`{}` {refusal}", shell(&line)),
            ));
        }
        let workspace = pin.workspace()?;
        let lookup = self.pinned(&workspace);
        // With what the lines run with, so a project is asked which
        // workspace it consults as its line will ask it: this one, named.
        let admitted = Admission {
            workspace: &workspace,
            lookup: &lookup,
        }
        .task(task)
        .map_err(|error| refused(error.code(), error))?;
        let named = |lines: Vec<Vec<String>>| match workspace.root().to_str() {
            Some(root) => lines.iter().map(|line| task::naming(line, root)).collect(),
            None => lines,
        };
        let lines = admitted.commands_in(&workspace);
        // As the TUI does: a change to the default before it is made makes
        // it first, by a line of its own, rather than leave the change to
        // find a workspace above home while it walks up from the root. Not
        // in front of a task that makes it already: the TUI offers that one
        // only with nothing made, but a browser may send it at any time.
        //
        // Every line names the workspace with `-W`, so what the panel shows,
        // and the transcript keeps, says which workspace each reached, and
        // reaches it pasted anywhere. All but those before it is made: `-W`
        // refuses a folder with no marker, so the default is named by the
        // lines after the `mindweaver init` that makes it, and a read of it
        // before then by none, as the TUI's lines find it. And all of a
        // workspace whose root no word of a line can hold, not UTF-8: its
        // lines run against it all the same, through the lookup, which
        // holds the path as it is, as they did before any line named it,
        // rather than every task on it refused for a word only shown. It is
        // only ever the launch's, since nothing whose path is not text is
        // offered as a switch.
        let init = Task::InitWorkspace.commands();
        let lines = if workspace.is_made() {
            named(lines)
        } else if admitted.writes() && !lines.is_empty() && !lines.starts_with(&init) {
            init.into_iter().chain(named(lines)).collect()
        } else {
            lines
        };
        Ok(Plan {
            lines,
            root: pin.root.clone(),
            lookup,
            chained: admitted.chained(),
            writes: admitted.writes(),
        })
    }
}

/// What a task runs: its lines, where they run from and with what, whether
/// one failing stops the rest, and whether they change something.
struct Plan {
    lines: Vec<Vec<String>>,
    root: PathBuf,
    lookup: Lookup,
    chained: bool,
    writes: bool,
}

impl Backend for PanelBackend {
    type Query = Query;
    type Task = Task;
    type Outcome = Outcome;
    type Switch = Switch;

    fn query(&self, query: Query) -> Result<serde_json::Value, Refusal> {
        let pin = self.pin();
        // What may be switched to is answered even when the workspace in
        // use will not open, gone since it was switched to, so the panel
        // can still leave it.
        let workspace = match query {
            Query::Workspaces {} => pin.opened_or_not(),
            _ => pin.workspace(),
        }?;
        // Each is strings, numbers and lists of them, and maps keyed by
        // field names, with every path written as text whatever its bytes:
        // nothing in either can fail to serialize.
        let snapshot = |workspace| {
            let resolved = Resolved {
                workspace,
                reason: pin.reason,
            };
            Snapshot::of(
                &resolved,
                self.lookup.home.as_deref(),
                &self.lookup.machine(),
            )
        };
        match query {
            Query::Snapshot {} => {
                Ok(serde_json::to_value(snapshot(workspace)).expect("a snapshot serializes"))
            }
            // Each the snapshot's own part, read whole and cut, so it says
            // what the snapshot says to the letter.
            Query::Projects {} => {
                Ok(serde_json::to_value(snapshot(workspace).projects)
                    .expect("a snapshot serializes"))
            }
            Query::Library {} => {
                Ok(serde_json::to_value(snapshot(workspace).library)
                    .expect("a snapshot serializes"))
            }
            Query::Project { entry } => {
                let project = snapshot(workspace)
                    .project(&entry)
                    .map_err(|error| named(error.code(), error))?;
                Ok(serde_json::to_value(project).expect("a snapshot serializes"))
            }
            Query::Files { holder, reference } => {
                let files = match linked(&workspace, holder)? {
                    None => Files::of_library(&workspace, &reference),
                    Some(project) => Files::of_project(&project, &reference),
                }
                .map_err(|error| named(error.code(), error))?;
                Ok(serde_json::to_value(files).expect("a listing serializes"))
            }
            Query::Contents {
                holder,
                reference,
                file,
            } => {
                let contents = match linked(&workspace, holder)? {
                    None => Contents::of_library(&workspace, &reference, &file),
                    Some(project) => Contents::of_project(&project, &reference, &file),
                }
                .map_err(|error| named(error.code(), error))?;
                Ok(serde_json::to_value(contents).expect("contents serialize"))
            }
            Query::Scan { roots, depth } => {
                let roots = roots
                    .iter()
                    .map(|root| policy::path(root))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| refused(error.code(), error))?;
                let request = scan::Request {
                    roots,
                    depth: depth.unwrap_or(DEFAULT_DEPTH),
                    from: None,
                    order: Order::Suggested,
                };
                let scanned = Scanned::of(&workspace, &scan::scan(&workspace, &request));
                Ok(serde_json::to_value(scanned).expect("a scan serializes"))
            }
            Query::Folders { path, hidden } => {
                let folder = match path {
                    Some(path) => {
                        policy::path(&path).map_err(|error| refused(error.code(), error))?
                    }
                    None => workspace.root().to_path_buf(),
                };
                let folders =
                    Folders::read(&workspace, &folder, hidden, self.lookup.home.as_deref())
                        .map_err(|error| named(error.code(), error))?;
                Ok(serde_json::to_value(folders).expect("a listing serializes"))
            }
            Query::Plan { selection } => {
                Admission {
                    workspace: &workspace,
                    lookup: &self.lookup,
                }
                .selection(&selection)
                .map_err(|error| refused(error.code(), error))?;
                let plan = install::plan(&workspace, &selection)
                    .map_err(|error| refused(error.code(), error))?;
                let planned = Planned::of(&workspace, plan, &self.lookup.agents);
                Ok(serde_json::to_value(planned).expect("a plan serializes"))
            }
            Query::Updates {} => {
                let updates =
                    Updates::read(&workspace).map_err(|error| refused(error.code(), error))?;
                Ok(serde_json::to_value(updates).expect("what an update would do serializes"))
            }
            // The holder a line would change, by the workspace on screen,
            // which a line of the panel's names as it runs, so the templates
            // offered and the records consulted are that one's.
            Query::Validate { holder, draft } => {
                // Named in a body, which is no part of the route's address,
                // so a holder that is not there is a 422, as a task's is.
                let holder = linked(&workspace, holder)
                    .map_err(|refusal| Refusal {
                        grounds: Grounds::Unfit,
                        ..refusal
                    })?
                    .unwrap_or_else(|| workspace.own());
                let judged = match draft {
                    Draft::Add {
                        kind,
                        name,
                        description,
                        template,
                        server,
                    } => draft::added(
                        Some(&workspace),
                        &holder,
                        kind,
                        (&name, &description),
                        template.as_deref(),
                        server.as_ref(),
                    ),
                    Draft::EditServer { name, server } => draft::retransported(
                        Some(&workspace),
                        &holder,
                        &Reference::parse(&crate::task::qualified(Kind::Mcp, &name)),
                        &server,
                    ),
                    Draft::Edit { reference, text } => draft::edited(
                        Some(&workspace),
                        &holder,
                        &Reference::parse(&reference),
                        &text,
                    ),
                };
                Ok(serde_json::to_value(judged).expect("a judgement serializes"))
            }
            Query::Workspaces {} => {
                let listed =
                    known::switchable(self.lookup.home.as_deref(), &self.launched(), &workspace);
                Ok(serde_json::to_value(listed).expect("a list serializes"))
            }
        }
    }

    /// The panel's reads, and the snapshot's parts, each the query the panel
    /// sends for it ([`routes::table`]).
    fn routes() -> Vec<Route<Query>> {
        routes::table()
    }

    /// The document `make api/openapi` writes from the very tables served
    /// and the schemas, under the `schema` feature (`openapi::document`),
    /// committed, and embedded as those bytes: so a plain build serves it
    /// without compiling schemars, and what it serves is what is
    /// committed, which `make api/openapi/check` holds to what Rust writes.
    fn document() -> &'static str {
        include_str!("../../../docs/api/openapi.json")
    }

    /// Each line as a shell would read it, and what the panel has somebody
    /// confirm first, both of the one plan: so what the panel asks about is
    /// the lines it shows, a `mindweaver init` in front of them counted.
    fn preview(&self, task: &Task) -> Result<Preview, Refusal> {
        let plan = self.plan(task, &self.pin())?;
        let confirm = cautioned(&plan.lines)
            .into_iter()
            .map(|(line, caution)| Confirm {
                line,
                caution,
                message: caution.to_string(),
            })
            // A number, a word and its words: nothing to fail on.
            .map(|confirm| serde_json::to_value(confirm).expect("a confirm serializes"))
            .collect();
        Ok(Preview {
            lines: plan.lines.iter().map(|line| shell(line)).collect(),
            confirm,
            // A run takes a cancel when one of its lines stops when asked:
            // the list every front end follows by, read off the lines
            // planned, a `mindweaver init` in front of them counted.
            cancellable: plan.lines.iter().any(|line| followed(line).is_some()),
        })
    }

    /// Planned again rather than trusted to the preview the server asked
    /// for first: the plan is of the disk as the run meets it, and the
    /// trait promises no preview came first.
    ///
    /// A save's text is put where its line reads it before the line runs,
    /// and taken away once the lines have run, whatever came of them
    /// ([`Scratch`]): the browser sends the text, never a path.
    ///
    /// Every line runs with one watch, as the TUI's lines do: a gather among
    /// them reports each step to `underway` as a [`progress::Step`], paced
    /// ([`progress::reported`]), and stops when the server's cancel reaches
    /// it, which is said before any line runs, so a cancel sent as soon as
    /// the run is told started is not lost. A line that is no gather reads
    /// neither.
    fn run(&self, task: Task, underway: &Underway) -> Result<Outcome, Refusal> {
        let pin = self.pin();
        let plan = self.plan(&task, &pin)?;
        let _scratch = match &task {
            Task::Save { text, .. } => Some(
                Scratch::put(&pin.workspace()?, text)
                    .map_err(|error| refused(error.code(), error))?,
            ),
            _ => None,
        };
        let following = underway.clone();
        let watch = progress::reported(move |step| {
            // Words, numbers and a flag: nothing to fail on.
            following.report(serde_json::to_value(step).expect("a step serializes"));
        });
        underway.on_cancel({
            let watch = watch.clone();
            move || watch.cancel()
        });
        let mut runs: Vec<Run> = Vec::new();
        for (index, words) in plan.lines.iter().enumerate() {
            let started = Instant::now();
            // Every refusal of the list, where the TUI refuses only a
            // screen: the plan refused them already, and a line that is one
            // all the same is not run here either.
            let (line, waited) =
                lock::waits(|| run_line(words, &plan.root, &plan.lookup, &watch, |_| true));
            if let Some(log) = &self.log {
                log(&timing(
                    index + 1,
                    plan.lines.len(),
                    started.elapsed(),
                    waited,
                ));
            }
            let run = ran(words, line);
            let failed = run.error.is_some();
            runs.push(run);
            if failed && plan.chained {
                break;
            }
        }
        if plan.writes {
            self.changed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .extend(runs.iter().cloned());
        }
        Ok(Outcome {
            ok: runs.iter().all(|run| run.ok),
            runs,
        })
    }

    /// Switch to the workspace `to` names, from now on: one the answer to
    /// [`Query::Workspaces`] lists, as it is listed when the switch arrives,
    /// opened and never made ([`policy::switched`]); and answer with its
    /// row as that list then has it. Nothing is switched when it is
    /// refused.
    ///
    /// What every query, preview and run after it acts on, and what each
    /// line they run names with `-W`: the launch's home, agent homes and
    /// all else stay as the launch decided them. The rule it is chosen by
    /// is the list's, the launch's for the launch's and `-W` for every
    /// other, which the snapshot then says.
    fn switch(&self, to: Switch) -> Result<serde_json::Value, Refusal> {
        let in_use = self.pin().opened_or_not()?;
        let listed = known::switchable(self.lookup.home.as_deref(), &self.launched(), &in_use);
        let (workspace, reason) =
            policy::switched(&listed, &to.path).map_err(|error| refused(error.code(), error))?;
        *self.pin.write().unwrap_or_else(PoisonError::into_inner) = Pin {
            root: workspace.root().to_path_buf(),
            reason,
        };
        let row = known::switchable(self.lookup.home.as_deref(), &self.launched(), &workspace)
            .into_iter()
            .find(|row| row.workspace.in_use);
        Ok(serde_json::to_value(row).expect("a row serializes"))
    }

    /// Every path core names for a snapshot of the workspace
    /// ([`watch::watched`]), with the home the snapshot is read with, each
    /// scope as the holder a query names: `{"type": "workspace"}`, or
    /// `{"type": "project", "entry": ...}` by the entry the snapshot lists.
    ///
    /// Of the workspace it acts on now, so a switch drops the paths of the
    /// one before. One that will not open now, gone or broken since it was
    /// pinned, is watched where its marker and ledger would be, so its
    /// coming back is seen.
    fn watched_paths(&self) -> Vec<Watched> {
        let Ok(workspace) = self.pin().opened_or_not() else {
            return Vec::new();
        };
        watch::watched(&workspace, self.lookup.home.as_deref(), &self.lookup.agents)
            .into_iter()
            .map(|watched| Watched {
                path: watched.path,
                scope: scope(watched.scope),
            })
            .collect()
    }
}

/// A save's text, in the file its line reads it from
/// ([`task::scratch_route`]): the workspace's scratch folder as core has it
/// ([`WeaverWorkspace::scratch_dir`]), the file named by the text
/// ([`task::scratch_name`]). Held for as long as this is: taken away when
/// it is dropped, whatever came of the line.
///
/// Made with `create_new`, in a folder made with `create_dir`, never
/// `create_dir_all`, so no `.mindweaver` is made where there is none, which
/// a line of the panel's would then take for a workspace. The folder is
/// looked at once it is there, and a link in its place refuses the save:
/// `create_dir` says a link is there as it says a folder is, and
/// `create_new` refuses a link only as the file itself, following one in
/// the folder's place, which a cloned workspace can bring to an agent's own
/// folder. A file already there under the name is taken as it is when it
/// holds the text exactly, as one an interrupted run left would, the name
/// being the text's hash; anything else there refuses the save, and is left
/// as it is.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// `text`, put where the line saving it reads it from, in `workspace`.
    fn put(workspace: &WeaverWorkspace, text: &str) -> Result<Scratch, CliError> {
        let folder = workspace.scratch_dir();
        let path = folder.join(task::scratch_name(text));
        let unwritable = |source| CliError::Scratch {
            path: path.clone(),
            source,
        };
        match fs::create_dir(&folder) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => {
                return Err(unwritable(error))
            }
            _ => {}
        }
        if !fs::symlink_metadata(&folder).is_ok_and(|found| found.is_dir()) {
            return Err(unwritable(io::Error::other(
                "its folder is a link or no folder, and nothing is written through one",
            )));
        }
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                // Held from here, so a write that fails halfway takes away
                // what it wrote.
                let scratch = Scratch { path: path.clone() };
                file.write_all(text.as_bytes())
                    .and_then(|()| file.sync_all())
                    .map_err(unwritable)?;
                Ok(scratch)
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let held = fs::symlink_metadata(&path).is_ok_and(|found| found.is_file())
                    && fs::read(&path).is_ok_and(|held| held == text.as_bytes());
                if held {
                    Ok(Scratch { path })
                } else {
                    Err(unwritable(error))
                }
            }
            Err(error) => Err(unwritable(error)),
        }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// What a watched path is of, as a query names the holder it reads from.
fn scope(scope: watch::Scope) -> serde_json::Value {
    let holder = match scope {
        watch::Scope::Workspace => Holder::Workspace,
        watch::Scope::Project(entry) => Holder::Project(entry),
    };
    serde_json::to_value(holder).expect("a holder serializes")
}

/// The project a query's `holder` names by its entry in `workspace`, or none
/// for the workspace's own: a read of an artifact is held to one or the
/// other, as a task's holder is ([`policy::linked`]). An entry no registry
/// holds, or one whose folder holds no project any more, is refused as
/// absent, as a read's parameter names it.
fn linked(workspace: &WeaverWorkspace, holder: Holder) -> Result<Option<MindProject>, Refusal> {
    match holder {
        Holder::Workspace => Ok(None),
        Holder::Project(entry) | Holder::Elsewhere(entry) => {
            let member =
                policy::linked(workspace, &entry).map_err(|error| named(error.code(), error))?;
            member
                .project
                .map(Some)
                .map_err(|error| named(error.code(), error))
        }
    }
}

/// What the log says of the `place`th of a task's `lines`, which took
/// `took`, of it `waited` for a workspace's lock: `run: line 1 of 1 took
/// 5402ms, 0ms of it waiting for a workspace's lock`, and the process
/// that held it when one did and said so.
fn timing(place: usize, lines: usize, took: Duration, waited: Waited) -> String {
    let said = format!(
        "run: line {place} of {lines} took {}ms, {}ms of it waiting for a workspace's lock",
        took.as_millis(),
        waited.total.as_millis()
    );
    match waited.holder {
        Some(pid) if !waited.total.is_zero() => format!("{said}, which process {pid} held"),
        _ => said,
    }
}

/// What came of `words`, as a run says it.
fn ran(words: &[String], ran: Ran) -> Run {
    let command = shell(words);
    match ran {
        Ran::Refused(refusal) => Run {
            command,
            stdout: String::new(),
            stderr: Vec::new(),
            error: Some(Said {
                code: refusal.code(),
                message: Redacted::new(&refusal.to_string()),
            }),
            ok: false,
        },
        Ran::Done(Ok(outcome)) => Run {
            command,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            error: None,
            ok: outcome.ok,
        },
        Ran::Done(Err(failure)) => Run {
            command,
            stdout: String::new(),
            stderr: failure.warnings,
            error: Some(Said {
                code: failure.error.code(),
                message: Redacted::new(&failure.error.to_string()),
            }),
            ok: false,
        },
    }
}

/// A line the panel ran, as the TUI keeps one it ran: its words for each
/// line of stderr and for why it could not run, without the codes, which a
/// transcript a person reads has no use for.
impl From<&Run> for Done {
    fn from(run: &Run) -> Self {
        Done {
            command: run.command.clone(),
            stdout: run.stdout.clone(),
            stderr: run.stderr.iter().map(|line| line.text.clone()).collect(),
            error: run.error.as_ref().map(|error| error.message.to_string()),
            ok: run.ok,
        }
    }
}

/// A refusal of `code`, saying `message`, of a request that is not one to
/// act on: a 422.
fn refused(code: &'static str, message: impl Display) -> Refusal {
    Refusal::unfit(code, message.to_string())
}

/// A refusal of `code`, saying what `error` says, of a read whose
/// parameters name what `error` is about: absent, a 404, when `error` says
/// that is not there, and unfit, a 422, for anything else.
fn named<E: Absence + Display>(code: &'static str, error: E) -> Refusal {
    if error.absent() {
        Refusal::absent(code, error.to_string())
    } else {
        Refusal::unfit(code, error.to_string())
    }
}

/// Whether an error core gave of what a read's parameters named says it is
/// not there: what tells a 404 from a 422 ([`Grounds`]).
///
/// Asked only of a read's parameters, a GET's query string, which is part
/// of the address of what it reads, so a value naming nothing makes the
/// address name nothing, as a path no route serves does. What a body names
/// is no part of the address, whose route is there: a task, a switch, a
/// plan or a draft naming an entry no registry holds is a request not to act
/// on, a 422 with the same code; and so is a refusal of a request that
/// names nothing at all, a workspace in use that will not open among them.
///
/// Decided here, where each refusal is built, rather than by the server
/// from the code, which would put a rule of the front end's in a crate that
/// knows none; and as a match with no wildcard for each error, so a
/// variant core adds is decided before it compiles. Not core's either: a
/// status is HTTP's, and a front end without a server has no use for one.
trait Absence {
    fn absent(&self) -> bool;
}

/// Something named that is not what it was named as: a folder with no
/// project or no workspace in it, an entry the registry or the loads do
/// not hold, a workspace this machine does not know.
impl Absence for WorkspaceError {
    fn absent(&self) -> bool {
        match self {
            WorkspaceError::NotAProject { .. }
            | WorkspaceError::NotAWorkspace { .. }
            | WorkspaceError::NotRegistered { .. }
            | WorkspaceError::NotLoaded { .. }
            | WorkspaceError::NotKnown { .. } => true,
            WorkspaceError::Create { .. }
            | WorkspaceError::Read { .. }
            | WorkspaceError::Parse { .. }
            | WorkspaceError::Malformed { .. }
            | WorkspaceError::Version { .. }
            | WorkspaceError::Edit { .. }
            | WorkspaceError::LoadsItself { .. }
            | WorkspaceError::NonUtf8Path { .. }
            | WorkspaceError::OutsideProject { .. }
            | WorkspaceError::InsideMind { .. }
            | WorkspaceError::NoDirectory { .. }
            | WorkspaceError::Target { .. }
            | WorkspaceError::NoSettings { .. }
            | WorkspaceError::OutsideWorkspace { .. }
            | WorkspaceError::OverlapsGathered { .. }
            | WorkspaceError::Lock(_) => false,
        }
    }
}

/// A path is refused for how it is spelled or where it leads, which is no
/// absence; an entry, for not being in the registry, which is.
impl Absence for PolicyError {
    fn absent(&self) -> bool {
        match self {
            PolicyError::Workspace(error) => error.absent(),
            PolicyError::Url { .. }
            | PolicyError::Relative { .. }
            | PolicyError::Unnormalized { .. }
            | PolicyError::InWeaverDir { .. }
            | PolicyError::Unlisted { .. }
            | PolicyError::YesUnasked { .. }
            | PolicyError::Server(_) => false,
        }
    }
}

/// Nothing by the name asked for: absent. One name for several, or one not
/// to be changed from here, is there and still refused.
impl Absence for LifecycleError {
    fn absent(&self) -> bool {
        match self {
            LifecycleError::NotFound { .. } => true,
            LifecycleError::Workspace(error) => error.absent(),
            LifecycleError::Received { .. }
            | LifecycleError::ReceivedServer { .. }
            | LifecycleError::NotAuthored { .. }
            | LifecycleError::Ambiguous { .. }
            | LifecycleError::Name { .. }
            | LifecycleError::Nested { .. }
            | LifecycleError::McpNested { .. }
            | LifecycleError::Unfit { .. }
            | LifecycleError::Unchanged { .. }
            | LifecycleError::Exists { .. }
            | LifecycleError::Unreadable { .. }
            | LifecycleError::DefinitionUnreadable { .. }
            | LifecycleError::Rewrite { .. }
            | LifecycleError::Outside { .. }
            | LifecycleError::NotText { .. }
            | LifecycleError::Unparsed { .. }
            | LifecycleError::NameChanged { .. }
            | LifecycleError::DefinitionUnparsed { .. }
            | LifecycleError::McpNameChanged { .. }
            | LifecycleError::ContentsUnfit { .. }
            | LifecycleError::HashMalformed { .. }
            | LifecycleError::Stale { .. }
            | LifecycleError::LeadsOut { .. }
            | LifecycleError::Io { .. }
            | LifecycleError::TransportNotMcp { .. }
            | LifecycleError::ServerUnreadable { .. }
            | LifecycleError::Server(_)
            | LifecycleError::Artifact(_) => false,
        }
    }
}

/// No file at the route asked for, or something there that is no file:
/// absent, as a file named is. A link leading out, or a file that is there
/// and cannot be read, is refused for what it is.
impl Absence for ContentsError {
    fn absent(&self) -> bool {
        match self {
            ContentsError::Missing { .. } | ContentsError::NotAFile { .. } => true,
            ContentsError::Find(error) => error.absent(),
            ContentsError::Route { .. }
            | ContentsError::Outside { .. }
            | ContentsError::NameUnfit { .. }
            | ContentsError::Cycle { .. }
            | ContentsError::Unreadable { .. } => false,
        }
    }
}

/// No folder at the path asked for, or something there that is no folder:
/// absent. One that is there and cannot be listed is not.
impl Absence for BrowseError {
    fn absent(&self) -> bool {
        match self {
            BrowseError::Missing { .. } | BrowseError::NotAFolder { .. } => true,
            BrowseError::Unlisted { .. } => false,
        }
    }
}
