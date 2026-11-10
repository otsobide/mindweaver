//! `mindweaver ui` end to end: the binary started as a person starts it, the
//! link read from what it prints, and the panel's requests sent to it over
//! loopback as a script sends them, with the token as Bearer. No browser is
//! opened: every launch passes --no-open.

// The panel is the `ui` feature's, which a build without it lacks.
#![cfg(feature = "ui")]

mod common;

use std::io::{BufRead, BufReader};
use std::net::{Ipv4Addr, TcpListener};
use std::time::Duration;

use common::contract;
use common::panel::*;
use common::*;
use mindweaver_cli::backend::PanelBackend;
use mindweaver_cli::{Command, UiOptions, WeaverCommand};
use mindweaver_server::Backend;
use serde::Deserialize;
use serde_json::{json, Value};

/// How `name` stands in the project the snapshot lists as `entry`, if it
/// holds it.
fn standing<'a>(snapshot: &'a Value, entry: &str, name: &str) -> Option<&'a str> {
    let project = snapshot["projects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|project| project["entry"] == entry)
        .unwrap_or_else(|| panic!("no project {entry}: {snapshot}"));
    project["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|held| held["name"] == name)
        .map(|held| held["state"].as_str().unwrap_or("none"))
}

#[test]
fn the_panel_runs_what_it_is_sent_and_the_next_snapshot_shows_it() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());

    let before = panel.snapshot();
    assert_eq!(standing(&before, "alpha", "commit-style"), None);

    let install = json!({
        "type": "install",
        "projects": ["alpha"],
        "kind": "skill",
        "name": "commit-style",
        "from": null,
    });
    let line = format!(
        "mindweaver -W {} install -p {} skill:commit-style",
        dir.path().display(),
        dir.path().join("alpha").display()
    );
    let (status, preview) = panel.post("/api/v1/tasks/preview", install.clone());
    assert_eq!(status, 200, "{preview}");
    assert_eq!(
        preview,
        json!({"lines": [line], "confirm": [], "cancellable": false})
    );
    assert_eq!(standing(&panel.snapshot(), "alpha", "commit-style"), None);

    let (status, ran) = panel.post("/api/v1/tasks", install);
    assert_eq!(status, 200, "{ran}");
    assert_eq!(ran["outcome"]["ok"], true, "{ran}");
    assert_eq!(ran["lines"], json!([line]));

    let after = panel.snapshot();
    assert_eq!(standing(&after, "alpha", "commit-style"), Some("current"));
    assert_eq!(standing(&after, "beta", "commit-style"), None);
    let (_, transcript) = panel.get("/api/v1/transcript");
    assert_eq!(transcript, json!([ran]));
}

/// Every read the API serves, over loopback with the token as Bearer, on
/// a workspace of two projects and a skill of its own: each answered, the
/// snapshot's parts as the snapshot holds them, and nothing changed by any
/// of them, the POSTs among them.
#[test]
fn every_read_route_answers_over_loopback_with_a_bearer_token() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());
    let root = dir.path().to_str().unwrap();
    let snapshot = panel.snapshot();
    assert_eq!(snapshot["workspace"]["path"], root);

    let cell = json!({
        "project": "alpha", "reference": "skill:commit-style", "origin": "workspace",
        "wanted": true,
    });
    let draft = json!({
        "type": "add", "kind": "skill", "name": "review", "description": "Read it",
        "template": null,
    });
    let asked: Vec<(&str, String, Option<Value>)> = vec![
        ("/snapshot", String::new(), None),
        ("/projects", String::new(), None),
        ("/project", String::from("?entry=beta"), None),
        ("/library", String::new(), None),
        (
            "/files",
            String::from("?reference=skill%3Acommit-style"),
            None,
        ),
        (
            "/contents",
            String::from("?reference=skill%3Acommit-style&file=SKILL.md"),
            None,
        ),
        ("/scan", format!("?root={}&depth=1", encoded(root)), None),
        ("/folders", format!("?path={}", encoded(root)), None),
        ("/updates", String::new(), None),
        ("/workspaces", String::new(), None),
        ("/plan", String::new(), Some(json!({"selection": [cell]}))),
        (
            "/validate",
            String::new(),
            Some(json!({"holder": {"type": "workspace"}, "draft": draft})),
        ),
    ];
    let served: Vec<&str> = PanelBackend::routes()
        .iter()
        .map(|route| route.path)
        .collect();
    let named: Vec<&str> = asked.iter().map(|(path, _, _)| *path).collect();
    assert_eq!(
        named, served,
        "every route the backend serves is asked here"
    );

    let mut answers = std::collections::BTreeMap::new();
    for (path, query, body) in asked {
        let method = if body.is_some() { "POST" } else { "GET" };
        let (status, answer) =
            panel.send(method, &format!("/api/v1{path}{query}"), Auth::Bearer, body);
        assert_eq!(status, 200, "{method} {path}{query}: {answer}");
        answers.insert(path, answer);
    }

    assert_eq!(answers["/snapshot"], snapshot);
    assert_eq!(answers["/projects"], snapshot["projects"]);
    assert_eq!(answers["/library"], snapshot["library"]);
    assert_eq!(answers["/project"], snapshot["projects"][1]);
    assert_eq!(answers["/files"]["manifest"], "SKILL.md");
    assert!(
        answers["/contents"]["text"]
            .as_str()
            .is_some_and(|text| text.contains("name: commit-style")),
        "{}",
        answers["/contents"]
    );
    assert_eq!(answers["/scan"]["roots"], json!([root]));
    let folders: Vec<&Value> = answers["/folders"]["folders"]
        .as_array()
        .unwrap()
        .iter()
        .map(|folder| &folder["name"])
        .collect();
    assert_eq!(folders, [&json!("alpha"), &json!("beta")]);
    assert!(answers["/updates"].is_object(), "{}", answers["/updates"]);
    assert_eq!(answers["/workspaces"][0]["in_use"], true);
    assert_eq!(answers["/plan"]["tasks"][0]["type"], "install");
    assert!(answers["/validate"].is_object(), "{}", answers["/validate"]);
    assert_eq!(panel.snapshot(), snapshot, "no read changed anything");
    let (_, transcript) = panel.get("/api/v1/transcript");
    assert_eq!(transcript, json!([]));
}

/// Over loopback, as the panel's header sends it: a switch to a path no
/// list holds is refused with its code and leaves the panel where it was;
/// one to a workspace the home's default knows is taken, and the next
/// snapshot, and the next line, are of it.
#[test]
fn the_panel_switches_only_to_a_workspace_this_machine_knows() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let team = home.path().join("team");
    fs::create_dir_all(&team).unwrap();
    weaver_at_home(home.path(), home.path(), &["init"]).unwrap();
    weaver_at_home(&team, home.path(), &["init"]).unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());

    for path in ["/etc", "--yes", "../team"] {
        let (status, refused) = panel.post("/api/v1/switch", json!({ "path": path }));
        assert_eq!(status, 422, "{path}: {refused}");
        assert_eq!(refused["code"], "switch_unlisted", "{path}: {refused}");
    }
    assert_eq!(
        panel.snapshot()["workspace"]["path"],
        json!(dir.path().to_str().unwrap())
    );

    let (status, switched) =
        panel.post("/api/v1/switch", json!({ "path": team.to_str().unwrap() }));
    assert_eq!(status, 200, "{switched}");
    assert_eq!(switched["in_use"], true, "{switched}");
    let snapshot = panel.snapshot();
    assert_eq!(snapshot["workspace"]["path"], json!(team.to_str().unwrap()));
    assert_eq!(snapshot["workspace"]["reason"], "explicit");
    let (status, preview) = panel.post("/api/v1/tasks/preview", json!({"type": "shelf"}));
    assert_eq!(status, 200, "{preview}");
    assert_eq!(
        preview["lines"],
        json!([format!("mindweaver -W {} gather list", team.display())])
    );
}

/// A save as the panel's editor sends it, over loopback: the text it read
/// and the hash it was read with, from the query of its file; the file on
/// disk holding the text once the run answered, and nothing left in the
/// workspace's scratch folder, where the text was put for the line to read.
#[test]
fn a_save_writes_the_text_and_leaves_nothing_in_the_scratch_folder() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());
    let workspace = json!({"type": "workspace"});
    let (status, read) = panel.get("/api/v1/contents?reference=skill%3Acommit-style&file=SKILL.md");
    assert_eq!(status, 200, "{read}");
    let text = read["text"]
        .as_str()
        .unwrap()
        .replace("writes commits", "writes its commits");

    let (status, ran) = panel.post(
        "/api/v1/tasks",
        json!({
            "type": "save", "holder": workspace, "kind": "skill", "name": "commit-style",
            "text": text, "hash": read["hash"],
        }),
    );

    assert_eq!(status, 200, "{ran}");
    assert_eq!(ran["outcome"]["ok"], true, "{ran}");
    let manifest = own_directory_for(dir.path(), Kind::Skill).join("commit-style/SKILL.md");
    assert_eq!(fs::read_to_string(manifest).unwrap(), text);
    let scratch = WeaverWorkspace::open(dir.path()).unwrap().scratch_dir();
    assert_eq!(fs::read_dir(scratch).unwrap().count(), 0);
}

/// What the stream tells over loopback, from the `resync` it starts with
/// to a run's start and finish and the change it made on disk, each held
/// to what the document says an event holds: a change's scopes the
/// holders a query names, which only the CLI's backend fills.
#[test]
fn every_event_the_stream_tells_holds_to_the_document() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());
    let install = json!({
        "type": "install", "projects": ["alpha"], "kind": "skill", "name": "commit-style",
        "from": null,
    });
    // Waiting for the change by the project's holder, not for any change:
    // the ledger's is the workspace's, which a debounce may tell apart.
    let alpha = json!({"type": "project", "entry": "alpha"});
    let reaches_alpha = |event: &Value| {
        event["type"] == "changed"
            && event["scopes"]
                .as_array()
                .is_some_and(|scopes| scopes.contains(&alpha))
    };

    let told = events(
        &panel,
        || {
            let (status, ran) = panel.post("/api/v1/tasks", install);
            assert_eq!(status, 200, "{ran}");
        },
        reaches_alpha,
    );

    let kinds: Vec<&str> = told
        .iter()
        .map(|event| event["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.first(), Some(&"resync"), "{told:?}");
    for kind in ["command_started", "command_finished"] {
        assert!(kinds.contains(&kind), "{kind}: {told:?}");
    }
    assert!(told.iter().any(reaches_alpha), "{told:?}");
    for event in &told {
        contract::told("/api/v1/events", event);
    }
}

/// The description of the API the panel serves is the document committed,
/// byte for byte, as JSON, to whoever holds the token; and nothing to
/// whoever does not.
#[test]
fn the_api_s_document_is_the_one_committed_and_only_with_the_token() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());
    let committed = include_str!("../../../docs/api/openapi.json");

    let (status, head, document) =
        panel.exchange("GET", "/api/v1/openapi.json", Auth::Bearer, None);
    assert_eq!(status, 200, "{document}");
    assert!(
        head.to_ascii_lowercase()
            .contains("\r\ncontent-type: application/json\r\n"),
        "{head}"
    );
    assert_eq!(document, committed);

    let (status, refusal) = panel.send("GET", "/api/v1/openapi.json", Auth::Nothing, None);
    assert_eq!(status, 401, "{refusal}");
    assert_eq!(refusal["code"], "token_required");
    let (status, _) = panel.send(
        "GET",
        "/api/v1/openapi.json",
        Auth::BearerOf("not-the-token"),
        None,
    );
    assert_eq!(status, 401);
}

#[test]
fn a_request_without_the_token_is_refused_with_a_401() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());

    for (method, path, body) in [
        ("GET", "/api/v1/snapshot", None),
        ("POST", "/api/v1/plan", Some(json!({"selection": []}))),
        ("POST", "/api/v1/tasks", Some(json!({"type": "shelf"}))),
    ] {
        let (status, refusal) = panel.send(method, path, Auth::Nothing, body);
        assert_eq!(status, 401, "{path}");
        assert_eq!(
            refusal,
            json!({
                "error": "this needs the launch token, as the cookie its link sets or as a Bearer token",
                "code": "token_required",
            })
        );
    }
    let (status, _) = panel.send("GET", "/api/v1/transcript", Auth::Nothing, None);
    assert_eq!(status, 401);
}

/// A read naming what is not there is a 404 with core's code for it, an
/// entry no registry holds or a folder that does not exist; one naming a
/// path the policy refuses, which is there or not, is a 422 with its own.
#[test]
fn a_read_naming_what_is_not_there_is_a_404_and_a_refused_path_a_422() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());
    let gone = dir.path().join("gone");

    for (asked, status, code) in [
        (
            "/api/v1/project?entry=ghost".to_owned(),
            404,
            "not_registered",
        ),
        (
            format!("/api/v1/folders?path={}", encoded(gone.to_str().unwrap())),
            404,
            "browse_missing",
        ),
        (
            "/api/v1/folders?path=Projects".to_owned(),
            422,
            "path_relative",
        ),
    ] {
        let (answered, refusal) = panel.get(&asked);
        assert_eq!(answered, status, "{asked}: {refusal}");
        assert_eq!(refusal["code"], code, "{asked}: {refusal}");
        assert!(refusal["error"].is_string(), "{asked}: {refusal}");
    }
}

/// What a front end without a terminal does not run is a 422 that names the
/// command, with its code, and nothing of it runs.
#[test]
fn a_refused_command_is_a_422_naming_the_command() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mindweaver", dir.path(), home.path());

    let edit = json!({
        "type": "edit",
        "holder": {"type": "workspace"},
        "kind": "skill",
        "name": "commit-style",
    });
    for path in ["/api/v1/tasks/preview", "/api/v1/tasks"] {
        let (status, refusal) = panel.post(path, edit.clone());
        assert_eq!(status, 422, "{path}");
        assert_eq!(
            refusal,
            json!({
                "error": "`mindweaver edit skill:commit-style` opens an editor, which needs a terminal to draw on",
                "code": "opens_editor",
            }),
            "{path}"
        );
    }
    let (_, transcript) = panel.get("/api/v1/transcript");
    assert_eq!(transcript, json!([]));
}

/// `mind ui` is `mindweaver ui` from where it is run: the panel is over the
/// workspace a command run there would use.
#[test]
fn mind_ui_in_a_project_serves_the_workspace_above_it() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let panel = serve("mind", &dir.path().join("alpha"), home.path());

    let snapshot = panel.snapshot();

    assert_eq!(
        snapshot["workspace"]["path"],
        dir.path().to_str().unwrap(),
        "{snapshot}"
    );
    assert_eq!(snapshot["workspace"]["reason"], "above");
}

/// Through the dev server the link is the dev server's, the note says where
/// the server itself listens, which the link no longer does, and a change
/// the dev server's proxy forwards, with the browser's cookie and the dev
/// server's Origin, is taken. From any other page it is not.
#[test]
fn through_the_dev_server_the_link_is_its_own_and_its_changes_are_taken() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let (mut started, stdout, link) = launch("mindweaver", dir.path(), home.path(), &["--dev"]);
    let token = link
        .strip_prefix("http://127.0.0.1:5173/?token=")
        .unwrap_or_else(|| panic!("not the dev server's link: {link}"))
        .to_owned();
    let mut note = String::new();
    BufReader::new(started.child.stderr.take().unwrap())
        .read_line(&mut note)
        .unwrap();
    let address = note
        .split_once(" on ")
        .and_then(|(_, rest)| rest.split_once(", through the panel's dev server at "))
        .map(|(address, _)| address.parse().unwrap())
        .unwrap_or_else(|| panic!("no address said: {note}"));
    let panel = Panel {
        started,
        address,
        token,
        stdout,
    };
    let shelf = json!({"type": "shelf"});

    let (status, ran) = panel.send(
        "POST",
        "/api/v1/tasks",
        Auth::Cookie("http://127.0.0.1:5173"),
        Some(shelf.clone()),
    );
    assert_eq!(status, 200, "{ran}");

    let (status, refusal) = panel.send(
        "POST",
        "/api/v1/tasks",
        Auth::Cookie("http://127.0.0.1:5174"),
        Some(shelf),
    );
    assert_eq!(status, 403, "{refusal}");
    assert_eq!(refusal["code"], "foreign_origin", "{refusal}");
    assert!(
        refusal["error"]
            .as_str()
            .is_some_and(|error| error.contains("only from its own pages")),
        "{refusal}"
    );
}

/// A port somebody holds is said as a failure, and nothing is served: no
/// link, and no other port in its place. Started rather than spawned, so a
/// panel that served after all fails the test once the wait is up, and is
/// killed, rather than hang it until a Ctrl-C.
#[test]
fn a_port_somebody_holds_is_a_failure_and_no_link() {
    let dir = workspace_with_own();
    let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = held.local_addr().unwrap().port().to_string();
    let mut args = vec![
        "ui",
        "--no-open",
        "--port",
        &port,
        "--home=",
        "--workspace=",
    ];
    args.extend(NO_AGENT_HOMES);

    let mut started = start("mindweaver", dir.path(), &args, &[]);
    let exited = exit(&mut started, Duration::from_secs(60));

    assert!(!exited.success(), "{exited:?}");
    let stdout = std::io::read_to_string(started.child.stdout.take().unwrap()).unwrap();
    let stderr = std::io::read_to_string(started.child.stderr.take().unwrap()).unwrap();
    assert_eq!(stdout, "");
    let said = format!("error: the panel cannot listen on 127.0.0.1:{port}: ");
    assert!(stderr.starts_with(&said), "{stderr}");
    assert!(
        stderr.ends_with("; name another with --port, or 0 for one the system picks\n"),
        "{stderr}"
    );
}

/// Ctrl-C is SIGINT here, sent by `kill`, as a terminal sends it: the panel
/// stops and exits as a command that went well does. On stdout, after the
/// link, what the TUI prints when it closes: every line that changed
/// something, with what each said, and no read. On stderr, where it served
/// and how it would stop.
#[cfg(unix)]
#[test]
fn ctrl_c_stops_the_panel_which_prints_what_it_changed() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let mut panel = serve("mindweaver", dir.path(), home.path());
    let (status, _) = panel.post("/api/v1/tasks", json!({"type": "shelf"}));
    assert_eq!(status, 200);
    let install = json!({
        "type": "install",
        "projects": ["alpha"],
        "kind": "skill",
        "name": "commit-style",
        "from": null,
    });
    let (status, ran) = panel.post("/api/v1/tasks", install);
    assert_eq!(status, 200, "{ran}");

    let (exited, rest, said) = interrupt(&mut panel);

    assert!(exited.success(), "{exited:?}");
    let run = &ran["outcome"]["runs"][0];
    let mut transcript = format!(
        "$ {}\n{}",
        run["command"].as_str().unwrap(),
        run["stdout"].as_str().unwrap()
    );
    for line in run["stderr"].as_array().unwrap() {
        transcript.push_str(&format!("{}\n", line["text"].as_str().unwrap()));
    }
    assert_eq!(rest, transcript);
    let named = format!("$ mindweaver -W {} install -p ", dir.path().display());
    assert!(rest.starts_with(&named), "{rest}");
    assert_eq!(
        said,
        format!(
            "note: serving the workspace at {} on {} until Ctrl-C\n",
            dir.path().display(),
            panel.address
        )
    );
}

/// `line` as the log lays it out, the time of day it was said and what
/// was said, with every time in that written `Nms`: what a test holds a
/// line to, apart from the times no test can say beforehand.
fn logged(line: &str) -> String {
    let (at, said) = line
        .split_once(' ')
        .unwrap_or_else(|| panic!("no time of day before {line:?}"));
    let clock: Vec<&str> = at
        .strip_suffix('Z')
        .unwrap_or_else(|| panic!("not in UTC: {line:?}"))
        .split([':', '.'])
        .collect();
    assert!(
        clock.len() == 4
            && clock.iter().zip([2, 2, 2, 3]).all(
                |(part, width)| part.len() == width && part.bytes().all(|b| b.is_ascii_digit())
            ),
        "not a time of day: {line:?}"
    );
    said.split(' ')
        .map(|word| {
            let (bare, after) = word
                .strip_suffix(',')
                .map_or((word, ""), |bare| (bare, ","));
            match bare.strip_suffix("ms") {
                Some(millis)
                    if !millis.is_empty() && millis.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    format!("Nms{after}")
                }
                _ => word.to_owned(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// With `--log`, every request the panel answers is said on stderr after the
/// note, once answered, and every run as the writer thread takes it, as
/// each of its lines has run, and as it ends, each after the time of day it
/// was said at; never the token, though the link and every request carried
/// it, nor what a request sent. Without it, the note alone, as ever
/// ([`ctrl_c_stops_the_panel_which_prints_what_it_changed`]).
#[cfg(unix)]
#[test]
fn with_log_each_request_and_run_is_said_on_stderr_and_never_the_token() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let mut panel = serve_with("mindweaver", dir.path(), home.path(), &["--log"]);
    panel.snapshot();
    let install = json!({
        "type": "install",
        "projects": ["alpha"],
        "kind": "skill",
        "name": "commit-style",
        "from": null,
    });
    let (status, ran) = panel.post("/api/v1/tasks", install);
    assert_eq!(status, 200, "{ran}");

    let (exited, _, said) = interrupt(&mut panel);

    assert!(exited.success(), "{exited:?}");
    assert!(!said.contains(&panel.token), "{said}");
    assert!(
        !said.contains("commit-style"),
        "what the run was sent: {said}"
    );
    let mut lines = said.lines();
    assert_eq!(
        lines.next(),
        Some(
            format!(
                "note: serving the workspace at {} on {} until Ctrl-C",
                dir.path().display(),
                panel.address
            )
            .as_str()
        )
    );
    assert_eq!(
        lines.map(logged).collect::<Vec<_>>(),
        [
            "GET /api/v1/snapshot 200 Nms",
            "run: Nms waiting its turn, Nms working out its 1 line",
            "run: line 1 of 1 took Nms, Nms of it waiting for a workspace's lock",
            "run: Nms running its 1 line",
            "POST /api/v1/tasks 200 Nms",
        ],
        "{said}"
    );
}

// ---------------------------------------------------------------------------
// The token file
// ---------------------------------------------------------------------------

/// What `--token-file` holds, read back as a script reads it, into a struct
/// that refuses a field it does not name: the contract, name by name.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Handed {
    version: u32,
    api: String,
    token: String,
}

/// `path`, read as [`Handed`].
fn handed(path: &Path) -> Handed {
    let text = fs::read_to_string(path).unwrap();
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text}"))
}

/// What a script that did not start the panel does: read where the API is,
/// and the token, from the file `--token-file` named, and call the API with
/// it as Bearer. The file is whole by the time the link is out, it is the
/// launch's own token, and no account but this one may read it. Once the
/// panel stops it is gone, and nothing it said holds the token but the
/// link it always printed.
#[test]
fn a_script_reaches_the_api_through_the_token_file_which_goes_when_ui_stops() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let run = TempDir::new().unwrap();
    let path = run.path().join("api.json");
    let panel = serve_with(
        "mindweaver",
        dir.path(),
        home.path(),
        &["--token-file", path.to_str().unwrap()],
    );

    let handed = handed(&path);

    assert_eq!(handed.version, 1);
    let origin = format!("http://{}", panel.address);
    assert_eq!(handed.api, format!("{origin}/api/v1"));
    assert_eq!(handed.token, panel.token);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{mode:o}");
    }
    let api = handed.api.strip_prefix(&origin).unwrap();
    let (status, snapshot) = panel.send(
        "GET",
        &format!("{api}/snapshot"),
        Auth::BearerOf(&handed.token),
        None,
    );
    assert_eq!(status, 200, "{snapshot}");
    assert_eq!(
        snapshot["workspace"]["path"],
        json!(dir.path().to_str().unwrap())
    );

    #[cfg(unix)]
    {
        let mut panel = panel;
        let (exited, rest, said) = interrupt(&mut panel);

        assert!(exited.success(), "{exited:?}");
        assert!(
            fs::symlink_metadata(&path).is_err(),
            "the file is still there"
        );
        assert_eq!(rest, "");
        assert_eq!(
            said,
            format!(
                "note: serving the workspace at {} on {} until Ctrl-C\n",
                dir.path().display(),
                panel.address
            )
        );
    }
}

/// How a script stops a panel it runs in the background: `kill`, `timeout`
/// or a service manager, whose SIGTERM `ui` takes as a Ctrl-C. So the
/// file goes as it does at a Ctrl-C, rather than stay behind, holding a
/// dead token, to refuse the next launch naming its path; and the panel
/// prints what it changed and exits as a command that went well does.
#[cfg(unix)]
#[test]
fn a_sigterm_stops_the_panel_as_ctrl_c_does_and_takes_the_token_file_away() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let run = TempDir::new().unwrap();
    let path = run.path().join("api.json");
    let token_file = ["--token-file", path.to_str().unwrap()];
    let mut panel = serve_with("mindweaver", dir.path(), home.path(), &token_file);
    let install = json!({
        "type": "install",
        "projects": ["alpha"],
        "kind": "skill",
        "name": "commit-style",
        "from": null,
    });
    let (status, ran) = panel.post("/api/v1/tasks", install);
    assert_eq!(status, 200, "{ran}");

    let (exited, rest, said) = signal(&mut panel, "-TERM");

    assert!(exited.success(), "{exited:?}");
    assert!(
        fs::symlink_metadata(&path).is_err(),
        "the file is still there"
    );
    let named = format!("$ mindweaver -W {} install -p ", dir.path().display());
    assert!(rest.starts_with(&named), "{rest}");
    assert_eq!(
        said,
        format!(
            "note: serving the workspace at {} on {} until Ctrl-C\n",
            dir.path().display(),
            panel.address
        )
    );
    let mut again = serve_with("mindweaver", dir.path(), home.path(), &token_file);
    assert_eq!(handed(&path).token, again.token);
    let (exited, _, _) = signal(&mut again, "-TERM");
    assert!(exited.success(), "{exited:?}");
    assert!(fs::symlink_metadata(&path).is_err());
}

/// Somebody's once it holds something else: left where it is when the
/// panel stops, as it was written, and said, with nothing gone wrong.
#[cfg(unix)]
#[test]
fn a_token_file_changed_while_the_panel_ran_is_left_where_it_is() {
    let dir = workspace_with_own();
    let home = TempDir::new().unwrap();
    let run = TempDir::new().unwrap();
    let path = run.path().join("api.json");
    let mut panel = serve_with(
        "mindweaver",
        dir.path(),
        home.path(),
        &["--token-file", path.to_str().unwrap()],
    );
    fs::write(&path, "{\"mine\": true}\n").unwrap();

    let (exited, _, said) = interrupt(&mut panel);

    assert!(exited.success(), "{exited:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "{\"mine\": true}\n");
    assert_eq!(
        said,
        format!(
            "note: serving the workspace at {} on {} until Ctrl-C\n\
             note: {}: changed since the API's address and token were written there, so it \
             is left as it is\n",
            dir.path().display(),
            panel.address,
            path.display()
        )
    );
}

/// `ui --token-file path`, started as a held port's is, so a panel served
/// after all fails the test once the wait is up rather than hang it; and
/// what it printed on stdout and on stderr once it exited, which it must
/// have done unsuccessfully.
fn refused_token_file(path: &Path) -> (String, String) {
    let dir = workspace_with_own();
    let mut args = vec![
        "ui",
        "--no-open",
        "--port",
        "0",
        "--home=",
        "--workspace=",
        "--token-file",
        path.to_str().unwrap(),
    ];
    args.extend(NO_AGENT_HOMES);

    let mut started = start("mindweaver", dir.path(), &args, &[]);
    let exited = exit(&mut started, Duration::from_secs(60));

    assert!(!exited.success(), "{exited:?}");
    let stdout = std::io::read_to_string(started.child.stdout.take().unwrap()).unwrap();
    let stderr = std::io::read_to_string(started.child.stderr.take().unwrap()).unwrap();
    (stdout, stderr)
}

/// Somebody's file at the path refuses the launch before anything is
/// served: no link, and the file exactly as it was, its bytes, its mode and
/// when it was last written.
#[test]
fn a_token_file_already_there_is_refused_and_left_exactly_as_it_was() {
    let run = TempDir::new().unwrap();
    let path = run.path().join("api.json");
    fs::write(&path, "{\"somebody\": \"else\"}\n").unwrap();
    let before = fs::metadata(&path).unwrap();

    let (stdout, stderr) = refused_token_file(&path);

    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        format!(
            "error: {}: already exists, and is left alone, so the panel was not served: \
             --token-file writes a file of its own, never over one\n",
            path.display()
        )
    );
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\"somebody\": \"else\"}\n"
    );
    let after = fs::metadata(&path).unwrap();
    assert_eq!(after.permissions(), before.permissions());
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
}

#[test]
fn a_token_file_in_a_folder_not_there_is_refused_and_no_folder_is_made() {
    let run = TempDir::new().unwrap();
    let path = run.path().join("run").join("api.json");

    let (stdout, stderr) = refused_token_file(&path);

    assert_eq!(stdout, "");
    assert_eq!(
        stderr,
        format!(
            "error: {}: its folder does not exist, so the panel was not served: \
             --token-file makes no folder\n",
            path.display()
        )
    );
    assert!(!run.path().join("run").exists());
}

/// The option on both binaries, as `mind ui`, `mind weaver ui` and `mindweaver
/// ui` take it, in either spelling, the path kept as it was typed for the
/// command to make it absolute from where it works; and none without it.
#[test]
fn both_binaries_take_a_token_file_for_ui() {
    let given = UiOptions {
        no_open: true,
        token_file: Some(PathBuf::from("run/api.json")),
        ..UiOptions::default()
    };
    let ui = |parsed: Option<WeaverCommand>| match parsed {
        Some(WeaverCommand::Ui(options)) => options,
        other => panic!("not ui: {other:?}"),
    };

    let mind =
        Cli::try_parse_from(["mind", "ui", "--no-open", "--token-file", "run/api.json"]).unwrap();
    let Some(Command::Ui(options)) = mind.command else {
        panic!("not ui: {:?}", mind.command);
    };
    assert_eq!(options, given);
    let nested = Cli::try_parse_from([
        "mind",
        "weaver",
        "ui",
        "--token-file=run/api.json",
        "--no-open",
    ])
    .unwrap();
    let Some(Command::Weaver { command }) = nested.command else {
        panic!("not weaver: {:?}", nested.command);
    };
    assert_eq!(ui(command), given);
    let weaver = WeaverCli::try_parse_from([
        "mindweaver",
        "ui",
        "--no-open",
        "--token-file",
        "run/api.json",
    ])
    .unwrap();
    assert_eq!(ui(weaver.command), given);

    let bare = WeaverCli::try_parse_from(["mindweaver", "ui"]).unwrap();
    assert_eq!(ui(bare.command).token_file, None);
}

#[test]
fn a_token_file_is_refused_without_a_path() {
    for line in [
        &["mind", "ui", "--token-file"][..],
        &["mindweaver", "ui", "--token-file"],
        &["mindweaver", "ui", "--token-file="],
    ] {
        let error = match line[0] {
            "mind" => Cli::try_parse_from(line).map(drop),
            _ => WeaverCli::try_parse_from(line).map(drop),
        }
        .expect_err(&line.join(" "));

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::InvalidValue,
            "{}",
            line.join(" ")
        );
    }
}
