//! Unit tests for `http/cache.rs`, kept beside it rather than inside it:
//! what tells one request's answer from another's, and what the cache does
//! with a file it did not write as it writes one. Answered through each
//! adapter, it is `tests/http_test.rs`'s.

use super::*;

use std::sync::atomic::{AtomicI64, Ordering};

use crate::mother::{random, UrlMother, WorkspaceMother};

/// A clock a test moves by hand.
#[derive(Debug)]
struct Hands(AtomicI64);

impl Clock for Hands {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

const FRESH: Duration = Duration::from_secs(600);

/// A cache of a workspace of its own, told the time by a clock at `now`.
fn cache_at(now: i64) -> (tempfile::TempDir, Cache, Arc<Hands>) {
    let (folder, workspace) = WorkspaceMother::empty();
    let clock = Arc::new(Hands(AtomicI64::new(now)));
    let cache = Cache::of(&workspace, FRESH).with_clock(Arc::clone(&clock) as Arc<dyn Clock>);
    (folder, cache, clock)
}

fn request() -> HttpRequest {
    HttpRequest::get(&format!(
        "https://{}/v0/{}",
        UrlMother::host(),
        random::word()
    ))
    .unwrap()
}

fn answer(body: &[u8]) -> HttpResponse {
    HttpResponse::new(
        200,
        [("content-type", "application/json"), ("etag", "\"v1\"")],
        body.to_vec(),
    )
}

#[test]
fn an_answer_kept_is_found_as_it_was_kept_and_when() {
    let (_folder, cache, clock) = cache_at(1_800_000_000);
    let request = request();
    let response = answer(&random::bytes(random::between(0, 4096)));
    let mut warnings = Vec::new();

    cache.keep(&request, &response, &mut warnings);
    let found = cache.find(&request, &mut warnings);

    assert!(warnings.is_empty(), "{warnings:?}");
    let Found::Kept {
        response: kept,
        stored,
    } = found
    else {
        panic!("nothing found");
    };
    assert_eq!(kept, response);
    assert_eq!(stored, clock.now());
}

#[test]
fn nothing_is_found_for_a_request_never_answered() {
    let (_folder, cache, _) = cache_at(1_800_000_000);
    let mut warnings = Vec::new();

    let found = cache.find(&request(), &mut warnings);

    assert!(matches!(found, Found::Nothing));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(!cache.folder().exists(), "a read makes nothing");
}

#[test]
fn an_answer_is_fresh_for_as_long_as_the_cache_says_and_no_longer() {
    let now = 1_800_000_000;
    let (_folder, cache, _) = cache_at(now);
    let fresh = FRESH.as_secs() as i64;

    assert!(cache.is_fresh(now));
    assert!(cache.is_fresh(now - fresh + 1));
    assert!(!cache.is_fresh(now - fresh));
    assert!(!cache.is_fresh(now - fresh * 10));
    // Kept later than now, by a clock that has gone back since.
    assert!(!cache.is_fresh(now + 1));
}

/// Another `Accept` may be answered otherwise; another version asking is
/// no reason to ask again, and a secret is never part of a key.
#[test]
fn a_request_is_kept_apart_by_its_url_and_its_headers_but_who_asks() {
    let first = request();
    let accepting = first.clone().header("accept", "application/json").unwrap();

    assert_ne!(
        key(&first),
        key(&first.at(&request().url().clone()).unwrap())
    );
    assert_ne!(key(&first), key(&accepting));
    assert_eq!(
        key(&first),
        key(&first.clone().header("user-agent", "another/1").unwrap())
    );
    assert_eq!(
        key(&accepting),
        key(&accepting
            .clone()
            .secret("x-api-key", &UrlMother::secret())
            .unwrap())
    );
    assert_eq!(key(&first), key(&first.clone()));
}

#[test]
fn a_key_is_a_file_name_of_hex_digits_saying_nothing_of_the_url() {
    let request = request();

    let name = key(&request).to_string();

    assert_eq!(name.len(), 64);
    assert!(name.bytes().all(|byte| byte.is_ascii_hexdigit()), "{name}");
    assert!(!name.contains(request.url().as_str()));
}

#[test]
fn a_kept_file_starts_with_a_line_saying_what_it_keeps_then_the_body() {
    let (_folder, cache, clock) = cache_at(1_800_000_000);
    let request = request();
    let body = br#"{"servers":[]}"#;

    cache.keep(&request, &answer(body), &mut Vec::new());

    let bytes = fs::read(cache.path(&request)).unwrap();
    let end = bytes.iter().position(|byte| *byte == b'\n').unwrap();
    let head: serde_json::Value = serde_json::from_slice(&bytes[..end]).unwrap();
    assert_eq!(
        head,
        serde_json::json!({
            "format": CACHE_FORMAT,
            "url": request.url().as_str(),
            "stored": clock.now(),
            "status": 200,
            "headers": { "content-type": "application/json", "etag": "\"v1\"" },
        })
    );
    assert_eq!(&bytes[end + 1..], body);
}

#[test]
fn a_kept_answer_another_build_kept_in_a_newer_format_is_left_alone() {
    let (_folder, cache, _) = cache_at(1_800_000_000);
    let request = request();
    cache.keep(&request, &answer(b"old"), &mut Vec::new());
    let newer = format!(
        "{{\"format\":{},\"shape\":\"another\"}}\nnewer",
        CACHE_FORMAT + 1
    );
    fs::write(cache.path(&request), &newer).unwrap();
    let mut warnings = Vec::new();

    let found = cache.find(&request, &mut warnings);

    assert!(matches!(found, Found::Newer));
    assert!(!found.may_be_replaced());
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(fs::read_to_string(cache.path(&request)).unwrap(), newer);
}

#[test]
fn a_kept_file_that_cannot_be_read_is_said_and_may_be_replaced() {
    let (_folder, cache, _) = cache_at(1_800_000_000);
    let request = request();
    cache.keep(&request, &answer(b"old"), &mut Vec::new());
    for broken in [
        String::from("no line at all"),
        String::from("not json\nbody"),
        String::from("{\"url\":\"x\"}\nbody"),
        format!("{{\"format\":{CACHE_FORMAT},\"url\":7}}\nbody"),
        format!(
            "{{\"format\":{CACHE_FORMAT},\"url\":\"{}\",\"stored\":1,\"status\":404,\"headers\":{{}}}}\nbody",
            request.url()
        ),
    ] {
        fs::write(cache.path(&request), &broken).unwrap();
        let mut warnings = Vec::new();

        let found = cache.find(&request, &mut warnings);

        assert!(matches!(found, Found::Nothing), "{broken}");
        assert!(found.may_be_replaced());
        let codes: Vec<&str> = warnings.iter().map(CacheError::code).collect();
        assert_eq!(codes, vec!["http_cache_unreadable"], "{broken}");
        assert!(
            warnings[0]
                .to_string()
                .starts_with(&cache.path(&request).display().to_string()),
            "{}",
            warnings[0]
        );
    }
}

/// What tells requests apart may change, and two keys can collide: a file
/// kept for another URL is never handed to this one.
#[test]
fn a_kept_answer_for_another_url_is_not_found() {
    let (_folder, cache, _) = cache_at(1_800_000_000);
    let (request, other) = (request(), request());
    cache.keep(&other, &answer(b"other"), &mut Vec::new());
    fs::copy(cache.path(&other), cache.path(&request)).unwrap();
    let mut warnings = Vec::new();

    let found = cache.find(&request, &mut warnings);

    assert!(matches!(found, Found::Nothing));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_kept_answer_longer_than_a_request_takes_is_not_found() {
    let (_folder, cache, _) = cache_at(1_800_000_000);
    let request = request();
    let body = random::bytes(random::between(2, 4096));
    cache.keep(&request, &answer(&body), &mut Vec::new());
    let mut warnings = Vec::new();

    let short = cache.find(&request.clone().at_most(body.len() - 1), &mut warnings);
    let whole = cache.find(&request.clone().at_most(body.len()), &mut warnings);

    assert!(matches!(short, Found::Nothing));
    assert!(matches!(whole, Found::Kept { .. }));
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_kept_answer_is_asked_again_by_its_tag_and_its_time() {
    let request = request().header("accept", "application/json").unwrap();
    let kept = Found::Kept {
        response: HttpResponse::new(
            200,
            [
                ("etag", "W/\"abc\""),
                ("last-modified", "Wed, 21 Oct 2026 07:28:00 GMT"),
            ],
            Vec::new(),
        ),
        stored: 0,
    };

    let asked = kept.revalidating(&request);

    assert_eq!(asked.header_value("if-none-match"), Some("W/\"abc\""));
    assert_eq!(
        asked.header_value("if-modified-since"),
        Some("Wed, 21 Oct 2026 07:28:00 GMT")
    );
    assert_eq!(asked.header_value("accept"), Some("application/json"));
    assert_eq!(Found::Nothing.revalidating(&request), request);
}

#[cfg(unix)]
#[test]
fn a_folder_that_is_a_link_is_said_and_neither_read_nor_written() {
    let (folder, cache, _) = cache_at(1_800_000_000);
    let elsewhere = folder.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::create_dir_all(cache.folder().parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&elsewhere, cache.folder()).unwrap();
    let request = request();
    let mut warnings = Vec::new();

    let found = cache.find(&request, &mut warnings);
    cache.keep(&request, &answer(b"kept"), &mut warnings);

    assert!(matches!(found, Found::Barred));
    assert!(!found.may_be_replaced());
    let codes: Vec<&str> = warnings.iter().map(CacheError::code).collect();
    assert_eq!(codes, vec!["http_cache_linked", "http_cache_linked"]);
    assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn a_kept_file_that_is_a_link_is_read_as_nothing_and_replaced_not_written_through() {
    let (folder, cache, _) = cache_at(1_800_000_000);
    let request = request();
    let outside = folder.path().join("outside");
    fs::write(&outside, "somebody's").unwrap();
    fs::create_dir_all(cache.folder()).unwrap();
    std::os::unix::fs::symlink(&outside, cache.path(&request)).unwrap();
    let mut warnings = Vec::new();

    let found = cache.find(&request, &mut warnings);
    cache.keep(&request, &answer(b"kept"), &mut warnings);

    assert!(matches!(found, Found::Nothing));
    let codes: Vec<&str> = warnings.iter().map(CacheError::code).collect();
    assert_eq!(codes, vec!["http_cache_unreadable"]);
    assert_eq!(fs::read_to_string(&outside).unwrap(), "somebody's");
    assert!(!fs::symlink_metadata(cache.path(&request))
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn a_folder_that_cannot_be_made_is_said_and_the_answer_handed_back_all_the_same() {
    let (_folder, cache, _) = cache_at(1_800_000_000);
    // A file where the folder goes.
    fs::create_dir_all(cache.folder().parent().unwrap()).unwrap();
    fs::write(cache.folder(), "not a folder").unwrap();
    let mut warnings = Vec::new();

    cache.keep(&request(), &answer(b"kept"), &mut warnings);

    let codes: Vec<&str> = warnings.iter().map(CacheError::code).collect();
    assert_eq!(codes, vec!["http_cache_unwritable"]);
}

#[test]
fn each_cache_error_gives_its_own_code_listed_in_order() {
    let path = PathBuf::from("/w/.mindweaver/cache/http");
    let every = [
        (
            CacheError::Unreadable {
                path: path.clone(),
                detail: String::from("no line"),
            },
            "http_cache_unreadable",
        ),
        (
            CacheError::Unwritable {
                path: path.clone(),
                source: io::Error::other("read-only"),
            },
            "http_cache_unwritable",
        ),
        (CacheError::Linked { path }, "http_cache_linked"),
    ];

    for (error, code) in &every {
        assert_eq!(error.code(), *code, "{error:?}");
    }
    let given: Vec<&str> = every.iter().map(|(_, code)| *code).collect();
    assert_eq!(CacheError::CODES, given.as_slice());
}
