//! MCP servers as a kind of the library: where each marker keeps them, as
//! it was written, and what listing, showing, validating, renaming and
//! removing one does, through the lifecycle every kind goes through.
//!
//! A marker written before the kind came says nothing of it, and must not
//! be read as if it named a folder: a workspace keeps its own in its
//! library unless its marker names a folder for them (D-05), and a project
//! only where its settings name one, so a folder called `mcp` that holds a
//! server's code is nobody's definitions.

use std::fs;
use std::path::{Path, PathBuf};

use mindweaver_core::create::{create_server, Start};
use mindweaver_core::lifecycle;
use mindweaver_core::mcp::{Address, Given, McpDefinition, Program};
use mindweaver_core::mother::{
    McpMother, MindflayerMother, ProjectMother, SettingsMother, SkillMother, UrlMother,
    WorkspaceMother,
};
use mindweaver_core::reading::Shown;
use mindweaver_core::template;
use mindweaver_core::{
    create, Agent, Catalog, CreateError, Declared, Directories, DiscoveryFailure, Kind,
    LifecycleError, MarkerFile, MindProject, Reference, Share, ValidationIssue, WeaverWorkspace,
    WorkspaceError,
};
use tempfile::TempDir;

/// Write each of `files`, a route and what it holds, under `root`.
fn write(root: &Path, files: &[(&str, &str)]) {
    for (route, text) in files {
        let path = root.join(route);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

/// What a repository implementing a server keeps at `mcp/`: its code, and
/// definitions it ships for its users, none of them the project's own.
fn unrelated_mcp_folder(root: &Path) {
    write(
        root,
        &[
            (
                "mcp/Cargo.toml",
                "[package]\nname = \"server\"\nversion = \"0.1.0\"\n",
            ),
            ("mcp/src/main.rs", "fn main() {}\n"),
            ("mcp/github.toml", &McpMother::stdio("github")),
            ("mcp/broken.toml", &McpMother::unreadable()),
        ],
    );
}

/// Every MCP server `holder` is read as holding, by name, with what could
/// not be read.
fn servers(holder: &MindProject) -> (Vec<String>, Vec<String>) {
    let catalog = Catalog::discover_kinds(std::slice::from_ref(holder), &[Kind::Mcp]);
    let names = catalog
        .artifacts()
        .iter()
        .map(|artifact| artifact.name().to_string())
        .collect();
    let failures = catalog
        .failures()
        .iter()
        .map(|failure| failure.to_string())
        .collect();
    (names, failures)
}

// ---------------------------------------------------------------------------
// Where each marker keeps them
// ---------------------------------------------------------------------------

#[test]
fn a_new_workspace_keeps_its_servers_in_its_library_and_never_beside_its_projects() {
    let (dir, workspace) = WorkspaceMother::empty();
    let library = dir.path().join(".mindweaver/library/mcp");
    write(&library, &[("deploy.toml", &McpMother::http("deploy"))]);
    unrelated_mcp_folder(dir.path());

    assert_eq!(workspace.own().directory_for(Kind::Mcp), Some(library));
    assert_eq!(
        servers(&workspace.own()),
        (vec![String::from("deploy")], vec![])
    );
}

#[test]
fn a_marker_a_mindflayer_build_wrote_reads_its_library_and_never_the_root() {
    let (dir, workspace) = MindflayerMother::empty();
    let marker = fs::read_to_string(dir.path().join(".mindflayer/flayer.toml")).unwrap();
    assert!(!marker.contains("mcp"), "{marker}");
    write(
        &dir.path().join(".mindflayer/library/mcp"),
        &[("deploy.toml", &McpMother::sse("deploy"))],
    );
    unrelated_mcp_folder(dir.path());

    assert_eq!(
        workspace.own().directory_for(Kind::Mcp),
        Some(dir.path().join(".mindflayer/library/mcp"))
    );
    assert_eq!(
        servers(&workspace.own()),
        (vec![String::from("deploy")], vec![])
    );
}

#[test]
fn a_workspace_keeping_its_skills_beside_its_projects_keeps_its_servers_in_its_library() {
    let dir = TempDir::new().unwrap();
    let beside = Directories::default()
        .with(Kind::Skill, "skills")
        .with(Kind::Rule, "rules");
    let (workspace, _) = WeaverWorkspace::init_with(dir.path(), &beside).unwrap();
    unrelated_mcp_folder(dir.path());

    assert_eq!(
        workspace.own().directory_for(Kind::Mcp),
        Some(dir.path().join(".mindweaver/library/mcp"))
    );
    assert_eq!(servers(&workspace.own()), (vec![], vec![]));
}

#[test]
fn a_workspace_marker_naming_a_folder_for_servers_is_followed_as_one_naming_any_kind_is() {
    let dir = TempDir::new().unwrap();
    let named = Directories::default().with(Kind::Mcp, "defs");
    let (workspace, _) = WeaverWorkspace::init_with(dir.path(), &named).unwrap();
    write(
        &dir.path().join("defs"),
        &[("deploy.toml", &McpMother::http("deploy"))],
    );
    write(
        &dir.path().join(".mindweaver/library/mcp"),
        &[("elsewhere.toml", &McpMother::sse("elsewhere"))],
    );

    assert_eq!(
        workspace.own().directory_for(Kind::Mcp),
        Some(dir.path().join("defs"))
    );
    assert_eq!(
        servers(&workspace.own()),
        (vec![String::from("deploy")], vec![])
    );
}

#[test]
fn a_new_workspace_marker_names_no_folder_for_servers_and_says_a_kind_unnamed_is_in_the_library() {
    let (dir, _) = WorkspaceMother::empty();

    let marker = fs::read_to_string(dir.path().join(".mindweaver/weaver.toml")).unwrap();

    assert!(!marker.contains("mcp"), "{marker}");
    assert!(
        marker.contains("a kind not named\n# here is kept there too"),
        "{marker}"
    );
}

#[test]
fn a_version_1_or_2_project_with_an_unrelated_mcp_folder_lists_nothing_of_it_and_says_nothing() {
    let dir = TempDir::new().unwrap();
    let first = ProjectMother::version_1(&dir.path().join("first"));
    let second = ProjectMother::older(&dir.path().join("second"));

    for project in [first, second] {
        unrelated_mcp_folder(project.root());
        // Where a version 1 marker kept every kind it knew, too.
        write(
            &project.mind_dir(),
            &[("mcp/github.toml", &McpMother::stdio("github"))],
        );

        assert_eq!(project.directory_for(Kind::Mcp), None, "{project:?}");
        assert_eq!(
            Catalog::folders(&project, Kind::Mcp),
            Vec::<std::path::PathBuf>::new()
        );
        assert_eq!(servers(&project), (vec![], vec![]), "{project:?}");
        let every = Catalog::discover(std::slice::from_ref(&project));
        assert!(
            every
                .artifacts()
                .iter()
                .all(|artifact| artifact.kind() != Kind::Mcp),
            "{project:?}"
        );
        assert_eq!(every.failures().len(), 0, "{:?}", every.failures());
    }
}

#[test]
fn a_project_marked_by_its_settings_reads_servers_only_where_they_name_a_folder() {
    let dir = TempDir::new().unwrap();
    let silent = ProjectMother::create(&dir.path().join("silent"), &[], Share::Local);
    let naming = ProjectMother::with_mcp(&dir.path().join("naming"), &[], Share::Local);

    unrelated_mcp_folder(silent.root());
    assert_eq!(silent.directory_for(Kind::Mcp), None);
    assert_eq!(servers(&silent), (vec![], vec![]));

    write(
        naming.root(),
        &[("mcp/github.toml", &McpMother::stdio("github"))],
    );
    assert_eq!(
        naming.directory_for(Kind::Mcp),
        Some(naming.root().join("mcp"))
    );
    assert_eq!(servers(&naming), (vec![String::from("github")], vec![]));
}

#[test]
fn a_project_a_settings_json_marks_reads_servers_only_where_its_directories_name_a_folder() {
    let dir = TempDir::new().unwrap();
    let authoring = SettingsMother::authoring();
    let silent = ProjectMother::json(&dir.path().join("silent"), &authoring);
    let naming = ProjectMother::json(
        &dir.path().join("naming"),
        &authoring.with(Kind::Mcp, "defs"),
    );

    unrelated_mcp_folder(silent.root());
    assert_eq!(silent.directory_for(Kind::Mcp), None);
    assert_eq!(Catalog::folders(&silent, Kind::Mcp), Vec::<PathBuf>::new());
    assert_eq!(servers(&silent), (vec![], vec![]));

    unrelated_mcp_folder(naming.root());
    write(
        naming.root(),
        &[("defs/github.toml", &McpMother::stdio("github"))],
    );
    assert_eq!(naming.marker_file(), MarkerFile::SettingsJson);
    assert_eq!(
        naming.directory_for(Kind::Mcp),
        Some(naming.root().join("defs"))
    );
    assert_eq!(servers(&naming), (vec![String::from("github")], vec![]));
}

#[test]
fn mind_init_names_no_folder_for_servers_and_makes_none() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(dir.path(), &[], Share::Local);

    let settings = fs::read_to_string(project.marker_path()).unwrap();

    assert!(!settings.contains("mcp"), "{settings}");
    assert!(!dir.path().join("mcp").exists());
}

// ---------------------------------------------------------------------------
// Listing and validating
// ---------------------------------------------------------------------------

/// A project whose settings name `mcp` for its servers, and that folder.
fn authoring() -> (TempDir, MindProject, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::with_mcp(&dir.path().join("app"), &[], Share::Local);
    let folder = project.directory_for(Kind::Mcp).unwrap();
    (dir, project, folder)
}

#[test]
fn a_server_is_one_file_directly_in_its_folder_named_by_its_stem() {
    let (_dir, project, folder) = authoring();
    let description = SkillMother::description();
    write(
        &folder,
        &[
            (
                "github.toml",
                &McpMother::create("github", &description, "stdio"),
            ),
            ("linear.toml", &McpMother::http("linear")),
            ("team/nested.toml", &McpMother::sse("nested")),
            (".hidden.toml", &McpMother::sse("hidden")),
            ("notes.md", "# Not a definition\n"),
        ],
    );

    let catalog = Catalog::discover(std::slice::from_ref(&project));

    let listed: Vec<(Kind, &str, Option<&str>)> = catalog
        .artifacts()
        .iter()
        .filter(|artifact| artifact.kind() == Kind::Mcp)
        .map(|artifact| {
            (
                artifact.kind(),
                artifact.name().as_str(),
                artifact.summary(),
            )
        })
        .collect();
    assert_eq!(listed[0], (Kind::Mcp, "github", Some(description.as_str())));
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert_eq!(listed[1].1, "linear");
    let github = &catalog.artifacts()[0];
    assert_eq!(github.path(), folder.join("github.toml"));
    assert_eq!(github.file(), folder.join("github.toml"));
    assert!(matches!(github.declared(), Declared::Mcp(_)));
    assert_eq!(github.validate(), []);
    assert_eq!(catalog.failures().len(), 0, "{:?}", catalog.failures());
}

#[test]
fn a_file_that_is_no_definition_is_a_failure_by_its_code_and_hides_none_beside_it() {
    let (_dir, project, folder) = authoring();
    let newer = McpMother::in_format("later", "2");
    write(
        &folder,
        &[
            ("github.toml", &McpMother::stdio("github")),
            ("later.toml", &newer),
            ("broken.toml", &McpMother::unreadable()),
        ],
    );

    let catalog = Catalog::discover_kinds(std::slice::from_ref(&project), &[Kind::Mcp]);

    let names: Vec<&str> = catalog
        .artifacts()
        .iter()
        .map(|artifact| artifact.name().as_str())
        .collect();
    assert_eq!(names, ["github"]);
    let codes: Vec<(String, &str)> = catalog
        .failures()
        .iter()
        .map(|failure| {
            let DiscoveryFailure::Artifact(error) = failure else {
                panic!("{failure:?}");
            };
            (
                error
                    .path()
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                error.code(),
            )
        })
        .collect();
    assert_eq!(
        codes,
        [
            (String::from("broken.toml"), "mcp_unparsed"),
            (String::from("later.toml"), "mcp_format_too_new"),
        ]
    );
    assert!(
        catalog.failures()[1]
            .to_string()
            .ends_with("written in format 2, by a newer Mindweaver; this one reads format 1"),
        "{}",
        catalog.failures()[1]
    );
}

#[test]
fn validate_holds_a_server_to_its_file_its_name_rule_and_its_format() {
    let (_dir, project, folder) = authoring();
    write(
        &folder,
        &[
            ("gh.toml", &McpMother::stdio("github")),
            ("bad.name.toml", &McpMother::sse("bad.name")),
            ("Workspace.toml", &McpMother::http("Workspace")),
            (
                "anthropic-skills.toml",
                &McpMother::stdio("anthropic-skills"),
            ),
            ("My_Server.toml", &McpMother::stdio("My_Server")),
            ("bare.toml", &McpMother::incomplete("bare", "http")),
        ],
    );

    let catalog = Catalog::discover_kinds(std::slice::from_ref(&project), &[Kind::Mcp]);
    let issues = |name: &str| {
        catalog
            .artifacts()
            .iter()
            .find(|artifact| artifact.name() == name)
            .unwrap()
            .validate()
    };

    assert_eq!(
        issues("gh"),
        [ValidationIssue::NameFileMismatch {
            name: String::from("github"),
            file: String::from("gh.toml"),
        }]
    );
    // Held to the keys an agent takes, not to kebab-case: capitals and
    // underscores are a server's, a dot is no agent's.
    assert_eq!(issues("My_Server"), []);
    assert_eq!(
        issues("bad.name"),
        [ValidationIssue::McpNameNotAKey {
            name: String::from("bad.name"),
        }]
    );
    // A name an agent keeps for itself is refused in any case, and one it
    // uses for its own only warned of.
    assert_eq!(
        issues("Workspace"),
        [ValidationIssue::McpNameReserved {
            name: String::from("Workspace"),
            agent: Agent::Claude,
            reserved: String::from("workspace"),
        }]
    );
    let claimed = issues("anthropic-skills");
    assert!(
        matches!(
            &claimed[..],
            [ValidationIssue::McpNameClaimed {
                agent: Agent::Claude,
                ..
            }]
        ),
        "{claimed:?}"
    );
    assert!(claimed[0].is_warning());
    assert_eq!(
        issues("bare"),
        [ValidationIssue::TransportIncomplete {
            transport: String::from("http"),
            field: String::from("url"),
        }]
    );
    assert!(issues("gh").iter().all(|issue| !issue.is_warning()));
}

#[test]
fn show_says_what_it_is_for_and_prints_its_file_whole() {
    let (_dir, project, folder) = authoring();
    let description = SkillMother::description();
    let text = McpMother::create("github", &description, "http");
    write(&folder, &[("github.toml", &text)]);
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();

    let shown = Shown::of(target.artifact.as_ref().unwrap()).unwrap();

    assert_eq!(shown.kind, Kind::Mcp);
    assert_eq!(shown.description.as_deref(), Some(description.as_str()));
    assert_eq!(shown.summary.as_deref(), Some(description.as_str()));
    assert_eq!(shown.file, folder.join("github.toml"));
    assert_eq!(shown.body, text);
    assert_eq!((shown.allowed_tools, shown.paths), (None, None));
}

// ---------------------------------------------------------------------------
// Writing: add, rename, remove
// ---------------------------------------------------------------------------

#[test]
fn add_is_refused_for_a_server_saying_where_to_write_one() {
    let (_dir, project, folder) = authoring();

    let error = create(&project, Kind::Mcp, "github", "GitHub").unwrap_err();

    assert!(
        matches!(error, CreateError::McpUnscaffolded { .. }),
        "{error}"
    );
    assert_eq!(error.code(), "add_mcp_unscaffolded");
    assert!(
        error.to_string().ends_with(&format!(
            "write its definition as <name>.toml in {}",
            folder.display()
        )),
        "{error}"
    );
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
}

/// Every file `folder` holds, by its name, with what it holds: what a
/// refusal is held to leaving as it was.
fn held(folder: &Path) -> Vec<(String, String)> {
    let mut held: Vec<(String, String)> = fs::read_dir(folder)
        .map(|entries| {
            entries
                .map(|entry| {
                    let path = entry.unwrap().path();
                    let text = fs::read_to_string(&path).unwrap_or_default();
                    (
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        text,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    held.sort();
    held
}

/// Add the server `name` to `project`, told `given` of how it is started
/// or reached.
fn add_server(
    project: &MindProject,
    name: &str,
    given: &Given,
) -> Result<lifecycle::Added, CreateError> {
    lifecycle::add(
        None,
        project,
        Kind::Mcp,
        name,
        &SkillMother::description(),
        Start::Server(given),
    )
}

#[test]
fn add_writes_a_server_from_what_it_is_told_that_validate_finds_nothing_wrong_with() {
    let (_dir, project, folder) = authoring();
    for _ in 0..20 {
        let given = McpMother::given();
        let name = McpMother::name();
        let description = SkillMother::description();

        let added = lifecycle::add(
            None,
            &project,
            Kind::Mcp,
            &name,
            &description,
            Start::Server(&given),
        )
        .unwrap();

        let file = folder.join(format!("{name}.toml"));
        assert_eq!(added.artifact.file(), file);
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            given.text(&name, &description)
        );
        let (names, failures) = servers(&project);
        assert!(names.contains(&name) && failures.is_empty(), "{failures:?}");
        let found = lifecycle::find(&project, &Reference::parse(&format!("mcp:{name}"))).unwrap();
        assert_eq!(found.artifact.unwrap().validate(), [], "{given:?}");
        assert_eq!(found.name, name.as_str());
    }
}

#[test]
fn add_refuses_what_validate_would_call_invalid_and_writes_nothing() {
    let (_dir, project, folder) = authoring();
    write(&folder, &[("github.toml", &McpMother::stdio("github"))]);
    let before = held(&folder);
    let given = McpMother::given();

    for (name, _, _) in McpMother::reserved() {
        let refused = add_server(&project, &name, &given).unwrap_err();
        assert_eq!(refused.code(), "add_name_unfit", "{name}: {refused}");
    }
    let twin = McpMother::twin("github");
    let clash = add_server(&project, &twin, &given).unwrap_err();
    assert_eq!(clash.code(), "add_name_unfit", "{clash}");
    assert!(clash.to_string().contains("differ only by case"), "{clash}");
    for (why, given, code) in McpMother::given_unfit() {
        let refused = add_server(&project, &McpMother::name(), &given).unwrap_err();

        assert_eq!(refused.code(), "add_mcp_unfit", "{why}: {refused}");
        let CreateError::Definition { issues, .. } = &refused else {
            panic!("{why}: {refused:?}");
        };
        assert!(
            issues.iter().any(|issue| issue.code() == code),
            "{why}: {issues:?}"
        );
    }
    assert_eq!(held(&folder), before);
}

#[test]
fn add_refuses_a_token_given_as_it_is_saying_where_and_never_the_token() {
    let (_dir, project, folder) = authoring();

    for (prefix, token) in McpMother::tokens() {
        for place in ["arg", "env", "header", "url", "secret_env"] {
            let (given, at) = McpMother::given_with_literal(place, &token);

            let refused = add_server(&project, &McpMother::name(), &given).unwrap_err();

            assert_eq!(refused.code(), "add_mcp_unfit", "{place}: {refused}");
            let said = refused.to_string();
            let rest = token[prefix.len()..].trim_start();
            assert!(
                said.contains(&format!("`{at}`")) && said.contains(&format!("`{prefix}`")),
                "{place}: {said}"
            );
            assert!(!said.contains(rest), "{place}: {said}");
            // Nor in what a front end is told of each issue.
            let CreateError::Definition { issues, .. } = &refused else {
                panic!("{place}: {refused:?}");
            };
            let sent = serde_json::to_string(issues).unwrap();
            assert!(!sent.contains(rest), "{place}: {sent}");
        }
        // Nor where it is refused before a definition is read, which says
        // what it refuses rather than where `validate` would.
        for (why, given, code) in McpMother::given_refused_unread(&token) {
            let refused = add_server(&project, &McpMother::name(), &given).unwrap_err();

            assert_eq!(refused.code(), code, "{why}: {refused}");
            let said = refused.to_string();
            let rest = token[prefix.len()..].trim_start();
            assert!(said.contains(&format!("`{prefix}`")), "{why}: {said}");
            assert!(!said.contains(rest), "{why}: {said}");
        }
    }
    assert_eq!(held(&folder), []);
}

#[test]
fn add_refuses_a_key_given_twice_and_an_address_typed_with_a_credential() {
    let (_dir, project, folder) = authoring();

    for (why, given, repeated) in McpMother::given_repeating() {
        let refused = add_server(&project, &McpMother::name(), &given).unwrap_err();

        assert_eq!(refused.code(), "add_mcp_key_repeated", "{why}: {refused}");
        assert_eq!(
            refused.to_string(),
            format!(
                "`{}` is given twice for the transport's `{}`, which holds each key once",
                repeated.key, repeated.table
            )
        );
    }
    for credentialed in UrlMother::every_credentialed() {
        let given = Given::Address(Address {
            url: credentialed.typed.clone(),
            ..Address::default()
        });

        let refused = add_server(&project, &McpMother::name(), &given).unwrap_err();

        assert_eq!(refused.code(), "add_mcp_url_userinfo", "{refused}");
        let said = refused.to_string();
        assert!(!said.contains(&credentialed.secret), "{said}");
        assert!(
            said.starts_with(&format!("`{}`", credentialed.clean)),
            "{said}"
        );
    }
    assert_eq!(held(&folder), []);
}

#[test]
fn add_refuses_a_name_beside_a_value_that_no_command_line_can_give() {
    let (_dir, project, folder) = authoring();

    for (why, given, unsayable) in McpMother::given_unsayable() {
        let refused = add_server(&project, &McpMother::name(), &given).unwrap_err();

        assert_eq!(refused.code(), "add_mcp_key_unsayable", "{why}: {refused}");
        assert_eq!(
            refused.to_string(),
            format!(
                "`{}` holds `=`, where `{} NAME=VALUE` ends the name, so no command line can \
                 give it; no variable's, header's or input's name holds one",
                unsayable.key, unsayable.option
            ),
            "{why}"
        );
        assert_eq!(given.unsayable(), Some(unsayable), "{why}");
    }
    // A token pasted where the name goes is said by what it starts with.
    let (prefix, token) = McpMother::tokens().remove(0);
    let given = Given::Program(Program {
        command: String::from("npx"),
        env: vec![(format!("{token}=1"), String::from("x"))],
        ..Program::default()
    });
    let refused = add_server(&project, &McpMother::name(), &given).unwrap_err();
    let said = refused.to_string();
    assert_eq!(refused.code(), "add_mcp_key_unsayable", "{said}");
    assert!(said.contains(&format!("`{prefix}`")), "{said}");
    assert!(!said.contains(token[prefix.len()..].trim_start()), "{said}");
    assert_eq!(held(&folder), []);
}

#[test]
fn add_refuses_a_transport_for_a_kind_no_agent_starts() {
    let (_dir, project, _folder) = authoring();
    let given = McpMother::given();

    for (kind, noun) in [(Kind::Skill, "skill"), (Kind::Rule, "rule")] {
        let refused = lifecycle::add(
            None,
            &project,
            kind,
            "deploy",
            "Ship it",
            Start::Server(&given),
        )
        .unwrap_err();

        assert_eq!(refused.code(), "add_transport_not_mcp");
        assert_eq!(
            refused.to_string(),
            format!(
                "`--command` and `--url` say how an MCP server is started or reached, and a \
                 {noun} is neither started nor reached"
            )
        );
        assert!(!project.root().join(kind.folder()).join("deploy").exists());
    }
}

#[test]
fn add_from_a_template_takes_its_transport_and_sets_its_name_and_description() {
    let (_dir, project, folder) = authoring();
    let templates = project.mind_dir().join("templates");
    let commented = format!(
        "# Our internal servers.\n{}",
        McpMother::http("internal").replacen(
            "name = \"internal\"\n",
            "name = \"internal\" # what agents key it by\n",
            1,
        )
    );
    write(&templates, &[("internal.toml", &commented)]);
    let template = template::find(None, &project, Kind::Mcp, "internal").unwrap();
    let listed = template::templates(None, &project).unwrap();
    let description = SkillMother::description();

    let added = lifecycle::add(
        None,
        &project,
        Kind::Mcp,
        "billing",
        &description,
        Start::Template(&template),
    )
    .unwrap();
    let quoted = lifecycle::add(
        None,
        &project,
        Kind::Mcp,
        "ledger",
        "The \"ledger\", and its \\ entries",
        Start::Template(&template),
    )
    .unwrap();

    assert!(
        listed
            .iter()
            .any(|each| each.kind == Kind::Mcp && each.name == "internal"),
        "{listed:?}"
    );
    let written = fs::read_to_string(folder.join("billing.toml")).unwrap();
    let was = McpDefinition::parse(&commented).unwrap().description;
    assert_eq!(
        written,
        commented
            .replacen("name = \"internal\" #", "name = \"billing\" #", 1)
            .replacen(
                &format!("description = {}", toml::Value::String(was)),
                &format!("description = {}", toml::Value::String(description)),
                1
            )
    );
    assert_eq!(added.artifact.validate(), []);
    assert_eq!(
        quoted.artifact.summary(),
        Some("The \"ledger\", and its \\ entries")
    );
    assert_eq!(quoted.artifact.validate(), []);
}

#[test]
fn add_from_a_template_holds_what_it_writes_to_what_validate_would_say() {
    let (_dir, project, folder) = authoring();
    let templates = project.mind_dir().join("templates");
    let (prefix, token) = McpMother::token();
    let (leaky, _) = McpMother::with_literal("leaky", "env", &token);
    write(
        &templates,
        &[
            ("leaky.toml", &leaky),
            ("broken.toml", &McpMother::unreadable()),
        ],
    );

    let leaky = template::find(None, &project, Kind::Mcp, "leaky").unwrap();
    let broken = template::find(None, &project, Kind::Mcp, "broken").unwrap();
    let refused = |template| {
        lifecycle::add(
            None,
            &project,
            Kind::Mcp,
            "ours",
            "Ours",
            Start::Template(template),
        )
        .unwrap_err()
    };

    let leaked = refused(&leaky);
    assert_eq!(leaked.code(), "add_mcp_unfit", "{leaked}");
    assert!(!leaked
        .to_string()
        .contains(token[prefix.len()..].trim_start()));
    assert_eq!(refused(&broken).code(), "add_manifest_unwritable");
    assert_eq!(held(&folder), []);
    // And what a front end is told of each before is what `add` says.
    for (template, code) in [
        ("leaky", "add_mcp_unfit"),
        ("broken", "add_manifest_unwritable"),
    ] {
        let judged = mindweaver_core::draft::added(
            None,
            &project,
            Kind::Mcp,
            ("ours", "Ours"),
            Some(template),
            None,
        );
        assert_eq!(
            judged.refused.map(|said| said.code),
            Some(code),
            "{template}"
        );
    }
    let unscaffolded =
        mindweaver_core::draft::added(None, &project, Kind::Mcp, ("ours", "Ours"), None, None);
    assert_eq!(
        unscaffolded.refused.map(|said| said.code),
        Some("add_mcp_unscaffolded")
    );
}

/// What an import from a registry makes is a definition's text whole,
/// written through `add`'s own path: its name and description set to those
/// given, every other byte kept, and held to what any server added is.
#[test]
fn add_from_a_definition_given_whole_sets_its_name_and_description_and_keeps_the_rest() {
    let (_dir, project, folder) = authoring();
    let text = format!(
        "# Imported, by hand.\n{}",
        McpMother::http("upstream").replacen(
            "name = \"upstream\"\n",
            "name = \"upstream\" # what agents key it by\n",
            1,
        )
    );
    let description = SkillMother::description();

    let foreseen = lifecycle::would_add(
        &project,
        Kind::Mcp,
        "billing",
        &description,
        Start::Definition(&text),
    )
    .unwrap();
    assert_eq!(foreseen, folder.join("billing.toml"));
    assert_eq!(held(&folder), [], "a dry run writes nothing");

    let added = lifecycle::add(
        None,
        &project,
        Kind::Mcp,
        "billing",
        &description,
        Start::Definition(&text),
    )
    .unwrap();

    let was = McpDefinition::parse(&text).unwrap().description;
    assert_eq!(added.artifact.file(), foreseen);
    assert_eq!(
        fs::read_to_string(&foreseen).unwrap(),
        text.replacen("name = \"upstream\" #", "name = \"billing\" #", 1)
            .replacen(
                &format!("description = {}", toml::Value::String(was)),
                &format!("description = {}", toml::Value::String(description.clone())),
                1
            )
    );
    assert_eq!(added.artifact.validate(), []);
}

/// Whatever `add` refuses, `would_add` refuses by the same code, before
/// anything is written: a file already where it goes, a name beside one
/// that differs only by case, a definition `validate` would call invalid,
/// and a definition given for a kind no agent starts.
#[test]
fn would_add_refuses_what_add_refuses_and_writes_nothing() {
    let (_dir, project, folder) = authoring();
    let name = McpMother::name();
    let twin = McpMother::twin(&name);
    write(
        &folder,
        &[(&format!("{name}.toml"), &McpMother::http(&name))],
    );
    let before = held(&folder);
    let (_, token) = McpMother::token();
    let (leaky, _) = McpMother::with_literal("leaky", "env", &token);
    let plain = McpMother::http("plain");

    for (kind, named, text, code) in [
        (Kind::Mcp, name.as_str(), plain.as_str(), "add_exists"),
        (Kind::Mcp, twin.as_str(), plain.as_str(), "add_name_unfit"),
        (Kind::Mcp, "leaky", leaky.as_str(), "add_mcp_unfit"),
        (
            Kind::Mcp,
            "broken",
            "format = 1\n",
            "add_manifest_unwritable",
        ),
        (
            Kind::Skill,
            "deploy",
            plain.as_str(),
            "add_definition_not_mcp",
        ),
    ] {
        let foreseen = lifecycle::would_add(&project, kind, named, "Ours", Start::Definition(text))
            .unwrap_err();
        let refused = lifecycle::add(None, &project, kind, named, "Ours", Start::Definition(text))
            .unwrap_err();

        assert_eq!(foreseen.code(), code, "{named}: {foreseen}");
        assert_eq!(refused.code(), code, "{named}: {refused}");
        assert_eq!(held(&folder), before, "{named}");
    }
}

#[test]
fn add_is_refused_in_a_project_that_keeps_no_servers_before_anything_else() {
    let dir = TempDir::new().unwrap();
    let older = ProjectMother::older(&dir.path().join("older"));
    let settings = ProjectMother::create(&dir.path().join("settings"), &[], Share::Local);

    for (project, hint) in [
        (
            older,
            "since its mind.toml names no folder for them; `mind migrate` moves it to a \
             settings.toml, whose `[directories]` can name one",
        ),
        (
            settings,
            "since its settings.toml names no folder for them; its `[directories]` can name one",
        ),
    ] {
        let error = create(&project, Kind::Mcp, "github", "GitHub").unwrap_err();
        let told = create_server(&project, "github", "GitHub", &McpMother::given()).unwrap_err();

        assert_eq!(
            error.to_string(),
            format!(
                "{}: keeps no MCP servers of its own, {hint}",
                project.root().display()
            )
        );
        assert_eq!(told.to_string(), error.to_string());
        assert_eq!(told.code(), "no_folder");

        assert!(
            matches!(
                error,
                CreateError::Workspace(WorkspaceError::NoDirectory {
                    kind: Kind::Mcp,
                    ..
                })
            ),
            "{error}"
        );
        assert_eq!(error.code(), "no_folder");
        assert!(!project.root().join("mcp").exists());
    }
}

#[test]
fn rename_moves_the_file_and_rewrites_its_name_keeping_every_other_byte() {
    let (_dir, project, folder) = authoring();
    let text = McpMother::stdio("github").replacen(
        "name = \"github\"\n",
        "name = \"github\" # what agents key it by\n",
        1,
    );
    write(&folder, &[("github.toml", &format!("# Ours.\n{text}"))]);
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();

    let renamed = lifecycle::rename(None, &project, &target, "octocat").unwrap();

    assert!(!folder.join("github.toml").exists());
    let after = fs::read_to_string(folder.join("octocat.toml")).unwrap();
    assert_eq!(
        after,
        format!("# Ours.\n{text}").replacen("name = \"github\" #", "name = \"octocat\" #", 1)
    );
    assert_eq!(renamed.target.name, "octocat");
    assert_eq!(renamed.target.artifact.unwrap().validate(), []);
}

#[test]
fn rename_holds_the_new_name_to_the_keys_an_agent_takes_and_to_what_none_keeps() {
    let (_dir, project, folder) = authoring();
    write(&folder, &[("github.toml", &McpMother::stdio("github"))]);
    let github = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();

    for (why, to) in McpMother::unwritable() {
        let refused = lifecycle::rename(None, &project, &github, &to).unwrap_err();

        assert_eq!(refused.code(), "rename_name_unfit", "{why}: {refused}");
        assert!(folder.join("github.toml").is_file(), "{why}");
    }
    // Wider than kebab-case, and a name only warned of is no reason to
    // refuse one.
    let wide = McpMother::name();
    let renamed = lifecycle::rename(None, &project, &github, &wide).unwrap();
    assert_eq!(renamed.target.name, wide.as_str());
    assert!(folder.join(format!("{wide}.toml")).is_file(), "{wide}");
    let claimed = lifecycle::rename(None, &project, &renamed.target, "anthropic-skills").unwrap();
    assert_eq!(claimed.target.name, "anthropic-skills");
    assert!(folder.join("anthropic-skills.toml").is_file());
}

#[test]
fn two_servers_whose_names_differ_only_by_case_clash_each_with_the_other() {
    let (_dir, project, folder) = authoring();
    let name = McpMother::name();
    let twin = McpMother::twin(&name);
    write(
        &folder,
        &[
            (&format!("{name}.toml"), &McpMother::stdio(&name)),
            (&format!("{twin}.toml"), &McpMother::http(&twin)),
            ("linear.toml", &McpMother::sse("linear")),
        ],
    );

    let catalog = Catalog::discover_kinds(std::slice::from_ref(&project), &[Kind::Mcp]);
    let issues = |of: &str| {
        catalog
            .artifacts()
            .iter()
            .find(|artifact| artifact.name() == of)
            .unwrap_or_else(|| panic!("no {of}"))
            .validate()
    };

    assert_eq!(
        issues(&name),
        [ValidationIssue::McpNameCaseClash {
            name: name.clone(),
            twin: twin.clone(),
        }]
    );
    assert_eq!(
        issues(&twin),
        [ValidationIssue::McpNameCaseClash {
            name: twin.clone(),
            twin: name.clone(),
        }]
    );
    assert!(!issues(&name)[0].is_warning());
    assert_eq!(issues("linear"), []);
}

#[test]
fn no_change_leaves_a_server_beside_one_named_as_it_is_in_another_case() {
    let (_dir, project, folder) = authoring();
    write(
        &folder,
        &[
            ("github.toml", &McpMother::stdio("github")),
            ("linear.toml", &McpMother::http("linear")),
        ],
    );
    let linear = lifecycle::find(&project, &Reference::parse("mcp:linear")).unwrap();

    let renamed = lifecycle::rename(None, &project, &linear, "GitHub").unwrap_err();
    assert_eq!(renamed.code(), "rename_name_unfit", "{renamed}");
    assert!(
        renamed.to_string().contains("differ only by case"),
        "{renamed}"
    );
    assert!(folder.join("linear.toml").is_file());

    // A server renamed only in case is beside no other of its name.
    let github = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();
    let recased = lifecycle::rename(None, &project, &github, "GitHub").unwrap();
    assert_eq!(recased.target.name, "GitHub");

    // Contents given whole are held to what sits beside them, as
    // `validate` holds them once written, and so is what a front end is
    // told of them before.
    write(&folder, &[("LINEAR.toml", &McpMother::sse("LINEAR"))]);
    let linear = lifecycle::find(&project, &Reference::parse("mcp:linear")).unwrap();
    let text = McpMother::http("linear");
    let edited =
        lifecycle::replace_contents(None, &project, &linear, text.as_bytes(), None).unwrap_err();
    assert_eq!(edited.code(), "edit_contents_unfit", "{edited}");
    let judged =
        mindweaver_core::draft::edited(None, &project, &Reference::parse("mcp:linear"), &text);
    assert_eq!(
        judged.refused.map(|refused| refused.code),
        Some("edit_contents_unfit")
    );
    assert!(
        judged
            .issues
            .iter()
            .any(|issue| issue.code() == "mcp_name_case_clash"),
        "{:?}",
        judged.issues
    );
}

#[test]
fn rename_refuses_a_route_a_server_it_cannot_read_and_a_name_taken() {
    let (_dir, project, folder) = authoring();
    write(
        &folder,
        &[
            ("github.toml", &McpMother::stdio("github")),
            ("linear.toml", &McpMother::http("linear")),
            ("later.toml", &McpMother::in_format("later", "2")),
        ],
    );
    let github = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();
    let later = lifecycle::find(&project, &Reference::parse("mcp:later")).unwrap();

    let nested = lifecycle::rename(None, &project, &github, "team/github").unwrap_err();
    assert!(
        matches!(nested, LifecycleError::McpNested { .. }),
        "{nested}"
    );
    assert_eq!(nested.code(), "rename_mcp_nested");
    let taken = lifecycle::rename(None, &project, &github, "linear").unwrap_err();
    assert!(matches!(taken, LifecycleError::Exists { .. }), "{taken}");
    let unread = lifecycle::rename(None, &project, &later, "sooner").unwrap_err();
    assert_eq!(unread.code(), "rename_definition_unreadable");
    assert!(folder.join("github.toml").is_file() && folder.join("later.toml").is_file());
}

#[test]
fn remove_takes_the_one_file_away() {
    let (_dir, project, folder) = authoring();
    write(
        &folder,
        &[
            ("github.toml", &McpMother::stdio("github")),
            ("linear.toml", &McpMother::http("linear")),
        ],
    );
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();

    assert_eq!(lifecycle::files(&target), 1);
    lifecycle::remove(None, &project, &target).unwrap();

    assert!(!folder.join("github.toml").exists());
    assert!(folder.join("linear.toml").is_file());
}

#[test]
fn contents_given_whole_are_held_to_the_format_and_to_the_files_name() {
    let (_dir, project, folder) = authoring();
    write(&folder, &[("github.toml", &McpMother::stdio("github"))]);
    let target = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();
    let replace = |text: &str| {
        lifecycle::replace_contents(None, &project, &target, text.as_bytes(), None)
            .map(|edited| edited.changed)
    };

    let renamed = replace(&McpMother::stdio("octocat")).unwrap_err();
    assert_eq!(renamed.code(), "edit_mcp_name_changed");
    let unread = replace(&McpMother::unreadable()).unwrap_err();
    assert_eq!(unread.code(), "edit_definition_unreadable");
    let incomplete = replace(&McpMother::incomplete("github", "stdio")).unwrap_err();
    assert_eq!(incomplete.code(), "edit_contents_unfit");
    assert!(
        incomplete.to_string().contains("has no `command`"),
        "{incomplete}"
    );

    let http = McpMother::http("github");
    assert!(replace(&http).unwrap());
    assert_eq!(
        fs::read_to_string(folder.join("github.toml")).unwrap(),
        http
    );
}

#[test]
fn a_token_pasted_for_a_variable_or_written_in_the_extras_is_written_by_no_change() {
    let (_dir, project, folder) = authoring();
    let (_, token) = McpMother::token();
    let github = McpMother::stdio("github");
    for place in ["env_var", "extras", "extras_args"] {
        let (leaky, at) = McpMother::with_literal("leaky", place, &token);
        write(&folder, &[("github.toml", &github), ("leaky.toml", &leaky)]);
        let clean = lifecycle::find(&project, &Reference::parse("mcp:github")).unwrap();
        let held = lifecycle::find(&project, &Reference::parse("mcp:leaky")).unwrap();
        let (contents, _) = McpMother::with_literal("github", place, &token);

        let edited = lifecycle::replace_contents(None, &project, &clean, contents.as_bytes(), None)
            .unwrap_err();
        let renamed = lifecycle::rename(None, &project, &held, "sealed").unwrap_err();

        assert_eq!(edited.code(), "edit_contents_unfit", "{place}: {edited}");
        assert_eq!(renamed.code(), "rename_unfit", "{place}: {renamed}");
        assert!(
            renamed.to_string().contains(&format!("`{at}`")),
            "{renamed}"
        );
        assert_eq!(
            fs::read_to_string(folder.join("github.toml")).unwrap(),
            github,
            "{place}"
        );
        assert!(folder.join("leaky.toml").is_file() && !folder.join("sealed.toml").exists());
    }
}

// ---------------------------------------------------------------------------
// What a message calls them
// ---------------------------------------------------------------------------

/// A message about more than one of a kind asks the kind for its plural,
/// which an abbreviation has no `s` to make: each one that names the kind
/// so, built for MCP servers, says `MCP servers` and never `mcps`.
#[test]
fn every_message_naming_more_than_one_server_calls_them_mcp_servers() {
    use mindweaver_core::install::InstallError;
    use mindweaver_core::migrate::Refusal;
    use mindweaver_core::{Misplaced, MisplacedTarget, ReapplyError};

    let path = || std::path::PathBuf::from(".x");
    let project = || String::from("app");
    let kind = Kind::Mcp;
    let messages: Vec<String> = vec![
        MisplacedTarget {
            settings: path(),
            kind,
            folder: path(),
            why: Misplaced::NotInstalled,
        }
        .to_string(),
        WorkspaceError::OutsideProject { path: path(), kind }.to_string(),
        WorkspaceError::InsideMind { path: path(), kind }.to_string(),
        WorkspaceError::Target {
            path: path(),
            kind,
            why: Misplaced::NotInstalled,
        }
        .to_string(),
        WorkspaceError::OutsideWorkspace { path: path(), kind }.to_string(),
        WorkspaceError::OverlapsGathered {
            path: path(),
            kind,
            gathered: path(),
        }
        .to_string(),
        Refusal::Linked {
            kind,
            folder: path(),
            target: path(),
        }
        .to_string(),
        Refusal::Tangled {
            kind,
            folder: path(),
            with: path(),
        }
        .to_string(),
        ReapplyError::NoFolder {
            project: project(),
            kind,
        }
        .to_string(),
        InstallError::NoTarget {
            project: project(),
            kind,
        }
        .to_string(),
        InstallError::NoUsableTarget {
            project: project(),
            kind,
        }
        .to_string(),
    ];

    for message in messages {
        assert!(message.contains("MCP servers"), "{message}");
        assert!(!message.contains("mcps"), "{message}");
    }
}
