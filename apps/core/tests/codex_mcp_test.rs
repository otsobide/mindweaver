//! An MCP server rendered into Codex's `.codex/config.toml`, and taken out
//! again: what the file holds afterwards, to the byte, for each transport
//! and each shape a secret is read in, beside the settings, tables and
//! servers somebody else keeps there, and what is refused. What an entry
//! holds is pinned beside `codex_mcp.rs`; these hold the file around it.

use std::fs;
use std::path::Path;

use mindweaver_core::codex_mcp::{self, CodexMcpError, FILE, MAX_BYTES};
use mindweaver_core::config_entry::{ConfigFile, Format, Written};
use mindweaver_core::mother::{random, McpMother};
use mindweaver_core::{AgentHomes, McpDefinition};
use tempfile::TempDir;

fn parsed(text: &str) -> McpDefinition {
    McpDefinition::parse(text).unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// `text`'s definition rendered into the project at `root`, with no agent
/// home given.
fn render(root: &Path, text: &str) -> codex_mcp::Rendered {
    codex_mcp::render(root, &parsed(text), &AgentHomes::default())
        .unwrap_or_else(|error| panic!("{error}\n{text}"))
}

fn read(root: &Path) -> String {
    fs::read_to_string(root.join(FILE)).unwrap()
}

fn write(root: &Path, text: &str) {
    fs::create_dir_all(root.join(".codex")).unwrap();
    fs::write(root.join(FILE), text).unwrap();
}

/// A Codex home of its own, in a temporary folder, holding `config` as its
/// `config.toml` when one is given: never the user's own.
fn codex_home(config: Option<&str>) -> (TempDir, AgentHomes) {
    let home = TempDir::new().unwrap();
    if let Some(config) = config {
        fs::write(home.path().join("config.toml"), config).unwrap();
    }
    let homes = AgentHomes {
        codex_home: Some(home.path().to_path_buf()),
        ..AgentHomes::default()
    };
    (home, homes)
}

/// The names in `dir`, sorted: what a write leaves beside the file.
fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A team's own `.codex/config.toml`: settings at the top, a table of
/// their own, a server of theirs with its environment as a table of its
/// own, a profile after it, and comments above and beside each.
const TEAMS: &str = r#"# The team's Codex settings.
model = "gpt-5-codex"
approval_policy = "on-request"

[sandbox_workspace_write]
network_access = false # kept off

# Their own server.
[mcp_servers.docs]
command = "docs-mcp"
args = ["--port", "0"]

[mcp_servers.docs.env]
DOCS_ROOT = "./docs"

# A faster profile.
[profiles.fast]
model = "gpt-5-codex-mini"
"#;

#[test]
fn a_program_is_written_into_a_file_and_a_folder_made_for_it() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::plain_program(&name));

    assert_eq!(rendered.written, Written::Created);
    assert_eq!(
        rendered.file,
        root.path().join(".codex").join("config.toml")
    );
    assert_eq!(
        read(root.path()),
        format!(
            r#"[mcp_servers.{name}]
command = "npx"
args = ["-y", "@example/server", "--workspace", "."]
cwd = "."
startup_timeout_sec = 20

[mcp_servers.{name}.env]
LOG_LEVEL = "info"
"#
        )
    );
    assert_eq!(rendered.entry.warnings, []);
    assert_eq!(names(root.path()), [".codex"]);
    assert_eq!(names(&root.path().join(".codex")), ["config.toml"]);
}

#[test]
fn a_secret_environment_variable_is_named_in_env_vars_and_passed_on_under_its_own_name() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    render(root.path(), &McpMother::program_for_codex(&name));

    assert_eq!(
        read(root.path()),
        format!(
            r#"[mcp_servers.{name}]
command = "npx"
args = ["-y", "@example/server", "--workspace", "."]
env_vars = ["EXAMPLE_API_KEY"]
cwd = "."
startup_timeout_sec = 20

[mcp_servers.{name}.env]
LOG_LEVEL = "info"
"#
        )
    );
}

#[test]
fn an_address_is_written_with_its_url_and_its_headers_as_they_are() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::plain_address(&name));

    assert_eq!(rendered.written, Written::Created);
    assert_eq!(
        read(root.path()),
        format!(
            r#"[mcp_servers.{name}]
url = "https://example.com/mcp"

[mcp_servers.{name}.http_headers]
X-Client = "mindweaver"
"#
        )
    );
}

#[test]
fn a_bearer_token_is_read_from_its_variable() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    render(root.path(), &McpMother::address_for_codex(&name));

    assert_eq!(
        read(root.path()),
        format!(
            r#"[mcp_servers.{name}]
url = "https://example.com/mcp"
bearer_token_env_var = "EXAMPLE_TOKEN"

[mcp_servers.{name}.http_headers]
X-Client = "mindweaver"
"#
        )
    );
}

#[test]
fn a_secret_header_that_is_its_whole_value_is_read_from_its_variable() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();

    render(root.path(), &McpMother::whole_header_for_codex(&name));

    assert_eq!(
        read(root.path()),
        format!(
            r#"[mcp_servers.{name}]
url = "https://example.com/mcp"

[mcp_servers.{name}.http_headers]
X-Client = "mindweaver"

[mcp_servers.{name}.env_http_headers]
X-Api-Key = "EXAMPLE_TOKEN"
"#
        )
    );
}

#[test]
fn the_team_s_settings_tables_servers_and_comments_come_back_byte_for_byte() {
    let root = TempDir::new().unwrap();
    write(root.path(), TEAMS);
    let name = McpMother::name();

    let rendered = render(root.path(), &McpMother::address_for_codex(&name));

    assert_eq!(rendered.written, Written::Changed);
    assert_eq!(
        read(root.path()),
        format!(
            r#"# The team's Codex settings.
model = "gpt-5-codex"
approval_policy = "on-request"

[sandbox_workspace_write]
network_access = false # kept off

# Their own server.
[mcp_servers.docs]
command = "docs-mcp"
args = ["--port", "0"]

[mcp_servers.docs.env]
DOCS_ROOT = "./docs"

[mcp_servers.{name}]
url = "https://example.com/mcp"
bearer_token_env_var = "EXAMPLE_TOKEN"

[mcp_servers.{name}.http_headers]
X-Client = "mindweaver"

# A faster profile.
[profiles.fast]
model = "gpt-5-codex-mini"
"#
        )
    );

    assert_eq!(
        codex_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), TEAMS);

    // Nor does a file Mindweaver made go while it holds the team's.
    render(root.path(), &McpMother::address_for_codex(&name));
    assert_eq!(
        codex_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), TEAMS);
    assert_eq!(names(&root.path().join(".codex")), ["config.toml"]);
}

#[test]
fn a_server_replaced_where_it_stands_leaves_the_rest_as_it_was() {
    let root = TempDir::new().unwrap();
    write(root.path(), TEAMS);
    let name = McpMother::name();
    render(root.path(), &McpMother::address_for_codex(&name));

    let rendered = render(root.path(), &McpMother::whole_header_for_codex(&name));

    assert_eq!(rendered.written, Written::Changed);
    let after = read(root.path());
    assert!(
        after.contains(&format!(
            "[mcp_servers.{name}]\nurl = \"https://example.com/mcp\"\n\n\
             [mcp_servers.{name}.http_headers]\nX-Client = \"mindweaver\"\n\n\
             [mcp_servers.{name}.env_http_headers]\nX-Api-Key = \"EXAMPLE_TOKEN\"\n"
        )),
        "{after}"
    );
    assert!(!after.contains("bearer_token_env_var"), "{after}");
    assert_eq!(
        codex_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), TEAMS);

    // Again, and nothing is written.
    render(root.path(), &McpMother::whole_header_for_codex(&name));
    let before = read(root.path());
    let rendered = render(root.path(), &McpMother::whole_header_for_codex(&name));
    assert_eq!(rendered.written, Written::Unchanged);
    assert_eq!(read(root.path()), before);
}

#[test]
fn an_entry_written_again_in_another_layout_is_still_the_one_mindweaver_wrote() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();
    let rendered = render(root.path(), &McpMother::program_for_codex(&name));
    let file = ConfigFile::new(root.path(), FILE, Format::Toml).unwrap();
    let key = ["mcp_servers", name.as_str()];

    let held = file.get(&key).unwrap().unwrap();
    assert_eq!(held.fingerprint(), rendered.entry.fingerprint());

    // As `codex mcp add` or a person might write it again: inline, its keys
    // in another order, an integer written as a float.
    let again = format!(
        "[mcp_servers]\n{name} = {{ startup_timeout_sec = 20.0, cwd = \".\", \
         env = {{ LOG_LEVEL = \"info\" }}, env_vars = [\"EXAMPLE_API_KEY\"], command = \"npx\", \
         args = [\"-y\", \"@example/server\", \"--workspace\", \".\"] }}\n"
    );
    write(root.path(), &again);
    let held = file.get(&key).unwrap().unwrap();
    assert_eq!(held.canonical(), rendered.entry.canonical());
    assert_eq!(held.fingerprint(), rendered.entry.fingerprint());

    // So rendering it again leaves their layout as it is.
    let rendered = render(root.path(), &McpMother::program_for_codex(&name));
    assert_eq!(rendered.written, Written::Unchanged);
    assert_eq!(read(root.path()), again);

    // And a value changed is another entry.
    write(root.path(), &again.replacen("\"info\"", "\"debug\"", 1));
    let held = file.get(&key).unwrap().unwrap();
    assert_ne!(held.fingerprint(), rendered.entry.fingerprint());
}

#[test]
fn the_file_mindweaver_made_goes_with_its_last_server_and_only_then() {
    let root = TempDir::new().unwrap();
    let (first, second) = (McpMother::name(), format!("{}x", McpMother::name()));
    assert_eq!(
        render(root.path(), &McpMother::program_for_codex(&first)).written,
        Written::Created
    );
    render(root.path(), &McpMother::address_for_codex(&second));

    // Another server is left in it.
    assert_eq!(
        codex_mcp::remove(root.path(), &first, true).unwrap(),
        Written::Changed
    );
    assert!(read(root.path()).contains(&second));

    // The last goes, and the file with it; Codex's folder stays.
    assert_eq!(
        codex_mcp::remove(root.path(), &second, true).unwrap(),
        Written::Removed
    );
    assert_eq!(names(root.path()), [".codex"]);
    assert!(names(&root.path().join(".codex")).is_empty());

    // Nothing there, and nothing is done.
    assert_eq!(
        codex_mcp::remove(root.path(), &second, true).unwrap(),
        Written::Unchanged
    );
}

#[test]
fn a_file_mindweaver_did_not_make_stays_when_its_last_server_goes() {
    let root = TempDir::new().unwrap();
    let name = McpMother::name();
    render(root.path(), &McpMother::address_for_codex(&name));

    assert_eq!(
        codex_mcp::remove(root.path(), &name, false).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), "");

    // Nor does one Mindweaver made once somebody has written beside it.
    let root = TempDir::new().unwrap();
    render(root.path(), &McpMother::address_for_codex(&name));
    let theirs = format!("model = \"gpt-5-codex\"\n\n{}", read(root.path()));
    write(root.path(), &theirs);
    assert_eq!(
        codex_mcp::remove(root.path(), &name, true).unwrap(),
        Written::Changed
    );
    assert_eq!(read(root.path()), "model = \"gpt-5-codex\"\n");
}

#[test]
fn a_file_mindweaver_made_stays_when_somebody_has_written_a_comment_in_it() {
    let name = McpMother::name();
    let comment = "# Our Codex servers: ask #infra before adding one.\n";
    for (place, above, below) in [("above", comment, ""), ("below", "", comment)] {
        let root = TempDir::new().unwrap();
        assert_eq!(
            render(root.path(), &McpMother::address_for_codex(&name)).written,
            Written::Created
        );
        write(root.path(), &format!("{above}{}{below}", read(root.path())));

        assert_eq!(
            codex_mcp::remove(root.path(), &name, true).unwrap(),
            Written::Changed,
            "{place}"
        );
        assert_eq!(read(root.path()), comment, "{place}");
    }
}

#[test]
fn a_server_not_there_is_taken_out_of_nothing() {
    let root = TempDir::new().unwrap();
    write(root.path(), TEAMS);
    for created in [false, true] {
        assert_eq!(
            codex_mcp::remove(root.path(), &McpMother::name(), created).unwrap(),
            Written::Unchanged
        );
    }
    assert_eq!(read(root.path()), TEAMS);

    let empty = TempDir::new().unwrap();
    assert_eq!(
        codex_mcp::remove(empty.path(), &McpMother::name(), true).unwrap(),
        Written::Unchanged
    );
    assert!(names(empty.path()).is_empty());
}

#[test]
fn a_file_that_is_not_toml_is_refused_and_left_as_it_is() {
    let name = McpMother::name();
    for broken in [
        "[mcp_servers.docs\ncommand = \"docs-mcp\"\n",
        "model = \n",
        "[mcp_servers.docs]\ncommand = \"a\"\ncommand = \"b\"\n",
    ] {
        let root = TempDir::new().unwrap();
        write(root.path(), broken);

        for result in [
            codex_mcp::render(
                root.path(),
                &parsed(&McpMother::address_for_codex(&name)),
                &AgentHomes::default(),
            )
            .map(|_| ()),
            codex_mcp::remove(root.path(), "docs", true).map(|_| ()),
        ] {
            let error = result.unwrap_err();
            assert_eq!(error.code(), "config_malformed", "{broken:?}: {error}");
            assert!(matches!(error, CodexMcpError::File(_)), "{error}");
        }
        assert_eq!(read(root.path()), broken);
    }
}

#[test]
fn a_file_past_two_mib_is_refused_unread() {
    let root = TempDir::new().unwrap();
    let padding = "#".repeat(MAX_BYTES as usize);
    let large = format!("{padding}\n");
    write(root.path(), &large);

    let error = codex_mcp::render(
        root.path(),
        &parsed(&McpMother::address_for_codex(&McpMother::name())),
        &AgentHomes::default(),
    )
    .unwrap_err();

    assert_eq!(MAX_BYTES, 2 * 1024 * 1024);
    assert_eq!(error.code(), "config_too_large", "{error}");
    assert_eq!(read(root.path()), large);
}

#[test]
fn a_definition_codex_is_refused_is_written_nowhere() {
    let name = McpMother::name();
    let (_, token) = McpMother::token();
    for (text, code) in [
        (McpMother::stdio(&name), "secret_env_renamed"),
        (
            McpMother::composite_secret_env(&name),
            "secret_env_composite",
        ),
        (McpMother::secret_argument(&name), "secret_in_argument"),
        (McpMother::secret_in_url(&name), "secret_in_url"),
        (
            McpMother::composite_secret_header(&name),
            "secret_header_composite",
        ),
        (McpMother::sse(&name), "sse_unsupported"),
        (
            McpMother::with_codex_extras(&name, "command = \"other\"\n"),
            "extra_repeated",
        ),
        (
            McpMother::with_codex_extras(&name, "experimental_environment = \"remote\"\n"),
            "extra_withheld",
        ),
        (McpMother::unread_input(&name), "codex_mcp_input_unread"),
        (McpMother::http(&name), "codex_mcp_input_in_text"),
        (
            McpMother::with_literal(&name, "headers", &token).0,
            "codex_mcp_unfit",
        ),
    ] {
        let root = TempDir::new().unwrap();
        let error =
            codex_mcp::render(root.path(), &parsed(&text), &AgentHomes::default()).unwrap_err();
        assert_eq!(error.code(), code, "{error}");
        assert!(names(root.path()).is_empty(), "{code}");

        write(root.path(), TEAMS);
        codex_mcp::render(root.path(), &parsed(&text), &AgentHomes::default()).unwrap_err();
        assert_eq!(read(root.path()), TEAMS, "{code}");
    }
}

/// A user's own `config.toml`, holding `server` among their own servers.
fn users(server: &str) -> String {
    format!(
        "model = \"gpt-5-codex\"\n\n\
         [mcp_servers.docs]\ncommand = \"docs-mcp\"\n\n\
         [mcp_servers.{server}]\nurl = \"http://localhost:3000/mcp\"\n"
    )
}

#[test]
fn a_server_the_user_s_own_config_names_already_is_refused_and_nothing_written() {
    let name = McpMother::name();
    let definition = parsed(&McpMother::address_for_codex(&name));
    let (home, homes) = codex_home(Some(&users(&name)));

    for theirs in [None, Some(TEAMS)] {
        let root = TempDir::new().unwrap();
        if let Some(theirs) = theirs {
            write(root.path(), theirs);
        }

        let error = codex_mcp::render(root.path(), &definition, &homes).unwrap_err();

        assert_eq!(error.code(), "codex_mcp_collision");
        assert_eq!(
            error.to_string(),
            format!(
                "`{name}` cannot be given to Codex: {} holds a server `{name}` already, and \
                 Codex reads a project's servers over the user's key by key, so the two would be \
                 read as one server made of both",
                home.path().join("config.toml").display()
            )
        );
        match theirs {
            None => assert!(names(root.path()).is_empty()),
            Some(theirs) => assert_eq!(read(root.path()), theirs),
        }
    }
    assert_eq!(
        fs::read_to_string(home.path().join("config.toml")).unwrap(),
        users(&name)
    );
}

#[test]
fn a_server_is_given_beside_the_user_s_others_and_with_no_user_s_config_to_read() {
    let name = McpMother::name();
    let definition = parsed(&McpMother::address_for_codex(&name));
    let (_other, others) = codex_home(Some(&users("other")));
    let (_empty, empty) = codex_home(None);

    for homes in [others, empty, AgentHomes::default()] {
        let root = TempDir::new().unwrap();
        let rendered = codex_mcp::render(root.path(), &definition, &homes).unwrap();
        assert_eq!(rendered.written, Written::Created, "{homes:?}");
    }
}

#[test]
fn a_user_s_config_that_does_not_read_is_refused_by_what_it_is() {
    let name = McpMother::name();
    let definition = parsed(&McpMother::address_for_codex(&name));
    for (config, code) in [
        ("[mcp_servers\n", "config_malformed"),
        ("mcp_servers = 1\n", "entry_not_an_object"),
    ] {
        let (home, homes) = codex_home(Some(config));
        let root = TempDir::new().unwrap();

        let error = codex_mcp::render(root.path(), &definition, &homes).unwrap_err();

        assert_eq!(error.code(), code, "{error}");
        assert!(
            error
                .to_string()
                .starts_with(&home.path().join("config.toml").display().to_string()),
            "{error}"
        );
        assert!(names(root.path()).is_empty());
    }
}

#[cfg(unix)]
#[test]
fn a_user_s_config_kept_elsewhere_and_linked_in_is_read_through_the_link() {
    let name = McpMother::name();
    let kept = TempDir::new().unwrap();
    fs::write(kept.path().join("config.toml"), users(&name)).unwrap();
    let (home, homes) = codex_home(None);
    std::os::unix::fs::symlink(
        kept.path().join("config.toml"),
        home.path().join("config.toml"),
    )
    .unwrap();
    let root = TempDir::new().unwrap();

    let error = codex_mcp::render(
        root.path(),
        &parsed(&McpMother::address_for_codex(&name)),
        &homes,
    )
    .unwrap_err();

    assert_eq!(error.code(), "codex_mcp_collision");
    assert_eq!(
        fs::read_to_string(kept.path().join("config.toml")).unwrap(),
        users(&name)
    );
}

/// A user's own `config.toml`, marking the project at `root` `level`.
fn trusting(root: &Path, level: &str) -> String {
    let key = toml::Value::String(root.display().to_string()).to_string();
    format!("[projects.{key}]\ntrust_level = \"{level}\"\n")
}

#[test]
fn a_project_the_user_s_config_marks_trusted_is_written_with_nothing_said() {
    let root = TempDir::new().unwrap();
    let (_home, homes) = codex_home(Some(&format!(
        "{}\n{}",
        users("other"),
        trusting(root.path(), "trusted")
    )));

    let rendered = codex_mcp::render(
        root.path(),
        &parsed(&McpMother::address_for_codex(&McpMother::name())),
        &homes,
    )
    .unwrap();

    assert_eq!(rendered.written, Written::Created);
    assert_eq!(rendered.warnings, []);
}

#[test]
fn a_project_codex_is_not_told_it_trusts_is_written_and_said_to_be_read_only_once_it_is() {
    let name = McpMother::name();
    let definition = parsed(&McpMother::address_for_codex(&name));
    let root = TempDir::new().unwrap();
    let file = root.path().join(FILE);
    let said = |rest: String| {
        format!(
            "Codex reads `{name}` from {} only in a project it trusts, and {rest}",
            file.display()
        )
    };

    let mut homes = Vec::new();
    for config in [
        Some(trusting(root.path(), "untrusted")),
        Some(users("other")),
        Some(trusting(&root.path().join("elsewhere"), "trusted")),
        None,
    ] {
        let (home, given) = codex_home(config.as_deref());
        let expected = said(format!(
            "{} does not mark {} `trusted`",
            home.path().join("config.toml").display(),
            root.path().display()
        ));
        homes.push((home, given, expected));
    }
    let none = said(format!(
        "no Codex home was given to say whether it trusts {}",
        root.path().display()
    ));

    for (given, expected) in homes
        .iter()
        .map(|(_, given, expected)| (given.clone(), expected.clone()))
        .chain([(AgentHomes::default(), none)])
    {
        let rendered = codex_mcp::render(root.path(), &definition, &given).unwrap();

        // Written all the same: trusting a project is the user's to do.
        assert!(read(root.path()).contains(&format!("[mcp_servers.{name}]")));
        let warnings: Vec<(&str, String)> = rendered
            .warnings
            .iter()
            .map(|warning| (warning.code(), warning.to_string()))
            .collect();
        assert_eq!(warnings, [("codex_mcp_untrusted", expected)]);
        assert_eq!(rendered.entry.warnings, []);
    }
}

#[cfg(unix)]
#[test]
fn a_project_reached_through_a_link_is_trusted_under_either_path() {
    let real = TempDir::new().unwrap();
    let above = TempDir::new().unwrap();
    let linked = above.path().join("project");
    std::os::unix::fs::symlink(real.path(), &linked).unwrap();
    let resolved = mindweaver_core::paths::canonical(real.path()).unwrap();
    let definition = parsed(&McpMother::address_for_codex(&McpMother::name()));

    for marked in [linked.as_path(), resolved.as_path()] {
        let (_home, homes) = codex_home(Some(&trusting(marked, "trusted")));
        let rendered = codex_mcp::render(&linked, &definition, &homes).unwrap();
        assert_eq!(rendered.warnings, [], "{}", marked.display());
    }
}

#[test]
fn a_project_whose_config_is_inside_an_agent_s_own_folder_is_refused() {
    // A project at the user's home, whose `.codex` is Codex's home.
    let root = TempDir::new().unwrap();
    let homes = AgentHomes {
        codex_home: Some(root.path().join(".codex")),
        ..AgentHomes::default()
    };
    let definition = parsed(&McpMother::address_for_codex(&random::kebab()));

    let error = codex_mcp::render(root.path(), &definition, &homes).unwrap_err();

    assert_eq!(error.code(), "codex_mcp_user_scope");
    assert_eq!(
        error.to_string(),
        format!(
            "{} is inside {}, which Codex reads for every project, so no server is written into \
             it: Mindweaver gives a server to one project at a time",
            root.path().join(FILE).display(),
            root.path().join(".codex").display()
        )
    );
    assert!(names(root.path()).is_empty());
}

#[cfg(unix)]
#[test]
fn a_symlink_on_the_way_is_refused_and_what_it_points_at_left_alone() {
    let name = McpMother::name();

    // `.codex` a link to somebody's own folder.
    let root = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    fs::write(elsewhere.path().join("config.toml"), TEAMS).unwrap();
    std::os::unix::fs::symlink(elsewhere.path(), root.path().join(".codex")).unwrap();
    let error = codex_mcp::render(
        root.path(),
        &parsed(&McpMother::address_for_codex(&name)),
        &AgentHomes::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");
    let error = codex_mcp::remove(root.path(), "docs", true).unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");
    assert_eq!(
        fs::read_to_string(elsewhere.path().join("config.toml")).unwrap(),
        TEAMS
    );

    // The file itself a link.
    let root = TempDir::new().unwrap();
    fs::create_dir(root.path().join(".codex")).unwrap();
    std::os::unix::fs::symlink(elsewhere.path().join("config.toml"), root.path().join(FILE))
        .unwrap();
    let error = codex_mcp::render(
        root.path(),
        &parsed(&McpMother::address_for_codex(&name)),
        &AgentHomes::default(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "config_symlink", "{error}");
    assert_eq!(
        fs::read_to_string(elsewhere.path().join("config.toml")).unwrap(),
        TEAMS
    );
}

#[test]
fn what_a_render_would_write_and_say_is_planned_and_refused_as_it_is_with_nothing_written() {
    let name = McpMother::name();
    let definition = parsed(&McpMother::address_for_codex(&name));
    let root = TempDir::new().unwrap();
    let (_trusting, trusted) = codex_home(Some(&trusting(root.path(), "trusted")));
    let (_colliding, colliding) = codex_home(Some(&users(&name)));

    let unsaid = codex_mcp::planned(root.path(), &definition, &trusted).unwrap();
    let untrusted = codex_mcp::planned(root.path(), &definition, &AgentHomes::default()).unwrap();
    let refused = codex_mcp::planned(root.path(), &definition, &colliding).unwrap_err();

    assert_eq!(unsaid.file, root.path().join(FILE));
    assert!(unsaid.warnings.is_empty(), "{:?}", unsaid.warnings);
    assert_eq!(
        untrusted
            .warnings
            .iter()
            .map(codex_mcp::Warning::code)
            .collect::<Vec<_>>(),
        ["codex_mcp_untrusted"]
    );
    assert_eq!(refused.code(), "codex_mcp_collision");
    assert!(names(root.path()).is_empty());
    // What it plans is what the render then writes, and says.
    let rendered = codex_mcp::render(root.path(), &definition, &AgentHomes::default()).unwrap();
    assert_eq!(rendered.entry, untrusted.entry);
    assert_eq!(rendered.warnings, untrusted.warnings);
}
