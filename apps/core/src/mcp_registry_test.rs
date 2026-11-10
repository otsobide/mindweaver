//! Unit tests for `mcp_registry.rs`, kept beside it rather than inside it:
//! how an answer is read, and which time a listing is as of. What a
//! registry is asked and what is made of its answers, through a fake, is
//! `tests/mcp_registry_test.rs`.

use serde_json::json;

use super::*;
use crate::mother::{RegistryMother, UpstreamNameMother};

#[test]
fn a_status_is_read_by_the_word_the_registry_writes_and_kept_when_unknown() {
    for (word, status) in [
        (Some("active"), Status::Active),
        (Some("deprecated"), Status::Deprecated),
        (Some("deleted"), Status::Deleted),
        (None, Status::Active),
        (
            Some("quarantined"),
            Status::Other(String::from("quarantined")),
        ),
    ] {
        assert_eq!(Status::of(word), status, "{word:?}");
        assert_eq!(status.slug(), word.unwrap_or("active"));
    }
}

#[test]
fn an_answer_reads_as_an_entry_of_what_the_registry_says_of_it() {
    let name = UpstreamNameMother::random();
    let version = RegistryMother::version();
    let answer = RegistryMother::answered(&name, &version, Vec::new(), Vec::new(), "deprecated");

    let entry = Entry::read(answer.clone()).unwrap();

    assert_eq!(entry.name, name);
    assert_eq!(entry.version, version);
    assert_eq!(entry.status, Status::Deprecated);
    assert!(entry.latest);
    assert_eq!(
        entry.updated_at.as_deref(),
        answer["_meta"]["io.modelcontextprotocol.registry/official"]["updatedAt"].as_str()
    );
    assert_eq!(
        entry.schema.as_deref(),
        Some(crate::mother::registry::SCHEMA)
    );
}

/// Go writes a list nobody filled as `null`, and the registry is written
/// in Go; a field a newer schema adds is read past.
#[test]
fn null_lists_read_as_empty_and_fields_unknown_are_read_past() {
    let entry = Entry::read(json!({
        "server": {
            "name": "io.example.acme/weather",
            "version": "1.0.0",
            "description": null,
            "packages": null,
            "remotes": null,
            "icons": [{ "src": "https://example.com/icon.png" }],
            "somethingNew": { "nested": true },
        },
    }))
    .unwrap();

    assert_eq!(entry.ways(), []);
    assert_eq!(entry.description, "");
    assert_eq!(entry.status, Status::Active, "no registry's word for it");
    assert_eq!(entry.updated_at, None);
}

#[test]
fn an_answer_with_no_server_name_or_version_does_not_read() {
    for answer in [
        json!({}),
        json!({ "server": { "version": "1.0.0" } }),
        json!({ "server": { "name": "io.example.acme/weather" } }),
        json!({ "server": { "name": 7, "version": "1.0.0" } }),
        json!([]),
    ] {
        assert!(Entry::read(answer.clone()).is_err(), "{answer}");
    }
}

/// A time is as of the registry's clock, so the newest is the one a
/// refresh asks from, however it was written.
#[test]
fn a_listing_is_as_of_the_newest_time_read_into_it() {
    let (older, newer) = ("2026-10-26T20:15:18.7Z", "2026-10-26T20:15:18.72Z");

    assert_eq!(later(None, Some(older)).as_deref(), Some(older));
    assert_eq!(
        later(Some(older.into()), Some(newer)).as_deref(),
        Some(newer)
    );
    assert_eq!(
        later(Some(newer.into()), Some(older)).as_deref(),
        Some(newer)
    );
    assert_eq!(
        later(Some(newer.into()), Some("yesterday")).as_deref(),
        Some(newer)
    );
    assert_eq!(later(Some(newer.into()), None).as_deref(), Some(newer));
    assert_eq!(later(None, Some("yesterday")), None);
}

#[test]
fn a_registry_is_at_an_address_a_request_can_ask_with_no_slash_after_it() {
    assert_eq!(Registry::official(Client::new()).base(), OFFICIAL);
    assert_eq!(
        Registry::at(" https://registry.example.com/mirror/ ", Client::new())
            .unwrap()
            .base(),
        "https://registry.example.com/mirror"
    );
    for (refused, code) in [
        ("ftp://registry.example.com", "http_url_invalid"),
        (
            "https://user:secret@registry.example.com",
            "http_url_credentialed",
        ),
        (
            "https://registry.example.com/?page=1",
            "mcp_registry_base_invalid",
        ),
        (
            "https://registry.example.com/#top",
            "mcp_registry_base_invalid",
        ),
        ("http://registry.example.com", "mcp_registry_base_insecure"),
        ("HTTP://10.0.0.7:8080/mirror", "mcp_registry_base_insecure"),
    ] {
        let error = Registry::at(refused, Client::new()).unwrap_err();
        assert_eq!(error.code(), code, "{refused}");
        assert!(!error.to_string().contains("secret"), "{error}");
    }
}

/// What a registry answers is a program agents start, so plain http is
/// taken only from one on this machine, a test's or a person's own.
#[test]
fn a_registry_on_this_machine_may_be_asked_over_plain_http() {
    for here in [
        "http://127.0.0.1:8080",
        "http://localhost/registry",
        "http://[::1]:9/",
        "http://127.3.4.5",
    ] {
        assert!(Registry::at(here, Client::new()).is_ok(), "{here}");
    }
}

/// A status goes out as the registry's word, and reads back from any word,
/// one this build does not know kept as it was written.
#[test]
fn a_status_goes_out_as_its_word_and_reads_back_from_it() {
    for status in [
        Status::Active,
        Status::Deprecated,
        Status::Deleted,
        Status::Other(String::from("quarantined")),
    ] {
        let written = serde_json::to_value(&status).unwrap();

        assert_eq!(written, json!(status.slug()));
        assert_eq!(serde_json::from_value::<Status>(written).unwrap(), status);
    }
}

/// Each way by the word a front end says it by: a package by its registry
/// type, a remote by the `type` a definition writes for its transport.
#[test]
fn a_way_is_said_by_its_registry_type_or_its_transport() {
    let url = Redacted::new("https://example.com/mcp");
    for (way, word) in [
        (
            Way::Package {
                registry_type: String::from("npm"),
                identifier: String::from("@acme/tools"),
                version: None,
                transport: String::from("stdio"),
            },
            "npm",
        ),
        (
            Way::Remote {
                transport: String::from("streamable-http"),
                url: url.clone(),
            },
            "http",
        ),
        (
            Way::Remote {
                transport: String::from("sse"),
                url: url.clone(),
            },
            "sse",
        ),
        (
            Way::Remote {
                transport: String::from("websocket"),
                url,
            },
            "websocket",
        ),
        (
            Way::Unreadable {
                detail: String::from("missing field `url`"),
            },
            "unreadable",
        ),
    ] {
        assert_eq!(way.word(), word, "{way:?}");
    }
}

/// What `gather mcp search --json` prints is a contract: each name pinned,
/// an entry's packages and remotes said only as the words of its ways, and
/// a server read back as it went out, as the TUI reads it.
#[test]
fn a_listing_goes_out_by_its_names_each_server_as_its_summary() {
    let name = UpstreamNameMother::random();
    let version = RegistryMother::version();
    let answer = RegistryMother::answered(
        &name,
        &version,
        vec![RegistryMother::package(
            "npm",
            "@acme/tools",
            Some(&version),
        )],
        vec![RegistryMother::remote(
            "streamable-http",
            "https://example.com/mcp",
        )],
        "deprecated",
    );
    let entry = Entry::read(answer).unwrap();
    let listing = Listing {
        query: String::from("acme"),
        entries: vec![entry.clone()],
        as_of: entry.updated_at.clone(),
        whole: true,
    };

    let written = serde_json::to_value(&listing).unwrap();

    assert_eq!(
        written,
        json!({
            "query": "acme",
            "servers": [{
                "name": name.as_str(),
                "version": version,
                "title": null,
                "description": entry.description,
                "status": "deprecated",
                "status_message": null,
                "updated_at": entry.updated_at,
                "ways": ["npm", "http"],
            }],
            "as_of": entry.updated_at,
            "whole": true,
        })
    );
    let read: Vec<Summary> = serde_json::from_value(written["servers"].clone()).unwrap();
    assert_eq!(read, [entry.summary()]);
}

#[test]
fn a_page_is_asked_for_from_one_to_the_most_the_registry_gives() {
    let registry = Registry::official(Client::new());

    assert_eq!(registry.clone().paging(0).page_limit, 1);
    assert_eq!(registry.clone().paging(7).page_limit, 7);
    assert_eq!(registry.paging(1000).page_limit, PAGE_LIMIT);
}
