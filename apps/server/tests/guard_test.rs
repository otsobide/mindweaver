//! What keeps other pages out: every request the panel's page would not send
//! is refused, whether or not it carries the cookie, and every one it would
//! send gets through.

mod common;

use common::*;

/// A header to set to a value, or, for `None`, to take out.
type Change = (&'static str, Option<String>);

fn set(name: &'static str, value: impl Into<String>) -> Change {
    (name, Some(value.into()))
}

fn unset(name: &'static str) -> Change {
    (name, None)
}

/// A change to the disk as the panel's page asks for one: the cookie its
/// launch link set, and everything else a browser sends with it.
fn from_the_panel(server: &Listening, changes: &[Change]) -> Request<Body> {
    let cookie = format!("{}={}", server.cookie(), server.token());
    let request = request(
        server,
        Method::POST,
        "/api/v1/tasks",
        Auth::Cookie(&cookie),
        Some(json!({ "name": "change" })),
    );
    changed(request, changes)
}

/// `request` with `changes` made to its headers.
fn changed(mut request: Request<Body>, changes: &[Change]) -> Request<Body> {
    let headers = request.headers_mut();
    for (name, value) in changes {
        match value {
            Some(value) => {
                headers.insert(*name, value.parse().unwrap());
            }
            None => {
                headers.remove(*name);
            }
        }
    }
    request
}

/// Each case names a part of the refusal it must get, so it shows which check
/// turned it away and not only that something did: the endpoint's JSON
/// extractor would answer a text body with a 415 of its own, and a case that
/// asserted only the status would pass with the guard's check gone.
#[tokio::test]
async fn a_request_the_panel_would_not_send_is_refused() {
    let (server, runs) = launch();
    let port = server.address().port();
    let host = "answers only as";
    let origin = "only from its own pages";
    let fetch = "Sec-Fetch-Site: same-origin";
    let json = "must be JSON";
    let custom = "X-Mindweaver: 1";

    let cases: Vec<(&str, Vec<Change>, StatusCode, &str, &str)> = vec![
        (
            "a name rebound to 127.0.0.1",
            vec![set("host", format!("attacker.example:{port}"))],
            StatusCode::FORBIDDEN,
            "unknown_host",
            host,
        ),
        (
            "another port",
            vec![set("host", format!("127.0.0.1:{}", port.wrapping_add(1)))],
            StatusCode::FORBIDDEN,
            "unknown_host",
            host,
        ),
        (
            "no Host",
            vec![unset("host")],
            StatusCode::FORBIDDEN,
            "unknown_host",
            host,
        ),
        (
            "a rebound name, forwarded as this one",
            vec![
                set("host", format!("attacker.example:{port}")),
                set("x-forwarded-host", format!("127.0.0.1:{port}")),
            ],
            StatusCode::FORBIDDEN,
            "unknown_host",
            host,
        ),
        (
            "a page on another site",
            vec![set("origin", "https://attacker.example")],
            StatusCode::FORBIDDEN,
            "foreign_origin",
            origin,
        ),
        (
            "a page on the same site, from another port",
            vec![set(
                "origin",
                format!("http://127.0.0.1:{}", port.wrapping_add(1)),
            )],
            StatusCode::FORBIDDEN,
            "foreign_origin",
            origin,
        ),
        (
            "a page under another of the server's names",
            vec![set("origin", format!("http://localhost:{port}"))],
            StatusCode::FORBIDDEN,
            "foreign_origin",
            origin,
        ),
        (
            "a sandboxed frame or a file",
            vec![set("origin", "null")],
            StatusCode::FORBIDDEN,
            "foreign_origin",
            origin,
        ),
        (
            "the same site from another port, as Fetch Metadata tells it",
            vec![unset("origin"), set("sec-fetch-site", "same-site")],
            StatusCode::FORBIDDEN,
            "same_origin_required",
            fetch,
        ),
        (
            "another site, as Fetch Metadata tells it",
            vec![unset("origin"), set("sec-fetch-site", "cross-site")],
            StatusCode::FORBIDDEN,
            "same_origin_required",
            fetch,
        ),
        (
            "no token",
            vec![unset("cookie")],
            StatusCode::UNAUTHORIZED,
            "token_required",
            "launch token",
        ),
        (
            "a text body, which a page may send anywhere unasked",
            vec![set("content-type", "text/plain")],
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_required",
            json,
        ),
        (
            "a form",
            vec![set("content-type", "application/x-www-form-urlencoded")],
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_required",
            json,
        ),
        (
            "no content type",
            vec![unset("content-type")],
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "json_required",
            json,
        ),
        (
            "no X-Mindweaver",
            vec![unset("x-mindweaver")],
            StatusCode::FORBIDDEN,
            "x_mindflayer_required",
            custom,
        ),
        (
            "X-Mindweaver other than 1",
            vec![set("x-mindweaver", "yes")],
            StatusCode::FORBIDDEN,
            "x_mindflayer_required",
            custom,
        ),
    ];
    for (case, changes, status, code, refusal) in cases {
        let reply = deliver(&server, from_the_panel(&server, &changes)).await;
        assert_eq!(reply.status, status, "{case}");
        assert_eq!(reply.json()["code"], code, "{case}");
        let error = reply.json()["error"].clone();
        assert!(
            error.as_str().is_some_and(|error| error.contains(refusal)),
            "{case}: {error}"
        );
    }
    let runs = runs.lock().unwrap();
    assert!(runs.is_empty(), "{runs:?}");
}

#[tokio::test]
async fn a_request_the_panel_would_send_gets_through() {
    let (server, runs) = launch();
    let port = server.address().port();

    let cases: Vec<(&str, Vec<Change>)> = vec![
        ("as the launch link names it", vec![]),
        (
            "with its own origin",
            vec![set("origin", format!("http://127.0.0.1:{port}"))],
        ),
        (
            "as localhost, from a page loaded as localhost",
            vec![
                set("host", format!("localhost:{port}")),
                set("origin", format!("http://localhost:{port}")),
            ],
        ),
        (
            "as [::1], from a page loaded as [::1]",
            vec![
                set("host", format!("[::1]:{port}")),
                set("origin", format!("http://[::1]:{port}")),
            ],
        ),
        (
            "JSON with its charset",
            vec![set("content-type", "application/json; charset=utf-8")],
        ),
        (
            "forwarded from anywhere, which is not looked at",
            vec![set("x-forwarded-host", "attacker.example")],
        ),
    ];
    for (case, changes) in &cases {
        let reply = deliver(&server, from_the_panel(&server, changes)).await;
        assert_eq!(reply.status, StatusCode::OK, "{case}: {}", reply.text());
    }
    assert_eq!(runs.lock().unwrap().len(), 2 * cases.len());
}

#[tokio::test]
async fn a_page_on_a_rebound_name_gets_neither_the_panel_nor_a_cookie() {
    let (server, _) = launch();
    let rebound = format!("attacker.example:{}", server.address().port());

    for path in ["/".to_owned(), format!("/?token={}", server.token())] {
        let mut request = request(&server, Method::GET, &path, Auth::Nothing, None);
        request
            .headers_mut()
            .insert("host", rebound.parse().unwrap());
        let reply = deliver(&server, request).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{path}");
        assert_eq!(
            reply.header("content-type"),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(reply.header("set-cookie"), None);
        assert!(reply.text().contains("answers only as 127.0.0.1:"));
        assert!(!reply.text().contains("unknown_host"), "{}", reply.text());
    }
}

#[tokio::test]
async fn a_change_by_cookie_without_fetch_metadata_is_refused_and_taken_by_bearer() {
    let (server, runs) = launch();

    let reply = deliver(&server, from_the_panel(&server, &[unset("sec-fetch-site")])).await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(reply.json()["code"], "same_origin_required");
    assert!(reply.json()["error"]
        .as_str()
        .unwrap()
        .contains("Sec-Fetch-Site"));
    assert!(runs.lock().unwrap().is_empty());

    let bearer = set("authorization", format!("Bearer {}", server.token()));
    let reply = deliver(
        &server,
        from_the_panel(&server, &[unset("sec-fetch-site"), bearer]),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(*runs.lock().unwrap(), ["start change", "end change"]);
}

#[tokio::test]
async fn a_read_by_cookie_needs_no_fetch_metadata() {
    let (server, _) = launch();
    let cookie = format!("{}={}", server.cookie(), server.token());
    let mut request = request(
        &server,
        Method::GET,
        "/api/v1/transcript",
        Auth::Cookie(&cookie),
        None,
    );
    request.headers_mut().remove("sec-fetch-site");

    let reply = deliver(&server, request).await;

    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn every_answer_carries_the_content_security_policy_and_nosniff() {
    let (server, _) = launch();
    let token = server.token();
    let mut rebound = request(&server, Method::GET, "/", Auth::Nothing, None);
    rebound.headers_mut().insert(
        "host",
        format!("attacker.example:{}", server.address().port())
            .parse()
            .unwrap(),
    );

    let replies = [
        ("the panel", get(&server, "/", Auth::Nothing).await),
        (
            "the launch link",
            get(&server, &format!("/?token={token}"), Auth::Nothing).await,
        ),
        (
            "the API",
            get(&server, "/api/v1/transcript", Auth::Bearer(token)).await,
        ),
        (
            "the gate's refusal",
            get(&server, "/api/v1/transcript", Auth::Nothing).await,
        ),
        ("the guard's refusal", deliver(&server, rebound).await),
    ];
    for (case, reply) in replies {
        assert_eq!(
            reply.header("content-security-policy"),
            Some("default-src 'self'; frame-ancestors 'none'"),
            "{case}"
        );
        assert_eq!(
            reply.header("x-content-type-options"),
            Some("nosniff"),
            "{case}"
        );
    }
}

/// A read is a GET, whose answer a browser may otherwise keep in the
/// profile's cache on disk, a file's whole text among them: every answer
/// under `/api` says not to, whoever turned it away, while the panel's page
/// keeps its own.
#[tokio::test]
async fn no_answer_under_the_api_is_stored_and_the_panel_keeps_its_own() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());
    let mut rebound = request(&server, Method::GET, "/api/v1/echo?echo=hi", token, None);
    rebound.headers_mut().insert(
        "host",
        format!("attacker.example:{}", server.address().port())
            .parse()
            .unwrap(),
    );

    let replies = [
        (
            "a read",
            get(&server, "/api/v1/echo?echo=hi", token).await,
            StatusCode::OK,
        ),
        (
            "a read with a body",
            post(&server, "/api/v1/judge", token, json!({ "echo": "hi" })).await,
            StatusCode::OK,
        ),
        (
            "the transcript",
            get(&server, "/api/v1/transcript", token).await,
            StatusCode::OK,
        ),
        (
            "a refusal of a parameter",
            get(&server, "/api/v1/echo?echo=hi&what=1", token).await,
            StatusCode::BAD_REQUEST,
        ),
        (
            "the backend's refusal",
            get(&server, "/api/v1/echo?echo=x&refuse=true", token).await,
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        (
            "a path no route names",
            get(&server, "/api/commands", token).await,
            StatusCode::NOT_FOUND,
        ),
        (
            "a method a route does not take",
            send(&server, Method::DELETE, "/api/v1/transcript", token, None).await,
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "the gate's refusal",
            get(&server, "/api/v1/echo?echo=hi", Auth::Nothing).await,
            StatusCode::UNAUTHORIZED,
        ),
        (
            "the guard's refusal",
            deliver(&server, rebound).await,
            StatusCode::FORBIDDEN,
        ),
    ];
    for (case, reply, status) in replies {
        assert_eq!(reply.status, status, "{case}: {}", reply.text());
        assert_eq!(reply.header("cache-control"), Some("no-store"), "{case}");
    }

    // Built into this binary or not, the panel's page is asked for again on
    // every load, and its files are the same for everybody.
    let page = get(&server, "/", Auth::Nothing).await;
    assert_eq!(page.header("cache-control"), Some("no-cache"));
}

#[tokio::test]
async fn no_answer_grants_another_origin_anything() {
    let (server, _) = launch();
    let port = server.address().port();

    for origin in [
        format!("http://127.0.0.1:{}", port.wrapping_add(1)),
        format!("http://127.0.0.1:{port}"),
    ] {
        // A preflight: what a browser asks before it lets a page send a
        // header of its own, or JSON, to another origin.
        let mut preflight = request(
            &server,
            Method::OPTIONS,
            "/api/v1/tasks",
            Auth::Nothing,
            None,
        );
        let headers = preflight.headers_mut();
        headers.insert("origin", origin.parse().unwrap());
        headers.insert("access-control-request-method", "POST".parse().unwrap());
        headers.insert(
            "access-control-request-headers",
            "content-type, x-mindweaver".parse().unwrap(),
        );

        let reply = deliver(&server, preflight).await;

        assert!(!reply.status.is_success(), "{origin}: {}", reply.status);
        let granted: Vec<_> = reply
            .headers
            .keys()
            .filter(|name| name.as_str().starts_with("access-control-"))
            .collect();
        assert!(granted.is_empty(), "{origin}: {granted:?}");
    }
}

/// Where `make web/dev` serves the panel from while it is worked on.
const DEV_SERVER: &str = "http://127.0.0.1:5173";

/// What the dev server's proxy forwards: the panel's change, with the
/// browser's Origin, the dev server's, and Host naming this server, as
/// `changeOrigin` makes it.
#[tokio::test]
async fn through_its_dev_server_the_panel_changes_things_and_nothing_else_does() {
    let server = Server::new(Fake::default())
        .dev_server(DEV_SERVER)
        .bind()
        .unwrap();
    let port = server.address().port();

    let proxied = from_the_panel(&server, &[set("origin", DEV_SERVER)]);
    let reply = deliver(&server, proxied).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());

    // Its own pages still: the panel built into the binary.
    let own = from_the_panel(
        &server,
        &[set("origin", format!("http://127.0.0.1:{port}"))],
    );
    assert_eq!(deliver(&server, own).await.status, StatusCode::OK);

    for (case, changes, code) in [
        (
            "another port",
            vec![set("origin", "http://127.0.0.1:5174")],
            "foreign_origin",
        ),
        (
            "another name",
            vec![set("origin", "http://localhost:5173")],
            "foreign_origin",
        ),
        (
            "naming the dev server in Host",
            vec![set("host", "127.0.0.1:5173"), set("origin", DEV_SERVER)],
            "unknown_host",
        ),
    ] {
        let reply = deliver(&server, from_the_panel(&server, &changes)).await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{case}");
        assert_eq!(reply.json()["code"], code, "{case}");
    }

    assert_eq!(
        server.url(),
        format!("{DEV_SERVER}/?token={}", server.token())
    );
}

/// Only when the front end names it: otherwise the dev server is one more
/// page on another port of loopback.
#[tokio::test]
async fn without_being_told_of_it_the_server_refuses_the_dev_servers_pages() {
    let (server, runs) = launch();

    let reply = deliver(
        &server,
        from_the_panel(&server, &[set("origin", DEV_SERVER)]),
    )
    .await;

    assert_eq!(reply.status, StatusCode::FORBIDDEN);
    assert_eq!(
        reply.json(),
        json!({
            "error": format!(
                "this server takes requests only from its own pages, at http://{}",
                server.address()
            ),
            "code": "foreign_origin",
        })
    );
    assert!(runs.lock().unwrap().is_empty());
}

/// Every route under the version as a script or the panel's page asks for
/// it, with what it is sent: one for each of the fake's reads, and each of
/// the server's own.
fn versioned() -> [(Method, &'static str, Option<Value>); 10] {
    [
        (Method::GET, "/api/v1/echo?echo=hi", None),
        (Method::GET, "/api/v1/words?word=a", None),
        (Method::POST, "/api/v1/judge", Some(json!({ "echo": "hi" }))),
        (
            Method::POST,
            "/api/v1/tasks/preview",
            Some(json!({ "name": "a" })),
        ),
        (Method::POST, "/api/v1/tasks", Some(json!({ "name": "a" }))),
        (
            Method::POST,
            "/api/v1/tasks/cancel",
            Some(json!({ "run": 1 })),
        ),
        (
            Method::POST,
            "/api/v1/switch",
            Some(json!({ "to": "beta" })),
        ),
        (Method::GET, "/api/v1/transcript", None),
        (Method::GET, "/api/v1/events", None),
        (Method::GET, "/api/v1/openapi.json", None),
    ]
}

/// The guards are the server's and know no route, so every one under the
/// version, the backend's reads and the server's own alike, holds each of
/// them, by Bearer and by cookie: a request with no token, or another
/// server's, is a 401; one naming another host or sent from another page a
/// 403, whichever way it holds the token; and a change by cookie that the
/// panel's page would not send is refused as one to `/tasks` is, each with
/// the code of the guard that turned it away. Every refusal, and every
/// answer, carries the policy, `nosniff` and `no-store`;
/// and either credential, sent as a script or the page sends it, is
/// answered: a cancel, with nothing under way, by the route itself, past
/// every guard.
#[tokio::test]
async fn every_versioned_route_holds_every_guard_by_bearer_and_cookie_alike() {
    let (server, _) = launch();
    let port = server.address().port();
    let cookie = format!("{}={}", server.cookie(), server.token());
    let paths = fake_routes()
        .into_iter()
        .map(|route| route.path)
        .chain(Own::ALL.iter().map(|own| own.path()));
    for path in paths {
        let served = format!("{API}{path}");
        assert!(
            versioned()
                .iter()
                .any(|(_, path, _)| path.split('?').next().is_some_and(|path| path == served)),
            "{served} is never asked for here"
        );
    }
    let carries_the_policy = |reply_headers: &HeaderMap, case: &str| {
        assert_eq!(
            reply_headers
                .get("content-security-policy")
                .map(|value| value.to_str().unwrap()),
            Some("default-src 'self'; frame-ancestors 'none'"),
            "{case}"
        );
        assert_eq!(
            reply_headers
                .get("x-content-type-options")
                .map(|value| value.to_str().unwrap()),
            Some("nosniff"),
            "{case}"
        );
        assert_eq!(
            reply_headers
                .get("cache-control")
                .map(|value| value.to_str().unwrap()),
            Some("no-store"),
            "{case}"
        );
    };

    for (method, path, body) in versioned() {
        let asked = |auth| request(&server, method.clone(), path, auth, body.clone());
        let mut refused = vec![
            (
                "no token",
                asked(Auth::Nothing),
                StatusCode::UNAUTHORIZED,
                "token_required",
            ),
            (
                "another server's token",
                asked(Auth::Bearer("0123456789abcdef")),
                StatusCode::UNAUTHORIZED,
                "token_required",
            ),
        ];
        for (credential, auth) in [
            ("by Bearer", Auth::Bearer(server.token())),
            ("by cookie", Auth::Cookie(&cookie)),
        ] {
            let rebound = set("host", format!("attacker.example:{port}"));
            let elsewhere = set(
                "origin",
                format!("http://127.0.0.1:{}", port.wrapping_add(1)),
            );
            refused.extend([
                (
                    credential,
                    changed(asked(auth), &[rebound]),
                    StatusCode::FORBIDDEN,
                    "unknown_host",
                ),
                (
                    credential,
                    changed(asked(auth), &[elsewhere]),
                    StatusCode::FORBIDDEN,
                    "foreign_origin",
                ),
            ]);
        }
        if method != Method::GET {
            let by_cookie = || asked(Auth::Cookie(&cookie));
            refused.extend([
                (
                    "by cookie, from another port as Fetch Metadata tells it",
                    changed(by_cookie(), &[set("sec-fetch-site", "same-site")]),
                    StatusCode::FORBIDDEN,
                    "same_origin_required",
                ),
                (
                    "by cookie, without X-Mindweaver",
                    changed(by_cookie(), &[unset("x-mindweaver")]),
                    StatusCode::FORBIDDEN,
                    "x_mindflayer_required",
                ),
                (
                    "by cookie, as text",
                    changed(by_cookie(), &[set("content-type", "text/plain")]),
                    StatusCode::UNSUPPORTED_MEDIA_TYPE,
                    "json_required",
                ),
            ]);
        }
        for (case, request, status, code) in refused {
            let case = format!("{method} {path}, {case}");
            let reply = deliver(&server, request).await;
            assert_eq!(reply.status, status, "{case}: {}", reply.text());
            assert_eq!(reply.json()["code"], code, "{case}");
            assert!(reply.json()["error"].is_string(), "{case}");
            carries_the_policy(&reply.headers, &case);
        }

        for (credential, auth) in [
            ("by Bearer", Auth::Bearer(server.token())),
            ("by cookie", Auth::Cookie(&cookie)),
        ] {
            let case = format!("{method} {path}, {credential}");
            // A stream that stays open, read only as far as its answer.
            let (status, headers) = if path == "/api/v1/events" {
                let stream = subscribe(&server, auth).await;
                (stream.status, stream.headers)
            } else {
                let reply = send(&server, method.clone(), path, auth, body.clone()).await;
                if path == "/api/v1/tasks/cancel" {
                    assert_eq!(reply.json()["code"], "not_running", "{case}");
                    (StatusCode::OK, reply.headers)
                } else {
                    (reply.status, reply.headers)
                }
            };
            assert_eq!(status, StatusCode::OK, "{case}");
            carries_the_policy(&headers, &case);
        }
    }
}
