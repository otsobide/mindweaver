//! Looking after projects and loads through the API, as the panel's pages
//! do: forgetting a project, following one that moved, making a folder a
//! project and linking it, moving one an older build made, changing
//! its agents, its share and its targets, loading another workspace and
//! forgetting it, and adopting a skill somebody keeps.

use serde_json::{json, Value};

use crate::{entries, linking, Scenario};

/// What a run of one line answered, the line being `line`.
fn ran_one(line: &str, stdout: &str, stderr: Value) -> Value {
    json!({
        "lines": [line],
        "outcome": {
            "ok": true,
            "runs": [{
                "command": line,
                "error": null,
                "ok": true,
                "stderr": stderr,
                "stdout": stdout,
            }],
        },
    })
}

#[test]
fn unlinking_a_project_says_first_that_the_workspace_forgets_it() {
    let scenario = Scenario::projects(&["api", "web"]);
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks/preview", json!({"type": "unlink", "entry": "web"}))
        .is(200)
        .answers(json!({
            "lines": ["mindweaver -W /home/you/Projects unlink web"],
            "confirm": [{
                "caution": "unlinks",
                "line": 0,
                "message": "makes the workspace forget a project, though nothing on disk is deleted",
            }],
            "cancellable": false,
        }));
    assert_eq!(entries(&served.get("/projects")), ["api", "web"]);
}

#[test]
fn unlinking_a_project_forgets_it_and_leaves_its_folder_as_it_is() {
    let scenario = Scenario::projects(&["api", "web"]);
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks", json!({"type": "unlink", "entry": "web"}))
        .is(200)
        .answers(ran_one(
            "mindweaver -W /home/you/Projects unlink web",
            "unlinked web\n",
            json!([]),
        ));
    assert_eq!(entries(&served.get("/projects")), ["api"]);
    assert!(scenario.exists("~/Projects/web/.mind/settings.toml"));
}

#[test]
fn a_project_that_moved_is_relinked_with_what_was_installed_in_it() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    std::fs::rename(
        scenario.at("~/Projects/api"),
        scenario.at("~/Projects/service"),
    )
    .expect("move the project by hand");
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({"type": "relink", "old": "api", "new": "/home/you/Projects/service"}),
        )
        .is(200)
        .answers(ran_one(
            "mindweaver -W /home/you/Projects relink api /home/you/Projects/service",
            "relinked api as service, from api\n  1 install kept with it\n",
            json!([]),
        ));
    served
        .get("/project?entry=service")
        .is(200)
        .answers_at("/health", json!("ok"))
        .answers_at("/artifacts/0/state", json!("current"));
}

#[test]
fn a_repository_is_made_a_project_and_linked_in_one_task() {
    let scenario = Scenario::projects(&["api"]);
    scenario.repository("~/Projects/site");
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({"type": "init_and_link", "path": "/home/you/Projects/site"}),
        )
        .is(200)
        .answers_at(
            "/lines",
            json!([
                "mind -W /home/you/Projects -C /home/you/Projects/site init",
                "mindweaver -W /home/you/Projects link /home/you/Projects/site",
            ]),
        )
        .answers_at("/outcome/runs/1/stdout", json!("linked site as site\n"));
    assert_eq!(entries(&served.get("/projects")), ["api", "site"]);
}

#[test]
fn a_project_an_older_build_made_is_moved_to_settings_only_when_asked() {
    let scenario = Scenario::projects(&["api"]);
    scenario
        .older_project("~/Projects/old")
        .given("~/Projects", "mindweaver link old");
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks/preview",
            json!({"type": "migrate", "project": "old", "yes": false}),
        )
        .is(200)
        .answers_at(
            "/lines",
            json!(["mindweaver -W /home/you/Projects migrate -p /home/you/Projects/old"]),
        );
    served
        .post(
            "/tasks",
            json!({"type": "migrate", "project": "old", "yes": true}),
        )
        .is(200)
        .answers(ran_one(
            "mindweaver -W /home/you/Projects migrate -p /home/you/Projects/old --yes",
            "wrote .mind/settings.toml for old: agents claude,codex,opencode, share shared\n\
             moved .mind/mind.toml to .mind/tmp/mind.toml.v2, as it was\n",
            json!([]),
        ));
    assert!(scenario.exists("~/Projects/old/.mind/settings.toml"));
    assert!(scenario.exists("~/Projects/old/.mind/tmp/mind.toml.v2"));
    served
        .get("/project?entry=old")
        .is(200)
        .answers_at("/marker", json!("settings.toml"))
        .answers_at("/migrates", json!(false));
}

#[test]
fn a_projects_settings_form_runs_its_agents_line_then_its_share_line() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    let served = scenario.serve("~/Projects");

    let answer = served.post(
        "/tasks",
        json!({
            "type": "settings",
            "holder": {"type": "project", "entry": "api"},
            "agents": {"type": "named", "agents": ["claude"]},
            "share": "shared",
            "edit_tracked": false,
        }),
    );

    answer.is(200).answers_at(
        "/lines",
        json!([
            "mind -W /home/you/Projects -C /home/you/Projects/api agents --yes claude",
            "mind -W /home/you/Projects -C /home/you/Projects/api share --yes shared",
        ]),
    );
    let stdout: Vec<Value> = answer.json()["outcome"]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|run| run["stdout"].clone())
        .collect();
    assert_eq!(
        stdout,
        [
            json!(
                "set agents to claude in api, where they were claude,codex,opencode\n\
                   removed review from .agents/skills/review\n"
            ),
            json!(
                "set share to shared in api, where it was local\n\
                   removed .mind/.gitignore, so .mind can be committed\n\
                   stopped keeping /.claude/skills/review/ out of git\n"
            ),
        ]
    );
    served.get("/project?entry=api").is(200).answers_at(
        "/settings",
        json!({"agents": ["claude"], "edit_tracked": false, "share": "shared", "targets": []}),
    );
}

#[test]
fn targets_set_from_the_panel_say_their_note_beside_what_they_moved() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({
                "type": "targets",
                "holder": {"type": "project", "entry": "api"},
                "kind": "skill",
                "folders": [".skills"],
            }),
        )
        .is(200)
        .answers(ran_one(
            "mind -W /home/you/Projects -C /home/you/Projects/api targets set --yes skills .skills",
            "set the targets of skills to .skills in api, where they were the folders its agents read\n\
             copied review to .skills/review, for claude, codex and opencode, kept out of git\n\
             removed review from .agents/skills/review\n\
             removed review from .claude/skills/review\n",
            json!([{
                "code": null,
                "text": "note: claude, codex and opencode read none of the folders api installs \
                         skills in (.skills), so none of them sees what is installed there until \
                         pointed at one",
            }]),
        ));
    assert_eq!(
        scenario.files("~/Projects/api/.skills"),
        ["review/SKILL.md"]
    );
}

#[test]
fn targets_reset_from_the_panel_send_the_copies_back_where_each_agent_reads() {
    let scenario = linking(&["api"]);
    scenario.given("~/Projects", "mindweaver install -p api review");
    // Set as a step: it says a note, which a given step may not.
    scenario
        .run("~/Projects/api", "mind targets set skills .skills --yes")
        .succeeds();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({
                "type": "targets",
                "holder": {"type": "project", "entry": "api"},
                "kind": "skill",
                "folders": null,
            }),
        )
        .is(200)
        .answers_at(
            "/lines",
            json!([
                "mind -W /home/you/Projects -C /home/you/Projects/api targets reset --yes skills"
            ]),
        )
        .answers_at("/outcome/ok", json!(true));
    assert!(!scenario.exists("~/Projects/api/.skills/review"));
    assert_eq!(
        scenario.files("~/Projects/api/.claude/skills"),
        ["review/SKILL.md"]
    );
    assert_eq!(
        scenario.files("~/Projects/api/.agents/skills"),
        ["review/SKILL.md"]
    );
}

#[test]
fn loading_a_workspace_offers_its_own_to_every_project() {
    let scenario = Scenario::projects(&["api"]);
    scenario
        .folder("~/src/team")
        .given("~/src/team", "mindweaver init")
        .given(
            "~/src/team",
            "mindweaver add skill deploy 'Ship the service to staging'",
        );
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks", json!({"type": "load", "path": "/home/you/src/team"}))
        .is(200)
        .answers(ran_one(
            "mindweaver -W /home/you/Projects load /home/you/src/team",
            "loaded team as ../src/team: 1 skill to install\n  into the workspace at /home/you/Projects\n",
            json!([]),
        ));
    let offers: Vec<Value> = served.get("/snapshot").json()["offers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|offer| json!({"name": offer["name"], "origin": offer["origin"]}))
        .collect();
    assert_eq!(
        offers,
        [json!({"name": "deploy", "origin": "load:../src/team"})]
    );
}

#[test]
fn unloading_a_workspace_forgets_it_by_its_entry() {
    let scenario = Scenario::projects(&["api"]);
    scenario
        .folder("~/src/team")
        .given("~/src/team", "mindweaver init")
        .given(
            "~/src/team",
            "mindweaver add skill deploy 'Ship the service to staging'",
        )
        .given("~/Projects", "mindweaver load ../src/team");
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks", json!({"type": "unload", "entry": "../src/team"}))
        .is(200)
        .answers(ran_one(
            "mindweaver -W /home/you/Projects unload ../src/team",
            "unloaded ../src/team\n  from the workspace at /home/you/Projects\n",
            json!([]),
        ));
    served
        .get("/snapshot")
        .is(200)
        .answers_at("/loaded", json!([]))
        .answers_at("/offers", json!([]));
}

#[test]
fn unloading_a_workspace_that_is_not_loaded_is_refused_before_it_runs() {
    let scenario = Scenario::projects(&["api"]);
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks", json!({"type": "unload", "entry": "../src/team"}))
        .refused(
            422,
            "not_loaded",
            "../src/team: not loaded by the workspace at /home/you/Projects",
        );
    served.get("/transcript").is(200).answers(json!([]));
}

#[test]
fn adopting_from_the_panel_brings_the_skill_into_the_library_at_once() {
    let scenario = Scenario::projects(&["api"]);
    scenario.file(
        "~/Projects/api/.claude/skills/notes/SKILL.md",
        "---\nname: notes\ndescription: My own notes\n---\n",
    );
    let served = scenario.serve("~/Projects");

    // The panel asked before it sent this, so the line says `--yes`.
    served
        .post(
            "/tasks",
            json!({"type": "adopt", "project": "api", "kind": "skill", "name": "notes"}),
        )
        .is(200)
        .answers(ran_one(
            "mindweaver -W /home/you/Projects adopt -p /home/you/Projects/api --yes skill:notes",
            "copied notes from api into the library, at /home/you/Projects/.mindweaver/library/skills/notes\n\
             recorded notes in api as installed from workspace, at .claude/skills/notes\n\
             recorded .agents/skills/notes missing: `mindweaver update -p api` puts it there\n\
             kept .claude/skills/notes out of git\n",
            json!([]),
        ));
    served
        .get("/project?entry=api")
        .is(200)
        .answers_at("/artifacts/0/state", json!("missing"))
        .answers_at("/artifacts/0/origin", json!("workspace"));
}
