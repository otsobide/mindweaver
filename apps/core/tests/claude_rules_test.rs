//! Rendering a project's rules into Claude's rules folder: a file a rule,
//! kept as written, Mindweaver's own told by what was recorded, and what is
//! refused.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::agents_md;
use mindweaver_core::claude_rules::{
    self, ClaudeRulesError, Mode, Recorded, Rendered, Warning, Written,
};
use mindweaver_core::{
    hash, Agent, AgentHomes, Artifact, Directories, MindProject, Names, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// A rule about Rust files only.
const SCOPED: &str =
    "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors; never panic in a library.\n";
/// A rule about every file.
const UNSCOPED: &str = "Never force-push a branch somebody else may have pulled.\n";

/// A workspace linking one project below it, shared and naming every
/// agent, and a library of rules to render from, in a git work tree whose
/// root is the workspace's.
struct Fixture {
    dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
    library: PathBuf,
}

fn fixture() -> Fixture {
    fixture_with(Share::Shared, true)
}

/// The same, sharing as `share` says, and in a git work tree only when
/// `in_git`.
fn fixture_with(share: Share, in_git: bool) -> Fixture {
    let dir = TempDir::new().unwrap();
    if in_git {
        gix::init(dir.path()).unwrap();
    }
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, &Agent::ALL, share, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    Fixture {
        library: dir.path().join("library"),
        dir,
        workspace,
        project,
    }
}

impl Fixture {
    /// The rule filed at `route` in the library, holding `source`, loaded.
    fn rule(&self, route: &str, source: &str) -> Artifact {
        let file = self.library.join(format!("{route}.md"));
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, source).unwrap();
        Artifact::rule(&file, route.to_owned(), &self.library).unwrap()
    }

    fn render(
        &self,
        rules: &[&Artifact],
        mode: Mode,
        recorded: &[Recorded],
    ) -> Result<Rendered, ClaudeRulesError> {
        claude_rules::render(
            &self.workspace,
            &self.project,
            rules.iter().copied(),
            mode,
            recorded,
            &AgentHomes::default(),
        )
    }

    /// `.claude/rules/mindweaver` in the project.
    fn folder(&self) -> PathBuf {
        self.project.root().join(claude_rules::folder())
    }

    fn file(&self, route: &str) -> PathBuf {
        self.project.root().join(claude_rules::file_for(route))
    }

    fn exclude_file(&self) -> PathBuf {
        self.dir.path().join(".git").join("info").join("exclude")
    }

    /// Whether gix, reading the exclude file as git does, holds the file at
    /// `route`'s place to be ignored.
    fn ignored(&self, route: &str) -> bool {
        let repository = gix::open_opts(self.dir.path(), gix::open::Options::isolated()).unwrap();
        let index = repository.index_or_empty().unwrap();
        let mut excludes = repository
            .excludes(&index, None, Default::default())
            .unwrap();
        let path = format!("app/.claude/rules/mindweaver/{route}.md");
        excludes
            .at_path(path.as_str(), Some(gix::index::entry::Mode::FILE))
            .unwrap()
            .is_excluded()
    }

    /// Every file under the folder, as a `/`-joined route from it, sorted.
    fn files(&self) -> Vec<String> {
        let mut found = Vec::new();
        walk(&self.folder(), "", &mut found);
        found.sort();
        found
    }
}

fn walk(folder: &Path, prefix: &str, found: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    for entry in entries {
        let entry = entry.unwrap();
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.file_type().unwrap().is_dir() {
            walk(&entry.path(), &format!("{name}/"), found);
        } else {
            found.push(name);
        }
    }
}

fn written(rendered: &Rendered) -> Vec<(&str, Written)> {
    rendered
        .files
        .iter()
        .map(|file| (file.route.as_str(), file.written))
        .collect()
}

#[test]
fn a_path_scoped_rule_is_one_file_and_no_section_in_the_block() {
    let fixture = fixture();
    let scoped = fixture.rule("rust/errors", SCOPED);
    let unscoped = fixture.rule("git/no-force-push", UNSCOPED);

    let rendered = fixture
        .render(&[&scoped, &unscoped], Mode::Scoped, &[])
        .unwrap();
    let block = agents_md::render(
        &fixture.workspace,
        &fixture.project,
        [&scoped, &unscoped],
        false,
        &AgentHomes::default(),
    )
    .unwrap();

    assert_eq!(written(&rendered), [("rust/errors", Written::Created)]);
    assert_eq!(fixture.files(), ["rust/errors.md"]);
    // The rule as written, front matter and all, for Claude to read `paths`
    // from.
    assert_eq!(
        fs::read_to_string(fixture.file("rust/errors")).unwrap(),
        SCOPED
    );
    assert_eq!(rendered.files[0].path, fixture.file("rust/errors"));
    assert_eq!(
        rendered.recorded(),
        [Recorded {
            route: "rust/errors".to_owned(),
            hash: hash::bytes(SCOPED.as_bytes()),
            names: Names::Mindweaver,
        }]
    );
    // Said, naming the project's agents it does not reach.
    assert_eq!(
        rendered.warnings,
        [Warning::ClaudeOnly {
            route: "rust/errors".to_owned(),
            missed: vec![Agent::Codex, Agent::OpenCode],
        }]
    );
    assert_eq!(
        rendered.warnings[0].to_string(),
        "rule `rust/errors` states `paths`, so it reaches claude only: codex and opencode \
         read no .claude/rules"
    );
    assert_eq!(block.routes, ["git/no-force-push"]);
    let text = fs::read_to_string(&block.file).unwrap();
    assert!(!text.contains("rust/errors"), "{text}");
    assert!(!text.contains("never panic"), "{text}");
}

#[test]
fn an_unscoped_rule_in_fallback_mode_is_a_file_and_no_agents_md_edit() {
    let fixture = fixture();
    let rule = fixture.rule("git/no-force-push", UNSCOPED);

    let rendered = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();

    assert_eq!(
        written(&rendered),
        [("git/no-force-push", Written::Created)]
    );
    assert_eq!(
        fs::read_to_string(fixture.file("git/no-force-push")).unwrap(),
        UNSCOPED
    );
    assert!(!fixture.project.root().join(agents_md::FILE).exists());
    assert_eq!(rendered.warnings, []);
}

#[test]
fn scoped_mode_writes_nothing_for_rules_about_every_file() {
    let fixture = fixture();
    let rule = fixture.rule("git/no-force-push", UNSCOPED);
    // A `paths` in a shape nothing reads says nothing.
    let unread = fixture.rule("commit-style", "---\npaths: 7\n---\nBody.\n");

    let rendered = fixture
        .render(&[&rule, &unread], Mode::Scoped, &[])
        .unwrap();

    assert_eq!(rendered.files, []);
    assert!(!fixture.project.root().join(".claude").exists());
}

#[test]
fn removing_the_last_rule_removes_the_file_and_the_empty_folder() {
    let fixture = fixture();
    let rule = fixture.rule("git/no-force-push", UNSCOPED);
    let first = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();

    let removed = fixture
        .render(&[], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(written(&removed), [("git/no-force-push", Written::Removed)]);
    assert_eq!(removed.recorded(), []);
    assert!(!fixture.folder().exists());
    // Not past Mindweaver's own folder, which may be somebody's.
    assert!(fixture.project.root().join(".claude/rules").is_dir());
}

#[test]
fn a_folder_holding_somebody_elses_file_stays_when_the_last_rule_goes() {
    let fixture = fixture();
    let rule = fixture.rule("git/no-force-push", UNSCOPED);
    let first = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();
    let theirs = fixture.folder().join("git").join("theirs.md");
    fs::write(&theirs, "Theirs.\n").unwrap();

    fixture
        .render(&[], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(fixture.files(), ["git/theirs.md"]);
}

#[test]
fn a_changed_rule_replaces_its_file_and_an_unchanged_one_is_left_alone() {
    let fixture = fixture();
    let a = fixture.rule("a", "Rule a.\n");
    let b = fixture.rule("b", "Rule b.\n");
    let first = fixture.render(&[&a, &b], Mode::Fallback, &[]).unwrap();

    let a = fixture.rule("a", "Rule a, changed.\n");
    let again = fixture
        .render(&[&a, &b], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(
        written(&again),
        [("a", Written::Changed), ("b", Written::Unchanged)]
    );
    assert_eq!(
        fs::read_to_string(fixture.file("a")).unwrap(),
        "Rule a, changed.\n"
    );
    assert_eq!(
        again.recorded(),
        [
            Recorded {
                route: "a".to_owned(),
                hash: hash::bytes(b"Rule a, changed.\n"),
                names: Names::Mindweaver,
            },
            first.recorded()[1].clone(),
        ]
    );
    assert_eq!(again.warnings, []);
}

#[test]
fn a_renamed_rule_takes_its_old_file_away() {
    let fixture = fixture();
    let old = fixture.rule("old", UNSCOPED);
    let first = fixture.render(&[&old], Mode::Fallback, &[]).unwrap();

    let new = fixture.rule("new", UNSCOPED);
    let renamed = fixture
        .render(&[&new], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(
        written(&renamed),
        [("new", Written::Created), ("old", Written::Removed)]
    );
    assert_eq!(fixture.files(), ["new.md"]);
}

#[test]
fn rules_about_every_file_leave_claudes_folder_once_it_reads_the_block() {
    let fixture = fixture();
    let scoped = fixture.rule("rust/errors", SCOPED);
    let unscoped = fixture.rule("git/no-force-push", UNSCOPED);
    let fallback = fixture
        .render(&[&scoped, &unscoped], Mode::Fallback, &[])
        .unwrap();
    assert_eq!(fixture.files(), ["git/no-force-push.md", "rust/errors.md"]);

    let scoped_only = fixture
        .render(&[&scoped, &unscoped], Mode::Scoped, &fallback.recorded())
        .unwrap();

    assert_eq!(
        written(&scoped_only),
        [
            ("git/no-force-push", Written::Removed),
            ("rust/errors", Written::Unchanged)
        ]
    );
    assert_eq!(fixture.files(), ["rust/errors.md"]);
}

#[test]
fn a_file_somebody_put_where_a_rule_goes_is_left_and_said() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    fs::create_dir_all(fixture.folder()).unwrap();
    fs::write(fixture.file("a"), "Theirs.\n").unwrap();

    let rendered = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();

    assert_eq!(written(&rendered), [("a", Written::Foreign)]);
    assert_eq!(rendered.recorded(), []);
    assert_eq!(
        rendered.warnings,
        [Warning::Foreign {
            route: "a".to_owned(),
            path: fixture.file("a"),
        }]
    );
    assert_eq!(fs::read_to_string(fixture.file("a")).unwrap(), "Theirs.\n");

    // Even one holding the rule byte for byte is not taken for Mindweaver's,
    // or it would go with the rule.
    fs::write(fixture.file("a"), "Rule a.\n").unwrap();
    let same = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();
    assert_eq!(written(&same), [("a", Written::Foreign)]);
}

#[test]
fn a_file_changed_since_it_was_written_is_neither_written_over_nor_taken_away() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let first = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();
    fs::write(fixture.file("a"), "Rule a, as somebody wants it.\n").unwrap();

    let rule = fixture.rule("a", "Rule a, changed.\n");
    let changed = fixture
        .render(&[&rule], Mode::Fallback, &first.recorded())
        .unwrap();
    let removed = fixture
        .render(&[], Mode::Fallback, &first.recorded())
        .unwrap();

    for rendered in [&changed, &removed] {
        assert_eq!(written(rendered), [("a", Written::Edited)]);
        // Kept as recorded, so it is still the edited copy of Mindweaver's.
        assert_eq!(rendered.recorded(), first.recorded());
        assert_eq!(
            rendered.warnings,
            [Warning::Edited {
                route: "a".to_owned(),
                path: fixture.file("a"),
            }]
        );
    }
    assert_eq!(
        fs::read_to_string(fixture.file("a")).unwrap(),
        "Rule a, as somebody wants it.\n"
    );

    // Edited back into the rule as it is now, it is current again.
    fs::write(fixture.file("a"), "Rule a, changed.\n").unwrap();
    let current = fixture
        .render(&[&rule], Mode::Fallback, &first.recorded())
        .unwrap();
    assert_eq!(written(&current), [("a", Written::Unchanged)]);
    assert_eq!(
        current.recorded()[0].hash,
        hash::bytes(b"Rule a, changed.\n")
    );
}

#[test]
fn a_recorded_file_somebody_deleted_is_written_again_or_forgotten() {
    let fixture = fixture();
    let a = fixture.rule("a", "Rule a.\n");
    let b = fixture.rule("deep/b", "Rule b.\n");
    let first = fixture.render(&[&a, &b], Mode::Fallback, &[]).unwrap();
    fs::remove_file(fixture.file("a")).unwrap();
    fs::remove_file(fixture.file("deep/b")).unwrap();

    let again = fixture
        .render(&[&a], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(
        written(&again),
        [("a", Written::Created), ("deep/b", Written::Gone)]
    );
    assert_eq!(again.recorded(), first.recorded()[..1]);
    // The folder it left empty goes too.
    assert_eq!(fixture.files(), ["a.md"]);
    assert!(!fixture.folder().join("deep").exists());
}

#[test]
fn the_last_rules_folder_somebody_deleted_takes_mindweavers_folder_with_it() {
    let fixture = fixture();
    let rule = fixture.rule("deep/b", "Rule b.\n");
    let first = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();
    fs::remove_dir_all(fixture.folder().join("deep")).unwrap();

    let again = fixture
        .render(&[], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(written(&again), [("deep/b", Written::Gone)]);
    assert!(!fixture.folder().exists());
    assert!(fixture.project.root().join(".claude/rules").is_dir());
}

#[cfg(unix)]
#[test]
fn a_link_in_place_of_a_rule_file_is_never_written_through_or_taken_away() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let first = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();
    let target = fixture.project.root().join("NOTES.md");
    fs::write(&target, "Rule a.\n").unwrap();
    fs::remove_file(fixture.file("a")).unwrap();
    std::os::unix::fs::symlink(&target, fixture.file("a")).unwrap();
    let unrecorded = fixture.rule("b", "Rule b.\n");
    fs::write(fixture.project.root().join("B.md"), "Theirs.\n").unwrap();
    std::os::unix::fs::symlink("../../../B.md", fixture.file("b")).unwrap();

    let changed = fixture.rule("a", "Rule a, changed.\n");
    let rendered = fixture
        .render(&[&changed, &unrecorded], Mode::Fallback, &first.recorded())
        .unwrap();
    let removed = fixture
        .render(&[], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(
        written(&rendered),
        [("a", Written::Edited), ("b", Written::Foreign)]
    );
    assert_eq!(written(&removed), [("a", Written::Edited)]);
    assert!(fs::symlink_metadata(fixture.file("a"))
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(fs::read_to_string(&target).unwrap(), "Rule a.\n");
    assert_eq!(
        fs::read_to_string(fixture.project.root().join("B.md")).unwrap(),
        "Theirs.\n"
    );
}

#[test]
fn a_place_that_cannot_be_written_fails_that_rule_alone() {
    let fixture = fixture();
    // A file where the folder of `git/...` would be.
    fs::create_dir_all(fixture.folder()).unwrap();
    fs::write(fixture.folder().join("git"), "a file").unwrap();
    let blocked = fixture.rule("git/no-force-push", UNSCOPED);
    let fine = fixture.rule("review/tone", "Say what a change does first.\n");

    let rendered = fixture
        .render(&[&blocked, &fine], Mode::Fallback, &[])
        .unwrap();

    assert_eq!(
        written(&rendered),
        [
            ("git/no-force-push", Written::Failed),
            ("review/tone", Written::Created)
        ]
    );
    assert_eq!(rendered.recorded().len(), 1);
    assert!(
        matches!(&rendered.warnings[..], [Warning::Failed { route, path, .. }]
            if route == "git/no-force-push" && *path == fixture.file("git/no-force-push")),
        "{:?}",
        rendered.warnings
    );
}

#[test]
fn a_skill_has_no_file_and_nothing_is_written() {
    let fixture = fixture();
    let folder = fixture.library.join("commit-style");
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("SKILL.md"),
        common::skill("commit-style", "Commits"),
    )
    .unwrap();
    let skill = Artifact::skill(&folder, &fixture.library).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let refused = fixture.render(&[&rule, &skill], Mode::Fallback, &[]);

    assert!(
        matches!(&refused, Err(ClaudeRulesError::NotARule { name }) if name == "skill:commit-style"),
        "{refused:?}"
    );
    assert!(!fixture.folder().exists());
}

#[test]
fn two_rules_of_one_name_are_refused_and_nothing_is_written() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let elsewhere = TempDir::new().unwrap();
    let file = elsewhere.path().join("a.md");
    fs::write(&file, "Another rule a.\n").unwrap();
    let other = Artifact::rule(&file, "a".to_owned(), elsewhere.path()).unwrap();

    let refused = fixture.render(&[&rule, &other], Mode::Fallback, &[]);

    assert!(
        matches!(&refused, Err(ClaudeRulesError::Twice { route }) if route == "a"),
        "{refused:?}"
    );
    assert!(!fixture.folder().exists());
}

#[test]
fn two_rules_of_one_name_are_refused_in_scoped_mode_when_only_one_is_scoped() {
    let fixture = fixture();
    let scoped = fixture.rule("a", SCOPED);
    let elsewhere = TempDir::new().unwrap();
    let file = elsewhere.path().join("a.md");
    fs::write(&file, UNSCOPED).unwrap();
    let unscoped = Artifact::rule(&file, "a".to_owned(), elsewhere.path()).unwrap();

    // The block takes only the rule about every file, so it has no second
    // section to compare it with: refusing the pair is this render's.
    let refused = fixture.render(&[&scoped, &unscoped], Mode::Scoped, &[]);

    assert!(
        matches!(&refused, Err(ClaudeRulesError::Twice { route }) if route == "a"),
        "{refused:?}"
    );
    assert!(!fixture.folder().exists());
}

#[test]
fn a_rule_that_cannot_be_read_is_refused_and_nothing_is_written() {
    let fixture = fixture();
    let a = fixture.rule("a", "Rule a.\n");
    let b = fixture.rule("b", "Rule b.\n");
    fs::remove_file(b.path()).unwrap();

    let refused = fixture.render(&[&a, &b], Mode::Fallback, &[]);

    assert!(
        matches!(refused, Err(ClaudeRulesError::Rule(_))),
        "{refused:?}"
    );
    assert!(!fixture.folder().exists());
}

#[test]
fn a_recorded_route_that_is_no_name_is_refused_and_nothing_is_taken_away() {
    let fixture = fixture();
    let outside = fixture.dir.path().join("precious.md");
    fs::write(&outside, UNSCOPED).unwrap();
    let recorded = [Recorded {
        route: "../../../../precious".to_owned(),
        hash: hash::bytes(UNSCOPED.as_bytes()),
        names: Names::Mindweaver,
    }];

    let refused = fixture.render(&[], Mode::Fallback, &recorded);

    assert!(
        matches!(&refused, Err(ClaudeRulesError::Route { route, .. }) if route == &recorded[0].route),
        "{refused:?}"
    );
    assert!(outside.exists());
}

#[test]
fn a_local_project_keeps_its_rule_files_out_of_git_and_takes_them_back_in() {
    let fixture = fixture_with(Share::Local, true);
    let id = fixture.project.settings().unwrap().id.as_str().to_owned();
    let file = fixture.exclude_file();
    let before = fs::read_to_string(&file).unwrap();
    let a = fixture.rule("a", "Rule a.\n");
    let force = fixture.rule("git/no-force-push", UNSCOPED);
    // Somebody's file gets no entry.
    let theirs = fixture.rule("theirs", "Rule theirs.\n");
    fs::create_dir_all(fixture.folder()).unwrap();
    fs::write(fixture.file("theirs"), "Theirs.\n").unwrap();

    let first = fixture
        .render(&[&a, &force, &theirs], Mode::Fallback, &[])
        .unwrap();

    assert_eq!(
        first.excluded,
        Some(mindweaver_core::Excluded::Written { file: file.clone() })
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!(
            "{before}# mindweaver:begin\n# project {id}\n/app/.claude/rules/mindweaver/a.md\n\
             /app/.claude/rules/mindweaver/git/no-force-push.md\n# mindweaver:end\n"
        )
    );
    assert!(fixture.ignored("a"));
    assert!(fixture.ignored("git/no-force-push"));
    assert!(!fixture.ignored("theirs"));

    let again = fixture
        .render(&[&a, &force, &theirs], Mode::Fallback, &first.recorded())
        .unwrap();
    assert_eq!(
        again.excluded,
        Some(mindweaver_core::Excluded::Unchanged { file: file.clone() })
    );

    let removed = fixture
        .render(&[&theirs], Mode::Fallback, &first.recorded())
        .unwrap();
    assert_eq!(removed.excluded, None);
    assert_eq!(removed.still_excluded, None);
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    assert!(!fixture.ignored("a"));
}

#[test]
fn a_shared_project_leaves_the_exclude_file_alone() {
    let fixture = fixture();
    let file = fixture.exclude_file();
    let before = fs::read(&file).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let first = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();
    let removed = fixture
        .render(&[], Mode::Fallback, &first.recorded())
        .unwrap();

    assert_eq!(first.excluded, None);
    assert_eq!(removed.still_excluded, None);
    assert_eq!(fs::read(&file).unwrap(), before);
}

#[test]
fn a_local_project_outside_git_is_told_nothing_keeps_its_files_out() {
    let fixture = fixture_with(Share::Local, false);
    let rule = fixture.rule("a", "Rule a.\n");

    let rendered = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();

    let excluded = rendered.excluded.unwrap();
    assert!(
        matches!(excluded, mindweaver_core::Excluded::Outside { .. }),
        "{excluded:?}"
    );
    assert!(excluded
        .warning()
        .unwrap()
        .contains("not in a git work tree"));
    assert!(fixture.file("a").is_file());
}

#[test]
fn an_exclude_block_that_cannot_be_read_refuses_the_render_with_nothing_written() {
    let fixture = fixture_with(Share::Local, true);
    let file = fixture.exclude_file();
    fs::write(&file, "# mindweaver:begin\n/mine/\n").unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let refused = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap_err();

    assert!(matches!(refused, ClaudeRulesError::Exclude(_)), "{refused}");
    assert!(
        refused.to_string().contains("no rule file was written"),
        "{refused}"
    );
    assert!(!fixture.folder().exists());
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "# mindweaver:begin\n/mine/\n"
    );
}

#[cfg(unix)]
#[test]
fn a_file_that_fails_to_land_takes_its_exclude_entry_back_out() {
    let fixture = fixture_with(Share::Local, true);
    let id = fixture.project.settings().unwrap().id.as_str().to_owned();
    let file = fixture.exclude_file();
    let before = fs::read_to_string(&file).unwrap();
    // A link to nothing where the folder of `git/...` would be, which no
    // folder can be made through.
    fs::create_dir_all(fixture.folder()).unwrap();
    std::os::unix::fs::symlink("nowhere", fixture.folder().join("git")).unwrap();
    let blocked = fixture.rule("git/no-force-push", UNSCOPED);
    let fine = fixture.rule("review/tone", "Say what a change does first.\n");

    let rendered = fixture
        .render(&[&blocked, &fine], Mode::Fallback, &[])
        .unwrap();

    assert_eq!(
        written(&rendered),
        [
            ("git/no-force-push", Written::Failed),
            ("review/tone", Written::Created)
        ]
    );
    assert_eq!(rendered.still_excluded, None);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!(
            "{before}# mindweaver:begin\n# project {id}\n\
             /app/.claude/rules/mindweaver/review/tone.md\n# mindweaver:end\n"
        )
    );
}

#[test]
fn the_workspaces_own_folder_is_refused() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let own = fixture.workspace.own();

    let refused = claude_rules::render(
        &fixture.workspace,
        &own,
        [&rule],
        Mode::Fallback,
        &[],
        &AgentHomes::default(),
    );

    assert!(
        matches!(&refused, Err(ClaudeRulesError::WorkspaceRoot { folder }) if folder == fixture.workspace.root()),
        "{refused:?}"
    );
    assert!(!fixture.dir.path().join(".claude").exists());
}

#[test]
fn a_folder_above_a_linked_project_is_refused_and_can_still_have_its_files_taken_out() {
    let mut fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let first = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();
    let package = fixture.project.root().join("packages").join("web");
    fs::create_dir_all(&package).unwrap();
    let (below, _) = MindProject::init(&package).unwrap();
    fixture.workspace.link(&below).unwrap();

    let refused = fixture.render(&[&rule], Mode::Fallback, &first.recorded());
    match refused {
        Err(ClaudeRulesError::AboveProjects { folder, projects }) => {
            assert_eq!(folder, fixture.project.root());
            assert_eq!(projects, [package]);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fixture.files(), ["a.md"]);

    let dropped = fixture
        .render(&[], Mode::Fallback, &first.recorded())
        .unwrap();
    assert_eq!(written(&dropped), [("a", Written::Removed)]);
    assert!(!fixture.folder().exists());
}

#[test]
fn a_project_whose_rules_folder_is_an_agents_own_is_refused() {
    let fixture = fixture();
    let rule = fixture.rule("a", "Rule a.\n");
    let root = fixture.project.root();
    // The project at the user's home, whose `.claude` is Claude's own, and
    // one at Codex's own folder.
    let claudes = AgentHomes {
        claude_config_dir: Some(root.join(".claude")),
        ..AgentHomes::default()
    };
    let codexs = AgentHomes {
        codex_home: Some(root.to_path_buf()),
        ..AgentHomes::default()
    };

    for (homes, reader) in [(&claudes, "Claude Code"), (&codexs, "Codex")] {
        let refused = claude_rules::render(
            &fixture.workspace,
            &fixture.project,
            [&rule],
            Mode::Fallback,
            &[],
            homes,
        );
        match refused {
            Err(ClaudeRulesError::UserScope {
                file, reader: said, ..
            }) => {
                assert_eq!(file, fixture.folder());
                assert_eq!(said, reader);
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(!fixture.folder().exists());
}

#[cfg(unix)]
#[test]
fn a_link_out_of_the_project_is_refused_and_nothing_is_written_or_taken_away_there() {
    let fixture = fixture();
    let outside = TempDir::new().unwrap();
    let theirs = outside.path().join("rules").join("mindflayer").join("a.md");
    fs::create_dir_all(theirs.parent().unwrap()).unwrap();
    fs::write(&theirs, "Rule a.\n").unwrap();
    std::os::unix::fs::symlink(outside.path(), fixture.project.root().join(".claude")).unwrap();
    let rule = fixture.rule("b", "Rule b.\n");
    let recorded = [Recorded {
        route: "a".to_owned(),
        hash: hash::bytes(b"Rule a.\n"),
        names: Names::Mindweaver,
    }];

    for rules in [vec![&rule], vec![]] {
        let refused = fixture.render(&rules, Mode::Fallback, &recorded);
        match refused {
            Err(ClaudeRulesError::LinkedOut { folder, target }) => {
                assert_eq!(folder, fixture.folder());
                assert!(
                    target.starts_with(mindweaver_core::paths::canonical(outside.path()).unwrap())
                );
            }
            other => panic!("{other:?}"),
        }
    }
    assert!(theirs.exists());
    assert!(!outside.path().join("rules/mindweaver/b.md").exists());
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_project_is_followed() {
    let fixture = fixture();
    let kept = fixture.project.root().join("config").join("claude");
    fs::create_dir_all(&kept).unwrap();
    std::os::unix::fs::symlink("config/claude", fixture.project.root().join(".claude")).unwrap();
    let rule = fixture.rule("a", "Rule a.\n");

    let rendered = fixture.render(&[&rule], Mode::Fallback, &[]).unwrap();

    assert_eq!(written(&rendered), [("a", Written::Created)]);
    assert!(kept.join("rules/mindweaver/a.md").is_file());
}

#[test]
fn a_path_scoped_rule_is_said_to_miss_only_the_agents_its_project_names() {
    let fixture = fixture();
    let rule = fixture.rule("rust/errors", SCOPED);
    let unscoped = fixture.rule("git/no-force-push", UNSCOPED);
    let missed = |project: &MindProject| -> Vec<Vec<Agent>> {
        claude_rules::render(
            &fixture.workspace,
            project,
            [&rule, &unscoped],
            Mode::Fallback,
            &[],
            &AgentHomes::default(),
        )
        .unwrap()
        .warnings
        .into_iter()
        .map(|warning| match warning {
            Warning::ClaudeOnly { missed, .. } => missed,
            other => panic!("{other:?}"),
        })
        .collect()
    };
    let project = |name: &str, agents: &[Agent]| {
        let root = fixture.dir.path().join(name);
        fs::create_dir_all(&root).unwrap();
        MindProject::init_settings(&root, agents, Share::Shared, &Directories::default())
            .unwrap()
            .0
    };

    assert_eq!(
        missed(&project("claude", &[Agent::Claude])),
        Vec::<Vec<Agent>>::new()
    );
    assert_eq!(
        missed(&project("two", &[Agent::OpenCode, Agent::Claude])),
        [[Agent::OpenCode]]
    );
    // A project a `mind.toml` marks names none, so any of them may work in
    // it.
    let older = fixture.dir.path().join("older");
    fs::create_dir_all(&older).unwrap();
    let (older, _) = MindProject::init(&older).unwrap();
    assert_eq!(missed(&older), [[Agent::Codex, Agent::OpenCode]]);

    let one = Warning::ClaudeOnly {
        route: "rust/errors".to_owned(),
        missed: vec![Agent::Codex],
    };
    assert!(
        one.to_string().ends_with("codex reads no .claude/rules"),
        "{one}"
    );
}
