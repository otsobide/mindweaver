//! Writing skills and rules through the API, as the panel's library and its
//! editor do: each a task run as the line the command line takes, the save
//! of an editor's text among them, which names a scratch file the server
//! writes rather than carry the text in the line.

use serde_json::json;

use crate::{added, linking, Scenario};

/// The SHA-256 of the text `add` gives `deploy`, which a read of the file
/// answers and a save names.
const DEPLOY_HASH: &str = "a1458ed72e1dd39aefb9046cc7bc6c577f00c5ad1397e10f691f5ca85c28bff0";

/// What the editor saves over it.
const SAVED: &str = "---\nname: deploy\ndescription: Ship the service to staging\n---\n\n\
                     # deploy\n\nRun make deploy.\n";

/// `~/Projects` linking `api`, with `deploy` of its own, as `add` wrote it.
fn library_with_deploy() -> Scenario {
    let scenario = Scenario::projects(&["api"]);
    scenario.given(
        "~/Projects",
        "mindweaver add skill deploy 'Ship the service to staging'",
    );
    scenario
}

/// The task the editor's save is, of `deploy` in the library, read with
/// `hash`.
fn save(hash: &str) -> serde_json::Value {
    json!({
        "type": "save",
        "holder": {"type": "workspace"},
        "kind": "skill",
        "name": "deploy",
        "text": SAVED,
        "hash": hash,
    })
}

/// The line a save of [`SAVED`] read with `hash` is: its text in the
/// workspace's scratch file, named by the first 16 digits of the text's
/// own SHA-256 (`50958fe22e107cbe...`), so the line previewed is the line
/// run.
fn save_line(hash: &str) -> String {
    format!(
        "mindweaver -W /home/you/Projects edit --contents .mindweaver/tmp/edit-50958fe22e107cbe.md \
         --if-unchanged {hash} skill:deploy"
    )
}

#[test]
fn adding_a_skill_to_the_library_writes_it_there() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({
                "type": "add", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "deploy", "description": "Ship the service to staging", "template": null,
            }),
        )
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects add skill deploy 'Ship the service to staging'"],
            "outcome": {
                "ok": true,
                "runs": [{
                    "command": "mindweaver -W /home/you/Projects add skill deploy 'Ship the service to staging'",
                    "error": null,
                    "ok": true,
                    "stderr": [],
                    "stdout": "created skill deploy at \
                               /home/you/Projects/.mindweaver/library/skills/deploy/SKILL.md\n",
                }],
            },
        }));
    assert_eq!(
        scenario.read("~/Projects/.mindweaver/library/skills/deploy/SKILL.md"),
        added("deploy", "Ship the service to staging")
    );
}

#[test]
fn adding_a_rule_to_a_project_runs_minds_line_in_that_project() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({
                "type": "add", "holder": {"type": "project", "entry": "api"}, "kind": "rule",
                "name": "style", "description": "How code here reads", "template": null,
            }),
        )
        .is(200)
        .answers(json!({
            "lines": ["mind -W /home/you/Projects -C /home/you/Projects/api add rule style 'How code here reads'"],
            "outcome": {
                "ok": true,
                "runs": [{
                    "command": "mind -W /home/you/Projects -C /home/you/Projects/api add rule style 'How code here reads'",
                    "error": null,
                    "ok": true,
                    "stderr": [],
                    "stdout": "created rule style at /home/you/Projects/api/rules/style.md\n",
                }],
            },
        }));
    assert_eq!(scenario.files("~/Projects/api/rules"), ["style.md"]);
}

#[test]
fn saving_writes_the_text_the_editor_held_and_leaves_no_scratch_file() {
    let scenario = library_with_deploy();
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks", save(DEPLOY_HASH))
        .is(200)
        .answers(json!({
            "lines": [save_line(DEPLOY_HASH)],
            "outcome": {
                "ok": true,
                "runs": [{
                    "command": save_line(DEPLOY_HASH),
                    "error": null,
                    "ok": true,
                    "stderr": [],
                    "stdout": "edited skill deploy: ok\n",
                }],
            },
        }));
    assert_eq!(
        scenario.read("~/Projects/.mindweaver/library/skills/deploy/SKILL.md"),
        SAVED
    );
    let left: Vec<String> = scenario
        .files("~/Projects/.mindweaver")
        .into_iter()
        .filter(|file| file.starts_with("tmp/"))
        .collect();
    assert_eq!(left, Vec::<String>::new());
}

#[test]
fn a_save_over_a_change_made_since_it_was_read_writes_nothing() {
    let scenario = library_with_deploy();
    // Changed in an editor after the panel read it.
    let changed = added("deploy", "Ship the service to production");
    scenario.file(
        "~/Projects/.mindweaver/library/skills/deploy/SKILL.md",
        &changed,
    );
    let served = scenario.serve("~/Projects");

    served.post("/tasks", save(DEPLOY_HASH)).is(200).answers(json!({
        "lines": [save_line(DEPLOY_HASH)],
        "outcome": {
            "ok": false,
            "runs": [{
                "command": save_line(DEPLOY_HASH),
                "error": {
                    "code": "edit_stale",
                    "message": format!(
                        "/home/you/Projects/.mindweaver/library/skills/deploy/SKILL.md: changed \
                         since it was read, its SHA-256 now \
                         116c7d75c3cd556bb3afb3824cb94cfffc34d205c3a4a8c284eb4f070449853e and not \
                         {DEPLOY_HASH}, so it is not written over; read it again"
                    ),
                },
                "ok": false,
                "stderr": [],
                "stdout": "",
            }],
        },
    }));
    assert_eq!(
        scenario.read("~/Projects/.mindweaver/library/skills/deploy/SKILL.md"),
        changed
    );
}

#[test]
fn renaming_a_skill_moves_its_folder_and_rewrites_only_its_name() {
    let scenario = library_with_deploy();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({
                "type": "rename", "holder": {"type": "workspace"}, "kind": "skill",
                "from": "deploy", "to": "ship",
            }),
        )
        .is(200)
        .answers_at(
            "/outcome/runs",
            json!([{
                "command": "mindweaver -W /home/you/Projects rename skill:deploy ship",
                "error": null,
                "ok": true,
                "stderr": [],
                "stdout": "renamed skill deploy to ship at \
                           /home/you/Projects/.mindweaver/library/skills/ship\n",
            }]),
        );
    assert!(!scenario.exists("~/Projects/.mindweaver/library/skills/deploy"));
    assert_eq!(
        scenario.read("~/Projects/.mindweaver/library/skills/ship/SKILL.md"),
        "---\nname: ship\ndescription: Ship the service to staging\n---\n\n# deploy\n"
    );
}

#[test]
fn removing_one_of_the_librarys_own_everywhere_says_first_all_it_takes() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks/preview",
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "review", "everywhere": true, "yes": true,
            }),
        )
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects remove --everywhere --yes skill:review"],
            "confirm": [
                {
                    "caution": "removes",
                    "line": 0,
                    "message": "deletes an artifact: its folder and everything in it, or its file",
                },
                {
                    "caution": "uninstalls",
                    "line": 0,
                    "message": "takes copies Mindweaver installed out of projects",
                },
            ],
            "cancellable": false,
        }));
    assert!(scenario.exists("~/Projects/.mindweaver/library/skills/review"));
}

#[test]
fn removing_it_everywhere_takes_each_projects_copy_out_then_the_librarys() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({
                "type": "remove", "holder": {"type": "workspace"}, "kind": "skill",
                "name": "review", "everywhere": true, "yes": true,
            }),
        )
        .is(200)
        .answers_at(
            "/outcome/runs",
            json!([{
                "command": "mindweaver -W /home/you/Projects remove --everywhere --yes skill:review",
                "error": null,
                "ok": true,
                "stderr": [],
                "stdout": "removed review from api\n\
                           removed skill review from \
                           /home/you/Projects/.mindweaver/library/skills/review\n",
            }]),
        );
    assert!(!scenario.exists("~/Projects/.mindweaver/library/skills/review"));
    assert!(!scenario.exists("~/Projects/api/.claude/skills/review"));
    assert!(!scenario.exists("~/Projects/api/.agents/skills/review"));
}
