//! Unit tests for `sources.rs`, kept beside it rather than inside it.
//!
//! The integration suite (`tests/sources_test.rs`) lists, renames, gathers
//! again and forgets sources of a real workspace; these hold its refusals to
//! the codes a gather gives the same conditions, and to saying no credential.

use super::*;

use crate::mother::{random, SourceMother, UrlMother};

/// One condition, one code: a name refused by `gather rename` is refused by
/// `gather git --name` for the same reason, and so is a credential typed
/// where either takes a name.
#[test]
fn every_code_a_source_s_refusal_echoes_is_the_one_a_gather_gives_it() {
    let core: Vec<&str> = crate::code::ALL
        .iter()
        .flat_map(|(_, codes)| codes.iter().copied())
        .collect();
    let source = SourceMother::random();

    assert_eq!(
        SourceError::ECHOED,
        [
            GatherError::NameInvalid {
                name: String::new()
            }
            .code(),
            GatherError::NameTaken {
                name: source.name.clone(),
                origin: source.origin(),
            }
            .code(),
            GatherError::CredentialsInUrl { url: String::new() }.code(),
        ]
    );
    assert!(SourceError::ECHOED.iter().all(|code| core.contains(code)));
}

#[test]
fn a_name_typed_with_a_credential_is_refused_and_never_said_with_it() {
    for url in UrlMother::every_credentialed() {
        let refused = unsaid(&url.typed).unwrap_err();

        assert_eq!(refused.code(), "credentials_in_url", "{}", url.typed);
        let said = refused.to_string();
        assert!(said.starts_with(&url.clean), "{said}");
        assert!(!said.contains(&url.secret), "{said}");
    }
}

#[test]
fn a_name_with_no_credential_is_no_refusal_of_unsaid() {
    for _ in 0..100 {
        let name = random::word();

        assert!(unsaid(&name).is_ok(), "{name}");
    }
}
