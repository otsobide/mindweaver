//! Unit tests for `export.rs`, kept beside it rather than inside it.
//!
//! What an export writes, and what it compares with what the folder held;
//! the yes it waits for before taking anything away; and each folder and
//! definition it refuses, before a byte is written.

use super::*;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{McpMother, ProjectMother, RuleMother, SkillMother, WorkspaceMother};

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// The folder `workspace` keeps its own of `kind` in.
fn library(workspace: &WeaverWorkspace, kind: Kind) -> PathBuf {
    workspace.own().directory_for(kind).unwrap()
}

/// A skill `name` in `workspace`'s library.
fn skill(workspace: &WeaverWorkspace, name: &str) -> PathBuf {
    let folder = library(workspace, Kind::Skill).join(name);
    write(
        &folder.join("SKILL.md"),
        &SkillMother::create(name, &SkillMother::description()),
    );
    folder
}

/// A rule at `route` in `workspace`'s library.
fn rule(workspace: &WeaverWorkspace, route: &str) -> PathBuf {
    let file = library(workspace, Kind::Rule).join(format!("{route}.md"));
    write(&file, &RuleMother::create(route));
    file
}

/// An MCP server `name` in `workspace`'s library, holding `definition`.
fn server(workspace: &WeaverWorkspace, name: &str, definition: &str) -> PathBuf {
    let file = library(workspace, Kind::Mcp).join(format!("{name}.toml"));
    write(&file, definition);
    file
}

/// Each artifact a plan reaches, as `kind:name change`.
fn said(plan: &Plan) -> Vec<String> {
    plan.artifacts
        .iter()
        .map(|step| format!("{}:{} {}", step.kind.slug(), step.name, step.change.done()))
        .collect()
}

#[test]
fn an_export_into_a_new_folder_makes_a_workspace_named_after_the_one_exported() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let name = random::kebab();
    skill(&workspace, &name);
    rule(&workspace, "git/commits");
    server(&workspace, "github", &McpMother::stdio("github"));
    let out = TempDir::new().unwrap();
    let into = out.path().join("backup");

    let exported = export(&workspace, &into, &[], false).unwrap();

    assert!(exported.written);
    assert!(exported.plan.made);
    assert_eq!(exported.plan.kinds, Kind::ALL.to_vec());
    assert_eq!(
        said(&exported.plan),
        [
            format!("skill:{name} added"),
            "rule:git/commits added".to_owned(),
            "mcp:github added".to_owned(),
        ]
    );
    let made = WeaverWorkspace::open(&into).unwrap();
    assert_eq!(made.name(), workspace.name());
    for kind in Kind::ALL {
        assert!(copy::same(&library(&workspace, kind), &library(&made, kind)).unwrap());
    }
}

#[test]
fn only_the_kinds_asked_for_are_exported() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, "deploy");
    rule(&workspace, "style");
    let out = TempDir::new().unwrap();

    let exported = export(&workspace, out.path(), &[Kind::Rule], false).unwrap();

    assert_eq!(exported.plan.kinds, [Kind::Rule]);
    assert_eq!(said(&exported.plan), ["rule:style added"]);
    let made = WeaverWorkspace::open(out.path()).unwrap();
    assert!(library(&made, Kind::Rule).join("style.md").is_file());
    assert!(!library(&made, Kind::Skill).exists());
}

#[test]
fn a_second_export_that_only_adds_needs_no_yes() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, "deploy");
    let out = TempDir::new().unwrap();
    export(&workspace, out.path(), &[], false).unwrap();
    skill(&workspace, "review");

    let exported = export(&workspace, out.path(), &[], false).unwrap();

    assert!(exported.written);
    assert!(!exported.plan.made);
    assert_eq!(
        said(&exported.plan),
        ["skill:deploy unchanged", "skill:review added"]
    );
    let made = WeaverWorkspace::open(out.path()).unwrap();
    assert!(library(&made, Kind::Skill)
        .join("review/SKILL.md")
        .is_file());
}

#[test]
fn an_export_that_would_change_or_remove_what_is_there_is_a_plan_until_yes() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let deploy = skill(&workspace, "deploy");
    let old = skill(&workspace, "old");
    let out = TempDir::new().unwrap();
    export(&workspace, out.path(), &[], false).unwrap();
    write(&deploy.join("notes.md"), "changed since\n");
    fs::remove_dir_all(&old).unwrap();
    let made = WeaverWorkspace::open(out.path()).unwrap();
    let exported_old = library(&made, Kind::Skill).join("old");

    let withheld = export(&workspace, out.path(), &[], false).unwrap();

    assert!(!withheld.written);
    assert!(withheld.plan.takes_away());
    assert_eq!(
        said(&withheld.plan),
        ["skill:deploy updated", "skill:old removed"]
    );
    assert!(exported_old.is_dir(), "nothing is written without a yes");

    let done = export(&workspace, out.path(), &[], true).unwrap();

    assert!(done.written);
    assert!(!exported_old.exists());
    assert!(copy::same(
        &library(&workspace, Kind::Skill),
        &library(&made, Kind::Skill)
    )
    .unwrap());
}

#[test]
fn a_kind_the_library_no_longer_holds_is_taken_out_of_the_export_with_a_yes() {
    let (_dir, workspace) = WorkspaceMother::empty();
    rule(&workspace, "style");
    let out = TempDir::new().unwrap();
    export(&workspace, out.path(), &[], false).unwrap();
    fs::remove_dir_all(library(&workspace, Kind::Rule)).unwrap();

    let done = export(&workspace, out.path(), &[], true).unwrap();

    assert_eq!(said(&done.plan), ["rule:style removed"]);
    let made = WeaverWorkspace::open(out.path()).unwrap();
    assert!(!library(&made, Kind::Rule).exists());
}

#[test]
fn a_folder_holding_files_of_its_own_keeps_them_beside_the_workspace_made() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, "deploy");
    let out = TempDir::new().unwrap();
    let readme = random::word();
    write(&out.path().join("README.md"), &readme);

    let exported = export(&workspace, out.path(), &[], false).unwrap();

    assert!(exported.plan.made);
    assert_eq!(
        fs::read_to_string(out.path().join("README.md")).unwrap(),
        readme
    );
}

#[test]
fn a_folder_whose_parent_is_not_there_is_not_made() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let out = TempDir::new().unwrap();
    let into = out.path().join("missing").join("backup");

    let refused = export(&workspace, &into, &[], false).unwrap_err();

    assert_eq!(refused.code(), "export_write_failed");
    assert!(!out.path().join("missing").exists());
}

#[test]
fn a_workspace_in_use_is_refused_for_each_thing_that_shows_it() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let out = TempDir::new().unwrap();
    let ledgered = WorkspaceMother::create(out.path());
    fs::write(ledgered.ledger_path(), b"").unwrap();
    let refused = plan(&workspace, out.path(), &[]).unwrap_err();
    assert_eq!(refused.code(), "export_into_workspace_in_use");
    assert!(refused.to_string().contains("it has a ledger"), "{refused}");

    let out = TempDir::new().unwrap();
    let mut linking = WorkspaceMother::create(out.path());
    ProjectMother::linked(&mut linking, "app");
    let refused = plan(&workspace, out.path(), &[]).unwrap_err();
    assert_eq!(refused.code(), "export_into_workspace_in_use");
    assert!(
        refused.to_string().contains("it links projects"),
        "{refused}"
    );
}

#[test]
fn the_workspace_itself_and_any_workspace_folder_are_refused() {
    let (dir, workspace) = WorkspaceMother::empty();

    let refused = plan(&workspace, dir.path(), &[]).unwrap_err();
    assert_eq!(refused.code(), "export_into_itself");

    let inside = workspace.weaver_dir().join("backup");
    let refused = plan(&workspace, &inside, &[]).unwrap_err();
    assert_eq!(refused.code(), "export_into_workspace_folder");

    let out = TempDir::new().unwrap();
    let elsewhere = out.path().join(".mindweaver").join("backup");
    let refused = plan(&workspace, &elsewhere, &[]).unwrap_err();
    assert_eq!(refused.code(), "export_into_workspace_folder");
}

#[test]
fn a_file_is_no_folder_to_export_into() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let out = TempDir::new().unwrap();
    let file = out.path().join("backup");
    fs::write(&file, b"").unwrap();

    let refused = plan(&workspace, &file, &[]).unwrap_err();

    assert_eq!(refused.code(), "export_not_a_folder");
}

#[test]
fn a_workspace_folder_with_no_marker_is_refused_rather_than_written_over() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let out = TempDir::new().unwrap();
    write(
        &out.path().join(".mindweaver/library/skills/kept/SKILL.md"),
        "somebody's\n",
    );

    let refused = plan(&workspace, out.path(), &[]).unwrap_err();

    assert_eq!(refused.code(), "export_into_unmarked");
}

#[test]
fn a_marker_keeping_a_kind_outside_its_library_is_refused() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let out = TempDir::new().unwrap();
    let directories = crate::workspace::Directories::default().with(Kind::Skill, "skills");
    WeaverWorkspace::init_with(out.path(), &directories).unwrap();

    let refused = plan(&workspace, out.path(), &[Kind::Skill]).unwrap_err();

    assert_eq!(refused.code(), "export_directory_elsewhere");
}

#[cfg(unix)]
#[test]
fn a_link_on_the_way_into_the_library_is_refused() {
    let (_dir, workspace) = WorkspaceMother::empty();
    skill(&workspace, "deploy");
    let out = TempDir::new().unwrap();
    export(&workspace, out.path(), &[], false).unwrap();
    let elsewhere = TempDir::new().unwrap();
    let skills = out.path().join(".mindweaver/library/skills");
    fs::remove_dir_all(&skills).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), &skills).unwrap();

    let refused = export(&workspace, out.path(), &[], true).unwrap_err();

    assert_eq!(refused.code(), "export_through_link");
    assert!(fs::read_dir(elsewhere.path()).unwrap().next().is_none());
}

#[test]
fn a_definition_holding_a_token_is_refused_and_the_token_is_never_said() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let (prefix, token) = McpMother::token();
    let (definition, at) = McpMother::with_literal("github", "env", &token);
    let file = server(&workspace, "github", &definition);
    let out = TempDir::new().unwrap();

    let refused = export(&workspace, out.path(), &[], true).unwrap_err();

    assert_eq!(refused.code(), "export_secret_literal");
    match &refused {
        ExportError::Secret {
            path,
            at: said,
            prefix: starts,
        } => {
            assert_eq!(path, &file);
            assert_eq!(said, &at);
            assert_eq!(starts, prefix);
        }
        other => panic!("not refused for its token: {other:?}"),
    }
    assert!(!refused.to_string().contains(&token));
    assert!(!out.path().join(".mindweaver").exists());
}

#[test]
fn a_token_is_no_reason_to_refuse_an_export_that_leaves_mcp_servers_out() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let (_, token) = McpMother::token();
    let (definition, _) = McpMother::with_literal("github", "env", &token);
    server(&workspace, "github", &definition);
    skill(&workspace, "deploy");
    let out = TempDir::new().unwrap();

    let exported = export(&workspace, out.path(), &[Kind::Skill], false).unwrap();

    assert_eq!(said(&exported.plan), ["skill:deploy added"]);
}

#[test]
fn a_definition_that_does_not_read_is_refused() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let file = server(&workspace, "broken", "this = [is not toml\n");
    let out = TempDir::new().unwrap();

    let refused = plan(&workspace, out.path(), &[Kind::Mcp]).unwrap_err();

    assert_eq!(refused.code(), "export_mcp_unread");
    assert!(refused.to_string().starts_with(&file.display().to_string()));
}
