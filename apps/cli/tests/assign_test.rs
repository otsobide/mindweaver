//! `mindweaver install` and `mindweaver uninstall` given several projects, by `-p`
//! once for each or by `--all-projects` (#49): each project's lines in the
//! order asked, one that refuses said of it while the rest go ahead, and a
//! refusal every project shares said once.

mod common;

use common::*;
use mindweaver_cli::Warning;

/// A workspace with a skill of its own, `commit-style`, linking `alpha`,
/// `beta` and `gamma`.
fn workspace_with_three() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    for name in ["alpha", "beta", "gamma"] {
        let root = dir.path().join(name);
        fs::create_dir(&root).unwrap();
        new_project(&root);
        weaver(dir.path(), &["link", name]).unwrap();
    }
    weaver(
        dir.path(),
        &["add", "skill", "commit-style", "How commits are written"],
    )
    .unwrap();
    dir
}

/// Somebody's own `commit-style`, written into `project` where Codex and
/// OpenCode read it, before Mindweaver put anything there.
fn somebodys(project: &Path) -> PathBuf {
    let copy = project.join(".agents/skills/commit-style");
    fs::create_dir_all(&copy).unwrap();
    fs::write(
        copy.join("SKILL.md"),
        skill_file("commit-style", "Our own way"),
    )
    .unwrap();
    copy
}

#[test]
fn installing_into_three_projects_leaves_somebodys_copy_alone_and_installs_into_the_other_two() {
    let dir = workspace_with_three();
    let theirs = somebodys(&dir.path().join("beta"));

    let outcome = weaver(
        dir.path(),
        &[
            "install",
            "commit-style",
            "-p",
            "alpha",
            "-p",
            "beta",
            "-p",
            "gamma",
        ],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        "\
installed commit-style into alpha, from workspace
installed commit-style into gamma, from workspace
"
    );
    assert_eq!(
        outcome.stderr,
        [
            "warning: beta already has a commit-style at .agents/skills/commit-style that \
             Mindweaver did not put there, so it was left alone"
        ]
    );
    assert!(!outcome.ok, "one of the three was left alone");
    for name in ["alpha", "gamma"] {
        for copy in copies(&dir.path().join(name), "commit-style") {
            assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
        }
    }
    assert!(fs::read_to_string(theirs.join("SKILL.md"))
        .unwrap()
        .contains("Our own way"));
    assert!(!dir.path().join("beta/.claude/skills/commit-style").exists());
}

#[test]
fn what_a_projects_rules_warn_of_is_said_of_that_project() {
    let dir = workspace_with_three();
    let rule = own_directory_for(dir.path(), Kind::Rule).join("style/comments.md");
    fs::create_dir_all(rule.parent().unwrap()).unwrap();
    fs::write(
        &rule,
        "---\npaths:\n  - \"**/*.rs\"\n---\n\n# Comments say why, not what\n",
    )
    .unwrap();
    // Claude alone reads a rule about some files, so `alpha`, naming no
    // other, has nothing to be warned of.
    weaver(dir.path(), &["agents", "-p", "alpha", "claude", "--yes"]).unwrap();

    let outcome = weaver(
        dir.path(),
        &["install", "style/comments", "-p", "alpha", "-p", "beta"],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        "\
installed rule style/comments into alpha, from workspace
installed rule style/comments into beta, from workspace
"
    );
    assert_eq!(
        outcome.stderr,
        [
            "warning: beta: rule `style/comments` states `paths`, so it reaches claude only: \
             codex and opencode read no .claude/rules"
        ]
    );
    assert!(!outcome.ok);

    // One project alone is said as it always was.
    let alone = weaver(dir.path(), &["install", "style/comments", "-p", "gamma"]).unwrap();

    assert_eq!(
        alone.stderr,
        [
            "warning: rule `style/comments` states `paths`, so it reaches claude only: codex \
             and opencode read no .claude/rules"
        ]
    );
}

#[test]
fn what_a_projects_rules_warn_of_after_an_uninstall_is_said_of_that_project() {
    let dir = workspace_with_three();
    weaver(
        dir.path(),
        &["add", "rule", "git/commits", "Commit one step at a time"],
    )
    .unwrap();
    weaver(dir.path(), &["add", "rule", "review/tone", "Review kindly"]).unwrap();
    for rule in ["git/commits", "review/tone"] {
        weaver(dir.path(), &["install", rule, "-p", "alpha", "-p", "beta"]).unwrap();
    }
    // Somebody's edit of another rule's section in `beta`, which every
    // render of its rules keeps and warns of.
    let agents_md = dir.path().join("beta").join("AGENTS.md");
    let rendered = fs::read_to_string(&agents_md).unwrap();
    assert!(rendered.contains("# Review kindly\n"), "{rendered}");
    fs::write(
        &agents_md,
        rendered.replace("# Review kindly\n", "# Review kindly, and briefly\n"),
    )
    .unwrap();

    let outcome = weaver(
        dir.path(),
        &["uninstall", "git/commits", "-p", "alpha", "-p", "beta"],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        "\
removed rule git/commits from alpha
removed rule git/commits from beta
"
    );
    assert_eq!(
        outcome.stderr,
        [format!(
            "warning: beta: {}: the section of rule `review/tone` was changed since it was \
             rendered, so it is left as it is",
            agents_md.display()
        )]
    );
    assert!(!outcome.ok);
}

#[test]
fn all_projects_installs_into_every_project_the_workspace_links() {
    let dir = workspace_with_three();

    let outcome = weaver(dir.path(), &["install", "commit-style", "--all-projects"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
installed commit-style into alpha, from workspace
installed commit-style into beta, from workspace
installed commit-style into gamma, from workspace
"
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    let again = weaver(dir.path(), &["install", "commit-style", "--all-projects"]).unwrap();
    assert_eq!(
        again.stdout,
        "\
commit-style in alpha was already current
commit-style in beta was already current
commit-style in gamma was already current
"
    );
}

#[test]
fn a_project_named_twice_is_installed_into_once() {
    let dir = workspace_with_three();

    let outcome = weaver(
        dir.path(),
        &["install", "commit-style", "-p", "alpha", "-p", "./alpha"],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        "installed commit-style into alpha, from workspace\n"
    );
}

#[test]
fn a_project_that_cannot_take_skills_says_why_and_the_rest_go_ahead() {
    let dir = workspace_with_three();
    // `beta` names no agent, so no skill has anywhere to go in it: written
    // by hand, since `mind agents` refuses to leave a project none.
    let settings = dir.path().join("beta").join(MIND_DIR).join(SETTINGS_FILE);
    let written = fs::read_to_string(&settings).unwrap();
    let agents = "agents = [\"claude\", \"codex\", \"opencode\"]";
    assert!(written.contains(agents), "{written}");
    fs::write(&settings, written.replace(agents, "agents = []")).unwrap();

    let outcome = weaver(dir.path(), &["install", "commit-style", "--all-projects"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
installed commit-style into alpha, from workspace
installed commit-style into gamma, from workspace
"
    );
    assert_eq!(
        outcome.stderr,
        [
            "error: beta: beta names no agent in its settings that skills are installed for, so \
             none can be installed into it"
        ]
    );
    assert!(!outcome.ok);
}

#[test]
fn a_name_no_project_is_offered_is_refused_once() {
    let dir = workspace_with_three();

    let failure = weaver(dir.path(), &["install", "nope", "--all-projects"]).unwrap_err();

    assert!(
        matches!(&*failure.error, CliError::Install(error) if error.to_string().starts_with("nothing called `nope`")),
        "{}",
        failure.error
    );
    assert!(failure.warnings.is_empty(), "{:?}", failure.warnings);
}

#[test]
fn a_project_the_workspace_does_not_link_refuses_the_install_before_any_is_made() {
    let dir = workspace_with_three();

    let failure = weaver(
        dir.path(),
        &["install", "commit-style", "-p", "alpha", "-p", "nowhere"],
    )
    .unwrap_err();

    assert!(
        matches!(&*failure.error, CliError::UnknownProject { wanted, .. } if wanted == "nowhere"),
        "{}",
        failure.error
    );
    assert!(copies(&dir.path().join("alpha"), "commit-style")
        .iter()
        .all(|copy| !copy.exists()));
}

#[test]
fn all_projects_and_a_project_are_not_asked_together() {
    let dir = workspace_with_three();
    for command in ["install", "uninstall"] {
        let line = line(
            "mindweaver",
            &[command, "commit-style", "-p", "alpha", "--all-projects"],
            dir.path(),
        );

        let refused = WeaverCli::try_parse_from(&line).unwrap_err();

        assert!(refused.to_string().contains("--all-projects"), "{refused}");
    }
}

#[test]
fn every_project_with_none_linked_installs_nothing_and_says_so() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let installed = weaver(dir.path(), &["install", "anything", "--all-projects"]).unwrap();
    let removed = weaver(dir.path(), &["uninstall", "anything", "--all-projects"]).unwrap();

    assert_eq!(
        installed.stdout,
        "nothing installed: no project is linked\n"
    );
    assert_eq!(removed.stdout, "nothing removed: no project is linked\n");
    assert!(installed.ok && removed.ok);
}

#[test]
fn every_project_of_a_default_workspace_nothing_has_made_is_none_and_makes_nothing() {
    let home = TempDir::new().unwrap();
    let here = TempDir::new().unwrap();

    for (command, done) in [("install", "installed"), ("uninstall", "removed")] {
        let outcome = weaver_at_home(
            here.path(),
            home.path(),
            &[command, "anything", "--all-projects"],
        )
        .unwrap();

        assert_eq!(
            outcome.stdout,
            format!("nothing {done}: no project is linked\n")
        );
        assert_eq!(outcome.stderr.len(), 1, "{:?}", outcome.stderr);
        assert!(
            outcome.stderr[0]
                .text
                .starts_with("hint: no workspace here or above"),
            "{:?}",
            outcome.stderr
        );
        assert!(outcome.ok);
    }
    assert!(!home.path().join(WEAVER_DIR).exists());
}

#[test]
fn uninstalling_from_several_takes_it_out_where_it_is_and_passes_over_where_it_is_not() {
    let dir = workspace_with_three();
    weaver(
        dir.path(),
        &["install", "commit-style", "-p", "alpha", "-p", "gamma"],
    )
    .unwrap();

    let outcome = weaver(dir.path(), &["uninstall", "commit-style", "--all-projects"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
removed commit-style from alpha
removed commit-style from gamma
"
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    for name in ["alpha", "beta", "gamma"] {
        for copy in copies(&dir.path().join(name), "commit-style") {
            assert!(!copy.exists(), "{}", copy.display());
        }
    }
}

#[test]
fn uninstalling_from_several_leaves_somebodys_copy_alone_and_says_so() {
    let dir = workspace_with_three();
    weaver(dir.path(), &["install", "commit-style", "-p", "alpha"]).unwrap();
    let theirs = somebodys(&dir.path().join("beta"));

    let outcome = weaver(
        dir.path(),
        &["uninstall", "commit-style", "-p", "alpha", "-p", "beta"],
    )
    .unwrap();

    assert_eq!(outcome.stdout, "removed commit-style from alpha\n");
    assert_eq!(
        outcome.stderr,
        [
            "warning: commit-style at .agents/skills/commit-style in beta was not installed by \
             Mindweaver, so it was left alone"
        ]
    );
    assert!(!outcome.ok);
    assert!(theirs.join("SKILL.md").is_file());
}

/// The panel tells somebody's copy from one changed since by the code
/// beside the line, which says it in words for a terminal alone.
#[test]
fn a_refusal_over_somebodys_copy_carries_the_code_foreign() {
    let dir = workspace_with_three();
    weaver(dir.path(), &["install", "commit-style", "-p", "alpha"]).unwrap();
    somebodys(&dir.path().join("beta"));

    let installed = weaver(dir.path(), &["install", "commit-style", "-p", "beta"]).unwrap();
    let among = weaver(
        dir.path(),
        &["install", "commit-style", "-p", "alpha", "-p", "beta"],
    )
    .unwrap();
    let removed = weaver(dir.path(), &["uninstall", "commit-style", "-p", "beta"]).unwrap();

    for refused in [&installed, &among, &removed] {
        let foreign: Vec<&Warning> = refused
            .stderr
            .iter()
            .filter(|line| {
                line.text.contains("Mindweaver did not put there")
                    || line.text.contains("was not installed by Mindweaver")
            })
            .collect();
        assert_eq!(foreign.len(), 1, "{refused:?}");
        assert_eq!(foreign[0].code, Some("foreign"), "{refused:?}");
        assert!(!refused.ok);
    }
}

#[test]
fn uninstalling_what_no_project_asked_has_is_refused() {
    let dir = workspace_with_three();

    let failure = weaver(
        dir.path(),
        &["uninstall", "commit-style", "-p", "alpha", "-p", "beta"],
    )
    .unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        "no project asked has commit-style installed, so there is nothing to take out"
    );
}

#[test]
fn uninstalling_needs_projects_to_take_it_out_of() {
    let dir = workspace_with_three();

    let failure = weaver(dir.path(), &["uninstall", "commit-style"]).unwrap_err();

    assert!(
        matches!(*failure.error, CliError::UninstallWhere),
        "{}",
        failure.error
    );
}
