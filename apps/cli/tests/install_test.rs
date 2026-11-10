//! The install screen and the rule it works by.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// install
//
// The screen is driven the way a person drives it (a sequence of key presses)
// and drawn into an in-memory terminal, so the two things a user actually
// meets, what a key does and what they see, are both under test. Only the
// event loop that reads a real keyboard is not, and it holds nothing else.
// ---------------------------------------------------------------------------

#[test]
fn the_screen_opens_on_the_projects_with_the_shelf_beside_them() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let screen = screen_for(&workspace, &ledger);

    assert_eq!(screen.focus, Focus::Projects);
    assert_eq!(screen.targets.len(), 2);
    let drawn = rendered(&screen);
    assert!(drawn.contains("Projects"), "{drawn}");
    assert!(drawn.contains("collapse"), "{drawn}");
    assert!(drawn.contains("tanukeys"), "{drawn}");
    assert!(drawn.contains("Skills in collapse"), "{drawn}");
    assert!(drawn.contains("[ ] deploy"), "{drawn}");
}

#[test]
fn moving_down_the_projects_changes_which_skills_are_shown() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Down]);

    assert_eq!(screen.current().unwrap().name(), "tanukeys");
    assert!(rendered(&screen).contains("Skills in tanukeys"));
}

#[test]
fn right_enters_the_skills_and_left_comes_back() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Right]);
    assert_eq!(screen.focus, Focus::Offers);
    // The footer says what the keys do here, and it is not what it said there.
    assert!(rendered(&screen).contains("space mark"));

    press(&mut screen, &[KeyCode::Left]);
    assert_eq!(screen.focus, Focus::Projects);
    press(&mut screen, &[KeyCode::Enter]);
    assert_eq!(screen.focus, Focus::Offers, "enter does what right does");
    press(&mut screen, &[KeyCode::Esc]);
    assert_eq!(screen.focus, Focus::Projects, "esc does what left does");
}

#[test]
fn h_j_k_and_l_move_as_the_arrows_do_either_way_round() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy", "lint"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Char('j')]);
    assert_eq!(screen.current().unwrap().name(), "tanukeys");
    press(&mut screen, &[KeyCode::Char('k'), KeyCode::Char('l')]);
    assert_eq!(screen.current().unwrap().name(), "collapse");
    assert_eq!(screen.focus, Focus::Offers);
    press(&mut screen, &[KeyCode::Char('j'), KeyCode::Char(' ')]);
    let drawn = rendered(&screen);
    assert!(drawn.contains("[ ] deploy"), "{drawn}");
    assert!(drawn.contains("[x] lint"), "{drawn}");
    press(&mut screen, &[KeyCode::Char('k'), KeyCode::Char(' ')]);
    assert_eq!(screen.counts(), (2, 0));
    press(&mut screen, &[KeyCode::Char('h')]);
    assert_eq!(screen.focus, Focus::Projects);

    // Turned round, by artifact: the same keys over its two columns.
    press(&mut screen, &[KeyCode::Char('v')]);
    assert_eq!(screen.focus, Focus::Artifacts);
    press(&mut screen, &[KeyCode::Char('j')]);
    assert_eq!(screen.chosen().unwrap().candidate.name(), "lint");
    press(&mut screen, &[KeyCode::Char('l')]);
    assert_eq!(screen.focus, Focus::Receivers);
    press(&mut screen, &[KeyCode::Char('j')]);
    assert_eq!(screen.receiver, 1);
    press(&mut screen, &[KeyCode::Char('k'), KeyCode::Char('h')]);
    assert_eq!(screen.receiver, 0);
    assert_eq!(screen.focus, Focus::Artifacts);
    press(&mut screen, &[KeyCode::Char('k')]);
    assert_eq!(screen.chosen().unwrap().candidate.name(), "deploy");
}

#[test]
fn space_marks_a_skill_and_the_screen_says_what_is_waiting() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (1, 0));
    let drawn = rendered(&screen);
    assert!(drawn.contains("[x] deploy"), "{drawn}");
    assert!(drawn.contains("+ install"), "{drawn}");
    assert!(drawn.contains("1 to install, 0 to remove"), "{drawn}");
}

#[test]
fn each_project_keeps_its_own_marks() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    // Mark it for the first project only.
    press(
        &mut screen,
        &[KeyCode::Right, KeyCode::Char(' '), KeyCode::Left],
    );
    press(&mut screen, &[KeyCode::Down]);

    let drawn = rendered(&screen);
    assert!(drawn.contains("Skills in tanukeys"), "{drawn}");
    assert!(
        drawn.contains("[ ] deploy"),
        "the mark belongs to the other project:\n{drawn}"
    );
    assert_eq!(screen.counts(), (1, 0), "and it is still waiting");
    // The project column shows where the marks are.
    assert!(drawn.contains("collapse"), "{drawn}");
}

#[test]
fn applying_asks_first_because_it_deletes_things() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    let step = press(&mut screen, &[KeyCode::Char('a')]);

    assert_eq!(step, Step::Stay, "not yet");
    assert!(screen.confirming);
    let drawn = rendered(&screen);
    assert!(drawn.contains("Install 1, remove 0?"), "{drawn}");

    assert_eq!(press(&mut screen, &[KeyCode::Char('n')]), Step::Stay);
    assert!(!screen.confirming, "answering no puts it away");
    assert_eq!(
        press(&mut screen, &[KeyCode::Char('a'), KeyCode::Char('y')]),
        Step::Apply
    );
}

#[test]
fn applying_nothing_says_so_rather_than_asking() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    let step = press(&mut screen, &[KeyCode::Char('a')]);

    assert_eq!(step, Step::Stay);
    assert!(!screen.confirming);
    assert!(rendered(&screen).contains("nothing marked"));
}

#[test]
fn q_leaves_without_doing_anything() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    assert_eq!(press(&mut screen, &[KeyCode::Char('q')]), Step::Quit);
}

#[test]
fn applying_installs_what_was_marked_and_reports_it() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    let origin = screen.targets[0].rows[0].candidate.origin();
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    // The line it ran, and what that said, as the TUI's transcript says it.
    assert_eq!(
        outcome.stdout,
        format!(
            "$ mindweaver install -p collapse --from {origin} skill:deploy\n\
             installed deploy into collapse, from {origin}\n"
        )
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    for copy in copies(&dir.path().join("collapse"), "deploy") {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
}

#[test]
fn an_installed_skill_opens_already_marked_next_time() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    install_screen::apply(&workspace, &screen, &Lookup::default());

    let reopened = screen_for(&workspace, &ledger);

    assert_eq!(
        reopened.targets[0].rows[0].candidate.standing,
        Standing::Current
    );
    assert!(rendered(&reopened).contains("[x] deploy"));
    assert_eq!(
        reopened.counts(),
        (0, 0),
        "nothing waiting until you touch it"
    );
}

#[test]
fn unmarking_an_installed_skill_removes_it() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);

    let mut again = screen_for(&workspace, &ledger);
    press(&mut again, &[KeyCode::Right, KeyCode::Char(' ')]);
    assert_eq!(again.counts(), (0, 1));
    assert!(rendered(&again).contains("- remove"));
    let outcome = install_screen::apply(&workspace, &again, &Lookup::default());

    assert_eq!(
        outcome.stdout,
        "$ mindweaver uninstall -p collapse skill:deploy\nremoved deploy from collapse\n"
    );
    for copy in copies(&dir.path().join("collapse"), "deploy") {
        assert!(!copy.exists(), "{}", copy.display());
    }
}

#[test]
fn an_installed_skill_whose_copies_are_gone_opens_unmarked_and_marking_it_puts_them_back() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);
    // What a `git clean -dX` does to a local project's copies; it takes the
    // project's `.mind` too, which this leaves.
    let copies = copies(&dir.path().join("collapse"), "deploy");
    for copy in &copies {
        fs::remove_dir_all(copy).unwrap();
    }

    let mut screen = screen_for(&workspace, &ledger);
    assert_eq!(
        screen.targets[0].rows[0].candidate.standing,
        Standing::Missing
    );
    let drawn = rendered(&screen);
    assert!(drawn.contains("[ ] deploy"), "{drawn}");
    assert!(
        drawn.contains("a copy gone since it was installed"),
        "{drawn}"
    );

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    assert_eq!(screen.counts(), (1, 0));
    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    assert!(outcome.ok, "{}", outcome.stdout);
    assert!(
        outcome
            .stdout
            .contains("\ninstalled deploy into collapse, from "),
        "{}",
        outcome.stdout
    );
    for copy in &copies {
        assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
    }
    assert!(rendered(&screen_for(&workspace, &ledger)).contains("[x] deploy"));
}

#[test]
fn a_skill_written_by_hand_is_shown_but_not_touched() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    // Same name as a shelf entry, but nobody here put it there: in a folder
    // an install of it would write.
    let written = copies(&dir.path().join("collapse"), "deploy").remove(0);
    fs::create_dir_all(&written).unwrap();
    fs::write(
        written.join("SKILL.md"),
        skill_file("deploy", "Written by a person"),
    )
    .unwrap();

    let mut screen = screen_for(&workspace, &ledger);
    let drawn = rendered(&screen);
    assert!(drawn.contains("[x] deploy"), "shown as present:\n{drawn}");
    assert!(drawn.contains("not installed by mindweaver"), "{drawn}");

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (0, 0), "the box did not move");
    let after = rendered(&screen);
    assert!(after.contains("left alone"), "and it says why:\n{after}");
    let kept = fs::read_to_string(written.join("SKILL.md")).unwrap();
    assert!(kept.contains("Written by a person"));
}

#[test]
fn a_skill_offered_changed_since_it_was_installed_is_marked_outdated_and_stays_ticked() {
    let dir = workspace_with_own();
    weaver(dir.path(), &["install", "-p", "alpha", "commit-style"]).unwrap();
    let manifest = own_directory_for(dir.path(), Kind::Skill).join("commit-style/SKILL.md");
    let since = fs::read_to_string(&manifest).unwrap() + "Since.\n";
    fs::write(&manifest, since).unwrap();
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();

    let screen = screen_for(&workspace, &ledger);

    let drawn = rendered(&screen);
    assert!(
        drawn.contains("[x] commit-style  (outdated; mindweaver update refreshes it)"),
        "{drawn}"
    );
    assert_eq!(screen.counts(), (0, 0));
}

#[test]
fn the_screen_opens_with_the_place_an_installation_came_from_ticked_though_its_copy_is_behind_both()
{
    let dir = workspace_with_own();
    let team = dir.path().join("team");
    fs::create_dir(&team).unwrap();
    weaver(&team, &["init"]).unwrap();
    weaver(
        &team,
        &[
            "add",
            "skill",
            "commit-style",
            "How the team writes commits",
        ],
    )
    .unwrap();
    weaver(dir.path(), &["load", "team"]).unwrap();
    weaver(
        dir.path(),
        &[
            "install",
            "-p",
            "alpha",
            "--from",
            "load:team",
            "skill:commit-style",
        ],
    )
    .unwrap();
    // The team moves on, so alpha's copy is behind where it came from and
    // is neither place's content, which the screen told it by until #78:
    // that ticked the workspace's own, listed first.
    let manifest = own_directory_for(&team, Kind::Skill).join("commit-style/SKILL.md");
    let since = fs::read_to_string(&manifest).unwrap() + "Since.\n";
    fs::write(&manifest, since).unwrap();
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();

    let mut screen = screen_for(&workspace, &ledger);

    let alpha = &screen.targets[listed(&screen, "alpha")];
    let ticked: Vec<(String, bool)> = alpha
        .rows
        .iter()
        .filter(|row| row.candidate.name() == "commit-style")
        .map(|row| (row.candidate.origin().into_string(), row.checked))
        .collect();
    assert_eq!(
        ticked,
        [
            (String::from("workspace"), false),
            (String::from("load:team"), true)
        ]
    );
    assert_eq!(screen.counts(), (0, 0));
    screen.cursor = listed(&screen, "alpha");
    let drawn = rendered(&screen);
    let lines: Vec<&str> = drawn.lines().collect();
    let at = lines
        .iter()
        .position(|line| line.contains("[x] commit-style  (outdated"))
        .unwrap_or_else(|| panic!("{drawn}"));
    assert!(lines[at + 1].contains("[load:team]"), "{drawn}");
}

#[test]
fn a_skill_changed_since_it_was_installed_is_shown_but_not_touched() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    installed(dir.path(), &workspace, &ledger);
    let manifest = copies(&dir.path().join("collapse"), "deploy")[0].join("SKILL.md");
    let edited = fs::read_to_string(&manifest).unwrap() + "Ours.\n";
    fs::write(&manifest, &edited).unwrap();

    let mut screen = screen_for(&workspace, &ledger);
    let drawn = rendered(&screen);
    assert!(drawn.contains("[x] deploy"), "shown as present:\n{drawn}");
    assert!(drawn.contains("changed since it was installed"), "{drawn}");

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (0, 0), "the box did not move");
    let after = rendered(&screen);
    assert!(after.contains("left alone"), "and it says why:\n{after}");
    assert_eq!(fs::read_to_string(&manifest).unwrap(), edited);
}

#[test]
fn one_name_from_two_shelves_cannot_be_marked_twice() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    // A second repository offering the same name.
    let other = repository(&[(
        "skills/deploy/SKILL.md",
        &skill_file("deploy", "A different deploy"),
    )]);
    weaver(
        dir.path(),
        &["gather", "git", &other.path().to_string_lossy()],
    )
    .unwrap();

    let mut screen = screen_for(&workspace, &ledger);
    assert_eq!(screen.targets[0].rows.len(), 2, "both are offered");

    // Mark the first, then the second: a project has one directory called
    // `deploy`, so the second unmarks the first rather than both being applied.
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    press(&mut screen, &[KeyCode::Down, KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (1, 0));
    assert!(rendered(&screen).contains("unmarked the one from"));
}

#[test]
fn a_mark_on_one_address_gathered_at_two_branches_installs_the_one_ticked() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["review"]);
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let url = source.path().to_string_lossy().into_owned();
    let head = fs::read_to_string(source.path().join(".git").join("HEAD")).unwrap();
    let branch = head.trim().rsplit('/').next().unwrap().to_owned();
    weaver(dir.path(), &["gather", "git", &url]).unwrap();
    weaver(dir.path(), &["gather", "git", &url, "--ref", &branch]).unwrap();
    let mut screen = screen_for(&workspace, &ledger);

    // The row offering it from the address alone, which is also the other
    // row's address without its branch.
    let target = &mut screen.targets[0];
    target.cursor = target
        .rows
        .iter()
        .position(|row| row.candidate.name() == "deploy" && row.candidate.origin() == url)
        .unwrap();
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    assert_eq!(
        outcome.stdout,
        format!(
            "$ mindweaver install -p collapse --from {url} skill:deploy\n\
             installed deploy into collapse, from {url}\n"
        )
    );
    assert!(outcome.ok, "{outcome:?}");
}

// ---------------------------------------------------------------------------
// What an interrupted install left behind
// ---------------------------------------------------------------------------

/// Leave in `project` what an install stopped halfway leaves: a stage named
/// for a process that has gone, holding half a copy, in its scratch folder.
fn interrupted(project: &Path) -> PathBuf {
    let left = project.join(".mind/tmp/4242-00000000deadbeef");
    fs::create_dir_all(left.join("new/deploy")).unwrap();
    fs::write(left.join("new/deploy/SKILL.md"), "half").unwrap();
    left
}

#[test]
fn an_install_clears_what_an_interrupted_one_left_and_says_so_once() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    let left = interrupted(&alpha);

    let outcome = mind(&alpha, &["install", "commit-style"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "installed commit-style into alpha, from workspace\n  \
         cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish\n"
    );
    assert!(
        outcome.ok && outcome.stderr.is_empty(),
        "{:?}",
        outcome.stderr
    );
    assert!(!left.exists());
    assert_eq!(
        mind(&alpha, &["install", "commit-style"]).unwrap().stdout,
        "commit-style in alpha was already current\n"
    );
}

#[test]
fn an_install_that_fails_still_says_what_it_cleared_before_the_error() {
    let dir = workspace_with_own();
    let alpha = dir.path().join("alpha");
    let left = interrupted(&alpha);
    // A file where a folder the copies go in would be: the copy is built,
    // but cannot be put in place.
    let skills = copies(&alpha, "commit-style")[0]
        .parent()
        .unwrap()
        .to_owned();
    fs::create_dir_all(skills.parent().unwrap()).unwrap();
    fs::write(&skills, "not a folder").unwrap();

    let failure = mind(&alpha, &["install", "commit-style"]).unwrap_err();

    assert_eq!(
        failure.warnings,
        ["cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish"]
    );
    assert!(
        failure.error.to_string().contains("cannot be written"),
        "{}",
        failure.error
    );
    assert!(!left.exists());
}

#[test]
fn applying_says_what_an_interrupted_install_left_in_a_project() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    interrupted(&dir.path().join("collapse"));
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    assert!(outcome.ok, "{}", outcome.stdout);
    assert!(
        outcome.stdout.ends_with(
            "\n  cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish\n"
        ),
        "{}",
        outcome.stdout
    );
}

#[test]
fn applying_says_what_an_install_that_failed_had_cleared() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let collapse = dir.path().join("collapse");
    interrupted(&collapse);
    let mut screen = screen_for(&workspace, &ledger);
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    // A file where a folder the copies go in would be, once the screen is
    // open: the copy is built, but cannot be put in place.
    let skills = copies(&collapse, "deploy")[0].parent().unwrap().to_owned();
    fs::create_dir_all(skills.parent().unwrap()).unwrap();
    fs::write(&skills, "not a folder").unwrap();

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    assert!(!outcome.ok);
    // Said before the error, being what it did, as the command says it.
    let said: Vec<&str> = outcome.stdout.lines().skip(1).collect();
    assert_eq!(
        said[0], "cleared .mind/tmp/4242-00000000deadbeef, left by an install that did not finish",
        "{}",
        outcome.stdout
    );
    assert!(
        said[1].starts_with("error: ") && said[1].contains("cannot be written"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn a_workspace_with_no_projects_says_so_instead_of_opening_a_screen() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let outcome = weaver(dir.path(), &["install"]).unwrap();

    assert!(outcome.ok);
    assert!(
        outcome.stdout.contains("manages no projects yet"),
        "{}",
        outcome.stdout
    );
    assert!(
        outcome.stdout.contains("mindweaver link"),
        "{}",
        outcome.stdout
    );
}

#[test]
fn opening_the_screen_makes_no_ledger() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    weaver(dir.path(), &["install"]).unwrap();

    assert!(!dir.path().join(WEAVER_DIR).join("mindweaver.db").exists());
}

#[test]
fn install_outside_a_workspace_says_what_to_run() {
    let dir = TempDir::new().unwrap();

    let failure = weaver(dir.path(), &["install"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::NotInWorkspace(_)));
    assert!(failure.error.to_string().contains("mindweaver init"));
}

#[test]
fn a_project_with_an_empty_shelf_is_told_where_skills_come_from() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let root = dir.path().join("collapse");
    fs::create_dir(&root).unwrap();
    mind(&root, &["init"]).unwrap();
    weaver(dir.path(), &["link", "collapse"]).unwrap();
    let workspace = WeaverWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();

    let screen = screen_for(&workspace, &ledger);

    assert!(rendered(&screen).contains("nothing on the shelf"));
}

/// A project beside the shelf's `beta`, marked by settings that name
/// `agents` and author skills nowhere, shared with the team, and linked to
/// the workspace.
fn with_settings(dir: &Path, agents: &str) {
    with_settings_sharing(dir, agents, "shared");
}

/// [`with_settings`], sharing as `share` says.
fn with_settings_sharing(dir: &Path, agents: &str, share: &str) {
    let root = dir.join("alpha");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    fs::write(
        root.join(MIND_DIR).join(SETTINGS_FILE),
        format!(
            r#"version = 5
id = "0123456789abcdef0123456789abcdef"
name = "alpha"
agents = [{agents}]
share = "{share}"

[directories]
rules = "rules"
"#
        ),
    )
    .unwrap();
    weaver(dir, &["link", "alpha"]).unwrap();
}

/// A project beside the shelf's `beta`, marked by settings that name every
/// agent and install skills in `targets`, shared, and linked.
fn installing_in(dir: &Path, targets: &str) {
    let root = dir.join("alpha");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    fs::write(
        root.join(MIND_DIR).join(SETTINGS_FILE),
        format!(
            r#"version = 5
id = "0123456789abcdef0123456789abcdef"
name = "alpha"
agents = ["claude", "codex", "opencode"]
share = "shared"

[targets]
skills = [{targets}]
"#
        ),
    )
    .unwrap();
    weaver(dir, &["link", "alpha"]).unwrap();
}

#[test]
fn an_install_says_each_target_no_install_may_use_and_installs_in_the_others() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    installing_in(dir.path(), r#"".skills", "../shared""#);
    let settings = dir.path().join("alpha").join(MIND_DIR).join(SETTINGS_FILE);
    let misplaced = format!(
        "warning: {}: ../shared is no folder to install skills in, because it is not inside the \
         project, so nothing is installed there",
        settings.display()
    );

    let done = weaver(dir.path(), &["install", "-p", "alpha", "deploy"]).unwrap();

    assert!(
        done.stdout.starts_with("installed deploy into alpha"),
        "{}",
        done.stdout
    );
    assert_eq!(done.stderr, std::slice::from_ref(&misplaced));
    assert!(!done.ok);
    assert!(dir.path().join("alpha/.skills/deploy/SKILL.md").is_file());
    assert!(!dir.path().join("shared").exists());

    // Refused for want of any other, it says why.
    installing_in_only(dir.path());
    let refused = weaver(dir.path(), &["install", "-p", "alpha", "deploy"]).unwrap_err();
    assert_eq!(
        refused.error.to_string(),
        "none of the folders alpha's settings name for its skills is one an install may use, so \
         none can be installed into it"
    );
    assert_eq!(refused.warnings, [misplaced]);
}

#[test]
fn a_route_no_skill_can_be_called_says_nothing_of_the_folders_for_skills() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    installing_in(dir.path(), r#"".skills", "../shared""#);

    let refused = weaver(dir.path(), &["install", "-p", "alpha", "git/commits"]).unwrap_err();

    // A rule's however it is typed, and none is offered.
    assert_eq!(
        refused.error.to_string(),
        "nothing called `git/commits` can be installed: gather it, load a workspace that has it \
         with `mindweaver load`, or add it to the workspace with `mindweaver add`"
    );
    assert_eq!(refused.warnings, Vec::<String>::new());
}

#[test]
fn a_bare_name_from_a_path_to_a_loaded_workspace_is_found_where_no_skill_can_go() {
    let (dir, _, _) = shelved(&["beta"], &["lint"]);
    let alpha = dir.path().join("alpha");
    fs::create_dir(&alpha).unwrap();
    gix::init(&alpha).unwrap();
    installing_in(dir.path(), r#""../shared""#);
    let team = dir.path().join("team");
    fs::create_dir(&team).unwrap();
    weaver(&team, &["init"]).unwrap();
    weaver(&team, &["add", "rule", "deploy", "Never on a Friday"]).unwrap();
    weaver(dir.path(), &["load", "team"]).unwrap();

    // A skill's refusal, that alpha has nowhere to put one, keeps the path
    // from finding the rule no more than a skill offered nowhere would.
    let done = mind(&alpha, &["install", "deploy", "--from", "../team"]).unwrap();

    assert_eq!(
        done.stdout,
        "installed rule deploy into alpha, from load:team\n"
    );
    assert!(done.ok && done.stderr.is_empty(), "{done:?}");
}

/// Change `alpha`'s settings to install skills in `../shared` alone.
fn installing_in_only(dir: &Path) {
    let path = dir.join("alpha").join(MIND_DIR).join(SETTINGS_FILE);
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace(r#"".skills", "#, "");
    fs::write(path, text).unwrap();
}

#[test]
fn the_screen_says_each_target_no_install_may_use_with_each_line_that_installs_the_kind() {
    let (dir, _, _) = shelved(&["beta"], &["deploy", "lint"]);
    installing_in(dir.path(), r#"".skills", ".mind/skills""#);
    let workspace = WeaverWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();
    let mut screen = screen_for(&workspace, &ledger);
    screen.cursor = listed(&screen, "alpha");

    press(
        &mut screen,
        &[
            KeyCode::Right,
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
        ],
    );
    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    // A line each, as `mindweaver install` says it of each install it is given.
    let said: Vec<&str> = outcome
        .stdout
        .lines()
        .filter(|line| line.contains(".mind/skills is no folder to install skills in"))
        .collect();
    assert_eq!(said.len(), 2, "{}", outcome.stdout);
    assert!(!outcome.ok);
    assert!(outcome
        .stdout
        .contains("\ninstalled deploy into alpha, from "));
    assert!(outcome
        .stdout
        .contains("\ninstalled lint into alpha, from "));
    assert!(dir.path().join("alpha/.skills/lint/SKILL.md").is_file());
}

/// Where the screen lists the project called `name`.
fn listed(screen: &Screen, name: &str) -> usize {
    screen
        .targets
        .iter()
        .position(|target| target.name() == name)
        .unwrap_or_else(|| panic!("{name} is not listed"))
}

#[test]
fn a_project_whose_settings_name_no_agent_is_said_and_hides_no_other() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), "");
    let workspace = WeaverWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();

    let mut screen = screen_for(&workspace, &ledger);

    assert_eq!(screen.targets.len(), 2);
    screen.cursor = listed(&screen, "alpha");
    let drawn = rendered(&screen);
    assert!(drawn.contains("alpha names no agent"), "{drawn}");
    assert!(!drawn.contains("nothing on the shelf"), "{drawn}");
    press(&mut screen, &[KeyCode::Right]);
    assert_eq!(screen.focus, Focus::Projects);
    assert_eq!(
        screen.message.as_deref(),
        Some(
            "alpha names no agent in its settings that skills are installed for, so none can \
             be installed into it"
        )
    );
    screen.cursor = listed(&screen, "beta");
    assert!(rendered(&screen).contains("[ ] deploy"));
    assert!(!dir.path().join("alpha/skills").exists());
}

#[test]
fn the_screen_installs_into_every_folder_a_project_marked_by_its_settings_has_agents_read() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), r#""claude", "codex""#);
    let workspace = WeaverWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();
    let mut screen = screen_for(&workspace, &ledger);
    screen.cursor = listed(&screen, "alpha");
    assert!(rendered(&screen).contains("[ ] deploy"));

    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    assert!(outcome.ok, "{}", outcome.stdout);
    assert!(
        outcome
            .stdout
            .contains("\ninstalled deploy into alpha, from "),
        "{}",
        outcome.stdout
    );
    let root = dir.path().join("alpha");
    let claude = fs::read(root.join(".claude/skills/deploy/SKILL.md")).unwrap();
    let agents = fs::read(root.join(".agents/skills/deploy/SKILL.md")).unwrap();
    assert_eq!(claude, agents);
    let mut reopened = screen_for(&workspace, &ledger);
    reopened.cursor = listed(&reopened, "alpha");
    assert!(rendered(&reopened).contains("[x] deploy"));

    // Unticked, both go.
    press(&mut reopened, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::apply(&workspace, &reopened, &Lookup::default());
    assert_eq!(
        outcome.stdout,
        "$ mindweaver uninstall -p alpha skill:deploy\nremoved deploy from alpha\n"
    );
    assert!(!root.join(".claude/skills/deploy").exists());
    assert!(!root.join(".agents/skills/deploy").exists());
}

#[test]
fn a_skill_somebody_keeps_in_one_agents_folder_is_left_alone_from_the_screen() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), r#""claude", "codex""#);
    let root = dir.path().join("alpha");
    fs::create_dir_all(root.join(".claude/skills/deploy")).unwrap();
    fs::write(
        root.join(".claude/skills/deploy/SKILL.md"),
        skill_file("deploy", "Written by a person"),
    )
    .unwrap();
    let workspace = WeaverWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();

    let mut screen = screen_for(&workspace, &ledger);
    screen.cursor = listed(&screen, "alpha");

    let drawn = rendered(&screen);
    assert!(drawn.contains("not installed by mindweaver"), "{drawn}");
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    assert_eq!(screen.counts(), (0, 0));
    assert!(!root.join(".agents/skills/deploy").exists());
}

/// Put a token into the URL of every source on the shelf, the way a Mindweaver
/// from before the refusal stored one.
fn token_in_the_shelf(workspace: &WeaverWorkspace) {
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute(
            "UPDATE sources SET url = 'https://me:ghp_t0ken@example.com/acme/skills'",
            [],
        )
        .unwrap();
}

const SCRUBBED: &str = "note: https://example.com/acme/skills: the user name, password or \
     token an older Mindweaver stored in this URL is taken out; gathering it again needs a git \
     credential helper that has it";

/// How many sources still hold a credential in the file, which a read hands
/// back without it.
fn tainted(workspace: &WeaverWorkspace) -> usize {
    workspace
        .ledger_for_reading()
        .unwrap()
        .tainted_sources()
        .unwrap()
        .len()
}

#[test]
fn opening_the_screen_scrubs_nothing_and_applying_scrubs_first_and_says_so_without_failing() {
    let (_dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    token_in_the_shelf(&workspace);
    let (projects, _) = install_screen::registered(&workspace);

    let mut screen = install_screen::open(&workspace, projects, &AgentHomes::default()).unwrap();

    assert_eq!(tainted(&workspace), 1, "opening the screen is a read");
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    // Said by the line that scrubbed it, after what it did, as the command
    // says it from the shell.
    assert!(outcome.ok, "{}", outcome.stdout);
    assert_eq!(
        outcome.stdout,
        format!(
            "$ mindweaver install -p collapse --from https://example.com/acme/skills skill:deploy\n\
             installed deploy into collapse, from https://example.com/acme/skills\n\
             {SCRUBBED}\n"
        )
    );
    assert_eq!(tainted(&workspace), 0);
}

#[test]
fn installing_and_uninstalling_one_skill_scrub_the_ledger_first() {
    let (dir, workspace, _ledger) = shelved(&["collapse"], &["deploy"]);
    token_in_the_shelf(&workspace);

    let installed = weaver(dir.path(), &["install", "-p", "collapse", "deploy"]).unwrap();
    assert!(installed.ok, "{:?}", installed.stderr);
    assert_eq!(installed.stderr, [SCRUBBED]);
    assert_eq!(
        installed.stdout,
        "installed deploy into collapse, from https://example.com/acme/skills\n"
    );

    token_in_the_shelf(&workspace);
    let removed = mind(&dir.path().join("collapse"), &["uninstall", "deploy"]).unwrap();
    assert!(removed.ok, "{:?}", removed.stderr);
    assert_eq!(removed.stderr, [SCRUBBED]);
}

#[test]
fn installing_a_skill_that_holds_a_claude_plugin_folder_leaves_it_out_and_warns() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    with_settings(dir.path(), r#""claude", "codex""#);
    let source = repository(&[
        (
            "skills/deploy/SKILL.md",
            &skill_file("deploy", "Ship the service"),
        ),
        ("skills/deploy/.claude-plugin/plugin.json", "{}"),
    ]);
    let url = source.path().to_string_lossy().into_owned();
    weaver(dir.path(), &["gather", "git", &url]).unwrap();

    let outcome = weaver(dir.path(), &["install", "-p", "alpha", "deploy"]).unwrap();

    assert_eq!(
        outcome.stdout,
        format!("installed deploy into alpha, from {url}\n")
    );
    assert_eq!(
        outcome.stderr,
        vec![
            "warning: deploy in alpha was installed without its .claude-plugin, since Claude \
             would load the skill as a plugin, with the hooks and MCP servers it declares"
        ]
    );
    assert!(!outcome.ok);
    let root = dir.path().join("alpha");
    assert!(root.join(".claude/skills/deploy/SKILL.md").is_file());
    assert!(!root.join(".claude/skills/deploy/.claude-plugin").exists());
    assert!(!root.join(".agents/skills/deploy/.claude-plugin").exists());
}

#[test]
fn a_local_project_outside_git_is_told_nothing_keeps_what_is_installed_out_of_it() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings_sharing(dir.path(), r#""claude""#, "local");
    let root = dir.path().join("alpha");

    let installed = weaver(dir.path(), &["install", "-p", "alpha", "deploy"]).unwrap();
    let removed = weaver(dir.path(), &["uninstall", "-p", "alpha", "deploy"]).unwrap();

    assert!(installed.ok, "{:?}", installed.stderr);
    assert_eq!(
        installed.stderr,
        vec![format!(
            "note: {}: not in a git work tree, so nothing keeps what is installed there out of git",
            root.display()
        )]
    );
    assert_eq!(removed.stdout, "removed deploy from alpha\n");
    assert!(removed.stderr.is_empty(), "{:?}", removed.stderr);
    assert!(!root.join(".claude/skills/deploy").exists());
}

#[test]
fn nothing_is_installed_into_a_folder_an_agent_reads_for_every_project() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), r#""claude""#);
    // Claude's folder is said to be the project's own `.claude`, as it is
    // for a project at the user's home.
    let claude = dir.path().join("alpha").join(".claude");
    let given = format!("--claude-config-dir={}", claude.display());

    let failure = weaver(dir.path(), &["install", "-p", "alpha", "deploy", &given]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        format!(
            "{} is inside {}, which Claude Code reads for every project, so nothing was \
             installed: Mindweaver installs into one project at a time",
            claude.join("skills").join("deploy").display(),
            claude.display()
        )
    );
    assert!(!claude.exists());
}

// ---------------------------------------------------------------------------
// Rules on the screen: the same column, one kind at a time
// ---------------------------------------------------------------------------

/// `shelved`, with `git/commits` a rule of the workspace's own.
fn with_a_rule(projects: &[&str]) -> (TempDir, WeaverWorkspace, Ledger) {
    let (dir, workspace, ledger) = shelved(projects, &["deploy"]);
    weaver(
        dir.path(),
        &[
            "add",
            "rule",
            "git/commits",
            "Commit one coherent step at a time",
        ],
    )
    .unwrap();
    (dir, workspace, ledger)
}

#[test]
fn tab_shows_the_rules_then_the_mcp_servers_in_the_column_the_skills_were_in_and_back() {
    let (_dir, workspace, ledger) = with_a_rule(&["collapse"]);
    let mut screen = screen_for(&workspace, &ledger);

    let skills = rendered(&screen);
    press(&mut screen, &[KeyCode::Tab]);
    let rules = rendered(&screen);
    press(&mut screen, &[KeyCode::Tab]);
    let servers = rendered(&screen);
    press(&mut screen, &[KeyCode::Tab]);

    assert!(skills.contains("Skills in collapse"), "{skills}");
    assert!(skills.contains("[ ] deploy"), "{skills}");
    assert!(!skills.contains("git/commits"), "{skills}");
    assert!(skills.contains("tab rules"), "{skills}");
    assert!(rules.contains("Rules in collapse"), "{rules}");
    assert!(rules.contains("[ ] git/commits"), "{rules}");
    assert!(
        rules.contains("Commit one coherent step at a time"),
        "{rules}"
    );
    assert!(!rules.contains("deploy"), "{rules}");
    assert!(rules.contains("tab MCP servers"), "{rules}");
    assert!(servers.contains("MCP servers in collapse"), "{servers}");
    assert!(servers.contains("tab skills"), "{servers}");
    assert_eq!(screen.kind, Kind::Skill, "round again past the last");
    press(&mut screen, &[KeyCode::BackTab]);
    assert_eq!(screen.kind, Kind::Mcp, "shift+tab goes the other way");
}

#[test]
fn a_rule_ticked_on_the_screen_is_installed_and_unticked_is_taken_out() {
    let (dir, workspace, ledger) = with_a_rule(&["collapse"]);
    let collapse = dir.path().join("collapse");
    let mut screen = screen_for(&workspace, &ledger);
    press(
        &mut screen,
        &[KeyCode::Tab, KeyCode::Right, KeyCode::Char(' ')],
    );
    assert_eq!(screen.counts(), (1, 0));

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    assert_eq!(
        outcome.stdout,
        "$ mindweaver install -p collapse --from workspace rule:git/commits\n\
         installed rule git/commits into collapse, from workspace\n"
    );
    assert!(outcome.ok && outcome.stderr.is_empty(), "{outcome:?}");
    assert_eq!(
        fs::read_to_string(collapse.join("AGENTS.md")).unwrap(),
        "<!-- mindweaver:begin -->\n<!-- mindweaver:rule git/commits -->\n\
         # Commit one coherent step at a time\n<!-- mindweaver:end -->\n"
    );

    let mut again = screen_for(&workspace, &ledger);
    press(&mut again, &[KeyCode::Tab]);
    assert!(rendered(&again).contains("[x] git/commits"));
    press(&mut again, &[KeyCode::Right, KeyCode::Char(' ')]);
    let outcome = install_screen::apply(&workspace, &again, &Lookup::default());

    assert_eq!(
        outcome.stdout,
        "$ mindweaver uninstall -p collapse rule:git/commits\n\
         removed rule git/commits from collapse\n"
    );
    assert!(!collapse.join("AGENTS.md").exists());
}

#[test]
fn a_skill_and_a_rule_of_one_name_each_keep_a_box_of_their_own() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    weaver(dir.path(), &["add", "rule", "deploy", "Never on a Friday"]).unwrap();
    let mut screen = screen_for(&workspace, &ledger);

    press(
        &mut screen,
        &[
            KeyCode::Right,
            KeyCode::Char(' '),
            KeyCode::Tab,
            KeyCode::Char(' '),
        ],
    );

    assert_eq!(screen.counts(), (2, 0));
    assert_eq!(screen.message, None, "neither displaced the other");
    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());
    let lines: Vec<&str> = outcome
        .stdout
        .lines()
        .filter(|line| line.starts_with("$ "))
        .collect();
    assert_eq!(lines.len(), 2, "{}", outcome.stdout);
    assert!(lines[0].ends_with(" skill:deploy"), "{}", outcome.stdout);
    assert_eq!(
        lines[1],
        "$ mindweaver install -p collapse --from workspace rule:deploy"
    );
    assert!(outcome
        .stdout
        .ends_with("\ninstalled rule deploy into collapse, from workspace\n"));
}

#[test]
fn a_project_offered_no_rules_is_told_where_rules_come_from() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Tab]);
    let drawn = rendered(&screen);
    press(&mut screen, &[KeyCode::Right]);

    // Wrapped where the column ends.
    assert!(
        drawn.contains("no rules to install: none of the workspace's own, and none"),
        "{drawn}"
    );
    assert!(
        drawn.contains("loaded; `mindweaver load <path>` or `mindweaver add rule` first"),
        "{drawn}"
    );
    assert_eq!(screen.focus, Focus::Projects);
    assert_eq!(
        screen.message.as_deref(),
        Some("no rules to install: none of the workspace's own, and none loaded")
    );
}

#[test]
fn a_rule_reaching_claude_alone_is_said_when_applied() {
    let (dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let rules = own_directory_for(dir.path(), Kind::Rule).join("style");
    fs::create_dir_all(&rules).unwrap();
    fs::write(
        rules.join("comments.md"),
        "---\npaths:\n  - \"**/*.rs\"\n---\n\n# Comments say why, not what\n",
    )
    .unwrap();
    let mut screen = screen_for(&workspace, &ledger);
    press(
        &mut screen,
        &[KeyCode::Tab, KeyCode::Right, KeyCode::Char(' ')],
    );

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    // Said as an install into the project alone says it, which needs no
    // `collapse: ` to say whose it is.
    assert_eq!(
        outcome.stdout,
        "$ mindweaver install -p collapse --from workspace rule:style/comments\n\
         installed rule style/comments into collapse, from workspace\n\
         warning: rule `style/comments` states `paths`, so it reaches claude only: codex and \
         opencode read no .claude/rules\n"
    );
    assert!(!outcome.ok);
}

#[test]
fn what_is_true_of_a_projects_rules_as_a_whole_is_said_by_each_line_that_renders_them() {
    let (dir, workspace, ledger) = with_a_rule(&["collapse"]);
    let collapse = dir.path().join("collapse");
    let rules = own_directory_for(dir.path(), Kind::Rule).join("review");
    fs::create_dir_all(&rules).unwrap();
    fs::write(rules.join("tone.md"), "# Say what a change does first\n").unwrap();
    fs::write(collapse.join("AGENTS.md"), "# Working here\n").unwrap();
    indexed(&collapse);
    let tracked = format!(
        "warning: {}: git tracks it, and the project is local without edit_tracked, so no \
         rules are written there, and codex and opencode read none of them",
        collapse.join("AGENTS.md").display()
    );
    let mut screen = screen_for(&workspace, &ledger);
    press(
        &mut screen,
        &[
            KeyCode::Tab,
            KeyCode::Right,
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
        ],
    );
    assert_eq!(screen.counts(), (2, 0));

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    // Each rule's line renders them all, and says so, as each command would
    // from the shell.
    assert_eq!(
        outcome.stdout,
        format!(
            "$ mindweaver install -p collapse --from workspace rule:git/commits\n\
             installed rule git/commits into collapse, from workspace\n\
             {tracked}\n\
             $ mindweaver install -p collapse --from workspace rule:review/tone\n\
             installed rule review/tone into collapse, from workspace\n\
             {tracked}\n"
        )
    );
    assert!(!outcome.ok);

    let mut again = screen_for(&workspace, &ledger);
    press(
        &mut again,
        &[KeyCode::Tab, KeyCode::Right, KeyCode::Char(' ')],
    );
    let outcome = install_screen::apply(&workspace, &again, &Lookup::default());

    // One rule is left, which codex and opencode still miss.
    assert_eq!(
        outcome.stdout,
        format!(
            "$ mindweaver uninstall -p collapse rule:git/commits\n\
             removed rule git/commits from collapse\n\
             {tracked}\n"
        )
    );
}

// ---------------------------------------------------------------------------
// The screen turned round, by artifact
//
// `v` lists every offer on the left and every project on the right, ticked
// where it holds the one under the cursor: the same boxes as the screen the
// right way round, so what is marked either way is one batch.
// ---------------------------------------------------------------------------

#[test]
fn v_turns_the_screen_round_to_every_project_for_one_artifact_and_back() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy", "review"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Char('v')]);

    assert_eq!(screen.focus, Focus::Artifacts);
    let drawn = rendered(&screen);
    assert!(drawn.contains("┏ Skills ━"), "{drawn}");
    assert!(drawn.contains("> deploy"), "{drawn}");
    assert!(drawn.contains("  review"), "{drawn}");
    assert!(drawn.contains("What review does"), "{drawn}");
    assert!(drawn.contains("Projects for deploy"), "{drawn}");
    assert!(drawn.contains("[ ] collapse"), "{drawn}");
    assert!(drawn.contains("[ ] tanukeys"), "{drawn}");
    assert!(drawn.contains("→/enter projects"), "{drawn}");
    assert!(drawn.contains("v by project"), "{drawn}");

    press(&mut screen, &[KeyCode::Down]);
    assert!(rendered(&screen).contains("Projects for review"));
    press(&mut screen, &[KeyCode::Char('v')]);
    assert_eq!(screen.focus, Focus::Projects);
    let back = rendered(&screen);
    assert!(back.contains("Skills in collapse"), "{back}");
    assert!(back.contains("v by skill"), "{back}");
}

#[test]
fn ticking_projects_for_one_artifact_marks_each_projects_box_and_applies_as_one_batch() {
    let (dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy", "review"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(
        &mut screen,
        &[
            KeyCode::Char('v'),
            KeyCode::Right,
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
        ],
    );

    assert_eq!(screen.focus, Focus::Receivers);
    assert_eq!(screen.counts(), (2, 0));
    let drawn = rendered(&screen);
    assert!(drawn.contains("[x] collapse  + install"), "{drawn}");
    assert!(drawn.contains("[x] tanukeys  + install"), "{drawn}");
    assert!(drawn.contains("deploy  (2)"), "{drawn}");
    assert!(drawn.contains("space mark"), "{drawn}");
    assert!(drawn.contains("2 to install, 0 to remove"), "{drawn}");

    // The right way round, each project holds its own mark.
    press(&mut screen, &[KeyCode::Char('v')]);
    let back = rendered(&screen);
    assert!(back.contains("collapse  (1)"), "{back}");
    assert!(back.contains("tanukeys  (1)"), "{back}");
    assert!(back.contains("[x] deploy  + install"), "{back}");

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    // One line for the artifact, with a `-p` for each project ticked.
    let origin = screen.targets[0].rows[0].candidate.origin();
    assert_eq!(
        outcome.stdout,
        format!(
            "$ mindweaver install -p collapse -p tanukeys --from {origin} skill:deploy\n\
             installed deploy into collapse, from {origin}\n\
             installed deploy into tanukeys, from {origin}\n"
        )
    );
    assert!(outcome.ok, "{}", outcome.stdout);
    for project in ["collapse", "tanukeys"] {
        for copy in copies(&dir.path().join(project), "deploy") {
            assert!(copy.join("SKILL.md").is_file(), "{}", copy.display());
        }
    }
}

#[test]
fn unticking_a_project_for_one_artifact_takes_it_out_of_that_project_alone() {
    let (dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);
    press(
        &mut screen,
        &[
            KeyCode::Char('v'),
            KeyCode::Right,
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
        ],
    );
    install_screen::apply(&workspace, &screen, &Lookup::default());

    let mut again = screen_for(&workspace, &ledger);
    press(
        &mut again,
        &[KeyCode::Char('v'), KeyCode::Right, KeyCode::Char(' ')],
    );

    let drawn = rendered(&again);
    assert!(drawn.contains("[ ] collapse  - remove"), "{drawn}");
    assert!(drawn.contains("[x] tanukeys"), "{drawn}");
    let outcome = install_screen::apply(&workspace, &again, &Lookup::default());
    assert_eq!(
        outcome.stdout,
        "$ mindweaver uninstall -p collapse skill:deploy\nremoved deploy from collapse\n"
    );
    assert!(copies(&dir.path().join("collapse"), "deploy")
        .iter()
        .all(|copy| !copy.exists()));
    assert!(copies(&dir.path().join("tanukeys"), "deploy")
        .iter()
        .all(|copy| copy.join("SKILL.md").is_file()));
}

#[test]
fn turning_the_screen_round_keeps_the_box_under_the_cursor() {
    let (_dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy", "review"]);
    let mut screen = screen_for(&workspace, &ledger);
    // tanukeys's review, the right way round.
    press(&mut screen, &[KeyCode::Down, KeyCode::Right, KeyCode::Down]);

    press(&mut screen, &[KeyCode::Char('v')]);

    assert_eq!(screen.focus, Focus::Receivers);
    assert_eq!(screen.chosen().unwrap().candidate.name(), "review");
    assert_eq!(screen.targets[screen.receiver].name(), "tanukeys");
    press(&mut screen, &[KeyCode::Char(' ')]);
    press(&mut screen, &[KeyCode::Char('v')]);
    assert_eq!(screen.focus, Focus::Offers);
    let back = rendered(&screen);
    assert!(back.contains("Skills in tanukeys"), "{back}");
    assert!(back.contains("> [x] review  + install"), "{back}");
}

#[test]
fn turned_round_somebodys_copy_is_held_and_says_why() {
    let (dir, workspace, ledger) = shelved(&["collapse", "tanukeys"], &["deploy"]);
    let written = copies(&dir.path().join("tanukeys"), "deploy").remove(0);
    fs::create_dir_all(&written).unwrap();
    fs::write(
        written.join("SKILL.md"),
        skill_file("deploy", "Written by a person"),
    )
    .unwrap();
    let mut screen = screen_for(&workspace, &ledger);

    press(
        &mut screen,
        &[
            KeyCode::Char('v'),
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Char(' '),
        ],
    );

    assert_eq!(screen.counts(), (0, 0), "the box did not move");
    let drawn = rendered(&screen);
    assert!(
        drawn.contains("[x] tanukeys  (not installed by mindweaver)"),
        "{drawn}"
    );
    assert!(drawn.contains("left alone"), "and it says why:\n{drawn}");
}

#[test]
fn turned_round_a_project_that_cannot_take_the_kind_has_no_box_and_says_why() {
    let (dir, _, _) = shelved(&["beta"], &["deploy"]);
    with_settings(dir.path(), "");
    let workspace = WeaverWorkspace::locate(dir.path()).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();
    let mut screen = screen_for(&workspace, &ledger);
    let alpha = listed(&screen, "alpha");

    press(&mut screen, &[KeyCode::Char('v'), KeyCode::Right]);
    screen.receiver = alpha;
    press(&mut screen, &[KeyCode::Char(' ')]);

    assert_eq!(screen.counts(), (0, 0));
    let drawn = rendered(&screen);
    assert!(drawn.contains("alpha  (takes no skills)"), "{drawn}");
    assert!(drawn.contains("[ ] beta"), "{drawn}");
    assert_eq!(
        screen.message.as_deref(),
        Some(
            "alpha names no agent in its settings that skills are installed for, so none can \
             be installed into it"
        )
    );
}

#[test]
fn turned_round_tab_shows_the_rules_and_each_project_for_one() {
    let (_dir, workspace, ledger) = with_a_rule(&["collapse", "tanukeys"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Char('v'), KeyCode::Tab]);

    assert_eq!(screen.kind, Kind::Rule);
    assert_eq!(screen.focus, Focus::Artifacts);
    let drawn = rendered(&screen);
    assert!(drawn.contains("┏ Rules ━"), "{drawn}");
    assert!(drawn.contains("> git/commits"), "{drawn}");
    assert!(!drawn.contains("deploy"), "{drawn}");
    assert!(drawn.contains("Projects for rule git/commits"), "{drawn}");
    assert!(drawn.contains("tab MCP servers"), "{drawn}");
    press(&mut screen, &[KeyCode::Right, KeyCode::Char(' ')]);
    assert_eq!(screen.counts(), (1, 0));
    assert!(rendered(&screen).contains("[x] collapse  + install"));
}

#[test]
fn turned_round_with_nothing_offered_says_where_it_comes_from() {
    let (_dir, workspace, ledger) = shelved(&["collapse"], &["deploy"]);
    let mut screen = screen_for(&workspace, &ledger);

    press(
        &mut screen,
        &[KeyCode::Char('v'), KeyCode::Tab, KeyCode::Right],
    );

    assert_eq!(screen.focus, Focus::Artifacts, "nothing to go into");
    assert_eq!(
        screen.message.as_deref(),
        Some("no rules to install: none of the workspace's own, and none loaded")
    );
    assert!(rendered(&screen).contains("`mindweaver add rule` first"));
}

/// A workspace linking `collapse`, its library holding the server `github`,
/// loading `team`, a workspace beside it whose library holds `linear`.
fn with_servers() -> (TempDir, WeaverWorkspace, Ledger) {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    let team = dir.path().join("team");
    for (root, name, text) in [
        (
            &hub,
            "github",
            mindweaver_core::mother::McpMother::plain_program("github"),
        ),
        (
            &team,
            "linear",
            mindweaver_core::mother::McpMother::address_for_codex("linear"),
        ),
    ] {
        fs::create_dir(root).unwrap();
        weaver(root, &["init"]).unwrap();
        let folder = own_directory_for(root, Kind::Mcp);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(format!("{name}.toml")), text).unwrap();
    }
    new_project(&hub.join("collapse"));
    weaver(&hub, &["link", "collapse"]).unwrap();
    weaver(&hub, &["load", "../team"]).unwrap();
    let workspace = WeaverWorkspace::locate(&hub).unwrap().unwrap();
    let ledger = workspace.ledger().unwrap();
    (dir, workspace, ledger)
}

/// A server from a loaded workspace ticked on the screen is shown, each
/// agent's line, before the screen applies, and only then is its line
/// given `--yes`; one of the library's own goes in as a skill does.
#[test]
fn a_server_ticked_on_the_screen_shows_what_each_agent_runs_before_its_line_says_yes() {
    let (dir, workspace, ledger) = with_servers();
    let collapse = dir.path().join("hub/collapse");
    let mut screen = screen_for(&workspace, &ledger);

    press(&mut screen, &[KeyCode::Tab, KeyCode::Tab, KeyCode::Right]);
    let servers = rendered(&screen);
    press(
        &mut screen,
        &[
            KeyCode::Char(' '),
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Char('a'),
        ],
    );
    let asked = rendered(&screen);

    assert_eq!(screen.kind, Kind::Mcp);
    assert!(servers.contains("MCP servers in collapse"), "{servers}");
    assert!(servers.contains("[ ] github"), "{servers}");
    assert!(servers.contains("[ ] linear"), "{servers}");
    assert!(screen.confirming);
    assert!(asked.contains("Install 2, remove 0?"), "{asked}");
    assert!(
        asked.contains("mcp linear into collapse, from load:../team:"),
        "{asked}"
    );
    for agent in ["claude", "codex", "opencode"] {
        assert!(
            asked.contains(&format!("  {agent} reaches: https://example.com/mcp")),
            "{asked}"
        );
    }
    // The library's own is no question: none of its lines is shown.
    assert!(!asked.contains("mcp github into"), "{asked}");
    assert_eq!(press(&mut screen, &[KeyCode::Char('y')]), Step::Apply);
    let lines: Vec<String> = screen
        .task()
        .commands()
        .iter()
        .map(|words| words.join(" "))
        .collect();
    assert_eq!(
        lines,
        [
            "mindweaver install -p collapse --from workspace mcp:github",
            "mindweaver install -p collapse --from load:../team --yes mcp:linear",
        ]
    );

    let outcome = install_screen::apply(&workspace, &screen, &Lookup::default());

    assert!(
        outcome.stdout.contains(
            "$ mindweaver install -p collapse --from load:../team --yes mcp:linear\n\
             installed mcp linear into collapse, from load:../team\n  \
             claude reaches: https://example.com/mcp\n"
        ),
        "{}",
        outcome.stdout
    );
    let held = fs::read_to_string(collapse.join(".mcp.json")).unwrap();
    assert!(held.contains("github") && held.contains("linear"), "{held}");
}

/// The install screen drawn into a terminal `width` columns wide and
/// `height` rows high, a row a line.
fn drawn_at(screen: &Screen, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(screen, frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .chunks(usize::from(width))
        .map(|line| line.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A confirmation taller than the terminal scrolls, its question and keys
/// kept in place, and its yes waits until every row of what each agent
/// would run has been on the screen: a `--yes` given to what nobody was
/// shown is no yes.
#[test]
fn a_confirmation_taller_than_the_screen_scrolls_and_waits_for_every_row_before_its_yes() {
    let (_dir, workspace, ledger) = with_servers();
    let mut screen = screen_for(&workspace, &ledger);
    press(
        &mut screen,
        &[
            KeyCode::Tab,
            KeyCode::Tab,
            KeyCode::Right,
            KeyCode::Down,
            KeyCode::Char(' '),
            KeyCode::Char('a'),
        ],
    );

    let first = drawn_at(&screen, 80, 14);

    assert!(first.contains("Install 1, remove 0?"), "{first}");
    assert!(first.contains("more rows below"), "{first}");
    assert!(first.contains("j  down"), "{first}");
    assert!(!first.contains("y  yes"), "{first}");
    assert_eq!(press(&mut screen, &[KeyCode::Char('y')]), Step::Stay);
    assert!(screen.confirming);
    let mut shown = first;
    for _ in 0..40 {
        if screen.unseen.get() == 0 {
            break;
        }
        press(&mut screen, &[KeyCode::Char('j')]);
        shown.push_str(&drawn_at(&screen, 80, 14));
    }
    assert_eq!(screen.unseen.get(), 0, "{shown}");
    for agent in ["claude", "codex", "opencode"] {
        assert!(
            shown.contains(&format!("{agent} reaches: https://example.com/mcp")),
            "{shown}"
        );
    }
    let last = drawn_at(&screen, 80, 14);
    assert!(last.contains("y  yes"), "{last}");
    assert_eq!(press(&mut screen, &[KeyCode::Char('y')]), Step::Apply);
}

#[test]
fn from_takes_a_sources_name_and_another_spelling_of_its_url_with_a_warning_that_fails_nothing() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    for name in ["collapse", "tanukeys"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        new_project(&dir.path().join(name));
        weaver(dir.path(), &["link", name]).unwrap();
    }
    let team = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let other = repository(&[(
        "skills/deploy/SKILL.md",
        &skill_file("deploy", "Ship it too"),
    )]);
    let url = team.path().to_string_lossy().into_owned();
    weaver(dir.path(), &["gather", "git", &url, "--name", "team"]).unwrap();
    weaver(
        dir.path(),
        &["gather", "git", &other.path().to_string_lossy()],
    )
    .unwrap();

    let named = weaver(
        dir.path(),
        &["install", "-p", "collapse", "deploy", "--from", "team"],
    )
    .unwrap();
    let respelled = format!("{url}/");
    let spelled = weaver(
        dir.path(),
        &["install", "-p", "tanukeys", "deploy", "--from", &respelled],
    )
    .unwrap();

    assert!(named.ok, "{:?}", named.stderr);
    assert_eq!(
        named.stdout,
        format!("installed deploy into collapse, from {url}\n")
    );
    assert!(named.stderr.is_empty(), "{:?}", named.stderr);
    assert!(spelled.ok, "{:?}", spelled.stderr);
    assert_eq!(
        spelled.stdout,
        format!("installed deploy into tanukeys, from {url}\n")
    );
    assert_eq!(
        spelled.stderr,
        [format!(
            "warning: --from {respelled} names the source `team` by another spelling of its \
             URL, which --from takes for now: name it `team`"
        )]
    );
    assert_eq!(spelled.stderr[0].code, Some("from_spelling_deprecated"));
}
