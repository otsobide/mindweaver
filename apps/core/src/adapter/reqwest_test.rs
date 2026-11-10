//! Unit tests for `adapter/reqwest.rs`, kept beside it rather than inside
//! it: how what reqwest says is put into core's words. What the adapter
//! does with a request is the contract's, `tests/http_test.rs`, which runs
//! it against a server on loopback.

use super::*;

use std::fmt;

use crate::mother::UrlMother;

/// An error with words of its own and, beneath it, `cause`.
#[derive(Debug)]
struct Said {
    words: String,
    cause: Option<Box<Said>>,
}

impl fmt::Display for Said {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.words)
    }
}

impl std::error::Error for Said {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause
            .as_deref()
            .map(|cause| cause as &(dyn std::error::Error + 'static))
    }
}

fn chain(words: &[&str]) -> Said {
    words
        .iter()
        .rev()
        .fold(None, |cause, words| {
            Some(Said {
                words: (*words).to_owned(),
                cause: cause.map(Box::new),
            })
        })
        .expect("one error at least")
}

#[test]
fn what_reqwest_said_is_said_with_every_reason_beneath_it() {
    let error = chain(&[
        "error sending request",
        "client error (Connect)",
        "Connection refused (os error 111)",
    ]);

    assert_eq!(
        said(&error).as_str(),
        "error sending request: client error (Connect): Connection refused (os error 111)"
    );
}

/// hyper and reqwest often say a reason again in the words of the error
/// above it: said once.
#[test]
fn a_reason_its_error_already_says_is_said_once() {
    let error = chain(&["operation timed out: timed out", "timed out"]);

    assert_eq!(said(&error).as_str(), "operation timed out: timed out");
}

#[test]
fn a_url_quoted_in_what_reqwest_said_is_said_without_its_credential() {
    for typed in UrlMother::every_credentialed() {
        let quoted = UrlMother::quoted(&typed);
        let error = chain(&["error sending request", &quoted.typed]);

        let words = said(&error);

        assert!(!words.contains(&typed.secret), "{words}");
        assert!(words.contains(&quoted.clean), "{words}");
    }
}

/// The client is made on the first request, so an adapter nothing asks
/// starts no thread.
#[test]
fn an_adapter_makes_no_client_until_it_is_asked() {
    let adapter = Reqwest::default();

    assert!(adapter.client.get().is_none());
}
