//! Gathering skills from a git repository onto the workspace's shelf, and
//! installing from there: the reference's "Gathering skills from
//! elsewhere".
//!
//! A source is named by its path from the workspace, as the reference names
//! one by its URL: a table pads every row to its longest source, which a
//! temporary folder's path would make as long as the system's says.

use crate::{skill, Scenario};

const SOURCE: &str = "~/src/acme-skills";

/// `~/Projects` linking `collapse`, beside `~/src/acme-skills`, a repository
/// offering two skills.
fn acme() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            ),
            (
                "skills/ddd-reviewer/SKILL.md",
                &skill(
                    "ddd-reviewer",
                    "Review a change against the DDD layering rules",
                ),
            ),
        ],
    );
    scenario
}

#[test]
fn gathering_a_repository_puts_its_skills_on_the_shelf_and_says_each() {
    let scenario = acme();

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  commit-style  How this repo writes commit messages\n\
             \x20 added  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 2 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert!(
        !scenario.exists("~/Projects/collapse/.claude"),
        "nothing is written into a project"
    );
}

#[test]
fn gathering_again_says_what_moved() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .change(
            SOURCE,
            &[(
                "skills/commit-style/SKILL.md",
                &format!(
                    "{}Keep the subject short.\n",
                    skill("commit-style", "How this repo writes commit messages")
                ),
            )],
        );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 updated    commit-style  How this repo writes commit messages\n\
             \x20 unchanged  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 0 added, 1 updated, 1 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();
}

#[test]
fn a_skill_breaking_the_format_is_gathered_with_a_warning() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/other-rules",
        &[(
            "skills/pdf--forms/SKILL.md",
            &skill("pdf--forms", "Fill in PDF forms"),
        )],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/other-rules")
        .exits(1)
        .prints(&format!(
            "src-other-rules: ../src/other-rules at {}\n\
             \x20 added  pdf--forms  Fill in PDF forms\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head("~/src/other-rules")
        ))
        .says(
            "warning: /home/you/Projects/.mindweaver/skills/..-src-other-rules/pdf--forms/SKILL.md: \
             `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, \
             joined by single hyphens\n",
        );
}

#[test]
fn gather_list_says_what_the_shelf_holds_and_from_where() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "commit-style  src-acme-skills  How this repo writes commit messages\n",
            "ddd-reviewer  src-acme-skills  Review a change against the DDD layering rules\n",
        ))
        .quiet();
}

#[test]
fn a_gathered_skill_installs_from_the_source_it_came_from() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver install -p collapse commit-style")
        .succeeds()
        .prints("installed commit-style into collapse, from ../src/acme-skills\n")
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"),
        scenario.read("~/src/acme-skills/skills/commit-style/SKILL.md")
    );
}

#[test]
fn a_gathered_skill_the_format_refuses_is_not_installed() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/other-rules",
        &[(
            "skills/pdf--forms/SKILL.md",
            &skill("pdf--forms", "Fill in PDF forms"),
        )],
    );
    // Gathered with its warning, which a step building the scenario would
    // refuse: the warning is the use case before this one.
    scenario
        .run("~/Projects", "mindweaver gather git ../src/other-rules")
        .exits(1);

    scenario
        .run("~/Projects", "mindweaver install -p collapse pdf--forms")
        .exits(1)
        .prints("")
        .says(
            "error: `pdf--forms` cannot be installed: `pdf--forms` is not usable as a name: \
             it has to be lowercase letters and digits, joined by single hyphens\n",
        );

    assert!(!scenario.exists("~/Projects/collapse/.claude"));
}

#[test]
fn a_url_carrying_a_token_is_refused_before_anything_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git https://me:ghp_t0ken@github.com/acme/private-skills",
        )
        .exits(1)
        .prints("")
        .says(
            "error: https://github.com/acme/private-skills: a password, a token or, over \
             http(s), a user name in the URL is refused, because gathering stores and shows the \
             URL: gather it without one, and configure a git credential helper that has it\n",
        );

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "nothing gathered yet\n",
            "  take some with `mindweaver gather git <url>`, or offer a workspace's own with \
             `mindweaver load <path>`\n",
        ));
}

#[test]
fn mindweaver_validate_reads_the_shelf_beside_the_library_and_the_projects() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver validate")
        .succeeds()
        .prints(concat!(
            "commit-style: ok\n",
            "ddd-reviewer: ok\n",
            "\n",
            "2 skills checked, 0 invalid\n",
        ))
        .quiet();
}
