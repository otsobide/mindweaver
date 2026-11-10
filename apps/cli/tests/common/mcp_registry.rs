//! The official MCP Registry on loopback: a server a test starts on
//! 127.0.0.1, on a port the system picks, which `--registry` points a
//! command at as at a mirror. It answers with what the real registry sent
//! on 2026-11-10, recorded once by #106 into core's
//! `tests/fixtures/mcp-registry`, so no command a test runs reaches the
//! network:
//!
//! - one version of a server, by the answer recorded for it, and its
//!   `latest` by the answer of the version the registry called its latest
//!   when it was recorded, which `clawfetch-latest.json` shows it answers
//!   byte for byte; a version nobody recorded by the registry's own 404;
//! - the search for `airtable` by its three recorded pages, each found by
//!   the cursor it was asked with, whatever `limit` asked for, as the
//!   registry pages one search; and any other search by one page holding
//!   each recorded latest version whose name holds the query, as the
//!   registry matches a search, each server as it was recorded.
//!
//! Standing on its own, the standard library alone, so the CLI suites that
//! share `common` and the end-to-end suite, which does not, both take it.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Each recorded answer of one version of a server: its file, the name
/// the registry gives the server, its version, and whether it was the
/// latest when it was recorded.
const SERVERS: &[(&str, &str, &str, bool)] = &[
    ("clawfetch-0.4.0.json", "ai.clawfetch/mcp", "0.4.0", true),
    ("loice-mcp-1.0.1.json", "ai.loice/mcp", "1.0.1", false),
    (
        "infrahub-0.1.2.json",
        "com.opsmill/infrahub-mcp",
        "0.1.2",
        true,
    ),
    (
        "buildkite-0.7.0.json",
        "io.github.buildkite/buildkite-mcp-server",
        "0.7.0",
        true,
    ),
    (
        "internet-monitor-1.0.2.json",
        "app.internetmonitor/internet-monitor",
        "1.0.2",
        true,
    ),
    ("biel-ai-2.0.0.json", "ai.biel/biel-ai", "2.0.0", true),
    (
        "devmargin-1.0.0.json",
        "app.devmargin/devmargin",
        "1.0.0",
        true,
    ),
    (
        "greenlight-1.0.0.json",
        "ai.agentlookups/greenlight",
        "1.0.0",
        true,
    ),
    ("anzenna-0.1.json", "ai.anzenna/anzenna", "0.1", true),
];

/// Each recorded page of the search for `airtable`, by the cursor it was
/// asked with.
const AIRTABLE: &[(Option<&str>, &str)] = &[
    (None, "search-airtable-1.json"),
    (
        Some("com.mcparmory/airtable:1.0.3"),
        "search-airtable-2.json",
    ),
    (
        Some("io.github.pipeworx-io/airtable:0.2.1"),
        "search-airtable-3.json",
    ),
];

/// What the registry answered a version it does not hold.
const UNKNOWN: &str = "loice-telegram-1.0.1.json";

/// The registry on loopback, and every request it was sent, by its target.
pub struct Registry {
    address: SocketAddr,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Registry {
    /// A registry answering what the real one answered, until the test
    /// ends.
    pub fn recorded() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().expect("where it listens");
        let asked = Arc::<Mutex<Vec<String>>>::default();
        let noted = Arc::clone(&asked);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                answer(stream, &noted);
            }
        });
        Registry { address, asked }
    }

    /// Its address, as `--registry` takes it.
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Each request it was sent, by its target, path and query, in order.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }

    /// Each request it was sent, by its path alone.
    pub fn paths(&self) -> Vec<String> {
        self.asked()
            .iter()
            .map(|target| target.split('?').next().unwrap_or_default().to_owned())
            .collect()
    }
}

/// Where core's suite keeps the recorded answers.
fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../core/tests/fixtures/mcp-registry")
}

/// A recorded answer, as it was recorded.
fn fixture(file: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(file)).expect("a recorded answer")
}

/// Answer one request on `stream`, noting its target in `asked`.
fn answer(mut stream: TcpStream, asked: &Mutex<Vec<String>>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    while !request.windows(4).any(|end| end == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
    let request = String::from_utf8_lossy(&request).into_owned();
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .unwrap_or_default()
        .to_owned();
    asked.lock().unwrap().push(target.clone());

    let (status, body) = answered(&target);
    let head = format!(
        "HTTP/1.1 {status} Answered\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&body);
}

/// The status and the body the registry answers `target` with.
fn answered(target: &str) -> (u16, Vec<u8>) {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if path == "/v0.1/servers" {
        let search = parameter(query, "search").unwrap_or_default();
        let cursor = parameter(query, "cursor");
        if search == "airtable" {
            let page = AIRTABLE
                .iter()
                .find(|(at, _)| at.map(str::to_owned) == cursor)
                .map_or(AIRTABLE[2].1, |(_, file)| file);
            return (200, fixture(page));
        }
        return (200, page_of(&search));
    }
    let found = SERVERS.iter().find(|(_, name, version, latest)| {
        let asked = |version: &str| {
            format!(
                "/v0.1/servers/{}/versions/{version}",
                name.replace('/', "%2F")
            )
        };
        path == asked(version) || (*latest && path == asked("latest"))
    });
    match found {
        Some((file, ..)) => (200, fixture(file)),
        None => (404, fixture(UNKNOWN)),
    }
}

/// One page holding each recorded latest version whose name holds
/// `search`, in any case, each as it was recorded.
fn page_of(search: &str) -> Vec<u8> {
    let search = search.to_lowercase();
    let servers: Vec<Vec<u8>> = SERVERS
        .iter()
        .filter(|(_, name, _, latest)| *latest && name.to_lowercase().contains(&search))
        .map(|(file, ..)| {
            let mut body = fixture(file);
            while body.last().is_some_and(u8::is_ascii_whitespace) {
                body.pop();
            }
            body
        })
        .collect();
    let mut page = b"{\"servers\":[".to_vec();
    page.extend(servers.join(&b","[..]));
    page.extend(format!("],\"metadata\":{{\"count\":{}}}}}", servers.len()).into_bytes());
    page
}

/// The value of `name` in `query`, decoded.
fn parameter(query: &str, name: &str) -> Option<String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == name)
        .map(|(_, value)| decoded(value))
}

/// `text` with each `%XX` it holds read as the byte it stands for.
fn decoded(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let hex = bytes
            .get(at + 1..at + 3)
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match (bytes[at], hex) {
            (b'%', Some(decoded)) => {
                out.push(decoded);
                at += 3;
            }
            (b'+', _) => {
                out.push(b' ');
                at += 1;
            }
            (byte, _) => {
                out.push(byte);
                at += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
