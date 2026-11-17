//! Rendering a project's rules into its `AGENTS.md`: the file made, changed
//! and taken away, every byte outside the block kept, and what is refused.

mod common;

use std::fs;
use std::path::PathBuf;

use mindweaver_core::agents_md::{
    self, AgentsMdError, Malformed, Rendered, Warning, Written, BEGIN, CODEX_BUDGET, CODEX_NEAR,
    END,
};
use mindweaver_core::{AgentHomes, Artifact, MindProject, Names, WeaverWorkspace};
use tempfile::TempDir;

/// What a team's `AGENTS.md` holds before Mindweaver writes in it.
const TEAM: &str = "# Working here\n\nRun `make test` before every push.\n";

/// No agent's own folder, as a front end hands over when a test pins them.
const NO_HOMES: AgentHomes = AgentHomes {
    claude_config_dir: None,
    codex_home: None,
    opencode_config_dir: None,
    agents_dir: None,
    claude_dir_in_home: None,
    opencode_dir_in_home: None,
};

/// A workspace linking one project below it, and a library of rules to
/// render from, in a git work tree whose root is the workspace's.
struct Fixture {
    dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
    library: PathBuf,
}

fn fixture() -> Fixture {
    let fixture = outside_git();
    gix::init(fixture.dir.path()).unwrap();
    fixture
}

/// The same, in no git work tree.
fn outside_git() -> Fixture {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let project = linked(&mut workspace, &dir.path().join("app"));
    Fixture {
        library: dir.path().join("library"),
        dir,
        workspace,
        project,
    }
}

/// A project made at `root` and linked by `workspace`.
fn linked(workspace: &mut WeaverWorkspace, root: &std::path::Path) -> MindProject {
    fs::create_dir_all(root).unwrap();
    let (project, _) = MindProject::init(root).unwrap();
    workspace.link(&project).unwrap();
    project
}

impl Fixture {
    /// The rule filed at `route` in the library, holding `source`, loaded.
    fn rule(&self, route: &str, source: &str) -> Artifact {
        let file = self.library.join(format!("{route}.md"));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        Artifact::rule(&file, route.to_owned(), &self.library).unwrap()
    }

    fn agents_md(&self) -> PathBuf {
        self.project.root().join(agents_md::FILE)
    }

    fn render(&self, rules: &[&Artifact], created: bool) -> Result<Rendered, AgentsMdError> {
        agents_md::render(
            &self.workspace,
            &self.project,
            rules.iter().copied(),
            created,
            &NO_HOMES,
        )
    }

    fn text(&self) -> String {
        fs::read_to_string(self.agents_md()).unwrap()
    }
}

#[test]
fn a_render_matches_the_golden_block() {
    let fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let tone = fixture.rule(
        "review/tone",
        "---\ndescription: How a review reads\n---\n\n# Tone\n\nSay what a change does before what is wrong with it.\nAsk, rather than tell, when unsure.\n",
    );
    let force = fixture.rule(
        "git/no-force-push",
        "Never force-push a branch somebody else may have pulled.\n\nRewrite only what is yours alone.\n",
    );
    // About some files only, so Claude's alone (#43), and not in the block.
    let errors = fixture.rule(
        "rust/errors",
        "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors; never panic in a library.\n",
    );
    // A `paths` in a shape nothing reads says nothing, so this one is about
    // every file.
    let commits = fixture.rule(
        "commit-style",
        "---\npaths: 7\n---\nWrite commit messages as `<area>: <imperative summary>`.\n",
    );

    let rendered = fixture
        .render(&[&tone, &force, &errors, &commits], false)
        .unwrap();
    assert_eq!(rendered.written, Written::Changed);
    assert_eq!(
        rendered.routes,
        ["commit-style", "git/no-force-push", "review/tone"]
    );
    assert_eq!(rendered.warnings, []);
    assert_eq!(fixture.text(), include_str!("fixtures/agents-md.md"));
}

#[test]
fn created_is_said_only_when_the_file_was_not_there() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");

    let first = fixture.render(&[&rule], false).unwrap();
    assert_eq!(first.file, fixture.agents_md());
    assert_eq!(first.written, Written::Created);
    assert!(first.created());
    assert_eq!(
        fixture.text(),
        format!("{BEGIN}\n<!-- mindweaver:rule a -->\nRule a.\n{END}\n")
    );

    let again = fixture.render(&[&rule], true).unwrap();
    assert_eq!(again.written, Written::Unchanged);
    assert!(!again.created());

    let other = fixture.rule("b", "Rule b.\n");
    let changed = fixture.render(&[&rule, &other], true).unwrap();
    assert_eq!(changed.written, Written::Changed);
    assert!(!changed.created());
}

#[test]
fn a_file_somebody_wrote_is_changed_and_never_said_to_be_created() {
    let fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    let rendered = fixture.render(&[&rule], false).unwrap();
    assert_eq!(rendered.written, Written::Changed);
    assert!(!rendered.created());
}

#[test]
fn no_rules_and_no_file_writes_nothing() {
    let fixture = fixture();
    let rendered = fixture.render(&[], true).unwrap();
    assert_eq!(rendered.written, Written::Unchanged);
    assert!(rendered.routes.is_empty());
    assert!(!fixture.agents_md().exists());
}

#[test]
fn text_around_the_block_is_the_same_through_a_render_a_second_one_and_a_removal() {
    let fixture = fixture();
    let before =
        format!("{TEAM}{BEGIN}\n<!-- mindweaver:rule old -->\nOld.\n{END}\n\n## Later\n\nMore.\n");
    fs::write(fixture.agents_md(), &before).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    fixture.render(&[&rule], false).unwrap();
    let rendered = fixture.text();
    assert!(rendered.starts_with(&format!("{TEAM}{BEGIN}\n")));
    assert!(rendered.ends_with(&format!("{END}\n\n## Later\n\nMore.\n")));

    let again = fixture.render(&[&rule], false).unwrap();
    assert_eq!(again.written, Written::Unchanged);
    assert_eq!(fixture.text(), rendered);

    let removed = fixture.render(&[], false).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert_eq!(fixture.text(), format!("{TEAM}\n## Later\n\nMore.\n"));
}

#[test]
fn a_crlf_file_keeps_crlf_through_a_render_a_second_one_and_a_removal() {
    let fixture = fixture();
    let before = "# Working here\r\n\r\nRun `make test`.\r\n";
    fs::write(fixture.agents_md(), before).unwrap();
    let rule = fixture.rule("a", "One.\nTwo.\n");

    fixture.render(&[&rule], false).unwrap();
    let rendered = fs::read(fixture.agents_md()).unwrap();
    assert_eq!(
        String::from_utf8(rendered.clone()).unwrap(),
        format!("{before}\r\n{BEGIN}\r\n<!-- mindweaver:rule a -->\r\nOne.\r\nTwo.\r\n{END}\r\n")
    );

    assert_eq!(
        fixture.render(&[&rule], false).unwrap().written,
        Written::Unchanged
    );
    assert_eq!(fs::read(fixture.agents_md()).unwrap(), rendered);

    fixture.render(&[], false).unwrap();
    assert_eq!(fs::read(fixture.agents_md()).unwrap(), before.as_bytes());
}

#[test]
fn removing_the_last_rule_takes_away_a_file_mindweaver_made() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    assert!(fixture.render(&[&rule], false).unwrap().created());

    let removed = fixture.render(&[], true).unwrap();
    assert_eq!(removed.written, Written::Removed);
    assert!(removed.routes.is_empty());
    assert!(!fixture.agents_md().exists());
}

#[test]
fn a_file_mindweaver_made_stays_once_somebody_wrote_in_it() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();
    let mut text = fixture.text();
    text.push_str("\nA line of somebody's.\n");
    fs::write(fixture.agents_md(), &text).unwrap();

    let removed = fixture.render(&[], true).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert_eq!(fixture.text(), "\nA line of somebody's.\n");
}

#[test]
fn a_file_mindweaver_did_not_make_stays_when_the_block_was_all_it_held() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();

    let removed = fixture.render(&[], false).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert_eq!(fixture.text(), "");
}

#[cfg(unix)]
#[test]
fn a_link_in_place_of_a_file_mindweaver_made_is_emptied_not_taken_away() {
    let fixture = fixture();
    let target = fixture.project.root().join("NOTES.md");
    fs::write(&target, "").unwrap();
    std::os::unix::fs::symlink("NOTES.md", fixture.agents_md()).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();
    assert!(fs::read_to_string(&target).unwrap().contains("Rule a."));

    let removed = fixture.render(&[], true).unwrap();
    assert_eq!(removed.written, Written::Changed);
    assert!(fs::symlink_metadata(fixture.agents_md())
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), "");
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_project_is_refused_and_what_it_points_at_left_alone() {
    let fixture = fixture();
    let outside = TempDir::new().unwrap();
    let theirs = outside.path().join("AGENTS.md");
    fs::write(&theirs, TEAM).unwrap();
    std::os::unix::fs::symlink(&theirs, fixture.agents_md()).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    match fixture.render(&[&rule], false) {
        Err(AgentsMdError::LinkedOut { file, target }) => {
            assert_eq!(file, fixture.agents_md());
            assert_eq!(target, mindweaver_core::paths::canonical(&theirs).unwrap());
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read_to_string(&theirs).unwrap(), TEAM);
}

#[cfg(unix)]
#[test]
fn a_link_to_nothing_in_place_of_the_file_is_never_written_through() {
    let fixture = fixture();
    std::os::unix::fs::symlink("NOTES.md", fixture.agents_md()).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::Write { path, .. }) if *path == fixture.agents_md()),
        "{refused:?}"
    );
    assert!(!fixture.project.root().join("NOTES.md").exists());
}

#[test]
fn a_malformed_block_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    let before = format!("{TEAM}{BEGIN}\n<!-- mindweaver:rule a -->\nRule a.\n");
    fs::write(fixture.agents_md(), &before).unwrap();
    let rule = fixture.rule("b", "Rule b.\n");

    match fixture.render(&[&rule], true) {
        Err(AgentsMdError::Malformed { file, problem }) => {
            assert_eq!(file, fixture.agents_md());
            assert_eq!(
                problem,
                Malformed::Unclosed {
                    line: 4,
                    names: Names::Mindweaver,
                }
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fixture.text(), before);
}

#[test]
fn a_duplicated_marker_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    let before = format!(
        "{BEGIN}\n<!-- mindweaver:rule a -->\nOne.\n<!-- mindweaver:rule a -->\nTwo.\n{END}\n"
    );
    fs::write(fixture.agents_md(), &before).unwrap();

    let refused = fixture.render(&[], true);
    assert!(
        matches!(
            refused,
            Err(AgentsMdError::Malformed {
                problem: Malformed::Repeated { line: 4, .. },
                ..
            })
        ),
        "{refused:?}"
    );
    assert_eq!(fixture.text(), before);
}

#[test]
fn two_rules_of_one_name_are_refused_and_nothing_is_written() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let elsewhere = TempDir::new().unwrap();
    let file = elsewhere.path().join("a.md");
    fs::write(&file, "Another rule a.\n").unwrap();
    let other = Artifact::rule(&file, "a".to_owned(), elsewhere.path()).unwrap();

    let refused = fixture.render(&[&rule, &other], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::Twice { route }) if route == "a"),
        "{refused:?}"
    );
    assert!(!fixture.agents_md().exists());
}

#[test]
fn a_rule_that_cannot_be_read_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    fs::remove_file(rule.path()).unwrap();

    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(refused, Err(AgentsMdError::Rule(_))),
        "{refused:?}"
    );
    assert_eq!(fixture.text(), TEAM);
}

#[test]
fn a_rule_holding_a_marker_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    let rule = fixture.rule("a", &format!("Before.\n{END}\nAfter.\n"));
    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::Marked { route, .. }) if route == "a"),
        "{refused:?}"
    );
    assert!(!fixture.agents_md().exists());
}

#[test]
fn a_skill_has_no_section() {
    let fixture = fixture();
    let folder = fixture.library.join("commit-style");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("SKILL.md"),
        common::skill("commit-style", "Commits"),
    )
    .unwrap();
    let skill = Artifact::skill(&folder, &fixture.library).unwrap();

    let refused = fixture.render(&[&skill], false);
    assert!(
        matches!(&refused, Err(AgentsMdError::NotARule { name }) if name == "skill:commit-style"),
        "{refused:?}"
    );
    assert!(!fixture.agents_md().exists());
}

#[test]
fn the_workspaces_own_folder_is_refused() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let own = fixture.workspace.own();

    let refused = agents_md::render(&fixture.workspace, &own, [&rule], false, &NO_HOMES);
    assert!(
        matches!(&refused, Err(AgentsMdError::WorkspaceRoot { folder }) if folder == fixture.workspace.root()),
        "{refused:?}"
    );
    assert!(!fixture.workspace.root().join(agents_md::FILE).exists());
}

#[test]
fn a_project_the_workspace_links_at_its_root_is_given_its_rules() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let project = linked(&mut workspace, dir.path());
    let library = TempDir::new().unwrap();
    let file = library.path().join("a.md");
    fs::write(&file, "Rule a.\n").unwrap();
    let rule = Artifact::rule(&file, "a".to_owned(), library.path()).unwrap();

    let rendered = agents_md::render(&workspace, &project, [&rule], false, &NO_HOMES).unwrap();
    assert!(rendered.created());
    assert_eq!(rendered.file, dir.path().join(agents_md::FILE));
}

#[test]
fn a_project_unlinked_from_the_workspaces_root_is_refused_a_block_and_can_drop_its_own() {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let project = linked(&mut workspace, dir.path());
    let library = TempDir::new().unwrap();
    let file = library.path().join("a.md");
    fs::write(&file, "Rule a.\n").unwrap();
    let rule = Artifact::rule(&file, "a".to_owned(), library.path()).unwrap();
    assert!(
        agents_md::render(&workspace, &project, [&rule], false, &NO_HOMES)
            .unwrap()
            .created()
    );
    workspace.unlink(dir.path()).unwrap();

    let refused = agents_md::render(&workspace, &project, [&rule], true, &NO_HOMES);
    assert!(
        matches!(refused, Err(AgentsMdError::WorkspaceRoot { .. })),
        "{refused:?}"
    );
    let dropped = agents_md::render(&workspace, &project, [], true, &NO_HOMES).unwrap();
    assert_eq!(dropped.written, Written::Removed);
    assert!(!dir.path().join(agents_md::FILE).exists());
}

#[test]
fn a_folder_above_a_linked_project_is_refused_and_nothing_is_written() {
    let mut fixture = fixture();
    let root = fixture.project.root().to_path_buf();
    let package = root.join("packages").join("web");
    linked(&mut fixture.workspace, &package);
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    match fixture.render(&[&rule], false) {
        Err(AgentsMdError::AboveProjects { folder, projects }) => {
            assert_eq!(folder, root);
            assert_eq!(projects, std::slice::from_ref(&package));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fixture.text(), TEAM);

    // The package below is given its own.
    let below = MindProject::open(&package).unwrap();
    agents_md::render(&fixture.workspace, &below, [&rule], false, &NO_HOMES).unwrap();
    assert!(package.join(agents_md::FILE).exists());
}

#[test]
fn a_block_above_a_project_linked_since_can_still_be_taken_out() {
    let mut fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();
    linked(&mut fixture.workspace, &fixture.project.root().join("web"));

    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(refused, Err(AgentsMdError::AboveProjects { .. })),
        "{refused:?}"
    );
    let dropped = fixture.render(&[], false).unwrap();
    assert_eq!(dropped.written, Written::Changed);
    assert_eq!(fixture.text(), TEAM);
}

#[test]
fn a_linked_project_gone_from_below_still_keeps_its_folder_above_out() {
    let mut fixture = fixture();
    let package = fixture.project.root().join("web");
    linked(&mut fixture.workspace, &package);
    fs::remove_dir_all(&package).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let refused = fixture.render(&[&rule], false);
    assert!(
        matches!(refused, Err(AgentsMdError::AboveProjects { .. })),
        "{refused:?}"
    );
}

#[test]
fn a_project_at_or_inside_an_agents_own_folder_is_refused_a_block_and_can_drop_its_own() {
    let fixture = fixture();
    fs::write(fixture.agents_md(), TEAM).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    fixture.render(&[&rule], false).unwrap();
    let at = AgentHomes {
        codex_home: Some(fixture.project.root().to_path_buf()),
        ..AgentHomes::default()
    };
    let above = AgentHomes {
        claude_config_dir: Some(fixture.dir.path().to_path_buf()),
        ..AgentHomes::default()
    };

    for (homes, reader) in [(&at, "Codex"), (&above, "Claude Code")] {
        let refused =
            agents_md::render(&fixture.workspace, &fixture.project, [&rule], false, homes);
        match refused {
            Err(AgentsMdError::UserScope {
                file,
                home,
                reader: said,
            }) => {
                assert_eq!(file, fixture.agents_md());
                assert!(fixture.project.root().starts_with(&home), "{home:?}");
                assert_eq!(said, reader);
            }
            other => panic!("{other:?}"),
        }
    }

    let dropped = agents_md::render(&fixture.workspace, &fixture.project, [], false, &at).unwrap();
    assert_eq!(dropped.written, Written::Changed);
    assert_eq!(fixture.text(), TEAM);
}

#[cfg(unix)]
#[test]
fn a_folder_spelled_through_a_link_is_still_above_what_it_leads_to() {
    let mut fixture = fixture();
    let elsewhere = TempDir::new().unwrap();
    linked(&mut fixture.workspace, &elsewhere.path().join("app"));
    MindProject::init(elsewhere.path()).unwrap();
    let through = fixture.workspace.root().join("through");
    std::os::unix::fs::symlink(elsewhere.path(), &through).unwrap();

    let above = MindProject::open(&through).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    let refused = agents_md::render(&fixture.workspace, &above, [&rule], false, &NO_HOMES);
    assert!(
        matches!(refused, Err(AgentsMdError::AboveProjects { .. })),
        "{refused:?}"
    );
}

/// A rule whose body is `bytes` long, give or take its line ending.
fn sized(fixture: &Fixture, route: &str, bytes: usize) -> Artifact {
    fixture.rule(route, &format!("{}\n", "x".repeat(bytes)))
}

#[test]
fn an_override_beside_the_file_is_warned_of() {
    let fixture = fixture();
    let beside = fixture.project.root().join(agents_md::OVERRIDE);
    fs::write(&beside, "Codex reads this instead.\n").unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let rendered = fixture.render(&[&rule], false).unwrap();
    assert_eq!(rendered.warnings, [Warning::Overridden { file: beside }]);
    assert!(rendered.warnings[0]
        .to_string()
        .contains("do not reach Codex"));

    // Said again on a render that changes nothing, and not once the block
    // has gone.
    let again = fixture.render(&[&rule], false).unwrap();
    assert_eq!(again.written, Written::Unchanged);
    assert_eq!(again.warnings.len(), 1);
    assert_eq!(fixture.render(&[], false).unwrap().warnings, []);
}

#[test]
fn a_project_in_no_git_work_tree_is_warned_of() {
    let fixture = outside_git();
    let rule = fixture.rule("a", "Rule a.\n");
    let rendered = fixture.render(&[&rule], false).unwrap();
    assert_eq!(
        rendered.warnings,
        [Warning::NoGitRoot {
            project: fixture.project.root().to_path_buf(),
        }]
    );
}

#[test]
fn files_from_the_git_root_down_nearing_codexs_budget_are_warned_of() {
    let fixture = fixture();
    let root = fixture.dir.path().join(agents_md::FILE);
    fs::write(&root, "r".repeat(20 * 1024)).unwrap();
    // An override hides its folder's AGENTS.md from Codex, so only it counts.
    let middle = fixture.project.root().join("packages");
    fs::create_dir_all(middle.join("web")).unwrap();
    let (project, _) = MindProject::init(middle.join("web")).unwrap();
    fs::write(middle.join(agents_md::FILE), "m".repeat(20 * 1024)).unwrap();
    fs::write(middle.join(agents_md::OVERRIDE), "o".repeat(1024)).unwrap();

    let small = sized(&fixture, "small", 1024);
    let rendered =
        agents_md::render(&fixture.workspace, &project, [&small], false, &NO_HOMES).unwrap();
    assert_eq!(rendered.warnings, []);

    let large = sized(&fixture, "large", 7 * 1024 + 512);
    let rendered =
        agents_md::render(&fixture.workspace, &project, [&large], false, &NO_HOMES).unwrap();
    let mine = project.root().join(agents_md::FILE);
    let bytes = 20 * 1024 + 1024 + fs::metadata(&mine).unwrap().len();
    assert!((CODEX_NEAR..=CODEX_BUDGET).contains(&bytes), "{bytes}");
    assert_eq!(
        rendered.warnings,
        [Warning::NearBudget {
            project: project.root().to_path_buf(),
            bytes,
            files: vec![root.clone(), middle.join(agents_md::OVERRIDE), mine.clone()],
        }]
    );

    let larger = sized(&fixture, "larger", 12 * 1024);
    let rendered =
        agents_md::render(&fixture.workspace, &project, [&larger], false, &NO_HOMES).unwrap();
    let bytes = 20 * 1024 + 1024 + fs::metadata(&mine).unwrap().len();
    assert!(bytes > CODEX_BUDGET);
    assert!(
        matches!(&rendered.warnings[..], [Warning::OverBudget { bytes: said, files, .. }] if *said == bytes && files.len() == 3),
        "{:?}",
        rendered.warnings
    );
    assert!(rendered.warnings[0]
        .to_string()
        .contains("cuts off the rest"));
}

#[test]
fn a_project_in_no_git_work_tree_counts_only_its_own_file_against_the_budget() {
    let fixture = outside_git();
    fs::write(
        fixture.dir.path().join(agents_md::FILE),
        "r".repeat(CODEX_BUDGET as usize),
    )
    .unwrap();
    let rule = fixture.rule("a", "Rule a.\n");
    let rendered = fixture.render(&[&rule], false).unwrap();
    assert!(
        matches!(&rendered.warnings[..], [Warning::NoGitRoot { .. }]),
        "{:?}",
        rendered.warnings
    );
}

#[cfg(unix)]
#[test]
fn a_file_codex_would_read_that_cannot_be_looked_at_is_said() {
    let fixture = fixture();
    // A link to itself, at the git root: neither there nor not.
    let looped = fixture.dir.path().join(agents_md::OVERRIDE);
    std::os::unix::fs::symlink(agents_md::OVERRIDE, &looped).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let rendered = fixture.render(&[&rule], false).unwrap();
    assert_eq!(rendered.written, Written::Created);
    match &rendered.warnings[..] {
        [Warning::Unmeasured { project, detail }] => {
            assert_eq!(project, fixture.project.root());
            assert!(detail.contains(&looped.display().to_string()), "{detail}");
        }
        other => panic!("{other:?}"),
    }
}
