//! The reading commands (`list`, `show`, `validate`) over every kind, from
//! wherever a project keeps them.

mod common;

use common::*;

// ---------------------------------------------------------------------------
// Reading skills, at whichever level
// ---------------------------------------------------------------------------

#[test]
fn show_prints_the_metadata_and_the_instructions() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: Ship it\nallowed-tools: Bash, Read\nlicense: MIT\n---\n\nRun the pipeline.\n",
    );

    let outcome = mind(dir.path(), &["show", "deploy"]).unwrap();

    assert!(outcome.stdout.starts_with("deploy\n"));
    assert!(outcome.stdout.contains("SKILL.md"));
    assert!(outcome.stdout.contains("Ship it"));
    assert!(outcome.stdout.contains("allowed-tools: Bash, Read"));
    assert!(outcome.stdout.contains("license: MIT"));
    assert!(outcome.stdout.contains("Run the pipeline."));
}

#[test]
fn show_prints_compatibility_and_the_metadata_that_is_strings() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: Ship it\nlicense: MIT\ncompatibility: Requires git and jq\nmetadata:\n  author: acme\n  owner:\n    team: ops\n  version: \"1.0\"\n---\n\nRun the pipeline.\n",
    );

    let outcome = mind(dir.path(), &["show", "deploy"]).unwrap();

    let file = dir.path().join("skills").join("deploy").join("SKILL.md");
    assert_eq!(
        outcome.stdout,
        format!(
            "deploy\n{}\n\nShip it\n\nlicense: MIT\ncompatibility: Requires git and jq\nmetadata:\n  author: acme\n  version: 1.0\n\nRun the pipeline.\n",
            file.display()
        )
    );
    // The nested entry is `validate`'s to report.
    let checked = mind(dir.path(), &["validate"]).unwrap();
    assert!(
        checked
            .stdout
            .contains("`metadata` entry `owner` is not a string"),
        "{}",
        checked.stdout
    );
}

#[test]
fn show_names_the_skill_it_could_not_find() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();

    let error = mind(dir.path(), &["show", "absent"]).unwrap_err();

    assert!(matches!(*error.error, CliError::UnknownArtifact(name) if name == "absent"));
}

#[test]
fn the_workspace_shows_both_projects_declaring_one_name() {
    let dir = workspace_with_two();
    // Give beta a skill named like alpha's, which is legal across projects.
    write_skill(
        &dir.path().join("beta"),
        "alpha",
        &skill_file("alpha", "The other one"),
    );

    let outcome = weaver(dir.path(), &["show", "alpha"]).unwrap();

    assert!(outcome.stdout.contains("alpha (alpha)"));
    assert!(outcome.stdout.contains("alpha (beta)"));
    assert!(outcome.stdout.contains("\n---\n"), "separated");
}

#[test]
fn validate_fails_and_explains_a_broken_skill() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("Deployment", "Ship it"));

    let outcome = mind(dir.path(), &["validate"]).unwrap();

    assert!(!outcome.ok);
    assert!(outcome.stdout.contains("2 problems"), "{}", outcome.stdout);
    assert!(outcome.stdout.contains("the directory is `deploy`"));
    assert!(outcome.stdout.contains("1 skill checked, 1 invalid"));
}

#[test]
fn validate_warns_about_what_an_agent_reads_past_without_failing() {
    // Nested metadata is what a shipping agent writes; a project holding such
    // a skill, installed or its own, still passes.
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(
        dir.path(),
        "nested",
        "---\nname: nested\ndescription: Nested metadata\ncompatibility: [claude-code]\nmetadata:\n  owner:\n    team: ops\n---\n",
    );
    write_skill(
        dir.path(),
        "worse",
        "---\nname: worse\ndescription: ''\nmetadata:\n  pinned: true\n---\n",
    );

    let outcome = mind(dir.path(), &["validate"]).unwrap();

    assert_eq!(
        outcome.stdout,
        "nested: ok, 2 warnings\n  \
         - warning: `compatibility` is not a string, so it is ignored, and left in the file as written\n  \
         - warning: `metadata` entry `owner` is not a string, so it is ignored, and left in the file as written\n\
         worse: 1 problem, 1 warning\n  \
         - `description` is empty\n  \
         - warning: `metadata` entry `pinned` is not a string, so it is ignored, and left in the file as written\n\
         \n\
         2 skills checked, 1 invalid\n"
    );
    assert!(!outcome.ok, "`worse` is invalid");
    let nested = mind(dir.path(), &["validate", "nested"]).unwrap();
    assert!(nested.ok, "{}", nested.stdout);
}

#[test]
fn validate_can_be_pointed_at_one_skill() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_skill(dir.path(), "bad", &skill_file("Bad", "Also fine"));

    let outcome = mind(dir.path(), &["validate", "good"]).unwrap();

    assert!(outcome.ok);
    assert!(outcome.stdout.contains("1 skill checked, 0 invalid"));
}

#[test]
fn an_unreadable_skill_is_a_warning_on_stderr() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_skill(dir.path(), "broken", "# no front matter at all\n");

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.stdout.contains("good"));
    assert_eq!(outcome.stderr.len(), 1);
    assert!(outcome.stderr[0].text.contains("front matter"));
    assert!(!outcome.ok);
}

#[test]
fn a_warning_carries_the_code_of_what_it_says_beside_its_text() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "broken", "# no front matter at all\n");

    let failure = mind(dir.path(), &["show", "broken"]).unwrap_err();

    // Core's code for what the file lacks, and core's for the condition the
    // CLI words as `nothing named`, which is what the panel acts on.
    assert_eq!(failure.warnings[0].code, Some("front_matter_missing"));
    assert!(failure.warnings[0].text.starts_with("warning: "));
    assert_eq!(failure.error.code(), "not_found");
}

#[test]
fn a_project_is_found_from_a_subdirectory() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));
    let deep = dir.path().join("apps/core/src");
    fs::create_dir_all(&deep).unwrap();

    assert!(mind(&deep, &["list"]).unwrap().stdout.contains("deploy"));
}

#[test]
fn init_run_twice_says_so_and_succeeds() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    let project = mind(dir.path(), &["init"]).unwrap();
    let workspace = weaver(dir.path(), &["init"]).unwrap();

    assert!(project.ok && workspace.ok);
    assert!(project.stdout.contains("already initialized"));
    assert!(workspace.stdout.contains("already initialized"));
}

#[test]
fn a_workspace_whose_projects_all_broke_does_not_claim_to_manage_none() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("alpha")).unwrap();
    fs::remove_dir_all(dir.path().join("beta")).unwrap();

    let outcome = weaver(dir.path(), &["list"]).unwrap();

    // "manages no projects yet" would send the user to link something that is
    // already linked; the registry has two entries that simply will not open.
    assert!(
        !outcome.stdout.contains("manages no projects yet"),
        "{}",
        outcome.stdout
    );
    assert!(outcome.stdout.contains("none of which could be opened"));
    assert!(outcome.stdout.contains("mindweaver unlink"));
    assert_eq!(outcome.stderr.len(), 2);
    assert!(!outcome.ok);
}

#[test]
fn a_failing_command_still_reports_what_it_had_already_noticed() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "broken", "# no front matter at all\n");

    let failure = mind(dir.path(), &["show", "broken"]).unwrap_err();

    // "no skill named `broken`" on its own is baffling when the file is right
    // there; the warning is the half that explains it.
    assert!(matches!(*failure.error, CliError::UnknownArtifact(_)));
    assert_eq!(failure.warnings.len(), 1);
    assert!(failure.warnings[0].text.contains("front matter"));
}

#[test]
fn unlink_reports_every_entry_it_removed() {
    let dir = workspace_with_two();
    fs::write(
        dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG),
        "version = 1\nname = \"w\"\nprojects = [\"alpha\", \"./alpha\", \"beta\"]\n",
    )
    .unwrap();

    let outcome = weaver(dir.path(), &["unlink", "alpha"]).unwrap();

    assert_eq!(outcome.stdout, "unlinked alpha\nunlinked ./alpha\n");
    // And it is genuinely gone, rather than reported gone.
    let listed = weaver(dir.path(), &["list"]).unwrap();
    assert!(!listed.stdout.contains("alpha"), "{}", listed.stdout);
    assert!(listed.stdout.contains("beta"));
}

#[test]
fn both_spellings_answer_version_and_help() {
    // `mind weaver --version` erroring while `mindweaver --version` worked was the
    // two spellings drifting, which is the one thing the shortcut must not do.
    for line in [
        vec!["mind", "weaver", "--version"],
        vec!["mind", "--version"],
    ] {
        let error = Cli::try_parse_from(&line).unwrap_err();
        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::DisplayVersion,
            "{line:?} did not answer with a version"
        );
    }
    assert_eq!(
        WeaverCli::try_parse_from(["mindweaver", "--version"])
            .unwrap_err()
            .kind(),
        clap::error::ErrorKind::DisplayVersion
    );
}

#[test]
fn each_binary_describes_itself_rather_than_the_crate() {
    let mind = Cli::try_parse_from(["mind", "--help"])
        .unwrap_err()
        .to_string();
    let weaver = WeaverCli::try_parse_from(["mindweaver", "--help"])
        .unwrap_err()
        .to_string();

    // One crate description cannot be right for two binaries: `mind --help`
    // used to advertise the pre-split, cross-project scope.
    assert!(mind.contains("in a mind project"), "{mind}");
    assert!(weaver.contains("weaver workspace"), "{weaver}");
    assert_ne!(mind.lines().next().unwrap(), weaver.lines().next().unwrap());
}

#[test]
fn list_says_in_its_help_every_kind_it_takes() {
    let mind = Cli::try_parse_from(["mind", "list", "--help"])
        .unwrap_err()
        .to_string();
    let weaver = WeaverCli::try_parse_from(["mindweaver", "list", "--help"])
        .unwrap_err()
        .to_string();

    for help in [mind, weaver] {
        for kind in Kind::ALL {
            assert!(help.contains(&format!("`{}`", kind.folder())), "{help}");
        }
    }
}

// ---------------------------------------------------------------------------
// More than one kind
// ---------------------------------------------------------------------------

#[test]
fn init_makes_a_folder_for_skills_and_rules_and_none_for_mcp_servers() {
    let dir = TempDir::new().unwrap();

    mind(dir.path(), &["init"]).unwrap();

    for kind in [Kind::Skill, Kind::Rule] {
        assert!(
            dir.path().join(kind.folder()).is_dir(),
            "no folder for {kind}"
        );
    }
    // An `mcp` at a repository's root is as likely to be a server's code,
    // so a project keeps definitions only where its settings say.
    assert!(!dir.path().join(Kind::Mcp.folder()).exists());
    let settings = fs::read_to_string(dir.path().join(".mind/settings.toml")).unwrap();
    assert!(!settings.contains("mcp"), "{settings}");
}

#[test]
fn the_kind_column_appears_only_when_more_than_one_kind_is_in_play() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let alone = mind(dir.path(), &["list"]).unwrap();
    assert_eq!(alone.stdout, "deploy  Ship it\n");

    write_rule(dir.path(), "no-force-push.md", "# Never force-push\n");
    let mixed = mind(dir.path(), &["list"]).unwrap();

    // The same rule the project column follows: qualify only when it
    // disambiguates.
    assert_eq!(
        mixed.stdout,
        "skill  deploy         Ship it\nrule   no-force-push  Never force-push\n"
    );
}

#[test]
fn a_kind_can_be_listed_on_its_own() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));
    write_rule(dir.path(), "no-force-push.md", "Never force-push.\n");

    let rules = mind(dir.path(), &["list", "rules"]).unwrap();
    let skills = mind(dir.path(), &["list", "skills"]).unwrap();

    // Narrowed to one kind, the column goes away again.
    assert_eq!(rules.stdout, "no-force-push  Never force-push.\n");
    assert_eq!(skills.stdout, "deploy  Ship it\n");
    // Singular is accepted too: one word in two grammatical positions.
    assert_eq!(mind(dir.path(), &["list", "rule"]).unwrap(), rules);
}

#[test]
fn an_unknown_kind_says_what_was_expected() {
    let error = Cli::try_parse_from(["mind", "list", "prompts"]).unwrap_err();

    let message = error.to_string();
    assert!(message.contains("skills"), "{message}");
    assert!(message.contains("rules"), "{message}");
}

#[test]
fn show_takes_a_qualified_name_when_one_name_belongs_to_two_kinds() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "The skill"));
    write_rule(dir.path(), "deploy.md", "The rule.\n");

    let both = mind(dir.path(), &["show", "deploy"]).unwrap();
    let just_the_rule = mind(dir.path(), &["show", "rule:deploy"]).unwrap();

    // Ambiguous on purpose: both are shown, and each is labelled by kind
    // because that is what tells them apart.
    assert!(both.stdout.contains("skill:deploy"));
    assert!(both.stdout.contains("rule:deploy"));
    assert!(both.stdout.contains("\n---\n"));
    // Resolved, the qualifier is no longer doing any work, so it goes.
    assert!(just_the_rule.stdout.starts_with("deploy\n"));
    assert!(just_the_rule.stdout.contains("The rule."));
}

#[test]
fn show_prints_no_metadata_block_for_a_rule() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_rule(
        dir.path(),
        "git/no-force-push.md",
        "# Never force-push\n\nUse --force-with-lease.\n",
    );

    let outcome = mind(dir.path(), &["show", "git/no-force-push"]).unwrap();

    let expected_head = "git/no-force-push\n";
    assert!(
        outcome.stdout.starts_with(expected_head),
        "{}",
        outcome.stdout
    );
    assert!(outcome.stdout.contains("no-force-push.md"));
    assert!(outcome.stdout.contains("Use --force-with-lease."));
    // A rule that declares nothing, as most do, gets no metadata section
    // rather than an empty one.
    assert!(!outcome.stdout.contains("allowed-tools"));
    assert!(!outcome.stdout.contains("description"));
}

#[test]
fn show_prints_what_a_rule_declares_and_not_its_front_matter() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_rule(
        dir.path(),
        "api.md",
        "---\ndescription: How the API is written\npaths:\n  - \"src/api/**/*.{ts,tsx}\"\n  - lib/**\n---\n\n# API rules\n\nReturn errors as values.\n",
    );
    write_rule(
        dir.path(),
        "scoped.md",
        "---\npaths: src/**\n---\n# Scoped\n",
    );

    let both = mind(dir.path(), &["show", "api"]).unwrap();
    let scoped = mind(dir.path(), &["show", "scoped"]).unwrap();

    let rules = dir.path().join("rules");
    // A pattern a line, since one can hold a comma of its own.
    assert_eq!(
        both.stdout,
        format!(
            "api\n{}\n\nHow the API is written\n\npaths:\n  - src/api/**/*.{{ts,tsx}}\n  - lib/**\n\n# API rules\n\nReturn errors as values.\n",
            rules.join("api.md").display()
        )
    );
    assert_eq!(
        scoped.stdout,
        format!(
            "scoped\n{}\n\npaths:\n  - src/**\n\n# Scoped\n",
            rules.join("scoped.md").display()
        )
    );
}

#[test]
fn validate_warns_about_rule_front_matter_it_cannot_read_and_still_passes() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    // Unquoted, a glob opening with `*` is a YAML alias.
    write_rule(
        dir.path(),
        "typescript.md",
        "---\npaths:\n  - **/*.ts\n---\n\n# TypeScript\n",
    );

    let outcome = mind(dir.path(), &["validate"]).unwrap();

    assert!(outcome.ok, "{}", outcome.stdout);
    assert!(
        outcome.stdout.contains(
            "typescript: ok, 1 warning\n  - warning: the front matter does not parse, so it is read as content, fences and all: "
        ),
        "{}",
        outcome.stdout
    );
}

#[test]
fn validate_counts_each_kind_by_its_own_name() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "one", &skill_file("one", "Fine"));
    write_skill(dir.path(), "two", &skill_file("two", "Fine"));
    write_rule(dir.path(), "solo.md", "Context.\n");

    let all = mind(dir.path(), &["validate"]).unwrap();
    let rules_only = mind(dir.path(), &["validate", "rules"]).unwrap();

    assert!(
        all.stdout
            .contains("2 skills and 1 rule checked, 0 invalid"),
        "{}",
        all.stdout
    );
    assert!(rules_only.stdout.contains("1 rule checked, 0 invalid"));
    // Narrowed to one kind, labels stop naming it.
    assert!(rules_only.stdout.contains("solo: ok"));
    assert!(all.stdout.contains("rule:solo: ok"));
}

#[test]
fn validate_can_still_be_pointed_at_one_artifact() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init"]).unwrap();
    write_skill(dir.path(), "good", &skill_file("good", "Fine"));
    write_rule(dir.path(), "empty.md", "\n  \n");
    write_rule(
        dir.path(),
        "scoped.md",
        "---\npaths:\n  - \"src/**\"\n---\n",
    );

    let one = mind(dir.path(), &["validate", "good"]).unwrap();
    let broken = mind(dir.path(), &["validate", "empty"]).unwrap();
    let scoped = mind(dir.path(), &["validate", "scoped"]).unwrap();

    assert!(one.ok);
    assert!(one.stdout.contains("1 skill checked, 0 invalid"));
    // A file of front matter alone has content, just none an agent reads.
    for broken in [broken, scoped] {
        assert!(!broken.ok);
        assert!(
            broken
                .stdout
                .contains("the file has no content beyond any front matter"),
            "{}",
            broken.stdout
        );
    }
}

#[test]
fn a_workspace_lists_every_kind_of_every_project() {
    let dir = workspace_with_two();
    write_rule(&dir.path().join("alpha"), "team/style.md", "House style.\n");

    let outcome = weaver(dir.path(), &["list"]).unwrap();

    // Three columns now: project, kind, name, each earning its place.
    assert_eq!(
        outcome.stdout,
        "alpha  skill  alpha       A skill\n\
         beta   skill  beta        A skill\n\
         alpha  rule   team/style  House style.\n"
    );
    // Narrowed to rules there is only one project left with any, so the
    // project column stops telling the reader anything and goes.
    let rules = weaver(dir.path(), &["list", "rules"]).unwrap();
    assert_eq!(rules.stdout, "team/style  House style.\n");
}

#[test]
fn a_rule_is_listed_by_the_first_line_of_its_body() {
    let dir = workspace_with_two();
    write_rule(
        &dir.path().join("alpha"),
        "api.md",
        "---\npaths:\n  - \"src/api/**\"\n---\n\n# Return errors as values\n",
    );
    write_rule(
        &dir.path().join("beta"),
        "plain.md",
        "# Never force-push\n\nUse the lease.\n",
    );

    let rules = weaver(dir.path(), &["list", "rules"]).unwrap();

    // Front matter is not what a rule says, so it is not its summary; a rule
    // without any is summarised as it always was.
    assert_eq!(
        rules.stdout,
        "alpha  api    Return errors as values\n\
         beta   plain  Never force-push\n"
    );
}

// ---------------------------------------------------------------------------
// Where a project keeps its artifacts
// ---------------------------------------------------------------------------

#[test]
fn init_can_be_told_where_this_project_keeps_its_skills() {
    let dir = TempDir::new().unwrap();

    let outcome = mind(dir.path(), &["init", "--skills", ".claude/skills"]).unwrap();

    assert!(outcome.ok);
    assert!(dir.path().join(".claude").join("skills").is_dir());
    let written = fs::read_to_string(dir.path().join(MIND_DIR).join(SETTINGS_FILE)).unwrap();
    // The kind that was not mentioned keeps its default, written down too,
    // since a kind the settings leave out has no folder at all.
    assert!(
        written.ends_with("\n[directories]\nskills = \".claude/skills\"\nrules = \"rules\"\n"),
        "{written}"
    );
    assert!(dir.path().join("rules").is_dir());
}

#[test]
fn listing_reads_from_where_the_project_says_its_skills_are() {
    let dir = TempDir::new().unwrap();
    // A folder no agent reads, so only the settings lead the listing there:
    // `.claude/skills` would be read for Claude whatever they said.
    mind(dir.path(), &["init", "--skills", "docs/skills"]).unwrap();
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));

    let outcome = mind(dir.path(), &["list", "skills"]).unwrap();

    assert_eq!(outcome.stdout, "deploy  Ship it\n");
    // And it really is the configured folder that was read.
    assert!(dir.path().join("docs/skills/deploy/SKILL.md").is_file());
}

#[test]
fn an_empty_project_says_which_directories_it_looked_in() {
    let dir = TempDir::new().unwrap();
    mind(dir.path(), &["init", "--skills", "docs/skills"]).unwrap();

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert!(outcome.stdout.starts_with("nothing found"));
    // The configured directory, not the marker: a project that keeps its
    // skills elsewhere is exactly the one whose empty listing needs explaining.
    //
    // Compared against what the project itself says rather than against
    // `"docs/skills"`, because the command prints a path and Windows prints
    // one with backslashes.
    for kind in [Kind::Skill, Kind::Rule] {
        let looked_in = directory_for(dir.path(), kind).display().to_string();
        assert!(outcome.stdout.contains(&looked_in), "{}", outcome.stdout);
    }
}

#[test]
fn init_refuses_a_directory_outside_the_project() {
    let dir = TempDir::new().unwrap();

    let failure = mind(dir.path(), &["init", "--skills", "../elsewhere"]).unwrap_err();

    assert!(
        failure.error.to_string().contains("inside the project"),
        "{}",
        failure.error
    );
    assert!(!dir.path().join(MIND_DIR).exists(), "nothing was made");
}

#[test]
fn init_refuses_a_directory_inside_the_mind_that_keeps_itself_out_of_git() {
    let dir = TempDir::new().unwrap();

    let failure = mind(dir.path(), &["init", "--skills", ".mind/skills"]).unwrap_err();

    assert_eq!(
        failure.error.to_string(),
        ".mind/skills: cannot hold a project's skills, because it is inside .mind, which holds \
         Mindweaver's own files and is kept out of git unless the project is shared"
    );
    assert!(!dir.path().join(MIND_DIR).exists(), "nothing was made");
}

#[test]
fn a_workspace_sees_a_project_that_keeps_its_skills_elsewhere() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let project = dir.path().join("collapse");
    fs::create_dir(&project).unwrap();
    // Not `.claude/skills`, which Claude's folder would reach anyway.
    mind(&project, &["init", "--skills", "docs/skills"]).unwrap();
    write_skill(&project, "deploy", &skill_file("deploy", "Ship it"));
    weaver(dir.path(), &["link", "collapse"]).unwrap();

    let outcome = weaver(dir.path(), &["list"]).unwrap();

    // The workspace reads the project's own answer rather than assuming one.
    assert!(outcome.stdout.contains("deploy"), "{}", outcome.stdout);
}

// ---------------------------------------------------------------------------
// A project marked by its settings
// ---------------------------------------------------------------------------

/// Write `contents` at `route` below `root`, wherever that is.
fn put(root: &Path, route: &str, contents: &str) {
    let path = root.join(route);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

/// A project in `dir`, marked by its settings, naming every agent and
/// authoring skills in `skills/`.
fn marked(dir: &Path) -> MindProject {
    let directories = mindweaver_core::Directories::default().with(Kind::Skill, "skills");
    MindProject::init_settings(
        dir,
        &mindweaver_core::Agent::ALL,
        mindweaver_core::Share::Shared,
        &directories,
    )
    .unwrap()
    .0
}

#[test]
fn a_skill_in_both_agents_folders_is_listed_once() {
    let dir = TempDir::new().unwrap();
    marked(dir.path());
    let deploy = skill_file("deploy", "Ship it");
    put(dir.path(), ".agents/skills/deploy/SKILL.md", &deploy);
    put(dir.path(), ".claude/skills/deploy/SKILL.md", &deploy);

    let outcome = mind(dir.path(), &["list"]).unwrap();

    assert_eq!(outcome.stdout, "deploy  Ship it\n");
    assert!(
        outcome.ok && outcome.stderr.is_empty(),
        "{:?}",
        outcome.stderr
    );
}

#[test]
fn copies_that_differ_are_a_warning_and_validate_counts_them() {
    let dir = TempDir::new().unwrap();
    marked(dir.path());
    put(
        dir.path(),
        ".agents/skills/deploy/SKILL.md",
        &skill_file("deploy", "Ship it"),
    );
    put(
        dir.path(),
        ".claude/skills/deploy/SKILL.md",
        &skill_file("deploy", "Ship it otherwise"),
    );
    let warning =
        format!(
        "warning: the copies of `deploy` at {} and {} differ, so the agents reading them do not \
         all load the same skill",
        dir.path().join(".agents").join("skills").join("deploy").display(),
        dir.path().join(".claude").join("skills").join("deploy").display()
    );

    let listed = mind(dir.path(), &["list"]).unwrap();
    let checked = mind(dir.path(), &["validate"]).unwrap();

    assert_eq!(listed.stdout, "deploy  Ship it\n");
    assert_eq!(listed.stderr, std::slice::from_ref(&warning));
    assert!(!listed.ok);
    assert_eq!(
        checked.stdout,
        "deploy: ok\n\n1 skill checked, 0 invalid, 1 with copies that differ\n"
    );
    assert_eq!(checked.stderr, [warning]);
    assert!(!checked.ok);
}

#[test]
fn an_empty_project_marked_by_its_settings_says_every_folder_it_looked_in() {
    let dir = TempDir::new().unwrap();
    let project = marked(dir.path());

    let outcome = mind(dir.path(), &["list"]).unwrap();

    let name = project.name();
    assert_eq!(
        outcome.stdout,
        format!(
            "nothing found\n  {name}: {}\n  {name}: {}\n  {name}: {}\n",
            dir.path().join(".agents").join("skills").display(),
            dir.path().join(".claude").join("skills").display(),
            dir.path().join("skills").display()
        )
    );
}
