//! MCP servers at the command line: added from how an agent starts or
//! reaches one, listed beside skills and rules, shown, validated, renamed
//! and removed as every kind is, installed by a name qualified as one,
//! and what no release does with one yet refused, saying what to do
//! instead.

mod common;

use common::mother::ServerOptionsMother;
use common::*;
use mindweaver_core::mcp::{Address, Given, McpDefinition};
use mindweaver_core::mother::{McpMother, ProjectMother, SettingsMother, UrlMother};
use mindweaver_core::{Agent, Share};

const GITHUB: &str = "GitHub's issues, pull requests and code";

/// A workspace whose library holds a skill, a rule and an MCP server: the
/// first two added with `add`, and the server's definition written into
/// the library, since it holds what `add` cannot write and the tests built
/// on it read back: a `cwd`, an input that is `required` and has a
/// `description`, named apart from the variable it is read from, and
/// extras for Claude.
fn library() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    weaver(dir.path(), &["add", "skill", "deploy", "Ship the service"]).unwrap();
    weaver(
        dir.path(),
        &["add", "rule", "git/commits", "Commit one step at a time"],
    )
    .unwrap();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("github.toml"),
        McpMother::create("github", GITHUB, "stdio"),
    )
    .unwrap();
    dir
}

/// What a repository implementing a server keeps at `mcp/`: its code, and
/// a definition it ships for its users, none of it the project's own.
fn unrelated_mcp_folder(root: &Path) {
    let folder = root.join("mcp");
    fs::create_dir_all(folder.join("src")).unwrap();
    fs::write(folder.join("Cargo.toml"), "[package]\nname = \"server\"\n").unwrap();
    fs::write(folder.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(folder.join("github.toml"), McpMother::stdio("github")).unwrap();
}

/// `mindweaver add mcp <name> <description>` in `dir`, told of its
/// transport by `options`.
fn add(dir: &Path, name: &str, description: &str, options: &[&str]) -> Result<Outcome, Failure> {
    let mut line = vec!["add", "mcp", name, description];
    line.extend_from_slice(options);
    weaver(dir, &line)
}

/// `add`, told of the transport `given` says by the options a command line
/// gives it by.
fn add_given(dir: &Path, name: &str, given: &Given) -> Result<Outcome, Failure> {
    add_told(weaver, dir, Kind::Mcp, name, given)
}

/// `add <kind> <name>`, run by `run` in `dir` and told of the transport
/// `given` says by the options a command line gives it by: at either
/// level, and for a kind no transport is for.
fn add_told(
    run: fn(&Path, &[&str]) -> Result<Outcome, Failure>,
    dir: &Path,
    kind: Kind,
    name: &str,
    given: &Given,
) -> Result<Outcome, Failure> {
    let options = ServerOptionsMother::words(given);
    let mut line = vec!["add", kind.slug(), name, GITHUB];
    line.extend(options.iter().map(String::as_str));
    run(dir, &line)
}

/// A weaver workspace in a folder of its own, made with `init`.
fn workspace() -> TempDir {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    dir
}

/// Every file the library's folder of servers under `dir` holds, by name.
fn written(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(own_directory_for(dir, Kind::Mcp))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn add_writes_a_program_from_its_options_that_validate_passes_with_no_warning() {
    let dir = workspace();
    let file = own_directory_for(dir.path(), Kind::Mcp).join("github.toml");

    let added = add(
        dir.path(),
        "github",
        GITHUB,
        &[
            "--command",
            "npx",
            "--arg",
            "-y",
            "--arg",
            "@modelcontextprotocol/server-github",
            "--secret-env",
            "GITHUB_PERSONAL_ACCESS_TOKEN",
        ],
    )
    .unwrap();
    let validated = weaver(dir.path(), &["validate"]).unwrap();

    assert_eq!(
        added.stdout,
        format!("created mcp github at {}\n", file.display())
    );
    assert!(added.stderr.is_empty(), "{:?}", added.stderr);
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        format!(
            "# An MCP server, as Mindweaver keeps it: each agent's own entry for it\n\
             # is written from this file, and `show` says how each agent is given it.\n\
             format = 1\n\
             name = \"github\"\n\
             description = \"{GITHUB}\"\n\
             \n\
             # How an agent starts it: the program, what it is started with, and what\n\
             # its environment is given beside the agent's own.\n\
             [transport]\n\
             type = \"stdio\"\n\
             command = \"npx\"\n\
             args = [\"-y\", \"@modelcontextprotocol/server-github\"]\n\
             env = {{ GITHUB_PERSONAL_ACCESS_TOKEN = \"{{GITHUB_PERSONAL_ACCESS_TOKEN}}\" }}\n\
             \n\
             # What is supplied when an agent starts it, each written between braces\n\
             # where it goes and read from the environment variable it names: no value\n\
             # is written here, and a secret never.\n\
             [[inputs]]\n\
             name = \"GITHUB_PERSONAL_ACCESS_TOKEN\"\n\
             kind = \"env\"\n\
             secret = true\n\
             env_var = \"GITHUB_PERSONAL_ACCESS_TOKEN\"\n"
        )
    );
    assert_eq!(
        validated.stdout,
        "github: ok\n\n1 MCP server checked, 0 invalid\n"
    );
    assert!(
        validated.ok && validated.stderr.is_empty(),
        "{:?}",
        validated.stderr
    );
}

#[test]
fn add_writes_an_address_with_its_headers_and_every_secret_read_from_a_variable() {
    let dir = workspace();

    add(
        dir.path(),
        "linear",
        "Linear's issues",
        &[
            "--url",
            "https://mcp.linear.app/mcp",
            "--header",
            "X-Client=mindweaver",
            "--bearer-env",
            "LINEAR_API_KEY",
        ],
    )
    .unwrap();
    let shown = weaver(dir.path(), &["show", "mcp:linear"]).unwrap();

    assert!(
        shown.stdout.contains(
            "\nLinear's issues\n\n\
             transport: http\n  \
             url: https://mcp.linear.app/mcp\n  \
             headers:\n    \
             Authorization: Bearer {LINEAR_API_KEY}\n    \
             X-Client: mindweaver\n\
             \n\
             inputs:\n  \
             LINEAR_API_KEY: header, secret, read from $LINEAR_API_KEY\n\
             \n\
             agents:\n  \
             claude: given it whole\n  \
             codex: given it whole\n  \
             opencode: given it whole\n"
        ),
        "{}",
        shown.stdout
    );
}

#[test]
fn every_transport_is_written_from_its_options_as_core_writes_it_from_what_it_is_told() {
    let dir = workspace();
    let folder = own_directory_for(dir.path(), Kind::Mcp);

    for given in [
        McpMother::given_program(),
        McpMother::given_address(false),
        McpMother::given_address(true),
    ] {
        let name = McpMother::name();

        add_given(dir.path(), &name, &given).unwrap();

        assert_eq!(
            fs::read_to_string(folder.join(format!("{name}.toml"))).unwrap(),
            given.text(&name, GITHUB),
            "{given:?}"
        );
    }
    let validated = weaver(dir.path(), &["validate", "mcp"]).unwrap();
    assert!(
        validated.ok && validated.stderr.is_empty(),
        "{}",
        validated.stdout
    );
    assert!(
        !validated.stdout.contains("warning"),
        "{}",
        validated.stdout
    );
}

#[test]
fn a_literal_secret_and_a_reserved_name_are_refused_and_nothing_is_written() {
    let dir = workspace();
    let (prefix, token) = McpMother::token();
    let rest = token[prefix.len()..].trim_start();
    let (reserved, agent, word) = McpMother::reserved()
        .into_iter()
        .find(|(_, agent, _)| *agent == Agent::Claude)
        .expect("a name Claude keeps");

    let refused = add_given(dir.path(), &reserved, &McpMother::given_program()).unwrap_err();

    assert_eq!(refused.error.code(), "add_name_unfit");
    assert_eq!(
        refused.error.to_string(),
        format!(
            "`{reserved}` is a name {} keeps for a server of its own (`{word}`), so no server \
             of yours can be keyed by it",
            agent.title()
        )
    );
    for place in ["env", "header"] {
        let (given, at) = McpMother::given_with_literal(place, &token);

        let refused = add_given(dir.path(), "leaky", &given).unwrap_err();

        assert_eq!(refused.error.code(), "add_mcp_unfit");
        assert_eq!(
            refused.error.to_string(),
            format!(
                "`leaky` would be a definition `validate` calls invalid, so nothing was \
                 written: `{at}` holds what looks like a token, starting `{prefix}`, and a \
                 definition holds no secret: declare a secret input read from an environment \
                 variable, and write it there between braces"
            )
        );
        assert!(!refused.error.to_string().contains(rest));
    }
    assert_eq!(written(dir.path()), Vec::<String>::new());
}

#[test]
fn no_token_given_anywhere_is_said_whole_by_the_refusal() {
    let dir = workspace();

    for (prefix, token) in McpMother::tokens() {
        let rest = token[prefix.len()..].trim_start().to_owned();
        for place in ["arg", "env", "header", "url", "secret_env"] {
            let (given, at) = McpMother::given_with_literal(place, &token);

            let refused = add_given(dir.path(), &McpMother::name(), &given).unwrap_err();

            let said = refused.error.to_string();
            assert_eq!(refused.error.code(), "add_mcp_unfit", "{place}: {said}");
            assert!(said.contains(&format!("`{at}`")), "{place}: {said}");
            assert!(!said.contains(&rest), "{place}: {said}");
        }
        // Nor where it is refused before a definition is read: a key given
        // twice, or what is left of an address typed with a credential.
        for (why, given, code) in McpMother::given_refused_unread(&token) {
            let refused = add_given(dir.path(), &McpMother::name(), &given).unwrap_err();

            assert_eq!(refused.error.code(), code, "{why}");
            let expected = match given.repeated() {
                Some(repeated) => format!(
                    "a key that looks like a token, starting `{prefix}`, is given twice for the \
                     transport's `{}`, which holds each key once",
                    repeated.table
                ),
                None => format!(
                    "the address, which holds what looks like a token, starting `{prefix}`, was \
                     typed with something before its host, which a definition never keeps and no \
                     agent is given: send a credential in a header read from an environment \
                     variable instead (`--secret-header` or `--bearer-env`)"
                ),
            };
            assert_eq!(refused.error.to_string(), expected, "{why}");
        }
    }
    assert_eq!(written(dir.path()), Vec::<String>::new());
}

#[test]
fn add_holds_a_server_to_the_one_beside_it_in_another_case_and_to_validates_rules() {
    let dir = workspace();
    add_given(dir.path(), "github", &McpMother::given_program()).unwrap();
    let twin = McpMother::twin("github");

    let clash = add_given(dir.path(), &twin, &McpMother::given_program()).unwrap_err();

    assert_eq!(clash.error.code(), "add_name_unfit");
    assert_eq!(
        clash.error.to_string(),
        format!(
            "`{twin}` and `github` differ only by case, which a file system that folds case, as \
             macOS's and Windows' do, holds as one file, so no library holds both everywhere: \
             rename one of them"
        )
    );
    for (why, given, code) in McpMother::given_unfit() {
        let refused = add_given(dir.path(), &McpMother::name(), &given).unwrap_err();
        assert_eq!(refused.error.code(), "add_mcp_unfit", "{why}");
        assert!(
            refused.error.to_string().contains("calls invalid"),
            "{why}: {code}: {}",
            refused.error
        );
    }
    assert_eq!(written(dir.path()), ["github.toml"]);
}

#[test]
fn a_key_given_twice_an_address_typed_with_a_credential_and_a_skill_given_a_transport_are_refused()
{
    let dir = workspace();

    for (why, given, repeated) in McpMother::given_repeating() {
        let refused = add_given(dir.path(), &McpMother::name(), &given).unwrap_err();

        assert_eq!(refused.error.code(), "add_mcp_key_repeated", "{why}");
        assert_eq!(
            refused.error.to_string(),
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

        let refused = add_given(dir.path(), &McpMother::name(), &given).unwrap_err();

        assert_eq!(refused.error.code(), "add_mcp_url_userinfo");
        assert_eq!(
            refused.error.to_string(),
            format!(
                "`{}` was typed with something before its host, which a definition never keeps \
                 and no agent is given: send a credential in a header read from an environment \
                 variable instead (`--secret-header` or `--bearer-env`)",
                credentialed.clean
            )
        );
    }
    let skill = add_told(
        weaver,
        dir.path(),
        Kind::Skill,
        "deploy",
        &McpMother::given(),
    )
    .unwrap_err();
    assert_eq!(skill.error.code(), "add_transport_not_mcp");
    assert_eq!(
        skill.error.to_string(),
        "`--command` and `--url` say how an MCP server is started or reached, and a skill is \
         neither started nor reached"
    );
    assert_eq!(written(dir.path()), Vec::<String>::new());
    assert!(!own_directory_for(dir.path(), Kind::Skill)
        .join("deploy")
        .exists());
}

#[test]
fn options_no_definition_can_hold_together_are_refused_by_the_grammar() {
    let dir = workspace();

    // `mind add`, `mind weaver add` and `mindweaver add`, which are one
    // grammar in three places, and for every kind, whose `add` takes the
    // same options and a template as surely.
    for (why, options) in ServerOptionsMother::conflicting() {
        for kind in Kind::ALL.map(Kind::slug) {
            for lead in [&["mind"][..], &["mind", "weaver"][..]] {
                let mut words = lead.to_vec();
                words.extend(["add", kind, "github", "GitHub"]);
                words.extend(&options);

                let error = Cli::try_parse_from(&words).err().map(|error| error.kind());

                assert!(
                    matches!(
                        error,
                        Some(
                            clap::error::ErrorKind::ArgumentConflict
                                | clap::error::ErrorKind::MissingRequiredArgument
                                | clap::error::ErrorKind::ValueValidation
                        )
                    ),
                    "{why}: {words:?}: {error:?}"
                );
            }
            let mut words = vec!["mindweaver", "add", kind, "github", "GitHub"];
            words.extend(&options);
            assert!(WeaverCli::try_parse_from(&words).is_err(), "{why}: {kind}");
        }
    }
    assert_eq!(written(dir.path()), Vec::<String>::new());
}

#[test]
fn an_option_given_no_equals_sign_is_refused_without_saying_what_it_was_given() {
    for (prefix, token) in McpMother::tokens() {
        let rest = token[prefix.len()..].trim_start().to_owned();
        for (option, options) in ServerOptionsMother::unpaired(&token) {
            for lead in [&["mind"][..], &["mind", "weaver"][..], &["mindweaver"][..]] {
                let mut words: Vec<&str> = lead.to_vec();
                words.extend(["add", "mcp", "github", "GitHub"]);
                words.extend(options.iter().map(String::as_str));

                let error = if lead == ["mindweaver"] {
                    WeaverCli::try_parse_from(&words).unwrap_err()
                } else {
                    Cli::try_parse_from(&words).unwrap_err()
                };

                assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
                assert_eq!(
                    error.to_string(),
                    format!(
                        "error: invalid value for '{option}': give it as a name, `=` and what \
                         goes with it (what was given is not repeated, since it may be a \
                         secret)\n\
                         \n\
                         Usage: {} add [OPTIONS] <KIND> <NAME> <DESCRIPTION>\n\
                         \n\
                         For more information, try '--help'.\n",
                        lead.join(" ")
                    )
                );
                assert!(!error.to_string().contains(&rest), "{error}");
            }
        }
    }
}

#[test]
fn add_takes_a_servers_transport_from_a_template_and_sets_its_name_and_description() {
    let dir = workspace();
    let templates = dir.path().join(WEAVER_DIR).join("templates");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("internal.toml"), McpMother::http("internal")).unwrap();

    let listed = weaver(dir.path(), &["templates"]).unwrap();
    let added = add(
        dir.path(),
        "billing",
        "Billing's ledger",
        &["--template", "internal"],
    )
    .unwrap();
    let shown = weaver(dir.path(), &["show", "mcp:billing"]).unwrap();
    let validated = weaver(dir.path(), &["validate", "mcp"]).unwrap();

    assert_eq!(
        listed.stdout,
        "full      skill  built in\n\
         internal  mcp    .mindweaver/templates/internal.toml\n"
    );
    assert!(
        added.stdout.starts_with("created mcp billing at "),
        "{}",
        added.stdout
    );
    assert!(
        shown.stdout.contains(
            "\nBilling's ledger\n\ntransport: http\n  url: https://{tenant}.example.com/mcp\n"
        ),
        "{}",
        shown.stdout
    );
    assert_eq!(
        validated.stdout,
        "billing: ok\n\n1 MCP server checked, 0 invalid\n"
    );
}

#[test]
fn mind_add_mcp_writes_a_server_where_a_projects_settings_name_a_folder_for_them() {
    let dir = TempDir::new().unwrap();
    let toml = dir.path().join("toml");
    let json = dir.path().join("json");
    ProjectMother::with_mcp(&toml, &Agent::ALL, Share::Local);
    ProjectMother::json(
        &json,
        &SettingsMother::authoring().with(Kind::Mcp, "servers"),
    );

    for (root, folder) in [(&toml, "mcp"), (&json, "servers")] {
        let added = mind(
            root,
            &[
                "add",
                "mcp",
                "github",
                GITHUB,
                "--command",
                "npx",
                "--secret-env",
                "GITHUB_TOKEN",
            ],
        )
        .unwrap();
        let validated = mind(root, &["validate", "mcp"]).unwrap();

        let file = root.join(folder).join("github.toml");
        assert_eq!(
            added.stdout,
            format!("created mcp github at {}\n", file.display())
        );
        assert!(file.is_file());
        assert_eq!(
            validated.stdout,
            "github: ok\n\n1 MCP server checked, 0 invalid\n"
        );
    }
}

#[test]
fn mindweaver_list_shows_mcp_servers_beside_skills_and_rules() {
    let dir = library();

    let listed = weaver(dir.path(), &["list"]).unwrap();
    let narrowed = weaver(dir.path(), &["list", "mcp"]).unwrap();

    assert_eq!(
        listed.stdout,
        format!(
            "skill  deploy       Ship the service\n\
             rule   git/commits  Commit one step at a time\n\
             mcp    github       {GITHUB}\n"
        )
    );
    assert!(listed.ok && listed.stderr.is_empty(), "{:?}", listed.stderr);
    assert_eq!(narrowed.stdout, format!("github  {GITHUB}\n"));
}

#[test]
fn list_json_carries_each_server_as_it_carries_every_kind() {
    let dir = library();

    let outcome = weaver(dir.path(), &["list", "--json"]).unwrap();

    let (data, warnings): (serde_json::Value, _) = enveloped(&outcome);
    assert!(warnings.is_empty(), "{warnings:?}");
    let github = data["library"]["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|artifact| artifact["kind"] == "mcp")
        .unwrap_or_else(|| panic!("{data}"));
    assert_eq!(github["name"], "github");
    assert_eq!(github["summary"], GITHUB);
    assert_eq!(
        github["targets"],
        serde_json::json!([".mindweaver/library/mcp/github.toml"])
    );
}

#[test]
fn show_says_what_a_server_is_for_how_it_is_started_and_what_is_supplied_for_it() {
    let dir = library();
    let file = own_directory_for(dir.path(), Kind::Mcp).join("github.toml");

    let shown = weaver(dir.path(), &["show", "mcp:github"]).unwrap();

    assert_eq!(
        shown.stdout,
        format!(
            "github\n{}\n\n{GITHUB}\n\n\
             transport: stdio\n  \
             command: npx\n  \
             args:\n    \
             - -y\n    \
             - @example/server\n    \
             - --workspace\n    \
             - {{workspace}}\n  \
             env:\n    \
             API_KEY: {{api_key}}\n    \
             LOG_LEVEL: info\n  \
             cwd: .\n\
             \n\
             inputs:\n  \
             api_key: env, secret, required, read from $EXAMPLE_API_KEY\n    \
             The key the server signs in with\n  \
             workspace: arg, read from $EXAMPLE_WORKSPACE\n\
             \n\
             agents:\n  \
             claude: given it without what it has no place for:\n    \
             - the transport's `cwd` is left out, since a program is started there in no folder \
             it is told\n  \
             codex: given none of it:\n    \
             - secret input `api_key` is written as environment variable `API_KEY`, and a \
             program is given a variable there only whole, under its own name, \
             `EXAMPLE_API_KEY`\n  \
             opencode: given it whole\n",
            file.display(),
        )
    );
    assert!(shown.ok && shown.stderr.is_empty(), "{:?}", shown.stderr);
}

#[test]
fn show_lays_an_address_out_by_its_url_and_headers_and_never_its_credential() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    let credentialed = UrlMother::credentialed();
    let text = McpMother::sse("events").replacen("https://example.com/sse", &credentialed.typed, 1);
    fs::write(folder.join("events.toml"), text).unwrap();

    let shown = weaver(dir.path(), &["show", "mcp:events"]).unwrap();
    let json = weaver(dir.path(), &["show", "mcp:events", "--json"]).unwrap();

    assert!(
        shown.stdout.contains(&format!(
            "transport: sse\n  \
             url: {}\n  \
             headers:\n    \
             X-Api-Key: {{key}}\n\
             \n\
             inputs:\n  \
             key: header, secret, read from $EXAMPLE_KEY\n\
             \n\
             agents:\n",
            credentialed.clean
        )),
        "{}",
        shown.stdout
    );
    let (data, warnings): (serde_json::Value, _) = enveloped(&json);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(data[0]["transport"]["url"], credentialed.clean.as_str());
    // Kept out of the text and the transport alike; the body is the file,
    // which holds what it was written with.
    assert!(
        !shown.stdout.contains(&credentialed.secret),
        "{}",
        shown.stdout
    );
    assert!(
        !data[0]["transport"]
            .to_string()
            .contains(&credentialed.secret),
        "{data}"
    );
}

#[test]
fn show_says_which_agent_is_given_none_of_a_server_and_why() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(folder.join("events.toml"), McpMother::sse("events")).unwrap();
    fs::write(
        folder.join("workspace-server.toml"),
        McpMother::secret_argument("workspace-server"),
    )
    .unwrap();

    let events = weaver(dir.path(), &["show", "mcp:events"]).unwrap();
    let argued = weaver(dir.path(), &["show", "mcp:workspace-server"]).unwrap();

    let sse = "    - the sse transport, server-sent events, is not one taken there\n";
    assert!(
        events.stdout.ends_with(&format!(
            "agents:\n  claude: given it whole\n  codex: given none of it:\n{sse}  \
             opencode: given none of it:\n{sse}"
        )),
        "{}",
        events.stdout
    );
    assert!(
        argued.stdout.contains(
            "  codex: given none of it:\n    \
             - secret input `workspace` is written in the transport's `args`, which hold no \
             reference to an environment variable there\n  \
             opencode: given it whole\n"
        ),
        "{}",
        argued.stdout
    );
    // Shown, never refused: what an agent cannot be given is not wrong.
    assert!(events.ok && argued.ok);
    assert!(events.stderr.is_empty() && argued.stderr.is_empty());
}

#[test]
fn show_json_says_what_each_agent_cannot_be_given_by_its_code() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(
        folder.join("linear.toml"),
        McpMother::composite_secret_header("linear"),
    )
    .unwrap();

    let shown = weaver(dir.path(), &["show", "mcp:linear", "--json"]).unwrap();

    let (data, warnings): (serde_json::Value, _) = enveloped(&shown);
    assert!(warnings.is_empty(), "{warnings:?}");
    let report = &data[0]["expressibility"];
    assert_eq!(
        report,
        &serde_json::json!([
            {"agent": "claude", "refused": [], "dropped": []},
            {
                "agent": "codex",
                "refused": [{
                    "code": "secret_header_composite",
                    "message": "secret input `token` is written in header `Authorization` beside \
                                other text, and a header is read from an environment variable \
                                there only as its whole value, or as `Authorization: Bearer` and \
                                the token",
                }],
                "dropped": [],
            },
            {"agent": "opencode", "refused": [], "dropped": []},
        ])
    );
    let skill = weaver(dir.path(), &["show", "skill:deploy", "--json"]).unwrap();
    let (data, _): (serde_json::Value, _) = enveloped(&skill);
    assert_eq!(data[0]["expressibility"], serde_json::Value::Null);
}

#[test]
fn validate_counts_a_server_in_words_and_says_what_is_wrong_with_one() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(
        folder.join("bare.toml"),
        McpMother::incomplete("bare", "http"),
    )
    .unwrap();

    let failure = weaver(dir.path(), &["validate"]).unwrap();

    assert!(!failure.ok);
    assert_eq!(
        failure.stdout,
        "skill:deploy: ok\n\
         rule:git/commits: ok\n\
         mcp:bare: 1 problem\n  \
         - the http transport has no `url`, which no agent can start or reach the server without\n\
         mcp:github: ok\n\
         \n\
         1 skill, 1 rule and 2 MCP servers checked, 1 invalid\n"
    );
}

#[test]
fn validate_reports_a_reserved_name_and_a_literal_token_and_never_the_token() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(folder.join("Workspace.toml"), McpMother::stdio("Workspace")).unwrap();
    let (prefix, token) = McpMother::token();
    let (leaky, at) = McpMother::with_literal("leaky", "env", &token);
    fs::write(folder.join("leaky.toml"), leaky).unwrap();
    fs::write(
        folder.join("anthropic-skills.toml"),
        McpMother::sse("anthropic-skills"),
    )
    .unwrap();

    let failure = weaver(dir.path(), &["validate", "mcp"]).unwrap();

    assert!(!failure.ok);
    assert_eq!(
        failure.stdout,
        format!(
            "Workspace: 1 problem\n  \
             - `Workspace` is a name Claude Code keeps for a server of its own (`workspace`), \
             so no server of yours can be keyed by it\n\
             anthropic-skills: ok, 1 warning\n  \
             - warning: `anthropic-skills` is the name Claude Code gives the skills Anthropic \
             provides (`anthropic-skills`), so a server of yours by it may be taken for theirs; \
             it is written all the same\n\
             github: ok\n\
             leaky: 1 problem\n  \
             - `{at}` holds what looks like a token, starting `{prefix}`, and a definition holds \
             no secret: declare a secret input read from an environment variable, and write it \
             there between braces\n\
             \n\
             4 MCP servers checked, 2 invalid\n"
        )
    );
    // Said where it is and what it starts with, never what follows.
    let rest = token[prefix.len()..].trim_start();
    let json = weaver(dir.path(), &["validate", "mcp", "--json"]).unwrap();
    assert!(!failure.stdout.contains(rest) && !json.stdout.contains(rest));
    let (data, _): (serde_json::Value, _) = enveloped(&json);
    let codes: Vec<&str> = data["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|checked| checked["issues"].as_array().unwrap())
        .map(|issue| issue["code"].as_str().unwrap())
        .collect();
    assert_eq!(
        codes,
        ["mcp_name_reserved", "mcp_name_claimed", "secret_literal"]
    );
    assert_eq!(data["invalid"], 2);
}

#[test]
fn a_server_is_renamed_and_removed_as_every_kind_is() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);

    weaver(dir.path(), &["rename", "mcp:github", "octocat"]).unwrap();

    assert!(!folder.join("github.toml").exists());
    let renamed = fs::read_to_string(folder.join("octocat.toml")).unwrap();
    assert!(renamed.contains("name = \"octocat\"\n"), "{renamed}");
    let listed = weaver(dir.path(), &["list", "mcp"]).unwrap();
    assert_eq!(listed.stdout, format!("octocat  {GITHUB}\n"));

    let dry = weaver(dir.path(), &["remove", "mcp:octocat"]).unwrap_err();
    assert!(folder.join("octocat.toml").is_file(), "{}", dry.error);
    weaver(dir.path(), &["remove", "mcp:octocat", "--yes"]).unwrap();
    assert!(!folder.join("octocat.toml").exists());
}

/// Linux alone, whose file systems hold `github.toml` and `Github.toml` as
/// two files, where macOS's and Windows' fold case and hold them as one:
/// the clash this is about, which a library made on Linux can carry there.
#[cfg(target_os = "linux")]
#[test]
fn edit_says_a_server_clashes_with_one_beside_it_in_another_case_as_validate_does() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    let twin = McpMother::twin("github");
    fs::write(folder.join(format!("{twin}.toml")), McpMother::stdio(&twin)).unwrap();

    // A stand-in editor that changes nothing.
    let edited = weaver(dir.path(), &["edit", "mcp:github", "--editor", "true"]).unwrap();
    let validated = weaver(dir.path(), &["validate", "mcp:github"]).unwrap();

    let problem = format!(
        "1 problem\n  \
         - `github` and `{twin}` differ only by case, which a file system that folds case, as \
         macOS's and Windows' do, holds as one file, so no library holds both everywhere: \
         rename one of them\n"
    );
    assert!(!edited.ok && !validated.ok);
    assert_eq!(
        edited.stdout,
        format!("left mcp github unchanged: {problem}")
    );
    assert!(
        validated.stdout.starts_with(&format!("github: {problem}")),
        "{}",
        validated.stdout
    );
}

#[test]
fn contents_given_whole_that_name_a_server_otherwise_are_refused() {
    let dir = library();
    fs::write(dir.path().join("draft.toml"), McpMother::sse("octocat")).unwrap();

    let failure = weaver(
        dir.path(),
        &["edit", "mcp:github", "--contents", "draft.toml"],
    )
    .unwrap_err();

    assert_eq!(failure.error.code(), "edit_mcp_name_changed");
}

#[test]
fn edit_writes_how_a_server_is_started_again_from_the_options_add_takes() {
    let dir = workspace();
    add_given(dir.path(), "github", &McpMother::given_program()).unwrap();
    let file = own_directory_for(dir.path(), Kind::Mcp).join("github.toml");
    let given = McpMother::given_address(false);
    let mut line = vec!["edit", "mcp:github"];
    let options = ServerOptionsMother::words(&given);
    line.extend(options.iter().map(String::as_str));

    let edited = weaver(dir.path(), &line).unwrap();
    let again = weaver(dir.path(), &line).unwrap();

    assert_eq!(edited.stdout, "edited mcp github: ok\n");
    assert_eq!(again.stdout, "left mcp github unchanged: ok\n");
    let definition = McpDefinition::parse(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(definition.description, GITHUB);
    let Given::Address(mut read) = given else {
        unreachable!("an address is given as one");
    };
    read.headers.sort();
    assert_eq!(Given::of(&definition), Some(Given::Address(read)));
    let validated = weaver(dir.path(), &["validate", "mcp:github"]).unwrap();
    assert!(validated.ok, "{}", validated.stdout);
}

#[test]
fn edit_refuses_a_transport_beside_an_editor_or_contents_and_for_a_skill() {
    let dir = library();
    for options in [
        &["--editor", "true", "--command", "npx"][..],
        &[
            "--contents",
            "draft.toml",
            "--url",
            "https://example.com/mcp",
        ][..],
    ] {
        for lead in [&["mind"][..], &["mind", "weaver"][..]] {
            let mut words = lead.to_vec();
            words.extend(["edit", "mcp:github"]);
            words.extend(options);
            let error = Cli::try_parse_from(&words).err().map(|error| error.kind());
            assert_eq!(
                error,
                Some(clap::error::ErrorKind::ArgumentConflict),
                "{words:?}"
            );
        }
    }

    let failure = weaver(dir.path(), &["edit", "skill:deploy", "--command", "npx"]).unwrap_err();

    assert_eq!(failure.error.code(), "edit_transport_not_mcp");
}

#[test]
fn a_front_end_with_no_terminal_runs_an_edit_told_a_servers_transport() {
    let line: Vec<String> = ["mindweaver", "edit", "mcp:github", "--command", "npx"]
        .map(String::from)
        .to_vec();
    let editor: Vec<String> = ["mindweaver", "edit", "mcp:github"]
        .map(String::from)
        .to_vec();

    assert_eq!(mindweaver_cli::task::refused(&line), None);
    assert_eq!(
        mindweaver_cli::task::refused(&editor),
        Some(mindweaver_cli::task::Refusal::Editor)
    );
}

#[test]
fn adding_a_server_told_nothing_of_its_transport_is_refused_with_what_to_do() {
    let dir = library();
    let folder = own_directory_for(dir.path(), Kind::Mcp);

    let added = weaver(dir.path(), &["add", "mcp", "linear", "Linear's issues"]).unwrap_err();

    assert_eq!(added.error.code(), "add_mcp_unscaffolded");
    assert_eq!(
        added.error.to_string(),
        format!(
            "an MCP server is added with how an agent starts it, `--command`, or where it \
             reaches it, `--url`, or from a `--template` that says so; or write its definition \
             as <name>.toml in {}",
            folder.display()
        )
    );
}

/// A server is found by its bare name wherever a skill is, as much as by
/// a name qualified as one.
#[test]
fn a_server_is_installed_by_its_bare_name_as_by_one_qualified() {
    let dir = library();
    for project in ["app", "web"] {
        new_project(&dir.path().join(project));
        weaver(dir.path(), &["link", project]).unwrap();
    }

    let bare = weaver(dir.path(), &["install", "-p", "app", "github"]).unwrap();
    let qualified = weaver(dir.path(), &["install", "-p", "web", "mcp:github"]).unwrap();

    for (outcome, project) in [(bare, "app"), (qualified, "web")] {
        assert!(
            outcome.stdout.starts_with(&format!(
                "installed mcp github into {project}, from workspace\n"
            )),
            "{}",
            outcome.stdout
        );
        assert!(
            fs::read_to_string(dir.path().join(project).join(".mcp.json"))
                .unwrap()
                .contains("github")
        );
    }
}

/// A project a `mind.toml` marks names no agent to give a server: a bare
/// name nothing offers is answered as nothing offered, as before servers
/// could be installed, and only a server's name with why none can go
/// there.
#[test]
fn a_bare_name_nothing_offers_is_not_offered_where_no_server_could_go_either() {
    let dir = library();
    older_project(&dir.path().join("legacy"));
    weaver(dir.path(), &["link", "legacy"]).unwrap();

    let typo = weaver(dir.path(), &["install", "-p", "legacy", "deplyo"]).unwrap_err();
    let server = weaver(dir.path(), &["install", "-p", "legacy", "github"]).unwrap_err();

    assert_eq!(typo.error.code(), "not_offered", "{}", typo.error);
    assert_eq!(server.error.code(), "no_target", "{}", server.error);
    assert_eq!(
        server.error.to_string(),
        "legacy names no agent in its settings that MCP servers are installed for, so none \
         can be installed into it"
    );
}

/// A server a project received is entries in its agents' config files,
/// never a definition the project keeps (D-23): `rename`, `edit` and
/// `remove` aimed at one are refused, saying what changes it instead.
#[test]
fn a_change_aimed_at_a_server_the_project_received_is_refused_with_what_to_do_instead() {
    let dir = library();
    let app = dir.path().join("app");
    new_project(&app);
    weaver(dir.path(), &["link", "app"]).unwrap();
    weaver(dir.path(), &["install", "-p", "app", "mcp:github"]).unwrap();
    let before = fs::read_to_string(app.join(".mcp.json")).unwrap();

    for line in [
        &["rename", "mcp:github", "octocat"][..],
        &["remove", "mcp:github", "--yes"],
        &["edit", "mcp:github"],
    ] {
        let refused = mind(&app, line).unwrap_err();

        assert_eq!(refused.error.code(), "change_received_mcp", "{line:?}");
        // The name bare, the one form an uninstall needs while the project
        // received nothing else of it.
        assert_eq!(
            refused.error.to_string(),
            "`mcp:github` is an MCP server app received, an entry in its agents' config files \
             written from the definition it was installed from rather than kept in it, so it is \
             not changed from here: uninstall it as `github`, or change its definition in the \
             library it came from and update",
            "{line:?}"
        );
    }
    assert_eq!(fs::read_to_string(app.join(".mcp.json")).unwrap(), before);
    // Typed bare, the name is found received as well.
    let bare = mind(&app, &["rename", "github", "octocat"]).unwrap_err();
    assert_eq!(bare.error.code(), "change_received_mcp");
    // A skill of the name received too, the bare name would be refused as
    // two, so the hint names the server qualified.
    weaver(dir.path(), &["add", "skill", "github", "Work with GitHub"]).unwrap();
    weaver(dir.path(), &["install", "-p", "app", "skill:github"]).unwrap();
    let both = mind(&app, &["rename", "mcp:github", "octocat"]).unwrap_err();
    assert!(both
        .error
        .to_string()
        .contains("uninstall it as `mcp:github`"));
    let uninstalled = mind(&app, &["uninstall", "mcp:github"]).unwrap();
    assert_eq!(uninstalled.stdout, "removed mcp github from app\n");
}

#[test]
fn a_project_an_older_mind_init_made_keeps_no_servers_and_says_how_to_keep_some() {
    let dir = TempDir::new().unwrap();
    let older = dir.path().join("older");
    let json = dir.path().join("json");
    fs::create_dir_all(&older).unwrap();
    fs::create_dir_all(&json).unwrap();
    older_project(&older);
    json_project(&json);

    for (root, marker) in [(&older, "mind.toml"), (&json, "settings.json")] {
        unrelated_mcp_folder(root);

        let listed = mind(root, &["list", "mcp"]).unwrap();
        let added = add_told(mind, root, Kind::Mcp, "github", &McpMother::given()).unwrap_err();

        assert!(listed.ok && listed.stderr.is_empty(), "{:?}", listed.stderr);
        assert!(!listed.stdout.contains("github"), "{}", listed.stdout);
        assert_eq!(added.error.code(), "no_folder");
        assert_eq!(
            added.error.to_string(),
            format!(
                "{}: keeps no MCP servers of its own, since its {marker} names no folder for \
                 them; `mind migrate` moves it to a settings.toml, whose `[directories]` can \
                 name one",
                root.display()
            )
        );
    }
}

#[test]
fn a_project_mind_init_makes_lists_nothing_of_an_unrelated_mcp_folder() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    unrelated_mcp_folder(dir.path());

    let listed = mind(dir.path(), &["list"]).unwrap();
    let added = add_told(mind, dir.path(), Kind::Mcp, "github", &McpMother::given()).unwrap_err();

    assert!(listed.ok && listed.stderr.is_empty(), "{:?}", listed.stderr);
    assert!(!listed.stdout.contains("github"), "{}", listed.stdout);
    assert_eq!(
        added.error.to_string(),
        format!(
            "{}: keeps no MCP servers of its own, since its settings.toml names no folder for \
             them; its `[directories]` can name one",
            dir.path().display()
        )
    );
}

#[test]
fn the_targets_of_servers_are_each_agents_config_file_and_never_a_folder() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    let name = dir.path().file_name().unwrap().to_str().unwrap().to_owned();

    let reset = mind(dir.path(), &["targets", "reset", "mcp"]).unwrap();
    let set = mind(dir.path(), &["targets", "set", "mcp", ".x"]).unwrap_err();

    assert!(reset.ok, "{:?}", reset.stderr);
    assert_eq!(
        reset.stdout,
        format!("nothing to change: {name} installs MCP servers where its agents read already\n")
    );
    assert_eq!(
        set.error.to_string(),
        ".x: cannot be where a project's MCP servers are installed, because MCP servers are \
         not installed into a project as folders yet"
    );
}

/// A library linking `app`, a project naming every agent, whose server
/// `github` is `text`'s definition, and a Codex home of its own marking
/// `app` trusted: never the user's.
fn installing(text: &str) -> (TempDir, TempDir, String) {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("github.toml"), text).unwrap();
    let app = dir.path().join("app");
    new_project(&app);
    weaver(dir.path(), &["link", "app"]).unwrap();
    let codex = TempDir::new().unwrap();
    // A TOML basic string, which a JSON one of a path is.
    let key = serde_json::to_string(&app.display().to_string()).unwrap();
    fs::write(
        codex.path().join("config.toml"),
        format!("[projects.{key}]\ntrust_level = \"trusted\"\n"),
    )
    .unwrap();
    let home = format!("--codex-home={}", codex.path().display());
    (dir, codex, home)
}

/// Every warning `outcome` says, with the code beside each.
fn warned(outcome: &Outcome) -> Vec<(Option<&'static str>, String)> {
    outcome
        .stderr
        .iter()
        .map(|warning| (warning.code, warning.text.clone()))
        .collect()
}

#[test]
fn an_install_says_what_each_agent_runs_then_what_to_set_and_what_each_agent_asks() {
    let (dir, _codex, home) = installing(&McpMother::program_for_codex("github"));

    let installed = weaver(dir.path(), &["install", "-p", "app", "github", &home]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed mcp github into app, from workspace\n  \
         claude runs: npx -y @example/server --workspace .\n  \
         codex runs: npx -y @example/server --workspace .\n  \
         opencode runs: npx -y @example/server --workspace .\n  \
         set EXAMPLE_API_KEY (a secret) where the agents run: each reads it there, and no file \
         holds it\n  \
         claude asks to approve it before it starts it in a session, though a run with `-p` \
         starts it unasked\n  \
         opencode starts it without asking\n"
    );
    // Claude's entry has no key for the folder it starts in, which is said.
    assert_eq!(
        warned(&installed),
        [(
            Some("cwd_unsupported"),
            String::from(
                "warning: `github`: the transport's `cwd` is left out, since a program is \
                 started there in no folder it is told"
            )
        )]
    );
    assert!(!installed.ok);
}

#[test]
fn an_agent_not_reached_is_said_by_its_refusal_and_codex_untrusted_as_the_warning_it_is() {
    let (dir, _codex, _home) = installing(&McpMother::create("github", GITHUB, "http"));
    let app = dir.path().join("app");

    let installed = weaver(dir.path(), &["install", "-p", "app", "mcp:github"]).unwrap();

    assert_eq!(
        installed.stdout,
        "installed mcp github into app, from workspace\n  \
         claude reaches: https://${EXAMPLE_TENANT}.example.com/mcp\n  \
         opencode reaches: https://{env:EXAMPLE_TENANT}.example.com/mcp\n  \
         set EXAMPLE_TENANT and EXAMPLE_TOKEN (a secret) where the agents run: each reads them \
         there, and no file holds them\n  \
         claude asks to approve it before it starts it in a session, though a run with `-p` \
         starts it unasked\n  \
         opencode starts it without asking\n"
    );
    assert_eq!(
        warned(&installed),
        [(
            Some("codex_mcp_input_in_text"),
            String::from(
                "warning: `github` cannot be given to Codex: input `tenant` is written in \
                 `transport.url`, where Codex reads no environment variable into it, so its \
                 entry could be given it there only as its value"
            )
        )]
    );
    assert!(!app.join(".codex").exists());

    // Given to Codex with no Codex home to say it trusts the project: its
    // entry is written, and said as what Codex will not read.
    let (dir, _codex, _home) = installing(&McpMother::plain_address("github"));
    let app = dir.path().join("app");
    let untrusted = weaver(dir.path(), &["install", "-p", "app", "github"]).unwrap();
    assert!(untrusted
        .stdout
        .contains("  codex reaches: https://example.com/mcp\n"));
    assert_eq!(
        warned(&untrusted),
        [(
            Some("codex_mcp_untrusted"),
            format!(
                "warning: Codex reads `github` from {} only in a project it trusts, and no \
                 Codex home was given to say whether it trusts {}",
                app.join(".codex/config.toml").display(),
                app.display()
            )
        )]
    );
}

/// What a loaded workspace offers is somebody else's to have written: its
/// server is said, each agent's line, and written only with `--yes`.
#[test]
fn a_loaded_workspaces_server_is_a_dry_run_that_writes_nothing_until_yes() {
    let (dir, _codex, home) = installing(&McpMother::plain_program("ours"));
    let team = TempDir::new().unwrap();
    weaver(team.path(), &["init"]).unwrap();
    let folder = own_directory_for(team.path(), Kind::Mcp);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("linear.toml"),
        McpMother::address_for_codex("linear"),
    )
    .unwrap();
    weaver(dir.path(), &["load", &team.path().display().to_string()]).unwrap();
    let app = dir.path().join("app");
    let ledger = WeaverWorkspace::locate(dir.path())
        .unwrap()
        .unwrap()
        .ledger_path();
    assert!(!ledger.exists(), "nothing has been recorded yet");

    let dry = weaver(dir.path(), &["install", "-p", "app", "linear", &home]).unwrap();

    let origin = format!(
        "load:{}",
        mindweaver_core::paths::relative_to(team.path(), dir.path())
            .and_then(|route| mindweaver_core::paths::to_config_string(&route))
            .unwrap()
    );
    assert_eq!(
        dry.stdout,
        format!(
            "would install mcp linear into app, from {origin}\n  \
             claude reaches: https://example.com/mcp\n  \
             codex reaches: https://example.com/mcp\n  \
             opencode reaches: https://example.com/mcp\n\
             nothing installed: it comes from elsewhere than this workspace's library, so add \
             --yes to go ahead\n"
        )
    );
    assert!(dry.ok && dry.stderr.is_empty(), "{:?}", dry.stderr);
    for file in [".mcp.json", ".codex", "opencode.json"] {
        assert!(!app.join(file).exists(), "{file}");
    }
    // Nor anything recorded, the ledger not even made.
    assert!(!ledger.exists(), "{}", ledger.display());
    let status = weaver(dir.path(), &["status", "-p", "app"]).unwrap();
    assert!(!status.stdout.contains("linear"), "{}", status.stdout);

    let yes = weaver(
        dir.path(),
        &["install", "-p", "app", "linear", "--yes", &home],
    )
    .unwrap();

    assert!(
        yes.stdout
            .starts_with(&format!("installed mcp linear into app, from {origin}\n")),
        "{}",
        yes.stdout
    );
    assert!(yes
        .stdout
        .contains("  set EXAMPLE_TOKEN (a secret) where the agents run"));
    assert!(fs::read_to_string(app.join(".mcp.json"))
        .unwrap()
        .contains("linear"));

    // Asked again, what the install would come to: nothing, while each
    // agent holds what it was given, and an update once the team changes
    // it, which waits for a yes as the install did.
    let current = weaver(dir.path(), &["install", "-p", "app", "linear", &home]).unwrap();
    fs::write(
        folder.join("linear.toml"),
        McpMother::address_for_codex("linear").replace("example.com", "example.org"),
    )
    .unwrap();
    let changed = weaver(dir.path(), &["install", "-p", "app", "linear", &home]).unwrap();

    let reaches = |host: &str| {
        ["claude", "codex", "opencode"]
            .map(|agent| format!("  {agent} reaches: https://{host}/mcp\n"))
            .concat()
    };
    assert_eq!(
        current.stdout,
        format!(
            "mcp linear in app was already current\n{}",
            reaches("example.com")
        )
    );
    assert_eq!(
        changed.stdout,
        format!(
            "would update mcp linear in app, from {origin}\n{}\
             nothing installed: it comes from elsewhere than this workspace's library, so add \
             --yes to go ahead\n",
            reaches("example.org")
        )
    );
    // And an update says what each agent would be given of it before it
    // writes it, in its dry run.
    let update = weaver(dir.path(), &["update", "-p", "app", &home]).unwrap();
    assert_eq!(
        update.stdout,
        format!(
            "would update mcp linear in app, from {origin}\n{}\
             nothing changed: add --yes to go ahead\n",
            reaches("example.org")
        )
    );
    assert!(fs::read_to_string(app.join(".mcp.json"))
        .unwrap()
        .contains("example.com"));
}

#[test]
fn an_update_says_what_each_agent_runs_now_and_an_uninstall_takes_every_entry_out() {
    let (dir, _codex, home) = installing(&McpMother::plain_program("github"));
    weaver(dir.path(), &["install", "-p", "app", "github", &home]).unwrap();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::write(
        folder.join("github.toml"),
        McpMother::plain_program("github").replacen("\"--workspace\", \".\"", "\"--verbose\"", 1),
    )
    .unwrap();

    let dry = weaver(dir.path(), &["update", "-p", "app", &home]).unwrap();
    let updated = weaver(dir.path(), &["update", "-p", "app", "--yes", &home]).unwrap();
    let removed = weaver(dir.path(), &["uninstall", "-p", "app", "github"]).unwrap();

    // What each agent would run said before anything is written, and the
    // same warning, as the update that writes it says them.
    assert_eq!(
        dry.stdout,
        "would update mcp github in app, from workspace\n  \
         claude runs: npx -y @example/server --verbose\n  \
         codex runs: npx -y @example/server --verbose\n  \
         opencode runs: npx -y @example/server --verbose\n\
         nothing changed: add --yes to go ahead\n"
    );
    assert_eq!(warned(&dry), warned(&updated));
    assert_eq!(dry.ok, updated.ok);
    assert_eq!(
        updated.stdout,
        "updated mcp github in app, from workspace\n  \
         claude runs: npx -y @example/server --verbose\n  \
         codex runs: npx -y @example/server --verbose\n  \
         opencode runs: npx -y @example/server --verbose\n  \
         claude asks to approve it before it starts it in a session, though a run with `-p` \
         starts it unasked\n  \
         opencode starts it without asking\n"
    );
    // Said of the project, as an update says every warning.
    assert_eq!(
        warned(&updated),
        [(
            Some("cwd_unsupported"),
            String::from(
                "warning: app: `github`: the transport's `cwd` is left out, since a program is \
                 started there in no folder it is told"
            )
        )]
    );
    assert_eq!(removed.stdout, "removed mcp github from app\n");
    let app = dir.path().join("app");
    for file in [".mcp.json", ".codex/config.toml", "opencode.json"] {
        assert!(!app.join(file).exists(), "{file}");
    }
}

/// Into several projects, each project's lines in the order asked, each
/// warning said of its project, and one asked about without a yes once.
#[test]
fn a_server_goes_into_several_projects_each_said_as_one_install_says_it() {
    let (dir, _codex, _home) = installing(&McpMother::create("github", GITHUB, "http"));
    new_project(&dir.path().join("web"));
    weaver(dir.path(), &["link", "web"]).unwrap();

    let installed = weaver(dir.path(), &["install", "-p", "app", "-p", "web", "github"]).unwrap();

    let lines = |project: &str| {
        format!(
            "installed mcp github into {project}, from workspace\n  \
             claude reaches: https://${{EXAMPLE_TENANT}}.example.com/mcp\n  \
             opencode reaches: https://{{env:EXAMPLE_TENANT}}.example.com/mcp\n  \
             set EXAMPLE_TENANT and EXAMPLE_TOKEN (a secret) where the agents run: each reads \
             them there, and no file holds them\n  \
             claude asks to approve it before it starts it in a session, though a run with \
             `-p` starts it unasked\n  \
             opencode starts it without asking\n"
        )
    };
    assert_eq!(
        installed.stdout,
        format!("{}{}", lines("app"), lines("web"))
    );
    let codes: Vec<(Option<&str>, bool)> = warned(&installed)
        .iter()
        .zip(["app", "web"])
        .map(|((code, text), project)| {
            (
                *code,
                text.starts_with(&format!(
                    "warning: {project}: `github` cannot be given to Codex"
                )),
            )
        })
        .collect();
    assert_eq!(
        codes,
        [
            (Some("codex_mcp_input_in_text"), true),
            (Some("codex_mcp_input_in_text"), true)
        ]
    );
    assert_eq!(installed.stderr.len(), 2);
}

/// Status lists a server's entries an agent each, how each stands and
/// where it came from, beside somebody's the agents' files hold, in the
/// text as in the JSON.
#[test]
fn status_lists_each_entry_of_a_server_an_agent_each_somebodys_among_them() {
    let (dir, _codex, home) = installing(&McpMother::plain_program("github"));
    let app = dir.path().join("app");
    weaver(dir.path(), &["install", "-p", "app", "github", &home]).unwrap();
    // Codex's entry changed by hand, and a server of somebody's beside ours.
    let codex = fs::read_to_string(app.join(".codex/config.toml")).unwrap();
    fs::write(
        app.join(".codex/config.toml"),
        codex.replacen("\"--workspace\"", "\"--verbose\"", 1),
    )
    .unwrap();
    let claude = fs::read_to_string(app.join(".mcp.json")).unwrap();
    fs::write(
        app.join(".mcp.json"),
        claude.replacen(
            "\"mcpServers\": {",
            "\"mcpServers\": {\n    \"theirs\": {\"type\": \"http\", \"url\": \"https://example.com/mcp\"},",
            1,
        ),
    )
    .unwrap();

    let text = weaver(dir.path(), &["status", "-p", "app"]).unwrap();
    let json = weaver(dir.path(), &["status", "-p", "app", "--json"]).unwrap();
    let here = mind(&app, &["status"]).unwrap();

    assert_eq!(
        text.stdout,
        "\
github  current  claude    workspace
theirs  foreign  claude
github  edited   codex     workspace
github  current  opencode  workspace
"
    );
    assert!(text.ok, "{:?}", text.stderr);
    assert_eq!(here.stdout, text.stdout);
    let data: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    let entries: Vec<(&str, &str, &str)> = data["data"]["projects"][0]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["agent"].as_str().unwrap(),
                entry["name"].as_str().unwrap(),
                entry["state"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            ("claude", "github", "current"),
            ("claude", "theirs", "foreign"),
            ("codex", "github", "edited"),
            ("opencode", "github", "current"),
        ]
    );
    // The installation's own row, as every installed artifact has, says it
    // as a whole.
    assert_eq!(
        data["data"]["projects"][0]["artifacts"][0]["state"],
        "edited"
    );
}

/// A project's own server, a definition it keeps, is a row of its own
/// beside the entries of one of the name it received, as the JSON lists
/// both.
#[test]
fn status_keeps_a_projects_own_server_beside_the_entries_of_one_it_received() {
    let dir = library();
    let app = dir.path().join("app");
    ProjectMother::with_mcp(&app, &Agent::ALL, Share::Shared);
    weaver(dir.path(), &["link", "app"]).unwrap();
    mind(&app, &["add", "mcp", "github", GITHUB, "--command", "npx"]).unwrap();
    weaver(dir.path(), &["install", "-p", "app", "mcp:github"]).unwrap();

    let text = weaver(dir.path(), &["status", "-p", "app"]).unwrap();

    assert_eq!(
        text.stdout,
        "\
github  authored  -
github  current   claude    workspace
github  current   opencode  workspace
"
    );
}

/// A server an agent of the project is given no entry of is said of it,
/// as a rule some of its agents read nowhere is, in `missed`.
#[test]
fn status_says_which_agents_a_server_the_project_received_does_not_reach() {
    let (dir, _codex, home) = installing(&McpMother::create("github", GITHUB, "http"));
    weaver(dir.path(), &["install", "-p", "app", "github", &home]).unwrap();

    let text = weaver(dir.path(), &["status", "-p", "app"]).unwrap();
    let json = weaver(dir.path(), &["status", "-p", "app", "--json"]).unwrap();

    assert_eq!(
        text.stdout,
        "\
github  current  claude    workspace
github  current  opencode  workspace
"
    );
    assert_eq!(
        warned(&text),
        [(
            None,
            String::from("warning: codex does not read mcp `github` in app")
        )]
    );
    assert!(!text.ok);
    let data: serde_json::Value = serde_json::from_str(&json.stdout).unwrap();
    assert_eq!(
        data["data"]["projects"][0]["missed"],
        serde_json::json!([{"kind": "mcp", "name": "github", "agents": ["codex"]}])
    );
}
