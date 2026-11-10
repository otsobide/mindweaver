//! Mothers of what an MCP registry answers ([`crate::mcp_registry`]): one
//! version of a server as its `/v0.1` API writes it, its packages and
//! remotes, and what they ask somebody to supply, as JSON.
//!
//! JSON rather than the types core reads it into, since what a test is
//! about is how core reads what the registry wrote: each named shape is one
//! a real answer holds, drawn afresh, and the tests hold what core makes of
//! recorded answers too (`tests/mcp_registry_test.rs`).

use serde_json::{json, Value};

use crate::mother::random;
use crate::mother::url::UrlMother;
use crate::mother::value::UpstreamNameMother;
use crate::value::UpstreamName;

/// The schema every answer drawn here says it is written in.
pub const SCHEMA: &str =
    "https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json";

/// Makes what a registry answers.
pub struct RegistryMother;

impl RegistryMother {
    /// One version of a server with one package of any registry type the
    /// import starts, active.
    pub fn random() -> Value {
        let registry_type = random::pick(&["npm", "pypi", "oci", "nuget"]);
        Self::answered(
            &UpstreamNameMother::random(),
            &Self::version(),
            vec![Self::package(
                registry_type,
                &Self::identifier(registry_type),
                Some(&Self::version()),
            )],
            Vec::new(),
            "active",
        )
    }

    /// A version as a publisher numbers one, `major.minor.patch`.
    pub fn version() -> String {
        format!(
            "{}.{}.{}",
            random::between(0, 9),
            random::between(0, 30),
            random::between(0, 200)
        )
    }

    /// A package's name as `registry_type` spells one: a scoped npm
    /// package, a PyPI project, an OCI image with its tag, a NuGet package
    /// of dotted words, a bundle's address, a crate.
    pub fn identifier(registry_type: &str) -> String {
        let word = random::word();
        match registry_type {
            "npm" => format!("@{word}/{}", random::kebab()),
            "oci" => format!("ghcr.io/{word}/{}:{}", random::kebab(), Self::version()),
            "nuget" => format!("{}.{}", capitalised(&word), capitalised(&random::word())),
            "mcpb" => format!(
                "https://{}/releases/download/v1/{}.mcpb",
                UrlMother::host(),
                random::kebab()
            ),
            _ => random::kebab(),
        }
    }

    /// One version of the server `name`, at `version`, listing `packages`
    /// and `remotes`, with the status `status` the registry says of it, as
    /// its API answers one: the `server.json`, and the registry's own part
    /// beside it.
    pub fn answered(
        name: &UpstreamName,
        version: &str,
        packages: Vec<Value>,
        remotes: Vec<Value>,
        status: &str,
    ) -> Value {
        let at = Self::time();
        json!({
            "server": {
                "$schema": SCHEMA,
                "name": name.as_str(),
                "description": format!("{} for {}", random::word(), random::word()),
                "version": version,
                "packages": packages,
                "remotes": remotes,
            },
            "_meta": {
                "io.modelcontextprotocol.registry/official": {
                    "status": status,
                    "statusChangedAt": at,
                    "publishedAt": at,
                    "updatedAt": at,
                    "isLatest": true,
                }
            }
        })
    }

    /// A package of `registry_type` named `identifier`, pinned to `version`
    /// when one is given, started and talked to over its standard streams.
    pub fn package(registry_type: &str, identifier: &str, version: Option<&str>) -> Value {
        let mut package = json!({
            "registryType": registry_type,
            "identifier": identifier,
            "transport": { "type": "stdio" },
        });
        if let Some(version) = version {
            package["version"] = json!(version);
        }
        package
    }

    /// A server already running at `url`, reached over `transport`
    /// (`streamable-http` or `sse`).
    pub fn remote(transport: &str, url: &str) -> Value {
        json!({ "type": transport, "url": url })
    }

    /// An environment variable or a header named `name`, given no value,
    /// so somebody supplies it: required or not, a secret or not, and
    /// described.
    pub fn asked(name: &str, required: bool, secret: bool) -> Value {
        json!({
            "name": name,
            "description": format!("the {} to use", random::word()),
            "isRequired": required,
            "isSecret": secret,
        })
    }

    /// An environment variable or a header named `name` whose value is
    /// `value`, as the registry writes it, `{variables}` and all, declared
    /// in `variables`.
    pub fn valued(name: &str, value: &str, variables: Value) -> Value {
        json!({ "name": name, "value": value, "variables": variables })
    }

    /// A variable a value names, described, required or not, a secret or not.
    pub fn variable(required: bool, secret: bool) -> Value {
        json!({
            "description": format!("your {}", random::word()),
            "isRequired": required,
            "isSecret": secret,
        })
    }

    /// A page of a listing holding `servers`, naming `cursor` as the next
    /// when there is one.
    pub fn page(servers: Vec<Value>, cursor: Option<&str>) -> Value {
        let count = servers.len();
        match cursor {
            Some(cursor) => json!({
                "servers": servers,
                "metadata": { "nextCursor": cursor, "count": count },
            }),
            None => json!({ "servers": servers, "metadata": { "count": count } }),
        }
    }

    /// A time as the registry writes one: UTC, its fraction of a second as
    /// short as it can be, as Go writes it.
    pub fn time() -> String {
        let fraction = random::drawn_from(random::DIGITS, random::between(1, 6));
        let fraction = fraction.trim_end_matches('0');
        format!(
            "20{:02}-{:02}-{:02}T{:02}:{:02}:{:02}{}Z",
            random::between(25, 30),
            random::between(1, 12),
            random::between(1, 28),
            random::between(0, 23),
            random::between(0, 59),
            random::between(0, 59),
            if fraction.is_empty() {
                String::new()
            } else {
                format!(".{fraction}")
            }
        )
    }
}

/// `word` with its first letter in capitals.
fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_ascii_uppercase().to_string() + chars.as_str()
    })
}
