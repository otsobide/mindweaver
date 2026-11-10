//! Unit tests for `import.rs`, kept beside it rather than inside it: what
//! each shape a registry answers is made into, drawn by the mother. What
//! recorded answers are made into is `tests/mcp_registry_test.rs`.

use serde_json::{json, Value};

use super::*;
use crate::mcp::{Input, Transport as Written};
use crate::mother::{random, McpMother, RegistryMother, UpstreamNameMother};

/// The provenance every import here is given.
const PROVENANCE: Provenance<'static> = Provenance {
    registry: "https://registry.example.com",
    fetched_at: "2026-11-10T12:00:00Z",
};

fn entry(answer: Value) -> Entry {
    Entry::read(answer).expect("an answer the mother drew reads")
}

/// One active version of a server listing `packages` and `remotes`.
fn listing(packages: Vec<Value>, remotes: Vec<Value>) -> Entry {
    entry(RegistryMother::answered(
        &UpstreamNameMother::random(),
        &RegistryMother::version(),
        packages,
        remotes,
        "active",
    ))
}

fn imported(entry: &Entry) -> Imported {
    import(entry, &PROVENANCE, None, None).unwrap_or_else(|error| panic!("{error}"))
}

fn stdio(imported: &Imported) -> (&str, &[String], &BTreeMap<String, String>) {
    match &imported.definition.transport {
        Written::Stdio(stdio) => (&stdio.command, &stdio.args, &stdio.env),
        other => panic!("not a program: {other:?}"),
    }
}

fn endpoint(imported: &Imported) -> (&str, &BTreeMap<String, String>) {
    match &imported.definition.transport {
        Written::Http(endpoint) | Written::Sse(endpoint) => (&endpoint.url, &endpoint.headers),
        other => panic!("not an address: {other:?}"),
    }
}

fn codes(notes: &[Note]) -> Vec<&'static str> {
    notes.iter().map(Note::code).collect()
}

/// Each input as `(name, kind, secret, variable)`.
fn inputs(imported: &Imported) -> Vec<(String, InputKind, bool, String)> {
    imported
        .definition
        .inputs
        .iter()
        .map(|input: &Input| {
            (
                input.name.to_string(),
                input.kind,
                input.secret,
                input
                    .env_var
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default(),
            )
        })
        .collect()
}

fn package_with(registry_type: &str, version: Option<&str>, extra: Value) -> (String, Value) {
    let identifier = RegistryMother::identifier(registry_type);
    let mut package = RegistryMother::package(registry_type, &identifier, version);
    if let Value::Object(extra) = extra {
        package.as_object_mut().unwrap().extend(extra);
    }
    (identifier, package)
}

// ---------------------------------------------------------------------------
// Packages, by registry type
// ---------------------------------------------------------------------------

#[test]
fn an_npm_package_is_started_by_npx_pinned_to_its_version() {
    let version = RegistryMother::version();
    let (identifier, package) = package_with("npm", Some(&version), json!({}));

    let imported = imported(&listing(vec![package], Vec::new()));

    let (command, args, env) = stdio(&imported);
    assert_eq!(command, "npx");
    assert_eq!(args, ["-y".to_owned(), format!("{identifier}@{version}")]);
    assert!(env.is_empty());
    assert_eq!(imported.notes, []);
}

#[test]
fn a_pypi_package_is_started_by_uvx_pinned_to_its_version() {
    let version = RegistryMother::version();
    let (identifier, package) = package_with("pypi", Some(&version), json!({}));

    let imported = imported(&listing(vec![package], Vec::new()));

    let (command, args, _) = stdio(&imported);
    assert_eq!(command, "uvx");
    assert_eq!(args, [format!("{identifier}=={version}")]);
}

#[test]
fn a_nuget_package_is_started_by_dnx_pinned_to_its_version() {
    let version = RegistryMother::version();
    let (identifier, package) = package_with("nuget", Some(&version), json!({}));

    let imported = imported(&listing(vec![package], Vec::new()));

    let (command, args, _) = stdio(&imported);
    assert_eq!(command, "dnx");
    assert_eq!(args, [format!("{identifier}@{version}")]);
}

#[test]
fn an_oci_image_is_run_by_docker_its_tag_in_its_identifier_and_its_variables_passed_in() {
    let (identifier, package) = package_with(
        "oci",
        None,
        json!({ "environmentVariables": [
            RegistryMother::asked("API_TOKEN", true, true),
            RegistryMother::valued("LOG_LEVEL", "debug", json!({})),
        ] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    let (command, args, env) = stdio(&imported);
    assert_eq!(command, "docker");
    assert_eq!(
        args,
        [
            "run",
            "-i",
            "--rm",
            "-e",
            "API_TOKEN",
            "-e",
            "LOG_LEVEL",
            &identifier
        ]
        .map(String::from)
    );
    assert_eq!(env["API_TOKEN"], "{API_TOKEN}");
    assert_eq!(env["LOG_LEVEL"], "debug");
    assert_eq!(imported.notes, [], "a tag is a version");
}

/// The schema before 2025-09-29 gave an image's tag as its version.
#[test]
fn an_oci_image_named_without_its_tag_is_given_its_version_as_one() {
    let image = format!("docker.io/{}/{}", random::word(), random::kebab());
    let version = RegistryMother::version();
    let package = RegistryMother::package("oci", &image, Some(&version));

    let imported = imported(&listing(vec![package], Vec::new()));

    assert_eq!(
        stdio(&imported).1.last(),
        Some(&format!("{image}:{version}"))
    );
}

#[test]
fn a_package_with_no_version_runs_whatever_is_latest_and_says_so() {
    for registry_type in ["npm", "pypi", "nuget"] {
        let (identifier, package) = package_with(registry_type, None, json!({}));

        let imported = imported(&listing(vec![package], Vec::new()));

        let (_, args, _) = stdio(&imported);
        assert_eq!(args.last(), Some(&identifier), "{registry_type}");
        assert_eq!(
            imported.notes,
            [Note::Unversioned {
                identifier: identifier.clone()
            }],
            "{registry_type}"
        );
    }
    let image = format!("ghcr.io/{}/{}", random::word(), random::kebab());
    let imported = imported(&listing(
        vec![RegistryMother::package("oci", &image, None)],
        Vec::new(),
    ));
    assert_eq!(codes(&imported.notes), ["mcp_registry_unversioned"]);
}

#[test]
fn a_bundle_a_crate_and_a_registry_type_unknown_are_manual_steps_and_never_guessed() {
    for (registry_type, code) in [
        ("mcpb", "mcp_registry_bundle"),
        ("cargo", "mcp_registry_crate"),
        ("gem", "mcp_registry_type_unknown"),
        (&random::word(), "mcp_registry_type_unknown"),
    ] {
        let (_, package) = package_with(registry_type, Some(&RegistryMother::version()), json!({}));
        let entry = listing(vec![package], Vec::new());

        let Err(ImportError::Manual { name, steps }) = import(&entry, &PROVENANCE, None, None)
        else {
            panic!("{registry_type} was imported");
        };
        assert_eq!(name, entry.name);
        assert_eq!(
            steps.iter().map(Manual::code).collect::<Vec<_>>(),
            [code],
            "{registry_type}"
        );
    }
}

#[test]
fn a_package_serving_over_http_once_started_is_a_manual_step() {
    for transport in ["streamable-http", "sse"] {
        let (_, package) = package_with(
            "npm",
            Some("1.0.0"),
            json!({ "transport": { "type": transport, "url": "http://localhost:{port}/mcp" } }),
        );

        let error =
            import(&listing(vec![package], Vec::new()), &PROVENANCE, None, None).unwrap_err();

        let ImportError::Manual { steps, .. } = error else {
            panic!("{error}");
        };
        assert_eq!(
            steps.iter().map(Manual::code).collect::<Vec<_>>(),
            ["mcp_registry_package_serves"]
        );
    }
}

#[test]
fn an_identifier_no_command_line_takes_as_one_word_is_a_manual_step() {
    for identifier in [
        String::from("--registry=https://evil.example.com"),
        format!("{} {}", random::word(), random::word()),
        format!("{}\u{1b}{}", random::word(), random::word()),
        format!("{{{}}}", random::word()),
        String::new(),
    ] {
        let package = RegistryMother::package("npm", &identifier, Some("1.0.0"));

        let error =
            import(&listing(vec![package], Vec::new()), &PROVENANCE, None, None).unwrap_err();

        assert_eq!(error.code(), "mcp_registry_import_manual", "{identifier:?}");
    }
    let version = format!("1.0 {}", random::word());
    let package = RegistryMother::package("pypi", &random::kebab(), Some(&version));
    let error = import(&listing(vec![package], Vec::new()), &PROVENANCE, None, None).unwrap_err();
    let ImportError::Manual { steps, .. } = error else {
        panic!("{error}");
    };
    assert_eq!(
        steps,
        [Manual::Identifier {
            identifier: version
        }]
    );
}

// ---------------------------------------------------------------------------
// What somebody supplies
// ---------------------------------------------------------------------------

#[test]
fn an_environment_variable_somebody_supplies_is_read_from_the_variable_it_names() {
    let (_, package) = package_with(
        "npm",
        Some("1.0.0"),
        json!({ "environmentVariables": [
            RegistryMother::asked("GITHUB_TOKEN", true, true),
            RegistryMother::asked("ORG", true, false),
        ] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    assert_eq!(stdio(&imported).2["GITHUB_TOKEN"], "{GITHUB_TOKEN}");
    assert_eq!(
        inputs(&imported),
        [
            (
                "GITHUB_TOKEN".into(),
                InputKind::Env,
                true,
                "GITHUB_TOKEN".into()
            ),
            ("ORG".into(), InputKind::Env, false, "ORG".into()),
        ]
    );
    assert!(imported
        .definition
        .inputs
        .iter()
        .all(|input| input.required));
}

#[test]
fn what_the_registry_says_is_optional_is_left_out_and_said() {
    let (_, package) = package_with(
        "npm",
        Some("1.0.0"),
        json!({ "environmentVariables": [
            RegistryMother::asked("DEBUG", false, false),
            RegistryMother::asked("EXTRA_KEY", false, true),
        ] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    assert!(stdio(&imported).2.is_empty());
    assert_eq!(imported.definition.inputs, []);
    assert_eq!(
        imported.notes,
        [
            Note::Optional {
                what: String::from("the environment variable `DEBUG`")
            },
            Note::Optional {
                what: String::from("the environment variable `EXTRA_KEY`")
            },
        ]
    );
}

#[test]
fn a_value_the_registry_fixes_is_written_as_it_is_and_a_default_where_one_is_asked() {
    let (_, package) = package_with(
        "npm",
        Some("1.0.0"),
        json!({ "environmentVariables": [
            { "name": "MODE", "value": "stdio" },
            { "name": "PORT", "isRequired": true, "default": "8080" },
        ] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    let env = stdio(&imported).2;
    assert_eq!(
        (env["MODE"].as_str(), env["PORT"].as_str()),
        ("stdio", "8080")
    );
    assert_eq!(imported.definition.inputs, []);
}

/// What the registry calls a secret is never written, whatever it wrote
/// there: a placeholder of a publisher's, or a value of somebody's.
#[test]
fn a_secret_is_never_written_whatever_value_the_registry_gives_it() {
    let written = format!("{}-{}", random::word(), random::number());
    let (_, package) = package_with(
        "npm",
        Some("1.0.0"),
        json!({ "environmentVariables": [
            { "name": "API_KEY", "value": written, "isSecret": true, "isRequired": true },
            { "name": "OTHER_KEY", "default": written, "isSecret": true, "isRequired": true },
        ] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    assert!(!imported.text.contains(&written), "{}", imported.text);
    assert_eq!(stdio(&imported).2["API_KEY"], "{API_KEY}");
    assert_eq!(stdio(&imported).2["OTHER_KEY"], "{OTHER_KEY}");
    assert!(imported.definition.inputs.iter().all(|input| input.secret));
}

#[test]
fn each_variable_a_value_names_is_an_input_and_the_rest_of_the_value_is_written() {
    let name = UpstreamNameMother::create("io.github.acme/weather");
    let remote = json!({
        "type": "streamable-http",
        "url": "https://{tenant}.example.com/mcp",
        "variables": { "tenant": RegistryMother::variable(true, false) },
        "headers": [RegistryMother::valued(
            "Authorization",
            "Bearer {api_key}",
            json!({ "api_key": RegistryMother::variable(true, true) }),
        )],
    });
    let entry = entry(RegistryMother::answered(
        &name,
        "1.0.0",
        Vec::new(),
        vec![remote],
        "active",
    ));

    let imported = imported(&entry);

    let (url, headers) = endpoint(&imported);
    assert_eq!(url, "https://{tenant}.example.com/mcp");
    assert_eq!(headers["Authorization"], "Bearer {api_key}");
    assert_eq!(
        inputs(&imported),
        [
            (
                "tenant".into(),
                InputKind::UrlVar,
                false,
                "ACME_WEATHER_TENANT".into()
            ),
            (
                "api_key".into(),
                InputKind::Header,
                true,
                "ACME_WEATHER_API_KEY".into()
            ),
        ]
    );
}

/// What servers write, though the schema declares the remote's variables
/// for its URL alone.
#[test]
fn a_header_names_the_remote_s_variables_as_well_as_its_own() {
    let remote = json!({
        "type": "streamable-http",
        "url": "https://mcp.example.com",
        "headers": [{ "name": "Authorization", "value": "Bearer {token}" }],
        "variables": { "token": RegistryMother::variable(true, true) },
    });

    let imported = imported(&listing(Vec::new(), vec![remote]));

    assert_eq!(endpoint(&imported).1["Authorization"], "Bearer {token}");
    assert_eq!(inputs(&imported).len(), 1);
    assert!(
        inputs(&imported)[0].2,
        "a secret, as the remote declares it"
    );
}

/// What the registry's schema leaves as it is written, servers write to be
/// supplied: `Bearer {smithery_api_key}`, with no `variables` at all.
#[test]
fn a_name_between_braces_no_variable_declares_is_supplied_all_the_same() {
    let remote = json!({
        "type": "streamable-http",
        "url": "https://mcp.example.com",
        "headers": [{ "name": "Authorization", "value": "Bearer {api_key}", "isRequired": true, "isSecret": true }],
    });

    let imported = imported(&listing(Vec::new(), vec![remote]));

    assert_eq!(endpoint(&imported).1["Authorization"], "Bearer {api_key}");
    assert_eq!(imported.definition.inputs.len(), 1);
    assert!(imported.definition.inputs[0].secret);
}

#[test]
fn a_name_written_as_a_shell_writes_it_is_read_as_one_between_braces() {
    let remote = json!({
        "type": "streamable-http",
        "url": "https://mcp.example.com",
        "headers": [{ "name": "X-Api-Key", "value": "${API_KEY}", "isRequired": true }],
    });

    let imported = imported(&listing(Vec::new(), vec![remote]));

    assert_eq!(endpoint(&imported).1["X-Api-Key"], "{API_KEY}");
    assert_eq!(
        inputs(&imported),
        [("API_KEY".into(), InputKind::Header, false, "API_KEY".into())]
    );
}

#[test]
fn braces_around_anything_but_a_name_are_text() {
    let (_, package) = package_with(
        "npm",
        Some("1.0.0"),
        json!({ "packageArguments": [{ "type": "positional", "value": "{\"debug\":true}" }] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    assert_eq!(
        stdio(&imported).1.last().map(String::as_str),
        Some("{\"debug\":true}")
    );
    assert_eq!(imported.definition.inputs, []);
}

/// A value asking for something is as required as what it asks for.
#[test]
fn a_value_whose_variable_is_required_is_written_though_the_value_is_not_said_to_be() {
    let (_, package) = package_with(
        "nuget",
        Some("1.0.0"),
        json!({ "environmentVariables": [RegistryMother::valued(
            "CHOICES",
            "{choices}",
            json!({ "choices": RegistryMother::variable(true, false) }),
        )] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    assert_eq!(stdio(&imported).2["CHOICES"], "{CHOICES}");
    assert_eq!(
        inputs(&imported),
        [("CHOICES".into(), InputKind::Env, false, "CHOICES".into())],
        "an environment variable's whole value is read from the variable it is"
    );
}

#[test]
fn a_header_somebody_supplies_whole_is_read_from_a_variable_named_for_the_server() {
    let name = UpstreamNameMother::create("ai.adside/adside");
    let remote = json!({
        "type": "streamable-http",
        "url": "https://api.example.com/mcp",
        "headers": [RegistryMother::asked("Authorization", true, true)],
    });
    let entry = entry(RegistryMother::answered(
        &name,
        "1.0.0",
        Vec::new(),
        vec![remote],
        "active",
    ));

    let imported = imported(&entry);

    assert_eq!(endpoint(&imported).1["Authorization"], "{Authorization}");
    assert_eq!(
        inputs(&imported),
        [(
            "Authorization".into(),
            InputKind::Header,
            true,
            "ADSIDE_AUTHORIZATION".into()
        )]
    );
}

#[test]
fn two_inputs_of_one_name_and_different_kinds_are_told_apart_by_a_number() {
    let remote = json!({
        "type": "streamable-http",
        "url": "https://{region}.example.com/mcp",
        "variables": { "region": RegistryMother::variable(true, false) },
        "headers": [RegistryMother::valued(
            "X-Region",
            "{region}",
            json!({ "region": RegistryMother::variable(true, false) }),
        )],
    });

    let imported = imported(&listing(Vec::new(), vec![remote]));

    let names: Vec<String> = inputs(&imported).into_iter().map(|input| input.0).collect();
    assert_eq!(names, ["region", "region-2"]);
    assert_eq!(endpoint(&imported).1["X-Region"], "{region-2}");
}

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

#[test]
fn package_arguments_follow_the_package_as_the_registry_says_them() {
    let (identifier, package) = package_with(
        "pypi",
        Some("2.0.0"),
        json!({ "packageArguments": [
            { "type": "positional", "value": "serve" },
            { "type": "named", "name": "--transport", "value": "stdio" },
            { "type": "named", "name": "--root", "isRequired": true, "valueHint": "workspace" },
            { "type": "positional", "valueHint": "config_file", "isRequired": true },
            { "type": "named", "name": "--dev" },
            { "type": "positional", "valueHint": "extra" },
            { "type": "flag", "name": "--mcp" },
        ] }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    assert_eq!(
        stdio(&imported).1,
        [
            format!("{identifier}==2.0.0"),
            "serve".into(),
            "--transport".into(),
            "stdio".into(),
            "--root".into(),
            "{workspace}".into(),
            "{config_file}".into(),
        ]
    );
    assert_eq!(
        inputs(&imported)
            .into_iter()
            .map(|input| (input.0, input.1))
            .collect::<Vec<_>>(),
        [
            ("workspace".to_owned(), InputKind::Arg),
            ("config_file".to_owned(), InputKind::Arg)
        ]
    );
    assert_eq!(
        codes(&imported.notes),
        [
            "mcp_registry_optional_left_out",
            "mcp_registry_optional_left_out",
            "mcp_registry_value_left_out"
        ]
    );
}

#[test]
fn runtime_arguments_the_command_does_not_hold_are_left_out_and_said() {
    let (identifier, package) = package_with(
        "oci",
        None,
        json!({
            "runtimeArguments": [
                { "type": "named", "name": "-i" },
                { "type": "named", "name": "--rm" },
                { "type": "named", "name": "-v", "value": "{workspace}:/workspace" },
            ],
            "environmentVariables": [RegistryMother::asked("TOKEN", true, true)],
        }),
    );

    let imported = imported(&listing(vec![package], Vec::new()));

    assert_eq!(
        stdio(&imported).1,
        ["run", "-i", "--rm", "-e", "TOKEN", &identifier].map(String::from)
    );
    assert_eq!(
        imported.notes,
        [Note::RuntimeArguments {
            arguments: vec![String::from("-v {workspace}:/workspace")]
        }]
    );
}

/// What a publisher hands `docker` whole, the image again among it, is
/// what the command holds already, and nothing is said of it.
#[test]
fn runtime_arguments_the_command_holds_already_are_said_nowhere() {
    let (identifier, package) = package_with(
        "oci",
        None,
        json!({
            "runtimeArguments": [
                { "type": "positional", "value": "run" },
                { "type": "named", "name": "-i" },
                { "type": "named", "name": "--rm" },
                { "type": "named", "name": "-e" },
                { "type": "positional", "value": "TOKEN" },
            ],
            "environmentVariables": [RegistryMother::asked("TOKEN", true, true)],
        }),
    );
    let mut entry = listing(vec![package], Vec::new());
    entry.packages[0]["runtimeArguments"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "type": "positional", "value": identifier }));

    assert_eq!(imported(&entry).notes, []);
}

// ---------------------------------------------------------------------------
// Remotes
// ---------------------------------------------------------------------------

#[test]
fn a_remote_over_streamable_http_is_an_http_transport_at_its_url() {
    let url = format!("https://{}/mcp", crate::mother::UrlMother::host());

    let imported = imported(&listing(
        Vec::new(),
        vec![RegistryMother::remote("streamable-http", &url)],
    ));

    assert!(matches!(imported.definition.transport, Written::Http(_)));
    assert_eq!(endpoint(&imported).0, url);
    assert_eq!(imported.notes, []);
}

#[test]
fn a_remote_over_server_sent_events_is_an_sse_transport_and_said() {
    let url = format!("https://{}/sse", crate::mother::UrlMother::host());

    let imported = imported(&listing(
        Vec::new(),
        vec![RegistryMother::remote("sse", &url)],
    ));

    assert!(matches!(imported.definition.transport, Written::Sse(_)));
    assert_eq!(
        imported.notes,
        [Note::Sse {
            url: Redacted::new(&url)
        }]
    );
}

#[test]
fn a_remote_whose_address_carries_a_credential_or_whose_transport_is_unknown_is_a_manual_step() {
    let credentialed = crate::mother::UrlMother::credentialed();
    for (remote, code) in [
        (
            RegistryMother::remote("streamable-http", &credentialed.typed),
            "mcp_registry_url_credentialed",
        ),
        (
            RegistryMother::remote("websocket", "wss://mcp.example.com"),
            "mcp_registry_transport_unknown",
        ),
    ] {
        let error =
            import(&listing(Vec::new(), vec![remote]), &PROVENANCE, None, None).unwrap_err();

        let ImportError::Manual { steps, .. } = &error else {
            panic!("{error}");
        };
        assert_eq!(steps.iter().map(Manual::code).collect::<Vec<_>>(), [code]);
        assert!(!error.to_string().contains(&credentialed.secret), "{error}");
    }
}

#[test]
fn a_header_named_as_no_header_is_and_one_named_twice_are_left_out_and_said() {
    let remote = json!({
        "type": "streamable-http",
        "url": "https://mcp.example.com",
        "headers": [
            { "name": "", "value": "x" },
            { "name": "Bad Header", "value": "x" },
            { "name": "X-Team", "value": "one" },
            { "name": "x-team", "value": "two" },
        ],
    });

    let imported = imported(&listing(Vec::new(), vec![remote]));

    assert_eq!(endpoint(&imported).1.len(), 1);
    assert_eq!(endpoint(&imported).1["X-Team"], "one");
    assert_eq!(codes(&imported.notes), ["mcp_registry_value_left_out"; 3]);
}

// ---------------------------------------------------------------------------
// Which way, which name, which status
// ---------------------------------------------------------------------------

/// A remote over streamable HTTP first, a package next, server-sent events
/// last, whatever order the registry lists them in.
#[test]
fn the_first_way_that_can_be_written_is_taken_in_the_order_that_runs_least() {
    let sse = RegistryMother::remote("sse", "https://mcp.example.com/sse");
    let http = RegistryMother::remote("streamable-http", "https://mcp.example.com/mcp");
    let (_, npm) = package_with("npm", Some("1.0.0"), json!({}));
    let (_, mcpb) = package_with("mcpb", Some("1.0.0"), json!({}));

    let all = listing(vec![mcpb.clone(), npm.clone()], vec![sse.clone(), http]);
    assert_eq!(imported(&all).way, 3);
    let packaged = listing(vec![mcpb.clone(), npm], vec![sse.clone()]);
    assert_eq!(imported(&packaged).way, 1);
    let events = listing(vec![mcpb], vec![sse]);
    assert_eq!(imported(&events).way, 1);
}

#[test]
fn a_way_named_is_the_one_taken_and_one_that_is_not_there_is_refused() {
    let (_, npm) = package_with("npm", Some("1.0.0"), json!({}));
    let entry = listing(
        vec![npm],
        vec![RegistryMother::remote("sse", "https://mcp.example.com/sse")],
    );

    assert_eq!(import(&entry, &PROVENANCE, Some(0), None).unwrap().way, 0);
    assert_eq!(import(&entry, &PROVENANCE, Some(1), None).unwrap().way, 1);
    let error = import(&entry, &PROVENANCE, Some(2), None).unwrap_err();
    assert_eq!(error.code(), "mcp_registry_way_missing");
    assert_eq!(entry.ways().len(), 2);
}

#[test]
fn a_server_listing_no_way_is_refused_and_one_whose_ways_do_not_read_says_why() {
    let error = import(&listing(Vec::new(), Vec::new()), &PROVENANCE, None, None).unwrap_err();
    assert_eq!(error.code(), "mcp_registry_server_wayless");

    let entry = listing(vec![json!({ "registryType": "npm" })], vec![json!(7)]);
    assert!(matches!(
        entry.ways()[..],
        [Way::Unreadable { .. }, Way::Unreadable { .. }]
    ));
    let ImportError::Manual { steps, .. } = import(&entry, &PROVENANCE, None, None).unwrap_err()
    else {
        panic!("imported");
    };
    assert_eq!(
        steps.iter().map(Manual::code).collect::<Vec<_>>(),
        ["mcp_registry_way_unreadable"; 2]
    );
}

#[test]
fn a_definition_is_named_after_the_server_and_its_owner_unless_told_another() {
    for (upstream, named) in [
        ("ai.clawfetch/mcp", "clawfetch-mcp"),
        (
            "io.github.buildkite/buildkite-mcp-server",
            "buildkite-mcp-server",
        ),
        ("ai.adside/adside", "adside"),
        (
            "io.github.joelverhagen/Knapcode.SampleMcpServer",
            "joelverhagen-Knapcode-SampleMcpServer",
        ),
        ("com.Airtable/airtable.mcp", "airtable-mcp"),
    ] {
        assert_eq!(
            named_after(&UpstreamNameMother::create(upstream)),
            named,
            "{upstream}"
        );
    }
    let long = UpstreamNameMother::create(&format!("io/{}", "a".repeat(150)));
    assert_eq!(named_after(&long).len(), MAX_NAME_SEGMENT_LEN);

    let given = McpMother::name();
    let imported = import(
        &entry(RegistryMother::random()),
        &PROVENANCE,
        None,
        Some(&given),
    )
    .unwrap();
    assert_eq!(imported.name, given);
    assert_eq!(imported.definition.name, given.as_str());
}

#[test]
fn a_name_validate_would_refuse_is_refused_with_what_it_would_say() {
    for name in ["workspace", "has space"] {
        let error = import(
            &entry(RegistryMother::random()),
            &PROVENANCE,
            None,
            Some(name),
        )
        .unwrap_err();

        let ImportError::Unfit { issues, .. } = &error else {
            panic!("{error}");
        };
        assert!(!issues.is_empty(), "{name}");
        assert_eq!(error.code(), "mcp_registry_import_unfit");
    }
}

/// What looks like a token, written where the registry fixes a value, is
/// what `add` refuses: the import makes nothing `validate` would call
/// invalid.
#[test]
fn a_token_the_registry_fixes_in_a_value_is_refused_as_add_refuses_one() {
    for (prefix, token) in McpMother::tokens() {
        let remote = json!({
            "type": "streamable-http",
            "url": "https://mcp.example.com",
            "headers": [{ "name": "X-Key", "value": token }],
        });

        let error =
            import(&listing(Vec::new(), vec![remote]), &PROVENANCE, None, None).unwrap_err();

        let ImportError::Unfit { issues, .. } = &error else {
            panic!("{prefix}: {error}");
        };
        assert!(
            issues.iter().any(|issue| issue.code() == "secret_literal"),
            "{prefix}: {issues:?}"
        );
        assert!(!error.to_string().contains(&token), "{prefix}: {error}");
    }
}

#[test]
fn a_deprecated_server_is_imported_and_said_a_deleted_one_or_one_of_a_status_unknown_is_not() {
    let (_, npm) = package_with("npm", Some("1.0.0"), json!({}));
    let mut answer = RegistryMother::answered(
        &UpstreamNameMother::random(),
        "1.0.0",
        vec![npm],
        Vec::new(),
        "deprecated",
    );
    answer["_meta"]["io.modelcontextprotocol.registry/official"]["statusMessage"] =
        json!("moved to another name");

    let deprecated = imported(&entry(answer.clone()));
    assert_eq!(
        deprecated.notes,
        [Note::Deprecated {
            message: Some(String::from("moved to another name"))
        }]
    );

    answer["_meta"]["io.modelcontextprotocol.registry/official"]["status"] = json!("deleted");
    let error = import(&entry(answer.clone()), &PROVENANCE, None, None).unwrap_err();
    assert_eq!(error.code(), "mcp_registry_server_deleted");
    assert!(
        error.to_string().contains("moved to another name"),
        "{error}"
    );

    answer["_meta"]["io.modelcontextprotocol.registry/official"]["status"] = json!("quarantined");
    let error = import(&entry(answer), &PROVENANCE, None, None).unwrap_err();
    assert_eq!(error.code(), "mcp_registry_status_unknown");
}

#[test]
fn the_definition_says_where_it_came_from() {
    let answer = RegistryMother::random();
    let entry = entry(answer);

    let imported = imported(&entry);

    let provenance = imported.definition.provenance.as_ref().unwrap();
    assert_eq!(
        provenance.registry.as_deref(),
        Some("https://registry.example.com")
    );
    assert_eq!(
        provenance.upstream_name.as_deref(),
        Some(entry.name.as_str())
    );
    assert_eq!(provenance.version.as_deref(), Some(entry.version.as_str()));
    assert_eq!(
        provenance.fetched_at.as_deref(),
        Some("2026-11-10T12:00:00Z")
    );
}

/// Whatever is drawn, the import is a definition `add` would write.
#[test]
fn every_import_drawn_passes_what_add_holds_a_definition_to() {
    for _ in 0..100 {
        let entry = entry(RegistryMother::random());

        let imported = imported(&entry);

        let issues = artifact::mcp_strict_issues(
            &imported.definition,
            &imported.name,
            &format!("{}.toml", imported.name),
            &[],
        );
        assert_eq!(issues, [], "{}", imported.text);
    }
}
