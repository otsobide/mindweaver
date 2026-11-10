//! Unit tests for `admit.rs`: what of a task from outside reaches a line, and
//! in what words.

use std::fs;
use std::path::Path;

use mindweaver_core::{Agent, Kind, MindProject, Share};
use tempfile::TempDir;

use super::*;
use crate::task::{shell, Mark, MindsChange, Toggles};

/// A workspace at `work` linking `alpha` under it and `beside` next to it,
/// and loading `library`, next to it too: nothing above any of them, so
/// `mind` run in `beside` consults no workspace at all.
fn fixture() -> (TempDir, WeaverWorkspace) {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("work");
    let (mut workspace, _) = WeaverWorkspace::init(&root).unwrap();
    for project in [root.join("alpha"), dir.path().join("beside")] {
        let (project, _) = MindProject::init(project).unwrap();
        workspace.link(&project).unwrap();
    }
    let (library, _) = WeaverWorkspace::init(dir.path().join("library")).unwrap();
    workspace.load(&library).unwrap();
    (dir, WeaverWorkspace::open(&root).unwrap())
}

/// `task` admitted against `workspace`, looked for from nowhere: no home,
/// nothing named or activated.
fn admitted(workspace: &WeaverWorkspace, task: &Task) -> Result<Task, PolicyError> {
    Admission {
        workspace,
        lookup: &Lookup::default(),
    }
    .task(task)
}

/// The lines `task`, once admitted, is, as a shell shows them.
fn lines(workspace: &WeaverWorkspace, task: &Task) -> Vec<String> {
    admitted(workspace, task)
        .unwrap()
        .commands()
        .iter()
        .map(|line| shell(line))
        .collect()
}

fn code(result: Result<Task, PolicyError>) -> &'static str {
    result.expect_err("it was admitted").code()
}

fn text(path: &Path) -> String {
    path.to_str().unwrap().to_owned()
}

fn add(holder: Holder) -> Task {
    Task::Add {
        holder,
        kind: Kind::Skill,
        name: String::from("deploy"),
        description: String::from("Ship it"),
        template: None,
        server: None,
    }
}

#[test]
fn a_project_held_by_its_entry_is_named_to_the_command_by_its_folder() {
    let (_dir, workspace) = fixture();
    let alpha = workspace.root().join("alpha");

    let admitted = admitted(&workspace, &add(Holder::Project(String::from("alpha")))).unwrap();

    assert_eq!(admitted, add(Holder::Project(text(&alpha))));
    assert_eq!(
        lines(&workspace, &add(Holder::Project(String::from("alpha")))),
        [format!(
            "mind -C {} add skill deploy 'Ship it'",
            text(&alpha)
        )]
    );
}

#[test]
fn a_project_s_toggles_are_held_to_its_entry_and_named_by_its_folder() {
    let (_dir, workspace) = fixture();
    let alpha = text(&workspace.root().join("alpha"));
    let toggles = |agents: &[Agent], share| Toggles {
        agents: agents.to_vec(),
        share,
        edit_tracked: false,
    };
    let toggled = |entry: &str| Task::Toggled {
        holder: Holder::Project(entry.to_owned()),
        was: toggles(&[Agent::Claude], Share::Shared),
        now: toggles(&[Agent::Codex, Agent::Claude], Share::Local),
    };

    assert_eq!(
        lines(&workspace, &toggled("alpha")),
        [
            format!("mind -C {alpha} agents --yes claude codex"),
            format!("mind -C {alpha} share --yes local"),
        ]
    );
    assert_eq!(
        code(admitted(&workspace, &toggled("beta"))),
        "not_registered"
    );
}

/// The browser's word for which workspace `mind` consults is not taken: the
/// backend asks, as the TUI does for its rows.
#[test]
fn whether_a_line_says_which_workspace_is_decided_here_not_sent() {
    let (dir, workspace) = fixture();
    let alpha = text(&workspace.root().join("alpha"));
    let beside = text(&dir.path().join("beside"));

    for sent in [
        Holder::Project(String::from("alpha")),
        Holder::Elsewhere(String::from("alpha")),
    ] {
        assert_eq!(
            admitted(&workspace, &add(sent)).unwrap(),
            add(Holder::Project(alpha.clone()))
        );
    }
    for sent in [
        Holder::Project(String::from("../beside")),
        Holder::Elsewhere(String::from("../beside")),
    ] {
        assert_eq!(
            admitted(&workspace, &add(sent)).unwrap(),
            add(Holder::Elsewhere(beside.clone()))
        );
    }
    assert_eq!(
        lines(&workspace, &add(Holder::Project(String::from("../beside")))),
        [format!("mind -W . -C {beside} add skill deploy 'Ship it'")]
    );
}

#[test]
fn the_workspaces_own_stay_the_workspaces() {
    let (_dir, workspace) = fixture();

    assert_eq!(
        admitted(&workspace, &add(Holder::Workspace)).unwrap(),
        add(Holder::Workspace)
    );
}

#[test]
fn a_holder_the_registry_does_not_hold_is_refused() {
    let (dir, workspace) = fixture();
    let absolute = text(&workspace.root().join("alpha"));
    let outside = text(&dir.path().join("beside"));

    for entry in [
        "beta",
        "-C",
        "--home=/etc",
        "./alpha",
        "../alpha",
        "alpha/..",
        "/etc",
        &absolute,
        &outside,
    ] {
        for holder in [
            Holder::Project(entry.to_owned()),
            Holder::Elsewhere(entry.to_owned()),
        ] {
            assert_eq!(code(admitted(&workspace, &add(holder))), "not_registered");
        }
    }
}

#[test]
fn each_project_an_install_names_is_named_by_its_folder() {
    let (dir, workspace) = fixture();
    let install = Task::Install {
        projects: vec![String::from("alpha"), String::from("../beside")],
        kind: Kind::Skill,
        name: String::from("deploy"),
        from: Some(String::from("--yes")),
        yes: false,
    };

    assert_eq!(
        lines(&workspace, &install),
        [format!(
            "mindweaver install -p {} -p {} --from=--yes skill:deploy",
            text(&workspace.root().join("alpha")),
            text(&dir.path().join("beside"))
        )]
    );
    let unknown = Task::Adopt {
        project: String::from("gamma"),
        kind: Kind::Skill,
        name: String::from("deploy"),
    };
    assert_eq!(code(admitted(&workspace, &unknown)), "not_registered");
    assert_eq!(
        lines(
            &workspace,
            &Task::Migrate {
                project: String::from("../beside"),
                yes: true,
            }
        ),
        [format!(
            "mindweaver migrate -p {} --yes",
            text(&dir.path().join("beside"))
        )]
    );
    let unknown = Task::Migrate {
        project: String::from("gamma"),
        yes: false,
    };
    assert_eq!(code(admitted(&workspace, &unknown)), "not_registered");
}

#[test]
fn an_entry_a_command_takes_goes_to_it_as_the_marker_spells_it() {
    let (dir, workspace) = fixture();
    let new = text(&dir.path().join("moved"));

    assert_eq!(
        lines(
            &workspace,
            &Task::Unlink {
                entry: String::from("../beside")
            }
        ),
        ["mindweaver unlink ../beside"]
    );
    assert_eq!(
        lines(
            &workspace,
            &Task::Relink {
                old: String::from("alpha"),
                new: new.clone(),
            }
        ),
        [format!("mindweaver relink alpha {new}")]
    );
    assert_eq!(
        lines(
            &workspace,
            &Task::Unload {
                entry: String::from("../library")
            }
        ),
        ["mindweaver unload ../library"]
    );
    assert_eq!(
        code(admitted(
            &workspace,
            &Task::Unlink {
                entry: String::from("-rf")
            }
        )),
        "not_registered"
    );
    assert_eq!(
        code(admitted(
            &workspace,
            &Task::Unload {
                entry: String::from("../beside")
            }
        )),
        "not_loaded"
    );
}

#[test]
fn a_folder_not_linked_yet_goes_to_the_command_as_given() {
    let (dir, workspace) = fixture();
    let fresh = dir.path().join("fresh");
    fs::create_dir(&fresh).unwrap();
    let fresh = text(&fresh);

    assert_eq!(
        lines(
            &workspace,
            &Task::InitAndLink {
                path: fresh.clone()
            }
        ),
        [
            format!("mind -C {fresh} init"),
            format!("mindweaver link {fresh}")
        ]
    );
    assert_eq!(
        lines(
            &workspace,
            &Task::Load {
                path: fresh.clone()
            }
        ),
        [format!("mindweaver load {fresh}")]
    );
}

#[test]
fn a_folder_that_is_not_absolute_normalized_and_outside_any_workspace_folder_is_refused() {
    let (_dir, workspace) = fixture();
    let weaver = text(&workspace.weaver_dir());
    let climbing = format!("{}{}..", text(workspace.root()), std::path::MAIN_SEPARATOR);

    for (path, expected) in [
        ("beta", "path_relative"),
        ("-x", "path_relative"),
        ("file:///etc", "path_url"),
        (climbing.as_str(), "path_unnormalized"),
        (weaver.as_str(), "path_in_mindflayer"),
    ] {
        for task in [
            Task::Link {
                path: path.to_owned(),
            },
            Task::InitAndLink {
                path: path.to_owned(),
            },
            Task::Load {
                path: path.to_owned(),
            },
            Task::LoadHere {
                path: path.to_owned(),
            },
            Task::Relink {
                old: String::from("alpha"),
                new: path.to_owned(),
            },
            Task::Minds {
                init: vec![path.to_owned()],
                link: vec![path.to_owned()],
                unlink: Vec::new(),
            },
            Task::Minds {
                init: Vec::new(),
                link: vec![path.to_owned()],
                unlink: Vec::new(),
            },
            marks(&[(path, MindsChange::Link)]),
            marks(&[(path, MindsChange::InitAndLink)]),
        ] {
            assert_eq!(code(admitted(&workspace, &task)), expected, "{task:?}");
        }
    }
}

/// The panel's marks, each a path and what to do to it.
fn marks(marked: &[(&str, MindsChange)]) -> Task {
    Task::Marks {
        marks: marked
            .iter()
            .map(|(path, change)| Mark {
                path: (*path).to_owned(),
                change: *change,
            })
            .collect(),
    }
}

#[test]
fn marks_on_folders_go_to_the_minds_screen_s_lines_as_given() {
    let (dir, workspace) = fixture();
    let fresh = dir.path().join("fresh");
    fs::create_dir(&fresh).unwrap();
    let fresh = text(&fresh);
    let beside = text(&dir.path().join("beside"));
    let other = text(&dir.path().join("other"));

    assert_eq!(
        lines(
            &workspace,
            &marks(&[
                (&other, MindsChange::Link),
                (&fresh, MindsChange::InitAndLink),
                (&beside, MindsChange::Unlink),
            ])
        ),
        [
            format!("mind -C {fresh} init"),
            format!("mindweaver link {other} {fresh}"),
            format!("mindweaver unlink {beside}"),
        ]
    );
    let library = text(&dir.path().join("library"));
    assert_eq!(
        code(admitted(
            &workspace,
            &marks(&[(&library, MindsChange::Unlink)])
        )),
        "not_registered"
    );
}

#[test]
fn a_mark_to_unlink_or_unload_names_a_folder_an_entry_points_at() {
    let (dir, workspace) = fixture();
    let beside = text(&dir.path().join("beside"));
    let library = text(&dir.path().join("library"));

    let minds = Task::Minds {
        init: Vec::new(),
        link: Vec::new(),
        unlink: vec![beside.clone()],
    };
    assert_eq!(
        lines(&workspace, &minds),
        [format!("mindweaver unlink {beside}")]
    );
    let wrong = Task::Minds {
        init: Vec::new(),
        link: Vec::new(),
        unlink: vec![library.clone()],
    };
    assert_eq!(code(admitted(&workspace, &wrong)), "not_registered");
    assert_eq!(
        lines(
            &workspace,
            &Task::UnloadHere {
                path: library.clone()
            }
        ),
        [format!("mind unload {library}")]
    );
    assert_eq!(
        code(admitted(&workspace, &Task::UnloadHere { path: beside })),
        "not_loaded"
    );
}

#[test]
fn a_batch_is_held_to_it_change_by_change() {
    let (_dir, workspace) = fixture();
    let change = |project: &str| Task::Uninstall {
        projects: vec![project.to_owned()],
        kind: Kind::Skill,
        name: String::from("deploy"),
        force: false,
    };

    let batch = Task::ApplyInstall {
        changes: vec![change("alpha"), change("nobody")],
    };
    assert_eq!(code(admitted(&workspace, &batch)), "not_registered");
    let batch = Task::ApplyInstall {
        changes: vec![change("alpha")],
    };
    assert_eq!(
        admitted(&workspace, &batch).unwrap(),
        Task::ApplyInstall {
            changes: vec![change(&text(&workspace.root().join("alpha")))]
        }
    );
}

/// Free text is the command's to judge, and the rule that no value is read
/// as a flag already keeps it a value: the policy is about what a value
/// names on the disk.
#[test]
fn what_names_nothing_on_the_disk_is_left_as_it_was_sent() {
    let (_dir, workspace) = fixture();
    let task = Task::Add {
        holder: Holder::Workspace,
        kind: Kind::Rule,
        name: String::from("--yes"),
        description: String::from("- the fast way"),
        template: Some(String::from("-rf")),
        server: None,
    };

    assert_eq!(admitted(&workspace, &task).unwrap(), task);
}

#[test]
fn nothing_is_admitted_against_a_workspace_that_links_nothing() {
    let dir = TempDir::new().unwrap();
    let workspace = WeaverWorkspace::unmade(dir.path()).unwrap();

    assert_eq!(
        code(admitted(
            &workspace,
            &add(Holder::Project(String::from(".")))
        )),
        "not_registered"
    );
    assert!(!dir.path().join(mindweaver_core::WEAVER_DIR).exists());
}

#[test]
fn every_project_a_selection_names_is_an_entry_the_registry_holds() {
    let (dir, workspace) = fixture();
    let admission = Admission {
        workspace: &workspace,
        lookup: &Lookup::default(),
    };
    let selection = |project: &str| {
        [
            Cell::of("alpha", Kind::Skill, "deploy", "workspace", true),
            Cell::of(project, Kind::Skill, "deploy", "workspace", false),
        ]
    };

    assert!(admission.selection(&selection("../beside")).is_ok());
    let alpha = text(&dir.path().join("work").join("alpha"));
    for project in [
        "-p",
        "--yes",
        "--home=/etc",
        "beside",
        "library",
        alpha.as_str(),
    ] {
        let refused = admission.selection(&selection(project)).unwrap_err();
        assert_eq!(refused.code(), "not_registered", "{project}");
    }
    assert!(admission.selection(&[]).is_ok());
}

/// A yes is admitted with an MCP server from elsewhere than the library
/// alone, the panel's confirmation of one; with any other install it is
/// refused by its code, whatever else the task names.
#[test]
fn a_yes_is_admitted_only_with_an_install_that_asks_for_one() {
    let (_dir, workspace) = fixture();
    let install = |kind: Kind, from: Option<&str>| Task::Install {
        projects: vec![String::from("alpha")],
        kind,
        name: String::from("github"),
        from: from.map(String::from),
        yes: true,
    };

    assert!(admitted(&workspace, &install(Kind::Mcp, Some("load:../team"))).is_ok());
    for task in [
        install(Kind::Skill, Some("load:../team")),
        install(Kind::Mcp, Some("workspace")),
        install(Kind::Mcp, None),
        Task::ApplyInstall {
            changes: vec![install(Kind::Rule, Some("workspace"))],
        },
    ] {
        assert_eq!(
            code(admitted(&workspace, &task)),
            "install_yes_unasked",
            "{task:?}"
        );
    }
}

/// A server's options naming something beside a value with an `=` in the
/// name are refused before a line is built, by the code `add`'s draft is
/// refused with: the line would end the name at it, and write another
/// pair than the one judged. One with an `=` in a value alone is admitted.
#[test]
fn a_server_naming_what_no_line_can_say_is_refused_before_a_line_is_built() {
    use mindweaver_core::mcp::Given;
    use mindweaver_core::mother::McpMother;

    let (_dir, workspace) = fixture();
    let add = |server: Given| Task::Add {
        holder: Holder::Workspace,
        kind: Kind::Mcp,
        name: String::from("acme"),
        description: String::from("Acme's tools"),
        template: None,
        server: Some(Box::new(server)),
    };
    let edit = |server: Given| Task::EditServer {
        holder: Holder::Project(String::from("alpha")),
        name: String::from("acme"),
        server: Box::new(server),
    };

    for (why, given, _) in McpMother::given_unsayable() {
        for task in [add(given.clone()), edit(given.clone())] {
            assert_eq!(
                code(admitted(&workspace, &task)),
                "add_mcp_key_unsayable",
                "{why}: {task:?}"
            );
        }
    }
    let Given::Program(mut program) = McpMother::given_program() else {
        unreachable!("a program is given as one");
    };
    program.env = vec![(String::from("OPTIONS"), String::from("a=b"))];
    for task in [
        add(Given::Program(program.clone())),
        edit(Given::Program(program)),
    ] {
        assert!(admitted(&workspace, &task).is_ok(), "{task:?}");
    }
}

fn export(folder: &str) -> Task {
    Task::Export {
        folder: folder.to_owned(),
        kinds: vec![Kind::Skill],
        yes: false,
    }
}

fn backup(url: &str, token_env: Option<&str>) -> Task {
    Task::Backup {
        url: url.to_owned(),
        branch: Some(String::from("library")),
        kinds: Vec::new(),
        message: Some(String::from("--yes")),
        token_env: token_env.map(str::to_owned),
    }
}

#[test]
fn an_export_writes_into_a_folder_named_as_a_folder_to_link_is() {
    let (dir, workspace) = fixture();
    let folder = text(&dir.path().join("backup"));

    assert_eq!(
        admitted(&workspace, &export(&folder)).unwrap(),
        export(&folder)
    );
    assert_eq!(
        lines(&workspace, &export(&folder)),
        [format!("mindweaver export --kind skills {folder}")]
    );
    let inside = text(&workspace.root().join(".mindweaver").join("library"));
    for (folder, refused) in [
        ("backup", "path_relative"),
        ("../backup", "path_relative"),
        ("file:///srv/backup", "path_url"),
        (inside.as_str(), "path_in_mindflayer"),
    ] {
        assert_eq!(
            code(admitted(&workspace, &export(folder))),
            refused,
            "{folder}"
        );
    }
}

#[test]
fn a_backup_s_repository_is_held_as_a_gather_s_source_and_its_token_named_by_its_variable() {
    let (dir, workspace) = fixture();
    let bare = text(&dir.path().join("library.git"));
    for url in [
        "https://github.com/you/library.git",
        "git@github.com:you/library.git",
        bare.as_str(),
    ] {
        let task = backup(url, Some("GITHUB_TOKEN"));

        assert_eq!(admitted(&workspace, &task).unwrap(), task, "{url}");
    }
    assert_eq!(
        lines(&workspace, &backup(&bare, Some("GITHUB_TOKEN"))),
        [format!(
            "mindweaver backup --branch library --message=--yes --token-env GITHUB_TOKEN {bare}"
        )]
    );
    let clone = text(&workspace.root().join(".mindweaver").join("cache").join("x"));
    for (task, refused) in [
        (backup("../library.git", None), "path_relative"),
        (backup(&clone, None), "path_in_mindflayer"),
        (
            backup("https://github.com/you/library.git", Some("HOME")),
            "token_variable_refused",
        ),
        (
            backup("https://github.com/you/library.git", Some("-GITHUB_TOKEN")),
            "token_variable_refused",
        ),
    ] {
        assert_eq!(code(admitted(&workspace, &task)), refused, "{task:?}");
    }
}

/// A result of a search, sent back to take, its repository `repository`
/// and its folder `path`.
fn take(repository: &str, path: Option<&str>) -> Task {
    Task::TakeSkill {
        hit: mindweaver_core::finder::Hit {
            name: String::from("commit-style"),
            repository: mindweaver_core::value::Redacted::new(repository),
            path: path.map(str::to_owned),
            reference: None,
            installs: Some(56),
            stars: None,
            finders: vec![mindweaver_core::finder::Finder::SkillsSh],
        },
    }
}

/// A result's repository is the gather's source, held as a gather's is
/// whatever finder named it: an address goes to the gather, which judges
/// it, and a folder on this machine only as a folder to link is.
#[test]
fn a_result_taken_is_held_to_its_repository_as_a_gather_is_to_its_source() {
    let (dir, workspace) = fixture();
    let folder = text(&dir.path().join("house-rules"));
    for task in [
        take("https://github.com/acme/skills", Some("skills/pdf")),
        take(&folder, None),
    ] {
        assert_eq!(admitted(&workspace, &task).unwrap(), task, "{task:?}");
    }
    assert_eq!(
        lines(&workspace, &take(&folder, Some("skills/commit-style"))),
        [format!(
            "mindweaver gather git --path skills/commit-style {folder}"
        )]
    );
    let clone = text(&workspace.root().join(".mindweaver").join("cache").join("x"));
    for (task, refused) in [
        (take("../house-rules", None), "path_relative"),
        (take("file://etc", None), "path_relative"),
        (take(&clone, None), "path_in_mindflayer"),
    ] {
        assert_eq!(code(admitted(&workspace, &task)), refused, "{task:?}");
    }
}
