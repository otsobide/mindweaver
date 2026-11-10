//! The `Http` contract: what every adapter a catalog is read through
//! promises, run once against `Reqwest` asking a server this test starts on
//! loopback and once against a site kept in memory, so the promise is the
//! port's rather than one library's. Then core's client through each:
//! redirects, statuses, JSON, pages and the cache, to show that what core
//! makes of an answer is the same whichever adapter brought it.
//!
//! Nothing reaches the network: the server binds 127.0.0.1 on a port the
//! system picks, the adapter reads no proxy, and the fake's sites are names
//! under `example.com` that nothing resolves.

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mindweaver_core::adapter::reqwest::Reqwest;
use mindweaver_core::http::{
    next_link, with_query, Cache, Client, FetchError, Via, MOST_REDIRECTS,
};
use mindweaver_core::mother::{random, MindflayerMother, UrlMother, WorkspaceMother};
use mindweaver_core::port::{Http, HttpError, HttpRequest, HttpResponse, USER_AGENT};
use mindweaver_core::value::Redacted;
use mindweaver_core::Clock;
use serde_json::{json, Value};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// What is asked
// ---------------------------------------------------------------------------

/// How a body comes.
#[derive(Debug, Clone)]
enum Body {
    /// These bytes, their length said first.
    Bytes(Vec<u8>),
    /// Bytes without end, fast, and no length said.
    Endless,
    /// A byte now and then, without end, and no length said.
    Trickle,
}

/// What a site answers a target with.
#[derive(Debug, Clone)]
struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: Body,
    /// How long before anything is answered.
    delay: Duration,
}

impl Answer {
    fn with_status(status: u16) -> Self {
        Answer {
            status,
            headers: Vec::new(),
            body: Body::Bytes(Vec::new()),
            delay: Duration::ZERO,
        }
    }

    fn bytes(bytes: Vec<u8>) -> Self {
        Answer {
            body: Body::Bytes(bytes),
            ..Answer::with_status(200)
        }
    }

    fn json(value: &Value) -> Self {
        Answer::bytes(serde_json::to_vec(value).unwrap()).with("content-type", "application/json")
    }

    fn redirect(status: u16, location: &str) -> Self {
        Answer::with_status(status).with("location", location)
    }

    fn body(self, body: Body) -> Self {
        Answer { body, ..self }
    }

    fn with_status_of(self, status: u16) -> Self {
        Answer { status, ..self }
    }

    fn with(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    fn after(self, delay: Duration) -> Self {
        Answer { delay, ..self }
    }
}

/// A request as a site received it.
#[derive(Debug, Clone)]
struct Asked {
    /// The path and the query.
    target: String,
    /// Each header, its name in lowercase.
    headers: Vec<(String, String)>,
}

impl Asked {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(held, _)| held == name)
            .map(|(_, value)| value.as_str())
    }
}

/// A site to ask, and the adapter that asks it: what a test of the contract
/// asks of each adapter, so the same test runs against all of them.
trait Upstream: Sized {
    /// A site answering nothing but a 404 yet.
    fn start() -> Self;
    /// Another site, on another origin, asked through the same adapter.
    fn beside(&self) -> Self;
    /// The URL of `target`, a path and a query, on this site.
    fn url(&self, target: &str) -> String;
    /// A URL nothing answers at.
    fn nowhere(&self) -> String;
    /// Answer `target` with `answer` from now on.
    fn answer(&self, target: &str, answer: Answer);
    /// What this site was asked, in order.
    fn asked(&self) -> Vec<Asked>;
    /// The adapter under test.
    fn http(&self) -> Arc<dyn Http>;
}

// ---------------------------------------------------------------------------
// A server on loopback, asked by reqwest
// ---------------------------------------------------------------------------

/// A site served on 127.0.0.1, one request a connection, from a thread of
/// its own, asked through `Reqwest`.
struct Loopback {
    address: SocketAddr,
    served: Arc<Served>,
    http: Arc<dyn Http>,
}

#[derive(Default)]
struct Served {
    answers: Mutex<BTreeMap<String, Answer>>,
    asked: Mutex<Vec<Asked>>,
    stopped: AtomicBool,
}

impl Loopback {
    fn listen(http: Arc<dyn Http>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().unwrap();
        let served = Arc::new(Served::default());
        let serving = Arc::clone(&served);
        thread::spawn(move || {
            for stream in listener.incoming() {
                if serving.stopped.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else {
                    continue;
                };
                let serving = Arc::clone(&serving);
                thread::spawn(move || serve(stream, &serving));
            }
        });
        Loopback {
            address,
            served,
            http,
        }
    }
}

impl Drop for Loopback {
    fn drop(&mut self) {
        self.served.stopped.store(true, Ordering::SeqCst);
        // What wakes the thread waiting to accept, to find it is stopped.
        let _ = TcpStream::connect(self.address);
    }
}

/// Answer the one request `stream` carries, as `served` says to.
fn serve(mut stream: TcpStream, served: &Served) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let Some(asked) = received(&mut stream) else {
        return;
    };
    let answer = served
        .answers
        .lock()
        .unwrap()
        .get(&asked.target)
        .cloned()
        .unwrap_or_else(|| Answer::with_status(404));
    served.asked.lock().unwrap().push(asked);
    thread::sleep(answer.delay);

    let mut head = format!(
        "HTTP/1.1 {} Answered\r\nconnection: close\r\n",
        answer.status
    );
    for (name, value) in &answer.headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    match answer.body {
        Body::Bytes(bytes) => {
            head.push_str(&format!("content-length: {}\r\n\r\n", bytes.len()));
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&bytes);
        }
        Body::Endless => {
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            let chunk = vec![b'x'; 64 * 1024];
            // Until the client hangs up, or 256 MiB in case it never does.
            for _ in 0..4096 {
                if stream.write_all(&chunk).is_err() {
                    break;
                }
            }
        }
        Body::Trickle => {
            head.push_str("\r\n");
            let _ = stream.write_all(head.as_bytes());
            // Until the client hangs up, or half a minute in case it never
            // does.
            for _ in 0..1500 {
                if stream.write_all(b"x").and_then(|_| stream.flush()).is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
    let _ = stream.flush();
}

/// The request `stream` carries, its line and its headers.
fn received(stream: &mut TcpStream) -> Option<Asked> {
    let mut bytes = Vec::new();
    let mut byte = [0; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() > 64 * 1024 || stream.read(&mut byte).ok()? == 0 {
            return None;
        }
        bytes.push(byte[0]);
    }
    let text = String::from_utf8(bytes).ok()?;
    let mut lines = text.split("\r\n");
    let target = lines.next()?.split(' ').nth(1)?.to_owned();
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    Some(Asked { target, headers })
}

impl Upstream for Loopback {
    fn start() -> Self {
        Loopback::listen(Arc::new(Reqwest::default()))
    }

    fn beside(&self) -> Self {
        Loopback::listen(Arc::clone(&self.http))
    }

    fn url(&self, target: &str) -> String {
        format!("http://{}{target}", self.address)
    }

    fn nowhere(&self) -> String {
        // A port nothing listens on: bound, then let go.
        let address = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        format!("http://{address}/{}", random::word())
    }

    fn answer(&self, target: &str, answer: Answer) {
        self.served
            .answers
            .lock()
            .unwrap()
            .insert(target.to_owned(), answer);
    }

    fn asked(&self) -> Vec<Asked> {
        self.served.asked.lock().unwrap().clone()
    }

    fn http(&self) -> Arc<dyn Http> {
        Arc::clone(&self.http)
    }
}

// ---------------------------------------------------------------------------
// Sites kept in memory
// ---------------------------------------------------------------------------

/// Sites of a test's own, kept in memory, each origin a map of targets to
/// answers: asked by looking the answer up.
///
/// Written to the contract alone, with no HTTP anywhere in it, so that what
/// the suite holds both adapters to is what the port says. Time is slept
/// rather than spent: a slow answer waits as long as the request would have,
/// at most its timeout.
#[derive(Debug, Default)]
struct Memory {
    sites: Mutex<BTreeMap<String, BTreeMap<String, Answer>>>,
    asked: Mutex<Vec<(String, Asked)>>,
}

impl Http for Memory {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let url = request.url().clone();
        let (origin, target) = split(&url);
        let answer = {
            let sites = self.sites.lock().unwrap();
            let Some(site) = sites.get(&origin) else {
                return Err(HttpError::Connect {
                    url,
                    detail: Redacted::new("no site there"),
                });
            };
            site.get(&target)
                .cloned()
                .unwrap_or_else(|| Answer::with_status(404))
        };
        self.asked.lock().unwrap().push((
            origin.clone(),
            Asked {
                target,
                headers: request
                    .headers()
                    .iter()
                    .map(|header| (header.name().to_owned(), header.value().to_owned()))
                    .collect(),
            },
        ));

        let timeout = request.timeout();
        let timed_out = || HttpError::TimedOut {
            url: request.url().clone(),
            after: timeout,
        };
        if answer.delay >= timeout {
            thread::sleep(timeout);
            return Err(timed_out());
        }
        thread::sleep(answer.delay);
        let body = match answer.body {
            Body::Bytes(bytes) if bytes.len() <= request.limit() => bytes,
            Body::Bytes(_) | Body::Endless => {
                return Err(HttpError::TooLarge {
                    url: request.url().clone(),
                    limit: request.limit(),
                })
            }
            Body::Trickle => {
                thread::sleep(timeout - answer.delay);
                return Err(timed_out());
            }
        };
        let headers = answer.headers.into_iter().map(|(name, value)| {
            if name.eq_ignore_ascii_case("location") && value.starts_with('/') {
                (name, format!("{origin}{value}"))
            } else {
                (name, value)
            }
        });
        Ok(HttpResponse::new(answer.status, headers, body))
    }
}

/// `url`'s origin and its target, the path and the query.
fn split(url: &str) -> (String, String) {
    let (scheme, rest) = url.split_once("://").unwrap();
    let at = rest.find(['/', '?']).unwrap_or(rest.len());
    let target = match &rest[at..] {
        "" => String::from("/"),
        target if target.starts_with('?') => format!("/{target}"),
        target => target.to_owned(),
    };
    (format!("{scheme}://{}", &rest[..at]), target)
}

/// One site of a [`Memory`].
struct Site {
    memory: Arc<Memory>,
    origin: String,
}

impl Site {
    fn on(memory: Arc<Memory>) -> Self {
        let origin = format!("https://{}", UrlMother::host());
        memory
            .sites
            .lock()
            .unwrap()
            .insert(origin.clone(), BTreeMap::new());
        Site { memory, origin }
    }
}

impl Upstream for Site {
    fn start() -> Self {
        Site::on(Arc::new(Memory::default()))
    }

    fn beside(&self) -> Self {
        Site::on(Arc::clone(&self.memory))
    }

    fn url(&self, target: &str) -> String {
        format!("{}{target}", self.origin)
    }

    fn nowhere(&self) -> String {
        format!("https://{}/{}", UrlMother::host(), random::word())
    }

    fn answer(&self, target: &str, answer: Answer) {
        self.memory
            .sites
            .lock()
            .unwrap()
            .get_mut(&self.origin)
            .unwrap()
            .insert(target.to_owned(), answer);
    }

    fn asked(&self) -> Vec<Asked> {
        self.memory
            .asked
            .lock()
            .unwrap()
            .iter()
            .filter(|(origin, _)| *origin == self.origin)
            .map(|(_, asked)| asked.clone())
            .collect()
    }

    fn http(&self) -> Arc<dyn Http> {
        Arc::clone(&self.memory) as Arc<dyn Http>
    }
}

/// Every test named, as a `#[test]` of its own against `$upstream`.
macro_rules! contract {
    ($upstream:ty as $module:ident: $($test:ident),* $(,)?) => {
        mod $module {
            $(
                #[test]
                fn $test() {
                    super::$test::<$upstream>();
                }
            )*
        }
    };
}

/// Every test named, against every adapter. A new adapter joins by an
/// `Upstream` of its own and a line here.
macro_rules! every_upstream {
    ($($test:ident),* $(,)?) => {
        contract!(super::Loopback as through_reqwest: $($test),*);
        contract!(super::Site as through_a_fake: $($test),*);
    };
}

every_upstream![
    a_get_hands_back_the_status_the_headers_core_reads_and_the_body,
    a_status_that_is_no_success_is_answered_like_any_other,
    a_redirect_is_handed_back_unfollowed_with_where_it_leads_made_absolute,
    a_get_says_who_is_asking_and_sends_the_headers_it_was_given,
    a_body_as_long_as_the_limit_is_read_whole,
    a_body_said_to_be_longer_than_the_limit_fails_with_a_code,
    a_body_without_end_fails_at_the_limit_rather_than_read_on,
    an_answer_slower_than_the_timeout_fails_within_about_it,
    a_body_that_trickles_past_the_timeout_fails_within_about_it,
    a_site_nothing_answers_at_fails_to_connect_with_a_code,
    a_redirect_to_the_same_site_is_followed_with_every_header,
    a_redirect_to_another_site_is_followed_without_the_secrets,
    a_redirect_that_never_ends_stops_after_the_most_followed,
    an_answer_that_is_no_success_is_a_failure_by_its_status,
    json_is_read_into_what_is_asked_for,
    json_that_is_not_what_is_asked_for_is_a_failure_by_its_code,
    pages_follow_a_cursor_until_a_page_names_none,
    pages_follow_the_next_link_each_answer_names,
    a_page_that_fails_keeps_the_pages_before_it_and_says_why,
    pages_stop_at_the_most_asked_for_and_say_so,
    a_page_named_twice_is_not_read_again,
    a_next_page_on_another_site_is_not_asked,
    a_fresh_answer_is_answered_from_the_cache_without_asking,
    a_stale_answer_is_asked_again_by_its_tag_and_kept_when_unchanged,
    a_stale_answer_that_changed_is_replaced,
    an_answer_to_a_request_carrying_a_secret_is_never_kept,
    a_kept_answer_holds_only_the_headers_core_reads,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// How long a test of a timeout gives a request, and how much longer than
/// that it may take to fail before the timeout is not what stopped it: a
/// slow answer here is slower than both together.
const TIMEOUT: Duration = Duration::from_millis(300);
const LEEWAY: Duration = Duration::from_secs(3);
const SLOW: Duration = Duration::from_secs(8);

/// A path a catalog could be at.
fn target() -> String {
    format!("/{}/{}", random::word(), random::word())
}

fn request(url: &str) -> HttpRequest {
    HttpRequest::get(url).expect("a URL a request may ask")
}

/// A clock a test moves by hand.
#[derive(Debug)]
struct Hands(AtomicI64);

impl Hands {
    fn new() -> Arc<Self> {
        Arc::new(Hands(AtomicI64::new(1_800_000_000)))
    }

    fn forward(&self, seconds: i64) {
        self.0.fetch_add(seconds, Ordering::SeqCst);
    }
}

impl Clock for Hands {
    fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

/// How long the caches here keep an answer fresh.
const FRESH: Duration = Duration::from_secs(600);

/// A client of `upstream` keeping answers in a workspace of its own, told
/// the time by `clock`.
fn caching<U: Upstream>(upstream: &U, clock: &Arc<Hands>) -> (TempDir, Client, Cache) {
    let (folder, workspace) = WorkspaceMother::empty();
    let cache = Cache::of(&workspace, FRESH).with_clock(Arc::clone(clock) as Arc<dyn Clock>);
    let client = Client::through(upstream.http()).caching(cache.clone());
    (folder, client, cache)
}

/// Every file under `folder`, with what it holds.
fn files_in(folder: &Path) -> Vec<(String, Vec<u8>)> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .map(|entry| {
            let path = entry.unwrap().path();
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&path).unwrap(),
            )
        })
        .collect()
}

/// A page of a catalog: its items and, unless it is the last, the cursor of
/// the next.
fn page(items: &[String], cursor: Option<&str>) -> Value {
    json!({ "items": items, "metadata": { "next_cursor": cursor } })
}

#[derive(Debug, serde::Deserialize)]
struct Page {
    items: Vec<String>,
    metadata: Metadata,
}

#[derive(Debug, serde::Deserialize)]
struct Metadata {
    next_cursor: Option<String>,
}

/// Some pages' items, three to five pages of one to four each.
fn items() -> Vec<Vec<String>> {
    (0..random::between(3, 5))
        .map(|_| {
            (0..random::between(1, 4))
                .map(|_| random::kebab())
                .collect()
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The port
// ---------------------------------------------------------------------------

fn a_get_hands_back_the_status_the_headers_core_reads_and_the_body<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let body = random::bytes(random::between(1, 4096));
    let next = upstream.url(&target());
    upstream.answer(
        &at,
        Answer::bytes(body.clone())
            .with("Content-Type", "application/octet-stream")
            .with("ETag", "\"v1\"")
            .with("Link", &format!("<{next}>; rel=\"next\""))
            .with("Set-Cookie", "session=s3cret")
            .with("X-Other", "1"),
    );

    let response = upstream.http().get(&request(&upstream.url(&at))).unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers(),
        &BTreeMap::from([
            (
                String::from("content-type"),
                String::from("application/octet-stream")
            ),
            (String::from("etag"), String::from("\"v1\"")),
            (String::from("link"), format!("<{next}>; rel=\"next\"")),
        ])
    );
    assert_eq!(response.body(), body.as_slice());
}

fn a_status_that_is_no_success_is_answered_like_any_other<U: Upstream>() {
    let upstream = U::start();
    for status in [404, 429, 500, 503] {
        let at = target();
        let body = random::text(40).into_bytes();
        upstream.answer(&at, Answer::bytes(body.clone()).with_status_of(status));

        let response = upstream.http().get(&request(&upstream.url(&at))).unwrap();

        assert_eq!(response.status(), status);
        assert!(!response.is_success());
        assert_eq!(response.body(), body.as_slice());
    }
}

fn a_redirect_is_handed_back_unfollowed_with_where_it_leads_made_absolute<U: Upstream>() {
    let upstream = U::start();
    let (at, to) = (target(), format!("{}?page=2", target()));
    upstream.answer(&at, Answer::redirect(302, &to));
    upstream.answer(&to, Answer::bytes(b"there".to_vec()));

    let response = upstream.http().get(&request(&upstream.url(&at))).unwrap();

    assert_eq!(response.status(), 302);
    assert!(response.is_redirect());
    assert_eq!(
        response.header("location"),
        Some(upstream.url(&to).as_str())
    );
    let asked: Vec<String> = upstream.asked().into_iter().map(|a| a.target).collect();
    assert_eq!(asked, vec![at]);
}

fn a_get_says_who_is_asking_and_sends_the_headers_it_was_given<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let token = UrlMother::secret();
    upstream.answer(&at, Answer::bytes(Vec::new()));

    upstream
        .http()
        .get(
            &request(&upstream.url(&at))
                .header("Accept", "application/json")
                .unwrap()
                .secret("Authorization", &format!("Bearer {token}"))
                .unwrap(),
        )
        .unwrap();

    let asked = upstream.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].header("user-agent"), Some(USER_AGENT));
    assert_eq!(asked[0].header("accept"), Some("application/json"));
    assert_eq!(
        asked[0].header("authorization"),
        Some(format!("Bearer {token}").as_str())
    );
    assert_eq!(asked[0].header("cookie"), None);
}

fn a_body_as_long_as_the_limit_is_read_whole<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let limit = random::between(1, 300_000);
    let body = random::bytes(limit);
    upstream.answer(&at, Answer::bytes(body.clone()));

    let response = upstream
        .http()
        .get(&request(&upstream.url(&at)).at_most(limit))
        .unwrap();

    assert_eq!(response.body(), body.as_slice());
}

fn a_body_said_to_be_longer_than_the_limit_fails_with_a_code<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let limit = random::between(1, 300_000);
    upstream.answer(&at, Answer::bytes(random::bytes(limit + 1)));
    let url = upstream.url(&at);

    let error = upstream
        .http()
        .get(&request(&url).at_most(limit))
        .unwrap_err();

    assert!(
        matches!(&error, HttpError::TooLarge { limit: said, .. } if *said == limit),
        "{error:?}"
    );
    assert_eq!(error.code(), "http_too_large");
    assert!(error.to_string().starts_with(&url), "{error}");
}

fn a_body_without_end_fails_at_the_limit_rather_than_read_on<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let limit = random::between(1, 300_000);
    upstream.answer(&at, Answer::bytes(Vec::new()).body(Body::Endless));
    let started = Instant::now();

    let error = upstream
        .http()
        .get(&request(&upstream.url(&at)).at_most(limit))
        .unwrap_err();

    assert_eq!(error.code(), "http_too_large", "{error:?}");
    assert!(started.elapsed() < LEEWAY, "{:?}", started.elapsed());
}

fn an_answer_slower_than_the_timeout_fails_within_about_it<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    upstream.answer(&at, Answer::bytes(Vec::new()).after(SLOW));
    let url = upstream.url(&at);
    let started = Instant::now();

    let error = upstream
        .http()
        .get(&request(&url).within(TIMEOUT))
        .unwrap_err();

    let took = started.elapsed();
    assert!(
        matches!(&error, HttpError::TimedOut { after, .. } if *after == TIMEOUT),
        "{error:?}"
    );
    assert_eq!(error.code(), "http_timed_out");
    assert!(error.to_string().starts_with(&url), "{error}");
    assert!(took >= TIMEOUT && took < TIMEOUT + LEEWAY, "{took:?}");
}

fn a_body_that_trickles_past_the_timeout_fails_within_about_it<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    upstream.answer(&at, Answer::bytes(Vec::new()).body(Body::Trickle));
    let started = Instant::now();

    let error = upstream
        .http()
        .get(&request(&upstream.url(&at)).within(TIMEOUT))
        .unwrap_err();

    let took = started.elapsed();
    assert_eq!(error.code(), "http_timed_out", "{error:?}");
    assert!(took < TIMEOUT + LEEWAY, "{took:?}");
}

fn a_site_nothing_answers_at_fails_to_connect_with_a_code<U: Upstream>() {
    let upstream = U::start();
    let url = upstream.nowhere();

    let error = upstream.http().get(&request(&url)).unwrap_err();

    assert_eq!(error.code(), "http_connect_failed", "{error:?}");
    assert!(error.to_string().starts_with(&url), "{error}");
}

// ---------------------------------------------------------------------------
// Redirects, statuses and JSON, through core's client
// ---------------------------------------------------------------------------

fn a_redirect_to_the_same_site_is_followed_with_every_header<U: Upstream>() {
    let upstream = U::start();
    let (at, to) = (target(), target());
    let token = UrlMother::secret();
    upstream.answer(&at, Answer::redirect(301, &upstream.url(&to)));
    upstream.answer(&to, Answer::bytes(b"moved".to_vec()));

    let fetched = Client::through(upstream.http())
        .get(
            &request(&upstream.url(&at))
                .secret("authorization", &token)
                .unwrap(),
        )
        .unwrap();

    assert_eq!(fetched.value.body(), b"moved");
    assert_eq!(fetched.via, Via::Network);
    let asked = upstream.asked();
    assert_eq!(asked.len(), 2);
    assert_eq!(asked[1].target, to);
    assert_eq!(asked[1].header("authorization"), Some(token.as_str()));
}

fn a_redirect_to_another_site_is_followed_without_the_secrets<U: Upstream>() {
    let upstream = U::start();
    let elsewhere = upstream.beside();
    let (at, to) = (target(), target());
    let token = UrlMother::secret();
    upstream.answer(&at, Answer::redirect(307, &elsewhere.url(&to)));
    elsewhere.answer(&to, Answer::bytes(b"moved".to_vec()));

    let fetched = Client::through(upstream.http())
        .get(
            &request(&upstream.url(&at))
                .header("accept", "application/json")
                .unwrap()
                .secret("x-api-key", &token)
                .unwrap()
                .header("authorization", &format!("Bearer {token}"))
                .unwrap(),
        )
        .unwrap();

    assert_eq!(fetched.value.body(), b"moved");
    let asked = elsewhere.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].header("accept"), Some("application/json"));
    assert_eq!(asked[0].header("x-api-key"), None);
    assert_eq!(asked[0].header("authorization"), None);
    assert!(
        asked[0]
            .headers
            .iter()
            .all(|(_, value)| !value.contains(&token)),
        "{asked:?}"
    );
}

fn a_redirect_that_never_ends_stops_after_the_most_followed<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    upstream.answer(&at, Answer::redirect(302, &at));
    let url = upstream.url(&at);

    let error = Client::through(upstream.http())
        .get(&request(&url))
        .unwrap_err();

    assert!(
        matches!(&error, FetchError::Redirects { most, .. } if *most == MOST_REDIRECTS),
        "{error:?}"
    );
    assert_eq!(error.code(), "http_redirects_exceeded");
    assert!(error.to_string().starts_with(&url), "{error}");
    assert_eq!(upstream.asked().len(), MOST_REDIRECTS + 1);
}

fn an_answer_that_is_no_success_is_a_failure_by_its_status<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    // A 429 is a rate limit, whatever it says, which is said by name.
    let status = random::pick(&[400, 401, 403, 404, 410, 500, 502, 503]);
    upstream.answer(&at, Answer::bytes(b"no".to_vec()).with_status_of(status));
    let url = upstream.url(&at);

    let error = Client::through(upstream.http())
        .get(&request(&url))
        .unwrap_err();

    assert!(
        matches!(&error, FetchError::Status { status: said, .. } if *said == status),
        "{error:?}"
    );
    assert_eq!(error.code(), "http_status_unsuccessful");
    assert_eq!(
        error.to_string(),
        format!("{url}: answered with status {status}")
    );
}

fn json_is_read_into_what_is_asked_for<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let items: Vec<String> = (0..random::between(0, 5))
        .map(|_| random::kebab())
        .collect();
    upstream.answer(&at, Answer::json(&page(&items, None)));

    let fetched = Client::through(upstream.http())
        .json::<Page>(&request(&upstream.url(&at)))
        .unwrap();

    assert_eq!(fetched.value.items, items);
    assert_eq!(fetched.value.metadata.next_cursor, None);
}

fn json_that_is_not_what_is_asked_for_is_a_failure_by_its_code<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    upstream.answer(&at, Answer::json(&json!({ "servers": [] })));
    let url = upstream.url(&at);

    let error = Client::through(upstream.http())
        .json::<Page>(&request(&url))
        .unwrap_err();

    assert_eq!(error.code(), "http_json_invalid", "{error:?}");
    assert!(error.to_string().starts_with(&url), "{error}");
}

// ---------------------------------------------------------------------------
// Pages
// ---------------------------------------------------------------------------

/// The URL of the page with `cursor`, as a catalog pages by one.
fn at_cursor(first: &str, cursor: &str) -> String {
    with_query(first, "cursor", cursor)
}

/// A catalog at `at` of `items` a page, paged by a cursor that needs
/// encoding: the cursors, in order.
fn catalog<U: Upstream>(upstream: &U, at: &str, items: &[Vec<String>]) -> Vec<String> {
    let first = upstream.url(at);
    let cursors: Vec<String> = (1..items.len())
        .map(|n| format!("{n}/{}+{}=&", random::word(), random::word()))
        .collect();
    for (n, page_items) in items.iter().enumerate() {
        let url = if n == 0 {
            first.clone()
        } else {
            at_cursor(&first, &cursors[n - 1])
        };
        let target = &url[url.find(at).unwrap()..];
        upstream.answer(
            target,
            Answer::json(&page(page_items, cursors.get(n).map(String::as_str))),
        );
    }
    cursors
}

fn by_cursor(first: &str) -> impl FnMut(&Page, &HttpResponse) -> Option<String> + '_ {
    |page, _| {
        page.metadata
            .next_cursor
            .as_deref()
            .map(|cursor| at_cursor(first, cursor))
    }
}

fn pages_follow_a_cursor_until_a_page_names_none<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let items = items();
    catalog(&upstream, &at, &items);
    let first = upstream.url(&at);

    let paged = Client::through(upstream.http()).pages(&request(&first), 10, by_cursor(&first));

    assert!(paged.failure.is_none(), "{:?}", paged.failure);
    let read: Vec<Vec<String>> = paged.pages.into_iter().map(|page| page.items).collect();
    assert_eq!(read, items);
    assert_eq!(upstream.asked().len(), items.len());
}

fn pages_follow_the_next_link_each_answer_names<U: Upstream>() {
    let upstream = U::start();
    let items = items();
    let targets: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(n, _)| format!("/search?q={}&page={}", random::word(), n + 1))
        .collect();
    for (n, page_items) in items.iter().enumerate() {
        let mut answer = Answer::json(&json!(page_items));
        if let Some(next) = targets.get(n + 1) {
            let last = upstream.url(targets.last().unwrap());
            answer = answer.with(
                "link",
                &format!(
                    "<{}>; rel=\"next\", <{last}>; rel=\"last\"",
                    upstream.url(next)
                ),
            );
        }
        upstream.answer(&targets[n], answer);
    }

    let paged = Client::through(upstream.http()).pages::<Vec<String>>(
        &request(&upstream.url(&targets[0])),
        10,
        |_, response| next_link(response).map(str::to_owned),
    );

    assert!(paged.failure.is_none(), "{:?}", paged.failure);
    assert_eq!(paged.pages, items);
}

fn a_page_that_fails_keeps_the_pages_before_it_and_says_why<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let items = items();
    let cursors = catalog(&upstream, &at, &items);
    let first = upstream.url(&at);
    let failing = at_cursor(&first, &cursors[1]);
    upstream.answer(
        &failing[failing.find(&at).unwrap()..],
        Answer::with_status(503),
    );

    let paged = Client::through(upstream.http()).pages(&request(&first), 10, by_cursor(&first));

    let read: Vec<Vec<String>> = paged.pages.into_iter().map(|page| page.items).collect();
    assert_eq!(read, items[..2].to_vec());
    let failure = paged.failure.expect("the third page failed");
    assert_eq!(failure.code(), "http_status_unsuccessful");
    assert!(failure.to_string().starts_with(&failing), "{failure}");
}

fn pages_stop_at_the_most_asked_for_and_say_so<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let items = items();
    catalog(&upstream, &at, &items);
    let first = upstream.url(&at);
    let most = items.len() - 1;

    let paged = Client::through(upstream.http()).pages(&request(&first), most, by_cursor(&first));

    assert_eq!(paged.pages.len(), most);
    let failure = paged.failure.expect("there was a page more");
    assert!(
        matches!(&failure, FetchError::Pages { most: said, .. } if *said == most),
        "{failure:?}"
    );
    assert_eq!(failure.code(), "http_pages_exceeded");
    assert_eq!(upstream.asked().len(), most);
}

fn a_page_named_twice_is_not_read_again<U: Upstream>() {
    let upstream = U::start();
    let (at, second) = (target(), target());
    let items = items();
    upstream.answer(
        &at,
        Answer::json(&json!(items[0]))
            .with("link", &format!("<{}>; rel=next", upstream.url(&second))),
    );
    upstream.answer(
        &second,
        Answer::json(&json!(items[1])).with("link", &format!("<{}>; rel=next", upstream.url(&at))),
    );

    let paged = Client::through(upstream.http()).pages::<Vec<String>>(
        &request(&upstream.url(&at)),
        10,
        |_, response| next_link(response).map(str::to_owned),
    );

    assert_eq!(paged.pages, items[..2].to_vec());
    let failure = paged.failure.expect("the second page named the first");
    assert_eq!(failure.code(), "http_page_repeated", "{failure:?}");
    assert_eq!(upstream.asked().len(), 2);
}

fn a_next_page_on_another_site_is_not_asked<U: Upstream>() {
    let upstream = U::start();
    let elsewhere = upstream.beside();
    let (at, there) = (target(), target());
    let token = UrlMother::secret();
    upstream.answer(
        &at,
        Answer::json(&json!(["one"])).with(
            "link",
            &format!("<{}>; rel=\"next\"", elsewhere.url(&there)),
        ),
    );
    elsewhere.answer(&there, Answer::json(&json!(["two"])));

    let paged = Client::through(upstream.http()).pages::<Vec<String>>(
        &request(&upstream.url(&at))
            .secret("authorization", &token)
            .unwrap(),
        10,
        |_, response| next_link(response).map(str::to_owned),
    );

    assert_eq!(paged.pages, vec![vec![String::from("one")]]);
    let failure = paged.failure.expect("the next page was elsewhere");
    assert_eq!(failure.code(), "http_page_elsewhere", "{failure:?}");
    assert!(elsewhere.asked().is_empty());
}

// ---------------------------------------------------------------------------
// The cache
// ---------------------------------------------------------------------------

fn a_fresh_answer_is_answered_from_the_cache_without_asking<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let body = random::bytes(random::between(1, 2048));
    upstream.answer(&at, Answer::bytes(body.clone()));
    let clock = Hands::new();
    let (_workspace, client, _) = caching(&upstream, &clock);
    let asked = request(&upstream.url(&at));

    let first = client.get(&asked).unwrap();
    clock.forward(FRESH.as_secs() as i64 - 1);
    upstream.answer(&at, Answer::bytes(b"changed".to_vec()));
    let second = client.get(&asked).unwrap();

    assert_eq!(first.via, Via::Network);
    assert_eq!(second.via, Via::Cache);
    assert_eq!(second.value.body(), body.as_slice());
    assert!(second.warnings.is_empty(), "{:?}", second.warnings);
    assert_eq!(upstream.asked().len(), 1);
}

fn a_stale_answer_is_asked_again_by_its_tag_and_kept_when_unchanged<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let body = random::bytes(random::between(1, 2048));
    let tag = format!("\"{}\"", random::hex(16));
    let when = "Wed, 21 Oct 2026 07:28:00 GMT";
    upstream.answer(
        &at,
        Answer::bytes(body.clone())
            .with("etag", &tag)
            .with("last-modified", when),
    );
    let clock = Hands::new();
    let (_workspace, client, _) = caching(&upstream, &clock);
    let asked = request(&upstream.url(&at));

    client.get(&asked).unwrap();
    clock.forward(FRESH.as_secs() as i64);
    upstream.answer(&at, Answer::with_status(304).with("etag", &tag));
    let again = client.get(&asked).unwrap();
    clock.forward(FRESH.as_secs() as i64 - 1);
    let fresh_again = client.get(&asked).unwrap();

    assert_eq!(again.via, Via::Revalidated);
    assert_eq!(again.value.body(), body.as_slice());
    assert_eq!(fresh_again.via, Via::Cache, "kept again as of the 304");
    let seen = upstream.asked();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].header("if-none-match"), None);
    assert_eq!(seen[1].header("if-none-match"), Some(tag.as_str()));
    assert_eq!(seen[1].header("if-modified-since"), Some(when));
}

fn a_stale_answer_that_changed_is_replaced<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    upstream.answer(&at, Answer::bytes(b"old".to_vec()).with("etag", "\"1\""));
    let clock = Hands::new();
    let (_workspace, client, _) = caching(&upstream, &clock);
    let asked = request(&upstream.url(&at));

    client.get(&asked).unwrap();
    clock.forward(FRESH.as_secs() as i64 + 1);
    upstream.answer(&at, Answer::bytes(b"new".to_vec()).with("etag", "\"2\""));
    let changed = client.get(&asked).unwrap();
    let kept = client.get(&asked).unwrap();

    assert_eq!(changed.via, Via::Network);
    assert_eq!(changed.value.body(), b"new");
    assert_eq!(kept.via, Via::Cache);
    assert_eq!(kept.value.body(), b"new");
    assert_eq!(kept.value.header("etag"), Some("\"2\""));
}

fn an_answer_to_a_request_carrying_a_secret_is_never_kept<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let token = UrlMother::secret();
    upstream.answer(&at, Answer::bytes(b"private".to_vec()));
    let clock = Hands::new();
    let (_workspace, client, cache) = caching(&upstream, &clock);

    for asked in [
        request(&upstream.url(&at))
            .secret("x-api-key", &token)
            .unwrap(),
        request(&upstream.url(&at))
            .header("authorization", &format!("Bearer {token}"))
            .unwrap(),
        request(&upstream.url(&at))
            .header("cookie", &format!("session={token}"))
            .unwrap(),
    ] {
        let first = client.get(&asked).unwrap();
        let second = client.get(&asked).unwrap();
        assert_eq!((first.via, second.via), (Via::Network, Via::Network));
    }

    assert_eq!(upstream.asked().len(), 6);
    assert!(files_in(cache.folder()).is_empty());
}

fn a_kept_answer_holds_only_the_headers_core_reads<U: Upstream>() {
    let upstream = U::start();
    let at = target();
    let cookie = UrlMother::secret();
    upstream.answer(
        &at,
        Answer::json(&json!(["one"]))
            .with("set-cookie", &format!("session={cookie}"))
            .with("x-request-id", "4f9c2a7e"),
    );
    let clock = Hands::new();
    let (_workspace, client, cache) = caching(&upstream, &clock);

    client.get(&request(&upstream.url(&at))).unwrap();

    let files = files_in(cache.folder());
    assert_eq!(
        files.len(),
        1,
        "{:?}",
        files.iter().map(|f| &f.0).collect::<Vec<_>>()
    );
    let (name, bytes) = &files[0];
    assert!(
        name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()),
        "{name}"
    );
    let text = String::from_utf8_lossy(bytes);
    assert!(!text.contains(&cookie), "{text}");
    assert!(!text.contains("x-request-id"), "{text}");
    assert!(text.contains(&upstream.url(&at)), "{text}");
    assert!(text.ends_with(r#"["one"]"#), "{text}");
}

// ---------------------------------------------------------------------------
// What only a fake can stand for
// ---------------------------------------------------------------------------

/// The server on loopback speaks plain HTTP alone, so the redirect from
/// `https` it would take is the fake's to show: core follows none, whichever
/// adapter handed it back.
#[test]
fn a_redirect_from_https_to_plain_http_is_not_followed() {
    let upstream = Site::start();
    let at = target();
    let to = format!("http://{}{}", UrlMother::host(), target());
    upstream.answer(&at, Answer::redirect(301, &to));
    let url = upstream.url(&at);

    let error = Client::through(upstream.http())
        .get(&request(&url))
        .unwrap_err();

    assert!(
        matches!(&error, FetchError::Downgraded { to: said, .. } if **said == to),
        "{error:?}"
    );
    assert_eq!(error.code(), "http_redirect_downgraded");
    assert_eq!(upstream.asked().len(), 1);
}

/// A redirect is asked within what is left of the request's time, so a
/// chain of slow ones ends when the first request would have.
#[test]
fn redirects_share_the_time_the_request_was_given() {
    let upstream = Site::start();
    let (at, to) = (target(), target());
    let half = TIMEOUT / 2 + Duration::from_millis(50);
    upstream.answer(&at, Answer::redirect(302, &to).after(half));
    upstream.answer(&to, Answer::bytes(Vec::new()).after(half));
    let started = Instant::now();

    let error = Client::through(upstream.http())
        .get(&request(&upstream.url(&at)).within(TIMEOUT))
        .unwrap_err();

    assert_eq!(error.code(), "http_timed_out", "{error:?}");
    assert!(
        started.elapsed() < TIMEOUT + LEEWAY,
        "{:?}",
        started.elapsed()
    );
}

/// The cache keeps nothing anywhere but the workspace's own folder for it,
/// under the names the workspace goes by: a Mindflayer build's keeps its
/// answers in its `.mindflayer`, never in a `.mindweaver` beside it.
#[test]
fn the_cache_is_the_workspaces_own_under_the_names_it_goes_by() {
    let (_mindweaver, workspace) = WorkspaceMother::empty();
    let (_mindflayer, older) = MindflayerMother::empty();

    for (workspace, folder) in [(workspace, ".mindweaver"), (older, ".mindflayer")] {
        let cache = Cache::of(&workspace, FRESH);
        assert_eq!(
            cache.folder(),
            workspace.root().join(folder).join("cache").join("http")
        );
        assert_eq!(cache.fresh_for(), FRESH);
    }
}
