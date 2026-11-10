//! Looking around the screen: what `mindweaver` and `mind` open on, moving
//! over the projects, and the keys and what each runs.

use crate::{collapse_and_tanukeys, Key};

#[test]
fn mindweaver_opens_on_the_workspaces_own_then_each_project_and_what_the_first_holds() {
    let scenario = collapse_and_tanukeys();

    let mut terminal = scenario.open("~/Projects", "mindweaver");

    terminal.draws(
        0,
        &[
            " mindweaver  Projects  · found above · nothing on the shelf  · /home/you/Projects",
            "┏ Projects (2) ━━━━━━━━━━━━━━━━━━━━┓┌ collapse ────────────────────────────────────────────────────────────────────────┐",
            "┃  ◆ Projects  1                   ┃│>   skill commit-style  current  CC CX OC  How every repo here writes commits     │",
            "┃> collapse  2                     ┃│    skill deploy        authored           Ship the service to staging            │",
            "┃  tanukeys  0                     ┃│                                                                                  │",
            "┃                                  ┃│                                                                                  │",
        ],
    );
    terminal.draws(
        21,
        &[
            "┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛└──────────────────────────────────────────────────────────────────────────────────┘",
            "q quit  ? keys  → open  a add  u update  m minds  l link  U unlink  v validate  g gather  L load  s shelf  i install  t",
            "every change is a command, printed when you quit; ? lists them",
        ],
    );
}

#[test]
fn moving_down_shows_what_the_next_project_holds_and_where_it_keeps_it() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal.presses(Key::Down).draws(
        1,
        &[
            "┏ Projects (2) ━━━━━━━━━━━━━━━━━━━━┓┌ tanukeys ────────────────────────────────────────────────────────────────────────┐",
            "┃  ◆ Projects  1                   ┃│nothing here yet                                                                  │",
            "┃  collapse  2                     ┃│                                                                                  │",
            "┃> tanukeys  0                     ┃│skills go in skills/                                                              │",
            "┃                                  ┃│rules go in rules/                                                                │",
            "┃                                  ┃│                                                                                  │",
            "┃                                  ┃│a adds a skill or a rule                                                          │",
        ],
    );
}

#[test]
fn the_keys_say_what_each_runs_as_a_command_line() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal.types("?").draws(
        1,
        &[
            "┏ keys ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓",
            "┃Every key here runs a command you could type instead.                                                                 ┃",
            "┃                                                                                                                      ┃",
            "┃  a      add a skill or a rule                                                                                        ┃",
            "┃         mind -C <project> add <kind> <name> <description>                                                            ┃",
            "┃  enter  show an artifact                                                                                             ┃",
            "┃         mind -C <project> show <kind:name>                                                                           ┃",
            "┃  e      edit it, in $VISUAL or $EDITOR                                                                               ┃",
            "┃         mind -C <project> edit <kind:name>                                                                           ┃",
            "┃  r      rename it                                                                                                    ┃",
            "┃         mind -C <project> rename <kind:name> <new-name>                                                              ┃",
        ],
    );
}

#[test]
fn validate_shows_what_mindweaver_validate_says_of_everything() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .types("v")
        .shows("3 skills checked, 0 invalid")
        .shows("commit-style (workspace): ok")
        .shows("commit-style (collapse): ok")
        .shows("deploy (collapse): ok");
}

#[test]
fn mind_opens_on_the_project_it_runs_in_with_its_artifacts_under_the_cursor() {
    let scenario = collapse_and_tanukeys();

    let mut terminal = scenario.open("~/Projects/collapse", "mind");

    terminal.shows("> collapse  2").draws(
        22,
        &["q quit  ? keys  enter show  e edit  r rename  d delete  u update  A adopt  a add  ← back  v validate"],
    );
}

#[test]
fn enter_on_an_artifact_shows_it_as_mind_show_does_and_runs_nothing_to_repeat() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .presses(Key::Right)
        .presses(Key::Down)
        .presses(Key::Enter)
        .shows(" mind -C collapse show skill:deploy ")
        .shows("Ship the service to staging");
    terminal
        .types("q")
        .shows(">   skill deploy        authored           Ship the service to staging");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    // A read changes nothing, so nothing of it is handed back.
    assert_eq!(closed.shown.trim_end(), "");
}

#[test]
fn the_keys_sheet_goes_to_its_last_line_and_back_to_its_first() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");
    let first = "Every key here runs a command you could type instead.";

    terminal.types("?").shows(first);
    // Its last line wraps at this width: its end is on a row of its own.
    terminal
        .types("G")
        .no_longer_shows(first)
        .shows("claude, CX codex, OC opencode.");
    terminal.types("g").shows(first);
    terminal.types(" ").no_longer_shows(first);
    terminal.types("q").shows("collapse  2");
}

#[test]
fn capital_r_reads_the_disk_again_after_a_shell_beside_the_screen_changed_it() {
    let scenario = collapse_and_tanukeys();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    // Drawn first: a key typed before the screen has the terminal is the
    // shell's, echoed where the transcript is printed.
    terminal.shows("collapse  2");
    terminal.presses(Key::Down).shows("> tanukeys  0");

    scenario.given(
        "~/Projects/tanukeys",
        "mind add skill review \"Read a change\"",
    );
    terminal
        .types("R")
        .shows("> tanukeys  1")
        .shows(">   skill review  authored           Read a change");

    terminal.types("q");
    let closed = terminal.exits();
    assert!(closed.status.success());
    assert_eq!(closed.shown.trim_end(), "");
}

#[test]
fn s_shows_the_shelf_as_gather_list_says_it_each_source_by_its_name() {
    let scenario = collapse_and_tanukeys();
    scenario
        .source(
            "~/src/acme-skills",
            &[(
                "skills/lint/SKILL.md",
                &crate::skill("lint", "Lint the code"),
            )],
        )
        .given("~/Projects", "mindweaver gather git ../src/acme-skills");
    let listed = scenario.run("~/Projects", "mindweaver gather list");
    // Its source by its name, which `--from` takes.
    assert_eq!(
        listed.stdout,
        "lint  src-acme-skills  unknown  Lint the code\n"
    );
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("1 on the shelf");

    terminal.types("s").shows(" mindweaver gather list ");
    for line in listed.stdout.lines() {
        terminal.shows(line);
    }

    terminal.types("q").types("q");
    let closed = terminal.exits();
    assert_eq!(closed.shown.trim_end(), "");
}

#[test]
fn w_lists_the_workspaces_this_machine_knows_with_the_one_on_screen_marked() {
    let scenario = collapse_and_tanukeys();
    scenario
        .given("~", "mindweaver init")
        .given("~/Projects", "mindweaver init")
        .folder("~/weavers/writing")
        .given("~/weavers/writing", "mindweaver init");
    let known = scenario.run("~/Projects", "mindweaver workspaces");
    assert_eq!(known.stdout.lines().count(), 3, "{}", known.stdout);
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal.types("w").shows(" mindweaver workspaces ");
    for line in known.stdout.lines() {
        terminal.shows(line);
    }
    terminal.shows("* Projects");

    terminal.types("q").types("q");
    let closed = terminal.exits();
    assert_eq!(closed.shown.trim_end(), "");
}
