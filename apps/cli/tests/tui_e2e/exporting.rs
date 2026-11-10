//! Keeping a copy of the library elsewhere, from the screen: exported into
//! a folder, and backed up to a git repository, each from a form of its
//! own, as `mindweaver export` and `mindweaver backup` do it.

use crate::{collapse_and_tanukeys, Key, Scenario};

/// `~/Projects` as [`collapse_and_tanukeys`] has it, its library holding
/// `commit-style` alone.
fn library() -> Scenario {
    collapse_and_tanukeys()
}

#[test]
fn x_exports_the_library_into_a_folder_laid_out_as_a_workspace() {
    let scenario = library();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse");

    terminal
        .types("x")
        .shows("Export the library into a folder")
        .types("../backup")
        .shows("mindweaver export ../backup")
        .presses(Key::Enter)
        .shows("added  skill  commit-style");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver export ../backup\n",
            "/home/you/backup, a workspace named Projects made there\n",
            "  added  skill  commit-style\n",
            "\n",
            "1 artifact: 1 added, 0 updated, 0 removed, 0 unchanged",
        )
    );
    assert_eq!(
        scenario.files("~/backup"),
        [
            ".mindweaver/library/skills/commit-style/SKILL.md",
            ".mindweaver/weaver.toml",
        ]
    );
}

#[test]
fn b_backs_the_library_up_to_a_repository_its_steps_drawn_as_it_goes() {
    let scenario = library();
    scenario.remote("~/backups/library.git");
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse");

    terminal
        .types("b")
        .shows("Back the library up to a git repository")
        .types("../backups/library.git")
        .shows("mindweaver backup ../backups/library.git")
        .presses(Key::Enter)
        .shows("pushed");

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    let head = scenario.branch_head("~/backups/library.git", "main");
    assert_eq!(
        closed.shown.trim_end(),
        format!(
            concat!(
                "$ mindweaver backup ../backups/library.git\n",
                "/home/you/backups/library.git, branch main\n",
                "  added  skill  commit-style\n",
                "\n",
                "1 artifact: 1 added, 0 updated, 0 removed, 0 unchanged\n",
                "pushed {head} to main",
            ),
            head = head
        )
    );
    assert_eq!(
        scenario.files_on("~/backups/library.git", "main"),
        [
            ".mindweaver/library/skills/commit-style/SKILL.md",
            ".mindweaver/weaver.toml",
        ]
    );
    assert!(scenario.files("~/Projects/.mindweaver/tmp").is_empty());
}
