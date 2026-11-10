//! Keeping a copy of the library elsewhere: the reference's "Exporting and
//! backing up the library", into a folder or a git repository, a kind at a
//! time, bringing either up to date, and getting it back with
//! `mindweaver load`.

use crate::Scenario;

/// `~/Projects` linking `collapse`, its library holding a skill, a rule
/// and an MCP server.
fn library() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario
        .given(
            "~/Projects",
            "mindweaver add skill deploy \"Ship the service to staging\"",
        )
        .given(
            "~/Projects",
            "mindweaver add rule git/commits \"How commits are written here\"",
        )
        .given(
            "~/Projects",
            "mindweaver add mcp github \"GitHub's issues and pull requests\" --command npx --arg @modelcontextprotocol/server-github",
        );
    scenario
}

#[test]
fn export_writes_the_library_into_a_folder_laid_out_as_a_workspace() {
    let scenario = library();

    scenario
        .run("~/Projects", "mindweaver export ../backup")
        .succeeds()
        .prints(concat!(
            "/home/you/backup, a workspace named Projects made there\n",
            "  added  skill  deploy\n",
            "  added  rule   git/commits\n",
            "  added  mcp    github\n",
            "\n",
            "3 artifacts: 3 added, 0 updated, 0 removed, 0 unchanged\n",
        ))
        .quiet();
    assert_eq!(
        scenario.files("~/backup"),
        [
            ".mindweaver/library/mcp/github.toml",
            ".mindweaver/library/rules/git/commits.md",
            ".mindweaver/library/skills/deploy/SKILL.md",
            ".mindweaver/weaver.toml",
        ]
    );
    assert_eq!(
        scenario.read("~/backup/.mindweaver/library/skills/deploy/SKILL.md"),
        scenario.read("~/Projects/.mindweaver/library/skills/deploy/SKILL.md")
    );
}

#[test]
fn export_with_a_kind_writes_only_that_kind() {
    let scenario = library();

    scenario
        .run("~/Projects", "mindweaver export ../backup --kind rules")
        .succeeds()
        .prints(concat!(
            "/home/you/backup, a workspace named Projects made there\n",
            "  added  rule  git/commits\n",
            "\n",
            "1 artifact: 1 added, 0 updated, 0 removed, 0 unchanged\n",
        ))
        .quiet();
    assert!(!scenario.exists("~/backup/.mindweaver/library/skills"));
}

#[test]
fn export_says_what_it_would_take_away_and_takes_it_only_with_yes() {
    let scenario = library();
    scenario
        .given("~/Projects", "mindweaver export ../backup")
        .given("~/Projects", "mindweaver remove rule:git/commits --yes")
        .given(
            "~/Projects",
            "mindweaver add skill review \"Read a change before it lands\"",
        );

    scenario
        .run("~/Projects", "mindweaver export ../backup")
        .succeeds()
        .prints(concat!(
            "/home/you/backup\n",
            "  would add     skill  review\n",
            "  would remove  rule   git/commits\n",
            "  unchanged     skill  deploy\n",
            "  unchanged     mcp    github\n",
            "\n",
            "4 artifacts: 1 to add, 0 to update, 1 to remove, 2 unchanged\n",
            "nothing exported: add --yes to go ahead\n",
        ))
        .quiet();
    assert!(scenario.exists("~/backup/.mindweaver/library/rules/git/commits.md"));

    scenario
        .run("~/Projects", "mindweaver export ../backup --yes")
        .succeeds()
        .prints(concat!(
            "/home/you/backup\n",
            "  added      skill  review\n",
            "  removed    rule   git/commits\n",
            "  unchanged  skill  deploy\n",
            "  unchanged  mcp    github\n",
            "\n",
            "4 artifacts: 1 added, 0 updated, 1 removed, 2 unchanged\n",
        ))
        .quiet();
    assert!(!scenario.exists("~/backup/.mindweaver/library/rules/git/commits.md"));
}

#[test]
fn a_workspace_loads_what_an_export_wrote() {
    let scenario = library();
    scenario
        .given("~/Projects", "mindweaver export ../backup")
        .folder("~/elsewhere")
        .given("~/elsewhere", "mindweaver init");

    scenario
        .run("~/elsewhere", "mindweaver load ../backup")
        .succeeds()
        .prints(concat!(
            "loaded Projects as ../backup: 1 skill and 1 rule and 1 MCP server to install\n",
            "  into the workspace at /home/you/elsewhere\n",
        ))
        .quiet();
}

#[test]
fn export_refuses_a_workspace_in_use() {
    let scenario = library();

    scenario
        .run("~/Projects", "mindweaver export .")
        .exits(1)
        .prints("")
        .says(concat!(
            "error: /home/you/Projects is where this workspace keeps what it would export: ",
            "export into a folder of its own\n",
        ));
}

#[test]
fn backup_sends_the_library_to_a_repository_in_a_commit_of_its_own() {
    let scenario = library();
    scenario.remote("~/backups/library.git");

    let ran = scenario.run("~/Projects", "mindweaver backup ../backups/library.git");

    let head = scenario.branch_head("~/backups/library.git", "main");
    ran.succeeds()
        .prints(&format!(
            concat!(
                "/home/you/backups/library.git, branch main\n",
                "  added  skill  deploy\n",
                "  added  rule   git/commits\n",
                "  added  mcp    github\n",
                "\n",
                "3 artifacts: 3 added, 0 updated, 0 removed, 0 unchanged\n",
                "pushed {head} to main\n",
            ),
            head = head
        ))
        .quiet();
    assert_eq!(
        scenario.files_on("~/backups/library.git", "main"),
        [
            ".mindweaver/library/mcp/github.toml",
            ".mindweaver/library/rules/git/commits.md",
            ".mindweaver/library/skills/deploy/SKILL.md",
            ".mindweaver/weaver.toml",
        ]
    );
    assert!(scenario.files("~/Projects/.mindweaver/tmp").is_empty());
}

#[test]
fn a_second_backup_with_nothing_changed_pushes_nothing() {
    let scenario = library();
    scenario
        .remote("~/backups/library.git")
        .given("~/Projects", "mindweaver backup ../backups/library.git");

    scenario
        .run("~/Projects", "mindweaver backup ../backups/library.git")
        .succeeds()
        .prints(concat!(
            "/home/you/backups/library.git, branch main\n",
            "  unchanged  skill  deploy\n",
            "  unchanged  rule   git/commits\n",
            "  unchanged  mcp    github\n",
            "\n",
            "3 artifacts: 0 added, 0 updated, 0 removed, 3 unchanged\n",
            "nothing to push: main holds this already\n",
        ))
        .quiet();
}

#[test]
fn backup_with_a_kind_and_a_branch_sends_only_that_kind_there() {
    let scenario = library();
    scenario.remote("~/backups/library.git");

    let ran = scenario.run(
        "~/Projects",
        "mindweaver backup ../backups/library.git --kind skills --branch skills",
    );

    let head = scenario.branch_head("~/backups/library.git", "skills");
    ran.succeeds()
        .prints(&format!(
            concat!(
                "/home/you/backups/library.git, branch skills\n",
                "  added  skill  deploy\n",
                "\n",
                "1 artifact: 1 added, 0 updated, 0 removed, 0 unchanged\n",
                "pushed {head} to skills\n",
            ),
            head = head
        ))
        .quiet();
    assert_eq!(
        scenario.files_on("~/backups/library.git", "skills"),
        [
            ".mindweaver/library/skills/deploy/SKILL.md",
            ".mindweaver/weaver.toml",
        ]
    );
}

#[test]
fn backup_refuses_a_token_variable_that_holds_nothing() {
    let scenario = library();

    scenario
        .run(
            "~/Projects",
            "mindweaver backup https://github.com/you/library-backup.git --token-env GITHUB_TOKEN",
        )
        .exits(1)
        .prints("")
        .says(concat!(
            "error: $GITHUB_TOKEN holds no token: set it for this command, ",
            "or leave --token-env out to log in as git does\n",
        ));
}

#[test]
fn backup_refuses_a_url_carrying_a_credential_and_never_says_it() {
    let scenario = library();

    scenario
        .run(
            "~/Projects",
            "mindweaver backup https://you:hunter2@github.com/you/library-backup.git",
        )
        .exits(1)
        .prints("")
        .says(concat!(
            "error: https://github.com/you/library-backup.git: carries a credential, which would ",
            "be kept in the check-out and said in every error: name a variable holding a token ",
            "with --token-env instead\n",
        ));
}
