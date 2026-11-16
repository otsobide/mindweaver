//! A skill somebody keeps in a project's agents' folders brought into the
//! workspace's library, and the project's copies of it recorded as
//! installed from there (#59): what is adopted, what is refused and why.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::skill;
use mindweaver_core::adopt::{self, AdoptError, Git};
use mindweaver_core::copy;
use mindweaver_core::hash;
use mindweaver_core::install::{self, Edits};
use mindweaver_core::ledger::Target;
use mindweaver_core::update;
use mindweaver_core::{
    create, Agent, AgentHomes, Directories, Kind, MindProject, Reference, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// A workspace linking `collapse`, a git repository marked by settings
/// naming every agent, sharing as `share` says, that keeps `notes` in
/// `.claude/skills` by hand.
fn given(share: Share) -> (TempDir, WeaverWorkspace, MindProject) {
    given_with(share, &Directories::default())
}

/// [`given`], the project authoring in `directories`.
fn given_with(share: Share, directories: &Directories) -> (TempDir, WeaverWorkspace, MindProject) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    let (project, _) = MindProject::init_settings(&root, &Agent::ALL, share, directories).unwrap();
    workspace.link(&project).unwrap();
    write_skill(&claudes(&project, "notes"), &skill("notes", "Keep notes"));
    (dir, workspace, project)
}

fn write_skill(folder: &Path, text: &str) {
    fs::create_dir_all(folder).unwrap();
    fs::write(folder.join("SKILL.md"), text).unwrap();
}

fn claudes(project: &MindProject, name: &str) -> PathBuf {
    project.root().join(".claude").join("skills").join(name)
}

fn agents(project: &MindProject, name: &str) -> PathBuf {
    project.root().join(".agents").join("skills").join(name)
}

/// Where the workspace keeps its own `name`.
fn library(workspace: &WeaverWorkspace, name: &str) -> PathBuf {
    workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join(name)
}

fn plan(
    workspace: &WeaverWorkspace,
    project: &MindProject,
    typed: &str,
) -> Result<adopt::Plan, AdoptError> {
    adopt::plan(
        workspace,
        project,
        &Reference::parse(typed),
        &AgentHomes::default(),
    )
}

/// The copy of the plan at `path`.
fn copy_at<'a>(plan: &'a adopt::Plan, path: &Path) -> &'a adopt::Copy {
    plan.copies
        .iter()
        .find(|copy| copy.path == path)
        .unwrap_or_else(|| panic!("no copy at {}: {:?}", path.display(), plan.copies))
}

#[test]
fn a_plan_names_the_library_copy_every_target_and_what_they_hold_and_writes_nothing() {
    let (_dir, workspace, project) = given(Share::Shared);

    let plan = plan(&workspace, &project, "notes").unwrap();

    assert_eq!(plan.name, "notes");
    assert_eq!(plan.library, library(&workspace, "notes"));
    assert_eq!(plan.source, claudes(&project, "notes"));
    assert_eq!(plan.hash, hash::tree(&claudes(&project, "notes")).unwrap());
    assert_eq!(plan.copies.len(), 2);
    let claude = copy_at(&plan, &claudes(&project, "notes"));
    assert!(claude.there);
    assert_eq!(claude.agents, [Agent::Claude]);
    let shared = copy_at(&plan, &agents(&project, "notes"));
    assert!(!shared.there);
    assert_eq!(shared.agents, [Agent::Codex, Agent::OpenCode]);
    // A shared project's copies are the team's: no exclude file to touch.
    assert!(plan.copies.iter().all(|copy| copy.git.is_none()));
    assert!(plan.provenance.is_empty());

    assert!(!library(&workspace, "notes").exists());
    assert!(!workspace.ledger_path().exists());
}

#[test]
fn a_skill_qualified_by_its_kind_is_planned_as_a_bare_name_is() {
    let (_dir, workspace, project) = given(Share::Shared);

    let plan = plan(&workspace, &project, "skill:notes").unwrap();

    assert_eq!(plan.name, "notes");
}

#[test]
fn a_rule_is_not_adopted() {
    let (_dir, workspace, project) = given(Share::Shared);

    let refused = plan(&workspace, &project, "rule:git/commits").unwrap_err();

    assert!(matches!(&refused, AdoptError::NotASkill { name, .. } if name == "git/commits"));
    assert!(refused.to_string().contains("only a skill"), "{refused}");
}

#[test]
fn a_name_the_library_holds_is_refused_with_both_paths() {
    let (_dir, workspace, project) = given(Share::Shared);
    create(&workspace.own(), Kind::Skill, "notes", "The library's").unwrap();

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    let said = refused.to_string();
    assert!(
        matches!(&refused, AdoptError::InLibrary { library: at, copy, .. }
            if *at == library(&workspace, "notes") && *copy == claudes(&project, "notes")),
        "{refused:?}"
    );
    assert!(
        said.contains(&library(&workspace, "notes").display().to_string()),
        "{said}"
    );
    assert!(
        said.contains(&claudes(&project, "notes").display().to_string()),
        "{said}"
    );
}

#[test]
fn a_library_skill_declaring_the_name_in_another_folder_holds_it_too() {
    let (_dir, workspace, project) = given(Share::Shared);
    write_skill(
        &library(&workspace, "jottings"),
        &skill("notes", "Misfiled"),
    );

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    assert!(
        matches!(&refused, AdoptError::InLibrary { library: at, .. }
            if *at == library(&workspace, "jottings")),
        "{refused:?}"
    );
}

#[test]
fn copies_in_two_folders_that_differ_are_refused() {
    let (_dir, workspace, project) = given(Share::Shared);
    write_skill(&agents(&project, "notes"), &skill("notes", "Other notes"));

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    assert!(matches!(refused, AdoptError::Differ { .. }), "{refused:?}");
}

#[test]
fn copies_in_two_folders_that_hold_the_same_are_both_adopted() {
    let (_dir, workspace, project) = given(Share::Shared);
    write_skill(&agents(&project, "notes"), &skill("notes", "Keep notes"));

    let plan = plan(&workspace, &project, "notes").unwrap();

    assert!(plan.copies.iter().all(|copy| copy.there), "{plan:?}");
}

#[test]
fn a_copy_holding_what_an_install_leaves_out_is_refused() {
    let (_dir, workspace, project) = given(Share::Shared);
    let plugin = claudes(&project, "notes").join(".claude-plugin");
    fs::create_dir(&plugin).unwrap();
    fs::write(plugin.join("plugin.json"), "{}").unwrap();

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    assert!(
        matches!(&refused, AdoptError::LeftOut { left, .. } if left == ".claude-plugin"),
        "{refused:?}"
    );
}

// Unix only: a link is made with the Unix call, which Windows refuses
// without the right to make one.
#[cfg(unix)]
#[test]
fn a_copy_holding_a_link_is_refused() {
    let (_dir, workspace, project) = given(Share::Shared);
    std::os::unix::fs::symlink("SKILL.md", claudes(&project, "notes").join("README.md")).unwrap();

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    assert!(
        matches!(refused, AdoptError::Unreproducible { .. }),
        "{refused:?}"
    );
}

#[cfg(unix)]
#[test]
fn a_copy_that_is_a_link_is_refused() {
    let (dir, workspace, project) = given(Share::Shared);
    let kept = dir.path().join("dotfiles/notes");
    fs::create_dir_all(kept.parent().unwrap()).unwrap();
    fs::rename(claudes(&project, "notes"), &kept).unwrap();
    std::os::unix::fs::symlink(&kept, claudes(&project, "notes")).unwrap();

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    assert!(refused.to_string().contains("it is a link to"), "{refused}");
}

#[test]
fn a_skill_breaking_the_rules_an_install_holds_it_to_is_refused() {
    let (_dir, workspace, project) = given(Share::Shared);
    write_skill(&claudes(&project, "jottings"), &skill("notes", "Misfiled"));
    write_skill(&claudes(&project, "blank"), &skill("blank", "\"\""));

    let misfiled = plan(&workspace, &project, "jottings").unwrap_err();
    let blank = plan(&workspace, &project, "blank").unwrap_err();

    assert!(matches!(misfiled, AdoptError::Unfit { .. }), "{misfiled:?}");
    assert!(matches!(blank, AdoptError::Unfit { .. }), "{blank:?}");
}

#[test]
fn a_skill_installed_already_is_not_adopted() {
    let (_dir, workspace, project) = given(Share::Shared);
    create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let ledger = workspace.ledger().unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install_with(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
        Edits::Kept,
    )
    .unwrap();

    let refused = plan(&workspace, &project, "deploy").unwrap_err();

    assert!(
        matches!(&refused, AdoptError::Installed { origin, .. } if origin == "workspace"),
        "{refused:?}"
    );
}

#[test]
fn the_projects_own_is_not_adopted() {
    let (_dir, workspace, project) = given_with(
        Share::Shared,
        &Directories::default().with(Kind::Skill, ".claude/skills"),
    );

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    assert!(
        matches!(&refused, AdoptError::Authored { path, .. } if *path == claudes(&project, "notes")),
        "{refused:?}"
    );
}

#[test]
fn nothing_where_an_install_would_go_is_nothing_to_adopt() {
    let (_dir, workspace, project) = given(Share::Shared);
    // OpenCode reads `.opencode/skills`, where no install of this project
    // writes.
    write_skill(
        &project.root().join(".opencode/skills/elsewhere"),
        &skill("elsewhere", "Not a target"),
    );

    let refused = plan(&workspace, &project, "elsewhere").unwrap_err();

    assert!(matches!(refused, AdoptError::NotHeld { .. }), "{refused:?}");
    assert!(
        refused
            .to_string()
            .contains(".agents/skills, .claude/skills")
            || refused
                .to_string()
                .contains(".claude/skills, .agents/skills"),
        "{refused}"
    );
}

#[test]
fn a_copy_inside_an_agents_own_folder_is_not_adopted() {
    let (_dir, workspace, project) = given(Share::Shared);
    let homes = AgentHomes {
        claude_config_dir: Some(project.root().join(".claude")),
        ..AgentHomes::default()
    };

    let refused =
        adopt::plan(&workspace, &project, &Reference::parse("notes"), &homes).unwrap_err();

    assert!(
        matches!(refused, AdoptError::UserScope { .. }),
        "{refused:?}"
    );
}

#[test]
fn a_local_project_keeps_an_untracked_copy_out_of_git_and_leaves_a_tracked_one() {
    let (_dir, workspace, project) = given(Share::Local);
    write_skill(&agents(&project, "notes"), &skill("notes", "Keep notes"));
    // The team committed `.claude/skills`, and nothing else.
    let agents_folder = project.root().join(".agents");
    let aside = project.root().join("aside");
    fs::rename(&agents_folder, &aside).unwrap();
    common::indexed(project.root());
    fs::rename(&aside, &agents_folder).unwrap();

    let plan = plan(&workspace, &project, "notes").unwrap();

    assert_eq!(
        copy_at(&plan, &claudes(&project, "notes")).git,
        Some(Git::Tracked)
    );
    assert_eq!(
        copy_at(&plan, &agents(&project, "notes")).git,
        Some(Git::KeptOut)
    );
}

#[test]
fn a_local_project_no_work_tree_holds_keeps_no_copy_out_of_git_and_says_so() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Local, &Directories::default())
            .unwrap();
    workspace.link(&project).unwrap();
    write_skill(&claudes(&project, "notes"), &skill("notes", "Keep notes"));

    let plan = plan(&workspace, &project, "notes").unwrap();

    assert!(plan.outside);
    assert!(
        plan.copies.iter().all(|copy| copy.git.is_none()),
        "{:?}",
        plan.copies
    );
    let done = adopted(&workspace, &project, "notes");
    assert!(done.plan.outside);
    assert!(done.excluded.is_none(), "{:?}", done.excluded);
}

#[test]
fn a_plan_refuses_an_exclude_block_the_adoption_could_not_edit() {
    let (_dir, workspace, project) = given(Share::Local);
    let file = project.root().join(".git/info/exclude");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, "# mindweaver:begin\n# project .\n").unwrap();

    let refused = plan(&workspace, &project, "notes").unwrap_err();

    assert!(matches!(refused, AdoptError::Exclude(_)), "{refused:?}");
}

#[test]
fn what_gh_skill_says_of_where_it_came_from_is_shown() {
    let (_dir, workspace, project) = given(Share::Shared);
    write_skill(
        &claudes(&project, "notes"),
        "---\nname: notes\ndescription: Keep notes\nmetadata:\n  github-repo: someone/skills\n  \
         author: someone\n  github-ref: v1\n---\n\n# notes\n",
    );

    let plan = plan(&workspace, &project, "notes").unwrap();

    assert_eq!(
        plan.provenance,
        [
            (String::from("github-repo"), String::from("someone/skills")),
            (String::from("github-ref"), String::from("v1")),
        ]
    );
}

// ---------------------------------------------------------------------------
// Adopting
// ---------------------------------------------------------------------------

fn adopted(workspace: &WeaverWorkspace, project: &MindProject, typed: &str) -> adopt::Adopted {
    adopt::apply(
        workspace,
        project,
        &Reference::parse(typed),
        &AgentHomes::default(),
    )
    .unwrap()
}

/// Every file under `root` but git's own, with what it holds: what adopting
/// must leave as it was.
fn files(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, found: &mut Vec<(PathBuf, Vec<u8>)>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|name| name == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(&path, found);
            } else {
                found.push((path.clone(), fs::read(&path).unwrap()));
            }
        }
    }
    let mut found = Vec::new();
    walk(root, &mut found);
    found.sort();
    found
}

fn standing(workspace: &WeaverWorkspace, project: &MindProject, name: &str) -> install::Standing {
    let ledger = workspace.ledger_for_reading().unwrap();
    install::standings(workspace, &ledger, project, Kind::Skill)
        .unwrap()
        .into_iter()
        .find(|(each, _)| each == name)
        .map(|(_, standing)| standing)
        .unwrap_or_else(|| panic!("{name} is not recorded"))
}

fn update(workspace: &WeaverWorkspace, project: &MindProject) -> update::Applied {
    update::apply(
        workspace,
        std::slice::from_ref(project),
        &[],
        Edits::Kept,
        &AgentHomes::default(),
    )
    .unwrap()
}

#[test]
fn adopting_copies_into_the_library_records_the_copies_and_leaves_the_project_as_it_was() {
    let (_dir, workspace, project) = given(Share::Shared);
    let before = files(project.root());

    let done = adopted(&workspace, &project, "notes");

    assert_eq!(files(project.root()), before);
    let library = library(&workspace, "notes");
    assert!(copy::same(&claudes(&project, "notes"), &library).unwrap());
    assert_eq!(done.plan.library, library);
    assert!(done.excluded.is_none());
    let record = workspace
        .ledger()
        .unwrap()
        .installation("collapse", Kind::Skill, "notes")
        .unwrap()
        .unwrap();
    assert_eq!(record.origin.as_deref(), Some("workspace"));
    assert_eq!(record.source_id, None);
    assert_eq!(
        record.content_hash,
        Some(hash::tree(&claudes(&project, "notes")).unwrap())
    );
    assert_eq!(
        record.targets,
        [
            Target {
                agent: None,
                path: String::from(".agents/skills/notes"),
                content_hash: None,
            },
            Target {
                agent: Some(String::from("claude")),
                path: String::from(".claude/skills/notes"),
                content_hash: None,
            },
        ]
    );
}

#[test]
fn a_target_with_no_copy_reads_as_missing_until_an_update_puts_it_there() {
    let (_dir, workspace, project) = given(Share::Shared);
    adopted(&workspace, &project, "notes");

    assert_eq!(
        standing(&workspace, &project, "notes"),
        install::Standing::Missing
    );
    update(&workspace, &project);

    assert!(copy::same(&claudes(&project, "notes"), &agents(&project, "notes")).unwrap());
    assert_eq!(
        standing(&workspace, &project, "notes"),
        install::Standing::Current
    );
}

#[test]
fn copies_in_every_target_are_current_once_adopted() {
    let (_dir, workspace, project) = given(Share::Shared);
    write_skill(&agents(&project, "notes"), &skill("notes", "Keep notes"));

    adopted(&workspace, &project, "notes");

    assert_eq!(
        standing(&workspace, &project, "notes"),
        install::Standing::Current
    );
}

#[test]
fn an_update_brings_an_adopted_copy_along_once_the_library_moves_on() {
    let (_dir, workspace, project) = given(Share::Shared);
    write_skill(&agents(&project, "notes"), &skill("notes", "Keep notes"));
    adopted(&workspace, &project, "notes");
    let newer = skill("notes", "Keep better notes");
    fs::write(library(&workspace, "notes").join("SKILL.md"), &newer).unwrap();
    assert_eq!(
        standing(&workspace, &project, "notes"),
        install::Standing::Outdated
    );

    update(&workspace, &project);

    for copy in [claudes(&project, "notes"), agents(&project, "notes")] {
        assert_eq!(fs::read_to_string(copy.join("SKILL.md")).unwrap(), newer);
    }
    assert_eq!(
        standing(&workspace, &project, "notes"),
        install::Standing::Current
    );
}

#[test]
fn a_copy_edited_after_it_was_adopted_reads_as_edited() {
    let (_dir, workspace, project) = given(Share::Shared);
    adopted(&workspace, &project, "notes");

    fs::write(claudes(&project, "notes").join("notes.md"), "mine").unwrap();

    assert_eq!(
        standing(&workspace, &project, "notes"),
        install::Standing::Edited
    );
}

#[test]
fn adopting_a_skill_twice_is_refused_as_installed_already() {
    let (_dir, workspace, project) = given(Share::Shared);
    adopted(&workspace, &project, "notes");

    let refused = adopt::apply(
        &workspace,
        &project,
        &Reference::parse("notes"),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(refused, AdoptError::Installed { .. }),
        "{refused:?}"
    );
}

#[test]
fn a_refused_adoption_writes_nothing() {
    let (_dir, workspace, project) = given(Share::Shared);
    create(&workspace.own(), Kind::Skill, "notes", "The library's").unwrap();
    let held = files(&library(&workspace, "notes"));

    let refused = adopt::apply(
        &workspace,
        &project,
        &Reference::parse("notes"),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(refused, AdoptError::InLibrary { .. }),
        "{refused:?}"
    );
    assert_eq!(files(&library(&workspace, "notes")), held);
    assert!(workspace
        .ledger()
        .unwrap()
        .installation("collapse", Kind::Skill, "notes")
        .unwrap()
        .is_none());
}

/// The lines of the project's group in the repository's exclude file.
fn excluded(project: &MindProject) -> Vec<String> {
    let id = project.settings().unwrap().id.clone();
    let text = fs::read_to_string(project.root().join(".git/info/exclude")).unwrap_or_default();
    let header = format!("# project {id}");
    text.lines()
        .skip_while(|line| *line != header)
        .skip(1)
        .take_while(|line| !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_local_project_keeps_an_adopted_copy_git_tracks_in_git_and_the_rest_out() {
    let (_dir, workspace, project) = given(Share::Local);
    write_skill(&agents(&project, "notes"), &skill("notes", "Keep notes"));
    // The team committed `.claude/skills`, and nothing else.
    let agents_folder = project.root().join(".agents");
    let aside = project.root().join("aside");
    fs::rename(&agents_folder, &aside).unwrap();
    common::indexed(project.root());
    fs::rename(&aside, &agents_folder).unwrap();

    let done = adopted(&workspace, &project, "notes");

    assert!(matches!(
        done.excluded,
        Some(mindweaver_core::Excluded::Written { .. })
    ));
    assert_eq!(excluded(&project), ["/.agents/skills/notes/"]);

    // And an update writing the tracked one leaves it in git still.
    fs::write(
        library(&workspace, "notes").join("SKILL.md"),
        skill("notes", "Keep better notes"),
    )
    .unwrap();
    update(&workspace, &project);
    assert_eq!(excluded(&project), ["/.agents/skills/notes/"]);
}

// Unix only: a folder's write permission is what is taken away, and Windows
// keeps none on a folder.
#[cfg(unix)]
#[test]
fn a_library_that_cannot_be_written_takes_back_the_exclude_lines_it_added() {
    use std::os::unix::fs::PermissionsExt;

    let (_dir, workspace, project) = given(Share::Local);
    let folder = workspace.own().directory_for(Kind::Skill).unwrap();
    fs::create_dir_all(&folder).unwrap();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();
    let probe = folder.join(".probe");
    let writable = fs::write(&probe, "").is_ok();
    let _ = fs::remove_file(&probe);

    let refused = adopt::apply(
        &workspace,
        &project,
        &Reference::parse("notes"),
        &AgentHomes::default(),
    );
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
    if writable {
        eprintln!("skipped: {} can be written all the same", folder.display());
        return;
    }

    assert!(
        matches!(refused, Err(AdoptError::Write { .. })),
        "{refused:?}"
    );
    assert!(excluded(&project).is_empty(), "{:?}", excluded(&project));
    assert!(!library(&workspace, "notes").exists());
    assert!(workspace
        .ledger()
        .unwrap()
        .installation("collapse", Kind::Skill, "notes")
        .unwrap()
        .is_none());
}
