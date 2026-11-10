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

// ---------------------------------------------------------------------------
// Wherever a repository keeps its skills (#101)
// ---------------------------------------------------------------------------

/// `~/Projects` linking `collapse`, beside `~/src/acme-skills`, a
/// repository keeping a skill in `skills/`, one where Claude reads them and
/// one in a plugin.
fn spread() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            ),
            (
                ".claude/skills/lint/SKILL.md",
                &skill("lint", "Tidy the code"),
            ),
            (
                "plugins/docs/skills/pdf/SKILL.md",
                &skill("pdf", "Read PDFs"),
            ),
        ],
    );
    scenario
}

#[test]
fn gathering_takes_skills_from_every_place_a_repository_keeps_them() {
    let scenario = spread();

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  commit-style  How this repo writes commit messages\n\
             \x20 added  lint          Tidy the code\n\
             \x20 added  pdf           Read PDFs\n\
             \n\
             3 skills: 3 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert_eq!(
        scenario.files("~/Projects/.mindweaver/skills"),
        [
            "..-src-acme-skills/commit-style/SKILL.md",
            "..-src-acme-skills/lint/SKILL.md",
            "..-src-acme-skills/pdf/SKILL.md",
        ]
    );
}

#[test]
fn a_dry_run_says_what_would_be_taken_and_from_where_and_writes_nothing() {
    let scenario = spread();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --dry-run",
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 would add  commit-style  skills/commit-style      How this repo writes commit messages\n\
             \x20 would add  lint          .claude/skills/lint      Tidy the code\n\
             \x20 would add  pdf           plugins/docs/skills/pdf  Read PDFs\n\
             \n\
             3 skills: 3 to add, 0 to update, 0 unchanged\n\
             nothing gathered: run it without --dry-run to gather\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    for untouched in ["skills", "cache", "mindweaver.db"] {
        assert!(
            !scenario.exists(&format!("~/Projects/.mindweaver/{untouched}")),
            "{untouched}"
        );
    }
}

#[test]
fn a_dry_run_after_a_gather_says_what_would_move() {
    let scenario = spread();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .change(
            SOURCE,
            &[(
                ".claude/skills/lint/SKILL.md",
                &format!("{}Fix what it can.\n", skill("lint", "Tidy the code")),
            )],
        );
    let shelf = scenario.read("~/Projects/.mindweaver/skills/..-src-acme-skills/lint/SKILL.md");

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --dry-run",
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 would update  lint          .claude/skills/lint      Tidy the code\n\
             \x20 unchanged     commit-style  skills/commit-style      How this repo writes commit messages\n\
             \x20 unchanged     pdf           plugins/docs/skills/pdf  Read PDFs\n\
             \n\
             3 skills: 0 to add, 1 to update, 2 unchanged\n\
             nothing gathered: run it without --dry-run to gather\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/.mindweaver/skills/..-src-acme-skills/lint/SKILL.md"),
        shelf
    );
}

#[test]
fn a_skill_md_at_the_root_makes_the_repository_one_skill() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/deploy-skill",
        &[
            ("SKILL.md", &skill("deploy", "Ship the service")),
            ("scripts/run.sh", "echo ship\n"),
            ("examples/inner/SKILL.md", &skill("inner", "An example")),
        ],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/deploy-skill")
        .succeeds()
        .prints(&format!(
            "src-deploy-skill: ../src/deploy-skill at {}\n\
             \x20 added  deploy  Ship the service\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head("~/src/deploy-skill")
        ))
        .quiet();

    assert_eq!(
        scenario.files("~/Projects/.mindweaver/skills"),
        [
            "..-src-deploy-skill/deploy/SKILL.md",
            "..-src-deploy-skill/deploy/examples/inner/SKILL.md",
            "..-src-deploy-skill/deploy/scripts/run.sh",
        ]
    );
}

#[test]
fn two_skills_one_source_calls_the_same_are_neither_gathered() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/ops/deploy/SKILL.md",
                &skill("deploy", "The ops one"),
            ),
            (
                "skills/web/deploy/SKILL.md",
                &skill("deploy", "The web one"),
            ),
            ("skills/review/SKILL.md", &skill("review", "Read a change")),
        ],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .exits(1)
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  review  Read a change\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .says(
            "warning: skills/ops/deploy: not taken, since skills/web/deploy in the same source \
             is called `deploy` too; gather the one wanted with `--path`\n\
             warning: skills/web/deploy: not taken, since skills/ops/deploy in the same source \
             is called `deploy` too; gather the one wanted with `--path`\n",
        );
}

#[test]
fn a_skill_s_claude_plugin_folder_is_left_off_the_shelf_with_a_note() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "plugins/ops/skills/deploy/SKILL.md",
                &skill("deploy", "Ship the service"),
            ),
            ("plugins/ops/skills/deploy/.claude-plugin/plugin.json", "{}"),
        ],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  deploy  Ship the service\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .says(
            "note: deploy was gathered without its .claude-plugin, since Claude would load the \
             skill as a plugin, with the hooks and MCP servers it declares\n",
        );

    assert_eq!(
        scenario.files("~/Projects/.mindweaver/skills"),
        ["..-src-acme-skills/deploy/SKILL.md"]
    );
}

#[test]
fn a_repository_holding_no_skill_says_where_it_looked() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source("~/src/notes", &[("README.md", "Just notes.\n")]);

    scenario
        .run("~/Projects", "mindweaver gather git ../src/notes")
        .exits(1)
        .prints("")
        .says(
            "error: ../src/notes: no skill found in the repository: looked for a `SKILL.md` \
             there, in each folder there, and in each folder of `skills`, \
             `skills/<namespace>`, `.agents/skills`, `.claude/skills` and \
             `plugins/<plugin>/skills`; name the folder that holds them with `--path`\n",
        );
}

#[test]
fn path_gathers_only_the_folder_it_names() {
    let scenario = spread();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --path .claude/skills",
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  lint  Tidy the code\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();
}

#[test]
fn a_branch_given_twice_over_is_refused_before_anything_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git acme/skills@v1 --ref v2",
        )
        .exits(1)
        .prints("")
        .says("error: the URL names the branch `v1`, and `--ref` names `v2`: give it once\n");

    assert!(!scenario.exists("~/Projects/.mindweaver/cache"));
}
