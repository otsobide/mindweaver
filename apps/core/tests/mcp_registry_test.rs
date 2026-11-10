//! The official MCP Registry read through a fake `Http` that answers what
//! the registry answered: each file in `tests/fixtures/mcp-registry` is the
//! body the real registry sent, on 2026-11-10, for the URL `RECORDED` names
//! beside it, recorded once with curl. They were saved as they came, but for
//! each em dash in a description, written as its JSON escape, `\u2014`,
//! which reads back as the same text: the repository holds no em dash
//! (`make lint`). Nothing here reaches the network.
//!
//! Among them are servers written in both schemas the registry serves most,
//! 2025-09-29 and 2025-12-11 (and one in 2025-10-17), a search read a page
//! at a time, the changes a refresh asks for with a server deleted among
//! them, and a version the registry no longer holds. What no recorded
//! answer holds (a package with no version, a registry type nobody
//! publishes to) each test makes of a recorded one, saying what it changed.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mindweaver_core::http::{Cache, Client, Via};
use mindweaver_core::mcp::{InputKind, Transport};
use mindweaver_core::mcp_registry::{
    Entry, ImportError, Imported, Listing, Manual, Note, Registry, RegistryError, Status, Way,
    OFFICIAL,
};
use mindweaver_core::mother::{ProjectMother, UrlMother, WorkspaceMother};
use mindweaver_core::port::{Http, HttpError, HttpRequest, HttpResponse};
use mindweaver_core::{Catalog, Clock, Kind, Share};
use serde_json::{json, Value};
use tempfile::TempDir;

/// Each recorded answer, its status, and the path and query it answered.
const RECORDED: &[(&str, u16, &str)] = &[
    (
        "search-airtable-1.json",
        200,
        "/v0.1/servers?search=airtable&version=latest&limit=4",
    ),
    (
        "search-airtable-2.json",
        200,
        "/v0.1/servers?search=airtable&version=latest&limit=4&cursor=com.mcparmory%2Fairtable%3A1.0.3",
    ),
    (
        "search-airtable-3.json",
        200,
        "/v0.1/servers?search=airtable&version=latest&limit=4&cursor=io.github.pipeworx-io%2Fairtable%3A0.2.1",
    ),
    (
        "changed-loice.json",
        200,
        "/v0.1/servers?search=loice&version=latest&limit=100&updated_since=2026-11-10T10%3A59%3A09.311573Z",
    ),
    (
        "loice-mcp-1.0.1.json",
        200,
        "/v0.1/servers/ai.loice%2Fmcp/versions/1.0.1",
    ),
    (
        "loice-telegram-1.0.1.json",
        404,
        "/v0.1/servers/ai.loice%2Ftelegram/versions/1.0.1",
    ),
    (
        "clawfetch-0.4.0.json",
        200,
        "/v0.1/servers/ai.clawfetch%2Fmcp/versions/0.4.0",
    ),
    (
        "clawfetch-latest.json",
        200,
        "/v0.1/servers/ai.clawfetch%2Fmcp/versions/latest",
    ),
    (
        "infrahub-0.1.2.json",
        200,
        "/v0.1/servers/com.opsmill%2Finfrahub-mcp/versions/0.1.2",
    ),
    (
        "buildkite-0.7.0.json",
        200,
        "/v0.1/servers/io.github.buildkite%2Fbuildkite-mcp-server/versions/0.7.0",
    ),
    (
        "knapcode-0.7.0-beta.json",
        200,
        "/v0.1/servers/io.github.joelverhagen%2FKnapcode.SampleMcpServer/versions/0.7.0-beta",
    ),
    (
        "internet-monitor-1.0.2.json",
        200,
        "/v0.1/servers/app.internetmonitor%2Finternet-monitor/versions/1.0.2",
    ),
    (
        "kite-lite-0.1.3.json",
        200,
        "/v0.1/servers/io.github.MauricioPerera%2Fkite-lite/versions/0.1.3",
    ),
    (
        "adside-1.0.0.json",
        200,
        "/v0.1/servers/ai.adside%2Fadside/versions/1.0.0",
    ),
    (
        "zarvis-0.2.0.json",
        200,
        "/v0.1/servers/com.heyzarvis%2Fzarvis/versions/0.2.0",
    ),
    (
        "biel-ai-2.0.0.json",
        200,
        "/v0.1/servers/ai.biel%2Fbiel-ai/versions/2.0.0",
    ),
    (
        "devmargin-1.0.0.json",
        200,
        "/v0.1/servers/app.devmargin%2Fdevmargin/versions/1.0.0",
    ),
    (
        "anzenna-0.1.json",
        200,
        "/v0.1/servers/ai.anzenna%2Fanzenna/versions/0.1",
    ),
    (
        "queryweaver-0.0.11.json",
        200,
        "/v0.1/servers/com.falkordb%2FQueryWeaver/versions/0.0.11",
    ),
    (
        "greenlight-1.0.0.json",
        200,
        "/v0.1/servers/ai.agentlookups%2Fgreenlight/versions/1.0.0",
    ),
];

/// When every import here says it was fetched.
const FETCHED_AT: i64 = 1_794_312_000;

// ---------------------------------------------------------------------------
// The fake registry
// ---------------------------------------------------------------------------

/// A registry kept in memory: each URL it answers, by the status and body
/// it answers with, and every URL it was asked, in order. A URL it does not
/// hold is answered as the registry answers one, a 404.
#[derive(Debug, Default)]
struct Recorded {
    answers: Mutex<BTreeMap<String, (u16, Vec<u8>)>>,
    asked: Mutex<Vec<String>>,
}

impl Http for Recorded {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let url = request.url().to_string();
        self.asked.lock().unwrap().push(url.clone());
        let (status, body) = self.answers.lock().unwrap().get(&url).cloned().unwrap_or((
            404,
            br#"{"title":"Not Found","status":404,"detail":"Endpoint not found"}"#.to_vec(),
        ));
        Ok(HttpResponse::new(
            status,
            [("content-type", "application/json")],
            body,
        ))
    }
}

impl Recorded {
    /// The registry at `base`, answering every recorded answer there.
    fn at(base: &str) -> Arc<Self> {
        let recorded = Arc::new(Recorded::default());
        for (file, status, path) in RECORDED {
            recorded.answer(&format!("{base}{path}"), *status, fixture(file));
        }
        recorded
    }

    fn answer(&self, url: &str, status: u16, body: Vec<u8>) {
        self.answers
            .lock()
            .unwrap()
            .insert(url.to_owned(), (status, body));
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

#[derive(Debug)]
struct Fixed;

impl Clock for Fixed {
    fn now(&self) -> i64 {
        FETCHED_AT
    }
}

fn fixture(file: &str) -> Vec<u8> {
    fs::read(fixtures().join(file)).unwrap_or_else(|error| panic!("{file}: {error}"))
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mcp-registry")
}

fn json(file: &str) -> Value {
    serde_json::from_slice(&fixture(file)).unwrap()
}

/// The official registry, answered by what it answered.
fn official() -> (Arc<Recorded>, Registry) {
    let recorded = Recorded::at(OFFICIAL);
    let registry = Registry::official(Client::through(recorded.clone() as Arc<dyn Http>))
        .with_clock(Arc::new(Fixed));
    (recorded, registry)
}

/// The version `version` of `name`, as the official registry recorded it.
fn entry(name: &str, version: &str) -> Entry {
    official().1.server(name, Some(version)).unwrap().value
}

/// `answer`, one version as the registry answers it, served where the
/// official registry would serve `name`'s `version`, and read back.
fn entry_of(name: &str, version: &str, answer: &Value) -> Entry {
    let (recorded, registry) = official();
    recorded.answer(
        &format!(
            "{OFFICIAL}/v0.1/servers/{}/versions/{version}",
            name.replace('/', "%2F")
        ),
        200,
        serde_json::to_vec(answer).unwrap(),
    );
    registry.server(name, Some(version)).unwrap().value
}

fn import(entry: &Entry) -> Imported {
    official()
        .1
        .import(entry, None, None)
        .unwrap_or_else(|error| panic!("{}: {error}", entry.name))
}

fn program(imported: &Imported) -> (String, Vec<String>, BTreeMap<String, String>) {
    match &imported.definition.transport {
        Transport::Stdio(stdio) => (stdio.command.clone(), stdio.args.clone(), stdio.env.clone()),
        other => panic!("not a program: {other:?}"),
    }
}

fn address(imported: &Imported) -> (bool, String, BTreeMap<String, String>) {
    match &imported.definition.transport {
        Transport::Http(endpoint) => (false, endpoint.url.to_string(), endpoint.headers.clone()),
        Transport::Sse(endpoint) => (true, endpoint.url.to_string(), endpoint.headers.clone()),
        other => panic!("not an address: {other:?}"),
    }
}

/// Each input as `(name, kind, secret, variable)`.
fn inputs(imported: &Imported) -> Vec<(String, InputKind, bool, String)> {
    imported
        .definition
        .inputs
        .iter()
        .map(|input| {
            (
                input.name.to_string(),
                input.kind,
                input.secret,
                input.env_var.as_deref().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

fn manual(error: ImportError) -> Vec<&'static str> {
    match error {
        ImportError::Manual { steps, .. } => steps.iter().map(Manual::code).collect(),
        other => panic!("not a manual step: {other}"),
    }
}

// ---------------------------------------------------------------------------
// Searching, page by page
// ---------------------------------------------------------------------------

#[test]
fn a_search_reads_every_page_by_the_cursor_each_names() {
    let (recorded, registry) = official();

    let listed = registry.paging(4).search("airtable");

    assert_eq!(
        recorded.asked(),
        RECORDED[..3]
            .iter()
            .map(|(_, _, path)| format!("{OFFICIAL}{path}"))
            .collect::<Vec<_>>()
    );
    assert!(listed.failure.is_none(), "{:?}", listed.failure);
    assert!(listed.warnings.is_empty(), "{:?}", listed.warnings);
    assert!(!listed.refreshed);
    let listing = &listed.listing;
    assert!(listing.whole);
    assert_eq!(listing.query, "airtable");
    assert_eq!(
        listing
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.version.as_str()))
            .collect::<Vec<_>>(),
        [
            ("ai.waystation/airtable", "0.3.1"),
            ("com.airtable/mcp", "0.1.0"),
            ("com.insightfulpipe/airtable", "1.0.0"),
            ("com.mcparmory/airtable", "1.0.3"),
            ("io.github.Automations-Project/airtable-user-mcp", "2.1.1"),
            ("io.github.domdomegg/airtable-mcp-server", "1.14.0"),
            ("io.github.mrfentmen/airtable-mcp", "1.1.0"),
            ("io.github.pipeworx-io/airtable", "0.2.1"),
        ]
    );
    assert_eq!(
        listing.as_of.as_deref(),
        Some("2026-10-30T07:24:54.962676Z"),
        "the newest time of all"
    );
    assert!(listing
        .entries
        .iter()
        .all(|entry| entry.status == Status::Active && entry.latest));
}

#[test]
fn a_search_reads_servers_written_in_either_schema_alike() {
    let listed = official().1.paging(4).search("airtable");

    let dates: Vec<&str> = listed
        .listing
        .entries
        .iter()
        .map(|entry| {
            let schema = entry.schema.as_deref().unwrap();
            &schema[schema.len() - 29..schema.len() - 19]
        })
        .collect();
    for date in ["2025-09-29", "2025-12-11", "2025-10-17"] {
        assert!(dates.contains(&date), "{date}: {dates:?}");
    }
}

/// The registry's address is a constant a front end, or a test, replaces:
/// a mirror serving the same API is read the same way.
#[test]
fn a_registry_at_another_address_is_asked_there_and_nowhere_else() {
    let base = format!("https://{}/mirror", UrlMother::host());
    let recorded = Recorded::at(&base);
    let registry = Registry::at(&format!("{base}/"), Client::through(recorded.clone())).unwrap();

    let listed = registry.paging(4).search("airtable");

    assert_eq!(listed.listing.entries.len(), 8);
    assert!(recorded.asked().iter().all(|url| url.starts_with(&base)));
}

#[test]
fn a_search_shorter_than_the_registry_takes_asks_nothing() {
    let (recorded, registry) = official();

    for query in ["", "  ", "ab", " ab "] {
        let listed = registry.search(query);

        assert_eq!(
            listed.failure.as_ref().map(RegistryError::code),
            Some("mcp_registry_query_short"),
            "{query:?}"
        );
        assert_eq!(listed.listing.entries, []);
        assert!(!listed.listing.whole);
    }
    assert_eq!(recorded.asked(), Vec::<String>::new());
}

#[test]
fn a_page_that_fails_keeps_the_servers_before_it_and_a_refresh_searches_again_whole() {
    let (recorded, registry) = official();
    let second = format!("{OFFICIAL}{}", RECORDED[1].2);
    recorded.answer(&second, 503, Vec::new());
    let registry = registry.paging(4);

    let listed = registry.search("airtable");

    assert_eq!(listed.listing.entries.len(), 4);
    assert_eq!(
        listed.failure.as_ref().map(RegistryError::code),
        Some("http_status_unsuccessful")
    );
    assert!(!listed.listing.whole);

    recorded.answer(&second, 200, fixture("search-airtable-2.json"));
    let again = registry.refresh(&listed.listing);

    assert!(
        !again.refreshed,
        "a listing that stopped short is read whole"
    );
    assert_eq!(again.listing.entries.len(), 8);
    assert!(again.listing.whole);
    assert!(recorded
        .asked()
        .iter()
        .all(|url| !url.contains("updated_since")));
}

#[test]
fn a_server_that_does_not_read_is_passed_over_and_said_and_hides_none_beside_it() {
    let (recorded, registry) = official();
    let mut page = json("search-airtable-1.json");
    page["servers"][1]["server"]
        .as_object_mut()
        .unwrap()
        .remove("name");
    recorded.answer(
        &format!("{OFFICIAL}{}", RECORDED[0].2),
        200,
        serde_json::to_vec(&page).unwrap(),
    );

    let listed = registry.paging(4).search("airtable");

    assert_eq!(listed.listing.entries.len(), 7);
    assert!(listed.listing.whole);
    assert_eq!(
        listed
            .warnings
            .iter()
            .map(RegistryError::code)
            .collect::<Vec<_>>(),
        ["mcp_registry_entry_unreadable"]
    );
}

// ---------------------------------------------------------------------------
// Refreshing what changed since
// ---------------------------------------------------------------------------

/// A search for `loice` read at 10:59:30 on 2026-11-10: `ai.loice/mcp` at
/// 1.0.1, its latest then, and `ai.loice/telegram` 1.0.1, published at
/// 10:59:09 and deleted at 11:00. Both as recorded, but for the second's
/// status, which is as it stood before it was deleted.
fn loice_before() -> Value {
    let mut mcp = json("loice-mcp-1.0.1.json");
    mcp["_meta"]["io.modelcontextprotocol.registry/official"]["isLatest"] = json!(true);
    let mut telegram = json("changed-loice.json")["servers"][1].clone();
    let official = &mut telegram["_meta"]["io.modelcontextprotocol.registry/official"];
    let published = official["publishedAt"].clone();
    official["status"] = json!("active");
    official["statusChangedAt"] = published.clone();
    official["updatedAt"] = published;
    official.as_object_mut().unwrap().remove("statusMessage");
    json!({ "servers": [mcp, telegram], "metadata": { "count": 2 } })
}

fn loice() -> (Arc<Recorded>, Registry, Listing) {
    let (recorded, registry) = official();
    let whole = format!("{OFFICIAL}/v0.1/servers?search=loice&version=latest&limit=100");
    recorded.answer(&whole, 200, serde_json::to_vec(&loice_before()).unwrap());
    let listed = registry.search("loice");
    assert!(listed.failure.is_none(), "{:?}", listed.failure);
    (recorded, registry, listed.listing)
}

#[test]
fn a_refresh_asks_only_what_changed_since_the_newest_time_its_listing_holds() {
    let (recorded, registry, before) = loice();
    assert_eq!(before.as_of.as_deref(), Some("2026-11-10T10:59:09.311573Z"));

    let listed = registry.refresh(&before);

    assert_eq!(
        recorded.asked().last().map(String::as_str),
        Some(format!("{OFFICIAL}{}", RECORDED[3].2).as_str())
    );
    assert!(listed.refreshed);
    assert!(listed.failure.is_none(), "{:?}", listed.failure);
    assert!(listed.listing.whole);
    assert_eq!(
        listed.listing.as_of.as_deref(),
        Some("2026-11-10T11:50:18.181322Z")
    );
}

/// The registry answers what changed since with what was deleted since
/// too, and a refresh expects it: a deleted server is taken out and said.
#[test]
fn a_refresh_replaces_what_changed_and_takes_out_what_was_deleted() {
    let (_, registry, before) = loice();

    let listed = registry.refresh(&before);

    assert_eq!(
        listed
            .listing
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.version.as_str()))
            .collect::<Vec<_>>(),
        [("ai.loice/mcp", "1.0.2")]
    );
    assert_eq!(listed.dropped.len(), 1);
    assert_eq!(listed.dropped[0].name, "ai.loice/telegram");
    assert_eq!(listed.dropped[0].status, Status::Active, "as it was listed");
}

#[test]
fn a_listing_with_no_time_is_searched_again_whole() {
    let (recorded, registry) = official();
    let listing = Listing {
        query: String::from("airtable"),
        entries: Vec::new(),
        as_of: None,
        whole: true,
    };

    let listed = registry.paging(4).refresh(&listing);

    assert!(!listed.refreshed);
    assert_eq!(listed.listing.entries.len(), 8);
    assert_eq!(recorded.asked().len(), 3);
}

// ---------------------------------------------------------------------------
// One version
// ---------------------------------------------------------------------------

#[test]
fn one_version_is_asked_by_its_name_and_version_and_the_latest_as_latest() {
    let (recorded, registry) = official();

    let pinned = registry.server("ai.clawfetch/mcp", Some("0.4.0")).unwrap();
    let latest = registry.server("ai.clawfetch/mcp", None).unwrap();

    assert_eq!(
        recorded.asked(),
        [
            format!("{OFFICIAL}/v0.1/servers/ai.clawfetch%2Fmcp/versions/0.4.0"),
            format!("{OFFICIAL}/v0.1/servers/ai.clawfetch%2Fmcp/versions/latest"),
        ]
    );
    assert_eq!(pinned.value, latest.value);
    assert_eq!(pinned.value.version, "0.4.0");
    assert_eq!(pinned.via, Via::Network);
}

/// What the registry answers for a version it deleted, as it answers one
/// it never held.
#[test]
fn a_version_the_registry_does_not_hold_or_deleted_is_unknown() {
    let (_, registry) = official();

    let error = registry
        .server("ai.loice/telegram", Some("1.0.1"))
        .unwrap_err();

    assert_eq!(error.code(), "mcp_registry_server_unknown");
    assert_eq!(
        error.to_string(),
        "the registry holds no version 1.0.1 of `ai.loice/telegram`"
    );
}

#[test]
fn a_name_no_registry_gives_a_server_asks_nothing() {
    let (recorded, registry) = official();

    for name in [
        "clawfetch",
        "ai.clawfetch/mcp/extra",
        "ai.clawfetch/mcp?x=1",
        "",
    ] {
        let error = registry.server(name, None).unwrap_err();

        assert_eq!(error.code(), "mcp_registry_name_invalid", "{name}");
    }
    assert_eq!(recorded.asked(), Vec::<String>::new());
}

#[test]
fn an_answer_is_kept_in_the_workspace_and_answered_from_there_while_fresh() {
    let (_folder, workspace) = WorkspaceMother::empty();
    let recorded = Recorded::at(OFFICIAL);
    let client =
        Client::through(recorded.clone()).caching(Cache::of(&workspace, Duration::from_secs(600)));
    let registry = Registry::official(client);

    let first = registry.server("ai.clawfetch/mcp", Some("0.4.0")).unwrap();
    let second = registry.server("ai.clawfetch/mcp", Some("0.4.0")).unwrap();

    assert_eq!((first.via, second.via), (Via::Network, Via::Cache));
    assert_eq!(first.value, second.value);
    assert_eq!(recorded.asked().len(), 1);
}

// ---------------------------------------------------------------------------
// Each registry type made into a definition
// ---------------------------------------------------------------------------

/// Each package and remote as the registry lists it, read one at a time:
/// what a front end shows somebody to choose a way from.
#[test]
fn the_ways_a_server_lists_are_read_in_the_order_the_registry_lists_them() {
    let listed = official().1.paging(4).search("airtable");

    let domdomegg = &listed.listing.entries[5];
    assert_eq!(domdomegg.name, "io.github.domdomegg/airtable-mcp-server");
    assert_eq!(
        domdomegg.ways(),
        [
            Way::Package {
                registry_type: "npm".into(),
                identifier: "airtable-mcp-server".into(),
                version: Some("1.14.0".into()),
                transport: "stdio".into(),
            },
            Way::Package {
                registry_type: "oci".into(),
                identifier: "docker.io/domdomegg/airtable-mcp-server:1.14.0".into(),
                version: None,
                transport: "stdio".into(),
            },
            Way::Package {
                registry_type: "mcpb".into(),
                identifier: "https://github.com/domdomegg/airtable-mcp-server/releases/download/v1.14.0/airtable-mcp-server.mcpb".into(),
                version: Some("1.14.0".into()),
                transport: "stdio".into(),
            },
        ]
    );
}

/// The whole file, so the format an import writes is pinned word for word.
#[test]
fn an_npm_package_is_started_by_npx_pinned_and_its_secret_read_from_its_variable() {
    let imported = import(&entry("ai.clawfetch/mcp", "0.4.0"));

    assert_eq!(imported.name, "clawfetch-mcp");
    assert_eq!(imported.way, 0);
    assert_eq!(imported.notes, []);
    assert_eq!(
        imported.text,
        r#"# An MCP server imported from an MCP registry: each agent's own entry
# for it is written from this file, and `show` says how each agent is
# given it.
format = 1
name = "clawfetch-mcp"
description = "Web intelligence for AI agents: fetch, render, extract, research. x402 micropayments, no API keys."

# How an agent starts it: the program, what it is started with, and what
# its environment is given beside the agent's own.
[transport]
type = "stdio"
command = "npx"
args = ["-y", "@clawfetch/mcp@0.4.0"]
env = { CLAWFETCH_PRIVATE_KEY = "{CLAWFETCH_PRIVATE_KEY}" }

# What is supplied when an agent starts it, each written between braces
# where it goes and read from the environment variable it names: no value
# is written here, and a secret never.
[[inputs]]
name = "CLAWFETCH_PRIVATE_KEY"
kind = "env"
description = "EVM private key for a Base wallet holding USDC (pays x402 micropayments gaslessly)"
required = true
secret = true
env_var = "CLAWFETCH_PRIVATE_KEY"

# Where it came from: the registry, what it names the server, and the
# version imported.
[provenance]
registry = "https://registry.modelcontextprotocol.io"
upstream_name = "ai.clawfetch/mcp"
version = "0.4.0"
fetched_at = 2026-11-10T12:00:00Z
"#
    );
}

/// The schema since 2025-12-11 leaves a package's version out; none
/// recorded does, so the recorded one is changed to.
#[test]
fn an_npm_package_with_no_version_runs_whatever_is_latest_and_says_so() {
    let mut answer = json("clawfetch-0.4.0.json");
    answer["server"]["packages"][0]
        .as_object_mut()
        .unwrap()
        .remove("version");

    let imported = import(&entry_of("ai.clawfetch/mcp", "0.4.0", &answer));

    assert_eq!(program(&imported).1, ["-y", "@clawfetch/mcp"]);
    assert_eq!(
        imported.notes,
        [Note::Unversioned {
            identifier: String::from("@clawfetch/mcp")
        }]
    );
}

#[test]
fn a_pypi_package_is_started_by_uvx_pinned_and_what_it_asks_for_read_from_variables() {
    let imported = import(&entry("com.opsmill/infrahub-mcp", "0.1.2"));

    let (command, args, env) = program(&imported);
    assert_eq!(
        (command.as_str(), args),
        ("uvx", vec!["infrahub-mcp==0.1.2".to_owned()])
    );
    assert_eq!(env["INFRAHUB_ADDRESS"], "{INFRAHUB_ADDRESS}");
    assert_eq!(
        inputs(&imported),
        [
            (
                "INFRAHUB_ADDRESS".into(),
                InputKind::Env,
                false,
                "INFRAHUB_ADDRESS".into()
            ),
            (
                "INFRAHUB_API_TOKEN".into(),
                InputKind::Env,
                true,
                "INFRAHUB_API_TOKEN".into()
            ),
        ]
    );
}

/// Its publisher lists the whole `docker` command line as its runtime
/// arguments, the image again among them, which the command holds already.
#[test]
fn an_oci_image_is_run_by_docker_its_tag_in_its_identifier_and_its_secret_passed_in() {
    let imported = import(&entry("io.github.buildkite/buildkite-mcp-server", "0.7.0"));

    let (command, args, env) = program(&imported);
    assert_eq!(command, "docker");
    assert_eq!(
        args,
        [
            "run",
            "-i",
            "--rm",
            "-e",
            "BUILDKITE_API_TOKEN",
            "ghcr.io/buildkite/buildkite-mcp-server:0.7.0"
        ]
    );
    assert_eq!(env["BUILDKITE_API_TOKEN"], "{BUILDKITE_API_TOKEN}");
    assert_eq!(imported.notes, []);
}

#[test]
fn a_nuget_package_is_started_by_dnx_pinned_with_its_arguments_and_what_it_asks_for() {
    let imported = import(&entry(
        "io.github.joelverhagen/Knapcode.SampleMcpServer",
        "0.7.0-beta",
    ));

    let (command, args, env) = program(&imported);
    assert_eq!(command, "dnx");
    assert_eq!(
        args,
        ["Knapcode.SampleMcpServer@0.7.0-beta", "mcp", "start"]
    );
    assert_eq!(env["WEATHER_CHOICES"], "{WEATHER_CHOICES}");
    assert_eq!(
        inputs(&imported),
        [(
            "WEATHER_CHOICES".into(),
            InputKind::Env,
            false,
            "WEATHER_CHOICES".into()
        )]
    );
    assert_eq!(
        imported.definition.inputs[0].description.as_deref(),
        Some("Comma separated list of weather descriptions to randomly select.")
    );
}

#[test]
fn a_bundle_is_a_manual_step_with_where_to_download_it_and_what_it_hashes_to() {
    let entry = entry("app.internetmonitor/internet-monitor", "1.0.2");

    let error = official().1.import(&entry, None, None).unwrap_err();

    assert!(
        error
            .to_string()
            .contains("6414b553daf4ab512828dcc53143c1170bfdb30fde001388731329e6acf3df52"),
        "{error}"
    );
    assert_eq!(manual(error), ["mcp_registry_bundle"]);
}

#[test]
fn a_crate_is_a_manual_step() {
    let entry = entry("io.github.MauricioPerera/kite-lite", "0.1.3");

    assert_eq!(
        manual(official().1.import(&entry, None, None).unwrap_err()),
        ["mcp_registry_crate"]
    );
}

/// No server recorded is published to a registry this build does not
/// know, so the recorded npm one is said to be.
#[test]
fn a_registry_type_unknown_is_a_manual_step_never_guessed() {
    let mut answer = json("clawfetch-0.4.0.json");
    answer["server"]["packages"][0]["registryType"] = json!("rubygems");

    let entry = entry_of("ai.clawfetch/mcp", "0.4.0", &answer);

    assert_eq!(
        manual(official().1.import(&entry, None, None).unwrap_err()),
        ["mcp_registry_type_unknown"]
    );
}

#[test]
fn a_package_that_serves_over_http_once_started_is_a_manual_step() {
    let entry = entry("com.falkordb/QueryWeaver", "0.0.11");

    assert_eq!(
        manual(official().1.import(&entry, None, None).unwrap_err()),
        ["mcp_registry_package_serves"]
    );
}

#[test]
fn a_remote_s_secret_header_is_read_whole_from_a_variable_named_for_the_server() {
    let imported = import(&entry("ai.adside/adside", "1.0.0"));

    let (events, url, headers) = address(&imported);
    assert!(!events);
    assert_eq!(url, "https://api.adside.ai/mcp");
    assert_eq!(headers["Authorization"], "{Authorization}");
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

/// URL variables, declared beside the URL since the 2025-12-11 schema and
/// already by servers written in 2025-09-29's.
#[test]
fn a_remote_s_url_variables_are_inputs_of_the_url_in_either_schema() {
    for (name, version, url, variables) in [
        (
            "com.heyzarvis/zarvis",
            "0.2.0",
            "https://relay.heyzarvis.com/t/{tunel}/mcp",
            vec![("tunel", "HEYZARVIS_ZARVIS_TUNEL")],
        ),
        (
            "ai.biel/biel-ai",
            "2.0.0",
            "https://mcp.biel.ai/v2/{project_slug}/mcp",
            vec![("project_slug", "BIEL_AI_PROJECT_SLUG")],
        ),
    ] {
        let imported = import(&entry(name, version));

        assert_eq!(address(&imported).1, url, "{name}");
        assert_eq!(
            inputs(&imported),
            variables
                .into_iter()
                .map(|(input, variable)| (
                    input.to_owned(),
                    InputKind::UrlVar,
                    false,
                    variable.to_owned()
                ))
                .collect::<Vec<_>>(),
            "{name}"
        );
    }
}

#[test]
fn a_remote_over_server_sent_events_is_one_and_said_and_taken_last() {
    let anzenna = import(&entry("ai.anzenna/anzenna", "0.1"));
    assert!(address(&anzenna).0);
    assert_eq!(
        anzenna.notes.iter().map(Note::code).collect::<Vec<_>>(),
        ["mcp_registry_sse"]
    );

    let biel = entry("ai.biel/biel-ai", "2.0.0");
    assert_eq!(import(&biel).way, 0, "streamable HTTP before events");
    let events = official().1.import(&biel, Some(1), None).unwrap();
    assert_eq!(
        address(&events).1,
        "https://mcp.biel.ai/sse?project_slug={project_slug}&domain={domain}"
    );
    assert_eq!(events.definition.inputs.len(), 2);
}

#[test]
fn a_header_whose_variable_is_required_is_written_with_its_bearer_and_its_secret_read() {
    let imported = import(&entry("app.devmargin/devmargin", "1.0.0"));

    assert_eq!(address(&imported).2["Authorization"], "Bearer {token}");
    assert_eq!(
        inputs(&imported),
        [(
            "token".into(),
            InputKind::Header,
            true,
            "DEVMARGIN_TOKEN".into()
        )]
    );
}

#[test]
fn a_deprecated_server_is_imported_and_carries_why() {
    let imported = import(&entry("ai.agentlookups/greenlight", "1.0.0"));

    assert_eq!(
        imported.notes.iter().map(Note::code).collect::<Vec<_>>(),
        ["mcp_registry_deprecated"]
    );
    assert!(imported.notes[0].to_string().contains("410 Gone"));
}

/// A deleted server reaches a reader only among what changed, as the
/// registry answers a refresh: it is never made into a definition.
#[test]
fn a_deleted_server_is_never_imported_and_says_why() {
    let deleted = entry_of_answer(&json("changed-loice.json")["servers"][1]);
    assert_eq!(deleted.status, Status::Deleted);

    let error = official().1.import(&deleted, None, None).unwrap_err();

    assert_eq!(error.code(), "mcp_registry_server_deleted");
    assert!(
        error
            .to_string()
            .contains("Published by mistake; the server is io.roxinbox/agent-outbox"),
        "{error}"
    );
}

fn entry_of_answer(answer: &Value) -> Entry {
    let name = answer["server"]["name"].as_str().unwrap();
    let version = answer["server"]["version"].as_str().unwrap();
    entry_of(name, version, answer)
}

// ---------------------------------------------------------------------------
// Every recorded server
// ---------------------------------------------------------------------------

/// Every server any recorded answer holds, as each answer holds it.
fn every_recorded() -> Vec<Value> {
    let mut answers = Vec::new();
    for (file, status, _) in RECORDED {
        if *status != 200 {
            continue;
        }
        let answer = json(file);
        match answer.get("servers") {
            Some(Value::Array(servers)) => answers.extend(servers.iter().cloned()),
            Some(_) => {}
            None => answers.push(answer),
        }
    }
    answers
}

/// What the registry calls a secret and gives a value anyway, at any depth
/// of `answer`.
fn secret_values(answer: &Value, found: &mut Vec<String>) {
    match answer {
        Value::Object(object) => {
            if object.get("isSecret") == Some(&json!(true)) {
                for key in ["value", "default"] {
                    if let Some(Value::String(value)) = object.get(key) {
                        found.push(value.clone());
                    }
                }
            }
            object
                .values()
                .for_each(|value| secret_values(value, found));
        }
        Value::Array(items) => items.iter().for_each(|item| secret_values(item, found)),
        _ => {}
    }
}

/// The acceptance: every recorded server not deleted is a definition that
/// passes what `validate` holds one to once written in a library, or a
/// manual step, and none holds a value the registry calls a secret.
#[test]
fn every_recorded_server_is_a_definition_validate_passes_or_a_manual_step() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::with_mcp(&dir.path().join("app"), &[], Share::Local);
    let folder = project.directory_for(Kind::Mcp).unwrap();
    let (_, registry) = official();
    let mut imported_names = std::collections::BTreeSet::new();

    for answer in every_recorded() {
        let entry = entry_of_answer(&answer);
        if entry.status == Status::Deleted {
            continue;
        }
        let imported = match registry.import(&entry, None, None) {
            Ok(imported) => imported,
            Err(ImportError::Manual { steps, .. }) => {
                assert!(!steps.is_empty(), "{}", entry.name);
                continue;
            }
            Err(error) => panic!("{}: {error}", entry.name),
        };
        let mut secrets = Vec::new();
        secret_values(&answer, &mut secrets);
        for secret in secrets {
            assert!(!imported.text.contains(&secret), "{}: {secret}", entry.name);
        }
        fs::write(
            folder.join(format!("{}.toml", imported.name)),
            &imported.text,
        )
        .unwrap();
        imported_names.insert(imported.name);
    }

    let catalog = Catalog::discover(std::slice::from_ref(&project));
    assert_eq!(catalog.failures().len(), 0, "{:?}", catalog.failures());
    let servers: Vec<_> = catalog
        .artifacts()
        .iter()
        .filter(|artifact| artifact.kind() == Kind::Mcp)
        .collect();
    assert_eq!(servers.len(), imported_names.len());
    for server in servers {
        assert_eq!(server.validate(), [], "{}", server.name());
    }
    assert!(imported_names.len() >= 15, "{imported_names:?}");
}
