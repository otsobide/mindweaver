//! The mothers make what they say they make.
//!
//! A test built on a mother trusts it the way it trusts a fixture written by
//! hand, and a mother that drifted (a "valid" name the rules now refuse, a
//! URL form whose clean spelling is no longer what redacting leaves) would
//! make every such test pass or fail for the wrong reason. Each is held here
//! to the rule it claims to follow, through core's own judgement of it.

use std::collections::BTreeSet;
use std::fs;

use super::*;
use crate::agent::Agent;
use crate::artifact::{name_issues, strict_issues, ValidationIssue};
use crate::frontmatter;
use crate::kind::Kind;
use crate::names::Names;
use crate::rule::RuleManifest;
use crate::settings::{Settings, Share};
use crate::skill::SkillManifest;
use crate::url::{carries_credentials, redacted};
use crate::workspace::{MindProject, WeaverWorkspace};

const DRAWS: usize = 100;

fn manifest(text: &str) -> SkillManifest {
    let document = frontmatter::split(text).expect("a skill's file has front matter");
    SkillManifest::parse(document.front_matter).expect("its front matter parses")
}

#[test]
fn each_credentialed_url_is_redacted_to_the_clean_url_beside_it() {
    for url in UrlMother::every_credentialed() {
        assert!(url.typed.contains(&url.secret), "{url:?}");
        assert!(!url.clean.contains(&url.secret), "{url:?}");
        assert_eq!(redacted(&url.typed), url.clean, "{url:?}");
    }
}

#[test]
fn the_credentialed_forms_are_all_different_with_secrets_of_their_own() {
    let every = UrlMother::every_credentialed();
    let typed: BTreeSet<&str> = every.iter().map(|url| url.typed.as_str()).collect();
    let secrets: BTreeSet<&str> = every.iter().map(|url| url.secret.as_str()).collect();

    assert_eq!(typed.len(), every.len());
    assert_eq!(secrets.len(), every.len());
}

#[test]
fn no_clean_url_carries_a_credential() {
    for _ in 0..DRAWS {
        for url in UrlMother::every_clean() {
            assert!(!carries_credentials(&url), "{url}");
        }
    }
}

#[test]
fn a_quoted_url_is_redacted_to_the_clean_words_beside_it() {
    for _ in 0..DRAWS {
        let quoted = UrlMother::quoted(&UrlMother::credentialed());

        assert_eq!(redacted(&quoted.typed), quoted.clean, "{quoted:?}");
    }
}

#[test]
fn every_text_a_hash_mother_calls_no_hash_is_refused_for_its_own_reason() {
    let not = ContentHashMother::not_hashes();
    let reasons: BTreeSet<&str> = not.iter().map(|(why, _)| *why).collect();

    assert_eq!(reasons.len(), not.len(), "a reason named twice");
    for (why, text) in not {
        assert!(crate::value::ContentHash::parse(&text).is_none(), "{why}");
    }
}

#[test]
fn every_text_a_pin_mother_calls_no_pin_is_refused_for_its_own_reason() {
    let not = PinMother::not_pins();
    let reasons: BTreeSet<&str> = not.iter().map(|(why, _)| *why).collect();

    assert_eq!(reasons.len(), not.len(), "a reason named twice");
    for (why, text) in not {
        assert!(crate::value::Pin::parse(&text).is_none(), "{why}");
    }
    for _ in 0..DRAWS {
        let pin = PinMother::random();
        assert_eq!(crate::value::Pin::parse(&pin), Some(pin.clone()), "{pin}");
    }
}

#[test]
fn every_text_an_id_mother_calls_no_id_is_refused() {
    for (why, text) in ProjectIdMother::not_ids() {
        assert!(crate::value::ProjectId::parse(&text).is_none(), "{why}");
    }
}

#[test]
fn every_unwritable_name_breaks_the_name_rule() {
    for (why, name) in ArtifactNameMother::unwritable() {
        assert!(!name_issues(&name).is_empty(), "{why}: {name:?}");
    }
}

#[test]
fn a_random_skill_is_one_mind_add_would_write() {
    for _ in 0..DRAWS {
        let (name, text) = SkillMother::random();

        let manifest = manifest(&text);
        assert_eq!(manifest.name, name);
        assert_eq!(strict_issues(&manifest, &name), [], "{text}");
    }
}

#[test]
fn a_skills_named_shapes_break_the_rule_they_are_named_for() {
    let name = random::kebab();

    let long = manifest(&SkillMother::with_long_description(&name));
    assert!(strict_issues(&long, &name)
        .iter()
        .any(|issue| matches!(issue, ValidationIssue::DescriptionTooLong { .. })));
    assert!(frontmatter::split(&SkillMother::without_front_matter(&name)).is_err());
    assert!(SkillManifest::parse(
        frontmatter::split(&SkillMother::without_description(&name))
            .unwrap()
            .front_matter
    )
    .is_err());
}

#[test]
fn a_rule_has_something_to_say_and_a_scoped_one_says_which_files() {
    for _ in 0..DRAWS {
        let rule = RuleMother::random();
        assert!(
            crate::artifact::rule_text_issues(&rule).is_empty(),
            "{rule}"
        );
        assert!(!RuleManifest::read(&rule).0.is_scoped(), "{rule}");
    }
    let scoped = RuleMother::scoped("tests", &["**/*_test.rs", "tests/**"]);
    let (manifest, body) = RuleManifest::read(&scoped);

    assert!(manifest.is_scoped());
    assert_eq!(
        manifest.paths.stated().map(Vec::as_slice),
        Some(&[String::from("**/*_test.rs"), String::from("tests/**")][..])
    );
    assert_eq!(body.trim_start(), RuleMother::create("tests"));
}

#[test]
fn every_definition_the_mother_makes_reads_as_the_transport_it_is_named_for() {
    use crate::mcp::{McpDefinition, Transport};

    let name = random::kebab();
    for (text, slug) in [
        (McpMother::stdio(&name), "stdio"),
        (McpMother::http(&name), "http"),
        (McpMother::sse(&name), "sse"),
    ] {
        let definition = McpDefinition::parse(&text).unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(definition.name, name.as_str());
        assert_eq!(definition.transport.slug(), slug);
        assert!(!definition.inputs.is_empty(), "{slug}");
        assert_eq!(definition.issues(), [], "{text}");
    }
    let sse = McpDefinition::parse(&McpMother::sse(&name)).unwrap();
    assert!(matches!(sse.transport, Transport::Sse(_)));
    for _ in 0..DRAWS {
        let (name, text) = McpMother::random();
        assert_eq!(crate::mcp::name::issues(&name), [], "{name}");
        assert_eq!(McpDefinition::parse(&text).unwrap().issues(), [], "{text}");
    }
}

#[test]
fn a_definitions_named_shapes_break_the_rule_they_are_named_for() {
    use crate::mcp::{McpDefinition, McpError};

    let name = random::kebab();

    assert!(matches!(
        McpDefinition::parse(&McpMother::in_format(&name, "2")),
        Err(McpError::FormatTooNew { found: 2 })
    ));
    for transport in ["stdio", "http", "sse"] {
        let incomplete = McpDefinition::parse(&McpMother::incomplete(&name, transport)).unwrap();
        assert!(matches!(
            &incomplete.issues()[..],
            [ValidationIssue::TransportIncomplete { .. }]
        ));
    }
    let unreferenced = McpDefinition::parse(&McpMother::unreferenced(&name)).unwrap();
    assert!(unreferenced
        .issues()
        .iter()
        .any(|issue| matches!(issue, ValidationIssue::InputUnreferenced { .. })));
    let in_userinfo = McpDefinition::parse(&McpMother::in_userinfo(&name)).unwrap();
    assert!(matches!(
        &in_userinfo.issues()[..],
        [ValidationIssue::InputInUserinfo { .. }]
    ));
}

#[test]
fn what_add_is_told_of_a_server_writes_a_definition_with_nothing_wrong() {
    use crate::mcp::McpDefinition;

    for _ in 0..DRAWS {
        let given = McpMother::given();
        let name = McpMother::name();

        let text = given.text(&name, &SkillMother::description());

        assert_eq!(given.repeated(), None, "{given:?}");
        let definition = McpDefinition::parse(&text).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(definition.name, name.as_str());
        assert_eq!(definition.issues(), [], "{text}");
    }
}

#[test]
fn what_add_is_told_in_a_shape_named_for_a_rule_breaks_that_rule() {
    use crate::mcp::McpDefinition;

    let read = |given: &crate::mcp::Given| {
        McpDefinition::parse(&given.text(&McpMother::name(), &SkillMother::description()))
            .unwrap()
            .issues()
    };
    for (why, given, code) in McpMother::given_unfit() {
        let issues = read(&given);
        assert_eq!(
            issues.iter().map(ValidationIssue::code).collect::<Vec<_>>(),
            [code],
            "{why}: {issues:?}"
        );
    }
    for (prefix, token) in McpMother::tokens() {
        for place in ["arg", "env", "header", "url", "secret_env"] {
            let (given, at) = McpMother::given_with_literal(place, &token);
            let issues = read(&given);
            assert_eq!(
                issues.first(),
                Some(&ValidationIssue::SecretLiteral {
                    at,
                    prefix: prefix.to_owned()
                }),
                "{place}: {issues:?}"
            );
            assert!(
                issues.iter().all(|issue| issue.code() == "secret_literal"),
                "{place}: {issues:?}"
            );
        }
    }
}

#[test]
fn what_add_is_told_of_a_token_it_refuses_unread_holds_it_where_it_is_named_for() {
    use crate::mcp::Given;
    use crate::url::{carries_credentials, redacted};

    for (_, token) in McpMother::tokens() {
        for (why, given, code) in McpMother::given_refused_unread(&token) {
            match (code, &given) {
                ("add_mcp_key_repeated", _) => assert_eq!(
                    given.repeated().map(|repeated| repeated.key),
                    Some(token.clone()),
                    "{why}"
                ),
                ("add_mcp_url_userinfo", Given::Address(address)) => {
                    assert_eq!(given.repeated(), None, "{why}");
                    assert!(carries_credentials(&address.url), "{why}");
                    assert!(redacted(&address.url).contains(&token), "{why}");
                }
                _ => panic!("{why}: {code}: {given:?}"),
            }
        }
    }
}

#[test]
fn input_and_variable_names_are_drawn_as_they_are_usually_written() {
    for _ in 0..DRAWS {
        let input = InputNameMother::random();
        let variable = VariableNameMother::random();

        assert!(
            input.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "{input}"
        );
        assert!(
            variable.chars().all(|c| c.is_ascii_uppercase() || c == '_'),
            "{variable}"
        );
        assert!(input.is_writable(), "{input}");
        assert!(variable.is_portable(), "{variable}");
        assert_eq!(InputNameMother::create(&input), input);
        assert_eq!(VariableNameMother::create(&variable), variable);
    }
}

#[test]
fn upstream_names_are_drawn_as_a_registry_names_a_server_and_the_ill_formed_are_not() {
    for _ in 0..DRAWS {
        let name = UpstreamNameMother::random();

        assert!(name.is_well_formed(), "{name}");
        assert_eq!(name.matches('/').count(), 1, "{name}");
        assert_eq!(UpstreamNameMother::create(&name), name);
    }
    for (why, text) in UpstreamNameMother::ill_formed() {
        assert!(
            !UpstreamNameMother::create(&text).is_well_formed(),
            "{why}: {text:?}"
        );
    }
}

/// What the registry mother draws reads as the registry's answer does,
/// and makes a definition, as a package the import starts.
#[test]
fn registry_answers_are_drawn_as_the_registry_writes_them() {
    for _ in 0..DRAWS {
        let answer = RegistryMother::random();

        let entry = crate::mcp_registry::Entry::read(answer.clone()).unwrap();

        assert!(entry.name.is_well_formed(), "{answer}");
        assert_eq!(entry.status, crate::mcp_registry::Status::Active);
        assert!(
            crate::mcp_registry::time::instant(entry.updated_at.as_deref().unwrap()).is_some(),
            "{answer}"
        );
        assert!(matches!(
            entry.ways()[..],
            [crate::mcp_registry::Way::Package { .. }]
        ));
    }
}

#[test]
fn input_and_variable_names_named_for_breaking_their_rule_break_it() {
    for (why, text) in InputNameMother::unwritable() {
        assert!(
            !InputNameMother::create(&text).is_writable(),
            "{why}: {text:?}"
        );
    }
    for (why, text) in VariableNameMother::unportable() {
        assert!(
            !VariableNameMother::create(&text).is_portable(),
            "{why}: {text:?}"
        );
    }
    for (prefix, token) in McpMother::tokens() {
        let (text, at) = McpMother::with_literal(&random::kebab(), "env", &token);
        let holding = crate::mcp::McpDefinition::parse(&text).unwrap();
        assert_eq!(
            holding.issues(),
            [ValidationIssue::SecretLiteral {
                at,
                prefix: prefix.to_owned()
            }]
        );
    }
    let unnamed =
        crate::mcp::McpDefinition::parse(&McpMother::secret_without_variable(&random::kebab()))
            .unwrap();
    assert!(matches!(
        &unnamed.issues()[..],
        [ValidationIssue::SecretWithoutEnvVar { .. }]
    ));
    let misnamed =
        crate::mcp::McpDefinition::parse(&McpMother::misnamed(&random::kebab())).unwrap();
    assert!(matches!(
        &misnamed.issues()[..],
        [
            ValidationIssue::InputNameMalformed { .. },
            ValidationIssue::EnvVarMalformed { .. }
        ]
    ));
}

#[test]
fn random_settings_are_written_and_read_back_as_they_were() {
    let dir = tempfile::TempDir::new().unwrap();
    for _ in 0..DRAWS {
        let settings = SettingsMother::random();
        let path = dir.path().join(crate::settings::SETTINGS_FILE);
        fs::write(&path, settings.text().unwrap()).unwrap();

        assert_eq!(Settings::read(&path).unwrap(), settings);
    }
}

#[test]
fn an_install_record_mother_files_each_copy_where_its_kind_goes() {
    let skill = InstallRecordMother::random();
    assert_eq!(skill.kind, Kind::Skill);
    assert!(skill.content_hash.is_some() && skill.origin.is_some());
    assert!(skill
        .targets
        .iter()
        .all(|target| target.path.ends_with(skill.name.as_str()) && target.content_hash.is_none()));

    let route = ArtifactNameMother::route();
    let rule = InstallRecordMother::rule(&route);
    assert_eq!(rule.kind, Kind::Rule);
    assert!(rule
        .targets
        .iter()
        .all(|target| target.content_hash.is_some()));
    assert_ne!(rule.targets[0].content_hash, rule.targets[1].content_hash);

    let old = InstallRecordMother::before_hashes("deploy");
    assert_eq!((old.origin, old.content_hash), (None, None));

    // A server's is entries, one in each agent's file under its name, and
    // no copy.
    let name = McpMother::name();
    let server = InstallRecordMother::mcp(&name);
    assert_eq!(server.kind, Kind::Mcp);
    assert!(server.targets.is_empty());
    let agents: Vec<Agent> = server.entries.iter().map(|entry| entry.agent).collect();
    assert_eq!(agents, Agent::ALL);
    let project = tempfile::TempDir::new().unwrap();
    for entry in &server.entries {
        assert_eq!(entry.name, name);
        assert_eq!(
            entry.agent.mcp_file(project.path()).ok(),
            Some(entry.file.as_str())
        );
    }
}

#[test]
fn any_install_record_holds_entries_for_a_server_alone_and_one_an_agent() {
    for _ in 0..DRAWS {
        let record = InstallRecordMother::any();
        let mut agents: Vec<Agent> = record.entries.iter().map(|entry| entry.agent).collect();
        assert!(record.kind == Kind::Mcp || agents.is_empty(), "{record:?}");
        let count = agents.len();
        agents.dedup();
        assert_eq!(agents.len(), count, "{record:?}");
        assert!(record.entries.iter().all(|entry| entry.name == record.name));
    }
}

#[test]
fn a_gathered_mother_files_what_it_gathered_under_its_sources_folder() {
    for _ in 0..DRAWS {
        let gathered = GatheredMother::random();

        assert!(
            gathered
                .path
                .ends_with(&format!("{}/{}", gathered.source.directory, gathered.name)),
            "{gathered:?}"
        );
        assert_eq!(gathered.revision, gathered.source.revision);
    }
}

#[test]
fn an_entry_mother_holds_no_credential() {
    for _ in 0..DRAWS {
        let entry = EntryMother::random();

        for text in entry.target.iter().chain(entry.detail.iter()) {
            assert!(!carries_credentials(text), "{entry:?}");
        }
    }
}

#[test]
fn the_fixture_mothers_make_a_workspace_a_project_and_a_repository() {
    let (dir, workspace) = WorkspaceMother::empty();
    assert!(WeaverWorkspace::open(dir.path()).is_ok());

    let project = ProjectMother::random(dir.path());
    assert!(MindProject::is_marked(project.root()));
    assert!(project.settings().is_some());

    let older = ProjectMother::older(&dir.path().join("older"));
    assert!(MindProject::is_marked(older.root()));
    assert!(older.settings().is_none());

    let first = ProjectMother::version_1(&dir.path().join("first"));
    assert_eq!(first.config().version(), 1);
    assert_eq!(
        first.directory_for(Kind::Skill),
        Some(first.mind_dir().join("skills"))
    );

    let authoring = ProjectMother::with_mcp(&dir.path().join("authoring"), &[], Share::Local);
    assert_eq!(
        authoring.directory_for(Kind::Mcp),
        Some(authoring.root().join("mcp"))
    );

    let json = ProjectMother::json(&dir.path().join("json"), &SettingsMother::authoring());
    assert_eq!(
        json.marker_file(),
        crate::workspace::MarkerFile::SettingsJson
    );
    assert!(json.warnings().is_empty(), "{:?}", json.warnings());
    let settings = json.settings().unwrap();
    assert_eq!(settings.version, crate::workspace::SETTINGS_VERSION);
    assert_eq!(settings.agents, Agent::ALL);
    assert_eq!(settings.share, Share::Local);
    assert_eq!(
        json.directory_for(Kind::Skill),
        Some(json.root().join("skills"))
    );
    assert!(json.root().join("rules").is_dir());
    assert_eq!(json.directory_for(Kind::Mcp), None);

    let mut workspace = workspace;
    let linked = ProjectMother::linked(&mut workspace, "linked");
    assert!(workspace.is_linked(linked.root()));

    let repository = GitRepositoryMother::with_skills(&["deploy"]);
    let opened = gix::open(repository.path()).unwrap();
    assert!(opened.head_id().is_ok());
    assert!(repository.path().join("skills/deploy/SKILL.md").is_file());
}

/// Each layout a different folder, every one of them where a skill is put
/// when it is asked for there, and one skill's repository holding its
/// `SKILL.md` at its root.
#[test]
fn a_git_repository_mother_lays_skills_out_where_it_is_told() {
    let layouts = GitRepositoryMother::layouts();
    let mut distinct = layouts.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), layouts.len(), "{layouts:?}");
    assert_eq!(layouts[0], ".");

    for layout in &layouts {
        let repository = GitRepositoryMother::with_skills_in(layout, &["deploy"]);
        assert!(gix::open(repository.path()).unwrap().head_id().is_ok());
        assert!(
            repository
                .path()
                .join(layout)
                .join("deploy/SKILL.md")
                .is_file(),
            "{layout}"
        );
    }

    let one = GitRepositoryMother::one_skill("deploy");
    assert!(one.path().join("SKILL.md").is_file());
    assert!(one.path().join("examples/inner/SKILL.md").is_file());
}

#[test]
fn a_mindflayer_mother_makes_a_workspace_under_the_old_names() {
    let (dir, workspace) = MindflayerMother::empty();

    assert!(dir.path().join(".mindflayer/flayer.toml").is_file());
    assert!(!dir.path().join(".mindweaver").exists());
    let opened = WeaverWorkspace::open(dir.path()).unwrap();
    assert_eq!(opened.names(), Names::Mindflayer);
    assert_eq!(opened, workspace);
    assert_eq!(
        workspace.ledger_path(),
        dir.path().join(".mindflayer/mindflayer.db")
    );
}

/// Each rule where its route says in the folder named, by a name a rule
/// can have, beside the README and the licence a gather leaves out, and a
/// changelog outside the folder.
#[test]
fn a_git_repository_mother_keeps_rules_by_their_routes_in_the_folder_it_is_told() {
    let routes = [
        random::kebab(),
        format!("{}/{}", random::kebab(), random::kebab()),
    ];
    let borrowed: Vec<&str> = routes.iter().map(String::as_str).collect();

    let repository = GitRepositoryMother::with_rules_in("rules", &borrowed);

    assert!(gix::open(repository.path()).unwrap().head_id().is_ok());
    for route in &routes {
        assert!(
            repository
                .path()
                .join(format!("rules/{route}.md"))
                .is_file(),
            "{route}"
        );
        assert!(
            crate::install::is_installable_name(Kind::Rule, route),
            "{route}"
        );
    }
    assert!(repository.path().join("rules/README.md").is_file());
    assert_eq!(
        fs::read_to_string(repository.path().join("rules/LICENSE")).unwrap(),
        GitRepositoryMother::LICENCE
    );
    assert!(repository.path().join("CHANGELOG.md").is_file());
}

#[test]
fn a_shelf_mother_gathers_every_rule_it_is_given_into_a_workspace() {
    let (_dir, workspace) = WorkspaceMother::empty();
    let ledger = workspace.ledger().unwrap();

    let report = ShelfMother::rules_into(&workspace, &ledger, &["git/commits", "tone"]);

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let gathered: Vec<(Kind, String)> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| (gathered.kind, gathered.name.into_string()))
        .collect();
    assert_eq!(
        gathered,
        [
            (Kind::Rule, String::from("git/commits")),
            (Kind::Rule, String::from("tone"))
        ]
    );
}

#[test]
fn a_shelf_mother_gathers_every_skill_it_is_given() {
    let (_dir, _workspace, ledger) = ShelfMother::gathered(&["deploy", "review"]);

    let names: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.name.into_string())
        .collect();
    assert_eq!(names, ["deploy", "review"]);
}

#[test]
fn a_mindflayer_mother_leaves_a_projects_blocks_under_the_old_markers() {
    let dir = tempfile::TempDir::new().unwrap();
    let new = format!(
        "# Team\n\n{}\n{}a -->\nRule a.\n{}\n",
        crate::agents_md::BEGIN,
        crate::agents_md::RULE,
        crate::agents_md::END
    );
    fs::write(dir.path().join("AGENTS.md"), &new).unwrap();
    fs::write(
        dir.path().join("CLAUDE.md"),
        format!(
            "{}\n@AGENTS.md\n{}\n",
            crate::agents_md::BEGIN,
            crate::agents_md::END
        ),
    )
    .unwrap();

    MindflayerMother::rendered_into(dir.path());

    assert_eq!(
        fs::read_to_string(dir.path().join("AGENTS.md")).unwrap(),
        "# Team\n\n<!-- mindflayer:begin -->\n<!-- mindflayer:rule a -->\nRule a.\n<!-- mindflayer:end -->\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap(),
        "<!-- mindflayer:begin -->\n@AGENTS.md\n<!-- mindflayer:end -->\n"
    );
}

#[test]
fn a_mindflayer_mother_leaves_a_projects_exclude_block_under_the_old_markers() {
    let dir = tempfile::TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    crate::exclude::add(
        dir.path(),
        "app",
        &[crate::exclude::Entry::file("AGENTS.md")],
    )
    .unwrap();

    MindflayerMother::rendered_into(dir.path());

    let text = fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
    assert!(
        text.ends_with("# mindflayer:begin\n# project app\n/AGENTS.md\n# mindflayer:end\n"),
        "{text}"
    );
}

#[test]
fn every_server_name_a_mother_draws_breaks_or_keeps_the_rule_it_is_named_for() {
    use crate::mcp::name::{issues, twins};

    for _ in 0..DRAWS {
        let name = McpMother::name();
        assert_eq!(issues(&name), [], "{name}");
        let twin = McpMother::twin(&name);
        assert_eq!(twins(&name, [twin.as_str()]), [twin.as_str()], "{name}");
    }
    for (why, name) in McpMother::unwritable() {
        assert!(
            issues(&name).iter().any(|issue| !issue.is_warning()),
            "{why}: {name:?}"
        );
    }
    for (name, _, _) in McpMother::reserved() {
        assert!(matches!(
            &issues(&name)[..],
            [ValidationIssue::McpNameReserved { .. }]
        ));
    }
    for (name, _, _) in McpMother::claimed() {
        assert!(matches!(
            &issues(&name)[..],
            [ValidationIssue::McpNameClaimed { .. }]
        ));
    }
    for (why, name) in McpMother::near_reserved() {
        assert_eq!(issues(&name), [], "{why}");
    }
}

#[test]
fn every_definition_named_for_what_codex_is_given_whole_holds_what_it_says_and_is_given_it() {
    use crate::mcp::McpDefinition;

    let name = McpMother::name();
    for (text, inputs) in [
        (McpMother::program_for_codex(&name), vec!["api_key"]),
        (McpMother::address_for_codex(&name), vec!["token"]),
        (McpMother::whole_header_for_codex(&name), vec!["token"]),
        (McpMother::plain_program(&name), vec![]),
        (McpMother::plain_address(&name), vec![]),
    ] {
        let definition = McpDefinition::parse(&text).unwrap();
        let declared: Vec<&str> = definition
            .inputs
            .iter()
            .map(|input| input.name.as_str())
            .collect();
        assert_eq!(declared, inputs, "{text}");
        assert_eq!(definition.issues(), [], "{text}");
        let entry = crate::codex_mcp::entry(&definition);
        assert!(entry.is_ok(), "{entry:?}\n{text}");
    }
}

#[test]
fn every_definition_named_for_what_an_agent_cannot_be_given_is_valid_and_is_so() {
    use crate::mcp::{Inexpressible, McpDefinition};

    let name = random::kebab();
    let read = |text: String| McpDefinition::parse(&text).unwrap();
    let shapes = [
        (McpMother::without_cwd(&name), Agent::Claude, None),
        (
            McpMother::stdio(&name),
            Agent::Claude,
            Some("cwd_unsupported"),
        ),
        (
            McpMother::secret_argument(&name),
            Agent::Codex,
            Some("secret_in_argument"),
        ),
        (
            McpMother::secret_in_url(&name),
            Agent::Codex,
            Some("secret_in_url"),
        ),
        (
            McpMother::composite_secret_header(&name),
            Agent::Codex,
            Some("secret_header_composite"),
        ),
        (McpMother::whole_secret_header(&name), Agent::Codex, None),
        (
            McpMother::stdio(&name),
            Agent::Codex,
            Some("secret_env_renamed"),
        ),
        (McpMother::whole_secret_env(&name), Agent::Codex, None),
        (
            McpMother::composite_secret_env(&name),
            Agent::Codex,
            Some("secret_env_composite"),
        ),
        (McpMother::program_for_codex(&name), Agent::Codex, None),
        (McpMother::plain_program(&name), Agent::Codex, None),
        (McpMother::plain_address(&name), Agent::Codex, None),
        (McpMother::address_for_codex(&name), Agent::Codex, None),
        (McpMother::whole_header_for_codex(&name), Agent::Codex, None),
        (
            McpMother::with_codex_extras(&name, "command = \"other\"\n"),
            Agent::Codex,
            Some("extra_repeated"),
        ),
        (
            McpMother::with_codex_extras(&name, "environment_id = \"cloud\"\n"),
            Agent::Codex,
            Some("extra_withheld"),
        ),
        (
            McpMother::sse(&name),
            Agent::OpenCode,
            Some("sse_unsupported"),
        ),
    ];
    let (_, blanked) = random::pick(&McpMother::blanked_variables());
    let (_, passed) = random::pick(&McpMother::passed_variables());
    let read_from = [
        (
            McpMother::header_read_from(&name, &blanked),
            Agent::Claude,
            Some("variable_blanked"),
        ),
        (
            McpMother::url_read_from(&name, &blanked),
            Agent::Claude,
            Some("variable_blanked"),
        ),
        (
            McpMother::header_read_from(&name, &passed),
            Agent::Claude,
            None,
        ),
        (
            McpMother::env_read_from(&name, &blanked),
            Agent::Claude,
            Some("cwd_unsupported"),
        ),
    ];
    let (_, reference) = random::pick(&McpMother::references());
    let (_, not_reference) = random::pick(&McpMother::not_references());
    let place = random::pick(&["command", "args", "env", "url", "headers"]);
    let texts = [
        (
            McpMother::with_text(&name, place, &reference).0,
            Agent::Claude,
            Some("text_expanded"),
        ),
        (
            McpMother::with_text(&name, "args", &not_reference).0,
            Agent::Claude,
            None,
        ),
        (
            McpMother::with_claude_extras(&name, "type = \"other\"\n"),
            Agent::Claude,
            Some("extra_repeated"),
        ),
    ];
    let (_, substitution) = random::pick(&McpMother::substitutions());
    let (_, not_substitution) = random::pick(&McpMother::not_substitutions());
    let filled = random::pick(&["command", "args", "env", "cwd", "url", "headers"]);
    let opencode = [
        (
            McpMother::with_text(&name, filled, &substitution).0,
            Agent::OpenCode,
            Some("text_substituted"),
        ),
        (
            McpMother::with_text(&name, "args", &not_substitution).0,
            Agent::OpenCode,
            None,
        ),
        (
            McpMother::with_opencode_extras(&name, "enabled = false\n"),
            Agent::OpenCode,
            Some("extra_repeated"),
        ),
        (
            McpMother::with_opencode_extras(&name, "timeout = 5000\n"),
            Agent::OpenCode,
            None,
        ),
    ];
    for (text, agent, code) in shapes
        .into_iter()
        .chain(read_from)
        .chain(texts)
        .chain(opencode)
    {
        let definition = read(text);
        assert_eq!(definition.issues(), [], "{agent:?} {code:?}");
        let said: Vec<&str> = definition
            .inexpressible(agent)
            .iter()
            .map(Inexpressible::code)
            .collect();
        assert_eq!(said, code.into_iter().collect::<Vec<_>>(), "{agent:?}");
    }
}
