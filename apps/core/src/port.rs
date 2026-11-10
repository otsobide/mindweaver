//! The ports: what core needs of the world beyond the files it reads and
//! writes, each as a trait core's rules are written against, with the
//! adapter a build runs with in [`crate::adapter`].
//!
//! The architecture is hexagonal. Core's rules (what a skill is, what an
//! install may overwrite, which copy is edited) sit in the middle and depend
//! on nothing outside them; whatever they need from outside is a port here,
//! a trait in core's own words; and an adapter on the edge implements it with
//! a library (`gix` for a [`Remote`], reqwest for [`Http`], SQLite for a
//! [`LedgerStore`], the system clock for a [`Clock`]). The
//! front ends, the CLI, the TUI and the panel's backend, are the other side's
//! adapters: they drive core through its functions and the command lines
//! every one of them runs (`commands::run_line`), and add nothing core has to
//! know. So a new store, transport or clock is an adapter written against a
//! port, and a new front end is one more driver of the same commands: neither
//! changes a rule.
//!
//! This is the seam D-14 chose for HTTP ([`Http`]), kept for every port: an
//! injectable trait, a default adapter built in, and a fake in a test. A port
//! speaks only
//! core's types. What an adapter's library says is turned into those at the
//! edge (an error into text, a commit into a [`Revision`]), rather than
//! carried through, which would tie core's public API to the library's
//! version and let a rule depend on how one library happens to work.
//!
//! The files themselves are not behind a port. What core promises about them
//! is how they are written (`create_new`, a temporary renamed into place, a
//! link never followed out of its holder), and those promises are made of the
//! operating system's own calls; an adapter that only seemed to keep them is
//! the failure a port here would invite (docs/architecture.md, "Values,
//! ports and adapters").

use std::collections::BTreeMap;
use std::fmt::{self, Debug};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use thiserror::Error;

use crate::backup;
use crate::code::codes;
use crate::gather::Watch;
use crate::kind::Kind;
use crate::ledger::{
    Action, Carried, Entry, Forgotten, FromSource, Gathered, InstallRecord, LedgerError, Merged,
    Moved, Outcome, Source, SourceKind, Taken, Target,
};
use crate::settings::ProjectId;
use crate::url::carries_credentials;
use crate::value::{Pin, Redacted, Revision, SourceName};
use crate::workspace::WorkspaceError;

/// Where a source's artifacts are fetched from: a repository a gather clones
/// into the workspace's cache, and later scrubs of anything a credential left
/// in it.
///
/// One method a step of a gather, rather than one that gathers: what a gather
/// does with what was fetched (reading it, holding it to the rules, placing
/// it on the shelf) is core's, and the same whichever remote fetched it.
pub trait Remote: Debug + Send + Sync {
    /// Fetch `url`, at `reference` when one is given, into `into`, replacing
    /// whatever was there, telling `watch` how it goes and stopping when it
    /// is cancelled ([`GitError::Cancelled`]).
    ///
    /// At `pin` when one is given (#99): that commit's files, whatever
    /// `reference` is at now, and the pin as the revision, or
    /// [`GitError::PinUnreachable`] when the repository has no such commit
    /// for it to reach. `reference` is still the branch the pin is looked
    /// for on where the commit cannot be asked for by its id alone.
    ///
    /// What is left at `into` holds no credential once this returns, whoever
    /// asked: a caller that forgot to refuse one is not trusted to.
    ///
    /// `token`, when a front end handed one over for this run (D-16), is
    /// offered only to the host [`token_host`](crate::url::token_host) says
    /// it is for, when that host asks who is fetching, and never asked of
    /// anyone, kept, nor told to whatever else answers for credentials.
    /// Without one, or where it is not offered, the remote logs in as it
    /// would anyway: git config's credential helpers, then ssh's own keys.
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        pin: Option<&Pin>,
        into: &Path,
        watch: &Watch,
        token: Option<&Token>,
    ) -> Result<Cloned, GitError>;

    /// Take out of the clone at `clone` every credential an older build left
    /// in what it wrote there, returning whether there were any.
    fn scrub(&self, clone: &Path) -> Result<bool, GitError>;
}

/// Where a backup is committed: a repository a copy of is checked out,
/// written into by an export, and sent back (`mindweaver backup`).
///
/// A port of its own rather than more of [`Remote`]'s: a gather only ever
/// reads, and the library it reads with cannot send (D-27, in
/// docs/architecture.md), so what sends is another adapter, the system's
/// `git`, behind words of core's. What a backup writes is core's
/// ([`crate::export`]); this only checks out, commits and pushes.
///
/// One method a step, so that which step a backup is at, and until which
/// one a cancel is honoured, is core's to say ([`backup::Progress`]): the
/// steps before the push are handed the backup's [`backup::Watch`], and
/// stop when it is cancelled ([`UpstreamError::Cancelled`]), what they
/// were running stopped rather than waited for; the push is handed none,
/// since a push stopped halfway leaves the remote as nobody can say.
///
/// No method ever asks a person for anything: a credential nobody
/// supplied is an error ([`UpstreamError::Credentials`]), as a gather's is.
pub trait Upstream: Debug + Send + Sync {
    /// Check `url` out into `into`, a folder not there yet, on `branch`, or
    /// the branch the remote names as its own when none is given, starting
    /// it from that one when the remote has no `branch` yet, and from
    /// nothing when it has no commit at all. The branch it is on is handed
    /// back.
    fn check_out(
        &self,
        url: &str,
        branch: Option<&str>,
        into: &Path,
        token: Option<&Token>,
        watch: &backup::Watch,
    ) -> Result<CheckedOut, UpstreamError>;

    /// Commit what `paths` (routes from the work tree's root) hold now in
    /// the work tree at `work`, as `message`, and say what is then to be
    /// sent to where it was checked out from: nothing at all when they hold
    /// what the branch held and the remote has the branch, and the commit
    /// it was started from, uncommitted to, when the remote has not got
    /// the branch yet.
    fn commit(
        &self,
        work: &Path,
        paths: &[&str],
        message: &str,
        watch: &backup::Watch,
    ) -> Result<Commit, UpstreamError>;

    /// Push what [`commit`](Self::commit) left at the head of the work tree
    /// at `work` to where it was checked out from, on its branch. Never
    /// stopped once started: what is sent by then is the remote's.
    fn push(&self, work: &Path, token: Option<&Token>) -> Result<(), UpstreamError>;
}

/// A token a front end was handed for one run (D-16): sent where the run
/// sends it, and never kept, printed or written into a `Debug`.
///
/// No `Display`, `Serialize` or text value, so nothing that says what a
/// command did can hold it by mistake: only an adapter reads it, through
/// [`reveal`](Self::reveal).
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    /// What it holds, for the adapter that sends it.
    pub(crate) fn reveal(&self) -> &str {
        &self.0
    }
}

impl Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)")
    }
}

/// A repository checked out for a backup: [`Upstream::check_out`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckedOut {
    /// The branch the work tree is on, which a publish pushes to.
    pub branch: String,
}

/// What [`Upstream::commit`] left to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Commit {
    /// The paths held what the branch held, and the remote has the branch,
    /// or holds no commit at all: nothing to send.
    Nothing,
    /// The head of the work tree, to push to its branch: the commit made of
    /// what the paths hold, or the one a branch the remote has not got was
    /// started from.
    Ready { revision: Revision },
}

/// What a backup sent: what [`Upstream::commit`] left, once
/// [`Upstream::push`] sent it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Published {
    /// The paths held what the branch held: nothing was committed or sent.
    Unchanged,
    /// A commit was made, and the branch now points at it.
    Pushed { revision: Revision },
}

/// Why a backup's repository could not be checked out or sent.
///
/// As [`GitError`], the failure as text, never the library's or the
/// process's error, and every URL in it [`redacted`](crate::url::redacted):
/// what `git` said is shown as it said it.
#[derive(Debug, Error)]
pub enum UpstreamError {
    #[error("`git` could not be run, and a backup is sent with it: {detail}")]
    Unavailable { detail: String },
    #[error("{url}: cannot be checked out: {detail}")]
    CheckOut { url: String, detail: String },
    /// The server asked who is sending, and nothing that may answer had a
    /// credential it would take. `url` is shown without its credentials.
    #[error(
        "{url}: needs credentials, and a backup never prompts for them: configure a git \
         credential helper, use ssh-agent for an ssh URL, or name a variable holding a token \
         with --token-env"
    )]
    Credentials { url: String },
    #[error("the backup could not be committed: {detail}")]
    Commit { detail: String },
    #[error("{url}: the backup could not be pushed: {detail}")]
    Push { url: String, detail: String },
    /// The backup's watch was cancelled while a step before the push ran,
    /// or before it started: what `git` was running was stopped, and
    /// nothing was sent.
    #[error("the backup was cancelled before anything was pushed")]
    Cancelled,
}

codes!(UpstreamError {
    Self::Unavailable { .. } => "backup_git_unavailable",
    Self::CheckOut { .. } => "backup_check_out_failed",
    Self::Credentials { .. } => "backup_credentials_needed",
    Self::Commit { .. } => "backup_commit_failed",
    Self::Push { .. } => "backup_push_failed",
    Self::Cancelled => "backup_git_cancelled",
});

/// The time, for what the ledger stamps an action or a gather with.
///
/// A port rather than a call to the system clock where the time is read, so a
/// fixture can pin it and a test can tell one moment from the next without
/// waiting for a second to pass.
pub trait Clock: Debug + Send + Sync {
    /// Seconds since the Unix epoch.
    fn now(&self) -> i64;
}

/// A clone that is on disk and ready to be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloned {
    /// The working tree.
    pub root: PathBuf,
    /// The commit that was checked out, when it could be resolved.
    pub revision: Option<Revision>,
}

/// Why a repository could not be cloned.
///
/// Every variant carries the failure as text rather than as the error of the
/// library an adapter fetched with. Those types are large and would become
/// part of this crate's public API, tying its version to the library's; what a
/// caller does with them is print them. Every URL in that text, the one typed
/// included, is [`redacted`](crate::url::redacted): this is what the gather
/// log and the screen are given.
#[derive(Debug, Error)]
pub enum GitError {
    #[error("{url}: not a URL git understands: {detail}")]
    Url { url: String, detail: String },
    #[error("`{reference}` is not a usable branch or tag name: {detail}")]
    Reference { reference: String, detail: String },
    #[error("{path}: the cache directory cannot be prepared: {source}")]
    Cache {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{url}: cannot be fetched: {detail}")]
    Fetch { url: String, detail: String },
    /// The server asked who is fetching, and nothing that may answer had a
    /// credential it would take. `url` is shown without its credentials.
    #[error(
        "{url}: needs credentials, and gathering never prompts for them: configure a git \
         credential helper, use ssh-agent for an ssh URL, or hand it a token with --token-env \
         or --gh"
    )]
    Credentials { url: String },
    #[error("{url}: cloned, but its files could not be checked out: {detail}")]
    Checkout { url: String, detail: String },
    /// The commit a source is pinned to is not one the repository has for
    /// `reference`, the remote's HEAD when it is `None`, to reach: mistyped,
    /// on another branch, or gone with a history somebody rewrote.
    #[error(
        "{url}: has no commit {pin} {on}, so nothing is gathered at it",
        on = on_branch(reference.as_deref())
    )]
    PinUnreachable {
        url: String,
        pin: Pin,
        reference: Option<String>,
    },
    /// The [`Watch`] was cancelled before the clone was finished. What there
    /// was of it is gone.
    #[error("the clone was cancelled before it was finished")]
    Cancelled,
    /// A file in the clone that names the URL it was made from could not be
    /// read, or rewritten without the credentials in it.
    #[error(transparent)]
    Scrub(#[from] WorkspaceError),
}

codes!(GitError {
    Self::Url { .. } => "git_url_invalid",
    Self::Reference { .. } => "git_reference_invalid",
    Self::Cache { .. } => "git_cache_failed",
    Self::Fetch { .. } => "git_fetch_failed",
    Self::Credentials { .. } => "git_credentials_needed",
    Self::Checkout { .. } => "git_checkout_failed",
    Self::PinUnreachable { .. } => "git_pin_unreachable",
    Self::Cancelled => "git_clone_cancelled",
} carried {
    Self::Scrub(error) => error,
});

/// Where a catalog is read from over HTTP: the MCP Registry, a skill
/// directory (D-14).
///
/// One `GET` a call, handed back whatever its status. What a status means,
/// which redirect is followed and with which headers, what is kept and for
/// how long, and how one page leads to the next are core's
/// ([`crate::http`]), and the same whichever adapter asked: a rule written
/// into an adapter is one the fake a test reads through would not keep.
///
/// So an adapter follows no redirect, and hands one back with its
/// `location` made absolute against the URL asked, the one thing about it
/// that takes a URL parser; keeps nothing between calls, no cookie, no
/// credential and no answer, so a credential lasts the request it was given
/// to; sends the request's headers, adding none that says who asks or
/// carries a credential (a `Host`, or reqwest's `Accept` of anything, is
/// neither); and reads a body only up to the request's
/// [`limit`](HttpRequest::limit) and within its
/// [`timeout`](HttpRequest::timeout), failing as soon as either is passed
/// rather than once the whole body is in.
pub trait Http: Debug + Send + Sync {
    /// Ask for `request`'s URL, and hand back what was answered, or why
    /// nothing was.
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError>;
}

/// Who is asking, in every request's `User-Agent`: the product and its
/// version, as GitHub's API, which refuses a request without one, asks.
pub const USER_AGENT: &str = concat!("mindweaver/", env!("CARGO_PKG_VERSION"));

/// How long a request is given unless it says otherwise: from asking to
/// the last byte of the body, redirects included, not each read on its own,
/// so a server that sends a byte now and then cannot hold a command for
/// longer than this.
pub const HTTP_TIMEOUT: Duration = Duration::from_secs(30);

/// How many bytes of a body a request takes unless it says otherwise: 8 MiB,
/// some forty times a page of the MCP Registry's hundred servers, so a
/// catalog that grows is not refused, and a server that sends without end
/// costs a command no more than this.
pub const HTTP_LIMIT: usize = 8 * 1024 * 1024;

/// The headers of an answer core reads, and so the only ones a
/// [`HttpResponse`] holds: its type, what a cached answer is checked again
/// by, the next page, where a redirect leads, and how long to wait and how
/// many requests are left, which tell a rate limit ([`crate::http`]).
///
/// A list rather than every header, because what an answer holds may be
/// kept on the disk ([`crate::http::Cache`]), and a `Set-Cookie` is a
/// credential: one that is never kept in the first place is never written
/// down by mistake. A catalog that needs another header adds it here.
pub const KEPT_HEADERS: &[&str] = &[
    "content-type",
    "etag",
    "last-modified",
    "link",
    "location",
    "retry-after",
    "x-ratelimit-remaining",
    "x-ratelimit-reset",
];

/// The headers a request sends as a secret whichever way they were added,
/// since each carries a credential by what it is for.
const SECRET_HEADERS: &[&str] = &["authorization", "cookie", "proxy-authorization"];

/// A `GET` core asks an [`Http`] for: the URL, the headers it adds, and how
/// long and how much it will wait for.
///
/// Made only through [`get`](Self::get) and [`at`](Self::at), which refuse
/// a URL that is not `http` or `https` or carries a credential, so an
/// adapter is never handed one: a token goes in a header, as a
/// [`secret`](Self::secret), which no cache keeps, no redirect carries to
/// another site and no `Debug` shows. Every request says who is asking
/// ([`USER_AGENT`]) from the start.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpRequest {
    url: Redacted,
    headers: Vec<Header>,
    timeout: Duration,
    limit: usize,
}

/// One header a request sends.
#[derive(Clone, PartialEq, Eq)]
pub struct Header {
    name: String,
    value: String,
    secret: bool,
}

impl Header {
    /// Its name, in lowercase, as HTTP/2 sends every name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What it says: for an adapter to send, never to keep or print.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Whether it carries a credential, for an adapter to mark it so to its
    /// library, which then leaves it out of what it logs.
    pub fn is_secret(&self) -> bool {
        self.secret
    }
}

impl Debug for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = if self.secret { "<secret>" } else { &self.value };
        write!(f, "{}: {value}", self.name)
    }
}

impl HttpRequest {
    /// A `GET` of `url`, given [`HTTP_TIMEOUT`] and [`HTTP_LIMIT`].
    pub fn get(url: &str) -> Result<Self, HttpError> {
        Ok(HttpRequest {
            url: asked(url)?,
            headers: vec![Header {
                name: String::from("user-agent"),
                value: String::from(USER_AGENT),
                secret: false,
            }],
            timeout: HTTP_TIMEOUT,
            limit: HTTP_LIMIT,
        })
    }

    /// This request at `url`, its headers, timeout and limit kept: the next
    /// page, or where a redirect leads, asked the way the first was.
    pub fn at(&self, url: &str) -> Result<Self, HttpError> {
        Ok(HttpRequest {
            url: asked(url)?,
            ..self.clone()
        })
    }

    /// This request sending `name: value` too, in place of any value it sent
    /// under that name. `Authorization`, `Cookie` and `Proxy-Authorization`
    /// are sent as a [`secret`](Self::secret) all the same.
    pub fn header(self, name: &str, value: &str) -> Result<Self, HttpError> {
        self.with(name, value, false)
    }

    /// This request sending `name: value` as a credential: never kept in the
    /// HTTP cache, which keeps nothing a request carrying one was answered,
    /// never sent on to another site a redirect leads to, never shown.
    pub fn secret(self, name: &str, value: &str) -> Result<Self, HttpError> {
        self.with(name, value, true)
    }

    /// This request given `timeout` in place of [`HTTP_TIMEOUT`].
    pub fn within(self, timeout: Duration) -> Self {
        HttpRequest { timeout, ..self }
    }

    /// This request taking at most `limit` bytes of a body in place of
    /// [`HTTP_LIMIT`].
    pub fn at_most(self, limit: usize) -> Self {
        HttpRequest { limit, ..self }
    }

    /// This request without its secrets: what a redirect to another site is
    /// asked as.
    pub fn without_secrets(&self) -> Self {
        HttpRequest {
            headers: self
                .headers
                .iter()
                .filter(|header| !header.secret)
                .cloned()
                .collect(),
            ..self.clone()
        }
    }

    /// The URL asked, which carries no credential.
    pub fn url(&self) -> &Redacted {
        &self.url
    }

    /// Every header it sends, `User-Agent` first, each name once.
    pub fn headers(&self) -> &[Header] {
        &self.headers
    }

    /// What it sends under `name`, in any case.
    pub fn header_value(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|header| header.name.eq_ignore_ascii_case(name))
            .map(|header| header.value.as_str())
    }

    /// Whether any header it sends is a secret.
    pub fn carries_secrets(&self) -> bool {
        self.headers.iter().any(|header| header.secret)
    }

    /// How long it is given, from asking to the last byte of the body.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// How many bytes of a body it takes at most.
    pub fn limit(&self) -> usize {
        self.limit
    }

    fn with(mut self, name: &str, value: &str, secret: bool) -> Result<Self, HttpError> {
        let name = name.to_ascii_lowercase();
        if name.is_empty() || !name.bytes().all(is_token) {
            return Err(HttpError::Header {
                name,
                detail: String::from("a header's name is letters, digits and `!#$%&'*+-.^_`|~`"),
            });
        }
        // Visible ASCII, spaces and tabs, as every HTTP library takes a
        // value: a line break would start another header, and what one
        // library takes beyond that another refuses. The value is not said,
        // since it may be a secret.
        if !value
            .bytes()
            .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte))
        {
            return Err(HttpError::Header {
                name,
                detail: String::from("its value holds a line break or a character beyond ASCII"),
            });
        }
        let secret = secret || SECRET_HEADERS.contains(&name.as_str());
        self.headers.retain(|header| header.name != name);
        self.headers.push(Header {
            name,
            value: value.to_owned(),
            secret,
        });
        Ok(self)
    }
}

impl Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpRequest")
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("timeout", &self.timeout)
            .field("limit", &self.limit)
            .finish()
    }
}

/// `url` as a request may ask it: `http` or `https`, naming a host, with no
/// space, control character or credential in it.
///
/// What else a URL must be is the adapter's library's to say: a parser of
/// core's own would be a second opinion of what a URL is, and the one that
/// sends it is the one that counts.
fn asked(url: &str) -> Result<Redacted, HttpError> {
    let refused = |detail: &str| HttpError::Url {
        url: Redacted::new(url),
        detail: Redacted::new(detail),
    };
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(refused("it has a space or a control character in it"));
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return Err(refused("it is not an http or https URL"));
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return Err(refused("it is not an http or https URL"));
    }
    if rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .is_empty()
    {
        return Err(refused("it names no host"));
    }
    if carries_credentials(url) {
        return Err(HttpError::Credentials {
            url: Redacted::new(url),
        });
    }
    Ok(Redacted::new(url))
}

/// Whether `byte` may be part of a header's name: RFC 9110's `tchar`.
fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// What an [`Http`] was answered: the status, the headers core reads
/// ([`KEPT_HEADERS`]), and the body, no longer than the request's limit.
///
/// Made only through [`new`](Self::new), which keeps those headers and no
/// other, so an adapter or a fake that hands over every header the server
/// sent hands core no more than it reads.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

impl HttpResponse {
    /// What was answered with `status`, `headers` as the server sent them
    /// (a name in any case, one sent twice twice) and `body`.
    ///
    /// A header sent twice is kept as one, its values joined by `, `, as
    /// HTTP reads a list such as `Link` sent over several lines.
    pub fn new<N, V>(status: u16, headers: impl IntoIterator<Item = (N, V)>, body: Vec<u8>) -> Self
    where
        N: AsRef<str>,
        V: AsRef<str>,
    {
        let mut kept: BTreeMap<String, String> = BTreeMap::new();
        for (name, value) in headers {
            let name = name.as_ref().to_ascii_lowercase();
            if !KEPT_HEADERS.contains(&name.as_str()) {
                continue;
            }
            let value = value.as_ref().trim();
            kept.entry(name)
                .and_modify(|held| {
                    held.push_str(", ");
                    held.push_str(value);
                })
                .or_insert_with(|| value.to_owned());
        }
        HttpResponse {
            status,
            headers: kept,
            body,
        }
    }

    /// The status it was answered with.
    pub fn status(&self) -> u16 {
        self.status
    }

    /// Whether the status says it was answered: 200 to 299.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Whether the status sends the request elsewhere, to where its
    /// `location` says: 301, 302, 303, 307 or 308.
    pub fn is_redirect(&self) -> bool {
        matches!(self.status, 301 | 302 | 303 | 307 | 308)
    }

    /// The header `name` holds, in any case, when it is one core reads and
    /// the server sent it.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Every header kept, by its name in lowercase.
    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    /// The body.
    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

impl Debug for HttpResponse {
    /// The body by its length: a catalog's page printed whole would bury
    /// whatever a failing test meant to say.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &format_args!("{} bytes", self.body.len()))
            .finish()
    }
}

/// Why a request was answered nothing.
///
/// In core's words, as [`GitError`] is: every URL in it is the one asked,
/// which carries no credential, and what the adapter's library said is
/// [`Redacted`] all the same, since it can quote one. No header's value is
/// ever in it, since one may be a secret.
#[derive(Debug, Error)]
pub enum HttpError {
    #[error("{url}: not a URL a catalog is read from: {detail}")]
    Url { url: Redacted, detail: Redacted },
    /// A URL with a user or a password in it, which would be kept and shown
    /// wherever the URL is: a credential goes in a header of its own.
    #[error(
        "{url}: carries a credential, which is never sent in a URL: \
         it goes in a header of its own"
    )]
    Credentials { url: Redacted },
    #[error("`{name}`: not a header that can be sent: {detail}")]
    Header { name: String, detail: String },
    #[error("{url}: cannot be reached: {detail}")]
    Connect { url: Redacted, detail: Redacted },
    #[error("{url}: answered nothing whole within {after:?}")]
    TimedOut { url: Redacted, after: Duration },
    #[error("{url}: answered more than the {limit} bytes it may")]
    TooLarge { url: Redacted, limit: usize },
    #[error("{url}: cannot be read: {detail}")]
    Fetch { url: Redacted, detail: Redacted },
}

codes!(HttpError {
    Self::Url { .. } => "http_url_invalid",
    Self::Credentials { .. } => "http_url_credentialed",
    Self::Header { .. } => "http_header_invalid",
    Self::Connect { .. } => "http_connect_failed",
    Self::TimedOut { .. } => "http_timed_out",
    Self::TooLarge { .. } => "http_too_large",
    Self::Fetch { .. } => "http_fetch_failed",
});

/// Which branch a pin was looked for on, as [`GitError::PinUnreachable`]
/// says it.
fn on_branch(reference: Option<&str>) -> String {
    match reference {
        Some(reference) => format!("that `{reference}` reaches"),
        None => String::from("that its default branch reaches"),
    }
}

/// Where a workspace's records are kept: the gathered sources and what each
/// supplied, every installation and its copies, the projects it knew, and
/// the action log.
///
/// Every method is a question or a change in core's words, never a query:
/// no caller writes SQL, and a store of another kind implements the same
/// methods ([`crate::ledger::Ledger`] holds it). Each write is whole or not
/// at all, as a transaction makes it; each URL a store is handed it keeps
/// [`redacted`](crate::url::redacted), and hands back as a `Redacted`, as
/// [`crate::ledger`] promises. `Send`, so a front end can read on a thread
/// of its own.
pub trait LedgerStore: Debug + Send {
    /// Stamp what it writes from now on by `clock`.
    fn stamp_by(&mut self, clock: Arc<dyn Clock>);

    /// Where it is kept, which every error it gives names: a file, or what a
    /// store kept nowhere says in its place (`:memory:` for SQLite's).
    fn path(&self) -> &Path;

    /// The source at this coordinate, if it has been gathered before: the
    /// one whose URL is `url` however it was spelled
    /// ([`crate::url::canonical`]), at the same branch, or at the remote's
    /// HEAD when `reference` is `None`, and from the same folder.
    fn source(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Option<Source>, LedgerError>;

    /// The source for this coordinate, registering it the first time, as
    /// `url` was typed, under the name its URL gives it
    /// ([`SourceName::derived`]), numbered when another source has that.
    ///
    /// Its `directory` is chosen once, here, and never recomputed: it is where
    /// files already are.
    fn source_for(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Source, LedgerError>;

    /// The plugin `parent`, a marketplace, lists as `entry` (#103),
    /// registering it the first time as a git source at `url`, from its
    /// folder `subdirectory`, called after the marketplace and the entry
    /// (`acme-tools.deploy`), numbered when another source has that, and
    /// moved there when the catalog says it is somewhere else now: a plugin
    /// is what its entry is, and where is the catalog's to say each time.
    ///
    /// Never found by [`source`](Self::source), which finds the sources no
    /// marketplace lists, so a plugin at a repository somebody also gathers
    /// with `gather git` is two sources, each called by its own name.
    fn child_for(
        &self,
        parent: &Source,
        entry: &str,
        url: &str,
        subdirectory: &str,
    ) -> Result<Source, LedgerError>;

    /// The plugins `parent`, a marketplace, has had gathered, by their
    /// entries: what a refresh or a removal of it reaches.
    fn children(&self, parent: &Source) -> Result<Vec<Source>, LedgerError>;

    /// The name and the folder a source registered from `url` now would be
    /// given ([`source_for`](Self::source_for)), registering nothing: what a
    /// gather's dry run says of a source it has not met
    /// ([`crate::gather::dry_run`]).
    fn would_register(&self, url: &str) -> Result<(SourceName, String), LedgerError>;

    /// The source called `name`, if one is.
    fn source_named(&self, name: &SourceName) -> Result<Option<Source>, LedgerError>;

    /// Call the source `id` `name` from now on. Whether no other source has
    /// it is the caller's to have asked ([`source_named`](Self::source_named)):
    /// a store refuses a name two sources would hold as a failure.
    fn rename_source(&self, id: i64, name: &SourceName) -> Result<(), LedgerError>;

    /// Take the source `id` from the folder `subdirectory` of its repository
    /// from now on: one an older build registered from `skills`, its default
    /// folder then, which a gather naming no folder takes the whole
    /// repository of since #101 ([`crate::gather::OLDER_DEFAULT_SUBDIRECTORY`]).
    /// Whether no other source is at the coordinate it moves to is the
    /// caller's to have asked ([`source`](Self::source)): a store refuses one
    /// two sources would hold as a failure.
    fn set_source_folder(&self, id: i64, subdirectory: &str) -> Result<(), LedgerError>;

    /// The sources the step to schema 7 made one with another ([`Merged`])
    /// whose folders are not gone yet, in the order the step made them one.
    ///
    /// Kept from the step until then, rather than handed back by the opening
    /// that took it: a read takes the step as often as a write does, the
    /// first command after an upgrade being a `status` or a `list` as often
    /// as not, and says nothing of it, so the first command that writes is
    /// the one to say each merge and clear its folders
    /// ([`crate::gather::scrub`]), whichever opened the ledger first. None
    /// once that is done, and none from a store that never had an older
    /// schema to step from.
    fn merged(&self) -> Result<Vec<Merged>, LedgerError>;

    /// Forget the merge [`merged`](Self::merged) hands back with the folders
    /// called `directory`, once they are gone: a folder name a source may be
    /// given again from then on, and a merge said for the last time.
    fn cleared_merge(&self, directory: &str) -> Result<(), LedgerError>;

    /// Note which revision a source was last gathered at, and the pin it
    /// was gathered at, or none, which is what it is pinned to from now on.
    fn saw_source(
        &self,
        id: i64,
        revision: Option<&str>,
        pin: Option<&Pin>,
    ) -> Result<(), LedgerError>;

    /// Forget a source, and with it what it brought; an installation from it
    /// keeps its `origin` but no longer points at it (see the schema's notes
    /// on what a forgotten source takes with it). Whether there was one to
    /// forget. A marketplace takes the plugins it lists with it, and what
    /// each brought (#103).
    ///
    /// For a gather that registered a source and then was cancelled, which
    /// leaves the ledger as it found it: a source nothing came from would
    /// otherwise hold on to its folder name. Its files are the caller's.
    fn forget_source(&self, id: i64) -> Result<bool, LedgerError>;

    /// Record one gathered artifact, replacing what that source held under the
    /// same name.
    ///
    /// The same name from a *different* source is a different row, on purpose:
    /// two repositories offering `commit-style` are two things to choose
    /// between, and this is what remembers which is which.
    fn record(&self, source: &Source, taken: &Taken) -> Result<(), LedgerError>;

    /// Forget the artifact of `kind` called `name` that `source` supplied,
    /// saying whether there was one: what a gather of it found it offers no
    /// more (#100), once its folder is off the shelf. What another source
    /// supplied under the name stays, as [`record`](Self::record) keeps one
    /// source's apart from another's.
    fn forget_gathered(&self, source: &Source, kind: Kind, name: &str)
        -> Result<bool, LedgerError>;

    /// Everything gathered, ordered the way a listing reads it: by kind, then
    /// name, then source.
    fn gathered(&self) -> Result<Vec<Gathered>, LedgerError>;

    /// Record that every artifact gathered at a path starting with `from`,
    /// a route from the workspace's root, is at the same path starting with
    /// `to` now, and how many were: for a shelf moved with the folder it is
    /// in, as a workspace a Mindflayer build made is moved to Mindweaver's
    /// names (`migrate --names`). One transaction, so a listing never finds
    /// some of them moved; what it was gathered from, and when, stay.
    fn shelf_moved(&self, from: &str, to: &str) -> Result<usize, LedgerError>;

    /// Every source anything was gathered from, or tried to be, ordered the
    /// way a listing reads them: by URL, then branch, then folder, each
    /// marketplace's plugins straight after it, by their entries (#103).
    ///
    /// Its own query rather than the sources of [`gathered`](Self::gathered):
    /// a source whose gather failed before it took anything, or whose every
    /// artifact has gone since, has no row there to be found by, and it is
    /// still a place the shelf is filled from. A row naming a kind this build
    /// has never heard of is skipped, as `gathered` skips it.
    ///
    /// Sorted once the URLs are redacted rather than by SQL, so a URL an
    /// older build stored with a credential is listed where the URL it is
    /// shown as goes.
    fn sources(&self) -> Result<Vec<Source>, LedgerError>;

    /// Every installation of what `source` supplied, in any project, by
    /// project, then kind, then name: what forgetting the source leaves
    /// orphaned (`mindweaver gather remove`, #100), each still recorded,
    /// its `origin` kept, once [`forget_source`](Self::forget_source) has
    /// let go of it.
    fn installed_from(&self, source: &Source) -> Result<Vec<FromSource>, LedgerError>;

    /// Record that an artifact was put into a project, replacing whatever was
    /// recorded of it before.
    ///
    /// Its targets are replaced as a whole rather than added to: they are the
    /// copies this install wrote, and a target an earlier install had and this
    /// one did not is no longer Mindweaver's to overwrite or delete. All of it
    /// or none, so a record never names some of an install's copies.
    fn installed(&self, project: &str, installation: &InstallRecord) -> Result<(), LedgerError>;

    /// Record what a render of a project's `kind` leaves it: each of
    /// `records` with its hash and copies in place of what was recorded of
    /// it, every other installation of `kind` in the project forgotten, and
    /// for each of `files`, a route from the project, whether Mindweaver made
    /// it ([`set_created`](Self::set_created)).
    ///
    /// For a kind the project is given as one render of all of it, as its
    /// rules are a block of one file: what is recorded of each is what that
    /// render wrote, so the whole is written at once, in one transaction, and
    /// a render's record is never half there. An installation keeps the time
    /// it was first recorded at, and its origin is taken in as every URL is.
    fn rendered(
        &self,
        project: &str,
        kind: Kind,
        records: &[InstallRecord],
        files: &[(&str, bool)],
    ) -> Result<(), LedgerError>;

    /// What the ledger records of installing `name` into a project, if it
    /// records anything.
    fn installation(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
    ) -> Result<Option<InstallRecord>, LedgerError>;

    /// Forget an installation, returning whether there was one.
    fn uninstalled(&self, project: &str, kind: Kind, name: &str) -> Result<bool, LedgerError>;

    /// Forget the copy an installation of `name` wrote at `path`, and the
    /// installation once it records no other, saying which went.
    ///
    /// For a change that reached one copy: the others are still what was
    /// installed, and forgetting the whole installation would leave them with
    /// no record, never updated or removed again. A skill's installation
    /// recording no copy at all, which only a build from before installations
    /// had targets wrote, is forgotten whole, as [`uninstalled`](Self::uninstalled)
    /// would. A rule's is one a build writes, for a rule that reaches no
    /// agent the project names (`install::rules`), never one from before
    /// targets, since rules were installed only after: a change at some other
    /// path leaves it what was installed, and it stays. So does an MCP
    /// server's, which records entries and never a copy (D-10). One
    /// transaction, so an install of the same name at the same moment never
    /// finds half of this done.
    fn uninstalled_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
    ) -> Result<Forgotten, LedgerError>;

    /// Record that the copy an installation of `name` wrote at `path` is
    /// called `to_name` now, at `to_path`, saying what became of its record.
    ///
    /// For a rename that reached one copy: it is what was installed, renamed,
    /// and stays Mindweaver's to tell apart (D-04), as an edit does, from
    /// where it came, with the hash of what was written. An installation it
    /// was the last copy of takes the new name whole, keeping when it was
    /// installed; one that records others keeps them under the old name,
    /// still what was installed, and the copy is recorded under the new one
    /// as an installation of its own from the same place. A name is one
    /// installation a project, so where another holds the new name already
    /// the copy is forgotten instead, as [`uninstalled_copy`](Self::uninstalled_copy) would.
    /// A skill's installation recording no copy at all, which only a build
    /// from before installations had targets wrote, is forgotten whole as
    /// there, and a rule's or an MCP server's stays as there. One
    /// transaction, so an install of either name at the same moment never
    /// finds half of this done.
    fn renamed_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
        to_name: &str,
        to_path: &str,
    ) -> Result<Carried, LedgerError>;

    /// Record that the installation of `name` has its copies at `targets`
    /// now, in place of those it recorded, and forget it when `targets` is
    /// empty. Whether there was one.
    ///
    /// For copies that moved without the installation changing: a project
    /// that names another agent is given a copy where that agent reads, and
    /// one that stops naming an agent loses the copy there, while what was
    /// installed, from where, its hash and when stay what they were. Writing
    /// the whole record again ([`installed`](Self::installed)) would say it was
    /// installed just now. One transaction, so an install of the same name
    /// at the same moment never finds half of this done.
    fn retargeted(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        targets: &[Target],
    ) -> Result<bool, LedgerError>;

    /// Record where each of a project's installations has its copies once
    /// the project has moved from a `mind.toml` to its settings, a hash for
    /// each record that had none and was found current, the `id` its
    /// settings hold now, as [`knew`](Self::knew) records one, and `log`, each
    /// line a log entry about the project.
    ///
    /// All of it in one transaction, where [`retargeted`](Self::retargeted) takes one
    /// an installation: a migration is all or nothing for its project, and
    /// one that fails here puts the project's `mind.toml` back, so a ledger
    /// left with some installations moved would name copies where that
    /// marker installs none. An installation it does not record any more is
    /// passed over, as `retargeted` finds nothing to do for one.
    fn migrated(
        &self,
        project: &str,
        id: &ProjectId,
        moved: &[Moved],
        log: &[(Action, String)],
    ) -> Result<(), LedgerError>;

    /// The names this ledger says Mindweaver put into a project.
    ///
    /// What separates an artifact this tool installed from one somebody wrote
    /// by hand, which is what stops an uninstall from deleting the second.
    fn installations(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError>;

    /// Every installation into a project of `kind`, each read as
    /// [`installation`](Self::installation) reads one, in name order.
    ///
    /// For a report on everything a project was given, which has to name
    /// what was installed even where no copy of it is left to find, and asks
    /// once rather than once a name.
    fn records(&self, project: &str, kind: Kind) -> Result<Vec<InstallRecord>, LedgerError>;

    /// Every copy this ledger says Mindweaver wrote into a project for
    /// `kind`, whichever installation wrote it, by path relative to the
    /// project, `/` separated, in path order, each once: the rules of a
    /// project share its `AGENTS.md`, a section each.
    ///
    /// By copy rather than by name, for a listing that has to say which of
    /// the folders holding a skill hold Mindweaver's copy: a name alone
    /// would claim a copy somebody keeps in one agent's folder because
    /// Mindweaver installed one of that name into another's. One query
    /// rather than one per installation, since a listing asks it of every
    /// project.
    fn copies(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError>;

    /// Whether Mindweaver made the file at `path`, a route from the project,
    /// rather than wrote into one that was there: what a render was told
    /// when it made it ([`crate::agents_md::Written::Created`]), kept until
    /// the file stops being Mindweaver's.
    fn created(&self, project: &str, path: &str) -> Result<bool, LedgerError>;

    /// Record whether Mindweaver made the file at `path`, a route from the
    /// project: kept when it did, forgotten when it did not or the file has
    /// stopped being Mindweaver's. A file noted twice keeps the time it was
    /// first noted.
    fn set_created(&self, project: &str, path: &str, created: bool) -> Result<(), LedgerError>;

    /// The id the ledger last saw the project at `route` hold, if it saw one
    /// there: a route from the workspace, spelled as a record spells its
    /// project.
    ///
    /// An id somebody edited into a form no project holds is no id, rather
    /// than a failure: nothing can hold it, so nothing is found by it.
    fn project_at(&self, route: &str) -> Result<Option<ProjectId>, LedgerError>;

    /// Every id the ledger knows, with the route it last saw it at, in route
    /// order: what a scan matches the projects it finds against.
    fn known_projects(&self) -> Result<Vec<(ProjectId, String)>, LedgerError>;

    /// Whether anything is recorded of the project at `route`: an
    /// installation, or a file Mindweaver made there. What a project the
    /// ledger knows no id of has to hold nothing of for its entry to be
    /// handed to another directory ([`relinked`](crate::ledger::Ledger::relinked)), since nothing
    /// says which project those records were of.
    fn holds(&self, route: &str) -> Result<bool, LedgerError>;

    /// Record that each project of `seen` holds its id at its route, and
    /// say how many of them the ledger did not know there already.
    ///
    /// For what a workspace's projects say now, read whenever it is opened
    /// to write ([`crate::workspace::WeaverWorkspace::ledger_to_write`]): the
    /// ledger follows them, so it knows each linked project by the id it
    /// holds. An id it last saw at another route moves here, unless that
    /// route is one of `stale`, the entries the workspace links that open
    /// no project any more: an id seen there is what finds that entry's
    /// project once it has moved, and only `mindweaver relink` takes it from
    /// there ([`relinked`](crate::ledger::Ledger::relinked)). Whatever id the ledger knew at the
    /// route goes, as the project there holds another now, and the records
    /// there are the new one's: they name copies in the directory it holds.
    ///
    /// One transaction, taken before anything is read, so a relink at the
    /// same moment comes wholly before this or wholly after it.
    fn knew(&self, seen: &[(ProjectId, String)], stale: &[String]) -> Result<usize, LedgerError>;

    /// Move every record of the project at `old` to `new`, as the project
    /// whose id is `id`, running `write` inside the same transaction: what it
    /// moved, by the count of installations, once `write` returns true, and
    /// nothing at all when it returns false, everything undone. The ledger's
    /// [`relinked`](crate::ledger::Ledger::relinked) is this with a write of
    /// any type; a store is handed it erased, so the port stays a trait
    /// object.
    fn relinked_by(
        &self,
        old: &str,
        new: &str,
        id: &ProjectId,
        write: &mut dyn FnMut() -> bool,
    ) -> Result<Option<usize>, LedgerError>;

    /// The sources whose stored URL still carries credentials, which only a
    /// Mindweaver from before they were refused could have stored.
    ///
    /// Each is handed back with the URL [`scrub_source`](Self::scrub_source) will give it,
    /// never the one it has: nothing the ledger returns carries a credential.
    fn tainted_sources(&self) -> Result<Vec<Source>, LedgerError>;

    /// Store a source's URL without the credentials in it.
    ///
    /// Two rows it would make one are one already: the URL a source is
    /// known by holds no credential ([`crate::url::canonical`]), so a row
    /// whose URL only a credential told from another's was made one with it
    /// by the step to schema 7, which kept the one whose shelf was filled
    /// most recently ([`Merged`]). Before that step the scrub made them one
    /// itself, by the same rule.
    ///
    /// An installation's origin is rewritten with the URL it spells: schema 3
    /// filled it in from the URL as it was stored, credentials and all, and
    /// nothing else would take them out of it.
    fn scrub_source(&self, id: i64) -> Result<(), LedgerError>;

    /// Rewrite every action log entry without the credentials of any URL in
    /// it, returning how many there were.
    fn scrub_log(&self) -> Result<usize, LedgerError>;

    /// Append to the action log.
    ///
    /// Failures are logged too. A log that only records what worked cannot
    /// answer the question anybody actually opens it with.
    fn log(
        &self,
        action: Action,
        target: Option<&str>,
        outcome: Outcome,
        detail: Option<&str>,
    ) -> Result<(), LedgerError>;

    /// The most recent actions, newest first.
    fn history(&self, limit: usize) -> Result<Vec<Entry>, LedgerError>;
}

#[cfg(test)]
#[path = "port_test.rs"]
mod tests;
