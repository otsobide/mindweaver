//! Switching what the backend acts on: carried to the backend on the one
//! thread every run goes through, refused as the backend refuses it with
//! nothing told, and, once taken, what the backend names watched in place
//! of what it named before, and every subscriber told to read everything
//! again. What may be switched to is the backend's to say: the server only
//! carries the request.

mod common;

use std::fs;
use std::time::Duration;

use common::*;
use mindweaver_server::{Event, DEBOUNCE};
use tempfile::TempDir;

/// How long a change takes at most to be told, as `watch_test` waits.
fn debounced() -> Duration {
    DEBOUNCE * 5
}

#[tokio::test]
async fn a_switch_is_answered_with_what_the_backend_acts_on_and_told_to_every_subscriber() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    let reply = post(&server, "/api/v1/switch", token, json!({ "to": "beta" })).await;

    assert_eq!(reply.status, StatusCode::OK, "{}", reply.text());
    assert_eq!(reply.json(), json!({ "on": "beta" }));
    assert_eq!(*runs.lock().unwrap(), ["switch beta"]);
    assert_eq!(
        stream.next().await,
        json!({"type": "switched", "switches": 1})
    );
}

/// A stream opened after a switch, as a tab's is once it is shown again,
/// having let go of it while hidden, is told how many there were in its
/// resync, which is how that tab learns it missed one; and the next switch
/// is counted after them.
#[tokio::test]
async fn a_stream_opened_after_a_switch_is_told_how_many_there_were() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());

    post(&server, "/api/v1/switch", token, json!({ "to": "beta" })).await;
    post(&server, "/api/v1/switch", token, json!({ "to": "gamma" })).await;
    let mut stream = subscribe(&server, token).await;

    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 2})
    );
    assert_eq!(stream.next().await, json!({"type": "watching"}));
    post(&server, "/api/v1/switch", token, json!({ "to": "beta" })).await;
    assert_eq!(
        stream.next().await,
        json!({"type": "switched", "switches": 3})
    );
}

/// The test #84 asks of the server: a switch the backend refuses, to a
/// place outside what it offers, is a 422 with the backend's code, and
/// nothing is switched, watched anew or told.
#[tokio::test]
async fn a_switch_the_backend_refuses_is_a_422_with_its_code_and_switches_nothing() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    let reply = post(
        &server,
        "/api/v1/switch",
        token,
        json!({ "to": "/etc", "refuse": true }),
    )
    .await;
    server.events().publish(Event::Resync { switches: 0 });

    assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        reply.json(),
        json!({ "error": "/etc is refused", "code": "fake_refused" })
    );
    assert!(runs.lock().unwrap().is_empty());
    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
}

#[tokio::test]
async fn a_switch_the_backend_cannot_read_is_refused_like_any_other_body() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());

    for body in [
        json!({ "nowhere": "beta" }),
        json!("beta"),
        json!({ "to": ["beta"] }),
    ] {
        let reply = post(&server, "/api/v1/switch", token, body.clone()).await;

        assert_eq!(reply.status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
        assert!(reply.json()["error"].is_string(), "{body}");
        assert_eq!(reply.json()["code"], "invalid_body", "{body}");
    }
    assert!(runs.lock().unwrap().is_empty());
}

/// A backend that panics while it switches may have switched first, so the
/// panel is told to read everything again all the same; the thread lives
/// on, and the next switch is taken.
#[tokio::test]
async fn a_switch_that_panics_is_a_500_still_told_and_the_next_is_taken() {
    let (server, runs) = launch();
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    let reply = post(
        &server,
        "/api/v1/switch",
        token,
        json!({ "to": "broken", "panic": true }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        reply.json(),
        json!({
            "error": "the backend panicked while switching",
            "code": "backend_failed",
        })
    );
    assert_eq!(
        stream.next().await,
        json!({"type": "switched", "switches": 1})
    );

    let reply = post(&server, "/api/v1/switch", token, json!({ "to": "after" })).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(*runs.lock().unwrap(), ["switch after"]);
}

/// A switch takes its turn behind the run under way, never landing while
/// it runs, and a run sent after it waits for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_switch_waits_for_the_run_before_it_and_the_run_after_it_waits_for_it() {
    let fake = Fake::default();
    let runs = Arc::clone(&fake.runs);
    let hold = Arc::clone(&fake.hold);
    let server = Arc::new(Server::new(fake).bind().unwrap());
    let sent = |path: &'static str, body: Value| {
        let server = Arc::clone(&server);
        tokio::spawn(async move { post(&server, path, Auth::Bearer(server.token()), body).await })
    };

    let release = Release(Arc::clone(&hold));
    let first = sent("/api/v1/tasks", json!({ "name": "first", "hold": true }));
    let reached = Arc::clone(&hold);
    tokio::task::spawn_blocking(move || reached.reached())
        .await
        .unwrap();
    let switched = sent("/api/v1/switch", json!({ "to": "beta" }));
    tokio::time::sleep(Duration::from_millis(100)).await;
    let after = sent("/api/v1/tasks", json!({ "name": "after" }));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(*runs.lock().unwrap(), ["start first"], "it landed mid-run");
    drop(release);

    for reply in [first, switched, after] {
        assert_eq!(reply.await.unwrap().status, StatusCode::OK);
    }
    assert_eq!(
        *runs.lock().unwrap(),
        [
            "start first",
            "end first",
            "switch beta",
            "start after",
            "end after"
        ]
    );
}

/// Nothing to replay: the transcript holds runs, and a switch is none.
#[tokio::test]
async fn a_switch_is_kept_in_no_transcript() {
    let (server, _) = launch();
    let token = Auth::Bearer(server.token());

    post(&server, "/api/v1/tasks", token, json!({ "name": "a" })).await;
    post(&server, "/api/v1/switch", token, json!({ "to": "beta" })).await;

    let transcript = get(&server, "/api/v1/transcript", token).await.json();
    assert_eq!(transcript.as_array().unwrap().len(), 1, "{transcript}");
}

/// Once switched, what the backend names now is watched and what it named
/// before is not, before the panel is told: a change to the folder of the
/// one before is nobody's, and one to the new one's is told by its scope.
#[tokio::test]
async fn once_switched_what_the_backend_names_now_is_watched_and_not_what_it_named_before() {
    let before = TempDir::new().unwrap();
    let after = TempDir::new().unwrap();
    let (server, _) = launch_watching(vec![Watched {
        path: before.path().to_path_buf(),
        scope: json!("alpha"),
    }]);
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    let reply = post(
        &server,
        "/api/v1/switch",
        token,
        json!({ "to": "beta", "watch": [after.path()] }),
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(
        stream.next().await,
        json!({"type": "switched", "switches": 1})
    );

    fs::write(before.path().join("old.md"), "# Old\n").unwrap();
    stream.quiet(debounced()).await;
    fs::write(after.path().join("new.md"), "# New\n").unwrap();
    assert_eq!(
        stream.next().await,
        json!({"type": "changed", "scopes": ["beta"]})
    );
}

/// A backend that named nothing at first, so nothing was watched, is
/// watched once a switch names something.
#[tokio::test]
async fn a_switch_naming_paths_where_nothing_was_watched_starts_watching_them() {
    let after = TempDir::new().unwrap();
    let (server, _) = launch_watching(Vec::new());
    let token = Auth::Bearer(server.token());
    let mut stream = opened(&server, token).await;

    post(
        &server,
        "/api/v1/switch",
        token,
        json!({ "to": "beta", "watch": [after.path()] }),
    )
    .await;
    let told = stream.until("switched").await;
    assert_eq!(
        told.last(),
        Some(&json!({"type": "switched", "switches": 1})),
        "{told:?}"
    );

    fs::write(after.path().join("new.md"), "# New\n").unwrap();
    assert_eq!(
        stream.until("changed").await.last(),
        Some(&json!({"type": "changed", "scopes": ["beta"]}))
    );
}
