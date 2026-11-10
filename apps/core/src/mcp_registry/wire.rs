//! The registry's JSON as serde reads it: each field core reads, typed and
//! nothing judged, and every other one read past, since a newer schema may
//! add any and the registry keeps each server in the schema its publisher
//! wrote it in (2025-07-09 to 2025-12-11 side by side, in one answer).
//!
//! A list or a map the registry sends as `null` reads as empty, as Go
//! writes a slice nobody filled (`"servers": null`). What a field holds is
//! the import's to judge ([`super::import`]), never this module's.

use std::collections::BTreeMap;

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

/// A publisher's `server.json`, what of it core reads. Its packages and
/// remotes are kept as JSON and read one at a time when they are asked
/// for, so that a listing shows a server whose one package does not read.
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
    #[serde(default, deserialize_with = "or_default")]
    pub packages: Vec<Value>,
    #[serde(default, deserialize_with = "or_default")]
    pub remotes: Vec<Value>,
}

/// A package somebody runs the server from.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Package {
    pub registry_type: String,
    pub identifier: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub file_sha256: Option<String>,
    pub transport: Transport,
    #[serde(default, deserialize_with = "or_default")]
    pub runtime_arguments: Vec<Argument>,
    #[serde(default, deserialize_with = "or_default")]
    pub package_arguments: Vec<Argument>,
    #[serde(default, deserialize_with = "or_default")]
    pub environment_variables: Vec<KeyValue>,
}

/// How a package's server is talked to once it is started.
#[derive(Debug, Deserialize)]
pub(super) struct Transport {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub url: Option<String>,
}

/// A server already running at an address.
#[derive(Debug, Deserialize)]
pub(super) struct Remote {
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(default, deserialize_with = "or_default")]
    pub headers: Vec<KeyValue>,
    #[serde(default, deserialize_with = "or_default")]
    pub variables: BTreeMap<String, Input>,
}

/// What every value the registry asks somebody for may say of it.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Input {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub is_required: bool,
    #[serde(default)]
    pub is_secret: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
}

/// An environment variable or a header: its name, and the value it is
/// given, with any `{variable}` in it.
#[derive(Debug, Deserialize)]
pub(super) struct KeyValue {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(flatten)]
    pub input: Input,
    #[serde(default, deserialize_with = "or_default")]
    pub variables: BTreeMap<String, Input>,
}

/// One of a command line's arguments: `positional`, a value as it is, or
/// `named`, a flag and the value after it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Argument {
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub value_hint: Option<String>,
    #[serde(flatten)]
    pub input: Input,
    #[serde(default, deserialize_with = "or_default")]
    pub variables: BTreeMap<String, Input>,
}

/// A field read as its default when the registry sends `null`.
fn or_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Option::<T>::deserialize(deserializer).map(Option::unwrap_or_default)
}
