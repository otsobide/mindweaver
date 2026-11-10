//! Unit tests for `credentials.rs`, kept beside it rather than inside it.

use super::*;

use tempfile::TempDir;

const PRIVATE: &str = "https://example.com/acme/private.git";

/// A cascade with no helper configured, whose prompt panics.
///
/// `gix`'s cascade hands its options to the prompt when no helper had a
/// credential, and asks a person unless the mode forbids it. So this panics on
/// any options that would let it ask, or that name an askpass program at all,
/// and otherwise runs the real, empty cascade.
fn no_helper_and_a_prompt_that_panics() -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    |action, options| {
        if options.mode != prompt::Mode::Disable || options.askpass.is_some() {
            panic!("the prompt was reached for {action:?}");
        }
        Cascade::default().invoke(action, options)
    }
}

/// A repository that reads no config but its own, so no helper or askpass of
/// the machine running the tests can answer.
fn isolated_repository(dir: &TempDir) -> gix::Repository {
    gix::ThreadSafeRepository::init_opts(
        dir.path(),
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )
    .expect("initialize a repository")
    .to_thread_local()
}

/// A helper that has a credential for every URL.
fn a_helper_that_answers() -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    |action, _| match action {
        Action::Get(context) => Ok(Some(protocol::Outcome {
            identity: gix::sec::identity::Account {
                username: "me".into(),
                password: "hunter2".into(),
                oauth_refresh_token: None,
            },
            next: context.into(),
        })),
        Action::Store(_) | Action::Erase(_) => Ok(None),
    }
}

#[test]
fn a_credential_no_helper_has_is_an_error_and_never_a_prompt() {
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(no_helper_and_a_prompt_that_panics(), shortfalls.clone());

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(
        matches!(answer, Err(protocol::Error::IdentityMissing { .. })),
        "{answer:?}"
    );
    assert_eq!(shortfalls.last(), Some(Shortfall::Missing));
}

#[test]
fn a_helper_that_quits_leaves_the_credential_missing() {
    let shortfalls = Shortfalls::default();
    // What `gix`'s cascade returns when a helper answers `quit=1` and gives
    // no credential.
    let mut credentials = never_prompting(|_, _| Err(protocol::Error::Quit), shortfalls.clone());

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(matches!(answer, Err(protocol::Error::Quit)), "{answer:?}");
    assert_eq!(shortfalls.last(), Some(Shortfall::Missing));
}

#[test]
fn a_credential_the_server_takes_leaves_no_shortfall() {
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(a_helper_that_answers(), shortfalls.clone());

    let Ok(Some(answer)) = credentials(Action::get_for_url(PRIVATE)) else {
        panic!("the helper's credential was not passed on");
    };
    assert_eq!(answer.identity.username, "me");
    credentials(answer.next.store()).unwrap();

    assert_eq!(shortfalls.last(), None);
}

#[test]
fn a_credential_the_server_refuses_is_a_shortfall() {
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(a_helper_that_answers(), shortfalls.clone());

    let Ok(Some(answer)) = credentials(Action::get_for_url(PRIVATE)) else {
        panic!("the helper's credential was not passed on");
    };
    credentials(answer.next.erase()).unwrap();

    assert_eq!(shortfalls.last(), Some(Shortfall::Refused));
}

#[test]
fn a_login_ssh_refuses_is_a_missing_credential() {
    for url in [
        "git@example.com:acme/private.git",
        "ssh://git@example.com/acme/private.git",
    ] {
        let shortfalls = Shortfalls::default();
        let mut credentials = never_prompting(a_helper_that_answers(), shortfalls.clone());

        let Ok(Some(answer)) = credentials(Action::get_for_url(url)) else {
            panic!("{url}: the helper's user name was not passed on");
        };
        credentials(answer.next.erase()).unwrap();

        assert_eq!(shortfalls.last(), Some(Shortfall::Missing), "{url}");
    }
}

#[test]
fn the_configured_helpers_have_nothing_when_git_config_names_none() {
    let dir = TempDir::new().unwrap();
    let shortfalls = Shortfalls::default();
    let mut credentials =
        never_prompting(configured(isolated_repository(&dir)), shortfalls.clone());

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(
        matches!(answer, Err(protocol::Error::IdentityMissing { .. })),
        "{answer:?}"
    );
    assert_eq!(shortfalls.last(), Some(Shortfall::Missing));
}

#[test]
fn a_report_on_an_answer_nobody_gave_is_ignored() {
    let dir = TempDir::new().unwrap();
    let mut helpers = configured(isolated_repository(&dir));

    let answer = helpers(
        Action::Erase("url=https://example.com\n".into()),
        prompt::Options {
            askpass: None,
            mode: prompt::Mode::Disable,
        },
    );

    assert!(matches!(answer, Ok(None)), "{answer:?}");
}

/// A token drawn for one test, as a front end hands one over.
fn a_token() -> Token {
    Token::new(crate::mother::random::hex(40))
}

/// Helpers that panic whatever they are asked: a token that answers is the
/// one answer, and nothing about it reaches them.
fn helpers_never_asked() -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    |action, _| panic!("the helpers were asked {action:?}")
}

/// Helpers that note every action they are asked, and answer as
/// [`a_helper_that_answers`] does.
fn noting(
    asked: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
) -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    let mut answers = a_helper_that_answers();
    move |action, options| {
        asked.borrow_mut().push(format!("{action:?}"));
        answers(action, options)
    }
}

#[test]
fn a_token_answers_its_host_as_x_access_token_before_any_helper() {
    let token = a_token();
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(
        offering(
            Offered::for_url(Some(&token), PRIVATE),
            helpers_never_asked(),
        ),
        shortfalls.clone(),
    );

    let Ok(Some(answer)) = credentials(Action::get_for_url(PRIVATE)) else {
        panic!("the token was not offered");
    };
    assert_eq!(answer.identity.username, "x-access-token");
    assert_eq!(answer.identity.password, token.reveal());
    // What the server made of it reaches no helper either way, so none is
    // told to keep it.
    credentials(answer.next.clone().store()).unwrap();
    assert_eq!(shortfalls.last(), None);
    credentials(answer.next.erase()).unwrap();
    assert_eq!(shortfalls.last(), Some(Shortfall::Refused));
}

#[test]
fn what_comes_back_of_a_token_holds_none_of_it() {
    let token = a_token();
    let mut credentials = offering(
        Offered::for_url(Some(&token), PRIVATE),
        helpers_never_asked(),
    );

    let Ok(Some(answer)) = credentials(
        Action::get_for_url(PRIVATE),
        prompt::Options {
            askpass: None,
            mode: prompt::Mode::Disable,
        },
    ) else {
        panic!("the token was not offered");
    };

    let Action::Store(stored) = answer.next.clone().store() else {
        panic!("a store is a store");
    };
    assert!(!stored.to_str_lossy().contains(token.reveal()), "{stored}");
    let offered = format!("{:?}", Offered::for_url(Some(&token), PRIVATE));
    assert!(!offered.contains(token.reveal()), "{offered}");
}

#[test]
fn a_token_is_not_offered_to_another_host_nor_over_what_it_never_goes_over() {
    let token = a_token();
    for asked in [
        // `url.<base>.insteadOf`, or a redirect, took the fetch elsewhere.
        "https://elsewhere.example.com/acme/private.git",
        // Or over plain http, across the network.
        "http://example.com/acme/private.git",
        // Or over ssh, where the helpers are asked for a user name.
        "ssh://git@example.com/acme/private.git",
        "git@example.com:acme/private.git",
    ] {
        let asked_of = std::rc::Rc::default();
        let mut credentials = never_prompting(
            offering(
                Offered::for_url(Some(&token), PRIVATE),
                noting(std::rc::Rc::clone(&asked_of)),
            ),
            Shortfalls::default(),
        );

        let Ok(Some(answer)) = credentials(Action::get_for_url(asked)) else {
            panic!("{asked}: the helpers' answer was not passed on");
        };

        assert_eq!(answer.identity.username, "me", "{asked}");
        assert_ne!(answer.identity.password, token.reveal(), "{asked}");
        assert_eq!(asked_of.borrow().len(), 1, "{asked}");
    }
}

#[test]
fn a_token_for_a_url_it_never_goes_over_is_offered_nowhere() {
    let token = a_token();
    for url in [
        "ssh://git@example.com/acme/private.git",
        "git@example.com:acme/private.git",
        "http://example.com/acme/private.git",
        "/srv/git/private",
        "file:///srv/git/private",
    ] {
        assert!(Offered::for_url(Some(&token), url).is_none(), "{url}");
    }
    assert!(Offered::for_url(None, PRIVATE).is_none());
}

#[test]
fn without_a_token_the_helpers_answer_and_none_having_one_is_an_error_never_a_prompt() {
    let shortfalls = Shortfalls::default();
    let mut credentials = never_prompting(
        offering(None, no_helper_and_a_prompt_that_panics()),
        shortfalls.clone(),
    );

    let answer = credentials(Action::get_for_url(PRIVATE));

    assert!(
        matches!(answer, Err(protocol::Error::IdentityMissing { .. })),
        "{answer:?}"
    );
    assert_eq!(shortfalls.last(), Some(Shortfall::Missing));
}

#[test]
fn after_a_token_another_connection_s_get_of_another_host_reaches_the_helpers() {
    let token = a_token();
    let asked_of = std::rc::Rc::default();
    let mut credentials = never_prompting(
        offering(
            Offered::for_url(Some(&token), PRIVATE),
            noting(std::rc::Rc::clone(&asked_of)),
        ),
        Shortfalls::default(),
    );

    let Ok(Some(answer)) = credentials(Action::get_for_url(PRIVATE)) else {
        panic!("the token was not offered");
    };
    credentials(answer.next.store()).unwrap();
    let Ok(Some(answer)) = credentials(Action::get_for_url("https://elsewhere.example.com/x.git"))
    else {
        panic!("the helpers' answer was not passed on");
    };
    credentials(answer.next.store()).unwrap();

    // The `Get` and the `Store` of the second host, and nothing of the
    // token's.
    let asked = asked_of.borrow();
    assert_eq!(asked.len(), 2, "{asked:?}");
    assert!(asked.iter().all(|action| !action.contains(token.reveal())));
}
