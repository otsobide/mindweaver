//! Reading a catalog over HTTP (D-14): what core asks an [`Http`] for, and
//! what it makes of what is answered.
//!
//! The port asks once and hands back whatever was answered. The rules are
//! here, so the CLI, the TUI and the server cannot disagree about them, and
//! a fake a test reads through is held to them as the real adapter is:
//!
//! - **A redirect** is followed here, at most [`MOST_REDIRECTS`] times, never
//!   from `https` to anything else, and with no secret header sent on to
//!   another site, as curl and browsers do.
//! - **A status** other than a success is a failure ([`FetchError::Status`]),
//!   with the URL asked, never the answer's body.
//! - **An answer** to a request carrying no secret may be kept in the
//!   workspace ([`Cache`]), and is then answered from there while it is
//!   fresh, and asked again with what tells the server which one is kept
//!   (`If-None-Match`, `If-Modified-Since`) once it is not.
//! - **Pages** follow one another as each says ([`Client::pages`]), up to a
//!   number the caller sets, never twice the same and never to another site.
//!
//! Failures are collected, as everywhere in core: a page that fails ends
//! the paging and is said beside the pages read before it ([`Paged`]), and
//! what the cache could not do is a warning beside an answer
//! ([`CacheError`]), never a reason to withhold it.
//!
//! The official MCP Registry is the first catalog read through it
//! ([`crate::mcp_registry`], #106), and the skill directories (#108) the
//! next.

mod cache;

use std::sync::Arc;
use std::time::Instant;

use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::adapter::reqwest::Reqwest;
use crate::code::codes;
use crate::port::{Http, HttpError, HttpRequest, HttpResponse};
use crate::value::Redacted;

pub use cache::{Cache, CacheError, CACHE_FORMAT};

/// How many redirects a request follows before it is a failure
/// ([`FetchError::Redirects`]): enough for a catalog moved, then put behind
/// a CDN, and few enough that a loop ends at once.
pub const MOST_REDIRECTS: usize = 5;

/// What reads a catalog: an [`Http`], [`Reqwest`] unless another is handed
/// over, and the cache it keeps answers in, if any.
#[derive(Debug, Clone)]
pub struct Client {
    http: Arc<dyn Http>,
    cache: Option<Cache>,
}

/// What was answered, how, and what the cache could not do on the way.
#[derive(Debug)]
pub struct Fetched<T> {
    /// The answer, or what core read of it.
    pub value: T,
    /// Where the answer came from.
    pub via: Via,
    /// What the cache could not do, said beside the answer.
    pub warnings: Vec<CacheError>,
}

/// Where an answer came from: for a front end to say a catalog's listing
/// may be as old as the cache keeps one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Via {
    /// Asked of the server.
    Network,
    /// Kept in the cache, and fresh: the server was not asked.
    Cache,
    /// Kept in the cache, and the server, asked whether it had changed,
    /// said it had not.
    Revalidated,
}

/// Every page read of a catalog, in the order they were read, and what
/// stopped the paging before the last one, if anything did.
#[derive(Debug)]
pub struct Paged<P> {
    pub pages: Vec<P>,
    /// Why there are no more pages than these, when the last of them named
    /// a next: kept beside them rather than in their place, since what was
    /// read is still what the catalog said.
    pub failure: Option<FetchError>,
    /// What the cache could not do, on any page.
    pub warnings: Vec<CacheError>,
}

impl Default for Client {
    fn default() -> Self {
        Client::new()
    }
}

impl Client {
    /// A client that reads through [`Reqwest`] and keeps nothing.
    pub fn new() -> Self {
        Client::through(Arc::new(Reqwest::default()))
    }

    /// A client that reads through `http`, a test's fake or another adapter.
    pub fn through(http: Arc<dyn Http>) -> Self {
        Client { http, cache: None }
    }

    /// This client keeping what it is answered in `cache`.
    pub fn caching(self, cache: Cache) -> Self {
        Client {
            cache: Some(cache),
            ..self
        }
    }

    /// What `request` is answered, once it is a success.
    pub fn get(&self, request: &HttpRequest) -> Result<Fetched<HttpResponse>, FetchError> {
        let mut warnings = Vec::new();
        let (value, via) = self.fetch(request, &mut warnings)?;
        Ok(Fetched {
            value,
            via,
            warnings,
        })
    }

    /// What `request` is answered, read as JSON into `T`.
    pub fn json<T: DeserializeOwned>(
        &self,
        request: &HttpRequest,
    ) -> Result<Fetched<T>, FetchError> {
        let fetched = self.get(request)?;
        Ok(Fetched {
            value: parsed(request, fetched.value.body())?,
            via: fetched.via,
            warnings: fetched.warnings,
        })
    }

    /// Every page of a catalog, from `first`, each read as JSON into `P`,
    /// at most `most` of them (one at least).
    ///
    /// `next` is told each page as it is read, and the answer it came in,
    /// and says the URL of the page after it, or nothing when it is the last:
    /// a cursor the page holds, put into the first page's URL
    /// ([`with_query`]), or the `Link` the answer names ([`next_link`]). Each
    /// page is asked as the first was, its headers, timeout and limit kept
    /// ([`HttpRequest::at`]), and kept in the cache, or not, as the first
    /// would be.
    ///
    /// The paging stops, said as its [`failure`](Paged::failure), at a page
    /// that cannot be read, at a next page that would be one more than
    /// `most`, one read already, which would go round for ever, or one on
    /// another site than the first, where its headers, a secret among them,
    /// were never meant to go.
    pub fn pages<P: DeserializeOwned>(
        &self,
        first: &HttpRequest,
        most: usize,
        mut next: impl FnMut(&P, &HttpResponse) -> Option<String>,
    ) -> Paged<P> {
        let mut paged = Paged {
            pages: Vec::new(),
            failure: None,
            warnings: Vec::new(),
        };
        let mut asked = first.clone();
        let mut seen: Vec<String> = vec![first.url().to_string()];
        loop {
            let page = self
                .fetch(&asked, &mut paged.warnings)
                .and_then(|(response, _)| {
                    let page: P = parsed(&asked, response.body())?;
                    let following = next(&page, &response);
                    Ok((page, following))
                });
            let following = match page {
                Ok((page, following)) => {
                    paged.pages.push(page);
                    following
                }
                Err(failure) => {
                    paged.failure = Some(failure);
                    break;
                }
            };
            let Some(url) = following else {
                break;
            };
            match next_page(first, &asked, &url, &seen, paged.pages.len(), most) {
                Ok(request) => {
                    seen.push(url);
                    asked = request;
                }
                Err(failure) => {
                    paged.failure = Some(failure);
                    break;
                }
            }
        }
        paged
    }

    /// What `request` is answered once it is a success, through the cache
    /// when there is one and the request carries no secret, and how.
    fn fetch(
        &self,
        request: &HttpRequest,
        warnings: &mut Vec<cache::CacheError>,
    ) -> Result<(HttpResponse, Via), FetchError> {
        // What a secret was sent for is never kept: kept by what tells the
        // request from another, the secret left out, it would be handed to
        // the next request without one; with the secret in, something of a
        // credential would be on the disk.
        let Some(cache) = self.cache.as_ref().filter(|_| !request.carries_secrets()) else {
            let response = self.follow(request)?;
            return Ok((answered(request, response)?, Via::Network));
        };

        let found = cache.find(request, warnings);
        if let cache::Found::Kept { response, stored } = &found {
            if cache.is_fresh(*stored) {
                return Ok((response.clone(), Via::Cache));
            }
        }
        let response = self.follow(&found.revalidating(request))?;
        let replaceable = found.may_be_replaced();
        if let cache::Found::Kept { response: kept, .. } = found {
            if response.status() == 304 {
                cache.keep(request, &kept, warnings);
                return Ok((kept, Via::Revalidated));
            }
        }
        let response = answered(request, response)?;
        if replaceable {
            cache.keep(request, &response, warnings);
        }
        Ok((response, Via::Network))
    }

    /// What `request` is answered once every redirect it is given is
    /// followed, as far as core follows one.
    ///
    /// The request's timeout is the whole of it, every redirect included:
    /// each is asked within what is left.
    fn follow(&self, request: &HttpRequest) -> Result<HttpResponse, FetchError> {
        let deadline = Instant::now() + request.timeout();
        let mut asked = request.clone();
        for _ in 0..=MOST_REDIRECTS {
            let response = self.http.get(&asked)?;
            let location = match response.header("location") {
                Some(location) if response.is_redirect() => location,
                _ => return Ok(response),
            };
            if is_https(asked.url()) && !is_https(location) {
                return Err(FetchError::Downgraded {
                    url: request.url().clone(),
                    to: Redacted::new(location),
                });
            }
            let next = asked.at(location)?;
            let next = if origin(location) == origin(asked.url()) {
                next
            } else {
                next.without_secrets()
            };
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(HttpError::TimedOut {
                    url: request.url().clone(),
                    after: request.timeout(),
                }
                .into());
            }
            asked = next.within(left);
        }
        Err(FetchError::Redirects {
            url: request.url().clone(),
            most: MOST_REDIRECTS,
        })
    }
}

/// The request for the page at `url`, which the page `asked` named after
/// the `read` pages of those from `first`, or why it is not asked.
fn next_page(
    first: &HttpRequest,
    asked: &HttpRequest,
    url: &str,
    seen: &[String],
    read: usize,
    most: usize,
) -> Result<HttpRequest, FetchError> {
    if read >= most.max(1) {
        return Err(FetchError::Pages {
            url: first.url().clone(),
            most: most.max(1),
        });
    }
    let request = asked.at(url)?;
    if origin(url) != origin(first.url()) {
        return Err(FetchError::Elsewhere {
            url: asked.url().clone(),
            next: request.url().clone(),
        });
    }
    if seen.iter().any(|seen| seen == url) {
        return Err(FetchError::Repeated {
            url: asked.url().clone(),
            next: request.url().clone(),
        });
    }
    Ok(request)
}

/// `response` when it is a success, and otherwise why `request` was not
/// answered.
fn answered(request: &HttpRequest, response: HttpResponse) -> Result<HttpResponse, FetchError> {
    if response.is_success() {
        Ok(response)
    } else {
        Err(FetchError::Status {
            url: request.url().clone(),
            status: response.status(),
        })
    }
}

/// `body`, read as JSON into `T`.
fn parsed<T: DeserializeOwned>(request: &HttpRequest, body: &[u8]) -> Result<T, FetchError> {
    serde_json::from_slice(body).map_err(|error| FetchError::Json {
        url: request.url().clone(),
        detail: Redacted::new(&error.to_string()),
    })
}

/// `url` with `name=value` added to its query, each encoded as a query's
/// part is, before any fragment: a cursor a catalog's page names, put into
/// the URL of the next.
///
/// Every byte but the letters, digits and `-._~` is written as `%XX`, so a
/// cursor holding a `&`, a `=`, a `+` or a `#` reaches the server as the
/// one it gave.
pub fn with_query(url: &str, name: &str, value: &str) -> String {
    let (head, fragment) = url.split_at(url.find('#').unwrap_or(url.len()));
    let separator = match head.find('?') {
        None => "?",
        Some(_) if head.ends_with('?') || head.ends_with('&') => "",
        Some(_) => "&",
    };
    format!(
        "{head}{separator}{}={}{fragment}",
        encoded(name),
        encoded(value)
    )
}

/// `text` as a query's part, or a path's: unreserved bytes as they are,
/// every other one as `%XX`, a `/` among them, so a name holding one is
/// still one part of a path.
pub(crate) fn encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The URL of the page after this one, as the answer's `Link` header names
/// it (`<URL>; rel="next"`, RFC 8288), when it names one.
///
/// As written, so a relative one is refused when it is asked
/// ([`HttpRequest::at`]) rather than read against a URL here: GitHub, the
/// API that pages this way, writes every one absolute.
pub fn next_link(response: &HttpResponse) -> Option<&str> {
    let mut rest = response.header("link")?;
    while let Some(open) = rest.find('<') {
        let after = &rest[open + 1..];
        let close = after.find('>')?;
        let target = &after[..close];
        let tail = &after[close + 1..];
        let end = tail.find('<').unwrap_or(tail.len());
        if tail[..end].split(';').any(names_next) {
            return Some(target.trim());
        }
        rest = &tail[end..];
    }
    None
}

/// Whether a `Link` parameter is a `rel` naming `next` among its words.
fn names_next(parameter: &str) -> bool {
    let Some((key, value)) = parameter.split_once('=') else {
        return false;
    };
    let value = value.trim().trim_end_matches(',').trim().trim_matches('"');
    key.trim().eq_ignore_ascii_case("rel")
        && value
            .split_ascii_whitespace()
            .any(|rel| rel.eq_ignore_ascii_case("next"))
}

/// The site `url` is on, the way a browser tells one from another: its
/// scheme and its host and port, as written, in lowercase.
///
/// As written rather than with a default port filled in, so `host` and
/// `host:443` read as two: the one way that is wrong is to keep a secret
/// back from a site that would have had it.
fn origin(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return String::new();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    format!("{scheme}://{authority}").to_ascii_lowercase()
}

/// Whether `url` is asked over TLS.
fn is_https(url: &str) -> bool {
    url.split_once("://")
        .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("https"))
}

/// Why a catalog was not read, or not read whole.
///
/// Every URL in it is one asked or named by an answer, said
/// [`Redacted`], as every URL core says is.
#[derive(Debug, Error)]
pub enum FetchError {
    /// The request was answered nothing.
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("{url}: answered with status {status}")]
    Status { url: Redacted, status: u16 },
    #[error("{url}: sent on more than {most} times, and not followed further")]
    Redirects { url: Redacted, most: usize },
    /// A redirect from `https` to plain `http` or another scheme: what a
    /// request was sent over TLS for is not asked without it.
    #[error("{url}: sent on to {to}, which is not https, and not followed")]
    Downgraded { url: Redacted, to: Redacted },
    #[error("{url}: did not answer the JSON expected: {detail}")]
    Json { url: Redacted, detail: Redacted },
    #[error("{url}: has more pages than the {most} read of it")]
    Pages { url: Redacted, most: usize },
    #[error("{url}: names {next} as its next page, which was read already")]
    Repeated { url: Redacted, next: Redacted },
    #[error("{url}: names {next} as its next page, on another site, which is not asked")]
    Elsewhere { url: Redacted, next: Redacted },
}

codes!(FetchError {
    Self::Status { .. } => "http_status_unsuccessful",
    Self::Redirects { .. } => "http_redirects_exceeded",
    Self::Downgraded { .. } => "http_redirect_downgraded",
    Self::Json { .. } => "http_json_invalid",
    Self::Pages { .. } => "http_pages_exceeded",
    Self::Repeated { .. } => "http_page_repeated",
    Self::Elsewhere { .. } => "http_page_elsewhere",
} carried {
    Self::Http(error) => error,
});

#[cfg(test)]
#[path = "http_test.rs"]
mod tests;
