//! What the event stream tells: the guide's "Following what happens", a
//! stream opened as a script opens one and read an event at a time, while
//! a test runs a task, a command in a terminal or a switch.
//!
//! What a change on disk is told as comes from the watcher, which takes in
//! what happens together before it tells it: a test waits for the event it
//! is about, and holds that one, not how many came before it.

use serde_json::{json, Value};

use crate::{guide_workspace, linking, Scenario};

/// Whether `event` is of the type `kind`.
fn is(kind: &'static str) -> impl Fn(&Value) -> bool {
    move |event| event["type"] == kind
}

#[test]
fn every_stream_starts_by_saying_to_read_everything_and_whether_it_watches() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    let mut stream = served.events();

    assert_eq!(
        [stream.told(), stream.told()],
        [
            json!({"type": "resync", "switches": 0}),
            json!({"type": "watching"}),
        ]
    );
}

#[test]
fn a_run_is_told_as_it_starts_and_once_it_finishes_whoever_sent_it() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");
    let mut stream = served.events();
    stream.opened();

    served
        .post(
            "/tasks",
            json!({"type": "link", "path": "/home/you/Projects/web"}),
        )
        .is(200);

    let told: Vec<Value> = stream
        .until(is("command_finished"))
        .into_iter()
        .filter(|event| event["type"] != "changed")
        .collect();
    let lines = json!(["mindweaver -W /home/you/Projects link /home/you/Projects/web"]);
    assert_eq!(
        told,
        [
            json!({"type": "command_started", "run": 1, "lines": lines, "cancellable": false}),
            json!({"type": "command_finished", "run": 1, "lines": lines}),
        ]
    );
}

/// A gather from the API is previewed as a run that takes a cancel, and
/// told step by step as it goes, each step in the words a terminal draws
/// it in, until it has put what it found on the shelf.
#[test]
fn a_gather_is_told_step_by_step_until_what_it_found_is_on_the_shelf() {
    let scenario = Scenario::projects(&["api"]);
    scenario.source(
        "~/src/acme-skills",
        &[(
            "skills/pdf/SKILL.md",
            "---\nname: pdf\ndescription: Read PDFs\n---\n",
        )],
    );
    let served = scenario.serve("~/Projects");
    let mut stream = served.events();
    stream.opened();
    let gather = json!({"type": "gather", "url": "/home/you/src/acme-skills",
                        "folder": "skills", "reference": null});
    let lines = json!(["mindweaver -W /home/you/Projects gather git /home/you/src/acme-skills"]);

    served
        .post("/tasks/preview", gather.clone())
        .answers(json!({"lines": lines, "confirm": [], "cancellable": true}));
    served
        .post("/tasks", gather)
        .is(200)
        .answers_at("/outcome/ok", json!(true));

    let told: Vec<Value> = stream
        .until(is("command_finished"))
        .into_iter()
        .filter(|event| event["type"] != "changed")
        .collect();
    assert_eq!(
        told.first(),
        Some(&json!({"type": "command_started", "run": 1, "lines": lines, "cancellable": true}))
    );
    assert_eq!(
        told.get(1),
        Some(&json!({"type": "progress", "run": 1, "progress": {
            "stage": "connecting", "done": null, "total": null,
            "said": "connecting", "cancellable": true,
        }}))
    );
    assert_eq!(
        told[told.len() - 2..],
        [
            json!({"type": "progress", "run": 1, "progress": {
                "stage": "placing", "done": 1, "total": 1,
                "said": "placing on the shelf 1/1", "cancellable": false,
            }}),
            json!({"type": "command_finished", "run": 1, "lines": lines}),
        ]
    );
    assert!(
        told[1..told.len() - 1]
            .iter()
            .all(|event| event["type"] == "progress" && event["run"] == 1),
        "{told:?}"
    );
    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints("pdf  /home/you/src/acme-skills  Read PDFs\n");
}

#[test]
fn a_change_made_in_a_terminal_is_told_by_the_project_it_reached() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");
    let mut stream = served.events();
    stream.opened();

    scenario.given(
        "~/Projects/api",
        "mind add skill deploy 'Ship the service to staging'",
    );

    let told = stream.until(is("changed"));
    assert_eq!(
        told.last(),
        Some(&json!({"type": "changed", "scopes": [{"type": "project", "entry": "api"}]}))
    );
}

#[test]
fn a_change_to_the_library_made_in_an_editor_is_told_as_the_workspaces() {
    let scenario = linking(&["api"]);
    let served = scenario.serve("~/Projects");
    let mut stream = served.events();
    stream.opened();

    scenario.file(
        "~/Projects/.mindweaver/library/skills/review/SKILL.md",
        &crate::added("review", "Read every change before it lands"),
    );

    let told = stream.until(is("changed"));
    assert_eq!(
        told.last(),
        Some(&json!({"type": "changed", "scopes": [{"type": "workspace"}]}))
    );
}

#[test]
fn a_switch_is_told_to_every_stream_with_how_many_have_been_taken() {
    let scenario = Scenario::empty();
    scenario
        .given("~", "mindweaver init")
        .folder("~/Projects")
        .given("~/Projects", "mindweaver init")
        .folder("~/Notes")
        .given("~/Notes", "mindweaver init");
    let served = scenario.serve("~/Projects");
    let mut first = served.events();
    first.opened();
    let mut second = served.events();
    second.opened();

    served
        .post("/switch", json!({"path": "/home/you/Notes"}))
        .is(200);

    for stream in [&mut first, &mut second] {
        assert_eq!(
            stream.until(is("switched")).last(),
            Some(&json!({"type": "switched", "switches": 1}))
        );
    }
}
