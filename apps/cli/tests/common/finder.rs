//! A skill directory on loopback: a server a test starts on 127.0.0.1, on
//! a port the system picks, which `--finder-url skills.sh=` (or `mindweaver
//! ui`'s) points a search at, answering skills.sh's search
//! (`/api/search`) with what the test hands it, in the shape skills.sh
//! answered one on 2026-11-10, so no search a test runs reaches the
//! network.
//!
//! A finder on this machine may name a folder on it as a result's source,
//! which is how a test takes a result with no network: a repository it made
//! in its temporary folder. D-15 has the owner read skills.sh's terms
//! before the service itself is asked.
//!
//! Standing on its own, the standard library alone, as the registry on
//! loopback does (`mcp_registry.rs`).

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Where skills.sh answers a search, below its address.
pub const SEARCH: &str = "/api/search";

/// skills.sh on loopback, and every request it was sent, by its target.
pub struct SkillsSh {
    address: SocketAddr,
    asked: Arc<Mutex<Vec<String>>>,
}

impl SkillsSh {
    /// A directory answering every search with `skills`, each `(name,
    /// source, installs)`, `source` a repository as skills.sh names one,
    /// `owner/repo`, or a folder on this machine; anything else with a 404.
    pub fn answering(skills: &[(&str, &str, u64)]) -> Self {
        let listed: Vec<String> = skills
            .iter()
            .map(|(name, source, installs)| {
                format!(
                    "{{\"id\": {id}, \"source\": {source}, \"skillId\": {name}, \"name\": {name}, \
                     \"installs\": {installs}}}",
                    id = json(&format!("{source}/{name}")),
                    source = json(source),
                    name = json(name),
                )
            })
            .collect();
        let body = format!(
            "{{\"searchType\": \"fuzzy\", \"skills\": [{}], \"count\": {}}}",
            listed.join(", "),
            skills.len()
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().expect("where it listens");
        let asked = Arc::<Mutex<Vec<String>>>::default();
        let noted = Arc::clone(&asked);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                answer(stream, &body, &noted);
            }
        });
        SkillsSh { address, asked }
    }

    /// Its address, as `--finder-url` takes it after `skills.sh=`.
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// `skills.sh=` and its address: what `--finder-url` is given.
    pub fn finder_url(&self) -> String {
        format!("skills.sh={}", self.url())
    }

    /// Each request it was sent, by its target, path and query, in order.
    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

/// `text` as a JSON string.
fn json(text: &str) -> String {
    serde_json::to_string(text).unwrap()
}

/// Answer one request on `stream` with `body` when it asks for a search,
/// noting its target in `asked`.
fn answer(mut stream: TcpStream, body: &str, asked: &Mutex<Vec<String>>) {
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

    let path = target.split('?').next().unwrap_or_default();
    let (status, body) = if path == SEARCH {
        (200, body)
    } else {
        (404, "{\"error\": \"not found\"}")
    };
    let head = format!(
        "HTTP/1.1 {status} Answered\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\
         connection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(body.as_bytes());
}
