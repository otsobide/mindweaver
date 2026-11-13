//! Unit tests for `known.rs`, kept beside it rather than inside it.
//!
//! The integration suite remembers, lists and forgets workspaces; these hold
//! the pieces a row is made of: how a workspace that opened is counted, how
//! one that did not is said with no credential in it, when the default
//! counts as made, and which rows a switch is offered to.

use super::*;

use std::fs;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{ProjectMother, UrlMother, WorkspaceMother};
use crate::workspace::WEAVER_DIR;

#[test]
fn no_home_is_no_default() {
    assert!(made_default(None).unwrap().is_none());
}

/// A read of an unmade one would answer from the template, which knows no
/// workspace, so it is no default until its marker is there; and asking
/// never makes it.
#[test]
fn a_home_with_no_marker_has_no_default_and_is_not_given_one() {
    let dir = TempDir::new().unwrap();

    assert!(made_default(Some(dir.path())).unwrap().is_none());
    assert!(!dir.path().join(WEAVER_DIR).exists());
}

#[test]
fn a_home_holding_a_workspace_has_it_for_its_default() {
    let (dir, workspace) = WorkspaceMother::empty();

    let default = made_default(Some(dir.path())).unwrap().unwrap();

    assert_eq!(default.root(), workspace.root());
}

#[test]
fn a_workspace_that_opened_is_listed_by_its_name_and_place_and_marked_nothing() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let row = KnownWorkspace::of(&workspace);

    assert_eq!(row.name.as_deref(), Some(workspace.name()));
    assert_eq!(row.path, workspace.root());
    assert!(!row.default && !row.remembered && !row.in_use);
    assert_eq!((row.projects, row.stale_projects), (0, 0));
    assert_eq!(row.stale, None);
}

#[test]
fn a_workspace_counts_each_project_it_links_and_those_that_would_not_open() {
    let (_dir, mut workspace) = WorkspaceMother::empty();
    let linked = random::between(1, 4);
    let mut roots = Vec::new();
    for n in 0..linked {
        roots.push(
            ProjectMother::linked(&mut workspace, &format!("p{n}"))
                .root()
                .to_path_buf(),
        );
    }
    let gone = random::between(0, linked);
    for root in &roots[..gone] {
        fs::remove_dir_all(root).unwrap();
    }

    let row = KnownWorkspace::of(&workspace);

    assert_eq!(row.projects, linked);
    assert_eq!(row.stale_projects, gone);
}

#[test]
fn a_workspace_that_would_not_open_is_listed_with_why_and_nothing_counted() {
    let error = WorkspaceError::NotAWorkspace {
        path: PathBuf::from("/w"),
    };

    let row = KnownWorkspace::gone(PathBuf::from("/w"), &error);

    assert_eq!(row.name, None);
    assert_eq!(row.path, Path::new("/w"));
    assert_eq!((row.projects, row.stale_projects), (0, 0));
    assert_eq!(
        row.stale.as_ref().map(Redacted::as_str),
        Some(error.to_string().as_str())
    );
}

/// A marker that does not parse is quoted in its own words, a source's URL
/// among them perhaps, and nothing says whoever wrote it left out the
/// credential.
#[test]
fn why_a_workspace_would_not_open_is_said_without_any_credential_it_quotes() {
    for typed in UrlMother::every_credentialed() {
        let error = WorkspaceError::Malformed {
            path: PathBuf::from("/w/.mindweaver/weaver.toml"),
            detail: format!("unknown source {}", typed.typed),
        };

        let row = KnownWorkspace::gone(PathBuf::from("/w"), &error);

        let stale = row.stale.unwrap();
        assert!(!stale.contains(&typed.secret), "{stale}");
        assert!(stale.ends_with(&typed.clean), "{typed:?} gave {stale}");
    }
}

#[test]
fn with_no_home_and_nothing_in_use_nothing_is_known() {
    let (known, failures) = known(None, None);

    assert_eq!(known, Known::default());
    assert!(failures.is_empty());
}

#[test]
fn a_workspace_in_use_that_nothing_remembers_is_the_one_row() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let (known, failures) = known(None, Some(&workspace));

    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(
        known.workspaces,
        [KnownWorkspace {
            in_use: true,
            ..KnownWorkspace::of(&workspace)
        }]
    );
}

/// The default before anything made it is no workspace to mark, so a list
/// asked for from inside it holds nothing.
#[test]
fn an_unmade_workspace_in_use_is_not_listed() {
    let dir = TempDir::new().unwrap();
    let unmade = WeaverWorkspace::unmade(dir.path()).unwrap();

    let (known, _) = known(Some(dir.path()), Some(&unmade));

    assert!(known.workspaces.is_empty(), "{known:?}");
    assert!(
        !dir.path().join(WEAVER_DIR).exists(),
        "listing makes nothing"
    );
}

#[test]
fn the_default_is_listed_first_as_the_default_and_never_as_remembered() {
    let (dir, workspace) = WorkspaceMother::empty();

    let (known, _) = known(Some(dir.path()), Some(&workspace));

    assert_eq!(known.workspaces.len(), 1, "{known:?}");
    let row = &known.workspaces[0];
    assert!(row.default && row.in_use && !row.remembered, "{row:?}");
}

fn launch(root: &Path, reason: Reason) -> Launch<'_> {
    Launch { root, reason }
}

#[test]
fn the_workspace_in_use_is_offered_no_switch_to_itself() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let rows = switchable(None, &launch(workspace.root(), Reason::Above), &workspace);

    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].launched);
    assert_eq!(rows[0].reason, Reason::Above, "the launch's own rule");
    assert!(!rows[0].switchable);
}

/// Launched on one and moved to another since: the launch is listed after
/// what is known, offered as a switch, and chosen by name from then on.
#[test]
fn the_workspace_launched_on_is_offered_once_another_is_in_use() {
    let dir = TempDir::new().unwrap();
    let launched = WorkspaceMother::create(&dir.path().join("first"));
    let in_use = WorkspaceMother::create(&dir.path().join("second"));

    let rows = switchable(None, &launch(launched.root(), Reason::Home), &in_use);

    let summary: Vec<(&Path, bool, bool, Reason, bool)> = rows
        .iter()
        .map(|row| {
            (
                row.workspace.path.as_path(),
                row.workspace.in_use,
                row.launched,
                row.reason,
                row.switchable,
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            (in_use.root(), true, false, Reason::Explicit, false),
            (launched.root(), false, true, Reason::Home, true),
        ]
    );
}

/// Every line names the workspace by its path, so one whose path is no
/// text a command line can hold is listed and never offered. Linux alone,
/// whose file systems take any bytes in a name, where macOS's refuse one
/// that is not UTF-8.
#[cfg(target_os = "linux")]
#[test]
fn a_workspace_whose_path_is_not_utf_8_is_never_offered_a_switch() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let dir = TempDir::new().unwrap();
    let launched = WorkspaceMother::create(&dir.path().join(OsStr::from_bytes(b"caf\xe9")));
    let in_use = WorkspaceMother::create(&dir.path().join("other"));

    let rows = switchable(None, &launch(launched.root(), Reason::Explicit), &in_use);

    let row = rows
        .iter()
        .find(|row| row.launched)
        .expect("the launch is listed");
    assert!(row.workspace.stale.is_none(), "it opens: {row:?}");
    assert!(!row.switchable, "{row:?}");
}

#[test]
fn a_workspace_launched_on_that_has_gone_is_listed_with_why_and_offered_nothing() {
    let dir = TempDir::new().unwrap();
    let launched = WorkspaceMother::create(&dir.path().join("first"));
    let in_use = WorkspaceMother::create(&dir.path().join("second"));
    let root = launched.root().to_path_buf();
    fs::remove_dir_all(&root).unwrap();

    let rows = switchable(None, &launch(&root, Reason::Explicit), &in_use);

    let row = rows.iter().find(|row| row.launched).unwrap();
    assert!(row.workspace.stale.is_some(), "{row:?}");
    assert_eq!(row.workspace.name, None);
    assert!(!row.switchable);
}

#[test]
fn a_row_one_may_switch_to_serializes_its_workspace_flat_beside_how_it_stands() {
    let (_dir, workspace) = WorkspaceMother::empty();

    let rows = switchable(None, &launch(workspace.root(), Reason::Above), &workspace);
    let value = serde_json::to_value(&rows[0]).unwrap();

    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "default",
            "in_use",
            "launched",
            "name",
            "path",
            "projects",
            "reason",
            "remembered",
            "stale",
            "stale_projects",
            "switchable",
        ]
    );
}

#[test]
fn nothing_is_remembered_without_a_default() {
    let (_dir, workspace) = WorkspaceMother::empty();

    assert!(remember(None, &workspace).unwrap().is_none());
}

#[test]
fn forgetting_with_no_default_made_names_what_was_not_known_and_where() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("gone");

    let refused = forget(dir.path(), &root).unwrap_err();

    assert!(
        matches!(&refused, WorkspaceError::NotKnown { path, .. } if path == &root),
        "{refused:?}"
    );
    assert!(!dir.path().join(WEAVER_DIR).exists());
}
