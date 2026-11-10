//! The answers a workspace keeps of what catalogs said over HTTP, in its
//! `.mindweaver/cache/http` (`.mindflayer/cache/http` in one a Mindflayer
//! build made), so a listing read twice in a row asks the server once.
//!
//! Held to what the rest of the workspace's folder is held to:
//!
//! - **No credential is kept.** An answer to a request carrying a secret is
//!   never kept ([`super::Client`] does not ask the cache for one), a URL
//!   carrying a credential is never asked at all ([`HttpRequest::get`]), and
//!   of an answer only the headers core reads are kept
//!   ([`crate::port::KEPT_HEADERS`]), a `Set-Cookie` never among them.
//! - **Each answer is one file**, named by the SHA-256 of what tells its
//!   request from another ([`hash::request`]), so a file's name says nothing
//!   a URL said and every filesystem takes it. It holds one line of JSON (its
//!   [`CACHE_FORMAT`], the URL asked, when it was kept, the status and the
//!   headers), then the body as it came.
//! - **Written whole or not at all**, through a temporary renamed onto it,
//!   so a reader never finds half an answer and two writers at once leave
//!   one whole one. That is also why it takes no lock: the workspace's is
//!   for changes that span several files, and a command reading a catalog
//!   should not wait for an install to finish, nor hold one up.
//! - **A link is never followed**: a folder that is one is said and neither
//!   read nor written, and a file that is one is read as nothing and
//!   replaced, never written through.
//! - **A format newer than this build's** is left as it is, neither read
//!   nor written over, as every format core reads is
//!   (docs/architecture.md, "Formats are versioned").
//!
//! What cannot be read or written is a warning beside the answer
//! ([`CacheError`]), never a failure: the cache only saves asking again.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::adapter::SystemClock;
use crate::code::codes;
use crate::hash;
use crate::port::{Clock, HttpRequest, HttpResponse};
use crate::value::ContentHash;
use crate::workspace::{replace_bytes_unresolved, WeaverWorkspace};

/// The format of a kept answer this build writes, in the line it starts
/// with. One a newer build wrote is left alone.
pub const CACHE_FORMAT: u32 = 1;

/// How long the line before a kept body may be: a URL and the few headers
/// kept, which never come near it.
const HEAD_LIMIT: usize = 64 * 1024;

/// Where a workspace keeps answers, and for how long one is answered from
/// there without asking.
#[derive(Debug, Clone)]
pub struct Cache {
    folder: PathBuf,
    fresh_for: Duration,
    clock: Arc<dyn Clock>,
}

impl Cache {
    /// The cache of `workspace` ([`WeaverWorkspace::http_cache_dir`]), an
    /// answer in which is answered from there for `fresh_for` after it was
    /// kept, and asked again after that.
    ///
    /// Made only for a workspace, so an answer is never kept anywhere but in
    /// the folder that is Mindweaver's.
    pub fn of(workspace: &WeaverWorkspace, fresh_for: Duration) -> Self {
        Cache {
            folder: workspace.http_cache_dir(),
            fresh_for,
            clock: Arc::new(SystemClock),
        }
    }

    /// This cache telling how old an answer is by `clock`, in place of the
    /// system's.
    pub fn with_clock(self, clock: Arc<dyn Clock>) -> Self {
        Cache { clock, ..self }
    }

    /// The folder it keeps answers in, whether or not it exists yet.
    pub fn folder(&self) -> &Path {
        &self.folder
    }

    /// How long an answer is answered from here without asking.
    pub fn fresh_for(&self) -> Duration {
        self.fresh_for
    }

    /// Whether an answer kept at `stored` is answered from here now.
    ///
    /// One kept later than now, by a clock that has since gone back, is
    /// not: asking again costs one request, and trusting it could keep it
    /// for as long as the clock is wrong.
    pub(super) fn is_fresh(&self, stored: i64) -> bool {
        let now = self.clock.now();
        stored <= now && now.abs_diff(stored) < self.fresh_for.as_secs()
    }

    /// What is kept for `request`.
    pub(super) fn find(&self, request: &HttpRequest, warnings: &mut Vec<CacheError>) -> Found {
        match fs::symlink_metadata(&self.folder) {
            Ok(folder) if folder.file_type().is_symlink() => {
                warnings.push(CacheError::Linked {
                    path: self.folder.clone(),
                });
                return Found::Barred;
            }
            Ok(_) => {}
            // Not made yet, or not readable, which keeping an answer will
            // meet and say.
            Err(_) => return Found::Nothing,
        }
        let path = self.path(request);
        let unreadable = |detail: String| CacheError::Unreadable {
            path: path.clone(),
            detail,
        };
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Found::Nothing,
            Err(error) => {
                warnings.push(unreadable(error.to_string()));
                return Found::Nothing;
            }
            Ok(file) if !file.is_file() => {
                warnings.push(unreadable(String::from("it is not a file")));
                return Found::Nothing;
            }
            Ok(_) => {}
        }

        // No more than an answer this request takes could hold: a kept one
        // longer than that is not one to hand it, and is asked again.
        let most = request.limit().saturating_add(HEAD_LIMIT);
        let mut bytes = Vec::new();
        let read = fs::File::open(&path).and_then(|file| {
            file.take(most.saturating_add(1) as u64)
                .read_to_end(&mut bytes)
        });
        if let Err(error) = read {
            warnings.push(unreadable(error.to_string()));
            return Found::Nothing;
        }
        if bytes.len() > most {
            return Found::Nothing;
        }
        match read_kept(&bytes, request) {
            Ok(found) => found,
            Err(detail) => {
                warnings.push(unreadable(detail));
                Found::Nothing
            }
        }
    }

    /// Keep `response` as what `request` is answered, as of now.
    pub(super) fn keep(
        &self,
        request: &HttpRequest,
        response: &HttpResponse,
        warnings: &mut Vec<CacheError>,
    ) {
        if let Err(warning) = self.made() {
            warnings.push(warning);
            return;
        }
        let path = self.path(request);
        let head = Head {
            format: CACHE_FORMAT,
            url: request.url().to_string(),
            stored: self.clock.now(),
            status: response.status(),
            headers: response.headers().clone(),
        };
        let mut bytes = match serde_json::to_vec(&head) {
            Ok(bytes) => bytes,
            Err(error) => {
                warnings.push(CacheError::Unwritable {
                    path,
                    source: io::Error::other(error),
                });
                return;
            }
        };
        bytes.push(b'\n');
        bytes.extend_from_slice(response.body());
        if let Err((path, source)) = replace_bytes_unresolved(&path, &bytes) {
            warnings.push(CacheError::Unwritable { path, source });
        }
    }

    /// The folder, made if it is not there, unless it is a link.
    fn made(&self) -> Result<(), CacheError> {
        let unwritable = |source: io::Error| CacheError::Unwritable {
            path: self.folder.clone(),
            source,
        };
        match fs::symlink_metadata(&self.folder) {
            Ok(folder) if folder.file_type().is_symlink() => Err(CacheError::Linked {
                path: self.folder.clone(),
            }),
            Ok(folder) if folder.is_dir() => Ok(()),
            Ok(_) => Err(unwritable(io::Error::other("it is not a folder"))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                fs::create_dir_all(&self.folder).map_err(unwritable)
            }
            Err(error) => Err(unwritable(error)),
        }
    }

    /// The file an answer to `request` is kept in.
    pub(super) fn path(&self, request: &HttpRequest) -> PathBuf {
        self.folder.join(key(request).as_str())
    }
}

/// What the cache holds for a request.
pub(super) enum Found {
    /// Nothing that can be answered: no file, or one for another request,
    /// or one that cannot be read, which an answer replaces.
    Nothing,
    /// The folder is a link: nothing is read or kept.
    Barred,
    /// An answer a newer build kept, in a format this one does not read,
    /// left as it is.
    Newer,
    /// An answer, and when it was kept.
    Kept { response: HttpResponse, stored: i64 },
}

impl Found {
    /// Whether an answer may be kept in its place.
    pub(super) fn may_be_replaced(&self) -> bool {
        !matches!(self, Found::Barred | Found::Newer)
    }

    /// `request`, asking the server to say whether what is kept has
    /// changed, by the tag or the time it was answered with, when there is
    /// something kept that says either.
    ///
    /// A tag or a time a server sent that no header could carry is not
    /// sent back: the answer is asked whole instead.
    pub(super) fn revalidating(&self, request: &HttpRequest) -> HttpRequest {
        let Found::Kept { response, .. } = self else {
            return request.clone();
        };
        let mut asked = request.clone();
        for (kept, conditional) in [
            ("etag", "if-none-match"),
            ("last-modified", "if-modified-since"),
        ] {
            if let Some(value) = response.header(kept) {
                if let Ok(conditioned) = asked.clone().header(conditional, value) {
                    asked = conditioned;
                }
            }
        }
        asked
    }
}

/// The line a kept answer starts with.
#[derive(Debug, Serialize, Deserialize)]
struct Head {
    format: u32,
    /// The URL asked, which carries no credential, as no URL asked does.
    url: String,
    /// When it was kept, in seconds since the Unix epoch, as the clock said.
    stored: i64,
    status: u16,
    headers: BTreeMap<String, String>,
}

/// What a kept file's `bytes` hold for `request`, or why they cannot be read.
fn read_kept(bytes: &[u8], request: &HttpRequest) -> Result<Found, String> {
    let Some(end) = bytes.iter().position(|byte| *byte == b'\n') else {
        return Err(String::from("it has no line saying what it keeps"));
    };
    let (head, body) = (&bytes[..end], &bytes[end + 1..]);
    // The format before the rest, so one a newer build wrote in a shape
    // this one does not know is left alone rather than called unreadable.
    let head: serde_json::Value =
        serde_json::from_slice(head).map_err(|error| error.to_string())?;
    let format = head
        .get("format")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| String::from("it says no format"))?;
    if format > u64::from(CACHE_FORMAT) {
        return Ok(Found::Newer);
    }
    let head: Head = serde_json::from_value(head).map_err(|error| error.to_string())?;
    // A file kept for another request, under the same key: what tells
    // requests apart has changed since, or two collided. Replaced.
    if head.url != request.url().as_str() || body.len() > request.limit() {
        return Ok(Found::Nothing);
    }
    if !(200..300).contains(&head.status) {
        return Err(format!("it keeps an answer of status {}", head.status));
    }
    Ok(Found::Kept {
        response: HttpResponse::new(head.status, head.headers, body.to_vec()),
        stored: head.stored,
    })
}

/// What a request's answer is kept under: its URL and every header it sends
/// but `User-Agent`, by name, since another `Accept` may be answered
/// otherwise, while another version of Mindweaver asking is not a reason to
/// ask again.
///
/// A secret is never part of it: a request carrying one is never kept, and
/// one is left out here all the same, so nothing written by this function
/// was ever a credential.
fn key(request: &HttpRequest) -> ContentHash {
    let mut headers: Vec<(&str, &str)> = request
        .headers()
        .iter()
        .filter(|header| !header.is_secret() && header.name() != "user-agent")
        .map(|header| (header.name(), header.value()))
        .collect();
    headers.sort_unstable();
    let mut canonical = format!("GET {}\n", request.url());
    for (name, value) in headers {
        canonical.push_str(name);
        canonical.push_str(": ");
        canonical.push_str(value);
        canonical.push('\n');
    }
    hash::request(&canonical)
}

/// What the cache could not do: said beside the answer, which is handed
/// back all the same.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("{path}: a kept answer cannot be read, so it is asked again: {detail}")]
    Unreadable { path: PathBuf, detail: String },
    #[error("{path}: an answer cannot be kept: {source}")]
    Unwritable {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{path}: is a link, so no answer is read from it or kept in it")]
    Linked { path: PathBuf },
}

codes!(CacheError {
    Self::Unreadable { .. } => "http_cache_unreadable",
    Self::Unwritable { .. } => "http_cache_unwritable",
    Self::Linked { .. } => "http_cache_linked",
});

#[cfg(test)]
#[path = "cache_test.rs"]
mod tests;
