//! A value typed into a task reaches its command as that value, end to end:
//! the line a front end builds from it (`Task::commands`) is run, and what
//! the command wrote is read back. The task module's own tests hold the
//! lines to the parser; these hold what the commands then do with what
//! they were given, for values drawn at random, led by the shapes of
//! options as often as not.

mod common;

use common::mother::ValueMother;
use common::*;
use mindweaver_core::mother::ArtifactNameMother;
use serde_json::Value;

/// What `list --json` says each artifact `holder` holds of its own is
/// summarised by, by its kind and name, from `root`.
fn summaries(root: &Path, holder: &Holder) -> Vec<((String, String), Value)> {
    let (listing, artifacts) = match holder {
        Holder::Workspace => (weaver(root, &["list", "--json"]), "library"),
        _ => (mind(root, &["list", "--json"]), "projects"),
    };
    let (data, warnings): (Value, _) = enveloped(&listing.unwrap());
    assert!(warnings.is_empty(), "{warnings:?}");
    let artifacts = match artifacts {
        "library" => &data["library"]["artifacts"],
        _ => &data["projects"][0]["artifacts"],
    };
    artifacts
        .as_array()
        .unwrap()
        .iter()
        .map(|artifact| {
            (
                (
                    artifact["kind"].as_str().unwrap().to_owned(),
                    artifact["name"].as_str().unwrap().to_owned(),
                ),
                artifact["summary"].clone(),
            )
        })
        .collect()
}

/// The kinds a task's `add` writes from a name and a description alone
/// (`create::scaffolds`): an MCP server needs how it is started or reached
/// too, which no task carries.
const ADDED: [Kind; 2] = [Kind::Skill, Kind::Rule];

/// A name no artifact `holder` holds goes by yet.
fn fresh(kind: Kind, taken: &[(String, String)]) -> String {
    loop {
        let name = match kind {
            Kind::Skill | Kind::Mcp => ArtifactNameMother::random(),
            Kind::Rule => ArtifactNameMother::route(),
        }
        .into_string();
        if !taken.contains(&(kind.slug().to_owned(), name.clone())) {
            return name;
        }
    }
}

/// A project in a folder of its own and a workspace in another, each with
/// the holder a task names it by from its root.
fn places() -> Vec<(TempDir, Holder)> {
    let project = TempDir::new().unwrap();
    new_project(project.path());
    let workspace = TempDir::new().unwrap();
    weaver(workspace.path(), &["init"]).unwrap();
    vec![
        (project, Holder::Project(String::from("."))),
        (workspace, Holder::Workspace),
    ]
}

/// What a description reads back as: as typed, without the space around
/// it, which `add` does not keep.
fn read_back(typed: &str) -> Value {
    Value::from(typed.trim())
}

#[test]
fn a_description_reaches_the_file_as_it_was_typed() {
    for (dir, holder) in places() {
        let mut added: Vec<(String, String)> = Vec::new();
        for at in 0..12 {
            let kind = ADDED[at % ADDED.len()];
            let name = fresh(kind, &added);
            let description = ValueMother::random();
            let task = Task::Add {
                holder: holder.clone(),
                kind,
                name: name.clone(),
                description: description.clone(),
                template: None,
                server: None,
            };
            let lines = task.commands();

            let outcome = typed(dir.path(), &lines[0])
                .unwrap_or_else(|failure| panic!("{lines:?}: {}", failure.error));

            assert!(outcome.ok, "{lines:?}: {:?}", outcome.stderr);
            added.push((kind.slug().to_owned(), name.clone()));
            let listed = summaries(dir.path(), &holder);
            let summary = listed
                .iter()
                .find(|(key, _)| *key == (kind.slug().to_owned(), name.clone()))
                .map(|(_, summary)| summary.clone());
            assert_eq!(
                summary,
                Some(read_back(&description)),
                "{lines:?}: {listed:?}"
            );
            if kind == Kind::Skill {
                let reference = format!("skill:{name}");
                let shown = match &holder {
                    Holder::Workspace => weaver(dir.path(), &["show", &reference, "--json"]),
                    _ => mind(dir.path(), &["show", &reference, "--json"]),
                };
                let (shown, _): (Vec<Value>, _) = enveloped(&shown.unwrap());
                assert_eq!(
                    shown[0]["description"],
                    read_back(&description),
                    "{lines:?}"
                );
            }
        }
        assert_eq!(summaries(dir.path(), &holder).len(), added.len());
    }
}

#[test]
fn a_template_named_by_a_value_is_looked_for_by_that_name_and_nothing_is_added() {
    for (dir, holder) in places() {
        for kind in Kind::ALL.into_iter().cycle().take(8) {
            // Never the one template every level has.
            let template = std::iter::repeat_with(ValueMother::random)
                .find(|template| template.trim() != "full")
                .unwrap();
            let task = Task::Add {
                holder: holder.clone(),
                kind,
                name: fresh(kind, &[]),
                description: String::from("Does it"),
                template: Some(template.clone()),
                server: None,
            };
            let lines = task.commands();

            let failure = typed(dir.path(), &lines[0]).unwrap_err();

            // Either way by the value as typed: one folder's name is looked
            // for, and anything else refused as no template's name.
            let (code, said) = if mindweaver_core::install::is_folder_name(&template) {
                (
                    "unknown_template",
                    format!("no {} template called `{template}`", kind.slug()),
                )
            } else {
                (
                    "template_name_invalid",
                    format!(
                        "`{template}` is not a template name: a template is one folder, \
                         named with no `/`"
                    ),
                )
            };
            assert_eq!(failure.error.code(), code, "{lines:?}");
            assert!(
                failure.error.to_string().starts_with(&said),
                "{lines:?}: {}",
                failure.error
            );
        }
        assert!(summaries(dir.path(), &holder).is_empty());
    }
}

#[cfg(feature = "ui")]
mod panel {
    //! The same, as the panel sends a task: the JSON a browser fills in, run
    //! by the backend with the launch's lookup and the workspace named.

    use super::*;
    use mindweaver_cli::backend::PanelBackend;
    use mindweaver_server::{Backend, Underway};
    use serde_json::json;

    #[test]
    fn a_description_the_panel_sends_reaches_the_file_as_it_was_typed() {
        let dir = workspace_with_two();
        let alpha = dir.path().join("alpha");
        let backend = PanelBackend::launch(dir.path(), &Lookup::default()).unwrap();
        let mut added: Vec<(String, String)> = Vec::new();

        for at in 0..8 {
            let kind = ADDED[at % ADDED.len()];
            let (holder, root, read_as) = if at % 4 < 2 {
                (json!({"type": "workspace"}), dir.path(), Holder::Workspace)
            } else {
                (
                    json!({"type": "project", "entry": "alpha"}),
                    alpha.as_path(),
                    Holder::Project(String::from(".")),
                )
            };
            let name = fresh(kind, &added);
            let description = ValueMother::random();
            let sent = json!({
                "type": "add",
                "holder": holder,
                "kind": kind.slug(),
                "name": name,
                "description": description,
                "template": null,
            });
            let task: Task = serde_json::from_value(sent.clone()).unwrap();

            let ran = Backend::run(&backend, task, &Underway::unfollowed()).unwrap();

            assert!(ran.ok, "{sent}: {}", serde_json::to_string(&ran).unwrap());
            added.push((kind.slug().to_owned(), name.clone()));
            let listed = summaries(root, &read_as);
            assert!(
                listed.contains(&(
                    (kind.slug().to_owned(), name.clone()),
                    read_back(&description)
                )),
                "{sent}: {listed:?}"
            );
        }
    }

    #[test]
    fn a_name_the_panel_sends_led_by_a_dash_is_judged_as_a_name_and_nothing_is_renamed() {
        let dir = workspace_with_own();
        let backend = PanelBackend::launch(dir.path(), &Lookup::default()).unwrap();
        let before = summaries(dir.path(), &Holder::Workspace);

        for _ in 0..8 {
            let to = ValueMother::dashed();
            let sent = json!({
                "type": "rename",
                "holder": {"type": "workspace"},
                "kind": "skill",
                "from": "commit-style",
                "to": to,
            });
            let task: Task = serde_json::from_value(sent.clone()).unwrap();

            let ran = Backend::run(&backend, task, &Underway::unfollowed()).unwrap();

            assert!(!ran.ok, "{sent}");
            let error = ran.runs[0].error.as_ref().expect("why it did not run");
            assert_eq!(
                serde_json::to_value(error).unwrap()["code"],
                "rename_name_unfit",
                "{sent}"
            );
        }
        assert_eq!(summaries(dir.path(), &Holder::Workspace), before);
    }
}
