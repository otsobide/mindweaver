//! What a script reads: every reading command's `--json`, one envelope
//! for all of them, `data` what the text lists, `warnings` and
//! `coded_warnings` what it says on stderr, and the text's exit status:
//! the reference's "Machine-readable output".

use serde_json::{json, Value};

use crate::{skill, Ran, Scenario};

/// What `ran` printed, read as the envelope, held to say nothing on
/// stderr and to warn of nothing in it: what every clean read prints.
fn clean(ran: &Ran) -> Value {
    ran.succeeds().quiet();
    let envelope: Value = serde_json::from_str(&ran.stdout).expect("--json prints JSON");
    assert_eq!(envelope["version"], 2, "{envelope:#}");
    assert_eq!(envelope["warnings"], json!([]), "{envelope:#}");
    assert_eq!(envelope["coded_warnings"], json!([]), "{envelope:#}");
    envelope["data"].clone()
}

/// `~/Projects` linking `collapse`, holding `commit-style` from the
/// library and `deploy`, its own.
fn collapse_holding_two() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill commit-style \"How every repo here writes commits\"",
        )
        .given("~/Projects", "mindweaver install -p collapse commit-style")
        .given(
            "~/Projects/collapse",
            "mind add skill deploy \"Ship the service to staging\"",
        );
    scenario
}

#[test]
fn validate_json_says_each_artifact_checked_and_what_could_not_be_read() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects/collapse",
            "mind add rule git/no-force-push \"Never force-push a shared branch\"",
        )
        .file(
            "~/Projects/collapse/skills/broken/SKILL.md",
            "# broken\n\nIts front matter is gone.\n",
        );
    let warning = "/home/you/Projects/collapse/skills/broken/SKILL.md: the file does not \
                   start with a `---` front matter fence";

    let ran = scenario.run("~/Projects/collapse", "mind validate --json");

    ran.exits(1).says(&format!("warning: {warning}\n"));
    let envelope: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        envelope,
        json!({
            "version": 2,
            "data": {
                "artifacts": [{
                    "kind": "rule",
                    "name": "git/no-force-push",
                    "path": "/home/you/Projects/collapse/rules/git/no-force-push.md",
                    "project": "/home/you/Projects/collapse",
                    "issues": [],
                    "invalid": false,
                    "copies_differ": false,
                }],
                "invalid": 0,
                "unreadable": 1,
                "differing": 0,
            },
            "warnings": [warning],
            "coded_warnings": [{"code": "front_matter_missing", "message": warning}],
        })
    );
}

#[test]
fn list_json_reads_a_project_as_every_front_end_reads_it() {
    let scenario = collapse_holding_two();
    let id = scenario.project_id("~/Projects/collapse");

    let data = clean(&scenario.run("~/Projects/collapse", "mind list --json"));

    assert_eq!(data["library"], Value::Null);
    let project = &data["projects"][0];
    assert_eq!(project["entry"], "collapse");
    assert_eq!(project["id"], id);
    assert_eq!(project["health"], "ok");
    assert_eq!(
        project["artifacts"],
        json!([
            {
                "kind": "skill", "name": "commit-style",
                "summary": "How every repo here writes commits", "state": "current",
                "agents": ["claude", "codex", "opencode"], "origin": "workspace",
                "targets": [".agents/skills/commit-style", ".claude/skills/commit-style"],
                "issues": [], "unread": null,
            },
            {
                "kind": "skill", "name": "deploy", "summary": "Ship the service to staging",
                "state": "authored", "agents": [], "origin": null,
                "targets": ["skills/deploy"], "issues": [], "unread": null,
            },
        ])
    );
    assert_eq!(project["tally"]["total"], 2);
}

#[test]
fn mindweaver_list_json_names_the_projects_holding_each_of_the_librarys_own() {
    let scenario = collapse_holding_two();

    let data = clean(&scenario.run("~/Projects", "mindweaver list --json"));

    assert_eq!(data["library"]["origin"], "workspace");
    assert_eq!(data["library"]["artifacts"][0]["name"], "commit-style");
    assert_eq!(
        data["library"]["holders"],
        json!([{"kind": "skill", "name": "commit-style", "entry": "collapse"}])
    );
    assert_eq!(data["projects"][0]["entry"], "collapse");
}

#[test]
fn show_json_holds_the_artifact_its_front_matter_and_its_body() {
    let scenario = collapse_holding_two();

    let data = clean(&scenario.run("~/Projects/collapse", "mind show deploy --json"));

    assert_eq!(
        data,
        json!([{
            "kind": "skill",
            "name": "deploy",
            "path": "/home/you/Projects/collapse/skills/deploy",
            "project": "/home/you/Projects/collapse",
            "file": "/home/you/Projects/collapse/skills/deploy/SKILL.md",
            "summary": "Ship the service to staging",
            "description": "Ship the service to staging",
            "allowed_tools": null,
            "license": null,
            "compatibility": null,
            "metadata": {},
            "paths": null,
            "transport": null,
            "inputs": null,
            "expressibility": null,
            "body": "\n# deploy\n",
        }])
    );
}

#[test]
fn templates_json_says_each_template_and_where_it_comes_from() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.file(
        "~/Projects/collapse/.mind/templates/runbook/SKILL.md",
        &skill("runbook", "A runbook"),
    );

    let data = clean(&scenario.run("~/Projects/collapse", "mind templates --json"));

    assert_eq!(
        data,
        json!([
            {"name": "full", "kind": "skill", "origin": null},
            {
                "name": "runbook",
                "kind": "skill",
                "origin": "/home/you/Projects/collapse/.mind/templates/runbook",
            },
        ])
    );
}

#[test]
fn workspaces_json_lists_the_weavers_this_machine_knows() {
    let scenario = Scenario::projects(&["collapse"]);

    let data = clean(&scenario.run("~/Projects", "mindweaver workspaces --json"));

    assert_eq!(
        data,
        json!({"workspaces": [{
            "name": "Projects",
            "path": "/home/you/Projects",
            "default": false,
            "remembered": false,
            "in_use": true,
            "projects": 1,
            "stale_projects": 0,
            "stale": null,
        }]})
    );
}

#[test]
fn scan_json_says_what_it_found_under_the_workspace_and_what_is_linked() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .repository("~/Projects/web")
        .given("~/Projects/web", "mind init");
    let collapse = scenario.project_id("~/Projects/collapse");
    let web = scenario.project_id("~/Projects/web");

    let data = clean(&scenario.run("~/Projects", "mindweaver scan --json"));

    assert_eq!(
        data,
        json!({
            "roots": ["/home/you/Projects"],
            "depth": 3,
            "found": [
                {
                    "path": "/home/you/Projects/collapse", "route": "collapse", "id": collapse,
                    "mind": true, "linked": true, "moved": null, "reason": "id",
                    "agent_files": [],
                },
                {
                    "path": "/home/you/Projects/web", "route": "web", "id": web,
                    "mind": true, "linked": false, "moved": null, "reason": "id",
                    "agent_files": [],
                },
            ],
            "untraced": [],
            "truncated": false,
        })
    );
}

#[test]
fn gather_list_json_says_where_each_offer_comes_from_as_from_takes_it() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .source(
            "~/src/acme-skills",
            &[("skills/review/SKILL.md", &skill("review", "Read a change"))],
        )
        .given("~/Projects", "mindweaver gather git ../src/acme-skills");
    let revision = scenario.commit("~/src/acme-skills");

    let mut data = clean(&scenario.run("~/Projects", "mindweaver gather list --json"));

    // When it was gathered, which no run makes the same.
    let gathered = data[0]
        .as_object_mut()
        .unwrap()
        .remove("gathered_at")
        .expect("when it was gathered");
    assert!(gathered.is_u64(), "{gathered}");
    assert_eq!(
        data,
        json!([{
            "place": "shelf",
            "kind": "skill",
            "name": "review",
            "path": ".mindweaver/skills/..-src-acme-skills/review",
            "summary": "Read a change",
            "revision": revision,
            "source": {
                "kind": "git",
                "url": "../src/acme-skills",
                "reference": null,
                "subdirectory": "skills",
                "revision": revision,
                "directory": "..-src-acme-skills",
                "name": "src-acme-skills",
            },
            "origin": "../src/acme-skills",
        }])
    );
}
