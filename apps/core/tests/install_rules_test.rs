//! Installing rules: offered by the route they are filed under, rendered
//! into a project marked by its settings, a section of the block in its
//! `AGENTS.md` and Claude's files as Claude reads them, recorded in the
//! ledger; copied into a project a `mind.toml` marks; and never over what
//! somebody wrote.

mod common;

use std::fs;
use std::path::PathBuf;

use mindweaver_core::agents_md::{self, Section};
use mindweaver_core::claude_md::{self, Fallback, Reach};
use mindweaver_core::claude_rules::{self, Warning as FileWarning};
use mindweaver_core::code::Said;
use mindweaver_core::install::rules::{self, Warning, Withheld};
use mindweaver_core::install::{
    self, Candidate, Edits, InstallError, Installation, Installed, Removed, Standing,
    WORKSPACE_ORIGIN,
};
use mindweaver_core::ledger::{InstallRecord, Target};
use mindweaver_core::lifecycle;
use mindweaver_core::migrate;
use mindweaver_core::mother::{RuleMother, ShelfMother};
use mindweaver_core::value::ContentHash;
use mindweaver_core::{
    hash, Agent, AgentHomes, Directories, Kind, Ledger, LifecycleError, MindProject, Missed,
    Reference, Share, State, Status, WeaverWorkspace,
};
use tempfile::TempDir;

/// A rule about every file.
const NO_FORCE_PUSH: &str = "Never force-push a branch somebody else may have pulled.\n";
/// Another, with front matter of its own.
const TONE: &str = "---\ndescription: How a review reads\n---\n\nSay what a change does before what is wrong with it.\n";

/// The own rule of `workspace` at `route`, holding `source`.
fn own_rule(workspace: &WeaverWorkspace, route: &str, source: &str) -> PathBuf {
    let file = workspace
        .own()
        .directory_for(Kind::Rule)
        .unwrap()
        .join(format!("{route}.md"));
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, source).unwrap();
    file
}

/// A workspace linking a project a `mind.toml` marks, which keeps its rules
/// in `rules`.
fn older() -> (TempDir, WeaverWorkspace, MindProject, Ledger) {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("app")).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    (dir, workspace, project, ledger)
}

fn copy_of(project: &MindProject, route: &str) -> PathBuf {
    let mut path = project.directory_for(Kind::Rule).unwrap();
    path.extend(route.split('/'));
    path.with_extension("md")
}

fn try_put(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Result<Installed, InstallError> {
    let candidate = install::offered(workspace, ledger, project, Kind::Rule, route, None)?;
    install::install(
        workspace,
        ledger,
        project,
        &candidate,
        &AgentHomes::default(),
    )
    .map(|done| done.installed)
}

fn put(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Installed {
    try_put(workspace, ledger, project, route).unwrap()
}

fn take(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    project: &MindProject,
    route: &str,
) -> Result<Removed, InstallError> {
    install::uninstall(
        workspace,
        ledger,
        project,
        Kind::Rule,
        route,
        &AgentHomes::default(),
    )
}

#[test]
fn a_project_a_mind_toml_marks_gets_a_copy_of_the_rule_filed_by_its_route() {
    let (_dir, workspace, project, ledger) = older();
    let file = own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    let copy = copy_of(&project, "git/no-force-push");

    let added = put(&workspace, &ledger, &project, "git/no-force-push");

    assert_eq!(
        added,
        Installed::Added {
            name: "git/no-force-push".into(),
            paths: vec![copy.clone()],
            unhashed: None,
        }
    );
    assert_eq!(fs::read_to_string(&copy).unwrap(), NO_FORCE_PUSH);
    assert!(!project.root().join("AGENTS.md").exists());
    let held = hash::bytes(NO_FORCE_PUSH.as_bytes());
    assert_eq!(
        ledger.records("app", Kind::Rule).unwrap(),
        [InstallRecord {
            kind: Kind::Rule,
            name: "git/no-force-push".into(),
            source_id: None,
            origin: Some(WORKSPACE_ORIGIN.into()),
            content_hash: Some(held.clone()),
            targets: vec![Target {
                agent: None,
                path: String::from("rules/git/no-force-push.md"),
                content_hash: Some(held),
            }],
            entries: Vec::new(),
        }]
    );
    assert!(matches!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Unchanged { .. }
    ));
    fs::write(&file, "Never force-push to main.\n").unwrap();
    assert!(matches!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Updated { .. }
    ));
    assert_eq!(
        fs::read_to_string(&copy).unwrap(),
        "Never force-push to main.\n"
    );

    let removed = take(&workspace, &ledger, &project, "git/no-force-push").unwrap();

    assert!(
        matches!(&removed, Removed::Removed { paths, still_excluded: None, .. } if *paths == [copy.clone()]),
        "{removed:?}"
    );
    assert!(!copy.exists());
    // The folder its route was filed in goes with it, and the folder of
    // rules stays.
    assert!(!copy.parent().unwrap().exists());
    assert!(project.directory_for(Kind::Rule).unwrap().is_dir());
    assert!(ledger.records("app", Kind::Rule).unwrap().is_empty());
}

#[test]
fn a_file_at_a_rules_place_nothing_recorded_is_somebodys_in_a_project_a_mind_toml_marks() {
    let (_dir, workspace, project, ledger) = older();
    own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    let copy = copy_of(&project, "git/no-force-push");
    fs::create_dir_all(copy.parent().unwrap()).unwrap();
    fs::write(&copy, "Our own rule.\n").unwrap();

    let surveyed = install::survey(&workspace, &ledger, &project, Kind::Rule).unwrap();
    assert_eq!(surveyed.len(), 1);
    assert_eq!(surveyed[0].standing, Standing::Foreign);

    assert_eq!(
        put(&workspace, &ledger, &project, "git/no-force-push"),
        Installed::Foreign {
            name: "git/no-force-push".into(),
            path: copy.clone(),
        }
    );
    assert_eq!(
        take(&workspace, &ledger, &project, "git/no-force-push").unwrap(),
        Removed::Foreign {
            name: "git/no-force-push".into(),
            path: copy.clone(),
        }
    );
    assert_eq!(fs::read_to_string(&copy).unwrap(), "Our own rule.\n");
    assert!(ledger.records("app", Kind::Rule).unwrap().is_empty());
}

/// A project a `mind.toml` marks, given two rules as copies, one of them
/// changed by hand since, then moved to settings naming every agent: the
/// project as it is then, the untouched copy, and the changed one.
fn moved_to_settings() -> (
    TempDir,
    WeaverWorkspace,
    MindProject,
    Ledger,
    PathBuf,
    PathBuf,
) {
    let (dir, workspace, project, ledger) = older();
    own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    own_rule(&workspace, "review/tone", TONE);
    put(&workspace, &ledger, &project, "git/no-force-push");
    put(&workspace, &ledger, &project, "review/tone");
    let untouched = copy_of(&project, "git/no-force-push");
    let changed = copy_of(&project, "review/tone");
    fs::write(&changed, "Ours now.\n").unwrap();

    migrate::project(
        Some(&workspace),
        &project,
        &Agent::ALL,
        Share::Shared,
        &AgentHomes::default(),
    )
    .unwrap()
    .unwrap();
    let project = MindProject::open(project.root()).unwrap();
    // A move to settings leaves a rule's copy, and its record, to the next
    // render of the project's rules.
    assert!(untouched.is_file());
    assert_eq!(
        ledger.records("app", Kind::Rule).unwrap()[0].targets[0].path,
        "rules/git/no-force-push.md"
    );
    (dir, workspace, project, ledger, untouched, changed)
}

#[test]
fn a_rules_copy_from_before_settings_goes_with_the_next_render_unless_changed() {
    let (_dir, workspace, project, ledger, untouched, changed) = moved_to_settings();

    let rendered = rules::render(&workspace, &ledger, &project, &AgentHomes::default())
        .unwrap()
        .unwrap();

    assert!(!untouched.exists());
    // The folder its route was filed in, emptied, goes with it.
    assert!(!untouched.parent().unwrap().exists());
    assert_eq!(fs::read_to_string(&changed).unwrap(), "Ours now.\n");
    assert!(
        rendered.warnings.contains(&Warning::Edited {
            route: String::from("review/tone"),
            path: changed.clone(),
        }),
        "{:?}",
        rendered.warnings
    );
    let text = fs::read_to_string(project.root().join("AGENTS.md")).unwrap();
    assert!(text.contains("<!-- mindweaver:rule git/no-force-push -->"));
    assert!(text.contains("<!-- mindweaver:rule review/tone -->"));
    // Each record names only what the render wrote, the changed copy no
    // longer among them: it is the project's own.
    for record in ledger.records("app", Kind::Rule).unwrap() {
        assert!(
            record
                .targets
                .iter()
                .all(|target| !target.path.starts_with("rules/")),
            "{record:?}"
        );
    }
}

#[test]
fn a_rules_copy_from_before_settings_goes_with_an_uninstall_before_any_render() {
    let (_dir, workspace, project, ledger, untouched, _) = moved_to_settings();

    let removed = take(&workspace, &ledger, &project, "git/no-force-push").unwrap();

    assert!(
        matches!(&removed, Removed::Removed { paths, .. } if *paths == [untouched.clone()]),
        "{removed:?}"
    );
    assert!(!untouched.exists());
    assert_eq!(
        ledger.installations("app", Kind::Rule).unwrap(),
        ["review/tone"]
    );
}

#[test]
fn a_rules_copy_changed_before_settings_refuses_neither_an_install_nor_an_uninstall() {
    let (_dir, workspace, project, ledger, _, changed) = moved_to_settings();
    // The render leaves it, the project's own, forced or not, so it is no
    // edit of the installation for a change to refuse.
    assert!(
        install::standings(&workspace, &ledger, &project, Kind::Rule)
            .unwrap()
            .iter()
            .all(|(_, standing)| *standing != Standing::Edited),
        "{:?}",
        install::standings(&workspace, &ledger, &project, Kind::Rule)
    );

    let candidate = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "review/tone",
        None,
    )
    .unwrap();
    let done = install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(
        !matches!(done.installed, Installed::Edited { .. }),
        "{:?}",
        done.installed
    );
    assert!(
        done.rendered.unwrap().warnings.contains(&Warning::Edited {
            route: String::from("review/tone"),
            path: changed.clone(),
        }),
        "said as left"
    );
    assert_eq!(fs::read_to_string(&changed).unwrap(), "Ours now.\n");
    let removed = take(&workspace, &ledger, &project, "review/tone").unwrap();
    assert!(matches!(removed, Removed::Removed { .. }), "{removed:?}");
    assert_eq!(fs::read_to_string(&changed).unwrap(), "Ours now.\n");
}

#[test]
fn an_uninstall_before_any_render_leaves_a_rules_copy_changed_before_settings() {
    let (_dir, workspace, project, ledger, _, changed) = moved_to_settings();

    let removed = take(&workspace, &ledger, &project, "review/tone").unwrap();

    assert!(matches!(removed, Removed::Removed { .. }), "{removed:?}");
    assert_eq!(fs::read_to_string(&changed).unwrap(), "Ours now.\n");
    assert_eq!(
        ledger.installations("app", Kind::Rule).unwrap(),
        ["git/no-force-push"]
    );
}

#[test]
fn a_rule_from_a_loaded_workspace_records_where_it_came_from() {
    let (dir, mut workspace, project, ledger) = older();
    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
    own_rule(&team, "review/tone", TONE);
    workspace.load(&team).unwrap();

    put(&workspace, &ledger, &project, "review/tone");

    assert_eq!(
        ledger.records("app", Kind::Rule).unwrap()[0]
            .origin
            .as_deref(),
        Some("load:team")
    );
    assert_eq!(
        fs::read_to_string(copy_of(&project, "review/tone")).unwrap(),
        TONE
    );
}

#[test]
fn two_places_offering_one_route_are_told_apart_with_from() {
    let (dir, mut workspace, project, ledger) = older();
    own_rule(&workspace, "review/tone", TONE);
    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
    own_rule(&team, "review/tone", "Be kind.\n");
    workspace.load(&team).unwrap();

    let twice = try_put(&workspace, &ledger, &project, "review/tone").unwrap_err();

    assert!(
        matches!(&twice, InstallError::OfferedTwice { origins, .. }
            if *origins == [String::from(WORKSPACE_ORIGIN), String::from("load:team")]),
        "{twice:?}"
    );
    let candidate = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "review/tone",
        Some("load:team"),
    )
    .unwrap();
    install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(copy_of(&project, "review/tone")).unwrap(),
        "Be kind.\n"
    );
}

#[cfg(unix)]
#[test]
fn a_rule_linked_out_of_the_workspace_that_offers_it_is_not_installed() {
    let (_dir, workspace, project, ledger) = older();
    let outside = TempDir::new().unwrap();
    let secret = outside.path().join("secret.md");
    fs::write(&secret, "token=hunter2\n").unwrap();
    let file = own_rule(&workspace, "leak", "placeholder\n");
    fs::remove_file(&file).unwrap();
    std::os::unix::fs::symlink(&secret, &file).unwrap();

    let refused = try_put(&workspace, &ledger, &project, "leak").unwrap_err();

    assert!(
        matches!(refused, InstallError::Outside { .. }),
        "{refused:?}"
    );
    assert!(!copy_of(&project, "leak").exists());
}

#[test]
fn a_route_that_is_no_name_is_neither_installed_nor_removed() {
    let (_dir, workspace, project, ledger) = older();

    let refused = take(&workspace, &ledger, &project, "../escape").unwrap_err();

    assert!(
        matches!(refused, InstallError::UnsafeRoute { .. }),
        "{refused:?}"
    );
    assert!(install::is_installable_name(
        Kind::Rule,
        "git/no-force-push"
    ));
    assert!(!install::is_installable_name(
        Kind::Skill,
        "git/no-force-push"
    ));
    assert!(!install::is_installable_name(Kind::Rule, "Git Notes"));
}

/// What a team's `AGENTS.md` might hold before Mindweaver writes in it.
const TEAM: &str = "# Working here\n\nRun `make test` before every push.\n";

/// A rule about Rust files only.
const SCOPED: &str =
    "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors; never panic in a library.\n";

/// A workspace in a git work tree, linking one project below it whose
/// settings name `agents` and share as `share`, with rules of its own to
/// install.
struct Fixture {
    dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
    ledger: Ledger,
}

fn fixture(share: Share, agents: &[Agent]) -> Fixture {
    let dir = TempDir::new().unwrap();
    gix::init(dir.path()).unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, agents, share, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    Fixture {
        dir,
        workspace,
        project,
        ledger,
    }
}

impl Fixture {
    /// The workspace's own rule at `route`, holding `source`.
    fn rule(&self, route: &str, source: &str) -> PathBuf {
        own_rule(&self.workspace, route, source)
    }

    fn install(&self, route: &str) -> Installation {
        self.try_install(route).unwrap()
    }

    fn try_install(&self, route: &str) -> Result<Installation, InstallError> {
        let candidate = install::offered(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Rule,
            route,
            None,
        )?;
        install::install(
            &self.workspace,
            &self.ledger,
            &self.project,
            &candidate,
            &AgentHomes::default(),
        )
    }

    fn uninstall(&self, route: &str) -> Removed {
        self.try_uninstall(route).unwrap()
    }

    fn try_uninstall(&self, route: &str) -> Result<Removed, InstallError> {
        install::uninstall(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Rule,
            route,
            &AgentHomes::default(),
        )
    }

    /// Take the rule at `route` out, edited since it was installed or not.
    fn uninstall_forced(&self, route: &str) -> Removed {
        install::uninstall_with(
            &self.workspace,
            &self.ledger,
            &self.project,
            Kind::Rule,
            route,
            &AgentHomes::default(),
            Edits::Forced,
        )
        .unwrap()
    }

    fn render(&self) -> rules::Rendered {
        rules::render(
            &self.workspace,
            &self.ledger,
            &self.project,
            &AgentHomes::default(),
        )
        .unwrap()
        .unwrap()
    }

    fn records(&self) -> Vec<InstallRecord> {
        self.ledger.records("app", Kind::Rule).unwrap()
    }

    /// How the installation of the rule at `route` stands.
    fn standing(&self, route: &str) -> Standing {
        install::standings(&self.workspace, &self.ledger, &self.project, Kind::Rule)
            .unwrap()
            .into_iter()
            .find(|(name, _)| name == route)
            .map_or(Standing::Absent, |(_, standing)| standing)
    }

    fn agents_md(&self) -> PathBuf {
        self.project.root().join("AGENTS.md")
    }

    fn text(&self) -> String {
        fs::read_to_string(self.agents_md()).unwrap()
    }

    fn claude_file(&self, route: &str) -> PathBuf {
        self.project.root().join(claude_rules::file_for(route))
    }

    /// Whether git, reading the exclude file, holds `route` in the project
    /// out of the work tree.
    fn ignored(&self, route: &str) -> bool {
        let repository = gix::open_opts(self.dir.path(), gix::open::Options::isolated()).unwrap();
        let index = repository.index_or_empty().unwrap();
        let mut excludes = repository
            .excludes(&index, None, Default::default())
            .unwrap();
        excludes
            .at_path(
                format!("app/{route}").as_str(),
                Some(gix::index::entry::Mode::FILE),
            )
            .unwrap()
            .is_excluded()
    }
}

/// The hash a section of `body` for `route` is recorded with.
fn section_hash(route: &str, body: &str) -> ContentHash {
    Section::new(route, body).unwrap().hash()
}

/// A target of the section of a rule, by its hash.
fn in_block(hash: ContentHash) -> Target {
    Target {
        agent: None,
        path: String::from("AGENTS.md"),
        content_hash: Some(hash),
    }
}

/// A target of a rule's file for Claude, holding `source`.
fn for_claude(route: &str, source: &str) -> Target {
    Target {
        agent: Some(String::from("claude")),
        path: format!(".claude/rules/mindweaver/{route}.md"),
        content_hash: Some(hash::bytes(source.as_bytes())),
    }
}

fn record(route: &str, source: &str, targets: Vec<Target>) -> InstallRecord {
    InstallRecord {
        kind: Kind::Rule,
        name: route.to_owned().into(),
        source_id: None,
        origin: Some(WORKSPACE_ORIGIN.into()),
        content_hash: Some(hash::bytes(source.as_bytes())),
        targets,
        entries: Vec::new(),
    }
}

#[test]
fn two_rules_rendered_again_and_one_taken_out_leave_the_block_and_the_ledger_alike() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("review/tone", TONE);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let first = fixture.install("review/tone");
    let second = fixture.install("git/no-force-push");

    assert!(
        matches!(&first.installed, Installed::Added { name, .. } if name == "review/tone"),
        "{first:?}"
    );
    assert!(
        matches!(&second.installed, Installed::Added { paths, .. } if *paths == [fixture.agents_md()]),
        "{second:?}"
    );
    let both = format!(
        "{TEAM}\n<!-- mindweaver:begin -->\n\
         <!-- mindweaver:rule git/no-force-push -->\n{NO_FORCE_PUSH}\n\
         <!-- mindweaver:rule review/tone -->\n\
         Say what a change does before what is wrong with it.\n\
         <!-- mindweaver:end -->\n"
    );
    assert_eq!(fixture.text(), both);
    // Claude reads the block itself, so no rule about every file is a file
    // of its own as well.
    let rendered = second.rendered.unwrap();
    assert_eq!(rendered.import.reach, Reach::Native);
    assert!(rendered.files.files.is_empty());
    assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
    let recorded = vec![
        record(
            "git/no-force-push",
            NO_FORCE_PUSH,
            vec![in_block(section_hash("git/no-force-push", NO_FORCE_PUSH))],
        ),
        record(
            "review/tone",
            TONE,
            vec![in_block(section_hash(
                "review/tone",
                "Say what a change does before what is wrong with it.",
            ))],
        ),
    ];
    assert_eq!(fixture.records(), recorded);

    let again = fixture.render();

    assert_eq!(again.block.unwrap().written, agents_md::Written::Unchanged);
    assert_eq!(fixture.text(), both);
    assert_eq!(fixture.records(), recorded);

    let removed = fixture.uninstall("review/tone");

    assert!(
        matches!(&removed, Removed::Removed { paths, .. } if *paths == [fixture.agents_md()]),
        "{removed:?}"
    );
    assert_eq!(
        fixture.text(),
        format!(
            "{TEAM}\n<!-- mindweaver:begin -->\n\
             <!-- mindweaver:rule git/no-force-push -->\n{NO_FORCE_PUSH}\
             <!-- mindweaver:end -->\n"
        )
    );
    assert_eq!(fixture.records(), recorded[..1]);
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
}

/// A rule gathered onto the shelf from a repository's folder of rules is
/// offered by its route, named by `--from` as its source's name, and
/// installed as the library's are: a section of the block in a project its
/// settings mark, recorded with the source as its origin and standing
/// current with the shelf (#111).
#[test]
fn a_rule_gathered_onto_the_shelf_installs_into_the_projects_agents_md_block() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let gathered = ShelfMother::rules_into(&fixture.workspace, &fixture.ledger, &["git/commits"]);
    let from = gathered.name.to_string();
    let candidate = install::offered(
        &fixture.workspace,
        &fixture.ledger,
        &fixture.project,
        Kind::Rule,
        "git/commits",
        Some(&from),
    )
    .unwrap();

    let installed = install::install(
        &fixture.workspace,
        &fixture.ledger,
        &fixture.project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(
        matches!(&installed.installed, Installed::Added { name, paths, .. }
            if name == "git/commits" && *paths == [fixture.agents_md()]),
        "{installed:?}"
    );
    let body = RuleMother::create("commits");
    assert_eq!(
        fixture.text(),
        format!(
            "<!-- mindweaver:begin -->\n<!-- mindweaver:rule git/commits -->\n{body}\
             <!-- mindweaver:end -->\n"
        )
    );
    let source = &fixture.ledger.sources().unwrap()[0];
    assert_eq!(
        fixture.records(),
        [InstallRecord {
            origin: Some(source.origin()),
            ..record(
                "git/commits",
                &body,
                vec![in_block(section_hash("git/commits", body.trim_end()))],
            )
        }]
    );
    assert_eq!(fixture.standing("git/commits"), Standing::Current);
}

#[test]
fn installing_a_rule_again_that_is_as_it_was_changes_nothing_and_says_so() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let file = fixture.rule("review/tone", TONE);
    fixture.install("review/tone");

    let again = fixture.install("review/tone");
    assert!(
        matches!(again.installed, Installed::Unchanged { .. }),
        "{again:?}"
    );

    fs::write(&file, "Say what a change does, kindly.\n").unwrap();
    let changed = fixture.install("review/tone");

    assert!(
        matches!(changed.installed, Installed::Updated { .. }),
        "{changed:?}"
    );
    assert!(fixture.text().contains("Say what a change does, kindly.\n"));
}

#[test]
fn a_render_gives_each_rule_as_it_is_where_it_was_installed_from() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let file = fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    fs::write(&file, "Never force-push to main.\n").unwrap();

    let rendered = fixture.render();

    assert_eq!(rendered.block.unwrap().written, agents_md::Written::Changed);
    assert!(fixture.text().contains("Never force-push to main.\n"));
    assert_eq!(
        fixture.records()[0].targets,
        [in_block(section_hash(
            "git/no-force-push",
            "Never force-push to main.\n"
        ))]
    );
}

#[test]
fn an_agents_md_the_user_wrote_stays_once_the_last_rule_is_taken_out() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");

    fixture.uninstall("git/no-force-push");

    assert_eq!(fixture.text(), TEAM);
    assert!(fixture.records().is_empty());
}

#[test]
fn an_agents_md_mindweaver_made_goes_with_the_last_rule_and_is_kept_out_of_git_meanwhile() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert_eq!(
        done.rendered.unwrap().block.unwrap().written,
        agents_md::Written::Created
    );
    assert!(fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert!(fixture.ignored("AGENTS.md"));

    fixture.uninstall("git/no-force-push");

    assert!(!fixture.agents_md().exists());
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert!(!fixture.ignored("AGENTS.md"));
}

#[test]
fn an_agents_md_mindweaver_made_and_somebody_wrote_in_stays_and_is_theirs() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    let written = fixture.text() + "\nA line of mine.\n";
    fs::write(fixture.agents_md(), &written).unwrap();

    fixture.uninstall("git/no-force-push");

    assert_eq!(fixture.text(), "\nA line of mine.\n");
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    // Theirs to commit, if they like.
    assert!(!fixture.ignored("AGENTS.md"));
}

#[test]
fn an_agents_md_that_was_there_in_a_local_project_is_not_kept_out_of_git() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    fixture.install("git/no-force-push");

    assert!(fixture.text().starts_with(TEAM));
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert!(!fixture.ignored("AGENTS.md"));
}

#[test]
fn a_tracked_agents_md_in_a_local_project_is_left_alone_and_claude_gets_the_fallback() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    common::indexed(fixture.dir.path());
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert_eq!(fixture.text(), TEAM);
    let rendered = done.rendered.unwrap();
    assert_eq!(rendered.block, None);
    assert_eq!(rendered.import.reach, Reach::Fallback(Fallback::NoBlock));
    assert_eq!(
        claude_md::reach(&fixture.project, &AgentHomes::default(), false).unwrap(),
        Reach::Fallback(Fallback::NoBlock)
    );
    assert_eq!(
        fs::read_to_string(fixture.claude_file("git/no-force-push")).unwrap(),
        NO_FORCE_PUSH
    );
    assert!(fixture.ignored(".claude/rules/mindweaver/git/no-force-push.md"));
    assert_eq!(
        rendered.warnings,
        [Warning::Withheld {
            file: fixture.agents_md(),
            why: Withheld::Tracked,
            missed: vec![Agent::Codex, Agent::OpenCode],
        }]
    );
    assert_eq!(
        rendered.warnings[0].to_string(),
        format!(
            "{}: git tracks it, and the project is local without edit_tracked, so no rules \
             are written there, and codex and opencode read none of them",
            fixture.agents_md().display()
        )
    );
    assert_eq!(
        fixture.records(),
        [record(
            "git/no-force-push",
            NO_FORCE_PUSH,
            vec![for_claude("git/no-force-push", NO_FORCE_PUSH)]
        )]
    );
}

#[test]
fn a_tracked_agents_md_in_a_shared_project_takes_the_block() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    common::indexed(fixture.dir.path());
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert!(fixture
        .text()
        .contains("<!-- mindweaver:rule git/no-force-push -->"));
    assert!(done.rendered.unwrap().warnings.is_empty());
}

#[test]
fn a_tracked_agents_md_in_a_local_project_with_edit_tracked_takes_the_block() {
    let mut fixture = fixture(Share::Local, &Agent::ALL);
    fixture
        .project
        .change_settings(|settings| settings.edit_tracked = true)
        .unwrap();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    common::indexed(fixture.dir.path());
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert!(fixture.text().starts_with(TEAM));
    assert!(fixture
        .text()
        .contains("<!-- mindweaver:rule git/no-force-push -->"));
    let rendered = done.rendered.unwrap();
    assert_eq!(rendered.block.unwrap().written, agents_md::Written::Changed);
    assert!(rendered.warnings.is_empty(), "{:?}", rendered.warnings);
    // The team's file, which git tracks: never Mindweaver's, never kept out.
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert_eq!(rendered.excluded, None);
    assert!(!fixture.ignored("AGENTS.md"));
}

#[test]
fn a_block_left_in_a_file_that_can_no_longer_be_edited_is_said() {
    let mut fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    common::indexed(fixture.dir.path());
    // Turned local by hand, as somebody might edit the settings.
    let settings = fixture.project.marker_path();
    let text = fs::read_to_string(&settings).unwrap();
    fs::write(&settings, text.replace("\"shared\"", "\"local\"")).unwrap();
    fixture.project = MindProject::open(fixture.project.root()).unwrap();
    let before = fixture.text();

    let rendered = fixture.render();

    assert_eq!(fixture.text(), before);
    assert_eq!(
        rendered.warnings[0],
        Warning::Left {
            file: fixture.agents_md(),
            why: Withheld::Tracked,
        }
    );
    assert_eq!(rendered.import.reach, Reach::Fallback(Fallback::NoBlock));
    assert!(fixture.claude_file("git/no-force-push").is_file());
}

#[test]
fn a_rule_whose_section_is_in_a_block_that_cannot_be_edited_is_not_uninstalled() {
    let mut fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    common::indexed(fixture.dir.path());
    fixture
        .project
        .change_settings(|settings| settings.share = Share::Local)
        .unwrap();
    let before = fixture.text();

    let refused = fixture.try_uninstall("git/no-force-push").unwrap_err();

    assert!(
        matches!(&refused, InstallError::Withheld { route, why: Withheld::Tracked, .. }
            if route == "git/no-force-push"),
        "{refused:?}"
    );
    assert_eq!(fixture.text(), before);
    assert_eq!(fixture.records().len(), 1);

    // Shared again, the section is still this workspace's to render, and
    // the rule its to take out.
    fixture
        .project
        .change_settings(|settings| settings.share = Share::Shared)
        .unwrap();
    fixture.render();
    fixture.uninstall("git/no-force-push");

    assert_eq!(fixture.text(), TEAM);
    assert!(fixture.records().is_empty());
}

#[test]
fn a_rule_about_some_files_is_a_file_for_claude_alone_and_says_so() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("rust/errors", SCOPED);

    let done = fixture.install("rust/errors");

    // No section, so no block, and no file made to hold one.
    assert!(!fixture.agents_md().exists());
    assert!(!fixture.ledger.created("app", "AGENTS.md").unwrap());
    assert_eq!(
        fs::read_to_string(fixture.claude_file("rust/errors")).unwrap(),
        SCOPED
    );
    let rendered = done.rendered.unwrap();
    assert_eq!(
        rendered.files.warnings,
        [FileWarning::ClaudeOnly {
            route: String::from("rust/errors"),
            missed: vec![Agent::Codex, Agent::OpenCode],
        }]
    );
    assert_eq!(
        fixture.records(),
        [record(
            "rust/errors",
            SCOPED,
            vec![for_claude("rust/errors", SCOPED)]
        )]
    );

    fixture.uninstall("rust/errors");

    assert!(!fixture.claude_file("rust/errors").exists());
    assert!(!fixture
        .project
        .root()
        .join(".claude/rules/mindweaver")
        .exists());
    assert!(fixture.records().is_empty());
}

#[test]
fn claude_given_the_rules_by_its_own_files_has_each_once() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    // A `CLAUDE.md` only above the project: Claude reads no `AGENTS.md`
    // there, and the import has nowhere of the project's to go.
    fs::write(fixture.dir.path().join("CLAUDE.md"), "# Above\n").unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    let rendered = done.rendered.unwrap();
    assert!(
        matches!(rendered.import.reach, Reach::Fallback(Fallback::Above(_))),
        "{:?}",
        rendered.import.reach
    );
    assert!(fixture
        .text()
        .contains("<!-- mindweaver:rule git/no-force-push -->"));
    assert_eq!(
        fs::read_to_string(fixture.claude_file("git/no-force-push")).unwrap(),
        NO_FORCE_PUSH
    );
    assert_eq!(
        fixture.records()[0].targets,
        [
            for_claude("git/no-force-push", NO_FORCE_PUSH),
            in_block(section_hash("git/no-force-push", NO_FORCE_PUSH)),
        ]
    );
}

#[test]
fn a_project_that_names_claude_alone_still_gets_the_block_claude_reads() {
    let fixture = fixture(Share::Shared, &[Agent::Claude]);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let done = fixture.install("git/no-force-push");

    assert!(fixture.text().contains(NO_FORCE_PUSH));
    assert!(done.rendered.unwrap().warnings.is_empty());
}

#[test]
fn a_rule_no_agent_of_the_project_reads_is_installed_and_said() {
    let fixture = fixture(Share::Shared, &[Agent::Codex]);
    fixture.rule("rust/errors", SCOPED);

    let done = fixture.install("rust/errors");

    // Codex reads no `.claude/rules`, and the project does not name Claude.
    assert!(!fixture.claude_file("rust/errors").exists());
    assert_eq!(
        done.rendered.unwrap().warnings,
        [Warning::Unreached {
            route: String::from("rust/errors")
        }]
    );
    assert_eq!(fixture.records()[0].targets, []);
}

#[test]
fn a_section_this_workspace_did_not_write_is_somebodys_and_refuses_the_render() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.rule("review/tone", TONE);
    let theirs = format!(
        "{TEAM}\n<!-- mindweaver:begin -->\n<!-- mindweaver:rule review/tone -->\nTheirs.\n<!-- mindweaver:end -->\n"
    );
    fs::write(fixture.agents_md(), &theirs).unwrap();

    let surveyed = install::survey(
        &fixture.workspace,
        &fixture.ledger,
        &fixture.project,
        Kind::Rule,
    )
    .unwrap();
    let tone = surveyed
        .iter()
        .find(|candidate| candidate.name() == "review/tone")
        .unwrap();
    assert_eq!(tone.standing, Standing::Foreign);

    let refused = fixture.install("review/tone");
    assert_eq!(
        refused.installed,
        Installed::Foreign {
            name: "review/tone".into(),
            path: fixture.agents_md(),
        }
    );
    let other = fixture.try_install("git/no-force-push").unwrap_err();
    assert!(
        matches!(&other, InstallError::NotRecorded { route, .. } if route == "review/tone"),
        "{other:?}"
    );
    assert_eq!(fixture.text(), theirs);
    assert!(fixture.records().is_empty());
}

#[test]
fn a_file_for_claude_nothing_recorded_is_somebodys_and_refuses_the_install() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("rust/errors", SCOPED);
    let file = fixture.claude_file("rust/errors");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, SCOPED).unwrap();

    let refused = fixture.install("rust/errors");

    assert_eq!(
        refused.installed,
        Installed::Foreign {
            name: "rust/errors".into(),
            path: file.clone(),
        }
    );
    assert_eq!(
        fixture.uninstall("rust/errors"),
        Removed::Foreign {
            name: "rust/errors".into(),
            path: file,
        }
    );
    assert!(fixture.records().is_empty());
}

#[test]
fn a_rule_no_longer_offered_refuses_the_render_until_it_is_taken_out() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let tone = fixture.rule("review/tone", TONE);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("review/tone");
    fs::remove_file(tone).unwrap();
    let before = fixture.text();

    let refused = fixture.try_install("git/no-force-push").unwrap_err();

    assert!(
        matches!(&refused, InstallError::NoLongerOffered { name, origin }
            if name == "review/tone" && origin.as_deref() == Some(WORKSPACE_ORIGIN)),
        "{refused:?}"
    );
    assert_eq!(fixture.text(), before);

    fixture.uninstall("review/tone");
    fixture.install("git/no-force-push");

    assert!(!fixture.text().contains("review/tone"));
    assert_eq!(fixture.records().len(), 1);
}

#[test]
fn rules_no_longer_offered_can_each_be_taken_out_and_the_rest_stay_as_they_are() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let sources = [
        fixture.rule("review/tone", TONE),
        fixture.rule("git/no-force-push", NO_FORCE_PUSH),
        fixture.rule("rust/errors", SCOPED),
    ];
    for route in ["review/tone", "git/no-force-push", "rust/errors"] {
        fixture.install(route);
    }
    for source in sources {
        fs::remove_file(source).unwrap();
    }
    // Edited by hand, which the block kept as the project holds it keeps.
    let edited = fixture
        .text()
        .replace(NO_FORCE_PUSH, "Never force-push, ever.\n");
    fs::write(fixture.agents_md(), &edited).unwrap();
    let recorded = fixture.records();

    fixture.uninstall("review/tone");

    assert_eq!(
        fixture.text(),
        format!(
            "{TEAM}\n<!-- mindweaver:begin -->\n\
             <!-- mindweaver:rule git/no-force-push -->\nNever force-push, ever.\n\
             <!-- mindweaver:end -->\n"
        )
    );
    assert_eq!(
        fs::read_to_string(fixture.claude_file("rust/errors")).unwrap(),
        SCOPED
    );
    let others: Vec<InstallRecord> = recorded
        .iter()
        .filter(|record| record.name != "review/tone")
        .cloned()
        .collect();
    assert_eq!(fixture.records(), others);

    // Edited, so taken out only when forced.
    assert_eq!(
        fixture.uninstall("git/no-force-push"),
        Removed::Edited {
            name: "git/no-force-push".into(),
            paths: vec![fixture.agents_md()],
        }
    );
    assert_eq!(fixture.records(), others);
    fixture.uninstall_forced("git/no-force-push");

    assert_eq!(fixture.text(), TEAM);
    assert!(fixture.claude_file("rust/errors").is_file());
    assert_eq!(fixture.records(), others[1..]);

    fixture.uninstall("rust/errors");

    assert!(!fixture.claude_file("rust/errors").exists());
    assert!(fixture.records().is_empty());
}

#[test]
fn a_render_keeps_another_rules_edited_section_as_the_block_holds_it_and_says_so() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let tone = fixture.rule("review/tone", TONE);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("review/tone");
    fixture.install("git/no-force-push");
    let edited = fixture
        .text()
        .replace(NO_FORCE_PUSH, "Never force-push, ever.\n");
    fs::write(fixture.agents_md(), &edited).unwrap();
    let recorded = fixture.records();
    // Both offers move on, and only the rule installed is brought up to date.
    fs::write(&tone, format!("{TONE}Be kind.\n")).unwrap();
    fixture.rule(
        "git/no-force-push",
        "Never force-push anything somebody pulled.\n",
    );

    let again = fixture.install("review/tone");

    assert_eq!(
        fixture.text(),
        format!(
            "{TEAM}\n<!-- mindweaver:begin -->\n\
             <!-- mindweaver:rule git/no-force-push -->\nNever force-push, ever.\n\n\
             <!-- mindweaver:rule review/tone -->\n\
             Say what a change does before what is wrong with it.\nBe kind.\n\
             <!-- mindweaver:end -->\n"
        )
    );
    // Its record is what was rendered, so it reads as edited still.
    assert_eq!(fixture.records()[0], recorded[0]);
    assert_eq!(fixture.standing("git/no-force-push"), Standing::Edited);
    let rendered = again.rendered.unwrap();
    assert_eq!(
        messages(rendered.warnings_of(Some("review/tone"))),
        [format!(
            "{}: the section of rule `git/no-force-push` was changed since it was rendered, so \
             it is left as it is",
            fixture.agents_md().display()
        )]
    );

    // Undone, it is what an earlier render wrote: behind its offer, not
    // current with it.
    let kept = fixture.text();
    let undone = kept.replace("Never force-push, ever.\n", NO_FORCE_PUSH);
    fs::write(fixture.agents_md(), undone).unwrap();
    assert_eq!(fixture.standing("git/no-force-push"), Standing::Outdated);
    fs::write(fixture.agents_md(), kept).unwrap();

    // Forced, it is the offer's again.
    install::install_with(
        &fixture.workspace,
        &fixture.ledger,
        &fixture.project,
        &fixture.typed("git/no-force-push").unwrap(),
        &AgentHomes::default(),
        Edits::Forced,
    )
    .unwrap();

    assert!(fixture
        .text()
        .contains("Never force-push anything somebody pulled.\n"));
    assert_eq!(fixture.standing("git/no-force-push"), Standing::Current);
    assert_eq!(fixture.standing("review/tone"), Standing::Current);
}

#[test]
fn a_forced_change_writes_over_and_takes_away_an_edited_file_for_claude() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("rust/errors", SCOPED);
    fixture.install("rust/errors");
    let file = fixture.claude_file("rust/errors");
    fs::write(&file, format!("{SCOPED}Unless it is a test.\n")).unwrap();

    assert!(matches!(
        fixture.install("rust/errors").installed,
        Installed::Edited { .. }
    ));
    install::install_with(
        &fixture.workspace,
        &fixture.ledger,
        &fixture.project,
        &fixture.typed("rust/errors").unwrap(),
        &AgentHomes::default(),
        Edits::Forced,
    )
    .unwrap();

    assert_eq!(fs::read_to_string(&file).unwrap(), SCOPED);
    assert_eq!(fixture.standing("rust/errors"), Standing::Current);

    fs::write(&file, "Ours now.\n").unwrap();
    assert!(matches!(
        fixture.uninstall("rust/errors"),
        Removed::Edited { .. }
    ));
    assert!(file.is_file());
    fixture.uninstall_forced("rust/errors");

    assert!(!file.exists());
    assert!(fixture.records().is_empty());
}

#[test]
fn a_block_that_cannot_be_read_refuses_the_install_with_nothing_recorded() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    let broken = "<!-- mindweaver:begin -->\nLeft open.\n";
    fs::write(fixture.agents_md(), broken).unwrap();

    let refused = fixture.try_install("git/no-force-push").unwrap_err();

    assert!(matches!(refused, InstallError::AgentsMd(_)), "{refused:?}");
    assert_eq!(fixture.text(), broken);
    assert!(fixture.records().is_empty());
}

#[test]
fn rendering_a_project_a_mind_toml_marks_renders_nothing() {
    let (_dir, workspace, project, ledger) = older();

    let rendered = rules::render(&workspace, &ledger, &project, &AgentHomes::default()).unwrap();

    assert_eq!(rendered, None);
    assert!(!project.root().join("AGENTS.md").exists());
}

/// How the rule at `route` stands in the fixture's project, as status says.
fn status_of(fixture: &Fixture, route: &str) -> Option<(State, Vec<Agent>, Option<String>)> {
    let (status, failures) = Status::of_project(&fixture.project, Some(&fixture.workspace));
    assert!(failures.is_empty(), "{failures:?}");
    status.projects[0].find(Kind::Rule, route).map(|rule| {
        (
            rule.state,
            rule.agents.clone(),
            rule.origin.clone().map(String::from),
        )
    })
}

#[test]
fn status_lists_the_rules_a_project_received_from_the_ledger_and_the_block() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.rule("rust/errors", SCOPED);
    fixture.install("git/no-force-push");
    fixture.install("rust/errors");

    assert_eq!(
        status_of(&fixture, "git/no-force-push"),
        Some((
            State::Current,
            Agent::ALL.to_vec(),
            Some(String::from(WORKSPACE_ORIGIN))
        ))
    );
    assert_eq!(
        status_of(&fixture, "rust/errors"),
        Some((
            State::Current,
            vec![Agent::Claude],
            Some(String::from(WORKSPACE_ORIGIN))
        ))
    );

    // The section taken out by hand: recorded, and no longer found.
    fs::remove_file(fixture.agents_md()).unwrap();

    assert_eq!(
        status_of(&fixture, "git/no-force-push"),
        Some((
            State::Missing,
            Vec::new(),
            Some(String::from(WORKSPACE_ORIGIN))
        ))
    );
    assert_eq!(
        status_of(&fixture, "rust/errors").map(|(state, ..)| state),
        Some(State::Current)
    );
}

#[test]
fn status_says_claude_reads_a_rule_by_its_own_file_where_it_reads_no_block() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fs::write(fixture.dir.path().join("CLAUDE.md"), "# Above\n").unwrap();
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");

    assert_eq!(
        status_of(&fixture, "git/no-force-push").map(|(state, agents, _)| (state, agents)),
        Some((State::Current, Agent::ALL.to_vec()))
    );

    // Claude's file gone, Claude reads the rule nowhere: the block is not
    // where it reads it.
    fs::remove_file(fixture.claude_file("git/no-force-push")).unwrap();

    assert_eq!(
        status_of(&fixture, "git/no-force-push").map(|(state, agents, _)| (state, agents)),
        Some((State::Missing, vec![Agent::Codex, Agent::OpenCode]))
    );
}

#[test]
fn status_says_a_rule_outdated_once_its_offer_changes_and_orphaned_once_it_goes() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let offered = fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");

    fs::write(&offered, "Never force-push, ever.\n").unwrap();
    let outdated = status_of(&fixture, "git/no-force-push");
    fs::remove_file(&offered).unwrap();
    let orphaned = status_of(&fixture, "git/no-force-push");

    // Every agent still reads what was rendered, until it is brought up to
    // date or taken out.
    assert_eq!(
        outdated.map(|(state, agents, _)| (state, agents)),
        Some((State::Outdated, Agent::ALL.to_vec()))
    );
    assert_eq!(
        orphaned.map(|(state, agents, _)| (state, agents)),
        Some((State::Orphaned, Agent::ALL.to_vec()))
    );
}

#[test]
fn status_says_a_rule_edited_in_its_section_or_its_file_is_edited() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.rule("rust/errors", SCOPED);
    fixture.install("git/no-force-push");
    fixture.install("rust/errors");

    let edited = fixture
        .text()
        .replace(NO_FORCE_PUSH, "Never force-push, ever.\n");
    fs::write(fixture.agents_md(), edited).unwrap();
    fs::write(fixture.claude_file("rust/errors"), "Ours now.\n").unwrap();

    assert_eq!(
        status_of(&fixture, "git/no-force-push").map(|(state, _, _)| state),
        Some(State::Edited)
    );
    assert_eq!(
        status_of(&fixture, "rust/errors").map(|(state, _, _)| state),
        Some(State::Edited)
    );
}

#[test]
fn status_lists_a_rule_copied_into_a_project_a_mind_toml_marks_as_its_copy() {
    let (_dir, workspace, project, ledger) = older();
    own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    put(&workspace, &ledger, &project, "git/no-force-push");

    let (status, _) = Status::of_project(&project, Some(&workspace));

    let rule = status.projects[0]
        .find(Kind::Rule, "git/no-force-push")
        .unwrap();
    assert_eq!(rule.state, State::Current);
    assert_eq!(rule.origin.as_deref(), Some(WORKSPACE_ORIGIN));
}

impl Fixture {
    /// The workspace's own skill `name`.
    fn skill(&self, name: &str) {
        let folder = self
            .workspace
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join(name);
        fs::create_dir_all(&folder).unwrap();
        fs::write(
            folder.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: What {name} does\n---\n\nSteps.\n"),
        )
        .unwrap();
    }

    /// What `typed` names among what the project is offered.
    fn typed(&self, typed: &str) -> Result<Candidate, InstallError> {
        install::offered_typed(
            &self.workspace,
            &self.ledger,
            &self.project,
            &Reference::parse(typed),
            None,
            self.workspace.root(),
        )
    }

    /// Install what `typed` names, as a command line would.
    fn install_typed(&self, typed: &str) -> Installation {
        let candidate = self.typed(typed).unwrap();
        install::install(
            &self.workspace,
            &self.ledger,
            &self.project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap()
    }

    /// Take out what `typed` names, as a command line would.
    fn uninstall_typed(&self, typed: &str) -> Result<(Kind, Removed), InstallError> {
        install::uninstall_typed(
            &self.workspace,
            &self.ledger,
            &self.project,
            &Reference::parse(typed),
            &AgentHomes::default(),
            Edits::Kept,
        )
    }
}

#[test]
fn a_name_typed_bare_is_of_the_one_kind_that_offers_it() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.skill("deploy");
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.rule("deploy-notes", NO_FORCE_PUSH);

    let rule = fixture.typed("git/no-force-push").unwrap();
    let skill = fixture.typed("deploy").unwrap();
    let also_rule = fixture.typed("deploy-notes").unwrap();

    assert_eq!(
        (rule.kind(), rule.name().as_str()),
        (Kind::Rule, "git/no-force-push")
    );
    assert_eq!(
        (skill.kind(), skill.name().as_str()),
        (Kind::Skill, "deploy")
    );
    assert_eq!(also_rule.kind(), Kind::Rule);
    let nothing = fixture.typed("nothing").unwrap_err();
    assert!(
        matches!(&nothing, InstallError::NotOffered { name, .. } if name == "nothing"),
        "{nothing:?}"
    );
}

#[test]
fn a_name_two_kinds_offer_is_installed_only_as_the_kind_it_is_qualified_with() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.skill("deploy");
    fixture.rule("deploy", NO_FORCE_PUSH);

    let refused = fixture.typed("deploy").unwrap_err();

    assert!(
        matches!(&refused, InstallError::OfferedAsKinds { kinds, .. } if kinds == &[Kind::Skill, Kind::Rule]),
        "{refused:?}"
    );
    assert_eq!(
        refused.to_string(),
        "`deploy` is offered as a skill and as a rule, so say which: `skill:deploy` or `rule:deploy`"
    );
    assert_eq!(fixture.typed("rule:deploy").unwrap().kind(), Kind::Rule);
    assert_eq!(fixture.typed("skill:deploy").unwrap().kind(), Kind::Skill);

    fixture.install_typed("rule:deploy");

    assert!(fixture.text().contains("<!-- mindweaver:rule deploy -->"));
    assert!(!fixture
        .project
        .root()
        .join(".claude/skills/deploy")
        .exists());
}

#[test]
fn a_project_no_kind_can_go_into_is_told_so_rather_than_sent_to_gather() {
    let fixture = fixture(Share::Shared, &[]);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);

    let refused = fixture.typed("deploy").unwrap_err();

    assert!(
        matches!(
            &refused,
            InstallError::NoTarget {
                kind: Kind::Skill,
                ..
            }
        ),
        "{refused:?}"
    );
}

#[test]
fn a_bare_name_is_taken_out_as_the_one_kind_installed_under_it() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.skill("deploy");
    fixture.rule("deploy", NO_FORCE_PUSH);
    fixture.install_typed("rule:deploy");

    let (kind, removed) = fixture.uninstall_typed("deploy").unwrap();

    assert_eq!(kind, Kind::Rule);
    assert!(
        matches!(&removed, Removed::Removed { name, .. } if name == "deploy"),
        "{removed:?}"
    );
    assert!(!fixture.agents_md().exists());
    assert!(fixture.records().is_empty());
}

#[test]
fn a_bare_name_installed_as_two_kinds_is_taken_out_only_once_qualified() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.skill("deploy");
    fixture.rule("deploy", NO_FORCE_PUSH);
    fixture.install_typed("rule:deploy");
    fixture.install_typed("skill:deploy");

    let refused = fixture.uninstall_typed("deploy").unwrap_err();

    assert_eq!(
        refused.to_string(),
        "`deploy` is installed as a skill and as a rule, so say which: `skill:deploy` or `rule:deploy`"
    );
    assert!(fixture.text().contains("<!-- mindweaver:rule deploy -->"));
    let (kind, _) = fixture.uninstall_typed("skill:deploy").unwrap();
    assert_eq!(kind, Kind::Skill);
    assert!(!fixture
        .project
        .root()
        .join(".claude/skills/deploy")
        .exists());
    assert!(fixture.text().contains("<!-- mindweaver:rule deploy -->"));
}

#[test]
fn a_bare_name_nothing_records_says_whose_is_there_of_either_kind() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    let theirs = format!(
        "{TEAM}\n<!-- mindweaver:begin -->\n<!-- mindweaver:rule review/tone -->\nTheirs.\n<!-- mindweaver:end -->\n"
    );
    fs::write(fixture.agents_md(), &theirs).unwrap();
    let skill = fixture.project.root().join(".claude/skills/notes");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "Mine.\n").unwrap();

    assert_eq!(
        fixture.uninstall_typed("review/tone").unwrap(),
        (
            Kind::Rule,
            Removed::Foreign {
                name: "review/tone".into(),
                path: fixture.agents_md(),
            }
        )
    );
    assert_eq!(
        fixture.uninstall_typed("notes").unwrap(),
        (
            Kind::Skill,
            Removed::Foreign {
                name: "notes".into(),
                path: skill,
            }
        )
    );
    assert_eq!(
        fixture.uninstall_typed("nothing").unwrap(),
        (
            Kind::Skill,
            Removed::Absent {
                name: "nothing".into(),
            }
        )
    );
    assert_eq!(fixture.text(), theirs);
}

#[test]
fn a_render_says_what_each_renderer_found_once_in_one_list() {
    // In no git work tree: what a local project keeps out of git is kept out
    // of nothing, which the block and Claude's files each find.
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, Share::Local, &Directories::default())
            .unwrap();
    workspace.link(&project).unwrap();
    let ledger = workspace.ledger().unwrap();
    own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    own_rule(&workspace, "rust/errors", SCOPED);
    put(&workspace, &ledger, &project, "git/no-force-push");
    let candidate = install::offered(
        &workspace,
        &ledger,
        &project,
        Kind::Rule,
        "rust/errors",
        None,
    )
    .unwrap();

    let rendered = install::install(
        &workspace,
        &ledger,
        &project,
        &candidate,
        &AgentHomes::default(),
    )
    .unwrap()
    .rendered
    .unwrap();

    let no_git_root = format!(
        "{}: in no git work tree, so Codex reads its AGENTS.md only when started in that folder \
         itself",
        project.root().display()
    );
    let claude_only = String::from(
        "rule `rust/errors` states `paths`, so it reaches claude only: codex and opencode read \
         no .claude/rules",
    );
    assert_eq!(
        messages(rendered.warnings_of(Some("rust/errors"))),
        [no_git_root.clone(), claude_only.clone()]
    );
    // What is true of a rule whatever the render is said of the one changed,
    // and of every one by a render that changed none.
    assert_eq!(
        messages(rendered.warnings_of(Some("git/no-force-push"))),
        std::slice::from_ref(&no_git_root)
    );
    let warned = rendered.warnings_of(None);
    assert_eq!(
        warned.iter().map(|said| said.code).collect::<Vec<_>>(),
        ["codex_no_git_root", "claude_only_rule"],
        "each beside its code"
    );
    assert_eq!(messages(warned), [no_git_root, claude_only]);
    assert_eq!(
        rendered.notes(),
        [format!(
            "{}: not in a git work tree, so nothing keeps what is installed there out of git",
            project.root().display()
        )]
    );
}

#[test]
fn the_offer_of_a_rule_a_project_holds_is_the_one_its_record_came_from() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("review/tone", TONE);
    let (mut workspace, team) = (
        WeaverWorkspace::open(fixture.workspace.root()).unwrap(),
        WeaverWorkspace::init(fixture.dir.path().join("team"))
            .unwrap()
            .0,
    );
    own_rule(&team, "review/tone", "Be kind.\n");
    workspace.load(&team).unwrap();
    let from = |origin: &str| {
        install::offered(
            &workspace,
            &fixture.ledger,
            &fixture.project,
            Kind::Rule,
            "review/tone",
            Some(origin),
        )
        .unwrap()
    };
    let current = |candidate: &Candidate| {
        install::is_current(&workspace, &fixture.ledger, &fixture.project, candidate)
    };
    assert!(!current(&from(WORKSPACE_ORIGIN)));

    install::install(
        &workspace,
        &fixture.ledger,
        &fixture.project,
        &from("load:team"),
        &AgentHomes::default(),
    )
    .unwrap();

    assert!(current(&from("load:team")));
    assert!(!current(&from(WORKSPACE_ORIGIN)));
}

#[test]
fn a_project_whose_settings_name_no_agent_receives_no_kind() {
    let named = fixture(Share::Shared, &[Agent::Codex]);
    let none = fixture(Share::Shared, &[]);

    // An MCP server among them, an entry in each agent's config file, with
    // none to write into.
    for kind in Kind::ALL {
        assert!(install::receives(&named.project, kind).is_ok(), "{kind}");
        let refused = install::receives(&none.project, kind).unwrap_err();
        assert!(
            matches!(&refused, InstallError::NoTarget { kind: refused, .. } if *refused == kind),
            "{refused:?}"
        );
    }
}

/// What each of `said` says, without its code.
fn messages(said: Vec<Said>) -> Vec<String> {
    said.into_iter()
        .map(|said| said.message.into_string())
        .collect()
}

/// What status says each rule the fixture's project received misses.
fn missed_in(fixture: &Fixture) -> Vec<Missed> {
    let (status, failures) = Status::of_project(&fixture.project, Some(&fixture.workspace));
    assert!(failures.is_empty(), "{failures:?}");
    status.projects[0].missed.clone()
}

#[test]
fn status_says_which_agents_read_a_rule_it_received_nowhere() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.rule("rust/errors", SCOPED);
    fixture.install("git/no-force-push");
    fixture.install("rust/errors");

    assert_eq!(
        missed_in(&fixture),
        [Missed {
            kind: Kind::Rule,
            name: "rust/errors".into(),
            agents: vec![Agent::Codex, Agent::OpenCode],
        }]
    );

    // Partial says a section gone already: no agent reads it, and that is
    // not said twice.
    fs::remove_file(fixture.agents_md()).unwrap();

    assert_eq!(
        missed_in(&fixture)
            .iter()
            .map(|missed| missed.name.as_str())
            .collect::<Vec<_>>(),
        ["rust/errors"]
    );
}

#[test]
fn status_says_the_agents_a_block_withheld_from_a_tracked_agents_md_misses() {
    let fixture = fixture(Share::Local, &Agent::ALL);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    common::indexed(fixture.dir.path());
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");

    assert_eq!(
        status_of(&fixture, "git/no-force-push").map(|(state, agents, _)| (state, agents)),
        Some((State::Current, vec![Agent::Claude]))
    );
    assert_eq!(
        missed_in(&fixture),
        [Missed {
            kind: Kind::Rule,
            name: "git/no-force-push".into(),
            agents: vec![Agent::Codex, Agent::OpenCode],
        }]
    );
}

#[test]
fn status_says_nothing_missed_of_a_rule_a_project_a_mind_toml_marks_holds() {
    let (_dir, workspace, project, ledger) = older();
    own_rule(&workspace, "git/no-force-push", NO_FORCE_PUSH);
    put(&workspace, &ledger, &project, "git/no-force-push");

    let (status, _) = Status::of_project(&project, Some(&workspace));

    assert_eq!(status.projects[0].missed, []);
}

#[test]
fn a_change_aimed_at_a_rule_the_project_received_is_refused_as_received() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("git/no-force-push", NO_FORCE_PUSH);
    fixture.install("git/no-force-push");
    let find = |typed: &str| {
        lifecycle::find_to_change(
            Some(&fixture.workspace),
            &fixture.project,
            &Reference::parse(typed),
        )
        .unwrap_err()
    };

    let received = find("git/no-force-push");
    let qualified = find("rule:git/no-force-push");
    let as_a_skill = find("skill:git/no-force-push");
    let nothing = find("nothing");

    assert!(
        matches!(&received, LifecycleError::Received { typed, project }
            if typed == "git/no-force-push" && project == "app"),
        "{received:?}"
    );
    assert_eq!(
        received.to_string(),
        "`git/no-force-push` is a rule app received, rendered from where it was installed from \
         rather than kept in it, so it is not changed from here: uninstall it, or change it \
         where it is offered and install it again"
    );
    assert!(
        matches!(qualified, LifecycleError::Received { .. }),
        "{qualified:?}"
    );
    assert!(
        matches!(as_a_skill, LifecycleError::NotFound { .. }),
        "{as_a_skill:?}"
    );
    assert!(
        matches!(nothing, LifecycleError::NotFound { .. }),
        "{nothing:?}"
    );
    // Without the workspace whose ledger says so, nothing says it was
    // received.
    assert!(matches!(
        lifecycle::find_to_change(
            None,
            &fixture.project,
            &Reference::parse("git/no-force-push")
        ),
        Err(LifecycleError::NotFound { .. })
    ));
}

/// A block a Mindflayer build rendered is given a new rule's section under
/// its own markers, and taken out with its last rule as a new one is.
#[test]
fn a_block_a_mindflayer_build_rendered_is_rendered_again_under_its_markers() {
    let fixture = fixture(Share::Shared, &[Agent::Codex]);
    fixture.rule("a", "Rule a.\n");
    fixture.rule("b", "Rule b.\n");
    fixture.install("a");
    mindweaver_core::mother::MindflayerMother::rendered_into(fixture.project.root());

    fixture.install("b");

    let text = fixture.text();
    assert!(text.contains(agents_md::FLAYER_BEGIN), "{text}");
    assert!(text.contains("<!-- mindflayer:rule b -->"), "{text}");
    assert!(!text.contains("mindweaver:"), "{text}");
    assert_eq!(fixture.standing("a"), Standing::Current);
    assert_eq!(fixture.standing("b"), Standing::Current);

    fixture.uninstall("a");
    fixture.uninstall("b");
    assert!(!fixture.agents_md().exists());
}

/// Claude's file of a rule a Mindflayer build installed stays in the folder
/// it was filed in, written over and taken away there, while a rule given a
/// file since goes into the new one.
#[test]
fn a_rule_file_a_mindflayer_build_filed_stays_where_its_record_says() {
    let scoped = |says: &str| format!("---\npaths:\n  - \"src/**\"\n---\n{says}\n");
    let fixture = fixture(Share::Local, &[Agent::Claude]);
    fixture.rule("a", &scoped("Rule a."));
    fixture.rule("b", &scoped("Rule b."));
    fixture.install("a");
    mindweaver_core::mother::MindflayerMother::installed_into(&fixture.workspace, &fixture.project);
    let old = |route: &str| {
        fixture
            .project
            .root()
            .join(".claude/rules/mindflayer")
            .join(format!("{route}.md"))
    };
    assert!(old("a").is_file());
    assert_eq!(fixture.standing("a"), Standing::Current);

    fixture.rule("a", &scoped("Rule a, changed."));
    fixture.install("a");
    fixture.install("b");

    assert_eq!(
        fs::read_to_string(old("a")).unwrap(),
        scoped("Rule a, changed.")
    );
    assert!(!fixture.claude_file("a").exists());
    assert_eq!(
        fs::read_to_string(fixture.claude_file("b")).unwrap(),
        scoped("Rule b.")
    );
    assert!(!old("b").exists());
    assert!(fixture.ignored(".claude/rules/mindflayer/a.md"));
    assert!(fixture.ignored(".claude/rules/mindweaver/b.md"));
    let paths: Vec<String> = fixture
        .records()
        .into_iter()
        .flat_map(|record| record.targets)
        .map(|target| target.path)
        .collect();
    assert_eq!(
        paths,
        [
            ".claude/rules/mindflayer/a.md",
            ".claude/rules/mindweaver/b.md"
        ]
    );

    fixture.uninstall("a");
    assert!(!old("a").exists());
    assert!(
        !fixture
            .project
            .root()
            .join(".claude/rules/mindflayer")
            .exists(),
        "its emptied folder goes with it"
    );
    assert!(!fixture.ignored(".claude/rules/mindflayer/a.md"));
    assert_eq!(fixture.standing("b"), Standing::Current);
}

/// A file at a rule's place in the folder a Mindflayer build filed them in,
/// that nothing records, is somebody's: the rule is not given a second file
/// in the new folder, which Claude would read beside it.
#[test]
fn a_file_for_claude_in_mindflayer_s_folder_nothing_recorded_is_somebodys() {
    let fixture = fixture(Share::Shared, &Agent::ALL);
    fixture.rule("rust/errors", SCOPED);
    let file = fixture
        .project
        .root()
        .join(".claude/rules/mindflayer/rust/errors.md");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, SCOPED).unwrap();

    let refused = fixture.install("rust/errors");

    assert_eq!(
        refused.installed,
        Installed::Foreign {
            name: "rust/errors".into(),
            path: file,
        }
    );
    assert!(!fixture.claude_file("rust/errors").exists());
    assert!(fixture.records().is_empty());
}
