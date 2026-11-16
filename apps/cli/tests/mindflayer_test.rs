//! What a Mindflayer build made keeps working (D-26): a workspace in a
//! `.mindflayer`, found walking up and as the default in home, read and
//! written under its own names, never given a `.mindweaver` beside it.
//!
//! Built by core's mother of what such a build made
//! (`mindweaver_core::mother::MindflayerMother`), never by hand.

mod common;

use common::*;
use mindweaver_core::mother::MindflayerMother;

/// Everything under `root` that says Mindweaver's names: none of it should
/// be there for a workspace under Mindflayer's.
fn made_new(root: &Path) -> bool {
    root.join(WEAVER_DIR).exists()
}

#[test]
fn a_workspace_a_mindflayer_build_made_links_adds_and_lists_under_its_own_names() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(dir.path());
    new_project(&dir.path().join("app"));

    weaver(dir.path(), &["link", "app"]).unwrap();
    let added = weaver(dir.path(), &["add", "skill", "review", "Read it first"]).unwrap();
    let listed = weaver(dir.path(), &["list"]).unwrap();

    assert_eq!(
        added.stdout,
        format!(
            "created skill review at {}\n",
            at(dir.path(), ".mindflayer/library/skills/review/SKILL.md")
        )
    );
    assert!(listed.stdout.contains("review"), "{}", listed.stdout);
    assert!(listed.ok, "{:?}", listed.stderr);
    let marker = std::fs::read_to_string(dir.path().join(".mindflayer/flayer.toml")).unwrap();
    assert!(marker.contains("projects = [\"app\"]"), "{marker}");
    assert!(!made_new(dir.path()));
}

#[test]
fn an_install_from_a_mindflayer_workspace_keeps_its_record_in_mindflayer_db() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(dir.path());
    new_project(&dir.path().join("app"));
    weaver(dir.path(), &["link", "app"]).unwrap();
    weaver(dir.path(), &["add", "skill", "review", "Read it first"]).unwrap();

    weaver(dir.path(), &["install", "-p", "app", "review"]).unwrap();
    let status = weaver(dir.path(), &["status"]).unwrap();

    assert!(dir.path().join(".mindflayer/mindflayer.db").is_file());
    assert!(status.stdout.contains("current"), "{}", status.stdout);
    assert!(!made_new(dir.path()));
}

#[test]
fn the_default_workspace_a_mindflayer_build_made_in_home_is_the_one_a_write_uses() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    MindflayerMother::create(home.path());

    let added = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &["add", "skill", "review", "Read it first"],
    )
    .unwrap();

    assert!(
        added
            .stdout
            .contains(&at(home.path(), ".mindflayer/library/skills/review")),
        "{}",
        added.stdout
    );
    assert!(!made_new(home.path()));
}

#[test]
fn init_where_a_mindflayer_build_made_a_workspace_makes_nothing_new() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(dir.path());

    let outcome = weaver(dir.path(), &["init"]).unwrap();

    assert!(
        outcome.stdout.contains("already initialized"),
        "{}",
        outcome.stdout
    );
    assert!(!made_new(dir.path()));
}

#[test]
fn a_workspace_a_mindflayer_build_s_activate_set_is_taken_and_said() {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    weaver(dir.path(), &["init", "hub"]).unwrap();
    let elsewhere = TempDir::new().unwrap();
    let spelled = hub.to_string_lossy().into_owned();

    let which = spawn(
        "mindweaver",
        elsewhere.path(),
        &["which"],
        &[("MINDFLAYER_WORKSPACE", &spelled)],
    );

    assert!(which.ok, "{}", which.stderr);
    assert_eq!(
        which.stdout,
        format!(
            "hub at {}, activated by $MINDFLAYER_WORKSPACE\n",
            hub.display()
        )
    );
    assert!(
        which.stderr.starts_with(
            "note: $MINDFLAYER_WORKSPACE is Mindflayer's name for $MINDWEAVER_WORKSPACE"
        ),
        "{}",
        which.stderr
    );
    // The new name, set, is the one read, and nothing is said.
    let both = spawn(
        "mindweaver",
        elsewhere.path(),
        &["which"],
        &[
            ("MINDFLAYER_WORKSPACE", "/nowhere"),
            ("MINDWEAVER_WORKSPACE", &spelled),
        ],
    );
    assert!(both.ok, "{}", both.stderr);
    assert!(!both.stderr.contains("note:"), "{}", both.stderr);
}

#[test]
fn a_home_a_mindflayer_build_read_from_its_variable_is_taken_and_said() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    MindflayerMother::create(home.path());
    let spelled = home.path().to_string_lossy().into_owned();

    let which = spawn(
        "mindweaver",
        elsewhere.path(),
        &["which"],
        &[("MINDFLAYER_HOME", &spelled)],
    );

    assert!(which.ok, "{}", which.stderr);
    assert!(which.stdout.contains("the default one"), "{}", which.stdout);
    assert!(
        which.stderr.contains(
            "note: $MINDFLAYER_HOME is Mindflayer's name for $MINDWEAVER_HOME, which is not set, \
             so it is taken; set MINDWEAVER_HOME in its place"
        ),
        "{}",
        which.stderr
    );
    assert!(!made_new(home.path()));
}

/// A workspace a Mindflayer build made, linking `app`, a local project in a
/// git work tree for Claude, into which it installed a rule about some
/// files (a file for Claude) and one about every file (a section of the
/// block of `AGENTS.md`): what a move to the new names reaches.
fn installed() -> TempDir {
    installed_with(&["rust"])
}

/// [`installed`], with a rule about some files for each of `scoped`.
fn installed_with(scoped: &[&str]) -> TempDir {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    let workspace = MindflayerMother::create(dir.path());
    let app = dir.path().join("app");
    std::fs::create_dir_all(&app).unwrap();
    mind(&app, &["init", "--agents", "claude,codex"]).unwrap();
    weaver(dir.path(), &["link", "app"]).unwrap();
    let rules = own_directory_for(dir.path(), Kind::Rule);
    std::fs::create_dir_all(&rules).unwrap();
    for route in scoped {
        std::fs::write(
            rules.join(format!("{route}.md")),
            "---\npaths:\n  - \"src/**\"\n---\nReturn errors.\n",
        )
        .unwrap();
        weaver(
            dir.path(),
            &["install", "-p", "app", &format!("rule:{route}")],
        )
        .unwrap();
    }
    std::fs::write(rules.join("team.md"), "Say what a change does first.\n").unwrap();
    weaver(dir.path(), &["install", "-p", "app", "rule:team"]).unwrap();
    let project = mindweaver_core::MindProject::open(&app).unwrap();
    MindflayerMother::installed_into(&workspace, &project);
    dir
}

#[test]
fn migrate_names_says_what_it_would_move_and_moves_nothing() {
    let dir = installed();
    let root = dir.path();

    let outcome = weaver(root, &["migrate", "--names"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!(
            "would move {} to {}\n\
             would rename {} to {}\n\
             would rename {} to {}\n\
             would point skills in the marker at .mindweaver/library/skills\n\
             would point rules in the marker at .mindweaver/library/rules\n\
             would spell the markers of the block in {} as Mindweaver's\n\
             would move {} to {}\n\
             would spell the markers of the block in {} as Mindweaver's\n\
             nothing moved: add --yes to go ahead\n",
            at(root, ".mindflayer"),
            at(root, ".mindweaver"),
            at(root, ".mindflayer/flayer.toml"),
            at(root, ".mindweaver/weaver.toml"),
            at(root, ".mindflayer/mindflayer.db"),
            at(root, ".mindweaver/mindweaver.db"),
            at(root, "app/AGENTS.md"),
            at(root, "app/.claude/rules/mindflayer/rust.md"),
            at(root, "app/.claude/rules/mindweaver/rust.md"),
            at(root, ".git/info/exclude"),
        )
    );
    assert!(outcome.ok);
    assert!(root.join(".mindflayer/flayer.toml").is_file());
    assert!(!made_new(root));
}

#[test]
fn migrate_names_with_yes_moves_it_all_and_a_second_run_has_nothing_to_move() {
    let dir = installed();
    let root = dir.path();

    let moved = weaver(root, &["migrate", "--names", "--yes"]).unwrap();

    assert!(
        moved.stdout.starts_with(&format!(
            "moved {0}/.mindflayer to {0}/.mindweaver\n",
            root.display()
        )),
        "{}",
        moved.stdout
    );
    assert!(moved.ok, "{:?}", moved.stderr);
    assert!(!root.join(".mindflayer").exists());
    assert!(root.join(".mindweaver/weaver.toml").is_file());
    assert!(root.join(".mindweaver/mindweaver.db").is_file());
    assert!(root.join("app/.claude/rules/mindweaver/rust.md").is_file());
    let agents = std::fs::read_to_string(root.join("app/AGENTS.md")).unwrap();
    assert!(agents.contains("<!-- mindweaver:rule team -->"), "{agents}");
    // Each rule as it was installed, wherever its copies went; the one
    // about some files reaches Claude alone, as it did before.
    let status = weaver(root, &["status"]).unwrap();
    for word in ["missing", "edited", "foreign"] {
        assert!(!status.stdout.contains(word), "{}", status.stdout);
    }
    let again = weaver(root, &["migrate", "--names", "--yes"]).unwrap();
    assert_eq!(
        again.stdout,
        "nothing to move: the workspace is under Mindweaver's names already\n"
    );
}

#[test]
fn a_rule_file_not_there_is_put_back_under_the_new_name_once_the_workspace_moved() {
    let dir = installed();
    let root = dir.path();
    std::fs::remove_file(root.join("app/.claude/rules/mindflayer/rust.md")).unwrap();

    let planned = weaver(root, &["migrate", "--names"]).unwrap();
    assert!(
        planned.stdout.contains(&format!(
            "would point the record of rule `rust` at {}: \
             its file is not there, so nothing moves, and an update puts it back there\n",
            at(root, "app/.claude/rules/mindweaver/rust.md")
        )),
        "{}",
        planned.stdout
    );
    weaver(root, &["migrate", "--names", "--yes"]).unwrap();
    let updated = weaver(root, &["update", "-p", "app", "--yes"]).unwrap();

    assert!(
        updated
            .stdout
            .contains("put back rule rust in app at .claude/rules/mindweaver/rust.md, from"),
        "{}",
        updated.stdout
    );
    assert!(root.join("app/.claude/rules/mindweaver/rust.md").is_file());
    assert!(!root.join("app/.claude/rules/mindflayer").exists());
    let exclude = std::fs::read_to_string(root.join(".git/info/exclude")).unwrap();
    assert!(!exclude.contains("mindflayer"), "{exclude}");
}

#[test]
fn migrate_names_leaves_a_block_git_tracks_in_a_local_project_and_says_so() {
    let dir = installed();
    let root = dir.path();
    indexed(root);
    let agents = std::fs::read_to_string(root.join("app/AGENTS.md")).unwrap();

    let moved = weaver(root, &["migrate", "--names", "--yes"]).unwrap();

    assert!(
        moved.stdout.contains(&format!(
            "left the markers of the block in {}/app/AGENTS.md as Mindflayer's: git tracks it, \
             and the project is local without edit_tracked\n",
            root.display()
        )),
        "{}",
        moved.stdout
    );
    assert_eq!(
        std::fs::read_to_string(root.join("app/AGENTS.md")).unwrap(),
        agents
    );
}

#[test]
fn migrate_names_names_each_tracked_rule_file_it_moves_and_none_it_leaves() {
    let dir = installed_with(&["go", "rust"]);
    let root = dir.path();
    indexed(root);
    let left = root.join("app/.claude/rules/mindflayer/go.md");
    std::fs::write(&left, "Mine now.\n").unwrap();

    let planned = weaver(root, &["migrate", "--names"]).unwrap();

    let hints: Vec<&str> = planned
        .stderr
        .iter()
        .map(|line| line.text.as_str())
        .filter(|line| line.contains("git rm"))
        .collect();
    assert_eq!(
        hints,
        [format!(
            "hint: git tracks what moved from {}; `git rm --cached \
             -- .claude/rules/mindflayer/rust.md` and `git add -- \
             .claude/rules/mindweaver/rust.md` in {} take the move into the index",
            at(root, "app/.claude/rules/mindflayer"),
            at(root, "app")
        )]
    );
    assert!(
        planned
            .stdout
            .contains(&format!("would leave {} where it is", left.display())),
        "{}",
        planned.stdout
    );
}

#[test]
fn a_second_migrate_names_says_what_it_leaves_and_moves_it_once_it_is_as_written() {
    let dir = installed();
    let root = dir.path();
    let old = root.join("app/.claude/rules/mindflayer/rust.md");
    let written = std::fs::read(&old).unwrap();
    std::fs::write(&old, "Mine now.\n").unwrap();
    weaver(root, &["migrate", "--names", "--yes"]).unwrap();

    let again = weaver(root, &["migrate", "--names"]).unwrap();

    assert_eq!(
        again.stdout,
        format!(
            "would leave {} where it is: changed since Mindweaver wrote it, so it is somebody's\n\
             nothing to move: the workspace is under Mindweaver's names already\n",
            old.display()
        )
    );
    std::fs::write(&old, &written).unwrap();
    let moved = weaver(root, &["migrate", "--names", "--yes"]).unwrap();
    assert_eq!(
        moved.stdout,
        format!(
            "moved {} to {root}/app/.claude/rules/mindweaver/rust.md\n",
            old.display(),
            root = root.display()
        )
    );
    assert!(!root.join("app/.claude/rules/mindflayer").exists());
}

#[test]
fn mind_migrate_names_moves_the_workspace_the_project_uses() {
    let dir = installed();
    let root = dir.path();

    let moved = mind(&root.join("app"), &["migrate", "--names", "--yes"]).unwrap();

    assert!(moved.ok, "{:?}", moved.stderr);
    assert!(root.join(".mindweaver/weaver.toml").is_file());
    assert!(!root.join(".mindflayer").exists());
}

#[test]
fn migrate_names_refuses_a_name_taken_and_moves_nothing() {
    let dir = installed();
    let root = dir.path();
    std::fs::create_dir(root.join(".mindweaver")).unwrap();

    let refused = weaver(root, &["migrate", "--names", "--yes"]).unwrap_err();

    assert_eq!(
        refused.error.to_string(),
        format!(
            "nothing moved: {}: already there, so {} cannot be moved onto it",
            at(root, ".mindweaver"),
            at(root, ".mindflayer")
        )
    );
    assert!(root.join(".mindflayer/flayer.toml").is_file());
}

/// `route` under `root`, as a command says the path: with the platform's
/// separator throughout.
fn at(root: &Path, route: &str) -> String {
    mindweaver_core::paths::joined(root, route)
        .display()
        .to_string()
}

/// What a line run on a workspace a Mindflayer build made says last.
fn hint(root: &Path) -> String {
    format!(
        "hint: the workspace at {} was made by a Mindflayer build, and is kept under its \
         names; `mindweaver migrate --names` moves it, and what it wrote in its projects, to \
         Mindweaver's",
        root.display()
    )
}

#[test]
fn every_line_on_a_mindflayer_workspace_says_once_how_to_move_it() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(dir.path());

    let listed = weaver(dir.path(), &["list"]).unwrap();
    let added = weaver(dir.path(), &["add", "skill", "review", "Read it first"]).unwrap();

    assert_eq!(listed.stderr, [hint(dir.path())]);
    assert!(listed.ok);
    assert_eq!(added.stderr, [hint(dir.path())]);
    // Not the move itself, which says what it moves, nor a line once it
    // has moved.
    let planned = weaver(dir.path(), &["migrate", "--names"]).unwrap();
    assert!(planned.stderr.is_empty(), "{:?}", planned.stderr);
    weaver(dir.path(), &["migrate", "--names", "--yes"]).unwrap();
    assert!(weaver(dir.path(), &["list"]).unwrap().stderr.is_empty());
}

#[test]
fn an_old_workspace_left_beside_a_new_one_is_said_and_the_new_one_read() {
    let dir = TempDir::new().unwrap();
    MindflayerMother::create(&dir.path().join("old"));
    weaver(dir.path(), &["init", "here"]).unwrap();
    let here = dir.path().join("here");
    std::fs::rename(dir.path().join("old/.mindflayer"), here.join(".mindflayer")).unwrap();

    let listed = weaver(&here, &["list"]).unwrap();

    assert_eq!(
        listed.stderr,
        [format!(
            "note: {}: left over beside the {} the workspace is read from, which an older build \
             reads instead; remove it, or move it aside, once nothing needs it",
            here.join(".mindflayer").display(),
            here.join(".mindweaver").display()
        )]
    );
    assert!(listed.stdout.starts_with("here "), "{}", listed.stdout);
}
