//! The workspace in your home before anything has made it: every command
//! that only reads answers as an empty workspace would, says where the
//! workspace will be, and leaves the home as it found it.

mod common;

use common::*;

/// A home with no workspace in it, and a directory with none above it to
/// run commands from.
fn unmade_home() -> (TempDir, TempDir) {
    (TempDir::new().unwrap(), TempDir::new().unwrap())
}

/// Whether anything has made the workspace in `home`.
fn made(home: &Path) -> bool {
    home.join(WEAVER_DIR).exists()
}

/// The one line a read adds when the default workspace is not made yet.
fn hint(home: &Path) -> String {
    format!(
        "hint: no workspace here or above, and the default one is not made yet; \
         the first command that writes to it, such as `mindweaver link <path>`, makes it in {}",
        home.join(WEAVER_DIR).display()
    )
}

/// What `init` would call the workspace in `home`.
fn name(home: &Path) -> String {
    home.file_name().unwrap().to_string_lossy().into_owned()
}

#[test]
fn weaver_list_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["list"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "{} manages no projects yet\n  link one with `mindweaver link <path>`\n",
            name(home.path())
        )
    );
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    assert!(!made(home.path()));
}

#[test]
fn weaver_show_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();
    // Where the workspace will keep its own: read before it is made, as it
    // would be after.
    let skill = WeaverWorkspace::unmade(home.path())
        .unwrap()
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), skill_file("deploy", "Ship it")).unwrap();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["show", "deploy"]).unwrap();

    assert!(outcome.stdout.starts_with("deploy\n"), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("Ship it"), "{}", outcome.stdout);
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    // `.mindweaver` is there already, holding the library the skill is in,
    // and the read adds nothing beside it: no marker, no ledger, no cache.
    let inside: Vec<_> = fs::read_dir(home.path().join(WEAVER_DIR))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(inside, ["library"]);
}

#[test]
fn weaver_show_of_nothing_fails_as_in_an_empty_workspace_and_makes_nothing() {
    let (home, elsewhere) = unmade_home();

    let failure = weaver_at_home(elsewhere.path(), home.path(), &["show", "deploy"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::UnknownArtifact(_)));
    assert_eq!(failure.warnings, vec![hint(home.path())]);
    assert!(!made(home.path()));
}

#[test]
fn weaver_validate_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["validate"]).unwrap();

    assert_eq!(outcome.stdout, "nothing to check\n");
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    assert!(!made(home.path()));
}

#[test]
fn weaver_list_of_a_project_names_none_and_makes_nothing() {
    let (home, elsewhere) = unmade_home();

    let failure =
        weaver_at_home(elsewhere.path(), home.path(), &["list", "-p", "alpha"]).unwrap_err();

    assert!(matches!(
        *failure.error,
        CliError::UnknownProject { ref known, .. } if known.is_empty()
    ));
    assert!(!made(home.path()));
}

#[test]
fn weaver_scan_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();
    fs::create_dir_all(home.path().join("code").join("api").join(".git")).unwrap();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["scan"]).unwrap();

    // What is under it does not wait for it to be made. Spelled by its path,
    // since it is not under where the command ran: `mindweaver link code/api`
    // typed there would miss it.
    let api = home.path().join("code").join("api");
    assert!(
        outcome
            .stdout
            .starts_with(&format!("{}  not linked", api.display())),
        "{}",
        outcome.stdout
    );
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    assert!(!made(home.path()));
}

#[test]
fn weaver_scan_reads_a_default_workspace_whose_home_is_not_there_yet() {
    let (scratch, elsewhere) = unmade_home();
    let home = scratch.path().join("nobody-made-this");

    let outcome = weaver_at_home(elsewhere.path(), &home, &["scan"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "no mind projects or git repositories under {}\n",
            home.display()
        )
    );
    assert_eq!(outcome.stderr, vec![hint(&home)]);
    assert!(outcome.ok);
    assert!(!home.exists());
}

#[test]
fn weaver_templates_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["templates"]).unwrap();

    assert!(outcome.stdout.contains("full"), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("built in"), "{}", outcome.stdout);
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    assert!(!made(home.path()));
}

#[test]
fn weaver_gather_list_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["gather", "list"]).unwrap();

    assert!(
        outcome.stdout.starts_with("nothing gathered yet\n"),
        "{}",
        outcome.stdout
    );
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    assert!(!made(home.path()));
}

#[test]
fn weaver_load_on_its_own_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["load"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "nothing loaded\n  offer another workspace's own with `mindweaver load <path>`\n  \
             in the workspace at {}, the default one\n",
            home.path().display()
        )
    );
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    assert!(!made(home.path()));
}

#[test]
fn weaver_install_on_its_own_reads_the_default_workspace_without_making_it() {
    let (home, elsewhere) = unmade_home();

    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["install"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "{} manages no projects yet\n  link one with `mindweaver link <path>`\n",
            name(home.path())
        )
    );
    assert_eq!(outcome.stderr, vec![hint(home.path())]);
    assert!(outcome.ok);
    assert!(!made(home.path()));
}

#[test]
fn the_workspace_in_home_keeps_its_own_inside_its_mindweaver() {
    let (home, elsewhere) = unmade_home();

    let skill = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &["add", "skill", "x", "Mine"],
    )
    .unwrap();
    let rule = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &["add", "rule", "git/no-force-push", "Never force-push"],
    )
    .unwrap();

    let library = home.path().join(WEAVER_DIR).join("library");
    assert_eq!(
        created(&skill, "skill x"),
        library.join("skills/x/SKILL.md")
    );
    assert_eq!(
        created(&rule, "rule git/no-force-push"),
        library.join("rules/git/no-force-push.md")
    );
    // Nothing directly in the home directory, above every project in it: no
    // skills/ or rules/, and no AGENTS.md or CLAUDE.md for an agent to read.
    let top: Vec<_> = fs::read_dir(home.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(top, [WEAVER_DIR]);
    assert_eq!(fs::read_dir(elsewhere.path()).unwrap().count(), 0);
}

#[test]
fn once_something_writes_the_workspace_is_made_and_the_hint_is_gone() {
    let (home, elsewhere) = unmade_home();
    let alpha = elsewhere.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    mind(&alpha, &["init"]).unwrap();

    weaver_at_home(elsewhere.path(), home.path(), &["link", "alpha"]).unwrap();
    let outcome = weaver_at_home(elsewhere.path(), home.path(), &["list"]).unwrap();

    assert!(made(home.path()));
    assert!(outcome.stderr.is_empty(), "{:?}", outcome.stderr);
    assert!(outcome.stdout.contains("alpha"), "{}", outcome.stdout);
}
