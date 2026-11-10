//! Gathering rules from a folder of a repository (#111): every markdown
//! file under it a rule, named by its route from there as a project's are,
//! but the README and the licence at its top; a route that is no name said
//! and not taken; each put on the shelf by source, recorded with what a
//! gather keeps of what it took, read back by discovery, and a dry run,
//! which writes nothing.

mod common;

use std::fs;

use common::{commit, repository, skill, url, workspace, write_files};
use mindweaver_core::gather::{self, Failure, GatherError, Request};
use mindweaver_core::mother::{random, GitRepositoryMother, RuleMother};
use mindweaver_core::{hash, Catalog, Kind, Licence, WeaverWorkspace};

/// A gather of the rules in `folder` of `source`.
fn rules_in(source: &tempfile::TempDir, folder: &str) -> Request {
    Request::git(url(source))
        .from_subdirectory(folder)
        .taking(Kind::Rule)
}

/// The names a list of what a report says, in order.
fn names(harvested: &[gather::Harvested]) -> Vec<String> {
    harvested.iter().map(|one| one.name.to_string()).collect()
}

/// Every file on the rules shelf of the source filed as `directory`, by its
/// route from there.
fn shelved(workspace: &WeaverWorkspace, directory: &str) -> Vec<String> {
    let shelf = workspace.gathered_dir(Kind::Rule).join(directory);
    let mut found = Vec::new();
    let mut folders = vec![shelf.clone()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
            } else {
                let route = path.strip_prefix(&shelf).unwrap();
                found.push(route.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    found.sort();
    found
}

/// A rule's route of a few kebab-case segments, as a collection files one.
fn route() -> String {
    (0..random::between(1, 3))
        .map(|_| random::kebab())
        .collect::<Vec<_>>()
        .join("/")
}

// ---------------------------------------------------------------------------
// What a gather of rules takes
// ---------------------------------------------------------------------------

#[test]
fn every_markdown_file_in_the_folder_named_is_a_rule_named_by_its_route() {
    let mut routes: Vec<String> = (0..random::between(1, 5)).map(|_| route()).collect();
    routes.sort();
    routes.dedup();
    let borrowed: Vec<&str> = routes.iter().map(String::as_str).collect();
    let source = GitRepositoryMother::with_rules_in("rules", &borrowed);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    assert_eq!(report.kind, Kind::Rule);
    assert_eq!(names(&report.added), routes);
    for (harvested, route) in report.added.iter().zip(&routes) {
        assert_eq!(harvested.route, format!("rules/{route}.md"));
        assert_eq!(harvested.directory, format!("{route}.md"));
        let topic = route.rsplit('/').next().unwrap();
        assert_eq!(harvested.summary.as_deref(), Some(topic));
    }
    let filed: Vec<String> = routes.iter().map(|route| format!("{route}.md")).collect();
    assert_eq!(shelved(&workspace, &report.directory), filed);
}

/// What the ledger keeps of each, as of a skill: where it is in the
/// repository, a hash by the recipe an install keeps of a rule's copy, its
/// bytes, and the licence of the folder it was gathered from, which no
/// rule is.
#[test]
fn each_rule_gathered_is_recorded_with_its_route_its_hash_and_its_folder_s_licence() {
    let source = GitRepositoryMother::with_rules_in("rules", &["git/commits"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:?}");
    let rule = &gathered[0];
    assert_eq!(rule.kind, Kind::Rule);
    assert_eq!(rule.name.as_str(), "git/commits");
    assert_eq!(
        rule.path,
        format!(".mindweaver/rules/{}/git/commits.md", report.directory)
    );
    assert_eq!(rule.upstream_path.as_deref(), Some("rules/git/commits.md"));
    let bytes = fs::read(source.path().join("rules/git/commits.md")).unwrap();
    assert_eq!(rule.content_hash, Some(hash::bytes(&bytes)));
    assert_eq!(
        rule.licence,
        Some(Licence::File {
            file: String::from("LICENSE"),
            text: Some(String::from("MIT License")),
        })
    );
    assert_eq!(rule.source.subdirectory, "rules");
}

#[test]
fn the_readme_and_the_licence_at_the_top_of_the_folder_are_no_rules() {
    let source = repository(&[
        ("rules/README.md", "# The rules\n"),
        ("rules/readme.markdown", "# Not markdown by its extension\n"),
        ("rules/LICENSE.md", "MIT License\n"),
        ("rules/style.md", &RuleMother::create("style")),
        ("rules/docs/readme.md", &RuleMother::create("readme")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    assert_eq!(names(&report.added), ["docs/readme", "style"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(
        shelved(&workspace, &report.directory),
        ["docs/readme.md", "style.md"]
    );
}

/// A route that is no rule's name is not taken: no install could file it,
/// and its name is where the shelf files it. Said by where it is, beside
/// the rules that were taken.
#[test]
fn a_route_that_is_no_name_is_said_by_where_it_is_and_not_taken() {
    let source = repository(&[
        ("rules/style.md", &RuleMother::create("style")),
        ("rules/Python/style.md", &RuleMother::create("python")),
        ("rules/go_lang.md", &RuleMother::create("go")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    assert_eq!(names(&report.added), ["style"]);
    let said: Vec<(String, &str)> = report
        .failures
        .iter()
        .map(|failure| (failure.to_string(), failure.code()))
        .collect();
    assert_eq!(
        said,
        [
            (
                String::from(
                    "rules/Python/style.md: not taken, since a rule is named by its route, and \
                     `Python` is not usable as a name: it has to be lowercase letters and \
                     digits, joined by single hyphens"
                ),
                "gathered_route_unusable"
            ),
            (
                String::from(
                    "rules/go_lang.md: not taken, since a rule is named by its route, and \
                     `go_lang` is not usable as a name: it has to be lowercase letters and \
                     digits, joined by single hyphens"
                ),
                "gathered_route_unusable"
            ),
        ]
    );
    assert!(matches!(report.failures[0], Failure::Unnamed { .. }));
    assert_eq!(shelved(&workspace, &report.directory), ["style.md"]);
    let gathered = ledger.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:?}");
}

/// A rule with nothing to say, or front matter that does not read, is
/// taken, said, as a skill breaking the format is: `validate` says so again.
#[test]
fn a_rule_breaking_the_format_is_gathered_with_a_warning() {
    let source = repository(&[
        ("rules/empty.md", "\n\n"),
        ("rules/style.md", &RuleMother::create("style")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    assert_eq!(names(&report.added), ["empty", "style"]);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    assert_eq!(
        report.warnings[0].path,
        workspace
            .gathered_dir(Kind::Rule)
            .join(&report.directory)
            .join("empty.md")
    );
    assert_eq!(report.warnings[0].issue.code(), "content_empty");
}

#[test]
fn the_whole_repository_is_read_as_rules_when_it_is_named() {
    let source = repository(&[
        ("README.md", "# Our rules\n"),
        ("LICENSE", "MIT License\n"),
        ("style.md", &RuleMother::create("style")),
        ("git/commits.md", &RuleMother::create("commits")),
        (".github/pull_request_template.md", "# Hidden\n"),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let report = gather::gather(&workspace, &ledger, &rules_in(&source, ".")).unwrap();

    assert_eq!(names(&report.added), ["git/commits", "style"]);
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let routes: Vec<&str> = report.added.iter().map(|one| one.route.as_str()).collect();
    assert_eq!(routes, ["git/commits.md", "style.md"]);
}

#[test]
fn a_folder_holding_no_rule_is_refused_saying_where_it_looked() {
    let source = repository(&[
        ("rules/README.md", "# Nothing yet\n"),
        ("rules/notes.txt", "not markdown\n"),
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let refused = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap_err();

    assert!(
        matches!(&refused, GatherError::NothingFound { kind: Kind::Rule, path, .. } if path == "rules"),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "gather_nothing_found");
    assert_eq!(
        refused.to_string(),
        format!(
            "{}: no rule found in `rules`: looked for a markdown file (`.md`) at any depth \
             there, but its README and its licence",
            url(&source)
        )
    );
    assert!(ledger.gathered().unwrap().is_empty());
}

#[test]
fn gathering_rules_again_says_what_moved() {
    let source = GitRepositoryMother::with_rules_in("rules", &["style", "tone"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();
    write_files(
        source.path(),
        &[("rules/tone.md", "# tone\n\nSay what a change does first.\n")],
    );
    commit(source.path());

    let again = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    assert_eq!(names(&again.updated), ["tone"]);
    assert_eq!(names(&again.unchanged), ["style"]);
    assert!(again.added.is_empty());
    let shelf = workspace.gathered_dir(Kind::Rule).join(&again.directory);
    assert_eq!(
        fs::read_to_string(shelf.join("tone.md")).unwrap(),
        "# tone\n\nSay what a change does first.\n"
    );
}

/// The shelf's rules are read where the gather put them, by the same names,
/// as `validate` reads them.
#[test]
fn discovery_reads_the_rules_on_the_shelf_by_the_names_a_gather_gave_them() {
    let source = GitRepositoryMother::with_rules_in("rules", &["git/commits", "tone"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    let catalog = Catalog::discover_gathered(&workspace, &[Kind::Rule]);

    let found: Vec<(Kind, &str)> = catalog
        .artifacts()
        .iter()
        .map(|artifact| (artifact.kind(), artifact.name().as_str()))
        .collect();
    assert_eq!(found, [(Kind::Rule, "git/commits"), (Kind::Rule, "tone")]);
    assert!(catalog.failures().is_empty(), "{:?}", catalog.failures());
    assert!(catalog
        .artifacts()
        .iter()
        .all(|artifact| artifact.validate().is_empty()));
    assert!(Catalog::discover_gathered(&workspace, &[Kind::Skill]).is_empty());
}

/// A source of rules is a source like any other: its own folder on the
/// rules shelf, which a gather of skills from another folder of the same
/// repository never shares.
#[test]
fn rules_and_skills_of_one_repository_are_two_sources_filed_apart() {
    let source = repository(&[
        ("rules/style.md", &RuleMother::create("style")),
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let rules = gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();
    let skills = gather::gather(&workspace, &ledger, &Request::git(url(&source))).unwrap();

    assert_eq!(names(&rules.added), ["style"]);
    assert_eq!(names(&skills.added), ["deploy"]);
    assert_eq!(ledger.sources().unwrap().len(), 2);
    assert!(workspace
        .gathered_dir(Kind::Rule)
        .join(&rules.directory)
        .join("style.md")
        .is_file());
    assert!(workspace
        .gathered_dir(Kind::Skill)
        .join(&skills.directory)
        .join("deploy/SKILL.md")
        .is_file());
    assert!(!workspace
        .gathered_dir(Kind::Skill)
        .join(&rules.directory)
        .exists());
}

/// A gather of rules from the root is not taken for the gather an older
/// build made of `skills/`, which a gather of skills naming no folder is
/// (#101): it names its folder, and is a source of its own.
#[test]
fn a_gather_of_rules_from_the_root_leaves_an_older_build_s_source_of_skills_alone() {
    let source = repository(&[
        ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
        ("style.md", &RuleMother::create("style")),
    ]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let older = Request::git(url(&source)).from_subdirectory("skills");
    gather::gather(&workspace, &ledger, &older).unwrap();

    gather::gather(&workspace, &ledger, &rules_in(&source, ".")).unwrap();

    let mut folders: Vec<String> = ledger
        .sources()
        .unwrap()
        .into_iter()
        .map(|source| source.subdirectory)
        .collect();
    folders.sort();
    assert_eq!(folders, [".", "skills"]);
}

// ---------------------------------------------------------------------------
// A dry run of rules
// ---------------------------------------------------------------------------

#[test]
fn a_dry_run_of_rules_says_what_would_be_taken_and_writes_nothing() {
    let source = GitRepositoryMother::with_rules_in("rules", &["git/commits", "tone"]);
    let (_dir, workspace) = workspace();

    let report = gather::dry_run(
        &workspace,
        &workspace.ledger_for_reading().unwrap(),
        &rules_in(&source, "rules"),
    )
    .unwrap();

    assert_eq!(names(&report.added), ["git/commits", "tone"]);
    let routes: Vec<&str> = report.added.iter().map(|one| one.route.as_str()).collect();
    assert_eq!(routes, ["rules/git/commits.md", "rules/tone.md"]);
    assert_eq!(report.kind, Kind::Rule);
    assert!(!workspace.gathered_dir(Kind::Rule).exists());
    assert!(!workspace.ledger_path().exists());
    assert!(!workspace.cache_dir().exists());
}

#[test]
fn a_dry_run_after_a_gather_of_rules_says_what_would_move() {
    let source = GitRepositoryMother::with_rules_in("rules", &["style", "tone"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    gather::gather(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();
    write_files(source.path(), &[("rules/tone.md", "# tone\n\nChanged.\n")]);
    commit(source.path());
    let tone = workspace.root().join(&ledger.gathered().unwrap()[1].path);
    let before = fs::read(&tone).unwrap();

    let report = gather::dry_run(&workspace, &ledger, &rules_in(&source, "rules")).unwrap();

    assert_eq!(names(&report.updated), ["tone"]);
    assert_eq!(names(&report.unchanged), ["style"]);
    assert_eq!(fs::read(&tone).unwrap(), before, "the shelf is as it was");
}
