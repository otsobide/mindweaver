//! Unit tests for `port.rs`, kept beside it rather than inside it.
//!
//! A port's error is what every adapter of it says in core's words, so its
//! codes and its messages are the same whichever adapter met the failure:
//! pinned here, beside the promise that a message says only what its fields
//! hold, which is what keeps a credential the adapter took out of a URL from
//! coming back in the words around it. An HTTP request and its answer are
//! the port's too, and so are the rules they are made by: which URL may be
//! asked, which header sent, which kept of an answer.

use super::*;

use std::io;

use crate::mother::random;
use crate::mother::{Credentialed, UrlMother};
use crate::url::redacted;

/// One of each variant with a code of its own, built from `url` and
/// `detail`, beside the code it gives.
fn every_own_error(url: &str, detail: &str) -> Vec<(GitError, &'static str)> {
    vec![
        (
            GitError::Url {
                url: url.to_owned(),
                detail: detail.to_owned(),
            },
            "git_url_invalid",
        ),
        (
            GitError::Reference {
                reference: String::from("main"),
                detail: detail.to_owned(),
            },
            "git_reference_invalid",
        ),
        (
            GitError::Cache {
                path: PathBuf::from("/w/.mindweaver/cache/acme"),
                source: io::Error::other(detail.to_owned()),
            },
            "git_cache_failed",
        ),
        (
            GitError::Fetch {
                url: url.to_owned(),
                detail: detail.to_owned(),
            },
            "git_fetch_failed",
        ),
        (
            GitError::Credentials {
                url: url.to_owned(),
            },
            "git_credentials_needed",
        ),
        (
            GitError::Checkout {
                url: url.to_owned(),
                detail: detail.to_owned(),
            },
            "git_checkout_failed",
        ),
        (
            GitError::PinUnreachable {
                url: url.to_owned(),
                pin: crate::mother::PinMother::random(),
                reference: Some(detail.to_owned()),
            },
            "git_pin_unreachable",
        ),
        (GitError::Cancelled, "git_clone_cancelled"),
    ]
}

/// A failure of `url` as an adapter hands it over: the URL and the words
/// quoting it both redacted at the edge, as the gix adapter redacts them.
fn as_an_adapter_says(typed: &Credentialed) -> Vec<GitError> {
    let quoted = UrlMother::quoted(typed);
    every_own_error(&redacted(&typed.typed), &redacted(&quoted.typed))
        .into_iter()
        .map(|(error, _)| error)
        .collect()
}

#[test]
fn each_git_error_of_its_own_gives_its_own_code() {
    for (error, code) in every_own_error("https://example.com/acme/skills.git", "refused") {
        assert_eq!(error.code(), code, "{error:?}");
    }
}

#[test]
fn a_pin_no_branch_reaches_is_said_with_the_branch_it_was_looked_for_on() {
    let pin = crate::mother::PinMother::random();
    let unreachable = |reference: Option<&str>| GitError::PinUnreachable {
        url: String::from("https://github.com/acme/skills"),
        pin: pin.clone(),
        reference: reference.map(str::to_owned),
    };

    assert_eq!(
        unreachable(None).to_string(),
        format!(
            "https://github.com/acme/skills: has no commit {pin} that its default branch \
             reaches, so nothing is gathered at it"
        )
    );
    assert_eq!(
        unreachable(Some("stable")).to_string(),
        format!(
            "https://github.com/acme/skills: has no commit {pin} that `stable` reaches, so \
             nothing is gathered at it"
        )
    );
}

#[test]
fn the_codes_a_git_error_gives_of_its_own_are_listed_in_order() {
    let given: Vec<&str> = every_own_error("u", "d")
        .into_iter()
        .map(|(_, code)| code)
        .collect();

    assert_eq!(GitError::CODES, given.as_slice());
}

/// A clone whose file could not be scrubbed failed for the workspace's
/// reason, and says the workspace's code, never one of the port's own.
#[test]
fn a_git_error_carrying_a_workspace_s_gives_the_workspace_s_code() {
    let carried = [
        WorkspaceError::NotAWorkspace {
            path: PathBuf::from("/w"),
        },
        WorkspaceError::Read {
            path: PathBuf::from("/w/.mindweaver/cache/acme/.git/config"),
            source: io::Error::other("denied"),
        },
        WorkspaceError::Malformed {
            path: PathBuf::from("/w/.mindweaver/cache/acme/.git/config"),
            detail: String::from("not a config"),
        },
    ];
    for workspace in carried {
        let code = workspace.code();
        let said = workspace.to_string();
        let error = GitError::from(workspace);

        assert_eq!(error.code(), code, "{error:?}");
        assert!(!GitError::CODES.contains(&code), "{code}");
        assert_eq!(error.to_string(), said, "said as the workspace says it");
    }
}

#[test]
fn a_url_git_does_not_understand_is_named_with_why() {
    let error = GitError::Url {
        url: String::from("htps//acme"),
        detail: String::from("no scheme"),
    };

    assert_eq!(
        error.to_string(),
        "htps//acme: not a URL git understands: no scheme"
    );
}

#[test]
fn a_reference_that_is_no_name_is_quoted_in_backticks() {
    let error = GitError::Reference {
        reference: String::from("main..dev"),
        detail: String::from("a ref cannot hold two dots"),
    };

    assert_eq!(
        error.to_string(),
        "`main..dev` is not a usable branch or tag name: a ref cannot hold two dots"
    );
}

#[test]
fn a_cache_that_cannot_be_prepared_names_its_folder_and_why() {
    let error = GitError::Cache {
        path: PathBuf::from("/w/.mindweaver/cache/acme"),
        source: io::Error::other("read-only file system"),
    };

    assert_eq!(
        error.to_string(),
        "/w/.mindweaver/cache/acme: the cache directory cannot be prepared: read-only file system"
    );
}

#[test]
fn a_fetch_that_failed_names_the_url_and_what_the_remote_said() {
    let error = GitError::Fetch {
        url: String::from("https://example.com/acme/skills.git"),
        detail: String::from("connection refused"),
    };

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/skills.git: cannot be fetched: connection refused"
    );
}

/// What to do about it is the message's: a gather never prompts, so the
/// person reading it is told where a credential is looked for instead.
#[test]
fn a_remote_that_asks_who_is_fetching_says_where_a_credential_is_looked_for() {
    let error = GitError::Credentials {
        url: String::from("https://example.com/acme/private.git"),
    };

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/private.git: needs credentials, and gathering never \
         prompts for them: configure a git credential helper, or use ssh-agent for an ssh URL"
    );
}

#[test]
fn a_checkout_that_failed_says_the_clone_was_made() {
    let error = GitError::Checkout {
        url: String::from("https://example.com/acme/skills.git"),
        detail: String::from("path too long"),
    };

    assert_eq!(
        error.to_string(),
        "https://example.com/acme/skills.git: cloned, but its files could not be checked out: \
         path too long"
    );
}

#[test]
fn a_cancelled_clone_says_it_was_cancelled() {
    assert_eq!(
        GitError::Cancelled.to_string(),
        "the clone was cancelled before it was finished"
    );
}

/// The adapter redacts at the edge, and the message holds what its fields
/// hold and nothing besides: no variant's words quote the URL again, nor
/// anything an adapter did not hand over, so no form of credential comes
/// back into what the gather log and the screen are given.
#[test]
fn no_git_error_built_at_the_edge_repeats_a_credential_however_it_was_typed() {
    for typed in UrlMother::every_credentialed() {
        for error in as_an_adapter_says(&typed) {
            let said = error.to_string();

            assert!(!said.contains(&typed.secret), "{typed:?} gave {said}");
            assert!(!format!("{error:?}").contains(&typed.secret), "{error:?}");
        }
    }
}

#[test]
fn a_git_error_about_a_url_names_it_as_the_redaction_left_it() {
    for typed in UrlMother::every_credentialed() {
        for error in as_an_adapter_says(&typed) {
            if matches!(
                error,
                GitError::Url { .. }
                    | GitError::Fetch { .. }
                    | GitError::Credentials { .. }
                    | GitError::Checkout { .. }
            ) {
                let said = error.to_string();

                assert!(
                    said.starts_with(&format!("{}: ", typed.clean)),
                    "{typed:?} gave {said}"
                );
            }
        }
    }
}

/// Each URL is said once: a message that named it twice would say it in a
/// spelling of its own the second time, which nothing redacted.
#[test]
fn a_git_error_says_its_url_once() {
    for _ in 0..50 {
        let url = UrlMother::https();
        let detail = random::word();
        for (error, _) in every_own_error(&url, &detail) {
            let said = error.to_string();

            assert!(said.matches(url.as_str()).count() <= 1, "{said}");
        }
    }
}

#[test]
fn a_clone_is_equal_to_one_at_the_same_place_and_revision() {
    let clone = Cloned {
        root: PathBuf::from("/w/.mindweaver/cache/acme"),
        revision: Some(Revision::from("4f9c2a7")),
    };

    assert_eq!(clone.clone(), clone);
    assert_ne!(
        Cloned {
            revision: None,
            ..clone.clone()
        },
        clone
    );
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

/// An https URL a catalog could be at, with a query.
fn catalog_url() -> String {
    format!(
        "https://{}/v0/{}?limit={}",
        UrlMother::host(),
        random::word(),
        random::between(1, 100)
    )
}

#[test]
fn a_request_asks_its_url_saying_who_asks_within_the_defaults() {
    let url = catalog_url();

    let request = HttpRequest::get(&url).unwrap();

    assert_eq!(request.url().as_str(), url);
    assert_eq!(request.timeout(), HTTP_TIMEOUT);
    assert_eq!(request.limit(), HTTP_LIMIT);
    let headers: Vec<(&str, &str)> = request
        .headers()
        .iter()
        .map(|header| (header.name(), header.value()))
        .collect();
    assert_eq!(headers, vec![("user-agent", USER_AGENT)]);
    assert!(!request.carries_secrets());
}

#[test]
fn the_user_agent_names_mindweaver_and_its_version() {
    assert_eq!(
        USER_AGENT,
        format!("mindweaver/{}", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn a_request_is_given_its_own_timeout_and_limit() {
    let timeout = Duration::from_millis(random::between(1, 100_000) as u64);
    let limit = random::between(1, 1 << 30);

    let request = HttpRequest::get(&catalog_url())
        .unwrap()
        .within(timeout)
        .at_most(limit);

    assert_eq!((request.timeout(), request.limit()), (timeout, limit));
}

#[test]
fn a_request_of_any_scheme_but_http_or_https_is_refused_by_its_code() {
    let host = UrlMother::host();
    for url in [
        format!("ftp://{host}/catalog"),
        format!("file:///srv/{host}"),
        format!("ssh://git@{host}/catalog"),
        format!("{host}/catalog"),
        String::from("/srv/catalog.json"),
        format!("https:/{host}"),
    ] {
        let error = HttpRequest::get(&url).unwrap_err();

        assert_eq!(error.code(), "http_url_invalid", "{url}: {error:?}");
        assert!(error.to_string().starts_with(&url), "{error}");
    }
}

#[test]
fn a_request_of_http_or_https_in_any_case_is_asked() {
    let host = UrlMother::host();
    for url in [
        format!("http://{host}/x"),
        format!("HTTPS://{host}"),
        format!("https://{host}:8443?q=1"),
        format!("http://127.0.0.1:{}/", random::between(1, 65535)),
        String::from("http://[::1]:8080/x"),
    ] {
        assert!(HttpRequest::get(&url).is_ok(), "{url}");
    }
}

#[test]
fn a_request_with_a_space_a_control_character_or_no_host_is_refused() {
    let host = UrlMother::host();
    for url in [
        format!("https://{host}/a b"),
        format!("https://{host}/a\nb"),
        format!("https://{host}/a\tb"),
        String::from("https:///path"),
        String::from("https://?q=1"),
        String::from("https://"),
    ] {
        let error = HttpRequest::get(&url).unwrap_err();

        assert_eq!(error.code(), "http_url_invalid", "{url:?}: {error:?}");
    }
}

/// A token in a URL would be kept wherever the URL is: in the cache's
/// files, in an error, in a log. A credential goes in a header.
#[test]
fn a_request_of_a_url_carrying_a_credential_is_refused_without_saying_it() {
    let mut tried = 0;
    for typed in UrlMother::every_credentialed() {
        if !typed.typed.to_ascii_lowercase().starts_with("http") {
            continue;
        }
        tried += 1;

        let error = HttpRequest::get(&typed.typed).unwrap_err();
        let at = HttpRequest::get(&catalog_url())
            .unwrap()
            .at(&typed.typed)
            .unwrap_err();

        for error in [error, at] {
            assert_eq!(error.code(), "http_url_credentialed", "{typed:?}");
            let said = error.to_string();
            assert!(!said.contains(&typed.secret), "{said}");
            assert!(!format!("{error:?}").contains(&typed.secret), "{error:?}");
            assert!(said.starts_with(&typed.clean), "{typed:?} gave {said}");
        }
    }
    assert!(tried > 0);
}

#[test]
fn a_request_at_another_url_keeps_its_headers_timeout_and_limit() {
    let first = HttpRequest::get(&catalog_url())
        .unwrap()
        .header("Accept", "application/json")
        .unwrap()
        .secret("Authorization", &UrlMother::secret())
        .unwrap()
        .within(Duration::from_secs(7))
        .at_most(1234);
    let next = catalog_url();

    let request = first.at(&next).unwrap();

    assert_eq!(request.url().as_str(), next);
    assert_eq!(request.headers(), first.headers());
    assert_eq!(
        (request.timeout(), request.limit()),
        (Duration::from_secs(7), 1234)
    );
}

#[test]
fn a_header_given_again_replaces_what_it_said_under_its_name_in_any_case() {
    let request = HttpRequest::get(&catalog_url())
        .unwrap()
        .header("Accept", "text/plain")
        .unwrap()
        .header("ACCEPT", "application/json")
        .unwrap();

    assert_eq!(request.header_value("accept"), Some("application/json"));
    assert_eq!(
        request
            .headers()
            .iter()
            .filter(|header| header.name() == "accept")
            .count(),
        1
    );
}

#[test]
fn a_header_named_for_a_credential_is_a_secret_however_it_was_added() {
    for name in ["Authorization", "cookie", "PROXY-AUTHORIZATION"] {
        let request = HttpRequest::get(&catalog_url())
            .unwrap()
            .header(name, &UrlMother::secret())
            .unwrap();

        assert!(request.carries_secrets(), "{name}");
    }
    let plain = HttpRequest::get(&catalog_url())
        .unwrap()
        .header("x-client", "mindweaver")
        .unwrap();
    assert!(!plain.carries_secrets());
}

#[test]
fn a_request_without_its_secrets_keeps_every_other_header() {
    let request = HttpRequest::get(&catalog_url())
        .unwrap()
        .header("accept", "application/json")
        .unwrap()
        .secret("x-api-key", &UrlMother::secret())
        .unwrap()
        .header("authorization", &UrlMother::secret())
        .unwrap();

    let without = request.without_secrets();

    assert!(!without.carries_secrets());
    let names: Vec<&str> = without.headers().iter().map(Header::name).collect();
    assert_eq!(names, vec!["user-agent", "accept"]);
}

#[test]
fn a_header_that_could_not_be_sent_is_refused_without_saying_its_value() {
    let secret = UrlMother::secret();
    for (name, value) in [
        ("x-key", format!("{secret}\r\nx-injected: 1")),
        ("x-key", format!("{secret}\n")),
        ("x-key", format!("{secret}\u{e9}")),
        ("x-key", format!("{secret}\0")),
        ("", secret.clone()),
        ("x key", secret.clone()),
        ("x:key", secret.clone()),
    ] {
        for secretly in [false, true] {
            let request = HttpRequest::get(&catalog_url()).unwrap();
            let error = if secretly {
                request.secret(name, &value)
            } else {
                request.header(name, &value)
            }
            .unwrap_err();

            assert_eq!(error.code(), "http_header_invalid", "{name:?}: {error:?}");
            assert!(!error.to_string().contains(&secret), "{error}");
            assert!(!format!("{error:?}").contains(&secret), "{error:?}");
        }
    }
}

#[test]
fn a_request_shows_no_secret_it_sends() {
    let secret = UrlMother::secret();
    let request = HttpRequest::get(&catalog_url())
        .unwrap()
        .secret("x-api-key", &secret)
        .unwrap()
        .header("authorization", &format!("Bearer {secret}"))
        .unwrap()
        .header("accept", "application/json")
        .unwrap();

    let shown = format!("{request:?}");

    assert!(!shown.contains(&secret), "{shown}");
    assert!(shown.contains("x-api-key: <secret>"), "{shown}");
    assert!(shown.contains("accept: application/json"), "{shown}");
}

#[test]
fn a_response_keeps_only_the_headers_core_reads_named_in_lowercase() {
    let response = HttpResponse::new(
        200,
        [
            ("Content-Type", "application/json"),
            ("ETag", "\"v1\""),
            ("Set-Cookie", "session=s3cret"),
            ("X-RateLimit-Remaining", "59"),
            ("Retry-After", " 120 "),
        ],
        Vec::new(),
    );

    assert_eq!(
        response.headers(),
        &BTreeMap::from([
            (
                String::from("content-type"),
                String::from("application/json")
            ),
            (String::from("etag"), String::from("\"v1\"")),
            (String::from("retry-after"), String::from("120")),
        ])
    );
    assert_eq!(response.header("Content-TYPE"), Some("application/json"));
    assert_eq!(response.header("set-cookie"), None);
}

#[test]
fn a_header_sent_twice_is_kept_as_one_list() {
    let response = HttpResponse::new(
        200,
        [
            ("link", "<https://a.example.com/2>; rel=\"next\""),
            ("Link", "<https://a.example.com/9>; rel=\"last\""),
        ],
        Vec::new(),
    );

    assert_eq!(
        response.header("link"),
        Some("<https://a.example.com/2>; rel=\"next\", <https://a.example.com/9>; rel=\"last\"")
    );
}

#[test]
fn a_response_says_whether_it_is_a_success_or_a_redirect() {
    for status in 100..600 {
        let response = HttpResponse::new(status, Vec::<(&str, &str)>::new(), Vec::new());

        assert_eq!(
            response.is_success(),
            (200..300).contains(&status),
            "{status}"
        );
        assert_eq!(
            response.is_redirect(),
            [301, 302, 303, 307, 308].contains(&status),
            "{status}"
        );
    }
}

#[test]
fn a_response_shows_its_body_by_its_length() {
    let body = random::bytes(random::between(100, 1000));
    let response = HttpResponse::new(200, Vec::<(&str, &str)>::new(), body.clone());

    assert!(
        format!("{response:?}").contains(&format!("{} bytes", body.len())),
        "{response:?}"
    );
    assert_eq!(response.body(), body.as_slice());
}

/// One of each variant of an HTTP error, about `url`, beside the code it
/// gives.
fn every_http_error(url: &str) -> Vec<(HttpError, &'static str)> {
    let url = Redacted::new(url);
    let detail = Redacted::new("refused");
    vec![
        (
            HttpError::Url {
                url: url.clone(),
                detail: detail.clone(),
            },
            "http_url_invalid",
        ),
        (
            HttpError::Credentials { url: url.clone() },
            "http_url_credentialed",
        ),
        (
            HttpError::Header {
                name: String::from("x-key"),
                detail: String::from("a line break"),
            },
            "http_header_invalid",
        ),
        (
            HttpError::Connect {
                url: url.clone(),
                detail: detail.clone(),
            },
            "http_connect_failed",
        ),
        (
            HttpError::TimedOut {
                url: url.clone(),
                after: Duration::from_secs(30),
            },
            "http_timed_out",
        ),
        (
            HttpError::TooLarge {
                url: url.clone(),
                limit: HTTP_LIMIT,
            },
            "http_too_large",
        ),
        (HttpError::Fetch { url, detail }, "http_fetch_failed"),
    ]
}

#[test]
fn each_http_error_gives_its_own_code_listed_in_order() {
    let every = every_http_error(&catalog_url());
    let given: Vec<&str> = every.iter().map(|(_, code)| *code).collect();

    for (error, code) in &every {
        assert_eq!(error.code(), *code, "{error:?}");
    }
    assert_eq!(HttpError::CODES, given.as_slice());
}

/// Built from a URL somebody typed with a credential, every message says
/// it as the redaction leaves it, once, and never the credential.
#[test]
fn no_http_error_says_a_credential_and_each_says_its_url_once() {
    for typed in UrlMother::every_credentialed() {
        for (error, _) in every_http_error(&typed.typed) {
            let said = error.to_string();

            assert!(!said.contains(&typed.secret), "{said}");
            assert!(!format!("{error:?}").contains(&typed.secret), "{error:?}");
            if !matches!(error, HttpError::Header { .. }) {
                assert!(said.starts_with(&format!("{}: ", typed.clean)), "{said}");
                assert_eq!(said.matches(typed.clean.as_str()).count(), 1, "{said}");
            }
        }
    }
}

#[test]
fn an_http_error_says_what_went_wrong_in_words_of_its_own() {
    let url = "https://registry.example.com/v0/servers";
    let said: Vec<String> = every_http_error(url)
        .into_iter()
        .map(|(error, _)| error.to_string())
        .collect();

    assert_eq!(
        said,
        vec![
            format!("{url}: not a URL a catalog is read from: refused"),
            format!(
                "{url}: carries a credential, which is never sent in a URL: \
                 it goes in a header of its own"
            ),
            String::from("`x-key`: not a header that can be sent: a line break"),
            format!("{url}: cannot be reached: refused"),
            format!("{url}: answered nothing whole within 30s"),
            format!("{url}: answered more than the 8388608 bytes it may"),
            format!("{url}: cannot be read: refused"),
        ]
    );
}
