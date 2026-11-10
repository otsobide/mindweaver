//! The snapshots the panel's tests render from, the answers to what its
//! page of an artifact asks, and what previewing and running a few tasks is
//! answered with, written from the real thing: workspaces built by running
//! the commands, read and changed through the panel's backend as the server
//! answers it, and written into the panel's `tests/fixtures` (`make
//! web/fixtures`), committed beside its tests.
//!
//! Written rather than drawn by hand, so that a test of the panel renders
//! what core says and nothing a person imagined it says: a field added,
//! renamed or holding something else reaches the fixtures the next time
//! they are written, and `make web/fixtures/check`, which CI's `types drift
//! (web)` runs, fails until they are committed. Under the `ts` feature, as
//! the export of the panel's types is, so a plain `cargo test` writes
//! nothing anywhere.
//!
//! What would differ from one run to the next is pinned: the temporary
//! folder reads as `/home`, each project's id is one of [`IDS`], and every
//! time the ledger keeps is [`AT`] or a minute after the one before.

#![cfg(feature = "ts")]

mod common;

use std::sync::Arc;

use common::*;
use mindweaver_cli::backend::{PanelBackend, Query};
use mindweaver_core::hash;
use mindweaver_core::ledger::{Action, Outcome as Logged, Taken};
use mindweaver_core::licence::Licence;
use mindweaver_core::mother::GitRepositoryMother;
use mindweaver_core::SourceKind;
use mindweaver_server::{Backend, Underway};
use serde_json::{json, Value};

/// Where the fixtures go: the panel's tests, beside this crate, whichever
/// folder cargo was started in.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/tests/fixtures");

/// What the temporary folder a fixture is built in reads as.
const HOME: &str = "/home";

/// The id each project reads as, by its route from the workspace, in place
/// of the random one `mind init` gave it: the workspace's own, then those
/// [`candidates`] makes and the one only [`assigning`] has, which only some
/// fixtures build.
const IDS: [(&str, &str); 7] = [
    ("api", "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5"),
    ("web", "0123456789abcdef0123456789abcdef"),
    ("docs", "fedcba9876543210fedcba9876543210"),
    ("blog", "b10b10b10b10b10b10b10b10b10b10b1"),
    ("drafts", "d4af7ed4af7ed4af7ed4af7ed4af7ed4"),
    ("archive/old", "01d01d01d01d01d01d01d01d01d01d01"),
    ("bare", "ba4eba4eba4eba4eba4eba4eba4eba4e"),
];

/// The first time the ledger keeps, in seconds since the epoch: an
/// afternoon in October 2026.
const AT: i64 = 1_792_850_400;

/// The source the shelf is filled from, and the commit it was taken at.
const SOURCE: &str = "https://github.com/acme/skills";
const REVISION: &str = "8d1c3f0a2b4e6d8f0a1c3e5b7d9f1a3c5e7b9d2f";

/// What the library's `review` says, as somebody would write a skill: front
/// matter beyond the two keys every skill has, and a body with something of
/// everything markdown draws, a table, a fence, a link to a file of its own
/// and one to another site among them.
const REVIEW: &str = "---
name: review
description: Read a change before it lands
license: MIT
metadata:
  owner: platform
---

# review

Read the diff **before** it lands, and say what you would change.

## Steps

1. Run `scripts/diff.sh` to see what changed.
2. Go through [the checklist](references/checklist.md).
3. Leave one comment per finding, and *say why*.

| check | why |
| :--- | ---: |
| tests | they pass |

The long version is at <https://example.com/review>.

```sh
scripts/diff.sh main
```
";

/// `you/Projects`, a workspace holding something of everything the panel
/// shows, built as somebody would build it, a command at a time:
///
/// - its library holds the skills `review`, with a script and a reference
///   beside its `SKILL.md`, and `lint`, the rules `git/commits` and
///   `style/comments`, and the MCP server `github`, a program started with
///   a secret its environment is given;
/// - `api`, for Claude and Codex and local, authors `deploy`, and received
///   `review` and `git/commits`, as they were written: nothing to do;
/// - `web`, for Claude and OpenCode and shared, received `lint`, changed in
///   the library since, and `review`, one copy of which was edited since,
///   and keeps a skill by hand in Claude's folder: something to do;
/// - `docs`, for Codex, installs skills into `.skills`, which Codex does not
///   read, received `review` there, and authors a skill whose file does not
///   read: broken;
/// - `gone` was linked, and its folder has gone since: stale;
/// - it loads `team`, beside it, which has gone too;
/// - its shelf holds `format` and `triage`, gathered from one source.
fn workspace(home: &Path) -> PathBuf {
    let root = home.join("Projects");
    fs::create_dir_all(&root).unwrap();
    weaver(&root, &["init"]).unwrap();
    for (kind, name, description) in [
        ("skill", "review", "Read a change before it lands"),
        ("skill", "lint", "Tidy the code before a commit"),
        ("rule", "git/commits", "Write commit messages that say why"),
        ("rule", "style/comments", "Comment the why, not the what"),
    ] {
        weaver(&root, &["add", kind, name, description]).unwrap();
    }
    // The snapshot carries it as it carries every kind, with nothing in the
    // panel to learn it.
    weaver(
        &root,
        &[
            "add",
            "mcp",
            "github",
            "GitHub's issues, pull requests and code",
            "--command",
            "npx",
            "--arg",
            "-y",
            "--arg",
            "@modelcontextprotocol/server-github",
            "--secret-env",
            "GITHUB_PERSONAL_ACCESS_TOKEN",
        ],
    )
    .unwrap();
    // Before anything installs it, so every copy is of this one.
    write_files(
        &own_directory_for(&root, Kind::Skill).join("review"),
        &[
            ("SKILL.md", REVIEW),
            ("scripts/diff.sh", "git diff \"$1\"...HEAD\n"),
            (
                "references/checklist.md",
                "# Checklist\n\n- Does it say why?\n- Is it tested?\n",
            ),
        ],
    );

    project(&root, "api", &["--agents", "claude,codex"]);
    mind(
        &root.join("api"),
        &["add", "skill", "deploy", "Ship a release"],
    )
    .unwrap();
    install(&root, "api", &["review", "git/commits"]);

    project(&root, "web", &["--agents", "claude,opencode", "--shared"]);
    install(&root, "web", &["review", "lint"]);
    let lint = own_directory_for(&root, Kind::Skill).join("lint/SKILL.md");
    let text = fs::read_to_string(&lint).unwrap();
    fs::write(&lint, format!("{text}\nRun the formatter first.\n")).unwrap();
    let web = root.join("web");
    let review = web.join(".claude/skills/review/SKILL.md");
    let text = fs::read_to_string(&review).unwrap();
    fs::write(&review, format!("{text}\nAsk for a second pair of eyes.\n")).unwrap();
    write_files(
        &web,
        &[(
            ".claude/skills/legacy/SKILL.md",
            &skill_file("legacy", "Kept by hand from before"),
        )],
    );

    project(&root, "docs", &["--agents", "codex"]);
    let docs = root.join("docs");
    mind(&docs, &["targets", "set", "skill", ".skills", "--yes"]).unwrap();
    install(&root, "docs", &["review"]);
    write_files(
        &docs,
        &[("skills/notes/SKILL.md", "Notes, with no front matter\n")],
    );

    project(&root, "gone", &[]);
    fs::remove_dir_all(root.join("gone")).unwrap();

    let team = home.join("team");
    fs::create_dir_all(&team).unwrap();
    weaver(&team, &["init"]).unwrap();
    weaver(&root, &["load", "../team"]).unwrap();
    fs::remove_dir_all(&team).unwrap();

    shelve(&root);
    pin_times(&root);
    root
}

/// `you/Assign`, a workspace to assign artifacts in, which the panel's
/// matrix draws, built as somebody would build it:
///
/// - its library holds the skills `deploy` and `review` and the rule
///   `git/commits`;
/// - it loads `team`, beside it, whose own `deploy` and `notes` it offers
///   too, so `deploy` is offered from two places;
/// - its shelf holds `format` and `triage`, gathered from one source;
/// - `api`, for Claude and Codex, received the team's `deploy` and the
///   library's `review`, as they were written;
/// - `web`, for Claude and OpenCode and shared, received `review`, one copy
///   of which was edited since, and keeps a `triage` of its own by hand in
///   Claude's folder, which ticks every offer of the name;
/// - `docs`, for Codex, received the team's `notes`, changed there since;
/// - `bare` names no agent, written by hand, since no command leaves a
///   project with none: nothing can be installed into it;
/// - `gone` was linked, and its folder has gone since.
fn assigning(home: &Path) -> PathBuf {
    let root = home.join("Assign");
    let team = home.join("team");
    for workspace in [&root, &team] {
        fs::create_dir_all(workspace).unwrap();
        weaver(workspace, &["init"]).unwrap();
    }
    for (kind, name, description) in [
        ("skill", "deploy", "Ship the service"),
        ("skill", "review", "Read a change before it lands"),
        ("rule", "git/commits", "Write commit messages that say why"),
    ] {
        weaver(&root, &["add", kind, name, description]).unwrap();
    }
    for (name, description) in [
        ("deploy", "Ship it the team's way"),
        ("notes", "Keep notes as the team does"),
    ] {
        weaver(&team, &["add", "skill", name, description]).unwrap();
    }
    weaver(&root, &["load", "../team"]).unwrap();
    shelve(&root);

    project(&root, "api", &["--agents", "claude,codex"]);
    let installed = weaver(
        &root,
        &[
            "install",
            "-p",
            "api",
            "--from",
            "load:../team",
            "skill:deploy",
        ],
    )
    .unwrap();
    assert!(installed.ok, "{installed:?}");
    install(&root, "api", &["skill:review"]);

    project(&root, "web", &["--agents", "claude,opencode", "--shared"]);
    install(&root, "web", &["skill:review"]);
    let web = root.join("web");
    let review = web.join(".claude/skills/review/SKILL.md");
    let text = fs::read_to_string(&review).unwrap();
    fs::write(&review, format!("{text}\nAsk for a second pair of eyes.\n")).unwrap();
    write_files(
        &web,
        &[(
            ".claude/skills/triage/SKILL.md",
            &skill_file("triage", "Our own way to sort issues"),
        )],
    );

    project(&root, "docs", &["--agents", "codex"]);
    install(&root, "docs", &["skill:notes"]);
    let notes = own_directory_for(&team, Kind::Skill).join("notes/SKILL.md");
    let text = fs::read_to_string(&notes).unwrap();
    fs::write(&notes, format!("{text}\nDate every note.\n")).unwrap();

    project(&root, "bare", &[]);
    let settings = root.join("bare").join(MIND_DIR).join(SETTINGS_FILE);
    let text = fs::read_to_string(&settings).unwrap();
    let agents = "agents = [\"claude\", \"codex\", \"opencode\"]";
    assert!(text.contains(agents), "{text}");
    fs::write(&settings, text.replace(agents, "agents = []")).unwrap();

    project(&root, "gone", &[]);
    fs::remove_dir_all(root.join("gone")).unwrap();

    pin_times(&root);
    root
}

/// `home`, beside what [`assigning`] built in it, made the default
/// workspace, and what it knows, each remembered by a `mindweaver init` run on
/// it, which leaves one made already as it was: `Assign`, `team`, which
/// `Assign` loads, `kit`, a workspace of its own nothing loads, and `old`,
/// gone since. So the panel's page of loads has one of each a known
/// workspace can be: this one, one it loads, one it could, the default
/// among those, and one that would not open.
fn acquainted(home: &Path) {
    weaver_at_home(home, home, &["init"]).unwrap();
    for name in ["Assign", "team", "kit", "old"] {
        let root = home.join(name);
        fs::create_dir_all(&root).unwrap();
        weaver_at_home(&root, home, &["init"]).unwrap();
    }
    fs::remove_dir_all(home.join("old")).unwrap();
}

/// The lookup a panel launched in `home` reads the workspaces it knows
/// with: the default one there, and nothing else of the environment's.
fn at_home(home: &Path) -> Lookup {
    Lookup {
        home: Some(home.to_path_buf()),
        ..Lookup::default()
    }
}

/// `you/Updating`, a workspace whose projects hold an installation of each
/// state an update says something of, which the panel's page of updates
/// draws, built as somebody would build it:
///
/// - its library holds the skills `lint`, `review` and `format` and the
///   rule `git/commits`;
/// - `api`, for Claude and Codex, received `lint`, `review` and
///   `git/commits`: `lint` and `git/commits` changed in the library since,
///   so outdated, and `review`'s copy in `.agents/skills` deleted since, so
///   missing;
/// - `web`, for Claude and OpenCode and shared, received `lint`, its copy
///   in Claude's folder edited since, `format`, removed from the library
///   alone since, so orphaned, and `release`, its copy edited too, then
///   removed from every project and the library, which leaves that copy
///   edited with nothing offering it any more;
/// - `gone` was linked, and its folder has gone since.
fn updating(home: &Path) -> PathBuf {
    let root = home.join("Updating");
    fs::create_dir_all(&root).unwrap();
    weaver(&root, &["init"]).unwrap();
    for (kind, name, description) in [
        ("skill", "lint", "Tidy the code before a commit"),
        ("skill", "review", "Read a change before it lands"),
        ("skill", "format", "Format the code the way the team does"),
        ("skill", "release", "Cut a release the way the team does"),
        ("rule", "git/commits", "Write commit messages that say why"),
    ] {
        weaver(&root, &["add", kind, name, description]).unwrap();
    }

    project(&root, "api", &["--agents", "claude,codex"]);
    install(
        &root,
        "api",
        &["skill:lint", "skill:review", "rule:git/commits"],
    );
    project(&root, "web", &["--agents", "claude,opencode", "--shared"]);
    install(
        &root,
        "web",
        &["skill:lint", "skill:format", "skill:release"],
    );

    let changed = |file: &Path, line: &str| {
        let text = fs::read_to_string(file).unwrap();
        fs::write(file, format!("{text}\n{line}\n")).unwrap();
    };
    changed(
        &own_directory_for(&root, Kind::Skill).join("lint/SKILL.md"),
        "Run the formatter first.",
    );
    changed(
        &own_directory_for(&root, Kind::Rule).join("git/commits.md"),
        "Name the issue a commit is for.",
    );
    changed(
        &root.join("web/.claude/skills/lint/SKILL.md"),
        "Our own way: lint before every push.",
    );
    changed(
        &root.join("web/.claude/skills/release/SKILL.md"),
        "Our own way: tag from main only.",
    );
    fs::remove_dir_all(root.join("api/.agents/skills/review")).unwrap();
    // The library's copy alone: what was installed from it is left behind.
    let removed = weaver(&root, &["remove", "--yes", "skill:format"]).unwrap();
    assert!(removed.stdout.starts_with("removed"), "{removed:?}");
    // Everywhere, as the panel removes one: the edited copy is left.
    let removed = weaver(&root, &["remove", "--everywhere", "--yes", "skill:release"]).unwrap();
    assert!(
        removed.ok
            && removed
                .stdout
                .contains("left release in web, where it is edited"),
        "{removed:?}"
    );

    project(&root, "gone", &[]);
    fs::remove_dir_all(root.join("gone")).unwrap();

    pin_times(&root);
    root
}

/// `name`, a folder of `root`, made a repository and a project with
/// `options` given to `mind init`, and linked.
fn project(root: &Path, name: &str, options: &[&str]) {
    let project = root.join(name);
    fs::create_dir(&project).unwrap();
    gix::init(&project).expect("initialize a repository");
    let mut init = vec!["init"];
    init.extend_from_slice(options);
    mind(&project, &init).unwrap();
    weaver(root, &["link", name]).unwrap();
}

/// Each of `names`, the workspace's own, into the project `entry`.
fn install(root: &Path, entry: &str, names: &[&str]) {
    for name in names {
        let installed = weaver(root, &["install", "-p", entry, name]).unwrap();
        assert!(installed.ok, "{name} into {entry}: {installed:?}");
    }
}

/// What there is to find beside the projects of the workspace at `root`,
/// left as somebody would leave it, for a scan to find and the panel to
/// link:
///
/// - `blog` and `drafts`, projects nobody linked;
/// - `notes`, a repository holding an `AGENTS.md`, and `scratch`, one
///   holding nothing an agent reads, neither a project yet;
/// - `old`, linked with `review` installed, which records its id, for
///   [`moved`] to move;
/// - `legacy`, linked, which a `mind.toml` marks, as an older Mindweaver
///   made a project.
fn candidates(root: &Path) {
    for name in ["blog", "drafts"] {
        let project = root.join(name);
        fs::create_dir(&project).unwrap();
        gix::init(&project).expect("initialize a repository");
        mind(&project, &["init"]).unwrap();
    }
    for (name, files) in [
        ("notes", &[("AGENTS.md", "# Agents\n")][..]),
        ("scratch", &[][..]),
    ] {
        let repository = root.join(name);
        fs::create_dir(&repository).unwrap();
        gix::init(&repository).expect("initialize a repository");
        write_files(&repository, files);
    }
    project(root, "old", &[]);
    // Said with the warning that the load `team` has gone, as every install
    // here is, so held to what it did rather than to how it exited.
    let installed = weaver(root, &["install", "-p", "old", "review"]).unwrap();
    assert!(
        installed.stdout.starts_with("installed review into old"),
        "{installed:?}"
    );
    let legacy = root.join("legacy");
    fs::create_dir(&legacy).unwrap();
    older_project(&legacy);
    weaver(root, &["link", "legacy"]).unwrap();
}

/// `old`, which [`candidates`] linked, moved to `archive/old`, leaving its
/// entry stale: apart, since a stale entry is a warning beside whatever
/// reads every project, and the first tasks the panel runs are written
/// from before it.
fn moved(root: &Path) {
    fs::create_dir(root.join("archive")).unwrap();
    fs::rename(root.join("old"), root.join("archive/old")).unwrap();
}

/// `files`, each a route from `root` and what it holds.
fn write_files(root: &Path, files: &[(&str, &str)]) {
    for (route, text) in files {
        let path = root.join(route);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

/// Two skills on the shelf of the workspace at `root`, from [`SOURCE`],
/// recorded as a gather records them: a gather itself would clone, and the
/// URL of a repository made in a temporary folder is a temporary path.
fn shelve(root: &Path) {
    let workspace = WeaverWorkspace::open(root).unwrap();
    let ledger = workspace.ledger().unwrap();
    let source = ledger
        .source_for(SourceKind::Git, SOURCE, None, "skills")
        .unwrap();
    ledger.saw_source(source.id, Some(REVISION), None).unwrap();
    for (name, summary) in [
        ("format", "Format the code the way the team does"),
        ("triage", "Sort new issues by what they need"),
    ] {
        let shelf = format!(".mindweaver/skills/{}/{name}", source.directory);
        write_files(
            &root.join(&shelf),
            &[("SKILL.md", &skill_file(name, summary))],
        );
        ledger
            .record(
                &source,
                &Taken {
                    kind: Kind::Skill,
                    name: name.into(),
                    path: shelf.clone(),
                    summary: Some(summary.to_owned()),
                    revision: Some(REVISION.into()),
                    upstream_path: format!("skills/{name}"),
                    content_hash: Some(hash::tree(&root.join(&shelf)).unwrap()),
                    licence: Licence::Unknown,
                },
            )
            .unwrap();
    }
    ledger
        .log(
            Action::Gather,
            Some(SOURCE),
            Logged::Ok,
            Some("2 added, 0 updated, 0 unchanged, 0 failed"),
        )
        .unwrap();
}

/// Every time the ledger at `root` keeps, set from [`AT`] on: each action a
/// minute after the one before it, and the shelf at the first.
fn pin_times(root: &Path) {
    let workspace = WeaverWorkspace::open(root).unwrap();
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(&format!(
            "UPDATE artifacts SET gathered_at = {AT}; UPDATE actions SET at = {AT} + id * 60;"
        ))
        .unwrap();
}

/// What the panel is answered with when it asks the backend launched from
/// `directory` with `lookup` for the snapshot, as the server sends it.
fn answered(directory: &Path, lookup: &Lookup) -> String {
    let backend = PanelBackend::launch(directory, lookup).unwrap();
    let snapshot = backend
        .query(Query::Snapshot {})
        .unwrap_or_else(|refusal| panic!("{}: {}", refusal.code, refusal.message));
    serde_json::to_string_pretty(&snapshot).unwrap() + "\n"
}

/// `query`, as the panel sends it, with what `backend` answers.
fn asked(backend: &PanelBackend, query: Value) -> Value {
    let read: Query = serde_json::from_value(query.clone()).unwrap();
    let answer = backend
        .query(read)
        .unwrap_or_else(|refusal| panic!("{query}: {}: {}", refusal.code, refusal.message));
    json!({"query": query, "answer": answer})
}

/// `json` as it reads wherever it was made: `dir` as [`HOME`], each
/// project's id as [`IDS`] says, `/` between the parts of a path, and the
/// folder a source gathered by its path under `dir` is kept in as it is
/// named under [`HOME`] ([`slugged`]).
fn pinned(json: &str, dir: &Path, root: Option<&Path>) -> String {
    let escaped = serde_json::to_string(&dir.to_string_lossy()).unwrap();
    let mut json = json.replace(escaped.trim_matches('"'), HOME);
    json = json.replace(&slugged(&dir.to_string_lossy()), &slugged(HOME));
    if let Some(root) = root {
        // Only those this fixture built: the candidates are not in every
        // one.
        for (route, id) in IDS {
            let Ok(project) = MindProject::open(root.join(route)) else {
                continue;
            };
            let random = project.settings().unwrap().id.to_string();
            json = json.replace(&random, id);
        }
    }
    if cfg!(windows) {
        json.replace("\\\\", "/")
    } else {
        json
    }
}

/// `path` as the folder a source at it is kept in begins: each run of what
/// is neither a letter, a digit, `.` nor `_` one `-`, the rest lowercase,
/// and no `-` at either end, as the ledger names a source's folder from
/// its URL. Spelled again here, since the fixture only has to find the
/// temporary folder's part of a name, which the ledger's own function is
/// no part of the public API to give.
fn slugged(path: &str) -> String {
    let mut out = String::new();
    for character in path.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_') {
            out.push(character.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_owned()
}

/// Write `json` as the fixture `name`.
fn write(name: &str, json: &str) {
    let folder = Path::new(FIXTURES);
    fs::create_dir_all(folder).unwrap();
    fs::write(folder.join(format!("{name}.json")), json).unwrap();
}

/// The workspace [`workspace`] builds, as the panel is answered with it.
#[test]
fn write_the_snapshot_of_a_workspace_holding_something_of_everything() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);

    let json = answered(&root, &Lookup::default());

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    write("workspace", &json);
}

/// The workspace [`workspace`] builds, with [`candidates`] beside its
/// projects and `old` [`moved`], as the panel is answered with it: the one
/// [`write_what_scanning_the_workspace_is_answered_with`] scans, so the
/// page of what could be linked draws a snapshot and a scan of one
/// workspace, and a project's page draws `legacy`, which a `mind.toml`
/// marks, as core reads it rather than as a test would draw it.
#[test]
fn write_the_snapshot_of_a_workspace_with_more_to_find() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    candidates(&root);
    moved(&root);
    // Again, for what `candidates` installed.
    pin_times(&root);

    let json = answered(&root, &Lookup::default());

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    let snapshot: Value = serde_json::from_str(&json).unwrap();
    let legacy = snapshot["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["entry"] == "legacy")
        .unwrap_or_else(|| panic!("{json}"));
    assert_eq!(legacy["migrates"], true, "{json}");
    write("candidates", &json);
}

/// What opening each of the library's own asks, as the panel's page of one
/// asks it, with what the backend answers: its files, then each file of
/// them, the one it is read from among them, every query of every one, in
/// the order the snapshot lists them.
#[test]
fn write_what_opening_each_of_the_librarys_own_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
    let snapshot = backend.query(Query::Snapshot {}).unwrap();

    let mut answers = Vec::new();
    for held in snapshot["library"]["artifacts"].as_array().unwrap() {
        let reference = format!(
            "{}:{}",
            held["kind"].as_str().unwrap(),
            held["name"].as_str().unwrap()
        );
        let holder = json!({"type": "workspace"});
        let files = asked(
            &backend,
            json!({"type": "files", "holder": holder, "reference": reference}),
        );
        let read: Vec<Value> = files["answer"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["folder"] != true)
            .map(|entry| {
                asked(
                    &backend,
                    json!({
                        "type": "contents",
                        "holder": holder,
                        "reference": reference,
                        "file": entry["path"],
                    }),
                )
            })
            .collect();
        answers.push(files);
        answers.extend(read);
    }

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    assert!(json.contains("references/checklist.md"), "{json}");
    write("library", &json);
}

/// The default workspace in a home where nothing has made it: what the
/// panel shows before anything is linked, gathered or added.
#[test]
fn write_the_snapshot_of_a_home_where_nothing_is_made() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    fs::create_dir_all(&home).unwrap();
    let lookup = Lookup {
        home: Some(home.clone()),
        ..Lookup::default()
    };

    let json = answered(&home, &lookup);

    let json = pinned(&json, dir.path(), None);
    assert!(!home.join(WEAVER_DIR).exists(), "the read made a workspace");
    write("unmade", &json);
}

/// What the panel is answered with when its page of what could be linked
/// scans the workspace [`workspace`] builds, with [`candidates`] beside its
/// projects: the query as the page sends it, every folder under the
/// workspace to core's own depth, and the answer.
#[test]
fn write_what_scanning_the_workspace_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    candidates(&root);
    moved(&root);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();

    let answers = vec![asked(
        &backend,
        json!({"type": "scan", "roots": [root.to_str().unwrap()], "depth": null}),
    )];

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    for (route, offers) in [("drafts", "link"), ("notes", "init_and_link")] {
        let found = answers[0]["answer"]["found"]
            .as_array()
            .unwrap()
            .iter()
            .find(|suggestion| suggestion["found"]["route"] == route)
            .unwrap_or_else(|| panic!("{route}: {json}"));
        assert_eq!(found["offers"], offers, "{json}");
    }
    write("scan", &json);
}

/// `trail`, a folder listing's, as the folder it leads to reads in a
/// fixture: from `/` through [`HOME`], the folders above the temporary one
/// and it put in their place, as [`pinned`] spells every path under it.
fn trail_from_home(trail: &mut Value, dir: &Path) {
    let crumbs = trail.as_array_mut().unwrap();
    crumbs.retain(|crumb| !dir.starts_with(crumb["path"].as_str().unwrap()));
    crumbs.splice(
        0..0,
        [
            json!({"name": "/", "path": "/"}),
            json!({"name": "home", "path": HOME}),
        ],
    );
}

/// What the panel's explorer is answered with as it walks the workspace
/// [`workspace`] builds, with [`candidates`] beside its projects and `old`
/// [`moved`], launched in its home: the workspace's root as the Link form
/// opens it, naming no folder, and as Discover opens it, naming the folder
/// its field holds; the home above it; `archive`, where `old` moved to;
/// and `notes`, a repository, with its hidden folder left out, then asked
/// for (`walked`). Beside them, a folder typed as no request may name one,
/// refused with its code (`refused`).
#[test]
fn write_what_walking_the_folders_of_the_workspace_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    candidates(&root);
    moved(&root);
    let backend = PanelBackend::launch(&root, &at_home(&home)).unwrap();
    let folders = |folder: &Path, hidden: bool| json!({"type": "folders", "path": folder.to_str().unwrap(), "hidden": hidden});

    let mut walked = vec![asked(
        &backend,
        json!({"type": "folders", "path": null, "hidden": false}),
    )];
    for (folder, hidden) in [
        (root.clone(), false),
        (home.clone(), false),
        (root.join("archive"), false),
        (root.join("notes"), false),
        (root.join("notes"), true),
    ] {
        walked.push(asked(&backend, folders(&folder, hidden)));
    }
    for each in &mut walked {
        trail_from_home(&mut each["answer"]["trail"], dir.path());
    }
    let typed = json!({"type": "folders", "path": "Projects", "hidden": false});
    let refused = backend
        .query(serde_json::from_value(typed.clone()).unwrap())
        .expect_err("a relative path is listed");

    let said: Vec<(&Value, &Value, &Value)> = walked[0]["answer"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|folder| (&folder["name"], &folder["is"], &folder["linked"]))
        .collect();
    assert!(
        said.contains(&(&json!("api"), &json!("project"), &json!(true))),
        "{said:?}"
    );
    assert!(
        said.contains(&(&json!("scratch"), &json!("repository"), &json!(false))),
        "{said:?}"
    );
    assert!(
        said.contains(&(&json!("archive"), &json!("folder"), &json!(false))),
        "{said:?}"
    );
    assert_eq!(walked[0]["answer"], walked[1]["answer"]);
    assert_eq!(walked[4]["answer"]["hidden"], 1);
    assert_eq!(refused.code, "path_relative");
    let answer = json!({
        "walked": walked,
        "refused": {"query": typed, "error": refused.message, "code": refused.code},
    });
    let json = serde_json::to_string_pretty(&answer).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    assert!(json.contains("\"home\": \"/home/you\""), "{json}");
    write("folders", &json);
}

/// What the panel is answered with when it previews and runs each of a few
/// tasks in turn on the workspace [`workspace`] builds, with `blog` made a
/// project beside the others and not linked: each task as the panel sends
/// it, its preview, and what its run is answered with, its lines and what
/// they produced, as the server sends both.
///
/// A link, which asks nothing; an unlink, which the panel has somebody
/// confirm; a validate of `docs`, which fails and says why with codes; a
/// removal everywhere, which takes two things away; then what the page of
/// what could be linked and a project's page send, against [`candidates`]
/// beside the projects: `drafts` and `notes` marked, as the scan
/// [`write_what_scanning_the_workspace_is_answered_with`] writes offers
/// them, the second made a project first; `old` relinked to where the scan
/// found it moved; `web`'s toggles turned to name Codex too; and `legacy`
/// migrated, its dry run first.
#[test]
fn write_what_previewing_and_running_a_few_tasks_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    candidates(&root);
    let blog = root.join("blog");
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
    let mut answers = Vec::new();
    let mut run = |sent: Value| {
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let outcome = backend.run(task, &Underway::unfollowed()).unwrap();
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        answers.push(json!({"task": sent, "preview": preview, "ran": ran}));
    };

    for sent in [
        json!({"type": "link", "path": blog.to_str().unwrap()}),
        json!({"type": "unlink", "entry": "gone"}),
        json!({"type": "validate", "holder": {"type": "project", "entry": "docs"}}),
        json!({
            "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
            "name": "lint", "everywhere": true, "yes": true,
        }),
    ] {
        run(sent);
    }

    moved(&root);
    let scan = PanelBackend::launch(&root, &Lookup::default())
        .unwrap()
        .query(Query::Scan {
            roots: vec![root.to_str().unwrap().to_owned()],
            depth: None,
        })
        .unwrap();
    let found = scan["found"].as_array().unwrap();
    // In the order the scan lists them, as the page marks them.
    let marks: Vec<Value> = found
        .iter()
        .filter(|suggestion| ["drafts", "notes"].contains(&suggestion["found"]["route"].as_str().unwrap()))
        .map(|suggestion| json!({"path": suggestion["found"]["path"], "change": suggestion["offers"]}))
        .collect();
    let old = found
        .iter()
        .find(|suggestion| suggestion["found"]["route"] == "archive/old")
        .unwrap_or_else(|| panic!("{scan}"));
    for sent in [
        json!({"type": "marks", "marks": marks}),
        json!({"type": "relink", "old": old["moved_from"], "new": old["found"]["path"]}),
        json!({
            "type": "toggled",
            "holder": {"type": "project", "entry": "web"},
            "was": {"agents": ["claude", "opencode"], "share": "shared", "edit_tracked": false},
            "now": {"agents": ["claude", "codex", "opencode"], "share": "shared", "edit_tracked": false},
        }),
        json!({"type": "migrate", "project": "legacy", "yes": false}),
        json!({"type": "migrate", "project": "legacy", "yes": true}),
        // What the page of one of the library's own sends: its removal
        // everywhere, the dry run first.
        json!({
            "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
            "name": "review", "everywhere": true, "yes": false,
        }),
        json!({
            "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
            "name": "review", "everywhere": true, "yes": true,
        }),
    ] {
        run(sent);
    }

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    for (index, ok) in [
        true, true, false, true, true, true, true, true, true, true, true,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            answers[index]["ran"]["outcome"]["ok"], ok,
            "{index}: {json}"
        );
    }
    write("commands", &json);
}

/// The workspace [`assigning`] builds, in a home [`acquainted`] with it
/// and others, as the panel is answered with it: what its matrix draws, and
/// its page of loads.
#[test]
fn write_the_snapshot_of_a_workspace_to_assign_in() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = assigning(&home);
    acquainted(&home);

    let json = answered(&root, &at_home(&home));

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    let snapshot: Value = serde_json::from_str(&json).unwrap();
    let states: Vec<&str> = snapshot["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|boxes| boxes["held"].as_array().unwrap())
        .map(|held| held["state"].as_str().unwrap())
        .collect();
    for state in ["current", "outdated", "edited", "foreign"] {
        assert!(states.contains(&state), "{state}: {json}");
    }
    let known: Vec<(&str, &Value, bool)> = snapshot["known"]
        .as_array()
        .unwrap()
        .iter()
        .map(|known| {
            (
                known["path"].as_str().unwrap(),
                &known["loaded_as"],
                known["loadable"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        known,
        [
            ("/home/you", &Value::Null, true),
            ("/home/you/Assign", &Value::Null, false),
            ("/home/you/team", &json!("../team"), false),
            ("/home/you/kit", &Value::Null, true),
            ("/home/you/old", &Value::Null, false),
        ],
        "{json}"
    );
    write("assigning", &json);
}

/// A box of [`assigning`]'s matrix, as the panel sends it in a plan.
fn cell(project: &str, reference: &str, origin: &str, wanted: bool) -> Value {
    json!({"project": project, "reference": reference, "origin": origin, "wanted": wanted})
}

/// The shelf's origin, as `--from` takes it.
const SHELF: &str = SOURCE;

/// `format`, from the shelf, into `api` and `docs`: what the panel's
/// matrix asks first, and what [`write_what_applying_a_plan_is_answered_with`]
/// runs.
fn format_into_two() -> Value {
    json!([
        cell("api", "skill:format", SHELF, true),
        cell("docs", "skill:format", SHELF, true),
    ])
}

/// The library's `deploy` into `api`, in place of the team's: one box,
/// what the panel's command palette plans for one install, and what
/// [`write_what_applying_a_plan_is_answered_with`] runs second.
fn deploy_into_api() -> Value {
    json!([cell("api", "skill:deploy", "workspace", true)])
}

/// What the panel is answered with when it plans boxes of the workspace
/// [`assigning`] builds: each query as the matrix sends it, the boxes in
/// the order they were marked, and the answer.
///
/// `format` into two projects, which is one install; the library's
/// `deploy` in place of the team's in `api`, which replaces it; and every
/// way a box somebody can mark is held back, beside a change that goes
/// ahead: `deploy` asked of `docs` from both places that offer it, which
/// clashes, and `web`'s edited `review` and its own `triage` asked away,
/// which are locked. Not `bare`'s, which the snapshot says no box of can
/// be asked, so the matrix marks none.
#[test]
fn write_what_planning_boxes_of_a_workspace_to_assign_in_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = assigning(&home);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
    let before = fs::read(root.join(WEAVER_DIR).join("mindweaver.db")).unwrap();

    let answers: Vec<Value> = [
        format_into_two(),
        deploy_into_api(),
        json!([
            cell("api", "skill:format", SHELF, true),
            cell("docs", "skill:deploy", "workspace", true),
            cell("docs", "skill:deploy", "load:../team", true),
            cell("web", "skill:review", "workspace", false),
            cell("web", "skill:triage", SHELF, false),
        ]),
    ]
    .into_iter()
    .map(|selection| asked(&backend, json!({"type": "plan", "selection": selection})))
    .collect();

    assert_eq!(
        fs::read(root.join(WEAVER_DIR).join("mindweaver.db")).unwrap(),
        before,
        "a plan wrote to the ledger"
    );
    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    let [simple, replaced, held_back] = &answers[..] else {
        panic!("{json}")
    };
    assert_eq!(
        simple["answer"]["tasks"].as_array().unwrap().len(),
        1,
        "{json}"
    );
    assert_eq!(
        replaced["answer"]["tasks"][0]["from"], "workspace",
        "{json}"
    );
    let held_back = &held_back["answer"];
    assert_eq!(
        held_back["conflicts"][0]["why"]["code"], "wanted_twice",
        "{json}"
    );
    let locked: Vec<&str> = held_back["locked"]
        .as_array()
        .unwrap()
        .iter()
        .map(|locked| locked["why"]["code"].as_str().unwrap())
        .collect();
    assert_eq!(locked, ["edited", "foreign"], "{json}");
    write("plans", &json);
}

/// What the panel is answered with when it applies a plan of boxes of the
/// workspace [`assigning`] builds, each on that workspace as it was built:
/// the plan's tasks sent back as one `apply_install`, its preview, and what
/// its run is answered with, as [`write_what_previewing_and_running_a_few_tasks_is_answered_with`]
/// writes each. `format` into two projects, which the matrix applies, and
/// the library's `deploy` into `api`, one box, which the command palette
/// applies.
#[test]
fn write_what_applying_a_plan_is_answered_with() {
    let mut answers = Vec::new();
    for selection in [format_into_two(), deploy_into_api()] {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("you");
        let root = assigning(&home);
        let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
        let planned = asked(&backend, json!({"type": "plan", "selection": selection}));

        let sent = json!({"type": "apply_install", "changes": planned["answer"]["tasks"]});
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let outcome = backend.run(task, &Underway::unfollowed()).unwrap();
        assert!(outcome.ok, "{outcome:?}");
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        let answer = json!({"task": sent, "preview": preview, "ran": ran});
        let json = serde_json::to_string(&answer).unwrap();
        let json = pinned(&json, dir.path(), Some(&root));
        assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
        answers.push(serde_json::from_str::<Value>(&json).unwrap());
    }

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    write("applied", &json);
}

/// What the panel is answered with when each change its page of loads
/// sends is previewed and run, each on the workspace [`assigning`] builds,
/// in the home [`acquainted`] with it, as it was built: `kit` loaded by the
/// path the snapshot lists it by, `team` unloaded by the entry it is loaded
/// by, and the workspace itself, typed, which the line refuses. Each task
/// as the page sends it, its preview, and what its run is answered with.
#[test]
fn write_what_each_change_the_page_of_loads_sends_is_answered_with() {
    let sent = |home: &Path| {
        [
            json!({"type": "load", "path": home.join("kit").to_str().unwrap()}),
            json!({"type": "unload", "entry": "../team"}),
            json!({"type": "load", "path": home.join("Assign").to_str().unwrap()}),
        ]
    };

    let mut answers = Vec::new();
    let mut written = Vec::new();
    for index in 0..3 {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("you");
        let root = assigning(&home);
        acquainted(&home);
        let backend = PanelBackend::launch(&root, &at_home(&home)).unwrap();
        let sent = sent(&home)[index].clone();
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let outcome = backend.run(task, &Underway::unfollowed()).unwrap();
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        let answer = json!({"task": sent, "preview": preview, "ran": ran});
        let json = serde_json::to_string(&answer).unwrap();
        let json = pinned(&json, dir.path(), Some(&root));
        assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
        written.push(serde_json::from_str::<Value>(&json).unwrap());
        answers.push(answer);
    }

    for (index, ok) in [true, true, false].into_iter().enumerate() {
        assert_eq!(
            answers[index]["ran"]["outcome"]["ok"], ok,
            "{index}: {}",
            answers[index]
        );
    }
    let refused = &answers[2]["ran"]["outcome"]["runs"][0]["error"]["code"];
    assert_eq!(refused, "loads_itself");
    let json = serde_json::to_string_pretty(&written).unwrap() + "\n";
    write("loads", &json);
}

/// What the panel is answered with when its header switches from the
/// workspace [`assigning`] builds, in the home [`acquainted`] with it: what
/// it may switch to, asked as the header opens; then, switched to `kit`,
/// what the switch is answered with, the snapshot of `kit`, which the
/// overview draws next, and what it may switch to from there; and a switch
/// to `old`, listed but gone since, which is refused.
#[test]
fn write_what_switching_from_a_workspace_to_assign_in_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = assigning(&home);
    acquainted(&home);
    let backend = PanelBackend::launch(&root, &at_home(&home)).unwrap();
    let workspaces = json!({"type": "workspaces"});
    let switch = |name: &str| json!({"path": home.join(name).to_str().unwrap()});

    let before = asked(&backend, workspaces.clone());
    let refused = backend
        .switch(serde_json::from_value(switch("old")).unwrap())
        .expect_err("a workspace gone since is switched to");
    let to_kit = switch("kit");
    let switched = backend
        .switch(serde_json::from_value(to_kit.clone()).unwrap())
        .unwrap_or_else(|refusal| panic!("{}: {}", refusal.code, refusal.message));
    let snapshot = backend.query(Query::Snapshot {}).unwrap();
    let after = asked(&backend, workspaces);

    assert_eq!(refused.code, "not_a_workspace");
    assert_eq!(snapshot["workspace"]["reason"], "explicit");
    let answer = json!({
        "workspaces": before,
        "switched": {"to": to_kit, "answer": switched, "snapshot": snapshot, "workspaces": after},
        "refused": {"to": switch("old"), "error": refused.message, "code": refused.code},
    });
    let json = serde_json::to_string_pretty(&answer).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    write("switches", &json);
}

/// The workspace [`updating`] builds, as the panel is answered with it:
/// what its page of updates and its projects table draw.
#[test]
fn write_the_snapshot_of_a_workspace_to_update() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = updating(&home);

    let json = answered(&root, &Lookup::default());

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    let snapshot: Value = serde_json::from_str(&json).unwrap();
    let states: Vec<&str> = snapshot["projects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|project| project["artifacts"].as_array().unwrap())
        .filter_map(|held| held["state"].as_str())
        .collect();
    for state in ["outdated", "edited", "missing", "orphaned"] {
        assert!(states.contains(&state), "{state}: {json}");
    }
    write("updating", &json);
}

/// What the panel is answered with when its page of updates asks what
/// updating every project of the workspace [`updating`] builds would do:
/// the query as the page sends it, and the answer.
#[test]
fn write_what_asking_what_an_update_would_do_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = updating(&home);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
    let before = fs::read(root.join(WEAVER_DIR).join("mindweaver.db")).unwrap();

    let answers = vec![asked(&backend, json!({"type": "updates"}))];

    assert_eq!(
        fs::read(root.join(WEAVER_DIR).join("mindweaver.db")).unwrap(),
        before,
        "asking wrote to the ledger"
    );
    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    let does: Vec<&str> = answers[0]["answer"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|project| project["installations"].as_array().unwrap())
        .map(|pending| pending["does"].as_str().unwrap())
        .collect();
    for step in ["refresh", "put_back", "leave", "orphaned"] {
        assert!(does.contains(&step), "{step}: {json}");
    }
    let forced: Vec<(&str, &str)> = answers[0]["answer"]["projects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|project| project["installations"].as_array().unwrap())
        .filter(|pending| pending["forced"] == true)
        .map(|pending| {
            (
                pending["name"].as_str().unwrap(),
                pending["does"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        forced,
        [("lint", "leave"), ("release", "orphaned")],
        "{json}"
    );
    write("updates", &json);
}

/// What the panel is answered with when its page of updates previews and
/// runs each change it offers, each on the workspace [`updating`] builds as
/// it was built, so each is answered as the first thing run there: every
/// project updated, one, one installation in one, an edited one forced,
/// an orphaned one uninstalled, an orphaned one edited since uninstalled
/// forced, and every orphaned one taken out. Each task as the page sends
/// it, its preview, and what its run is answered with.
#[test]
fn write_what_each_change_the_page_of_updates_sends_is_answered_with() {
    let project = |entry: &str| json!({"type": "project", "entry": entry});
    let sent = [
        json!({"type": "update", "holder": {"type": "workspace"}, "reference": null, "force": false}),
        json!({"type": "update", "holder": project("api"), "reference": null, "force": false}),
        json!({"type": "update", "holder": project("api"), "reference": "skill:lint", "force": false}),
        json!({"type": "update", "holder": project("web"), "reference": "skill:lint", "force": true}),
        json!({"type": "uninstall", "projects": ["web"], "kind": "skill", "name": "format", "force": false}),
        json!({"type": "uninstall", "projects": ["web"], "kind": "skill", "name": "release", "force": true}),
        json!({"type": "uninstall_orphaned", "holder": {"type": "workspace"}}),
    ];

    let mut answers = Vec::new();
    let mut written = Vec::new();
    for sent in sent {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("you");
        let root = updating(&home);
        let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let outcome = backend.run(task, &Underway::unfollowed()).unwrap();
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        let answer = json!({"task": sent, "preview": preview, "ran": ran});
        let json = serde_json::to_string(&answer).unwrap();
        let json = pinned(&json, dir.path(), Some(&root));
        assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
        written.push(serde_json::from_str::<Value>(&json).unwrap());
        answers.push(answer);
    }

    // Every project leaves the edited copy, and says so by its code; it and
    // every orphaned one taken out say `gone` would not open, beside what
    // they did.
    let said = answers[0]["ran"]["outcome"]["runs"][0]["stderr"].to_string();
    assert!(said.contains("\"edited\""), "{said}");
    for (index, ok) in [false, true, true, true, true, true, false]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            answers[index]["ran"]["outcome"]["ok"], ok,
            "{index}: {}",
            answers[index]
        );
    }
    let forced = answers[5]["ran"]["outcome"]["runs"][0]["stdout"].as_str();
    assert_eq!(forced, Some("removed release from web\n"));
    let taken = answers[6]["ran"]["outcome"]["runs"][0]["stdout"].as_str();
    assert_eq!(taken, Some("removed format from web\n"));
    let json = serde_json::to_string_pretty(&written).unwrap() + "\n";
    write("updated", &json);
}

/// What the library's `review` holds once the editor has added two things
/// to its front matter that `validate` warns of and `edit --contents`
/// writes all the same: a `compatibility` that says nothing, and a
/// `metadata` entry that is no string.
fn review_warned() -> String {
    REVIEW.replace(
        "license: MIT\nmetadata:\n  owner: platform\n",
        "license: MIT\ncompatibility: \"\"\nmetadata:\n  owner: platform\n  teams: [api, web]\n",
    )
}

/// What the library's `review` holds once the editor has emptied its
/// description, which `edit --contents` refuses.
fn review_unfit() -> String {
    REVIEW.replace(
        "description: Read a change before it lands",
        "description: \"\"",
    )
}

/// What the library's `review` holds once the editor has changed a line of
/// its body: what saving writes.
fn review_saved() -> String {
    REVIEW.replace(
        "Read the diff **before** it lands",
        "Read the diff **before** it lands, every time",
    )
}

/// A draft of `text` for the library's `review`, as the editor asks core to
/// judge it.
fn edited_review(text: &str) -> Value {
    json!({
        "type": "validate", "holder": {"type": "workspace"},
        "draft": {"type": "edit", "reference": "skill:review", "text": text},
    })
}

/// A draft of the skill `name`, described as `description`, as the page
/// adding one asks core to judge it.
fn added_skill(name: &str, description: &str, template: Option<&str>) -> Value {
    json!({
        "type": "validate", "holder": {"type": "workspace"},
        "draft": {
            "type": "add", "kind": "skill", "name": name, "description": description,
            "template": template, "server": null,
        },
    })
}

/// What the panel is answered with when its editor and the page adding one
/// of the library's own ask core to judge what is typed, on the workspace
/// [`workspace`] builds (`drafts.json`): `review` as it is, with what
/// `validate` warns of, and with its description emptied; and skills to
/// add, one that would be written, from nothing and from `full`, one whose
/// name is refused and one whose name is taken.
///
/// Beside them (`validated.json`), what `mindweaver validate` says of the
/// library once `review` holds the draft it warns of, written with `mindweaver
/// edit --contents`: what the panel's verdict is held to, word for word.
#[test]
fn write_what_judging_drafts_in_the_library_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = workspace(&home);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();

    let answers = vec![
        asked(&backend, edited_review(REVIEW)),
        asked(&backend, edited_review(&review_warned())),
        asked(&backend, edited_review(&review_unfit())),
        asked(
            &backend,
            added_skill("deploy-docs", "Publish the docs", None),
        ),
        asked(
            &backend,
            added_skill("deploy-docs", "Publish the docs", Some("full")),
        ),
        asked(
            &backend,
            added_skill("Deploy Docs", "Publish the docs", None),
        ),
        asked(&backend, added_skill("review", "Read it again", None)),
    ];
    for (index, refused) in [
        None,
        None,
        Some("edit_contents_unfit"),
        None,
        None,
        Some("add_name_unfit"),
        Some("add_exists"),
    ]
    .into_iter()
    .enumerate()
    {
        let answer = &answers[index]["answer"];
        assert_eq!(
            answer["refused"]["code"].as_str(),
            refused,
            "{index}: {answer}"
        );
    }
    let warned = answers[1]["answer"]["issues"].as_array().unwrap();
    assert_eq!(warned.len(), 2, "{warned:?}");

    let draft = home.join("review.md");
    fs::write(&draft, review_warned()).unwrap();
    weaver(
        &root,
        &[
            "edit",
            "--contents",
            draft.to_str().unwrap(),
            "skill:review",
        ],
    )
    .unwrap();
    let said = weaver(&root, &["validate"]).unwrap().stdout;
    for issue in warned {
        let line = format!("  - warning: {}\n", issue["message"].as_str().unwrap());
        assert!(said.contains(&line), "{line:?} in {said}");
    }

    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    write("drafts", &json);
    let validated = json!({"text": review_warned(), "said": said});
    let json = serde_json::to_string_pretty(&validated).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    write("validated", &json);
}

/// What the panel is answered with when each change its pages of the
/// library and of a project send is previewed and run, each on the
/// workspace [`workspace`] builds (`authored.json`): `review` saved from
/// the editor, and saved again once its file was changed on disk since it
/// was read, which is refused; a skill added from `full`; `lint` renamed;
/// and the skill `web` keeps by hand in Claude's folder adopted into the
/// library.
#[test]
fn write_what_each_change_authoring_sends_is_answered_with() {
    let read = mindweaver_core::hash::bytes(REVIEW.as_bytes());
    let save = json!({
        "type": "save", "holder": {"type": "workspace"}, "kind": "skill", "name": "review",
        "text": review_saved(), "hash": read,
    });
    let sent = [
        (save.clone(), false),
        (save, true),
        (
            json!({
                "type": "add", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "deploy-docs", "description": "Publish the docs", "template": "full",
                "server": null,
            }),
            false,
        ),
        (
            json!({
                "type": "rename", "holder": {"type": "workspace"}, "kind": "skill",
                "from": "lint", "to": "tidy",
            }),
            false,
        ),
        (
            json!({"type": "adopt", "project": "web", "kind": "skill", "name": "legacy"}),
            false,
        ),
    ];

    let mut answers = Vec::new();
    let mut written = Vec::new();
    for (sent, changed) in sent {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("you");
        let root = workspace(&home);
        if changed {
            let review = own_directory_for(&root, Kind::Skill).join("review/SKILL.md");
            fs::write(&review, format!("{REVIEW}\nChanged on disk since.\n")).unwrap();
        }
        let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let outcome = backend.run(task, &Underway::unfollowed()).unwrap();
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        let answer = json!({"task": sent, "preview": preview, "ran": ran});
        let json = serde_json::to_string(&answer).unwrap();
        let json = pinned(&json, dir.path(), Some(&root));
        assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
        written.push(serde_json::from_str::<Value>(&json).unwrap());
        answers.push(answer);
    }

    for (index, ok) in [true, false, true, true, true].into_iter().enumerate() {
        assert_eq!(
            answers[index]["ran"]["outcome"]["runs"][0]["error"].is_null(),
            ok,
            "{index}: {}",
            answers[index]
        );
    }
    let stale = &answers[1]["ran"]["outcome"]["runs"][0]["error"]["code"];
    assert_eq!(stale, "edit_stale");
    let json = serde_json::to_string_pretty(&written).unwrap() + "\n";
    write("authored", &json);
}

/// `you/Serving`, a workspace keeping MCP servers, which the panel's page
/// of them draws, built as somebody would build it:
///
/// - its library holds `github`, a program started with a secret its
///   environment is given, `linear`, an address reached with a bearer
///   token, and `events`, an address reached over server-sent events,
///   which Codex and OpenCode cannot reach;
/// - it loads `team`, beside it, whose own server `sentry` it offers too,
///   which waits for a yes before it is installed;
/// - `api`, for every agent, received `github`, written into each agent's
///   config file;
/// - `web`, for Claude and OpenCode and shared, received nothing yet.
fn serving(home: &Path) -> PathBuf {
    let root = home.join("Serving");
    let team = home.join("team");
    for workspace in [&root, &team] {
        fs::create_dir_all(workspace).unwrap();
        weaver(workspace, &["init"]).unwrap();
    }
    let servers: [(&Path, &[&str]); 4] = [
        (
            &root,
            &[
                "github",
                "GitHub's issues, pull requests and code",
                "--command",
                "npx",
                "--arg=-y",
                "--arg",
                "@modelcontextprotocol/server-github",
                "--secret-env",
                "GITHUB_PERSONAL_ACCESS_TOKEN",
            ],
        ),
        (
            &root,
            &[
                "linear",
                "Linear's issues",
                "--url",
                "https://mcp.linear.app/mcp",
                "--bearer-env",
                "LINEAR_API_KEY",
            ],
        ),
        (
            &root,
            &[
                "events",
                "An older server, reached over server-sent events",
                "--url",
                "https://events.example.com/sse",
                "--sse",
            ],
        ),
        (
            &team,
            &[
                "sentry",
                "Sentry's issues, as the team runs it",
                "--command",
                "npx",
                "--arg=-y",
                "--arg",
                "@sentry/mcp-server",
                "--secret-env",
                "SENTRY_ACCESS_TOKEN",
            ],
        ),
    ];
    for (workspace, server) in servers {
        let mut line = vec!["add", "mcp"];
        line.extend_from_slice(server);
        weaver(workspace, &line).unwrap();
    }
    weaver(&root, &["load", "../team"]).unwrap();

    project(&root, "api", &["--agents", "claude,codex,opencode"]);
    // Codex is given it all the same in a project nothing marks trusted,
    // which the line warns of, so its outcome is not looked at here.
    weaver(&root, &["install", "-p", "api", "mcp:github"]).unwrap();
    for file in [".mcp.json", ".codex/config.toml", "opencode.json"] {
        assert!(root.join("api").join(file).is_file(), "{file}");
    }
    project(&root, "web", &["--agents", "claude,opencode", "--shared"]);

    pin_times(&root);
    root
}

/// The workspace [`serving`] builds, as the panel is answered with it: what
/// its page of MCP servers draws.
#[test]
fn write_the_snapshot_of_a_workspace_serving_mcp_servers() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = serving(&home);

    let json = answered(&root, &Lookup::default());

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    write("serving", &json);
}

/// How a server is started, as the page's form sends it.
fn started(command: &str, args: &[&str], secret: &str) -> Value {
    json!({
        "type": "program", "command": command, "args": args, "env": [],
        "secret_env": [secret], "arg_env": [],
    })
}

/// What the panel's page of MCP servers asks of the workspace [`serving`]
/// builds, with what the backend answers (`servers.json`): its form
/// judging a server to add, one that would be written and one whose
/// secret is read from a variable no agent can refer to, and `github`
/// given another transport; and the matrix planning `sentry`, the team's,
/// into `web`, which waits for a yes, and the library's `linear` into
/// `web`, which does not.
///
/// No token: a fixture is a file of the repository, which holds none for
/// a scanner to stop, so what core refuses here is a variable's name.
#[test]
fn write_what_the_page_of_mcp_servers_asks_is_answered_with() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = serving(&home);
    let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
    let own = json!({"type": "workspace"});
    let leaky = started("npx", &["-y", "@acme/server"], "ACME-TOKEN");
    let server = started("npx", &["-y", "@acme/server"], "ACME_TOKEN");

    let answers: Vec<Value> = [
        json!({
            "type": "validate", "holder": own,
            "draft": {
                "type": "add", "kind": "mcp", "name": "acme", "description": "Acme's tools",
                "template": null, "server": server,
            },
        }),
        json!({
            "type": "validate", "holder": own,
            "draft": {
                "type": "add", "kind": "mcp", "name": "acme", "description": "Acme's tools",
                "template": null, "server": leaky,
            },
        }),
        json!({
            "type": "validate", "holder": own,
            "draft": {
                "type": "edit_server", "name": "github",
                "server": started("docker", &["run", "-i", "ghcr.io/github/github-mcp-server"], "GITHUB_PERSONAL_ACCESS_TOKEN"),
            },
        }),
        json!({"type": "plan", "selection": [cell("web", "mcp:sentry", "load:../team", true)]}),
        json!({"type": "plan", "selection": [cell("web", "mcp:linear", "workspace", true)]}),
    ]
    .into_iter()
    .map(|query| asked(&backend, query))
    .collect();

    assert_eq!(
        answers[0]["answer"]["refused"],
        Value::Null,
        "{}",
        answers[0]
    );
    assert_eq!(answers[1]["answer"]["refused"]["code"], "add_mcp_unfit");
    assert_eq!(
        answers[2]["answer"]["refused"],
        Value::Null,
        "{}",
        answers[2]
    );
    assert_eq!(answers[3]["answer"]["asks"][0]["confirmed"]["yes"], true);
    assert_eq!(answers[4]["answer"]["asks"], json!([]));
    let json = serde_json::to_string_pretty(&answers).unwrap() + "\n";
    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    write("servers", &json);
}

/// What each change the panel's page of MCP servers and its matrix send is
/// answered with, previewed and run each on the workspace [`serving`]
/// builds (`served.json`): a server added from the form, `github` given
/// another transport, and `sentry`, the team's, into `web`, by the plan's
/// task, which installs nothing, then by the task its confirmation
/// carries.
#[test]
fn write_what_each_change_the_page_of_mcp_servers_sends_is_answered_with() {
    let sent = [
        json!({
            "type": "add", "holder": {"type": "workspace"}, "kind": "mcp", "name": "acme",
            "description": "Acme's tools", "template": null,
            "server": started("npx", &["-y", "@acme/server"], "ACME_TOKEN"),
        }),
        json!({
            "type": "edit_server", "holder": {"type": "workspace"}, "name": "github",
            "server": started("docker", &["run", "-i", "ghcr.io/github/github-mcp-server"], "GITHUB_PERSONAL_ACCESS_TOKEN"),
        }),
        // The plan's task as the matrix sends it before anybody says yes,
        // whose line installs nothing, then its confirmation.
        json!({
            "type": "apply_install",
            "changes": [{
                "type": "install", "projects": ["web"], "kind": "mcp", "name": "sentry",
                "from": "load:../team", "yes": false,
            }],
        }),
        json!({
            "type": "apply_install",
            "changes": [{
                "type": "install", "projects": ["web"], "kind": "mcp", "name": "sentry",
                "from": "load:../team", "yes": true,
            }],
        }),
    ];

    let mut written = Vec::new();
    for sent in sent {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("you");
        let root = serving(&home);
        let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let outcome = backend.run(task, &Underway::unfollowed()).unwrap();
        assert!(
            outcome.runs.iter().all(|run| run.error.is_none()),
            "{sent}: {outcome:?}"
        );
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        let answer = json!({"task": sent, "preview": preview, "ran": ran});
        let json = serde_json::to_string(&answer).unwrap();
        let json = pinned(&json, dir.path(), Some(&root));
        assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
        written.push(serde_json::from_str::<Value>(&json).unwrap());
    }
    let json = serde_json::to_string_pretty(&written).unwrap() + "\n";
    write("served", &json);
}

/// What the panel's forms for a copy of the library send, previewed and
/// run, each on the workspace [`workspace`] builds, `you/Projects`:
///
/// - an export of every kind into `you/backup`, which adds;
/// - an export of skills alone told yes, previewed with what it takes away;
/// - a backup to a repository on GitHub, naming the variable a token is read
///   from, previewed as a run that takes a cancel;
/// - a backup naming none, run and cancelled at its first step, as the
///   server's cancel reaches it when the panel's stop is pressed as soon as
///   it starts: before `git` runs at all, so no repository is reached, and
///   none read the configuration of whoever writes the fixtures.
#[test]
fn write_what_each_change_the_library_s_copies_send_is_answered_with() {
    let backup = |token_env: Option<&str>| {
        json!({
            "type": "backup", "url": "https://github.com/you/library-backup.git",
            "branch": null, "kinds": [], "message": null, "token_env": token_env,
        })
    };
    // The folder an export writes into, beside the workspace, by its path.
    let export = |dir: &Path, kinds: Value, yes: bool| {
        let folder = dir.join("you").join("backup");
        json!({"type": "export", "folder": folder.to_str().unwrap(), "kinds": kinds, "yes": yes})
    };
    let sent: [(&dyn Fn(&Path) -> Value, bool); 4] = [
        (&|dir| export(dir, json!([]), false), true),
        (&|dir| export(dir, json!(["skill"]), true), true),
        (&|_| backup(Some("GITHUB_TOKEN")), false),
        (&|_| backup(None), true),
    ];

    let mut answers = Vec::new();
    let mut written = Vec::new();
    for (sent, runs) in sent {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("you");
        let root = workspace(&home);
        let sent = sent(dir.path());
        let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        // A run of the backup that names a variable would read it from
        // the environment of whoever writes the fixtures: previewed alone.
        let ran = runs.then(|| {
            let underway = cancelled_at_first_step();
            let outcome = backend.run(task, &underway).unwrap();
            json!({"lines": preview.lines, "outcome": outcome})
        });
        let answer = json!({"task": sent, "preview": preview, "ran": ran});
        let json = serde_json::to_string(&answer).unwrap();
        let json = pinned(&json, dir.path(), Some(&root));
        assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
        written.push(serde_json::from_str::<Value>(&json).unwrap());
        answers.push(answer);
    }

    let runs = |index: usize| &answers[index]["ran"]["outcome"]["runs"][0];
    assert!(runs(0)["error"].is_null(), "{}", answers[0]);
    assert!(runs(1)["error"].is_null(), "{}", answers[1]);
    assert_eq!(answers[1]["preview"]["confirm"][0]["caution"], "removes");
    assert_eq!(answers[2]["preview"]["cancellable"], true);
    assert!(answers[2]["ran"].is_null());
    assert_eq!(runs(3)["error"]["code"], "backup_cancelled");
    let json = serde_json::to_string_pretty(&written).unwrap() + "\n";
    write("kept", &json);
}

/// A run numbered 1 that asks itself to stop on the first step it reports,
/// as the server's cancel would: what an export, which reports none, never
/// meets.
fn cancelled_at_first_step() -> Underway {
    let itself: Arc<std::sync::OnceLock<Underway>> = Arc::default();
    let stopping = Arc::clone(&itself);
    let underway = Underway::new(1, move |_| {
        if let Some(underway) = stopping.get() {
            underway.cancel();
        }
    });
    itself.set(underway.clone()).unwrap();
    underway
}

/// Each step a backup reports, as the backend tells the panel it
/// (`progress`'s `Step`): what the panel draws while one runs, and stops
/// offering a stop at once it pushes.
#[test]
fn write_the_steps_a_backup_reports() {
    use mindweaver_cli::progress::{Progress, Step};
    use mindweaver_core::backup::Progress as Backup;

    let steps: Vec<Step> = [
        Backup::CheckingOut,
        Backup::Exporting,
        Backup::Committing,
        Backup::Pushing,
    ]
    .into_iter()
    .map(|step| Step::of(Progress::Backup(step)))
    .collect();

    let json = serde_json::to_string_pretty(&steps).unwrap() + "\n";
    write("steps", &json);
}

/// What `acme-skills`, the repository [`sourcing`] gathers, holds: two
/// skills, `format` declaring the licence it is under, and `triage` beside
/// a licence file, which declares none.
const ACME_SKILLS: [(&str, &str); 3] = [
    (
        "skills/format/SKILL.md",
        "---\nname: format\ndescription: Format the code the way the team does\nlicense: MIT\n---\n\n# format\n\nRun the formatter, then read what it changed.\n",
    ),
    (
        "skills/triage/SKILL.md",
        "---\nname: triage\ndescription: Sort new issues by what they need\n---\n\n# triage\n\nLabel it, size it, and say who could take it.\n",
    ),
    (
        "skills/triage/LICENSE",
        "Apache License\nVersion 2.0, January 2004\n",
    ),
];

/// What `team-rules` holds: two rules kept in `rules`, beside the licence
/// every rule there is under, and a README a gather of rules leaves.
const TEAM_RULES: [(&str, &str); 4] = [
    (
        "rules/git/commits.md",
        "# Commits\n\nSay why in the subject.\n",
    ),
    (
        "rules/style/comments.md",
        "# Comments\n\nComment the why, not the what.\n",
    ),
    ("rules/LICENSE", "MIT License\n"),
    ("README.md", "# Team rules\n"),
];

/// What `acme-tools`, the repository nothing gathers until the page adds
/// it, holds: two skills.
const ACME_TOOLS: [(&str, &str); 2] = [
    (
        "skills/lint/SKILL.md",
        "---\nname: lint\ndescription: Tidy the code before a commit\n---\n\n# lint\n\nRun the linter.\n",
    ),
    (
        "skills/review/SKILL.md",
        "---\nname: review\ndescription: Read a change before it lands\n---\n\n# review\n\nRead the diff.\n",
    ),
];

/// A repository at `you/src/<name>` under `home`, holding `files` in one
/// commit made at a fixed time, so its id is the same at every run: its
/// path, as a person on this machine types it.
fn source_at(home: &Path, name: &str, files: &[(&str, &str)]) -> String {
    let path = home.join("src").join(name);
    GitRepositoryMother::write(&path, files);
    GitRepositoryMother::commit(&path);
    path.to_str().unwrap().to_owned()
}

/// The commit the repository at `path` holds.
fn commit_of(path: &str) -> String {
    gix::open(path).unwrap().head_id().unwrap().to_string()
}

/// `you/Sources`, a workspace whose shelf is filled from repositories beside
/// it, which the panel's page of sources draws and changes, built as
/// somebody would build it, each source gathered by its path, as the page
/// names a folder on this machine:
///
/// - `src/acme-skills`, [`ACME_SKILLS`], gathered as it is, and called
///   `src-acme-skills`, from its path;
/// - `src/team-rules`, [`TEAM_RULES`], gathered as rules from `rules`,
///   called `team-rules` and pinned to its commit;
/// - `src/acme-tools`, [`ACME_TOOLS`], which nothing gathered yet.
fn sourcing(home: &Path) -> PathBuf {
    let root = home.join("Sources");
    fs::create_dir_all(&root).unwrap();
    weaver(&root, &["init"]).unwrap();

    let skills = source_at(home, "acme-skills", &ACME_SKILLS);
    let gathered = weaver(&root, &["gather", "git", &skills]).unwrap();
    assert!(gathered.ok, "{gathered:?}");
    let rules = source_at(home, "team-rules", &TEAM_RULES);
    let pin = commit_of(&rules);
    let gathered = weaver(
        &root,
        &[
            "gather",
            "git",
            "--kind",
            "rules",
            "--path",
            "rules",
            "--name",
            "team-rules",
            "--pin",
            &pin,
            &rules,
        ],
    )
    .unwrap();
    assert!(gathered.ok, "{gathered:?}");
    source_at(home, "acme-tools", &ACME_TOOLS);

    pin_times(&root);
    root
}

/// The workspace [`sourcing`] builds, as the panel is answered with it: what
/// its page of sources draws, each source with its name, its pin and the
/// licences what it supplied says.
#[test]
fn write_the_snapshot_of_a_workspace_filled_from_sources() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("you");
    let root = sourcing(&home);

    let json = answered(&root, &Lookup::default());

    let json = pinned(&json, dir.path(), Some(&root));
    assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
    let snapshot: Value = serde_json::from_str(&json).unwrap();
    let names: Vec<&str> = snapshot["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["src-acme-skills", "team-rules"], "{json}");
    assert!(snapshot["sources"][1]["pin"].is_string(), "{json}");
    assert_eq!(
        snapshot["sources"][0]["licences"],
        json!([
            {"licence": "Apache License", "count": 1},
            {"licence": "MIT", "count": 1},
        ])
    );
    write("sources", &json);
}

/// Each change the panel's page of sources sends, previewed, then run on
/// the workspace [`sourcing`] builds, as it was built, with what the
/// backend answers: `acme-tools` added, its dry run first, then gathered,
/// then gathered and stopped at its first step, before anything is placed;
/// `src-acme-skills` gathered again, then every source; `src-acme-skills`
/// renamed, then renamed to a name `team-rules` has, which is refused;
/// `team-rules` forgotten, its dry run first; and `src-acme-skills`
/// gathered again and stopped at its first step.
#[test]
fn write_what_each_change_the_page_of_sources_sends_is_answered_with() {
    let tools = |dir: &Path| {
        let path = dir.join("you").join("src").join("acme-tools");
        path.to_str().unwrap().to_owned()
    };
    let gather = |dir: &Path, dry_run: bool| {
        json!({
            "type": "gather", "url": tools(dir), "folder": ".", "reference": null,
            "kind": "skill", "name": null, "pin": null, "dry_run": dry_run,
        })
    };
    let rename = |to: &str| json!({"type": "rename_source", "from": "src-acme-skills", "to": to});
    let remove = |yes: bool| json!({"type": "remove_source", "name": "team-rules", "yes": yes});
    // Each task, and whether its run asks itself to stop at its first step.
    let again = |_: &Path| json!({"type": "update_sources", "names": ["src-acme-skills"]});
    let sent: [(&dyn Fn(&Path) -> Value, bool); 10] = [
        (&|dir| gather(dir, true), false),
        (&|dir| gather(dir, false), false),
        (&|dir| gather(dir, false), true),
        (&again, false),
        (&|_| json!({"type": "update_sources", "names": []}), false),
        (&|_| rename("acme-skills"), false),
        (&|_| rename("team-rules"), false),
        (&|_| remove(false), false),
        (&|_| remove(true), false),
        (&again, true),
    ];

    let mut answers = Vec::new();
    let mut written = Vec::new();
    for (sent, stopped) in sent {
        let dir = TempDir::new().unwrap();
        let home = dir.path().join("you");
        let root = sourcing(&home);
        let sent = sent(dir.path());
        let backend = PanelBackend::launch(&root, &Lookup::default()).unwrap();
        let task: Task = serde_json::from_value(sent.clone()).unwrap();
        let preview = backend
            .preview(&task)
            .unwrap_or_else(|refusal| panic!("{sent}: {}: {}", refusal.code, refusal.message));
        let underway = if stopped {
            cancelled_at_first_step()
        } else {
            Underway::unfollowed()
        };
        let outcome = backend.run(task, &underway).unwrap();
        let ran = json!({"lines": preview.lines, "outcome": outcome});
        let answer = json!({"task": sent, "preview": preview, "ran": ran});
        let json = serde_json::to_string(&answer).unwrap();
        let json = pinned(&json, dir.path(), Some(&root));
        assert!(!json.contains(&*dir.path().to_string_lossy()), "{json}");
        written.push(serde_json::from_str::<Value>(&json).unwrap());
        answers.push(answer);
    }

    let run = |index: usize| &answers[index]["ran"]["outcome"]["runs"][0];
    let said = |index: usize| run(index)["stdout"].as_str().unwrap_or_default().to_owned();
    for index in [0, 1, 2] {
        assert_eq!(answers[index]["preview"]["cancellable"], true, "{index}");
    }
    assert!(said(0).contains("would add"), "{}", answers[0]);
    assert!(said(1).contains("2 skills: 2 added"), "{}", answers[1]);
    assert_eq!(
        run(2)["error"]["code"],
        "gather_cancelled",
        "{}",
        answers[2]
    );
    assert_eq!(answers[3]["ran"]["outcome"]["ok"], true, "{}", answers[3]);
    assert_eq!(answers[4]["ran"]["outcome"]["ok"], true, "{}", answers[4]);
    assert!(
        said(5).starts_with("renamed source src-acme-skills to acme-skills"),
        "{}",
        answers[5]
    );
    assert_eq!(
        run(6)["error"]["code"],
        "source_name_taken",
        "{}",
        answers[6]
    );
    assert!(
        said(7).starts_with("would forget the source team-rules"),
        "{}",
        answers[7]
    );
    assert!(answers[7]["preview"]["confirm"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(answers[8]["preview"]["confirm"][0]["caution"], "forgets");
    assert!(
        said(8).starts_with("forgot the source team-rules"),
        "{}",
        answers[8]
    );
    // Stopped, it says so of the source it stopped at, by the gather's code.
    assert_eq!(run(9)["error"], Value::Null, "{}", answers[9]);
    assert_eq!(
        run(9)["stderr"][0]["code"],
        "gather_cancelled",
        "{}",
        answers[9]
    );
    let json = serde_json::to_string_pretty(&written).unwrap() + "\n";
    write("gathered", &json);
}

/// Each step a gather reports, as the backend tells the panel it
/// (`progress`'s `Step`): what the panel draws while one runs, a bar where
/// the step counts, and stops offering a stop at once it places.
#[test]
fn write_the_steps_a_gather_reports() {
    use mindweaver_cli::progress::{Progress, Step};

    let steps: Vec<Step> = [
        Gather::Connecting,
        Gather::Receiving {
            done: 120,
            total: Some(340),
        },
        Gather::Resolving {
            done: 80,
            total: None,
        },
        Gather::CheckingOut {
            done: 12,
            total: Some(12),
        },
        Gather::Harvesting { done: 1, total: 2 },
        Gather::Placing { done: 1, total: 2 },
    ]
    .into_iter()
    .map(|step| Step::of(Progress::Gather(step)))
    .collect();

    let json = serde_json::to_string_pretty(&steps).unwrap() + "\n";
    write("gathering", &json);
}
