//! The backend's reads, served under `/api/v1` from the table it hands the
//! server: a path and its parameters reach the backend as the query the
//! table makes of them, and what no route takes is refused, with its
//! status, before the backend is asked anything.

mod common;

use common::*;
use std::io::ErrorKind;

/// `GET path` with the token, as a script sends it.
async fn read(server: &Listening, path: &str) -> Reply {
    get(server, path, Auth::Bearer(server.token())).await
}

#[tokio::test]
async fn a_path_and_its_parameters_reach_the_backend_as_the_query_the_table_makes() {
    let (server, runs) = launch();

    for (path, answer) in [
        (
            "/api/v1/echo?echo=what+is%20linked",
            json!({ "echo": "what is linked" }),
        ),
        (
            "/api/v1/echo?refuse=false&echo=a%2Fb",
            json!({ "echo": "a/b" }),
        ),
        (
            "/api/v1/words?word=a&times=2&word=b",
            json!({ "echo": "words", "words": ["a", "b"], "times": 2 }),
        ),
        ("/api/v1/words", json!({ "echo": "words" })),
    ] {
        let reply = read(&server, path).await;
        assert_eq!(reply.status, StatusCode::OK, "{path}: {}", reply.text());
        assert_eq!(reply.header("content-type"), Some("application/json"));
        assert_eq!(reply.json(), answer, "{path}");
    }

    let reply = post(
        &server,
        "/api/v1/judge",
        Auth::Bearer(server.token()),
        json!({ "echo": "a draft", "words": ["x"] }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.json(), json!({ "echo": "a draft", "words": ["x"] }));
    assert!(runs.lock().unwrap().is_empty(), "a read runs nothing");
}

/// Each refused with a 400, as a query string is, and what it says names
/// the route and the parameter, its code the condition: the backend is
/// never asked.
#[tokio::test]
async fn a_parameter_no_route_takes_or_needs_or_reads_is_refused_with_a_400() {
    let (server, _) = launch();

    for (path, code, said) in [
        (
            "/api/v1/echo?echo=hi&token=secret",
            "unknown_parameter",
            "/api/v1/echo takes no parameter `token`, only `echo`, `refuse` or `absent`",
        ),
        (
            "/api/v1/echo?echo=hi&Echo=hi",
            "unknown_parameter",
            "/api/v1/echo takes no parameter `Echo`, only `echo`, `refuse` or `absent`",
        ),
        (
            "/api/v1/echo",
            "missing_parameter",
            "/api/v1/echo needs `echo`, and was sent none",
        ),
        (
            "/api/v1/echo?refuse=true",
            "missing_parameter",
            "/api/v1/echo needs `echo`, and was sent none",
        ),
        (
            "/api/v1/echo?echo=a&echo=b",
            "repeated_parameter",
            "/api/v1/echo takes one `echo`, and was sent more than one",
        ),
        (
            "/api/v1/echo?echo=a&refuse=yes",
            "malformed_parameter",
            "/api/v1/echo takes `refuse` as `true` or `false`, not `yes`",
        ),
        (
            "/api/v1/words?times=-2",
            "malformed_parameter",
            "/api/v1/words takes `times` as a whole number, not `-2`",
        ),
        (
            "/api/v1/echo?echo=",
            "invalid_parameters",
            "/api/v1/echo: there is nothing to echo",
        ),
    ] {
        let reply = read(&server, path).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{path}");
        assert_eq!(
            reply.json(),
            json!({ "error": said, "code": code }),
            "{path}"
        );
    }
}

/// A body is read as a task's is: refused with a 422 when the route cannot
/// read it, as `deny_unknown_fields` refuses a field, and with axum's own
/// status when it is no JSON at all. A POST takes no parameter: what it
/// reads is in its body.
#[tokio::test]
async fn a_body_a_post_route_cannot_read_is_refused_and_so_is_a_parameter() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());

    for body in [json!({ "echo": "hi", "wrong": 1 }), json!({}), json!([1])] {
        let reply = post(&server, "/api/v1/judge", token, body.clone()).await;
        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert_eq!(reply.json()["code"], "invalid_body", "{body}");
        let error = reply.json()["error"].as_str().unwrap().to_owned();
        assert!(error.starts_with("/api/v1/judge: "), "{body}: {error}");
    }
    let reply = post(&server, "/api/v1/judge", token, json!({ "echo": "" })).await;
    assert_eq!(
        reply.json(),
        json!({
            "error": "/api/v1/judge: there is nothing to echo",
            "code": "invalid_body",
        })
    );

    let reply = send(&server, Method::POST, "/api/v1/judge", token, None).await;
    assert_eq!(reply.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(reply.json()["code"], "json_required");
    assert!(reply.json()["error"].is_string());

    let reply = post(
        &server,
        "/api/v1/judge?echo=hi",
        token,
        json!({ "echo": "hi" }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        reply.json(),
        json!({
            "error": "/api/v1/judge takes no parameter, and was sent `echo`",
            "code": "unknown_parameter",
        })
    );
}

/// A backend with a bug, which panics on the blocking pool while it
/// answers: a 500 with the server's code, and the next read answered.
#[tokio::test]
async fn a_read_the_backend_fails_at_is_a_500_with_its_code() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());

    let reply = post(
        &server,
        "/api/v1/judge",
        token,
        json!({ "echo": "x", "panic": true }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        reply.json(),
        json!({
            "error": "the backend failed while answering the query",
            "code": "backend_failed",
        })
    );
    assert_eq!(
        read(&server, "/api/v1/echo?echo=hi").await.status,
        StatusCode::OK
    );
}

/// A query the backend read and will not answer, as its front end's rules
/// say: a 422, the code beside the message for a script to act on.
#[tokio::test]
async fn a_read_the_backend_refuses_says_why_with_its_code() {
    let (server, _) = launch();

    let reply = read(&server, "/api/v1/echo?echo=%2Fetc%2Fpasswd&refuse=true").await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        reply.json(),
        json!({ "error": "/etc/passwd is refused", "code": "fake_refused" })
    );
}

/// A query naming what the backend says is not there: a 404, because the
/// backend said so, with its own code; the server reads no code to tell.
#[tokio::test]
async fn a_read_naming_what_is_not_there_is_a_404_with_the_backends_code() {
    let (server, _) = launch();

    let reply = read(&server, "/api/v1/echo?echo=ghost&absent=true").await;

    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    assert_eq!(
        reply.json(),
        json!({ "error": "ghost is not there", "code": "fake_absent" })
    );
}

#[tokio::test]
async fn a_route_asked_with_a_method_it_does_not_take_says_so_in_json() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());

    for (method, path, allowed) in [
        (Method::POST, "/api/v1/echo", "GET,HEAD"),
        (Method::DELETE, "/api/v1/words", "GET,HEAD"),
        (Method::GET, "/api/v1/judge", "POST"),
        (Method::PUT, "/api/v1/judge", "POST"),
    ] {
        let body = (method != Method::GET).then(|| json!({ "echo": "hi" }));
        let reply = send(&server, method.clone(), path, token, body).await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path}"
        );
        assert_eq!(reply.header("allow"), Some(allowed), "{method} {path}");
        assert_eq!(
            reply.json(),
            json!({
                "error": format!("{path} does not take {method}"),
                "code": "method_not_allowed",
            })
        );
    }
}

/// A path under the version that no route names, or one of a route's with
/// more after it, is a JSON 404 like any other under `/api`.
#[tokio::test]
async fn a_path_no_route_names_is_a_json_404() {
    let (server, _) = launch();

    for path in [
        "/api/v1",
        "/api/v1/",
        "/api/v1/echo/",
        "/api/v1/echo/deeper",
        "/api/v1/Echo",
        "/api/v2/echo",
        "/api/echo",
    ] {
        let reply = read(&server, path).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(
            reply.json(),
            json!({
                "error": format!("no such endpoint: GET {path}"),
                "code": "unknown_route",
            })
        );
    }
}

/// The server's own routes take no parameter, and say so as a backend's
/// route says it of one it does not take: nothing is run, switched or
/// streamed for a request a script misspelled.
#[tokio::test]
async fn the_servers_own_routes_take_no_parameter() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());

    for (method, path, sent, body) in [
        (
            Method::POST,
            "/api/v1/tasks/preview",
            "yes",
            Some(json!({ "name": "a" })),
        ),
        (
            Method::POST,
            "/api/v1/tasks",
            "yes",
            Some(json!({ "name": "a" })),
        ),
        (
            Method::POST,
            "/api/v1/switch",
            "to",
            Some(json!({ "to": "beta" })),
        ),
        (Method::GET, "/api/v1/transcript", "view", None),
        (Method::GET, "/api/v1/events", "since", None),
        (Method::GET, "/api/v1/openapi.json", "format", None),
    ] {
        let asked = format!("{path}?{sent}=1");
        let reply = send(&server, method.clone(), &asked, token, body).await;
        assert_eq!(reply.status, StatusCode::BAD_REQUEST, "{asked}");
        assert_eq!(
            reply.json(),
            json!({
                "error": format!("{path} takes no parameter, and was sent `{sent}`"),
                "code": "unknown_parameter",
            })
        );
    }
    assert!(runs.lock().unwrap().is_empty(), "nothing ran nor switched");
}

/// The server's own routes are served from their table (`Own::ALL`), at
/// its path and by its method alone, so a description written from it says
/// how each is asked for: any other method is a 405 naming that one.
#[tokio::test]
async fn the_servers_own_routes_are_served_as_their_table_says() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());

    for own in Own::ALL {
        let path = format!("{API}{}", own.path());
        let (other, allowed) = match own.method() {
            mindweaver_server::Method::Get => (Method::POST, "GET,HEAD"),
            mindweaver_server::Method::Post => (Method::GET, "POST"),
        };
        let body = (other == Method::POST).then(|| json!({ "name": "a" }));
        let reply = send(&server, other.clone(), &path, token, body).await;
        assert_eq!(
            reply.status,
            StatusCode::METHOD_NOT_ALLOWED,
            "{other} {path}"
        );
        assert_eq!(reply.header("allow"), Some(allowed), "{other} {path}");
        assert_eq!(reply.json()["code"], "method_not_allowed", "{other} {path}");
    }
    assert!(runs.lock().unwrap().is_empty(), "nothing ran nor switched");
}

/// The description of the API is the backend's, served as the bytes it
/// hands over, as JSON, to whoever holds the token and to nobody else, as
/// every route is.
#[tokio::test]
async fn the_document_the_backend_hands_over_is_served_as_it_is_behind_the_token() {
    let (server, _) = launch();
    let path = format!("{API}/openapi.json");

    let reply = get(&server, &path, Auth::Bearer(server.token())).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.header("content-type"), Some("application/json"));
    assert_eq!(reply.text(), FAKE_DOCUMENT);
    let cookie = format!("{}={}", server.cookie(), server.token());
    let reply = get(&server, &path, Auth::Cookie(&cookie)).await;
    assert_eq!(reply.text(), FAKE_DOCUMENT);

    let refused = get(&server, &path, Auth::Nothing).await;
    assert_eq!(refused.status, StatusCode::UNAUTHORIZED);
    assert_eq!(refused.json()["code"], "token_required");
    let other = get(&server, &path, Auth::Bearer("not-the-token")).await;
    assert_eq!(other.status, StatusCode::UNAUTHORIZED);
}

/// A backend whose table cannot be served: one path twice.
struct Crooked;

impl Backend for Crooked {
    type Query = FakeQuery;
    type Task = FakeTask;
    type Outcome = FakeOutcome;
    type Switch = FakeSwitch;

    fn query(&self, _: FakeQuery) -> Result<Value, Refusal> {
        unreachable!("never served")
    }

    fn routes() -> Vec<Route<FakeQuery>> {
        let mut routes = fake_routes();
        routes.extend(fake_routes().into_iter().take(1));
        routes
    }

    fn document() -> &'static str {
        unreachable!("never served")
    }

    fn preview(&self, _: &FakeTask) -> Result<Preview, Refusal> {
        unreachable!("never served")
    }

    fn run(&self, _: FakeTask, _: &Underway) -> Result<FakeOutcome, Refusal> {
        unreachable!("never served")
    }

    fn switch(&self, _: FakeSwitch) -> Result<Value, Refusal> {
        unreachable!("never served")
    }

    fn watched_paths(&self) -> Vec<Watched> {
        Vec::new()
    }
}

/// Rather than the router panicking on it, or serving one of the two.
#[test]
fn a_table_that_cannot_be_served_fails_the_bind_with_why() {
    let error = Server::new(Crooked).bind().err().expect("refused");

    assert_eq!(error.kind(), ErrorKind::InvalidInput);
    assert_eq!(error.to_string(), "the route `/echo` is in the table twice");
}
