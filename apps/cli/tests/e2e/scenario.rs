//! The world an end-to-end test builds for itself, and what one command run
//! in it gave back.
//!
//! A scenario is a temporary folder standing for `/home`, with `you` in it as
//! the home of the person running the commands. Every path a test spells is
//! spelled as that person would, `~/Projects/collapse` or
//! `/home/you/Projects`, and every path a command prints is read back the
//! same way, so what a test expects reads as the reference shows it
//! (`docs/reference.md`), where the home is `/home/you` too.
//!
//! Each command is the built binary, started as a process from the folder it
//! is typed in, with an environment holding nothing but `PATH` and that home:
//! no `MINDWEAVER_*` or `MINDFLAYER_*` variable, no agent home and no git
//! configuration of the developer's reaches it, and no option is added to the
//! line to keep them out, so the line run is the line typed. A variable a test is about is
//! given to that one command alone.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use mindweaver_core::mother::GitRepositoryMother;
use tempfile::TempDir;

/// What the scenario's root is read as in a command's output, and what a
/// test may spell it as in a line or a path.
const ROOT: &str = "/home";

/// The home's name under the root: `/home/you`, as in the reference.
const YOU: &str = "you";

/// A world of folders, repositories and Mindweaver's own files, built step by
/// step for one test and gone when it ends.
pub struct Scenario {
    /// Kept for its lifetime: dropping it takes the world away.
    root: TempDir,
    /// The root with its links resolved, as a command may print it: on macOS
    /// the system's temporary folder is reached through `/var`, a link.
    resolved: PathBuf,
}

impl Scenario {
    /// An empty world: a home with nothing in it, not even a workspace.
    pub fn empty() -> Self {
        let root = TempDir::new().expect("make the scenario's root");
        fs::create_dir(root.path().join(YOU)).expect("make the home");
        let resolved = root
            .path()
            .canonicalize()
            .expect("resolve the scenario's root");
        Self { root, resolved }
    }

    /// A workspace, `~/Projects`, linking each of `projects`: a git
    /// repository in it given `mind init`, as the reference's
    /// `~/Projects/collapse` is. The starting point of most use cases.
    pub fn projects(projects: &[&str]) -> Self {
        let scenario = Self::empty();
        for project in projects {
            let at = format!("~/Projects/{project}");
            scenario.repository(&at).given(&at, "mind init");
        }
        scenario.given("~/Projects", "mindweaver init");
        for project in projects {
            scenario.given("~/Projects", &format!("mindweaver link {project}"));
        }
        scenario
    }

    /// The home, `/home/you`, where it is on the disk.
    pub fn home(&self) -> PathBuf {
        self.root.path().join(YOU)
    }

    /// Where `spelled`, a path as the person types it (`~/x`, `/home/you/x`),
    /// is on the disk.
    pub fn at(&self, spelled: &str) -> PathBuf {
        if spelled == "~" {
            return self.home();
        }
        if let Some(rest) = spelled.strip_prefix("~/") {
            return self.home().join(rest);
        }
        if let Some(rest) = spelled.strip_prefix(ROOT) {
            if rest.is_empty() || rest.starts_with('/') {
                return self.root.path().join(rest.trim_start_matches('/'));
            }
        }
        panic!("`{spelled}` is not a path under the scenario's {ROOT}: spell it from ~ or {ROOT}")
    }

    // ---------------------------------------------------------------------
    // The world outside Mindweaver: what a person or a git server made.
    // ---------------------------------------------------------------------

    /// A folder at `at`, and the folders above it.
    pub fn folder(&self, at: &str) -> &Self {
        fs::create_dir_all(self.at(at)).expect("make the folder");
        self
    }

    /// A git repository at `at`, with nothing committed: a project to be,
    /// as a person's clone is. Made with gix, so no `git` binary is needed.
    pub fn repository(&self, at: &str) -> &Self {
        let path = self.at(at);
        fs::create_dir_all(&path).expect("make the repository's folder");
        gix::init(&path).expect("initialize a repository");
        self
    }

    /// A project at `at` as a Mindweaver from before settings made one:
    /// marked by a `.mind/mind.toml` naming the folders it authors in and
    /// nothing else. No command of this build writes that marker any more,
    /// so core's own `MindProject::init`, kept to write it for tests of such
    /// a project, makes it, as that build's `mind init` did.
    pub fn older_project(&self, at: &str) -> &Self {
        let path = self.at(at);
        fs::create_dir_all(&path).expect("make the project's folder");
        mindweaver_core::MindProject::init(&path).expect("mark it with a mind.toml");
        self
    }

    /// A file at `at` holding `contents`, as an editor saves one.
    pub fn file(&self, at: &str, contents: &str) -> &Self {
        let path = self.at(at);
        fs::create_dir_all(path.parent().expect("a file has a folder")).unwrap();
        fs::write(path, contents).expect("write the file");
        self
    }

    /// A git repository at `at` holding `files` in one commit: a source to
    /// gather from, as a team's repository of skills is.
    pub fn source(&self, at: &str, files: &[(&str, &str)]) -> &Self {
        let path = self.at(at);
        GitRepositoryMother::write(&path, files);
        GitRepositoryMother::commit(&path);
        self
    }

    /// `files` changed in the source at `at`, and committed on top of what
    /// it held: the source moved on.
    pub fn change(&self, at: &str, files: &[(&str, &str)]) -> &Self {
        let path = self.at(at);
        GitRepositoryMother::write(&path, files);
        GitRepositoryMother::commit(&path);
        self
    }

    /// A bare repository at `at` with nothing committed, its HEAD naming
    /// `main`, as a new repository on a forge is: where a backup is sent.
    /// Made with gix, as every repository here is.
    pub fn remote(&self, at: &str) -> &Self {
        let path = self.at(at);
        fs::create_dir_all(&path).expect("make the remote's folder");
        gix::init_bare(&path).expect("initialize a bare repository");
        fs::write(path.join("HEAD"), "ref: refs/heads/main\n").expect("name main");
        self
    }

    /// The files the branch `branch` of the repository at `at` holds, by
    /// route, sorted: what a clone of it would check out.
    pub fn files_on(&self, at: &str, branch: &str) -> Vec<String> {
        let repository = gix::open(self.at(at)).expect("open the repository");
        let mut reference = repository
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap_or_else(|error| panic!("{at} has no branch {branch}: {error}"));
        let tree = reference
            .peel_to_commit()
            .expect("the branch is on a commit")
            .tree()
            .expect("the commit has a tree");
        let mut files: Vec<String> = tree
            .traverse()
            .breadthfirst
            .files()
            .expect("walk the tree")
            .into_iter()
            .filter(|entry| !entry.mode.is_tree())
            .map(|entry| entry.filepath.to_string())
            .collect();
        files.sort();
        files
    }

    /// The commit the branch `branch` of the repository at `at` is on,
    /// abbreviated as a backup says it.
    pub fn branch_head(&self, at: &str, branch: &str) -> String {
        let repository = gix::open(self.at(at)).expect("open the repository");
        let mut reference = repository
            .find_reference(&format!("refs/heads/{branch}"))
            .unwrap_or_else(|error| panic!("{at} has no branch {branch}: {error}"));
        let commit = reference
            .peel_to_commit()
            .expect("the branch is on a commit");
        commit.id.to_hex_with_len(7).to_string()
    }

    /// The commit the source at `at` is on, abbreviated as a gather says it.
    pub fn head(&self, at: &str) -> String {
        let repository = gix::open(self.at(at)).expect("open the source");
        let id = repository.head_id().expect("a source has a commit");
        id.to_hex_with_len(7).to_string()
    }

    /// The commit the source at `at` is on, whole, as `--json` says it.
    pub fn commit(&self, at: &str) -> String {
        let repository = gix::open(self.at(at)).expect("open the source");
        let id = repository.head_id().expect("a source has a commit");
        id.to_string()
    }

    // ---------------------------------------------------------------------
    // What a test reads back from the disk.
    // ---------------------------------------------------------------------

    /// What the file at `at` holds.
    pub fn read(&self, at: &str) -> String {
        fs::read_to_string(self.at(at)).unwrap_or_else(|error| panic!("read {at}: {error}"))
    }

    /// Whether anything is at `at`.
    pub fn exists(&self, at: &str) -> bool {
        self.at(at).symlink_metadata().is_ok()
    }

    /// Every file under `at`, by its path from there with forward slashes,
    /// sorted: what a folder holds, to be held to a list.
    pub fn files(&self, at: &str) -> Vec<String> {
        fn walk(root: &Path, dir: &Path, into: &mut Vec<String>) {
            for entry in fs::read_dir(dir).expect("read the folder") {
                let path = entry.expect("read an entry").path();
                if path.is_dir() {
                    walk(root, &path, into);
                } else {
                    let relative = path.strip_prefix(root).expect("under the root");
                    let parts: Vec<String> = relative
                        .components()
                        .map(|part| part.as_os_str().to_string_lossy().into_owned())
                        .collect();
                    into.push(parts.join("/"));
                }
            }
        }
        let root = self.at(at);
        let mut files = Vec::new();
        walk(&root, &root, &mut files);
        files.sort();
        files
    }

    /// The id the project at `at` was given, which `mind init` draws afresh:
    /// what a test puts in place of one where a command prints it.
    pub fn project_id(&self, at: &str) -> String {
        let settings = self.read(&format!("{at}/.mind/settings.toml"));
        settings
            .lines()
            .find_map(|line| line.strip_prefix("id = \""))
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or_else(|| panic!("no id in {at}/.mind/settings.toml"))
            .to_owned()
    }

    // ---------------------------------------------------------------------
    // Running Mindweaver.
    // ---------------------------------------------------------------------

    /// Run `line` from `at` as a step of building the world, held to what
    /// every such step must do: exit 0 and say nothing on stderr. A step
    /// that warns would be building something other than what the test
    /// names.
    pub fn given(&self, at: &str, line: &str) -> &Self {
        let ran = self.run(at, line);
        if ran.status != Some(0) || !ran.stderr.is_empty() {
            panic!("a step building the scenario did not go cleanly:\n{ran}");
        }
        self
    }

    /// Run `line`, as typed in a shell, from the folder `at`.
    pub fn run(&self, at: &str, line: &str) -> Ran {
        self.run_with(at, line, &[])
    }

    /// [`run`](Self::run), with `env` set for this command alone: for what a
    /// use case says through the environment, such as an activated
    /// workspace. A value may be a path spelled from `~` or `/home`.
    pub fn run_with(&self, at: &str, line: &str, env: &[(&str, &str)]) -> Ran {
        let mut command = self.command(at, line, env);
        let output = command.output().expect("run the binary");
        self.ran(at, line, output)
    }

    /// [`run`](Self::run), with `input` on its standard input and the pipe
    /// closed after it, as `cmd < file` gives it.
    pub fn run_fed(&self, at: &str, line: &str, input: &str) -> Ran {
        use std::io::Write as _;

        let mut child = self
            .command(at, line, &[])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the binary");
        // Dropped once written, so the command reads to the end.
        child
            .stdin
            .take()
            .expect("its standard input")
            .write_all(input.as_bytes())
            .expect("write its standard input");
        let output = child.wait_with_output().expect("run the binary");
        self.ran(at, line, output)
    }

    /// The process `line` is: its binary, its words with `~` and `/home`
    /// read as this world's, started from `at` in an environment of its own.
    pub(crate) fn command(&self, at: &str, line: &str, env: &[(&str, &str)]) -> Command {
        let words =
            shell_words::split(line).unwrap_or_else(|error| panic!("read `{line}`: {error}"));
        let (binary, args) = words
            .split_first()
            .unwrap_or_else(|| panic!("`{line}` names no command"));
        let program = match binary.as_str() {
            "mind" => env!("CARGO_BIN_EXE_mind"),
            "mindweaver" => env!("CARGO_BIN_EXE_mindweaver"),
            other => panic!("`{other}` is not one of the binaries under test"),
        };
        let dir = self.at(at);
        assert!(
            dir.is_dir(),
            "`{line}` is typed in {at}, which is no folder"
        );

        let mut command = Command::new(program);
        command.current_dir(dir);
        command.args(args.iter().map(|word| self.word(word)));
        // Nothing of the developer's environment but where programs are:
        // what a variable would change is a test's to say.
        command.env_clear();
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        // Without these Windows starts no process; harmless elsewhere.
        for variable in ["SYSTEMROOT", "SystemRoot", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(variable) {
                command.env(variable, value);
            }
        }
        command
            .env("HOME", self.home())
            .env("USERPROFILE", self.home());
        for (name, value) in env {
            command.env(name, self.word(value));
        }
        command
    }

    /// A word of a line as the shell and this world read it: a leading `~`
    /// is the home, and a path under `/home`, alone or after an option's
    /// `=`, is under the scenario's root.
    fn word(&self, word: &str) -> String {
        let (option, value) = match word.split_once('=') {
            Some((option, value)) if option.starts_with('-') => (Some(option), value),
            _ => (None, word),
        };
        let spelled_here = value == "~"
            || value.starts_with("~/")
            || value == ROOT
            || value.starts_with(&format!("{ROOT}/"));
        let value = if spelled_here {
            self.at(value).to_string_lossy().into_owned()
        } else {
            value.to_owned()
        };
        match option {
            Some(option) => format!("{option}={value}"),
            None => value,
        }
    }

    /// What a command printed, every path under the root read back as
    /// `/home`.
    fn ran(&self, at: &str, line: &str, output: Output) -> Ran {
        Ran {
            at: at.to_owned(),
            line: line.to_owned(),
            status: output.status.code(),
            stdout: self.seen(&output.stdout),
            stderr: self.seen(&output.stderr),
        }
    }

    /// `bytes`, something a command or the server said, every path under
    /// the root read as `/home`.
    pub(crate) fn seen(&self, bytes: &[u8]) -> String {
        let text = String::from_utf8_lossy(bytes);
        // The resolved spelling first: on a system where the temporary
        // folder is a link, it is the longer one, and holds the other.
        let mut seen = text.replace(&*self.resolved.to_string_lossy(), ROOT);
        seen = seen.replace(&*self.root.path().to_string_lossy(), ROOT);
        seen
    }
}

/// What one command did: its exit status and what it printed on each
/// stream, paths read as `/home`.
pub struct Ran {
    at: String,
    line: String,
    /// `None` when a signal ended it.
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Ran {
    /// It exited 0.
    pub fn succeeds(&self) -> &Self {
        self.exits(0)
    }

    /// It exited with `code`: 1 for a refusal or a failure Mindweaver says,
    /// 2 for a line its parser refuses.
    pub fn exits(&self, code: i32) -> &Self {
        if self.status != Some(code) {
            panic!("expected `{}` to exit {code}:\n{self}", self.line);
        }
        self
    }

    /// Its standard output was exactly `expected`.
    pub fn prints(&self, expected: &str) -> &Self {
        if self.stdout != expected {
            panic!(
                "`{}` printed something else on stdout.\n--- expected\n{expected}--- got\n{}---\n{self}",
                self.line, self.stdout
            );
        }
        self
    }

    /// Its standard error was exactly `expected`.
    pub fn says(&self, expected: &str) -> &Self {
        if self.stderr != expected {
            panic!(
                "`{}` said something else on stderr.\n--- expected\n{expected}--- got\n{}---\n{self}",
                self.line, self.stderr
            );
        }
        self
    }

    /// It said nothing on stderr: no warning, no hint.
    pub fn quiet(&self) -> &Self {
        self.says("")
    }
}

impl fmt::Display for Ran {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = match self.status {
            Some(code) => code.to_string(),
            None => String::from("none, ended by a signal"),
        };
        write!(
            f,
            "$ {}    (in {})\nexit: {status}\nstdout:\n{}\nstderr:\n{}",
            self.line, self.at, self.stdout, self.stderr
        )
    }
}
