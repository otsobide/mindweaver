//! The workspaces this machine knows: remembered by `mindweaver init` in the
//! default workspace, listed by `mindweaver workspaces`, and forgotten by
//! `mindweaver workspaces forget`, each with a temporary home.

mod common;

use common::*;

/// A temporary directory holding `home`, a made default workspace.
fn made_home() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    weaver_at_home(&home, &home, &["init"]).unwrap();
    (dir, home)
}

fn known_in(home: &Path) -> Vec<PathBuf> {
    WeaverWorkspace::open(home).unwrap().config().known.clone()
}

// ---------------------------------------------------------------------------
// Remembering
// ---------------------------------------------------------------------------

#[test]
fn init_remembers_the_workspace_in_the_default_one() {
    let (_dir, home) = made_home();

    let outcome = weaver_at_home(&home, &home, &["init", "weavers/writing"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "initialized weaver workspace `writing` in {}\n  remembered as weavers/writing by the default workspace\n",
            home.join("weavers/writing").join(WEAVER_DIR).display()
        )
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    assert_eq!(known_in(&home), [PathBuf::from("weavers/writing")]);
}

#[test]
fn init_again_remembers_a_workspace_made_before_the_default_and_nothing_more() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    weaver_at_home(dir.path(), &home, &["init", "writing"]).unwrap();
    weaver_at_home(&home, &home, &["init"]).unwrap();
    let marker = fs::read_to_string(dir.path().join("writing/.mindweaver/weaver.toml")).unwrap();

    let first = weaver_at_home(dir.path(), &home, &["init", "writing"]).unwrap();
    let second = weaver_at_home(dir.path(), &home, &["init", "writing"]).unwrap();

    let already = format!(
        "weaver workspace `writing` already initialized in {}\n",
        dir.path().join("writing").join(WEAVER_DIR).display()
    );
    assert_eq!(
        first.stdout,
        format!("{already}  remembered as ../writing by the default workspace\n")
    );
    assert_eq!(second.stdout, already, "already known says nothing more");
    assert_eq!(known_in(&home), [PathBuf::from("../writing")]);
    assert_eq!(
        fs::read_to_string(dir.path().join("writing/.mindweaver/weaver.toml")).unwrap(),
        marker,
        "the workspace itself is left as it was"
    );
}

#[test]
fn init_remembers_nothing_with_no_default() {
    let (dir, home) = made_home();

    // `common::weaver` pins `--home=`, which names no default at all.
    let outcome = weaver(dir.path(), &["init", "writing"]).unwrap();

    assert!(!outcome.stdout.contains("remembered"), "{}", outcome.stdout);
    assert!(known_in(&home).is_empty());
}

#[test]
fn init_never_makes_the_default_to_remember_one() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();

    let outcome = weaver_at_home(dir.path(), &home, &["init", "writing"]).unwrap();

    assert!(!outcome.stdout.contains("remembered"), "{}", outcome.stdout);
    assert!(!home.join(WEAVER_DIR).exists(), "the default was made");
}

#[test]
fn the_default_workspace_does_not_remember_itself() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();

    let made = weaver_at_home(&home, &home, &["init"]).unwrap();
    let again = weaver_at_home(&home, &home, &["init"]).unwrap();

    assert!(!made.stdout.contains("remembered"), "{}", made.stdout);
    assert!(!again.stdout.contains("remembered"), "{}", again.stdout);
    assert!(!registry(&home).contains("known"), "{}", registry(&home));
}

#[test]
fn a_default_that_cannot_remember_is_a_warning_after_the_init() {
    let (dir, home) = made_home();
    let path = home.join(WEAVER_DIR).join(WEAVER_CONFIG);
    let newer = registry(&home).replace("version = 2", "version = 99");
    fs::write(&path, &newer).unwrap();

    let outcome = weaver_at_home(dir.path(), &home, &["init", "writing"]).unwrap();

    assert!(
        outcome
            .stdout
            .starts_with("initialized weaver workspace `writing`"),
        "{}",
        outcome.stdout
    );
    assert_eq!(
        outcome.stderr,
        [format!(
            "warning: the default workspace could not remember it: {}: written by a newer Mindweaver (workspace marker version 99, this one reads 2)",
            path.display()
        )]
    );
    assert!(!outcome.ok);
    assert!(dir.path().join("writing").join(WEAVER_DIR).is_dir());
    assert_eq!(fs::read_to_string(&path).unwrap(), newer);
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

/// `made_home`, with `home/weavers/writing` and `notes`, beside home, made
/// and remembered, and a project linked into `writing`.
fn home_with_two() -> (TempDir, PathBuf) {
    let (dir, home) = made_home();
    weaver_at_home(&home, &home, &["init", "weavers/writing"]).unwrap();
    weaver_at_home(dir.path(), &home, &["init", "notes"]).unwrap();
    let writing = home.join("weavers/writing");
    fs::create_dir(writing.join("essays")).unwrap();
    new_project(&writing.join("essays"));
    weaver(&writing, &["link", "essays"]).unwrap();
    (dir, home)
}

#[test]
fn workspaces_lists_the_default_then_what_it_remembers_with_the_one_in_use_marked() {
    let (dir, home) = home_with_two();
    let writing = home.join("weavers/writing");

    let outcome = weaver_at_home(&writing.join("essays"), &home, &["workspaces"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "  home     {}                  no projects; the default one\n\
             * writing  {}  1 project\n  \
             notes    {}                 no projects\n",
            home.display(),
            writing.display(),
            dir.path().join("notes").display(),
        )
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
}

#[test]
fn mind_weaver_workspaces_is_the_same_list() {
    let (_dir, home) = home_with_two();
    let essays = home.join("weavers/writing/essays");

    let weaver = weaver_at_home(&essays, &home, &["workspaces"]).unwrap();
    let mind = mind_at_home(&essays, &home, &["weaver", "workspaces"]).unwrap();

    assert_eq!(mind, weaver);
}

#[test]
fn a_remembered_workspace_that_has_gone_is_shown_not_raised() {
    let (dir, home) = home_with_two();
    let notes = dir.path().join("notes");
    fs::remove_dir_all(&notes).unwrap();

    let outcome = weaver_at_home(&home, &home, &["workspaces"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "* home     {}                  no projects; the default one\n  \
             writing  {}  1 project\n  \
             -        {}                 cannot be opened\n\
             \n  `mindweaver workspaces forget <path>` forgets one that cannot be opened\n",
            home.display(),
            home.join("weavers/writing").display(),
            notes.display(),
        )
    );
    assert_eq!(
        outcome.stderr,
        [format!(
            "warning: {}: not a weaver workspace (no .mindweaver/weaver.toml or .mindflayer/flayer.toml)",
            notes.display()
        )]
    );
    assert!(!outcome.ok);
}

#[test]
fn stale_projects_are_counted_beside_the_rest() {
    let (_dir, home) = home_with_two();
    let writing = home.join("weavers/writing");
    fs::create_dir(writing.join("letters")).unwrap();
    new_project(&writing.join("letters"));
    weaver(&writing, &["link", "letters"]).unwrap();
    fs::remove_dir_all(writing.join("essays")).unwrap();

    let outcome = weaver_at_home(&home, &home, &["workspaces"]).unwrap();

    assert!(
        outcome.stdout.contains("  2 projects, 1 stale\n"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome.ok,
        "a project's stale entry is its workspace's to say"
    );
}

#[test]
fn one_in_use_that_nothing_remembers_is_listed_with_how_to_remember_it() {
    let (dir, home) = made_home();
    weaver(dir.path(), &["init", "elsewhere"]).unwrap();
    let elsewhere = dir.path().join("elsewhere");

    let outcome = weaver_at_home(&elsewhere, &home, &["workspaces"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "  home       {}       no projects; the default one\n\
             * elsewhere  {}  no projects; not remembered\n\
             \n  `mindweaver init <path>` remembers the one in use, and changes nothing else in it\n",
            home.display(),
            elsewhere.display(),
        )
    );
}

#[test]
fn listing_never_makes_the_default() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();
    weaver(dir.path(), &["init", "writing"]).unwrap();
    let writing = dir.path().join("writing");

    let found = weaver_at_home(&writing, &home, &["workspaces"]).unwrap();
    let nowhere = weaver_at_home(&home, &home, &["workspaces"]).unwrap();

    assert_eq!(
        found.stdout,
        format!(
            "* writing  {}  no projects; not remembered\n",
            writing.display()
        ),
        "no `mindweaver init` hint while there is no default to remember it in"
    );
    assert_eq!(nowhere.stdout, "no workspaces known\n");
    assert_eq!(
        nowhere.stderr,
        [format!(
            "hint: no workspace here or above, and the default one is not made yet; the first command that writes to it, such as `mindweaver link <path>`, makes it in {}",
            home.join(WEAVER_DIR).display()
        )]
    );
    assert!(nowhere.ok, "a hint is no failure");
    assert!(!home.join(WEAVER_DIR).exists(), "the default was made");
}

#[test]
fn nothing_is_known_with_no_default() {
    let (_dir, home) = home_with_two();

    // `common::weaver` pins `--home=`: the default and what it remembers are
    // out of reach, and only the workspace in use is left to list.
    let outcome = weaver(&home.join("weavers/writing"), &["workspaces"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "* writing  {}  1 project; not remembered\n",
            home.join("weavers/writing").display()
        )
    );
}

/// What `mindweaver workspaces --json` prints in `data`, field by field.
#[derive(Debug, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Listed {
    workspaces: Vec<Workspace>,
}

#[derive(Debug, serde::Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Workspace {
    name: Option<String>,
    path: String,
    default: bool,
    remembered: bool,
    in_use: bool,
    projects: usize,
    stale_projects: usize,
    stale: Option<String>,
}

#[test]
fn json_prints_the_list_in_the_envelope_with_its_warnings() {
    let (dir, home) = home_with_two();
    let notes = dir.path().join("notes");
    fs::remove_dir_all(&notes).unwrap();
    let writing = home.join("weavers/writing");

    let outcome = weaver_at_home(&writing, &home, &["workspaces", "--json"]).unwrap();

    let (listed, warnings): (Listed, _) = enveloped(&outcome);
    let gone = format!(
        "{}: not a weaver workspace (no .mindweaver/weaver.toml or .mindflayer/flayer.toml)",
        notes.display()
    );
    assert_eq!(
        listed.workspaces,
        [
            Workspace {
                name: Some(String::from("home")),
                path: home.to_string_lossy().into_owned(),
                default: true,
                remembered: false,
                in_use: false,
                projects: 0,
                stale_projects: 0,
                stale: None,
            },
            Workspace {
                name: Some(String::from("writing")),
                path: writing.to_string_lossy().into_owned(),
                default: false,
                remembered: true,
                in_use: true,
                projects: 1,
                stale_projects: 0,
                stale: None,
            },
            Workspace {
                name: None,
                path: notes.to_string_lossy().into_owned(),
                default: false,
                remembered: true,
                in_use: false,
                projects: 0,
                stale_projects: 0,
                stale: Some(gone.clone()),
            },
        ]
    );
    // On stderr, as the text says it, for whoever reads the terminal, and in
    // the envelope for a script.
    assert_eq!(outcome.stderr, [format!("warning: {gone}")]);
    assert_eq!(warnings, [gone]);
    assert!(!outcome.ok, "the exit is the text's");
}

// ---------------------------------------------------------------------------
// Forgetting
// ---------------------------------------------------------------------------

#[test]
fn forget_removes_every_spelling_of_a_path() {
    let (_dir, home) = home_with_two();
    let writing = home.join("weavers/writing");
    let path = home.join(WEAVER_DIR).join(WEAVER_CONFIG);
    let absolute = writing.to_string_lossy().into_owned();
    let text = registry(&home).replace(
        "\"weavers/writing\",",
        // A literal string, which reads no escapes: a Windows path in a
        // basic one would.
        &format!("\"weavers/writing\", \"./weavers/writing/\", '{absolute}',"),
    );
    fs::write(&path, text).unwrap();

    let outcome =
        weaver_at_home(&home, &home, &["workspaces", "forget", "weavers/writing"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!("forgot weavers/writing\nforgot ./weavers/writing/\nforgot {absolute}\n")
    );
    assert_eq!(known_in(&home), [PathBuf::from("../notes")]);
    let listed = weaver_at_home(&home, &home, &["workspaces"]).unwrap();
    assert!(!listed.stdout.contains("writing"), "{}", listed.stdout);
    assert!(
        writing.join(WEAVER_DIR).is_dir(),
        "the workspace itself stays"
    );
}

#[test]
fn a_workspace_that_has_gone_is_forgotten_by_its_path() {
    let (dir, home) = home_with_two();
    let notes = dir.path().join("notes");
    fs::remove_dir_all(&notes).unwrap();

    let outcome = weaver_at_home(dir.path(), &home, &["workspaces", "forget", "notes"]).unwrap();
    let listed = weaver_at_home(&home, &home, &["workspaces"]).unwrap();

    assert_eq!(outcome.stdout, "forgot ../notes\n");
    assert!(listed.ok && listed.stderr.is_empty(), "{listed:?}");
    assert!(
        !listed.stdout.contains("cannot be opened"),
        "{}",
        listed.stdout
    );
}

#[test]
fn forgetting_what_is_not_known_is_refused_and_writes_nothing() {
    let (dir, home) = home_with_two();
    let before = registry(&home);

    let failure =
        weaver_at_home(dir.path(), &home, &["workspaces", "forget", "elsewhere"]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        format!(
            "{}: not known to the default workspace at {}",
            dir.path().join("elsewhere").display(),
            home.display()
        )
    );
    assert_eq!(registry(&home), before);
}

#[test]
fn forgetting_with_no_default_is_refused() {
    let (dir, home) = home_with_two();
    let before = registry(&home);

    let failure = weaver(dir.path(), &["workspaces", "forget", "notes"]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        format!(
            "{}: not known, since there is no default workspace to remember it (--home or $MINDWEAVER_HOME names none)",
            dir.path().join("notes").display()
        )
    );
    assert_eq!(registry(&home), before);
}

#[test]
fn forgetting_with_the_default_not_made_is_refused_and_makes_nothing() {
    let dir = TempDir::new().unwrap();
    let home = dir.path().join("home");
    fs::create_dir(&home).unwrap();

    let failure =
        weaver_at_home(dir.path(), &home, &["workspaces", "forget", "notes"]).unwrap_err();

    assert!(
        matches!(*failure.error, CliError::Workspace(_)),
        "{}",
        failure.error
    );
    assert!(!home.join(WEAVER_DIR).exists(), "the default was made");
}

#[test]
fn json_is_for_the_list_and_not_for_forget() {
    let dir = TempDir::new().unwrap();

    let line = line(
        "mindweaver",
        &["workspaces", "--json", "forget", "notes"],
        dir.path(),
    );

    assert!(WeaverCli::try_parse_from(&line).is_err(), "{line:?}");
}
