//! A run that takes its time, followed and stopped: what it reports
//! reaches every stream as `progress`, `POST /api/v1/tasks/cancel` reaches
//! what the backend said a cancel does, and whatever is sent meanwhile is
//! told the writer is busy and waits its turn (#112). Over the fake
//! backend, whose task says what it reports, whether its preview takes a
//! cancel, and waits at the fake's hold until a cancel or the test lets it
//! go.

mod common;

use std::time::Duration;

use common::*;
use mindweaver_server::Event;

/// A server over a fake, shared with the tasks a test sends apart, the
/// fake's record of its runs, its hold and the last run it was handed.
struct Launched {
    server: Arc<Listening>,
    runs: Arc<Mutex<Vec<String>>>,
    hold: Arc<Hold>,
    underway: Arc<Mutex<Option<Underway>>>,
}

fn launched() -> Launched {
    let fake = Fake::default();
    Launched {
        runs: Arc::clone(&fake.runs),
        hold: Arc::clone(&fake.hold),
        underway: Arc::clone(&fake.underway),
        server: Arc::new(Server::new(fake).bind().unwrap()),
    }
}

impl Launched {
    /// `POST path` with `body`, sent on a task of its own, so the test goes
    /// on while it waits its turn.
    fn apart(&self, path: &'static str, body: Value) -> tokio::task::JoinHandle<Reply> {
        let server = Arc::clone(&self.server);
        tokio::spawn(async move { post(&server, path, Auth::Bearer(server.token()), body).await })
    }

    /// `POST path` with `body`, waited for.
    async fn post(&self, path: &str, body: Value) -> Reply {
        post(&self.server, path, Auth::Bearer(self.server.token()), body).await
    }

    /// Wait until a run is held at the fake's hold.
    async fn reached(&self) {
        let hold = Arc::clone(&self.hold);
        tokio::task::spawn_blocking(move || hold.reached())
            .await
            .unwrap();
    }

    async fn opened(&self) -> Stream {
        opened(&self.server, Auth::Bearer(self.server.token())).await
    }
}

/// The task of a gather as the fake has one: it reports two steps, takes a
/// cancel, and waits at the hold until it is cancelled or let go.
fn gather() -> Value {
    json!({
        "name": "gather",
        "hold": true,
        "cancellable": true,
        "report": [{"stage": "connecting"}, {"stage": "receiving", "done": 3}],
    })
}

/// The fake's lines for a task named `gather`.
fn lines() -> Value {
    json!(["fake run gather"])
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_s_steps_reach_the_stream_and_a_cancel_reaches_what_the_backend_said_one_does() {
    let launched = launched();
    let _release = Release(Arc::clone(&launched.hold));
    let mut stream = launched.opened().await;

    let ran = launched.apart("/api/v1/tasks", gather());
    launched.reached().await;
    assert_eq!(
        stream.next().await,
        json!({"type": "command_started", "run": 1, "lines": lines(), "cancellable": true})
    );
    assert_eq!(
        stream.next().await,
        json!({"type": "progress", "run": 1, "progress": {"stage": "connecting"}})
    );
    assert_eq!(
        stream.next().await,
        json!({"type": "progress", "run": 1, "progress": {"stage": "receiving", "done": 3}})
    );
    let cancelled = launched
        .post("/api/v1/tasks/cancel", json!({"run": 1}))
        .await;

    assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.text());
    assert_eq!(cancelled.json(), json!({"run": 1, "lines": lines()}));
    assert_eq!(stream.next().await, json!({"type": "cancelling", "run": 1}));
    assert_eq!(
        stream.next().await,
        json!({"type": "command_finished", "run": 1, "lines": lines()})
    );
    let ran = ran.await.unwrap();
    assert_eq!(ran.status, StatusCode::OK);
    assert_eq!(ran.json()["outcome"]["cancelled"], true);
    assert_eq!(
        *launched.runs.lock().unwrap(),
        ["start gather", "cancel gather", "end gather"]
    );
}

/// Asked again, a cancel is answered again and handed to the run again,
/// and told once; a run past the point where it can stop goes on to its
/// end all the same.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_asked_twice_is_told_once_and_a_run_too_late_to_stop_goes_on() {
    let launched = launched();
    let release = Release(Arc::clone(&launched.hold));
    let mut stream = launched.opened().await;
    let ran = launched.apart(
        "/api/v1/tasks",
        json!({"name": "gather", "cancellable": true, "hold": true, "late": true}),
    );
    launched.reached().await;
    stream.until("command_started").await;

    for _ in 0..2 {
        let cancelled = launched
            .post("/api/v1/tasks/cancel", json!({"run": 1}))
            .await;
        assert_eq!(cancelled.status, StatusCode::OK, "{}", cancelled.text());
    }
    drop(release);

    assert_eq!(stream.next().await, json!({"type": "cancelling", "run": 1}));
    assert_eq!(stream.next().await["type"], "command_finished");
    assert_eq!(ran.await.unwrap().json()["outcome"]["cancelled"], true);
    assert_eq!(
        *launched.runs.lock().unwrap(),
        [
            "start gather",
            "cancel gather",
            "cancel gather",
            "end gather"
        ]
    );
}

/// A task sent while a run holds the writer thread waits its turn, and
/// every stream is told what it waits behind, rather than nothing for as
/// long as the run takes.
#[tokio::test(flavor = "multi_thread")]
async fn whatever_is_sent_behind_a_run_under_way_is_told_busy_and_waits_its_turn() {
    let launched = launched();
    let release = Release(Arc::clone(&launched.hold));
    let mut stream = launched.opened().await;
    let first = launched.apart("/api/v1/tasks", gather());
    launched.reached().await;
    stream.until("command_started").await;
    stream.next().await;
    stream.next().await;

    let after = launched.apart("/api/v1/tasks", json!({"name": "after"}));
    let switched = launched.apart("/api/v1/switch", json!({"to": "beta"}));

    let busy = json!({"type": "busy", "run": 1, "lines": lines(), "cancellable": true,
                      "cancelling": false});
    assert_eq!(stream.next().await, busy);
    assert_eq!(stream.next().await, busy);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(*launched.runs.lock().unwrap(), ["start gather"]);
    drop(release);
    for reply in [first, after, switched] {
        assert_eq!(reply.await.unwrap().status, StatusCode::OK);
    }
    assert_eq!(
        stream.next().await,
        json!({"type": "command_finished", "run": 1, "lines": lines()})
    );
    let next = stream.next().await;
    // The run and the switch queued in whichever order they reached the
    // queue; the run is numbered 2 either way.
    if next["type"] == "switched" {
        assert_eq!(stream.next().await["run"], 2);
    } else {
        assert_eq!(next["type"], "command_started");
        assert_eq!(next["run"], 2);
        assert_eq!(next["cancellable"], false);
    }
}

/// A tab opened, or shown again, in the middle of a run is told what the
/// writer is busy with and the latest step it reported, without waiting
/// for the next, which a gather still connecting may not report for long.
#[tokio::test(flavor = "multi_thread")]
async fn a_stream_opened_while_a_run_is_under_way_is_told_of_it_and_its_latest_step() {
    let launched = launched();
    let release = Release(Arc::clone(&launched.hold));
    let ran = launched.apart("/api/v1/tasks", gather());
    launched.reached().await;

    let mut stream = subscribe(&launched.server, Auth::Bearer(launched.server.token())).await;

    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
    assert_eq!(stream.next().await, json!({"type": "watching"}));
    assert_eq!(
        stream.next().await,
        json!({"type": "busy", "run": 1, "lines": lines(), "cancellable": true,
               "cancelling": false})
    );
    assert_eq!(
        stream.next().await,
        json!({"type": "progress", "run": 1, "progress": {"stage": "receiving", "done": 3}})
    );
    drop(release);
    ran.await.unwrap();
}

/// Nothing is asked of a run a cancel does not name as the one under way:
/// one over, so a cancel that crosses the end of its run never stops the
/// one queued behind it, or one under way that takes no cancel.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_of_a_run_not_under_way_or_that_takes_none_is_refused_and_asks_nothing() {
    let launched = launched();
    let release = Release(Arc::clone(&launched.hold));
    let mut stream = launched.opened().await;

    let none = launched
        .post("/api/v1/tasks/cancel", json!({"run": 1}))
        .await;
    assert_eq!(none.status, StatusCode::CONFLICT);
    assert_eq!(
        none.json(),
        json!({"code": "not_running",
               "error": "run 1 is not under way, and nothing is: a run is stopped only while it runs"})
    );
    let first = launched
        .post("/api/v1/tasks", json!({"name": "first"}))
        .await;
    assert_eq!(first.status, StatusCode::OK);
    let second = launched.apart(
        "/api/v1/tasks",
        json!({"name": "second", "hold": true, "cancellable": true}),
    );
    launched.reached().await;

    let crossed = launched
        .post("/api/v1/tasks/cancel", json!({"run": 1}))
        .await;
    let ahead = launched
        .post("/api/v1/tasks/cancel", json!({"run": 3}))
        .await;

    assert_eq!(crossed.status, StatusCode::CONFLICT);
    assert_eq!(
        crossed.json(),
        json!({"code": "not_running",
               "error": "run 1 is not under way, run 2 is: a run is stopped only while it runs"})
    );
    assert_eq!(ahead.json()["code"], "not_running");
    drop(release);
    assert!(second.await.unwrap().json()["outcome"]
        .get("cancelled")
        .is_none());
    let told = stream.until("command_finished").await;
    let told = [told, stream.until("command_finished").await].concat();
    assert!(
        told.iter().all(|event| event["type"] != "cancelling"),
        "{told:?}"
    );
    assert!(!launched
        .runs
        .lock()
        .unwrap()
        .iter()
        .any(|run| run.starts_with("cancel")));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_of_a_run_whose_preview_takes_none_is_refused_though_it_says_what_one_does() {
    let launched = launched();
    let release = Release(Arc::clone(&launched.hold));
    let ran = launched.apart("/api/v1/tasks", json!({"name": "link", "hold": true}));
    launched.reached().await;

    let refused = launched
        .post("/api/v1/tasks/cancel", json!({"run": 1}))
        .await;

    assert_eq!(refused.status, StatusCode::CONFLICT);
    assert_eq!(
        refused.json(),
        json!({"code": "uncancellable",
               "error": "run 1 takes no cancel: none of its lines stops when asked, so it runs to its end"})
    );
    drop(release);
    assert!(ran.await.unwrap().json()["outcome"]
        .get("cancelled")
        .is_none());
    assert_eq!(*launched.runs.lock().unwrap(), ["start link", "end link"]);
}

/// What the backend said a cancel does panics: the request that asked is
/// told the backend failed, and the server, and the run, go on.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_whose_hook_panics_is_said_to_have_failed_and_the_run_goes_on() {
    let launched = launched();
    let release = Release(Arc::clone(&launched.hold));
    let ran = launched.apart(
        "/api/v1/tasks",
        json!({"name": "gather", "hold": true, "cancellable": true, "cancel_panics": true}),
    );
    launched.reached().await;

    let failed = launched
        .post("/api/v1/tasks/cancel", json!({"run": 1}))
        .await;

    assert_eq!(failed.status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(failed.json()["code"], "backend_failed");
    drop(release);
    assert_eq!(ran.await.unwrap().status, StatusCode::OK);
    let after = launched
        .post("/api/v1/tasks", json!({"name": "after"}))
        .await;
    assert_eq!(after.status, StatusCode::OK);
}

/// A step a backend reports once its call has returned would say a run
/// over is still going: it reaches nobody.
#[tokio::test(flavor = "multi_thread")]
async fn a_step_reported_once_the_run_is_over_reaches_nobody() {
    let launched = launched();
    let mut stream = launched.opened().await;
    launched
        .post("/api/v1/tasks", json!({"name": "quick"}))
        .await;
    stream.until("command_finished").await;

    let kept = launched.underway.lock().unwrap().clone().unwrap();
    kept.report(json!({"stage": "late"}));
    launched
        .server
        .events()
        .publish(Event::Resync { switches: 0 });

    assert_eq!(kept.run(), 1);
    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
}

/// A server stopping asks the run under way to stop, as leaving the TUI
/// stops its gather, rather than wait for as long as the network takes:
/// the fake's held run goes on only once it is cancelled, so a stop that
/// asked nothing would never end.
#[tokio::test(flavor = "multi_thread")]
async fn stopping_the_server_asks_the_run_under_way_to_stop() {
    let fake = Fake::default();
    let runs = Arc::clone(&fake.runs);
    let hold = Arc::clone(&fake.hold);
    let server = Server::new(fake).bind().unwrap();
    let address = server.address();
    let token = server.token().to_owned();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let serving = tokio::spawn(server.serve(async {
        let _ = stopped.await;
    }));
    let running = tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let mut socket = std::net::TcpStream::connect(address).unwrap();
        socket.set_read_timeout(Some(PATIENCE)).unwrap();
        let body = json!({"name": "gather", "hold": true, "cancellable": true}).to_string();
        write!(
            socket,
            "POST /api/v1/tasks HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer {token}\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        let mut answer = String::new();
        socket.read_to_string(&mut answer).unwrap();
        answer
    });
    let reached = Arc::clone(&hold);
    tokio::task::spawn_blocking(move || reached.reached())
        .await
        .unwrap();

    stop.send(()).unwrap();

    tokio::time::timeout(PATIENCE, serving)
        .await
        .expect("the server stopped though a run was under way")
        .unwrap()
        .unwrap();
    let answer = running.await.unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(answer.contains("\"cancelled\":true"), "{answer}");
    assert_eq!(
        *runs.lock().unwrap(),
        ["start gather", "cancel gather", "end gather"]
    );
    // Let go regardless, so a failure above leaves no thread waiting.
    hold.release();
}
