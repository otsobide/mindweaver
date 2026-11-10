//! `mindweaver gather mcp`: an MCP registry searched, and one of its
//! servers written into the library (#107), each line run in process
//! through the real parsers, against the registry on loopback
//! (`common::mcp_registry`) answering what the real one recorded. Nothing
//! reaches the network: every line names the loopback registry with
//! `--registry`.

mod common;

use std::path::Path;

use common::mcp_registry::Registry;
use common::*;
use mindweaver_core::http::Client;
use mindweaver_core::mcp_registry::Registry as Core;
use tempfile::TempDir;

/// A workspace, made, and the registry on loopback.
fn workspace() -> (TempDir, Registry) {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    (dir, Registry::recorded())
}

/// `mindweaver gather mcp <words>`, asking `registry`.
fn gather_mcp(dir: &Path, registry: &Registry, words: &[&str]) -> Result<Outcome, Failure> {
    let url = registry.url();
    let mut line = vec!["gather", "mcp", words[0], "--registry", url.as_str()];
    line.extend(&words[1..]);
    weaver(dir, &line)
}

/// The folder the library keeps its servers in, and what it holds.
fn library(dir: &Path) -> (std::path::PathBuf, Vec<String>) {
    let folder = own_directory_for(dir, Kind::Mcp);
    let held = if folder.exists() {
        names_in(&folder)
    } else {
        Vec::new()
    };
    (folder, held)
}

/// The text without its `fetched_at` line, which is the clock's.
fn unfetched(text: &str) -> String {
    text.lines()
        .filter(|line| !line.starts_with("fetched_at = "))
        .map(|line| format!("{line}\n"))
        .collect()
}

#[test]
fn a_search_reads_every_page_and_lists_each_server_by_name_version_status_and_ways() {
    let (dir, registry) = workspace();

    let searched = gather_mcp(dir.path(), &registry, &["search", "airtable"]).unwrap();

    assert_eq!(
        searched.stdout,
        "ai.waystation/airtable                           0.3.1   active  http, sse\n\
         com.airtable/mcp                                 0.1.0   active  http\n\
         com.insightfulpipe/airtable                      1.0.0   active  http\n\
         com.mcparmory/airtable                           1.0.3   active  pypi, oci\n\
         io.github.Automations-Project/airtable-user-mcp  2.1.1   active  npm\n\
         io.github.domdomegg/airtable-mcp-server          1.14.0  active  npm, oci, mcpb\n\
         io.github.mrfentmen/airtable-mcp                 1.1.0   active  npm\n\
         io.github.pipeworx-io/airtable                   0.2.1   active  npm, http\n"
    );
    assert!(searched.ok && searched.stderr.is_empty(), "{searched:?}");
    assert_eq!(registry.paths(), ["/v0.1/servers"; 3]);
    assert!(
        registry.asked()[0].ends_with("search=airtable&version=latest&limit=100"),
        "{:?}",
        registry.asked()
    );
}

/// What a script, or the TUI, reads: each server by its summary, its
/// description and its status's word among it.
#[test]
fn a_search_as_json_holds_each_server_by_its_summary() {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Searched {
        query: String,
        servers: Vec<serde_json::Value>,
        as_of: Option<String>,
        whole: bool,
    }
    let (dir, registry) = workspace();

    let json = gather_mcp(dir.path(), &registry, &["search", "--json", "airtable"]).unwrap();

    let (searched, warnings): (Searched, _) = enveloped(&json);
    assert!(warnings.is_empty());
    assert_eq!(searched.query, "airtable");
    assert!(searched.whole);
    assert!(searched.as_of.is_some());
    assert_eq!(searched.servers.len(), 8);
    assert_eq!(
        searched.servers[5],
        serde_json::json!({
            "name": "io.github.domdomegg/airtable-mcp-server",
            "version": "1.14.0",
            "title": "Airtable",
            "description": "Read and write access to Airtable database schemas, tables, and records.",
            "status": "active",
            "status_message": null,
            "updated_at": searched.servers[5]["updated_at"],
            "ways": ["npm", "oci", "mcpb"],
        })
    );
}

#[test]
fn a_search_nothing_matches_says_so_and_one_too_short_asks_nothing() {
    let (dir, registry) = workspace();

    let none = gather_mcp(dir.path(), &registry, &["search", "nothing-here"]).unwrap();
    let short = gather_mcp(dir.path(), &registry, &["search", "ab"]).unwrap_err();

    assert_eq!(
        none.stdout,
        "the registry lists no server whose name holds `nothing-here`\n"
    );
    assert!(none.ok);
    assert_eq!(short.error.code(), "mcp_registry_query_short");
    assert_eq!(registry.asked().len(), 1, "{:?}", registry.asked());
}

/// The acceptance: without `--yes` nothing is written, not even the
/// library's folder, and what is said is the command an agent would run.
#[test]
fn add_without_yes_writes_nothing_and_says_the_command_an_agent_would_run() {
    let (dir, registry) = workspace();
    let (folder, _) = library(dir.path());

    let foreseen = gather_mcp(
        dir.path(),
        &registry,
        &["add", "io.github.buildkite/buildkite-mcp-server"],
    )
    .unwrap();

    assert_eq!(
        foreseen.stdout,
        format!(
            "io.github.buildkite/buildkite-mcp-server 0.7.0, active\n  runs: docker run -i --rm -e \
             BUILDKITE_API_TOKEN ghcr.io/buildkite/buildkite-mcp-server:0.7.0\n  reads \
             $BUILDKITE_API_TOKEN (a secret) where the agents run\nwould create mcp \
             buildkite-mcp-server at {}\nnothing written: add --yes to go ahead\n",
            folder.join("buildkite-mcp-server.toml").display()
        )
    );
    assert!(foreseen.ok && foreseen.stderr.is_empty(), "{foreseen:?}");
    assert!(!folder.exists(), "{folder:?}");
    assert_eq!(
        registry.paths(),
        ["/v0.1/servers/io.github.buildkite%2Fbuildkite-mcp-server/versions/latest"]
    );
}

/// With `--yes`, the definition core's import makes, written through
/// `add`'s path, which `validate` passes.
#[test]
fn add_with_yes_writes_what_the_import_makes_into_the_library() {
    let (dir, registry) = workspace();
    let (folder, _) = library(dir.path());

    let added = gather_mcp(
        dir.path(),
        &registry,
        &[
            "add",
            "--yes",
            "--version",
            "0.1.2",
            "com.opsmill/infrahub-mcp",
        ],
    )
    .unwrap();

    let file = folder.join("opsmill-infrahub-mcp.toml");
    assert_eq!(
        added.stdout,
        format!(
            "com.opsmill/infrahub-mcp 0.1.2, active\n  runs: uvx infrahub-mcp==0.1.2\n  reads \
             $INFRAHUB_ADDRESS and $INFRAHUB_API_TOKEN (a secret) where the agents run\ncreated \
             mcp opsmill-infrahub-mcp at {}\n",
            file.display()
        )
    );
    let core = Core::at(&registry.url(), Client::new()).unwrap();
    let entry = core
        .server("com.opsmill/infrahub-mcp", Some("0.1.2"))
        .unwrap()
        .value;
    let imported = core.import(&entry, None, None).unwrap();
    assert_eq!(
        unfetched(&fs::read_to_string(&file).unwrap()),
        unfetched(&imported.text)
    );
    let validated = weaver(dir.path(), &["validate", "mcp"]).unwrap();
    assert_eq!(
        validated.stdout,
        "opsmill-infrahub-mcp: ok\n\n1 MCP server checked, 0 invalid\n"
    );
}

/// What `add` refuses is refused before a byte is written, a dry run
/// included: a file already where it goes, and a name an agent keeps for
/// itself, which the import holds to the same rules.
#[test]
fn add_refuses_what_add_refuses_and_leaves_the_library_as_it_was() {
    let (dir, registry) = workspace();
    gather_mcp(dir.path(), &registry, &["add", "--yes", "ai.clawfetch/mcp"]).unwrap();
    let (_, before) = library(dir.path());

    for (words, code) in [
        (vec!["add", "ai.clawfetch/mcp"], "add_exists"),
        (vec!["add", "--yes", "ai.clawfetch/mcp"], "add_exists"),
        (
            vec!["add", "--yes", "--name", "computer-use", "ai.clawfetch/mcp"],
            "mcp_registry_import_unfit",
        ),
        (
            vec!["add", "--yes", "--version", "9.9.9", "ai.clawfetch/mcp"],
            "mcp_registry_server_unknown",
        ),
        (
            vec!["add", "--yes", "app.internetmonitor/internet-monitor"],
            "mcp_registry_import_manual",
        ),
        (
            vec!["add", "--yes", "--way", "2", "ai.clawfetch/mcp"],
            "mcp_registry_way_missing",
        ),
        (
            vec!["add", "--yes", "not-a-name"],
            "mcp_registry_name_invalid",
        ),
    ] {
        let refused = gather_mcp(dir.path(), &registry, &words).unwrap_err();

        assert_eq!(refused.error.code(), code, "{words:?}: {}", refused.error);
        assert_eq!(library(dir.path()).1, before, "{words:?}");
    }
}

/// A server listing several ways says which it writes, and `--way` writes
/// another, by its place from 1; one the import says something of says it
/// as a note, which fails nothing.
#[test]
fn way_writes_another_of_the_ways_listed_and_a_note_fails_nothing() {
    let (dir, registry) = workspace();

    let added = gather_mcp(
        dir.path(),
        &registry,
        &["add", "--way", "2", "ai.biel/biel-ai"],
    )
    .unwrap();

    assert!(
        added.stdout.starts_with(
            "ai.biel/biel-ai 2.0.0, active\n  way 2 of 2 (sse); --way writes another: 1 http\n  \
             reaches: https://mcp.biel.ai/sse?project_slug={project_slug}&domain={domain}\n"
        ),
        "{}",
        added.stdout
    );
    assert!(added.ok);
    assert_eq!(
        stderr_codes(&added),
        [Some(String::from("mcp_registry_sse"))]
    );
    assert!(
        added.stderr[0].text.starts_with("note: "),
        "{:?}",
        added.stderr
    );
}

/// A registry elsewhere is asked only over https: what it answers is a
/// program agents start.
#[test]
fn a_registry_over_plain_http_elsewhere_is_refused_before_anything_is_asked() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();

    for words in [
        [
            "gather",
            "mcp",
            "search",
            "--registry",
            "http://registry.example",
            "airtable",
        ],
        [
            "gather",
            "mcp",
            "add",
            "--registry",
            "http://registry.example",
            "ai.clawfetch/mcp",
        ],
    ] {
        let refused = weaver(dir.path(), &words).unwrap_err();

        assert_eq!(
            refused.error.code(),
            "mcp_registry_base_insecure",
            "{words:?}"
        );
    }
}

/// A read: a search and a dry run make no workspace in a home that has
/// none, as `gather list` makes none.
#[test]
fn a_search_and_a_dry_run_make_no_default_workspace() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();
    let registry = Registry::recorded();
    let url = registry.url();

    let searched = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &["gather", "mcp", "search", "--registry", &url, "clawfetch"],
    )
    .unwrap();
    let foreseen = weaver_at_home(
        elsewhere.path(),
        home.path(),
        &[
            "gather",
            "mcp",
            "add",
            "--registry",
            &url,
            "ai.clawfetch/mcp",
        ],
    )
    .unwrap();

    assert_eq!(searched.stdout, "ai.clawfetch/mcp  0.4.0  active  npm\n");
    assert!(foreseen
        .stdout
        .contains("would create mcp clawfetch-mcp at "));
    assert!(!home.path().join(WEAVER_DIR).exists());
}
