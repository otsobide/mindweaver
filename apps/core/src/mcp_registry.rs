//! Reading the official MCP Registry (#106): what it lists, what changed
//! since, and one version of one server.
//!
//! The registry (`registry.modelcontextprotocol.io`) lists MCP servers as
//! their publishers describe them, in the `server.json` format, and serves
//! them read-only at `/v0.1`, the API it calls stable while the registry is
//! in preview. Read over HTTP through [`Client`], as every catalog is
//! (D-14), so its rules hold here as anywhere: redirects, statuses, pages,
//! the cache.
//!
//! - **A search** ([`Registry::search`]) asks for the latest version of
//!   every server whose name holds the words given, a page of
//!   [`PAGE_LIMIT`] at a time by the cursor each page names, at most
//!   [`MOST_PAGES`] of them. What it lists is a [`Listing`].
//! - **A refresh** ([`Registry::refresh`]) asks only for what changed since
//!   the newest time a listing holds (`updated_since`), which the registry
//!   answers with every change, a server deleted since among them: one is
//!   taken out of the listing ([`Listed::dropped`]), every other replaces
//!   what the listing held of its server.
//! - **One version** ([`Registry::server`]), or the latest, by the name the
//!   registry gives the server ([`UpstreamName`]).
//!
//! Failures are collected, as everywhere in core: a server in a page that
//! does not read is a warning beside the rest
//! ([`RegistryError::Unreadable`]), and a page that fails ends the listing
//! with what was read before it ([`Listed::failure`]).
//!
//! Nothing here writes a definition, nor is called from a command yet:
//! `mindweaver gather mcp` (#107) is the first front end to.

pub(crate) mod time;
mod wire;

use std::collections::BTreeMap;

use thiserror::Error;

use crate::code::codes;
use crate::http::{encoded, with_query, CacheError, Client, FetchError, Fetched};
use crate::port::HttpRequest;
use crate::value::{Redacted, UpstreamName};

/// The official registry's address, the one a front end reads unless it is
/// told another (a mirror, or a test's own server on loopback).
pub const OFFICIAL: &str = "https://registry.modelcontextprotocol.io";

/// The version of the registry's API read, the first part of every path
/// asked: `v0.1`, which the registry froze as stable, where `v0` may still
/// change.
pub const API: &str = "v0.1";

/// How many servers a page is asked for: the most the registry gives one.
pub const PAGE_LIMIT: usize = 100;

/// How many pages a search or a refresh reads before it stops and says so:
/// two thousand servers, which a search narrow enough to choose from never
/// comes near, and which keeps one too wide (`mcp`) from reading the whole
/// registry, tens of thousands, a page at a time.
pub const MOST_PAGES: usize = 20;

/// The fewest characters a search is asked with, which the registry
/// refuses fewer than, once the spaces around them are trimmed.
pub const SHORTEST_QUERY: usize = 3;

/// A registry, read through a [`Client`].
#[derive(Debug, Clone)]
pub struct Registry {
    /// Where it is, with no `/` at the end: every path asked follows it.
    base: String,
    client: Client,
    page_limit: usize,
}

/// What a registry says of one version of one server.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// What the registry names it, `io.github.owner/server`.
    pub name: UpstreamName,
    /// The version of the server this entry describes.
    pub version: String,
    /// What it is for, as its publisher put it: a sentence of a hundred
    /// characters at most, which the registry holds it to.
    pub description: String,
    /// What it is shown as, when its publisher gave it a title.
    pub title: Option<String>,
    /// The schema its `server.json` was written in, by its address, whose
    /// path names the date (`.../2025-12-11/server.schema.json`).
    pub schema: Option<String>,
    /// Whether it stands, is deprecated, or was deleted.
    pub status: Status,
    /// Why its status changed, when the registry was told.
    pub status_message: Option<String>,
    /// When it last changed, as the registry wrote the time: what a
    /// refresh asks for what changed since.
    pub updated_at: Option<String>,
    /// Whether it is the server's latest version.
    pub latest: bool,
}

/// Whether a version stands, as the registry says it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Listed, and offered.
    Active,
    /// Listed still, though its publisher says not to rely on it, with why
    /// in the entry's [`status_message`](Entry::status_message).
    Deprecated,
    /// Taken out of the registry: listed only to a reader asking what
    /// changed, so that one holding it can take it out too.
    Deleted,
    /// A word this build does not know, as the registry wrote it.
    Other(String),
}

impl Status {
    /// The word the registry writes for it.
    pub fn slug(&self) -> &str {
        match self {
            Status::Active => "active",
            Status::Deprecated => "deprecated",
            Status::Deleted => "deleted",
            Status::Other(word) => word,
        }
    }

    /// What the registry's word `slug` says; none written reads as
    /// [`Active`](Self::Active), since only the official registry says a
    /// status and a mirror serving the same API may not.
    fn of(slug: Option<&str>) -> Self {
        match slug {
            None | Some("active") => Status::Active,
            Some("deprecated") => Status::Deprecated,
            Some("deleted") => Status::Deleted,
            Some(other) => Status::Other(other.to_owned()),
        }
    }
}

/// What a search found: the latest version of every server it names, by
/// name, none of them deleted.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    /// The words searched for, as they were asked.
    pub query: String,
    /// One entry a server, sorted by the name the registry gives it.
    pub entries: Vec<Entry>,
    /// The newest time the registry gave any entry read into it, deleted
    /// ones included, as it wrote the time: what a refresh asks for every
    /// change since. None when it read none.
    pub as_of: Option<String>,
    /// Whether every page was read: a listing that stopped short is
    /// searched again whole rather than refreshed, since a change after a
    /// page it never read would not be asked for.
    pub whole: bool,
}

/// A listing as a search or a refresh left it, and what happened on the way.
#[derive(Debug)]
pub struct Listed {
    pub listing: Listing,
    /// Whether it was read as the changes since the listing it refreshed,
    /// rather than whole.
    pub refreshed: bool,
    /// Each server a refresh took out, deleted since, as the registry last
    /// said it: what a front end tells somebody who saw it listed.
    pub dropped: Vec<Entry>,
    /// Why the listing stopped short, when it did: kept beside what was
    /// read, which is still what the registry said.
    pub failure: Option<RegistryError>,
    /// What was read past on the way: a server that does not read, and what
    /// the cache could not do.
    pub warnings: Vec<RegistryError>,
}

impl Registry {
    /// The official registry ([`OFFICIAL`]), read through `client`.
    pub fn official(client: Client) -> Self {
        Registry::at(OFFICIAL, client)
            .unwrap_or_else(|error| unreachable!("the official registry's address: {error}"))
    }

    /// The registry at `base`, an `http` or `https` address every path of
    /// its API follows, read through `client`.
    ///
    /// Refused when no request could be asked of it ([`HttpRequest::get`]:
    /// another scheme, no host, a credential), and when it holds a query or
    /// a fragment, which the paths asked would follow rather than precede.
    pub fn at(base: &str, client: Client) -> Result<Self, RegistryError> {
        let base = base.trim().trim_end_matches('/');
        HttpRequest::get(base).map_err(FetchError::from)?;
        if base.contains(['?', '#']) {
            return Err(RegistryError::Base {
                url: Redacted::new(base),
            });
        }
        Ok(Registry {
            base: base.to_owned(),
            client,
            page_limit: PAGE_LIMIT,
        })
    }

    /// This registry asking for `limit` servers a page, from one to
    /// [`PAGE_LIMIT`].
    pub fn paging(self, limit: usize) -> Self {
        Registry {
            page_limit: limit.clamp(1, PAGE_LIMIT),
            ..self
        }
    }

    /// Where it is, as every path asked of it follows.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The latest version of every server whose name holds `query`, read
    /// whole, page after page.
    ///
    /// Refused before anything is asked when `query`, trimmed, is shorter
    /// than [`SHORTEST_QUERY`], which the registry would refuse.
    pub fn search(&self, query: &str) -> Listed {
        let query = query.trim();
        let mut listed = Listed {
            listing: Listing {
                query: query.to_owned(),
                entries: Vec::new(),
                as_of: None,
                whole: false,
            },
            refreshed: false,
            dropped: Vec::new(),
            failure: None,
            warnings: Vec::new(),
        };
        if query.chars().count() < SHORTEST_QUERY {
            listed.failure = Some(RegistryError::Query {
                query: query.to_owned(),
            });
            return listed;
        }
        self.read_into(&mut listed, None);
        listed
    }

    /// `listing`, with every change the registry made to what it searched
    /// for since the newest time it holds ([`Listing::as_of`]): a server
    /// published or changed since replaces what the listing held of it, a
    /// new one is added, and one deleted since is taken out and said
    /// ([`Listed::dropped`]).
    ///
    /// Searched again whole instead when the listing holds no time or
    /// stopped short ([`Listing::whole`]). A refresh that stops short says
    /// so as a search does, and the listing it leaves is not whole, so the
    /// next is a whole search.
    pub fn refresh(&self, listing: &Listing) -> Listed {
        let Some(as_of) = listing.as_of.as_deref().filter(|_| listing.whole) else {
            return self.search(&listing.query);
        };
        let mut listed = Listed {
            listing: listing.clone(),
            refreshed: true,
            dropped: Vec::new(),
            failure: None,
            warnings: Vec::new(),
        };
        self.read_into(&mut listed, Some(as_of));
        listed
    }

    /// The version `version` of the server the registry names `name`, or
    /// its latest when none is given.
    ///
    /// A name that is no registry's ([`UpstreamName::is_well_formed`]) is
    /// refused before anything is asked. One the registry does not hold,
    /// or no longer does, is [`RegistryError::Unknown`], as it answers a
    /// version deleted.
    pub fn server(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Result<Fetched<Entry>, RegistryError> {
        let upstream = UpstreamName::from(name);
        if !upstream.is_well_formed() {
            return Err(RegistryError::Name { name: upstream });
        }
        let version = version.map(str::trim).filter(|version| !version.is_empty());
        let url = format!(
            "{}/{API}/servers/{}/versions/{}",
            self.base,
            encoded(name),
            encoded(version.unwrap_or("latest"))
        );
        let request = HttpRequest::get(&url).map_err(FetchError::from)?;
        let fetched = match self.client.json::<serde_json::Value>(&request) {
            Ok(fetched) => fetched,
            Err(FetchError::Status { status: 404, .. }) => {
                return Err(RegistryError::Unknown {
                    name: upstream,
                    version: version.map(str::to_owned),
                })
            }
            Err(error) => return Err(error.into()),
        };
        let entry = Entry::read(fetched.value).map_err(|detail| RegistryError::Unreadable {
            url: request.url().clone(),
            detail: Redacted::new(&detail),
        })?;
        Ok(Fetched {
            value: entry,
            via: fetched.via,
            warnings: fetched.warnings,
        })
    }

    /// Read `listed`'s query into its listing, page after page: whole when
    /// `since` is none, and otherwise only what changed since then.
    fn read_into(&self, listed: &mut Listed, since: Option<&str>) {
        let listing = &mut listed.listing;
        let mut url = format!("{}/{API}/servers", self.base);
        url = with_query(&url, "search", &listing.query);
        url = with_query(&url, "version", "latest");
        url = with_query(&url, "limit", &self.page_limit.to_string());
        if let Some(since) = since {
            url = with_query(&url, "updated_since", since);
        }
        let first = match HttpRequest::get(&url) {
            Ok(first) => first,
            Err(error) => {
                listing.whole = false;
                listed.failure = Some(FetchError::from(error).into());
                return;
            }
        };

        // Each page's address, in the order read, so a server that does not
        // read is said with the page it is on.
        let mut asked: Vec<String> = vec![first.url().to_string()];
        let paged = self
            .client
            .pages::<wire::Page>(&first, MOST_PAGES, |page, _| {
                let cursor = page.metadata.next_cursor.as_deref()?;
                if cursor.is_empty() {
                    return None;
                }
                let next = with_query(first.url(), "cursor", cursor);
                asked.push(next.clone());
                Some(next)
            });
        listed
            .warnings
            .extend(paged.warnings.into_iter().map(RegistryError::from));

        let mut entries: BTreeMap<UpstreamName, Entry> = if since.is_some() {
            listing
                .entries
                .drain(..)
                .map(|entry| (entry.name.clone(), entry))
                .collect()
        } else {
            BTreeMap::new()
        };
        let mut newest = listing.as_of.clone();
        for (page, url) in paged.pages.into_iter().zip(&asked) {
            for server in page.servers {
                let entry = match Entry::read(server) {
                    Ok(entry) => entry,
                    Err(detail) => {
                        listed.warnings.push(RegistryError::Unreadable {
                            url: Redacted::new(url),
                            detail: Redacted::new(&detail),
                        });
                        continue;
                    }
                };
                newest = later(newest, entry.updated_at.as_deref());
                if entry.status == Status::Deleted {
                    if let Some(held) = entries.remove(&entry.name) {
                        listed.dropped.push(held);
                    }
                    continue;
                }
                entries.insert(entry.name.clone(), entry);
            }
        }
        listing.entries = entries.into_values().collect();
        listing.as_of = newest;
        listing.whole = paged.failure.is_none();
        listed.failure = paged.failure.map(RegistryError::from);
    }
}

/// The later of `held` and `seen`, each a time the registry wrote, as it
/// wrote it; one that is no time is passed over.
fn later(held: Option<String>, seen: Option<&str>) -> Option<String> {
    let Some(seen) = seen.filter(|seen| time::instant(seen).is_some()) else {
        return held;
    };
    match held.as_deref().and_then(time::instant) {
        Some(at) if Some(at) >= time::instant(seen) => held,
        _ => Some(seen.to_owned()),
    }
}

impl Entry {
    /// What the registry answered for one version, read as an entry, or why
    /// it does not read as one: no `server`, or one naming no name or no
    /// version.
    pub(crate) fn read(value: serde_json::Value) -> Result<Entry, String> {
        let response: wire::Response =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        let official = response.meta.and_then(|meta| meta.official);
        let (status, status_message, updated_at, latest) = match official {
            Some(official) => (
                Status::of(official.status.as_deref()),
                official.status_message,
                official
                    .updated_at
                    .or(official.status_changed_at)
                    .or(official.published_at),
                official.is_latest,
            ),
            None => (Status::Active, None, None, false),
        };
        let server = response.server;
        Ok(Entry {
            name: UpstreamName::from(server.name),
            version: server.version,
            description: server.description.unwrap_or_default(),
            title: server.title,
            schema: server.schema,
            status,
            status_message,
            updated_at,
            latest,
        })
    }
}

/// Why a registry was not read, or not read whole, or what was read past.
///
/// Every URL in it is one asked or one an answer named, said
/// [`Redacted`], as every URL core says is.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// What reading over HTTP met: the address refused, the server not
    /// answering, a status that is no success, an answer that is no JSON.
    #[error(transparent)]
    Fetch(#[from] FetchError),
    /// What the cache could not do, said beside an answer.
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("{url}: holds a query or a fragment, so no path of a registry's API can follow it")]
    Base { url: Redacted },
    #[error("`{query}` is too short to search for: the registry takes {SHORTEST_QUERY} characters or more")]
    Query { query: String },
    #[error("`{name}` is no name a registry gives a server, which is a namespace, a slash and a name, as io.github.owner/server is")]
    Name { name: UpstreamName },
    #[error("the registry holds no {} of `{name}`", version.as_deref().map_or_else(|| String::from("version"), |version| format!("version {version}")))]
    Unknown {
        name: UpstreamName,
        version: Option<String>,
    },
    #[error("{url}: answered a server that does not read as one, which is passed over: {detail}")]
    Unreadable { url: Redacted, detail: Redacted },
}

codes!(RegistryError {
    Self::Base { .. } => "mcp_registry_base_invalid",
    Self::Query { .. } => "mcp_registry_query_short",
    Self::Name { .. } => "mcp_registry_name_invalid",
    Self::Unknown { .. } => "mcp_registry_server_unknown",
    Self::Unreadable { .. } => "mcp_registry_entry_unreadable",
} carried {
    Self::Fetch(error) => error,
    Self::Cache(error) => error,
});

#[cfg(test)]
#[path = "mcp_registry_test.rs"]
mod tests;
