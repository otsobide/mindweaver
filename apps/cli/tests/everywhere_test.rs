//! `mindweaver remove --everywhere` (#50): one of the workspace's own taken out
//! of every project it links, what was changed since it was installed and
//! somebody's own of that name left and said, and then the library copy.
//! And `uninstall --orphaned`, which takes out what removing it from the
//! workspace alone left behind.

mod common;

use std::collections::BTreeMap;

use common::*;

/// A workspace with a skill of its own, `commit-style`, linking `alpha`,
/// `beta`, `collapse` and `tanukeys`: installed into the first three,
/// changed by hand in `collapse`, and `tanukeys` holding somebody's own of
/// that name.
fn workspace_with_copies() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    for name in ["alpha", "beta", "collapse", "tanukeys"] {
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
    let installed = weaver(
        dir.path(),
        &[
            "install",
            "commit-style",
            "-p",
            "alpha",
            "-p",
            "beta",
            "-p",
            "collapse",
        ],
    )
    .unwrap();
    assert!(installed.ok, "{:?}", installed.stderr);
    fs::write(
        dir.path()
            .join("collapse/.claude/skills/commit-style/SKILL.md"),
        skill_file("commit-style", "Our way"),
    )
    .unwrap();
    let theirs = dir.path().join("tanukeys/.claude/skills/commit-style");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill_file("commit-style", "Mine")).unwrap();
    dir
}

/// Every file under `root`, by its path, with what it holds: what a dry run
/// must leave exactly as it found it.
fn every_file(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut folders = vec![root.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                files.insert(path.clone(), fs::read(&path).unwrap());
            }
        }
    }
    files
}

#[test]
fn remove_everywhere_without_yes_says_each_copy_with_how_it_stands_then_the_library_copy() {
    let dir = workspace_with_copies();
    let library = own_directory_for(dir.path(), Kind::Skill).join("commit-style");

    let outcome = weaver(dir.path(), &["remove", "commit-style", "--everywhere"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "\
would remove commit-style from alpha, where it is current
would remove commit-style from beta, where it is current
would leave commit-style in collapse, where it is edited, at .claude/skills/commit-style
would leave commit-style in tanukeys, where it is foreign, at .claude/skills/commit-style
would remove skill commit-style from {} and the 1 file in it
nothing removed: add --yes to go ahead
",
            library.display()
        )
    );
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(outcome.ok);
}

#[test]
fn the_dry_run_writes_nothing() {
    let dir = workspace_with_copies();
    let before = every_file(dir.path());

    weaver(dir.path(), &["remove", "commit-style", "--everywhere"]).unwrap();
    weaver(dir.path(), &["remove", "skill:commit-style"]).unwrap_err();

    assert_eq!(every_file(dir.path()), before);
}

#[test]
fn a_dry_run_makes_no_default_workspace() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    for args in [
        &["remove", "commit-style", "--everywhere"][..],
        &["remove", "commit-style"][..],
    ] {
        let failure = weaver_at_home(elsewhere.path(), home.path(), args).unwrap_err();
        assert!(
            failure
                .error
                .to_string()
                .contains("nothing named `commit-style`"),
            "{}",
            failure.error
        );
    }

    assert!(!home.path().join(WEAVER_DIR).exists());
}

#[test]
fn removing_it_everywhere_leaves_an_edited_copy_and_somebodys_and_says_so() {
    let dir = workspace_with_copies();
    let library = own_directory_for(dir.path(), Kind::Skill).join("commit-style");

    let outcome = weaver(
        dir.path(),
        &["remove", "commit-style", "--everywhere", "--yes"],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "\
removed commit-style from alpha
removed commit-style from beta
left commit-style in collapse, where it is edited, at .claude/skills/commit-style
left commit-style in tanukeys, where it is foreign, at .claude/skills/commit-style
removed skill commit-style from {}
",
            library.display()
        )
    );
    assert_eq!(
        outcome.stderr,
        [
            "note: nothing offers commit-style in collapse any more; `mindweaver uninstall -p \
             collapse commit-style --force` takes it out"
        ]
    );
    assert!(outcome.ok);
    assert!(!library.exists());
    for name in ["alpha", "beta"] {
        for copy in copies(&dir.path().join(name), "commit-style") {
            assert!(!copy.exists(), "{}", copy.display());
        }
    }
    for copy in copies(&dir.path().join("collapse"), "commit-style") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
    let theirs = dir
        .path()
        .join("tanukeys/.claude/skills/commit-style/SKILL.md");
    assert!(fs::read_to_string(theirs).unwrap().contains("Mine"));

    // The edited copy is still Mindweaver's install, and the note's command
    // takes it out.
    let status = weaver(dir.path(), &["status", "-p", "collapse"]).unwrap();
    assert!(
        status.stdout.contains("commit-style  edited"),
        "{}",
        status.stdout
    );
    let forced = weaver(
        dir.path(),
        &["uninstall", "-p", "collapse", "commit-style", "--force"],
    )
    .unwrap();
    assert_eq!(forced.stdout, "removed commit-style from collapse\n");
}

#[test]
fn a_rule_of_the_workspaces_own_goes_from_every_project_that_received_it() {
    let dir = workspace_with_copies();
    weaver(dir.path(), &["add", "rule", "git/no-force-push", "Never"]).unwrap();
    weaver(
        dir.path(),
        &["install", "git/no-force-push", "-p", "alpha", "-p", "beta"],
    )
    .unwrap();

    let would = weaver(dir.path(), &["remove", "git/no-force-push", "--everywhere"]).unwrap();
    assert!(
        would.stdout.starts_with(
            "\
would remove rule git/no-force-push from alpha, where it is current
would remove rule git/no-force-push from beta, where it is current
would remove rule git/no-force-push from "
        ),
        "{}",
        would.stdout
    );

    let outcome = weaver(
        dir.path(),
        &["remove", "rule:git/no-force-push", "--everywhere", "--yes"],
    )
    .unwrap();

    assert!(outcome.ok, "{:?}", outcome.stderr);
    assert!(
        outcome.stdout.starts_with(
            "\
removed rule git/no-force-push from alpha
removed rule git/no-force-push from beta
removed rule git/no-force-push from "
        ),
        "{}",
        outcome.stdout
    );
    for name in ["alpha", "beta"] {
        let block = fs::read_to_string(dir.path().join(name).join("AGENTS.md")).unwrap_or_default();
        assert!(!block.contains("git/no-force-push"), "{block}");
    }
}

#[test]
fn everywhere_is_for_the_workspaces_own_and_not_a_projects() {
    let dir = workspace_with_copies();

    let refused = WeaverCli::try_parse_from(line(
        "mindweaver",
        &["remove", "commit-style", "-p", "alpha", "--everywhere"],
        dir.path(),
    ))
    .unwrap_err();

    assert!(
        refused.to_string().contains("cannot be used with"),
        "{refused}"
    );
}

#[test]
fn removing_one_of_the_workspaces_own_alone_says_whose_copies_it_orphans() {
    let dir = workspace_with_copies();
    let library = own_directory_for(dir.path(), Kind::Skill).join("commit-style");

    let failure = weaver(dir.path(), &["remove", "commit-style"]).unwrap_err();
    assert!(matches!(*failure.error, CliError::Unconfirmed { .. }));
    assert_eq!(
        failure.warnings,
        [
            "warning: commit-style is installed in alpha, beta and collapse, which removing it \
             alone would leave orphaned there; --everywhere takes it out of them too"
        ]
    );

    let outcome = weaver(dir.path(), &["remove", "commit-style", "--yes"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!("removed skill commit-style from {}\n", library.display())
    );
    assert_eq!(
        outcome.stderr,
        [
            "warning: commit-style is still installed in alpha, beta and collapse, orphaned now \
             that nothing offers it; `mindweaver uninstall --orphaned --all-projects` takes out what \
             nothing offers"
        ]
    );
    assert!(!outcome.ok);
    let status = weaver(dir.path(), &["status", "-p", "alpha"]).unwrap();
    assert!(
        status.stdout.contains("commit-style  orphaned"),
        "{}",
        status.stdout
    );
}

/// Takes write permission away from a folder, and gives it back when
/// dropped, so that a failed assertion does not leave a temporary folder
/// that cannot be cleaned up.
#[cfg(unix)]
struct ReadOnly(PathBuf);

#[cfg(unix)]
impl ReadOnly {
    fn new(folder: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(folder, fs::Permissions::from_mode(0o555)).unwrap();
        ReadOnly(folder.to_path_buf())
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
    }
}

// Unix only: a folder's write permission is what is taken away, and
// Windows keeps none on a folder.
#[cfg(unix)]
#[test]
fn a_removal_that_fails_does_not_say_its_copies_are_orphaned() {
    let dir = workspace_with_copies();
    weaver(dir.path(), &["add", "rule", "git/no-force-push", "Never"]).unwrap();
    weaver(dir.path(), &["install", "git/no-force-push", "-p", "alpha"]).unwrap();
    let folder = own_directory_for(dir.path(), Kind::Rule).join("git");
    let _read_only = ReadOnly::new(&folder);

    let failure = weaver(dir.path(), &["remove", "git/no-force-push", "--yes"]).unwrap_err();

    assert!(
        failure.error.to_string().contains("cannot be changed"),
        "{}",
        failure.error
    );
    // Still offered from the library, so nothing is orphaned.
    assert!(failure.warnings.is_empty(), "{:?}", failure.warnings);
    assert!(folder.join("no-force-push.md").is_file());
}

#[test]
fn removing_one_installed_nowhere_says_nothing_of_projects() {
    let dir = workspace_with_copies();
    weaver(dir.path(), &["add", "skill", "review", "How reviews go"]).unwrap();

    let outcome = weaver(dir.path(), &["remove", "review", "--yes"]).unwrap();

    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(outcome.ok);
}

#[test]
fn uninstall_orphaned_takes_out_what_nothing_offers_any_more_but_an_edit() {
    let dir = workspace_with_copies();
    weaver(dir.path(), &["add", "skill", "review", "How reviews go"]).unwrap();
    weaver(dir.path(), &["install", "review", "--all-projects"]).unwrap();
    fs::remove_dir_all(own_directory_for(dir.path(), Kind::Skill).join("commit-style")).unwrap();

    let outcome = weaver(dir.path(), &["uninstall", "--orphaned", "--all-projects"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "\
removed commit-style from alpha
removed commit-style from beta
"
    );
    assert_eq!(
        outcome.stderr,
        [
            "warning: commit-style in collapse was changed since it was installed, at \
             .claude/skills/commit-style, so it was left alone; --force takes it out"
        ]
    );
    assert!(!outcome.ok, "an edited one was left");
    for name in ["alpha", "beta", "collapse", "tanukeys"] {
        for copy in copies(&dir.path().join(name), "review") {
            assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
        }
    }
    let theirs = dir
        .path()
        .join("tanukeys/.claude/skills/commit-style/SKILL.md");
    assert!(fs::read_to_string(&theirs).unwrap().contains("Mine"));

    let forced = weaver(
        dir.path(),
        &["uninstall", "--orphaned", "-p", "collapse", "--force"],
    )
    .unwrap();
    assert_eq!(forced.stdout, "removed commit-style from collapse\n");
    assert!(forced.ok, "{:?}", forced.stderr);
    assert!(theirs.is_file());
}

#[test]
fn uninstall_orphaned_leaves_what_cannot_be_read_and_says_why() {
    let dir = workspace_with_copies();
    let manifest = own_directory_for(dir.path(), Kind::Skill)
        .join("commit-style")
        .join("SKILL.md");
    fs::write(
        &manifest,
        "---\nname: commit-style\ndescription: [unclosed\n---\n",
    )
    .unwrap();

    let outcome = weaver(dir.path(), &["uninstall", "--orphaned", "-p", "alpha"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "nothing removed: nothing installed in alpha is orphaned\n"
    );
    assert_eq!(outcome.stderr.len(), 1, "{:?}", outcome.stderr);
    assert!(
        outcome.stderr[0]
            .text
            .starts_with(&format!("warning: {}: ", manifest.display())),
        "{:?}",
        outcome.stderr
    );
    assert!(!outcome.ok, "what could not be read is a warning");
    for copy in copies(&dir.path().join("alpha"), "commit-style") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

#[test]
fn mind_uninstall_orphaned_takes_them_out_of_the_project_it_is_in() {
    let dir = workspace_with_copies();
    fs::remove_dir_all(own_directory_for(dir.path(), Kind::Skill).join("commit-style")).unwrap();

    let outcome = mind(&dir.path().join("alpha"), &["uninstall", "--orphaned"]).unwrap();

    assert_eq!(outcome.stdout, "removed commit-style from alpha\n");
    assert!(outcome.ok, "{:?}", outcome.stderr);
    for copy in copies(&dir.path().join("beta"), "commit-style") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

#[test]
fn uninstall_orphaned_with_nothing_orphaned_says_so_and_does_not_fail() {
    let dir = workspace_with_copies();

    let outcome = weaver(
        dir.path(),
        &["uninstall", "--orphaned", "-p", "alpha", "-p", "beta"],
    )
    .unwrap();

    assert_eq!(
        outcome.stdout,
        "nothing removed: nothing installed in alpha and beta is orphaned\n"
    );
    assert!(outcome.ok, "{:?}", outcome.stderr);
}

#[test]
fn uninstall_takes_a_name_or_orphaned_and_not_both() {
    let dir = TempDir::new().unwrap();
    for args in [
        &["uninstall", "-p", "alpha"][..],
        &["uninstall", "-p", "alpha", "deploy", "--orphaned"][..],
    ] {
        assert!(
            WeaverCli::try_parse_from(line("mindweaver", args, dir.path())).is_err(),
            "{args:?}"
        );
    }
    assert!(Cli::try_parse_from(line("mind", &["uninstall"], dir.path())).is_err());
}
