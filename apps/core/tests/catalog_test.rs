//! Discovery of every kind, against real mind projects on disk.

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::ledger::{InstallRecord, Target};
use mindweaver_core::{
    Agent, ArtifactCopy, Catalog, Declared, Directories, DiscoveryFailure, Kind, Ledger,
    MindProject, Provenance, Reference, RuleManifest, Share, ValidationIssue, WeaverWorkspace,
};
use tempfile::TempDir;

/// One mind project per name, all under a single temporary parent so their
/// paths sort the way their names do.
fn projects(names: &[&str]) -> (TempDir, Vec<MindProject>) {
    let parent = TempDir::new().expect("create a temporary directory");
    let projects = names
        .iter()
        .map(|name| {
            let root = parent.path().join(name);
            fs::create_dir_all(&root).expect("create the project root");
            MindProject::init(&root)
                .expect("initialize the mind project")
                .0
        })
        .collect();
    (parent, projects)
}

fn project(name: &str) -> (TempDir, MindProject) {
    let (parent, mut projects) = projects(&[name]);
    (parent, projects.remove(0))
}

/// Write a skill: a directory holding SKILL.md.
fn write_skill(project: &MindProject, directory: &str, contents: &str) {
    let dir = project.directory_for(Kind::Skill).unwrap().join(directory);
    fs::create_dir_all(&dir).expect("create the skill directory");
    fs::write(dir.join("SKILL.md"), contents).expect("write SKILL.md");
}

/// Write a rule: one markdown file, at whatever depth `route` names.
fn write_rule(project: &MindProject, route: &str, contents: &str) {
    let path = project.directory_for(Kind::Rule).unwrap().join(route);
    fs::create_dir_all(path.parent().unwrap()).expect("create the rule folder");
    fs::write(path, contents).expect("write the rule");
}

fn skill_file(name: &str) -> String {
    format!("---\nname: {name}\ndescription: Does {name} things\n---\n\n# {name}\n\nSteps.\n")
}

// ---------------------------------------------------------------------------
// Both kinds
// ---------------------------------------------------------------------------

#[test]
fn finds_every_kind_in_a_project() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "commit-style", &skill_file("commit-style"));
    write_rule(
        &one,
        "no-force-push.md",
        "# No force push\n\nUse --force-with-lease.\n",
    );

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert!(catalog.failures().is_empty());
    let found: Vec<(Kind, &str)> = catalog
        .artifacts()
        .iter()
        .map(|artifact| (artifact.kind(), artifact.name().as_str()))
        .collect();
    assert_eq!(
        found,
        vec![(Kind::Skill, "commit-style"), (Kind::Rule, "no-force-push")]
    );
    assert_eq!(catalog.kinds(), vec![Kind::Skill, Kind::Rule]);
}

#[test]
fn discovery_can_be_narrowed_to_one_kind() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "commit-style", &skill_file("commit-style"));
    write_rule(&one, "no-force-push.md", "Context.\n");

    let rules = Catalog::discover_kinds(std::slice::from_ref(&one), &[Kind::Rule]);

    assert_eq!(rules.kinds(), vec![Kind::Rule]);
    assert_eq!(rules.artifacts().len(), 1);
}

#[test]
fn artifacts_are_grouped_by_kind_then_named_in_order() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "zebra", &skill_file("zebra"));
    write_skill(&one, "alpaca", &skill_file("alpaca"));
    write_rule(&one, "yak.md", "Context.\n");
    write_rule(&one, "ant.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let found: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|a| a.name().as_str())
        .collect();
    // Blocks of like things, alphabetical within each, rather than an
    // alphabetical shuffle of two different sorts of thing.
    assert_eq!(found, vec!["alpaca", "zebra", "ant", "yak"]);
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

#[test]
fn a_nested_rule_is_named_by_its_route() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "git/no-force-push.md", "Never force-push.\n");
    write_rule(&one, "ci/no-force-push.md", "Nor in CI.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let found: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|a| a.name().as_str())
        .collect();
    // The stem alone would make these one name for two files; the route keeps
    // folders as grouping and nothing else.
    assert_eq!(found, vec!["ci/no-force-push", "git/no-force-push"]);
    assert!(catalog.failures().is_empty());
}

#[test]
fn a_rules_summary_is_its_opening_line() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "heading.md", "# Never force-push\n\nBody.\n");
    write_rule(&one, "prose.md", "\n\nJust prose, no heading.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let summaries: Vec<Option<&str>> = catalog.artifacts().iter().map(|a| a.summary()).collect();

    // The hashes are stripped, because a row of them summarises nothing.
    assert_eq!(
        summaries,
        vec![Some("Never force-push"), Some("Just prose, no heading.")]
    );
}

#[test]
fn a_rule_without_front_matter_declares_nothing() {
    let (_dir, one) = project("alpha");
    let text = "# Never force-push\n\nUse --force-with-lease.\n";
    write_rule(&one, "context.md", text);

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let rule = &catalog.artifacts()[0];

    assert_eq!(rule.declared(), &Declared::Rule(RuleManifest::default()));
    assert_eq!(rule.description(), None);
    assert_eq!(rule.manifest(), None);
    assert_eq!(rule.contents().unwrap(), text);
}

#[test]
fn a_rule_declares_what_its_front_matter_says_and_is_summarised_by_its_body() {
    let (_dir, one) = project("alpha");
    write_rule(
        &one,
        "api.md",
        "---\nname: not-its-name\ndescription: How the API is written\npaths:\n  - \"src/api/**\"\n---\n\n# API rules\n\nReturn errors as values.\n",
    );

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let rule = &catalog.artifacts()[0];

    // A rule is named by its route whatever its front matter says.
    assert_eq!(rule.name(), "api");
    assert_eq!(rule.summary(), Some("API rules"));
    assert_eq!(rule.description(), Some("How the API is written"));
    let Declared::Rule(manifest) = rule.declared() else {
        panic!("a rule declares a rule's front matter");
    };
    assert_eq!(
        manifest.paths.stated(),
        Some(&vec![String::from("src/api/**")])
    );
    // What an agent is given is the body, never the front matter.
    assert_eq!(
        rule.contents().unwrap(),
        "\n# API rules\n\nReturn errors as values.\n"
    );
    assert!(rule.validate().is_empty());
}

#[test]
fn a_horizontal_rule_at_the_top_of_a_rule_is_content() {
    let (_dir, one) = project("alpha");
    let text = "---\n\nNever force-push.\n\n---\n\nUse the lease.\n";
    write_rule(&one, "fenced.md", text);

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let rule = &catalog.artifacts()[0];

    // As it was before a rule could declare anything.
    assert_eq!(rule.summary(), Some("---"));
    assert_eq!(rule.contents().unwrap(), text);
    assert!(rule.validate().is_empty());
}

#[test]
fn front_matter_that_does_not_parse_is_content_with_a_warning() {
    let (_dir, one) = project("alpha");
    let text = "---\npaths:\n  - **/*.ts\n---\n\n# TypeScript\n";
    write_rule(&one, "typescript.md", text);

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let rule = &catalog.artifacts()[0];

    assert!(catalog.failures().is_empty());
    assert_eq!(rule.contents().unwrap(), text);
    let issues = rule.validate();
    assert!(
        matches!(
            issues.as_slice(),
            [ValidationIssue::FrontMatterUnparsed { .. }]
        ),
        "{issues:?}"
    );
    assert!(issues[0].is_warning());
}

#[test]
fn an_empty_rule_is_the_one_thing_that_can_be_wrong_with_it() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "empty.md", "\n   \n\n");
    write_rule(&one, "fine.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let empty = &catalog.artifacts()[0];
    assert_eq!(empty.name(), "empty");
    assert_eq!(empty.validate(), vec![ValidationIssue::Empty]);
    assert!(catalog.artifacts()[1].validate().is_empty());
}

#[test]
fn a_rules_name_is_checked_segment_by_segment() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "Git/no-force-push.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let issues = catalog.artifacts()[0].validate();

    // The bad segment is named, not the whole route: `no-force-push` is fine.
    assert_eq!(
        issues,
        vec![ValidationIssue::NameNotKebabCase {
            segment: "Git".into()
        }]
    );
}

#[test]
fn only_markdown_files_are_rules_and_hidden_ones_are_not() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "real.md", "Context.\n");
    write_rule(&one, "notes.txt", "not a rule");
    write_rule(&one, ".hidden.md", "not a rule either");
    write_rule(&one, ".git/config.md", "nor this");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let found: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|a| a.name().as_str())
        .collect();
    assert_eq!(found, vec!["real"]);
    assert!(catalog.failures().is_empty());
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

#[test]
fn a_skill_keeps_its_declared_name_and_its_directory_is_checked_against_it() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deployment"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));
    let skill = &catalog.artifacts()[0];

    // The name comes from where it is declared. A rule declares nowhere, so
    // its name is its route; a skill declares one, so a mismatch with the
    // directory is a problem rather than a renaming.
    assert_eq!(skill.name(), "deployment");
    assert_eq!(
        skill.validate(),
        vec![ValidationIssue::NameDirectoryMismatch {
            name: "deployment".into(),
            directory: "deploy".into(),
        }]
    );
}

#[test]
fn a_skills_own_directory_is_never_walked_into() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deploy"));
    let assets = one
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy/references");
    fs::create_dir_all(&assets).unwrap();
    fs::write(assets.join("SKILL.md"), skill_file("nested")).unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    // The directory belongs to the skill, assets and all.
    assert_eq!(catalog.artifacts().len(), 1);
}

#[test]
fn one_broken_artifact_does_not_hide_the_others() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "good", &skill_file("good"));
    write_skill(&one, "no-front-matter", "# Just markdown\n");
    write_skill(&one, "unterminated", "---\nname: unterminated\n");
    write_rule(&one, "fine.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert_eq!(
        catalog.artifacts().len(),
        2,
        "the good ones are still listed"
    );
    assert_eq!(catalog.failures().len(), 2);
    for failure in catalog.failures() {
        assert!(matches!(failure, DiscoveryFailure::Artifact(_)));
    }
}

#[test]
fn a_project_with_no_folder_for_a_kind_is_not_a_failure() {
    let (_dir, one) = project("alpha");
    fs::remove_dir(one.directory_for(Kind::Rule).unwrap()).unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert!(catalog.is_empty());
    assert!(catalog.failures().is_empty());
}

#[test]
fn locating_finds_what_discovery_reads_broken_or_not() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "good", &skill_file("good"));
    write_skill(&one, "no-front-matter", "# Just markdown\n");
    fs::create_dir_all(one.directory_for(Kind::Skill).unwrap().join("assets")).unwrap();
    write_rule(&one, "git/no-force-push.md", "Never.\n");
    write_rule(&one, ".hidden.md", "Not a rule.\n");
    write_rule(&one, "notes.txt", "Not a rule either.\n");
    let skills = one.directory_for(Kind::Skill).unwrap();
    let rules = one.directory_for(Kind::Rule).unwrap();

    let (found_skills, skill_failures) = Catalog::locate(&skills, Kind::Skill);
    let (found_rules, rule_failures) = Catalog::locate(&rules, Kind::Rule);

    // A skill whose front matter is broken is still where one is: nothing
    // is read, so nothing fails.
    assert_eq!(
        found_skills,
        [skills.join("good"), skills.join("no-front-matter")]
    );
    assert_eq!(found_rules, [rules.join("git/no-force-push.md")]);
    assert!(skill_failures.is_empty() && rule_failures.is_empty());
}

#[test]
fn locating_in_a_folder_that_is_not_there_finds_nothing_and_no_failure() {
    let dir = TempDir::new().unwrap();

    let (found, failures) = Catalog::locate(&dir.path().join("missing"), Kind::Rule);

    assert!(found.is_empty());
    assert!(failures.is_empty());
}

#[test]
fn discovery_looks_in_each_skills_own_folder_and_nothing_below_it() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deploy"));
    let skills = one.directory_for(Kind::Skill).unwrap();
    fs::create_dir_all(skills.join("deploy/scripts/deep")).unwrap();
    // A folder with no manifest yet: one written into it makes it a skill.
    fs::create_dir_all(skills.join("draft")).unwrap();
    fs::write(skills.join("README.md"), "Not a skill.\n").unwrap();

    let folders = Catalog::looked_in(&skills, Kind::Skill);

    assert_eq!(
        folders,
        [skills.clone(), skills.join("deploy"), skills.join("draft")]
    );
}

#[test]
fn discovery_looks_in_every_folder_of_rules_it_walks() {
    let (_dir, one) = project("alpha");
    let rules = one.directory_for(Kind::Rule).unwrap();
    write_rule(&one, "git/no-force-push.md", "Never.\n");
    write_rule(&one, "team/style/naming.md", "Names.\n");
    // An empty folder is walked: a rule written into it is found.
    fs::create_dir_all(rules.join("empty")).unwrap();
    // A hidden one is not, so nothing in it is a rule.
    write_rule(&one, ".drafts/later.md", "Later.\n");

    let folders = Catalog::looked_in(&rules, Kind::Rule);

    assert_eq!(
        folders,
        [
            rules.clone(),
            rules.join("empty"),
            rules.join("git"),
            rules.join("team"),
            rules.join("team").join("style"),
        ]
    );
}

#[test]
fn discovery_looks_in_a_folder_that_is_not_there_and_nowhere_else() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing");

    for kind in Kind::ALL {
        assert_eq!(
            Catalog::looked_in(&missing, kind),
            std::slice::from_ref(&missing)
        );
    }
}

// ---------------------------------------------------------------------------
// References
// ---------------------------------------------------------------------------

#[test]
fn a_bare_name_finds_it_in_any_kind() {
    let (_dir, one) = project("alpha");
    write_skill(&one, "deploy", &skill_file("deploy"));
    write_rule(&one, "deploy.md", "Context.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let both = catalog.find(&Reference::parse("deploy"));
    assert_eq!(both.len(), 2);
    let just_the_rule = catalog.find(&Reference::parse("rule:deploy"));
    assert_eq!(just_the_rule.len(), 1);
    assert_eq!(just_the_rule[0].kind(), Kind::Rule);
    assert!(catalog.find(&Reference::parse("absent")).is_empty());
}

#[test]
fn a_qualifier_is_separated_by_a_colon_so_a_route_is_never_one() {
    // A rule's name IS a route, so `/` cannot also mean "kind". With `:`,
    // `skills/naming` is a rule filed under `skills/` and nothing else, and
    // a rules folder holding rules about writing skills is not exotic.
    for typed in ["git/no-force-push", "skills/naming", "rule/x"] {
        let bare = Reference::parse(typed);
        assert_eq!(bare.kind(), None, "{typed} was read as qualified");
        assert_eq!(bare.name(), typed);
        assert_eq!(bare.typed(), typed);
    }

    let qualified = Reference::parse("rule:git/no-force-push");
    assert_eq!(qualified.kind(), Some(Kind::Rule));
    assert_eq!(qualified.name(), "git/no-force-push");
    assert_eq!(qualified.typed(), "rule:git/no-force-push");
}

#[test]
fn a_rule_filed_under_a_kind_word_is_reachable_by_the_name_it_is_listed_under() {
    let (_dir, one) = project("alpha");
    write_rule(&one, "skills/naming.md", "How to name a skill.\n");
    write_rule(&one, "naming.md", "Something else entirely.\n");

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    // The name a listing prints has to be a name the tool accepts back, and
    // has to mean that artifact rather than a different one.
    let nested = catalog.find(&Reference::parse("skills/naming"));
    assert_eq!(nested.len(), 1);
    assert_eq!(nested[0].summary(), Some("How to name a skill."));
    let other = catalog.find(&Reference::parse("naming"));
    assert_eq!(other[0].summary(), Some("Something else entirely."));
}

#[test]
fn find_spans_the_projects_a_workspace_manages() {
    let (_dir, made) = projects(&["alpha", "beta"]);
    write_rule(&made[0], "shared.md", "One.\n");
    write_rule(&made[1], "shared.md", "The other.\n");

    let catalog = Catalog::discover(&made);

    let matches = catalog.find(&Reference::parse("shared"));
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].project_name(), Some("alpha"));
    assert_eq!(matches[1].project_name(), Some("beta"));
}

#[test]
fn the_shelf_is_read_source_by_source_and_found_as_it_is_now() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = mindweaver_core::WeaverWorkspace::init(dir.path()).unwrap();
    let shelf = workspace.gathered_dir(Kind::Skill);
    for (source, folder, contents) in [
        ("acme-skills", "deploy", skill_file("deploy")),
        ("other-rules", "a--b", skill_file("a--b")),
        ("other-rules", "broken", String::from("no front matter\n")),
    ] {
        let skill = shelf.join(source).join(folder);
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), contents).unwrap();
    }
    // A stray file is not a source.
    fs::write(shelf.join("notes.txt"), "not a source\n").unwrap();

    let catalog = Catalog::discover_gathered(&workspace, &Kind::ALL);

    let names: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|a| a.name().as_str())
        .collect();
    assert_eq!(names, ["a--b", "deploy"]);
    assert_eq!(catalog.failures().len(), 1, "{:?}", catalog.failures());
    let doubled = &catalog.artifacts()[0];
    assert_eq!(
        doubled.validate(),
        vec![ValidationIssue::NameNotKebabCase {
            segment: "a--b".into()
        }]
    );
    assert!(Catalog::discover_gathered(&workspace, &[Kind::Rule]).is_empty());
}

#[test]
fn a_workspace_that_never_gathered_has_an_empty_shelf_and_no_failure() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = mindweaver_core::WeaverWorkspace::init(dir.path()).unwrap();

    let catalog = Catalog::discover_gathered(&workspace, &Kind::ALL);

    assert!(catalog.is_empty());
    assert!(catalog.failures().is_empty());
}

// ---------------------------------------------------------------------------
// A project marked by its settings
// ---------------------------------------------------------------------------

/// A project called `name` under `parent`, marked by its settings, naming
/// `agents` and authoring skills in `skills`, if anywhere.
fn marked(parent: &TempDir, name: &str, agents: &[Agent], skills: Option<&str>) -> MindProject {
    let root = parent.path().join(name);
    fs::create_dir_all(&root).unwrap();
    let directories = skills.map_or_else(Directories::default, |skills| {
        Directories::default().with(Kind::Skill, skills)
    });
    MindProject::init_settings(&root, agents, Share::Shared, &directories)
        .unwrap()
        .0
}

/// Write `contents` as the SKILL.md of `folder`, a path below the project.
fn put(project: &MindProject, folder: &str, contents: &str) -> PathBuf {
    let dir = mindweaver_core::paths::joined(project.root(), folder);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("SKILL.md"), contents).unwrap();
    dir
}

#[test]
fn a_skill_in_both_agents_folders_is_listed_once_for_all_three_agents() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, Some("skills"));
    let shared = put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    let claudes = put(&one, ".claude/skills/deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert!(catalog.failures().is_empty(), "{:?}", catalog.failures());
    assert_eq!(catalog.artifacts().len(), 1);
    let deploy = &catalog.artifacts()[0];
    assert_eq!(deploy.name(), "deploy");
    assert_eq!(deploy.agents(), Agent::ALL);
    assert!(!deploy.copies_differ());
    let copies = deploy.copies();
    let paths: Vec<&Path> = copies.iter().map(|copy| copy.path()).collect();
    assert_eq!(paths, [shared.as_path(), claudes.as_path()]);
    assert_eq!(copies[0].agents(), [Agent::Codex, Agent::OpenCode]);
    assert_eq!(copies[1].agents(), [Agent::Claude, Agent::OpenCode]);
    // Nothing records it, and it is not where the project authors: somebody
    // keeps it there by hand.
    assert_eq!(deploy.provenance(), Provenance::Foreign);
}

#[test]
fn a_skill_only_in_claudes_folder_is_seen_by_claude_and_opencode() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, Some("skills"));
    put(&one, ".claude/skills/review", &skill_file("review"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let review = &catalog.artifacts()[0];
    assert_eq!(catalog.artifacts().len(), 1);
    // OpenCode reads Claude's folder as well as its own, though it is sent
    // its copies only in `.agents/skills`.
    assert_eq!(review.agents(), [Agent::Claude, Agent::OpenCode]);
    assert_eq!(review.copies().len(), 1);
}

#[test]
fn copies_that_differ_are_listed_once_and_said() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, None);
    let shared = put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    let claudes = put(
        &one,
        ".claude/skills/deploy",
        "---\nname: deploy\ndescription: Does it another way\n---\n",
    );

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert_eq!(catalog.artifacts().len(), 1);
    let deploy = &catalog.artifacts()[0];
    assert!(deploy.copies_differ());
    // Described by its first copy.
    assert_eq!(deploy.summary(), Some("Does deploy things"));
    assert_eq!(catalog.failures().len(), 1);
    let failure = &catalog.failures()[0];
    assert!(matches!(failure, DiscoveryFailure::Diverged { .. }));
    assert!(!failure.is_unreadable());
    assert_eq!(failure.path(), shared);
    assert_eq!(
        failure.to_string(),
        format!(
            "the copies of `deploy` at {} and {} differ, so the agents reading them do not all \
             load the same skill",
            shared.display(),
            claudes.display()
        )
    );
}

#[test]
fn each_copy_is_validated_in_its_own_folder() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, None);
    put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    put(
        &one,
        ".claude/skills/ship",
        "---\nname: deploy\ndescription: \"\"\n---\n",
    );

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let deploy = &catalog.artifacts()[0];
    let copies = deploy.copies();
    assert_eq!(copies[0].issues(), []);
    assert_eq!(
        copies[1].issues(),
        [
            ValidationIssue::NameDirectoryMismatch {
                name: "deploy".into(),
                directory: "ship".into()
            },
            ValidationIssue::DescriptionEmpty
        ]
    );
    // The artifact is judged on both, though it reads as its first.
    assert_eq!(deploy.validate(), copies[1].issues());
}

#[test]
fn an_authored_folder_that_is_an_agents_folder_is_read_once() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, Some(".claude/skills"));
    put(&one, ".claude/skills/deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert!(catalog.failures().is_empty(), "{:?}", catalog.failures());
    assert_eq!(catalog.artifacts().len(), 1);
    let deploy = &catalog.artifacts()[0];
    assert_eq!(deploy.copies().len(), 1);
    assert_eq!(deploy.provenance(), Provenance::Authored);
    assert_eq!(deploy.agents(), [Agent::Claude, Agent::OpenCode]);
    assert_eq!(
        Catalog::folders(&one, Kind::Skill),
        [
            one.root().join(".agents").join("skills"),
            one.root().join(".claude").join("skills")
        ]
    );
}

#[test]
fn a_skill_authored_where_no_agent_reads_is_one_more_copy_of_its_name() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, Some("skills"));
    let authored = put(
        &one,
        "skills/deploy",
        "---\nname: deploy\ndescription: Edited since it was installed\n---\n",
    );
    let claudes = put(&one, ".claude/skills/deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    // One entry, read as the project's own, which is what a change from the
    // project reaches. It reaches no agent until it is installed, so the
    // agents are those of the copy in Claude's folder, and the two are not
    // compared: a source edited since is no warning.
    assert!(catalog.failures().is_empty(), "{:?}", catalog.failures());
    assert_eq!(catalog.artifacts().len(), 1);
    let deploy = &catalog.artifacts()[0];
    assert_eq!(deploy.path(), authored);
    assert_eq!(deploy.summary(), Some("Edited since it was installed"));
    assert_eq!(deploy.agents(), [Agent::Claude, Agent::OpenCode]);
    assert!(!deploy.copies_differ());
    assert_eq!(deploy.provenance(), Provenance::Authored);
    let copies = deploy.copies();
    let found: Vec<(&Path, &[Agent], bool)> = copies
        .iter()
        .map(|copy| (copy.path(), copy.agents(), copy.is_authored()))
        .collect();
    assert_eq!(
        found,
        [
            (authored.as_path(), &[][..], true),
            (
                claudes.as_path(),
                &[Agent::Claude, Agent::OpenCode][..],
                false
            ),
        ]
    );
    assert_eq!(
        Catalog::folders(&one, Kind::Skill),
        [
            one.root().join(".agents").join("skills"),
            one.root().join(".claude").join("skills"),
            one.root().join("skills")
        ]
    );
}

#[test]
fn only_the_copies_an_agent_reads_are_compared_beside_the_projects_own() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, Some("skills"));
    let authored = put(&one, "skills/deploy", &skill_file("deploy"));
    let shared = put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    let claudes = put(
        &one,
        ".claude/skills/deploy",
        "---\nname: deploy\ndescription: Does it another way\n---\n",
    );

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert_eq!(catalog.artifacts().len(), 1);
    let deploy = &catalog.artifacts()[0];
    assert_eq!(deploy.path(), authored);
    assert!(deploy.copies_differ());
    assert_eq!(catalog.failures().len(), 1, "{:?}", catalog.failures());
    match &catalog.failures()[0] {
        DiscoveryFailure::Diverged { paths, .. } => assert_eq!(paths, &[shared, claudes]),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_project_is_read_only_in_the_folders_of_the_agents_it_names() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &[Agent::Codex], None);
    put(&one, ".claude/skills/review", &skill_file("review"));
    put(&one, ".agents/skills/deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let found: Vec<(&str, Vec<Agent>)> = catalog
        .artifacts()
        .iter()
        .map(|artifact| (artifact.name().as_str(), artifact.agents()))
        .collect();
    assert_eq!(found, [("deploy", vec![Agent::Codex])]);
}

#[test]
fn a_project_a_mind_toml_marks_is_read_only_where_its_marker_says() {
    let (_dir, one) = project("alpha");
    put(&one, ".claude/skills/review", &skill_file("review"));
    write_skill(&one, "deploy", &skill_file("deploy"));

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    let names: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|a| a.name().as_str())
        .collect();
    assert_eq!(names, ["deploy"]);
    assert_eq!(catalog.artifacts()[0].agents(), []);
    assert_eq!(
        Catalog::folders(&one, Kind::Skill),
        [one.directory_for(Kind::Skill).unwrap()]
    );
}

#[test]
fn what_a_project_authors_is_found_apart_from_its_agents_folders() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, Some("skills"));
    let authored = put(&one, "skills/deploy", &skill_file("deploy"));
    put(&one, ".claude/skills/deploy", &skill_file("deploy"));
    put(&one, ".agents/skills/review", &skill_file("review"));

    let catalog = Catalog::discover_authored(std::slice::from_ref(&one), &Kind::ALL);

    let paths: Vec<&Path> = catalog.artifacts().iter().map(|a| a.path()).collect();
    assert_eq!(paths, [authored.as_path()]);
}

#[cfg(unix)]
#[test]
fn one_folder_reached_through_a_link_is_one_copy() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, None);
    put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    fs::create_dir_all(one.root().join(".claude/skills")).unwrap();
    std::os::unix::fs::symlink(
        "../../.agents/skills/deploy",
        one.root().join(".claude/skills/deploy"),
    )
    .unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert!(catalog.failures().is_empty(), "{:?}", catalog.failures());
    let deploy = &catalog.artifacts()[0];
    assert_eq!(deploy.copies().len(), 2);
    assert!(!deploy.copies_differ());
    assert_eq!(deploy.agents(), Agent::ALL);
}

#[cfg(unix)]
#[test]
fn two_agents_folders_that_are_one_on_disk_are_read_once() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, None);
    let shared = put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    put(&one, ".agents/skills/broken", "no front matter at all\n");
    fs::create_dir_all(one.root().join(".claude")).unwrap();
    std::os::unix::fs::symlink("../.agents/skills", one.root().join(".claude/skills")).unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    // Each skill there once, and each failure there said once.
    assert_eq!(catalog.artifacts().len(), 1);
    let deploy = &catalog.artifacts()[0];
    assert_eq!(deploy.path(), shared);
    assert_eq!(deploy.copies().len(), 1);
    assert_eq!(deploy.agents(), Agent::ALL);
    assert_eq!(catalog.failures().len(), 1, "{:?}", catalog.failures());
    assert_eq!(
        Catalog::folders(&one, Kind::Skill),
        [one.root().join(".agents").join("skills")]
    );
}

#[cfg(unix)]
#[test]
fn a_copy_holding_a_link_is_said_rather_than_followed_to_compare_it() {
    let parent = TempDir::new().unwrap();
    let one = marked(&parent, "alpha", &Agent::ALL, None);
    put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    let claudes = put(&one, ".claude/skills/deploy", &skill_file("deploy"));
    std::os::unix::fs::symlink("/dev/zero", claudes.join("endless")).unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&one));

    assert_eq!(catalog.artifacts().len(), 1);
    assert!(!catalog.artifacts()[0].copies_differ());
    assert_eq!(catalog.failures().len(), 1, "{:?}", catalog.failures());
    let failure = &catalog.failures()[0];
    assert!(matches!(failure, DiscoveryFailure::Uncompared { .. }));
    assert!(failure.is_unreadable());
    assert_eq!(failure.path(), claudes);
}

/// A workspace in a temporary folder, linking `alpha` beside it, a project
/// marked by its settings naming every agent and authoring skills in
/// `skills`, if anywhere.
fn linked(skills: Option<&str>) -> (TempDir, WeaverWorkspace, MindProject) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let project = marked(&dir, "alpha", &Agent::ALL, skills);
    workspace.link(&project).unwrap();
    (dir, workspace, project)
}

/// Record in `ledger` that `name` was installed into `alpha`, the route the
/// workspace stores it by, with a copy at each of `paths`.
fn record(ledger: &Ledger, name: &str, paths: &[&str]) {
    ledger
        .installed(
            "alpha",
            &InstallRecord {
                kind: Kind::Skill,
                name: name.to_owned().into(),
                source_id: None,
                origin: Some("workspace".into()),
                content_hash: None,
                targets: paths
                    .iter()
                    .map(|path| Target {
                        agent: None,
                        path: (*path).to_owned(),
                        content_hash: None,
                    })
                    .collect(),
                entries: Vec::new(),
            },
        )
        .unwrap();
}

#[test]
fn a_skill_where_the_project_authors_for_an_agent_is_installed_or_authored_as_the_ledger_says() {
    let (_dir, workspace, one) = linked(Some(".claude/skills"));
    put(&one, ".claude/skills/audit", &skill_file("audit"));
    put(&one, ".claude/skills/deploy", &skill_file("deploy"));
    let ledger = workspace.ledger().unwrap();
    record(&ledger, "deploy", &[".claude/skills/deploy"]);

    let catalog = Catalog::discover(std::slice::from_ref(&one)).with_records(&workspace, &ledger);

    assert!(catalog.failures().is_empty(), "{:?}", catalog.failures());
    let found: Vec<(&str, usize, Provenance)> = catalog
        .artifacts()
        .iter()
        .map(|artifact| {
            (
                artifact.name().as_str(),
                artifact.copies().len(),
                artifact.provenance(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            ("audit", 1, Provenance::Authored),
            ("deploy", 1, Provenance::Installed)
        ]
    );
    // Without the ledger, nothing is anybody's but the project's.
    let unrecorded = Catalog::discover(std::slice::from_ref(&one));
    assert_eq!(unrecorded.artifacts()[1].provenance(), Provenance::Authored);
}

#[test]
fn a_copy_in_an_agents_folder_is_installed_only_at_a_path_the_ledger_records() {
    let (_dir, workspace, one) = linked(Some("skills"));
    put(&one, ".agents/skills/deploy", &skill_file("deploy"));
    put(&one, ".claude/skills/deploy", &skill_file("deploy"));
    put(&one, ".agents/skills/review", &skill_file("review"));
    put(&one, ".agents/skills/triage", &skill_file("triage"));
    put(&one, ".claude/skills/triage", &skill_file("triage"));
    let ledger = workspace.ledger().unwrap();
    record(
        &ledger,
        "deploy",
        &[".agents/skills/deploy", ".claude/skills/deploy"],
    );
    // Installed under that name, somewhere else: the copy here is still
    // somebody's.
    record(&ledger, "review", &["skills/review"]);
    record(&ledger, "triage", &[".claude/skills/triage"]);

    let catalog = Catalog::discover(std::slice::from_ref(&one)).with_records(&workspace, &ledger);

    let found: Vec<(&str, Provenance)> = catalog
        .artifacts()
        .iter()
        .map(|artifact| (artifact.name().as_str(), artifact.provenance()))
        .collect();
    assert_eq!(
        found,
        [
            ("deploy", Provenance::Installed),
            ("review", Provenance::Foreign),
            ("triage", Provenance::Foreign)
        ]
    );
    let triage: Vec<Provenance> = catalog.artifacts()[2]
        .copies()
        .iter()
        .map(ArtifactCopy::provenance)
        .collect();
    assert_eq!(triage, [Provenance::Foreign, Provenance::Installed]);
}

#[test]
fn a_project_the_workspace_does_not_link_has_no_record_in_its_ledger() {
    let (dir, workspace, _) = linked(None);
    let other = marked(&dir, "beta", &Agent::ALL, None);
    put(&other, ".claude/skills/deploy", &skill_file("deploy"));
    let ledger = workspace.ledger().unwrap();
    ledger
        .installed(
            "beta",
            &InstallRecord {
                kind: Kind::Skill,
                name: "deploy".into(),
                source_id: None,
                origin: None,
                content_hash: None,
                targets: vec![Target {
                    agent: None,
                    path: String::from(".claude/skills/deploy"),
                    content_hash: None,
                }],
                entries: Vec::new(),
            },
        )
        .unwrap();

    let catalog = Catalog::discover(std::slice::from_ref(&other)).with_records(&workspace, &ledger);

    assert_eq!(catalog.artifacts()[0].provenance(), Provenance::Foreign);
}

/// What a reference serializes to is a contract: what was typed, and the two
/// parts read from it, the kind as a command line takes it.
#[test]
fn a_reference_serializes_what_was_typed_and_what_it_names() {
    let qualified = serde_json::to_string(&Reference::parse("rule:git/no-force")).unwrap();
    let bare = serde_json::to_string(&Reference::parse("deploy")).unwrap();

    assert_eq!(
        qualified,
        r#"{"kind":"rule","name":"git/no-force","typed":"rule:git/no-force"}"#
    );
    assert_eq!(bare, r#"{"kind":null,"name":"deploy","typed":"deploy"}"#);
}
