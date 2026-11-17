//! Finding a project again by its id: giving a copy an id of its own, and
//! following a project that moved to where it went, with what it had.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::exclude::{self, Followed};
use mindweaver_core::install;
use mindweaver_core::relink::{self, RelinkError};
use mindweaver_core::scan;
use mindweaver_core::{
    Agent, AgentHomes, Directories, Kind, MindProject, ProjectId, Share, WeaverWorkspace,
    WorkspaceError,
};
use tempfile::TempDir;

/// A workspace keeping `deploy` as its own and linking `collapse`, a
/// project marked by its settings in a repository of its own, shared as
/// `share` says, with `deploy` installed into it.
fn installed(share: Share) -> (TempDir, WeaverWorkspace, MindProject) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path().join("Projects")).unwrap();
    mindweaver_core::create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let root = dir.path().join("Projects/collapse");
    fs::create_dir_all(&root).unwrap();
    gix::init(&root).unwrap();
    let (project, _) =
        MindProject::init_asked(&root, &Agent::ALL, share, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    let (ledger, _) = workspace.ledger_to_write().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    (dir, workspace, project)
}

fn id_of(root: &Path) -> ProjectId {
    MindProject::open(root)
        .unwrap()
        .settings()
        .unwrap()
        .id
        .clone()
}

fn exclude_file(repository: &Path) -> PathBuf {
    repository.join(".git").join("info").join("exclude")
}

#[test]
fn a_new_id_changes_the_id_line_alone_and_keeps_the_exclude_group_and_records_with_it() {
    let (_dir, workspace, project) = installed(Share::Local);
    let root = project.root().to_path_buf();
    let was = id_of(&root);
    let settings = fs::read_to_string(project.marker_path()).unwrap();

    let renewed = relink::new_id(Some(&workspace), &project).unwrap();

    assert_eq!(renewed.was, was);
    assert_ne!(renewed.id, was);
    assert_eq!(id_of(&root), renewed.id);
    assert_eq!(
        fs::read_to_string(project.marker_path()).unwrap(),
        settings.replace(was.as_str(), renewed.id.as_str())
    );
    assert_eq!(
        renewed.excluded,
        Some(Followed::Written {
            file: exclude_file(&root)
        })
    );
    let listed = exclude::listed(&root, renewed.id.as_str())
        .unwrap()
        .unwrap();
    assert_eq!(
        listed.lines,
        ["/.agents/skills/deploy/", "/.claude/skills/deploy/"]
    );
    assert!(exclude::listed(&root, was.as_str())
        .unwrap()
        .unwrap()
        .lines
        .is_empty());
    let ledger = workspace.ledger_for_reading().unwrap();
    assert_eq!(
        ledger.project_at("collapse").unwrap(),
        Some(renewed.id.clone())
    );
    assert_eq!(
        ledger.installations("collapse", Kind::Skill).unwrap(),
        ["deploy"]
    );
}

#[test]
fn a_shared_project_or_one_no_workspace_links_is_given_a_new_id_all_the_same() {
    let (_dir, workspace, project) = installed(Share::Shared);
    let renewed = relink::new_id(Some(&workspace), &project).unwrap();
    assert_eq!(renewed.excluded, None);
    assert_eq!(id_of(project.root()), renewed.id);

    let alone = TempDir::new().unwrap();
    let (project, _) = MindProject::init_asked(
        alone.path(),
        &Agent::ALL,
        Share::Shared,
        &Directories::default(),
    )
    .unwrap();
    let renewed = relink::new_id(None, &project).unwrap();
    assert_eq!(id_of(alone.path()), renewed.id);
}

#[test]
fn a_project_an_older_marker_marks_is_pointed_at_the_migration_for_a_new_id() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();

    let refused = relink::new_id(None, &project).unwrap_err();

    assert!(
        matches!(
            refused,
            RelinkError::Workspace(WorkspaceError::NoSettings { .. })
        ),
        "{refused}"
    );
    assert!(refused.to_string().contains("`mind migrate`"), "{refused}");
}

/// Copy the directory `from` to `to`, `.mind` and all, as `cp -r` would.
fn copied(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copied(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn scanned(workspace: &WeaverWorkspace) -> mindweaver_core::Scan {
    mindweaver_core::scan(workspace, &mindweaver_core::scan::Request::default())
}

#[test]
fn a_scan_finds_a_project_moved_from_a_stale_entry_by_its_id() {
    let (dir, workspace, project) = installed(Share::Local);
    let moved = dir.path().join("Projects/apps/collapse");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(project.root(), &moved).unwrap();

    let scan = scanned(&workspace);

    let found = scan.found.iter().find(|found| found.path == moved).unwrap();
    assert_eq!(found.route, "apps/collapse");
    assert_eq!(found.moved.as_deref(), Some("collapse"));
    assert!(scan.untraced.is_empty());
    assert!(scan.failures.is_empty(), "{:?}", scan.failures);
}

#[test]
fn a_moved_project_is_matched_to_its_entry_by_where_each_points() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path().join("Projects")).unwrap();
    mindweaver_core::create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let elsewhere = dir.path().join("Elsewhere");
    let project = linked(&mut workspace, &elsewhere.join("collapse"));
    linked(&mut workspace, &elsewhere.join("other"));
    install_deploy(&workspace, &project);
    let moved = elsewhere.join("apps/collapse");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(project.root(), &moved).unwrap();

    let request = scan::Request {
        roots: vec![elsewhere.clone()],
        ..scan::Request::default()
    };
    let scan = mindweaver_core::scan(&workspace, &request);

    let found = scan.found.iter().find(|found| found.path == moved).unwrap();
    // Outside the workspace, spelled by its path, where the marker spells
    // the entry by a route climbing out of it.
    let left = found.moved.as_deref().unwrap();
    assert_eq!(Path::new(left), project.root());
    let matched: Vec<PathBuf> = workspace
        .members()
        .into_iter()
        .filter(|member| scan::moved_from(left, workspace.root(), &member.root))
        .map(|member| member.entry)
        .collect();
    assert_eq!(matched, [PathBuf::from("../Elsewhere/collapse")]);
}

#[test]
fn a_project_moved_before_the_ledger_learned_its_id_leaves_its_entry_untraced() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let project = linked(&mut workspace, &dir.path().join("collapse"));
    let moved = dir.path().join("apps/collapse");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(project.root(), &moved).unwrap();

    let scan = scanned(&workspace);

    let found = scan.found.iter().find(|found| found.path == moved).unwrap();
    assert_eq!(found.moved, None);
    assert_eq!(scan.untraced, ["collapse"]);
    assert!(!workspace.ledger_path().exists());
    // Nor is it said once nothing found could be its project.
    workspace.link(&MindProject::open(&moved).unwrap()).unwrap();
    assert!(scanned(&workspace).untraced.is_empty());
}

#[test]
fn a_stale_entry_whose_id_the_ledger_knows_is_not_untraced() {
    let (dir, workspace, project) = installed(Share::Local);
    fs::rename(project.root(), dir.path().join("gone")).unwrap();
    let other = dir.path().join("Projects/other");
    fs::create_dir_all(&other).unwrap();
    MindProject::init_asked(&other, &Agent::ALL, Share::Local, &Directories::default()).unwrap();

    let scan = scanned(&workspace);

    assert!(scan.found.iter().all(|found| found.moved.is_none()));
    assert!(scan.untraced.is_empty(), "{:?}", scan.untraced);
}

#[test]
fn a_copy_beside_its_project_is_a_duplicate_and_never_a_move() {
    let (dir, workspace, project) = installed(Share::Local);
    let copy = dir.path().join("Projects").join("copy");
    copied(project.root(), &copy);

    let scan = scanned(&workspace);

    assert!(scan.found.iter().all(|found| found.moved.is_none()));
    let said: Vec<String> = scan.failures.iter().map(ToString::to_string).collect();
    assert_eq!(
        said,
        [format!(
            "{} and {} are both project {}, copied with its .mind; \
             `mind -C <copy> init --new-id` gives a copy an id of its own",
            project.root().display(),
            copy.display(),
            id_of(project.root())
        )]
    );
}

#[test]
fn two_copies_of_a_project_that_moved_are_a_duplicate_and_neither_is_its_move() {
    let (dir, workspace, project) = installed(Share::Local);
    let first = dir.path().join("Projects/first");
    let second = dir.path().join("Projects/second");
    copied(project.root(), &second);
    fs::rename(project.root(), &first).unwrap();

    let scan = scanned(&workspace);

    assert!(scan.found.iter().all(|found| found.moved.is_none()));
    assert!(matches!(
        &scan.failures[..],
        [mindweaver_core::scan::ScanFailure::Duplicate(duplicate)]
            if duplicate.paths == [first.clone(), second.clone()]
    ));
}

#[test]
fn two_linked_copies_are_a_failure_of_the_status_naming_both() {
    let (dir, mut workspace, project) = installed(Share::Local);
    let copy = dir.path().join("Projects").join("copy");
    copied(project.root(), &copy);
    workspace.link(&MindProject::open(&copy).unwrap()).unwrap();

    let (status, failures) = mindweaver_core::Status::of_workspace(&workspace);

    assert_eq!(status.projects.len(), 2);
    let said: Vec<String> = failures.iter().map(ToString::to_string).collect();
    assert_eq!(said.len(), 1, "{said:?}");
    assert!(
        said[0].starts_with(&format!(
            "{} and {} are both project {}",
            project.root().display(),
            copy.display(),
            id_of(&copy)
        )),
        "{said:?}"
    );
}

/// Install `deploy`, the workspace's own, into `project`.
fn install_deploy(workspace: &WeaverWorkspace, project: &MindProject) {
    let (ledger, _) = workspace.ledger_to_write().unwrap();
    let candidate =
        install::offered(workspace, &ledger, project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        workspace,
        &ledger,
        project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
}

/// A project marked by its settings at `root`, local, linked by `workspace`.
fn linked(workspace: &mut WeaverWorkspace, root: &Path) -> MindProject {
    fs::create_dir_all(root).unwrap();
    let (project, _) =
        MindProject::init_asked(root, &Agent::ALL, Share::Local, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    project
}

fn marker(workspace: &WeaverWorkspace) -> String {
    fs::read_to_string(workspace.config_path()).unwrap()
}

/// How each artifact of the project at `entry` stands, as `mindweaver status`
/// reports it.
fn states(workspace: &WeaverWorkspace, entry: &str) -> Vec<(String, String)> {
    let (status, failures) = mindweaver_core::Status::of_workspace(workspace);
    assert!(failures.is_empty(), "{failures:?}");
    status
        .projects
        .iter()
        .find(|project| project.entry.as_deref() == Some(entry))
        .unwrap()
        .artifacts
        .iter()
        .map(|artifact| (artifact.name.to_string(), artifact.state.slug().to_owned()))
        .collect()
}

#[test]
fn a_moved_project_is_relinked_in_its_place_and_every_install_is_current_again() {
    let (dir, mut workspace, project) = installed(Share::Local);
    let tanukeys = dir.path().join("Projects/tanukeys");
    gix::init(&tanukeys).unwrap();
    linked(&mut workspace, &tanukeys);
    let before = marker(&workspace);
    let id = id_of(project.root());
    let moved = dir.path().join("Projects/apps/collapse");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(project.root(), &moved).unwrap();

    let relinked = relink::relink(&mut workspace, project.root(), &moved).unwrap();

    assert_eq!(relinked.was, PathBuf::from("collapse"));
    assert_eq!(relinked.entry, PathBuf::from("apps/collapse"));
    assert_eq!(relinked.id, id);
    assert_eq!(relinked.installations, 1);
    assert_eq!(
        marker(&workspace),
        before.replace("\"collapse\"", "\"apps/collapse\"")
    );
    assert_eq!(
        states(&workspace, "apps/collapse"),
        [(String::from("deploy"), String::from("current"))]
    );
    let ledger = workspace.ledger_for_reading().unwrap();
    assert_eq!(
        ledger.project_at("apps/collapse").unwrap(),
        Some(id.clone())
    );
    assert!(!ledger.holds("collapse").unwrap());
    // The repository moved whole, its exclude file with it.
    assert_eq!(relinked.excluded, Some(Followed::Nothing));
    assert_eq!(
        exclude::listed(&moved, id.as_str()).unwrap().unwrap().lines,
        ["/.agents/skills/deploy/", "/.claude/skills/deploy/"]
    );
}

#[test]
fn a_project_moved_inside_its_repository_takes_its_exclude_lines_along() {
    let dir = TempDir::new().unwrap();
    let repository = dir.path().join("monorepo");
    gix::init(&repository).unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(&repository).unwrap();
    mindweaver_core::create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let project = linked(&mut workspace, &repository.join("packages/app"));
    install_deploy(&workspace, &project);
    let id = id_of(project.root());
    let moved = repository.join("apps/app");
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::rename(project.root(), &moved).unwrap();

    let relinked = relink::relink(&mut workspace, project.root(), &moved).unwrap();

    assert_eq!(
        relinked.excluded,
        Some(Followed::Written {
            file: exclude_file(&repository)
        })
    );
    assert_eq!(
        exclude::listed(&moved, id.as_str()).unwrap().unwrap().lines,
        [
            "/apps/app/.agents/skills/deploy/",
            "/apps/app/.claude/skills/deploy/"
        ]
    );
    assert_eq!(
        states(&workspace, "apps/app"),
        [(String::from("deploy"), String::from("current"))]
    );
}

#[test]
fn a_project_holding_another_id_is_refused_and_nothing_changes() {
    let (dir, mut workspace, project) = installed(Share::Local);
    fs::rename(project.root(), dir.path().join("gone")).unwrap();
    let other = dir.path().join("other");
    fs::create_dir_all(&other).unwrap();
    MindProject::init_asked(&other, &Agent::ALL, Share::Local, &Directories::default()).unwrap();
    let before = marker(&workspace);
    let ledger = fs::read(workspace.ledger_path()).unwrap();

    let refused = relink::relink(&mut workspace, project.root(), &other).unwrap_err();

    assert!(
        matches!(&refused, RelinkError::Mismatch { was, id, .. }
            if *was == id_of(&dir.path().join("gone")) && *id == id_of(&other)),
        "{refused}"
    );
    assert!(
        refused.to_string().contains("nothing was changed"),
        "{refused}"
    );
    assert_eq!(marker(&workspace), before);
    assert_eq!(fs::read(workspace.ledger_path()).unwrap(), ledger);
}

#[test]
fn an_entry_still_holding_a_project_has_not_moved() {
    let (dir, mut workspace, project) = installed(Share::Local);
    let copy = dir.path().join("copy");
    copied(project.root(), &copy);
    let other = dir.path().join("other");
    fs::create_dir_all(&other).unwrap();
    MindProject::init_asked(&other, &Agent::ALL, Share::Local, &Directories::default()).unwrap();

    let refused = relink::relink(&mut workspace, project.root(), &copy).unwrap_err();
    assert!(matches!(refused, RelinkError::Copied(_)), "{refused}");
    assert!(
        refused.to_string().contains("copied with its .mind"),
        "{refused}"
    );

    let refused = relink::relink(&mut workspace, project.root(), &other).unwrap_err();
    assert!(matches!(refused, RelinkError::Present { .. }), "{refused}");
}

#[test]
fn an_entry_whose_marker_would_not_open_has_not_moved_and_a_copy_is_never_its_move() {
    let (dir, mut workspace, project) = installed(Share::Local);
    let id = id_of(project.root());
    let copy = dir.path().join("Projects/copy");
    copied(project.root(), &copy);
    // Its id is what nothing can read now, so nothing tells the copy apart.
    let settings = fs::read_to_string(project.marker_path()).unwrap();
    let newer: Vec<&str> = settings
        .lines()
        .map(|line| {
            if line.starts_with("version") {
                "version = 99"
            } else {
                line
            }
        })
        .collect();
    fs::write(project.marker_path(), newer.join("\n")).unwrap();
    let before = marker(&workspace);
    let ledger = fs::read(workspace.ledger_path()).unwrap();
    let excluded = fs::read(exclude_file(project.root())).unwrap();

    let scan = scanned(&workspace);
    let found = scan.found.iter().find(|found| found.path == copy).unwrap();
    assert_eq!(found.moved, None);

    let refused = relink::relink(&mut workspace, project.root(), &copy).unwrap_err();
    assert!(
        matches!(&refused, RelinkError::Unread { path, .. } if path == project.root()),
        "{refused}"
    );
    assert!(
        refused
            .to_string()
            .contains("written by a newer Mindweaver"),
        "{refused}"
    );
    assert_eq!(marker(&workspace), before);
    assert_eq!(fs::read(workspace.ledger_path()).unwrap(), ledger);
    assert_eq!(fs::read(exclude_file(project.root())).unwrap(), excluded);
    assert_eq!(
        exclude::listed(project.root(), id.as_str())
            .unwrap()
            .unwrap()
            .lines,
        ["/.agents/skills/deploy/", "/.claude/skills/deploy/"]
    );
}

#[test]
fn a_linked_copy_refuses_the_relink_until_it_is_given_an_id_of_its_own() {
    let (dir, mut workspace, project) = installed(Share::Local);
    let copy = dir.path().join("Projects/copy");
    copied(project.root(), &copy);
    workspace.link(&MindProject::open(&copy).unwrap()).unwrap();
    let moved = dir.path().join("Projects/moved");
    fs::rename(project.root(), &moved).unwrap();
    // Two directories hold the id, so the scan offers neither as the move.
    assert!(scanned(&workspace)
        .found
        .iter()
        .all(|found| found.moved.is_none()));

    let refused = relink::relink(&mut workspace, project.root(), &moved).unwrap_err();
    assert!(
        matches!(&refused, RelinkError::Copied(duplicate)
            if duplicate.paths == [copy.clone(), moved.clone()]),
        "{refused}"
    );

    relink::new_id(Some(&workspace), &MindProject::open(&copy).unwrap()).unwrap();
    let found = scanned(&workspace);
    let found = found
        .found
        .iter()
        .find(|found| found.path == moved)
        .unwrap();
    assert_eq!(found.moved.as_deref(), Some("collapse"));
    let relinked = relink::relink(&mut workspace, project.root(), &moved).unwrap();
    assert_eq!(relinked.installations, 1);
}

#[test]
fn a_workspace_with_no_ledger_moves_the_entry_alone() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let project = linked(&mut workspace, &dir.path().join("collapse"));
    let moved = dir.path().join("moved");
    fs::rename(project.root(), &moved).unwrap();

    let relinked = relink::relink(&mut workspace, project.root(), &moved).unwrap();

    assert_eq!(relinked.installations, 0);
    assert!(workspace.is_linked(&moved));
    assert!(!workspace.is_linked(project.root()));
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn records_no_id_says_were_the_projects_are_never_handed_to_another_directory() {
    let (dir, mut workspace, project) = installed(Share::Local);
    // As a ledger an older build made keeps them, its project unread since.
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute("DELETE FROM projects", [])
        .unwrap();
    let moved = dir.path().join("moved");
    fs::rename(project.root(), &moved).unwrap();

    let refused = relink::relink(&mut workspace, project.root(), &moved).unwrap_err();

    assert!(matches!(refused, RelinkError::Unknown { .. }), "{refused}");
    assert!(workspace.is_linked(project.root()));
}

#[test]
fn what_is_not_an_entry_or_not_a_project_with_an_id_is_refused() {
    let (dir, mut workspace, project) = installed(Share::Local);
    let legacy = dir.path().join("legacy");
    MindProject::init(&legacy).unwrap();
    let moved = dir.path().join("moved");
    fs::rename(project.root(), &moved).unwrap();

    let refused = relink::relink(&mut workspace, &legacy, &moved).unwrap_err();
    assert!(
        matches!(
            refused,
            RelinkError::Workspace(WorkspaceError::NotRegistered { .. })
        ),
        "{refused}"
    );
    let refused = relink::relink(&mut workspace, project.root(), &legacy).unwrap_err();
    assert!(matches!(refused, RelinkError::NoId { .. }), "{refused}");
    let refused =
        relink::relink(&mut workspace, project.root(), &dir.path().join("nothing")).unwrap_err();
    assert!(
        matches!(
            refused,
            RelinkError::Workspace(WorkspaceError::NotAProject { .. })
        ),
        "{refused}"
    );
}
