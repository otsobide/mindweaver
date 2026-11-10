//! The TUI: every project a workspace manages, and a key for each command.
//!
//! `mindweaver` on its own opens it. What it changes, it changes by running a
//! command line (`mindweaver link beta`, `mind -C beta add skill deploy '…'`),
//! parsed by the parsers the binaries use and run through the same functions,
//! and those lines are printed when it closes. There is nothing the screen can
//! do that a command cannot, and nothing a command says that the screen says
//! differently.
//!
//! Split the way the install screen it embeds is: [`state`] is what a key
//! does, [`ui`] is what the screen looks like, and this file is the loop and
//! the work: the only parts that touch a terminal or a disk.

pub mod minds;
pub mod state;
pub mod ui;

use std::io::{self, IsTerminal as _};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::progress::Watch;
use mindweaver_core::code::Said;
use mindweaver_core::doctor::Examined;
use mindweaver_core::resolve::{self, resolve, Intent, Resolved};
use mindweaver_core::status::{EntryStatus, State};
use mindweaver_core::view::{self, Directory, Held, LoadedWorkspace, Snapshot};
use mindweaver_core::{
    scan, AgentHomes, Kind, MarkerFile, MindProject, Template, WeaverWorkspace, WorkspaceError,
};
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::terminal::{self, EnterAlternateScreen};
use ratatui::DefaultTerminal;

use crate::commands::project::scan_warnings;
use crate::commands::status::{unread, unseen};
use crate::commands::workspace::same_directory;
use crate::commands::{consulted, run_line, Ran};
use crate::task::{shell, Refusal, Task};
use crate::{install, CliError, Failure, Lookup, Outcome};
use state::{
    App, Choice, Done, Holdings, Item, LoadedSource, Member, Mode, Performed, Runner, Running,
    Scope, Step, Unlinked, Update, View, Workspace,
};

/// How long the loop waits for a key before drawing again, so a resize
/// repaints without waiting for one.
const IDLE: Duration = Duration::from_millis(200);
/// The same while a task runs apart: oftener, so its count moves as the work
/// does.
const WATCHING: Duration = Duration::from_millis(50);

/// Open the TUI on the workspace `lookup` resolves from `directory`, and run
/// it until it is closed.
///
/// What comes back is the transcript: every command that changed something,
/// so the session can be read, and repeated, once the screen is gone.
pub fn run(directory: &Path, lookup: &Lookup) -> Result<Outcome, Failure> {
    // Checked before anything is drawn: a screen written into a pipe is noise
    // in whatever reads it.
    if !io::stdout().is_terminal() {
        return Err(CliError::NotATerminal.into());
    }
    let app =
        App::new(directory.to_path_buf(), load(directory, lookup)?).with_lookup(lookup.clone());
    open(app, "mindweaver")
}

/// Open the TUI straight onto the minds screen: what `mindweaver minds` does.
pub fn run_minds(directory: &Path, roots: &[PathBuf], lookup: &Lookup) -> Result<Outcome, Failure> {
    if !io::stdout().is_terminal() {
        return Err(CliError::NotATerminal.into());
    }
    open(minds(directory, roots, lookup)?, "mindweaver minds")
}

/// The TUI `mindweaver minds` opens from `directory`: on the minds screen of the
/// workspace the TUI itself would open there, offering what is under
/// `roots`, absolute, or under the workspace root when there are none.
///
/// Read from `directory` rather than from the root of a workspace found
/// first: the default workspace before it is made has no marker to stop a
/// walk up from its root, which would carry on to any workspace above it.
pub fn minds(directory: &Path, roots: &[PathBuf], lookup: &Lookup) -> Result<App, CliError> {
    let view = load_under(directory, lookup, roots)?;
    if matches!(view, View::Nowhere) {
        return Err(CliError::NotInWorkspace(directory.to_path_buf()));
    }
    let mut app = App::new(directory.to_path_buf(), view)
        .with_lookup(lookup.clone())
        .with_roots(roots.to_vec());
    if let Some(workspace) = app.workspace() {
        app.mode = Mode::Minds(minds::Minds::of(workspace));
    }
    Ok(app)
}

/// Open the TUI on the project at or above `directory`: what `mind` does on
/// its own.
///
/// A project a workspace manages opens in that workspace, on the project:
/// the same screen `mindweaver` opens, since that is where the project's
/// neighbours and its installs are. One no workspace manages opens on its own.
pub fn run_project(directory: &Path, lookup: &Lookup) -> Result<Outcome, Failure> {
    if !io::stdout().is_terminal() {
        return Err(CliError::NotATerminal.into());
    }
    let mut app = App::opened_on(
        Scope::Project,
        directory.to_path_buf(),
        load_project(directory, lookup)?,
    )
    .with_lookup(lookup.clone());
    app.focus_on(directory);
    open(app, "mind")
}

/// Run a TUI until it is closed, and hand back its transcript.
fn open(mut app: App, command: &'static str) -> Result<Outcome, Failure> {
    let mut terminal =
        ratatui::try_init().map_err(|source| CliError::NoTerminal { command, source })?;
    let outcome = drive(&mut terminal, &mut app);
    // However the loop ended, a key that asked to leave or a terminal that
    // failed it, a task still running is stopped and waited for rather than
    // left behind, so what it did is in the transcript and nothing is half
    // done when the process ends.
    if let Some(running) = app.running.as_mut() {
        running.cancel();
    }
    wait(&mut app);
    ratatui::restore();
    outcome.map_err(|source| CliError::Screen { source })?;

    Ok(Outcome {
        stdout: app.transcript(),
        stderr: Vec::new(),
        ok: true,
    })
}

/// Draw, wait for a key, do what it asks, repeat.
///
/// The only part a test cannot drive, which is why it holds nothing but the
/// loop: what a key means is [`App::press`]'s answer, what the work does is
/// [`step`]'s, and what the screen looks like is [`ui::draw`]'s.
fn drive(terminal: &mut DefaultTerminal, app: &mut App) -> Result<(), io::Error> {
    loop {
        settle(app);
        terminal.draw(|frame| ui::draw(app, frame))?;

        // A poll rather than a blocking read, so a resize repaints instead of
        // waiting for somebody to press something first.
        let patience = if app.running.is_some() {
            WATCHING
        } else {
            IDLE
        };
        if !event::poll(patience)? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        // Windows reports both press and release; acting on each would do
        // everything twice.
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.press(key) {
            Step::Stay => {}
            // A task still running is waited for by `open`, which does that
            // however the loop ends.
            Step::Quit => return Ok(()),
            // An editor draws a screen of its own, so this one steps aside
            // for it (raw mode off, back to the normal screen) and is drawn
            // again from scratch once the editor is closed.
            Step::Run(task) if task.takes_terminal() => {
                ratatui::restore();
                step(app, task);
                terminal::enable_raw_mode()?;
                ratatui::crossterm::execute!(io::stdout(), EnterAlternateScreen)?;
                terminal.clear()?;
                while event::poll(Duration::ZERO)? {
                    let _ = event::read()?;
                }
            }
            // Started and left to run: the frames that follow show it.
            Step::Run(task) if task.runs_apart() => step(app, task),
            Step::Run(task) => {
                // Said before it starts, because a frozen screen reads as a
                // hung one.
                app.busy = Some(task.describe());
                terminal.draw(|frame| ui::draw(app, frame))?;
                step(app, task);
                app.busy = None;
                // Keys pressed while it ran were aimed at a screen that has
                // since changed, so they are dropped rather than replayed on
                // the new one.
                while event::poll(Duration::ZERO)? {
                    let _ = event::read()?;
                }
            }
        }
    }
}

/// Carry out a task and show what came of it: everything the loop does
/// between the key that asked and the next frame. A task that runs apart is
/// started instead, and [`settle`] shows what came of it once it is over.
///
/// Public so a test can do exactly what the loop does, without a terminal.
pub fn step(app: &mut App, task: Task) {
    if task.runs_apart() {
        start(app, task);
        return;
    }
    let performed = perform(&task, app);
    conclude(app, &task, performed);
}

/// Take in what the task running apart has reported since the last frame,
/// and show what came of it once it is over: what the loop does before each
/// frame, and public for a test to do the same between the frames it draws.
pub fn settle(app: &mut App) {
    if let Some((task, dones)) = app.heard() {
        conclude(app, &task, Performed::Ran(dones));
    }
}

/// Wait for the task running apart to be over, and show what came of it:
/// what leaving the TUI does, and how a test runs one to its end.
pub fn wait(app: &mut App) {
    if let Some((task, dones)) = app.waited() {
        conclude(app, &task, Performed::Ran(dones));
    }
}

/// Run a task's lines on a thread of their own, reporting each step back.
///
/// A thread rather than the loop's own, so the screen goes on drawing while
/// the network takes its time, and answers the key that stops it. The lines
/// are the ones [`perform`] would run, through the same runner.
fn start(app: &mut App, task: Task) {
    let (send, updates) = mpsc::channel();
    let watch = Watch::new();
    let reporting = watch.clone().reporting_to({
        let send = send.clone();
        move |progress| {
            let _ = send.send(Update::Progress(progress));
        }
    });
    let runner = runner(app);
    let lines = app.lines(&task);
    let chained = task.chained();
    let base = app.base().to_path_buf();
    let lookup = app.lookup.clone();
    thread::spawn(move || {
        let dones = run_all(&runner, &lines, &base, &lookup, &reporting, chained);
        // Nobody to tell is a screen already closed, which needs no answer.
        let _ = send.send(Update::Finished(dones));
    });
    app.running = Some(Running::new(task, watch, updates));
}

/// Show what came of a task, with the disk read again when it asked for that.
fn conclude(app: &mut App, task: &Task, performed: Performed) {
    let view = task.reloads().then(|| {
        match app.opened {
            Scope::Workspace => load_under(&app.directory, &app.lookup, &app.roots),
            Scope::Project => load_project(&app.directory, &app.lookup),
        }
        .map_err(|error| error.to_string())
    });
    app.finish(task, performed, view);
}

/// Do the work a task names.
pub fn perform(task: &Task, app: &App) -> Performed {
    let base = app.base().to_path_buf();
    match task {
        Task::OpenInstall => open_install(
            &base,
            app.workspace().is_some_and(|workspace| !workspace.made),
            &app.lookup.agents,
        ),
        // Asked about rather than failed: `mindweaver link` would refuse it, and
        // `mind init` is what would make it work.
        Task::Link { path } if needs_init(&base.join(path)) => {
            Performed::NeedsInit { path: path.clone() }
        }
        _ => Performed::Ran(run_all(
            &runner(app),
            &app.lines(task),
            &base,
            &app.lookup,
            &Watch::new(),
            task.chained(),
        )),
    }
}

/// What runs `app`'s command lines: its stand-in, or the real thing.
fn runner(app: &App) -> Runner {
    app.runner.clone().unwrap_or_else(|| Runner::new(execute))
}

/// A directory that exists and is not a mind project.
fn needs_init(directory: &Path) -> bool {
    directory.is_dir()
        && matches!(
            MindProject::open(directory),
            Err(WorkspaceError::NotAProject { .. })
        )
}

/// Run command lines in order, stopping at the first that fails when they
/// are `chained` ([`Task::chained`]), and running every one when they stand
/// alone.
fn run_all(
    runner: &Runner,
    lines: &[Vec<String>],
    base: &Path,
    lookup: &Lookup,
    watch: &Watch,
    chained: bool,
) -> Vec<Done> {
    let mut dones = Vec::new();
    for words in lines {
        let done = runner.run(words, base, lookup, watch);
        let failed = done.error.is_some();
        dones.push(done);
        if failed && chained {
            break;
        }
    }
    dones
}

/// Run one command line through the real parsers, from `base`.
///
/// The words are what a person would type from the workspace root, so a `-C`
/// or a `-W` among them is resolved against it rather than against wherever
/// this process happened to be started. Where to look for a workspace is
/// otherwise the TUI's own, passed along rather than looked up again: a
/// screen opened on the workspace `-W` names, or the one activated, runs
/// every line against it, and a command that falls back to the default
/// workspace falls back to the one on screen. With neither, each walks up
/// from `base` first, as from the shell, which is why a change to the default
/// before it is made makes it first ([`App::lines`]). A gather among them
/// reports to `watch`, and stops when it is cancelled. All of which is
/// [`run_line`]'s, shared by every front end that runs lines.
pub fn execute(words: &[String], base: &Path, lookup: &Lookup, watch: &Watch) -> Done {
    let command = shell(words);
    // The text alone: a screen shows each line as a terminal would, and a
    // code beside it is for a front end that acts on one, which this does
    // not.
    match run_line(words, base, lookup, watch, |refusal| {
        matches!(refusal, Refusal::Screen | Refusal::Panel | Refusal::Input)
    }) {
        Ran::Refused(Refusal::Screen) => Done::failed(command, SCREEN_IN_A_SCREEN),
        Ran::Refused(Refusal::Panel) => Done::failed(command, PANEL_IN_A_SCREEN),
        Ran::Refused(Refusal::Input) => Done::failed(command, INPUT_IN_A_SCREEN),
        Ran::Refused(refusal) => Done::failed(command, refusal.to_string()),
        Ran::Done(Ok(outcome)) => Done {
            command,
            stdout: outcome.stdout,
            stderr: outcome.stderr.into_iter().map(|line| line.text).collect(),
            error: None,
            ok: outcome.ok,
        },
        Ran::Done(Err(failure)) => Done {
            command,
            stdout: String::new(),
            stderr: failure.warnings.into_iter().map(|line| line.text).collect(),
            error: Some(failure.error.to_string()),
            ok: false,
        },
    }
}

/// Why a command that draws is not run from here: what the TUI refuses of
/// the list every front end shares ([`Refusal`]), one of the three refusals
/// a front end with a terminal has. The rest it runs: an editor, given the
/// terminal ([`Task::takes_terminal`]), and `show`, printed for whoever
/// sits at it.
const SCREEN_IN_A_SCREEN: &str = "opens a screen of its own, so it is not run from inside this one";

/// Why `ui` is not run from here, the second: it serves until Ctrl-C, and the
/// screen takes that key as a key, so nothing would ever stop it.
const PANEL_IN_A_SCREEN: &str =
    "serves the panel until Ctrl-C, which this screen keeps for itself, so it is not run from inside it";

/// Why `edit --contents -` is not run from here, the third: it reads
/// standard input, the terminal whose keys the screen reads, in raw mode,
/// where no key ends it.
const INPUT_IN_A_SCREEN: &str =
    "reads standard input, the keys this screen reads, so it is not run from inside it";

/// Build the install screen for the workspace at `base`, which is the one on
/// screen, and the root the lines it amounts to run from once it is applied:
/// the default workspace too when nothing has made it yet (`unmade`), with
/// nothing to install into.
///
/// Opened there exactly rather than found by walking up from it, as `mindweaver
/// install` would: before the default is made there is no marker at `base`
/// to stop the walk, which would carry on to any workspace above home. Built
/// from the ledger as it is, as `mindweaver install` builds it, so opening the
/// screen makes nothing and scrubs nothing: the first line applying it runs
/// opens the ledger to write, as that command does from the shell.
fn open_install(base: &Path, unmade: bool, homes: &AgentHomes) -> Performed {
    let workspace = if unmade {
        WeaverWorkspace::unmade(base)
    } else {
        WeaverWorkspace::open(base)
    };
    let screen = workspace.map_err(CliError::from).and_then(|workspace| {
        // What would not open is already on screen, beside what did.
        let (projects, _) = install::registered(&workspace);
        install::open(&workspace, projects, homes)
    });
    match screen {
        Ok(screen) => Performed::Install(screen),
        Err(error) => Performed::Ran(vec![Done::failed(
            String::from("mindweaver install"),
            error.to_string(),
        )]),
    }
}

// ---------------------------------------------------------------------------
// Reading what the screen shows
// ---------------------------------------------------------------------------

/// Read everything the screen shows from the disk: the workspace `lookup`
/// resolves from `directory`, as a read resolves it (the default one in
/// home as it will be, when nothing has made it yet).
///
/// Opening a screen is no reason to make it. The first change made from the
/// screen is, and that runs a command that writes, which makes it the way it
/// would from the shell.
pub fn load(directory: &Path, lookup: &Lookup) -> Result<View, CliError> {
    load_under(directory, lookup, &[])
}

/// What [`load`] shows, with what could be linked looked for under `roots`,
/// absolute, rather than under the workspace root: what `mindweaver minds DIR`
/// opens on.
pub fn load_under(directory: &Path, lookup: &Lookup, roots: &[PathBuf]) -> Result<View, CliError> {
    Ok(match resolve(&lookup.inputs(directory, Intent::Read))? {
        Some(resolved) => View::Workspace(Box::new(workspace_view(&resolved, lookup, roots))),
        None => View::Nowhere,
    })
}

/// What `mind` shows: the workspace managing the project at or above
/// `directory`, or that project on its own.
///
/// Opening a project's screen makes nothing: the default workspace in home
/// counts when it already manages the project, and is otherwise only where
/// `mind link` would put it.
pub fn load_project(directory: &Path, lookup: &Lookup) -> Result<View, CliError> {
    let Some(project) = MindProject::locate(directory)? else {
        return Ok(View::Nowhere);
    };
    // The workspace `mind` consults about the project, asked of the function
    // that decides it for the `mind` lines this screen runs, so the two
    // cannot come to disagree.
    let consulted = resolve::consulted(&lookup.inputs(project.root(), Intent::Read), &project)?;
    if let Some(resolved) = &consulted {
        if resolved.workspace.is_linked(project.root()) {
            return Ok(View::Workspace(Box::new(workspace_view(
                resolved,
                lookup,
                &[],
            ))));
        }
    }
    let above = consulted.map(|resolved| resolved.workspace);
    // Read as a snapshot reads each project, with `above`'s templates, the
    // ones `mind add` will find, and no records: it does not link the
    // project, so it says nothing of what was installed there, and doctor,
    // as `mind doctor` examines it, nothing of the servers installed.
    let (read, examined) =
        view::Project::read_examined(&project, above.as_ref(), &lookup.machine());
    Ok(View::Workspace(Box::new(Workspace {
        name: project.name().to_owned(),
        // `.`, because the commands run from the project itself.
        members: vec![held(String::from("."), &read, &examined)],
        root: read.path,
        unlinked: Vec::new(),
        roots: Vec::new(),
        scan_warnings: Vec::new(),
        shelf: 0,
        sources: Vec::new(),
        // What `mind load` has put into the workspace above, so the load form
        // can show it and unload it from here too.
        loaded: above
            .as_ref()
            .map(LoadedWorkspace::of)
            .unwrap_or_default()
            .iter()
            .map(loaded_source)
            .collect(),
        scope: Scope::Project,
        above: above
            .map(|workspace| workspace.root().to_path_buf())
            .or_else(|| lookup.home.clone()),
        reason: None,
        made: true,
    })))
}

/// A workspace as the screen shows it, from the snapshot every front end
/// reads ([`Snapshot`]): its own artifacts first, then every project it
/// manages, and what could be linked under `roots`, or under its root when
/// there are none, which no snapshot holds, being a scan of the disk.
fn workspace_view(resolved: &Resolved, lookup: &Lookup, roots: &[PathBuf]) -> Workspace {
    let workspace = &resolved.workspace;
    let snapshot = Snapshot::of(resolved, lookup.home.as_deref(), &lookup.machine());
    let mut members = vec![own(&snapshot)];
    // What doctor found of each, in the order `projects` lists them.
    members.extend(
        snapshot
            .projects
            .iter()
            .zip(&snapshot.doctor)
            .map(|(project, examined)| member(project, examined, workspace, lookup)),
    );
    // Spelled from the workspace root, the default, where the TUI runs its
    // lines, and in the order `mindweaver scan --suggest` lists them, which core
    // ranks: the screens show it rather than sorting their own.
    let request = scan::Request {
        roots: roots.to_vec(),
        order: scan::Order::Suggested,
        ..scan::Request::default()
    };
    let scanned = scan(workspace, &request);
    let scan_warnings = scan_warnings(&scanned);
    let unlinked = scanned
        .found
        .into_iter()
        .filter(|found| !found.linked)
        .map(|found| Unlinked {
            route: found.route,
            mind: found.mind,
            moved: found.moved,
            reason: found.reason,
            agent_files: found.agent_files,
        })
        .collect();
    Workspace {
        members,
        unlinked,
        roots: roots.to_vec(),
        scan_warnings,
        shelf: snapshot.shelf.len(),
        sources: snapshot.sources.clone(),
        loaded: snapshot.loaded.iter().map(loaded_source).collect(),
        scope: Scope::Workspace,
        above: None,
        reason: Some(snapshot.workspace.reason),
        made: snapshot.workspace.made,
        name: snapshot.workspace.name,
        root: snapshot.workspace.path,
    }
}

/// A workspace this one loads, as the load form shows it: by its entry when
/// it would not open, with why in place of what it offers.
fn loaded_source(loaded: &LoadedWorkspace) -> LoadedSource {
    LoadedSource {
        entry: loaded.entry.clone(),
        root: loaded.path.clone(),
        name: loaded.name.clone().unwrap_or_else(|| loaded.entry.clone()),
        offers: match &loaded.error {
            Some(error) => Err(error.message.to_string()),
            None => Ok(loaded.offers),
        },
    }
}

/// The workspace's own artifacts, its library, as the first row.
fn own(snapshot: &Snapshot) -> Member {
    let library = &snapshot.library;
    Member {
        entry: String::new(),
        root: snapshot.workspace.path.clone(),
        name: snapshot.workspace.name.clone(),
        own: true,
        elsewhere: false,
        // No project's: no settings, no agents, and nothing installed into
        // them, so nothing any agent misses.
        holdings: Ok(Holdings {
            items: items(&library.artifacts),
            agents: Vec::new(),
            named: Vec::new(),
            share: None,
            older: None,
            edit_tracked: false,
            targets: None,
            unreached: Vec::new(),
            found: Vec::new(),
            warnings: messages(&library.failures),
            directories: directories(&library.directories),
            templates: choices(&library.templates),
        }),
    }
}

/// One entry in `screen`'s registry, as its snapshot read it.
///
/// Its lines consult the workspace on screen, whatever `mind` would consult
/// about the project from the shell: where that is another, or cannot be
/// read, the row is [`elsewhere`](Member::elsewhere) and its lines say `-W
/// .`. So its templates are the screen's too, the ones the add form's line
/// will find. Which workspace `mind` consults is the lookup's, which no
/// snapshot holds, so it is asked here.
fn member(
    project: &view::Project,
    examined: &Examined,
    screen: &WeaverWorkspace,
    lookup: &Lookup,
) -> Member {
    let entry = project.entry.clone().unwrap_or_default();
    if let Some(stale) = &project.stale {
        return Member {
            name: entry.clone(),
            entry,
            root: project.path.clone(),
            own: false,
            elsewhere: false,
            holdings: Err(stale.message.to_string()),
        };
    }
    let consults = MindProject::open(&project.path)
        .map_err(CliError::from)
        .and_then(|opened| consulted(&opened, lookup));
    let elsewhere = !matches!(
        consults,
        Ok(Some(workspace)) if same_directory(workspace.root(), screen.root())
    );
    Member {
        elsewhere,
        ..held(entry, project, examined)
    }
}

/// What one project holds, as a snapshot read it, under `entry`, and what
/// doctor found of it.
fn held(entry: String, project: &view::Project, examined: &Examined) -> Member {
    let name = project.name.clone().unwrap_or_else(|| entry.clone());
    // What `status` warns of, said in its words: the folders no install
    // uses first, then the agents that read none of the rest, then each rule
    // some of them read nowhere.
    let mut unreached: Vec<String> = project
        .misplaced
        .iter()
        .map(|misplaced| misplaced.message.to_string())
        .collect();
    unreached.extend(
        project
            .unreached
            .iter()
            .map(|unreached| unread(&name, unreached, false)),
    );
    unreached.extend(project.missed.iter().map(|missed| unseen(&name, missed)));
    // What doctor could not read to tell, beside what the read could not,
    // as `mindweaver doctor` says it on stderr: a check left unmade is no
    // clean bill. A line once, a folder neither could list being one.
    let mut warnings = messages(&project.failures);
    for line in messages(&examined.failures) {
        if !warnings.contains(&line) {
            warnings.push(line);
        }
    }
    let settings = project.settings.as_ref();
    Member {
        entry,
        root: project.path.clone(),
        name,
        own: false,
        elsewhere: false,
        holdings: Ok(Holdings {
            items: items_and_entries(&project.artifacts, &project.entries),
            agents: project.agents.clone(),
            named: settings
                .map(|settings| settings.agents.clone())
                .unwrap_or_default(),
            share: settings.map(|settings| settings.share),
            older: project
                .marker
                .filter(|file| *file != MarkerFile::SettingsToml),
            edit_tracked: settings.is_some_and(|settings| settings.edit_tracked),
            targets: settings.and_then(|settings| {
                settings
                    .targets
                    .iter()
                    .find(|targets| targets.kind == Kind::Skill)
                    .map(|targets| targets.folders.clone())
            }),
            unreached,
            found: examined
                .findings
                .iter()
                .map(|finding| format!("{}; fix: {}", finding.message, finding.fix))
                .collect(),
            warnings,
            directories: directories(&project.directories),
            templates: choices(&project.templates),
        }),
    }
}

/// A holder's artifacts as rows.
fn items(artifacts: &[Held]) -> Vec<Item> {
    artifacts
        .iter()
        .map(|held| Item {
            kind: held.kind,
            name: held.name.to_string(),
            summary: held.summary.clone().unwrap_or_default(),
            // What `validate` would call invalid: a warning is no reason
            // for a mark.
            issues: held
                .issues
                .iter()
                .filter(|issue| !issue.is_warning())
                .map(ToString::to_string)
                .collect(),
            broken: held
                .unread
                .as_ref()
                .map(|unread| unread.message.to_string()),
            state: held.state,
            agents: held.agents.clone(),
        })
        .collect()
}

/// A project's artifacts as rows, and after them each MCP server's entry
/// somebody keeps in one of its agents' config files, as `status` lists
/// it (D-23): a row of its own, foreign, with the agent whose file holds
/// it. One Mindweaver installed is its installation's row already, whose
/// badges say which agents it reaches.
fn items_and_entries(artifacts: &[Held], entries: &[EntryStatus]) -> Vec<Item> {
    let mut rows = items(artifacts);
    rows.extend(
        entries
            .iter()
            .filter(|entry| entry.state == State::Foreign)
            .map(|entry| Item {
                kind: Kind::Mcp,
                name: entry.name.to_string(),
                summary: String::new(),
                issues: Vec::new(),
                broken: None,
                state: Some(entry.state),
                agents: vec![entry.agent],
            }),
    );
    rows
}

/// What each of `said` says, one line each.
fn messages(said: &[Said]) -> Vec<String> {
    said.iter().map(|said| said.message.to_string()).collect()
}

/// Where a holder keeps each kind, relative to it, so an empty one can say
/// where it looked.
fn directories(directories: &[Directory]) -> Vec<(Kind, String)> {
    directories
        .iter()
        .map(|directory| (directory.kind, directory.path.clone()))
        .collect()
}

/// The templates the add form can offer.
fn choices(templates: &[Template]) -> Vec<Choice> {
    templates
        .iter()
        .map(|template| Choice {
            name: template.name.clone(),
            kind: template.kind,
        })
        .collect()
}
