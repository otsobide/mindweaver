//! What doctor finds in a project and around it, a check a test: each
//! project, each agent home and each variable set built here, in a
//! temporary folder, never the real home or an agent's own.

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::doctor::{Examined, Machine, Report};
use mindweaver_core::install;
use mindweaver_core::reapply::{self, Change};
use mindweaver_core::{
    claude_md, Agent, AgentHomes, Directories, Kind, MindProject, Share, WeaverWorkspace,
};
use tempfile::TempDir;

/// A workspace at the root of a git work tree, linking `app`, a project
/// below it whose settings name `agents`, and a home of its own holding
/// each agent's folder, empty.
struct Fixture {
    dir: TempDir,
    workspace: WeaverWorkspace,
    project: MindProject,
}

fn fixture(agents: &[Agent]) -> Fixture {
    fixture_in(agents, true)
}

/// [`fixture`], in a git work tree only when `git`.
fn fixture_in(agents: &[Agent], git: bool) -> Fixture {
    let dir = TempDir::new().unwrap();
    if git {
        gix::init(dir.path()).unwrap();
    }
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let root = dir.path().join("app");
    fs::create_dir_all(&root).unwrap();
    let (project, _) =
        MindProject::init_settings(&root, agents, Share::Local, &Directories::default()).unwrap();
    workspace.link(&project).unwrap();
    Fixture {
        dir,
        workspace,
        project,
    }
}

impl Fixture {
    fn root(&self) -> &Path {
        self.project.root()
    }

    /// The fixture's own home, where each agent's folder is.
    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }

    /// Every agent's folder in the fixture's home, none of them made.
    fn homes(&self) -> AgentHomes {
        AgentHomes::defaults_under(&self.home())
    }

    /// Codex's own config in the fixture's home.
    fn codex_config(&self) -> PathBuf {
        self.homes().codex_home.unwrap().join("config.toml")
    }

    fn write(&self, path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// What doctor finds in `app`, with the fixture's homes and `set` the
    /// variables set.
    fn examined_with(&self, set: &[&str]) -> Examined {
        let machine = Machine::new(self.homes(), set.iter().map(|name| (*name).to_owned()));
        let mut report = Report::of_project(&self.project, Some(&self.workspace), &machine);
        report.projects.remove(0)
    }

    fn examined(&self) -> Examined {
        self.examined_with(&[])
    }

    /// Give the workspace's library the rule `route`, holding `text`, and
    /// install it into `app`, as `mindweaver install` does.
    fn install_rule(&self, route: &str, text: &str) {
        let file = self
            .workspace
            .own()
            .directory_for(Kind::Rule)
            .unwrap()
            .join(format!("{route}.md"));
        self.write(&file, text);
        self.install(Kind::Rule, route);
    }

    /// Give the workspace's library the MCP server `name`, defined by
    /// `text`, and install it into `app`.
    fn install_server(&self, name: &str, text: &str) {
        let file = self
            .workspace
            .own()
            .directory_for(Kind::Mcp)
            .unwrap()
            .join(format!("{name}.toml"));
        self.write(&file, text);
        self.install(Kind::Mcp, name);
    }

    /// Install the library's `name` of `kind` into `app`.
    fn install(&self, kind: Kind, name: &str) {
        let ledger = self.workspace.ledger().unwrap();
        let candidate =
            install::offered(&self.workspace, &ledger, &self.project, kind, name, None).unwrap();
        install::install(
            &self.workspace,
            &ledger,
            &self.project,
            &candidate,
            &self.homes(),
        )
        .unwrap();
    }
}

/// A fixture held to have nothing above it that Claude would take for a
/// `CLAUDE.md`, or a test of what Claude reads would pass or fail for the
/// wrong reason.
fn clean(agents: &[Agent]) -> Fixture {
    let fixture = fixture(agents);
    assert!(
        claude_md::reads_agents_md(fixture.dir.path(), &AgentHomes::default()).unwrap(),
        "a CLAUDE.md, .claude/CLAUDE.md or CLAUDE.local.md above {} keeps these tests from \
         telling anything",
        fixture.dir.path().display()
    );
    fixture
}

/// The code of each finding, in order.
fn codes(examined: &Examined) -> Vec<&'static str> {
    examined
        .findings
        .iter()
        .map(|finding| finding.code)
        .collect()
}

/// `root` as the key of Codex's `[projects.<key>]` table, quoted as TOML
/// reads it: a Windows path in a basic string would read as escapes.
fn projects_key(root: &Path) -> String {
    toml_edit::Key::new(root.to_string_lossy())
        .display_repr()
        .into_owned()
}

/// Trust `root` in Codex's config, as `level`.
fn trusting(root: &Path, level: &str) -> String {
    format!(
        "[projects.{}]\ntrust_level = \"{level}\"\n",
        projects_key(root)
    )
}

#[test]
fn a_codex_project_with_a_config_nothing_trusts_is_found() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(&fixture.codex_config(), "model = \"o4\"\n");

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_codex_untrusted"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.path, fixture.codex_config());
    assert_eq!(
        finding.message,
        format!(
            "{}: does not mark {} `trusted`, and Codex reads {} only in a project it trusts",
            fixture.codex_config().display(),
            fixture.root().display(),
            fixture.root().join(".codex/config.toml").display()
        ),
        "named from the file the fix is made in"
    );
    assert_eq!(finding.agents, vec![Agent::Codex]);
    assert!(
        finding.fix.contains(&format!(
            "`[projects.{}]` in {}",
            projects_key(fixture.root()),
            fixture.codex_config().display()
        )),
        "{}",
        finding.fix
    );
    assert!(examined.failures.is_empty(), "{:?}", examined.failures);
}

#[test]
fn a_codex_project_trusted_or_with_no_config_is_not_found() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.codex_config(), "");
    assert!(codes(&fixture.examined()).is_empty());

    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "trusted"),
    );
    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn a_codex_project_its_user_marks_untrusted_is_found_whatever_it_holds() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "untrusted"),
    );

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_codex_distrusted"]);
    assert_eq!(examined.findings[0].path, fixture.codex_config());
}

#[test]
fn trust_is_not_asked_of_a_project_that_does_not_name_codex() {
    let fixture = fixture(&[Agent::Claude]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "untrusted"),
    );

    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn a_codex_config_that_does_not_parse_is_a_failure_and_no_finding() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    fixture.write(&fixture.codex_config(), "[projects\n");

    let examined = fixture.examined();

    assert!(codes(&examined).is_empty());
    assert_eq!(examined.failures.len(), 1, "{:?}", examined.failures);
}

#[test]
fn with_no_codex_home_a_config_nothing_trusts_says_so() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join(".codex/config.toml"), "");
    let machine = Machine::new(AgentHomes::default(), Vec::new());

    let report = Report::of_project(&fixture.project, Some(&fixture.workspace), &machine);

    let finding = &report.projects[0].findings[0];
    assert_eq!(finding.code, "doctor_codex_untrusted");
    assert_eq!(finding.path, fixture.root().join(".codex/config.toml"));
    assert!(
        finding
            .message
            .starts_with(&format!("{}: ", finding.path.display())),
        "{}",
        finding.message
    );
    assert!(finding.fix.contains("--codex-home"), "{}", finding.fix);
}

#[test]
fn every_project_a_workspace_links_is_examined_once_and_a_stale_one_says_why() {
    let mut fixture = fixture(&[Agent::Claude]);
    let gone = fixture.dir.path().join("gone");
    let (other, _) = MindProject::init_settings(
        &gone,
        &[Agent::Claude],
        Share::Local,
        &Directories::default(),
    )
    .unwrap();
    fixture.workspace.link(&other).unwrap();
    fs::remove_dir_all(&gone).unwrap();
    // A second spelling of `app`, after the stale one: an entry of its own,
    // and the same project.
    let marker = fixture.workspace.config_path();
    let text = fs::read_to_string(&marker).unwrap();
    assert!(text.contains("\"gone\""), "{text}");
    fs::write(&marker, text.replacen("\"gone\"", "\"gone\", \"./app\"", 1)).unwrap();
    let workspace = WeaverWorkspace::open(fixture.dir.path()).unwrap();
    assert_eq!(workspace.members().len(), 3);
    let machine = Machine::new(fixture.homes(), Vec::new());

    let report = Report::of_workspace(&workspace, &machine);

    let entries: Vec<Option<&str>> = report
        .projects
        .iter()
        .map(|examined| examined.entry.as_deref())
        .collect();
    assert_eq!(entries, vec![Some("app"), Some("gone")]);
    assert!(report.projects[0].failures.is_empty());
    assert_eq!(report.projects[1].name, None);
    assert_eq!(report.projects[1].failures.len(), 1);
}

#[test]
fn an_agents_override_hiding_agents_md_from_codex_is_found() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join("AGENTS.md"), "# Working here\n");
    fixture.write(&fixture.root().join("AGENTS.override.md"), "# Mine\n");

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_codex_overridden"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.path, fixture.root().join("AGENTS.override.md"));
    assert!(
        finding
            .fix
            .contains(&fixture.root().join("AGENTS.md").display().to_string()),
        "{}",
        finding.fix
    );
}

#[test]
fn an_agents_override_with_no_agents_md_to_hide_is_not_found() {
    let fixture = fixture(&[Agent::Codex]);
    fixture.write(&fixture.root().join("AGENTS.override.md"), "# Mine\n");

    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn a_codex_project_in_no_git_work_tree_is_found() {
    let fixture = fixture_in(&[Agent::Codex], false);

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_codex_no_git_root"]);
    assert_eq!(examined.findings[0].path, fixture.root());
}

#[test]
fn no_git_root_is_not_said_of_a_project_codex_does_not_work_in() {
    let fixture = fixture_in(&[Agent::Claude, Agent::OpenCode], false);

    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn agents_md_files_near_and_past_codex_s_budget_are_found() {
    let fixture = fixture(&[Agent::Codex]);
    let agents_md = fixture.root().join("AGENTS.md");
    // Above the project, at the git root, a file Codex reads first.
    fixture.write(
        &fixture.dir.path().join("AGENTS.md"),
        &"a".repeat(20 * 1024),
    );
    fixture.write(&agents_md, &"b".repeat(9 * 1024));

    let near = fixture.examined();
    assert_eq!(codes(&near), vec!["doctor_codex_near_budget"]);
    assert_eq!(near.findings[0].path, agents_md);
    assert!(near.findings[0].message.contains("29696 bytes"));

    fixture.write(&agents_md, &"b".repeat(13 * 1024));
    let past = fixture.examined();
    assert_eq!(codes(&past), vec!["doctor_codex_over_budget"]);
    assert!(past.findings[0].fix.contains("project_doc_max_bytes"));
}

#[test]
fn a_claude_local_md_made_after_the_rules_keeps_them_from_claude_and_is_found() {
    let fixture = clean(&[Agent::Claude]);
    fixture.install_rule("commits", "Write commit messages in the imperative.\n");
    assert!(codes(&fixture.examined()).is_empty());

    let local = fixture.root().join("CLAUDE.local.md");
    fixture.write(&local, "# Mine\n");
    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_claude_misses_rules"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.path, local);
    assert_eq!(finding.agents, vec![Agent::Claude]);
    assert!(
        finding.message.ends_with(": commits"),
        "{}",
        finding.message
    );
    assert!(
        finding.fix.contains("`mind install rule:commits`"),
        "{}",
        finding.fix
    );

    // What the fix says: installed again, every rule is rendered for
    // Claude as the project now stands.
    fixture.install_rule("commits", "Write commit messages in the imperative.\n");
    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn an_import_of_agents_md_where_claude_reads_its_own_file_is_not_found() {
    let fixture = clean(&[Agent::Claude]);
    fixture.install_rule("commits", "Write commit messages in the imperative.\n");
    fixture.write(&fixture.root().join("CLAUDE.local.md"), "@AGENTS.md\n");

    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn rules_kept_from_claude_are_not_asked_of_a_project_that_does_not_name_claude() {
    let fixture = clean(&[Agent::Codex]);
    fixture.install_rule("commits", "Write commit messages in the imperative.\n");
    fixture.write(&fixture.root().join("CLAUDE.local.md"), "# Mine\n");

    assert!(codes(&fixture.examined()).is_empty());
}

/// Put a skill `name` reading `description` in `folder`.
fn skill_in(fixture: &Fixture, folder: &Path, name: &str, description: &str) -> PathBuf {
    let copy = folder.join(name);
    fixture.write(&copy.join("SKILL.md"), &common::skill(name, description));
    copy
}

#[test]
fn copies_of_one_skill_that_differ_in_folders_opencode_reads_alike_are_found() {
    let fixture = fixture(&[Agent::OpenCode]);
    let agents = skill_in(
        &fixture,
        &fixture.root().join(".agents").join("skills"),
        "review",
        "Read it",
    );
    let claude = skill_in(
        &fixture,
        &fixture.root().join(".claude").join("skills"),
        "review",
        "Read it twice",
    );

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_copies_differ"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.agents, vec![Agent::OpenCode]);
    assert_eq!(finding.path, agents);
    assert!(
        finding
            .message
            .contains(&format!("also at {}", claude.display())),
        "{}",
        finding.message
    );
}

#[test]
fn copies_that_hold_the_same_or_that_no_one_agent_reads_both_of_are_not_found() {
    let fixture = fixture(&[Agent::Claude, Agent::Codex, Agent::OpenCode]);
    skill_in(
        &fixture,
        &fixture.root().join(".agents").join("skills"),
        "review",
        "Read it",
    );
    skill_in(
        &fixture,
        &fixture.root().join(".claude/skills"),
        "review",
        "Read it",
    );
    assert!(codes(&fixture.examined()).is_empty());

    let apart = self::fixture(&[Agent::Claude, Agent::Codex]);
    skill_in(
        &apart,
        &apart.root().join(".agents/skills"),
        "review",
        "Read it",
    );
    skill_in(
        &apart,
        &apart.root().join(".claude/skills"),
        "review",
        "Read it twice",
    );
    assert!(codes(&apart.examined()).is_empty());
}

#[test]
fn a_project_skill_a_different_one_at_user_scope_shadows_is_found_for_each_agent_reading_both() {
    let fixture = fixture(&[Agent::Claude, Agent::Codex, Agent::OpenCode]);
    let homes = fixture.homes();
    let ours = skill_in(
        &fixture,
        &fixture.root().join(".agents/skills"),
        "review",
        "Read it",
    );
    let theirs = skill_in(
        &fixture,
        &homes.agents_dir.unwrap().join("skills"),
        "review",
        "Read it my way",
    );

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_user_skill_differs"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.path, ours);
    assert_eq!(finding.agents, vec![Agent::Codex, Agent::OpenCode]);
    assert!(
        finding
            .message
            .contains(&format!("at user scope, at {}", theirs.display())),
        "{}",
        finding.message
    );
}

#[test]
fn a_skill_at_user_scope_holding_the_same_or_read_by_no_agent_of_the_project_is_not_found() {
    let fixture = fixture(&[Agent::Claude]);
    let homes = fixture.homes();
    skill_in(
        &fixture,
        &fixture.root().join(".claude/skills"),
        "review",
        "Read it",
    );
    skill_in(
        &fixture,
        &homes.claude_config_dir.unwrap().join("skills"),
        "review",
        "Read it",
    );
    // Claude reads no `~/.agents/skills`.
    skill_in(
        &fixture,
        &homes.agents_dir.unwrap().join("skills"),
        "review",
        "Read it my way",
    );

    assert!(codes(&fixture.examined()).is_empty());
}

#[test]
fn a_skill_in_claude_s_own_folder_differing_from_the_project_s_is_found_for_claude() {
    let fixture = fixture(&[Agent::Claude]);
    let ours = skill_in(
        &fixture,
        &fixture.root().join(".claude/skills"),
        "review",
        "Read it",
    );
    skill_in(
        &fixture,
        &fixture.homes().claude_config_dir.unwrap().join("skills"),
        "review",
        "Read it my way",
    );

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_user_skill_differs"]);
    assert_eq!(examined.findings[0].path, ours);
    assert_eq!(examined.findings[0].agents, vec![Agent::Claude]);
}

/// A server started as a program, its environment given the secret read
/// from `variable`.
fn server(name: &str, variable: &str) -> String {
    format!(
        "format = 1\nname = \"{name}\"\ndescription = \"Issues and code\"\n\n\
         [transport]\ntype = \"stdio\"\ncommand = \"github-mcp\"\n\
         env = {{ {variable} = \"{{{variable}}}\" }}\n\n\
         [[inputs]]\nname = \"{variable}\"\nkind = \"env\"\nsecret = true\n\
         env_var = \"{variable}\"\n"
    )
}

/// The findings of `examined` of `code`.
fn of<'a>(examined: &'a Examined, code: &str) -> Vec<&'a mindweaver_core::doctor::Finding> {
    examined
        .findings
        .iter()
        .filter(|finding| finding.code == code)
        .collect()
}

#[test]
fn a_variable_an_installed_server_reads_that_is_not_set_is_found_as_a_secret() {
    let fixture = fixture(&[Agent::Claude, Agent::OpenCode]);
    fixture.install_server("github", &server("github", "DOCTOR_TEST_TOKEN"));

    let unset = fixture.examined();

    assert_eq!(codes(&unset), vec!["doctor_variable_unset"]);
    let finding = &unset.findings[0];
    assert_eq!(finding.agents, vec![Agent::Claude, Agent::OpenCode]);
    assert_eq!(finding.path, fixture.root().join(".mcp.json"));
    assert!(
        finding
            .message
            .contains("`github` is given DOCTOR_TEST_TOKEN (a secret) where Claude Code and OpenCode start it"),
        "{}",
        finding.message
    );

    assert!(codes(&fixture.examined_with(&["DOCTOR_TEST_TOKEN"])).is_empty());
}

#[test]
fn a_server_of_an_installed_one_s_name_at_codex_s_and_opencode_s_user_scope_is_found() {
    let fixture = fixture(&[Agent::Claude, Agent::Codex, Agent::OpenCode]);
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "trusted"),
    );
    fixture.install_server("github", &server("github", "DOCTOR_TEST_TOKEN"));
    let set = ["DOCTOR_TEST_TOKEN"];
    assert!(codes(&fixture.examined_with(&set)).is_empty());

    fixture.write(
        &fixture.codex_config(),
        &format!(
            "{}\n[mcp_servers.github]\nurl = \"https://example.com/mcp\"\n",
            trusting(fixture.root(), "trusted")
        ),
    );
    let opencode = fixture
        .homes()
        .opencode_config_dir
        .unwrap()
        .join("opencode.jsonc");
    fixture.write(
        &opencode,
        "// Mine.\n{\"mcp\": {\"servers\": {\"github\": {\"type\": \"remote\"}}},}\n",
    );
    let examined = fixture.examined_with(&set);

    let found = of(&examined, "doctor_server_at_user_scope");
    let at: Vec<(&[Agent], &Path)> = found
        .iter()
        .map(|finding| (finding.agents.as_slice(), finding.path.as_path()))
        .collect();
    assert_eq!(
        at,
        vec![
            (&[Agent::Codex][..], fixture.codex_config().as_path()),
            (&[Agent::OpenCode][..], opencode.as_path()),
        ]
    );
    assert_eq!(codes(&examined).len(), 2, "{:?}", codes(&examined));
    assert!(
        found[0].fix.contains("uninstall `mcp:github`"),
        "{}",
        found[0].fix
    );
}

#[test]
fn a_user_config_that_does_not_parse_is_said_once_however_many_servers_ask_of_it() {
    let fixture = fixture(&[Agent::Codex, Agent::OpenCode]);
    fixture.write(
        &fixture.codex_config(),
        &trusting(fixture.root(), "trusted"),
    );
    fixture.install_server("github", &server("github", "DOCTOR_TEST_TOKEN"));
    fixture.install_server("tracker", &server("tracker", "DOCTOR_TEST_TOKEN"));
    fixture.write(&fixture.codex_config(), "[projects\n");
    let opencode = fixture
        .homes()
        .opencode_config_dir
        .unwrap()
        .join("opencode.json");
    fixture.write(&opencode, "{\"mcp\": ");

    let examined = fixture.examined_with(&["DOCTOR_TEST_TOKEN"]);

    assert!(codes(&examined).is_empty(), "{:?}", codes(&examined));
    let said: Vec<String> = examined
        .failures
        .iter()
        .map(|failure| failure.message.to_string())
        .collect();
    assert_eq!(said.len(), 2, "{said:?}");
    assert!(said[0].starts_with(&fixture.codex_config().display().to_string()));
    assert!(said[1].starts_with(&opencode.display().to_string()));
}

#[test]
fn a_stage_an_install_left_in_mind_tmp_is_found_and_left_there() {
    let fixture = fixture(&[Agent::Claude]);
    let tmp = fixture.root().join(".mind/tmp");
    let stage = tmp.join("4242-00112233445566ff");
    fs::create_dir_all(stage.join("review")).unwrap();
    // Not named as a stage is: somebody else's.
    fs::create_dir_all(tmp.join("notes")).unwrap();

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_leftover_stage"]);
    assert_eq!(examined.findings[0].path, stage);
    assert!(examined.findings[0].agents.is_empty());
    assert!(stage.is_dir(), "doctor sweeps nothing");
}

#[test]
fn an_older_version_left_beside_a_copy_where_an_agent_reads_is_found_by_its_copy() {
    let fixture = fixture(&[Agent::Claude, Agent::OpenCode]);
    let new = fixture.root().join(".claude/skills/.review.mindweaver-new");
    let old = fixture.root().join(".agents/skills/.review.mindflayer-old");
    fs::create_dir_all(&new).unwrap();
    fs::create_dir_all(&old).unwrap();
    // Neither a copy's older version nor a folder.
    fixture.write(
        &fixture.root().join(".claude/skills/.notes.mindweaver-new"),
        "",
    );

    let examined = fixture.examined();

    assert_eq!(
        codes(&examined),
        vec!["doctor_leftover_beside", "doctor_leftover_beside"]
    );
    let paths: Vec<&Path> = examined
        .findings
        .iter()
        .map(|finding| finding.path.as_path())
        .collect();
    assert_eq!(paths, vec![old.as_path(), new.as_path()]);
    assert!(examined.findings[0].fix.contains("`review`"));
    assert!(new.is_dir() && old.is_dir(), "doctor removes nothing");
}

#[test]
fn a_kind_installed_where_some_of_its_agents_read_none_of_it_is_found_as_status_says() {
    let fixture = fixture(&[Agent::Claude, Agent::Codex]);
    reapply::apply(
        Some(&fixture.workspace),
        &fixture.project,
        &Change::Targets {
            kind: Kind::Skill,
            folders: Some(vec![PathBuf::from(".skills")]),
        },
        &fixture.homes(),
    )
    .unwrap();
    let project = MindProject::open(fixture.root()).unwrap();
    let machine = Machine::new(fixture.homes(), Vec::new());

    let report = Report::of_project(&project, Some(&fixture.workspace), &machine);

    let examined = &report.projects[0];
    assert_eq!(codes(examined), vec!["doctor_unreached"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.path, fixture.root().join(".skills"));
    assert_eq!(finding.agents, vec![Agent::Claude, Agent::Codex]);
    assert!(
        finding.fix.contains("`mind targets reset skill`"),
        "{}",
        finding.fix
    );
}

#[test]
fn a_rule_about_some_files_codex_reads_nowhere_is_found_as_status_says() {
    let fixture = clean(&[Agent::Claude, Agent::Codex]);
    fixture.install_rule(
        "errors",
        "---\npaths:\n  - \"src/**/*.rs\"\n---\nReturn errors; never panic in a library.\n",
    );

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_missed"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.agents, vec![Agent::Codex]);
    assert_eq!(finding.path, fixture.root().join("AGENTS.md"));
    assert!(
        finding
            .message
            .contains("Codex does not read rule `errors`"),
        "{}",
        finding.message
    );
}

#[test]
fn a_rule_a_tracked_agents_md_keeps_from_codex_in_a_local_project_is_found_with_its_fix() {
    let fixture = clean(&[Agent::Claude, Agent::Codex]);
    let agents_md = fixture.root().join("AGENTS.md");
    fixture.write(&agents_md, "# Working here\n");
    // Local, and git tracks the team's AGENTS.md, so the block is kept
    // out of it (D-02): Claude is given the rule in its own folder, and
    // Codex nothing.
    common::indexed(fixture.dir.path());
    fixture.install_rule("commits", "Write commit messages in the imperative.\n");

    let examined = fixture.examined();

    assert_eq!(codes(&examined), vec!["doctor_missed"]);
    let finding = &examined.findings[0];
    assert_eq!(finding.agents, vec![Agent::Codex]);
    assert_eq!(finding.path, agents_md);
    assert_eq!(
        finding.message,
        format!(
            "{}: Codex does not read rule `commits`, which the project received",
            agents_md.display()
        )
    );
    assert!(
        finding.fix.contains("(`mind share shared`)")
            && finding.fix.contains("(`mind share local --edit-tracked`)"),
        "{}",
        finding.fix
    );
}

#[test]
fn opencode_s_folders_in_the_home_are_read_beside_those_its_variables_name() {
    let fixture = fixture(&[Agent::OpenCode]);
    fixture.install_server("github", &server("github", "DOCTOR_TEST_TOKEN"));
    let home = fixture.home();
    // As the front end hands them over while `$CLAUDE_CONFIG_DIR` and
    // `$OPENCODE_CONFIG_DIR` name folders of their own.
    let homes = AgentHomes {
        claude_config_dir: Some(home.join("claude")),
        opencode_config_dir: Some(home.join("opencode")),
        claude_dir_in_home: Some(home.join(".claude")),
        opencode_dir_in_home: Some(home.join(".config").join("opencode")),
        ..fixture.homes()
    };
    let ours = skill_in(
        &fixture,
        &fixture.root().join(".opencode").join("skills"),
        "review",
        "Read it",
    );
    let theirs = skill_in(
        &fixture,
        &home.join(".claude").join("skills"),
        "review",
        "Mine",
    );
    // Claude's own, which OpenCode does not read.
    skill_in(
        &fixture,
        &home.join("claude").join("skills"),
        "review",
        "Not read",
    );
    let global = home.join(".config").join("opencode").join("opencode.json");
    fixture.write(&global, "{\"mcp\": {\"github\": {\"type\": \"remote\"}}}\n");
    let machine = Machine::new(homes, [String::from("DOCTOR_TEST_TOKEN")]);

    let report = Report::of_project(&fixture.project, Some(&fixture.workspace), &machine);

    let examined = &report.projects[0];
    assert!(examined.failures.is_empty(), "{:?}", examined.failures);
    let found: Vec<(&str, &Path)> = examined
        .findings
        .iter()
        .map(|finding| (finding.code, finding.path.as_path()))
        .collect();
    assert_eq!(
        found,
        vec![
            ("doctor_user_skill_differs", ours.as_path()),
            ("doctor_server_at_user_scope", global.as_path()),
        ]
    );
    assert!(
        examined.findings[0]
            .message
            .contains(&format!("at {}", theirs.display())),
        "{}",
        examined.findings[0].message
    );
}
