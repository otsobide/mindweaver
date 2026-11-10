//! The [`Http`] a build reads a catalog with: reqwest's blocking client,
//! over rustls.
//!
//! reqwest because gix's HTTP transport builds it already, so reading a
//! catalog costs the build no crate; blocking because core is synchronous,
//! and a front end calls it from a thread of its own (the TUI's worker, the
//! server's writer thread or tokio's blocking pool), never from an async
//! task. Everything reqwest says is turned into core's own words here, at
//! the edge ([`HttpError`], [`HttpResponse`]), so no reqwest type reaches
//! the port or anything behind it.

use std::io::{self, Read};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, LOCATION};
use reqwest::redirect::Policy;

use crate::port::{Http, HttpError, HttpRequest, HttpResponse};
use crate::value::Redacted;

/// How long a connection is given to be made, whatever a request's timeout:
/// a host that never answers is said to be unreachable in this long rather
/// than once a long timeout for a slow catalog runs out.
const CONNECT: Duration = Duration::from_secs(10);

/// How much of a body is read at a time, and so how far past a request's
/// limit a read can go before it is refused: never more than this.
const CHUNK: usize = 64 * 1024;

/// reqwest, as the [`Http`] a catalog is read through.
///
/// Its client is made on the first request rather than with the adapter, so
/// a front end that never reads a catalog never starts the thread a blocking
/// client runs on, and kept for the next, which reuses its connections. It
/// keeps nothing else between requests: no cookie store (reqwest's
/// `cookies` is not built), no credential, no default header.
#[derive(Debug, Default)]
pub struct Reqwest {
    client: OnceLock<Result<Client, String>>,
}

impl Http for Reqwest {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let client = match self.client.get_or_init(made) {
            Ok(client) => client,
            Err(detail) => {
                return Err(HttpError::Fetch {
                    url: request.url().clone(),
                    detail: Redacted::new(detail),
                })
            }
        };
        let started = Instant::now();
        let response = client
            .get(request.url().as_str())
            .headers(headers(request)?)
            // The whole exchange, the body included: reqwest holds the
            // body's reads to a request's own timeout as one deadline,
            // where the client's would be a fresh wait for each read.
            .timeout(request.timeout())
            .send()
            .map_err(|error| failed(request, error))?;

        let status = response.status().as_u16();
        // A length said beyond the limit is refused before a byte of the
        // body is read; one that says nothing, or lies, is held to it as
        // the body comes ([`body`]).
        if response
            .content_length()
            .is_some_and(|length| length > request.limit() as u64)
        {
            return Err(too_large(request));
        }
        let mut kept: Vec<(String, String)> = Vec::new();
        for (name, value) in response.headers() {
            let Ok(value) = value.to_str() else {
                continue;
            };
            if name == LOCATION {
                kept.push((name.as_str().to_owned(), absolute(&response, value)));
            } else {
                kept.push((name.as_str().to_owned(), value.to_owned()));
            }
        }
        let body = body(response, request, started)?;
        Ok(HttpResponse::new(status, kept, body))
    }
}

/// The client every request goes through, or why none could be made.
fn made() -> Result<Client, String> {
    Client::builder()
        // Core follows a redirect itself ([`crate::http`]), under rules a
        // fake keeps too: which are followed, and which headers go on.
        .redirect(Policy::none())
        // A proxy is read from nowhere: the environment is the front end's
        // to read, and a proxy it names is still to be handed over (an open
        // question in docs/architecture.md). Taken from the environment
        // here, a test asking a server on loopback would ask a developer's
        // proxy instead.
        .no_proxy()
        .connect_timeout(CONNECT)
        .build()
        .map_err(|error| said(&error).to_string())
}

/// `request`'s headers as reqwest sends them, each secret marked sensitive,
/// which keeps it out of what reqwest logs.
fn headers(request: &HttpRequest) -> Result<HeaderMap, HttpError> {
    let mut headers = HeaderMap::new();
    for header in request.headers() {
        // Checked when the request was made, so a refusal here is a rule of
        // reqwest's core's check does not know: said without the value,
        // which may be a secret.
        let refused = |detail: &str| HttpError::Header {
            name: header.name().to_owned(),
            detail: detail.to_owned(),
        };
        let name = HeaderName::from_bytes(header.name().as_bytes())
            .map_err(|_| refused("reqwest does not take it as a header's name"))?;
        let mut value = HeaderValue::from_str(header.value())
            .map_err(|_| refused("reqwest does not take its value"))?;
        value.set_sensitive(header.is_secret());
        headers.insert(name, value);
    }
    Ok(headers)
}

/// The body of `response`, read a chunk at a time and refused as soon as it
/// passes the request's limit or its time runs out.
///
/// Read to one byte past the limit rather than to the length the server
/// said, which a server can say wrong or not at all: what counts is what
/// came. The deadline is checked between reads as well as by reqwest, so a
/// body that trickles in is stopped even by a reqwest that only timed each
/// read.
fn body(
    mut response: Response,
    request: &HttpRequest,
    started: Instant,
) -> Result<Vec<u8>, HttpError> {
    let mut body = Vec::new();
    let mut chunk = vec![0; CHUNK];
    loop {
        let read = match response.read(&mut chunk) {
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(unread(request, error)),
        };
        if read == 0 {
            return Ok(body);
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > request.limit() {
            return Err(too_large(request));
        }
        if started.elapsed() > request.timeout() {
            return Err(timed_out(request));
        }
    }
}

/// Where a `Location` leads, as an absolute URL: one given relative to the
/// URL asked is read against it, as a browser reads it.
fn absolute(response: &Response, location: &str) -> String {
    response
        .url()
        .join(location)
        .map_or_else(|_| location.to_owned(), String::from)
}

/// What a request that reqwest could not send or got no answer to is
/// reported as.
fn failed(request: &HttpRequest, error: reqwest::Error) -> HttpError {
    // The URL is said once, by the variant; reqwest would say it again.
    let error = error.without_url();
    let url = request.url().clone();
    if error.is_timeout() {
        timed_out(request)
    } else if error.is_connect() {
        HttpError::Connect {
            url,
            detail: said(&error),
        }
    } else if error.is_builder() {
        HttpError::Url {
            url,
            detail: said(&error),
        }
    } else {
        HttpError::Fetch {
            url,
            detail: said(&error),
        }
    }
}

/// What a body that could not be read whole is reported as: a timeout when
/// that is what reqwest stopped it for.
fn unread(request: &HttpRequest, error: io::Error) -> HttpError {
    let timed_out_by_reqwest = error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<reqwest::Error>())
        .is_some_and(reqwest::Error::is_timeout);
    if timed_out_by_reqwest || error.kind() == io::ErrorKind::TimedOut {
        timed_out(request)
    } else {
        HttpError::Fetch {
            url: request.url().clone(),
            detail: said(&error),
        }
    }
}

fn timed_out(request: &HttpRequest) -> HttpError {
    HttpError::TimedOut {
        url: request.url().clone(),
        after: request.timeout(),
    }
}

fn too_large(request: &HttpRequest) -> HttpError {
    HttpError::TooLarge {
        url: request.url().clone(),
        limit: request.limit(),
    }
}

/// What reqwest said, and what each error under it said, as one line
/// without the credentials of any URL it quotes.
///
/// The causes too, since reqwest's own words are the step that failed
/// ("error sending request") and the reason is beneath them ("connection
/// refused").
fn said(error: &dyn std::error::Error) -> Redacted {
    let mut words = error.to_string();
    let mut cause = error.source();
    while let Some(inner) = cause {
        let more = inner.to_string();
        if !words.contains(&more) {
            words.push_str(": ");
            words.push_str(&more);
        }
        cause = inner.source();
    }
    Redacted::new(&words)
}

#[cfg(test)]
#[path = "reqwest_test.rs"]
mod tests;
