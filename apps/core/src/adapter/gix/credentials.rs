//! What a fetch answers when the server asks who is fetching.
//!
//! `gix`'s own answer runs the credential helpers git config names and, when
//! none of them has a credential, asks a person: the askpass program, then the
//! terminal. Mindweaver keeps the first half and refuses the second. The TUI
//! owns the terminal while a gather runs, so a prompt there is a screen that
//! stops answering, and a server has no terminal at all. A credential nobody
//! has is an error instead, which a person can act on afterwards.
//!
//! A token a front end handed over for the run (D-16) is the one answer that
//! comes before the helpers', for the host it was given for alone
//! ([`offering`]).
//!
//! Nothing here is public: the callback's types are `gix`'s, and a front end
//! only ever sees the error a missing credential becomes.

// The callback's error is `gix`'s, fixed by the signature it asks for, so
// boxing it as clippy suggests is not ours to do.
#![allow(clippy::result_large_err)]

use std::cell::Cell;
use std::rc::Rc;

use gix::bstr::ByteSlice;
use gix::credentials::helper::{Action, Cascade};
use gix::credentials::protocol;
use gix::prompt;

use crate::port::Token;
use crate::url::token_host;

/// The user a token is offered as: the one GitHub takes any token with, an
/// app's installation token among them, and that GitLab and the other
/// forges taking a token as a password ignore.
const TOKEN_USER: &str = "x-access-token";

/// Why a fetch got no credential it could use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shortfall {
    /// The server asked, and nothing that may answer had one it would take:
    /// no helper had one to give (or one told git to stop asking), or, over
    /// ssh, ssh had no key it accepted.
    Missing,
    /// A helper gave one over http(s), and the server turned it down.
    Refused,
}

/// Where the callback leaves the last [`Shortfall`] it saw, for the clone to
/// read once its fetch has failed.
///
/// Shared rather than read off the fetch's error: that error says the same
/// thing several layers down a chain of `gix` types, and finding it there
/// would tie this crate to the shape of every one of them. An `Rc` rather than
/// an `Arc`, because `gix` asks for credentials on the thread that is cloning.
#[derive(Debug, Clone, Default)]
pub(super) struct Shortfalls(Rc<Cell<Option<Shortfall>>>);

impl Shortfalls {
    pub(super) fn last(&self) -> Option<Shortfall> {
        self.0.get()
    }
}

/// The credentials callback a fetch is given: `helpers`, run with every
/// prompt turned off, noting in `shortfalls` whatever they could not supply.
///
/// `helpers` takes the prompt options it is to run with, because that is where
/// `gix` decides whether to ask a person: [`configured`] is the real cascade,
/// and a test can stand in a cascade whose prompt panics. A config override
/// (`gitoxide.credentials.terminalPrompt=false`) would also stop the prompt,
/// and is rejected: the guarantee would rest on a string `gix` reads among the
/// user's own settings, where no test here can see it, and the credential a
/// front end supplies needs this callback to arrive through anyway.
pub(super) fn never_prompting(
    mut helpers: impl FnMut(Action, prompt::Options) -> protocol::Result,
    shortfalls: Shortfalls,
) -> impl FnMut(Action) -> protocol::Result {
    let mut over_ssh = false;
    move |action| {
        if let Action::Get(context) = &action {
            over_ssh = is_ssh(context);
        }
        // `gix` sends an `Erase` only once the server has refused what the
        // `Get` before it answered.
        let refused = matches!(action, Action::Erase(_));
        let answer = helpers(
            action,
            prompt::Options {
                askpass: None,
                mode: prompt::Mode::Disable,
            },
        );
        if refused {
            // ssh authenticates with keys of its own, and `gix` asks the
            // helpers only for a user name to hand it. A refusal there means
            // ssh had no key the server takes, which is as missing as a
            // credential gets. Over http(s) it is of a credential a helper did
            // give, and `gix` already says so.
            shortfalls.0.set(Some(if over_ssh {
                Shortfall::Missing
            } else {
                Shortfall::Refused
            }));
        } else if let Err(
            // A helper that answers `quit=1` has none either, and tells git to
            // stop asking anyone else.
            protocol::Error::IdentityMissing { .. } | protocol::Error::Quit,
        ) = answer
        {
            shortfalls.0.set(Some(Shortfall::Missing));
        }
        answer
    }
}

/// A token handed over for one run, and the host it was handed over for
/// ([`token_host`]), which is the only one it is offered to.
#[derive(Debug, Clone)]
pub(super) struct Offered {
    token: Token,
    host: String,
}

impl Offered {
    /// `token`, for the host a fetch of `url` sends one to: `None` when it
    /// sends one to none, over ssh, plain http or from a path, where the
    /// token is never offered.
    pub(super) fn for_url(token: Option<&Token>, url: &str) -> Option<Self> {
        Some(Self {
            token: token?.clone(),
            host: token_host(url)?,
        })
    }

    /// Whether `context` asks for a credential of this token's host, over
    /// a transport a token goes over.
    ///
    /// The URL of the `Get` rather than the one typed, as [`is_ssh`] reads
    /// it: a redirect, or `url.<base>.insteadOf` in git config, may have
    /// taken the fetch to another host, or to plain http, where this token
    /// is not to go.
    fn asked_by(&self, context: &protocol::Context) -> bool {
        context
            .url
            .clone()
            .or_else(|| context.to_url())
            .and_then(|url| token_host(&url.to_str_lossy()))
            .is_some_and(|host| host == self.host)
    }
}

/// `helpers`, with `offered`'s token answering in their place the `Get` of
/// its host, and the `Store` or `Erase` that tells whether the server took
/// it kept from them.
///
/// In their place rather than after them: the token is what the front end
/// was told to log in with, and a helper's credential for the same host
/// would be another login nobody asked for, as a backup's token clears
/// every helper too. And kept from them, since a `Store` is how a helper
/// such as `store` or `cache` learns a credential to keep, and the token
/// lasts one run (D-16). Any other `Get`, of another host or over a
/// transport it does not go over, the helpers answer as they would without
/// it, so a gather a rewrite takes to ssh still has ssh's user name.
pub(super) fn offering(
    offered: Option<Offered>,
    mut helpers: impl FnMut(Action, prompt::Options) -> protocol::Result,
) -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    let mut answered = false;
    move |action, prompt| {
        match &action {
            Action::Get(context) => {
                answered = false;
                if let Some(offered) = offered.as_ref().filter(|offered| offered.asked_by(context))
                {
                    answered = true;
                    return Ok(Some(protocol::Outcome {
                        identity: gix::sec::identity::Account {
                            username: TOKEN_USER.into(),
                            password: offered.token.reveal().into(),
                            oauth_refresh_token: None,
                        },
                        // The context asked, not the token: what comes back
                        // as the `Store` or `Erase` is never handed on.
                        next: context.clone().into(),
                    }));
                }
            }
            Action::Store(_) | Action::Erase(_) if answered => return Ok(None),
            Action::Store(_) | Action::Erase(_) => {}
        }
        helpers(action, prompt)
    }
}

/// Whether `context` asks about a URL that is fetched by running ssh, the
/// scp-like `host:path` included.
///
/// The `Get`'s URL rather than the one the gather was given, because it is
/// the one `gix` connected to: git config's `url.<base>.insteadOf` rewrites a
/// typed https URL to ssh, or the reverse, before the transport is chosen.
fn is_ssh(context: &protocol::Context) -> bool {
    context
        .url
        .clone()
        .or_else(|| context.to_url())
        .and_then(|url| gix::url::parse(url.as_bstr()).ok())
        .is_some_and(|url| url.scheme == gix::url::Scheme::Ssh)
}

/// The credential helpers git config names, run the way `gix` runs them when
/// it is given no callback.
///
/// The cascade is built for each `Get` from the URL the transport is at, which
/// a redirect may have changed and which `credential.<url>.*` sections match
/// against. It is kept for the `Store` or `Erase` that follows, which tells
/// the same helpers whether their answer was accepted.
pub(super) fn configured(
    repository: gix::Repository,
) -> impl FnMut(Action, prompt::Options) -> protocol::Result {
    let mut current: Option<Cascade> = None;
    move |action, prompt| {
        if let Action::Get(context) = &action {
            let url = context
                .url
                .clone()
                .or_else(|| context.to_url())
                .ok_or(protocol::Error::UrlMissing)?;
            // The prompt options git config asks for come back too, and are
            // dropped: they are exactly what is being refused.
            let (cascade, _, _) = repository
                .config_snapshot()
                .credential_helpers(gix::url::parse(url.as_bstr())?)
                .map_err(|source| protocol::Error::ConfigureCredentialHelpers {
                    source: Box::new(source),
                })?;
            current = Some(cascade);
        }
        match current.as_mut() {
            Some(cascade) => cascade.invoke(action, prompt),
            // A `Store` or `Erase` with no `Get` before it reports on an
            // answer no helper gave.
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[path = "credentials_test.rs"]
mod tests;
