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
    ] {
        let error = Registry::at(refused, Client::new()).unwrap_err();
        assert_eq!(error.code(), code, "{refused}");
        assert!(!error.to_string().contains("secret"), "{error}");
    }
}

#[test]
fn a_page_is_asked_for_from_one_to_the_most_the_registry_gives() {
    let registry = Registry::official(Client::new());

    assert_eq!(registry.clone().paging(0).page_limit, 1);
    assert_eq!(registry.clone().paging(7).page_limit, 7);
    assert_eq!(registry.paging(1000).page_limit, PAGE_LIMIT);
}
