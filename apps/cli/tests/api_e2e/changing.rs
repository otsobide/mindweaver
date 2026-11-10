//! Changing things: the guide's "Changing things", every change a task,
//! previewed as the command lines it is and run through the same parser as
//! a line typed. What a run leaves is read back with the command line, so
//! a test holds the API to changing nothing a command cannot.

use serde_json::json;

use crate::{entries, guide_workspace, linking, Scenario};

/// The guide's install of `review` into `web`.
fn install_review_into_web() -> serde_json::Value {
    json!({"type": "install", "projects": ["web"], "kind": "skill", "name": "review", "from": null})
}

/// The guide's link of `web`.
fn link_web() -> serde_json::Value {
    json!({"type": "link", "path": "/home/you/Projects/web"})
}

#[test]
fn a_preview_says_the_lines_a_task_is_and_runs_none_of_them() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks/preview", link_web())
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects link /home/you/Projects/web"],
            "confirm": [],
            "cancellable": false,
        }));
    assert_eq!(entries(&served.get("/projects")), ["api"]);
    served.get("/transcript").is(200).answers(json!([]));
}

#[test]
fn linking_a_folder_runs_the_line_its_preview_said() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.post("/tasks", link_web()).is(200).answers(json!({
        "lines": ["mindweaver -W /home/you/Projects link /home/you/Projects/web"],
        "outcome": {
            "ok": true,
            "runs": [{
                "command": "mindweaver -W /home/you/Projects link /home/you/Projects/web",
                "error": null,
                "ok": true,
                "stderr": [],
                "stdout": "linked web as web\n",
            }],
        },
    }));
    assert_eq!(entries(&served.get("/projects")), ["api", "web"]);
}

#[test]
fn installing_a_skill_writes_a_copy_where_each_of_the_projects_agents_reads() {
    let scenario = linking(&["api", "web"]);
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks", install_review_into_web())
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review"],
            "outcome": {
                "ok": true,
                "runs": [{
                    "command": "mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review",
                    "error": null,
                    "ok": true,
                    "stderr": [],
                    "stdout": "installed review into web, from workspace\n",
                }],
            },
        }));
    assert_eq!(
        scenario.files("~/Projects/web/.claude/skills"),
        ["review/SKILL.md"]
    );
    assert_eq!(
        scenario.files("~/Projects/web/.agents/skills"),
        ["review/SKILL.md"]
    );
    scenario
        .run("~/Projects/web", "mind status")
        .succeeds()
        .prints("review  current  claude,codex,opencode  workspace\n")
        .quiet();
}

#[test]
fn a_preview_says_which_of_its_lines_take_something_away() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks/preview",
            json!({"type": "uninstall", "projects": ["api"], "kind": "skill", "name": "review", "force": false}),
        )
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects uninstall -p /home/you/Projects/api skill:review"],
            "confirm": [{
                "caution": "uninstalls",
                "line": 0,
                "message": "takes copies Mindweaver installed out of projects",
            }],
            "cancellable": false,
        }));
    assert_eq!(
        scenario.files("~/Projects/api/.claude/skills"),
        ["review/SKILL.md"]
    );
}

#[test]
fn a_run_the_command_refuses_is_still_answered_with_its_error_and_code() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({"type": "install", "projects": ["api"], "kind": "skill", "name": "deploy", "from": null}),
        )
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects install -p /home/you/Projects/api skill:deploy"],
            "outcome": {
                "ok": false,
                "runs": [{
                    "command": "mindweaver -W /home/you/Projects install -p /home/you/Projects/api skill:deploy",
                    "error": {
                        "code": "not_offered",
                        "message": "nothing called `deploy` can be installed: gather it, load a \
                                    workspace that has it with `mindweaver load`, or add it to the \
                                    workspace with `mindweaver add`",
                    },
                    "ok": false,
                    "stderr": [],
                    "stdout": "",
                }],
            },
        }));
}

#[test]
fn updating_every_project_writes_what_is_outdated_and_says_what_it_left_with_its_code() {
    let scenario = linking(&["api", "web"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style 'How every repo here writes commits'",
        )
        .given("~/Projects", "mindweaver install -p api commit-style")
        .given("~/Projects", "mindweaver install -p web review")
        .file(
            "~/Projects/.mindweaver/library/skills/commit-style/SKILL.md",
            &crate::added("commit-style", "How every repo here writes commits, now"),
        )
        .file(
            "~/Projects/web/.claude/skills/review/SKILL.md",
            &crate::added("review", "Mine now"),
        );
    let served = scenario.serve("~/Projects");

    // What the panel's Updates page sends to update them all.
    served
        .post(
            "/tasks",
            json!({"type": "update", "holder": {"type": "workspace"}, "reference": null, "force": false}),
        )
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects update --all --yes"],
            "outcome": {
                "ok": false,
                "runs": [{
                    "command": "mindweaver -W /home/you/Projects update --all --yes",
                    "error": null,
                    "ok": false,
                    "stderr": [{
                        "code": "edited",
                        "text": "warning: review in web was changed since it was installed, at \
                                 .claude/skills/review, so it was left alone; --force writes over it",
                    }],
                    "stdout": "updated commit-style in api, from workspace\n",
                }],
            },
        }));
    scenario
        .run("~/Projects/api", "mind status")
        .succeeds()
        .prints("commit-style  current  claude,codex,opencode  workspace\n")
        .quiet();
    assert_eq!(
        scenario.read("~/Projects/web/.claude/skills/review/SKILL.md"),
        crate::added("review", "Mine now")
    );
}

#[test]
fn a_plan_applied_is_one_task_running_every_line_it_amounts_to() {
    let scenario = linking(&["api"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style 'How every repo here writes commits'",
        )
        .given("~/Projects", "mindweaver install -p api commit-style");
    let served = scenario.serve("~/Projects");

    // The tasks `/plan` answered for ticking review in api and unticking
    // commit-style, sent back as the panel's matrix sends them.
    served
        .post(
            "/tasks",
            json!({"type": "apply_install", "changes": [
                {"from": "workspace", "kind": "skill", "name": "review", "projects": ["api"], "type": "install"},
                {"force": false, "kind": "skill", "name": "commit-style", "projects": ["api"], "type": "uninstall"},
            ]}),
        )
        .is(200)
        .answers(json!({
            "lines": [
                "mindweaver -W /home/you/Projects install -p /home/you/Projects/api --from workspace skill:review",
                "mindweaver -W /home/you/Projects uninstall -p /home/you/Projects/api skill:commit-style",
            ],
            "outcome": {
                "ok": true,
                "runs": [
                    {
                        "command": "mindweaver -W /home/you/Projects install -p /home/you/Projects/api --from workspace skill:review",
                        "error": null,
                        "ok": true,
                        "stderr": [],
                        "stdout": "installed review into api, from workspace\n",
                    },
                    {
                        "command": "mindweaver -W /home/you/Projects uninstall -p /home/you/Projects/api skill:commit-style",
                        "error": null,
                        "ok": true,
                        "stderr": [],
                        "stdout": "removed commit-style from api\n",
                    },
                ],
            },
        }));
    scenario
        .run("~/Projects/api", "mind status")
        .succeeds()
        .prints("review  current  claude,codex,opencode  workspace\n")
        .quiet();
}

#[test]
fn folders_marked_to_be_made_projects_and_linked_are_one_line_each() {
    let scenario = Scenario::projects(&["api"]);
    scenario.repository("~/Projects/site");
    let served = scenario.serve("~/Projects");

    // What the panel's page of what could be linked sends for a repository
    // ticked to be made a project and linked.
    served
        .post(
            "/tasks",
            json!({"type": "minds", "init": ["/home/you/Projects/site"], "link": ["/home/you/Projects/site"], "unlink": []}),
        )
        .is(200)
        .answers(json!({
            "lines": [
                "mind -W /home/you/Projects -C /home/you/Projects/site init",
                "mindweaver -W /home/you/Projects link /home/you/Projects/site",
            ],
            "outcome": {
                "ok": true,
                "runs": [
                    {
                        "command": "mind -W /home/you/Projects -C /home/you/Projects/site init",
                        "error": null,
                        "ok": true,
                        "stderr": [],
                        "stdout": "initialized mind project `site` in /home/you/Projects/site/.mind\n",
                    },
                    {
                        "command": "mindweaver -W /home/you/Projects link /home/you/Projects/site",
                        "error": null,
                        "ok": true,
                        "stderr": [],
                        "stdout": "linked site as site\n",
                    },
                ],
            },
        }));
    assert_eq!(entries(&served.get("/projects")), ["api", "site"]);
}

#[test]
fn unticking_an_agent_of_a_project_takes_the_copies_only_it_read_along() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    let served = scenario.serve("~/Projects");

    // What a project's settings form sends with codex and opencode unticked.
    served
        .post(
            "/tasks",
            json!({
                "type": "toggled",
                "holder": {"type": "project", "entry": "api"},
                "was": {"agents": ["claude", "codex", "opencode"], "share": "local", "edit_tracked": false},
                "now": {"agents": ["claude"], "share": "local", "edit_tracked": false},
            }),
        )
        .is(200)
        .answers(json!({
            "lines": ["mind -W /home/you/Projects -C /home/you/Projects/api agents --yes claude"],
            "outcome": {
                "ok": true,
                "runs": [{
                    "command": "mind -W /home/you/Projects -C /home/you/Projects/api agents --yes claude",
                    "error": null,
                    "ok": true,
                    "stderr": [],
                    "stdout": "set agents to claude in api, where they were claude,codex,opencode\n\
                               removed review from .agents/skills/review\n",
                }],
            },
        }));
    assert!(!scenario.exists("~/Projects/api/.agents/skills/review"));
    scenario
        .run("~/Projects/api", "mind status")
        .succeeds()
        .prints("review  current  claude  workspace\n")
        .quiet();
}

#[test]
fn the_transcript_holds_each_run_in_order_and_no_preview_or_read() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");
    served.post("/tasks/preview", link_web()).is(200);
    served.given(link_web()).given(install_review_into_web());
    served.get("/snapshot").is(200);

    let answer = served.get("/transcript");

    answer.is(200);
    let lines: Vec<serde_json::Value> = answer
        .json()
        .as_array()
        .unwrap()
        .iter()
        .map(|ran| ran["lines"].clone())
        .collect();
    assert_eq!(
        lines,
        [
            json!(["mindweaver -W /home/you/Projects link /home/you/Projects/web"]),
            json!([
                "mindweaver -W /home/you/Projects install -p /home/you/Projects/web skill:review"
            ]),
        ]
    );
}

#[test]
fn what_nothing_offers_any_more_is_taken_out_of_every_project_in_one_task() {
    let scenario = linking(&["api", "web"]);
    scenario.given("~/Projects", "mindweaver install --all-projects review");
    // Out of the library alone: it warns of what it left, which a given
    // step may not.
    scenario
        .run("~/Projects", "mindweaver remove review --yes")
        .exits(1);
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({"type": "uninstall_orphaned", "holder": {"type": "workspace"}}),
        )
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects uninstall --all-projects --orphaned"],
            "outcome": {
                "ok": true,
                "runs": [{
                    "command": "mindweaver -W /home/you/Projects uninstall --all-projects --orphaned",
                    "error": null,
                    "ok": true,
                    "stderr": [],
                    "stdout": "removed review from api\nremoved review from web\n",
                }],
            },
        }));
    assert!(!scenario.exists("~/Projects/api/.claude/skills/review"));
    assert!(!scenario.exists("~/Projects/web/.claude/skills/review"));
}

#[test]
fn a_read_the_panel_runs_as_a_line_is_answered_with_what_the_command_printed() {
    let scenario = linking(&["api"]);
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({"type": "validate", "holder": {"type": "workspace"}}),
        )
        .is(200)
        .answers_at(
            "/outcome/runs/0",
            json!({
                "command": "mindweaver -W /home/you/Projects validate",
                "error": null,
                "ok": true,
                "stderr": [],
                "stdout": "review: ok\n\n1 skill checked, 0 invalid\n",
            }),
        );
}

#[test]
fn the_tasks_that_only_read_are_the_lines_of_the_commands_that_list() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    let lines: Vec<serde_json::Value> = ["shelf", "workspaces", "refresh"]
        .iter()
        .map(|kind| {
            served
                .post("/tasks/preview", json!({"type": kind}))
                .is(200)
                .json()["lines"]
                .clone()
        })
        .collect();

    assert_eq!(
        lines,
        [
            json!(["mindweaver -W /home/you/Projects gather list"]),
            json!(["mindweaver -W /home/you/Projects workspaces"]),
            // Read everything again: no line, since nothing runs.
            json!([]),
        ]
    );
}
