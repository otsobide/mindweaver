//! What every server suite shares: a fake backend with types of its own, and
//! a way to send it requests without a socket.
//!
//! Everything is re-exported so a suite says `use common::*;` and nothing
//! else; not every suite uses every helper.

#![allow(dead_code, unused_imports)]

pub mod mother;

pub use axum::body::{Body, Bytes};
pub use axum::http::{header, HeaderMap, Method, Request, StatusCode};
pub use mindweaver_server::{
    Arguments, Backend, Condition, Grounds, Listening, Own, Parameter, Preview, Refusal, Route,
    Server, Shape, Underway, Watched, API,
};
pub use serde_json::{json, Value};
pub use std::sync::{Arc, Condvar, Mutex};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize, Serializer};
use std::time::Duration;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// The fake backend
// ---------------------------------------------------------------------------

/// The fake's read: say back what was sent, so a test can tell it arrived
/// as the route it was sent to makes it, or refuse to, or panic, as a
/// backend with a bug would. What it answers is itself, each field left at
/// its default left out.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FakeQuery {
    pub echo: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub refuse: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub absent: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub panic: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub times: Option<u64>,
}

/// What `GET /api/v1/words` sends: a parameter named otherwise than the
/// field it fills, as a route may name one.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Words {
    #[serde(default)]
    word: Vec<String>,
    times: Option<u64>,
}

/// The fake's reads, one of each shape a parameter has and a POST:
///
/// - `GET /echo?echo=...&refuse=true`, read as the query whole, and refused
///   by the route itself when there is nothing to echo; with `absent=true`
///   in place of `refuse`, refused as naming what is not there;
/// - `GET /words?word=a&word=b&times=2`, its words in `words`;
/// - `POST /judge`, its body read as the query whole, and refused by the
///   route as the echo route refuses; the one a query that panics is sent
///   to, `{"echo": "x", "panic": true}`.
pub fn fake_routes() -> Vec<Route<FakeQuery>> {
    vec![
        Route {
            method: mindweaver_server::Method::Get,
            path: "/echo",
            about: "What was sent, said back",
            parameters: vec![
                Parameter::required("echo", Shape::Text, "What to say back"),
                Parameter::optional("refuse", Shape::Flag, "Whether to refuse it"),
                Parameter::optional("absent", Shape::Flag, "Whether it is not there"),
            ],
            body: None,
            answers: "FakeQuery",
            query: echoed,
        },
        Route {
            method: mindweaver_server::Method::Get,
            path: "/words",
            about: "Words, said back",
            parameters: vec![
                Parameter::optional("word", Shape::List, "One of the words"),
                Parameter::optional("times", Shape::Number, "How many times"),
            ],
            body: None,
            answers: "FakeQuery",
            query: |sent| {
                let Words { word, times } = sent.read()?;
                Ok(FakeQuery {
                    echo: String::from("words"),
                    refuse: false,
                    absent: false,
                    panic: false,
                    words: word,
                    times,
                })
            },
        },
        Route {
            method: mindweaver_server::Method::Post,
            path: "/judge",
            about: "What was sent in the body, said back",
            parameters: Vec::new(),
            body: Some("FakeQuery"),
            answers: "FakeQuery",
            query: echoed,
        },
    ]
}

/// What was sent, read as the query whole, or refused by the route when
/// there is nothing to echo.
fn echoed(sent: Arguments) -> Result<FakeQuery, String> {
    let query: FakeQuery = sent.read()?;
    if query.echo.is_empty() {
        return Err(String::from("there is nothing to echo"));
    }
    Ok(query)
}

/// The fake's task: a name to run under, whether to wait at the fake's hold
/// before finishing, whether to finish with an outcome JSON cannot carry,
/// whether to refuse it, when its lines are asked for (`refuse`) or only
/// once it is handed over to run (`refuse_run`), whether its run panics, as
/// a backend with a bug would, what its preview says to confirm first, in
/// words of the fake's own (`confirm`), whether its preview says it takes
/// a cancel (`cancellable`), what it reports as it runs, before it reaches
/// the hold (`report`), whether what it says a cancel does panics
/// (`cancel_panics`), and whether a cancel leaves it waiting at the hold
/// all the same (`late`), as a gather past the point where it can stop
/// goes on.
///
/// A run takes no notice of `refuse`: one whose lines were refused and is
/// handed over all the same runs, and the runs it leaves say so. Nor of
/// `cancellable`: what its preview says is the server's to hold a cancel
/// to, and a run says what a cancel does whatever its preview said.
#[derive(Deserialize)]
pub struct FakeTask {
    pub name: String,
    #[serde(default)]
    pub hold: bool,
    #[serde(default)]
    pub unwritable: bool,
    #[serde(default)]
    pub refuse: bool,
    #[serde(default)]
    pub refuse_run: bool,
    #[serde(default)]
    pub panic: bool,
    #[serde(default)]
    pub confirm: Vec<String>,
    #[serde(default)]
    pub cancellable: bool,
    #[serde(default)]
    pub report: Vec<Value>,
    #[serde(default)]
    pub cancel_panics: bool,
    #[serde(default)]
    pub late: bool,
}

/// The fake's switch: what to act on from now on, by a name of the fake's
/// own, whether to refuse it, whether it panics, and, when it is taken,
/// what to watch from then on in place of what the fake watches now
/// (`watch`, each path watched under the name as its scope).
#[derive(Deserialize)]
pub struct FakeSwitch {
    pub to: String,
    #[serde(default)]
    pub refuse: bool,
    #[serde(default)]
    pub panic: bool,
    #[serde(default)]
    pub watch: Option<Vec<std::path::PathBuf>>,
}

/// What the fake refuses with: a code of its own, which a real backend's
/// would be one of its front end's, on the grounds of a request it will not
/// act on.
pub fn refusal(what: &str) -> Refusal {
    Refusal::unfit("fake_refused", format!("{what} is refused"))
}

/// What the fake refuses a read naming what is not there with: a code of
/// its own, on the grounds that it is absent.
pub fn absence(what: &str) -> Refusal {
    Refusal::absent("fake_absent", format!("{what} is not there"))
}

/// Every code the fake refuses with, beside the server's own.
pub const FAKE_CODES: [&str; 2] = ["fake_refused", "fake_absent"];

/// The fake's description of the API, which the server serves as it is:
/// laid out as no serializer would write it, so a server that read it and
/// wrote it again would be caught.
pub const FAKE_DOCUMENT: &str = "{\"openapi\": \"3.1.1\",\n  \"info\": {\"title\": \"the fake\", \"version\": \"1\"}, \"paths\": {}}\n";

/// What running a fake task says.
#[derive(Serialize)]
pub struct FakeOutcome {
    pub ran: String,
    /// The thread it ran on.
    pub thread: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unwritable: Option<Unwritable>,
    /// Whether it was asked to stop before it finished, which a run that
    /// takes a cancel says as how it finished.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cancelled: bool,
}

/// A part of an outcome that refuses to serialize, as a path that is not
/// UTF-8 or a map keyed by something other than strings does.
pub struct Unwritable;

impl Serialize for Unwritable {
    fn serialize<S: Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("this part cannot be written"))
    }
}

/// A backend that knows nothing of mindweaver, which is the point: the server
/// must carry types it has never heard of.
#[derive(Default)]
pub struct Fake {
    /// `start <name>` and `end <name>` for every task it ran, in order, so
    /// two runs at once would show as one starting before the other ended.
    pub runs: Arc<Mutex<Vec<String>>>,
    /// Where a task that asks to be held waits.
    pub hold: Arc<Hold>,
    /// What it asks the server to watch, which a test changes as it goes.
    pub watched: Arc<Mutex<Vec<Watched>>>,
    /// What it acts on, by the name the last switch it took gave it: none
    /// before any.
    pub on: Arc<Mutex<Option<String>>>,
    /// The last run it was handed, as it was handed it, kept past its end
    /// for a test to report through once it is over.
    pub underway: Arc<Mutex<Option<Underway>>>,
}

impl Backend for Fake {
    type Query = FakeQuery;
    type Task = FakeTask;
    type Outcome = FakeOutcome;
    type Switch = FakeSwitch;

    fn query(&self, query: FakeQuery) -> Result<Value, Refusal> {
        if query.refuse {
            return Err(refusal(&query.echo));
        }
        if query.absent {
            return Err(absence(&query.echo));
        }
        if query.panic {
            panic!("the read of {} panics, as asked", query.echo);
        }
        Ok(serde_json::to_value(query).unwrap())
    }

    fn routes() -> Vec<Route<FakeQuery>> {
        fake_routes()
    }

    fn document() -> &'static str {
        FAKE_DOCUMENT
    }

    fn preview(&self, task: &FakeTask) -> Result<Preview, Refusal> {
        if task.refuse {
            return Err(refusal(&task.name));
        }
        Ok(Preview {
            lines: vec![format!("fake run {}", task.name)],
            confirm: task
                .confirm
                .iter()
                .map(|what| json!({ "fake_takes": what }))
                .collect(),
            cancellable: task.cancellable,
        })
    }

    /// A cancel is recorded among the runs, `cancel <name>`, and lets a
    /// held run go on, as a gather stops waiting on the network.
    fn run(&self, task: FakeTask, underway: &Underway) -> Result<FakeOutcome, Refusal> {
        *self.underway.lock().unwrap() = Some(underway.clone());
        if task.refuse_run {
            return Err(refusal(&task.name));
        }
        if task.panic {
            panic!("{} panics, as asked", task.name);
        }
        self.runs
            .lock()
            .unwrap()
            .push(format!("start {}", task.name));
        underway.on_cancel({
            let runs = Arc::clone(&self.runs);
            let hold = Arc::clone(&self.hold);
            let name = task.name.clone();
            let panics = task.cancel_panics;
            let late = task.late;
            move || {
                assert!(!panics, "the cancel of {name} panics, as asked");
                runs.lock().unwrap().push(format!("cancel {name}"));
                if !late {
                    hold.release();
                }
            }
        });
        for progress in &task.report {
            underway.report(progress.clone());
        }
        if task.hold {
            self.hold.wait();
        }
        self.runs.lock().unwrap().push(format!("end {}", task.name));
        Ok(FakeOutcome {
            ran: task.name,
            thread: std::thread::current().name().map(str::to_owned),
            unwritable: task.unwritable.then_some(Unwritable),
            cancelled: underway.is_cancelled(),
        })
    }

    /// Recorded among the runs, `switch <to>`, so a test sees it take its
    /// turn among them.
    fn switch(&self, to: FakeSwitch) -> Result<Value, Refusal> {
        if to.refuse {
            return Err(refusal(&to.to));
        }
        if to.panic {
            panic!("the switch to {} panics, as asked", to.to);
        }
        self.runs.lock().unwrap().push(format!("switch {}", to.to));
        if let Some(watch) = to.watch {
            *self.watched.lock().unwrap() = watch
                .into_iter()
                .map(|path| Watched {
                    path,
                    scope: json!(to.to),
                })
                .collect();
        }
        *self.on.lock().unwrap() = Some(to.to.clone());
        Ok(json!({ "on": to.to }))
    }

    fn watched_paths(&self) -> Vec<Watched> {
        self.watched.lock().unwrap().clone()
    }
}

/// A point a run waits at until the test lets it go, so the test can do
/// things while a write is known to be under way.
#[derive(Default)]
pub struct Hold {
    /// Whether a run has reached the hold, and whether it has been let go.
    state: Mutex<(bool, bool)>,
    changed: Condvar,
}

impl Hold {
    /// A run arrives, says so, and waits to be let go.
    fn wait(&self) {
        let mut state = self.state.lock().unwrap();
        state.0 = true;
        self.changed.notify_all();
        while !state.1 {
            state = self.changed.wait(state).unwrap();
        }
    }

    /// Block until a run is waiting here, or fail the test after long enough
    /// that none is coming.
    pub fn reached(&self) {
        let state = self.state.lock().unwrap();
        let (state, _) = self
            .changed
            .wait_timeout_while(state, Duration::from_secs(10), |state| !state.0)
            .unwrap();
        assert!(state.0, "no run reached the hold");
    }

    /// Let the waiting run go on.
    pub fn release(&self) {
        self.state.lock().unwrap().1 = true;
        self.changed.notify_all();
    }
}

/// Releases the hold when dropped, so an assertion that fails while a run is
/// held cannot leave it waiting, and the test hanging on it, for ever.
pub struct Release(pub Arc<Hold>);

impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}

/// A server over a fake, bound and ready, and the fake's record of its runs.
pub fn launch() -> (Listening, Arc<Mutex<Vec<String>>>) {
    let fake = Fake::default();
    let runs = Arc::clone(&fake.runs);
    (Server::new(fake).bind().unwrap(), runs)
}

/// A server over a fake that logs, and the lines it has logged so far.
pub fn launch_logged() -> (Listening, Arc<Mutex<Vec<String>>>) {
    launch_logging(Fake::default())
}

/// A server over `fake` that logs, for a test that holds a run at the
/// fake's hold, and the lines it has logged so far.
pub fn launch_logging(fake: Fake) -> (Listening, Arc<Mutex<Vec<String>>>) {
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&lines);
    let server = Server::new(fake)
        .log(move |line| sink.lock().unwrap().push(line.to_owned()))
        .bind()
        .unwrap();
    (server, lines)
}

/// A logged line with every time it says, `3ms`, written `Nms`, and those
/// times in the order it says them: what a test holds a line of the writer
/// thread's to, whose times are in the middle of it.
pub fn times(line: &str) -> (String, Vec<Duration>) {
    let mut said = Vec::new();
    let mut times = Vec::new();
    for word in line.split(' ') {
        let (bare, after) = match word.strip_suffix(',') {
            Some(bare) => (bare, ","),
            None => (word, ""),
        };
        match bare
            .strip_suffix("ms")
            .and_then(|millis| millis.parse().ok())
        {
            Some(millis) => {
                times.push(Duration::from_millis(millis));
                said.push(format!("Nms{after}"));
            }
            None => said.push(word.to_owned()),
        }
    }
    (said.join(" "), times)
}

/// A logged line without the time it says at its end, ` 3ms`, and that
/// time: what a test holds a line's words to, apart from a time no test
/// can say beforehand.
pub fn untimed(line: &str) -> (&str, Duration) {
    let (said, time) = line
        .rsplit_once(' ')
        .unwrap_or_else(|| panic!("no time at the end of {line:?}"));
    let millis = time
        .strip_suffix("ms")
        .and_then(|millis| millis.parse().ok())
        .unwrap_or_else(|| panic!("not a time in milliseconds at the end of {line:?}"));
    (said, Duration::from_millis(millis))
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// What a request carries to prove it may be answered.
#[derive(Clone, Copy)]
pub enum Auth<'a> {
    Nothing,
    Bearer(&'a str),
    /// A whole `name=value` pair, sent as the panel's page sends it: with the
    /// headers the browser adds to every request the page makes, and the one
    /// the panel adds itself.
    Cookie(&'a str),
}

/// What came back.
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl Reply {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|error| panic!("not JSON ({error}): {:?}", self.text()))
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }
}

/// The request [`send`] sends, for a test that changes it before it goes.
///
/// It names the server as the launch link does, `Host: 127.0.0.1:<port>`:
/// over the socket every request names one, a browser's and a script's alike.
pub fn request(
    server: &Listening,
    method: Method,
    path: &str,
    auth: Auth<'_>,
    body: Option<Value>,
) -> Request<Body> {
    let mut request = Request::builder()
        .method(method)
        .uri(path)
        .header(header::HOST, server.address().to_string());
    match auth {
        Auth::Nothing => {}
        Auth::Bearer(token) => {
            request = request.header(header::AUTHORIZATION, format!("Bearer {token}"))
        }
        Auth::Cookie(pair) => {
            request = request
                .header(header::COOKIE, pair)
                .header(header::ORIGIN, format!("http://{}", server.address()))
                .header("sec-fetch-site", "same-origin")
                .header("x-mindweaver", "1")
        }
    }
    match body {
        Some(body) => request
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string())),
        None => request.body(Body::empty()),
    }
    .unwrap()
}

/// Send a request through the server's router, in process.
pub async fn deliver(server: &Listening, request: Request<Body>) -> Reply {
    let response = server.router().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    Reply {
        status,
        headers,
        body,
    }
}

/// Send one request to the server, in process.
pub async fn send(
    server: &Listening,
    method: Method,
    path: &str,
    auth: Auth<'_>,
    body: Option<Value>,
) -> Reply {
    deliver(server, request(server, method, path, auth, body)).await
}

/// `GET path`.
pub async fn get(server: &Listening, path: &str, auth: Auth<'_>) -> Reply {
    send(server, Method::GET, path, auth, None).await
}

/// `POST path` with a JSON body.
pub async fn post(server: &Listening, path: &str, auth: Auth<'_>, body: Value) -> Reply {
    send(server, Method::POST, path, auth, Some(body)).await
}

// ---------------------------------------------------------------------------
// The event stream
// ---------------------------------------------------------------------------

/// How long a test waits for an event it expects before it fails: long
/// past the watcher's debounce, for a machine under load, and still bounded.
pub const PATIENCE: Duration = Duration::from_secs(10);

/// `GET /api/v1/events` answered, read an event at a time as a browser's
/// `EventSource` reads it.
pub struct Stream {
    pub status: StatusCode,
    pub headers: HeaderMap,
    body: axum::body::BodyDataStream,
    buffer: String,
}

impl Stream {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(|value| value.to_str().unwrap())
    }

    /// The next event's JSON, or a failed test once [`PATIENCE`] has
    /// passed without one.
    pub async fn next(&mut self) -> Value {
        self.within(PATIENCE)
            .await
            .unwrap_or_else(|| panic!("no event in {PATIENCE:?}"))
    }

    /// The next event's JSON, or none when `wait` passes without one or
    /// the stream ends: comments, which keep the connection alive, are
    /// passed over.
    pub async fn within(&mut self, wait: Duration) -> Option<Value> {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if let Some(end) = self.buffer.find("\n\n") {
                let frame: String = self.buffer.drain(..end + 2).collect();
                let data: Vec<&str> = frame
                    .lines()
                    .filter_map(|line| line.strip_prefix("data:"))
                    .map(|data| data.strip_prefix(' ').unwrap_or(data))
                    .collect();
                if data.is_empty() {
                    continue;
                }
                return Some(serde_json::from_str(&data.join("\n")).unwrap());
            }
            let chunk = tokio::time::timeout_at(deadline, self.body.next())
                .await
                .ok()??;
            self.buffer
                .push_str(std::str::from_utf8(&chunk.unwrap()).unwrap());
        }
    }

    /// Nothing told within `wait`, where a change is nobody's. On macOS,
    /// FSEvents now and then tells a change to a folder watched that
    /// nobody made there (`docs/architecture.md`, open questions), which
    /// costs the panel a snapshot and nothing more: there what comes is
    /// passed over, and a test holds the watcher to what it must tell.
    pub async fn quiet(&mut self, wait: Duration) {
        if cfg!(target_os = "macos") {
            while self.within(wait).await.is_some() {}
        } else {
            let told = self.within(wait).await;
            assert_eq!(told, None, "told where nothing changed");
        }
    }

    /// Every event up to the first of `type`, which is the last returned,
    /// or a failed test once [`PATIENCE`] has passed without one.
    pub async fn until(&mut self, kind: &str) -> Vec<Value> {
        let mut seen = Vec::new();
        loop {
            let event = self.next().await;
            let found = event["type"] == kind;
            seen.push(event);
            if found {
                return seen;
            }
        }
    }
}

/// Open the event stream, in process.
pub async fn subscribe(server: &Listening, auth: Auth<'_>) -> Stream {
    let response = server
        .router()
        .oneshot(request(server, Method::GET, "/api/v1/events", auth, None))
        .await
        .unwrap();
    Stream {
        status: response.status(),
        headers: response.headers().clone(),
        body: response.into_body().into_data_stream(),
        buffer: String::new(),
    }
}

/// The event stream opened, past what it says first: to ask for
/// everything, and that everything named is watched.
pub async fn opened(server: &Listening, auth: Auth<'_>) -> Stream {
    let mut stream = subscribe(server, auth).await;
    assert_eq!(
        stream.next().await,
        json!({"type": "resync", "switches": 0})
    );
    assert_eq!(stream.next().await, json!({"type": "watching"}));
    stream
}

/// A server over a fake that names `watched` to watch, bound and ready, and
/// the list, which a test changes for the next time the server asks.
pub fn launch_watching(watched: Vec<Watched>) -> (Listening, Arc<Mutex<Vec<Watched>>>) {
    let fake = Fake::default();
    *fake.watched.lock().unwrap() = watched;
    let list = Arc::clone(&fake.watched);
    (Server::new(fake).bind().unwrap(), list)
}
