//! The event stream at `/api/v1/events`: who may open it, what it says first,
//! what one that falls behind is told, and that it ends when the server
//! stops.

mod common;

use std::io::{Read, Write};
use std::sync::mpsc;

use common::*;
use mindweaver_server::{Event, CAPACITY};

#[tokio::test]
async fn the_stream_needs_the_launch_token() {
    let (server, _) = launch();

    let reply = get(&server, "/api/v1/events", Auth::Nothing).await;

    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert!(reply.json()["error"].is_string());
    assert_eq!(reply.json()["code"], "token_required");
}

/// The panel's own request: a GET by cookie, which needs no more than the
/// token, and is answered as a stream no cache keeps, starting with the
/// word to ask for everything, since whatever came before was missed.
#[tokio::test]
async fn the_stream_takes_the_cookie_and_starts_by_asking_for_everything() {
    let (server, _) = launch();
    let cookie = format!("{}={}", server.cookie(), server.token());

    let mut stream = subscribe(&server, Auth::Cookie(&cookie)).await;

    assert_eq!(stream.status, StatusCode::OK);
    assert_eq!(stream.header("content-type"), Some("text/event-stream"));
    assert_eq!(stream.header("cache-control"), Some("no-store"));
    assert!(stream.header("content-security-policy").is_some());
    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
    // The fake names nothing to watch, which is everything watched.
    assert_eq!(stream.next().await, json!({"type": "watching"}));
}

#[tokio::test]
async fn a_stream_that_falls_behind_is_told_to_ask_for_everything_again() {
    let (server, _) = launch();
    let mut stream = subscribe(&server, Auth::Bearer(server.token())).await;
    let events = server.events();

    // Nobody reads while these go out, so the stream falls behind.
    for each in 0..CAPACITY + 10 {
        events.publish(Event::Changed {
            scopes: vec![json!(each)],
        });
    }

    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
    assert_eq!(stream.next().await, json!({"type": "watching"}));
    // What it missed, then how things are watched, which may be among it.
    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
    assert_eq!(stream.next().await, json!({"type": "watching"}));
    // Then the latest the bus kept, in order.
    assert_eq!(
        stream.next().await,
        json!({"type": "changed", "scopes": [10]})
    );
}

/// A stream is a request under way for as long as the page is open, which
/// a graceful stop would wait for without end: the stop ends it first.
#[tokio::test]
async fn stopping_the_server_ends_its_streams() {
    let (server, _) = launch();
    let address = server.address();
    let token = server.token().to_owned();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve(async {
        let _ = stopped.await;
    }));
    let (opened, open) = mpsc::channel();
    let reading = tokio::task::spawn_blocking(move || {
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket.set_read_timeout(Some(PATIENCE)).unwrap();
        write!(
            socket,
            "GET /api/v1/events HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\r\n"
        )
        .unwrap();
        let mut seen = Vec::new();
        let mut buffer = [0; 1024];
        let mut told = false;
        loop {
            let read = socket.read(&mut buffer).expect("the stream, then its end");
            if read == 0 {
                break;
            }
            seen.extend_from_slice(&buffer[..read]);
            if !told && String::from_utf8_lossy(&seen).contains("resync") {
                opened.send(()).unwrap();
                told = true;
            }
        }
        String::from_utf8_lossy(&seen).into_owned()
    });
    tokio::task::spawn_blocking(move || open.recv_timeout(PATIENCE))
        .await
        .unwrap()
        .expect("the stream opened");

    stop.send(()).unwrap();

    tokio::time::timeout(PATIENCE, serving)
        .await
        .expect("the server stopped though a stream was open")
        .unwrap()
        .unwrap();
    let seen = tokio::time::timeout(PATIENCE, reading)
        .await
        .expect("the stream ended")
        .unwrap();
    assert!(seen.starts_with("HTTP/1.1 200"), "{seen}");
}

#[tokio::test]
async fn a_run_is_told_as_it_starts_and_once_it_is_in_the_transcript() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    post(
        &server,
        "/api/v1/tasks",
        token,
        json!({ "name": "link beta" }),
    )
    .await;

    let lines = json!(["fake run link beta"]);
    assert_eq!(
        stream.next().await,
        json!({"type": "command_started", "run": 1, "lines": lines, "cancellable": false})
    );
    assert_eq!(
        stream.next().await,
        json!({"type": "command_finished", "run": 1, "lines": lines})
    );
    let transcript = get(&server, "/api/v1/transcript", token).await.json();
    assert_eq!(transcript[0]["lines"], lines);
}

/// Nothing of it ran, so nothing started.
#[tokio::test]
async fn a_task_whose_lines_are_refused_is_told_to_nobody() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    let reply = post(
        &server,
        "/api/v1/tasks",
        token,
        json!({ "name": "unlink -rf", "refuse": true }),
    )
    .await;
    server.events().publish(Event::Resync { switches: 0 });

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(reply.json()["code"], "fake_refused");
    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
}

/// A panel showing a run under way waits for its end: one the backend
/// refuses once it is handed over, or that panics, is told finished too,
/// and the next still runs.
#[tokio::test]
async fn a_run_refused_or_panicking_is_still_told_finished() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    for (run, task) in [
        json!({ "name": "refused", "refuse_run": true }),
        json!({ "name": "broken", "panic": true }),
        json!({ "name": "after" }),
    ]
    .into_iter()
    .enumerate()
    {
        let name = task["name"].as_str().unwrap().to_owned();
        post(&server, "/api/v1/tasks", token, task).await;
        let lines = json!([format!("fake run {name}")]);
        assert_eq!(
            stream.next().await,
            json!({"type": "command_started", "run": run + 1, "lines": lines,
                   "cancellable": false})
        );
        assert_eq!(
            stream.next().await,
            json!({"type": "command_finished", "run": run + 1, "lines": lines})
        );
    }
    assert_eq!(runs.lock().unwrap().last().unwrap(), "end after");
}
