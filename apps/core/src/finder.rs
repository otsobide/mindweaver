//! The skill directories `mindweaver gather search` asks (D-15, #108), each
//! a [`Finder`]: a service that helps find a skill and names where it is, a
//! git repository and a folder in it, and is never a source itself. What is
//! taken is gathered from that repository as `gather git` gathers it, so a
//! directory whose API changes or goes away breaks nothing on the shelf,
//! and its failure is a warning.
//!
//! Which finders a workspace asks is its marker's to say ([`enabled`]): its
//! `finders`, and GitHub's code search alone when it names none. The two
//! directories whose APIs nobody documents are never asked until a person
//! lists them, having read their terms (D-15), and a finder that is not
//! enabled is never sent anything.
//!
//! Each is asked through the [`Client`] its caller hands over ([`Search`]),
//! so whether answers are kept is the caller's, and a test's transport a
//! fake. What a finder answers is somebody else's JSON, read leniently: an
//! answer in a shape this build does not read is a failure of that finder
//! alone, beside what the others found ([`Failure::Unreadable`]); a result
//! that does not read, or whose address is no git source a gather would
//! take, is dropped and said ([`Failure::Dropped`]); and every word a front
//! end is handed is made printable first.

mod claude_plugins;
mod github;
mod skills_sh;

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use serde::de::{self, Deserializer};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

use crate::code::{codes, Said};
use crate::follow;
use crate::gather::{route_of, DEFAULT_SUBDIRECTORY};
use crate::http::{with_query, Client, FetchError};
use crate::port::{HttpError, HttpRequest, Token};
use crate::url::{self, carries_credentials, is_path};
use crate::value::Redacted;
use crate::WeaverWorkspace;

/// How many results each finder is asked for: its first page, a screen's
/// worth or two. A search is where a person starts looking, and one asking
/// each finder for more than anyone reads makes its limit run out sooner,
/// GitHub's above all, which allows a few code searches a minute.
pub const PER_FINDER: usize = 20;

/// How long an answer a finder gave is answered from the workspace's cache
/// without asking again: long enough to read a listing and take a result
/// from it by its number, which asks again, and find it where it was;
/// short enough that a search made an hour later asks.
pub const FRESH_FOR: Duration = Duration::from_secs(10 * 60);

/// The finders a workspace asks when its marker names none: GitHub's code
/// search alone, D-15's default, which still asks nothing without a token.
pub const DEFAULT_FINDERS: &[Finder] = &[Finder::Github];

/// A skill directory a search can ask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Finder {
    /// GitHub's code search, for a `SKILL.md`: its documented REST API,
    /// asked only with a token, which it answers no code search without.
    Github,
    /// skills.sh's `/api/search`: unstable, since nobody documents it.
    SkillsSh,
    /// The search api.claude-plugins.dev keeps of the skills it lists:
    /// unstable, since nobody documents it.
    ClaudePlugins,
}

impl Finder {
    /// Every finder, in the order a search asks them and lists what they
    /// found: the documented one first.
    pub const ALL: [Finder; 3] = [Finder::Github, Finder::SkillsSh, Finder::ClaudePlugins];

    /// The word the marker and a command line name it by: the service's own
    /// name, so what `finders` lists is what a person reading its terms
    /// looked up.
    pub fn name(self) -> &'static str {
        match self {
            Finder::Github => "github",
            Finder::SkillsSh => "skills.sh",
            Finder::ClaudePlugins => "claude-plugins.dev",
        }
    }

    /// The finder `name` names, if any.
    pub fn named(name: &str) -> Option<Finder> {
        Finder::ALL.into_iter().find(|finder| finder.name() == name)
    }

    /// Whether what it answers may change shape without a word: an API
    /// nobody documents, read as it answers today, best-effort.
    pub fn is_unstable(self) -> bool {
        match self {
            Finder::Github => false,
            Finder::SkillsSh | Finder::ClaudePlugins => true,
        }
    }

    /// Whether it is asked only with a token: GitHub's code search, which
    /// answers no request without one.
    pub fn needs_token(self) -> bool {
        match self {
            Finder::Github => true,
            Finder::SkillsSh | Finder::ClaudePlugins => false,
        }
    }

    /// Where it is asked unless a caller names another address that speaks
    /// the same API ([`Search::at`]).
    pub fn address(self) -> &'static str {
        match self {
            Finder::Github => "https://api.github.com",
            Finder::SkillsSh => "https://skills.sh",
            Finder::ClaudePlugins => "https://api.claude-plugins.dev",
        }
    }
}

impl fmt::Display for Finder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// By its name, the word a command line takes, rather than derived from the
/// variant, as a kind and an agent go out.
impl Serialize for Finder {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// By its name, as it goes out: what a front end reading a search's JSON
/// back reads ([`Hit`]).
impl<'de> Deserialize<'de> for Finder {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Finder::named(&name).ok_or_else(|| de::Error::custom(format!("no finder `{name}`")))
    }
}

/// `FINDER=URL`, a finder by its name and the address it is asked at in
/// place of its own, split at the first `=`, which no finder's name holds:
/// what `gather search --finder-url` takes.
///
/// Read here rather than by the command line's parser, so whether it names
/// a finder is core's to say, with a code, as the address is.
pub fn addressed(text: &str) -> Result<(Finder, String), SearchError> {
    text.split_once('=')
        .and_then(|(name, url)| Some((Finder::named(name)?, url.to_owned())))
        .ok_or_else(|| SearchError::Unaddressed {
            given: Redacted::new(text),
        })
}

/// The finders a workspace's marker enables, and what it lists that is no
/// finder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enabled {
    /// Each finder enabled, in [`Finder::ALL`]'s order, once.
    pub finders: Vec<Finder>,
    /// Whether the marker names them (`finders`), rather than leaving them
    /// to [`DEFAULT_FINDERS`].
    pub listed: bool,
    /// Each name it lists that this build knows no finder by.
    pub unknown: Vec<UnknownFinder>,
    /// The marker they are listed in, which what is said of them names.
    pub config: PathBuf,
}

/// The finders `workspace` enables: those its marker lists in `finders`,
/// or [`DEFAULT_FINDERS`] when it has no such key.
///
/// A name this build does not know is passed over and said, rather than
/// refused: a later build may know a finder this one does not, and the
/// marker it wrote is still read. A workspace not made yet, the default
/// before a write makes it, names none, and so enables the default.
pub fn enabled(workspace: &WeaverWorkspace) -> Enabled {
    let config = workspace.config_path();
    let Some(listed) = &workspace.config().finders else {
        return Enabled {
            finders: DEFAULT_FINDERS.to_vec(),
            listed: false,
            unknown: Vec::new(),
            config,
        };
    };
    let finders = Finder::ALL
        .into_iter()
        .filter(|finder| listed.iter().any(|name| name == finder.name()))
        .collect();
    let mut unknown: Vec<UnknownFinder> = Vec::new();
    for name in listed {
        let said = printable(name);
        if Finder::named(name).is_none() && !unknown.iter().any(|known| known.name == said) {
            unknown.push(UnknownFinder {
                name: said,
                config: config.clone(),
            });
        }
    }
    Enabled {
        finders,
        listed: true,
        unknown,
        config,
    }
}

impl Enabled {
    /// The finders a search asks: those `named`, each of which is to be
    /// enabled, or every one enabled when none is named.
    ///
    /// One named and not enabled is refused rather than asked or passed
    /// over: asking it is what enabling it allows, which is the person's
    /// to do, by hand, having read its terms (D-15).
    pub fn asked(&self, named: &[Finder]) -> Result<Vec<Finder>, SearchError> {
        if let Some(finder) = named.iter().find(|finder| !self.finders.contains(finder)) {
            return Err(SearchError::NotEnabled {
                finder: *finder,
                config: self.config.clone(),
            });
        }
        let asked: Vec<Finder> = if named.is_empty() {
            self.finders.clone()
        } else {
            Finder::ALL
                .into_iter()
                .filter(|finder| named.contains(finder))
                .collect()
        };
        if asked.is_empty() {
            return Err(SearchError::NoneEnabled {
                config: self.config.clone(),
            });
        }
        Ok(asked)
    }
}

/// A name the marker lists among its finders that this build knows no
/// finder by: passed over, and said.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{}: `{name}` is among its finders, and this build knows none by that name, so it is passed over",
    .config.display()
)]
pub struct UnknownFinder {
    /// The name, printable.
    pub name: String,
    pub config: PathBuf,
}

codes!(UnknownFinder => "finder_unknown");

/// What a search is doing, said while it does it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Asking `finder`, after `done` of the `total` asked have answered.
    Asking {
        finder: Finder,
        done: usize,
        total: usize,
    },
}

impl Progress {
    /// Whether a cancel now still stops the search: always, since it writes
    /// nothing a cancel would leave half done; it stops before the next
    /// finder is asked, the one being asked answering first, within its
    /// timeout.
    pub fn cancellable(self) -> bool {
        match self {
            Progress::Asking { .. } => true,
        }
    }
}

/// How a front end follows a search and stops it: a [`follow::Watch`] of
/// its [`Progress`].
pub type Watch = follow::Watch<Progress>;

/// Finders asked through a client, for one search.
#[derive(Debug, Clone)]
pub struct Search {
    client: Client,
    /// The address a finder is asked at where a caller named another than
    /// its own ([`Finder::address`]).
    at: Vec<(Finder, String)>,
    token: Option<Token>,
    watch: Watch,
}

impl Search {
    /// Each finder at its own address, through `client`, with no token,
    /// followed by nobody.
    pub fn new(client: Client) -> Self {
        Search {
            client,
            at: Vec::new(),
            token: None,
            watch: Watch::new(),
        }
    }

    /// Ask `finder` at `base` rather than its own address: a GitHub
    /// Enterprise server's API (`https://ghe.example/api/v3`), a mirror's,
    /// or a test's.
    ///
    /// Over https, or over plain http to this machine alone: a token and a
    /// query go to it, and neither should cross a network unread. No query,
    /// fragment or credential in it, since the search puts its own query
    /// after it.
    pub fn at(mut self, finder: Finder, base: &str) -> Result<Self, SearchError> {
        let refused = |why: &str| SearchError::Address {
            finder,
            url: Redacted::new(base),
            why: why.to_owned(),
        };
        let base = base.trim_end_matches('/');
        HttpRequest::get(base).map_err(|error| refused(&error.to_string()))?;
        if base.contains(['?', '#']) {
            return Err(refused(
                "it has a query or a fragment, which a search puts its own in place of",
            ));
        }
        if !scheme_is(base, "https") && !(scheme_is(base, "http") && on_this_machine(base)) {
            return Err(refused(
                "it is not https, which only an address on this machine may leave out",
            ));
        }
        self.at.retain(|(each, _)| *each != finder);
        self.at.push((finder, base.to_owned()));
        Ok(self)
    }

    /// Send `token` to the finders that need one, at their address, and to
    /// no other, as a secret header, which the HTTP cache keeps nothing of.
    pub fn with_token(mut self, token: Option<Token>) -> Self {
        self.token = token;
        self
    }

    /// Report each finder asked to `watch`, and stop before the next when it
    /// is cancelled.
    pub fn watched_by(mut self, watch: Watch) -> Self {
        self.watch = watch;
        self
    }

    /// Where `finder` is asked.
    pub fn base(&self, finder: Finder) -> &str {
        self.at
            .iter()
            .find(|(each, _)| *each == finder)
            .map_or(finder.address(), |(_, base)| base.as_str())
    }

    /// The host a token for this search is for, which `gh auth token` is
    /// asked for (`--gh`): GitHub's, `github.com`, though its API answers
    /// at `api.github.com`, or the host of the address GitHub's finder is
    /// asked at in its place, a GitHub Enterprise server's own.
    pub fn token_host(&self) -> String {
        let host = host_of(self.base(Finder::Github)).unwrap_or_default();
        match host.as_str() {
            "api.github.com" => String::from("github.com"),
            _ => host,
        }
    }

    /// Whether the token handed over for this search may go on to the
    /// gather that takes `hit`: only when its repository is on the host the
    /// token is for ([`Search::token_host`]), over https.
    ///
    /// A finder's answer is somebody else's, and may name any host: one
    /// naming another would otherwise be handed a GitHub token by a
    /// gather that sends it as a login.
    pub fn token_reaches(&self, hit: &Hit) -> bool {
        url::token_host(&hit.repository).is_some_and(|host| host == self.token_host())
    }

    /// What each of `finders` finds for `query`, in [`Finder::ALL`]'s order,
    /// each in the order it ranked them, a skill two of them name once.
    ///
    /// One that needs a token, asked without one, is skipped and sent
    /// nothing; when that leaves none to ask, nothing is searched. One that
    /// fails, is rate limited or answers in a shape this build does not read
    /// is a failure beside what the others found, never a reason to hide it.
    /// A cancel stops it before the next finder is asked.
    pub fn run(&self, query: &str, finders: &[Finder]) -> Result<Found, SearchError> {
        if query.trim().is_empty() {
            return Err(SearchError::Empty);
        }
        let mut found = Found::default();
        for finder in Finder::ALL
            .into_iter()
            .filter(|each| finders.contains(each))
        {
            if finder.needs_token() && self.token.is_none() {
                found.skipped.push(finder);
            } else {
                found.asked.push(finder);
            }
        }
        if found.asked.is_empty() {
            return Err(SearchError::NoneAsked {
                skipped: found.skipped,
            });
        }
        let total = found.asked.len();
        for (done, finder) in found.asked.clone().into_iter().enumerate() {
            if self.watch.is_cancelled() {
                return Err(SearchError::Cancelled);
            }
            self.watch.report(Progress::Asking {
                finder,
                done,
                total,
            });
            match self.ask(finder, query, &mut found.warnings) {
                Ok(listed) => {
                    let local = on_this_machine(self.base(finder));
                    for (at, entry) in listed.into_iter().enumerate() {
                        match entry.and_then(|listed| listed.resolved(local)) {
                            Ok(hit) => found.add(finder, hit),
                            Err((name, why)) => found.failures.push(Failure::Dropped {
                                finder,
                                at: at + 1,
                                name,
                                why: Redacted::new(&why),
                            }),
                        }
                    }
                }
                Err(failure) => found.failures.push(failure),
            }
        }
        Ok(found)
    }

    /// What `finder` answers for `query`, each result read on its own, or
    /// why there is nothing to read.
    fn ask(
        &self,
        finder: Finder,
        query: &str,
        warnings: &mut Vec<crate::http::CacheError>,
    ) -> Result<Vec<Entry>, Failure> {
        let failed = |error: FetchError| Failure::Failed { finder, error };
        let base = self.base(finder);
        let request = match finder {
            Finder::Github => github::request(base, query, self.token.as_ref()),
            Finder::SkillsSh => skills_sh::request(base, query),
            Finder::ClaudePlugins => claude_plugins::request(base, query),
        }
        .map_err(|error| failed(error.into()))?;
        let fetched = self
            .client
            .json::<serde_json::Value>(&request)
            .map_err(failed)?;
        warnings.extend(fetched.warnings);
        let read = match finder {
            Finder::Github => github::read,
            Finder::SkillsSh => skills_sh::read,
            Finder::ClaudePlugins => claude_plugins::read,
        };
        read(fetched.value).map_err(|detail| Failure::Unreadable {
            finder,
            detail: Redacted::new(&detail),
        })
    }
}

/// The URL of `path` below `base`, with `query`, each value encoded as the
/// part of a query it is: a finder's address may have a path of its own, as
/// a GitHub Enterprise server's API does (`/api/v3`).
fn below(base: &str, path: &str, query: &[(&str, &str)]) -> Result<HttpRequest, HttpError> {
    let url = query.iter().fold(
        format!("{}/{path}", base.trim_end_matches('/')),
        |url, (name, value)| with_query(&url, name, value),
    );
    HttpRequest::get(&url)
}

/// One result of a finder's answer, read or not: its name, when it said
/// one, and what is wrong with one that is not.
type Entry = Result<Listed, (Option<String>, String)>;

/// Each of `items` read as `T` and then as a result by `listed`, what
/// serde said of one that does not read made printable, and one that does
/// not said by the string under `label`, which names it in the answer.
fn entries<T: serde::de::DeserializeOwned>(
    items: Vec<serde_json::Value>,
    label: &str,
    listed: impl Fn(T) -> Result<Listed, String>,
) -> Vec<Entry> {
    items
        .into_iter()
        .map(|item| {
            let name = item
                .get(label)
                .and_then(serde_json::Value::as_str)
                .map(printable);
            serde_json::from_value::<T>(item)
                .map_err(|error| printable(&error.to_string()))
                .and_then(&listed)
                .map_err(|why| (name, why))
        })
        .collect()
}

/// A count as a finder gives one: a whole number, or nothing known, so a
/// count given in another form is no reason to drop what it counts.
fn count(value: Option<&serde_json::Value>) -> Option<u64> {
    value.and_then(serde_json::Value::as_u64)
}

/// One result as a finder listed it, before it is held to what a git source
/// is.
#[derive(Debug, Default)]
struct Listed {
    name: Option<String>,
    /// The repository: a URL, GitHub's `owner/repo`, or a forge's address
    /// of a folder, which names a branch and the folder too.
    address: Option<String>,
    /// The folder in it, where the finder says it apart from the address.
    path: Option<String>,
    installs: Option<u64>,
    stars: Option<u64>,
}

impl Listed {
    /// The result, if its address names a git source a gather would take,
    /// from a finder on this machine when `local`; its name and why it is
    /// not otherwise.
    ///
    /// A folder on this machine is taken from a finder on this machine
    /// alone, a person's own or a test's: a service elsewhere naming one
    /// would have a gather put on the shelf whatever a folder here holds,
    /// which nothing it lists should reach. An address carrying a
    /// credential is dropped without being repeated, and a folder that
    /// climbs out of the repository is refused here, as `gather git
    /// --path` refuses it.
    fn resolved(self, local: bool) -> Result<Hit, (Option<String>, String)> {
        let name = self.name.as_deref().map(printable);
        let refused = |why: &str| (name.clone(), why.to_owned());
        let Some(typed) = self.address.as_deref().filter(|typed| !typed.is_empty()) else {
            return Err(refused("it names no repository"));
        };
        if printable(typed) != typed {
            return Err(refused(
                "its repository has a control character or a space at either end",
            ));
        }
        if carries_credentials(typed) {
            return Err(refused("its repository's address carries a credential"));
        }
        let located = url::located(typed);
        if names_folder(&located.url) && !local {
            return Err(refused(
                "it names a folder on this machine, which only a finder on this machine may",
            ));
        }
        let path = match self.path.or(located.path) {
            Some(path) => {
                let path = path.trim_matches('/');
                if printable(path) != path {
                    return Err(refused(
                        "its folder has a control character or a space at either end",
                    ));
                }
                let Some(route) = route_of(path) else {
                    return Err(refused("its folder climbs out of the repository"));
                };
                (route != DEFAULT_SUBDIRECTORY).then_some(route)
            }
            None => None,
        };
        let reference = located.reference.filter(|reference| !reference.is_empty());
        if reference
            .as_deref()
            .is_some_and(|reference| printable(reference) != reference)
        {
            return Err(refused(
                "its branch has a control character or a space at either end",
            ));
        }
        let canonical = url::canonical(&located.url);
        let named = name
            .clone()
            .filter(|name| !name.is_empty())
            .or_else(|| {
                path.as_deref()
                    .and_then(|path| path.rsplit('/').next())
                    .map(str::to_owned)
            })
            .or_else(|| last_part(&canonical));
        let Some(named) = named else {
            return Err(refused("it has no name"));
        };
        Ok(Hit {
            name: named,
            repository: Redacted::new(&canonical),
            path,
            reference,
            installs: self.installs,
            stars: self.stars,
            finders: Vec::new(),
        })
    }
}

/// What a search found, what it asked, and what it could not read.
#[derive(Debug, Default)]
pub struct Found {
    /// Each skill, once, in [`Finder::ALL`]'s order and each finder's own.
    pub hits: Vec<Hit>,
    /// Each finder asked, in the order it was.
    pub asked: Vec<Finder>,
    /// Each finder to ask that needs a token, and was given none: sent
    /// nothing.
    pub skipped: Vec<Finder>,
    /// Each finder that failed, and each result dropped, beside the rest.
    pub failures: Vec<Failure>,
    /// What the cache could not do on the way, which stopped nothing.
    pub warnings: Vec<crate::http::CacheError>,
}

impl Found {
    /// The result listed `number`, counting from 1, as a search lists them
    /// and `--take` names one.
    pub fn take(&self, number: usize) -> Result<&Hit, SearchError> {
        number
            .checked_sub(1)
            .and_then(|at| self.hits.get(at))
            .ok_or(SearchError::NoSuchResult {
                asked: number,
                found: self.hits.len(),
            })
    }

    /// `hit`, found by `finder`, listed once however many finders name it:
    /// by its repository, as [`url::canonical`] spells every spelling of
    /// one, and its folder there, which holds one skill.
    ///
    /// A result with no folder, which a finder that does not say where a
    /// skill is gives (skills.sh), is told by its name in its repository:
    /// a repository holds many skills, and by the repository alone every
    /// skill such a finder lists of one would be folded into the first,
    /// unsaid. Folded into one that has a folder, it takes that folder, the
    /// branch it was found at with it, so a take gathers what was found
    /// rather than the whole repository.
    fn add(&mut self, finder: Finder, hit: Hit) {
        let same = |each: &Hit| {
            each.repository == hit.repository
                && match (&each.path, &hit.path) {
                    (Some(held), Some(path)) => held == path,
                    _ => each.name == hit.name,
                }
        };
        match self.hits.iter_mut().find(|each| same(each)) {
            Some(each) => {
                if !each.finders.contains(&finder) {
                    each.finders.push(finder);
                }
                if each.path.is_none() && hit.path.is_some() {
                    each.path = hit.path;
                    each.reference = hit.reference;
                }
                each.installs = each.installs.or(hit.installs);
                each.stars = each.stars.or(hit.stars);
            }
            None => self.hits.push(Hit {
                finders: vec![finder],
                ..hit
            }),
        }
    }
}

/// By hand: what went wrong goes out as errors do, each its code and its
/// message ([`Said`]), and what the cache could not do is the envelope's,
/// said beside it as a warning, rather than data.
impl Serialize for Found {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut found = serializer.serialize_struct("Found", 4)?;
        found.serialize_field("hits", &self.hits)?;
        found.serialize_field("asked", &self.asked)?;
        found.serialize_field("skipped", &self.skipped)?;
        found.serialize_field(
            "failures",
            &self
                .failures
                .iter()
                .map(|failure| Said::of(failure.code(), failure))
                .collect::<Vec<_>>(),
        )?;
        found.end()
    }
}

/// One skill a search found: where it is, as `gather git` takes it, and
/// what the finders say of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hit {
    /// What it is called, printable: the name a finder gave it, or its
    /// folder's, or its repository's for one at the root.
    pub name: String,
    /// The repository it is in, as [`url::canonical`] spells it.
    #[serde(deserialize_with = "redacting")]
    pub repository: Redacted,
    /// The folder in the repository it is in: none for the root, or where
    /// no finder said, which a take gathers the whole repository from.
    pub path: Option<String>,
    /// The branch a finder named, none for the repository's default.
    pub reference: Option<String>,
    /// How many times it was installed, where a finder counts that.
    pub installs: Option<u64>,
    /// How many stars its repository has, where a finder counts them.
    pub stars: Option<u64>,
    /// Each finder that found it, in the order they were asked.
    pub finders: Vec<Finder>,
}

impl Hit {
    /// The folder a gather that takes it looks in: its own, or the
    /// repository's root, where every layout a gather knows is looked for.
    pub fn folder(&self) -> &str {
        self.path.as_deref().unwrap_or(DEFAULT_SUBDIRECTORY)
    }
}

/// A repository read back as text, made a [`Redacted`] as every URL is,
/// whatever it holds.
fn redacting<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Redacted, D::Error> {
    String::deserialize(deserializer).map(|text| Redacted::new(&text))
}

/// Why a search asked nothing, or took nothing.
#[derive(Debug, Error)]
pub enum SearchError {
    /// Nothing to look for, which no finder answers.
    #[error("say what to look for: an empty query finds nothing")]
    Empty,
    /// The marker enables no finder, and none was named.
    #[error(
        "no finder is enabled, so nothing was searched: `finders` in {} lists none; \
         github is GitHub's code search, and skills.sh and claude-plugins.dev are unstable \
         and undocumented, so read their terms before listing them",
        .config.display()
    )]
    NoneEnabled { config: PathBuf },
    /// A finder named that the marker does not enable.
    #[error(
        "{finder} is not enabled, so it was not asked: list it in `finders` in {} to search it{}",
        .config.display(),
        if .finder.is_unstable() {
            ", having read its terms: its API is unstable and undocumented"
        } else {
            ""
        }
    )]
    NotEnabled { finder: Finder, config: PathBuf },
    /// Every finder to ask needs a token, and none was handed over.
    #[error(
        "nothing was searched: {} needs a token, handed over with --token-env or --gh, \
         and no finder that needs none is enabled",
        listing(.skipped)
    )]
    NoneAsked { skipped: Vec<Finder> },
    /// `--finder-url` given what names no finder.
    #[error(
        "`{given}` names no finder's address: give a finder's name (github, skills.sh, \
         claude-plugins.dev), `=` and the address it is asked at"
    )]
    Unaddressed { given: Redacted },
    /// An address a finder may not be asked at ([`Search::at`]).
    #[error("{url}: {finder} is not asked there: {why}")]
    Address {
        finder: Finder,
        url: Redacted,
        why: String,
    },
    /// `--take` naming a result the search did not list.
    #[error("there is no result {asked} to take: {}", results(*found))]
    NoSuchResult { asked: usize, found: usize },
    /// Its [`Watch`] was cancelled before every finder was asked: nothing
    /// is written by a search, so nothing is left half done.
    #[error("cancelled, before every finder was asked")]
    Cancelled,
}

codes!(SearchError {
    Self::Empty => "search_query_empty",
    Self::NoneEnabled { .. } => "finder_none_enabled",
    Self::NotEnabled { .. } => "finder_not_enabled",
    Self::NoneAsked { .. } => "finder_none_asked",
    Self::Unaddressed { .. } => "finder_url_unnamed",
    Self::Address { .. } => "finder_url_invalid",
    Self::NoSuchResult { .. } => "search_result_missing",
    Self::Cancelled => "search_cancelled",
});

/// Finders by name, `github` or `github and skills.sh`.
fn listing(finders: &[Finder]) -> String {
    let names: Vec<&str> = finders.iter().map(|finder| finder.name()).collect();
    match names.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => String::from("no finder"),
    }
}

/// How many results a search found, as a refusal to take another says it.
fn results(found: usize) -> String {
    match found {
        0 => String::from("the search found none"),
        1 => String::from("the search found 1"),
        found => format!("the search found {found}"),
    }
}

/// A finder that failed, or a result of one dropped, said beside what the
/// rest found.
#[derive(Debug, Error)]
pub enum Failure {
    /// Its answer did not come back: refused, timed out, rate limited.
    #[error("{finder}: {error}")]
    Failed { finder: Finder, error: FetchError },
    /// Its answer came back in a shape this build does not read: an API
    /// nobody documents that changed, and nothing it found is listed.
    #[error(
        "{finder}: its answer is not in the shape this build reads, so nothing it found is \
         listed: {detail}"
    )]
    Unreadable { finder: Finder, detail: Redacted },
    /// One result that does not read, or names no git source a gather would
    /// take.
    #[error(
        "{finder}: result {at}{}: {why}, so it is not listed",
        .name.as_deref().map(|name| format!(" (`{name}`)")).unwrap_or_default()
    )]
    Dropped {
        finder: Finder,
        /// Where it is among the finder's results, counting from 1.
        at: usize,
        /// Its name, when it said one, printable.
        name: Option<String>,
        /// Why, which may quote what the finder said of it, a URL among it.
        why: Redacted,
    },
}

impl Failure {
    /// The finder it is of.
    pub fn finder(&self) -> Finder {
        match self {
            Failure::Failed { finder, .. }
            | Failure::Unreadable { finder, .. }
            | Failure::Dropped { finder, .. } => *finder,
        }
    }
}

codes!(Failure {
    Self::Unreadable { .. } => "finder_answer_unreadable",
    Self::Dropped { .. } => "finder_result_dropped",
} carried {
    Self::Failed { error, .. } => error,
});

/// `text` with every control character a space, and trimmed: what a finder
/// says, which a terminal would otherwise act on, a carriage return in a
/// name letting a row overwrite the one above it.
fn printable(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_owned()
}

/// Whether `url`, as [`url::located`] reads what a finder named, is a folder
/// on this machine: a path, or a `file://` URL, as `gather git` takes one.
fn names_folder(url: &str) -> bool {
    is_path(url)
        || url
            .get(..7)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file://"))
}

/// Whether `url`'s scheme is `scheme`, in any case.
fn scheme_is(url: &str, scheme: &str) -> bool {
    url.split_once("://")
        .is_some_and(|(given, _)| given.eq_ignore_ascii_case(scheme))
}

/// The host `url` names, in lowercase, without a login or a port, an IPv6
/// one in its brackets.
fn host_of(url: &str) -> Option<String> {
    let (_, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host_and_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = match host_and_port.strip_prefix('[') {
        Some(inside) => format!("[{}]", inside.split_once(']')?.0),
        None => host_and_port
            .split_once(':')
            .map_or(host_and_port, |(host, _)| host)
            .to_owned(),
    };
    Some(host.to_ascii_lowercase())
}

/// Whether `url` is asked of this machine: `localhost`, or a loopback
/// address, as a test's finder and a person's own are.
fn on_this_machine(url: &str) -> bool {
    host_of(url).is_some_and(|host| {
        host == "localhost"
            || host == "[::1]"
            || host
                .parse::<std::net::Ipv4Addr>()
                .is_ok_and(|address| address.is_loopback())
    })
}

/// The last part of `url`'s path, a repository's name.
fn last_part(url: &str) -> Option<String> {
    url.trim_end_matches('/')
        .rsplit(['/', ':'])
        .next()
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
#[path = "finder_test.rs"]
mod tests;
