//! Creating, finding and reading mind projects and weaver workspaces.

use std::fs;
use std::path::Path;

use mindweaver_core::ledger::{Action, LedgerError, Outcome};
use mindweaver_core::workspace::{MIND_FORMAT_VERSION, MIND_TOML_VERSION, WEAVER_FORMAT_VERSION};
use mindweaver_core::{
    resolve, Directories, Initialization, Inputs, Intent, Kind, MindProject, Reason, Registration,
    Resolved, WeaverWorkspace, WorkspaceError, MIND_CONFIG, MIND_DIR, WEAVER_CONFIG, WEAVER_DIR,
};
use tempfile::TempDir;

#[test]
fn initializing_a_mind_project_creates_its_marker_and_its_skills_folder() {
    let dir = TempDir::new().unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::Created);
    assert!(dir.path().join(MIND_DIR).join(MIND_CONFIG).is_file());
    assert!(project.directory_for(Kind::Skill).unwrap().is_dir());
    assert_eq!(project.root(), dir.path());
    assert_eq!(project.config().version(), MIND_TOML_VERSION);
}

#[test]
fn a_new_project_is_named_after_its_directory() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("collapse");
    fs::create_dir(&root).unwrap();

    let (project, _) = MindProject::init(&root).unwrap();

    assert_eq!(project.name(), "collapse");
}

#[test]
fn initializing_twice_never_rewrites_the_marker() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let marker = dir.path().join(MIND_DIR).join(MIND_CONFIG);
    fs::write(&marker, "version = 1\nname = \"renamed by hand\"\n").unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert_eq!(project.name(), "renamed by hand");
}

#[test]
fn initializing_fills_in_a_skills_folder_someone_deleted() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path()).unwrap();
    fs::remove_dir(project.directory_for(Kind::Skill).unwrap()).unwrap();

    let (project, outcome) = MindProject::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    assert!(project.directory_for(Kind::Skill).unwrap().is_dir());
}

#[test]
fn a_project_is_found_from_any_directory_below_it() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    let deep = dir.path().join("apps/core/src");
    fs::create_dir_all(&deep).unwrap();

    let found = MindProject::locate(&deep)
        .unwrap()
        .expect("a project above");

    assert_eq!(found.root(), dir.path());
}

#[test]
fn locate_finds_nothing_when_there_is_no_project() {
    let dir = TempDir::new().unwrap();

    assert!(MindProject::locate(dir.path()).unwrap().is_none());
    assert!(WeaverWorkspace::locate(dir.path()).unwrap().is_none());
}

#[test]
fn opening_a_directory_that_is_not_a_project_says_so() {
    let dir = TempDir::new().unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotAProject { .. }));
    assert!(error.to_string().contains(MIND_DIR));
}

#[test]
fn an_empty_mind_directory_is_not_a_project() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(MIND_DIR)).unwrap();

    assert!(MindProject::locate(dir.path()).unwrap().is_none());
}

#[test]
fn a_marker_from_a_newer_mindweaver_is_refused() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "version = 99\nname = \"future\"\n",
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Version { found: 99, .. }));
}

#[test]
fn a_corrupt_marker_is_reported_as_such() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "not = toml = at all",
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Parse { .. }));
}

#[test]
fn initializing_a_workspace_creates_an_empty_registry() {
    let dir = TempDir::new().unwrap();

    let (workspace, outcome) = WeaverWorkspace::init(dir.path()).unwrap();

    assert_eq!(outcome, Initialization::Created);
    assert!(dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG).is_file());
    assert!(workspace.config().projects.is_empty());
    assert_eq!(workspace.projects().0, vec![]);
}

#[test]
fn a_workspace_opens_the_projects_it_references() {
    let dir = TempDir::new().unwrap();
    WeaverWorkspace::init(dir.path()).unwrap();
    for name in ["alpha", "beta"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        MindProject::init(&root).unwrap();
    }
    fs::write(
        dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"beta\"]\n",
    )
    .unwrap();

    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let (projects, failures) = workspace.projects();

    assert!(failures.is_empty());
    let names: Vec<&str> = projects.iter().map(MindProject::name).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn a_stale_reference_is_reported_without_losing_the_rest() {
    let dir = TempDir::new().unwrap();
    WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    MindProject::init(&root).unwrap();
    fs::write(
        dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"alpha\", \"moved-away\"]\n",
    )
    .unwrap();

    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let (projects, failures) = workspace.projects();

    assert_eq!(projects.len(), 1);
    assert_eq!(failures.len(), 1);
    assert!(failures[0].to_string().contains("moved-away"));
}

#[test]
fn members_keep_the_registry_order_and_the_entry_a_stale_one_came_from() {
    let dir = TempDir::new().unwrap();
    WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    MindProject::init(&root).unwrap();
    fs::write(
        dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG),
        "version = 1\nname = \"work\"\nprojects = [\"moved-away\", \"alpha\"]\n",
    )
    .unwrap();

    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let members = workspace.members();

    let entries: Vec<&Path> = members.iter().map(|m| m.entry.as_path()).collect();
    assert_eq!(entries, vec![Path::new("moved-away"), Path::new("alpha")]);
    // The stale one keeps its entry, which is what `unlink` needs to be given.
    assert!(members[0].project.is_err());
    assert_eq!(members[0].root, dir.path().join("moved-away"));
    assert_eq!(members[1].project.as_ref().unwrap().name(), "alpha");
}

#[test]
fn one_directory_can_be_both_a_workspace_and_a_project() {
    let dir = TempDir::new().unwrap();

    WeaverWorkspace::init(dir.path()).unwrap();
    MindProject::init(dir.path()).unwrap();

    assert!(WeaverWorkspace::locate(dir.path()).unwrap().is_some());
    assert!(MindProject::locate(dir.path()).unwrap().is_some());
}

// ---------------------------------------------------------------------------
// Registering projects: link and unlink
// ---------------------------------------------------------------------------

/// A workspace with `count` mind projects beside it, none of them registered.
fn workspace_with(count: usize) -> (TempDir, WeaverWorkspace, Vec<MindProject>) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let projects = ["alpha", "beta", "gamma"]
        .iter()
        .take(count)
        .map(|name| {
            let root = dir.path().join(name);
            fs::create_dir(&root).unwrap();
            MindProject::init(&root).unwrap().0
        })
        .collect();
    (dir, workspace, projects)
}

fn config_text(workspace: &WeaverWorkspace) -> String {
    fs::read_to_string(workspace.config_path()).unwrap()
}

#[test]
fn link_registers_a_project_relative_to_the_workspace() {
    let (_dir, mut workspace, projects) = workspace_with(1);

    let (entry, outcome) = workspace.link(&projects[0]).unwrap();

    assert_eq!(outcome, Registration::Added);
    assert_eq!(entry, std::path::Path::new("alpha"));
    assert_eq!(workspace.config().projects, vec![entry]);
    assert!(config_text(&workspace).contains("projects = [\"alpha\"]"));
}

#[test]
fn link_keeps_the_comments_that_explain_the_file() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    let before = config_text(&workspace);
    let comments: Vec<&str> = before.lines().filter(|l| l.starts_with('#')).collect();
    assert!(!comments.is_empty(), "the template ships with comments");

    workspace.link(&projects[0]).unwrap();

    let after = config_text(&workspace);
    for comment in &comments {
        assert!(
            after.contains(comment),
            "editing dropped the comment {comment:?}:\n{after}"
        );
    }
    // And the keys it was not asked to touch.
    assert!(after.contains(&format!("version = {WEAVER_FORMAT_VERSION}")));
    assert!(after.contains(&format!("name = \"{}\"", workspace.name())));
}

/// The skill directories a workspace lets `gather search` ask are a key a
/// person writes by hand (D-15), which moves no version: a build from
/// before it reads past it, and an edit keeps it, comment and all.
#[test]
fn the_finders_a_marker_lists_are_read_and_kept_through_an_edit() {
    let (_dir, workspace, projects) = workspace_with(1);
    assert_eq!(workspace.config().finders, None);
    let marker = workspace.config_path();
    let listed = "# Read their terms first.\nfinders = [\"github\", \"skills.sh\"]\n";
    let text = fs::read_to_string(&marker).unwrap().replacen(
        "loaded = []\n",
        &format!("loaded = []\n{listed}"),
        1,
    );
    fs::write(&marker, text).unwrap();
    let mut workspace = WeaverWorkspace::open(workspace.root()).unwrap();

    workspace.link(&projects[0]).unwrap();

    assert_eq!(
        workspace.config().finders,
        Some(vec![String::from("github"), String::from("skills.sh")])
    );
    let after = config_text(&workspace);
    assert!(after.contains(listed), "{after}");
    assert!(after.contains(&format!("version = {WEAVER_FORMAT_VERSION}")));
}

#[test]
fn linking_the_same_project_twice_changes_nothing() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    let after_first = config_text(&workspace);

    let (_, outcome) = workspace.link(&projects[0]).unwrap();

    assert_eq!(outcome, Registration::AlreadyRegistered);
    assert_eq!(workspace.config().projects.len(), 1);
    assert_eq!(
        config_text(&workspace),
        after_first,
        "the file was rewritten"
    );
}

#[test]
fn link_matches_on_where_an_entry_points_not_how_it_is_spelled() {
    let (dir, mut workspace, _) = workspace_with(0);
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    MindProject::init(&root).unwrap();

    // The same directory, reached by a path nobody would normalise by hand.
    let awkward = MindProject::open(dir.path().join("./beta/../alpha")).unwrap();
    workspace.link(&awkward).unwrap();
    let (_, outcome) = workspace.link(&MindProject::open(&root).unwrap()).unwrap();

    assert_eq!(outcome, Registration::AlreadyRegistered);
    assert_eq!(workspace.config().projects.len(), 1);
}

#[test]
fn a_project_outside_the_workspace_is_stored_as_a_route_out_of_it() {
    let outer = TempDir::new().unwrap();
    let inner = outer.path().join("workspace");
    fs::create_dir(&inner).unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(&inner).unwrap();
    let sibling = outer.path().join("collapse");
    fs::create_dir(&sibling).unwrap();
    let (project, _) = MindProject::init(&sibling).unwrap();

    let (entry, _) = workspace.link(&project).unwrap();

    assert_eq!(entry, std::path::Path::new("../collapse"));
    let (opened, failures) = workspace.projects();
    assert!(failures.is_empty());
    assert_eq!(opened[0].root(), project.root());
}

#[test]
fn a_linked_project_is_one_the_workspace_can_open() {
    let (_dir, mut workspace, projects) = workspace_with(2);
    workspace.link(&projects[0]).unwrap();
    workspace.link(&projects[1]).unwrap();

    let (opened, failures) = workspace.projects();

    assert!(failures.is_empty());
    let names: Vec<&str> = opened.iter().map(MindProject::name).collect();
    assert_eq!(names, vec!["alpha", "beta"]);
}

#[test]
fn unlink_removes_only_the_entry_it_was_given() {
    let (_dir, mut workspace, projects) = workspace_with(2);
    workspace.link(&projects[0]).unwrap();
    workspace.link(&projects[1]).unwrap();

    let removed = workspace.unlink(projects[0].root()).unwrap();

    assert_eq!(removed, vec![std::path::PathBuf::from("alpha")]);
    assert_eq!(
        workspace.config().projects,
        vec![std::path::PathBuf::from("beta")]
    );
    assert!(config_text(&workspace).contains("# Mindweaver workspace."));
}

#[test]
fn unlinking_the_first_entry_leaves_the_rest_laid_out_as_they_were() {
    let (_dir, mut workspace, projects) = workspace_with(3);
    for project in &projects {
        workspace.link(project).unwrap();
    }

    workspace.unlink(projects[0].root()).unwrap();
    assert!(
        config_text(&workspace).contains("projects = [\"beta\", \"gamma\"]\n"),
        "{}",
        config_text(&workspace)
    );

    let text = config_text(&workspace).replace(
        "projects = [\"beta\", \"gamma\"]",
        "projects = [\n    \"beta\",\n    \"gamma\",\n]",
    );
    fs::write(workspace.config_path(), text).unwrap();
    let mut workspace = WeaverWorkspace::open(workspace.root()).unwrap();
    workspace.unlink(projects[1].root()).unwrap();
    assert!(
        config_text(&workspace).contains("projects = [\n    \"gamma\",\n]\n"),
        "{}",
        config_text(&workspace)
    );
}

#[test]
fn unlinking_takes_the_comment_above_its_entry_and_leaves_the_others() {
    let (_dir, mut workspace, projects) = workspace_with(3);
    for project in &projects {
        workspace.link(project).unwrap();
    }
    let text = config_text(&workspace).replace(
        "projects = [\"alpha\", \"beta\", \"gamma\"]",
        "projects = [\n    # the app\n    \"alpha\",\n    # the site\n    \"beta\",\n    \
         # notes, keep\n    \"gamma\",\n]",
    );
    fs::write(workspace.config_path(), text).unwrap();
    let mut workspace = WeaverWorkspace::open(workspace.root()).unwrap();

    workspace.unlink(projects[1].root()).unwrap();
    assert!(
        config_text(&workspace).contains(
            "projects = [\n    # the app\n    \"alpha\",\n    # notes, keep\n    \"gamma\",\n]\n"
        ),
        "{}",
        config_text(&workspace)
    );

    workspace.unlink(projects[0].root()).unwrap();
    assert!(
        config_text(&workspace).contains("projects = [\n    # notes, keep\n    \"gamma\",\n]\n"),
        "{}",
        config_text(&workspace)
    );
}

#[test]
fn unlink_says_so_when_the_project_was_never_registered() {
    let (_dir, mut workspace, projects) = workspace_with(1);

    let error = workspace.unlink(projects[0].root()).unwrap_err();

    assert!(matches!(error, WorkspaceError::NotRegistered { .. }));
}

#[test]
fn unlink_works_on_an_entry_whose_directory_has_gone() {
    let (dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();

    // The stale entry is exactly the one worth removing, and it can no longer
    // be opened as a project.
    let removed = workspace.unlink(&dir.path().join("alpha")).unwrap();

    assert_eq!(removed, vec![std::path::PathBuf::from("alpha")]);
    assert!(workspace.config().projects.is_empty());
}

#[test]
fn link_restores_a_projects_key_someone_deleted() {
    let (_dir, workspace, projects) = workspace_with(1);
    fs::write(
        workspace.config_path(),
        "# kept\nversion = 1\nname = \"work\"\n",
    )
    .unwrap();
    let mut workspace = WeaverWorkspace::open(workspace.root()).unwrap();

    workspace.link(&projects[0]).unwrap();

    assert_eq!(workspace.config().projects.len(), 1);
    assert!(config_text(&workspace).contains("# kept"));
}

#[test]
fn linking_leaves_no_temporary_file_behind() {
    let (_dir, mut workspace, projects) = workspace_with(1);

    workspace.link(&projects[0]).unwrap();

    let leftovers: Vec<_> = fs::read_dir(workspace.weaver_dir())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .filter(|name| name.to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "left behind {leftovers:?}");
}

#[test]
fn a_root_is_normalized_so_two_spellings_of_one_directory_are_one_project() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    fs::create_dir(dir.path().join("beta")).unwrap();
    MindProject::init(&root).unwrap();

    let direct = MindProject::open(&root).unwrap();
    let roundabout = MindProject::open(dir.path().join("beta/../alpha")).unwrap();

    // Not cosmetic: a workspace resolves `../collapse` by joining it onto its
    // own root, so without this every such project would carry a `..` and stop
    // comparing equal to itself reached any other way.
    assert_eq!(direct.root(), roundabout.root());
    assert!(!roundabout.root().to_string_lossy().contains(".."));
}

#[test]
fn unlink_removes_every_spelling_that_pointed_at_the_project() {
    let (dir, workspace, projects) = workspace_with(1);
    fs::write(
        workspace.config_path(),
        "version = 1\nname = \"w\"\nprojects = [\"alpha\", \"./alpha\", \"./alpha/\"]\n",
    )
    .unwrap();
    let mut workspace = WeaverWorkspace::open(workspace.root()).unwrap();
    let _ = (&dir, &projects);

    let removed = workspace.unlink(projects[0].root()).unwrap();

    // Removing one of three and reporting success would leave the project
    // registered while telling the user it is gone.
    assert_eq!(removed.len(), 3);
    assert!(workspace.config().projects.is_empty());
}

#[test]
fn an_already_registered_link_reports_the_spelling_in_the_file() {
    let (_dir, workspace, projects) = workspace_with(1);
    fs::write(
        workspace.config_path(),
        "version = 1\nname = \"w\"\nprojects = [\"./alpha/\"]\n",
    )
    .unwrap();
    let mut workspace = WeaverWorkspace::open(workspace.root()).unwrap();

    let (entry, outcome) = workspace.link(&projects[0]).unwrap();

    assert_eq!(outcome, Registration::AlreadyRegistered);
    assert_eq!(entry, std::path::Path::new("./alpha/"), "not a fresh guess");
}

#[test]
fn a_link_that_changes_nothing_does_not_rewrite_the_file() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    let before = fs::metadata(workspace.config_path())
        .unwrap()
        .modified()
        .unwrap();

    workspace.link(&projects[0]).unwrap();

    let after = fs::metadata(workspace.config_path())
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(before, after, "an idempotent link touched the file");
}

#[test]
fn a_failed_unlink_leaves_the_file_alone() {
    let (_dir, mut workspace, projects) = workspace_with(1);
    workspace.link(&projects[0]).unwrap();
    let before = fs::read_to_string(workspace.config_path()).unwrap();

    let error = workspace
        .unlink(std::path::Path::new("/nowhere"))
        .unwrap_err();

    assert!(matches!(error, WorkspaceError::NotRegistered { .. }));
    assert_eq!(fs::read_to_string(workspace.config_path()).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn an_entry_is_only_stored_as_a_route_when_the_route_really_resolves() {
    use std::os::unix::fs::symlink;

    // `/tmp` is a symlink to `/private/tmp` on every Mac, so a workspace
    // reached through one is not an exotic case. A lexical `..` climbs out of
    // the link's target, not out of the directory the name suggests.
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("real/deep/ws")).unwrap();
    fs::create_dir(dir.path().join("real/sibling")).unwrap();
    symlink("real/deep", dir.path().join("link")).unwrap();

    let (mut workspace, _) = WeaverWorkspace::init(dir.path().join("link/ws")).unwrap();
    let (project, _) = MindProject::init(dir.path().join("real/sibling")).unwrap();

    let (entry, _) = workspace.link(&project).unwrap();

    let (opened, failures) = workspace.projects();
    assert!(
        failures.is_empty(),
        "the entry it just wrote does not open: {failures:?}"
    );
    assert_eq!(opened.len(), 1);
    // Read back through the directory's real spelling too, which is what any
    // later `cd` into it will use.
    let real = WeaverWorkspace::open(dir.path().join("real/deep/ws")).unwrap();
    assert_eq!(
        real.projects().0.len(),
        1,
        "entry {} does not resolve",
        entry.display()
    );
}

#[cfg(unix)]
#[test]
fn rewriting_the_config_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, mut workspace, projects) = workspace_with(1);
    let path = workspace.config_path();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

    workspace.link(&projects[0]).unwrap();

    let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "a config chmodded to 600 came back {mode:o}");
}

// ---------------------------------------------------------------------------
// Where a project keeps its artifacts
// ---------------------------------------------------------------------------

#[test]
fn a_new_project_keeps_its_artifacts_beside_the_code() {
    let dir = TempDir::new().unwrap();

    let (project, _) = MindProject::init(dir.path()).unwrap();

    // Beside the code, not inside `.mind`: these are files the agents
    // themselves read, and an agent does not know what a `.mind` is.
    assert_eq!(
        project.directory_for(Kind::Skill).unwrap(),
        dir.path().join("skills")
    );
    assert_eq!(
        project.directory_for(Kind::Rule).unwrap(),
        dir.path().join("rules")
    );
    assert!(dir.path().join("skills").is_dir());
    assert!(dir.path().join("rules").is_dir());
}

#[test]
fn the_marker_spells_out_where_each_kind_goes() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let written = fs::read_to_string(dir.path().join(MIND_DIR).join(MIND_CONFIG)).unwrap();

    // In the file, not left to a default somebody has to go and read.
    assert!(written.contains("[directories]"), "{written}");
    assert!(written.contains(r#"skills = "skills""#), "{written}");
    assert!(written.contains(r#"rules = "rules""#), "{written}");
}

#[test]
fn a_project_can_keep_its_skills_where_its_agents_look() {
    let dir = TempDir::new().unwrap();
    let directories = Directories::default().with(Kind::Skill, ".claude/skills");

    let (project, _) = MindProject::init_with(dir.path(), &directories).unwrap();

    assert_eq!(
        project.directory_for(Kind::Skill).unwrap(),
        dir.path().join(".claude/skills")
    );
    assert!(dir.path().join(".claude").join("skills").is_dir());
    // The kind that was not mentioned keeps its default.
    assert_eq!(
        project.directory_for(Kind::Rule).unwrap(),
        dir.path().join("rules")
    );
}

#[test]
fn where_a_project_keeps_things_survives_reopening_it() {
    let dir = TempDir::new().unwrap();
    let directories = Directories::default().with(Kind::Skill, "docs/skills");
    MindProject::init_with(dir.path(), &directories).unwrap();

    let reopened = MindProject::open(dir.path()).unwrap();

    assert_eq!(
        reopened.directory_for(Kind::Skill).unwrap(),
        dir.path().join("docs/skills")
    );
}

#[test]
fn a_second_init_does_not_move_where_a_project_keeps_things() {
    let dir = TempDir::new().unwrap();
    MindProject::init(dir.path()).unwrap();

    let (project, outcome) = MindProject::init_with(
        dir.path(),
        &Directories::default().with(Kind::Skill, "elsewhere"),
    )
    .unwrap();

    assert_eq!(outcome, Initialization::AlreadyInitialized);
    // The marker already answered the question, and `init` never overwrites it.
    assert_eq!(
        project.directory_for(Kind::Skill).unwrap(),
        dir.path().join("skills")
    );
    assert!(!dir.path().join("elsewhere").exists());
}

#[test]
fn a_directory_outside_the_project_is_refused_before_anything_is_made() {
    let dir = TempDir::new().unwrap();

    // `/etc/skills` is the one that used to slip through on Windows, where it
    // is not `is_absolute()`: it names the root of the current drive, which is
    // no more inside the project than an absolute path is. `.` is the project
    // itself rather than a place in it, and an empty name is not a place.
    //
    // No `C:/skills` here: on Linux that is a directory called `C:`, and it
    // really is inside the project. What a path means is the platform's answer
    // to give, which is the whole reason this check asks about components.
    for outside in ["/etc/skills", "../elsewhere", ".", ""] {
        let directories = Directories::default().with(Kind::Skill, outside);
        let error = MindProject::init_with(dir.path(), &directories).unwrap_err();

        assert!(
            matches!(error, WorkspaceError::OutsideProject { .. }),
            "{outside}: {error}"
        );
    }
    // Refused before anything was written, not halfway through.
    assert!(!dir.path().join(MIND_DIR).exists());
}

#[test]
fn a_marker_written_before_directories_existed_is_read_the_way_it_was_written() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(MIND_DIR).join("skills")).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        "version = 1\nname = \"collapse\"\n",
    )
    .unwrap();

    let project = MindProject::open(dir.path()).unwrap();

    // Today's default would point it at a directory it has never had, and it
    // would list nothing without saying why.
    assert_eq!(
        project.directory_for(Kind::Skill).unwrap(),
        dir.path().join(MIND_DIR).join("skills")
    );
}

#[test]
fn a_marker_from_the_future_is_still_refused() {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join(MIND_DIR)).unwrap();
    fs::write(
        dir.path().join(MIND_DIR).join(MIND_CONFIG),
        format!("version = {}\nname = \"x\"\n", MIND_FORMAT_VERSION + 1),
    )
    .unwrap();

    let error = MindProject::open(dir.path()).unwrap_err();

    assert!(matches!(error, WorkspaceError::Version { .. }), "{error}");
}

/// The workspace a command started in `start` uses, with nothing named or
/// activated and the default in `home`.
fn resolved(start: &Path, home: Option<&Path>, intent: Intent) -> Option<Resolved> {
    resolve(&Inputs {
        flag: None,
        activated: None,
        start,
        home,
        intent,
    })
    .unwrap()
}

#[test]
fn with_nothing_above_the_default_workspace_is_made_in_home() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let resolved =
        resolved(elsewhere.path(), Some(home.path()), Intent::Write).expect("the one in home");

    assert!(home.path().join(WEAVER_DIR).join(WEAVER_CONFIG).is_file());
    assert_eq!(resolved.reason, Reason::Home);
    assert!(resolved.workspace.is_default(Some(home.path())));
    assert!(!elsewhere.path().join(WEAVER_DIR).exists());
}

#[test]
fn a_workspace_above_is_used_before_the_default_one() {
    let home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    WeaverWorkspace::init(dir.path()).unwrap();
    let below = dir.path().join("deep/er");
    fs::create_dir_all(&below).unwrap();

    let resolved = resolved(&below, Some(home.path()), Intent::Write).unwrap();

    assert_eq!(resolved.reason, Reason::Above);
    assert!(!resolved.workspace.is_default(Some(home.path())));
    assert!(!home.path().join(WEAVER_DIR).exists());
}

#[test]
fn with_no_home_there_is_no_default_workspace() {
    let dir = TempDir::new().unwrap();

    for intent in [Intent::Read, Intent::Write] {
        assert!(resolved(dir.path(), None, intent).is_none());
    }
}

// ---------------------------------------------------------------------------
// The default workspace, as a read sees it
// ---------------------------------------------------------------------------

#[test]
fn a_read_with_nothing_above_sees_the_default_workspace_without_making_it() {
    let parent = TempDir::new().unwrap();
    let home = parent.path().join("you");
    fs::create_dir(&home).unwrap();
    let elsewhere = TempDir::new().unwrap();

    let workspace = resolved(elsewhere.path(), Some(&home), Intent::Read)
        .expect("the one in home, unmade")
        .workspace;

    assert!(!workspace.is_made());
    assert!(workspace.is_default(Some(&home)));
    // Named as `init` would name it, and as empty as one made a moment ago.
    assert_eq!(workspace.name(), "you");
    assert!(workspace.config().projects.is_empty());
    assert!(workspace.loads().is_empty());
    assert_eq!(
        workspace.own().directory_for(Kind::Skill).unwrap(),
        home.join(WEAVER_DIR).join("library/skills")
    );
    assert!(!home.join(WEAVER_DIR).exists());
    assert!(!elsewhere.path().join(WEAVER_DIR).exists());
}

#[test]
fn a_read_sees_the_default_workspace_itself_once_it_is_made() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    WeaverWorkspace::init(home.path()).unwrap();

    let workspace = resolved(elsewhere.path(), Some(home.path()), Intent::Read)
        .unwrap()
        .workspace;

    assert!(workspace.is_made());
    assert!(workspace.is_default(Some(home.path())));
}

#[test]
fn a_read_uses_a_workspace_above_before_the_default_one() {
    let home = TempDir::new().unwrap();
    let dir = TempDir::new().unwrap();
    WeaverWorkspace::init(dir.path()).unwrap();
    let below = dir.path().join("deep/er");
    fs::create_dir_all(&below).unwrap();

    let workspace = resolved(&below, Some(home.path()), Intent::Read)
        .unwrap()
        .workspace;

    assert!(workspace.is_made());
    assert!(!workspace.is_default(Some(home.path())));
    assert!(!home.path().join(WEAVER_DIR).exists());
}

#[test]
fn nothing_is_written_through_an_unmade_workspace() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let (project, _) = MindProject::init(elsewhere.path()).unwrap();
    let mut workspace = WeaverWorkspace::unmade(home.path()).unwrap();

    let link = workspace.link(&project).unwrap_err();
    let ledger = workspace.ledger().unwrap_err();

    assert!(
        matches!(link, WorkspaceError::NotAWorkspace { .. }),
        "{link}"
    );
    assert!(matches!(ledger, LedgerError::Unmade { .. }), "{ledger}");
    // Reading it is another matter: an empty ledger, as a new one would be.
    assert!(workspace
        .ledger_for_reading()
        .unwrap()
        .gathered()
        .unwrap()
        .is_empty());
    assert!(!home.path().join(WEAVER_DIR).exists());
}

// ---------------------------------------------------------------------------
// Reading the ledger
// ---------------------------------------------------------------------------

#[test]
fn reading_the_ledger_of_a_workspace_that_has_none_makes_none() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();

    let ledger = workspace.ledger_for_reading().unwrap();

    assert!(ledger.gathered().unwrap().is_empty());
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn reading_the_ledger_sees_what_was_written_to_it() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    workspace
        .ledger()
        .unwrap()
        .log(Action::Gather, Some("https://h/a"), Outcome::Ok, None)
        .unwrap();

    let history = workspace.ledger_for_reading().unwrap().history(10).unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history[0].target.as_deref(), Some("https://h/a"));
}
