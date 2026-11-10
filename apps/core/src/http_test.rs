//! Unit tests for `http.rs`, kept beside it rather than inside it: the
//! words a page names the next by, and the sites a URL is on. What the
//! client makes of an answer, through each adapter, is `tests/http_test.rs`.

use super::*;

use crate::mother::{random, UrlMother};

fn linked(link: &str) -> HttpResponse {
    HttpResponse::new(200, [("link", link)], Vec::new())
}

#[test]
fn a_query_is_begun_or_added_to_before_any_fragment() {
    let host = UrlMother::host();
    for (url, expected) in [
        (
            format!("https://{host}/v0/servers"),
            format!("https://{host}/v0/servers?cursor=abc"),
        ),
        (
            format!("https://{host}/v0/servers?limit=100"),
            format!("https://{host}/v0/servers?limit=100&cursor=abc"),
        ),
        (
            format!("https://{host}/v0/servers?"),
            format!("https://{host}/v0/servers?cursor=abc"),
        ),
        (
            format!("https://{host}/v0/servers?limit=1&"),
            format!("https://{host}/v0/servers?limit=1&cursor=abc"),
        ),
        (
            format!("https://{host}/v0/servers#top"),
            format!("https://{host}/v0/servers?cursor=abc#top"),
        ),
    ] {
        assert_eq!(with_query(&url, "cursor", "abc"), expected);
    }
}

/// A cursor is the server's, whatever it holds: one with a `&`, a `=` or a
/// `#` in it would otherwise reach it as another query.
#[test]
fn a_query_s_name_and_value_reach_the_server_as_they_were_given() {
    let url = format!("https://{}/x", UrlMother::host());

    assert_eq!(
        with_query(&url, "next page", "a/b+c=d&e#f%g é~_.-Z9"),
        format!("{url}?next%20page=a%2Fb%2Bc%3Dd%26e%23f%25g%20%C3%A9~_.-Z9")
    );
}

#[test]
fn a_query_value_of_unreserved_characters_is_left_as_it_is() {
    let url = format!("https://{}/x", UrlMother::host());
    for _ in 0..100 {
        let value = random::drawn_from(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-._~",
            random::between(0, 40),
        );

        assert_eq!(with_query(&url, "v", &value), format!("{url}?v={value}"));
    }
}

#[test]
fn the_next_link_is_the_one_named_next_among_any() {
    let host = UrlMother::host();
    let (next, last, first) = (
        format!("https://{host}/search?q=a&page=2"),
        format!("https://{host}/search?q=a&page=9"),
        format!("https://{host}/search?q=a&page=1"),
    );

    for link in [
        format!("<{next}>; rel=\"next\", <{last}>; rel=\"last\""),
        format!("<{last}>; rel=\"last\", <{next}>; rel=\"next\""),
        format!("<{first}>; rel=\"first\",<{next}>;rel=next"),
        format!("<{next}>; title=\"a, b\"; rel=\"next\""),
        format!("<{next}>; REL=\"Next\""),
        format!("<{next}>; rel=\"prefetch next\""),
    ] {
        assert_eq!(next_link(&linked(&link)), Some(next.as_str()), "{link}");
    }
}

#[test]
fn an_answer_naming_no_next_page_has_no_next_link() {
    let url = format!("https://{}/search?page=9", UrlMother::host());
    for link in [
        format!("<{url}>; rel=\"last\""),
        format!("<{url}>; rel=\"nextish\""),
        format!("<{url}>"),
        format!("<{url}; rel=\"next\""),
        String::new(),
    ] {
        assert_eq!(next_link(&linked(&link)), None, "{link}");
    }
    assert_eq!(
        next_link(&HttpResponse::new(
            200,
            Vec::<(&str, &str)>::new(),
            Vec::new()
        )),
        None
    );
}

/// Read from a `Link` sent over several lines as from one: a response
/// keeps a header sent twice as one list.
#[test]
fn a_next_link_sent_on_a_line_of_its_own_is_found() {
    let host = UrlMother::host();
    let next = format!("https://{host}/p/2");
    let response = HttpResponse::new(
        200,
        [
            ("link", format!("<https://{host}/p/9>; rel=\"last\"")),
            ("link", format!("<{next}>; rel=\"next\"")),
        ],
        Vec::new(),
    );

    assert_eq!(next_link(&response), Some(next.as_str()));
}

#[test]
fn a_site_is_its_scheme_host_and_port_in_any_case() {
    let host = UrlMother::host();
    assert_eq!(
        origin(&format!("HTTPS://{}/a?b#c", host.to_uppercase())),
        format!("https://{host}")
    );
    assert_eq!(
        origin(&format!("https://{host}?q=1")),
        format!("https://{host}")
    );
    assert_eq!(
        origin(&format!("https://{host}#x")),
        format!("https://{host}")
    );
    assert_ne!(
        origin(&format!("https://{host}/a")),
        origin(&format!("http://{host}/a"))
    );
    assert_ne!(
        origin(&format!("https://{host}/a")),
        origin(&format!("https://{host}:8443/a"))
    );
    assert_ne!(
        origin(&format!("https://{host}/a")),
        origin(&format!("https://api.{host}/a"))
    );
}

#[test]
fn only_an_https_url_is_asked_over_tls() {
    let host = UrlMother::host();
    assert!(is_https(&format!("https://{host}")));
    assert!(is_https(&format!("HTTPS://{host}")));
    assert!(!is_https(&format!("http://{host}")));
    assert!(!is_https(&format!("ftp://{host}")));
    assert!(!is_https("/relative"));
}

#[test]
fn each_fetch_error_of_its_own_gives_its_own_code_listed_in_order() {
    let url = Redacted::new(&format!("https://{}/x", UrlMother::host()));
    let next = Redacted::new(&format!("https://{}/y", UrlMother::host()));
    let every = [
        (
            FetchError::Status {
                url: url.clone(),
                status: 404,
            },
            "http_status_unsuccessful",
        ),
        (
            FetchError::RateLimited {
                url: url.clone(),
                status: 429,
                resets: None,
            },
            "http_rate_limited",
        ),
        (
            FetchError::Redirects {
                url: url.clone(),
                most: MOST_REDIRECTS,
            },
            "http_redirects_exceeded",
        ),
        (
            FetchError::Downgraded {
                url: url.clone(),
                to: next.clone(),
            },
            "http_redirect_downgraded",
        ),
        (
            FetchError::Json {
                url: url.clone(),
                detail: Redacted::new("expected value"),
            },
            "http_json_invalid",
        ),
        (
            FetchError::Pages {
                url: url.clone(),
                most: 3,
            },
            "http_pages_exceeded",
        ),
        (
            FetchError::Repeated {
                url: url.clone(),
                next: next.clone(),
            },
            "http_page_repeated",
        ),
        (FetchError::Elsewhere { url, next }, "http_page_elsewhere"),
    ];

    for (error, code) in &every {
        assert_eq!(error.code(), *code, "{error:?}");
    }
    let given: Vec<&str> = every.iter().map(|(_, code)| *code).collect();
    assert_eq!(FetchError::CODES, given.as_slice());
}

/// A request answered nothing failed for the port's reason, and says the
/// port's code and words, never ones of the client's own.
#[test]
fn a_fetch_error_carrying_an_http_error_says_its_code_and_its_words() {
    let http = HttpError::TimedOut {
        url: Redacted::new(&format!("https://{}/x", UrlMother::host())),
        after: std::time::Duration::from_secs(30),
    };
    let (code, said) = (http.code(), http.to_string());

    let error = FetchError::from(http);

    assert_eq!(error.code(), code);
    assert!(!FetchError::CODES.contains(&code));
    assert_eq!(error.to_string(), said);
}

/// What an answer with `status` and `headers` fails as.
fn failed_as(status: u16, headers: &[(&str, &str)]) -> FetchError {
    let request = HttpRequest::get(&format!("https://{}/search", UrlMother::host())).unwrap();
    answered(
        &request,
        HttpResponse::new(status, headers.iter().copied(), Vec::new()),
    )
    .unwrap_err()
}

#[test]
fn a_429_is_a_rate_limit_whatever_it_says() {
    let error = failed_as(429, &[]);

    assert!(
        matches!(
            &error,
            FetchError::RateLimited {
                status: 429,
                resets: None,
                ..
            }
        ),
        "{error:?}"
    );
    assert_eq!(error.code(), "http_rate_limited");
    assert!(
        error.to_string().ends_with(
            "rate limited (status 429), and does not say when it lets a request through again"
        ),
        "{error}"
    );
}

/// GitHub's API answers its limits with a `403`, the first with no request
/// left and the second with a wait asked for; any other `403` is a token
/// without the right, and no limit.
#[test]
fn a_403_is_a_rate_limit_only_when_it_says_it_is_one() {
    let left = failed_as(403, &[("X-RateLimit-Remaining", "0")]);
    let wait = failed_as(403, &[("Retry-After", "30")]);
    let refused = failed_as(403, &[("X-RateLimit-Remaining", "12")]);

    assert_eq!(left.code(), "http_rate_limited", "{left:?}");
    assert_eq!(wait.code(), "http_rate_limited", "{wait:?}");
    assert!(
        matches!(&refused, FetchError::Status { status: 403, .. }),
        "{refused:?}"
    );
}

#[test]
fn a_rate_limit_says_when_it_resets_as_its_answer_says() {
    for (headers, said) in [
        (vec![("retry-after", "1")], "in 1 second"),
        (vec![("retry-after", " 60 ")], "in 60 seconds"),
        // A wait asked for comes first, as GitHub asks a caller to read it.
        (
            vec![("retry-after", "5"), ("x-ratelimit-reset", "1800000000")],
            "in 5 seconds",
        ),
        (
            vec![("x-ratelimit-reset", "1800000000")],
            "at 2027-01-15T08:00:00Z",
        ),
        (
            vec![("retry-after", "Wed, 21 Oct 2026 07:28:00 GMT")],
            "at Wed, 21 Oct 2026 07:28:00 GMT",
        ),
    ] {
        let error = failed_as(429, &headers);

        assert!(
            error
                .to_string()
                .ends_with(&format!(", and lets a request through again {said}")),
            "{headers:?}: {error}"
        );
    }
}

/// A date the server wrote is shown as text, with nothing a terminal would
/// act on.
#[test]
fn a_reset_said_as_a_date_is_shown_without_control_characters() {
    let resets = match failed_as(429, &[("retry-after", "soon\u{7}now")]) {
        FetchError::RateLimited { resets, .. } => resets,
        other => panic!("{other:?}"),
    };

    assert_eq!(resets, Some(Reset::Said(String::from("soon now"))));
}

#[test]
fn a_time_since_the_epoch_is_said_as_the_utc_time_it_is() {
    for (seconds, said) in [
        (0, "1970-01-01T00:00:00Z"),
        (951_782_400, "2000-02-29T00:00:00Z"),
        (1_709_208_000, "2024-02-29T12:00:00Z"),
        (1_798_761_599, "2026-12-31T23:59:59Z"),
        (-1, "1969-12-31T23:59:59Z"),
    ] {
        assert_eq!(utc(seconds), said, "{seconds}");
    }
}
