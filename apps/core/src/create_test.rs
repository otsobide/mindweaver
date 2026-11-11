//! Unit tests for `create.rs`, kept beside it rather than inside it.
//!
//! The integration suite adds artifacts to projects on the disk; these hold
//! what is built before a byte is written: a description's checks, a
//! skill's manifest, a rule template filled in, the placeholders, and the
//! plan a front end judges a draft by.

use super::*;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{ProjectMother, SkillMother};
use crate::settings::Share;
use crate::workspace::Directories;

const DRAWS: usize = 200;

fn project() -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &[], Share::Shared);
    (dir, project)
}

/// What a manifest says once parsed back, as discovery reads it.
fn read_back(manifest: &str) -> SkillManifest {
    let document = frontmatter::split(manifest).expect("a manifest has front matter");
    SkillManifest::parse(document.front_matter).expect("its front matter parses")
}

#[test]
fn filling_puts_the_name_and_the_description_wherever_a_placeholder_is() {
    assert_eq!(
        fill(
            "# {{name}}\n\n{{description}}. Run {{name}} again.\n",
            "deploy",
            "Ship it"
        ),
        "# deploy\n\nShip it. Run deploy again.\n"
    );
}

#[test]
fn text_without_a_placeholder_is_filled_as_it_is() {
    for _ in 0..DRAWS {
        let text = random::text(60).replace('{', "(");

        assert_eq!(fill(&text, &random::kebab(), "x"), text, "{text:?}");
    }
}

/// Only the doubled braces are a placeholder: a template's own braces,
/// such as a shell's or a format string's, are its business.
#[test]
fn braces_that_spell_no_placeholder_are_left_as_they_are() {
    let text = "${name} {name} {{ name }} {{NAME}} {{description }}";

    assert_eq!(fill(text, "deploy", "Ship it"), text);
}

#[test]
fn any_name_and_description_leave_no_placeholder_behind() {
    for _ in 0..DRAWS {
        let (name, description) = (random::kebab(), SkillMother::description());
        let text = format!(
            "{}{NAME_PLACEHOLDER}{}{DESCRIPTION_PLACEHOLDER}{}",
            random::word(),
            random::word(),
            random::word()
        );
        let filled = fill(&text, &name, &description);

        assert!(!filled.contains(NAME_PLACEHOLDER), "{filled:?}");
        assert!(!filled.contains(DESCRIPTION_PLACEHOLDER), "{filled:?}");
        assert!(
            filled.contains(&name) && filled.contains(&description),
            "{filled:?}"
        );
    }
}

#[test]
fn an_empty_description_is_refused_for_the_kind_it_was_for() {
    for kind in Kind::ALL {
        let error = check_description(kind, "").unwrap_err();

        assert!(
            matches!(error, CreateError::DescriptionEmpty { kind: refused } if refused == kind),
            "{error:?}"
        );
        assert_eq!(
            error.to_string(),
            format!("a {kind} needs a description: one line saying what it is")
        );
    }
}

#[test]
fn a_description_over_more_than_one_line_is_refused() {
    for description in [
        "one\ntwo",
        "one\r\ntwo",
        "one\rtwo",
        "trailing\n",
        "\nleading",
    ] {
        for kind in Kind::ALL {
            assert!(
                matches!(
                    check_description(kind, description),
                    Err(CreateError::DescriptionNotOneLine)
                ),
                "{description:?}"
            );
        }
    }
}

#[test]
fn a_description_of_the_limit_is_taken_and_one_over_it_is_refused() {
    let at_the_limit = "a".repeat(MAX_DESCRIPTION_LEN);
    let over = "a".repeat(MAX_DESCRIPTION_LEN + 1);

    assert!(check_description(Kind::Skill, &at_the_limit).is_ok());
    assert!(matches!(
        check_description(Kind::Skill, &over),
        Err(CreateError::DescriptionTooLong { length }) if length == MAX_DESCRIPTION_LEN + 1
    ));
}

/// Characters, not bytes: a description in a script that spends several
/// bytes a character is held to the same length as one in ASCII.
#[test]
fn a_description_s_length_is_counted_in_characters() {
    let wide = "\u{4e2d}".repeat(MAX_DESCRIPTION_LEN);

    assert!(wide.len() > MAX_DESCRIPTION_LEN);
    assert!(check_description(Kind::Rule, &wide).is_ok());
}

#[test]
fn any_one_line_description_within_the_limit_is_taken() {
    for _ in 0..DRAWS {
        let description = random::text(80).replace(['\n', '\r'], " ");
        if description.is_empty() {
            continue;
        }

        assert!(
            check_description(random::pick(&Kind::ALL), &description).is_ok(),
            "{description:?}"
        );
    }
}

#[test]
fn a_skill_s_manifest_opens_with_its_name_and_description_and_a_heading() {
    let manifest = skill_manifest("deploy", "Ship the service", "", "").unwrap();

    assert_eq!(
        manifest,
        "---\nname: deploy\ndescription: Ship the service\n---\n\n# deploy\n"
    );
}

#[test]
fn a_skill_s_manifest_reads_back_as_the_name_and_description_it_was_given() {
    for _ in 0..DRAWS {
        let (name, description) = (random::kebab(), SkillMother::description());
        let manifest = skill_manifest(&name, &description, "", "").unwrap();
        let parsed = read_back(&manifest);

        assert_eq!(parsed.name, name, "{manifest}");
        assert_eq!(parsed.description, description, "{manifest}");
    }
}

/// Serialized rather than formatted, so YAML's own characters in a
/// description come back as typed rather than as a mapping, a comment or a
/// quoted string.
#[test]
fn a_description_in_yaml_s_own_characters_reads_back_as_typed() {
    for description in [
        "Use when: the deploy fails",
        "# not a comment",
        "\"quoted\" at the start",
        "'single' too",
        "- a list item?",
        "{ a: mapping }",
        "[1, 2]",
        "yes",
        "null",
        "123",
        "a #hash inside",
        "trailing colon:",
        "&anchor *alias",
        "!tag value",
        "%percent",
        "@at and `tick`",
    ] {
        let manifest = skill_manifest("deploy", description, "", "").unwrap();

        assert_eq!(read_back(&manifest).description, description, "{manifest}");
    }
}

#[test]
fn a_skill_s_manifest_keeps_the_rest_of_the_front_matter_and_the_body() {
    let manifest = skill_manifest(
        "deploy",
        "Ship it",
        "license: MIT\nallowed-tools: Bash\n",
        "\n# Deploy\n\nSteps.\n",
    )
    .unwrap();

    assert_eq!(
        manifest,
        "---\nname: deploy\ndescription: Ship it\nlicense: MIT\nallowed-tools: Bash\n---\n\n# Deploy\n\nSteps.\n"
    );
}

/// A manifest is never front matter alone: a body of nothing but space is
/// given the heading a skill without a template opens with.
#[test]
fn a_skill_s_body_of_only_space_is_given_its_heading() {
    for body in ["", " ", "\n\n", "\t\n "] {
        let manifest = skill_manifest("deploy", "Ship it", "", body).unwrap();

        assert!(
            manifest.ends_with("---\n\n# deploy\n"),
            "{body:?}: {manifest}"
        );
    }
}

/// What the rest of a template's front matter says of the name or the
/// description, the line-by-line drop missed, refuses the manifest rather
/// than leave a name that is not the one typed.
#[test]
fn front_matter_that_sets_the_name_again_is_refused() {
    let refused = skill_manifest("deploy", "Ship it", "\"name\": other\n", "");

    assert!(refused.is_err(), "{refused:?}");
}

#[test]
fn front_matter_that_does_not_parse_is_refused_with_why() {
    let refused = skill_manifest("deploy", "Ship it", "license: [unclosed\n", "").unwrap_err();

    assert!(
        refused.starts_with("its front matter does not parse: "),
        "{refused}"
    );
}

/// The strict rules `rename` and `install` hold a skill to, reached
/// through the same function, so a rule added there reaches what `add`
/// writes: a manifest built around a name or a description breaking one is
/// refused here as it would be there.
#[test]
fn a_manifest_breaking_a_strict_rule_is_refused_as_breaking_it() {
    let long = "a".repeat(MAX_DESCRIPTION_LEN + 1);
    for (name, description) in [("Deploy", "Ship it"), ("deploy", long.as_str())] {
        let refused = skill_manifest(name, description, "", "").unwrap_err();

        assert!(
            refused.starts_with("it would break a rule: "),
            "{name}: {refused}"
        );
    }
}

/// What `validate` only warns of, such as a `compatibility` too long, is
/// written: strict when writing means the rules an agent cannot read past.
#[test]
fn front_matter_validate_only_warns_of_is_written() {
    let compatibility = "a".repeat(crate::skill::MAX_COMPATIBILITY_LEN + 1);

    let manifest = skill_manifest(
        "deploy",
        "Ship it",
        &format!("compatibility: {compatibility}\n"),
        "",
    )
    .unwrap();

    assert!(manifest.contains(&compatibility), "{manifest}");
}

#[test]
fn a_rule_template_without_front_matter_opens_with_the_description() {
    let filled = rule_from_template("Always {{name}}.\n", "lint", "Run the linter").unwrap();

    assert_eq!(filled, "# Run the linter\n\nAlways lint.\n");
}

#[test]
fn a_rule_template_that_places_the_description_gets_no_heading_of_its_own() {
    let filled = rule_from_template(
        "## {{description}}\n\nFor {{name}}.\n",
        "lint",
        "Run the linter",
    )
    .unwrap();

    assert_eq!(filled, "## Run the linter\n\nFor lint.\n");
}

/// Above the fences the heading would leave the front matter as content,
/// so it goes below them, the template's blank lines before its body
/// folded into the one it is given.
#[test]
fn a_rule_template_s_front_matter_stays_above_the_heading() {
    let filled = rule_from_template(
        "---\npaths:\n  - \"src/**\"\n---\n\n\nBody of {{name}}.\n",
        "lint",
        "Run the linter",
    )
    .unwrap();

    assert_eq!(
        filled,
        "---\npaths:\n  - \"src/**\"\n---\n\n# Run the linter\n\nBody of lint.\n"
    );
}

#[test]
fn a_description_a_rule_template_declares_is_replaced_by_the_one_typed() {
    let filled = rule_from_template(
        "---\ndescription: whatever the template said\npaths:\n  - \"*.rs\"\n---\nBody.\n",
        "lint",
        "Use when: linting Rust",
    )
    .unwrap();
    let (manifest, _) = RuleManifest::read(&filled);

    assert_eq!(
        manifest.description.stated().map(String::as_str),
        Some("Use when: linting Rust")
    );
    assert!(!filled.contains("whatever the template said"), "{filled}");
}

#[test]
fn a_rule_template_s_crlf_fences_are_kept_as_written() {
    let filled = rule_from_template(
        "---\r\npaths:\r\n  - \"src/**\"\r\n---\r\nBody.\r\n",
        "lint",
        "Lint",
    )
    .unwrap();

    assert!(filled.starts_with("---\r\npaths:\r\n"), "{filled:?}");
    assert!(
        filled.contains("---\r\n\n# Lint\n\nBody.\r\n"),
        "{filled:?}"
    );
}

#[test]
fn a_rule_template_whose_paths_are_not_text_is_refused() {
    let refused = rule_from_template("---\npaths: 3\n---\nBody.\n", "lint", "Lint").unwrap_err();

    assert_eq!(refused, issue_list(&[ValidationIssue::PathsNotStrings]));
}

/// The template's `description` is dropped line by line with what
/// continues it, so even one that is no text gives way to the one typed.
#[test]
fn a_rule_template_s_description_of_any_shape_gives_way_to_the_one_typed() {
    let filled = rule_from_template(
        "---\ndescription:\n  nested: map\npaths:\n  - \"x\"\n---\nBody.\n",
        "lint",
        "Lint",
    )
    .unwrap();

    assert_eq!(
        RuleManifest::read(&filled)
            .0
            .description
            .stated()
            .map(String::as_str),
        Some("Lint")
    );
    assert!(!filled.contains("nested"), "{filled}");
}

/// Any description and name fill a template with no front matter into a
/// rule whose opening line is the description.
#[test]
fn any_rule_template_without_front_matter_opens_with_what_was_typed() {
    for _ in 0..DRAWS {
        let (name, description) = (random::kebab(), SkillMother::description());
        let body = format!("{} {{{{name}}}} {}\n", random::word(), random::word());
        let filled = rule_from_template(&body, &name, &description).unwrap();

        assert!(
            filled.starts_with(&format!("# {description}\n\n")),
            "{filled:?}"
        );
        assert!(filled.contains(&name), "{filled:?}");
    }
}

#[test]
fn a_name_validate_would_refuse_is_refused_before_anything_is_planned() {
    let (_dir, project) = project();
    for kind in Kind::ALL {
        let unwritable = match kind {
            Kind::Skill | Kind::Rule => crate::mother::ArtifactNameMother::unwritable(),
            Kind::Mcp => crate::mother::McpMother::unwritable(),
        };
        for (why, name) in unwritable {
            let refused = plan(&project, kind, &name, "Ship it", Start::Scratch);

            assert!(
                matches!(refused, Err(CreateError::Name { .. })),
                "{why}: {name:?} as a {kind:?} gave {refused:?}"
            );
        }
    }
}

#[test]
fn a_skill_named_by_a_route_is_refused_as_nested() {
    let (_dir, project) = project();

    let refused = plan(
        &project,
        Kind::Skill,
        "ops/deploy",
        "Ship it",
        Start::Scratch,
    )
    .unwrap_err();

    assert!(matches!(refused, CreateError::Nested { .. }), "{refused:?}");
    assert_eq!(refused.code(), "add_skill_nested");
}

/// The description is trimmed before it is checked, so one of nothing but
/// space is empty, and a line with a newline at its end is one line.
#[test]
fn a_description_is_trimmed_before_it_is_checked_and_written() {
    let (_dir, project) = project();

    assert!(matches!(
        plan(&project, Kind::Rule, "lint", "  \n\t ", Start::Scratch),
        Err(CreateError::DescriptionEmpty { .. })
    ));
    let planned = plan(
        &project,
        Kind::Rule,
        "lint",
        "  Run the linter\n",
        Start::Scratch,
    )
    .unwrap();
    assert!(
        matches!(&planned, Scaffold::Rule { text, .. } if text == "# Run the linter\n"),
        "{planned:?}"
    );
}

#[test]
fn a_rule_is_planned_at_its_route_in_the_project_s_folder_for_rules() {
    let (_dir, project) = project();
    let folder = project.directory_for(Kind::Rule).unwrap();

    let planned = plan(
        &project,
        Kind::Rule,
        "git/no-force",
        "Never force",
        Start::Scratch,
    )
    .unwrap();

    assert_eq!(planned.path(), folder.join("git").join("no-force.md"));
    assert!(matches!(
        &planned,
        Scaffold::Rule { name, text, .. } if name == "git/no-force" && text == "# Never force\n"
    ));
}

#[test]
fn a_skill_is_planned_as_its_folder_holding_its_manifest_alone() {
    let (_dir, project) = project();
    let folder = project.directory_for(Kind::Skill).unwrap();

    let planned = plan(&project, Kind::Skill, "deploy", "Ship it", Start::Scratch).unwrap();

    assert_eq!(planned.path(), folder.join("deploy"));
    match &planned {
        Scaffold::Skill { files, .. } => {
            assert_eq!(files.len(), 1, "{files:?}");
            assert_eq!(files[0].0, Path::new("SKILL.md"));
            assert_eq!(
                String::from_utf8_lossy(&files[0].1),
                "---\nname: deploy\ndescription: Ship it\n---\n\n# deploy\n"
            );
        }
        other => panic!("not a skill: {other:?}"),
    }
}

/// Planning is a read: the folder of the kind, and the artifact, are left
/// for the write to make.
#[test]
fn planning_writes_nothing() {
    let (_dir, project) = project();
    let skills = project.directory_for(Kind::Skill).unwrap();
    let _ = fs::remove_dir_all(&skills);

    let planned = plan(&project, Kind::Skill, "deploy", "Ship it", Start::Scratch).unwrap();

    assert!(!planned.path().exists());
    assert!(!skills.exists());
}

#[test]
fn a_kind_the_project_keeps_no_folder_for_cannot_be_planned() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init_settings(
        dir.path(),
        &[],
        Share::Shared,
        &Directories::default().with(Kind::Skill, "skills"),
    )
    .unwrap();
    assert_eq!(project.directory_for(Kind::Rule), None);

    let refused = plan(&project, Kind::Rule, "lint", "Lint", Start::Scratch).unwrap_err();

    assert!(matches!(refused, CreateError::Workspace(_)), "{refused:?}");
}

#[test]
fn a_plan_is_free_while_nothing_is_where_it_goes() {
    let (_dir, project) = project();

    let planned = plan(&project, Kind::Rule, "lint", "Lint", Start::Scratch).unwrap();

    assert!(planned.taken().is_none());
}

/// Whatever is there refuses it, a link to nothing included: the write
/// would refuse it too, with `create_new`.
#[test]
fn a_plan_is_taken_by_whatever_is_already_where_it_goes() {
    let (_dir, project) = project();
    let planned = plan(&project, Kind::Skill, "deploy", "Ship it", Start::Scratch).unwrap();
    fs::create_dir_all(planned.path().parent().unwrap()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("/nowhere/at/all", planned.path()).unwrap();
    #[cfg(not(unix))]
    fs::create_dir_all(planned.path()).unwrap();

    let taken = planned.taken().expect("something is there");

    assert!(
        matches!(&taken, CreateError::Exists { path } if path == planned.path()),
        "{taken:?}"
    );
    assert_eq!(taken.code(), "add_exists");
}

#[test]
fn a_planned_rule_reads_as_the_rule_it_would_be() {
    let (_dir, project) = project();
    let planned = plan(
        &project,
        Kind::Rule,
        "git/no-force",
        "Never force",
        Start::Scratch,
    )
    .unwrap();

    let artifact = planned.read_as(&project).unwrap();

    assert_eq!(artifact.kind(), Kind::Rule);
    assert_eq!(artifact.name().as_str(), "git/no-force");
    assert_eq!(artifact.summary(), Some("Never force"));
    assert!(!planned.path().exists(), "read, never written");
}

#[test]
fn a_planned_skill_reads_as_the_skill_it_would_be() {
    let (_dir, project) = project();
    let planned = plan(&project, Kind::Skill, "deploy", "Ship it", Start::Scratch).unwrap();

    let artifact = planned.read_as(&project).unwrap();

    assert_eq!(artifact.kind(), Kind::Skill);
    assert_eq!(artifact.name().as_str(), "deploy");
    assert!(!planned.path().exists(), "read, never written");
}

#[test]
fn writing_a_new_file_over_one_there_is_refused_and_leaves_it() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("rule.md");
    let held = random::bytes(64);
    fs::write(&path, &held).unwrap();

    let refused = write_new(&path, b"new").unwrap_err();

    assert!(matches!(refused, CreateError::Exists { .. }), "{refused:?}");
    assert_eq!(fs::read(&path).unwrap(), held);
}

#[test]
fn a_new_file_is_written_with_exactly_what_it_is_given() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("rule.md");
    let contents = random::bytes(random::between(0, 256));

    write_new(&path, &contents).unwrap();

    assert_eq!(fs::read(&path).unwrap(), contents);
}

#[test]
fn a_new_file_in_a_folder_that_is_not_there_cannot_be_written() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("gone").join("rule.md");

    let refused = write_new(&path, b"x").unwrap_err();

    assert!(
        matches!(&refused, CreateError::Write { path: at, .. } if at == &path),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "add_write_failed");
}

#[test]
fn each_create_error_of_its_own_gives_its_own_code() {
    let errors = [
        (
            CreateError::Name {
                name: String::new(),
                issues: vec![ValidationIssue::NameEmpty],
            },
            "add_name_unfit",
        ),
        (
            CreateError::Nested {
                name: String::from("a/b"),
            },
            "add_skill_nested",
        ),
        (
            CreateError::DescriptionEmpty { kind: Kind::Skill },
            "add_description_empty",
        ),
        (
            CreateError::DescriptionNotOneLine,
            "add_description_not_one_line",
        ),
        (
            CreateError::DescriptionTooLong { length: 2000 },
            "add_description_too_long",
        ),
        (
            CreateError::Exists {
                path: PathBuf::from("/p"),
            },
            "add_exists",
        ),
        (
            CreateError::Manifest {
                reason: String::from("x"),
            },
            "add_manifest_unwritable",
        ),
        (
            CreateError::Write {
                path: PathBuf::from("/p"),
                source: io::Error::other("full"),
            },
            "add_write_failed",
        ),
        (
            CreateError::McpUnscaffolded {
                directory: PathBuf::from("/p/mcp"),
            },
            "add_mcp_unscaffolded",
        ),
        (
            CreateError::TransportNotMcp { kind: Kind::Skill },
            "add_transport_not_mcp",
        ),
        (
            CreateError::DefinitionNotMcp { kind: Kind::Rule },
            "add_definition_not_mcp",
        ),
        (
            CreateError::KeyRepeated {
                table: "env",
                key: String::from("K"),
            },
            "add_mcp_key_repeated",
        ),
        (
            CreateError::KeyUnsayable {
                option: "--env",
                key: String::from("K=V"),
            },
            "add_mcp_key_unsayable",
        ),
        (
            CreateError::UrlUserinfo {
                url: crate::value::Redacted::new("https://example.com/mcp"),
            },
            "add_mcp_url_userinfo",
        ),
        (
            CreateError::Definition {
                name: String::from("x"),
                issues: Vec::new(),
            },
            "add_mcp_unfit",
        ),
    ];
    let given: Vec<&str> = errors.iter().map(|(_, code)| *code).collect();
    for (error, code) in errors {
        assert_eq!(error.code(), code, "{error:?}");
    }
    assert_eq!(CreateError::CODES, given.as_slice());
}

#[test]
fn a_description_too_long_says_its_length_and_the_limit() {
    let error = CreateError::DescriptionTooLong { length: 2000 };

    assert_eq!(
        error.to_string(),
        format!(
            "the description is 2000 characters, over the {MAX_DESCRIPTION_LEN} character limit"
        )
    );
}

#[test]
fn a_name_refused_says_every_issue_it_has() {
    let issues = artifact::name_issues("Bad_Name/x--y");
    let error = CreateError::Name {
        name: String::from("Bad_Name/x--y"),
        issues: issues.clone(),
    };

    assert_eq!(error.to_string(), issue_list(&issues));
    assert!(issues.len() >= 2, "{issues:?}");
}
