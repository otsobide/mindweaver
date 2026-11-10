//! The registry's JSON as serde reads it: each field core reads, typed and
//! nothing judged, and every other one read past, since a newer schema may
//! add any and the registry keeps each server in the schema its publisher
//! wrote it in (2025-07-09 to 2025-12-11 side by side, in one answer).
//!
//! A list the registry sends as `null` reads as empty, as Go writes a slice
//! nobody filled (`"servers": null`).

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// A page of a listing: its servers, each read on its own so that one that
/// does not read hides none beside it, and the cursor of the page after it.
#[derive(Debug, Deserialize)]
pub(super) struct Page {
    #[serde(default, deserialize_with = "or_default")]
    pub servers: Vec<Value>,
    #[serde(default, deserialize_with = "or_default")]
    pub metadata: Metadata,
}

/// What a page says of the paging.
#[derive(Debug, Default, Deserialize)]
pub(super) struct Metadata {
    #[serde(rename = "nextCursor", default)]
    pub next_cursor: Option<String>,
}

/// One version of a server as the registry answers it: the `server.json`
/// its publisher wrote, and what the registry says of it.
#[derive(Debug, Deserialize)]
pub(super) struct Response {
    pub server: Server,
    #[serde(rename = "_meta", default)]
    pub meta: Option<Meta>,
}

/// The registry's own part of an answer, by the extension that holds it.
#[derive(Debug, Deserialize)]
pub(super) struct Meta {
    #[serde(rename = "io.modelcontextprotocol.registry/official", default)]
    pub official: Option<Official>,
}

/// What the official registry says of a version: whether it stands, and
/// when it last changed.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Official {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub status_message: Option<String>,
    #[serde(default)]
    pub status_changed_at: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub is_latest: bool,
}

/// A publisher's `server.json`, what of it core reads.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Server {
    #[serde(rename = "$schema", default)]
    pub schema: Option<String>,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// A field read as its default when the registry sends `null`.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}
