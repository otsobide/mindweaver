//! The two levels: `mind` for one project, `mindweaver` for the workspace and
//! what it owns itself.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// The two levels
// ---------------------------------------------------------------------------

#[test]
fn mind_init_creates_a_project_and_weaver_init_a_workspace() {
    let project = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();

    let one = mind(project.path(), &["init"]).unwrap();
    let two = weaver(workspace.path(), &["init"]).unwrap();

    assert!(project.path().join(MIND_DIR).join(SETTINGS_FILE).is_file());
    assert!(!project.path().join(MIND_DIR).join(MIND_CONFIG).exists());
    // `.mind` is the marker and the configuration; the skills sit beside the
    // code, where the agents that read them look.
    assert!(project.path().join("skills").is_dir());
    assert!(!project.path().join(WEAVER_DIR).exists());
    assert!(one.stdout.starts_with("initialized mind project"));

    assert!(workspace
        .path()
        .join(WEAVER_DIR)
        .join(WEAVER_CONFIG)
        .is_file());
    assert!(!workspace.path().join(MIND_DIR).exists());
    assert!(two.stdout.starts_with("initialized weaver workspace"));
}

#[test]
fn mind_init_writes_settings_kept_out_of_git_and_empty_folders_that_git_does_not_list() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();
    assert_eq!(untracked(&root), Vec::<String>::new());

    let outcome = mind(&root, &["init"]).unwrap();

    let marker = root.join(MIND_DIR);
    assert_eq!(
        outcome.stdout,
        format!(
            "initialized mind project `collapse` in {}\n",
            marker.display()
        )
    );
    assert_eq!(names_in(&root), [".git", ".mind", "rules", "skills"]);
    // TOML, with comments saying what each key does, and no JSON beside it.
    assert_eq!(names_in(&marker), [".gitignore", "settings.toml"]);
    for kind in ["skills", "rules"] {
        assert_eq!(names_in(&root.join(kind)), Vec::<String>::new(), "{kind}");
    }
    let id = MindProject::open(&root)
        .unwrap()
        .settings()
        .unwrap()
        .id
        .to_string();
    let written = fs::read_to_string(marker.join(SETTINGS_FILE)).unwrap();
    assert!(
        written.starts_with(
            "# Mindweaver mind project: what Mindweaver needs to know about the project\n"
        ),
        "{written}"
    );
    for comment in [
        "# `name` is what the project is shown as, and `agents` the agents that work\n",
        "# `share` is \"local\" to keep .mind, and what is installed, out of git\n",
        "# `directories` says where the project keeps the skills and rules it writes\n",
        "# `id` names the project for good, wherever it moves, and `version` is how\n",
    ] {
        assert!(written.contains(comment), "{comment}{written}");
    }
    assert!(
        written.ends_with(&format!(
            "\nversion = 5\n\
             id = \"{id}\"\n\
             name = \"collapse\"\n\
             agents = [\"claude\", \"codex\", \"opencode\"]\n\
             share = \"local\"\n\
             edit_tracked = false\n\
             \n\
             [directories]\n\
             skills = \"skills\"\n\
             rules = \"rules\"\n"
        )),
        "{written}"
    );
    assert_eq!(
        fs::read_to_string(marker.join(".gitignore")).unwrap(),
        mindweaver_core::MIND_IGNORE
    );
    // The `.mind` ignores itself, and git lists no empty folder: nothing for
    // `git status` to offer until an artifact is written.
    assert_eq!(untracked(&root), Vec::<String>::new());
    // And what the project authors is not kept out: its first skill is there
    // for the team to commit, as the code beside it is.
    mind(&root, &["add", "skill", "deploy", "Ship it"]).unwrap();
    assert_eq!(untracked(&root), ["skills"]);
}

#[test]
fn mind_init_shared_writes_no_gitignore_so_git_lists_its_settings() {
    let parent = TempDir::new().unwrap();
    let root = parent.path().join("collapse");
    fs::create_dir(&root).unwrap();
    gix::init(&root).unwrap();

    mind(&root, &["init", "--shared"]).unwrap();

    assert_eq!(names_in(&root.join(MIND_DIR)), ["settings.toml"]);
    let project = MindProject::open(&root).unwrap();
    assert_eq!(
        project.settings().unwrap().share,
        mindweaver_core::Share::Shared
    );
    // The team's to commit: the one thing git has to offer.
    assert_eq!(untracked(&root), [".mind"]);
}

#[test]
fn mind_init_writes_the_agents_it_is_given_in_the_order_given() {
    let dir = TempDir::new().unwrap();

    mind(dir.path(), &["init", "--agents", "opencode,claude"]).unwrap();

    let written = fs::read_to_string(dir.path().join(MIND_DIR).join(SETTINGS_FILE)).unwrap();
    assert!(
        written.contains("\nagents = [\"opencode\", \"claude\"]\n"),
        "{written}"
    );
}

#[test]
fn mind_init_refuses_an_agent_it_does_not_know_before_writing_anything() {
    let dir = TempDir::new().unwrap();

    let error = Cli::try_parse_from(line(
        "mind",
        &["init", "--agents", "claude,cursor"],
        dir.path(),
    ))
    .unwrap_err();

    let said = error.to_string();
    assert!(
        said.contains("'cursor'") && said.contains("[possible values: claude, codex, opencode]"),
        "{said}"
    );
    assert!(!dir.path().join(MIND_DIR).exists());
}

#[test]
fn weaver_init_makes_the_workspace_in_the_directory_it_is_given() {
    let dir = TempDir::new().unwrap();
    let writing = dir.path().join("weavers").join("writing");

    // Typed from where the command works, and made, parents and all, as
    // `python -m venv` makes the directory it is given.
    let outcome = weaver(dir.path(), &["init", "weavers/writing"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "initialized weaver workspace `writing` in {}\n",
            writing.join(WEAVER_DIR).display()
        )
    );
    assert!(writing.join(WEAVER_DIR).join(WEAVER_CONFIG).is_file());
    assert!(!dir.path().join(WEAVER_DIR).exists());
}

#[test]
fn weaver_init_given_a_workspace_already_there_changes_nothing() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init", "writing"]).unwrap();
    let marker = dir
        .path()
        .join("writing")
        .join(WEAVER_DIR)
        .join(WEAVER_CONFIG);
    let before = fs::read_to_string(&marker).unwrap();

    let outcome = weaver(dir.path(), &["init", "writing", "--skills", "elsewhere"]).unwrap();

    assert!(
        outcome.stdout.contains("already initialized"),
        "{outcome:?}"
    );
    assert_eq!(fs::read_to_string(&marker).unwrap(), before);
}

#[test]
fn weaver_init_says_in_the_marker_that_its_own_go_in_its_library() {
    let dir = TempDir::new().unwrap();

    weaver(dir.path(), &["init"]).unwrap();

    let name = dir.path().file_name().unwrap().to_string_lossy();
    assert_eq!(
        registry(dir.path()),
        format!(
            "# Mindweaver workspace.\n\
             #\n\
             # It orchestrates the mind projects listed below, so their skills can be\n\
             # managed together. Paths are relative to this file's grandparent, the\n\
             # directory holding .mindweaver.\n\
             #\n\
             # `directories` says where the workspace keeps its own skills and rules,\n\
             # the ones every project it manages can install. By default that is its\n\
             # library, inside .mindweaver where no agent looks, and a kind not named\n\
             # here is kept there too. Each folder is made the first time something\n\
             # is added to it.\n\
             #\n\
             # `loaded` lists other weaver workspaces whose own skills its projects\n\
             # can install too, read from there each time: `mindweaver load <path>`.\n\
             version = 2\n\
             name = \"{name}\"\n\
             projects = []\n\
             loaded = []\n\
             \n\
             [directories]\n\
             skills = \".mindweaver/library/skills\"\n\
             rules = \".mindweaver/library/rules\"\n"
        )
    );
    // Made by the first artifact added, as before.
    assert!(!dir.path().join(WEAVER_DIR).join("library").exists());
}

#[test]
fn weaver_is_a_shortcut_for_mind_weaver() {
    let dir = workspace_with_two();

    for args in [
        vec!["list"],
        vec!["validate"],
        vec!["show", "alpha"],
        vec!["link", "alpha"],
        vec!["scan"],
    ] {
        let direct = weaver(dir.path(), &args).unwrap();
        let nested = {
            let mut nested = vec!["weaver"];
            nested.extend(args.iter().copied());
            mind(dir.path(), &nested).unwrap()
        };
        assert_eq!(
            direct, nested,
            "`mindweaver {args:?}` differs from `mind weaver`"
        );
    }
}

#[test]
fn mind_init_no_longer_takes_a_kind() {
    // The old surface named the level, as `mind init weaver` would; that lives
    // at `mindweaver init` now, and leaving both spellings alive would be two
    // ways to do one thing.
    assert!(Cli::try_parse_from(["mind", "init", "weaver"]).is_err());
    assert!(Cli::try_parse_from(["mind", "init", "mind"]).is_err());
}

#[test]
fn mind_list_sees_only_its_own_project_inside_a_workspace() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    let project_level = mind(&alpha, &["list"]).unwrap();
    let workspace_level = weaver(&alpha, &["list"]).unwrap();

    assert_eq!(project_level.stdout.lines().count(), 1);
    assert!(project_level.stdout.contains("alpha"));
    assert!(
        !project_level.stdout.contains("beta"),
        "the project level leaked its neighbour:\n{}",
        project_level.stdout
    );
    assert_eq!(workspace_level.stdout.lines().count(), 2);
    assert!(workspace_level.stdout.contains("beta"));
}

#[test]
fn only_the_workspace_level_names_the_project_each_skill_came_from() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    let project = mind(&alpha, &["validate"]).unwrap();
    let workspace = weaver(&alpha, &["validate"]).unwrap();

    assert!(project.stdout.contains("alpha: ok"), "{}", project.stdout);
    assert!(
        workspace.stdout.contains("alpha (alpha): ok"),
        "{}",
        workspace.stdout
    );
}

#[test]
fn each_level_names_the_command_that_creates_what_is_missing() {
    let dir = TempDir::new().unwrap();

    let project = mind(dir.path(), &["list"]).unwrap_err();
    let workspace = weaver(dir.path(), &["list"]).unwrap_err();

    assert!(matches!(*project.error, CliError::NotInProject(_)));
    assert!(project.error.to_string().contains("mind init"));
    assert!(matches!(*workspace.error, CliError::NotInWorkspace(_)));
    assert!(workspace.error.to_string().contains("mindweaver init"));
}

#[test]
fn ls_is_an_alias_at_both_levels() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");

    assert_eq!(
        mind(&alpha, &["list"]).unwrap(),
        mind(&alpha, &["ls"]).unwrap()
    );
    assert_eq!(
        weaver(dir.path(), &["list"]).unwrap(),
        weaver(dir.path(), &["ls"]).unwrap()
    );
}

// ---------------------------------------------------------------------------
// Both levels: the workspace's own artifacts, and every command at either
// level
// ---------------------------------------------------------------------------

#[test]
fn weaver_add_writes_into_the_workspace_and_listings_name_it_so() {
    let dir = workspace_with_own();

    assert!(own_directory_for(dir.path(), Kind::Skill)
        .join("commit-style/SKILL.md")
        .is_file());
    let listed = weaver(dir.path(), &["list"]).unwrap().stdout;
    assert!(
        listed.contains("workspace  commit-style  How every repo here writes commits"),
        "{listed}"
    );
    assert!(listed.contains("alpha      alpha"), "{listed}");
    let checked = weaver(dir.path(), &["validate"]).unwrap().stdout;
    assert!(
        checked.contains("commit-style (workspace): ok"),
        "{checked}"
    );
    // A project holds what is in its own folders; the workspace's reach it
    // by being installed.
    assert!(!mind(&dir.path().join("alpha"), &["list"])
        .unwrap()
        .stdout
        .contains("commit-style"));
}

#[test]
fn a_workspace_can_be_told_where_to_keep_its_own() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init", "--skills", "shared/skills"]).unwrap();

    weaver(dir.path(), &["add", "skill", "deploy", "Ship it"]).unwrap();

    assert!(dir.path().join("shared/skills/deploy/SKILL.md").is_file());
    let marker = fs::read_to_string(dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG)).unwrap();
    assert!(marker.contains("skills = \"shared/skills\""), "{marker}");
}

#[test]
fn a_workspace_that_keeps_its_own_beside_its_projects_still_does() {
    // The marker as `mindweaver init` wrote it before the library, word for word.
    let dir = TempDir::new().unwrap();
    let marker = "# Mindweaver workspace.\n\
         #\n\
         # It orchestrates the mind projects listed below, so their skills can be\n\
         # managed together. Paths are relative to this file's grandparent, the\n\
         # directory holding .mindweaver.\n\
         #\n\
         # `directories` says where the workspace keeps its own skills and rules:\n\
         # the ones every project it manages can install, beside the projects'\n\
         # own. They are made the first time something is added to them.\n\
         #\n\
         # `loaded` lists other weaver workspaces whose own skills its projects\n\
         # can install too, read from there each time: `mindweaver load <path>`.\n\
         version = 2\n\
         name = \"projects\"\n\
         projects = []\n\
         loaded = []\n\
         \n\
         [directories]\n\
         skills = \"skills\"\n\
         rules = \"rules\"\n";
    fs::create_dir(dir.path().join(WEAVER_DIR)).unwrap();
    fs::write(dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG), marker).unwrap();

    let skill = weaver(dir.path(), &["add", "skill", "commit-style", "Shared"]).unwrap();
    let rule = weaver(
        dir.path(),
        &["add", "rule", "git/no-force-push", "Never force-push"],
    )
    .unwrap();
    let listed = weaver(dir.path(), &["list"]).unwrap();
    let renamed = weaver(dir.path(), &["rename", "commit-style", "commits"]).unwrap();
    weaver(dir.path(), &["remove", "git/no-force-push", "--yes"]).unwrap();

    assert_eq!(
        created(&skill, "skill commit-style"),
        dir.path().join("skills/commit-style/SKILL.md")
    );
    assert_eq!(
        created(&rule, "rule git/no-force-push"),
        dir.path().join("rules/git/no-force-push.md")
    );
    assert_eq!(
        listed.stdout,
        "skill  commit-style       Shared\nrule   git/no-force-push  Never force-push\n"
    );
    assert!(renamed.ok, "{:?}", renamed.stderr);
    assert!(dir.path().join("skills/commits/SKILL.md").is_file());
    assert!(!dir.path().join("rules/git/no-force-push.md").exists());
    assert!(!dir.path().join(WEAVER_DIR).join("library").exists());
    assert_eq!(
        registry(dir.path()),
        marker,
        "the marker is read, not rewritten"
    );
}

#[test]
fn a_workspace_cannot_keep_its_own_on_the_shelf() {
    let dir = TempDir::new().unwrap();

    let failure = weaver(dir.path(), &["init", "--skills", ".mindweaver/skills"]).unwrap_err();
    // A folder holding the shelf overlaps it too, which the words say rather
    // than leaving a folder nobody typed to explain itself.
    let holding = weaver(dir.path(), &["init", "--rules", ".mindweaver"]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        ".mindweaver/skills: cannot hold the workspace's skills, \
         because it overlaps .mindweaver/skills, which is reserved for gathering"
    );
    assert_eq!(
        holding.error.to_string(),
        ".mindweaver: cannot hold the workspace's rules, \
         because it overlaps .mindweaver/skills, which is reserved for gathering"
    );
    assert!(!dir.path().join(WEAVER_DIR).exists());
}

#[test]
fn every_change_at_the_workspace_level_acts_on_its_own_unless_p_names_a_project() {
    let dir = workspace_with_own();

    weaver(dir.path(), &["rename", "commit-style", "commits"]).unwrap();
    weaver(
        dir.path(),
        &["add", "-p", "alpha", "rule", "style", "Plainly"],
    )
    .unwrap();
    weaver(dir.path(), &["remove", "alpha", "-p", "alpha", "--yes"]).unwrap();

    assert!(own_directory_for(dir.path(), Kind::Skill)
        .join("commits/SKILL.md")
        .is_file());
    assert!(dir.path().join("alpha/rules/style.md").is_file());
    assert!(!dir.path().join("alpha/skills/alpha").exists());
    // `-p` at the workspace level is `mind` in that project: the same
    // function, so the same answer.
    assert_eq!(
        weaver(dir.path(), &["list", "-p", "alpha"]).unwrap(),
        mind(&dir.path().join("alpha"), &["list"]).unwrap()
    );
    assert_eq!(
        weaver(dir.path(), &["templates"]).unwrap().stdout,
        "full  skill  built in\n"
    );
}

#[test]
fn p_says_which_projects_there_are_when_it_names_none_of_them() {
    let dir = workspace_with_two();

    let failure = weaver(dir.path(), &["add", "-p", "gamma", "skill", "x", "y"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::UnknownProject { .. }));
    assert!(failure.error.to_string().contains("it manages alpha, beta"));
}

#[test]
fn mind_link_and_unlink_act_on_the_workspace_above() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();

    assert_eq!(
        mind(&alpha, &["link"]).unwrap().stdout,
        "linked alpha as alpha\n"
    );
    assert!(weaver(dir.path(), &["scan"])
        .unwrap()
        .stdout
        .contains("alpha  linked"));
    assert_eq!(
        mind(&alpha, &["unlink"]).unwrap().stdout,
        "unlinked alpha\n"
    );

    let alone = TempDir::new().unwrap();
    mind(alone.path(), &["init"]).unwrap();
    let failure = mind(alone.path(), &["link"]).unwrap_err();
    assert!(matches!(*failure.error, CliError::NoWorkspaceAbove(_)));
}

#[test]
fn a_skill_of_the_workspaces_installs_at_either_level() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");

    let from_the_project = mind(&alpha, &["install", "commit-style"]).unwrap();
    let from_the_workspace =
        weaver(dir.path(), &["install", "-p", "beta", "commit-style"]).unwrap();

    assert_eq!(
        from_the_project.stdout,
        "installed commit-style into alpha, from workspace\n"
    );
    assert_eq!(
        from_the_workspace.stdout,
        "installed commit-style into beta, from workspace\n"
    );
    let (in_alpha, in_beta) = (
        copies(&alpha, "commit-style"),
        copies(&dir.path().join("beta"), "commit-style"),
    );
    for copy in in_alpha.iter().chain(&in_beta) {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }

    assert_eq!(
        mind(&alpha, &["uninstall", "commit-style"]).unwrap().stdout,
        "removed commit-style from alpha\n"
    );
    for copy in &in_alpha {
        assert!(!copy.exists(), "{}", copy.display());
    }
    let again = mind(&alpha, &["uninstall", "commit-style"]).unwrap_err();
    assert!(matches!(*again.error, CliError::NotInstalled { .. }));
}

#[cfg(unix)]
#[test]
fn a_copy_holding_a_link_is_left_alone_until_an_install_is_forced_to_write_over_it() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    mind(&alpha, &["install", "commit-style"]).unwrap();
    // A file in the copy swapped for a link to the same bytes, which the
    // hash refuses.
    let file = copies(&alpha, "commit-style")[0].join("SKILL.md");
    let elsewhere = dir.path().join("SKILL.md");
    fs::rename(&file, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &file).unwrap();

    // No install writes a link, so one there is an edit, left alone until
    // the install is forced.
    let refused = mind(&alpha, &["install", "commit-style"]).unwrap();
    assert_eq!(refused.stdout, "");
    assert_eq!(
        refused.stderr,
        [format!(
            "warning: commit-style in alpha was changed since it was installed, at {}, so it \
             was left alone; --force writes over it",
            copies(&alpha, "commit-style")[0]
                .strip_prefix(&alpha)
                .unwrap()
                .display()
        )]
    );
    assert!(!refused.ok);
    let again = mind(&alpha, &["install", "commit-style", "--force"]).unwrap();

    // Written over, as it says, though the link led to the same bytes.
    assert_eq!(
        again.stdout,
        "updated commit-style in alpha, from workspace\n"
    );
    assert!(again.stderr.is_empty(), "{:?}", again.stderr);
    assert!(again.ok);
    assert!(!fs::symlink_metadata(&file)
        .unwrap()
        .file_type()
        .is_symlink());
    let status = mind(&alpha, &["status"]).unwrap().stdout;
    assert!(status.contains("commit-style  current"), "{status}");
}

#[test]
fn a_copy_changed_since_it_was_installed_is_taken_out_only_with_force() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    mind(&alpha, &["install", "commit-style"]).unwrap();
    let copies = copies(&alpha, "commit-style");
    let manifest = copies[0].join("SKILL.md");
    let edited = fs::read_to_string(&manifest).unwrap() + "Ours.\n";
    fs::write(&manifest, &edited).unwrap();

    let refused = mind(&alpha, &["uninstall", "commit-style"]).unwrap();
    let from_the_workspace =
        weaver(dir.path(), &["uninstall", "-p", "alpha", "commit-style"]).unwrap();

    let said = format!(
        "warning: commit-style in alpha was changed since it was installed, at {}, so it was \
         left alone; --force takes it out",
        // A route in the project, which is said with `/` on every platform.
        mindweaver_core::paths::to_config_string(copies[0].strip_prefix(&alpha).unwrap()).unwrap()
    );
    for refused in [&refused, &from_the_workspace] {
        assert_eq!(refused.stdout, "");
        assert_eq!(refused.stderr, [said.as_str()]);
        assert!(!refused.ok);
    }
    assert_eq!(fs::read_to_string(&manifest).unwrap(), edited);
    for copy in &copies {
        assert!(copy.is_dir(), "{}", copy.display());
    }

    let forced = weaver(
        dir.path(),
        &["uninstall", "-p", "alpha", "commit-style", "--force"],
    )
    .unwrap();

    assert_eq!(forced.stdout, "removed commit-style from alpha\n");
    for copy in &copies {
        assert!(!copy.exists(), "{}", copy.display());
    }
}

#[test]
fn a_copy_changed_since_it_was_installed_is_written_over_only_with_force() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    mind(&alpha, &["install", "commit-style"]).unwrap();
    let copies = copies(&alpha, "commit-style");
    let manifest = copies[0].join("SKILL.md");
    let written = fs::read_to_string(&manifest).unwrap();
    let edited = written.clone() + "Ours.\n";
    let status = |of: &str| {
        let report = mind(&alpha, &["status"]).unwrap().stdout;
        assert!(report.contains(&format!("commit-style  {of}")), "{report}");
    };

    // From the project, then from the workspace: each edit is left alone
    // until the install is forced, and forcing writes it over.
    for from_the_workspace in [false, true] {
        let install = |force: bool| {
            let mut args = if from_the_workspace {
                vec!["install", "-p", "alpha", "commit-style"]
            } else {
                vec!["install", "commit-style"]
            };
            if force {
                args.push("--force");
            }
            if from_the_workspace {
                weaver(dir.path(), &args)
            } else {
                mind(&alpha, &args)
            }
            .unwrap()
        };
        fs::write(&manifest, &edited).unwrap();
        status("edited");

        let refused = install(false);
        assert!(!refused.ok);
        assert_eq!(fs::read_to_string(&manifest).unwrap(), edited);
        let forced = install(true);

        assert!(forced.ok, "{:?}", forced.stderr);
        assert_eq!(
            forced.stdout,
            "updated commit-style in alpha, from workspace\n"
        );
        assert_eq!(fs::read_to_string(&manifest).unwrap(), written);
        status("current");
    }
}

#[test]
fn force_is_for_one_install_and_not_for_the_screen() {
    let dir = workspace_with_own();
    let line = line("mindweaver", &["install", "--force"], dir.path());

    let refused = WeaverCli::try_parse_from(&line).unwrap_err();

    assert!(refused.to_string().contains("--force"), "{refused}");
}

#[test]
fn installing_without_a_screen_needs_a_project_and_a_skill() {
    let dir = workspace_with_own();

    let nowhere = weaver(dir.path(), &["install", "commit-style"]).unwrap_err();
    let nothing = weaver(dir.path(), &["install", "-p", "alpha"]).unwrap_err();

    assert!(matches!(*nowhere.error, CliError::InstallWhere));
    assert!(matches!(*nothing.error, CliError::InstallWhat));
}

#[test]
fn installing_needs_the_project_to_be_managed() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let loose = dir.path().join("loose");
    fs::create_dir(&loose).unwrap();
    mind(&loose, &["init"]).unwrap();

    let failure = mind(&loose, &["install", "anything"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotManaged { .. }));
    assert!(failure.error.to_string().contains("mind link"));
}

#[test]
fn one_name_from_two_places_is_installed_from_the_one_named() {
    let (dir, _workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    weaver(dir.path(), &["add", "skill", "deploy", "Our own deploy"]).unwrap();
    let project = dir.path().join("collapse");

    let twice = mind(&project, &["install", "deploy"]).unwrap_err();
    let ours = mind(&project, &["install", "deploy", "--from", "workspace"]).unwrap();

    assert!(
        twice.error.to_string().contains("--from"),
        "{}",
        twice.error
    );
    assert!(ours.stdout.contains("from workspace"), "{}", ours.stdout);
    for copy in copies(&project, "deploy") {
        let installed = fs::read_to_string(copy.join("SKILL.md")).unwrap();
        assert!(installed.contains("Our own deploy"), "{}", copy.display());
    }
}

#[test]
fn mind_on_its_own_is_the_tui_too() {
    let bare = Cli::try_parse_from(["mind"]).unwrap();
    let named = Cli::try_parse_from(["mind", "tui"]).unwrap();

    assert!(bare.command.is_none());
    assert!(matches!(named.command, Some(mindweaver_cli::Command::Tui)));
}
