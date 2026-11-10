//! A request the API cannot take, and what it answers instead: the guide's
//! "When something goes wrong". Every refusal is `{"error", "code"}`, a 400
//! for a request the route cannot read, a 404 for a read naming what is not
//! there, a 422 for what Mindweaver will not do, and a refused task runs
//! nothing.

use serde_json::json;

use crate::{guide_workspace, Ask, Scenario};

#[test]
fn a_read_left_without_a_parameter_it_needs_says_which() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.get("/project").refused(
        400,
        "missing_parameter",
        "/api/v1/project needs `entry`, and was sent none",
    );
}

#[test]
fn a_parameter_the_route_does_not_take_is_refused_by_its_name() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.get("/projects?entry=api").refused(
        400,
        "unknown_parameter",
        "/api/v1/projects takes no parameter, and was sent `entry`",
    );
}

#[test]
fn a_value_its_parameter_cannot_read_is_refused() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.get("/scan?depth=deep").refused(
        400,
        "malformed_parameter",
        "/api/v1/scan takes `depth` as a whole number, not `deep`",
    );
}

#[test]
fn a_project_the_workspace_does_not_link_is_not_found() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.get("/project?entry=blog").refused(
        404,
        "not_registered",
        "blog: not registered in the workspace at /home/you/Projects",
    );
}

#[test]
fn a_file_the_artifact_does_not_hold_is_not_found() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .get("/contents?reference=skill:review&file=notes.md")
        .refused(
            404,
            "contents_missing",
            "/home/you/Projects/.mindweaver/library/skills/review/notes.md: no such file",
        );
}

#[test]
fn a_method_the_route_does_not_take_is_refused() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served.send(Ask::at("DELETE", "/api/v1/snapshot")).refused(
        405,
        "method_not_allowed",
        "/api/v1/snapshot does not take DELETE",
    );
}

#[test]
fn a_body_not_said_to_be_json_is_refused() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .send(
            Ask::post(
                "/tasks/preview",
                json!({"type": "link", "path": "/home/you/Projects/web"}),
            )
            .untyped(),
        )
        .refused(
            415,
            "json_required",
            "Expected request with `Content-Type: application/json`",
        );
}

#[test]
fn a_body_that_is_not_json_is_refused() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .send(Ask::post_text("/tasks/preview", "link web"))
        .refused(
            400,
            "malformed_json",
            "Failed to parse the request body as JSON: expected value at line 1 column 1",
        );
}

#[test]
fn a_task_nobody_knows_is_refused_with_every_one_there_is() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks/preview", json!({"type": "deploy"}))
        .refused(
            422,
            "invalid_body",
            "Failed to deserialize the JSON body into the target type: type: unknown variant \
             `deploy`, expected one of `init_workspace`, `init_project`, `link`, `link_here`, \
             `init_and_link`, `unlink`, `relink`, `load`, `load_here`, `unload`, \
             `unload_here`, `add`, `show`, `edit`, `save`, `edit_server`, `rename`, `remove`, \
             `validate`, `minds`, `marks`, `shelf`, `workspaces`, `gather`, `export`, `backup`, `targets`, \
             `settings`, `toggled`, `update`, `install`, `uninstall`, `uninstall_orphaned`, \
             `adopt`, `migrate`, `open_install`, `apply_install`, `refresh` at line 1 column 16",
        );
}

#[test]
fn a_folder_named_by_a_relative_path_is_refused_and_nothing_runs() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks", json!({"type": "link", "path": "web"}))
        .refused(
            422,
            "path_relative",
            "`web` is not an absolute path: name a folder by its absolute path, which reads \
             the same from anywhere",
        );
    served.get("/transcript").is(200).answers(json!([]));
}

#[test]
fn a_folder_inside_a_mindweaver_is_refused() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks/preview",
            json!({"type": "link", "path": "/home/you/Projects/.mindweaver/library"}),
        )
        .refused(
            422,
            "path_in_mindflayer",
            "`/home/you/Projects/.mindweaver/library` is inside a `.mindweaver`, which holds a \
             workspace's own files: name a folder outside it",
        );
}

#[test]
fn a_task_that_opens_an_editor_is_refused_since_the_server_has_no_terminal() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks/preview",
            json!({"type": "edit", "holder": {"type": "workspace"}, "kind": "skill", "name": "review"}),
        )
        .refused(
            422,
            "opens_editor",
            "`mindweaver edit skill:review` opens an editor, which needs a terminal to draw on",
        );
}

#[test]
fn showing_an_artifact_whole_is_refused_for_reading_its_files_one_at_a_time() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks/preview",
            json!({"type": "show", "holder": {"type": "workspace"}, "reference": "skill:review"}),
        )
        .refused(
            422,
            "show_unheld",
            "`mindweaver show skill:review` prints an artifact whole, read through any link on the \
             way, where a front end without a terminal reads one file at a time, held to its \
             holder",
        );
}

/// A folder to gather from is named by a path that reads the same from
/// anywhere, as a folder to link is, and nothing is gathered.
#[test]
fn a_gather_from_a_folder_named_from_here_is_refused_and_nothing_is_gathered() {
    let scenario = Scenario::projects(&["api"]);
    scenario.source(
        "~/src/acme-skills",
        &[(
            "pdf/SKILL.md",
            "---\nname: pdf\ndescription: Read PDFs\n---\n",
        )],
    );
    let served = scenario.serve("~/Projects");

    served
        .post(
            "/tasks",
            json!({"type": "gather", "url": "../src/acme-skills", "folder": "", "reference": null}),
        )
        .refused(
            422,
            "path_relative",
            "`../src/acme-skills` is not an absolute path: name a folder by its absolute path, \
             which reads the same from anywhere",
        );
    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "nothing gathered yet\n",
            "  take some with `mindweaver gather git <url>`, or offer a workspace's own with \
             `mindweaver load <path>`\n",
        ));
}

/// A cancel names a run while it runs: one long over is not stopped, and
/// one that takes no cancel is not asked to.
#[test]
fn a_cancel_of_a_run_that_is_over_is_refused_as_not_running() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");
    served
        .post(
            "/tasks",
            json!({"type": "link", "path": "/home/you/Projects/web"}),
        )
        .is(200);

    served.post("/tasks/cancel", json!({"run": 1})).refused(
        409,
        "not_running",
        "run 1 is not under way, and nothing is: a run is stopped only while it runs",
    );
}

#[test]
fn the_install_screen_is_refused_since_it_draws_on_a_terminal() {
    let scenario = guide_workspace();
    let served = scenario.serve("~/Projects");

    served
        .post("/tasks/preview", json!({"type": "open_install"}))
        .refused(
            422,
            "opens_screen",
            "`mindweaver install` opens a screen of its own, which needs a terminal to draw on",
        );
}
