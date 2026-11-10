//! What every CLI suite shares: running the real parsers, and the fixtures
//! and screens they drive.
//!
//! Everything is re-exported so a suite says `use common::*;` and nothing
//! else; not every suite uses every helper.

#![allow(dead_code, unused_imports)]

pub mod contract;
pub mod mcp_registry;
pub mod mother;
pub mod panel;

pub use clap::Parser;
pub use mindweaver_cli::install::state::{Focus, Screen, Step};
pub use mindweaver_cli::install::{self as install_screen, ui};
pub use mindweaver_cli::tui;
pub use mindweaver_cli::tui::state::{
    AgentsLine, App, Done, Holder, Mode, Runner, Scope, Task, View,
};
pub use mindweaver_cli::{
    agent_homes, default_home, run, run_watched, run_weaver_cli, run_weaver_cli_watched,
    AgentHomeOptions, Cli, CliError, Failure, Lookup, Outcome, WeaverCli,
};
// The watch every line is run with, of either run's step, and a gather's
// step as core says it, which a test wraps in it.
pub use mindweaver_cli::progress::{Progress, Watch};
pub use mindweaver_core::backup::Progress as Backup;
pub use mindweaver_core::gather::Progress as Gather;
pub use mindweaver_core::install::Standing;
pub use mindweaver_core::ledger::Ledger;
pub use mindweaver_core::{AgentHomes, Reason, WeaverWorkspace};
pub use mindweaver_core::{
    Kind, MindProject, MIND_CONFIG, MIND_DIR, SETTINGS_FILE, SETTINGS_JSON_FILE, WEAVER_CONFIG,
    WEAVER_DIR,
};
pub use ratatui::backend::TestBackend;
pub use ratatui::crossterm::event::KeyCode;
pub use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
pub use ratatui::Terminal;
pub use std::fs;
pub use std::path::{Path, PathBuf};
pub use tempfile::TempDir;

/// Every option that would otherwise send a command looking outside the
/// fixture, which `line` pins empty unless a test gives its own: `--home`, so
/// no test falls back to, or creates, the workspace in the real home;
/// `--workspace`, so none picks up a workspace the developer's shell
/// activated; and each agent home, so none reads the developer's real agent
/// folders.
pub const PINNED: [&str; 6] = [
    "--home",
    "--workspace",
    "--claude-config-dir",
    "--codex-home",
    "--opencode-config-dir",
    "--agents-dir",
];

/// The short spelling of an option in [`PINNED`] that has one.
const SHORT: [(&str, &str); 1] = [("--workspace", "-W")];

/// Build an argument line with `-C dir` appended, and every option in
/// [`PINNED`] set to empty.
pub fn line(binary: &str, args: &[&str], dir: &Path) -> Vec<String> {
    let mut line: Vec<String> = vec![binary.to_owned()];
    line.extend(args.iter().map(|arg| (*arg).to_owned()));
    line.push("-C".to_owned());
    line.push(dir.to_string_lossy().into_owned());
    for option in PINNED {
        if !gives(args, option) {
            line.push(format!("{option}="));
        }
    }
    line
}

/// Whether a test gave `option` itself, in any spelling clap takes, the
/// short one included: pinning it as well would be an option given twice,
/// which clap refuses.
fn gives(args: &[&str], option: &str) -> bool {
    let short = SHORT
        .iter()
        .find(|(long, _)| *long == option)
        .map(|(_, short)| *short);
    args.iter().any(|arg| {
        arg.strip_prefix(option)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
            || short.is_some_and(|short| arg.starts_with(short))
    })
}

/// Every variable that would send a spawned binary outside the fixture: to
/// the real home, a workspace the developer's shell activated, or the real
/// agent folders. Under Mindweaver's names and Mindflayer's, which a binary
/// still reads while the new ones are not set, so a shell an older build
/// activated reaches no test either.
pub const CLEARED: [&str; 7] = [
    "MINDWEAVER_HOME",
    "MINDWEAVER_WORKSPACE",
    "MINDFLAYER_HOME",
    "MINDFLAYER_WORKSPACE",
    "CLAUDE_CONFIG_DIR",
    "CODEX_HOME",
    "OPENCODE_CONFIG_DIR",
];

/// Run the built `binary` from `dir`, as a person would from a shell, with
/// `env` as the only variables of [`CLEARED`] it sees, and a temporary
/// directory as its home.
///
/// For what an in-process test cannot reach: what the environment says.
/// Setting a variable in a test would set it for every test running in the
/// same process. The home is a directory rather than empty because, with
/// `HOME` empty, the standard library falls back to the account's real one.
pub fn spawn(binary: &str, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Spawned {
    let home = TempDir::new().unwrap();
    let output = command(binary, dir, args, env, home.path())
        .output()
        .expect("run the binary");
    Spawned {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        ok: output.status.success(),
    }
}

/// [`spawn`], with `input` on its standard input and nothing after it: for
/// what a command reads from a pipe, as `edit --contents -` does.
pub fn spawn_fed(
    binary: &str,
    dir: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    input: &[u8],
) -> Spawned {
    use std::io::Write as _;
    use std::process::Stdio;

    let home = TempDir::new().unwrap();
    let mut child = command(binary, dir, args, env, home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start the binary");
    // Dropped once written, so the command reads to its end.
    child
        .stdin
        .take()
        .expect("its standard input")
        .write_all(input)
        .expect("write its standard input");
    let output = child.wait_with_output().expect("run the binary");
    Spawned {
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        ok: output.status.success(),
    }
}

/// [`spawn`], left running with its stdout and stderr piped: for a command
/// that goes on until it is stopped, `ui`, whose output is read as it goes.
pub fn start(binary: &str, dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Started {
    let home = TempDir::new().unwrap();
    let child = command(binary, dir, args, env, home.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("start the binary");
    Started { child, _home: home }
}

/// The command [`spawn`] and [`start`] run: `binary` from `dir`, with `env`
/// the only variables of [`CLEARED`] it sees and `home` as its home.
fn command(
    binary: &str,
    dir: &Path,
    args: &[&str],
    env: &[(&str, &str)],
    home: &Path,
) -> std::process::Command {
    let program = match binary {
        "mind" => env!("CARGO_BIN_EXE_mind"),
        "mindweaver" => env!("CARGO_BIN_EXE_mindweaver"),
        _ => panic!("no binary called {binary}"),
    };
    let mut command = std::process::Command::new(program);
    command.current_dir(dir).args(args);
    for variable in CLEARED {
        command.env_remove(variable);
    }
    command.env("HOME", home).env("USERPROFILE", home);
    command.envs(env.iter().copied());
    command
}

/// A binary [`start`] left running, killed if the test is done with it
/// before it stops, so a failed assertion leaves no server behind.
pub struct Started {
    pub child: std::process::Child,
    /// Its home, kept for as long as it runs.
    _home: TempDir,
}

impl Drop for Started {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// What a spawned binary printed, and whether it exited successfully.
#[derive(Debug)]
pub struct Spawned {
    pub stdout: String,
    pub stderr: String,
    pub ok: bool,
}

/// What `mind ...` parses into, pinned as [`mind`] runs it.
///
/// `try_parse_from` rather than `parse_from`: the latter exits the process on a
/// parse error, which in a test harness kills the whole run and hides which
/// argument line was at fault.
pub fn parsed_mind(dir: &Path, args: &[&str]) -> Cli {
    let line = line("mind", args, dir);
    Cli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"))
}

/// What `mindweaver ...` parses into, through the second binary's parser.
pub fn parsed_weaver(dir: &Path, args: &[&str]) -> WeaverCli {
    let line = line("mindweaver", args, dir);
    WeaverCli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}"))
}

/// Run `mind ...`.
pub fn mind(dir: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    run(&parsed_mind(dir, args))
}

/// Run `mindweaver ...`.
pub fn weaver(dir: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    run_weaver_cli(&parsed_weaver(dir, args))
}

/// Run a line a front end built (`Task::commands`), its binary first, with
/// `-C dir` and every option in [`PINNED`] right after the binary rather
/// than appended as [`line`] appends them: after a `--`, which a line puts
/// before positionals led by a dash, an option appended would be read as
/// one more positional. The line names no directory of its own.
pub fn typed(dir: &Path, words: &[String]) -> Result<Outcome, Failure> {
    let (binary, rest) = words.split_first().expect("a line names its binary");
    let mut line = vec![
        binary.clone(),
        String::from("-C"),
        dir.to_string_lossy().into_owned(),
    ];
    line.extend(PINNED.iter().map(|option| format!("{option}=")));
    line.extend(rest.iter().cloned());
    match binary.as_str() {
        "mind" => {
            run(&Cli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}")))
        }
        "mindweaver" => run_weaver_cli(
            &WeaverCli::try_parse_from(&line).unwrap_or_else(|error| panic!("{line:?}: {error}")),
        ),
        other => panic!("no binary called {other}"),
    }
}

/// What every reading command's `--json` prints, read back field by field:
/// nothing else is allowed in it, so a field renamed or added fails here
/// before it reaches a script that reads it.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub version: u32,
    pub data: serde_json::Value,
    pub warnings: Vec<String>,
    pub coded_warnings: Vec<Coded>,
}

/// One of an envelope's `coded_warnings`.
#[derive(Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Coded {
    pub code: Option<String>,
    pub message: String,
}

/// The envelope a `--json` outcome printed, its version held to the one this
/// suite was written against, and its data read as `T`.
///
/// Its `coded_warnings` are held to say what its `warnings` say, in the same
/// order, so every test of a report checks that the two agree.
pub fn enveloped<T: serde::de::DeserializeOwned>(outcome: &Outcome) -> (T, Vec<String>) {
    let envelope = envelope(outcome);
    let data = serde_json::from_value(envelope.data)
        .unwrap_or_else(|error| panic!("not the data asked for: {error}\n{}", outcome.stdout));
    (data, envelope.warnings)
}

/// The code of each warning a `--json` outcome printed, in order, as its
/// `coded_warnings` say: none for one no code is given to.
pub fn warning_codes(outcome: &Outcome) -> Vec<Option<String>> {
    envelope(outcome)
        .coded_warnings
        .into_iter()
        .map(|coded| coded.code)
        .collect()
}

/// The envelope a `--json` outcome printed, as [`enveloped`] holds it.
fn envelope(outcome: &Outcome) -> Envelope {
    let envelope: Envelope = serde_json::from_str(&outcome.stdout)
        .unwrap_or_else(|error| panic!("not the envelope: {error}\n{}", outcome.stdout));
    assert_eq!(envelope.version, 2, "{}", outcome.stdout);
    let messages: Vec<&str> = envelope
        .coded_warnings
        .iter()
        .map(|coded| coded.message.as_str())
        .collect();
    assert_eq!(messages, envelope.warnings, "{}", outcome.stdout);
    envelope
}

/// The code of each line an outcome said on stderr, in order: what
/// [`warning_codes`] reads back from the JSON of the same report.
pub fn stderr_codes(outcome: &Outcome) -> Vec<Option<String>> {
    outcome
        .stderr
        .iter()
        .map(|line| line.code.map(str::to_owned))
        .collect()
}

/// Write a skill into an already initialized mind project.
/// Where an initialized project keeps a kind, asked of the project itself
/// rather than assumed, so a test that moves one still writes to the right
/// place.
pub fn directory_for(root: &Path, kind: Kind) -> PathBuf {
    MindProject::open(root)
        .expect("an initialized mind project")
        .directory_for(kind)
        .unwrap()
}

/// Every copy an install puts of the skill `name` in the project at `root`,
/// one in each folder core's `install::targets` gives: asked of core as
/// [`directory_for`] is, so a test reads the same for a project marked by its
/// settings, whose agents' folders take the copies, and one a `mind.toml`
/// marks, whose one folder does.
pub fn copies(root: &Path, name: &str) -> Vec<PathBuf> {
    let project = MindProject::open(root).expect("an initialized mind project");
    let targets = mindweaver_core::install::targets(&project, Kind::Skill)
        .expect("somewhere to install into");
    assert!(!targets.is_empty(), "{}: no target", root.display());
    targets
        .into_iter()
        .map(|target| target.folder.join(name))
        .collect()
}

/// Where the workspace at `root` keeps its own of a kind, asked of the
/// workspace as [`directory_for`] asks a project.
pub fn own_directory_for(root: &Path, kind: Kind) -> PathBuf {
    WeaverWorkspace::open(root)
        .expect("an initialized weaver workspace")
        .own()
        .directory_for(kind)
        .unwrap()
}

pub fn write_skill(root: &Path, directory: &str, contents: &str) {
    let dir = directory_for(root, Kind::Skill).join(directory);
    fs::create_dir_all(&dir).expect("create the skill directory");
    fs::write(dir.join("SKILL.md"), contents).expect("write SKILL.md");
}

pub fn skill_file(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nSteps.\n")
}

/// A workspace with `alpha` and `beta` linked, each holding one skill.
pub fn workspace_with_two() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        new_project(&root);
        write_skill(&root, name, &skill_file(name, "A skill"));
        weaver(dir.path(), &["link", name]).unwrap();
    }
    dir
}

/// A workspace with `gamma` and `delta` initialized under it but not linked.
pub fn workspace_with_two_unlinked() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    for name in ["gamma", "delta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        new_project(&root);
    }
    dir
}

/// Run `mindweaver ...` with `home` as the home directory.
pub fn weaver_at_home(dir: &Path, home: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    let mut args: Vec<&str> = args.to_vec();
    let home = home.to_string_lossy().into_owned();
    args.push("--home");
    args.push(&home);
    weaver(dir, &args)
}

/// Run `mind ...` with `home` as the home directory.
pub fn mind_at_home(dir: &Path, home: &Path, args: &[&str]) -> Result<Outcome, Failure> {
    let mut args: Vec<&str> = args.to_vec();
    let home = home.to_string_lossy().into_owned();
    args.push("--home");
    args.push(&home);
    mind(dir, &args)
}

/// The path `add` said it created `what` (`skill deploy`) at, as a path: one
/// joined from a marker's `/` and the platform's own separator compares
/// equal to its other spellings, where the text would not on Windows.
pub fn created(outcome: &Outcome, what: &str) -> PathBuf {
    let path = outcome
        .stdout
        .strip_prefix(&format!("created {what} at "))
        .and_then(|rest| rest.strip_suffix('\n'))
        .unwrap_or_else(|| panic!("not one creation of {what}: {:?}", outcome.stdout));
    PathBuf::from(path)
}

/// Write a rule into an already initialized mind project.
pub fn write_rule(root: &Path, route: &str, contents: &str) {
    let path = directory_for(root, Kind::Rule).join(route);
    fs::create_dir_all(path.parent().unwrap()).expect("create the rule folder");
    fs::write(path, contents).expect("write the rule");
}

/// The names in `dir`, sorted: what a test holds a folder to when nothing
/// else may be there.
pub fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// What git would call untracked in the repository at `work_tree`, as `git
/// status` lists it: a folder holding only what is ignored is not listed, and
/// neither is an empty one. Asked of gix in process, with the repository
/// opened isolated, so neither a `git` binary nor the user's git config has
/// a say.
pub fn untracked(work_tree: &Path) -> Vec<String> {
    let repository = gix::open_opts(work_tree, gix::open::Options::isolated()).unwrap();
    let index = repository.index_or_empty().unwrap();
    let options = repository
        .dirwalk_options()
        .unwrap()
        .emit_untracked(gix::dir::walk::EmissionMode::CollapseDirectory);
    repository
        .dirwalk_iter(index, None::<&str>, Default::default(), options)
        .unwrap()
        .map(|item| item.unwrap().entry.rela_path.to_string())
        .collect()
}

/// A git repository holding these files, with one commit.
pub fn repository(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, contents) in files {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    let repo = gix::init(dir.path()).expect("initialize a repository");
    let tree = write_tree(&repo, dir.path());
    let who = gix::actor::Signature {
        name: "Fixture".into(),
        email: "fixture@example.com".into(),
        time: gix::date::Time::new(0, 0),
    };
    let id = repo
        .write_object(&gix::objs::Commit {
            tree,
            parents: Default::default(),
            author: who.clone(),
            committer: who,
            encoding: None,
            message: "fixture".into(),
            extra_headers: Vec::new(),
        })
        .unwrap()
        .detach();

    let head = fs::read_to_string(dir.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().strip_prefix("ref: ").unwrap();
    let reference = dir.path().join(".git").join(branch);
    fs::create_dir_all(reference.parent().unwrap()).unwrap();
    fs::write(reference, format!("{id}\n")).unwrap();
    dir
}

pub fn write_tree(repo: &gix::Repository, dir: &Path) -> gix::ObjectId {
    use gix::objs::tree::{Entry, EntryKind};

    let mut entries = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }
        let path = entry.path();
        let (kind, oid) = if path.is_dir() {
            (EntryKind::Tree, write_tree(repo, &path))
        } else {
            (
                EntryKind::Blob,
                repo.write_blob(fs::read(&path).unwrap()).unwrap().detach(),
            )
        };
        entries.push(Entry {
            mode: kind.into(),
            filename: name.to_string_lossy().as_bytes().into(),
            oid,
        });
    }
    entries.sort();
    repo.write_object(&gix::objs::Tree { entries })
        .unwrap()
        .detach()
}

/// Put every file under `work_tree`, a repository, in its index, as `git
/// add` would: what git then tracks, without a commit.
pub fn indexed(work_tree: &Path) {
    let repository = gix::open_opts(work_tree, gix::open::Options::isolated()).unwrap();
    let tree = write_tree(&repository, work_tree);
    let mut index = repository.index_from_tree(&tree).unwrap();
    index.write(Default::default()).unwrap();
}

/// Make `root`, a folder, a mind project as a person would: a git
/// repository, given `mind init`.
///
/// A repository because a project usually is one, and because a local
/// project's install keeps its copies out of git through the repository's
/// exclude file: outside one, every install would say on stderr that nothing
/// does, and each test reading stderr would have to read past it.
pub fn new_project(root: &Path) {
    gix::init(root).expect("initialize a repository");
    mind(root, &["init"]).unwrap();
}

/// Make `root`, a folder, a project the way a version 2 `mind init` did:
/// marked by a `mind.toml`, and authoring in `skills/` and `rules/`, which
/// is where an install puts its copy too.
///
/// For what such a project does that one marked by its settings does not: a
/// copy installed in the folder the project authors in, so that a change
/// made from the project reaches it. Made through core
/// rather than `mind init`, so these tests keep such a project whatever
/// `mind init` writes.
pub fn older_project(root: &Path) {
    MindProject::init(root).expect("mark a project with a mind.toml");
}

/// Make `root`, a folder, a project the way a version 3 `mind init` did: a
/// git repository whose `.mind` holds a `settings.json` naming every agent,
/// local, and authoring in `skills/` and `rules/`, and the `.gitignore` that
/// keeps it out of git ([`ProjectMother::json`]), so these tests keep such a
/// project whatever `mind init` writes.
///
/// [`ProjectMother::json`]: mindweaver_core::mother::ProjectMother::json
pub fn json_project(root: &Path) {
    gix::init(root).expect("initialize a repository");
    mindweaver_core::mother::ProjectMother::json(
        root,
        &mindweaver_core::mother::SettingsMother::authoring(),
    );
}

/// A workspace with `projects` linked and a shelf holding `skills`.
pub fn shelved(projects: &[&str], skills: &[&str]) -> (TempDir, WeaverWorkspace, Ledger) {
    shelved_with(projects, skills, new_project)
}

/// [`shelved`], each project made the way an older Mindweaver made it
/// ([`older_project`]).
pub fn shelved_older(projects: &[&str], skills: &[&str]) -> (TempDir, WeaverWorkspace, Ledger) {
    shelved_with(projects, skills, older_project)
}

/// [`shelved`], each project made by `make`.
fn shelved_with(
    projects: &[&str],
    skills: &[&str],
    make: fn(&Path),
) -> (TempDir, WeaverWorkspace, Ledger) {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    for name in projects {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        make(&root);
        weaver(dir.path(), &["link", name]).unwrap();
    }

    let files: Vec<(String, String)> = skills
        .iter()
        .map(|name| {
            (
                format!("skills/{name}/SKILL.md"),
                skill_file(name, &format!("What {name} does")),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(p, b)| (p.as_str(), b.as_str()))
        .collect();
    let source = repository(&borrowed);
    weaver(
        dir.path(),
        &["gather", "git", &source.path().to_string_lossy()],
    )
    .unwrap();

    let workspace = WeaverWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();
    (dir, workspace, ledger)
}

pub fn screen_for(workspace: &WeaverWorkspace, ledger: &Ledger) -> Screen {
    let (projects, _) = install_screen::registered(workspace);
    install_screen::build(workspace, ledger, projects, &AgentHomes::default()).unwrap()
}

/// Everything the screen would show, as plain text.
pub fn rendered(screen: &Screen) -> String {
    let mut terminal = Terminal::new(TestBackend::new(96, 24)).unwrap();
    terminal.draw(|frame| ui::draw(screen, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(96)
        .map(|line| line.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn press(screen: &mut Screen, keys: &[KeyCode]) -> Step {
    let mut step = Step::Stay;
    for key in keys {
        step = screen.press(*key);
    }
    step
}

pub fn tui_in(dir: &Path) -> App {
    App::new(
        dir.to_path_buf(),
        tui::load(dir, &Lookup::default()).unwrap(),
    )
}

pub fn press_key(app: &mut App, code: KeyCode) -> tui::state::Step {
    app.press(KeyEvent::from(code))
}

pub fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.press(KeyEvent::from(KeyCode::Char(c)));
    }
}

/// Press a key and, if it asked for work, do the work the way the loop does.
pub fn press_and_run(app: &mut App, code: KeyCode) -> Option<Task> {
    match press_key(app, code) {
        tui::state::Step::Run(task) => {
            tui::step(app, task.clone());
            // To its end, as the loop's frames would see it, when it runs
            // apart.
            tui::wait(app);
            Some(task)
        }
        _ => None,
    }
}

pub fn tui_drawn(app: &App) -> String {
    tui_drawn_at(app, 110, 30)
}

/// The screen drawn into a terminal `width` columns wide and `height` rows
/// high, a row a line.
pub fn tui_drawn_at(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| tui::ui::draw(app, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|line| line.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn registry(dir: &Path) -> String {
    fs::read_to_string(dir.join(WEAVER_DIR).join(WEAVER_CONFIG)).unwrap()
}

/// `workspace_with_two`, plus `gamma`, a mind project nobody linked, and
/// `repo`, a git repository that is not a mind project yet.
pub fn workspace_with_candidates() -> TempDir {
    let dir = workspace_with_two();
    let gamma = dir.path().join("gamma");
    fs::create_dir(&gamma).unwrap();
    new_project(&gamma);
    fs::create_dir_all(dir.path().join("repo/.git")).unwrap();
    dir
}

/// Install `deploy` from the shelf into `collapse`, through the screen.
pub fn installed(dir: &Path, workspace: &WeaverWorkspace, ledger: &Ledger) {
    let mut screen = screen_for(workspace, ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    install_screen::apply(workspace, &screen, &Lookup::default());
    for copy in copies(&dir.join("collapse"), "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

/// `workspace_with_two`, with a skill of the workspace's own.
pub fn workspace_with_own() -> TempDir {
    let dir = workspace_with_two();
    weaver(
        dir.path(),
        &[
            "add",
            "skill",
            "commit-style",
            "How every repo here writes commits",
        ],
    )
    .unwrap();
    dir
}

/// The TUI `mind` opens in `dir`.
pub fn mind_tui_in(dir: &Path) -> App {
    let mut app = App::opened_on(
        Scope::Project,
        dir.to_path_buf(),
        tui::load_project(dir, &Lookup::default()).unwrap(),
    );
    app.focus_on(dir);
    app
}
