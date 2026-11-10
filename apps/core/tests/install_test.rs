//! Putting a gathered skill into a mind project, and taking it back out.

mod common;

use std::collections::BTreeSet;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use common::{repository, skill, url};
use mindweaver_core::copy;
use mindweaver_core::gather::{self, Request};
use mindweaver_core::install::{self, Installed, Removed, Standing};
use mindweaver_core::ledger::{Ledger, Target};
use mindweaver_core::AgentHomes;
use mindweaver_core::{
    Agent, Directories, Kind, MindProject, Settings, Share, Targets, WeaverWorkspace, MIND_DIR,
    SETTINGS_FILE,
};
use tempfile::TempDir;

/// A workspace with one project in it, and a shelf holding `names`.
fn ready(names: &[&str]) -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();

    let files: Vec<(String, String)> = names
        .iter()
        .map(|name| {
            (
                format!("skills/{name}/SKILL.md"),
                skill(name, &format!("What {name} does")),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, body)| (path.as_str(), body.as_str()))
        .collect();
    let source = repository(&borrowed);

    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    // The clone lives in the shelf, so the temporary can go.
    drop(source);
    (dir, workspace, project, ledger)
}

fn candidate<'a>(candidates: &'a [install::Candidate], name: &str) -> &'a install::Candidate {
    candidates
        .iter()
        .find(|candidate| candidate.name() == name)
        .unwrap_or_else(|| panic!("no candidate named {name}"))
}

fn installed_at(project: &MindProject, name: &str) -> std::path::PathBuf {
    project.directory_for(Kind::Skill).unwrap().join(name)
}

#[test]
fn the_shelf_is_what_a_project_can_be_offered() {
    let (_dir, workspace, project, ledger) = ready(&["deploy", "commit-style"]);

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(candidates.len(), 2);
    assert!(candidates
        .iter()
        .all(|candidate| candidate.standing == Standing::Absent));
}

#[test]
fn installing_copies_the_skill_into_the_directory_the_project_named() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    // The project keeps its skills where its agents look, not where we assume.
    let (project, _) = MindProject::init_with(
        &root,
        &Directories::default().with(Kind::Skill, ".claude/skills"),
    )
    .unwrap();
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let outcome = install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(matches!(outcome, Installed::Added { .. }), "{outcome:?}");
    assert!(root.join(".claude/skills/deploy/SKILL.md").is_file());
}

#[test]
fn a_skills_whole_directory_arrives_with_it() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("skills/deploy/scripts/run.sh", "echo hi\n"),
    ]);
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(installed_at(&project, "deploy")
        .join("scripts")
        .join("run.sh")
        .is_file());
}

#[test]
fn a_second_survey_knows_what_is_already_there() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap();

    let again = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(again[0].standing, Standing::Current);
    assert!(again[0].standing.present());
    assert!(again[0].standing.ours());
}

#[test]
fn installing_the_same_skill_twice_changes_nothing() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap();

    let again = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let outcome = install::install(
        &workspace,
        &ledger,
        &project,
        &again[0],
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(
        matches!(outcome, Installed::Unchanged { .. }),
        "{outcome:?}"
    );
}

#[test]
fn uninstalling_takes_it_back_out() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap();

    let outcome = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(matches!(outcome, Removed::Removed { .. }), "{outcome:?}");
    assert!(!installed_at(&project, "deploy").exists());
    let after = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    assert_eq!(after[0].standing, Standing::Absent);
}

#[test]
fn a_project_whose_settings_name_no_agent_is_offered_nothing_and_refused_one_by_name() {
    let (dir, workspace, _, ledger) = ready(&["deploy"]);
    let root = dir.path().join("alpha");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    let settings = Settings::new(
        "alpha",
        Vec::new(),
        Share::Local,
        Directories::default().with(Kind::Skill, "skills"),
    );
    fs::write(
        root.join(MIND_DIR).join(SETTINGS_FILE),
        settings.text().unwrap(),
    )
    .unwrap();
    let project = MindProject::open(&root).unwrap();

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let refused =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap_err();

    assert!(candidates.is_empty(), "{candidates:?}");
    assert!(
        matches!(
            refused,
            install::InstallError::NoTarget {
                kind: Kind::Skill,
                ..
            }
        ),
        "{refused}"
    );
    assert_eq!(
        refused.to_string(),
        "alpha names no agent in its settings that skills are installed for, so none can be \
         installed into it"
    );
    // Its own folder is what it authors in, never where an install goes.
    assert!(!root.join("skills").join("deploy").exists());
}

// ---------------------------------------------------------------------------
// A project marked by its settings: a copy for every agent it names
// ---------------------------------------------------------------------------

/// A workspace with a shelf holding `names`, and one project marked by its
/// settings, naming `agents` and authoring its own skills in `skills`.
fn ready_v3(names: &[&str], agents: &[Agent]) -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let (dir, workspace, _, ledger) = ready(names);
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init_settings(
        &root,
        agents,
        Share::Shared,
        &Directories::default().with(Kind::Skill, "skills"),
    )
    .unwrap();
    (dir, workspace, project, ledger)
}

/// Where a project's agents read `name`: Codex's and OpenCode's folder, then
/// Claude's.
fn agents_copies(project: &MindProject, name: &str) -> [std::path::PathBuf; 2] {
    let root = project.root();
    [
        root.join(".agents").join("skills").join(name),
        root.join(".claude").join("skills").join(name),
    ]
}

fn install_from_shelf(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    name: &str,
) -> install::Installation {
    let candidate = install::offered(workspace, ledger, project, Kind::Skill, name, None).unwrap();
    install::install(
        workspace,
        ledger,
        project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap()
}

/// [`ready_v3`], with every agent, installing skills in `folders` instead
/// of where the agents read.
fn ready_with_targets(
    names: &[&str],
    folders: &[&str],
) -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let (dir, workspace, mut project, ledger) = ready_v3(names, &Agent::ALL);
    project
        .change_settings(|settings| {
            settings.targets = Targets::default().with(
                Kind::Skill,
                folders.iter().map(std::path::PathBuf::from).collect(),
            );
        })
        .unwrap();
    (dir, workspace, project, ledger)
}

#[test]
fn a_project_whose_settings_install_skills_in_one_folder_gets_one_copy_there_alone() {
    let (_dir, workspace, project, ledger) = ready_with_targets(&["deploy"], &[".skills"]);

    let done = install_from_shelf(&workspace, &ledger, &project, "deploy");

    let skills = project.root().join(".skills").join("deploy");
    assert_eq!(
        done.installed,
        Installed::Added {
            name: "deploy".into(),
            paths: vec![skills.clone()],
            unhashed: None,
        }
    );
    assert!(skills.join("SKILL.md").is_file());
    for standard in agents_copies(&project, "deploy") {
        assert!(!standard.exists(), "{}", standard.display());
    }
    let record = ledger
        .installation("alpha", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    // One copy for all three agents, so no one agent's.
    assert_eq!(
        record.targets,
        [Target {
            agent: None,
            path: String::from(".skills/deploy"),
            content_hash: None,
        }]
    );
    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    assert_eq!(surveyed[0].standing, Standing::Current);
}

#[test]
fn a_target_no_install_may_use_gets_no_copy_while_the_others_do() {
    let (dir, workspace, project, ledger) = ready_with_targets(&["deploy"], &[".skills"]);
    let path = project.root().join(MIND_DIR).join(SETTINGS_FILE);
    let text = fs::read_to_string(&path).unwrap().replace(
        "[\".skills\"]",
        "[\".skills\", \"../shared\", \".mind/skills\"]",
    );
    fs::write(&path, text).unwrap();
    let project = MindProject::open(project.root()).unwrap();

    install_from_shelf(&workspace, &ledger, &project, "deploy");

    assert!(project.root().join(".skills/deploy/SKILL.md").is_file());
    assert!(!dir.path().join("shared").exists());
    assert!(!project.root().join(".mind/skills").exists());
    assert_eq!(project.misplaced_targets().len(), 2);
}

#[test]
fn a_project_installing_skills_only_where_no_install_may_go_is_offered_nothing() {
    let (_dir, workspace, project, ledger) = ready_with_targets(&["deploy"], &[".skills"]);
    let path = project.root().join(MIND_DIR).join(SETTINGS_FILE);
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("[\".skills\"]", "[\"../shared\"]");
    fs::write(&path, text).unwrap();
    let project = MindProject::open(project.root()).unwrap();

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let refused =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap_err();

    assert!(candidates.is_empty(), "{candidates:?}");
    assert_eq!(
        refused.to_string(),
        "none of the folders alpha's settings name for its skills is one an install may use, so \
         none can be installed into it"
    );
}

#[test]
fn installing_into_a_project_marked_by_its_settings_puts_one_copy_where_each_agent_reads() {
    let (_dir, workspace, project, ledger) = ready_v3(&["deploy"], &Agent::ALL);

    let done = install_from_shelf(&workspace, &ledger, &project, "deploy");

    let [agents, claude] = agents_copies(&project, "deploy");
    assert_eq!(
        done.installed,
        Installed::Added {
            name: "deploy".into(),
            paths: vec![agents.clone(), claude.clone()],
            unhashed: None,
        }
    );
    assert!(agents.join("SKILL.md").is_file());
    assert!(copy::same(&agents, &claude).unwrap());
    // The folder the project authors in is its own, and gets no copy.
    assert!(!project.root().join("skills").join("deploy").exists());

    let record = ledger
        .installation("alpha", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    let hash = mindweaver_core::hash::tree(&agents).unwrap();
    assert_eq!(record.content_hash.as_deref(), Some(hash.as_str()));
    assert_eq!(mindweaver_core::hash::tree(&claude).unwrap(), hash);
    assert_eq!(
        record.targets,
        [
            // Two agents read this one, so it is neither's alone.
            Target {
                agent: None,
                path: String::from(".agents/skills/deploy"),
                content_hash: None,
            },
            Target {
                agent: Some(String::from("claude")),
                path: String::from(".claude/skills/deploy"),
                content_hash: None,
            },
        ]
    );
}

#[test]
fn a_project_naming_one_agent_gets_the_one_copy_that_agent_reads() {
    let (_dir, workspace, project, ledger) = ready_v3(&["deploy"], &[Agent::Codex]);

    install_from_shelf(&workspace, &ledger, &project, "deploy");

    let [agents, claude] = agents_copies(&project, "deploy");
    assert!(agents.is_dir());
    assert!(!claude.exists());
    let record = ledger
        .installation("alpha", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(
        record.targets,
        [Target {
            agent: Some(String::from("codex")),
            path: String::from(".agents/skills/deploy"),
            content_hash: None,
        }]
    );
}

#[test]
fn installing_into_every_agents_folder_again_changes_nothing_and_uninstalling_takes_every_copy() {
    let (_dir, workspace, project, ledger) = ready_v3(&["deploy"], &Agent::ALL);
    install_from_shelf(&workspace, &ledger, &project, "deploy");

    let again = install_from_shelf(&workspace, &ledger, &project, "deploy");
    assert!(
        matches!(again.installed, Installed::Unchanged { .. }),
        "{again:?}"
    );
    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    assert_eq!(surveyed[0].standing, Standing::Current);

    let removed = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    let [agents, claude] = agents_copies(&project, "deploy");
    assert_eq!(
        removed,
        Removed::Removed {
            name: "deploy".into(),
            paths: vec![agents.clone(), claude.clone()],
            still_excluded: None,
            rendered: None,
        }
    );
    assert!(!agents.exists() && !claude.exists());
    assert!(ledger
        .installation("alpha", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
}

#[test]
fn a_copy_gone_from_one_agents_folder_is_put_back_by_the_next_install() {
    let (_dir, workspace, project, ledger) = ready_v3(&["deploy"], &Agent::ALL);
    install_from_shelf(&workspace, &ledger, &project, "deploy");
    let [agents, claude] = agents_copies(&project, "deploy");
    fs::remove_dir_all(&claude).unwrap();

    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let again = install::install(
        &workspace,
        &ledger,
        &project,
        &surveyed[0],
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(surveyed[0].standing, Standing::Missing);
    assert!(
        matches!(again.installed, Installed::Updated { .. }),
        "{again:?}"
    );
    assert!(copy::same(&agents, &claude).unwrap());
}

#[test]
fn somebodys_skill_in_either_agents_folder_refuses_the_install_and_leaves_the_other_alone() {
    for held in 0..2 {
        let (_dir, workspace, project, ledger) = ready_v3(&["deploy"], &Agent::ALL);
        let copies = agents_copies(&project, "deploy");
        fs::create_dir_all(&copies[held]).unwrap();
        fs::write(copies[held].join("SKILL.md"), skill("deploy", "Mine")).unwrap();

        let surveyed = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
        let outcome = install::install(
            &workspace,
            &ledger,
            &project,
            &surveyed[0],
            &AgentHomes::default(),
        )
        .unwrap()
        .installed;

        assert_eq!(surveyed[0].standing, Standing::Foreign);
        assert_eq!(
            outcome,
            Installed::Foreign {
                name: "deploy".into(),
                path: copies[held].clone(),
            }
        );
        assert!(fs::read_to_string(copies[held].join("SKILL.md"))
            .unwrap()
            .contains("Mine"));
        assert!(!copies[1 - held].exists(), "{held}");
        assert!(ledger
            .installation("alpha", Kind::Skill, "deploy")
            .unwrap()
            .is_none());
        // Nor is it taken out, from either folder.
        let removed = install::uninstall(
            &workspace,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            &AgentHomes::default(),
        )
        .unwrap();
        assert!(matches!(removed, Removed::Foreign { .. }), "{removed:?}");
        assert!(copies[held].join("SKILL.md").is_file());
    }
}

#[test]
fn a_skill_somebody_writes_beside_an_installed_one_in_a_folder_it_was_not_installed_in_is_theirs() {
    // Installed for Codex alone, then Claude is named: what was put in
    // Claude's folder by hand meanwhile is not the installation's.
    let (_dir, workspace, project, ledger) = ready_v3(&["deploy"], &[Agent::Codex]);
    install_from_shelf(&workspace, &ledger, &project, "deploy");
    let [agents, claude] = agents_copies(&project, "deploy");
    fs::create_dir_all(&claude).unwrap();
    fs::write(claude.join("SKILL.md"), skill("deploy", "Mine")).unwrap();
    let mut project = project;
    project
        .change_settings(|settings| settings.agents = Agent::ALL.to_vec())
        .unwrap();

    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let removed = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(surveyed[0].standing, Standing::Foreign);
    // Only what the installation wrote goes.
    assert_eq!(
        removed,
        Removed::Removed {
            name: "deploy".into(),
            paths: vec![agents.clone()],
            still_excluded: None,
            rendered: None,
        }
    );
    assert!(!agents.exists());
    assert!(claude.join("SKILL.md").is_file());
}

#[cfg(unix)]
#[test]
fn two_agents_folders_that_are_one_on_disk_get_one_copy() {
    let (_dir, workspace, project, ledger) = ready_v3(&["deploy"], &Agent::ALL);
    let root = project.root();
    fs::create_dir_all(root.join(".agents").join("skills")).unwrap();
    fs::create_dir_all(root.join(".claude")).unwrap();
    std::os::unix::fs::symlink(
        root.join(".agents").join("skills"),
        root.join(".claude").join("skills"),
    )
    .unwrap();

    let targets = install::targets(&project, Kind::Skill).unwrap();
    let done = install_from_shelf(&workspace, &ledger, &project, "deploy");

    assert_eq!(targets.len(), 1, "{targets:?}");
    assert_eq!(targets[0].agents, Agent::ALL);
    assert!(
        matches!(done.installed, Installed::Added { .. }),
        "{done:?}"
    );
    assert!(root.join(".claude/skills/deploy/SKILL.md").is_file());
    let record = ledger
        .installation("alpha", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(
        record.targets,
        [Target {
            agent: None,
            path: String::from(".agents/skills/deploy"),
            content_hash: None,
        }]
    );
}

#[test]
fn a_record_naming_a_copy_outside_the_project_is_refused_rather_than_removed() {
    let (dir, workspace, project, ledger) = ready_v3(&["deploy"], &[Agent::Codex]);
    let outside = dir.path().join("deploy");
    fs::create_dir(&outside).unwrap();
    ledger
        .installed(
            "alpha",
            &mindweaver_core::ledger::InstallRecord {
                kind: Kind::Skill,
                name: "deploy".into(),
                source_id: None,
                origin: None,
                content_hash: None,
                targets: vec![Target {
                    agent: None,
                    path: String::from("../deploy"),
                    content_hash: None,
                }],
                entries: Vec::new(),
            },
        )
        .unwrap();

    let refused = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(refused, install::InstallError::Recorded { .. }),
        "{refused}"
    );
    assert!(outside.is_dir());
}

// ---------------------------------------------------------------------------
// What was not installed by Mindweaver is not Mindweaver's to touch
// ---------------------------------------------------------------------------

/// A skill somebody wrote by hand, of a name the shelf also offers.
fn write_by_hand(project: &MindProject, name: &str, description: &str) {
    let dir = installed_at(project, name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), skill(name, description)).unwrap();
}

#[test]
fn a_hand_written_skill_is_seen_as_present_but_not_ours() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    write_by_hand(&project, "deploy", "Written by a person");

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    assert_eq!(candidates[0].standing, Standing::Foreign);
    assert!(candidates[0].standing.present(), "the box is ticked");
    assert!(!candidates[0].standing.ours(), "but it is not ours to move");
}

#[test]
fn installing_does_not_overwrite_a_hand_written_skill() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    write_by_hand(&project, "deploy", "Written by a person");

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let outcome = install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(matches!(outcome, Installed::Foreign { .. }), "{outcome:?}");
    let kept = fs::read_to_string(installed_at(&project, "deploy").join("SKILL.md")).unwrap();
    assert!(kept.contains("Written by a person"), "{kept}");
}

#[test]
fn uninstalling_does_not_delete_a_hand_written_skill() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    write_by_hand(&project, "deploy", "Written by a person");

    let outcome = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(matches!(outcome, Removed::Foreign { .. }), "{outcome:?}");
    assert!(
        installed_at(&project, "deploy").join("SKILL.md").is_file(),
        "somebody's work was deleted"
    );
}

#[test]
fn uninstalling_something_that_is_not_there_says_so() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);

    let outcome = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(matches!(outcome, Removed::Absent { .. }), "{outcome:?}");
}

// ---------------------------------------------------------------------------
// Several projects, and the log
// ---------------------------------------------------------------------------

#[test]
fn two_projects_each_keep_their_own_answer() {
    let (dir, workspace, one, ledger) = ready(&["deploy"]);
    let other_root = dir.path().join("tanukeys");
    fs::create_dir(&other_root).unwrap();
    let (two, _) = MindProject::init(&other_root).unwrap();

    let candidates = install::survey(&workspace, &ledger, &one, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &one,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(
        install::survey(&workspace, &ledger, &one, Kind::Skill).unwrap()[0].standing,
        Standing::Current
    );
    assert_eq!(
        install::survey(&workspace, &ledger, &two, Kind::Skill).unwrap()[0].standing,
        Standing::Absent,
        "installing into one project must not claim the other"
    );
}

#[test]
fn the_log_records_both_directions() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap();
    install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    let history = ledger.history(10).unwrap();
    let actions: Vec<&str> = history.iter().map(|entry| entry.action.as_str()).collect();
    assert_eq!(actions[0], "uninstall");
    assert_eq!(actions[1], "install");
    assert!(history[0].detail.as_deref().unwrap().contains("deploy"));
}

#[test]
fn a_shelf_entry_whose_files_have_gone_is_named_in_the_error() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let install::Offer::Shelf(gathered) = &candidates[0].offer else {
        panic!("a gathered skill is offered from the shelf");
    };
    fs::remove_dir_all(workspace.root().join(&gathered.path)).unwrap();

    let error = install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("deploy"), "{error}");
}

#[test]
fn the_folder_is_named_after_the_skill_rather_than_after_its_shelf_folder() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();

    // A source whose folder disagrees with what the skill declares. The shelf
    // keeps the source's spelling, because renaming on the way in would repair
    // the symptom and hide it from `validate`.
    let source = repository(&[(
        "skills/a-different-folder/SKILL.md",
        &skill("deploy", "Ship it"),
    )]);
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    let shelved = workspace.root().join(&ledger.gathered().unwrap()[0].path);
    assert!(
        shelved.ends_with("a-different-folder"),
        "the shelf keeps the source's folder: {}",
        shelved.display()
    );

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        candidate(&candidates, "deploy"),
        &AgentHomes::default(),
    )
    .unwrap();

    // But inside a project a skill's directory has to match its declared name:
    // that is what `validate` checks and what an agent uses to find it. So the
    // copy is filed under `deploy`, and the mismatch does not travel.
    assert!(installed_at(&project, "deploy").join("SKILL.md").is_file());
    assert!(!installed_at(&project, "a-different-folder").exists());
}

// ---------------------------------------------------------------------------
// Strict on the way in
// ---------------------------------------------------------------------------

/// A workspace with one project, and a shelf gathered from these files.
fn gathered(files: &[(&str, &str)]) -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    let source = repository(files);
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();
    (dir, workspace, project, ledger)
}

#[test]
fn a_gathered_skill_whose_name_breaks_the_rules_is_offered_but_not_installed() {
    let (_dir, workspace, project, ledger) = ready(&["a--b"]);

    // Offered, so nothing gathered is hidden; refused, so nothing an agent
    // would skip lands in the project.
    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let error = install::install(
        &workspace,
        &ledger,
        &project,
        candidate(&candidates, "a--b"),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(&error, install::InstallError::Unfit { name, .. } if name == "a--b"),
        "{error}"
    );
    assert!(
        error.to_string().contains("joined by single hyphens"),
        "{error}"
    );
    assert!(!installed_at(&project, "a--b").exists());
    assert!(ledger
        .installations("collapse", Kind::Skill)
        .unwrap()
        .is_empty());
}

#[test]
fn a_skill_with_an_empty_description_is_not_installed() {
    let (_dir, workspace, project, ledger) = gathered(&[(
        "skills/quiet/SKILL.md",
        "---\nname: quiet\ndescription: ''\n---\n",
    )]);

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let error = install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "`quiet` cannot be installed: `description` is empty"
    );
    assert!(!installed_at(&project, "quiet").exists());
}

#[test]
fn a_shelf_copy_that_now_declares_another_name_is_not_installed() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    // The shelf is a folder anybody can edit, and a source can rename a skill
    // while the record of its old name stays: either way the record says
    // `deploy`, the copy would be filed under `deploy`, and it declares `ship`.
    let shelved = workspace.root().join(&ledger.gathered().unwrap()[0].path);
    fs::write(shelved.join("SKILL.md"), skill("ship", "Ship it")).unwrap();

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let error = install::install(
        &workspace,
        &ledger,
        &project,
        candidate(&candidates, "deploy"),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert_eq!(
        error.to_string(),
        "`deploy` cannot be installed: `name` is `ship` but the directory is `deploy`; they have to match"
    );
    assert!(!installed_at(&project, "deploy").exists());
}

#[test]
fn what_agents_read_past_does_not_stop_an_install() {
    // Nested metadata and an over-long compatibility are reported by
    // `validate`, and are no reason for an agent to skip the skill.
    let long = "x".repeat(501);
    let manifest = format!(
        "---\nname: deploy\ndescription: Ship it\ncompatibility: {long}\nmetadata:\n  owner:\n    team: ops\n---\n"
    );
    let (_dir, workspace, project, ledger) =
        gathered(&[("skills/deploy/SKILL.md", manifest.as_str())]);

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let outcome = install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap()
    .installed;

    assert!(matches!(outcome, Installed::Added { .. }), "{outcome:?}");
    // Line endings are the gathering checkout's (CRLF under autocrlf on
    // Windows), not Mindweaver's, so they are left out of the comparison.
    let copied = fs::read_to_string(installed_at(&project, "deploy").join("SKILL.md")).unwrap();
    assert_eq!(copied.replace("\r\n", "\n"), manifest, "copied as written");
}

// ---------------------------------------------------------------------------
// Staged outside the folders agents read
// ---------------------------------------------------------------------------

/// A workspace managing one project and offering skills of its own, each with
/// `files` references besides its `SKILL.md`, so that copying one takes long
/// enough for somebody looking at the project to see it happen.
fn own_skills(names: &[&str], files: usize) -> (TempDir, WeaverWorkspace, MindProject) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    let (project, _) = MindProject::init(&root).unwrap();
    workspace.link(&project).unwrap();
    for name in names {
        let skill =
            mindweaver_core::create(&workspace.own(), Kind::Skill, name, &format!("Our {name}"))
                .unwrap();
        for file in 0..files {
            fs::write(
                skill.path().join(format!("reference-{file}.md")),
                "Read this first.\n",
            )
            .unwrap();
        }
    }
    (dir, workspace, project)
}

/// Change one of the workspace's own skills, so installing it again updates.
fn edit_own(workspace: &WeaverWorkspace, name: &str, round: usize) {
    let skill = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join(name);
    fs::write(skill.join("reference-0.md"), format!("Round {round}.\n")).unwrap();
}

fn install_named(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    name: &str,
) -> Result<install::Installation, install::InstallError> {
    let candidate = install::offered(
        workspace,
        ledger,
        project,
        Kind::Skill,
        name,
        Some("workspace"),
    )?;
    install::install(
        workspace,
        ledger,
        project,
        &candidate,
        &AgentHomes::default(),
    )
}

/// The names directly inside `dir`, sorted.
fn listing(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// What is left in a project's scratch folder besides the folder's own files.
fn staged(project: &MindProject) -> Vec<String> {
    listing(&project.scratch_dir())
        .into_iter()
        .filter(|name| !name.starts_with('.'))
        .collect()
}

#[test]
fn while_an_install_runs_and_after_it_the_skills_folder_holds_only_the_skill() {
    let (_dir, workspace, project) = own_skills(&["deploy"], 200);
    let ledger = workspace.ledger().unwrap();
    let skills = project.directory_for(Kind::Skill).unwrap();
    let done = AtomicBool::new(false);

    // Somebody listing the folder the whole time, as an agent globbing it
    // for skills would.
    let seen = std::thread::scope(|scope| {
        let watcher = scope.spawn(|| {
            let mut seen = BTreeSet::new();
            while !done.load(Ordering::Relaxed) {
                if let Ok(entries) = fs::read_dir(&skills) {
                    seen.extend(
                        entries
                            .flatten()
                            .map(|entry| entry.file_name().to_string_lossy().into_owned()),
                    );
                }
            }
            seen
        });
        for round in 0..10 {
            edit_own(&workspace, "deploy", round);
            let outcome =
                install_named(&workspace, &ledger, &project, "deploy").map(|done| done.installed);
            assert!(
                matches!(
                    outcome,
                    Ok(Installed::Added { .. } | Installed::Updated { .. })
                ),
                "{outcome:?}"
            );
        }
        done.store(true, Ordering::Relaxed);
        watcher.join().unwrap()
    });

    assert_eq!(seen, BTreeSet::from([String::from("deploy")]));
    assert_eq!(listing(&skills), ["deploy"]);
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));
}

#[test]
fn the_copy_is_built_in_the_projects_own_scratch_folder_which_ignores_itself() {
    let (_dir, workspace, project) = own_skills(&["deploy"], 0);
    let ledger = workspace.ledger().unwrap();
    assert!(
        !project.scratch_dir().exists(),
        "made only when first needed"
    );

    install_named(&workspace, &ledger, &project, "deploy").unwrap();

    assert_eq!(
        project.scratch_dir(),
        project.root().join(".mind").join("tmp")
    );
    let ignore = fs::read_to_string(project.scratch_dir().join(".gitignore")).unwrap();
    assert!(ignore.lines().any(|line| line == "*"), "{ignore}");
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));
}

#[test]
fn an_install_never_builds_in_a_stage_another_install_is_using() {
    let (_dir, workspace, project) = own_skills(&["deploy"], 0);
    let ledger = workspace.ledger().unwrap();
    // Another install into the same project, halfway through its copy.
    let other = copy::Stage::open(&project.scratch_dir()).unwrap();
    fs::create_dir(other.path().join("new")).unwrap();
    fs::write(other.path().join("new").join("SKILL.md"), "theirs").unwrap();

    let outcome = install_named(&workspace, &ledger, &project, "deploy").unwrap();

    assert!(
        matches!(outcome.installed, Installed::Added { .. }),
        "{outcome:?}"
    );
    assert!(outcome.swept.is_empty(), "{outcome:?}");
    assert_eq!(
        fs::read_to_string(other.path().join("new").join("SKILL.md")).unwrap(),
        "theirs"
    );
    assert_eq!(staged(&project).len(), 1, "only the other one's is left");
    drop(other);
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));
}

/// What every install opens first, and builds in: so where installs running
/// at once in one project make their staging paths. From threads of one
/// process, which share a process id, so only the random part of each name
/// keeps them apart; and all held at once, as installs overlapping would.
#[test]
fn stages_opened_at_once_in_one_project_never_share_a_path() {
    let (_dir, _workspace, project) = own_skills(&[], 0);
    let scratch = project.scratch_dir();
    let runs = 8;
    let all_open = std::sync::Barrier::new(runs);

    let paths: Vec<std::path::PathBuf> = std::thread::scope(|scope| {
        let opening: Vec<_> = (0..runs)
            .map(|_| {
                scope.spawn(|| {
                    // Unwrapped only once all have tried, so one that cannot
                    // open fails the test rather than leaving the rest waiting.
                    let stage = copy::Stage::open(&scratch);
                    all_open.wait();
                    let stage = stage.unwrap();
                    assert!(stage.swept().is_empty(), "{:?}", stage.swept());
                    stage.path().to_path_buf()
                })
            })
            .collect();
        opening.into_iter().map(|run| run.join().unwrap()).collect()
    });

    let distinct: BTreeSet<_> = paths.iter().collect();
    assert_eq!(distinct.len(), runs, "{paths:?}");
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));
}

#[test]
fn two_installs_of_different_skills_at_once_each_land_whole() {
    let (_dir, workspace, project) = own_skills(&["deploy", "review"], 100);
    workspace.ledger().unwrap();

    std::thread::scope(|scope| {
        for name in ["deploy", "review"] {
            let (workspace, project) = (&workspace, &project);
            scope.spawn(move || {
                let ledger = workspace.ledger().unwrap();
                for round in 0..10 {
                    edit_own(workspace, name, round);
                    let outcome =
                        install_named(workspace, &ledger, project, name).map(|done| done.installed);
                    assert!(
                        matches!(
                            outcome,
                            Ok(Installed::Added { .. } | Installed::Updated { .. })
                        ),
                        "{name}, round {round}: {outcome:?}"
                    );
                }
            });
        }
    });

    assert_eq!(
        listing(&project.directory_for(Kind::Skill).unwrap()),
        ["deploy", "review"]
    );
    for name in ["deploy", "review"] {
        let own = workspace
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join(name);
        assert!(
            copy::same(&own, &installed_at(&project, name)).unwrap(),
            "{name}"
        );
    }
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));
}

/// The same skill, twice at once: what two terminals, or two workspaces
/// linking one project, can do, and where a staging path named after the
/// skill was shared. Unix only, because there one of the two always gets its
/// copy in place; on Windows a rename fails while anybody has a file inside
/// open, so both can lose a round and leave the copy before it.
#[cfg(unix)]
#[test]
fn two_installs_of_one_skill_at_once_leave_one_whole_copy_and_nothing_beside_it() {
    let (_dir, workspace, project) = own_skills(&["deploy"], 100);
    workspace.ledger().unwrap();
    let own = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    let rounds = 10;
    let start = std::sync::Barrier::new(2);
    let edited = std::sync::Barrier::new(2);

    std::thread::scope(|scope| {
        for first in [true, false] {
            let (workspace, project, start, edited) = (&workspace, &project, &start, &edited);
            scope.spawn(move || {
                let ledger = workspace.ledger().unwrap();
                for round in 0..rounds {
                    start.wait();
                    if first {
                        edit_own(workspace, "deploy", round);
                    }
                    edited.wait();
                    // One of the two may find the other's copy arriving under
                    // it and fail to put its own in place: said, and harmless.
                    match install_named(workspace, &ledger, project, "deploy") {
                        Ok(_) | Err(install::InstallError::Write { .. }) => {}
                        Err(error) => panic!("round {round}: {error}"),
                    }
                }
            });
        }
    });

    assert!(copy::same(&own, &installed_at(&project, "deploy")).unwrap());
    assert_eq!(
        listing(&project.directory_for(Kind::Skill).unwrap()),
        ["deploy"]
    );
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));
}

#[test]
fn a_skills_folder_that_is_not_there_yet_is_made() {
    // A fresh clone has no empty folders, so a project's skills folder can be
    // missing although its marker names it.
    let (_dir, workspace, project) = own_skills(&["deploy"], 0);
    let ledger = workspace.ledger().unwrap();
    fs::remove_dir(project.directory_for(Kind::Skill).unwrap()).unwrap();

    let outcome = install_named(&workspace, &ledger, &project, "deploy")
        .unwrap()
        .installed;

    assert!(matches!(outcome, Installed::Added { .. }), "{outcome:?}");
    assert!(installed_at(&project, "deploy").join("SKILL.md").is_file());
}

#[test]
fn what_an_interrupted_install_left_is_cleared_and_said_by_the_next() {
    let (_dir, workspace, project) = own_skills(&["deploy"], 0);
    let ledger = workspace.ledger().unwrap();
    // As an install stopped halfway leaves it: a stage named for a process
    // that has gone, holding half a copy.
    let left = project.scratch_dir().join("4242-00000000deadbeef");
    fs::create_dir_all(left.join("new").join("deploy")).unwrap();
    fs::write(left.join("new").join("deploy").join("SKILL.md"), "half").unwrap();

    let first = install_named(&workspace, &ledger, &project, "deploy").unwrap();

    assert!(
        matches!(first.installed, Installed::Added { .. }),
        "{first:?}"
    );
    assert!(!left.exists());
    assert_eq!(first.swept, [left]);
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));

    // Said once: the next install has nothing left to clear.
    edit_own(&workspace, "deploy", 1);
    let second = install_named(&workspace, &ledger, &project, "deploy").unwrap();
    assert!(
        matches!(second.installed, Installed::Updated { .. }),
        "{second:?}"
    );
    assert!(second.swept.is_empty(), "{second:?}");
}

#[test]
fn an_install_that_fails_still_says_what_an_interrupted_one_left_and_it_cleared() {
    let (_dir, workspace, project) = own_skills(&["deploy"], 0);
    let ledger = workspace.ledger().unwrap();
    let left = project.scratch_dir().join("4242-00000000deadbeef");
    fs::create_dir_all(left.join("new").join("deploy")).unwrap();
    // A file where the skills folder goes, so the copy is built and swept
    // for, but cannot be put in place.
    let skills = project.directory_for(Kind::Skill).unwrap();
    fs::remove_dir(&skills).unwrap();
    fs::write(&skills, "not a folder").unwrap();

    let error = install_named(&workspace, &ledger, &project, "deploy").unwrap_err();

    assert!(
        matches!(error, install::InstallError::Write { .. }),
        "{error}"
    );
    assert!(!left.exists());
    assert_eq!(error.swept(), [left]);
    assert!(staged(&project).is_empty(), "{:?}", staged(&project));
}

#[test]
fn what_an_earlier_version_left_beside_the_copy_is_cleared_and_said_by_the_next_install() {
    let (_dir, workspace, project) = own_skills(&["deploy"], 0);
    let ledger = workspace.ledger().unwrap();
    install_named(&workspace, &ledger, &project, "deploy").unwrap();
    // As an install of a version that staged beside leaves it when stopped:
    // a half-made copy next to the skill, and the one it was replacing,
    // under Mindweaver's names and under those of a Mindflayer build.
    let skills = project.directory_for(Kind::Skill).unwrap();
    let left = [
        skills.join(".deploy.mindweaver-new"),
        skills.join(".deploy.mindweaver-old"),
        skills.join(".deploy.mindflayer-new"),
        skills.join(".deploy.mindflayer-old"),
    ];
    for folder in &left {
        fs::create_dir(folder).unwrap();
        fs::write(folder.join("SKILL.md"), "half").unwrap();
    }

    // Nothing of the skill's own has changed, and that is no reason to leave
    // a copy an agent loads.
    let again = install_named(&workspace, &ledger, &project, "deploy").unwrap();

    assert!(
        matches!(again.installed, Installed::Unchanged { .. }),
        "{again:?}"
    );
    assert_eq!(again.swept, left);
    assert_eq!(listing(&skills), ["deploy"]);
    assert!(
        install_named(&workspace, &ledger, &project, "deploy")
            .unwrap()
            .swept
            .is_empty(),
        "said once"
    );
}

// ---------------------------------------------------------------------------
// The workspace's own skills, offered beside the shelf
// ---------------------------------------------------------------------------

/// `ready`, with the project linked and a skill of the workspace's own.
fn with_own(shelf: &[&str], own: &[&str]) -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let (dir, _, project, ledger) = ready(shelf);
    let mut workspace = WeaverWorkspace::open(dir.path()).unwrap();
    workspace.link(&project).unwrap();
    for name in own {
        mindweaver_core::create(&workspace.own(), Kind::Skill, name, &format!("Our {name}"))
            .unwrap();
    }
    (dir, workspace, project, ledger)
}

#[test]
fn the_workspace_offers_its_own_skills_first_and_installs_them_like_any_other() {
    let (_dir, workspace, project, ledger) = with_own(&["deploy"], &["commit-style"]);

    let candidates = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();

    let offered: Vec<(&str, String)> = candidates
        .iter()
        .map(|candidate| (candidate.name().as_str(), candidate.origin().into_string()))
        .collect();
    assert_eq!(offered[0], ("commit-style", String::from("workspace")));
    assert_eq!(offered.len(), 2);

    install::install(
        &workspace,
        &ledger,
        &project,
        &candidates[0],
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(installed_at(&project, "commit-style")
        .join("SKILL.md")
        .is_file());
    let again = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    assert_eq!(again[0].standing, Standing::Current);
}

#[test]
fn one_name_offered_twice_is_installed_only_from_where_it_is_asked_for() {
    let (_dir, workspace, project, ledger) = with_own(&["deploy"], &["deploy"]);

    let twice =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap_err();
    let ours = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        Some("workspace"),
    )
    .unwrap();
    let nothing =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "absent", None).unwrap_err();

    assert!(
        matches!(twice, install::InstallError::OfferedTwice { .. }),
        "{twice}"
    );
    assert!(twice.to_string().contains("--from"), "{twice}");
    assert_eq!(ours.origin(), "workspace");
    assert!(
        matches!(nothing, install::InstallError::NotOffered { .. }),
        "{nothing}"
    );
}

#[test]
fn an_origin_as_printed_names_its_one_offer_though_another_branch_has_its_address() {
    let (_dir, workspace, project, ledger) = ready(&["review"]);
    let source = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let address = url(&source);
    let head = fs::read_to_string(source.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().rsplit('/').next().unwrap().to_owned();
    gather::gather(&workspace, &ledger, &Request::git(&address)).unwrap();
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(&address).at(Some(&branch)),
    )
    .unwrap();
    let from = |typed: &str| {
        install::offered(
            &workspace,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            Some(typed),
        )
    };

    // `<url>` is the first's origin and the second's address without its
    // branch: the origin is the one it names.
    let plain = from(&address).unwrap();
    let branched = from(&format!("{address}#{branch}")).unwrap();

    assert_eq!(plain.origin(), address);
    assert_eq!(branched.origin(), format!("{address}#{branch}"));
}

#[test]
fn from_takes_a_sources_name_and_another_spelling_of_its_url_which_is_said() {
    let (_dir, workspace, project, ledger) = ready(&["review"]);
    let one = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it"))]);
    let other = repository(&[("skills/deploy/SKILL.md", &skill("deploy", "Ship it too"))]);
    gather::gather(
        &workspace,
        &ledger,
        &Request::git(url(&one)).named(Some("one")),
    )
    .unwrap();
    gather::gather(&workspace, &ledger, &Request::git(url(&other))).unwrap();
    let from = |typed: &str| {
        install::offered(
            &workspace,
            &ledger,
            &project,
            Kind::Skill,
            "deploy",
            Some(typed),
        )
        .unwrap()
    };

    // Each by its name, the one a listing shows, though both offer it.
    let named = from("one");
    let respelled = from(&format!("{}/", url(&one)));
    let printed = from(&url(&one));

    for candidate in [&named, &respelled, &printed] {
        assert_eq!(candidate.origin(), url(&one));
        // Listed by its name, the one it is installed from by its origin.
        assert_eq!(candidate.offer.listed_from(), "one");
    }
    assert_eq!(install::respelled(&named, "one"), None);
    assert_eq!(install::respelled(&printed, &url(&one)), None);
    let said = install::respelled(&respelled, &format!("{}/", url(&one))).unwrap();
    assert_eq!(said.code(), "from_spelling_deprecated");
    assert_eq!(
        said.to_string(),
        format!(
            "--from {}/ names the source `one` by another spelling of its URL, which --from \
             takes for now: name it `one`",
            url(&one)
        )
    );
}

#[test]
fn from_typed_with_credentials_names_the_source_they_were_taken_out_of_and_repeats_none() {
    let (_dir, workspace, project, ledger) = ready(&["deploy"]);
    // As if gathered from an https address rather than from a path.
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute(
            "UPDATE sources SET url = 'https://example.com/acme/skills'",
            [],
        )
        .unwrap();
    let typed = Some("https://me:ghp_t0ken@example.com/acme/skills");

    let deploy =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", typed).unwrap();
    let nothing =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "absent", typed).unwrap_err();

    assert_eq!(deploy.origin(), "https://example.com/acme/skills");
    assert!(
        matches!(nothing, install::InstallError::NotOffered { .. }),
        "{nothing}"
    );
    assert!(!nothing.to_string().contains("ghp_t0ken"), "{nothing}");
}

// ---------------------------------------------------------------------------
// What no copy holds
// ---------------------------------------------------------------------------

/// A shelf holding `deploy` with a `.claude-plugin` folder beside its
/// `SKILL.md`, as a plugin's skill is gathered, and a project marked by its
/// settings naming every agent.
fn plugin_skill() -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let (dir, workspace, _, ledger) = gathered(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        (
            "skills/deploy/.claude-plugin/plugin.json",
            r#"{"name": "deploy", "hooks": "hooks.json"}"#,
        ),
        ("skills/deploy/scripts/.claude-plugin/example.json", "{}"),
    ]);
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    (dir, workspace, project, ledger)
}

#[test]
fn a_gathered_skill_holding_a_claude_plugin_folder_is_installed_without_it_and_says_so() {
    let (_dir, workspace, project, ledger) = plugin_skill();

    let done = install_from_shelf(&workspace, &ledger, &project, "deploy");

    assert!(
        matches!(done.installed, Installed::Added { .. }),
        "{done:?}"
    );
    assert_eq!(done.left_out.len(), 1, "{:?}", done.left_out);
    assert!(done.left_out[0].path.ends_with("deploy/.claude-plugin"));
    assert!(done.left_out[0].reason.contains("as a plugin"));
    for copy in agents_copies(&project, "deploy") {
        assert!(copy.join("SKILL.md").is_file());
        assert!(!copy.join(".claude-plugin").exists(), "{}", copy.display());
        // Only the top one: further down it is the skill's own file.
        assert!(copy.join("scripts/.claude-plugin/example.json").is_file());
    }
    let record = ledger
        .installation("alpha", Kind::Skill, "deploy")
        .unwrap()
        .unwrap();
    let [agents, _] = agents_copies(&project, "deploy");
    assert_eq!(
        record.content_hash,
        Some(mindweaver_core::hash::tree(&agents).unwrap())
    );
}

#[test]
fn a_skill_installed_without_what_is_left_out_is_still_current_and_installs_unchanged() {
    let (_dir, workspace, project, ledger) = plugin_skill();
    install_from_shelf(&workspace, &ledger, &project, "deploy");

    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Skill).unwrap();
    let again = install::install(
        &workspace,
        &ledger,
        &project,
        &surveyed[0],
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(install::is_current(
        &workspace,
        &ledger,
        &project,
        &surveyed[0]
    ));
    assert!(
        matches!(again.installed, Installed::Unchanged { .. }),
        "{again:?}"
    );
    // Still not all that was offered, so still said.
    assert_eq!(again.left_out.len(), 1);
}

// ---------------------------------------------------------------------------
// What keeps a local project's copies out of git
// ---------------------------------------------------------------------------

/// A shelf holding `deploy`, and a project marked by its settings, sharing
/// as `share` says, naming every agent, at the root of a git repository of
/// its own when `in_git`.
fn local_v3(share: Share, in_git: bool) -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let (dir, workspace, _, ledger) = ready(&["deploy"]);
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    if in_git {
        gix::init(&root).unwrap();
    }
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, share, &Directories::default()).unwrap();
    (dir, workspace, project, ledger)
}

fn exclude_file(project: &MindProject) -> std::path::PathBuf {
    project.root().join(".git").join("info").join("exclude")
}

/// Whether gix, reading the exclude file as git does, holds the folder at
/// `path` in the project's work tree to be ignored.
fn ignored(project: &MindProject, path: &str) -> bool {
    let repository = gix::open_opts(project.root(), gix::open::Options::isolated()).unwrap();
    let index = repository.index_or_empty().unwrap();
    let mut excludes = repository
        .excludes(&index, None, Default::default())
        .unwrap();
    excludes
        .at_path(path, Some(gix::index::entry::Mode::DIR))
        .unwrap()
        .is_excluded()
}

#[test]
fn a_local_install_keeps_every_copy_out_of_git_and_its_uninstall_takes_them_back_in() {
    let (_dir, workspace, project, ledger) = local_v3(Share::Local, true);
    let id = project.settings().unwrap().id.as_str().to_owned();
    let file = exclude_file(&project);
    let before = fs::read_to_string(&file).unwrap();

    let done = install_from_shelf(&workspace, &ledger, &project, "deploy");

    let [agents, claude] = agents_copies(&project, "deploy");
    assert!(copy::same(&agents, &claude).unwrap());
    assert_eq!(
        ledger
            .installation("alpha", Kind::Skill, "deploy")
            .unwrap()
            .unwrap()
            .targets
            .len(),
        2
    );
    assert_eq!(
        done.excluded,
        Some(mindweaver_core::Excluded::Written { file: file.clone() })
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!(
            "{before}# mindweaver:begin\n# project {id}\n/.agents/skills/deploy/\n\
             /.claude/skills/deploy/\n# mindweaver:end\n"
        )
    );
    assert!(ignored(&project, ".agents/skills/deploy"));
    assert!(ignored(&project, ".claude/skills/deploy"));

    let again = install_from_shelf(&workspace, &ledger, &project, "deploy");
    assert!(
        matches!(again.installed, Installed::Unchanged { .. }),
        "{again:?}"
    );
    assert_eq!(
        again.excluded,
        Some(mindweaver_core::Excluded::Unchanged { file: file.clone() })
    );

    let removed = install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(
        matches!(
            removed,
            Removed::Removed {
                still_excluded: None,
                ..
            }
        ),
        "{removed:?}"
    );
    assert!(!agents.exists() && !claude.exists());
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    assert!(!ignored(&project, ".claude/skills/deploy"));
}

#[test]
fn a_local_install_keeps_no_copy_git_tracks_out_of_git() {
    let (dir, workspace, _, ledger) = ready(&["deploy", "commit-style"]);
    let root = dir.path().join("alpha");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Local, &Directories::default())
            .unwrap();
    let id = project.settings().unwrap().id.as_str().to_owned();
    install_from_shelf(&workspace, &ledger, &project, "deploy");
    // The team commits deploy's copies, as a project that was shared would
    // have, and its lines go.
    mindweaver_core::exclude::forget(project.root(), &id).unwrap();
    common::indexed(project.root());
    let file = exclude_file(&project);
    let before = fs::read_to_string(&file).unwrap();

    install_from_shelf(&workspace, &ledger, &project, "deploy");
    install_from_shelf(&workspace, &ledger, &project, "commit-style");

    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!(
            "{before}# mindweaver:begin\n# project {id}\n/.agents/skills/commit-style/\n\
             /.claude/skills/commit-style/\n# mindweaver:end\n"
        )
    );
    assert!(!ignored(&project, ".claude/skills/deploy"));
    assert!(ignored(&project, ".claude/skills/commit-style"));
}

#[test]
fn a_shared_install_leaves_the_exclude_file_alone() {
    let (_dir, workspace, project, ledger) = local_v3(Share::Shared, true);
    let file = exclude_file(&project);
    let before = fs::read(&file).unwrap();

    let done = install_from_shelf(&workspace, &ledger, &project, "deploy");
    install::uninstall(
        &workspace,
        &ledger,
        &project,
        Kind::Skill,
        "deploy",
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(done.excluded, None);
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn a_local_install_outside_git_says_nothing_keeps_its_copies_out() {
    let (_dir, workspace, project, ledger) = local_v3(Share::Local, false);

    let done = install_from_shelf(&workspace, &ledger, &project, "deploy");

    let excluded = done.excluded.unwrap();
    assert!(
        matches!(excluded, mindweaver_core::Excluded::Outside { .. }),
        "{excluded:?}"
    );
    assert!(excluded
        .warning()
        .unwrap()
        .contains("not in a git work tree"));
    assert!(agents_copies(&project, "deploy")[1].is_dir());
}

#[test]
fn an_exclude_block_that_cannot_be_read_refuses_the_install_with_nothing_written() {
    let (_dir, workspace, project, ledger) = local_v3(Share::Local, true);
    let file = exclude_file(&project);
    fs::write(&file, "# mindweaver:begin\n/mine/\n").unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    let refused = install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert!(
        matches!(refused, install::InstallError::Exclude(_)),
        "{refused}"
    );
    assert!(
        refused.to_string().contains("nothing was installed"),
        "{refused}"
    );
    for copy in agents_copies(&project, "deploy") {
        assert!(!copy.exists(), "{}", copy.display());
    }
    assert!(ledger
        .installation("alpha", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# mindweaver:begin\n/mine/\n"
    );
}

#[test]
fn copies_that_cannot_be_put_in_place_take_their_exclude_entries_back_out() {
    let (_dir, workspace, project, ledger) = local_v3(Share::Local, true);
    let file = exclude_file(&project);
    let before = fs::read_to_string(&file).unwrap();
    // A file where Claude's folder of skills would be, so no copy goes in.
    fs::create_dir_all(project.root().join(".claude")).unwrap();
    fs::write(project.root().join(".claude").join("skills"), "a file").unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    let refused = install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap_err();

    // Named, the one that is broken rather than the first.
    let install::InstallError::Write { path, .. } = &refused else {
        panic!("{refused}");
    };
    assert_eq!(*path, agents_copies(&project, "deploy")[1]);
    assert!(!agents_copies(&project, "deploy")[0].exists());
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
}

// ---------------------------------------------------------------------------
// Never into the agents' own folders
// ---------------------------------------------------------------------------

#[test]
fn a_project_at_the_users_home_is_refused_an_install_into_its_agents_own_folders() {
    // The temporary folder stands for the user's home, and the project is
    // marked at its root, so its targets are the agents' folders for every
    // project.
    let (dir, workspace, _, ledger) = ready(&["deploy"]);
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    let (project, _) =
        MindProject::init_settings(&home, &Agent::ALL, Share::Shared, &Directories::default())
            .unwrap();
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();

    let refused = install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::defaults_under(&home),
    )
    .unwrap_err();

    let install::InstallError::UserScope { path, reader, .. } = &refused else {
        panic!("{refused}");
    };
    assert_eq!(*path, home.join(".agents").join("skills").join("deploy"));
    assert_eq!(*reader, "more than one agent");
    assert!(
        refused
            .to_string()
            .contains("Mindweaver installs into one project at a time"),
        "{refused}"
    );
    assert!(!home.join(".agents").exists() && !home.join(".claude").exists());
    assert!(ledger
        .installation("home", Kind::Skill, "deploy")
        .unwrap()
        .is_none());
    // Somewhere else under that home is one project like any other.
    let (dir, workspace, project, ledger) = ready_v3(&["deploy"], &Agent::ALL);
    let candidate =
        install::offered(&workspace, &ledger, &project, Kind::Skill, "deploy", None).unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::defaults_under(dir.path()),
    )
    .unwrap();
}

#[test]
fn a_standing_is_serialized_as_its_slug() {
    let serialized = serde_json::to_string(&[
        Standing::Absent,
        Standing::Current,
        Standing::Outdated,
        Standing::Edited,
        Standing::Missing,
        Standing::Orphaned,
        Standing::Foreign,
    ])
    .unwrap();

    assert_eq!(
        serialized,
        r#"["absent","current","outdated","edited","missing","orphaned","foreign"]"#
    );
}

/// What an offer serializes to is a contract: the place that offers it, what
/// that place holds of it, and its origin as `--from` takes it.
#[test]
fn each_offer_serializes_its_place_what_it_holds_and_its_origin() {
    let workspace = install::Offer::Workspace {
        kind: Kind::Skill,
        name: "deploy".into(),
        summary: Some(String::from("Ship it")),
        path: std::path::PathBuf::from("/work/.mindweaver/skills/deploy"),
    };
    let loaded = install::Offer::Loaded {
        kind: Kind::Rule,
        name: "git/no-force".into(),
        summary: None,
        path: std::path::PathBuf::from("/team/.mindweaver/rules/git/no-force.md"),
        entry: String::from("../team"),
        workspace: String::from("team"),
    };

    assert_eq!(
        serde_json::to_string(&workspace).unwrap(),
        concat!(
            r#"{"place":"workspace","kind":"skill","name":"deploy","summary":"Ship it","#,
            r#""path":"/work/.mindweaver/skills/deploy","origin":"workspace"}"#,
        )
    );
    assert_eq!(
        serde_json::to_string(&loaded).unwrap(),
        concat!(
            r#"{"place":"loaded","kind":"rule","name":"git/no-force","summary":null,"#,
            r#""path":"/team/.mindweaver/rules/git/no-force.md","entry":"../team","#,
            r#""workspace":"team","origin":"load:../team"}"#,
        )
    );
}

/// A candidate from the shelf: what was gathered, its source with it, and
/// how it stands in the project. Built with a credential in the source's
/// URL, which nothing the ledger hands back holds, to prove that neither
/// the source nor the origin prints it.
#[test]
fn a_candidate_from_the_shelf_serializes_without_the_credentials_its_source_carries() {
    let candidate = install::Candidate {
        offer: install::Offer::Shelf(gather::Gathered {
            kind: Kind::Skill,
            name: "review".into(),
            path: String::from(".mindweaver/skills/github.com-acme-skills/review"),
            summary: Some(String::from("Review a change")),
            revision: Some("4f9c2a7".into()),
            gathered_at: 1_790_000_000,
            source: mindweaver_core::ledger::Source {
                id: 1,
                kind: mindweaver_core::SourceKind::Git,
                url: mindweaver_core::value::Redacted::new(
                    "https://me:s3cret@github.com/acme/skills.git",
                ),
                reference: Some(String::from("main")),
                subdirectory: String::from("skills"),
                revision: Some("4f9c2a7".into()),
                directory: String::from("github.com-acme-skills"),
                name: mindweaver_core::value::SourceName::parse("acme-skills").unwrap(),
            },
        }),
        standing: Standing::Outdated,
    };

    let serialized = serde_json::to_string(&candidate).unwrap();

    assert!(!serialized.contains("s3cret"), "{serialized}");
    assert_eq!(
        serialized,
        concat!(
            r#"{"offer":{"place":"shelf","kind":"skill","name":"review","#,
            r#""path":".mindweaver/skills/github.com-acme-skills/review","#,
            r#""summary":"Review a change","revision":"4f9c2a7","gathered_at":1790000000,"#,
            r#""source":{"kind":"git","url":"https://github.com/acme/skills.git","#,
            r#""reference":"main","subdirectory":"skills","revision":"4f9c2a7","#,
            r#""directory":"github.com-acme-skills","name":"acme-skills"},"#,
            r#""origin":"https://github.com/acme/skills.git#main"},"#,
            r#""standing":"outdated"}"#,
        )
    );
}
