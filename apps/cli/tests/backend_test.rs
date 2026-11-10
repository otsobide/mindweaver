//! The panel's backend, driven as the server drives it: every query and task
//! as the JSON a browser sends, read into the backend's own types, and
//! handed to the `Backend` trait. No port is bound: the server's own suites
//! cover the transport, over a fake backend.

// The backend is the panel's, which a build without the `ui` feature lacks.
#![cfg(feature = "ui")]

mod common;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use common::contract;
use common::*;
use mindweaver_cli::backend::{Outcome, PanelBackend, Query, Switch};
use mindweaver_cli::task::{naming, shell, Caution};
use mindweaver_core::watch;
use mindweaver_server::{Backend, Grounds, Own, Preview, Ran, Refusal, Underway, API};
use serde_json::{json, Value};

/// The backend over the workspace at `root`, launched there with nothing
/// named, activated or at home, and no agent home: it can reach no
/// workspace but the fixture's.
fn backend(root: &Path) -> Checked {
    Checked(PanelBackend::launch(root, &Lookup::default()).unwrap())
}

/// What a browser sent, read as the server reads it.
fn task(sent: Value) -> Task {
    serde_json::from_value(sent.clone()).unwrap_or_else(|error| panic!("{sent}: {error}"))
}

/// A read a browser asked for: the query the server hands the backend,
/// and the JSON it was read from, whose `type` names the route asked.
struct Asked {
    query: Query,
    sent: Value,
}

fn query(sent: Value) -> Asked {
    Asked {
        query: serde_json::from_value(sent.clone())
            .unwrap_or_else(|error| panic!("{sent}: {error}")),
        sent,
    }
}

/// The panel's backend, each answer it gives held to what the API's
/// document says the route it answers answers with, as the server would
/// send it: a read's at its route, a preview's, a run's and a switch's at
/// the server's own, and a refusal as `{"code", "error"}` under the
/// status its grounds say. So the suite that holds the backend to its
/// rules holds its answers to the document too, and a route answering
/// outside it fails here, where no port is bound.
#[derive(Clone)]
struct Checked(PanelBackend);

impl std::ops::Deref for Checked {
    type Target = PanelBackend;

    fn deref(&self) -> &PanelBackend {
        &self.0
    }
}

impl Checked {
    /// The route `asked` is the query of: the one its `type` names, as the
    /// panel's transport asks it.
    fn query(&self, asked: Asked) -> Result<Value, Refusal> {
        let kind = asked.sent["type"].as_str().unwrap().to_owned();
        let route = PanelBackend::routes()
            .into_iter()
            .find(|route| route.path.strip_prefix('/') == Some(&kind))
            .unwrap_or_else(|| panic!("no route reads {kind}"));
        let answer = Backend::query(&self.0, asked.query);
        answered(route.method.as_str(), route.path, &answer);
        answer
    }

    fn preview(&self, task: &Task) -> Result<Preview, Refusal> {
        let answer = Backend::preview(&self.0, task);
        answered("POST", Own::Preview.path(), &answer);
        answer
    }

    /// Answered, as the server answers a run, with the lines that ran
    /// beside what they produced: run as one nobody follows.
    fn run(&self, task: Task) -> Result<Outcome, Refusal> {
        self.followed(task, &Underway::unfollowed())
    }

    /// [`run`](Self::run), followed as the server follows it: each step it
    /// reports handed to `underway`, which a cancel reaches.
    fn followed(&self, task: Task, underway: &Underway) -> Result<Outcome, Refusal> {
        let answer = Backend::run(&self.0, task, underway);
        let ran = answer.as_ref().map(|outcome| Ran {
            lines: outcome.runs.iter().map(|run| run.command.clone()).collect(),
            outcome: serde_json::to_value(outcome).unwrap(),
        });
        answered("POST", Own::Run.path(), &ran.map_err(Clone::clone));
        answer
    }

    fn switch(&self, to: Switch) -> Result<Value, Refusal> {
        let answer = Backend::switch(&self.0, to);
        answered("POST", Own::Switch.path(), &answer);
        answer
    }
}

/// That `answer`, given to `method` at `path` under the API's version,
/// holds to the document as the server would send it.
fn answered<T: serde::Serialize>(method: &str, path: &str, answer: &Result<T, Refusal>) {
    let path = format!("{API}{path}");
    match answer {
        Ok(answer) => {
            contract::answered(method, &path, 200, &serde_json::to_value(answer).unwrap());
        }
        Err(refusal) => {
            let refused = json!({"code": refusal.code, "error": refusal.message});
            contract::answered(method, &path, refusal.grounds.status().as_u16(), &refused);
        }
    }
}

/// Every file under `dir` with what it holds, to tell that nothing changed.
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                folders.push(entry.path());
            } else if kind.is_file() {
                found.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    found
}

/// A path under `dir`, spelled with the platform's separator.
fn under(dir: &Path, parts: &[&str]) -> String {
    let mut text = dir.to_str().unwrap().to_owned();
    for part in parts {
        text.push(std::path::MAIN_SEPARATOR);
        text.push_str(part);
    }
    text
}

fn add_to(holder: Value) -> Value {
    json!({
        "type": "add",
        "holder": holder,
        "kind": "skill",
        "name": "deploy",
        "description": "Ship it",
        "template": null,
    })
}

/// A browser's task, shaped to make a line act somewhere it should not:
/// each refused with a code by its preview and by its run, and nothing of it
/// run, the disk left as it was.
#[test]
fn a_task_shaped_to_escape_is_refused_with_a_code_and_nothing_runs() {
    let dir = workspace_with_two();
    let root = dir.path();
    let alpha = under(root, &["alpha"]);
    let gamma = under(root, &["gamma"]);
    let weaver = under(root, &[WEAVER_DIR]);
    fs::create_dir(&gamma).unwrap();
    let backend = backend(root);
    let before = files(root);

    let cases: Vec<(&str, Value, &str)> = vec![
        // Values that start with a dash.
        (
            "a path",
            json!({"type": "link", "path": "--yes"}),
            "path_relative",
        ),
        (
            "a -C",
            json!({"type": "link", "path": "-C"}),
            "path_relative",
        ),
        (
            "an entry",
            json!({"type": "unlink", "entry": "-rf"}),
            "not_registered",
        ),
        (
            "a holder",
            add_to(json!({"type": "project", "entry": "--home=/etc"})),
            "not_registered",
        ),
        (
            "a project",
            json!({"type": "install", "projects": ["-W"], "kind": "skill", "name": "alpha", "from": null}),
            "not_registered",
        ),
        (
            "a load",
            json!({"type": "unload", "entry": "--workspace="}),
            "not_loaded",
        ),
        // `..` parts.
        (
            "a link climbing out",
            json!({"type": "link", "path": under(root, &["alpha", "..", "..", "etc"])}),
            "path_unnormalized",
        ),
        (
            "a load climbing out",
            json!({"type": "load", "path": under(root, &[".."])}),
            "path_unnormalized",
        ),
        (
            "a holder climbing back",
            add_to(json!({"type": "project", "entry": "alpha/.."})),
            "not_registered",
        ),
        (
            "a holder from beside",
            json!({"type": "validate", "holder": {"type": "elsewhere", "entry": "../alpha"}}),
            "not_registered",
        ),
        // Relative paths.
        (
            "a bare name",
            json!({"type": "link", "path": "gamma"}),
            "path_relative",
        ),
        (
            "a path from here",
            json!({"type": "init_and_link", "path": "./gamma"}),
            "path_relative",
        ),
        (
            "a mark",
            json!({"type": "minds", "init": ["gamma"], "link": ["gamma"], "unlink": []}),
            "path_relative",
        ),
        (
            "a new place",
            json!({"type": "relink", "old": "alpha", "new": "elsewhere"}),
            "path_relative",
        ),
        // Entries nothing links.
        (
            "an unlink",
            json!({"type": "unlink", "entry": "gamma"}),
            "not_registered",
        ),
        (
            "a holder",
            json!({"type": "validate", "holder": {"type": "elsewhere", "entry": "gamma"}}),
            "not_registered",
        ),
        (
            "a holder by its path",
            json!({"type": "update", "holder": {"type": "project", "entry": alpha}, "reference": null, "force": false}),
            "not_registered",
        ),
        (
            "one project of two",
            json!({"type": "uninstall", "projects": ["alpha", "gamma"], "kind": "skill", "name": "alpha", "force": false}),
            "not_registered",
        ),
        (
            "a mark to unlink",
            json!({"type": "minds", "init": [], "link": [], "unlink": [gamma]}),
            "not_registered",
        ),
        (
            "an unload",
            json!({"type": "unload_here", "path": gamma}),
            "not_loaded",
        ),
        // Inside a `.mindweaver`.
        (
            "the folder itself",
            json!({"type": "link", "path": weaver}),
            "path_in_mindflayer",
        ),
        (
            "its library",
            json!({"type": "load", "path": under(root, &[WEAVER_DIR, "skills"])}),
            "path_in_mindflayer",
        ),
        (
            "a clone on its shelf",
            json!({"type": "init_and_link", "path": under(root, &[WEAVER_DIR, "shelf", "x"])}),
            "path_in_mindflayer",
        ),
        // URLs.
        (
            "a file URL",
            json!({"type": "link", "path": "file:///etc"}),
            "path_url",
        ),
        (
            "a file URL",
            json!({"type": "load", "path": "file:///etc/"}),
            "path_url",
        ),
        // A gather's source, where it names a folder on this machine.
        (
            "a gather from here",
            json!({"type": "gather", "url": "../etc", "folder": "skills", "reference": null}),
            "path_relative",
        ),
        (
            "a gather by a file URL with no root",
            json!({"type": "gather", "url": "file://etc", "folder": "skills", "reference": null}),
            "path_relative",
        ),
        (
            "a gather from the workspace's own clones",
            json!({"type": "gather", "url": under(root, &[WEAVER_DIR, "cache", "x"]),
                   "folder": "skills", "reference": null}),
            "path_in_mindflayer",
        ),
        (
            "a gather climbing out",
            json!({"type": "gather", "url": under(root, &["alpha", "..", "..", "etc"]),
                   "folder": "skills", "reference": null}),
            "path_unnormalized",
        ),
        // What no front end without a terminal runs.
        (
            "an editor",
            json!({"type": "edit", "holder": {"type": "workspace"}, "kind": "skill", "name": "x"}),
            "opens_editor",
        ),
        ("a screen", json!({"type": "open_install"}), "opens_screen"),
        (
            "an editor in a batch",
            json!({"type": "apply_install", "changes": [
                {"type": "uninstall", "projects": ["alpha"], "kind": "skill", "name": "alpha", "force": false},
                {"type": "edit", "holder": {"type": "workspace"}, "kind": "skill", "name": "x"},
            ]}),
            "opens_editor",
        ),
    ];

    for (what, sent, code) in cases {
        let preview = backend
            .preview(&task(sent.clone()))
            .map(|preview| preview.lines);
        let run = backend.run(task(sent.clone()));
        assert_eq!(
            preview.map_err(|refusal| refusal.code),
            Err(code),
            "{what}: {sent}"
        );
        assert_eq!(
            run.map_err(|refusal| refusal.code),
            Err(code),
            "{what}: {sent}"
        );
    }
    assert_eq!(files(root), before, "something ran");
}

/// A refusal of a command says which line it would have been, so the
/// panel can say which command it will not run.
#[test]
fn a_refused_command_is_named_in_what_the_refusal_says() {
    let dir = workspace_with_two();

    let refusal = backend(dir.path())
        .preview(&task(json!({
            "type": "edit", "holder": {"type": "workspace"}, "kind": "skill", "name": "x",
        })))
        .unwrap_err();

    assert_eq!(
        refusal,
        Refusal {
            code: "opens_editor",
            message: String::from(
                "`mindweaver edit skill:x` opens an editor, which needs a terminal to draw on"
            ),
            grounds: Grounds::Unfit,
        }
    );
}

/// The browser never supplies them: a task has no field for any, and one
/// with fields sent with one does not read as a task at all. One with none,
/// `shelf` say, passes it over, as serde does for a variant with no fields
/// to hold it to; nothing of it reaches a line either way.
#[test]
fn a_task_cannot_carry_a_directory_a_home_a_workspace_or_an_agent_home() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    for field in [
        "directory",
        "home",
        "workspace",
        "claude_config_dir",
        "codex_home",
        "opencode_config_dir",
        "agents_dir",
    ] {
        let mut sent = json!({"type": "validate", "holder": {"type": "workspace"}});
        sent[field] = json!("/etc");
        assert!(
            serde_json::from_value::<Task>(sent.clone()).is_err(),
            "{sent}"
        );
        let mut sent = json!({"type": "shelf"});
        sent[field] = json!("/etc");
        assert_eq!(
            backend.preview(&task(sent)).map(|preview| preview.lines),
            Ok(vec![format!(
                "mindweaver -W {} gather list",
                dir.path().display()
            )])
        );
    }
}

/// The holder's entry becomes the `-C` the backend resolved, absolute, and
/// the line's only one; the change lands in that project and nowhere else.
#[test]
fn a_task_held_by_a_linked_project_runs_in_that_project_with_the_directory_resolved() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    let beta = dir.path().join("beta");
    let backend = backend(dir.path());
    let sent = add_to(json!({"type": "project", "entry": "alpha"}));
    let line = format!(
        "mind -W {} -C {} add skill deploy 'Ship it'",
        dir.path().display(),
        alpha.display()
    );
    let beta_before = files(&beta);

    assert_eq!(
        backend
            .preview(&task(sent.clone()))
            .map(|preview| preview.lines),
        Ok(vec![line.clone()])
    );
    let outcome = backend.run(task(sent)).unwrap();

    assert_eq!(outcome.runs.len(), 1);
    let run = &outcome.runs[0];
    assert_eq!(run.command, line);
    assert_eq!(line.split(' ').filter(|word| *word == "-C").count(), 1);
    assert!(outcome.ok, "{outcome:?}");
    assert!(
        run.stdout.starts_with("created skill deploy at "),
        "{:?}",
        run.stdout
    );
    assert!(directory_for(&alpha, Kind::Skill)
        .join("deploy")
        .join("SKILL.md")
        .is_file());
    assert_eq!(files(&beta), beta_before);
}

/// What came of each line, with the code of what went wrong beside it, in
/// JSON as the server sends it on.
#[test]
fn a_line_that_fails_says_so_with_its_code() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());

    let outcome = backend
        .run(task(json!({
            "type": "remove",
            "holder": {"type": "project", "entry": "alpha"},
            "kind": "skill",
            "name": "missing",
            "everywhere": false,
            "yes": true,
        })))
        .unwrap();

    assert!(!outcome.ok);
    let json = serde_json::to_value(&outcome).unwrap();
    assert_eq!(json["runs"][0]["error"]["code"], "not_found");
    assert_eq!(json["runs"][0]["ok"], false);
    let mut keys: Vec<&String> = json["runs"][0].as_object().unwrap().keys().collect();
    keys.sort_unstable();
    assert_eq!(keys, ["command", "error", "ok", "stderr", "stdout"]);
}

/// What `mindweaver ui` prints once it stops: every line of a task that changed
/// something, failed ones included, with what each said, as the TUI's
/// transcript lays it out; a read changed nothing, and is left out. Kept by
/// a clone, as `ui` keeps one while the server holds the backend.
#[test]
fn the_transcript_holds_the_lines_that_changed_something_and_no_read() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    let kept = backend.clone();

    backend.run(task(json!({"type": "shelf"}))).unwrap();
    let added = backend
        .run(task(add_to(json!({"type": "workspace"}))))
        .unwrap();
    let missing = backend
        .run(task(json!({
            "type": "remove",
            "holder": {"type": "workspace"},
            "kind": "skill",
            "name": "missing",
            "everywhere": false,
            "yes": true,
        })))
        .unwrap();

    let added = &added.runs[0];
    let missing = &missing.runs[0];
    assert!(added.ok && !missing.ok);
    let mut expected = format!("$ {}\n{}", added.command, added.stdout);
    for line in &added.stderr {
        expected.push_str(&format!("{}\n", line.text));
    }
    expected.push_str(&format!("$ {}\n", missing.command));
    for line in &missing.stderr {
        expected.push_str(&format!("{}\n", line.text));
    }
    let error = missing.error.as_ref().unwrap();
    expected.push_str(&format!("error: {}\n", error.message));
    assert_eq!(kept.transcript(), expected);
    assert!(!expected.contains("gather list"));
}

#[test]
fn a_snapshot_is_of_the_workspace_the_panel_was_launched_on() {
    let dir = workspace_with_two();
    let backend = backend(&dir.path().join("alpha"));

    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();

    assert_eq!(snapshot["format"], 1);
    assert_eq!(snapshot["workspace"]["path"], dir.path().to_str().unwrap());
    assert_eq!(snapshot["workspace"]["reason"], "above");
    let entries: Vec<&Value> = snapshot["projects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|project| &project["entry"])
        .collect();
    assert_eq!(entries, [&json!("alpha"), &json!("beta")]);
}

/// The projects, the library and one project are the snapshot's own parts,
/// each exactly as the snapshot holds it; one project by its entry as the
/// snapshot lists it, and nothing else that names it.
#[test]
fn the_projects_the_library_and_one_project_are_the_snapshots_own_parts() {
    let dir = workspace_with_own();
    let backend = backend(dir.path());
    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();

    let projects = backend.query(query(json!({"type": "projects"}))).unwrap();
    let library = backend.query(query(json!({"type": "library"}))).unwrap();
    let beta = backend
        .query(query(json!({"type": "project", "entry": "beta"})))
        .unwrap();

    assert_eq!(projects, snapshot["projects"]);
    assert_eq!(library, snapshot["library"]);
    assert_eq!(beta, snapshot["projects"][1]);
    let alpha = dir.path().join("alpha");
    for entry in ["./beta", "gamma", alpha.to_str().unwrap(), ""] {
        let refused = backend
            .query(query(json!({"type": "project", "entry": entry})))
            .unwrap_err();
        assert_eq!(refused.code, "not_registered", "{entry}");
    }
}

#[test]
fn a_query_sent_with_a_field_it_does_not_have_is_refused() {
    for sent in [
        json!({"type": "snapshot", "home": "/etc"}),
        json!({"type": "contents", "holder": {"type": "workspace"}, "reference": "x", "file": "SKILL.md", "raw": true}),
        json!({"type": "contents", "holder": {"type": "workspace"}, "reference": "x"}),
        json!({"type": "files", "holder": {"type": "workspace"}, "reference": "x", "file": "SKILL.md"}),
        json!({"type": "files", "holder": {"type": "workspace"}}),
    ] {
        assert!(
            serde_json::from_value::<Query>(sent.clone()).is_err(),
            "{sent}"
        );
    }
}

/// `alpha`'s own skill, holding beside its `SKILL.md` what a gathered skill
/// might carry to run in a browser.
fn contents_fixture() -> (TempDir, Checked, PathBuf) {
    let dir = workspace_with_two();
    let skill = directory_for(&dir.path().join("alpha"), Kind::Skill).join("alpha");
    let backend = backend(dir.path());
    (dir, backend, skill)
}

fn contents(backend: &Checked, file: &str) -> Result<Value, Refusal> {
    backend.query(query(json!({
        "type": "contents",
        "holder": {"type": "project", "entry": "alpha"},
        "reference": "skill:alpha",
        "file": file,
    })))
}

#[test]
fn a_skills_file_comes_back_as_text_inside_json() {
    let (_dir, backend, _skill) = contents_fixture();

    let answer = contents(&backend, "SKILL.md").unwrap();

    assert_eq!(answer["text"], skill_file("alpha", "A skill"));
    assert_eq!(answer["withheld"], Value::Null);
    assert_eq!(answer["kind"], "skill");
    // What `edit --if-unchanged` takes of it, core's hash of its bytes.
    assert_eq!(
        answer["hash"],
        mindweaver_core::hash::bytes(skill_file("alpha", "A skill").as_bytes()).as_str()
    );
    // Cut where core cuts it, so the panel draws what it declares apart.
    assert_eq!(answer["manifest"]["front_matter"][0]["key"], "name");
    assert_eq!(answer["manifest"]["front_matter"][0]["value"], "alpha");
}

/// Markup a browser would render, or run, reaches it only as a string in
/// JSON: nothing in the answer says what type of file it is, so nothing can
/// serve it as one.
#[test]
fn an_html_asset_or_an_svg_holding_a_script_is_not_served_as_content() {
    let (_dir, backend, skill) = contents_fixture();
    let page = "<!doctype html><script>fetch('/api/v1/tasks',{method:'POST'})</script>\n";
    let image =
        "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(document.cookie)</script></svg>\n";
    fs::write(skill.join("page.html"), page).unwrap();
    fs::write(skill.join("logo.svg"), image).unwrap();

    for (file, markup) in [("page.html", page), ("logo.svg", image)] {
        let answer = contents(&backend, file).unwrap();
        assert_eq!(answer["text"], markup, "{file}");
        let mut keys: Vec<&String> = answer.as_object().unwrap().keys().collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["file", "hash", "kind", "manifest", "name", "size", "text", "withheld"]
        );
        // No file but the one the skill is read from is cut into parts.
        assert_eq!(answer["manifest"], Value::Null, "{file}");
        // What goes over the wire is a JSON object, the markup a string in it.
        let wire = serde_json::to_string(&answer).unwrap();
        assert!(wire.starts_with('{'), "{wire}");
    }
}

#[cfg(unix)]
#[test]
fn a_file_that_links_out_of_its_holder_is_not_served_as_content() {
    let (dir, backend, skill) = contents_fixture();
    let secret = dir.path().join("secret.env");
    fs::write(&secret, "TOKEN=1\n").unwrap();
    std::os::unix::fs::symlink(&secret, skill.join("notes.md")).unwrap();

    let refusal = contents(&backend, "notes.md").unwrap_err();

    assert_eq!(refusal.code, "contents_outside");
}

/// `show` prints an artifact whole, read through its links, which is what
/// a terminal's user asked for and not what a browser may: the file goes to
/// the browser only through the contents query, which refuses it.
#[cfg(unix)]
#[test]
fn a_rule_that_links_out_of_its_project_is_not_shown_to_the_browser() {
    let (dir, backend, _skill) = contents_fixture();
    let secret = dir.path().join("secret.txt");
    fs::write(&secret, "TOP SECRET KEY\n").unwrap();
    let rules = directory_for(&dir.path().join("alpha"), Kind::Rule);
    fs::create_dir_all(&rules).unwrap();
    std::os::unix::fs::symlink(&secret, rules.join("leak.md")).unwrap();
    let show = json!({
        "type": "show",
        "holder": {"type": "project", "entry": "alpha"},
        "reference": "rule:leak",
    });

    for refusal in [
        backend.preview(&task(show.clone())).unwrap_err(),
        backend.run(task(show.clone())).unwrap_err(),
    ] {
        assert_eq!(refusal.code, "show_unheld");
        assert!(!refusal.message.contains("TOP SECRET"), "{refusal:?}");
    }
    let asked = backend.query(query(json!({
        "type": "contents",
        "holder": {"type": "project", "entry": "alpha"},
        "reference": "rule:leak",
        "file": "leak.md",
    })));
    assert_eq!(
        asked.map_err(|refusal| refusal.code),
        Err("contents_outside")
    );
}

#[test]
fn an_oversized_file_is_not_served_as_content() {
    let (_dir, backend, skill) = contents_fixture();
    let cap = mindweaver_core::contents::CAP;
    fs::write(
        skill.join("huge.md"),
        "a".repeat(usize::try_from(cap).unwrap() + 1),
    )
    .unwrap();

    let answer = contents(&backend, "huge.md").unwrap();

    assert_eq!(answer["text"], Value::Null);
    assert_eq!(answer["withheld"], "too_large");
    assert_eq!(answer["size"], cap + 1);
}

#[test]
fn contents_are_asked_of_a_linked_project_or_the_workspace_and_nothing_else() {
    let (dir, backend, _skill) = contents_fixture();
    let ask = |holder: Value, reference: &str, file: &str| {
        backend.query(query(json!({
            "type": "contents", "holder": holder, "reference": reference, "file": file,
        })))
    };

    for entry in [
        "gamma",
        "../alpha",
        "alpha/..",
        dir.path().join("alpha").to_str().unwrap(),
    ] {
        assert_eq!(
            ask(
                json!({"type": "project", "entry": entry}),
                "alpha",
                "SKILL.md"
            )
            .map_err(|refusal| refusal.code),
            Err("not_registered"),
            "{entry}"
        );
    }
    assert_eq!(
        ask(
            json!({"type": "project", "entry": "alpha"}),
            "alpha",
            "../../beta/skills/beta/SKILL.md"
        )
        .map_err(|refusal| refusal.code),
        Err("contents_route_unfit")
    );
    // The workspace's own library holds nothing of `alpha`'s.
    assert_eq!(
        ask(json!({"type": "workspace"}), "alpha", "SKILL.md").map_err(|refusal| refusal.code),
        Err("not_found")
    );
}

/// What a skill holds is listed by the route each file is asked for by, of
/// a linked project or the workspace's own and nothing else, as a file is.
#[test]
fn an_artifacts_files_are_listed_by_the_route_each_is_read_by() {
    let (dir, backend, skill) = contents_fixture();
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("references").join("guide.md"), "# Guide\n").unwrap();
    let files = |holder: Value, reference: &str| {
        backend.query(query(json!({
            "type": "files", "holder": holder, "reference": reference,
        })))
    };

    let listed = files(json!({"type": "project", "entry": "alpha"}), "skill:alpha").unwrap();

    assert_eq!(listed["manifest"], "SKILL.md");
    let routes: Vec<&Value> = listed["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| &entry["path"])
        .collect();
    assert_eq!(
        routes,
        [
            &json!("SKILL.md"),
            &json!("references"),
            &json!("references/guide.md")
        ]
    );
    let guide = contents(&backend, "references/guide.md").unwrap();
    assert_eq!(guide["text"], "# Guide\n");
    for entry in ["gamma", dir.path().join("alpha").to_str().unwrap()] {
        assert_eq!(
            files(json!({"type": "project", "entry": entry}), "alpha")
                .map_err(|refusal| refusal.code),
            Err("not_registered"),
            "{entry}"
        );
    }
    assert_eq!(
        files(json!({"type": "workspace"}), "alpha").map_err(|refusal| refusal.code),
        Err("not_found")
    );
}

/// A link in a skill that leads out of its project is listed with why,
/// and nothing of where it leads reaches the browser.
#[cfg(unix)]
#[test]
fn a_link_out_of_its_holder_is_listed_with_why_and_never_followed() {
    let (dir, backend, skill) = contents_fixture();
    let secrets = dir.path().join("secrets");
    fs::create_dir_all(&secrets).unwrap();
    fs::write(secrets.join("token"), "TOP SECRET\n").unwrap();
    std::os::unix::fs::symlink(&secrets, skill.join("secrets")).unwrap();

    let listed = backend
        .query(query(json!({
            "type": "files",
            "holder": {"type": "project", "entry": "alpha"},
            "reference": "skill:alpha",
        })))
        .unwrap();

    let entries = listed["entries"].as_array().unwrap();
    let secrets = entries
        .iter()
        .find(|entry| entry["path"] == "secrets")
        .unwrap();
    assert_eq!(secrets["refused"]["code"], "contents_outside");
    assert_eq!(secrets["size"], Value::Null);
    let wire = serde_json::to_string(&listed).unwrap();
    assert!(
        !wire.contains("token") && !wire.contains("TOP SECRET"),
        "{wire}"
    );
}

/// Launched on the default workspace before anything made it, the backend
/// reads it as it will be, and its first change makes it first, by a line of
/// its own, as the TUI's does.
#[test]
fn the_default_workspace_is_made_by_the_first_change_and_said_first() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let fresh = dir.path().join("fresh");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&fresh).unwrap();
    new_project(&fresh);
    let backend = Checked(PanelBackend::launch(&home, &Lookup::at_home(Some(&home))).unwrap());

    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();
    assert_eq!(snapshot["workspace"]["made"], false);
    assert_eq!(snapshot["workspace"]["reason"], "home");
    let link = json!({"type": "link", "path": fresh.to_str().unwrap()});
    assert_eq!(
        backend.preview(&task(link.clone())).unwrap().lines,
        [
            String::from("mindweaver init"),
            format!("mindweaver -W {} link {}", home.display(), fresh.display())
        ]
    );
    assert!(!home.join(WEAVER_DIR).exists(), "a preview made it");

    let outcome: Outcome = backend.run(task(link)).unwrap();

    assert!(outcome.ok, "{outcome:?}");
    let workspace = WeaverWorkspace::open(&home).unwrap();
    assert!(workspace.is_linked(&fresh));
}

/// Its own making is one `mindweaver init`, not a second in front of it.
#[test]
fn the_default_workspace_asked_to_be_made_is_made_by_one_line() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let backend = Checked(PanelBackend::launch(&home, &Lookup::at_home(Some(&home))).unwrap());
    let init = json!({"type": "init_workspace"});

    assert_eq!(
        backend.preview(&task(init.clone())).unwrap().lines,
        [String::from("mindweaver init")]
    );
    let outcome: Outcome = backend.run(task(init)).unwrap();

    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(outcome.runs.len(), 1, "{outcome:?}");
    assert!(WeaverWorkspace::open(&home).is_ok());
}

/// Pinned to the one it was launched on: a workspace that has gone since is
/// refused, never made again by the first change.
#[test]
fn a_workspace_gone_since_launch_is_refused_rather_than_made_again() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    fs::remove_dir_all(dir.path().join(WEAVER_DIR)).unwrap();

    let refusal = backend
        .preview(&task(
            json!({"type": "link", "path": dir.path().join("alpha").to_str().unwrap()}),
        ))
        .unwrap_err();

    assert_eq!(refusal.code, "not_a_workspace");
    assert!(!dir.path().join(WEAVER_DIR).exists());
}

/// What the server watches is what core names for a snapshot, each path's
/// scope the holder a query reads it from, so the server learns no path of
/// its own.
#[test]
fn the_backend_watches_what_core_names_each_by_the_holder_a_query_names() {
    let dir = workspace_with_two();
    let root = dir.path();
    let workspace = WeaverWorkspace::open(root).unwrap();

    let watched = backend(root).watched_paths();

    let named: Vec<(PathBuf, Value)> = watch::watched(&workspace, None, &AgentHomes::default())
        .into_iter()
        .map(|each| {
            let scope = match each.scope {
                watch::Scope::Workspace => json!({"type": "workspace"}),
                watch::Scope::Project(entry) => json!({"type": "project", "entry": entry}),
            };
            (each.path, scope)
        })
        .collect();
    assert!(!named.is_empty());
    assert_eq!(
        watched
            .into_iter()
            .map(|each| (each.path, each.scope))
            .collect::<Vec<_>>(),
        named
    );
    let marker = root.join(WEAVER_DIR).join("weaver.toml");
    assert!(named.contains(&(marker, json!({"type": "workspace"}))));
    let alpha = root.join("alpha").join(".mind").join("settings.toml");
    assert!(named.contains(&(alpha, json!({"type": "project", "entry": "alpha"}))));
}

/// Gone since launch: watched where its marker would be, for its coming
/// back, rather than not at all.
#[test]
fn a_workspace_gone_since_launch_is_watched_for_its_marker() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    fs::remove_dir_all(dir.path().join(WEAVER_DIR)).unwrap();

    let watched = backend.watched_paths();

    let marker = dir.path().join(WEAVER_DIR).join("weaver.toml");
    assert!(watched
        .iter()
        .any(|each| each.path == marker && each.scope == json!({"type": "workspace"})));
}

/// What the panel has somebody confirm comes with the lines, each by its
/// place among them and in the CLI's words; a line that takes nothing away
/// asks nothing.
#[test]
fn a_preview_says_what_its_lines_take_away_for_the_panel_to_confirm() {
    let dir = workspace_with_two();
    let backend = backend(dir.path());
    let unlink = Caution::Unlinks.to_string();
    let removes = Caution::Removes.to_string();
    let uninstalls = Caution::Uninstalls.to_string();

    for (sent, confirm) in [
        (
            json!({"type": "unlink", "entry": "alpha"}),
            json!([{"line": 0, "caution": "unlinks", "message": unlink}]),
        ),
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "deploy", "everywhere": true, "yes": true,
            }),
            json!([
                {"line": 0, "caution": "removes", "message": removes},
                {"line": 0, "caution": "uninstalls", "message": uninstalls},
            ]),
        ),
        // Its dry run, which takes nothing.
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "deploy", "everywhere": true, "yes": false,
            }),
            json!([]),
        ),
        // Its `mind init`, its `mindweaver link` and its `mindweaver unlink`.
        (
            json!({
                "type": "minds",
                "init": [under(dir.path(), &["gamma"])],
                "link": [under(dir.path(), &["gamma"])],
                "unlink": [under(dir.path(), &["beta"])],
            }),
            json!([{"line": 2, "caution": "unlinks", "message": unlink}]),
        ),
        (add_to(json!({"type": "workspace"})), json!([])),
        (
            json!({"type": "link", "path": under(dir.path(), &["alpha"])}),
            json!([]),
        ),
    ] {
        let preview = backend.preview(&task(sent.clone())).unwrap();
        assert_eq!(Value::Array(preview.confirm), confirm, "{sent}");
    }
}

/// Counted among the lines the panel shows, the `mindweaver init` that makes
/// the default workspace first among them.
#[test]
fn what_to_confirm_counts_the_line_that_makes_the_default_workspace() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let lookup = Lookup {
        home: Some(home.clone()),
        ..Lookup::default()
    };
    let backend = Checked(PanelBackend::launch(&home, &lookup).unwrap());

    let preview = backend
        .preview(&task(json!({
            "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
            "name": "deploy", "everywhere": false, "yes": true,
        })))
        .unwrap_or_else(|refusal| panic!("{refusal:?}"));

    assert_eq!(
        preview.lines,
        [
            String::from("mindweaver init"),
            format!("mindweaver -W {} remove --yes skill:deploy", home.display())
        ]
    );
    assert_eq!(preview.confirm.len(), 1, "{preview:?}");
    assert_eq!(preview.confirm[0]["line"], 1);
    assert!(!home.join(WEAVER_DIR).exists(), "a preview made it");
}

/// What a scan the panel asks for is answered with: what `mindweaver scan
/// --suggest --json` finds, each directory as it prints it and in its
/// order, the words the command says why in, and what marking each asks
/// for; nothing for one linked.
#[test]
fn a_scan_is_answered_with_what_weaver_scan_suggest_finds_in_its_order_and_words() {
    let dir = workspace_with_candidates();
    let backend = backend(dir.path());

    let answer = backend
        .query(query(json!({"type": "scan", "roots": [], "depth": null})))
        .unwrap();

    let suggested = weaver(dir.path(), &["scan", "--suggest", "--json"]).unwrap();
    let printed: Value = serde_json::from_str(&suggested.stdout).unwrap();
    let found: Vec<&Value> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|suggestion| &suggestion["found"])
        .collect();
    assert_eq!(
        Value::Array(found.into_iter().cloned().collect()),
        printed["data"]["found"]
    );
    let offered: Vec<(&str, &Value, &Value, &Value)> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|suggestion| {
            (
                suggestion["found"]["route"].as_str().unwrap(),
                &suggestion["why"],
                &suggestion["offers"],
                &suggestion["moved_from"],
            )
        })
        .collect();
    assert_eq!(
        offered,
        [
            (
                "gamma",
                &json!("mind project with an id"),
                &json!("link"),
                &Value::Null
            ),
            (
                "repo",
                &json!("git repository"),
                &json!("init_and_link"),
                &Value::Null
            ),
            ("alpha", &Value::Null, &Value::Null, &Value::Null),
            ("beta", &Value::Null, &Value::Null, &Value::Null),
        ]
    );
    assert_eq!(answer["roots"], json!([dir.path().to_str().unwrap()]));
    assert_eq!(answer["depth"], 3);
    assert_eq!(answer["warnings"], json!([]));
}

/// Only the folders the browser names, each as a folder to link is named,
/// and only read: one the policy refuses is refused with its code before
/// anything is walked, and a scan leaves the disk as it found it.
#[test]
fn a_scan_walks_only_folders_the_policy_allows_and_writes_nothing() {
    let dir = workspace_with_candidates();
    let backend = backend(dir.path());
    let before = files(dir.path());
    let climbing = under(dir.path(), &["repo", ".."]);
    let weaver_dir = under(dir.path(), &[WEAVER_DIR]);

    for (root, code) in [
        ("repo", "path_relative"),
        ("file:///etc", "path_url"),
        (climbing.as_str(), "path_unnormalized"),
        (weaver_dir.as_str(), "path_in_mindflayer"),
    ] {
        let refusal = backend
            .query(query(
                json!({"type": "scan", "roots": [under(dir.path(), &["repo"]), root], "depth": 1}),
            ))
            .unwrap_err();
        assert_eq!(refusal.code, code, "{root}");
    }

    let answer = backend
        .query(query(
            json!({"type": "scan", "roots": [under(dir.path(), &["repo"])], "depth": 0}),
        ))
        .unwrap();
    assert_eq!(answer["found"].as_array().unwrap().len(), 1, "{answer}");
    assert_eq!(answer["depth"], 0);
    assert_eq!(files(dir.path()), before);
    // A field a scan does not have is refused, as a task's is.
    assert!(serde_json::from_value::<Query>(
        json!({"type": "scan", "roots": [], "depth": 1, "deep": true})
    )
    .is_err());
}

/// A folder named that is not there is a warning with its code beside an
/// answer, as `mindweaver scan` warns of it, never an empty answer passed off as
/// complete.
#[test]
fn a_scan_says_what_it_could_not_read_as_weaver_scan_warns_of_it() {
    let dir = workspace_with_candidates();
    let missing = under(dir.path(), &["wrok"]);

    let answer = backend(dir.path())
        .query(query(
            json!({"type": "scan", "roots": [missing.clone()], "depth": 1}),
        ))
        .unwrap();

    let scan = weaver(dir.path(), &["scan", &missing]).unwrap();
    assert_eq!(answer["found"], json!([]));
    let said: Vec<Value> = scan
        .stderr
        .iter()
        .map(|line| json!({"text": line.text, "code": line.code}))
        .collect();
    assert_eq!(answer["warnings"], Value::Array(said));
    assert_eq!(answer["warnings"][0]["code"], "scan_folder_unlisted");
}

/// What the panel's explorer is answered with: the folders directly inside
/// the one it names, as core lists them, each with what it is and whether
/// the workspace links it; none named is the workspace's root.
#[test]
fn a_folder_is_answered_with_what_is_directly_inside_it_as_core_lists_it() {
    let dir = workspace_with_candidates();
    fs::create_dir(dir.path().join(".cache")).unwrap();
    let backend = backend(dir.path());

    let root = backend
        .query(query(
            json!({"type": "folders", "path": null, "hidden": false}),
        ))
        .unwrap();
    let named = backend
        .query(query(
            json!({"type": "folders", "path": dir.path().to_str().unwrap(), "hidden": false}),
        ))
        .unwrap();

    assert_eq!(root, named);
    assert_eq!(root["path"], json!(dir.path().to_str().unwrap()));
    assert_eq!(root["is"], "workspace");
    let said: Vec<(&str, &str, bool)> = root["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|folder| {
            (
                folder["name"].as_str().unwrap(),
                folder["is"].as_str().unwrap(),
                folder["linked"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("alpha", "project", true),
            ("beta", "project", true),
            ("gamma", "project", false),
            ("repo", "repository", false),
        ]
    );
    assert_eq!(root["hidden"], 1);
    assert_eq!(root["failures"], json!([]));
    // Launched with no home, so no way back to one is named.
    assert_eq!(root["home"], Value::Null);

    let hidden = backend
        .query(query(
            json!({"type": "folders", "path": null, "hidden": true}),
        ))
        .unwrap();
    assert_eq!(hidden["folders"][0]["name"], ".cache");
    let inside = backend
        .query(query(
            json!({"type": "folders", "path": under(dir.path(), &["repo"]), "hidden": true}),
        ))
        .unwrap();
    assert_eq!(inside["is"], "repository");
    assert_eq!(inside["folders"][0]["name"], ".git");
    assert_eq!(
        inside["trail"].as_array().unwrap().last().unwrap(),
        &json!({"name": "repo", "path": under(dir.path(), &["repo"])})
    );
}

/// The home the panel was launched with is named beside every listing, so
/// the explorer can go back to it, and is listed as any folder is.
#[test]
fn a_folder_listing_names_the_launchs_home() {
    let dir = workspace_with_candidates();
    let home = dir.path().join("home");
    fs::create_dir_all(home.join("Projects")).unwrap();
    let lookup = Lookup {
        home: Some(home.clone()),
        ..Lookup::default()
    };
    let backend = Checked(PanelBackend::launch(dir.path(), &lookup).unwrap());

    let root = backend
        .query(query(
            json!({"type": "folders", "path": null, "hidden": false}),
        ))
        .unwrap();
    let at_home = backend
        .query(query(
            json!({"type": "folders", "path": root["home"], "hidden": false}),
        ))
        .unwrap();

    assert_eq!(root["home"], json!(home.to_str().unwrap()));
    assert_eq!(at_home["folders"][0]["name"], "Projects");
    assert!(
        !home.join(WEAVER_DIR).exists(),
        "a listing made the default"
    );
}

/// Only a folder the browser may name, as a folder to link is: one the
/// policy refuses is refused with its code before anything of it is read,
/// as given and through a link into a `.mindweaver`, one not there is
/// refused with core's, and a listing leaves the disk as it found it.
#[test]
fn a_folder_is_listed_only_where_the_policy_allows_and_nothing_is_written() {
    let dir = workspace_with_candidates();
    let backend = backend(dir.path());
    let before = files(dir.path());
    let climbing = under(dir.path(), &["repo", ".."]);
    let weaver_dir = under(dir.path(), &[WEAVER_DIR]);
    // Inside a `.mindweaver` and not there: refused for where it is, which
    // nothing had to be read to tell.
    let unmade = under(dir.path(), &[WEAVER_DIR, "nowhere"]);
    let missing = under(dir.path(), &["wrok"]);
    // A link into the workspace's own `.mindweaver`, which only a system
    // with links can make.
    #[cfg(unix)]
    let through = {
        let link = dir.path().join("hub");
        std::os::unix::fs::symlink(dir.path().join(WEAVER_DIR), &link).unwrap();
        Some(link.to_str().unwrap().to_owned())
    };
    #[cfg(not(unix))]
    let through: Option<String> = None;
    let mut cases = vec![
        ("repo", "path_relative"),
        ("file:///etc", "path_url"),
        ("--home=/etc", "path_relative"),
        (climbing.as_str(), "path_unnormalized"),
        (weaver_dir.as_str(), "path_in_mindflayer"),
        (unmade.as_str(), "path_in_mindflayer"),
        (missing.as_str(), "browse_missing"),
    ];
    if let Some(through) = &through {
        cases.push((through.as_str(), "path_in_mindflayer"));
    }

    for (path, code) in cases {
        let refusal = backend
            .query(query(
                json!({"type": "folders", "path": path, "hidden": true}),
            ))
            .unwrap_err();
        assert_eq!(refusal.code, code, "{path}");
    }

    let listed = backend
        .query(query(
            json!({"type": "folders", "path": null, "hidden": true}),
        ))
        .unwrap();
    let names: Vec<&str> = listed["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|folder| folder["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&WEAVER_DIR), "{names:?}");
    if through.is_some() {
        assert!(!names.contains(&"hub"), "{names:?}");
        fs::remove_file(dir.path().join("hub")).unwrap();
    }
    assert_eq!(files(dir.path()), before);
    // A field a listing does not have is refused, as a task's is.
    assert!(serde_json::from_value::<Query>(
        json!({"type": "folders", "path": null, "hidden": false, "depth": 1})
    )
    .is_err());
}

/// A read whose parameters name what is not there is refused as absent,
/// a 404, with core's code: an entry no registry holds, an artifact
/// nothing holds, a file or a folder that does not exist, or that is no
/// file or folder. What is there and still refused is unfit, a 422, as is
/// anything a body names, a draft's holder or a task's project, whose
/// route is there whatever the body holds.
#[test]
fn a_read_naming_what_is_not_there_is_absent_and_a_body_naming_it_unfit() {
    let (dir, backend, skill) = contents_fixture();
    let root = dir.path();
    fs::create_dir(skill.join("references")).unwrap();
    let alpha = json!({"type": "project", "entry": "alpha"});
    let ghost = json!({"type": "project", "entry": "ghost"});
    let gone = under(root, &["gone"]);
    let skill_file = skill.join("SKILL.md");
    let not_a_folder = skill_file.to_str().unwrap();

    for (what, sent, code, grounds) in [
        (
            "an entry no registry holds",
            json!({"type": "project", "entry": "ghost"}),
            "not_registered",
            Grounds::Absent,
        ),
        (
            "an artifact's files, of an entry no registry holds",
            json!({"type": "files", "holder": ghost, "reference": "skill:alpha"}),
            "not_registered",
            Grounds::Absent,
        ),
        (
            "an artifact nothing holds",
            json!({"type": "files", "holder": alpha, "reference": "skill:ghost"}),
            "not_found",
            Grounds::Absent,
        ),
        (
            "a file that is not there",
            json!({"type": "contents", "holder": alpha, "reference": "skill:alpha", "file": "GHOST.md"}),
            "contents_missing",
            Grounds::Absent,
        ),
        (
            "a folder asked for as a file",
            json!({"type": "contents", "holder": alpha, "reference": "skill:alpha", "file": "references"}),
            "contents_not_a_file",
            Grounds::Absent,
        ),
        (
            "a file's route that climbs out",
            json!({"type": "contents", "holder": alpha, "reference": "skill:alpha", "file": "../x"}),
            "contents_route_unfit",
            Grounds::Unfit,
        ),
        (
            "a folder that does not exist",
            json!({"type": "folders", "path": gone, "hidden": false}),
            "browse_missing",
            Grounds::Absent,
        ),
        (
            "a file asked for as a folder",
            json!({"type": "folders", "path": not_a_folder, "hidden": false}),
            "browse_not_a_folder",
            Grounds::Absent,
        ),
        (
            "a folder the policy refuses",
            json!({"type": "folders", "path": "Projects", "hidden": false}),
            "path_relative",
            Grounds::Unfit,
        ),
        (
            "a draft's holder no registry holds, in a body",
            json!({"type": "validate", "holder": ghost, "draft": {
                "type": "edit", "reference": "skill:alpha", "text": "x",
            }}),
            "not_registered",
            Grounds::Unfit,
        ),
    ] {
        let refusal = backend.query(query(sent.clone())).unwrap_err();
        assert_eq!(
            (refusal.code, refusal.grounds),
            (code, grounds),
            "{what}: {sent}"
        );
    }

    let sent = add_to(json!({"type": "project", "entry": "ghost"}));
    for refusal in [
        backend.preview(&task(sent.clone())).unwrap_err(),
        backend.run(task(sent.clone())).unwrap_err(),
    ] {
        assert_eq!(
            (refusal.code, refusal.grounds),
            ("not_registered", Grounds::Unfit),
            "a task's project, in a body"
        );
    }
}

/// A project found moved offers no link beside the entry it left stale,
/// but names that entry as the marker spells it, which a relink takes.
#[test]
fn a_scan_names_the_entry_a_moved_project_left_for_its_relink() {
    let dir = TempDir::new().unwrap();
    let projects = dir.path().join("Projects");
    let collapse = projects.join("collapse");
    fs::create_dir_all(&collapse).unwrap();
    weaver(&projects, &["init"]).unwrap();
    weaver(&projects, &["add", "skill", "deploy", "Ship the service"]).unwrap();
    new_project(&collapse);
    weaver(&projects, &["link", "collapse"]).unwrap();
    weaver(&projects, &["install", "-p", "collapse", "deploy"]).unwrap();
    fs::create_dir_all(projects.join("apps")).unwrap();
    let moved = projects.join("apps/collapse");
    fs::rename(&collapse, &moved).unwrap();
    let backend = backend(&projects);

    let answer = backend
        .query(query(json!({"type": "scan", "roots": [], "depth": null})))
        .unwrap();

    let suggestion = &answer["found"][0];
    assert_eq!(suggestion["found"]["moved"], "collapse");
    assert_eq!(suggestion["moved_from"], "collapse");
    assert_eq!(suggestion["offers"], Value::Null);
    assert_eq!(suggestion["why"], "moved: collapse -> apps/collapse");
    let relink = task(json!({
        "type": "relink",
        "old": suggestion["moved_from"],
        "new": suggestion["found"]["path"],
    }));
    assert_eq!(
        backend.preview(&relink).unwrap().lines,
        [format!(
            "mindweaver -W {} relink collapse {}",
            projects.display(),
            moved.to_str().unwrap()
        )]
    );
    let outcome = backend.run(relink).unwrap();
    assert!(outcome.ok, "{outcome:?}");
    assert!(registry(&projects).contains("\"apps/collapse\""));
}

/// The panel's link batch is the minds screen's: for the same directories
/// marked, the lines the backend previews for what the panel sends are the
/// lines the screen's task is, each naming the workspace as every line of
/// the panel's does, each directory by its path, from a scan of a
/// folder outside the workspace, where both spell it so. Inside it they
/// spell a folder apart, and the batch is the same folder for folder
/// ([`inside_the_workspace_the_panels_link_batch_is_the_minds_screens_folder_for_folder`]).
#[test]
fn the_panels_link_batch_is_the_minds_screens_for_the_same_selection() {
    let dir = workspace_with_candidates();
    let work = TempDir::new().unwrap();
    let delta = work.path().join("acme/delta");
    fs::create_dir_all(&delta).unwrap();
    new_project(&delta);
    fs::create_dir_all(work.path().join("plain/.git")).unwrap();
    let root = work.path().to_str().unwrap();
    let backend = backend(dir.path());

    // Every directory found and not linked, marked as the scan offers it.
    let answer = backend
        .query(query(
            json!({"type": "scan", "roots": [root], "depth": null}),
        ))
        .unwrap();
    let marks: Vec<Value> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|suggestion| !suggestion["offers"].is_null())
        .map(|suggestion| json!({"path": suggestion["found"]["path"], "change": suggestion["offers"]}))
        .collect();
    assert_eq!(marks.len(), 2, "{answer}");
    let previewed = backend
        .preview(&task(json!({"type": "marks", "marks": marks})))
        .unwrap()
        .lines;

    // The same ones ticked on the screen, opened on the same folder.
    let mut app = tui::minds(dir.path(), &[work.path().to_path_buf()], &Lookup::default()).unwrap();
    for _ in 0..2 {
        press_key(&mut app, KeyCode::Char(' '));
        press_key(&mut app, KeyCode::Down);
    }
    let tui::state::Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let named = dir.path().to_str().unwrap();
    let screened: Vec<String> = screen
        .task()
        .unwrap()
        .commands()
        .iter()
        .map(|line| shell(&naming(line, named)))
        .collect();

    assert_eq!(previewed, screened);
    assert_eq!(
        previewed,
        [
            format!("mind -W {named} -C {} init", under(work.path(), &["plain"])),
            format!(
                "mindweaver -W {named} link {} {}",
                under(work.path(), &["acme", "delta"]),
                under(work.path(), &["plain"])
            ),
        ]
    );
}

/// Inside the workspace the two spell a folder apart: the screen by the
/// route the scan gives from the root, where its lines run, and the panel
/// by its path, which is all the path policy lets a browser name. The same
/// batch all the same, folder for folder, and run, each leaves the
/// workspace linking the same entries, the repository made a project first.
#[test]
fn inside_the_workspace_the_panels_link_batch_is_the_minds_screens_folder_for_folder() {
    let panel = workspace_with_candidates();
    let screened = workspace_with_candidates();
    let backend = backend(panel.path());

    // Every directory found and not linked, marked as the scan offers it,
    // under the workspace's own folder, where the page starts.
    let answer = backend
        .query(query(json!({"type": "scan", "roots": [], "depth": null})))
        .unwrap();
    let marks: Vec<Value> = answer["found"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|suggestion| !suggestion["offers"].is_null())
        .map(|suggestion| json!({"path": suggestion["found"]["path"], "change": suggestion["offers"]}))
        .collect();
    assert_eq!(marks.len(), 2, "{answer}");
    let marked = task(json!({"type": "marks", "marks": marks}));
    let previewed = backend.preview(&marked).unwrap().lines;

    // The same ones ticked on the screen, in a workspace laid out the same.
    let mut app = tui::minds(screened.path(), &[], &Lookup::default()).unwrap();
    for _ in 0..2 {
        press_key(&mut app, KeyCode::Char(' '));
        press_key(&mut app, KeyCode::Down);
    }
    let Mode::Minds(screen) = &app.mode else {
        panic!("{:?}", app.mode)
    };
    let ticked = screen.task().unwrap();
    let Task::Minds { init, link, unlink } = &ticked else {
        panic!("{ticked:?}")
    };
    assert_eq!(link, &["gamma", "repo"]);
    // Each route read from the root the screen's lines run from.
    let spelled = |routes: &[String]| -> Vec<String> {
        routes
            .iter()
            .map(|route| under(panel.path(), &[route]))
            .collect()
    };
    let by_path = Task::Minds {
        init: spelled(init),
        link: spelled(link),
        unlink: spelled(unlink),
    };
    let by_path: Vec<String> = by_path
        .commands()
        .iter()
        .map(|line| shell(&naming(line, panel.path().to_str().unwrap())))
        .collect();
    assert_eq!(previewed, by_path);

    let outcome = backend.run(marked).unwrap();
    assert!(outcome.ok, "{outcome:?}");
    tui::step(&mut app, ticked);
    tui::wait(&mut app);

    let entries = |dir: &Path| -> Vec<PathBuf> {
        WeaverWorkspace::open(dir)
            .unwrap()
            .members()
            .into_iter()
            .map(|member| member.entry)
            .collect()
    };
    assert_eq!(entries(panel.path()), entries(screened.path()));
    assert_eq!(
        entries(panel.path()),
        ["alpha", "beta", "gamma", "repo"].map(PathBuf::from)
    );
    for dir in [panel.path(), screened.path()] {
        assert!(
            MindProject::is_marked(&dir.join("repo")),
            "{}",
            dir.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Loading and unloading another workspace
// ---------------------------------------------------------------------------

/// Under the folder it returns: `home`, made the default workspace, then
/// beside it `Projects`, linking `alpha`, and `team`, with a skill of its
/// own, each made by a `mindweaver init` that remembers it there. Its second
/// value is `home`, its third `Projects`.
fn knowing() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let root = dir.path().join("Projects");
    let team = dir.path().join("team");
    for workspace in [&home, &root, &team] {
        fs::create_dir_all(workspace).unwrap();
        weaver_at_home(workspace, &home, &["init"]).unwrap();
    }
    weaver(
        &team,
        &["add", "skill", "notes", "Keep notes as the team does"],
    )
    .unwrap();
    fs::create_dir_all(root.join("alpha")).unwrap();
    new_project(&root.join("alpha"));
    weaver(&root, &["link", "alpha"]).unwrap();
    (dir, home, root)
}

/// What the TUI's load form, opened on `app`, runs once `typed` is typed
/// into it, or with nothing typed, the first loaded one picked and its
/// unload confirmed, which it then runs: the task, run as the loop runs it.
fn loaded_from_the_screen(app: &mut App, typed: Option<&str>) -> Task {
    press_key(app, KeyCode::Char('L'));
    assert!(matches!(app.mode, Mode::Loading(_)), "{:?}", app.mode);
    match typed {
        Some(typed) => {
            type_text(app, typed);
            press_and_run(app, KeyCode::Enter).unwrap()
        }
        None => {
            press_key(app, KeyCode::Enter);
            assert!(matches!(app.mode, Mode::Confirming(_)), "{:?}", app.mode);
            press_and_run(app, KeyCode::Char('y')).unwrap()
        }
    }
}

/// Loading one of the workspaces this machine knows from the panel, by the
/// path the snapshot lists it by, is the task the TUI's load form runs for
/// that path typed, and unloading it, by the entry the snapshot then lists
/// it by, the task the form runs for it picked: the same `Task` each time,
/// so the same lines, the panel's naming the workspace, and run from the
/// panel each leaves the workspace's
/// marker as the screen's left it.
#[test]
fn loading_and_unloading_from_the_panel_are_the_tuis_tasks_for_the_same_workspace() {
    let (_dir, home, root) = knowing();
    let lookup = Lookup::at_home(Some(&home));
    let backend = Checked(PanelBackend::launch(&root, &lookup).unwrap());
    let mut app =
        App::new(root.clone(), tui::load(&root, &lookup).unwrap()).with_lookup(lookup.clone());
    let named = root.to_str().unwrap().to_owned();
    let lines = |task: &Task| -> Vec<String> {
        task.commands()
            .iter()
            .map(|line| shell(&naming(line, &named)))
            .collect()
    };
    let unloaded = registry(&root);

    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();
    let known = snapshot["known"]
        .as_array()
        .unwrap()
        .iter()
        .find(|known| known["name"] == "team")
        .unwrap_or_else(|| panic!("{snapshot}"));
    assert_eq!(known["loadable"], true, "{known}");
    let path = known["path"].as_str().unwrap();
    let load = task(json!({"type": "load", "path": path}));
    let previewed = backend.preview(&load).unwrap();
    let screened = loaded_from_the_screen(&mut app, Some(path));

    assert_eq!(screened, load);
    assert_eq!(previewed.lines, lines(&screened));
    assert_eq!(
        previewed.lines,
        [format!("mindweaver -W {named} load {path}")]
    );
    assert!(previewed.confirm.is_empty(), "{:?}", previewed.confirm);
    let loaded = registry(&root);
    assert!(loaded.contains("loaded = [\"../team\"]"), "{loaded}");

    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();
    let entry = snapshot["loaded"][0]["entry"].as_str().unwrap();
    let known = snapshot["known"]
        .as_array()
        .unwrap()
        .iter()
        .find(|known| known["name"] == "team")
        .unwrap();
    assert_eq!(known["loaded_as"], entry, "{known}");
    assert_eq!(known["loadable"], false, "{known}");
    let unload = task(json!({"type": "unload", "entry": entry}));
    let previewed = backend.preview(&unload).unwrap();
    let screened = loaded_from_the_screen(&mut app, None);

    assert_eq!(screened, unload);
    assert_eq!(previewed.lines, lines(&screened));
    let asked: Vec<&str> = previewed
        .confirm
        .iter()
        .map(|confirm| confirm["caution"].as_str().unwrap())
        .collect();
    assert_eq!(asked, ["unloads"]);
    assert_eq!(registry(&root), unloaded);

    assert!(backend.run(load).unwrap().ok);
    assert_eq!(registry(&root), loaded);
    assert!(backend.run(unload).unwrap().ok);
    assert_eq!(registry(&root), unloaded);
}

// ---------------------------------------------------------------------------
// A plan of boxes ticked and unticked
// ---------------------------------------------------------------------------

/// Under the folder it returns: a workspace at `home`, with `deploy` and
/// `review` of its own, loading `team` beside it, whose `deploy` is the
/// team's; linking `alpha` and `beta`, each a repository given `mind init`,
/// `alpha` holding the workspace's `deploy` and `beta` its `review`. Its
/// second value is `home`.
fn assigning() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let team = dir.path().join("team");
    for root in [&home, &team] {
        fs::create_dir_all(root).unwrap();
        weaver(root, &["init"]).unwrap();
    }
    weaver(&home, &["add", "skill", "deploy", "Ship the service"]).unwrap();
    weaver(&home, &["add", "skill", "review", "Review a change"]).unwrap();
    weaver(&team, &["add", "skill", "deploy", "Ship it the team's way"]).unwrap();
    weaver(&home, &["load", "../team"]).unwrap();
    for name in ["alpha", "beta"] {
        let root = home.join(name);
        fs::create_dir(&root).unwrap();
        new_project(&root);
        weaver(&home, &["link", name]).unwrap();
    }
    weaver(
        &home,
        &[
            "install",
            "-p",
            "alpha",
            "--from",
            "workspace",
            "skill:deploy",
        ],
    )
    .unwrap();
    weaver(&home, &["install", "-p", "beta", "skill:review"]).unwrap();
    (dir, home)
}

fn plan(backend: &Checked, selection: Value) -> Result<Value, Refusal> {
    backend.query(query(json!({"type": "plan", "selection": selection})))
}

fn cell(project: &str, reference: &str, origin: &str, wanted: bool) -> Value {
    json!({"project": project, "reference": reference, "origin": origin, "wanted": wanted})
}

/// What the projects under `home` hold: each file outside a `.git`, a
/// `.mind` or the `.mindweaver`, by its route from `home`, and what the
/// workspace records installed in each, by kind and name, with where it
/// came from.
#[allow(clippy::type_complexity)]
fn held(
    home: &Path,
) -> (
    BTreeMap<PathBuf, Vec<u8>>,
    Vec<(String, Kind, String, Option<String>)>,
) {
    let kept = |route: &Path| {
        !route.components().any(|part| {
            [".git", MIND_DIR, WEAVER_DIR].contains(&part.as_os_str().to_str().unwrap_or(""))
        })
    };
    let copies = files(home)
        .into_iter()
        .map(|(path, bytes)| (path.strip_prefix(home).unwrap().to_path_buf(), bytes))
        .filter(|(route, _)| kept(route))
        .collect();
    let workspace = WeaverWorkspace::locate(home).unwrap().unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();
    let mut records = Vec::new();
    for project in ["alpha", "beta"] {
        for kind in Kind::ALL {
            for record in ledger.records(project, kind).unwrap() {
                records.push((
                    project.to_owned(),
                    kind,
                    record.name.into_string(),
                    record.origin.map(String::from),
                ));
            }
        }
    }
    (copies, records)
}

/// `home`, a workspace loading `team`, which keeps the MCP server
/// `github` of its own, a program started with a secret, and linking
/// `alpha`, a project `mind init` made for every agent.
fn serving() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    let team = dir.path().join("team");
    for root in [&home, &team] {
        fs::create_dir_all(root).unwrap();
        weaver(root, &["init"]).unwrap();
    }
    weaver(
        &team,
        &[
            "add",
            "mcp",
            "github",
            "GitHub",
            "--command",
            "npx",
            "--arg=-y",
            "--arg",
            "@acme/server",
            "--secret-env",
            "GITHUB_TOKEN",
        ],
    )
    .unwrap();
    weaver(&home, &["load", "../team"]).unwrap();
    let root = home.join("alpha");
    fs::create_dir(&root).unwrap();
    new_project(&root);
    weaver(&home, &["link", "alpha"]).unwrap();
    (dir, home)
}

/// A server from a loaded workspace waits for a yes: its plan's task
/// carries none, and its line writes nothing, while beside it the plan
/// hands what each agent would run, as the install screen's confirmation
/// says it, and the one task carrying the yes, which writes it.
#[test]
fn a_server_that_asks_first_is_planned_with_what_each_agent_would_run_and_its_yes_apart() {
    let (dir, home) = serving();
    let backend = backend(&home);
    let before = files(dir.path());
    let entry = |name: &str| home.join("alpha").join(name);

    let answer = plan(
        &backend,
        json!([cell("alpha", "mcp:github", "load:../team", true)]),
    )
    .unwrap();

    assert_eq!(files(dir.path()), before);
    let plain = json!({
        "type": "install", "projects": ["alpha"], "kind": "mcp", "name": "github",
        "from": "load:../team", "yes": false,
    });
    assert_eq!(answer["tasks"], json!([plain]));
    let mut yes = plain.clone();
    yes["yes"] = json!(true);
    assert_eq!(answer["asks"][0]["task"], 0);
    assert_eq!(answer["asks"][0]["confirmed"], yes);
    let lines: Vec<&str> = answer["asks"][0]["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| line.as_str().unwrap())
        .collect();
    assert_eq!(
        lines[..4],
        [
            "mcp github into alpha, from load:../team:",
            "  claude runs: npx -y @acme/server",
            "  codex runs: npx -y @acme/server",
            "  opencode runs: npx -y @acme/server",
        ],
        "{lines:?}"
    );

    let dry = backend.run(task(plain)).unwrap();
    assert!(dry.runs[0].stdout.contains("nothing installed"), "{dry:?}");
    assert!(!entry(".mcp.json").exists());
    let preview = backend.preview(&task(yes.clone())).unwrap();
    assert!(
        preview.lines[0].ends_with("--from load:../team --yes mcp:github"),
        "{preview:?}"
    );
    backend.run(task(yes)).unwrap();
    for file in [".mcp.json", ".codex/config.toml", "opencode.json"] {
        assert!(entry(file).is_file(), "{file}");
    }
}

/// A yes is the confirmation's alone: one sent with an install that asks
/// nothing, a skill's or the library's own server's, is refused by its
/// code before a line is built, and nothing runs.
#[test]
fn a_yes_sent_with_an_install_that_asks_nothing_is_refused_and_nothing_runs() {
    let (dir, home) = serving();
    weaver(&home, &["add", "skill", "deploy", "Ship it"]).unwrap();
    let backend = backend(&home);
    let before = files(dir.path());

    for (kind, from) in [
        ("skill", json!("workspace")),
        ("mcp", json!("workspace")),
        ("mcp", Value::Null),
    ] {
        let sent = json!({
            "type": "install", "projects": ["alpha"], "kind": kind, "name": "deploy",
            "from": from, "yes": true,
        });

        let refused = backend.preview(&task(sent.clone())).unwrap_err();

        assert_eq!(refused.code, "install_yes_unasked", "{sent}");
        assert_eq!(
            backend.run(task(sent)).unwrap_err().code,
            "install_yes_unasked"
        );
    }
    assert_eq!(files(dir.path()), before);
}

#[test]
fn a_plan_is_answered_with_the_tasks_that_carry_it_out_and_changes_nothing() {
    let (dir, home) = assigning();
    let backend = backend(&home);
    let before = files(dir.path());

    let answer = plan(
        &backend,
        json!([
            cell("beta", "skill:deploy", "load:../team", true),
            cell("alpha", "skill:deploy", "workspace", false),
            // Held from there already: as it is.
            cell("beta", "skill:review", "workspace", true),
        ]),
    )
    .unwrap();

    assert_eq!(
        answer,
        json!({
            "tasks": [
                {
                    "type": "install",
                    "projects": ["beta"],
                    "kind": "skill",
                    "name": "deploy",
                    "from": "load:../team",
                    "yes": false,
                },
                {"type": "uninstall", "projects": ["alpha"], "kind": "skill", "name": "deploy", "force": false},
            ],
            "conflicts": [],
            "locked": [],
            "asks": [],
        })
    );
    assert_eq!(files(dir.path()), before);

    // Sent back as one batch, each project named by the folder its entry
    // points at, as any task's are.
    let batch = task(json!({"type": "apply_install", "changes": answer["tasks"]}));
    assert_eq!(
        backend.preview(&batch).unwrap().lines,
        [
            format!(
                "mindweaver -W {} install -p {} --from load:../team skill:deploy",
                home.display(),
                under(&home, &["beta"])
            ),
            format!(
                "mindweaver -W {} uninstall -p {} skill:deploy",
                home.display(),
                under(&home, &["alpha"])
            ),
        ]
    );
}

#[test]
fn a_plan_names_only_projects_the_workspace_links_whatever_a_value_starts_with() {
    let (dir, home) = assigning();
    let backend = backend(&home);
    let before = files(dir.path());
    let alpha = under(&home, &["alpha"]);

    for project in [
        "--yes",
        "-C",
        "-W",
        "--home=/etc",
        "gamma",
        "../team",
        &alpha,
    ] {
        let refusal = plan(
            &backend,
            json!([
                cell("alpha", "skill:review", "workspace", true),
                cell(project, "skill:deploy", "workspace", true),
            ]),
        )
        .unwrap_err();
        assert_eq!(refusal.code, "not_registered", "{project}");
    }

    // Anywhere else a value that starts with a dash is that value, which
    // nothing offers.
    let answer = plan(
        &backend,
        json!([cell("alpha", "--yes", "--from=/etc", true)]),
    )
    .unwrap();
    assert_eq!(answer["tasks"], json!([]));
    assert_eq!(
        answer["locked"][0]["why"]["code"], "not_offered",
        "{answer}"
    );
    assert_eq!(files(dir.path()), before);

    for sent in [
        json!({"type": "plan", "selection": [], "home": "/etc"}),
        json!({"type": "plan", "selection": [
            {"project": "alpha", "reference": "skill:deploy", "origin": "workspace", "wanted": true, "force": true},
        ]}),
        json!({"type": "plan", "selection": [
            {"project": "alpha", "reference": "skill:deploy", "origin": "workspace"},
        ]}),
    ] {
        assert!(
            serde_json::from_value::<Query>(sent.clone()).is_err(),
            "{sent}"
        );
    }
}

#[test]
fn a_plan_says_what_clashes_and_what_is_locked_by_the_codes_core_gives() {
    let (_dir, home) = assigning();
    for copy in copies(&home.join("beta"), "review") {
        fs::write(copy.join("SKILL.md"), skill_file("review", "Mine now")).unwrap();
    }
    let backend = backend(&home);

    let answer = plan(
        &backend,
        json!([
            cell("alpha", "skill:deploy", "workspace", true),
            cell("alpha", "skill:deploy", "load:../team", true),
            cell("beta", "skill:review", "workspace", false),
        ]),
    )
    .unwrap();

    assert_eq!(answer["tasks"], json!([]));
    let conflicts = answer["conflicts"].as_array().unwrap();
    assert_eq!(conflicts.len(), 1, "{answer}");
    assert_eq!(conflicts[0]["why"]["code"], "wanted_twice");
    assert_eq!(conflicts[0]["cells"].as_array().unwrap().len(), 2);
    assert_eq!(
        answer["locked"],
        json!([{
            "cell": cell("beta", "skill:review", "workspace", false),
            "why": {
                "code": "edited",
                "message": "`review` in beta was changed since it was installed, so it is left \
                            alone unless a change to it is forced",
            },
        }])
    );
}

/// Put the screen's cursor on `project`'s box of `name` from `origin`, and
/// press space, as a person marking it does.
fn mark(screen: &mut Screen, project: &str, name: &str, origin: &str) {
    screen.cursor = screen
        .targets
        .iter()
        .position(|target| target.name() == project)
        .unwrap();
    let target = &mut screen.targets[screen.cursor];
    target.cursor = target
        .rows
        .iter()
        .position(|row| row.candidate.name() == name && row.candidate.origin() == origin)
        .unwrap();
    screen.focus = Focus::Offers;
    press(screen, &[KeyCode::Char(' ')]);
}

/// The panel's batch and the install screen's are one plan: the same boxes
/// marked on each, in two workspaces made alike, leave both holding the
/// same copies, recorded alike, by the same tasks.
#[test]
fn applying_a_plans_tasks_leaves_what_the_install_screen_leaves_for_the_same_marks() {
    let (_screened, screened) = assigning();
    let (_panel, panel) = assigning();

    let workspace = WeaverWorkspace::locate(&screened).unwrap().unwrap();
    let (projects, _) = install_screen::registered(&workspace);
    let mut screen = install_screen::open(&workspace, projects, &AgentHomes::default()).unwrap();
    // The team's `deploy` in the place of the workspace's, which unticks it.
    mark(&mut screen, "alpha", "deploy", "load:../team");
    mark(&mut screen, "alpha", "review", "workspace");
    mark(&mut screen, "beta", "deploy", "workspace");
    mark(&mut screen, "beta", "review", "workspace");
    let applied = install_screen::apply(&workspace, &screen, &Lookup::default());
    assert!(applied.ok, "{}", applied.stdout);

    let backend = backend(&panel);
    let answer = plan(
        &backend,
        json!([
            cell("alpha", "skill:deploy", "workspace", false),
            cell("alpha", "skill:deploy", "load:../team", true),
            cell("alpha", "skill:review", "workspace", true),
            cell("beta", "skill:deploy", "workspace", true),
            cell("beta", "skill:review", "workspace", false),
        ]),
    )
    .unwrap();
    assert_eq!(
        (&answer["conflicts"], &answer["locked"]),
        (&json!([]), &json!([]))
    );
    let ran = backend
        .run(task(
            json!({"type": "apply_install", "changes": answer["tasks"]}),
        ))
        .unwrap();
    assert!(ran.ok, "{ran:?}");

    // The same tasks, in whichever order each was marked.
    let Task::ApplyInstall { changes } = screen.task() else {
        panic!("{:?}", screen.task())
    };
    let sorted = |tasks: Vec<Value>| {
        let mut said: Vec<String> = tasks.iter().map(Value::to_string).collect();
        said.sort();
        said
    };
    assert_eq!(
        sorted(serde_json::from_value(serde_json::to_value(&changes).unwrap()).unwrap()),
        sorted(serde_json::from_value(answer["tasks"].clone()).unwrap())
    );
    let state = held(&panel);
    assert_eq!(state, held(&screened));
    let (_, records) = state;
    assert_eq!(
        records,
        [
            (
                String::from("alpha"),
                Kind::Skill,
                String::from("deploy"),
                Some(String::from("load:../team"))
            ),
            (
                String::from("alpha"),
                Kind::Skill,
                String::from("review"),
                Some(String::from("workspace"))
            ),
            (
                String::from("beta"),
                Kind::Skill,
                String::from("deploy"),
                Some(String::from("workspace"))
            ),
        ]
    );
    let team = fs::read_to_string(copies(&panel.join("alpha"), "deploy")[0].join("SKILL.md"));
    assert!(team.unwrap().contains("the team's way"));
}

/// Everything the ledger of the workspace at `home` records of what was
/// installed, and what it logged, as text a row to a line: each
/// installation and the targets it wrote, with every hash, each file it
/// made, and each action with how it went. Without the times, which no two
/// runs share.
fn recorded(home: &Path) -> Vec<String> {
    let workspace = WeaverWorkspace::open(home).unwrap();
    let ledger = rusqlite::Connection::open(workspace.ledger_path()).unwrap();
    let mut rows = Vec::new();
    for sql in [
        "SELECT project, kind, name, coalesce(source_id, ''), coalesce(origin, ''), \
                coalesce(content_hash, '') \
           FROM installations ORDER BY project, kind, name",
        "SELECT i.project || ' ' || i.kind || ' ' || i.name, coalesce(t.agent, ''), t.path, \
                coalesce(t.content_hash, ''), '', '' \
           FROM installation_targets t JOIN installations i ON i.id = t.installation_id \
          ORDER BY i.project, i.kind, i.name, t.path",
        "SELECT project, path, '', '', '', '' FROM created_files ORDER BY project, path",
        "SELECT action, coalesce(target, ''), outcome, coalesce(detail, ''), '', '' \
           FROM actions ORDER BY id",
    ] {
        let mut statement = ledger.prepare(sql).unwrap();
        let found = statement
            .query_map([], |row| {
                (0..6)
                    .map(|at| row.get::<_, rusqlite::types::Value>(at))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap();
        for row in found {
            rows.push(format!("{:?}", row.unwrap()));
        }
    }
    rows
}

/// What the panel applies is what its lines do typed into a shell: the same
/// boxes planned in two workspaces made alike, the batch applied in one and
/// its previewed lines run as `mindweaver` commands in the other, leave the same
/// copies and the same ledger, every hash and every action logged alike.
#[test]
fn applying_a_batch_leaves_the_ledger_as_running_its_lines_in_the_cli() {
    let (_panel, panel) = assigning();
    let (_typed, typed) = assigning();
    let selection = json!([
        cell("alpha", "skill:deploy", "workspace", false),
        cell("alpha", "skill:deploy", "load:../team", true),
        cell("alpha", "skill:review", "workspace", true),
        cell("beta", "skill:deploy", "workspace", true),
        cell("beta", "skill:review", "workspace", false),
    ]);

    let backend = backend(&panel);
    let answer = plan(&backend, selection.clone()).unwrap();
    let applied = backend
        .run(task(
            json!({"type": "apply_install", "changes": answer["tasks"]}),
        ))
        .unwrap();
    assert!(applied.ok, "{applied:?}");

    let other = self::backend(&typed);
    let answer = plan(&other, selection).unwrap();
    let lines = other
        .preview(&task(
            json!({"type": "apply_install", "changes": answer["tasks"]}),
        ))
        .unwrap()
        .lines;
    assert_eq!(lines.len(), applied.runs.len(), "{lines:?}");
    for line in &lines {
        // A line names a folder by its path, which a temporary one spells
        // with nothing a shell would quote.
        assert!(!line.contains(['\'', '"', '\\']), "{line}");
        let words: Vec<&str> = line.split_whitespace().collect();
        assert_eq!(words[0], "mindweaver", "{line}");
        let ran = weaver(&typed, &words[1..]).unwrap();
        assert!(ran.ok, "{line}: {ran:?}");
    }

    assert_eq!(held(&panel), held(&typed));
    let ledger = recorded(&panel);
    assert_eq!(ledger, recorded(&typed));
    assert!(
        ledger.iter().any(|row| row.contains("load:../team")),
        "{ledger:#?}"
    );
}

/// Somebody's copy is locked by the plan, and refused by the command all
/// the same when a task that would change it is sent without one: the plan
/// says it before anything runs, and the run is what keeps it.
#[test]
fn a_task_reaching_somebodys_copy_is_refused_by_its_line_and_writes_nothing() {
    let (dir, home) = assigning();
    for copy in copies(&home.join("alpha"), "review") {
        fs::create_dir_all(&copy).unwrap();
        fs::write(copy.join("SKILL.md"), skill_file("review", "Mine")).unwrap();
    }
    let backend = backend(&home);
    let answer = plan(
        &backend,
        json!([cell("alpha", "skill:review", "workspace", false)]),
    )
    .unwrap();
    assert_eq!(answer["tasks"], json!([]));
    assert_eq!(answer["locked"][0]["why"]["code"], "foreign", "{answer}");
    let before = files(dir.path());
    let ledger = recorded(&home);

    for change in [
        json!({"type": "uninstall", "projects": ["alpha"], "kind": "skill", "name": "review", "force": false}),
        json!({
            "type": "install", "projects": ["alpha"], "kind": "skill", "name": "review",
            "from": "workspace",
        }),
    ] {
        let ran = backend
            .run(task(json!({"type": "apply_install", "changes": [change]})))
            .unwrap();
        assert!(!ran.ok, "{change}: {ran:?}");
        let said: Vec<Option<&str>> = ran.runs[0].stderr.iter().map(|line| line.code).collect();
        let refused = ran.runs[0]
            .error
            .as_ref()
            .map(|error| error.code)
            .into_iter()
            .chain(said.into_iter().flatten())
            .collect::<Vec<_>>();
        assert!(refused.contains(&"foreign"), "{change}: {ran:?}");
    }
    assert_eq!(files(dir.path()), before);
    assert_eq!(recorded(&home), ledger);
}

// ---------------------------------------------------------------------------
// What an update would do, and the tasks the page of updates sends
// ---------------------------------------------------------------------------

/// Under the folder it returns: a workspace at `home` with the skills
/// `lint`, `review` and `old` of its own, linking `alpha` and `beta`, each a
/// repository given `mind init`: both received `lint`, changed in the
/// library since, `beta`'s copy of it in Claude's folder edited by hand;
/// `alpha` received `review`, its copy in `.agents/skills` deleted since;
/// and `beta` received `old`, removed from the library alone since. Its
/// second value is `home`.
fn updating() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    weaver(&home, &["init"]).unwrap();
    for (name, description) in [
        ("lint", "Tidy the code"),
        ("review", "Review a change"),
        ("old", "Done the old way"),
    ] {
        weaver(&home, &["add", "skill", name, description]).unwrap();
    }
    for (project, installs) in [("alpha", &["lint", "review"]), ("beta", &["lint", "old"])] {
        let root = home.join(project);
        fs::create_dir(&root).unwrap();
        new_project(&root);
        weaver(&home, &["link", project]).unwrap();
        for name in installs {
            let installed = weaver(&home, &["install", "-p", project, name]).unwrap();
            assert!(installed.ok, "{installed:?}");
        }
    }
    let changed = |file: &Path, line: &str| {
        let text = fs::read_to_string(file).unwrap();
        fs::write(file, format!("{text}\n{line}\n")).unwrap();
    };
    changed(
        &own_directory_for(&home, Kind::Skill).join("lint/SKILL.md"),
        "Run the formatter first.",
    );
    changed(
        &home.join("beta/.claude/skills/lint/SKILL.md"),
        "Our own way.",
    );
    fs::remove_dir_all(home.join("alpha/.agents/skills/review")).unwrap();
    let removed = weaver(&home, &["remove", "--yes", "skill:old"]).unwrap();
    assert!(removed.stdout.starts_with("removed"), "{removed:?}");
    (dir, home)
}

#[test]
fn what_an_update_would_do_is_answered_as_core_surveys_it_and_changes_nothing() {
    let (dir, home) = updating();
    let backend = backend(&home);
    let before = files(dir.path());

    let answer = backend.query(query(json!({"type": "updates"}))).unwrap();

    assert_eq!(files(dir.path()), before);
    assert_eq!(
        answer["states"],
        json!([
            {"state": "outdated", "count": 1},
            {"state": "edited", "count": 1},
            {"state": "missing", "count": 1},
            {"state": "orphaned", "count": 1},
        ])
    );
    assert_eq!(answer["writes"], true);
    let listed: Vec<(&str, &str, &str, &str)> = answer["projects"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|project| {
            project["installations"]
                .as_array()
                .unwrap()
                .iter()
                .map(move |pending| {
                    (
                        project["entry"].as_str().unwrap(),
                        pending["name"].as_str().unwrap(),
                        pending["state"].as_str().unwrap(),
                        pending["does"].as_str().unwrap(),
                    )
                })
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("alpha", "lint", "outdated", "refresh"),
            ("alpha", "review", "missing", "put_back"),
            ("beta", "lint", "edited", "leave"),
            ("beta", "old", "orphaned", "orphaned"),
        ]
    );
    let left: Vec<&Value> = answer["projects"][1]["installations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|pending| &pending["left"]["code"])
        .collect();
    assert_eq!(left, [&json!("edited"), &json!("no_longer_offered")]);
}

/// Asked of the default workspace before anything has made it: nothing to
/// update, and nothing made to say so, as a dry run of `update --all`.
#[test]
fn what_an_update_would_do_in_a_home_where_nothing_is_made_is_nothing_and_makes_nothing() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let backend = Checked(PanelBackend::launch(&home, &Lookup::at_home(Some(&home))).unwrap());

    let answer = backend.query(query(json!({"type": "updates"}))).unwrap();

    assert_eq!(answer["projects"], json!([]));
    assert_eq!(answer["writes"], false);
    assert_eq!(answer["unread"], json!([]));
    assert!(!home.join(WEAVER_DIR).exists(), "asking made the workspace");
}

#[test]
fn a_query_of_what_an_update_would_do_takes_no_field() {
    let refused = serde_json::from_value::<Query>(json!({"type": "updates", "all": true}));

    assert!(refused.is_err(), "{refused:?}");
}

/// Each change the page of updates and the page of one of the library's own
/// offer, as the browser sends it, is the line somebody would type for it,
/// each project named by the folder its entry points at; and what takes
/// something away, a forced update, an uninstall, forced or not, and a
/// removal told yes, is asked about first, while a removal's dry run is
/// not.
#[test]
fn each_change_the_pages_of_updates_and_of_the_library_send_is_the_line_the_cli_takes() {
    let (_dir, home) = updating();
    let backend = backend(&home);
    let alpha = under(&home, &["alpha"]);
    let beta = under(&home, &["beta"]);
    let project = |entry: &str| json!({"type": "project", "entry": entry});
    let named = home.display();

    for (sent, line, confirm) in [
        (
            json!({"type": "update", "holder": {"type": "workspace"}, "reference": null, "force": false}),
            format!("mindweaver -W {named} update --all --yes"),
            Vec::new(),
        ),
        (
            json!({"type": "update", "holder": project("alpha"), "reference": null, "force": false}),
            format!("mind -W {named} -C {alpha} update --yes"),
            Vec::new(),
        ),
        (
            json!({"type": "update", "holder": project("alpha"), "reference": "skill:lint", "force": false}),
            format!("mind -W {named} -C {alpha} update --yes skill:lint"),
            Vec::new(),
        ),
        (
            json!({"type": "update", "holder": project("beta"), "reference": "skill:lint", "force": true}),
            format!("mind -W {named} -C {beta} update --yes --force skill:lint"),
            vec!["forces"],
        ),
        (
            json!({"type": "uninstall", "projects": ["beta"], "kind": "skill", "name": "old", "force": false}),
            format!("mindweaver -W {named} uninstall -p {beta} skill:old"),
            vec!["uninstalls"],
        ),
        (
            json!({"type": "uninstall", "projects": ["beta"], "kind": "skill", "name": "old", "force": true}),
            format!("mindweaver -W {named} uninstall -p {beta} --force skill:old"),
            vec!["uninstalls", "forces"],
        ),
        (
            json!({"type": "uninstall_orphaned", "holder": {"type": "workspace"}}),
            format!("mindweaver -W {named} uninstall --all-projects --orphaned"),
            vec!["uninstalls"],
        ),
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "lint", "everywhere": true, "yes": false,
            }),
            format!("mindweaver -W {named} remove --everywhere skill:lint"),
            Vec::new(),
        ),
        (
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "lint", "everywhere": true, "yes": true,
            }),
            format!("mindweaver -W {named} remove --everywhere --yes skill:lint"),
            vec!["removes", "uninstalls"],
        ),
    ] {
        let preview = backend.preview(&task(sent.clone())).unwrap();

        assert_eq!(preview.lines, [line], "{sent}");
        let asked: Vec<&str> = preview
            .confirm
            .iter()
            .map(|confirm| confirm["caution"].as_str().unwrap())
            .collect();
        assert_eq!(asked, confirm, "{sent}");
    }
}

/// Updating every project from the panel leaves what `mindweaver update --all
/// --yes` leaves, in two workspaces made alike: the outdated copy and the
/// one gone written from the library, the edited one left as it was, and
/// the edit said by the code an install refuses it with.
#[test]
fn updating_every_project_from_the_panel_leaves_what_the_cli_leaves() {
    let (_typed, typed) = updating();
    let (_panel, panel) = updating();

    let cli = weaver(&typed, &["update", "--all", "--yes"]).unwrap();
    let ran = backend(&panel)
        .run(task(json!({
            "type": "update", "holder": {"type": "workspace"}, "reference": null, "force": false,
        })))
        .unwrap();

    assert!(!cli.ok);
    assert!(!ran.ok);
    let run = &ran.runs[0];
    assert_eq!(run.stdout, cli.stdout);
    assert_eq!(run.stderr, cli.stderr);
    assert!(run
        .stderr
        .iter()
        .any(|line| line.code == Some("edited") && line.text.contains("beta")));
    let state = |home: &Path| {
        files(home)
            .into_iter()
            .filter_map(|(path, bytes)| {
                let route = path.strip_prefix(home).unwrap().to_path_buf();
                let kept = !route.components().any(|part| {
                    [".git", MIND_DIR, WEAVER_DIR]
                        .contains(&part.as_os_str().to_str().unwrap_or(""))
                });
                kept.then_some((route, bytes))
            })
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(state(&panel), state(&typed));
    let edited = fs::read_to_string(panel.join("beta/.claude/skills/lint/SKILL.md")).unwrap();
    assert!(edited.contains("Our own way."), "{edited}");
    let after = backend(&panel)
        .query(query(json!({"type": "updates"})))
        .unwrap();
    assert_eq!(after["states"][0], json!({"state": "outdated", "count": 0}));
    assert_eq!(after["states"][2], json!({"state": "missing", "count": 0}));
}

/// A copy changed since it was installed that nothing offers any more, as
/// removing it from the library leaves one: core says it needs forcing,
/// an uninstall not forced leaves it and says so by the code an install
/// refuses an edit with, and the forced one the page of updates sends then
/// takes it out.
#[test]
fn an_edited_copy_nothing_offers_any_more_is_taken_out_by_the_forced_uninstall_core_asks_for() {
    let (_dir, home) = updating();
    let copy = home.join("beta/.claude/skills/old");
    let text = fs::read_to_string(copy.join("SKILL.md")).unwrap();
    fs::write(copy.join("SKILL.md"), format!("{text}\nOur own way.\n")).unwrap();
    let backend = backend(&home);

    let answer = backend.query(query(json!({"type": "updates"}))).unwrap();
    let old = &answer["projects"][1]["installations"][1];
    assert_eq!(
        (&old["name"], &old["state"], &old["does"], &old["forced"]),
        (
            &json!("old"),
            &json!("edited"),
            &json!("orphaned"),
            &json!(true)
        ),
        "{answer}"
    );

    let uninstall = |force| {
        task(json!({
            "type": "uninstall", "projects": ["beta"], "kind": "skill", "name": "old",
            "force": force,
        }))
    };
    let left = backend.run(uninstall(false)).unwrap();
    assert!(!left.ok, "{left:?}");
    assert!(
        left.runs[0]
            .stderr
            .iter()
            .any(|line| line.code == Some("edited")),
        "{left:?}"
    );
    assert!(copy.exists());
    let taken = backend.run(uninstall(true)).unwrap();
    assert!(taken.ok, "{taken:?}");
    assert!(!copy.exists());
    let after = backend.query(query(json!({"type": "updates"}))).unwrap();
    assert_eq!(after["states"][3], json!({"state": "orphaned", "count": 0}));
    assert_eq!(after["states"][1], json!({"state": "edited", "count": 1}));
}

/// A draft as the panel sends it to be judged.
fn judge(holder: Value, draft: Value) -> Asked {
    query(json!({"type": "validate", "holder": holder, "draft": draft}))
}

fn added(name: &str, description: &str) -> Value {
    json!({
        "type": "add", "kind": "skill", "name": name, "description": description,
        "template": null,
    })
}

/// A draft is judged by the command that would write it, by its code and in
/// its words, with nothing written: what `add` refuses, in the workspace's
/// own or a project's, and a draft it would write.
#[test]
fn a_draft_is_judged_as_the_command_writing_it_would_judge_it_and_nothing_is_written() {
    let dir = workspace_with_two();
    let root = dir.path();
    weaver(root, &["add", "skill", "deploy", "Ship it"]).unwrap();
    let backend = backend(root);
    let before = files(root);
    let own = json!({"type": "workspace"});
    let alpha = json!({"type": "project", "entry": "alpha"});

    let fit = backend
        .query(judge(own.clone(), added("review", "Read it")))
        .unwrap();
    let taken = backend.query(judge(own, added("deploy", "Again"))).unwrap();
    let unfit = backend
        .query(judge(alpha, added("Bad Name", "Read it")))
        .unwrap();

    assert_eq!(fit, json!({"issues": [], "refused": null}));
    assert_eq!(files(root), before);
    let said = weaver(root, &["add", "skill", "deploy", "Again"]).unwrap_err();
    assert_eq!(taken["refused"]["code"], said.error.code());
    assert_eq!(taken["refused"]["message"], said.error.to_string());
    let said = mind(
        &root.join("alpha"),
        &["add", "skill", "Bad Name", "Read it"],
    )
    .unwrap_err();
    assert_eq!(unfit["refused"]["code"], "add_name_unfit");
    assert_eq!(unfit["refused"]["message"], said.error.to_string());
}

/// How a server is started, as the panel's form sends it: a program read
/// with a secret from `variable`.
fn program(variable: &str) -> Value {
    json!({
        "type": "program", "command": "npx", "args": ["-y", "@acme/server"],
        "env": [], "secret_env": [variable], "arg_env": [],
    })
}

/// A server's add and its edit, as the panel's form sends each, are judged
/// as the line would judge it and run as the lines `add mcp` and `edit`
/// take, the form's fields their options: a value shaped like a token is
/// refused by core, in its words, before anything is written.
#[test]
fn a_servers_add_and_edit_are_judged_and_run_as_add_mcp_and_edit_told_its_options() {
    let dir = workspace_with_two();
    let root = dir.path();
    let backend = backend(root);
    let own = json!({"type": "workspace"});
    let adding = json!({
        "type": "add", "kind": "mcp", "name": "github", "description": "GitHub",
        "template": null, "server": program("GITHUB_TOKEN"),
    });
    let (_, token) = mindweaver_core::mother::McpMother::tokens().remove(0);
    let leaky = json!({
        "type": "program", "command": "npx", "args": [], "env": [["API_KEY", token]],
        "secret_env": [], "arg_env": [],
    });

    let fit = backend.query(judge(own.clone(), adding.clone())).unwrap();
    let mut sent = adding.clone();
    sent["holder"] = own.clone();
    let lines = backend.preview(&task(sent.clone())).unwrap();
    let ran = backend.run(task(sent)).unwrap();
    let unfit = backend
        .query(judge(
            own.clone(),
            json!({"type": "edit_server", "name": "github", "server": leaky}),
        ))
        .unwrap();
    let edited = backend
        .run(task(json!({
            "type": "edit_server", "holder": own, "name": "github",
            "server": program("GH_TOKEN"),
        })))
        .unwrap();

    assert_eq!(fit, json!({"issues": [], "refused": null}));
    assert_eq!(
        lines.lines,
        [format!(
            "mindweaver -W {} add --command npx --arg=-y --arg @acme/server \
             --secret-env GITHUB_TOKEN mcp github GitHub",
            root.display()
        )]
    );
    assert!(ran.ok, "{ran:?}");
    assert_eq!(unfit["refused"]["code"], "edit_contents_unfit", "{unfit}");
    assert!(!unfit.to_string().contains(&token[4..]), "{unfit}");
    assert!(edited.ok, "{edited:?}");
    let file = own_directory_for(root, Kind::Mcp).join("github.toml");
    let written = fs::read_to_string(file).unwrap();
    assert!(written.contains("env_var = \"GH_TOKEN\""), "{written}");
    assert!(written.contains("description = \"GitHub\""), "{written}");
}

/// What `validate` would say of contents an edit would write is what it says
/// once they are written, word for word, a warning said as one.
#[test]
fn contents_are_judged_in_the_words_validate_says_them_in_once_written() {
    let dir = workspace_with_two();
    let root = dir.path();
    weaver(root, &["add", "skill", "deploy", "Ship it"]).unwrap();
    let text = "---\nname: deploy\ndescription: Ship it\ncompatibility: \"\"\n---\n\n# deploy\n";

    let judged = backend(root)
        .query(judge(
            json!({"type": "workspace"}),
            json!({"type": "edit", "reference": "skill:deploy", "text": text}),
        ))
        .unwrap();

    assert_eq!(judged["refused"], Value::Null, "{judged}");
    let issues = judged["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1, "{judged}");
    let draft = root.join("draft.md");
    fs::write(&draft, text).unwrap();
    weaver(
        root,
        &[
            "edit",
            "--contents",
            draft.to_str().unwrap(),
            "skill:deploy",
        ],
    )
    .unwrap();
    let said = weaver(root, &["validate"]).unwrap().stdout;
    for issue in issues {
        let marked = if issue["warning"] == true {
            "warning: "
        } else {
            ""
        };
        let line = format!("  - {marked}{}\n", issue["message"].as_str().unwrap());
        assert!(said.contains(&line), "{line:?} in {said}");
    }
}

/// Refused by `edit --contents`, with what `validate` would say beside it,
/// and a holder the workspace does not link refused by the path policy.
#[test]
fn contents_an_edit_refuses_and_a_holder_not_linked_are_refused_with_their_codes() {
    let dir = workspace_with_two();
    let root = dir.path();
    let backend = backend(root);
    let unfit = json!({
        "type": "edit", "reference": "skill:alpha",
        "text": "---\nname: alpha\ndescription: \"\"\n---\n",
    });

    let judged = backend
        .query(judge(
            json!({"type": "project", "entry": "alpha"}),
            unfit.clone(),
        ))
        .unwrap();
    let elsewhere = backend
        .query(judge(json!({"type": "project", "entry": "../gone"}), unfit))
        .unwrap_err();

    assert_eq!(judged["refused"]["code"], "edit_contents_unfit");
    assert_eq!(judged["issues"][0]["code"], "description_empty");
    assert_eq!(elsewhere.code, "not_registered");
    let refused: Result<Query, _> = serde_json::from_value(json!({
        "type": "validate", "holder": {"type": "workspace"},
        "draft": {"type": "edit", "reference": "skill:alpha", "text": "", "path": "/etc"},
    }));
    assert!(refused.is_err(), "a field a draft does not have is refused");
}

/// A save of the library's `deploy`, `text` in place of what the hash of
/// `read` says it was read from.
fn save(text: &str, read: &str) -> Task {
    task(json!({
        "type": "save", "holder": {"type": "workspace"}, "kind": "skill", "name": "deploy",
        "text": text, "hash": mindweaver_core::hash::bytes(read.as_bytes()),
    }))
}

/// Whatever the workspace's scratch folder holds: nothing, once a run is
/// over, a folder made for it left in place.
fn scratch(root: &Path) -> Vec<PathBuf> {
    let folder = WeaverWorkspace::open(root).unwrap().scratch_dir();
    fs::read_dir(&folder)
        .map(|entries| entries.map(|entry| entry.unwrap().path()).collect())
        .unwrap_or_default()
}

/// The browser sends the text, the backend puts it where the line it
/// previewed reads it, the line writes it into the artifact, and the file
/// is taken away once it ran: the same line previewed and run.
#[test]
fn a_save_writes_the_text_through_edit_contents_and_takes_its_file_away_after() {
    let dir = workspace_with_two();
    let root = dir.path();
    weaver(root, &["add", "skill", "deploy", "Ship it"]).unwrap();
    let manifest = own_directory_for(root, Kind::Skill).join("deploy/SKILL.md");
    let read = fs::read_to_string(&manifest).unwrap();
    let text = read.replace("Ship it", "Ship it today");
    let backend = backend(root);

    let preview = backend.preview(&save(&text, &read)).unwrap();
    let ran = backend.run(save(&text, &read)).unwrap();

    assert!(ran.ok, "{ran:?}");
    assert_eq!(preview.lines, [ran.runs[0].command.clone()]);
    let route = mindweaver_cli::task::scratch_route(&text);
    assert_eq!(
        ran.runs[0].command,
        format!(
            "mindweaver -W {} edit --contents {route} --if-unchanged {} skill:deploy",
            root.display(),
            mindweaver_core::hash::bytes(read.as_bytes())
        )
    );
    assert!(
        ran.runs[0].stdout.starts_with("edited skill deploy"),
        "{ran:?}"
    );
    assert_eq!(fs::read_to_string(&manifest).unwrap(), text);
    assert_eq!(scratch(root), Vec::<PathBuf>::new());
}

/// A file changed since the text was read is not written over, and the text
/// is taken away all the same, as it is from a save the edit refuses.
#[test]
fn a_save_the_edit_refuses_writes_nothing_and_still_takes_its_file_away() {
    let dir = workspace_with_two();
    let root = dir.path();
    weaver(root, &["add", "skill", "deploy", "Ship it"]).unwrap();
    let manifest = own_directory_for(root, Kind::Skill).join("deploy/SKILL.md");
    let read = fs::read_to_string(&manifest).unwrap();
    let since = read.replace("Ship it", "Ship it now");
    fs::write(&manifest, &since).unwrap();
    let backend = backend(root);

    let stale = backend
        .run(save(&read.replace("Ship it", "Ship"), &read))
        .unwrap();
    let unfit = backend
        .run(save(&since.replace("Ship it now", "\"\""), &since))
        .unwrap();

    for (ran, code) in [(&stale, "edit_stale"), (&unfit, "edit_contents_unfit")] {
        assert!(!ran.ok, "{ran:?}");
        assert_eq!(ran.runs[0].error.as_ref().unwrap().code, code, "{ran:?}");
    }
    assert_eq!(fs::read_to_string(&manifest).unwrap(), since);
    assert_eq!(scratch(root), Vec::<PathBuf>::new());
}

/// A file an interrupted run left under the name is the same text, and is
/// taken; anything else there refuses the save before a line runs, and is
/// left as it is.
#[test]
fn a_save_takes_the_text_left_under_its_name_and_refuses_anything_else_there() {
    let dir = workspace_with_two();
    let root = dir.path();
    weaver(root, &["add", "skill", "deploy", "Ship it"]).unwrap();
    let manifest = own_directory_for(root, Kind::Skill).join("deploy/SKILL.md");
    let read = fs::read_to_string(&manifest).unwrap();
    let text = read.replace("Ship it", "Ship it today");
    let left = root.join(mindweaver_cli::task::scratch_route(&text));
    fs::create_dir_all(left.parent().unwrap()).unwrap();
    fs::write(&left, &text).unwrap();
    let backend = backend(root);

    let taken = backend.run(save(&text, &read)).unwrap();
    fs::write(&left, "somebody's").unwrap();
    let refused = backend.run(save(&text, &text)).unwrap_err();

    assert!(taken.ok, "{taken:?}");
    assert_eq!(fs::read_to_string(&manifest).unwrap(), text);
    assert_eq!(refused.code, "scratch_unwritable");
    assert_eq!(fs::read_to_string(&left).unwrap(), "somebody's");
}

/// A cloned workspace can bring its scratch folder as a link, to an agent's
/// own folder: the text is never written through it, and no line runs.
#[cfg(unix)]
#[test]
fn a_save_whose_scratch_folder_is_a_link_is_refused_and_writes_nothing_where_it_leads() {
    let dir = workspace_with_two();
    let root = dir.path();
    weaver(root, &["add", "skill", "deploy", "Ship it"]).unwrap();
    let manifest = own_directory_for(root, Kind::Skill).join("deploy/SKILL.md");
    let read = fs::read_to_string(&manifest).unwrap();
    let elsewhere = TempDir::new().unwrap();
    let commands = elsewhere.path().join("commands");
    fs::create_dir(&commands).unwrap();
    let folder = WeaverWorkspace::open(root).unwrap().scratch_dir();
    std::os::unix::fs::symlink(&commands, &folder).unwrap();
    let backend = backend(root);

    let refused = backend
        .run(save(&read.replace("Ship it", "Ship it today"), &read))
        .unwrap_err();

    assert_eq!(refused.code, "scratch_unwritable");
    assert!(refused.message.contains("is a link"), "{}", refused.message);
    assert_eq!(fs::read_dir(&commands).unwrap().count(), 0);
    assert_eq!(fs::read_to_string(&manifest).unwrap(), read);
}

// ---------------------------------------------------------------------------
// Switching to another workspace
// ---------------------------------------------------------------------------

/// What a browser sends to switch, read as the server reads it.
fn switch(sent: Value) -> Switch {
    serde_json::from_value(sent.clone()).unwrap_or_else(|error| panic!("{sent}: {error}"))
}

/// The backend over `root`, launched there, at `home`, with `root` found
/// above where it starts, as `mindweaver ui` run in it finds it.
fn launched_knowing(home: &Path, root: &Path) -> Checked {
    Checked(PanelBackend::launch(root, &Lookup::at_home(Some(home))).unwrap())
}

/// The path, name, and whether in use, launched on and offered, of each
/// workspace the backend may switch to.
fn offered(backend: &Checked) -> Vec<(String, bool, bool, bool)> {
    let answer = backend.query(query(json!({"type": "workspaces"}))).unwrap();
    answer
        .as_array()
        .unwrap_or_else(|| panic!("{answer}"))
        .iter()
        .map(|row| {
            (
                row["name"].as_str().unwrap_or("").to_owned(),
                row["in_use"] == true,
                row["launched"] == true,
                row["switchable"] == true,
            )
        })
        .collect()
}

/// The workspace a snapshot is of, and the rule it says chose it.
fn snapshot_of(backend: &Checked) -> (Value, Value) {
    let snapshot = backend.query(query(json!({"type": "snapshot"}))).unwrap();
    (
        snapshot["workspace"]["path"].clone(),
        snapshot["workspace"]["reason"].clone(),
    )
}

#[test]
fn what_may_be_switched_to_is_every_known_workspace_and_the_launch() {
    let (_dir, home, root) = knowing();
    let backend = launched_knowing(&home, &root);

    assert_eq!(
        offered(&backend),
        [
            (String::from("home"), false, false, true),
            (String::from("Projects"), true, true, false),
            (String::from("team"), false, false, true),
        ]
    );
}

#[test]
fn a_query_of_what_may_be_switched_to_takes_no_field() {
    let refused = serde_json::from_value::<Query>(json!({"type": "workspaces", "path": "/"}));

    assert!(refused.is_err(), "{refused:?}");
}

/// Switched to a workspace this machine knows, every read is of it, every
/// line names it, every run lands in it, what is watched is its own, and
/// the snapshot says it was named, as `-W` names it; switched back to the
/// launch, the rule that chose it at launch.
#[test]
fn a_switch_to_a_known_workspace_is_what_every_query_preview_and_run_acts_on() {
    let (dir, home, root) = knowing();
    let team = dir.path().join("team");
    let backend = launched_knowing(&home, &root);
    let kept = backend.clone();
    let path = |root: &Path| json!(root.to_str().unwrap());

    let answer = backend
        .switch(switch(json!({"path": team.to_str().unwrap()})))
        .unwrap();

    assert_eq!(answer["path"], path(&team));
    assert_eq!(answer["in_use"], true);
    assert_eq!(answer["reason"], "explicit");
    assert_eq!(snapshot_of(&backend), (path(&team), json!("explicit")));
    assert_eq!(kept.root(), team, "a clone acts on another");
    let validate = json!({"type": "validate", "holder": {"type": "workspace"}});
    assert_eq!(
        backend.preview(&task(validate.clone())).unwrap().lines,
        [format!("mindweaver -W {} validate", team.display())]
    );
    let add = json!({
        "type": "add", "holder": {"type": "workspace"}, "kind": "skill",
        "name": "minutes", "description": "Take the minutes", "template": null,
    });
    let ran = backend.run(task(add)).unwrap();
    assert!(ran.ok, "{ran:?}");
    assert!(
        team.join(".mindweaver/library/skills/minutes").is_dir(),
        "{ran:?}"
    );
    assert!(!root.join(".mindweaver/library/skills/minutes").exists());
    let watched = backend.watched_paths();
    assert!(watched.iter().any(|each| each.path.starts_with(&team)));
    assert!(
        !watched.iter().any(|each| each.path.starts_with(&root)),
        "{watched:?}"
    );

    backend
        .switch(switch(json!({"path": root.to_str().unwrap()})))
        .unwrap();
    assert_eq!(snapshot_of(&backend), (path(&root), json!("above")));
    assert_eq!(
        backend.preview(&task(validate)).unwrap().lines,
        [format!("mindweaver -W {} validate", root.display())]
    );
}

/// The server's test the issue names: a path no list holds, however it is
/// spelled, a value starting with a dash among them, is refused with a
/// code, and nothing is switched; the next read and the next line are of
/// the workspace before, and the disk is as it was.
#[test]
fn a_switch_to_a_path_outside_the_list_is_refused_and_nothing_is_switched() {
    let (dir, home, root) = knowing();
    let stranger = dir.path().join("stranger");
    fs::create_dir_all(&stranger).unwrap();
    weaver(&stranger, &["init"]).unwrap();
    let team = dir.path().join("team");
    let backend = launched_knowing(&home, &root);
    let before = files(dir.path());
    let team = team.to_str().unwrap();

    for sent in [
        // A workspace, but one nothing remembers.
        stranger.to_str().unwrap().to_owned(),
        // A listed one, spelled otherwise.
        format!("{team}/"),
        format!("{team}/.mindweaver"),
        under(dir.path(), &["home", "..", "team"]),
        String::from("team"),
        String::from("../team"),
        // Values that start with a dash.
        String::from("--yes"),
        String::from("-W"),
        String::from("--workspace=/"),
        String::from("-C/"),
        // Nothing at all, and somewhere else entirely.
        String::new(),
        String::from("/"),
        String::from("file:///etc"),
    ] {
        let refused = backend.switch(switch(json!({"path": sent})));

        assert_eq!(
            refused.as_ref().map_err(|refusal| refusal.code),
            Err("switch_unlisted"),
            "{sent:?}: {refused:?}"
        );
    }
    assert_eq!(
        snapshot_of(&backend),
        (json!(root.to_str().unwrap()), json!("above"))
    );
    assert_eq!(
        backend
            .preview(&task(json!({"type": "shelf"})))
            .unwrap()
            .lines,
        [format!("mindweaver -W {} gather list", root.display())]
    );
    assert_eq!(files(dir.path()), before);
}

#[test]
fn a_switch_carrying_anything_but_a_path_is_refused() {
    for sent in [
        json!({"path": "/", "workspace": "/"}),
        json!({"path": "/", "home": "/"}),
        json!({"type": "load", "path": "/"}),
        json!({}),
        json!({"path": ["/"]}),
    ] {
        assert!(
            serde_json::from_value::<Switch>(sent.clone()).is_err(),
            "{sent}"
        );
    }
}

/// A workspace the list names that will not open, gone since, is refused
/// with why it will not, never made, and nothing is switched.
#[test]
fn a_known_workspace_that_does_not_open_is_refused_and_never_made() {
    let (dir, home, root) = knowing();
    let team = dir.path().join("team");
    let backend = launched_knowing(&home, &root);
    fs::remove_dir_all(team.join(WEAVER_DIR)).unwrap();

    assert_eq!(
        offered(&backend)[2],
        (String::new(), false, false, false),
        "listed, and offered nothing"
    );
    let refused = backend.switch(switch(json!({"path": team.to_str().unwrap()})));

    assert_eq!(
        refused.as_ref().map_err(|refusal| refusal.code),
        Err("not_a_workspace"),
        "{refused:?}"
    );
    assert!(!team.join(WEAVER_DIR).exists(), "it was made");
    assert_eq!(
        snapshot_of(&backend),
        (json!(root.to_str().unwrap()), json!("above"))
    );
}

/// A workspace whose root no word of a command line can hold, not UTF-8,
/// is acted on as it was before every line named it: its lines name no
/// workspace, run against it all the same, and nothing switches to it.
#[cfg(unix)]
#[test]
fn a_workspace_whose_root_is_not_text_runs_its_lines_naming_none() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new().unwrap();
    let root = dir.path().join(OsStr::from_bytes(b"caf\xe9"));
    fs::create_dir(&root).unwrap();
    WeaverWorkspace::init(&root).unwrap();
    let backend = backend(&root);
    let add = add_to(json!({"type": "workspace"}));

    assert_eq!(
        backend
            .preview(&task(add.clone()))
            .map(|preview| preview.lines),
        Ok(vec![String::from("mindweaver add skill deploy 'Ship it'")])
    );
    let outcome = backend.run(task(add)).unwrap();

    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(
        outcome.runs[0].command,
        "mindweaver add skill deploy 'Ship it'"
    );
    assert!(root
        .join(".mindweaver/library/skills/deploy/SKILL.md")
        .is_file());
    let offered: Vec<_> = offered(&backend)
        .into_iter()
        .map(|(_, in_use, launched, switchable)| (in_use, launched, switchable))
        .collect();
    assert_eq!(offered, [(true, true, false)]);
}

/// Switched to a workspace that has gone since, the panel can still read
/// where it may go, the one in use among it with why it does not open, and
/// go back.
#[test]
fn a_workspace_gone_since_it_was_switched_to_can_still_be_left() {
    let (dir, home, root) = knowing();
    let team = dir.path().join("team");
    let backend = launched_knowing(&home, &root);
    backend
        .switch(switch(json!({"path": team.to_str().unwrap()})))
        .unwrap();
    fs::remove_dir_all(team.join(WEAVER_DIR)).unwrap();

    let snapshot = backend.query(query(json!({"type": "snapshot"})));
    assert_eq!(
        snapshot.as_ref().map_err(|refusal| refusal.code),
        Err("not_a_workspace")
    );
    let listed = backend.query(query(json!({"type": "workspaces"}))).unwrap();
    let in_use: Vec<&Value> = listed
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["in_use"] == true)
        .collect();
    assert_eq!(in_use.len(), 1, "{listed}");
    assert_eq!(in_use[0]["path"], json!(team.to_str().unwrap()));
    assert!(in_use[0]["stale"].is_string(), "{listed}");
    assert!(offered(&backend)
        .iter()
        .any(|row| row.0 == "Projects" && row.3));
    backend
        .switch(switch(json!({"path": root.to_str().unwrap()})))
        .unwrap();

    assert_eq!(
        snapshot_of(&backend),
        (json!(root.to_str().unwrap()), json!("above"))
    );
}

// ---------------------------------------------------------------------------
// The log of how long each line took
// ---------------------------------------------------------------------------

/// The backend over the workspace at `root`, as [`backend`] launches it,
/// saying how long each line took to what it hands back.
fn logging(root: &Path) -> (Checked, Arc<Mutex<Vec<String>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    let log: mindweaver_cli::backend::Log =
        Arc::new(move |line: &str| sink.lock().unwrap().push(line.to_owned()));
    let backend = PanelBackend::launch(root, &Lookup::default())
        .unwrap()
        .logging(log);
    (Checked(backend), lines)
}

/// A logged line with each of its times, `3ms`, written `Nms`, and those
/// times in the order it says them.
fn times(line: &str) -> (String, Vec<std::time::Duration>) {
    let mut times = Vec::new();
    let said: Vec<String> = line
        .split(' ')
        .map(|word| {
            let (bare, after) = word
                .strip_suffix(',')
                .map_or((word, ""), |bare| (bare, ","));
            match bare
                .strip_suffix("ms")
                .and_then(|millis| millis.parse().ok())
            {
                Some(millis) => {
                    times.push(std::time::Duration::from_millis(millis));
                    format!("Nms{after}")
                }
                None => word.to_owned(),
            }
        })
        .collect();
    (said.join(" "), times)
}

/// Each line of a task, once it has run, by its place among them rather
/// than its words, which are the request's: how long it took, and how much
/// of that it waited for a workspace's lock, none here. Nothing is said of
/// a read, which runs no line.
#[test]
fn each_line_a_task_runs_is_logged_with_how_long_it_took() {
    let dir = workspace_with_two_unlinked();
    let (backend, lines) = logging(dir.path());
    let gamma = dir.path().join("gamma");
    fs::remove_dir_all(gamma.join(".mind")).unwrap();

    backend.query(query(json!({"type": "snapshot"}))).unwrap();
    let outcome = backend
        .run(task(
            json!({"type": "init_and_link", "path": gamma.to_str().unwrap()}),
        ))
        .unwrap();

    assert!(outcome.ok, "{outcome:?}");
    let said: Vec<(String, Vec<std::time::Duration>)> = lines
        .lock()
        .unwrap()
        .iter()
        .map(|line| times(line))
        .collect();
    let words: Vec<&str> = said.iter().map(|(words, _)| words.as_str()).collect();
    assert_eq!(
        words,
        [
            "run: line 1 of 2 took Nms, Nms of it waiting for a workspace's lock",
            "run: line 2 of 2 took Nms, Nms of it waiting for a workspace's lock",
        ]
    );
    for (_, times) in &said {
        assert_eq!(times[1], std::time::Duration::ZERO, "{said:?}");
    }
}

/// A line kept out of the workspace by another writer holding its lock says
/// how long it waited for it, and which process held it: this one, whose
/// other thread stands in for another writer, as a lock held on another
/// handle keeps it out as another process would.
#[test]
fn a_line_kept_waiting_for_the_workspace_s_lock_says_how_long_and_by_whom() {
    let dir = workspace_with_two_unlinked();
    let (backend, lines) = logging(dir.path());
    let gamma = dir.path().join("gamma");
    let held = WeaverWorkspace::open(dir.path()).unwrap().lock().unwrap();
    let hold = std::time::Duration::from_millis(200);
    let letting_go = std::thread::spawn(move || {
        std::thread::sleep(hold);
        drop(held);
    });

    let outcome = backend
        .run(task(
            json!({"type": "link", "path": gamma.to_str().unwrap()}),
        ))
        .unwrap();
    letting_go.join().unwrap();

    assert!(outcome.ok, "{outcome:?}");
    let lines = lines.lock().unwrap();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let (words, times) = times(&lines[0]);
    assert_eq!(
        words,
        format!(
            "run: line 1 of 1 took Nms, Nms of it waiting for a workspace's lock, which \
             process {} held",
            std::process::id()
        )
    );
    assert!(times[1] > std::time::Duration::ZERO, "{lines:?}");
    assert!(times[1] <= times[0], "{lines:?}");
}

// ---------------------------------------------------------------------------
// A gather, followed and stopped (#112)
// ---------------------------------------------------------------------------

/// A repository on this disk offering two skills, and its `file://` URL.
fn source() -> (TempDir, String) {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it")),
        ("skills/review/SKILL.md", &skill_file("review", "Read it")),
    ]);
    let url = format!("file://{}", source.path().display());
    (source, url)
}

fn gather(url: &str) -> Task {
    task(json!({"type": "gather", "url": url, "folder": ".", "reference": null}))
}

/// A run numbered 1, whose every step is kept, and asked to stop on the
/// step `stop_at` says, as a cancel the server reaches it with would.
fn followed(stop_at: fn(&Value) -> bool) -> (Underway, Arc<Mutex<Vec<Value>>>) {
    let steps = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&steps);
    let itself: Arc<std::sync::OnceLock<Underway>> = Arc::default();
    let stopping = Arc::clone(&itself);
    let underway = Underway::new(1, move |step| {
        let stop = stop_at(&step);
        kept.lock().unwrap().push(step);
        if let Some(underway) = stop.then(|| stopping.get()).flatten() {
            underway.cancel();
        }
    });
    itself.set(underway.clone()).unwrap();
    (underway, steps)
}

/// The stage of each step, each once, in the order they came.
fn stages(steps: &[Value]) -> Vec<String> {
    let mut stages: Vec<String> = steps
        .iter()
        .map(|step| step["stage"].as_str().unwrap().to_owned())
        .collect();
    stages.dedup();
    stages
}

/// What `mindweaver gather list` says of the workspace at `root`.
fn shelf(root: &Path) -> String {
    weaver(root, &["gather", "list", "--json"]).unwrap().stdout
}

#[cfg(unix)]
#[test]
fn a_gather_takes_a_cancel_and_reports_each_step_it_takes_as_it_takes_it() {
    let (_source, url) = source();
    let dir = workspace_with_two();
    let root = dir.path();
    let backend = backend(root);
    let (underway, steps) = followed(|_| false);

    let preview = backend.preview(&gather(&url)).unwrap();
    let outcome = backend.followed(gather(&url), &underway).unwrap();

    assert_eq!(
        preview.lines,
        [format!("mindweaver -W {} gather git {url}", root.display())]
    );
    assert!(preview.cancellable);
    assert!(outcome.ok, "{outcome:?}");
    let steps = steps.lock().unwrap();
    let stages = stages(&steps);
    // What gix counts of a clone from this disk is its own: whichever of
    // its steps it says, they come between connecting and the harvest.
    assert_eq!(stages.first().map(String::as_str), Some("connecting"));
    assert_eq!(
        stages[stages.len() - 2..],
        [String::from("harvesting"), String::from("placing")],
        "{steps:?}"
    );
    assert!(
        stages[1..stages.len() - 2]
            .iter()
            .all(|stage| ["receiving", "resolving", "checking_out"].contains(&stage.as_str())),
        "{steps:?}"
    );
    assert_eq!(
        steps.last(),
        Some(&json!({"stage": "placing", "done": 2, "total": 2,
                     "said": "placing on the shelf 2/2", "cancellable": false}))
    );
    assert!(steps
        .iter()
        .all(|step| step["cancellable"] == (step["stage"] != "placing")));
    for step in steps.iter() {
        contract::contract(|contract| contract.holds("/components/schemas/Step", step))
            .unwrap_or_else(|error| panic!("{step}: {error}"));
    }
    let listed = shelf(root);
    assert!(
        listed.contains("\"deploy\"") && listed.contains("\"review\""),
        "{listed}"
    );
}

/// The acceptance of #112: a cancel the server hands the run, as soon as
/// the gather says it is connecting, stops it before it places anything,
/// and the shelf and the sources are as they were.
#[cfg(unix)]
#[test]
fn a_cancel_stops_a_gather_and_leaves_the_shelf_as_it_was() {
    let (_source, url) = source();
    let dir = workspace_with_two();
    let root = dir.path();
    let backend = backend(root);
    let shelved = WeaverWorkspace::open(root)
        .unwrap()
        .gathered_dir(Kind::Skill);
    let before = shelf(root);
    let (underway, steps) = followed(|step| step["stage"] == "connecting");

    let outcome = backend.followed(gather(&url), &underway).unwrap();

    assert!(!outcome.ok, "{outcome:?}");
    assert_eq!(
        outcome.runs[0].error.as_ref().map(|error| error.code),
        Some("gather_cancelled"),
        "{outcome:?}"
    );
    assert!(underway.is_cancelled());
    let steps = steps.lock().unwrap();
    assert!(
        steps.iter().all(|step| step["stage"] != "placing"),
        "{steps:?}"
    );
    assert_eq!(shelf(root), before);
    assert!(!shelved.exists() || fs::read_dir(&shelved).unwrap().next().is_none());
}

/// A cancel that comes before the run says what one does, sent as soon as
/// the server told the run started, is not lost: the gather stops at its
/// first look, before it registers its source.
#[cfg(unix)]
#[test]
fn a_cancel_before_the_gather_starts_stops_it_before_anything() {
    let (_source, url) = source();
    let dir = workspace_with_two();
    let root = dir.path();
    let backend = backend(root);
    let before = shelf(root);
    let (underway, steps) = followed(|_| false);
    underway.cancel();

    let outcome = backend.followed(gather(&url), &underway).unwrap();

    assert_eq!(
        outcome.runs[0].error.as_ref().map(|error| error.code),
        Some("gather_cancelled"),
        "{outcome:?}"
    );
    assert!(steps.lock().unwrap().is_empty());
    assert_eq!(shelf(root), before);
}

/// Every task but a gather runs with nothing to report and nothing a
/// cancel stops, and its preview says it takes none.
#[test]
fn a_task_that_is_no_gather_takes_no_cancel_and_reports_nothing() {
    let dir = workspace_with_two();
    let root = dir.path();
    let backend = backend(root);
    let gamma = root.join("gamma");
    fs::create_dir(&gamma).unwrap();
    new_project(&gamma);
    let link = task(json!({"type": "link", "path": gamma.to_str().unwrap()}));
    let (underway, steps) = followed(|_| true);

    assert!(!backend.preview(&link).unwrap().cancellable);
    assert!(backend.followed(link, &underway).unwrap().ok);
    assert!(steps.lock().unwrap().is_empty());
    assert!(!underway.is_cancelled());
}
