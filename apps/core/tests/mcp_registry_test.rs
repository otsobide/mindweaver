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
//! answer holds each test makes of a recorded one, saying what it changed.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mindweaver_core::http::{Cache, Client, Via};
use mindweaver_core::mcp_registry::{Listing, Registry, RegistryError, Status, OFFICIAL};
use mindweaver_core::mother::{UrlMother, WorkspaceMother};
use mindweaver_core::port::{Http, HttpError, HttpRequest, HttpResponse};
use serde_json::{json, Value};

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
    let registry = Registry::official(Client::through(recorded.clone() as Arc<dyn Http>));
    (recorded, registry)
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
