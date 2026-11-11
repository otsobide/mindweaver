//! Gathering from a Claude Code plugin marketplace (#103, D-13).
//!
//! A marketplace is a git repository whose catalog,
//! `.claude-plugin/marketplace.json`, lists plugins, each in the
//! marketplace's own repository or in a repository of its own. It is a
//! source of its own kind ([`SourceKind::Marketplace`]), its catalog's
//! repository, and each plugin gathered from it is a git source whose
//! parent it is ([`Source::parent`]), known by its entry in the catalog,
//! gathered only when it is chosen: by name (`--plugin`), or every one
//! (`--all`), which waits for a yes past [`ASKED_BEYOND`], since a
//! catalog can list thousands and each is a repository to fetch.
//!
//! What a catalog says is somebody else's JSON, read leniently and never
//! obeyed: a key this build does not know is passed over, an entry that
//! cannot be read is said beside the rest and costs none of them, a
//! plugin's skills are all that is taken of it, what else it holds (its
//! commands, agents, hooks, MCP and LSP servers) said and left, and a
//! source that is a command to run, or a `headersHelper` naming one, is
//! never run. Only a plugin kept in git is gathered: one from npm or an
//! archive is said and passed over, and so is one a catalog fetched from
//! elsewhere names by a path on this machine, which a catalog has no
//! business reaching, as a link in a clone leads nowhere out of it.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::adapter::gix::Gix;
use crate::code::codes;
use crate::copy;
use crate::gather::{self, route_of, GatherError, Progress, Report, Watch, DEFAULT_SUBDIRECTORY};
use crate::ledger::{Action, Ledger, LedgerError, Outcome, Source, SourceKind};
use crate::lock::LockError;
use crate::port::{Cloned, GitError, Remote, Token};
use crate::url::{carries_credentials, fetched, is_path, located, redacted};
use crate::value::{Pin, Redacted, Revision, SourceName};
use crate::workspace::WeaverWorkspace;

/// Where a marketplace keeps its catalog, from its repository's root.
pub const CATALOG: &str = ".claude-plugin/marketplace.json";

/// How many plugins `--all` gathers unasked: more is a list of what it
/// would gather, until a yes says to (`--yes`).
///
/// claude-community lists 2283 plugins, each a repository to fetch, so
/// gathering a catalog whole is something to have meant. A catalog of a
/// team's own few is not.
pub const ASKED_BEYOND: usize = 25;

/// How much of a catalog is read: more than any catalog listing thousands
/// needs, and a bound on what a repository can make a gather read.
const READ_AT_MOST: u64 = 8 * 1024 * 1024;

/// How much of a plugin's own manifest is read for its licence.
const MANIFEST_AT_MOST: u64 = 1024 * 1024;

/// Where a plugin keeps its own manifest, from its folder.
const PLUGIN_MANIFEST: &str = ".claude-plugin/plugin.json";

/// The longest name a plugin is gathered under: Claude Code's own names are
/// a few words, and this one is a source's name and a folder's.
const NAME_AT_MOST: usize = 64;

/// What to gather from a marketplace, and how.
#[derive(Debug, Clone)]
pub struct Request {
    /// The marketplace's repository, as somebody typed it: `owner/repo`, a
    /// git URL or a folder on this machine, at a branch with `@ref`.
    pub url: String,
    /// A branch or tag of it, `--ref`.
    pub reference: Option<String>,
    /// What to call it, `--name`, in place of its catalog's own name.
    pub name: Option<String>,
    /// Which of its plugins to gather.
    pub chosen: Chosen,
    /// Whether `--all` was told yes, for a catalog listing more than
    /// [`ASKED_BEYOND`] to gather.
    pub yes: bool,
    /// Told how it is going, and able to stop it between two plugins or
    /// before one starts placing.
    pub watch: Watch,
    /// What fetches the catalog and each plugin of a repository of its own.
    pub remote: Arc<dyn Remote>,
    /// A token for the catalog's host (D-16), handed to the catalog's
    /// fetch and to each plugin's on that host alone.
    pub token: Option<Token>,
}

/// Which of a marketplace's plugins a gather of it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Chosen {
    /// None: its catalog is recorded, and listed (`--list`).
    Listed,
    /// These, by their names in the catalog (`--plugin`), refused whole
    /// for a name it does not list, before any is fetched.
    Plugins(Vec<String>),
    /// Each the catalog lists (`--all`).
    All,
    /// These again, gathered before (`gather update`): one the catalog no
    /// longer lists is said, and the rest gathered.
    Again(Vec<String>),
}

impl Request {
    /// List the catalog of the marketplace at `url`, gathering nothing.
    pub fn listing(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            reference: None,
            name: None,
            chosen: Chosen::Listed,
            yes: false,
            watch: Watch::new(),
            remote: Arc::new(Gix),
            token: None,
        }
    }

    /// Gather the plugins called `names` from it.
    pub fn plugins(mut self, names: Vec<String>) -> Self {
        self.chosen = Chosen::Plugins(names);
        self
    }

    /// Gather every plugin it lists, `yes` said or not.
    pub fn all(mut self, yes: bool) -> Self {
        self.chosen = Chosen::All;
        self.yes = yes;
        self
    }

    /// At that branch or tag.
    pub fn at(mut self, reference: Option<impl Into<String>>) -> Self {
        self.reference = reference.map(Into::into);
        self
    }

    /// Called `name`, when one is given.
    pub fn named(mut self, name: Option<impl Into<String>>) -> Self {
        self.name = name.map(Into::into);
        self
    }

    pub fn watched_by(mut self, watch: Watch) -> Self {
        self.watch = watch;
        self
    }

    /// Fetched through `remote` rather than [`Gix`].
    pub fn through(mut self, remote: Arc<dyn Remote>) -> Self {
        self.remote = remote;
        self
    }

    pub fn with_token(mut self, token: Option<Token>) -> Self {
        self.token = token;
        self
    }

    /// The marketplace `source` gathered again (`gather update`), as the
    /// ledger records it, `entries` of it again, fetched as `how` says, a
    /// front end's gather of it ([`gather::Request::again`]): its watch, its
    /// remote and its token.
    pub fn again(source: &Source, entries: Vec<String>, how: &gather::Request) -> Self {
        Self {
            url: source.url.to_string(),
            reference: source.reference.clone(),
            name: None,
            chosen: Chosen::Again(entries),
            yes: true,
            watch: how.watch.clone(),
            remote: Arc::clone(&how.remote),
            token: how.token.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// The catalog
// ---------------------------------------------------------------------------

/// What a marketplace's catalog says of itself, and every plugin it lists
/// that can be read, in the order it lists them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "MarketplaceCatalog"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "MarketplaceCatalog"))]
pub struct Catalog {
    /// What it calls itself, as written: what its source is called unless
    /// `--name` says otherwise, a name being free.
    pub name: Option<String>,
    pub description: Option<String>,
    /// Who keeps it, by the name it gives.
    pub owner: Option<String>,
    pub entries: Vec<Entry>,
    /// Each plugin it lists that could not be read, said beside the rest:
    /// not serialized, a front end saying each as a warning, which in JSON
    /// are the envelope's.
    #[serde(skip)]
    pub failures: Vec<UnreadEntry>,
}

impl Catalog {
    /// The catalog `value` is, a JSON document as somebody wrote it, of the
    /// marketplace fetched from `from`: an object listing `plugins`, or why
    /// it is not one. Each plugin is read on its own ([`entry`]), one that
    /// cannot be among the failures; `from` says whether a plugin it names
    /// on this machine is gathered ([`Place::refusal`]).
    pub fn of(value: &Value, from: &str) -> Result<Catalog, String> {
        let here = on_this_machine(from);
        let object = value
            .as_object()
            .ok_or_else(|| String::from("it is not a JSON object"))?;
        let plugins = object
            .get("plugins")
            .and_then(Value::as_array)
            .ok_or_else(|| String::from("it lists no `plugins`"))?;
        let metadata = object.get("metadata").and_then(Value::as_object);
        // Where a plugin named bare is: a route from the repository's root,
        // or why it leads nowhere a plugin can be.
        let root = match metadata.and_then(|metadata| text(metadata.get("pluginRoot"))) {
            None => Ok(String::from(DEFAULT_SUBDIRECTORY)),
            Some(typed) => route_of(&typed).ok_or(typed),
        };
        let mut entries = Vec::new();
        let mut failures = Vec::new();
        for (at, plugin) in plugins.iter().enumerate() {
            match entry(plugin, at + 1, &root, here) {
                Ok(entry) => entries.push(entry),
                Err(failure) => failures.push(failure),
            }
        }
        // One name is one plugin: two the same, in any case, are neither,
        // since which of them `--plugin` means is not a gather's to pick.
        let mut seen: Vec<String> = Vec::new();
        let mut twice: HashSet<String> = HashSet::new();
        for entry in &entries {
            let folded = entry.name.to_ascii_lowercase();
            if seen.contains(&folded) {
                twice.insert(folded);
            } else {
                seen.push(folded);
            }
        }
        entries.retain(|entry| {
            let clashes = twice.contains(&entry.name.to_ascii_lowercase());
            if clashes {
                failures.push(UnreadEntry::Twice {
                    name: entry.name.clone(),
                });
            }
            !clashes
        });
        Ok(Catalog {
            name: text(object.get("name")),
            description: metadata
                .and_then(|metadata| text(metadata.get("description")))
                .or_else(|| text(object.get("description"))),
            owner: object
                .get("owner")
                .and_then(|owner| text(owner.get("name")).or_else(|| text(Some(owner)))),
            entries,
            failures,
        })
    }

    /// The plugin the catalog lists as `name`, exactly.
    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    /// How many of its plugins a gather takes: those kept in git.
    pub fn gatherable(&self) -> usize {
        self.entries.iter().filter(|entry| entry.gatherable).count()
    }

    /// Whether `--all` waits for a yes on it: past [`ASKED_BEYOND`] plugins
    /// to gather.
    pub fn asks_first(&self) -> bool {
        self.gatherable() > ASKED_BEYOND
    }
}

/// One plugin a catalog lists, as it says it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "PluginEntry"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "PluginEntry"))]
pub struct Entry {
    /// What it is called, which `--plugin` takes.
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    /// Where it is.
    pub source: Place,
    /// Where its skills are, each a route from its folder, when its entry
    /// lists them; `None` when it lists none, and a gather looks where
    /// Claude Code does ([`gather::plugin`]).
    pub skills: Option<Vec<String>>,
    /// The licence it declares, as written, which a skill of it saying
    /// none is under.
    pub licence: Option<String>,
    /// What its entry declares beside skills, which is not gathered.
    pub declares: Vec<Extra>,
    /// Whether its entry names a `headersHelper`, a command that prints the
    /// headers a fetch of it would send, which is never run.
    pub headers_helper: bool,
    /// Whether a gather takes it: a plugin kept in git.
    pub gatherable: bool,
    /// What it is called as a source once it is gathered, after its
    /// marketplace (`acme-tools.deploy`), and `None` until it is.
    #[serde(default, deserialize_with = "maybe_named")]
    pub gathered: Option<SourceName>,
}

/// Where a plugin a catalog lists is: what its entry's `source` says.
///
/// Tagged by `type`, each spelled here rather than from the variant's
/// name, as Claude Code spells a source's kind, beside what it says of it:
/// `{"type": "relative", "path": "plugins/deploy"}`, `{"type": "github",
/// "repo": "acme/deploy", "ref": "v2", "sha": null}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "PluginPlace"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "PluginPlace"))]
#[serde(tag = "type")]
pub enum Place {
    /// In the marketplace's own repository, at `path`, a route from its
    /// root: `./path`, or a name under the catalog's `pluginRoot`.
    #[serde(rename = "relative")]
    Relative { path: String },
    /// A GitHub repository, `owner/repo`.
    #[serde(rename = "github")]
    Github {
        repo: String,
        #[serde(rename = "ref")]
        reference: Option<String>,
        #[serde(default, deserialize_with = "pinned")]
        sha: Option<Pin>,
    },
    /// A git repository at a URL.
    #[serde(rename = "url")]
    Url {
        #[serde(deserialize_with = "redacting")]
        url: Redacted,
        #[serde(rename = "ref")]
        reference: Option<String>,
        #[serde(default, deserialize_with = "pinned")]
        sha: Option<Pin>,
    },
    /// A folder of a git repository, `path` a route from its root.
    #[serde(rename = "git-subdir")]
    GitSubdir {
        #[serde(deserialize_with = "redacting")]
        url: Redacted,
        path: String,
        #[serde(rename = "ref")]
        reference: Option<String>,
        #[serde(default, deserialize_with = "pinned")]
        sha: Option<Pin>,
    },
    /// An npm package, which is not gathered.
    #[serde(rename = "npm")]
    Npm { package: Option<String> },
    /// An archive to download, which is not gathered.
    #[serde(rename = "archive")]
    Archive,
    /// A command to run, which is never run.
    #[serde(rename = "command")]
    Command,
    /// A kind of source this build does not read, as the catalog names it.
    #[serde(rename = "other")]
    Other { kind: String },
}

/// Where a plugin kept in git is fetched from.
struct Fetched<'a> {
    url: String,
    reference: Option<&'a str>,
    sha: Option<&'a Pin>,
    folder: String,
}

impl Place {
    /// How a listing names it, in a few words: `./plugins/deploy`, `github
    /// acme/deploy@v2`, a URL and its folder.
    pub fn said(&self) -> String {
        let at = |reference: &Option<String>| {
            reference
                .as_deref()
                .map_or_else(String::new, |reference| format!("@{reference}"))
        };
        match self {
            Place::Relative { path } if path == DEFAULT_SUBDIRECTORY => String::from("./"),
            Place::Relative { path } => format!("./{path}"),
            Place::Github {
                repo, reference, ..
            } => format!("github {repo}{}", at(reference)),
            Place::Url { url, reference, .. } => format!("{url}{}", at(reference)),
            Place::GitSubdir {
                url,
                path,
                reference,
                ..
            } => format!("{url}{}, path {path}", at(reference)),
            Place::Npm { package } => match package {
                Some(package) => format!("npm {package}"),
                None => String::from("npm"),
            },
            Place::Archive => String::from("archive"),
            Place::Command => String::from("command"),
            Place::Other { kind } => kind.clone(),
        }
    }

    /// The commit it is pinned to, when its entry pins it.
    pub fn sha(&self) -> Option<&Pin> {
        match self {
            Place::Github { sha, .. } | Place::Url { sha, .. } | Place::GitSubdir { sha, .. } => {
                sha.as_ref()
            }
            Place::Relative { .. }
            | Place::Npm { .. }
            | Place::Archive
            | Place::Command
            | Place::Other { .. } => None,
        }
    }

    /// Why a gather does not take the plugin `entry` from here, when it
    /// does not: what is not kept in git, and, for a catalog fetched from
    /// elsewhere, `here` false, a repository of its own on this machine,
    /// a path or a `file://` URL.
    ///
    /// A catalog is somebody else's, and gathering one of its plugins
    /// fetches whatever it names: one from GitHub naming a folder of the
    /// machine it is gathered on would put that folder's skills on the
    /// shelf under the catalog's name. One on this machine may name its
    /// neighbours, which is how a team keeps a catalog beside its plugins.
    pub fn refusal(&self, entry: &str, here: bool) -> Option<NotGathered> {
        let entry = entry.to_owned();
        match self {
            Place::Relative { .. } | Place::Github { .. } => None,
            // As it is fetched, so `owner/repo` is GitHub's, as it is for
            // a `git-subdir`.
            Place::Url { .. } | Place::GitSubdir { .. } => (!here
                && self
                    .fetched()
                    .is_some_and(|fetched| on_this_machine(&fetched.url)))
            .then_some(NotGathered::Local { entry }),
            Place::Npm { .. } => Some(NotGathered::Npm { entry }),
            Place::Archive => Some(NotGathered::Archive { entry }),
            Place::Command => Some(NotGathered::Command { entry }),
            Place::Other { kind } => Some(NotGathered::Unknown {
                entry,
                kind: kind.clone(),
            }),
        }
    }

    /// Where a plugin of a repository of its own is fetched from: `None`
    /// for one in the marketplace's repository, which is fetched already,
    /// and for one no gather takes.
    fn fetched(&self) -> Option<Fetched<'_>> {
        match self {
            Place::Github {
                repo,
                reference,
                sha,
            } => Some(Fetched {
                url: fetched(repo).into_owned(),
                reference: reference.as_deref(),
                sha: sha.as_ref(),
                folder: String::from(DEFAULT_SUBDIRECTORY),
            }),
            Place::Url {
                url,
                reference,
                sha,
            } => Some(Fetched {
                url: url.to_string(),
                reference: reference.as_deref(),
                sha: sha.as_ref(),
                folder: String::from(DEFAULT_SUBDIRECTORY),
            }),
            Place::GitSubdir {
                url,
                path,
                reference,
                sha,
            } => Some(Fetched {
                url: fetched(url).into_owned(),
                reference: reference.as_deref(),
                sha: sha.as_ref(),
                folder: path.clone(),
            }),
            Place::Relative { .. }
            | Place::Npm { .. }
            | Place::Archive
            | Place::Command
            | Place::Other { .. } => None,
        }
    }
}

/// What a plugin holds or declares beside skills, which a gather does not
/// take: said, and left where it is.
///
/// Matched with no wildcard, and written as the words a listing names each
/// by, so a rename of a variant changes neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Extra {
    Commands,
    Agents,
    Hooks,
    McpServers,
    LspServers,
}

impl Extra {
    /// Every one, in the order a listing says them.
    pub const ALL: [Extra; 5] = [
        Extra::Commands,
        Extra::Agents,
        Extra::Hooks,
        Extra::McpServers,
        Extra::LspServers,
    ];

    /// The word JSON holds it as.
    pub const fn slug(self) -> &'static str {
        match self {
            Extra::Commands => "commands",
            Extra::Agents => "agents",
            Extra::Hooks => "hooks",
            Extra::McpServers => "mcp_servers",
            Extra::LspServers => "lsp_servers",
        }
    }

    /// The words a sentence names them by.
    pub const fn said(self) -> &'static str {
        match self {
            Extra::Commands => "commands",
            Extra::Agents => "agents",
            Extra::Hooks => "hooks",
            Extra::McpServers => "MCP servers",
            Extra::LspServers => "LSP servers",
        }
    }

    /// The key a catalog entry declares it under, as Claude Code reads it.
    const fn key(self) -> &'static str {
        match self {
            Extra::Commands => "commands",
            Extra::Agents => "agents",
            Extra::Hooks => "hooks",
            Extra::McpServers => "mcpServers",
            Extra::LspServers => "lspServers",
        }
    }

    /// Where a plugin keeps it unsaid, from its folder, as Claude Code
    /// finds it there.
    const fn kept_at(self) -> &'static str {
        match self {
            Extra::Commands => "commands",
            Extra::Agents => "agents",
            Extra::Hooks => "hooks",
            Extra::McpServers => ".mcp.json",
            Extra::LspServers => ".lsp.json",
        }
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// What a plugin holds or declares beside skills, which a gather does
    /// not take.
    Extra as "PluginExtra", by slug { Commands, Agents, Hooks, McpServers, LspServers }
}

impl Serialize for Extra {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

impl<'de> Deserialize<'de> for Extra {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let word = String::deserialize(deserializer)?;
        Extra::ALL
            .into_iter()
            .find(|extra| extra.slug() == word)
            .ok_or_else(|| D::Error::custom(format!("`{word}` is nothing a plugin holds")))
    }
}

/// What a plugin `object` names that is not gathered, in order: what its
/// entry declares, and what its folder `at` holds, once it is fetched.
fn extras(object: Option<&Map<String, Value>>, at: Option<&Path>) -> Vec<Extra> {
    Extra::ALL
        .into_iter()
        .filter(|extra| {
            let declared = object
                .and_then(|object| object.get(extra.key()))
                .is_some_and(|value| !value.is_null());
            let kept = at.is_some_and(|folder| folder.join(extra.kept_at()).exists());
            declared || kept
        })
        .collect()
}

/// Whether `url`, a marketplace's or a plugin's, is on this machine: a path
/// ([`is_path`]), or a `file://` URL, which names one.
pub fn on_this_machine(url: &str) -> bool {
    is_path(url)
        || url
            .split_once("://")
            .is_some_and(|(scheme, _)| scheme.eq_ignore_ascii_case("file"))
}

/// The text `value` holds, when it holds some.
fn text(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// Whether `name` is one a plugin is gathered under: letters, digits, `.`,
/// `_` and `-`, starting with a letter or a digit, at most
/// [`NAME_AT_MOST`], so that it is a source's name after its marketplace's
/// and a folder's, and one word on a command line.
pub fn is_plugin_name(name: &str) -> bool {
    name.len() <= NAME_AT_MOST
        && name
            .bytes()
            .next()
            .is_some_and(|first| first.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The plugin `value` is, the `at`th the catalog lists, under `root`, where
/// a plugin named bare is, or why it cannot be read; `here` whether the
/// catalog is on this machine ([`Place::refusal`]).
fn entry(
    value: &Value,
    at: usize,
    root: &Result<String, String>,
    here: bool,
) -> Result<Entry, UnreadEntry> {
    let object = value.as_object().ok_or(UnreadEntry::Unnamed { at })?;
    let name = text(object.get("name")).ok_or(UnreadEntry::Unnamed { at })?;
    if !is_plugin_name(&name) {
        return Err(UnreadEntry::NameUnusable {
            name: redacted(&name).into_owned(),
        });
    }
    let unreadable = |why: &str| UnreadEntry::Unreadable {
        name: name.clone(),
        why: why.to_owned(),
    };
    let source = place(object.get("source"), &name, root)?;
    let skills = match object.get("skills") {
        None | Some(Value::Null) => None,
        Some(Value::String(one)) => Some(vec![one.clone()]),
        Some(Value::Array(listed)) => Some(
            listed
                .iter()
                .map(|one| one.as_str().map(str::to_owned))
                .collect::<Option<Vec<String>>>()
                .ok_or_else(|| unreadable("its `skills` lists something other than a path"))?,
        ),
        Some(_) => {
            return Err(unreadable(
                "its `skills` is neither a path nor a list of them",
            ))
        }
    };
    let headers_helper = object.contains_key("headersHelper")
        || object
            .get("source")
            .and_then(Value::as_object)
            .is_some_and(|source| source.contains_key("headersHelper"));
    Ok(Entry {
        description: text(object.get("description")),
        version: text(object.get("version")),
        skills,
        licence: text(object.get("license")),
        declares: extras(Some(object), None),
        headers_helper,
        gatherable: source.refusal(&name, here).is_none(),
        source,
        gathered: None,
        name,
    })
}

/// Where the plugin `name`'s `source` says it is, under `root`.
fn place(
    source: Option<&Value>,
    name: &str,
    root: &Result<String, String>,
) -> Result<Place, UnreadEntry> {
    let unreadable = |why: String| UnreadEntry::Unreadable {
        name: name.to_owned(),
        why,
    };
    // What a catalog may not say, a credential, is refused before it is
    // repeated, and said without it.
    let held = |key: &str, value: Option<&Value>| -> Result<Option<String>, UnreadEntry> {
        match value {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(text)) if carries_credentials(text) => {
                Err(UnreadEntry::Credentialed {
                    name: name.to_owned(),
                    text: redacted(text).into_owned(),
                })
            }
            Some(Value::String(text)) => Ok(Some(text.clone())),
            Some(_) => Err(unreadable(format!("its `{key}` is no text"))),
        }
    };
    let object = match source {
        None | Some(Value::Null) => return Err(unreadable(String::from("it names no `source`"))),
        Some(Value::String(path)) => {
            if carries_credentials(path) {
                return Err(UnreadEntry::Credentialed {
                    name: name.to_owned(),
                    text: redacted(path).into_owned(),
                });
            }
            return relative(path, root).map_err(unreadable);
        }
        Some(Value::Object(object)) => object,
        Some(_) => {
            return Err(unreadable(String::from(
                "its `source` is neither a path nor an object",
            )))
        }
    };
    let required = |key: &str| -> Result<String, UnreadEntry> {
        held(key, object.get(key))?
            .ok_or_else(|| unreadable(format!("its `source` names no `{key}`")))
    };
    let reference = || held("ref", object.get("ref"));
    let sha = || -> Result<Option<Pin>, UnreadEntry> {
        match held("sha", object.get("sha"))? {
            None => Ok(None),
            Some(typed) => Pin::parse(&typed).map(Some).ok_or_else(|| {
                unreadable(format!(
                    "its `sha`, `{typed}`, is no commit in full, 40 hexadecimal digits"
                ))
            }),
        }
    };
    let kind = held("source", object.get("source"))?
        .ok_or_else(|| unreadable(String::from("its `source` names no kind of source")))?;
    match kind.as_str() {
        "github" => {
            let repo = required("repo")?;
            // `owner/repo` and nothing else, which is a GitHub repository.
            if fetched(&repo) == repo.as_str() {
                return Err(unreadable(format!(
                    "its `repo`, `{repo}`, is no `owner/repo` on GitHub"
                )));
            }
            Ok(Place::Github {
                repo,
                reference: reference()?,
                sha: sha()?,
            })
        }
        "url" => Ok(Place::Url {
            url: Redacted::new(&required("url")?),
            reference: reference()?,
            sha: sha()?,
        }),
        "git-subdir" => {
            let url = required("url")?;
            let typed = required("path")?;
            let path = route_of(&typed).ok_or_else(|| {
                unreadable(format!(
                    "its `path`, `{typed}`, is no folder inside the repository"
                ))
            })?;
            Ok(Place::GitSubdir {
                url: Redacted::new(&url),
                path,
                reference: reference()?,
                sha: sha()?,
            })
        }
        "npm" => Ok(Place::Npm {
            package: held("package", object.get("package"))?,
        }),
        "archive" => Ok(Place::Archive),
        // Never run, nor kept: what it would run is nothing a gather needs.
        "command" => Ok(Place::Command),
        other => Ok(Place::Other {
            kind: other.to_owned(),
        }),
    }
}

/// A plugin in the marketplace's own repository at `typed`: `./path` and
/// `.` from its root, and a name bare under its `pluginRoot`, `root`, as
/// Claude Code reads one. Refused when it climbs out of the repository.
fn relative(typed: &str, root: &Result<String, String>) -> Result<Place, String> {
    // Held to a route of its own first, so one starting at a root of its
    // own, or climbing, is refused wherever it would be joined.
    let own = route_of(typed)
        .ok_or_else(|| format!("its `source`, `{typed}`, is no folder inside the repository"))?;
    let from_root = typed == DEFAULT_SUBDIRECTORY || typed.starts_with("./");
    let path = match (from_root, root) {
        (true, _) => own,
        (false, Err(root)) => {
            return Err(format!(
                "the catalog's `pluginRoot`, `{root}`, is no folder inside the repository"
            ))
        }
        (false, Ok(root)) if root == DEFAULT_SUBDIRECTORY => own,
        (false, Ok(root)) => format!("{root}/{own}"),
    };
    Ok(Place::Relative { path })
}

/// A pin read back as the text it was written, held to being one.
fn pinned<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Pin>, D::Error> {
    match Option::<String>::deserialize(deserializer)? {
        None => Ok(None),
        Some(text) => Pin::parse(&text)
            .map(Some)
            .ok_or_else(|| D::Error::custom(format!("`{text}` is no commit in full"))),
    }
}

/// A URL read back as text, made a [`Redacted`] as every URL is.
fn redacting<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Redacted, D::Error> {
    String::deserialize(deserializer).map(|text| Redacted::new(&text))
}

/// A source's name read back as the text it was written, held to being
/// one.
fn named<'de, D: Deserializer<'de>>(deserializer: D) -> Result<SourceName, D::Error> {
    let text = String::deserialize(deserializer)?;
    SourceName::parse(&text).ok_or_else(|| D::Error::custom(format!("`{text}` names no source")))
}

fn maybe_named<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<SourceName>, D::Error> {
    match Option::<String>::deserialize(deserializer)? {
        None => Ok(None),
        Some(text) => SourceName::parse(&text)
            .map(Some)
            .ok_or_else(|| D::Error::custom(format!("`{text}` names no source"))),
    }
}

/// The catalog in the clone at `root`, of the repository `url`.
///
/// Its file, or a folder on the way to it, is not read when it is a link
/// leading out of the clone: a repository can link its catalog to a file
/// of the machine it is gathered on, whose words the listing would quote.
fn read_catalog(root: &Path, url: &str) -> Result<Catalog, MarketplaceError> {
    let file = root.join(CATALOG);
    let unreadable = |why: String| MarketplaceError::Unreadable {
        url: url.to_owned(),
        why,
    };
    match fs::symlink_metadata(&file) {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(MarketplaceError::NoCatalog {
                url: url.to_owned(),
            })
        }
        Err(error) => return Err(unreadable(error.to_string())),
    }
    let inside = fs::canonicalize(root)
        .ok()
        .zip(fs::canonicalize(&file).ok())
        .is_some_and(|(root, file)| file.starts_with(root));
    if !inside {
        return Err(MarketplaceError::Outside {
            url: url.to_owned(),
        });
    }
    let mut bytes = Vec::new();
    File::open(&file)
        .and_then(|opened| opened.take(READ_AT_MOST + 1).read_to_end(&mut bytes))
        .map_err(|error| unreadable(error.to_string()))?;
    if bytes.len() as u64 > READ_AT_MOST {
        return Err(unreadable(format!(
            "it is larger than {} MiB",
            READ_AT_MOST / (1024 * 1024)
        )));
    }
    let value: Value =
        serde_json::from_slice(&bytes).map_err(|error| unreadable(error.to_string()))?;
    Catalog::of(&value, url).map_err(unreadable)
}

/// The licence the plugin at `folder` declares in its own manifest, when
/// it reads and is inside `clone`.
fn manifest_licence(folder: &Path, clone: &Path) -> Option<String> {
    let file = folder.join(PLUGIN_MANIFEST);
    let inside = fs::canonicalize(clone)
        .ok()
        .zip(fs::canonicalize(&file).ok())
        .is_some_and(|(clone, file)| file.starts_with(clone));
    if !inside {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(&file)
        .and_then(|opened| opened.take(MANIFEST_AT_MOST).read_to_end(&mut bytes))
        .ok()?;
    let value: Value = serde_json::from_slice(&bytes).ok()?;
    text(value.get("license"))
}

// ---------------------------------------------------------------------------
// Gathering
// ---------------------------------------------------------------------------

/// What a marketplace's catalog lists, as a gather of it read it: the
/// source it is, by the name it has or would be given, at the revision
/// fetched, and its catalog, each plugin marked with the source it is once
/// gathered.
///
/// What `gather marketplace --list --json` prints, and what a front end
/// lists entries from, read back by these names: the TUI's, and the
/// panel's, which reads it through the TypeScript and the schema written
/// from it (`MarketplaceListing`, a name of its own beside the snapshot's
/// `Listing`, as each type it names has, beside the ledger's `Entry` and an
/// install's `Place`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts", ts(rename = "MarketplaceListing"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "schema", schemars(rename = "MarketplaceListing"))]
pub struct Listing {
    #[serde(deserialize_with = "named")]
    pub name: SourceName,
    /// What was fetched, without a credential.
    #[serde(deserialize_with = "redacting")]
    pub url: Redacted,
    pub reference: Option<String>,
    pub revision: Option<Revision>,
    pub catalog: Catalog,
}

/// What came of one plugin a gather of a marketplace chose.
#[derive(Debug)]
pub struct Plugin {
    /// What the catalog calls it, or called it.
    pub entry: String,
    /// Whether it was named (`--plugin`), which makes one not gathered a
    /// failure, where `--all` passes it over, said.
    pub named: bool,
    /// What it is called as a source, once it is one.
    pub name: Option<SourceName>,
    /// Where it was gathered from: its repository, without a credential,
    /// and its folder there.
    pub url: Option<Redacted>,
    pub folder: Option<String>,
    pub took: Took,
    /// What it declares or holds beside skills, left where it is.
    pub left: Vec<Extra>,
    /// Whether its entry names a `headersHelper`, which was not run.
    pub unrun: bool,
}

impl Plugin {
    /// Whether what came of it is a failure: one that could not be
    /// gathered, and one named that no gather takes, the catalog no longer
    /// listing it among them. A cancel's plugins after it are not.
    pub fn fails(&self) -> bool {
        match &self.took {
            Took::Gathered(_) | Took::Unreached => false,
            Took::Failed(_) => true,
            Took::NotGathered(_) => self.named,
        }
    }
}

/// What a gather of a plugin came to.
#[derive(Debug)]
pub enum Took {
    /// Gathered, and what its gather did.
    Gathered(Report),
    /// It could not be, for this reason.
    Failed(GatherError),
    /// Not taken by any gather, for this reason: not kept in git, or, of
    /// one gathered again (`gather update`), no longer in the catalog,
    /// which leaves it as it was, its source and its shelf.
    NotGathered(NotGathered),
    /// Not tried, a plugin before it cancelled.
    Unreached,
}

/// What gathering a marketplace did.
#[derive(Debug)]
pub struct Gathered {
    pub listing: Listing,
    /// Each plugin chosen, in the order chosen, and what came of it.
    pub plugins: Vec<Plugin>,
    /// Whether `--all` stopped short of gathering for want of a yes, the
    /// catalog listing more than [`ASKED_BEYOND`] to gather: nothing was
    /// written, and `listing` is what it would gather.
    pub unconfirmed: bool,
}

/// What `request` was typed with, each value held to its rule, before
/// anything is fetched.
struct Typed {
    /// What is fetched ([`located`]).
    url: String,
    reference: Option<String>,
    name: Option<SourceName>,
}

/// Read what `request` was typed with, refusing what no gather of a
/// marketplace takes before anything is registered or fetched, as a
/// gather of a git source refuses it, by its codes.
fn typed(request: &Request) -> Result<Typed, MarketplaceError> {
    let mut given = vec![
        Some(request.url.as_str()),
        request.reference.as_deref(),
        request.name.as_deref(),
    ];
    let names = match &request.chosen {
        Chosen::Plugins(names) | Chosen::Again(names) => names.as_slice(),
        Chosen::Listed | Chosen::All => &[],
    };
    given.extend(names.iter().map(|name| Some(name.as_str())));
    if let Some(carrying) = given
        .into_iter()
        .flatten()
        .find(|text| carries_credentials(text))
    {
        return Err(GatherError::CredentialsInUrl {
            url: redacted(carrying).into_owned(),
        }
        .into());
    }
    let at = located(&request.url);
    if request.token.is_some() {
        gather::token_host(&request.url)?;
    }
    let reference = match (at.reference, &request.reference) {
        (Some(named), Some(given)) if named != *given => {
            return Err(GatherError::ReferenceTwice {
                named,
                given: given.clone(),
            }
            .into())
        }
        (Some(named), _) => Some(named),
        (None, given) => given.clone(),
    };
    if let Some(path) = at.path {
        if route_of(&path).as_deref() != Some(DEFAULT_SUBDIRECTORY) {
            return Err(MarketplaceError::FolderNamed { path });
        }
    }
    let name = match &request.name {
        Some(typed) => Some(
            SourceName::parse(typed).ok_or_else(|| GatherError::NameInvalid {
                name: typed.clone(),
            })?,
        ),
        None => None,
    };
    Ok(Typed {
        url: at.url,
        reference,
        name,
    })
}

/// The marketplace a gather of `typed` is of, when one was registered.
fn known(ledger: &Ledger, typed: &Typed) -> Result<Option<Source>, LedgerError> {
    ledger.source(
        SourceKind::Marketplace,
        &typed.url,
        typed.reference.as_deref(),
        DEFAULT_SUBDIRECTORY,
    )
}

/// Refuse `name` when a source other than `found` has it, as a gather of a
/// git source refuses it.
fn taken(ledger: &Ledger, name: &SourceName, found: Option<&Source>) -> Result<(), GatherError> {
    if let Some(holder) = ledger.source_named(name)? {
        if found.is_none_or(|found| found.id != holder.id) {
            return Err(GatherError::NameTaken {
                name: name.clone(),
                origin: holder.origin(),
            });
        }
    }
    Ok(())
}

/// `wanted`, or the first of it numbered that no source but `except` is
/// called: what a marketplace is called after its catalog's own name.
fn free(
    ledger: &Ledger,
    wanted: &SourceName,
    except: Option<i64>,
) -> Result<SourceName, LedgerError> {
    for n in 1.. {
        let candidate = if n == 1 {
            wanted.clone()
        } else {
            wanted.numbered(n)
        };
        match ledger.source_named(&candidate)? {
            Some(holder) if Some(holder.id) != except => {}
            _ => return Ok(candidate),
        }
    }
    unreachable!("the loop returns as soon as a name is free")
}

/// The name a marketplace of `catalog` not registered yet is given: what
/// `--name` said, else its catalog's own when it is a name a source can
/// have, numbered when another has it, else what its URL gives it.
fn new_name(
    ledger: &Ledger,
    typed: &Typed,
    catalog: &Catalog,
    except: Option<i64>,
) -> Result<Option<SourceName>, LedgerError> {
    if let Some(name) = &typed.name {
        return Ok(Some(name.clone()));
    }
    match catalog.name.as_deref().and_then(SourceName::parse) {
        Some(own) => free(ledger, &own, except).map(Some),
        None => Ok(None),
    }
}

/// Fetch the marketplace's repository `typed` names into `into`.
fn fetch(request: &Request, typed: &Typed, into: &Path) -> Result<Cloned, GatherError> {
    request.watch.report(Progress::Connecting);
    request
        .remote
        .fetch(
            &typed.url,
            typed.reference.as_deref(),
            None,
            into,
            &request.watch,
            request.token.as_ref(),
        )
        .map_err(|error| match error {
            GitError::Cancelled => GatherError::Cancelled,
            error => error.into(),
        })
}

/// The catalog of the marketplace `request` names, read with nothing
/// written: fetched into a folder of this run's own in the workspace's
/// scratch folder, gone once this returns, and named as a gather would name
/// it. `ledger` is only read, and the workspace is not held, as a gather's
/// dry run holds it not ([`gather::dry_run`]).
///
/// What `--all` asks first of a catalog listing more than [`ASKED_BEYOND`]
/// to gather ([`Catalog::asks_first`]), so a command that has not been
/// told yes writes nothing, its ledger not even made.
pub fn read(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Listing, MarketplaceError> {
    let typed = typed(request)?;
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled.into());
    }
    let known = known(ledger, &typed)?;
    if let Some(name) = &typed.name {
        taken(ledger, name, known.as_ref())?;
    }
    let scratch = scratch(workspace);
    let outcome = (|| {
        let clone = fetch(request, &typed, &scratch.join("clone"))?;
        let mut catalog = read_catalog(&clone.root, &typed.url)?;
        let name = match &known {
            Some(source) => typed.name.clone().unwrap_or_else(|| source.name.clone()),
            None => match new_name(ledger, &typed, &catalog, None)? {
                Some(name) => name,
                None => ledger.would_register(&typed.url)?.0,
            },
        };
        if let Some(source) = &known {
            marked(&mut catalog, &ledger.children(source)?);
        }
        Ok(Listing {
            name,
            url: Redacted::new(&typed.url),
            reference: typed.reference.clone(),
            revision: clone.revision,
            catalog,
        })
    })();
    let _ = fs::remove_dir_all(&scratch);
    outcome
}

/// A folder of this run's own in the workspace's scratch folder, which no
/// agent reads, under a name no other run is given.
fn scratch(workspace: &WeaverWorkspace) -> PathBuf {
    workspace
        .scratch_dir()
        .join(format!("marketplace-{}", copy::unique_name()))
}

/// Each entry of `catalog` marked with the source it is, among `children`.
fn marked(catalog: &mut Catalog, children: &[Source]) {
    for entry in &mut catalog.entries {
        entry.gathered = children
            .iter()
            .find(|child| child.entry.as_deref() == Some(entry.name.as_str()))
            .map(|child| child.name.clone());
    }
}

/// Gather from the marketplace `request` names into `workspace`: its
/// catalog fetched and read, the marketplace recorded once it reads, its
/// clone kept in the cache, and then each plugin chosen gathered as a
/// source of its own, its child ([`LedgerStore::child_for`]).
///
/// The workspace is held from the start, as a gather of a git source holds
/// it, until the last plugin is placed. The catalog is fetched into a
/// folder of the run's own first, so a marketplace whose catalog does not
/// read is never registered, nor is anything written for `--all` that
/// waits for a yes ([`Catalog::asks_first`]); it is moved into the cache
/// once it reads. A cancel is honoured until a plugin starts placing: one
/// before the marketplace is registered leaves everything as it was, and
/// one between two plugins leaves what was gathered before it.
///
/// One plugin's failure is said beside the rest, which are gathered all the
/// same. One new to the ledger that took nothing is forgotten again,
/// clone and all: a plugin is listed as a source once something of it is
/// on the shelf.
///
/// [`LedgerStore::child_for`]: crate::port::LedgerStore::child_for
pub fn gather(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
) -> Result<Gathered, MarketplaceError> {
    let typed = typed(request)?;
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled.into());
    }
    let _lock = workspace.lock().map_err(GatherError::from)?;
    let scratch = scratch(workspace);
    let outcome = run(workspace, ledger, request, &typed, &scratch);
    let _ = fs::remove_dir_all(&scratch);
    match &outcome {
        // Nothing written, nor logged.
        Ok(gathered) if gathered.unconfirmed => {}
        Ok(gathered) => {
            let detail = format!(
                "{} listed, {} chosen, {} gathered, {} failed",
                gathered.listing.catalog.entries.len(),
                gathered.plugins.len(),
                gathered
                    .plugins
                    .iter()
                    .filter(|plugin| matches!(plugin.took, Took::Gathered(_)))
                    .count(),
                gathered
                    .plugins
                    .iter()
                    .filter(|plugin| plugin.fails())
                    .count(),
            );
            ledger.log(Action::Gather, Some(&typed.url), Outcome::Ok, Some(&detail))?;
        }
        Err(MarketplaceError::Gather(GatherError::Cancelled)) => {}
        Err(error) => {
            let _ = ledger.log(
                Action::Gather,
                Some(&typed.url),
                Outcome::Failed,
                Some(&error.to_string()),
            );
        }
    }
    outcome
}

/// The gather itself, the workspace held, with the logging left to
/// [`gather`].
fn run(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
    typed: &Typed,
    scratch: &Path,
) -> Result<Gathered, MarketplaceError> {
    let known = known(ledger, typed)?;
    if let Some(name) = &typed.name {
        taken(ledger, name, known.as_ref())?;
    }
    let fetched = fetch(request, typed, &scratch.join("clone"))?;
    let mut catalog = read_catalog(&fetched.root, &typed.url)?;
    let picks = picked(&catalog, &request.chosen, &typed.url)?;

    if request.chosen == Chosen::All && !request.yes && catalog.asks_first() {
        let name = match &known {
            Some(source) => typed.name.clone().unwrap_or_else(|| source.name.clone()),
            None => match new_name(ledger, typed, &catalog, None)? {
                Some(name) => name,
                None => ledger.would_register(&typed.url)?.0,
            },
        };
        if let Some(source) = &known {
            marked(&mut catalog, &ledger.children(source)?);
        }
        return Ok(Gathered {
            listing: Listing {
                name,
                url: Redacted::new(&typed.url),
                reference: typed.reference.clone(),
                revision: fetched.revision,
                catalog,
            },
            plugins: Vec::new(),
            unconfirmed: true,
        });
    }
    // The last moment a cancel leaves everything as it was.
    if request.watch.is_cancelled() {
        return Err(GatherError::Cancelled.into());
    }

    let new = known.is_none();
    let mut parent = match known {
        Some(source) => source,
        None => ledger.source_for(
            SourceKind::Marketplace,
            &typed.url,
            typed.reference.as_deref(),
            DEFAULT_SUBDIRECTORY,
        )?,
    };
    let wanted = if new {
        new_name(ledger, typed, &catalog, Some(parent.id))?
    } else {
        typed.name.clone()
    };
    if let Some(wanted) = wanted.filter(|wanted| *wanted != parent.name) {
        ledger.rename_source(parent.id, &wanted)?;
        parent.name = wanted;
    }
    let cache = workspace.cache_dir().join(&parent.directory);
    if let Err(source) = moved(&fetched.root, &cache) {
        if new {
            ledger.forget_source(parent.id)?;
        }
        return Err(MarketplaceError::Unplaced {
            path: cache,
            source,
        });
    }
    let clone = Cloned {
        root: cache,
        revision: fetched.revision,
    };
    ledger.saw_source(parent.id, clone.revision.as_deref(), None)?;

    let children = ledger.children(&parent)?;
    let mut plugins = Vec::with_capacity(picks.len());
    let mut stopped = false;
    for pick in picks {
        let named = !matches!(request.chosen, Chosen::All);
        let plugin = match pick {
            Pick::Gone(entry) => Plugin {
                name: children
                    .iter()
                    .find(|child| child.entry.as_deref() == Some(entry.as_str()))
                    .map(|child| child.name.clone()),
                took: Took::NotGathered(NotGathered::Unlisted {
                    entry: entry.clone(),
                }),
                entry,
                named,
                url: None,
                folder: None,
                left: Vec::new(),
                unrun: false,
            },
            Pick::Listed(at) if stopped => Plugin {
                entry: catalog.entries[at].name.clone(),
                named,
                name: None,
                url: None,
                folder: None,
                took: Took::Unreached,
                left: Vec::new(),
                unrun: false,
            },
            Pick::Listed(at) => {
                let found = Found {
                    parent: &parent,
                    clone: &clone,
                    marketplace: &typed.url,
                    children: &children,
                };
                take(
                    workspace,
                    ledger,
                    request,
                    &found,
                    &catalog.entries[at],
                    named,
                )
            }
        };
        stopped |= matches!(plugin.took, Took::Failed(GatherError::Cancelled));
        plugins.push(plugin);
    }
    marked(&mut catalog, &ledger.children(&parent)?);
    Ok(Gathered {
        listing: Listing {
            name: parent.name.clone(),
            url: Redacted::new(&typed.url),
            reference: typed.reference.clone(),
            revision: clone.revision.clone(),
            catalog,
        },
        plugins,
        unconfirmed: false,
    })
}

/// Put the clone at `from` where the cache keeps the marketplace's, `to`,
/// in place of what was there: a clone is made again by every gather, as a
/// git source's is.
fn moved(from: &Path, to: &Path) -> io::Result<()> {
    match fs::remove_dir_all(to) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(cache) = to.parent() {
        fs::create_dir_all(cache)?;
    }
    fs::rename(from, to)
}

/// One plugin a gather chose: listed by the catalog, the `n`th entry, or no
/// longer, by the name it was gathered under.
enum Pick {
    Listed(usize),
    Gone(String),
}

/// The plugins of `catalog` that `chosen` names, in the order given, each
/// once: refused whole for a name `--plugin` gives that the catalog does
/// not list, before anything is gathered, as `gather update` refuses a
/// name no source has.
fn picked(catalog: &Catalog, chosen: &Chosen, url: &str) -> Result<Vec<Pick>, MarketplaceError> {
    let index = |name: &str| catalog.entries.iter().position(|entry| entry.name == name);
    fn once(names: &[String]) -> Vec<&str> {
        let mut seen: Vec<&str> = Vec::new();
        for name in names {
            if !seen.contains(&name.as_str()) {
                seen.push(name);
            }
        }
        seen
    }
    match chosen {
        Chosen::Listed => Ok(Vec::new()),
        Chosen::All => Ok((0..catalog.entries.len()).map(Pick::Listed).collect()),
        Chosen::Plugins(names) => {
            let unknown: Vec<String> = once(names)
                .into_iter()
                .filter(|name| index(name).is_none())
                .map(str::to_owned)
                .collect();
            if !unknown.is_empty() {
                return Err(MarketplaceError::Unknown {
                    names: unknown,
                    url: url.to_owned(),
                });
            }
            Ok(once(names)
                .into_iter()
                .filter_map(|name| index(name).map(Pick::Listed))
                .collect())
        }
        Chosen::Again(names) => Ok(once(names)
            .into_iter()
            .map(|name| match index(name) {
                Some(at) => Pick::Listed(at),
                None => Pick::Gone(name.to_owned()),
            })
            .collect()),
    }
}

/// Where a gather of a marketplace stands when it takes a plugin: the
/// marketplace, its clone, its URL, and the plugins of it registered before.
struct Found<'a> {
    parent: &'a Source,
    clone: &'a Cloned,
    marketplace: &'a str,
    children: &'a [Source],
}

/// Gather the plugin `entry` lists as a source of its own, the
/// marketplace's child: fetched into its own folder in the cache when it
/// is a repository of its own, read from the marketplace's clone when it is
/// in that, and placed on the shelf ([`gather::plugin`]).
fn take(
    workspace: &WeaverWorkspace,
    ledger: &Ledger,
    request: &Request,
    found: &Found,
    entry: &Entry,
    named: bool,
) -> Plugin {
    let mut plugin = Plugin {
        entry: entry.name.clone(),
        named,
        name: None,
        url: None,
        folder: None,
        took: Took::Unreached,
        left: entry.declares.clone(),
        unrun: entry.headers_helper,
    };
    if let Some(refused) = entry
        .source
        .refusal(&entry.name, on_this_machine(found.marketplace))
    {
        plugin.took = Took::NotGathered(refused);
        return plugin;
    }
    if request.watch.is_cancelled() {
        plugin.took = Took::Failed(GatherError::Cancelled);
        return plugin;
    }
    let own = entry.source.fetched();
    let (url, folder) = match (&own, &entry.source) {
        (Some(own), _) => (own.url.clone(), own.folder.clone()),
        (None, Place::Relative { path }) => (found.parent.url.to_string(), path.clone()),
        // Every other is refused above, or is a repository of its own.
        (None, _) => (
            found.parent.url.to_string(),
            String::from(DEFAULT_SUBDIRECTORY),
        ),
    };
    let new = !found
        .children
        .iter()
        .any(|child| child.entry.as_deref() == Some(entry.name.as_str()));
    let child = match ledger.child_for(found.parent, &entry.name, &url, &folder) {
        Ok(child) => child,
        Err(error) => {
            plugin.took = Took::Failed(error.into());
            return plugin;
        }
    };
    plugin.name = Some(child.name.clone());
    plugin.url = Some(child.url.clone());
    plugin.folder = Some(folder.clone());
    let cache = workspace.cache_dir().join(&child.directory);

    let outcome = (|| -> Result<(Report, PathBuf), GatherError> {
        let fetched_own;
        let clone = match &own {
            None => found.clone,
            Some(own) => {
                // The token goes to the host it was handed over for, and to
                // no plugin elsewhere.
                let token = request.token.as_ref().filter(|_| {
                    crate::url::token_host(found.marketplace).is_some()
                        && crate::url::token_host(found.marketplace)
                            == crate::url::token_host(&own.url)
                });
                request.watch.report(Progress::Connecting);
                fetched_own = request
                    .remote
                    .fetch(
                        &own.url,
                        own.reference,
                        own.sha,
                        &cache,
                        &request.watch,
                        token,
                    )
                    .map_err(|error| match error {
                        GitError::Cancelled => GatherError::Cancelled,
                        error => error.into(),
                    })?;
                &fetched_own
            }
        };
        let at = match folder.as_str() {
            DEFAULT_SUBDIRECTORY => clone.root.clone(),
            folder => clone.root.join(folder),
        };
        let licence = entry
            .licence
            .clone()
            .or_else(|| manifest_licence(&at, &clone.root));
        let report = gather::plugin(
            workspace,
            ledger,
            &child,
            &gather::Plugin {
                clone,
                folder: &folder,
                skills: entry.skills.as_deref(),
                licence: licence.as_deref(),
                pin: entry.source.sha(),
                url: &url,
            },
            &request.watch,
        )?;
        Ok((report, at))
    })();

    match outcome {
        Ok((report, at)) => {
            let _ = ledger.log(
                Action::Gather,
                Some(&url),
                Outcome::Ok,
                Some(&gather::logged(&report)),
            );
            plugin.left = extras(None, Some(&at))
                .into_iter()
                .chain(entry.declares.iter().copied())
                .collect::<std::collections::BTreeSet<Extra>>()
                .into_iter()
                .collect();
            plugin.took = Took::Gathered(report);
        }
        Err(error) => {
            if !matches!(error, GatherError::Cancelled) {
                let _ = ledger.log(
                    Action::Gather,
                    Some(&url),
                    Outcome::Failed,
                    Some(&error.to_string()),
                );
            }
            // A plugin new to the ledger that took nothing is not one of
            // its sources: forgotten, and its clone with it.
            if new {
                let _ = ledger.forget_source(child.id);
                if own.is_some() {
                    let _ = fs::remove_dir_all(&cache);
                }
                plugin.name = None;
            }
            plugin.took = Took::Failed(error);
        }
    }
    plugin
}

// ---------------------------------------------------------------------------
// What goes wrong
// ---------------------------------------------------------------------------

/// A plugin a catalog lists that cannot be read: said beside the rest,
/// which are listed and gathered all the same.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UnreadEntry {
    /// The `at`th it lists, counted from 1, names no plugin.
    #[error("the catalog's entry {at} names no plugin, so it is not listed")]
    Unnamed { at: usize },
    /// It names one no plugin is gathered under ([`is_plugin_name`]).
    #[error(
        "`{name}`: not a name a plugin is gathered under (letters, digits, `.`, `_` and `-`, \
         starting with a letter or a digit, 64 at most), so it is not listed"
    )]
    NameUnusable { name: String },
    /// Two plugins it lists are called this, in any case.
    #[error("{name}: the catalog lists another plugin called that too, so neither is listed")]
    Twice { name: String },
    /// Where it is cannot be read.
    #[error("{name}: {why}, so it is not listed")]
    Unreadable { name: String, why: String },
    /// Where it is carries a credential, said without it.
    #[error(
        "{name}: its source names {text} with a password, a token or a user name in it, which \
         is never kept nor repeated, so it is not listed"
    )]
    Credentialed { name: String, text: String },
}

codes!(UnreadEntry {
    Self::Unnamed { .. } => "marketplace_entry_unnamed",
    Self::NameUnusable { .. } => "marketplace_entry_name_invalid",
    Self::Twice { .. } => "marketplace_entry_twice",
    Self::Unreadable { .. } => "marketplace_entry_unreadable",
} echoed {
    Self::Credentialed { .. } => "credentials_in_url",
});

/// A plugin a catalog lists that no gather takes, not being kept in git.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NotGathered {
    #[error("{entry}: an npm package, which is not gathered: only a plugin kept in git is")]
    Npm { entry: String },
    #[error("{entry}: an archive, which is not gathered: only a plugin kept in git is")]
    Archive { entry: String },
    /// Its source is a command, which is never run, whatever it says.
    #[error(
        "{entry}: its source is a command to run, which Mindweaver never runs: only a plugin \
         kept in git is gathered"
    )]
    Command { entry: String },
    #[error("{entry}: a `{kind}` source, which this build does not read, so it is not gathered")]
    Unknown { entry: String, kind: String },
    /// A repository of its own on this machine, named by a catalog fetched
    /// from elsewhere.
    #[error(
        "{entry}: a repository on this machine, which a catalog fetched from elsewhere does not \
         reach, so it is not gathered"
    )]
    Local { entry: String },
    /// Gathered before, and no longer listed by its catalog: left as it
    /// was, until its source is forgotten.
    #[error(
        "{entry}: gathered before, and no longer listed by its marketplace's catalog, so it is \
         left as it was: `mindweaver gather remove` forgets it"
    )]
    Unlisted { entry: String },
}

codes!(NotGathered {
    Self::Npm { .. } => "marketplace_npm_not_gathered",
    Self::Archive { .. } => "marketplace_archive_not_gathered",
    Self::Command { .. } => "marketplace_command_refused",
    Self::Unknown { .. } => "marketplace_source_unknown",
    Self::Local { .. } => "marketplace_plugin_local",
    Self::Unlisted { .. } => "marketplace_plugin_unlisted",
});

/// Why a marketplace could not be gathered from at all, as opposed to one
/// of its plugins, which is said in its [`Plugin`].
#[derive(Debug, Error)]
pub enum MarketplaceError {
    #[error(transparent)]
    Gather(#[from] GatherError),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Lock(#[from] LockError),
    /// Its repository has no catalog.
    #[error(
        "{url}: has no `.claude-plugin/marketplace.json`, so it is no Claude Code marketplace: \
         `mindweaver gather git` gathers a repository of skills"
    )]
    NoCatalog { url: String },
    /// Its catalog does not read as one.
    #[error("{url}: its `.claude-plugin/marketplace.json` cannot be read: {why}")]
    Unreadable { url: String, why: String },
    /// Its catalog, or a folder on the way to it, is a link leading out of
    /// the repository.
    #[error(
        "{url}: its `.claude-plugin/marketplace.json` is a link leading out of the repository, \
         so it is not read"
    )]
    Outside { url: String },
    /// What was typed names a folder of the repository.
    #[error(
        "`{path}`: a marketplace's catalog is at its repository's root, so no folder of it is \
         gathered from: give the repository"
    )]
    FolderNamed { path: String },
    /// `--plugin` named what the catalog does not list.
    #[error(
        "{} not listed by the catalog of {url}, so nothing is gathered: `mindweaver gather \
         marketplace --list` lists what it has",
        quoted(names)
    )]
    Unknown { names: Vec<String>, url: String },
    /// Its clone could not be put in the cache.
    #[error("{path}: the catalog's clone cannot be put there: {source}")]
    Unplaced {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

codes!(MarketplaceError {
    Self::NoCatalog { .. } => "marketplace_catalog_missing",
    Self::Unreadable { .. } => "marketplace_catalog_unreadable",
    Self::Outside { .. } => "marketplace_catalog_outside",
    Self::FolderNamed { .. } => "marketplace_folder_named",
    Self::Unknown { .. } => "marketplace_plugin_unknown",
    Self::Unplaced { .. } => "marketplace_clone_unplaced",
} carried {
    Self::Gather(error) => error,
    Self::Ledger(error) => error,
    Self::Lock(error) => error,
});

/// `names` as a sentence lists them, each in backticks.
fn quoted(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
    match quoted.as_slice() {
        [] => String::new(),
        [one] => format!("{one} is"),
        [rest @ .., last] => format!("{} and {last} are", rest.join(", ")),
    }
}

#[cfg(test)]
#[path = "marketplace_test.rs"]
mod tests;
