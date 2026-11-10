//! What the TUI knows, and what a key does to it.
//!
//! Kept apart from the terminal and the drawing, as the install screen it
//! embeds is, so a test drives it the way a person does. Nothing here touches
//! a file. A key that asks for work returns a [`Task`], every front end's
//! ([`crate::task`]), and a task is a command line: [`Task::commands`] is
//! exactly what the caller parses, with the parsers `mind` and `mindweaver` use,
//! and runs. There is nothing this screen can do that a command cannot.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;

use crate::progress::{Progress, Watch};
use mindweaver_core::create;
use mindweaver_core::finder::Hit;
use mindweaver_core::mcp_registry::Summary;
use mindweaver_core::scan;
use mindweaver_core::view::ShelfSource;
use mindweaver_core::{
    paths, Agent, Kind, Layout, MarkerFile, Reason, Share, State, DEFAULT_SUBDIRECTORY,
};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::minds::{self, Minds};
use crate::install::state::{Screen as InstallScreen, Step as InstallStep};
use crate::render::counted_kind;
use crate::task::qualified;
// Re-exported where the screens, and their tests, have always found them: a
// key still asks for a task, though a task is every front end's now.
pub use crate::task::{shell, AgentsLine, Holder, Task, Toggles};
use crate::Lookup;

/// How far a page key scrolls a page of output.
const PAGE: usize = 10;

// ---------------------------------------------------------------------------
// What is on screen
// ---------------------------------------------------------------------------

/// What the TUI was opened on.
///
/// It decides what the TUI offers to make when there is nothing yet, and at
/// which level its keys act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `mindweaver`: a workspace, its own artifacts, and the projects it manages.
    Workspace,
    /// `mind`, on a project no workspace manages: that project on its own.
    Project,
}

/// What the TUI is looking at.
#[derive(Debug, Clone)]
pub enum View {
    /// Nothing yet where the TUI was opened: no workspace for `mindweaver`, no
    /// project for `mind`.
    Nowhere,
    /// A workspace and everything it holds, or one project on its own.
    /// Boxed, since `Nowhere` holds nothing and a view is moved whole.
    Workspace(Box<Workspace>),
}

/// A workspace as the screen shows it, or a project on its own.
#[derive(Debug, Clone)]
pub struct Workspace {
    pub name: String,
    pub root: PathBuf,
    /// The workspace's own artifacts first, then every registered entry in
    /// the order the marker lists them. On its own, a project is the one.
    pub members: Vec<Member>,
    /// What sits under the root, or under the folders `mindweaver minds` was
    /// given, and could be linked, but is not, most worth linking first, in
    /// the order `mindweaver scan --suggest` gives.
    pub unlinked: Vec<Unlinked>,
    /// Where `unlinked` was looked for: the folders `mindweaver minds` was
    /// given, none meaning the root.
    pub roots: Vec<PathBuf>,
    /// What the look for `unlinked` could not read, and that it stopped
    /// early, in the words `mindweaver scan` warns with: a folder `mindweaver minds`
    /// was given that is not there among them, which would otherwise be an
    /// empty screen with nothing to say why.
    pub scan_warnings: Vec<String>,
    /// How many artifacts are on the shelf.
    pub shelf: usize,
    /// The sources the shelf is filled from, as the snapshot lists them
    /// (`mindweaver gather sources`): what the sources screen picks from.
    pub sources: Vec<ShelfSource>,
    /// The workspaces it loads, in marker order.
    pub loaded: Vec<LoadedSource>,
    pub scope: Scope,
    /// For a project on its own: the workspace above it, which `mind link`
    /// would add it to.
    pub above: Option<PathBuf>,
    /// Why this is the workspace on screen, as core's `resolve` answered it:
    /// named with `-W`, activated, found walking up, or the default one in
    /// home, opened because none of the others gave one. Asked rather than
    /// worked out from the root, which does not say: home can be reached all
    /// four ways. `None` for a project on its own.
    pub reason: Option<Reason>,
    /// Whether it is on disk. Only the default workspace is ever shown
    /// otherwise: before anything has made it, as it will be once the first
    /// change made from here does.
    pub made: bool,
}

impl Workspace {
    /// How many projects it manages, not counting its own artifacts.
    pub fn projects(&self) -> usize {
        self.members.iter().filter(|member| !member.own).count()
    }

    /// Where the scan found the project `member`'s entry was, moved: the one
    /// unlinked directory holding the id the workspace last saw there, as
    /// `mindweaver scan` says it (`moved`). Matched by core's
    /// `scan::moved_from`, as the panel's backend matches it, since the
    /// entry is spelled as the marker has it and the scan's route as
    /// `mindweaver link` would take it.
    pub fn moved_to(&self, member: &Member) -> Option<&Unlinked> {
        self.unlinked.iter().find(|unlinked| {
            unlinked
                .moved
                .as_deref()
                .is_some_and(|old| scan::moved_from(old, &self.root, &member.root))
        })
    }
}

/// One holder of artifacts: the workspace's own, or a registered entry.
#[derive(Debug, Clone)]
pub struct Member {
    /// The entry as the marker spells it, which is what a command is given.
    pub entry: String,
    /// Where it points.
    pub root: PathBuf,
    /// The project's name, or the entry when it would not open.
    pub name: String,
    /// The workspace's own artifacts rather than a project's.
    pub own: bool,
    /// A project whose own lines would consult some other workspace than
    /// the one on screen: one linked from outside its tree, or from above a
    /// nearer workspace. Its lines name the one on screen instead.
    pub elsewhere: bool,
    /// What it holds, or why it could not be opened.
    pub holdings: Result<Holdings, String>,
}

impl Member {
    /// The level its commands run at.
    pub fn holder(&self) -> Holder {
        match (self.own, self.elsewhere) {
            (true, _) => Holder::Workspace,
            (false, true) => Holder::Elsewhere(self.entry.clone()),
            (false, false) => Holder::Project(self.entry.clone()),
        }
    }

    /// Its artifacts; none when it could not be opened.
    pub fn items(&self) -> &[Item] {
        self.holdings
            .as_ref()
            .map_or(&[], |holdings| holdings.items.as_slice())
    }

    /// How many things about it want attention: artifacts `validate` would
    /// reject, and files that could not be read at all.
    pub fn problems(&self) -> usize {
        match &self.holdings {
            Ok(holdings) => {
                let invalid = holdings
                    .items
                    .iter()
                    .filter(|item| item.broken.is_some() || !item.issues.is_empty())
                    .count();
                invalid + holdings.warnings.len()
            }
            Err(_) => 1,
        }
    }
}

/// What an open project holds.
#[derive(Debug, Clone, Default)]
pub struct Holdings {
    pub items: Vec<Item>,
    /// The agents the project's settings name, in report order: a slot each
    /// on every row, so a row's badges line up with the next one's. Empty
    /// for the workspace's own, and for a project a `mind.toml` marks, which
    /// names none.
    pub agents: Vec<Agent>,
    /// The same, in the order its settings name them, which the toggles
    /// keep for the agents they keep.
    pub named: Vec<Agent>,
    /// Whether what is installed into it is the team's or this checkout's,
    /// as its settings say: none for the workspace's own, and for a project
    /// a `mind.toml` marks, which has no settings to change.
    pub share: Option<Share>,
    /// The older marker it is read from, a `mind.toml` or a `settings.json`,
    /// which `mind migrate` moves to a `settings.toml`: none for a project
    /// its `settings.toml` marks, and for the workspace's own. Until it is
    /// moved, its settings are changed by nothing, and the header says so.
    pub older: Option<MarkerFile>,
    /// Whether its settings let a tracked file be edited to reach an agent.
    pub edit_tracked: bool,
    /// Where its settings install skills, each folder as they spell it, when
    /// they name `targets` for them: none when skills go where its agents
    /// read, and for the workspace's own, which have no settings.
    pub targets: Option<Vec<String>>,
    /// What it installs that some of its agents do not see, as `status` says
    /// it: each folder its settings name that no install uses, each kind
    /// installed only where some of them do not read, and each rule some of
    /// them read nowhere.
    pub unreached: Vec<String>,
    /// What doctor finds keeps its agents from what it gives them, as
    /// `mindweaver doctor` says it, each with its fix, one line each: none
    /// for the workspace's own, which is no project's.
    pub found: Vec<String>,
    /// Files that could not be read, one line each: the read's, and those
    /// doctor could not read to tell.
    pub warnings: Vec<String>,
    /// Where each kind lives, relative to the project, so an empty project
    /// can say where it looked.
    pub directories: Vec<(Kind, String)>,
    /// What `mind add --template` can start from in this project.
    pub templates: Vec<Choice>,
}

/// A template the add form can offer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub kind: Kind,
}

/// One artifact.
#[derive(Debug, Clone)]
pub struct Item {
    pub kind: Kind,
    pub name: String,
    pub summary: String,
    /// What `validate` says about it; empty when it is well formed.
    pub issues: Vec<String>,
    /// Why it cannot be read, when it cannot. Listed anyway, by where it sits,
    /// because a file that does not read is the one most in need of opening.
    pub broken: Option<String>,
    /// How it stands in its project, as `status` says: none for the
    /// workspace's own, which is no project's.
    pub state: Option<State>,
    /// The agents among the project's that read a copy of it, as `status`
    /// says.
    pub agents: Vec<Agent>,
}

impl Item {
    /// `skill:deploy`: qualified, so a command names this artifact and not a
    /// rule that happens to share its name.
    pub fn reference(&self) -> String {
        qualified(self.kind, &self.name)
    }
}

/// A workspace this one loads, as the screen shows it.
#[derive(Debug, Clone)]
pub struct LoadedSource {
    /// The entry as the marker spells it: what `mindweaver unload` is given.
    pub entry: String,
    /// Where it points: what `mind unload` is given, from a project.
    pub root: PathBuf,
    /// The loaded workspace's name, or the entry when it cannot be opened.
    pub name: String,
    /// How many of its own it offers to install, or why it cannot be read.
    pub offers: Result<usize, String>,
}

/// A directory under the workspace, or under a folder `mindweaver minds` was
/// given, that is not linked yet.
#[derive(Debug, Clone)]
pub struct Unlinked {
    /// What `mindweaver link` is given for it from the workspace root, where the
    /// TUI runs its lines: the route from there, or its path when it is
    /// outside.
    pub route: String,
    /// A mind project already, rather than a repository that needs
    /// `mind init` before it can be linked.
    pub mind: bool,
    /// The entry it moved from, as the scan spelled it, when it holds the id
    /// the workspace last saw at an entry where no project is any more.
    pub moved: Option<String>,
    /// Why it is worth linking, which core ranked it by.
    pub reason: scan::Reason,
    /// The names at its root an agent reads, which make a repository's
    /// reason.
    pub agent_files: Vec<String>,
}

// ---------------------------------------------------------------------------
// What a key can ask for
// ---------------------------------------------------------------------------

/// What a key press asks the caller to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Redraw and keep going.
    Stay,
    /// Close the TUI.
    Quit,
    /// Do this, then show what came of it.
    Run(Task),
}

// ---------------------------------------------------------------------------
// What came of it
// ---------------------------------------------------------------------------

/// Commands that ran, as a shell session would show them: each line after a
/// `$`, then what it printed, its warnings and its error, if it had one.
///
/// What the TUI prints when it closes, and what the install screen `mindweaver
/// install` opens prints once it is applied, so the lines either ran can be
/// read off and run again.
pub fn transcript(dones: &[Done]) -> String {
    let mut text = String::new();
    for done in dones {
        let _ = writeln!(text, "$ {}", done.command);
        text.push_str(&done.stdout);
        for line in &done.stderr {
            let _ = writeln!(text, "{line}");
        }
        if let Some(error) = &done.error {
            let _ = writeln!(text, "error: {error}");
        }
    }
    text
}

/// One command the TUI ran, and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// The command line, as it would be typed.
    pub command: String,
    pub stdout: String,
    pub stderr: Vec<String>,
    /// Why it could not run, when it could not.
    pub error: Option<String>,
    /// Whether it ran and found everything in order: what its exit code
    /// would have said.
    pub ok: bool,
}

impl Done {
    /// A command that did not get as far as saying anything.
    pub fn failed(command: String, error: impl Into<String>) -> Self {
        Self {
            command,
            stdout: String::new(),
            stderr: Vec::new(),
            error: Some(error.into()),
            ok: false,
        }
    }
}

/// What runs one command line from where the commands run, and says what came
/// of it: the real parsers and commands ([`super::execute`]) unless something
/// is stood in for them.
///
/// A stand-in is for a test of what the screen does while a command takes its
/// time, which a real one on a local disk never does. It is handed the words
/// all the same, so there is still nothing the screen runs that is not a
/// command line.
#[derive(Clone)]
pub struct Runner(Arc<RunLine>);

/// The function a [`Runner`] is: the words, where they run from, where to
/// look for a workspace, and the watch a gather among them reports to.
type RunLine = dyn Fn(&[String], &Path, &Lookup, &Watch) -> Done + Send + Sync;

impl Runner {
    pub fn new(
        run: impl Fn(&[String], &Path, &Lookup, &Watch) -> Done + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(run))
    }

    /// Run `words` from `base`.
    pub fn run(&self, words: &[String], base: &Path, lookup: &Lookup, watch: &Watch) -> Done {
        (self.0)(words, base, lookup, watch)
    }
}

impl fmt::Debug for Runner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Runner")
    }
}

/// What a task running apart sends back to the screen.
#[derive(Debug)]
pub enum Update {
    /// It reached this step.
    Progress(Progress),
    /// It is over, and this is what each of its command lines said.
    Finished(Vec<Done>),
}

/// A task running on a thread of its own while the screen goes on drawing.
#[derive(Debug)]
pub struct Running {
    pub task: Task,
    /// The last step it reported, once it has reported one.
    pub progress: Option<Progress>,
    /// Whether it has been asked to stop.
    pub cancelling: bool,
    /// Shares its flag with the watch the task reports to, without its
    /// sink: holding a sender here would keep the channel open after the
    /// thread is gone, and a thread that died would never be heard of.
    pub watch: Watch,
    updates: Receiver<Update>,
}

impl Running {
    pub fn new(task: Task, watch: Watch, updates: Receiver<Update>) -> Self {
        Self {
            task,
            progress: None,
            cancelling: false,
            watch,
            updates,
        }
    }

    /// Ask it to stop. A gather stops before it places anything, and one
    /// already placing finishes, as it would from the command line; a
    /// backup stops before it pushes, and one already pushing finishes.
    pub fn cancel(&mut self) {
        self.watch.cancel();
        self.cancelling = true;
    }

    /// Take in what it has sent: each step, and what came of it once it is
    /// over. Without `wait`, only what has arrived; with it, everything until
    /// it is over.
    fn hear(&mut self, wait: bool) -> Option<Vec<Done>> {
        loop {
            let update = if wait {
                self.updates.recv().map_err(|_| TryRecvError::Disconnected)
            } else {
                self.updates.try_recv()
            };
            match update {
                Ok(Update::Progress(progress)) => self.progress = Some(progress),
                Ok(Update::Finished(dones)) => return Some(dones),
                Err(TryRecvError::Empty) => return None,
                // Gone without a word, which is a thread that panicked: said,
                // rather than waited for forever.
                Err(TryRecvError::Disconnected) => {
                    return Some(vec![Done::failed(
                        self.task.describe(),
                        "stopped before it could say how it went",
                    )])
                }
            }
        }
    }
}

/// What carrying out a task produced.
#[derive(Debug)]
pub enum Performed {
    /// Commands ran; what each said, in order.
    Ran(Vec<Done>),
    /// The install screen, built and ready to be shown.
    Install(InstallScreen),
    /// A directory that exists but is not a mind project. Linking it needs
    /// `mind init` first, and whether to run that is asked, not assumed.
    NeedsInit { path: String },
}

// ---------------------------------------------------------------------------
// The screen
// ---------------------------------------------------------------------------

/// Which column the keyboard is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Projects,
    Items,
}

/// How a line of text should read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Info,
    Good,
    Warn,
    Bad,
}

/// One line about the last thing that happened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub tone: Tone,
}

impl Message {
    fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Info,
        }
    }

    fn good(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Good,
        }
    }

    fn warn(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Warn,
        }
    }

    fn bad(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            tone: Tone::Bad,
        }
    }
}

/// What the keys are doing right now.
#[derive(Debug)]
pub enum Mode {
    /// Moving around the projects and what they hold.
    Browse,
    Adding(AddForm),
    Renaming(RenameForm),
    Linking(LinkForm),
    Loading(LoadForm),
    Gathering(GatherForm),
    /// The library about to be exported into a folder: `x`.
    Exporting(ExportForm),
    /// The library about to be backed up to a git repository: `b`.
    Backing(BackupForm),
    /// A yes or no before something that writes.
    Confirming(Confirm),
    /// A page of output: validate, show, the shelf, a report.
    Reading(Reader),
    /// The sources the shelf is filled from, one picked to gather again or
    /// forget: `S`.
    Sources(SourcesScreen),
    /// The skill directories about to be searched, `f` on the shelf's page,
    /// or the MCP Registry, `r` there.
    Searching(SearchForm),
    /// What a search of the skill directories found, one picked to take.
    Results(Results),
    /// What a search of the MCP Registry found, one picked to add.
    Servers(ServerResults),
    /// The install screen, embedded as it is.
    Installing(InstallScreen),
    /// Which minds the workspace manages, ticked, to change in one pass.
    Minds(Minds),
    /// A project's agents and share mode, toggled, to change in one pass.
    Settings(SettingsForm),
    /// Where a project installs skills, typed.
    Targets(TargetsForm),
}

/// The whole TUI.
#[derive(Debug)]
pub struct App {
    /// Where it was opened, which is where `mindweaver init` makes a workspace,
    /// or `mind init` a project.
    pub directory: PathBuf,
    /// What it was opened on: `mindweaver`'s workspace, or `mind`'s project.
    pub opened: Scope,
    pub view: View,
    pub focus: Focus,
    /// Which project the cursor is on.
    pub cursor: usize,
    /// Which artifact of that project.
    pub item: usize,
    pub mode: Mode,
    pub message: Option<Message>,
    /// Every command that changed something, in the order it ran.
    pub done: Vec<Done>,
    /// What is running, drawn over everything until it finishes.
    pub busy: Option<String>,
    /// What is running on a thread of its own, drawn over everything with
    /// the last step it reported until it is over.
    pub running: Option<Running>,
    /// Where to look for a workspace: what every command this TUI runs is
    /// told, so none of them looks again and finds another.
    pub lookup: Lookup,
    /// Where to look for what could be linked, every time the disk is read
    /// again: the folders `mindweaver minds` was given, none meaning the
    /// workspace root.
    pub roots: Vec<PathBuf>,
    /// What runs its command lines, when it is not the real thing: a test's
    /// stand-in.
    pub runner: Option<Runner>,
}

impl App {
    /// The TUI `mindweaver` opens: on a workspace.
    pub fn new(directory: PathBuf, view: View) -> Self {
        Self::opened_on(Scope::Workspace, directory, view)
    }

    /// The TUI opened on `scope`.
    pub fn opened_on(scope: Scope, directory: PathBuf, view: View) -> Self {
        let mut app = Self {
            directory,
            opened: scope,
            view,
            focus: Focus::Projects,
            cursor: 0,
            item: 0,
            mode: Mode::Browse,
            message: None,
            done: Vec::new(),
            busy: None,
            running: None,
            lookup: Lookup::default(),
            roots: Vec::new(),
            runner: None,
        };
        app.settle_in();
        app
    }

    /// Where the cursor starts, and what is worth saying first.
    ///
    /// On the first project rather than on the workspace's own row above
    /// them: the projects are what is usually worked on, and their own row
    /// is one key up.
    fn settle_in(&mut self) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        let first = workspace.members.iter().position(|member| !member.own);
        let lonely = workspace.scope == Scope::Workspace && workspace.projects() == 0;
        let default = workspace.reason == Some(Reason::Home);
        let message = match (&workspace.above, workspace.scope) {
            _ if default && !workspace.made => Some(format!(
                "no workspace here or above: your default one, in {}, is made by the first change; `mindweaver init` makes one here",
                workspace.root.display()
            )),
            _ if default => Some(format!(
                "no workspace here or above: this is your default one, in {}; `mindweaver init` makes one here",
                workspace.root.display()
            )),
            _ if lonely => Some(format!(
                "{} manages no projects yet; l links one",
                workspace.name
            )),
            (Some(above), Scope::Project) => Some(format!(
                "no workspace manages this project, but the one at {} would; l links it",
                above.display()
            )),
            _ => None,
        };
        self.cursor = first.unwrap_or(0);
        self.message = message.map(Message::info);
    }

    /// Tell every command this TUI runs where to look for a workspace: the
    /// lookup its screen was read with.
    pub fn with_lookup(mut self, lookup: Lookup) -> Self {
        self.lookup = lookup;
        self
    }

    /// Look for what could be linked under `roots` rather than under the
    /// workspace root, each time the disk is read again: the folders the
    /// screen was read with, so a change made from it does not send the
    /// minds screen back to the workspace root.
    pub fn with_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.roots = roots;
        self
    }

    /// Put the cursor on the project `path` is in, in its artifacts if it
    /// has any: where `mind` opens, when the project it was run in is managed.
    ///
    /// The deepest project holding the path, since `mind` can be run from
    /// anywhere inside one.
    pub fn focus_on(&mut self, path: &Path) {
        let at = self.workspace().and_then(|workspace| {
            workspace
                .members
                .iter()
                .enumerate()
                .filter(|(_, member)| !member.own && path.starts_with(&member.root))
                .max_by_key(|(_, member)| member.root.components().count())
                .map(|(at, _)| at)
        });
        if let Some(at) = at {
            self.cursor = at;
            self.item = 0;
            let items = self
                .member()
                .is_some_and(|member| !member.items().is_empty());
            self.focus = if items { Focus::Items } else { Focus::Projects };
        }
    }

    /// The level the keys act at: a project on its own, or a workspace.
    pub fn scope(&self) -> Scope {
        self.workspace()
            .map_or(self.opened, |workspace| workspace.scope)
    }

    /// The workspace, when there is one.
    pub fn workspace(&self) -> Option<&Workspace> {
        match &self.view {
            View::Workspace(workspace) => Some(workspace),
            View::Nowhere => None,
        }
    }

    /// The project under the cursor.
    pub fn member(&self) -> Option<&Member> {
        self.workspace()?.members.get(self.cursor)
    }

    /// The artifact under the cursor, in that project.
    pub fn current_item(&self) -> Option<&Item> {
        self.member()?.items().get(self.item)
    }

    /// Where commands run from: the workspace root, or where the TUI was
    /// opened when there is no workspace yet.
    pub fn base(&self) -> &Path {
        self.workspace()
            .map_or(self.directory.as_path(), |workspace| &workspace.root)
    }

    /// The command lines a task runs from [`base`](Self::base): its
    /// [`commands`](Task::commands), after a `mindweaver init` when it changes
    /// the default workspace before anything has made it.
    ///
    /// Made by a line of its own rather than left to the change, which would
    /// make it as well from the shell. The lines run from the workspace root
    /// and walk up from there like any command, and before it is made there
    /// is no marker at the root to stop them: with a workspace above home,
    /// the change would land in that one. `mindweaver init` makes the workspace
    /// where it is run, and the transcript says so.
    pub fn lines(&self, task: &Task) -> Vec<Vec<String>> {
        let mut lines = task.commands();
        let unmade = self.workspace().is_some_and(|workspace| !workspace.made);
        if unmade && task.writes() && !lines.is_empty() {
            lines.splice(0..0, Task::InitWorkspace.commands());
        }
        lines
    }

    /// Every command that changed something, as a shell session would show
    /// it ([`transcript`]). Printed when the TUI closes, so what it did
    /// outlives the screen and can be done again without it.
    pub fn transcript(&self) -> String {
        transcript(&self.done)
    }

    // -----------------------------------------------------------------------
    // Keys
    // -----------------------------------------------------------------------

    /// Feed it a key.
    pub fn press(&mut self, key: KeyEvent) -> Step {
        // From anywhere, typing included: raw mode swallows the signal, and a
        // screen that cannot be left that way feels broken.
        let quit = key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        // The screen is the running task's until it is over. Esc stops it,
        // and so does leaving, which waits for it to stop; a key for anything
        // else was aimed at a screen that is about to change.
        if let Some(running) = &mut self.running {
            if quit || key.code == KeyCode::Esc {
                running.cancel();
            }
            return if quit { Step::Quit } else { Step::Stay };
        }
        if quit {
            return Step::Quit;
        }
        // A message explains the key before this one, so it goes as soon as
        // another is pressed rather than sitting there being about nothing.
        self.message = None;

        let mode = std::mem::replace(&mut self.mode, Mode::Browse);
        let (mode, step) = match mode {
            Mode::Browse => self.browse(key),
            Mode::Adding(form) => self.adding(form, key),
            Mode::Renaming(form) => self.renaming(form, key),
            Mode::Linking(form) => self.linking(form, key),
            Mode::Loading(form) => self.loading(form, key),
            Mode::Gathering(form) => gathering(form, key),
            Mode::Exporting(form) => exporting(form, key),
            Mode::Backing(form) => backing(form, key),
            Mode::Confirming(confirm) => confirming(confirm, key),
            Mode::Reading(reader) => reading(reader, key),
            Mode::Sources(screen) => self.in_sources(screen, key),
            Mode::Searching(form) => searching(form, key),
            Mode::Results(screen) => results(screen, key),
            Mode::Servers(screen) => servers(screen, key),
            Mode::Installing(screen) => self.installing(screen, key),
            Mode::Minds(screen) => self.in_minds(screen, key),
            Mode::Settings(form) => self.setting(form, key),
            Mode::Targets(form) => self.targeting(form, key),
        };
        self.mode = mode;
        step
    }

    fn browse(&mut self, key: KeyEvent) -> (Mode, Step) {
        if self.workspace().is_none() {
            // With nothing here there is one thing to do, or leave: make what
            // the TUI was opened for.
            let make = match self.opened {
                Scope::Workspace => Task::InitWorkspace,
                Scope::Project => Task::InitProject,
            };
            let step = match key.code {
                KeyCode::Enter | KeyCode::Char('i' | 'y') => Step::Run(make),
                KeyCode::Char('q') | KeyCode::Esc => Step::Quit,
                _ => Step::Stay,
            };
            return (Mode::Browse, step);
        }

        match key.code {
            KeyCode::Char('q') => return (Mode::Browse, Step::Quit),
            KeyCode::Char('?') => return (Mode::Reading(Reader::help()), Step::Stay),
            KeyCode::Char('R') => return (Mode::Browse, Step::Run(Task::Refresh)),
            // Whatever is on screen: which workspaces there are is the
            // machine's to say, not the workspace's or the project's.
            KeyCode::Char('w') => return (Mode::Browse, Step::Run(Task::Workspaces)),
            KeyCode::Char(key @ ('e' | 'r' | 'd')) => return self.on_item(key),
            KeyCode::Char('a') => return self.start_adding(),
            KeyCode::Char('t') => return self.start_settings(),
            KeyCode::Char('T') => return self.start_targets(),
            KeyCode::Char('v') => {
                let holder = match self.scope() {
                    Scope::Workspace => Holder::Workspace,
                    Scope::Project => Holder::Project(String::from(".")),
                };
                return (Mode::Browse, Step::Run(Task::Validate { holder }));
            }
            _ => {}
        }
        // What only a workspace has: its registry, its shelf, its installs.
        if let KeyCode::Char(
            key @ ('l' | 'u' | 'U' | 'M' | 'A' | 'g' | 's' | 'S' | 'i' | 'm' | 'L' | 'x' | 'b'),
        ) = key.code
        {
            return match self.scope() {
                Scope::Workspace => self.in_workspace(key),
                Scope::Project => self.on_its_own(key),
            };
        }
        match self.focus {
            Focus::Projects => self.in_projects(key),
            Focus::Items => self.in_items(key),
        }
    }

    fn in_workspace(&mut self, key: char) -> (Mode, Step) {
        match key {
            'l' => (Mode::Linking(LinkForm::default()), Step::Stay),
            'L' => (Mode::Loading(LoadForm::default()), Step::Stay),
            'u' => self.start_updating(),
            'U' => self.start_unlinking(),
            'M' => self.start_relinking(),
            'A' => self.start_adopting(),
            'g' => (Mode::Gathering(GatherForm::default()), Step::Stay),
            // The workspace's own, wherever the cursor is: what is exported
            // and backed up is the library, whichever row is lit.
            'x' => (Mode::Exporting(ExportForm::default()), Step::Stay),
            'b' => (Mode::Backing(BackupForm::default()), Step::Stay),
            's' => (Mode::Browse, Step::Run(Task::Shelf)),
            'S' => self.start_sources(),
            'm' => match self.workspace() {
                Some(workspace) => (Mode::Minds(Minds::of(workspace)), Step::Stay),
                None => (Mode::Browse, Step::Stay),
            },
            _ => (Mode::Browse, Step::Run(Task::OpenInstall)),
        }
    }

    /// A project no workspace manages has no registry, shelf or installs of
    /// its own; the one thing to do about that is link it to a workspace.
    fn on_its_own(&mut self, key: char) -> (Mode, Step) {
        let above = self
            .workspace()
            .and_then(|workspace| workspace.above.clone());
        match (key, above) {
            ('l', Some(_)) => (Mode::Browse, Step::Run(Task::LinkHere)),
            // Loading is into the workspace above, which does not have to
            // manage this project yet: `mind load`, as `mind link` is.
            ('L', Some(_)) => (Mode::Loading(LoadForm::default()), Step::Stay),
            ('l' | 'L', None) => {
                self.message = Some(Message::info(
                    "no workspace above this project; `mindweaver init` in the folder that holds your projects makes one",
                ));
                (Mode::Browse, Step::Stay)
            }
            _ => {
                self.message = Some(Message::info(
                    "that is a workspace's, and no workspace manages this project; l links it to one",
                ));
                (Mode::Browse, Step::Stay)
            }
        }
    }

    fn in_projects(&mut self, key: KeyEvent) -> (Mode, Step) {
        let count = self
            .workspace()
            .map_or(0, |workspace| workspace.members.len());
        match key.code {
            KeyCode::Up | KeyCode::Char('k') if self.cursor > 0 => {
                self.cursor -= 1;
                self.item = 0;
            }
            KeyCode::Down | KeyCode::Char('j') if self.cursor + 1 < count => {
                self.cursor += 1;
                self.item = 0;
            }
            KeyCode::Right | KeyCode::Enter | KeyCode::Tab => {
                let empty = self
                    .member()
                    .map(|member| (member.items().is_empty(), member.name.clone()));
                match empty {
                    Some((false, _)) => self.focus = Focus::Items,
                    Some((true, name)) => {
                        self.message = Some(Message::info(format!(
                            "nothing in {name} yet; a adds a skill or a rule"
                        )));
                    }
                    None => {}
                }
            }
            _ => {}
        }
        (Mode::Browse, Step::Stay)
    }

    fn in_items(&mut self, key: KeyEvent) -> (Mode, Step) {
        let count = self.member().map_or(0, |member| member.items().len());
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.item = self.item.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') if self.item + 1 < count => self.item += 1,
            KeyCode::Left | KeyCode::Esc | KeyCode::Tab | KeyCode::BackTab => {
                self.focus = Focus::Projects;
            }
            KeyCode::Enter => {
                if let (Some(member), Some(item)) = (self.member(), self.current_item()) {
                    let task = Task::Show {
                        holder: member.holder(),
                        reference: item.reference(),
                    };
                    return (Mode::Browse, Step::Run(task));
                }
            }
            _ => {}
        }
        (Mode::Browse, Step::Stay)
    }

    /// `e`, `r` and `d`: edit, rename or delete the artifact under the cursor.
    ///
    /// Only from the artifacts column. From the projects column the cursor in
    /// the other one is dimmed, and a key that deletes what a dimmed cursor
    /// happens to rest on is a key that deletes the wrong thing.
    fn on_item(&mut self, key: char) -> (Mode, Step) {
        let picked = match (self.focus, self.member(), self.current_item()) {
            (Focus::Items, Some(member), Some(item)) => {
                Some((member.holder(), item.kind, item.name.clone()))
            }
            _ => None,
        };
        let Some((holder, kind, name)) = picked else {
            self.message = Some(Message::info(
                "pick an artifact first; → moves over to them",
            ));
            return (Mode::Browse, Step::Stay);
        };

        match key {
            'e' => (Mode::Browse, Step::Run(Task::Edit { holder, kind, name })),
            'r' => (
                Mode::Renaming(RenameForm::new(self.cursor, kind, name)),
                Step::Stay,
            ),
            _ => {
                let what = match kind.layout() {
                    Layout::Directory { .. } => "Its folder goes, and everything in it.",
                    Layout::Files { .. } | Layout::Flat { .. } => "The file goes.",
                };
                let mut question = vec![
                    format!("Delete {kind} {name}?"),
                    String::new(),
                    what.to_owned(),
                ];
                // One of the workspace's own may have been installed into
                // its projects, where removing it alone leaves each copy
                // orphaned: a second yes takes them out first, as
                // `--everywhere` does. A project's own was installed
                // nowhere, so it has only the one.
                let also = (holder == Holder::Workspace).then(|| {
                    question.push(String::new());
                    question.push(String::from(
                        "Everywhere takes it out of every project it was installed into first, \
                         but for a copy changed since.",
                    ));
                    Also {
                        key: 'e',
                        label: "everywhere",
                        task: Task::Remove {
                            holder: holder.clone(),
                            kind,
                            name: name.clone(),
                            everywhere: true,
                            yes: true,
                        },
                    }
                });
                let confirm = Confirm {
                    question,
                    task: Task::Remove {
                        holder,
                        kind,
                        name,
                        everywhere: false,
                        yes: true,
                    },
                    also,
                    back: Box::new(Mode::Browse),
                };
                (Mode::Confirming(confirm), Step::Stay)
            }
        }
    }

    fn renaming(&mut self, mut form: RenameForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        match key.code {
            KeyCode::Esc => (Mode::Browse, Step::Stay),
            KeyCode::Enter => match self.rename_task(&form) {
                Some(task) => (Mode::Renaming(form), Step::Run(task)),
                None => (Mode::Browse, Step::Stay),
            },
            _ => {
                form.name.press(key);
                (Mode::Renaming(form), Step::Stay)
            }
        }
    }

    /// What the rename form would run, if it were submitted now.
    pub fn rename_task(&self, form: &RenameForm) -> Option<Task> {
        let member = self.workspace()?.members.get(form.member)?;
        Some(Task::Rename {
            holder: member.holder(),
            kind: form.kind,
            from: form.from.clone(),
            to: form.name.text().trim().to_owned(),
        })
    }

    /// The templates the add form can offer for `kind`, in the order it
    /// offers them.
    pub fn templates_for(&self, member: usize, kind: Kind) -> Vec<&Choice> {
        self.workspace()
            .and_then(|workspace| workspace.members.get(member))
            .and_then(|member| member.holdings.as_ref().ok())
            .map(|holdings| {
                holdings
                    .templates
                    .iter()
                    .filter(|choice| choice.kind == kind)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn start_adding(&mut self) -> (Mode, Step) {
        let refusal = match self.member() {
            Some(member) if member.holdings.is_ok() => None,
            Some(member) => Some(format!(
                "{} cannot be opened, so nothing can be added to it",
                member.name
            )),
            None => Some(String::from("link a project first; l links one")),
        };
        match refusal {
            None => (Mode::Adding(AddForm::new(self.cursor)), Step::Stay),
            Some(refusal) => {
                self.message = Some(Message::info(refusal));
                (Mode::Browse, Step::Stay)
            }
        }
    }

    /// `u`: bring what is installed in the project under the cursor up to
    /// date, or only the artifact under it in the artifacts column; on the
    /// workspace's own row, in every project it links, which is where its
    /// own reach once installed. No question first: an update writes over
    /// nothing somebody changed, and its lines are shown in full after.
    fn start_updating(&mut self) -> (Mode, Step) {
        let Some(member) = self.member() else {
            self.message = Some(Message::info("link a project first; l links one"));
            return (Mode::Browse, Step::Stay);
        };
        if !member.own && member.holdings.is_err() {
            self.message = Some(Message::info(format!(
                "{} cannot be opened, so nothing in it can be updated",
                member.name
            )));
            return (Mode::Browse, Step::Stay);
        }
        let reference = match (self.focus, self.current_item()) {
            (Focus::Items, Some(item)) => Some(item.reference()),
            _ => None,
        };
        let task = Task::Update {
            holder: member.holder(),
            reference,
            force: false,
        };
        (Mode::Browse, Step::Run(task))
    }

    fn start_unlinking(&mut self) -> (Mode, Step) {
        let Some(member) = self.member() else {
            self.message = Some(Message::info("nothing to unlink"));
            return (Mode::Browse, Step::Stay);
        };
        if member.own {
            self.message = Some(Message::info(
                "these are the workspace's own, not a project it links, so there is nothing to unlink",
            ));
            return (Mode::Browse, Step::Stay);
        }
        let confirm = Confirm {
            question: vec![
                format!("Unlink {}?", member.name),
                String::new(),
                String::from("The workspace forgets it; nothing on disk is deleted."),
            ],
            task: Task::Unlink {
                entry: member.entry.clone(),
            },
            also: None,
            back: Box::new(Mode::Browse),
        };
        (Mode::Confirming(confirm), Step::Stay)
    }

    /// `M`: the stale entry under the cursor pointed at where the scan found
    /// its project, moved, once somebody says yes, as `mindweaver relink` does.
    /// Only where the scan found one: which directory holds the project is
    /// the scan's to say, by the id it holds, and a guess here could hand
    /// what was installed to the wrong one.
    fn start_relinking(&mut self) -> (Mode, Step) {
        let (Some(workspace), Some(member)) = (self.workspace(), self.member()) else {
            self.message = Some(Message::info("nothing to relink"));
            return (Mode::Browse, Step::Stay);
        };
        if member.own || member.holdings.is_ok() {
            self.message = Some(Message::info(
                "M relinks a project whose entry went stale to where it moved; this one is where its entry says",
            ));
            return (Mode::Browse, Step::Stay);
        }
        // Not "nothing holds its id": the ledger learns an id only when it
        // is written, so a project moved before then is found by nobody,
        // though it may be right there.
        let Some(moved) = workspace.moved_to(member) else {
            self.message = Some(Message::info(format!(
                "nothing found under {} is known to have moved from {}; `mindweaver relink {} <new>` follows it to where it went",
                super::ui::looked_under(workspace),
                member.entry,
                member.entry
            )));
            return (Mode::Browse, Step::Stay);
        };
        let confirm = Confirm {
            question: vec![
                format!("Relink {} to {}?", member.entry, moved.route),
                String::new(),
                String::from(
                    "The project there holds the id this entry was, so it is this one, moved. \
                     The entry follows it, and what was installed stays installed.",
                ),
            ],
            task: Task::Relink {
                old: member.entry.clone(),
                new: moved.route.clone(),
            },
            also: None,
            back: Box::new(Mode::Browse),
        };
        (Mode::Confirming(confirm), Step::Stay)
    }

    /// `A`: the foreign skill under the cursor brought into the workspace's
    /// library, and its copies recorded as installed from there, once
    /// somebody says yes, as `mindweaver adopt` does. Only on a row `status`
    /// calls foreign: what Mindweaver installed is its own already, what the
    /// project authors is the project's, and a rule is never adopted. Core
    /// refuses the rest, a name the library holds or copies that differ,
    /// which the line's error says.
    fn start_adopting(&mut self) -> (Mode, Step) {
        let picked = match (self.focus, self.member(), self.current_item()) {
            (Focus::Items, Some(member), Some(item)) if !member.own => {
                Some((member, item.kind, item.name.clone(), item.state))
            }
            _ => None,
        };
        let Some((member, kind, name, state)) = picked else {
            self.message = Some(Message::info(
                "A adopts somebody's skill from a project's agents' folders; pick one first, → moves over to them",
            ));
            return (Mode::Browse, Step::Stay);
        };
        if kind != Kind::Skill || state != Some(State::Foreign) {
            let what = state.map_or("", State::slug);
            self.message = Some(Message::info(format!(
                "A adopts a skill somebody keeps in an agent's folder, which status calls foreign; this {kind} is {what}"
            )));
            return (Mode::Browse, Step::Stay);
        }
        let confirm = Confirm {
            question: vec![
                format!("Adopt {name} from {} into the library?", member.name),
                String::new(),
                String::from(
                    "It is copied into the workspace's library, and the project's copies are \
                     recorded as installed from there, as they are: updates reach them from now \
                     on. Nothing in the project is written.",
                ),
            ],
            task: Task::Adopt {
                project: member.entry.clone(),
                kind,
                name,
            },
            also: None,
            back: Box::new(Mode::Browse),
        };
        (Mode::Confirming(confirm), Step::Stay)
    }

    /// `S`: the sources the shelf is filled from, to gather one again, or
    /// every one, or to forget one, once somebody says yes.
    fn start_sources(&mut self) -> (Mode, Step) {
        let none = self
            .workspace()
            .is_none_or(|workspace| workspace.sources.is_empty());
        if none {
            self.message = Some(Message::info(
                "nothing gathered yet; g gathers from a git repository",
            ));
            return (Mode::Browse, Step::Stay);
        }
        (Mode::Sources(SourcesScreen::default()), Step::Stay)
    }

    /// The sources screen's keys: `u` gathers the one picked again, `U`
    /// every one, each as `mindweaver gather update` does and followed as a
    /// gather is, and `d` forgets the one picked once somebody says yes, as
    /// `mindweaver gather remove --yes` does.
    fn in_sources(&mut self, mut screen: SourcesScreen, key: KeyEvent) -> (Mode, Step) {
        let sources: &[ShelfSource] = self
            .workspace()
            .map_or(&[], |workspace| workspace.sources.as_slice());
        let count = sources.len();
        // Kept on a source while the list shrinks under it, as a removal
        // read back makes it.
        screen.pick = screen.pick.min(count.saturating_sub(1));
        let picked = sources.get(screen.pick).cloned();
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return (Mode::Browse, Step::Stay),
            KeyCode::Up | KeyCode::Char('k') => screen.pick = screen.pick.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') if screen.pick + 1 < count => screen.pick += 1,
            KeyCode::Char('u') | KeyCode::Enter => {
                if let Some(picked) = picked {
                    let task = Task::UpdateSources {
                        names: vec![picked.source.name.to_string()],
                    };
                    return (Mode::Sources(screen), Step::Run(task));
                }
            }
            KeyCode::Char('U') => {
                let task = Task::UpdateSources { names: Vec::new() };
                return (Mode::Sources(screen), Step::Run(task));
            }
            KeyCode::Char('d') => {
                if let Some(picked) = picked {
                    let held: Vec<String> = picked
                        .shelved
                        .iter()
                        .map(|counted| counted_kind(counted.count, counted.kind))
                        .collect();
                    let held = if held.is_empty() {
                        String::from("its folder")
                    } else {
                        format!("its {}", held.join(", "))
                    };
                    let name = picked.source.name.to_string();
                    let confirm = Confirm {
                        question: vec![
                            format!("Forget the source {name}?"),
                            String::new(),
                            format!(
                                "This takes {held} off the shelf, and its clone out of the \
                                 cache. What was installed from it stays where it is, \
                                 orphaned: nothing offers it any more."
                            ),
                        ],
                        task: Task::RemoveSource { name, yes: true },
                        also: None,
                        back: Box::new(Mode::Sources(screen)),
                    };
                    return (Mode::Confirming(confirm), Step::Stay);
                }
            }
            _ => {}
        }
        (Mode::Sources(screen), Step::Stay)
    }

    /// `t`: the agents and share toggles of the project under the cursor,
    /// from either column, as `a` adds to it from either.
    fn start_settings(&mut self) -> (Mode, Step) {
        let refusal = match self.member() {
            None => String::from("link a project first; l links one"),
            Some(member) if member.own => String::from(
                "these are the workspace's own, which reach agents by being installed into a project",
            ),
            Some(member) => match &member.holdings {
                Err(_) => format!(
                    "{} cannot be opened, so its agents cannot be changed",
                    member.name
                ),
                Ok(holdings) if holdings.older == Some(MarkerFile::SettingsJson) => {
                    unchanged(&member.name)
                }
                Ok(holdings) => match holdings.share {
                    Some(share) => {
                        let form = SettingsForm::new(
                            self.cursor,
                            &holdings.named,
                            share,
                            holdings.edit_tracked,
                        );
                        return (Mode::Settings(form), Step::Stay);
                    }
                    None => format!(
                        "{} is marked by a mind.toml, which names no agents; `mind migrate` moves it to settings",
                        member.name
                    ),
                },
            },
        };
        self.message = Some(Message::info(refusal));
        (Mode::Browse, Step::Stay)
    }

    /// `T`: where the project under the cursor installs skills, from either
    /// column, as `t` changes its agents from either.
    fn start_targets(&mut self) -> (Mode, Step) {
        let refusal = match self.member() {
            None => String::from("link a project first; l links one"),
            Some(member) if member.own => String::from(
                "these are the workspace's own, which reach agents by being installed into a project",
            ),
            Some(member) => match &member.holdings {
                Err(_) => format!(
                    "{} cannot be opened, so where it installs cannot be changed",
                    member.name
                ),
                Ok(holdings) if holdings.older == Some(MarkerFile::SettingsJson) => {
                    unchanged(&member.name)
                }
                Ok(holdings) if holdings.share.is_some() => {
                    let form =
                        TargetsForm::new(self.cursor, Kind::Skill, holdings.targets.clone());
                    return (Mode::Targets(form), Step::Stay);
                }
                Ok(_) => format!(
                    "{} is marked by a mind.toml, which installs where it names; `mind migrate` moves it to settings",
                    member.name
                ),
            },
        };
        self.message = Some(Message::info(refusal));
        (Mode::Browse, Step::Stay)
    }

    fn targeting(&mut self, mut form: TargetsForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        match key.code {
            KeyCode::Esc => (Mode::Browse, Step::Stay),
            KeyCode::Enter => match self.targets_task(&form) {
                Some(task) => (Mode::Targets(form), Step::Run(task)),
                None if self.member_of(form.member).is_none() => (Mode::Browse, Step::Stay),
                None => {
                    form.error = Some(String::from(
                        "nothing changed yet: type the folders, or clear them for where its agents read",
                    ));
                    (Mode::Targets(form), Step::Stay)
                }
            },
            _ => {
                form.folders.press(key);
                (Mode::Targets(form), Step::Stay)
            }
        }
    }

    /// What the targets form would run, if it were submitted now: nothing
    /// while it says what the project says already.
    pub fn targets_task(&self, form: &TargetsForm) -> Option<Task> {
        let member = self.member_of(form.member)?;
        form.task(member.holder())
    }

    fn setting(&mut self, mut form: SettingsForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        let last = SettingsForm::FIELDS - 1;
        match key.code {
            KeyCode::Esc => return (Mode::Browse, Step::Stay),
            KeyCode::Enter => {
                return match self.settings_task(&form) {
                    Some(task) => (Mode::Settings(form), Step::Run(task)),
                    None if self.member_of(form.member).is_none() => (Mode::Browse, Step::Stay),
                    None => {
                        form.error = Some(String::from(
                            "nothing toggled yet: space ticks an agent, ←→ picks the share mode",
                        ));
                        (Mode::Settings(form), Step::Stay)
                    }
                };
            }
            KeyCode::Down | KeyCode::Tab | KeyCode::Char('j') => {
                form.field = (form.field + 1).min(last);
            }
            KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
                form.field = form.field.saturating_sub(1);
            }
            KeyCode::Char(' ') | KeyCode::Left | KeyCode::Right => form.toggle(key.code),
            _ => {}
        }
        (Mode::Settings(form), Step::Stay)
    }

    /// The member at `at`, if the screen still has one there.
    fn member_of(&self, at: usize) -> Option<&Member> {
        self.workspace()?.members.get(at)
    }

    /// What the agents and share toggles would run, if they were applied
    /// now: nothing while they say what the project says already.
    pub fn settings_task(&self, form: &SettingsForm) -> Option<Task> {
        let member = self.member_of(form.member)?;
        form.task(member.holder())
    }

    fn adding(&mut self, mut form: AddForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        match key.code {
            KeyCode::Esc => return (Mode::Browse, Step::Stay),
            KeyCode::Enter => {
                return match self.add_task(&form) {
                    Some(task) => (Mode::Adding(form), Step::Run(task)),
                    // The project it was for has gone from under it.
                    None => (Mode::Browse, Step::Stay),
                };
            }
            KeyCode::Tab | KeyCode::Down => form.field = form.field.next(),
            KeyCode::BackTab | KeyCode::Up => form.field = form.field.previous(),
            _ => match form.field {
                AddField::Kind => {
                    if matches!(
                        key.code,
                        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                    ) {
                        form.kind = other_kind(form.kind);
                        // Templates belong to a kind, so the pick does not
                        // carry across.
                        form.template = None;
                    }
                }
                AddField::Template => {
                    let offered = self.templates_for(form.member, form.kind).len();
                    form.template = match key.code {
                        KeyCode::Right | KeyCode::Char(' ') => {
                            step_through(form.template, offered, true)
                        }
                        KeyCode::Left => step_through(form.template, offered, false),
                        _ => form.template,
                    };
                }
                AddField::Name => {
                    form.name.press(key);
                }
                AddField::Description => {
                    form.description.press(key);
                }
            },
        }
        (Mode::Adding(form), Step::Stay)
    }

    /// What the add form would run, if it were submitted now.
    pub fn add_task(&self, form: &AddForm) -> Option<Task> {
        let member = self.workspace()?.members.get(form.member)?;
        let template = form.template.and_then(|at| {
            self.templates_for(form.member, form.kind)
                .get(at)
                .map(|choice| choice.name.clone())
        });
        Some(Task::Add {
            holder: member.holder(),
            kind: form.kind,
            name: form.name.text().trim().to_owned(),
            description: form.description.text().trim().to_owned(),
            template,
            // The TUI's form adds the kinds a name and a description make,
            // which a server, needing how it is started, is not.
            server: None,
        })
    }

    fn linking(&mut self, mut form: LinkForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        let choices = self
            .workspace()
            .map_or(0, |workspace| workspace.unlinked.len());
        match key.code {
            KeyCode::Esc => return (Mode::Browse, Step::Stay),
            KeyCode::Up => form.pick = form.pick.saturating_sub(1),
            KeyCode::Down if form.pick + 1 < choices => form.pick += 1,
            KeyCode::Enter => {
                return match self.link_task(&form) {
                    // Making a repository a mind project writes into it, so
                    // it is asked about first.
                    Some(Task::InitAndLink { path }) => {
                        let confirm = Confirm::init_and_link(path, Mode::Linking(form));
                        (Mode::Confirming(confirm), Step::Stay)
                    }
                    Some(task) => (Mode::Linking(form), Step::Run(task)),
                    None => {
                        form.error = Some(String::from(
                            "type the path of a project, relative to the workspace",
                        ));
                        (Mode::Linking(form), Step::Stay)
                    }
                };
            }
            _ => {
                form.path.press(key);
            }
        }
        (Mode::Linking(form), Step::Stay)
    }

    /// What the link form would run, if it were submitted now: the path typed,
    /// or else the directory picked from what the workspace found.
    pub fn link_task(&self, form: &LinkForm) -> Option<Task> {
        let typed = form.path.text().trim();
        if !typed.is_empty() {
            return Some(Task::Link {
                path: typed.to_owned(),
            });
        }
        let picked = self.workspace()?.unlinked.get(form.pick)?;
        let path = picked.route.clone();
        Some(if picked.mind {
            Task::Link { path }
        } else {
            Task::InitAndLink { path }
        })
    }

    fn loading(&mut self, mut form: LoadForm, key: KeyEvent) -> (Mode, Step) {
        form.error = None;
        let choices = self
            .workspace()
            .map_or(0, |workspace| workspace.loaded.len());
        match key.code {
            KeyCode::Esc => return (Mode::Browse, Step::Stay),
            KeyCode::Up => form.pick = form.pick.saturating_sub(1),
            KeyCode::Down if form.pick + 1 < choices => form.pick += 1,
            KeyCode::Enter => {
                if let Some(task) = self.load_task(&form) {
                    // The workspace a load goes into skips the one being
                    // loaded, so loading the one on screen would quietly go
                    // somewhere else: into the one above, or your home's.
                    if self.loads_itself(&form) {
                        form.error = Some(String::from(
                            "that is the workspace this would load into, and a workspace cannot load itself",
                        ));
                        return (Mode::Loading(form), Step::Stay);
                    }
                    return (Mode::Loading(form), Step::Run(task));
                }
                // Nothing typed: enter on a loaded one offers to unload it,
                // asked first as unlinking is.
                let scope = self.scope();
                let picked = self
                    .workspace()
                    .and_then(|workspace| workspace.loaded.get(form.pick))
                    .cloned();
                return match picked {
                    Some(source) => {
                        let task = match scope {
                            Scope::Workspace => Task::Unload {
                                entry: source.entry,
                            },
                            Scope::Project => Task::UnloadHere {
                                path: source.root.to_string_lossy().into_owned(),
                            },
                        };
                        let confirm = Confirm {
                            question: vec![
                                format!("Unload {}?", source.name),
                                String::new(),
                                String::from(
                                    "What it offers stops being offered here; what was installed from it stays, and nothing on disk is deleted.",
                                ),
                            ],
                            task,
                            also: None,
                            back: Box::new(Mode::Loading(form)),
                        };
                        (Mode::Confirming(confirm), Step::Stay)
                    }
                    None => {
                        form.error =
                            Some(String::from("type the path of a weaver workspace to load"));
                        (Mode::Loading(form), Step::Stay)
                    }
                };
            }
            _ => {
                form.path.press(key);
            }
        }
        (Mode::Loading(form), Step::Stay)
    }

    /// Whether the path typed into the load form is the workspace it would
    /// load into: the one on screen, or for a project the one above it.
    fn loads_itself(&self, form: &LoadForm) -> bool {
        let Some(workspace) = self.workspace() else {
            return false;
        };
        let into = match workspace.scope {
            Scope::Workspace => Some(workspace.root.as_path()),
            Scope::Project => workspace.above.as_deref(),
        };
        let typed = self.base().join(form.path.text().trim());
        into.is_some_and(|into| {
            let (a, b) = (paths::normalize(&typed), paths::normalize(into));
            a == b
                || matches!(
                    (std::fs::canonicalize(&a), std::fs::canonicalize(&b)),
                    (Ok(a), Ok(b)) if a == b
                )
        })
    }

    /// What the load form would run, if it were submitted now: `mindweaver load`
    /// on a workspace, `mind load` on a project on its own. Nothing without a
    /// path: picking a loaded one is for unloading it.
    pub fn load_task(&self, form: &LoadForm) -> Option<Task> {
        let path = form.path.text().trim();
        if path.is_empty() {
            return None;
        }
        let path = path.to_owned();
        Some(match self.scope() {
            Scope::Workspace => Task::Load { path },
            Scope::Project => Task::LoadHere { path },
        })
    }

    fn in_minds(&mut self, mut screen: Minds, key: KeyEvent) -> (Mode, Step) {
        match screen.press(key.code) {
            minds::Key::Stay => (Mode::Minds(screen), Step::Stay),
            minds::Key::Leave => {
                if !screen.idle() {
                    self.message = Some(Message::info("left the minds screen; nothing applied"));
                }
                (Mode::Browse, Step::Stay)
            }
            minds::Key::Apply => match screen.task() {
                // Asked first, like every other change that writes: `mind
                // init` writes into a repository, and unlinking forgets.
                Some(task) => {
                    let confirm = Confirm {
                        question: screen.question(),
                        task,
                        also: None,
                        back: Box::new(Mode::Minds(screen)),
                    };
                    (Mode::Confirming(confirm), Step::Stay)
                }
                None => {
                    self.message = Some(Message::info("nothing marked"));
                    (Mode::Minds(screen), Step::Stay)
                }
            },
        }
    }

    fn installing(&mut self, mut screen: InstallScreen, key: KeyEvent) -> (Mode, Step) {
        match screen.press(key.code) {
            InstallStep::Stay => (Mode::Installing(screen), Step::Stay),
            // The install screen's own way out leaves it, not the TUI.
            InstallStep::Quit => {
                if !screen.idle() {
                    self.message = Some(Message::info("left the install screen; nothing applied"));
                }
                (Mode::Browse, Step::Stay)
            }
            // Kept open under the box saying what runs, as a form is: what
            // runs is the lines its marks amount to, taken from it here.
            InstallStep::Apply => {
                let task = screen.task();
                (Mode::Installing(screen), Step::Run(task))
            }
        }
    }

    // -----------------------------------------------------------------------
    // What came back
    // -----------------------------------------------------------------------

    /// Take in what the task running apart has said since this was last
    /// asked; once it is over, it is no longer running, and here is what came
    /// of it.
    pub fn heard(&mut self) -> Option<(Task, Vec<Done>)> {
        self.hear(false)
    }

    /// The same, waiting until it is over.
    pub fn waited(&mut self) -> Option<(Task, Vec<Done>)> {
        self.hear(true)
    }

    fn hear(&mut self, wait: bool) -> Option<(Task, Vec<Done>)> {
        let dones = self.running.as_mut()?.hear(wait)?;
        let running = self.running.take()?;
        Some((running.task, dones))
    }

    /// Show what came of a task, with the disk read again when it asked for
    /// that.
    pub fn finish(
        &mut self,
        task: &Task,
        performed: Performed,
        view: Option<Result<View, String>>,
    ) {
        if let Some(view) = view {
            match view {
                Ok(view) => self.reload(view),
                Err(error) => self.message = Some(Message::bad(error)),
            }
        }

        let dones = match performed {
            Performed::Install(screen) => {
                if screen.targets.is_empty() {
                    self.message = Some(Message::info(
                        "no projects to install into yet; l links one",
                    ));
                } else {
                    self.mode = Mode::Installing(screen);
                }
                return;
            }
            Performed::NeedsInit { path } => {
                let back = std::mem::replace(&mut self.mode, Mode::Browse);
                self.mode = Mode::Confirming(Confirm::init_and_link(path, back));
                return;
            }
            Performed::Ran(dones) => dones,
        };

        if task.writes() {
            self.done.extend(dones.iter().cloned());
        }
        let failure = dones.iter().find_map(|done| done.error.clone());

        match (task, failure) {
            (Task::Refresh, _) => self.message = Some(Message::info("read the disk again")),
            // Output worth reading in full, whether or not it went well.
            (
                Task::Show { .. }
                | Task::Validate { .. }
                | Task::Shelf
                | Task::UpdateSources { .. }
                | Task::RemoveSource { .. }
                | Task::Workspaces
                | Task::ApplyInstall { .. }
                | Task::Install { .. }
                | Task::Uninstall { .. }
                | Task::UninstallOrphaned { .. }
                | Task::Update { .. }
                | Task::Minds { .. },
                _,
            )
            | (
                Task::Gather { .. }
                | Task::ImportServer { yes: true, .. }
                | Task::Export { .. }
                | Task::Backup { .. }
                | Task::Settings { .. }
                | Task::Targets { .. },
                None,
            ) => {
                let mut reader = Reader::of(&dones);
                // The shelf's page offers a search of what is not on it yet.
                reader.searches = *task == Task::Shelf;
                self.mode = Mode::Reading(reader);
            }
            // What the line printed, read back as the JSON it is, for a
            // result to be picked from; the same words, a table's, as the
            // command lays them out.
            (Task::SearchSkills { query }, None) => match Results::of(query, &dones) {
                Ok(results) => self.mode = Mode::Results(results),
                Err(error) => self.message = Some(Message::bad(error)),
            },
            (Task::SearchServers { query }, None) => match ServerResults::of(query, &dones) {
                Ok(results) => self.mode = Mode::Servers(results),
                Err(error) => self.message = Some(Message::bad(error)),
            },
            // What would be written, read in full, with the key that writes
            // it: the dry run is what the yes is given to, as the install
            // screen shows each agent's line before it applies.
            (
                Task::ImportServer {
                    server,
                    version,
                    yes: false,
                },
                None,
            ) => {
                let mut reader = Reader::of(&dones);
                reader.writes = Some(Task::ImportServer {
                    server: server.clone(),
                    version: version.clone(),
                    yes: true,
                });
                self.mode = Mode::Reading(reader);
            }
            (_, Some(error)) => {
                // A form stays open with the reason in it, so a typo is fixed
                // where it was made rather than typed out again.
                if !self.form_error(&error) {
                    self.message = Some(Message::bad(error));
                }
            }
            (_, None) => self.succeeded(task, &dones),
        }
    }

    fn succeeded(&mut self, task: &Task, dones: &[Done]) {
        let said = dones
            .last()
            .and_then(|done| done.stdout.lines().next())
            .unwrap_or("done");
        // Ran, but found something wrong on the way: an edit that left the
        // skill invalid is not news to give in green.
        let clean = dones.iter().all(|done| done.ok);
        self.message = Some(if clean {
            Message::good(said)
        } else {
            Message::warn(said)
        });

        match task {
            Task::InitWorkspace | Task::Link { .. } | Task::InitAndLink { .. } => {
                if let Task::Link { path } | Task::InitAndLink { path } = task {
                    self.select_member(path);
                }
                // Straight back to linking while there is something left to
                // link: a workspace is usually set up several projects at once.
                let more = self
                    .workspace()
                    .is_some_and(|workspace| !workspace.unlinked.is_empty());
                self.mode = if more {
                    Mode::Linking(LinkForm::default())
                } else {
                    Mode::Browse
                };
            }
            Task::Add { kind, name, .. } | Task::Rename { kind, to: name, .. } => {
                self.mode = Mode::Browse;
                self.select_item(*kind, name);
            }
            // The project the TUI was opened on, now a project, or now one a
            // workspace manages, which is what the screen shows from here on.
            Task::InitProject | Task::LinkHere => {
                self.mode = Mode::Browse;
                let here = self.directory.clone();
                self.cursor = 0;
                self.settle_in();
                self.focus_on(&here);
                self.message = Some(Message::good(said));
            }
            _ => self.mode = Mode::Browse,
        }
    }

    /// Put an error into the form that is open, if one is.
    fn form_error(&mut self, error: &str) -> bool {
        let slot = match &mut self.mode {
            Mode::Adding(form) => &mut form.error,
            Mode::Renaming(form) => &mut form.error,
            Mode::Linking(form) => &mut form.error,
            Mode::Loading(form) => &mut form.error,
            Mode::Gathering(form) => &mut form.error,
            Mode::Searching(form) => &mut form.error,
            Mode::Exporting(form) => &mut form.error,
            Mode::Backing(form) => &mut form.error,
            Mode::Settings(form) => &mut form.error,
            Mode::Targets(form) => &mut form.error,
            _ => return false,
        };
        *slot = Some(error.to_owned());
        true
    }

    /// Take a fresh reading of the disk, keeping the cursor on what it was on
    /// wherever that has moved to.
    fn reload(&mut self, view: View) {
        let entry = self.member().map(|member| member.entry.clone());
        let item = self
            .current_item()
            .map(|item| (item.kind, item.name.clone()));
        self.view = view;

        let members = self.workspace().map(|workspace| {
            let at = entry.and_then(|entry| {
                workspace
                    .members
                    .iter()
                    .position(|member| member.entry == entry)
            });
            (at, workspace.members.len())
        });
        let Some((at, count)) = members else {
            self.cursor = 0;
            self.item = 0;
            self.focus = Focus::Projects;
            return;
        };
        self.cursor = at.unwrap_or(self.cursor).min(count.saturating_sub(1));

        let items = self.member().map(|member| {
            let at = item.and_then(|(kind, name)| {
                member
                    .items()
                    .iter()
                    .position(|item| item.kind == kind && item.name == name)
            });
            (at, member.items().len())
        });
        let (at, count) = items.unwrap_or((None, 0));
        self.item = at.unwrap_or(self.item).min(count.saturating_sub(1));
        if count == 0 {
            self.focus = Focus::Projects;
        }
    }

    /// Move the cursor to the project a path names.
    fn select_member(&mut self, path: &str) {
        let Some(workspace) = self.workspace() else {
            return;
        };
        let target = paths::normalize(&workspace.root.join(path));
        // Not the workspace's own row, which shares the root with a project
        // linked as `.`.
        if let Some(at) = workspace
            .members
            .iter()
            .position(|member| !member.own && member.root == target)
        {
            self.cursor = at;
            self.item = 0;
            self.focus = Focus::Projects;
        }
    }

    /// Move the cursor to an artifact of the current project.
    fn select_item(&mut self, kind: Kind, name: &str) {
        let at = self.member().and_then(|member| {
            member
                .items()
                .iter()
                .position(|item| item.kind == kind && item.name == name)
        });
        if let Some(at) = at {
            self.item = at;
            self.focus = Focus::Items;
        }
    }
}

/// What `t` and `T` say of `project`, which a `settings.json` marks: said
/// before a form opens, as a `mind.toml`'s is, rather than by the command
/// the form would run, which core refuses the same way.
fn unchanged(project: &str) -> String {
    format!(
        "{project} is marked by a settings.json, which is read as it was written and not changed; \
         `mind migrate` moves it to a settings.toml"
    )
}

/// The kind after this one, round and round, among the kinds `add` writes,
/// as core says ([`create::scaffolds`]): one it refuses would be a choice
/// the form offers only to refuse.
fn other_kind(kind: Kind) -> Kind {
    let kinds: Vec<Kind> = Kind::ALL
        .into_iter()
        .filter(|&kind| create::scaffolds(kind))
        .collect();
    let at = kinds.iter().position(|each| *each == kind).unwrap_or(0);
    kinds[(at + 1) % kinds.len()]
}

/// The next pick among `count` choices, where no pick at all is a choice too
/// and comes first: none, the first, …, the last, none again.
fn step_through(current: Option<usize>, count: usize, forward: bool) -> Option<usize> {
    match (current, forward) {
        (_, _) if count == 0 => None,
        (None, true) => Some(0),
        (None, false) => Some(count - 1),
        (Some(at), true) => (at + 1 < count).then_some(at + 1),
        (Some(at), false) => at.checked_sub(1),
    }
}

fn gathering(mut form: GatherForm, key: KeyEvent) -> (Mode, Step) {
    form.error = None;
    match key.code {
        KeyCode::Esc => return (Mode::Browse, Step::Stay),
        KeyCode::Enter => {
            return match form.task() {
                Some(task) => (Mode::Gathering(form), Step::Run(task)),
                None => {
                    form.error = Some(String::from("a repository URL is needed"));
                    (Mode::Gathering(form), Step::Stay)
                }
            };
        }
        KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % GATHER_FIELDS.len(),
        KeyCode::BackTab | KeyCode::Up => {
            form.field = (form.field + GATHER_FIELDS.len() - 1) % GATHER_FIELDS.len();
        }
        _ => {
            form.input_mut().press(key);
        }
    }
    (Mode::Gathering(form), Step::Stay)
}

/// The search form's keys: typing the query, and enter searches: the skill
/// directories' line running apart as a gather does, stopped by `esc`
/// between two finders, and the registry's as any other line.
fn searching(mut form: SearchForm, key: KeyEvent) -> (Mode, Step) {
    form.error = None;
    match key.code {
        KeyCode::Esc => return (Mode::Browse, Step::Stay),
        KeyCode::Enter => {
            return match form.task() {
                Some(task) => (Mode::Searching(form), Step::Run(task)),
                None => {
                    form.error = Some(String::from("say what to look for"));
                    (Mode::Searching(form), Step::Stay)
                }
            };
        }
        _ => {
            form.query.press(key);
        }
    }
    (Mode::Searching(form), Step::Stay)
}

/// The results' keys: `↑↓` picks one, enter gathers it, `f` searches again,
/// what was looked for already typed, and `esc` leaves.
fn results(mut screen: Results, key: KeyEvent) -> (Mode, Step) {
    let count = screen.hits.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => return (Mode::Browse, Step::Stay),
        KeyCode::Up | KeyCode::Char('k') => screen.pick = screen.pick.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') if screen.pick + 1 < count => screen.pick += 1,
        KeyCode::Char('f') => {
            let form = SearchForm {
                query: Input::with(&screen.query),
                error: None,
                of: Searched::Skills,
            };
            return (Mode::Searching(form), Step::Stay);
        }
        KeyCode::Enter => {
            if let Some(task) = screen.task() {
                return (Mode::Results(screen), Step::Run(task));
            }
        }
        _ => {}
    }
    (Mode::Results(screen), Step::Stay)
}

/// The registry's results' keys: `↑↓` picks one, enter says what adding it
/// would write, which the page it opens writes on `y`, `r` searches again,
/// what was looked for already typed, and `esc` leaves.
fn servers(mut screen: ServerResults, key: KeyEvent) -> (Mode, Step) {
    let count = screen.servers.len();
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => return (Mode::Browse, Step::Stay),
        KeyCode::Up | KeyCode::Char('k') => screen.pick = screen.pick.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') if screen.pick + 1 < count => screen.pick += 1,
        KeyCode::Char('r') => {
            let form = SearchForm {
                query: Input::with(&screen.query),
                error: None,
                of: Searched::Servers,
            };
            return (Mode::Searching(form), Step::Stay);
        }
        KeyCode::Enter => {
            if let Some(task) = screen.task() {
                return (Mode::Servers(screen), Step::Run(task));
            }
        }
        _ => {}
    }
    (Mode::Servers(screen), Step::Stay)
}

/// The export form's keys: typing on the folder's line, a space on a kind's
/// or on the yes ticks it, and enter runs what it says, once asked about
/// when it may take something away.
fn exporting(mut form: ExportForm, key: KeyEvent) -> (Mode, Step) {
    form.error = None;
    let last = ExportForm::lines() - 1;
    match key.code {
        KeyCode::Esc => return (Mode::Browse, Step::Stay),
        KeyCode::Enter => {
            let task = match form.task() {
                Ok(task) => task,
                Err(missing) => {
                    form.error = Some(missing.to_owned());
                    return (Mode::Exporting(form), Step::Stay);
                }
            };
            if !form.yes {
                return (Mode::Exporting(form), Step::Run(task));
            }
            // Asked here, worded for what it is about, as a deletion is:
            // `--yes` is what lets it delete.
            let confirm = Confirm {
                question: vec![
                    format!(
                        "Export into {}, changing or removing what an earlier export wrote there?",
                        form.folder.text().trim()
                    ),
                    String::new(),
                    String::from(
                        "What the library no longer holds is deleted from the folder, and what \
                         changed is written over, edits made there since included.",
                    ),
                ],
                task,
                also: None,
                back: Box::new(Mode::Exporting(form)),
            };
            return (Mode::Confirming(confirm), Step::Stay);
        }
        KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % (last + 1),
        KeyCode::BackTab | KeyCode::Up => form.field = (form.field + last) % (last + 1),
        KeyCode::Char(' ') if form.field > 0 => form.tick(),
        _ if form.field == 0 => {
            form.folder.press(key);
        }
        _ => {}
    }
    (Mode::Exporting(form), Step::Stay)
}

/// The backup form's keys: typing on its four lines of text, a space on a
/// kind's line ticks it, and enter runs it, apart, as a gather runs.
fn backing(mut form: BackupForm, key: KeyEvent) -> (Mode, Step) {
    form.error = None;
    let count = BackupForm::lines();
    match key.code {
        KeyCode::Esc => return (Mode::Browse, Step::Stay),
        KeyCode::Enter => {
            return match form.task() {
                Ok(task) => (Mode::Backing(form), Step::Run(task)),
                Err(missing) => {
                    form.error = Some(missing.to_owned());
                    (Mode::Backing(form), Step::Stay)
                }
            };
        }
        KeyCode::Tab | KeyCode::Down => form.field = (form.field + 1) % count,
        KeyCode::BackTab | KeyCode::Up => form.field = (form.field + count - 1) % count,
        _ => match form.input_mut() {
            Some(input) => {
                input.press(key);
            }
            None if key.code == KeyCode::Char(' ') => form.tick(),
            None => {}
        },
    }
    (Mode::Backing(form), Step::Stay)
}

fn confirming(confirm: Confirm, key: KeyEvent) -> (Mode, Step) {
    match key.code {
        // Back to where it was asked from while it runs, so a failure lands in
        // the form that led here.
        KeyCode::Char('y') | KeyCode::Enter => (*confirm.back, Step::Run(confirm.task)),
        KeyCode::Char('n') | KeyCode::Esc => (*confirm.back, Step::Stay),
        KeyCode::Char(key) if confirm.also.as_ref().is_some_and(|also| also.key == key) => {
            let task = confirm.also.map(|also| also.task);
            (*confirm.back, task.map_or(Step::Stay, Step::Run))
        }
        _ => (Mode::Confirming(confirm), Step::Stay),
    }
}

fn reading(mut reader: Reader, key: KeyEvent) -> (Mode, Step) {
    let last = reader.lines.len().saturating_sub(1);
    match key.code {
        KeyCode::Char('f') if reader.searches => {
            return (Mode::Searching(SearchForm::default()), Step::Stay);
        }
        KeyCode::Char('r') if reader.searches => {
            return (
                Mode::Searching(SearchForm::of(Searched::Servers)),
                Step::Stay,
            );
        }
        // Back to browsing while it runs, as a confirmation goes back to
        // where it was asked from: what it says lands in a page of its own.
        KeyCode::Char('y') if reader.writes.is_some() => {
            if let Some(task) = reader.writes.take() {
                return (Mode::Browse, Step::Run(task));
            }
        }
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => return (Mode::Browse, Step::Stay),
        KeyCode::Up | KeyCode::Char('k') => reader.scroll = reader.scroll.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => reader.scroll = (reader.scroll + 1).min(last),
        KeyCode::PageUp => reader.scroll = reader.scroll.saturating_sub(PAGE),
        KeyCode::PageDown | KeyCode::Char(' ') => {
            reader.scroll = (reader.scroll + PAGE).min(last);
        }
        KeyCode::Home | KeyCode::Char('g') => reader.scroll = 0,
        KeyCode::End | KeyCode::Char('G') => reader.scroll = last,
        _ => {}
    }
    (Mode::Reading(reader), Step::Stay)
}

// ---------------------------------------------------------------------------
// Forms, questions and pages
// ---------------------------------------------------------------------------

/// One line of text being typed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Input {
    text: String,
    /// In characters, not bytes, so a key never lands inside one.
    cursor: usize,
}

impl Input {
    /// An input that starts out holding `text`, with the cursor at its end.
    pub fn with(text: &str) -> Self {
        Self {
            text: text.to_owned(),
            cursor: text.chars().count(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Apply an editing key, and say whether it was one.
    pub fn press(&mut self, key: KeyEvent) -> bool {
        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            // Ctrl-U clears the line, as it does at a shell prompt.
            KeyCode::Char('u') if control => {
                self.text.clear();
                self.cursor = 0;
            }
            KeyCode::Char(c) if !control && !key.modifiers.contains(KeyModifiers::ALT) => {
                let at = self.byte(self.cursor);
                self.text.insert(at, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            KeyCode::Delete if self.cursor < self.len() => {
                let at = self.byte(self.cursor);
                self.text.remove(at);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.len(),
            _ => return false,
        }
        true
    }

    fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// The byte offset of a character position.
    fn byte(&self, position: usize) -> usize {
        self.text
            .char_indices()
            .nth(position)
            .map_or(self.text.len(), |(at, _)| at)
    }
}

/// Which line of the add form the keyboard is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddField {
    Kind,
    Template,
    Name,
    Description,
}

impl AddField {
    const ORDER: [AddField; 4] = [
        AddField::Kind,
        AddField::Template,
        AddField::Name,
        AddField::Description,
    ];

    fn next(self) -> Self {
        let at = Self::ORDER
            .iter()
            .position(|each| *each == self)
            .unwrap_or(0);
        Self::ORDER[(at + 1) % Self::ORDER.len()]
    }

    fn previous(self) -> Self {
        let at = Self::ORDER
            .iter()
            .position(|each| *each == self)
            .unwrap_or(0);
        Self::ORDER[(at + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

/// A skill or a rule about to be created.
#[derive(Debug, Clone)]
pub struct AddForm {
    /// The project it goes into.
    pub member: usize,
    pub kind: Kind,
    /// Which of that kind's templates, if any.
    pub template: Option<usize>,
    pub name: Input,
    pub description: Input,
    pub field: AddField,
    /// Why the last attempt did not work.
    pub error: Option<String>,
}

impl AddForm {
    /// Opens on the name, because that is what is typed first; the kind and
    /// the template are the lines above it.
    pub fn new(member: usize) -> Self {
        Self {
            member,
            kind: Kind::Skill,
            template: None,
            name: Input::default(),
            description: Input::default(),
            field: AddField::Name,
            error: None,
        }
    }
}

/// An artifact about to get a new name.
#[derive(Debug, Clone)]
pub struct RenameForm {
    /// The project it is in.
    pub member: usize,
    pub kind: Kind,
    /// The name it has now.
    pub from: String,
    /// The name it will have, starting out as the one it has: most renames
    /// change a word, not the whole thing.
    pub name: Input,
    pub error: Option<String>,
}

impl RenameForm {
    pub fn new(member: usize, kind: Kind, from: String) -> Self {
        Self {
            member,
            kind,
            name: Input::with(&from),
            from,
            error: None,
        }
    }
}

/// A project about to be linked: typed, or picked from what the workspace
/// found under itself.
#[derive(Debug, Clone, Default)]
pub struct LinkForm {
    pub path: Input,
    /// Which of the unlinked directories is picked.
    pub pick: usize,
    pub error: Option<String>,
}

/// A weaver workspace about to be loaded, typed; or one already loaded,
/// picked to unload.
#[derive(Debug, Clone, Default)]
pub struct LoadForm {
    pub path: Input,
    /// Which of the loaded workspaces is picked.
    pub pick: usize,
    pub error: Option<String>,
}

/// The sources the shelf is filled from, one picked: what `S` opens.
#[derive(Debug, Clone, Default)]
pub struct SourcesScreen {
    /// Which of the workspace's sources is picked, in the order the
    /// snapshot lists them.
    pub pick: usize,
}

/// The lines of the gather form, in order.
pub const GATHER_FIELDS: [&str; 3] = ["url", "folder", "ref"];

/// A repository about to be gathered from.
#[derive(Debug, Clone)]
pub struct GatherForm {
    pub url: Input,
    /// The folder inside it to take skills from; empty for all of it.
    pub folder: Input,
    /// A branch or tag; empty for the repository's default.
    pub reference: Input,
    /// Which of [`GATHER_FIELDS`] the keyboard is on.
    pub field: usize,
    pub error: Option<String>,
}

/// Empty rather than holding the default folder, `.`, which a line names by
/// saying nothing: the hint under the form says what an empty one means.
impl Default for GatherForm {
    fn default() -> Self {
        Self {
            url: Input::default(),
            folder: Input::default(),
            reference: Input::default(),
            field: 0,
            error: None,
        }
    }
}

impl GatherForm {
    /// The input the keyboard is on.
    pub fn input(&self, field: usize) -> &Input {
        match field {
            0 => &self.url,
            1 => &self.folder,
            _ => &self.reference,
        }
    }

    fn input_mut(&mut self) -> &mut Input {
        match self.field {
            0 => &mut self.url,
            1 => &mut self.folder,
            _ => &mut self.reference,
        }
    }

    /// What it would run, if it were submitted now. Nothing without a URL.
    pub fn task(&self) -> Option<Task> {
        let url = self.url.text().trim();
        if url.is_empty() {
            return None;
        }
        let folder = match self.folder.text().trim() {
            "" => DEFAULT_SUBDIRECTORY,
            folder => folder,
        };
        let reference = Some(self.reference.text().trim())
            .filter(|reference| !reference.is_empty())
            .map(str::to_owned);
        // Skills, the form having no field for the kind: rules are
        // gathered from a command line, or a task the API is sent.
        Some(Task::Gather {
            url: url.to_owned(),
            folder: folder.to_owned(),
            reference,
            kind: mindweaver_core::gather::DEFAULT_KIND,
        })
    }
}

/// What a search form looks in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Searched {
    /// The skill directories the workspace enables: `f` on the shelf's page.
    #[default]
    Skills,
    /// The official MCP Registry: `r` on the shelf's page.
    Servers,
}

/// The skill directories, or the MCP Registry, about to be searched for
/// what `query` finds: what `f` and `r` open on the shelf's page.
#[derive(Debug, Clone, Default)]
pub struct SearchForm {
    pub query: Input,
    pub error: Option<String>,
    pub of: Searched,
}

impl SearchForm {
    /// An empty form searching what `of` names.
    pub fn of(of: Searched) -> Self {
        SearchForm {
            of,
            ..SearchForm::default()
        }
    }

    /// What it would run, if it were submitted now: nothing with no query,
    /// which nothing answers. A query the registry finds too short is the
    /// line's to refuse, in core's words.
    pub fn task(&self) -> Option<Task> {
        let query = self.query.text().trim();
        if query.is_empty() {
            return None;
        }
        let query = query.to_owned();
        Some(match self.of {
            Searched::Skills => Task::SearchSkills { query },
            Searched::Servers => Task::SearchServers { query },
        })
    }
}

/// What a search of the skill directories found, one of it picked to take:
/// the screen a search from the shelf opens once it is over.
#[derive(Debug, Clone)]
pub struct Results {
    /// What was looked for.
    pub query: String,
    /// Each result, as core listed it, numbered from 1 on the screen as on
    /// a command line.
    pub hits: Vec<Hit>,
    /// What the search said beside them, a line each: a finder skipped for
    /// want of a token, one that failed, a result dropped.
    pub said: Vec<String>,
    /// Which result is picked.
    pub pick: usize,
}

impl Results {
    /// What the search for `query` printed, read back: its results from the
    /// JSON the line printed, by the names the envelope's `data` gives them,
    /// and what it said on stderr; or why it could not be read.
    ///
    /// The JSON rather than the table, since a table's columns are for
    /// reading, and the envelope's names are a contract a test holds.
    pub fn of(query: &str, dones: &[Done]) -> Result<Self, String> {
        let unreadable = || String::from("the search's answer could not be read");
        let done = dones.last().ok_or_else(unreadable)?;
        let envelope: serde_json::Value =
            serde_json::from_str(&done.stdout).map_err(|_| unreadable())?;
        let hits = envelope
            .pointer("/data/hits")
            .cloned()
            .ok_or_else(unreadable)?;
        let hits: Vec<Hit> = serde_json::from_value(hits).map_err(|_| unreadable())?;
        Ok(Results {
            query: query.to_owned(),
            hits,
            said: done.stderr.clone(),
            pick: 0,
        })
    }

    /// The gather that takes the result picked: `mindweaver gather git` of its
    /// repository, at the branch a finder named, from its folder there, as
    /// `gather search --take` takes it, but by the line it amounts to, so
    /// the transcript gathers the same again however the finders answer by
    /// then.
    pub fn task(&self) -> Option<Task> {
        let hit = self.hits.get(self.pick)?;
        Some(Task::Gather {
            url: hit.repository.to_string(),
            folder: hit.folder().to_owned(),
            reference: hit.reference.clone(),
            kind: Kind::Skill,
        })
    }
}

/// What a search of the MCP Registry found, one of it picked to add: the
/// screen a search from the shelf opens once it is over.
#[derive(Debug, Clone)]
pub struct ServerResults {
    /// What was looked for.
    pub query: String,
    /// Each server, as core listed it.
    pub servers: Vec<Summary>,
    /// What the search said beside them, a line each: a server that did
    /// not read, and why the listing stopped short.
    pub said: Vec<String>,
    /// Which server is picked.
    pub pick: usize,
}

impl ServerResults {
    /// What the search for `query` printed, read back: its servers from
    /// the JSON the line printed, by the names the envelope's `data` gives
    /// them, and what it said on stderr; or why it could not be read.
    pub fn of(query: &str, dones: &[Done]) -> Result<Self, String> {
        let unreadable = || String::from("the registry search's answer could not be read");
        let done = dones.last().ok_or_else(unreadable)?;
        let envelope: serde_json::Value =
            serde_json::from_str(&done.stdout).map_err(|_| unreadable())?;
        let servers = envelope
            .pointer("/data/servers")
            .cloned()
            .ok_or_else(unreadable)?;
        let servers: Vec<Summary> = serde_json::from_value(servers).map_err(|_| unreadable())?;
        Ok(ServerResults {
            query: query.to_owned(),
            servers,
            said: done.stderr.clone(),
            pick: 0,
        })
    }

    /// What adding the server picked would write, at the version listed,
    /// so what was read is what is added however the registry has moved
    /// on since: `mindweaver gather mcp add`, a dry run, whose page writes
    /// it on `y`.
    pub fn task(&self) -> Option<Task> {
        let server = self.servers.get(self.pick)?;
        Some(Task::ImportServer {
            server: server.name.to_string(),
            version: Some(server.version.clone()),
            yes: false,
        })
    }
}

/// Every kind, in core's order, ticked: what the export and backup forms
/// start from, which names no kind on the line, every kind being what
/// naming none asks.
fn every_kind() -> Vec<(Kind, bool)> {
    Kind::ALL.into_iter().map(|kind| (kind, true)).collect()
}

/// The kinds a form's ticks ask for: none named when every one is ticked,
/// which asks for every kind, and those ticked otherwise; refused with no
/// kind ticked, which would be no export at all.
fn kinds_ticked(kinds: &[(Kind, bool)]) -> Result<Vec<Kind>, &'static str> {
    let ticked: Vec<Kind> = kinds
        .iter()
        .filter(|(_, ticked)| *ticked)
        .map(|(kind, _)| *kind)
        .collect();
    match ticked.len() {
        0 => Err("tick at least one kind"),
        all if all == kinds.len() => Ok(Vec::new()),
        _ => Ok(ticked),
    }
}

/// The workspace's own about to be exported into a folder: what `x` opens.
#[derive(Debug, Clone)]
pub struct ExportForm {
    /// The folder to write into, as `mindweaver export` takes it, from the
    /// workspace's root.
    pub folder: Input,
    /// Every kind, in core's order, and whether it is ticked.
    pub kinds: Vec<(Kind, bool)>,
    /// Whether what an earlier export wrote there may be changed or
    /// removed: `--yes`, asked about before it runs.
    pub yes: bool,
    /// Which line the keyboard is on: the folder, each kind, then the yes.
    pub field: usize,
    pub error: Option<String>,
}

impl Default for ExportForm {
    fn default() -> Self {
        Self {
            folder: Input::default(),
            kinds: every_kind(),
            yes: false,
            field: 0,
            error: None,
        }
    }
}

impl ExportForm {
    /// How many lines the keyboard moves over: the folder, a kind each,
    /// and the yes.
    pub fn lines() -> usize {
        Kind::ALL.len() + 2
    }

    /// The line of the yes.
    pub fn yes_field() -> usize {
        Self::lines() - 1
    }

    /// Tick the kind, or the yes, the keyboard is on.
    fn tick(&mut self) {
        match self.field {
            0 => {}
            at if at == Self::yes_field() => self.yes = !self.yes,
            at => {
                if let Some((_, ticked)) = self.kinds.get_mut(at - 1) {
                    *ticked = !*ticked;
                }
            }
        }
    }

    /// What it would run, if it were submitted now, or what it still
    /// lacks: a folder, and a kind.
    pub fn task(&self) -> Result<Task, &'static str> {
        let folder = self.folder.text().trim();
        if folder.is_empty() {
            return Err("a folder to export into is needed");
        }
        Ok(Task::Export {
            folder: folder.to_owned(),
            kinds: kinds_ticked(&self.kinds)?,
            yes: self.yes,
        })
    }
}

/// The lines of the backup form that are typed, in order.
pub const BACKUP_FIELDS: [&str; 4] = ["url", "branch", "message", "token env"];

/// The workspace's own about to be backed up to a git repository: what `b`
/// opens.
#[derive(Debug, Clone)]
pub struct BackupForm {
    /// The repository, as `git` takes it: an https or ssh URL, or a path
    /// from the workspace's root.
    pub url: Input,
    /// The branch to commit to; empty for the repository's own.
    pub branch: Input,
    /// What the commit says; empty for the command's own words.
    pub message: Input,
    /// The name of an environment variable holding a token, never a token;
    /// empty to log in as `git` is configured to.
    pub token_env: Input,
    /// Every kind, in core's order, and whether it is ticked.
    pub kinds: Vec<(Kind, bool)>,
    /// Which line the keyboard is on: one of [`BACKUP_FIELDS`], then each
    /// kind.
    pub field: usize,
    pub error: Option<String>,
}

impl Default for BackupForm {
    fn default() -> Self {
        Self {
            url: Input::default(),
            branch: Input::default(),
            message: Input::default(),
            token_env: Input::default(),
            kinds: every_kind(),
            field: 0,
            error: None,
        }
    }
}

impl BackupForm {
    /// How many lines the keyboard moves over.
    pub fn lines() -> usize {
        BACKUP_FIELDS.len() + Kind::ALL.len()
    }

    /// The line of [`BACKUP_FIELDS`] at `field`.
    pub fn input(&self, field: usize) -> &Input {
        match field {
            0 => &self.url,
            1 => &self.branch,
            2 => &self.message,
            _ => &self.token_env,
        }
    }

    /// The line being typed on, when the keyboard is on one.
    fn input_mut(&mut self) -> Option<&mut Input> {
        match self.field {
            0 => Some(&mut self.url),
            1 => Some(&mut self.branch),
            2 => Some(&mut self.message),
            3 => Some(&mut self.token_env),
            _ => None,
        }
    }

    /// Tick the kind the keyboard is on.
    fn tick(&mut self) {
        if let Some((_, ticked)) = self
            .field
            .checked_sub(BACKUP_FIELDS.len())
            .and_then(|at| self.kinds.get_mut(at))
        {
            *ticked = !*ticked;
        }
    }

    /// What it would run, if it were submitted now, or what it still
    /// lacks: a repository, and a kind.
    pub fn task(&self) -> Result<Task, &'static str> {
        let url = self.url.text().trim();
        if url.is_empty() {
            return Err("a repository URL is needed");
        }
        let given = |input: &Input| {
            Some(input.text().trim())
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        Ok(Task::Backup {
            url: url.to_owned(),
            branch: given(&self.branch),
            kinds: kinds_ticked(&self.kinds)?,
            message: given(&self.message),
            token_env: given(&self.token_env),
        })
    }
}

/// A project's agents and share mode, toggled: what `t` opens.
#[derive(Debug, Clone)]
pub struct SettingsForm {
    /// The project it is for.
    pub member: usize,
    /// Every agent, in report order, and whether it is ticked.
    pub ticked: Vec<(Agent, bool)>,
    pub share: Share,
    pub edit_tracked: bool,
    /// Which line the keyboard is on: an agent's, then the share mode's,
    /// then `edit_tracked`'s ([`SettingsForm::FIELDS`] in all).
    pub field: usize,
    pub error: Option<String>,
    /// What the project names now, in its own order, which a change keeps
    /// for the agents it keeps.
    named: Vec<Agent>,
    was: (Share, bool),
}

impl SettingsForm {
    /// A line for each agent, the share mode and `edit_tracked`.
    pub const FIELDS: usize = Agent::ALL.len() + 2;

    /// Opens on the first agent, each ticked as the project names it.
    pub fn new(member: usize, named: &[Agent], share: Share, edit_tracked: bool) -> Self {
        Self {
            member,
            ticked: Agent::ALL
                .iter()
                .map(|agent| (*agent, named.contains(agent)))
                .collect(),
            share,
            edit_tracked,
            field: 0,
            error: None,
            named: named.to_vec(),
            was: (share, edit_tracked),
        }
    }

    /// The line of the share mode, after the agents'.
    pub const fn share_field() -> usize {
        Agent::ALL.len()
    }

    /// What a key does to the line the keyboard is on: space ticks an agent
    /// or `edit_tracked`, and on the share mode, it and ← → pick the other.
    fn toggle(&mut self, key: KeyCode) {
        let share = Self::share_field();
        match self.field {
            at if at < share && key == KeyCode::Char(' ') => {
                if let Some((_, ticked)) = self.ticked.get_mut(at) {
                    *ticked = !*ticked;
                }
            }
            at if at == share => {
                self.share = match self.share {
                    Share::Local => Share::Shared,
                    Share::Shared => Share::Local,
                };
            }
            at if at == share + 1 && key == KeyCode::Char(' ') => {
                self.edit_tracked = !self.edit_tracked;
            }
            _ => {}
        }
    }

    /// The agents ticked, in report order.
    fn ticked(&self) -> Vec<Agent> {
        self.ticked
            .iter()
            .filter(|(_, ticked)| *ticked)
            .map(|(agent, _)| *agent)
            .collect()
    }

    /// The task the toggles amount to for `holder`: none while they say
    /// what the project says already. Built by the task module, from what
    /// the project said when the form opened and what the toggles say now,
    /// so the panel's toggles amount to the same lines.
    fn task(&self, holder: Holder) -> Option<Task> {
        let (share, edit_tracked) = self.was;
        let was = Toggles {
            agents: self.named.clone(),
            share,
            edit_tracked,
        };
        let now = Toggles {
            agents: self.ticked(),
            share: self.share,
            edit_tracked: self.edit_tracked,
        };
        Task::settings(holder, &was, &now)
    }
}

/// Where a project installs a kind, typed: what `T` opens.
#[derive(Debug, Clone)]
pub struct TargetsForm {
    /// The project it is for.
    pub member: usize,
    /// The kind it is about: skills, the one installed for now.
    pub kind: Kind,
    /// The folders, relative to the project, separated by spaces; empty for
    /// where its agents read. Starts out holding what the settings say.
    pub folders: Input,
    pub error: Option<String>,
    /// What the settings name now.
    was: Option<Vec<String>>,
}

impl TargetsForm {
    pub fn new(member: usize, kind: Kind, was: Option<Vec<String>>) -> Self {
        Self {
            member,
            kind,
            folders: Input::with(&was.as_deref().unwrap_or_default().join(" ")),
            error: None,
            was,
        }
    }

    /// The task the form amounts to for `holder`: none while it says what
    /// the project says already. Which folders an install may use is core's
    /// to say, when the command runs, so the form takes whatever is typed.
    ///
    /// Split on spaces, as a shell would split the line without quotes: a
    /// folder whose name holds one is named on the command line instead.
    fn task(&self, holder: Holder) -> Option<Task> {
        let typed: Vec<String> = self
            .folders
            .text()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let folders = (!typed.is_empty()).then_some(typed);
        if folders == self.was {
            return None;
        }
        Some(Task::Targets {
            holder,
            kind: self.kind,
            folders,
        })
    }
}

/// A yes or no before something that writes.
#[derive(Debug)]
pub struct Confirm {
    pub question: Vec<String>,
    /// What yes runs.
    pub task: Task,
    /// A second yes, under a key of its own, that runs something more: the
    /// same question answered "and everywhere else too".
    pub also: Option<Also>,
    /// Where any answer goes back to.
    pub back: Box<Mode>,
}

/// A second yes a [`Confirm`] offers: the key that gives it, the word the
/// keys line calls it, and what it runs.
#[derive(Debug)]
pub struct Also {
    pub key: char,
    pub label: &'static str,
    pub task: Task,
}

impl Confirm {
    /// Whether to make a directory a mind project so it can be linked.
    fn init_and_link(path: String, back: Mode) -> Self {
        Self {
            question: vec![
                format!("{path} is not a mind project yet."),
                String::new(),
                String::from(
                    "Make it one (a .mind marker kept out of git, and empty skills/ and rules/ folders) and link it?",
                ),
            ],
            task: Task::InitAndLink { path },
            also: None,
            back: Box::new(back),
        }
    }
}

/// A page of output to read.
#[derive(Debug, Clone)]
pub struct Reader {
    pub title: String,
    pub lines: Vec<(Tone, String)>,
    /// The first line shown.
    pub scroll: usize,
    /// Whether `f` on it searches the skill directories and `r` the MCP
    /// Registry: the shelf's page, where what is not on the shelf yet is
    /// looked for.
    pub searches: bool,
    /// What `y` on it runs: what adding a registry's server writes, on the
    /// page saying what it would write.
    pub writes: Option<Task>,
}

impl Reader {
    /// What some commands said: their output, their warnings, and why they
    /// failed if they did, in that order.
    pub fn of(dones: &[Done]) -> Self {
        let title = dones
            .iter()
            .map(|done| done.command.as_str())
            .collect::<Vec<_>>()
            .join(" && ");
        let mut lines = Vec::new();
        for done in dones {
            lines.extend(
                done.stdout
                    .lines()
                    .map(|line| (Tone::Plain, line.to_owned())),
            );
            lines.extend(done.stderr.iter().map(|line| (Tone::Warn, line.clone())));
            if let Some(error) = &done.error {
                lines.push((Tone::Bad, format!("error: {error}")));
            }
        }
        if lines.is_empty() {
            lines.push((Tone::Info, String::from("nothing to show")));
        }
        Self {
            title,
            lines,
            scroll: 0,
            searches: false,
            writes: None,
        }
    }

    /// Every key, and the command it runs.
    pub fn help() -> Self {
        let rows: [(&str, &str, &[&str]); 25] = [
            (
                "a",
                "add a skill or a rule",
                &["mind -C <project> add <kind> <name> <description>"],
            ),
            (
                "enter",
                "show an artifact",
                &["mind -C <project> show <kind:name>"],
            ),
            (
                "e",
                "edit it, in $VISUAL or $EDITOR",
                &["mind -C <project> edit <kind:name>"],
            ),
            (
                "r",
                "rename it",
                &["mind -C <project> rename <kind:name> <new-name>"],
            ),
            (
                "d",
                "delete it, once you say yes; on the ◆ row, e takes it out of every project too",
                &[
                    "mind -C <project> remove --yes <kind:name>",
                    "mindweaver remove --everywhere --yes <kind:name>, with e on the workspace's own",
                ],
            ),
            (
                "l",
                "link a project",
                &[
                    "mindweaver link <path>",
                    "mind -C <path> init, first, for a repository that is not one yet",
                ],
            ),
            (
                "u",
                "update what is installed in the project, or the artifact under the cursor",
                &[
                    "mind -C <project> update --yes [<kind:name>]",
                    "mindweaver update --all --yes [<kind:name>], on the workspace's own",
                ],
            ),
            ("U", "unlink the project", &["mindweaver unlink <path>"]),
            (
                "M",
                "relink a stale project to where the scan found it moved",
                &["mindweaver relink <old> <new>"],
            ),
            (
                "A",
                "adopt somebody's skill under the cursor into the library, once you say yes",
                &["mindweaver adopt -p <project> --yes <kind:name>"],
            ),
            (
                "t",
                "the project's agents and sharing, toggled",
                &[
                    "mind -C <project> agents --yes <agent>...",
                    "mind -C <project> share --yes local|shared [--edit-tracked]",
                ],
            ),
            (
                "T",
                "where the project installs skills, instead of where its agents read",
                &[
                    "mind -C <project> targets set --yes skills <dir>...",
                    "mind -C <project> targets reset --yes skills, with none typed",
                ],
            ),
            (
                "L",
                "load another workspace's own skills, read live from there",
                &[
                    "mindweaver load <path>",
                    "mindweaver unload <path>, on one already loaded",
                    "mind load / mind unload, on a project on its own",
                ],
            ),
            ("v", "validate everything", &["mindweaver validate"]),
            (
                "g",
                "gather from a git repository",
                &["mindweaver gather git <url>"],
            ),
            (
                "x",
                "export the library into a folder, a workspace of its own",
                &["mindweaver export [--kind <kind>]... [--yes] <folder>"],
            ),
            (
                "b",
                "back the library up to a git repository",
                &["mindweaver backup [--branch <branch>] [--kind <kind>]... [--token-env <variable>] <url>"],
            ),
            (
                "esc",
                "while a gather, an update of sources or a backup runs: stop it, until it places or pushes",
                &[],
            ),
            (
                "s",
                "what is on the shelf; f there searches the skill directories, enter on a result gathers it; r searches the MCP Registry, enter on a server says what adding it writes, y there writes it",
                &[
                    "mindweaver gather list",
                    "mindweaver gather search --json <query>",
                    "mindweaver gather git [--path <folder>] [--ref <branch>] <url>",
                    "mindweaver gather mcp search --json <query>",
                    "mindweaver gather mcp add [--version <version>] [--yes] <server>",
                ],
            ),
            (
                "S",
                "the sources it is filled from: u gathers one again, U every one, d forgets one once you say yes",
                &[
                    "mindweaver gather update <name>",
                    "mindweaver gather update --all",
                    "mindweaver gather remove --yes <name>",
                ],
            ),
            (
                "w",
                "the workspaces this machine knows, the one in use marked",
                &["mindweaver workspaces"],
            ),
            ("i", "install skills and rules", &["mindweaver install"]),
            (
                "m",
                "which minds the workspace manages, ticked",
                &["mindweaver link <path>...", "mindweaver unlink <path>..."],
            ),
            ("R", "read the disk again", &[]),
            ("q", "quit, printing what was run", &[]),
        ];
        let mut lines = vec![
            (
                Tone::Plain,
                String::from("Every key here runs a command you could type instead."),
            ),
            (Tone::Plain, String::new()),
        ];
        // Each command on a line of its own under what the key does: side by
        // side, the longest wraps into the next key's row on any terminal
        // narrower than a hundred columns.
        for (key, what, commands) in rows {
            lines.push((Tone::Plain, format!("  {key:<6} {what}")));
            for command in commands {
                lines.push((Tone::Info, format!("         {command}")));
            }
        }
        lines.push((Tone::Plain, String::new()));
        lines.push((
            Tone::Info,
            String::from(
                "On the workspace's own row the same keys run `mindweaver <cmd>` instead of `mind -C <project> <cmd>`.",
            ),
        ));
        lines.push((
            Tone::Info,
            String::from(
                "A project whose own commands would reach another workspace runs `mind -W . -C <project> <cmd>`, so the line acts on this one.",
            ),
        ));
        lines.push((
            Tone::Info,
            String::from(
                "What the link form and m offer is what `mindweaver scan --suggest` lists, in its order.",
            ),
        ));
        let legend: Vec<String> = Agent::ALL
            .iter()
            .map(|agent| format!("{} {}", super::ui::badge(*agent), agent.name()))
            .collect();
        lines.push((
            Tone::Info,
            format!(
                "A project's rows say how each artifact stands, as `mindweaver status` does, and which of its agents read it: {}.",
                legend.join(", ")
            ),
        ));
        Self {
            title: String::from("keys"),
            lines,
            scroll: 0,
            searches: false,
            writes: None,
        }
    }
}

#[cfg(test)]
#[path = "state_test.rs"]
mod tests;
