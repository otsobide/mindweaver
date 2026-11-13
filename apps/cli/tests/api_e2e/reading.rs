//! Reading what is where: the guide's "Reading", every read a GET naming
//! what it reads in its parameters, or a POST that changes nothing.

use serde_json::json;

use crate::{added, entries, guide_workspace, linking, Scenario, REVIEW};

/// The counts of each state a project's tally holds, in the snapshot's
/// order, from `current` to `authored`.
fn states(counts: [u64; 7]) -> serde_json::Value {
    let names = [
        "current", "outdated", "edited", "missing", "orphaned", "foreign", "authored",
    ];
    names
        .iter()
        .zip(counts)
        .map(|(state, count)| json!({"count": count, "state": state}))
        .collect()
}

#[test]
fn the_snapshot_says_which_workspace_it_reads_and_why() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.get("/snapshot").is(200).answers_at(
        "/workspace",
        json!({
            "made": true,
            "name": "Projects",
            "path": "/home/you/Projects",
            "reason": "above",
        }),
    );
}

#[test]
fn projects_lists_each_project_the_workspace_links_by_its_entry() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    let answer = served.get("/projects");

    answer.is(200);
    assert_eq!(entries(&answer), ["api"]);
}

#[test]
fn a_project_is_read_by_its_entry_with_its_settings() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.get("/project?entry=api").is(200).answers_at(
        "/settings",
        json!({
            "agents": ["claude", "codex", "opencode"],
            "edit_tracked": false,
            "share": "local",
            "targets": [],
        }),
    );
}

#[test]
fn a_project_says_how_each_thing_it_holds_stands_and_where_its_copies_are() {
    let scenario = linking(&["api"]);
    scenario
        .given("~/Projects", "mindweaver install -p api review")
        .given(
            "~/Projects/api",
            "mind add skill deploy 'Ship the service to staging'",
        )
        .file(
            "~/Projects/api/.claude/skills/notes/SKILL.md",
            &added("notes", "My own notes"),
        );
    let served = scenario.serve("~/Projects");

    let answer = served.get("/project?entry=api");

    answer.is(200).answers_at(
        "/artifacts",
        json!([
            {
                "agents": [],
                "issues": [],
                "kind": "skill",
                "name": "deploy",
                "origin": null,
                "state": "authored",
                "summary": "Ship the service to staging",
                "targets": ["skills/deploy"],
                "unread": null,
            },
            {
                "agents": ["claude", "opencode"],
                "issues": [],
                "kind": "skill",
                "name": "notes",
                "origin": null,
                "state": "foreign",
                "summary": "My own notes",
                "targets": [".claude/skills/notes"],
                "unread": null,
            },
            {
                "agents": ["claude", "codex", "opencode"],
                "issues": [],
                "kind": "skill",
                "name": "review",
                "origin": "workspace",
                "state": "current",
                "summary": REVIEW,
                "targets": [".agents/skills/review", ".claude/skills/review"],
                "unread": null,
            },
        ]),
    );
    answer
        .answers_at("/tally/states", states([1, 0, 0, 0, 0, 1, 1]))
        .answers_at("/tally/total", json!(3))
        .answers_at("/health", json!("ok"));
}

#[test]
fn the_library_lists_the_workspaces_own_and_which_projects_hold_a_copy() {
    let scenario = linking(&["api"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style 'How every repo here writes commits'",
        )
        .given("~/Projects", "mindweaver install -p api commit-style");
    let served = scenario.serve("~/Projects");

    served
        .get("/library")
        .is(200)
        .answers_at(
            "/artifacts",
            json!([
                {
                    "agents": [],
                    "issues": [],
                    "kind": "skill",
                    "name": "commit-style",
                    "origin": null,
                    "state": null,
                    "summary": "How every repo here writes commits",
                    "targets": [".mindweaver/library/skills/commit-style"],
                    "unread": null,
                },
                {
                    "agents": [],
                    "issues": [],
                    "kind": "skill",
                    "name": "review",
                    "origin": null,
                    "state": null,
                    "summary": REVIEW,
                    "targets": [".mindweaver/library/skills/review"],
                    "unread": null,
                },
            ]),
        )
        .answers_at(
            "/holders",
            json!([{"entry": "api", "kind": "skill", "name": "commit-style"}]),
        );
}

#[test]
fn an_artifacts_files_are_listed_with_the_one_it_is_read_from() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .get("/files?reference=skill:review")
        .is(200)
        .answers(json!({
            "entries": [
                {"folder": false, "path": "SKILL.md", "refused": null, "size": 74},
            ],
            "kind": "skill",
            "manifest": "SKILL.md",
            "name": "review",
            "truncated": false,
        }));
}

#[test]
fn a_file_is_read_as_text_with_the_hash_a_save_names() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .get("/contents?reference=skill:review&file=SKILL.md")
        .is(200)
        .answers(json!({
            "file": "SKILL.md",
            "hash": "785660eaf65b8fce834ca050b26ed265b4d787871877ff87f05b52212d32dbca",
            "kind": "skill",
            "manifest": {
                "body": "\n# review\n",
                "front_matter": [
                    {"key": "name", "value": "review"},
                    {"key": "description", "value": REVIEW},
                ],
            },
            "name": "review",
            "size": 74,
            "text": added("review", REVIEW),
            "withheld": null,
        }));
}

#[test]
fn a_projects_own_file_is_read_with_the_project_named() {
    let scenario = Scenario::projects(&["api"]);
    scenario.given(
        "~/Projects/api",
        "mind add skill deploy 'Ship the service to staging'",
    );
    let served = scenario.serve("~/Projects");

    served
        .get("/contents?reference=skill:deploy&file=SKILL.md&project=api")
        .is(200)
        .answers_at(
            "/text",
            json!(added("deploy", "Ship the service to staging")),
        )
        .answers_at(
            "/hash",
            json!("a1458ed72e1dd39aefb9046cc7bc6c577f00c5ad1397e10f691f5ca85c28bff0"),
        );
}

#[test]
fn a_file_too_large_to_show_is_said_by_its_size_alone() {
    let scenario = guide_workspace();
    // One byte past the most a file is read to: 1 MiB.
    scenario.file(
        "~/Projects/.mindweaver/library/skills/review/notes.md",
        &"x".repeat((1 << 20) + 1),
    );
    let served = scenario.serve("~/Projects");

    served
        .get("/contents?reference=skill:review&file=notes.md")
        .is(200)
        .answers(json!({
            "file": "notes.md",
            "hash": null,
            "kind": "skill",
            "manifest": null,
            "name": "review",
            "size": 1_048_577,
            "text": null,
            "withheld": "too_large",
        }));
}

#[test]
fn scan_offers_to_link_a_project_found_beside_the_linked_one() {
    let scenario = guide_workspace();
    let api = scenario.project_id("~/Projects/api");
    let web = scenario.project_id("~/Projects/web");
    let served = scenario.serve("~/Projects");

    served
        .get("/scan?root=/home/you/Projects&depth=1")
        .is(200)
        .answers(json!({
            "depth": 1,
            "found": [
                {
                    "found": {
                        "agent_files": [],
                        "id": web,
                        "linked": false,
                        "mind": true,
                        "moved": null,
                        "path": "/home/you/Projects/web",
                        "reason": "id",
                        "route": "web",
                    },
                    "moved_from": null,
                    "offers": "link",
                    "why": "mind project with an id",
                },
                {
                    "found": {
                        "agent_files": [],
                        "id": api,
                        "linked": true,
                        "mind": true,
                        "moved": null,
                        "path": "/home/you/Projects/api",
                        "reason": "id",
                        "route": "api",
                    },
                    "moved_from": null,
                    "offers": null,
                    "why": null,
                },
            ],
            "roots": ["/home/you/Projects"],
            "truncated": false,
            "untraced": [],
            "warnings": [],
        }));
}

#[test]
fn folders_says_what_each_folder_inside_one_is_and_whether_it_is_linked() {
    let scenario = guide_workspace();
    scenario
        .folder("~/Projects/notes")
        .repository("~/Projects/site");
    let served = scenario.serve("~/Projects");

    served
        .get("/folders?path=/home/you/Projects")
        .is(200)
        .answers_at(
            "/folders",
            json!([
                {
                    "is": "project",
                    "linked": true,
                    "name": "api",
                    "path": "/home/you/Projects/api",
                    "symlink": false,
                },
                {
                    "is": "folder",
                    "linked": false,
                    "name": "notes",
                    "path": "/home/you/Projects/notes",
                    "symlink": false,
                },
                {
                    "is": "repository",
                    "linked": false,
                    "name": "site",
                    "path": "/home/you/Projects/site",
                    "symlink": false,
                },
                {
                    "is": "project",
                    "linked": false,
                    "name": "web",
                    "path": "/home/you/Projects/web",
                    "symlink": false,
                },
            ]),
        )
        .answers_at("/is", json!("workspace"));
}

#[test]
fn updates_says_what_an_update_of_every_project_would_write_and_what_it_leaves() {
    let scenario = linking(&["api", "web"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style 'How every repo here writes commits'",
        )
        .given("~/Projects", "mindweaver install -p api commit-style")
        .given("~/Projects", "mindweaver install -p web review")
        // The library's moved on since api's copy was made, and web's copy
        // was changed where it is.
        .file(
            "~/Projects/.mindweaver/library/skills/commit-style/SKILL.md",
            &added("commit-style", "How every repo here writes commits, now"),
        )
        .file(
            "~/Projects/web/.claude/skills/review/SKILL.md",
            &added("review", "Mine now"),
        );
    let served = scenario.serve("~/Projects");

    served.get("/updates").is(200).answers(json!({
        "projects": [
            {
                "entry": "api",
                "installations": [{
                    "copies": [],
                    "does": "refresh",
                    "forced": false,
                    "kind": "skill",
                    "left": null,
                    "name": "commit-style",
                    "origin": "workspace",
                    "state": "outdated",
                }],
                "name": "api",
                "writes": true,
            },
            {
                "entry": "web",
                "installations": [{
                    "copies": [".claude/skills/review"],
                    "does": "leave",
                    "forced": true,
                    "kind": "skill",
                    "left": {
                        "code": "edited",
                        "message": "skill `review` in web was changed since it was installed, \
                                    at .claude/skills/review, so an update leaves it alone, \
                                    every copy of it; forcing the update writes over the change",
                    },
                    "name": "review",
                    "origin": "workspace",
                    "state": "edited",
                }],
                "name": "web",
                "writes": false,
            },
        ],
        "states": [
            {"count": 1, "state": "outdated"},
            {"count": 1, "state": "edited"},
            {"count": 0, "state": "missing"},
            {"count": 0, "state": "orphaned"},
        ],
        "unread": [],
        "writes": true,
    }));
}

#[test]
fn workspaces_lists_the_one_served_when_the_machine_knows_no_other() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.get("/workspaces").is(200).answers(json!([{
        "default": false,
        "in_use": true,
        "launched": true,
        "name": "Projects",
        "path": "/home/you/Projects",
        "projects": 1,
        "reason": "above",
        "remembered": false,
        "stale": null,
        "stale_projects": 0,
        "switchable": false,
    }]));
}

#[test]
fn a_plan_of_ticked_boxes_is_the_tasks_that_carry_it_out() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/plan",
            json!({"selection": [
                {"project": "api", "reference": "skill:review", "origin": "workspace", "wanted": true},
            ]}),
        )
        .is(200)
        .answers(json!({
            "asks": [],
            "conflicts": [],
            "locked": [],
            "tasks": [{
                "from": "workspace",
                "kind": "skill",
                "name": "review",
                "projects": ["api"],
                "type": "install",
                "yes": false,
            }],
        }));
}

#[test]
fn a_plan_says_a_box_it_may_not_change_is_locked_and_plans_the_rest() {
    let scenario = linking(&["api", "web"]);
    scenario
        .given("~/Projects", "mindweaver install -p web review")
        .file(
            "~/Projects/web/.claude/skills/review/SKILL.md",
            &added("review", "Mine now"),
        );
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/plan",
            json!({"selection": [
                {"project": "web", "reference": "skill:review", "origin": "workspace", "wanted": false},
                {"project": "api", "reference": "skill:review", "origin": "workspace", "wanted": true},
            ]}),
        )
        .is(200)
        .answers(json!({
            "conflicts": [],
            "locked": [{
                "cell": {
                    "origin": "workspace",
                    "project": "web",
                    "reference": "skill:review",
                    "wanted": false,
                },
                "why": {
                    "code": "edited",
                    "message": "`review` in web was changed since it was installed, so it is \
                                left alone unless a change to it is forced",
                },
            }],
            "tasks": [{
                "from": "workspace",
                "kind": "skill",
                "name": "review",
                "projects": ["api"],
                "type": "install",
                "yes": false,
            }],
            "asks": [],
        }));
}

#[test]
fn a_draft_is_judged_as_the_command_would_refuse_it_with_nothing_written() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/validate",
            json!({
                "holder": {"type": "workspace"},
                "draft": {
                    "type": "add", "kind": "skill", "name": "Review",
                    "description": REVIEW, "template": null,
                },
            }),
        )
        .is(200)
        .answers(json!({
            "issues": [],
            "refused": {
                "code": "add_name_unfit",
                "message": "`Review` is not usable as a name: it has to be lowercase letters \
                            and digits, joined by single hyphens",
            },
        }));
    // By the names the folder lists, not by asking for `Review`, which a
    // file system that folds case, as macOS's does, finds as the `review`
    // already there.
    let skills = std::fs::read_dir(scenario.at("~/Projects/.mindweaver/library/skills")).unwrap();
    let names: Vec<_> = skills.map(|entry| entry.unwrap().file_name()).collect();
    assert!(!names.iter().any(|name| name == "Review"), "{names:?}");
}

#[test]
fn a_draft_that_breaks_no_rule_is_judged_fit() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/validate",
            json!({
                "holder": {"type": "workspace"},
                "draft": {
                    "type": "add", "kind": "skill", "name": "deploy",
                    "description": "Ship the service to staging", "template": null,
                },
            }),
        )
        .is(200)
        .answers(json!({"issues": [], "refused": null}));
    assert!(!scenario.exists("~/Projects/.mindweaver/library/skills/deploy"));
}
