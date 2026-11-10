//! The JSON endpoints: what reaches the backend, what comes back, and what a
//! request the API does not know gets. The backend's reads, served from its
//! table, have a suite of their own (`routes_test`).

mod common;

use common::*;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

#[tokio::test]
async fn a_preview_shows_a_tasks_lines_and_runs_nothing() {
    let (server, runs) = launch();

    let reply = post(
        &server,
        "/api/v1/tasks/preview",
        Auth::Bearer(server.token()),
        json!({ "name": "link beta" }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json(),
        json!({ "lines": ["fake run link beta"], "confirm": [], "cancellable": false })
    );
    assert!(runs.lock().unwrap().is_empty());
}

/// What the backend says to confirm goes to the panel as the backend wrote
/// it, beside the lines it is about: the server reads none of it.
#[tokio::test]
async fn a_preview_carries_what_the_backend_says_to_confirm_as_it_says_it() {
    let (server, runs) = launch();

    let reply = post(
        &server,
        "/api/v1/tasks/preview",
        Auth::Bearer(server.token()),
        json!({ "name": "unlink beta", "confirm": ["beta", "its copies"] }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        reply.json(),
        json!({
            "lines": ["fake run unlink beta"],
            "confirm": [{ "fake_takes": "beta" }, { "fake_takes": "its copies" }],
            "cancellable": false,
        })
    );
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_preview_the_backend_refuses_says_why_with_its_code() {
    let (server, runs) = launch();

    let reply = post(
        &server,
        "/api/v1/tasks/preview",
        Auth::Bearer(server.token()),
        json!({ "name": "edit", "refuse": true }),
    )
    .await;

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        reply.json(),
        json!({ "error": "edit is refused", "code": "fake_refused" })
    );
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unknown_api_path_gets_a_json_404() {
    let (server, _) = launch();

    for path in [
        "/api/no-such-endpoint",
        "/api/v1/tasks/deeper",
        "/api",
        "/api/",
    ] {
        let reply = get(&server, path, Auth::Bearer(server.token())).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert_eq!(reply.header("content-type"), Some("application/json"));
        assert_eq!(
            reply.json(),
            json!({
                "error": format!("no such endpoint: GET {path}"),
                "code": "unknown_route",
            })
        );
    }
}

/// What the panel's private protocol answered before the API was versioned
/// answers nothing now, by any method, with the token or without: the panel
/// was its only client, and ships in the same binary as the server.
#[tokio::test]
async fn no_unversioned_route_answers_any_more() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());

    for path in [
        "/api/query",
        "/api/commands",
        "/api/commands/preview",
        "/api/switch",
        "/api/transcript",
        "/api/events",
        "/api/snapshot",
        "/api/echo",
        "/api/v0/echo",
        "/api/v2/tasks",
        "/api/V1/tasks",
    ] {
        for method in [Method::GET, Method::POST] {
            let body = (method == Method::POST).then(|| json!({ "name": "a", "echo": "a" }));
            let reply = send(&server, method.clone(), path, token, body).await;
            assert_eq!(reply.status, StatusCode::NOT_FOUND, "{method} {path}");
            assert_eq!(
                reply.json(),
                json!({
                    "error": format!("no such endpoint: {method} {path}"),
                    "code": "unknown_route",
                })
            );
        }
        let reply = get(&server, path, Auth::Nothing).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{path}");
    }
    assert!(runs.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_endpoint_asked_with_the_wrong_method_says_so_in_json() {
    let (server, _) = launch();

    let reply = get(&server, "/api/v1/tasks", Auth::Bearer(server.token())).await;

    assert_eq!(reply.status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(reply.header("allow"), Some("POST"));
    assert_eq!(
        reply.json(),
        json!({
            "error": "/api/v1/tasks does not take GET",
            "code": "method_not_allowed",
        })
    );
}

/// What a body is made of, before any route reads it: JSON at all, of a
/// size the server reads, and there whole. Each refused with its own code,
/// and the words axum has for it, and nothing runs.
#[tokio::test]
async fn a_body_that_is_no_json_too_large_or_cut_off_is_refused_with_its_code() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());
    let sent = |body: Body| {
        let mut request = request(
            &server,
            Method::POST,
            "/api/v1/tasks",
            token,
            Some(json!({})),
        );
        *request.body_mut() = body;
        request
    };
    let cut_off = Body::from_stream(futures_util::stream::iter([
        Ok(Bytes::from_static(b"{\"name\": ")),
        Err(std::io::Error::other("the connection was lost")),
    ]));

    for (case, body, status, code) in [
        (
            "no JSON",
            Body::from("{\"name\": link beta}"),
            StatusCode::BAD_REQUEST,
            "malformed_json",
        ),
        (
            "too large",
            Body::from(format!("{{\"name\": \"{}\"}}", "a".repeat(3 << 20))),
            StatusCode::PAYLOAD_TOO_LARGE,
            "body_too_large",
        ),
        (
            "cut off",
            cut_off,
            StatusCode::BAD_REQUEST,
            "body_unreadable",
        ),
    ] {
        let reply = deliver(&server, sent(body)).await;
        assert_eq!(reply.status, status, "{case}: {}", reply.text());
        assert_eq!(reply.json()["code"], code, "{case}");
        assert!(reply.json()["error"].is_string(), "{case}");
    }
    assert!(runs.lock().unwrap().is_empty());
}

/// Whatever turned a request away under `/api`, a guard, the gate, a route,
/// what a body holds, a backend's rules or a backend that failed, the answer
/// is `{"error": ..., "code": ...}` and nothing else: the words for a person
/// and the word for a script, the code one the server gives of its own or
/// the backend's, and one of the server's a route lists among what it may
/// be refused for (`Route::conditions`, `Own::conditions`), which a
/// description of the API says. Every condition the server has is met here
/// but two no request can bring about, a way to fail axum has yet to add
/// and a writer thread that has stopped, so one added later is met here
/// too, or said.
#[tokio::test]
async fn every_answer_under_the_api_that_is_no_success_carries_an_error_and_a_code() {
    let fake = Fake::default();
    let hold = Arc::clone(&fake.hold);
    let server = Server::new(fake).bind().unwrap();
    let port = server.address().port();
    let token = Auth::Bearer(server.token());
    let cookie = format!("{}={}", server.cookie(), server.token());
    let by_cookie = || {
        request(
            &server,
            Method::POST,
            "/api/v1/tasks",
            Auth::Cookie(&cookie),
            Some(json!({ "name": "a" })),
        )
    };
    let with = |mut request: Request<Body>, name: &'static str, value: String| {
        request.headers_mut().insert(name, value.parse().unwrap());
        request
    };
    let without = |mut request: Request<Body>, name: &'static str| {
        request.headers_mut().remove(name);
        request
    };
    let carrying = |body: Body| {
        let mut request = request(
            &server,
            Method::POST,
            "/api/v1/tasks",
            token,
            Some(json!({})),
        );
        *request.body_mut() = body;
        request
    };
    let read = |path: &str| request(&server, Method::GET, path, token, None);
    let sent = |path: &str, body: Value| request(&server, Method::POST, path, token, Some(body));

    let asked: Vec<(&str, Request<Body>)> = vec![
        (
            "no token",
            request(
                &server,
                Method::GET,
                "/api/v1/echo?echo=x",
                Auth::Nothing,
                None,
            ),
        ),
        (
            "no token, the bare prefix",
            request(&server, Method::GET, "/api", Auth::Nothing, None),
        ),
        (
            "another host",
            with(
                read("/api/v1/echo?echo=x"),
                "host",
                format!("attacker.example:{port}"),
            ),
        ),
        (
            "another page",
            with(
                read("/api/v1/echo?echo=x"),
                "origin",
                format!("http://127.0.0.1:{}", port.wrapping_add(1)),
            ),
        ),
        (
            "a change by cookie from another port",
            with(by_cookie(), "sec-fetch-site", String::from("same-site")),
        ),
        (
            "a change by cookie without the header",
            without(by_cookie(), "x-mindweaver"),
        ),
        (
            "a change by cookie as text",
            with(by_cookie(), "content-type", String::from("text/plain")),
        ),
        (
            "a body with no type",
            without(
                sent("/api/v1/tasks", json!({ "name": "a" })),
                "content-type",
            ),
        ),
        ("a body that is no JSON", carrying(Body::from("{nope"))),
        (
            "a body the task cannot be read from",
            sent("/api/v1/tasks", json!({ "nombre": "a" })),
        ),
        (
            "a body too large",
            carrying(Body::from(format!(
                "{{\"name\": \"{}\"}}",
                "a".repeat(3 << 20)
            ))),
        ),
        (
            "a body cut off",
            carrying(Body::from_stream(futures_util::stream::iter([
                Ok(Bytes::from_static(b"{")),
                Err(std::io::Error::other("the connection was lost")),
            ]))),
        ),
        (
            "a parameter no route takes",
            read("/api/v1/echo?echo=x&what=1"),
        ),
        (
            "a parameter given twice",
            read("/api/v1/echo?echo=a&echo=b"),
        ),
        (
            "a parameter it cannot read",
            read("/api/v1/words?times=many"),
        ),
        ("a parameter left out", read("/api/v1/echo")),
        (
            "parameters the route cannot read",
            read("/api/v1/echo?echo="),
        ),
        (
            "a parameter to a route of the server's own",
            read("/api/v1/transcript?view=all"),
        ),
        ("a path no route names", read("/api/v1/nothing")),
        ("the bare prefix", read("/api/")),
        (
            "a method a route does not take",
            request(&server, Method::DELETE, "/api/v1/transcript", token, None),
        ),
        (
            "a read the backend refuses",
            read("/api/v1/echo?echo=x&refuse=true"),
        ),
        (
            "a read naming what is not there",
            read("/api/v1/echo?echo=x&absent=true"),
        ),
        (
            "a read the backend fails at",
            sent("/api/v1/judge", json!({ "echo": "x", "panic": true })),
        ),
        (
            "a preview the backend refuses",
            sent(
                "/api/v1/tasks/preview",
                json!({ "name": "a", "refuse": true }),
            ),
        ),
        (
            "a run the backend refuses",
            sent("/api/v1/tasks", json!({ "name": "a", "refuse_run": true })),
        ),
        (
            "a run the backend fails at",
            sent("/api/v1/tasks", json!({ "name": "a", "panic": true })),
        ),
        (
            "a switch the backend refuses",
            sent("/api/v1/switch", json!({ "to": "a", "refuse": true })),
        ),
        (
            "a cancel of a run over",
            sent("/api/v1/tasks/cancel", json!({ "run": 1 })),
        ),
        (
            "a cancel naming no run",
            sent("/api/v1/tasks/cancel", json!({ "all": true })),
        ),
    ];

    // A cancel of the run under way, which takes none, needs a run held
    // under way: met first, and let go before anything above is sent,
    // since a run among them would wait behind it.
    let release = Release(Arc::clone(&hold));
    let held = {
        let router = server.router();
        let request = sent("/api/v1/tasks", json!({ "name": "held", "hold": true }));
        tokio::spawn(tower::ServiceExt::oneshot(router, request))
    };
    let reached = Arc::clone(&hold);
    tokio::task::spawn_blocking(move || reached.reached())
        .await
        .unwrap();
    let uncancellable = sent("/api/v1/tasks/cancel", json!({ "run": 1 }));
    let path = uncancellable.uri().path().to_owned();
    let reply = deliver(&server, uncancellable).await;
    drop(release);
    held.await.unwrap().unwrap();
    let listed = |path: &str| {
        let path = path.strip_prefix(API)?;
        fake_routes()
            .iter()
            .find(|route| route.path == path)
            .map(Route::conditions)
            .or_else(|| {
                Own::ALL
                    .iter()
                    .find(|own| own.path() == path)
                    .map(|own| own.conditions())
            })
    };
    assert_eq!(reply.status, StatusCode::CONFLICT, "{}", reply.text());
    assert_eq!(reply.json()["code"], "uncancellable");
    assert!(listed(&path).is_some_and(|listed| listed.contains(&Condition::Uncancellable)));
    let mut met: Vec<String> = vec![String::from("uncancellable")];

    for (case, request) in asked {
        let path = request.uri().path().to_owned();
        let reply = deliver(&server, request).await;
        assert!(!reply.status.is_success(), "{case}: {}", reply.status);
        assert_eq!(
            reply.header("content-type"),
            Some("application/json"),
            "{case}"
        );
        let said = reply.json();
        let fields: Vec<&String> = said
            .as_object()
            .map(|said| said.keys().collect())
            .unwrap_or_default();
        assert_eq!(fields, ["code", "error"], "{case}: {said}");
        assert!(
            said["error"]
                .as_str()
                .is_some_and(|error| !error.is_empty()),
            "{case}: {said}"
        );
        let code = said["code"].as_str().unwrap_or_default().to_owned();
        assert!(
            Condition::CODES.contains(&code.as_str()) || FAKE_CODES.contains(&code.as_str()),
            "{case}: {said}"
        );
        if let Some(condition) = Condition::ALL
            .iter()
            .find(|condition| condition.code() == code)
        {
            assert_eq!(reply.status, condition.status(), "{case}: {said}");
            if let Some(listed) = listed(&path) {
                assert!(
                    listed.contains(condition),
                    "{case}: {path} lists {listed:?}, and was refused for {condition:?}"
                );
            }
        }
        met.push(code);
    }

    let unmet: Vec<&str> = Condition::CODES
        .iter()
        .chain(&FAKE_CODES)
        .copied()
        .filter(|code| !met.iter().any(|met| met == code))
        .collect();
    assert_eq!(unmet, ["body_refused", "writer_stopped"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_server_answers_on_loopback_on_a_port_the_system_chose() {
    let (server, _) = launch();
    let address = server.address();
    let token = server.token().to_owned();
    assert_eq!(address.ip(), Ipv4Addr::LOCALHOST);
    assert_ne!(address.port(), 0);

    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve(async {
        let _ = stopped.await;
    }));
    let response = tokio::task::spawn_blocking(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        write!(
            stream,
            "GET /api/v1/echo?echo=over+the+wire HTTP/1.1\r\nHost: {address}\r\n\
             Authorization: Bearer {token}\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    })
    .await
    .unwrap();
    stop.send(()).unwrap();
    serving.await.unwrap().unwrap();

    assert!(response.starts_with("HTTP/1.1 200 OK\r\n"), "{response}");
    assert!(
        response.ends_with(r#"{"echo":"over the wire"}"#),
        "{response}"
    );
}

/// What the panel's dev server needs, whose proxy names the port in its
/// settings. The port was free a moment ago: another process could take it
/// in between, which would fail this test rather than pass it wrongly.
#[test]
fn the_server_listens_on_the_port_it_is_given() {
    let free = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();

    let server = Server::new(Fake::default()).port(free).bind().unwrap();

    assert_eq!(
        server.address(),
        SocketAddr::from((Ipv4Addr::LOCALHOST, free))
    );
    assert_eq!(
        server.url(),
        format!("http://127.0.0.1:{free}/?token={}", server.token())
    );
}

/// Another port would be one the client that asked for this one never
/// finds.
#[test]
fn a_port_somebody_holds_is_an_error_rather_than_another_port() {
    let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = held.local_addr().unwrap().port();

    let error = Server::new(Fake::default()).port(port).bind().err();

    assert_eq!(
        error.map(|error| error.kind()),
        Some(std::io::ErrorKind::AddrInUse)
    );
}
