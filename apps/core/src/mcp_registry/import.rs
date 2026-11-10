//! What one of a registry's servers is made into: a definition of the
//! library's ([`crate::mcp`]), written from one of the ways the registry
//! says the server is started or reached, every input read from a variable,
//! and no secret's value anywhere in it.
//!
//! A way is a package somebody runs or a remote already running:
//!
//! - **A package** is started by the command its registry calls for, the
//!   package named and pinned to its version where the registry gives one:
//!   npm's `npx -y <id>@<version>`, PyPI's `uvx <id>==<version>`, NuGet's
//!   `dnx <id>@<version>`, and an OCI image's `docker run -i --rm <image>`,
//!   its tag in the identifier, each of the environment's variables passed
//!   into the container with `-e <name>`, since the definition's `env` is
//!   the `docker` process's and no container sees it otherwise. Its
//!   `packageArguments` follow it. Its `runtimeArguments` are left out and
//!   said ([`Note::RuntimeArguments`]), but for those the command already
//!   holds: the command is the registry type's, as #106 fixes it, and what
//!   a publisher hands `docker` (a mount, a port, the whole command line
//!   again) is theirs to give by hand once they have read it.
//! - **A remote** is reached at its URL, over streamable HTTP, or over
//!   server-sent events, which the MCP specification has replaced and Codex
//!   and OpenCode do not take ([`Note::Sse`]).
//!
//! What no command of the registry type's can start is a manual step
//! ([`Manual`]), never a guess: an MCP bundle (`mcpb`), a Rust crate, a
//! registry type this build does not know, and a package that serves over
//! HTTP once started, which no agent starts and reaches in one entry.
//!
//! **What somebody supplies** is an input ([`crate::mcp::Input`]), read
//! from an environment variable (D-11) and never given a value: a value the
//! registry leaves to whoever runs the server, a `{variable}` in one, and
//! the whole value of anything it calls a secret, whatever the registry
//! wrote there. A value with nothing to supply is written as it is. One
//! asking for something is written only when the registry says it, or a
//! variable in it, is required: an agent given a reference to a variable
//! nobody set starts the server with nothing there, and Claude Code refuses
//! the whole file, so an optional one is left out and said
//! ([`Note::Optional`]). Where a value somebody would supply has a default
//! and is no secret, the default is written.
//!
//! Every `{name}` in a value is something to supply, described by the
//! `variables` beside it when they name it (a header's, or the remote's
//! beside its URL, which servers name in their headers too), and by the
//! value it stands in otherwise: the registry's schema says a name no
//! `variables` hold stays as it is written, but its servers write `Bearer
//! {api_key}` with no `variables` at all, and an agent sent the braces
//! would only be refused. `${name}`, as a shell writes one, is read the
//! same way.
//!
//! **The variable** an input is read from is the environment variable's
//! own name when the input is one's whole value, so every agent, Codex's
//! `env_vars` among them, passes it on as it is; a name already spelled
//! as a variable (`{INFOBIP_API_KEY}`) as it is; and otherwise the
//! definition's name and the input's, in capitals (`DEVMARGIN_TOKEN` for
//! `{token}`), so that two servers asking for an `api_key` ask for two.
//!
//! The text is held to the strict function `add` holds a definition to
//! ([`crate::artifact::mcp_strict_issues`]) before it is handed back, so
//! an import never makes a definition `validate` would call invalid: one
//! holding what looks like a token where a value is written as it is is
//! refused ([`ImportError::Unfit`]).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use thiserror::Error;

use super::{wire, Entry, Status};
use crate::artifact::{self, ValidationIssue};
use crate::code::codes;
use crate::mcp::{inline, quoted, InputKind, McpDefinition, MCP_FORMAT};
use crate::skill::MAX_NAME_SEGMENT_LEN;
use crate::url::carries_credentials;
use crate::value::{Redacted, UpstreamName, VariableName};

/// Where an import came from, as its definition's provenance says it.
pub(super) struct Provenance<'a> {
    pub registry: &'a str,
    pub fetched_at: &'a str,
}

/// One way a server is started or reached, as the registry lists it:
/// what a front end shows to choose from, by its place among them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Way {
    /// A package somebody runs.
    Package {
        /// The registry it is published to, as the registry writes it
        /// (`npm`, `pypi`, `oci`, `nuget`, `mcpb`, `cargo`).
        registry_type: String,
        /// Its name there, or for a bundle the address it is downloaded from.
        identifier: String,
        /// The version it is pinned to, when the registry gives one.
        version: Option<String>,
        /// How its server is talked to once started (`stdio`,
        /// `streamable-http`, `sse`).
        transport: String,
    },
    /// A server already running at an address.
    Remote {
        /// `streamable-http` or `sse`.
        transport: String,
        /// Where it is, `{variables}` and all.
        url: Redacted,
    },
    /// One the registry lists that does not read as either.
    Unreadable {
        /// What serde said of it.
        detail: String,
    },
}

/// A definition made of a registry's server, not written yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The name it is keyed by, and its file's stem.
    pub name: String,
    /// What its file holds.
    pub text: String,
    /// What the text declares, as `show` reads it.
    pub definition: McpDefinition,
    /// Which of [`Entry::ways`] it was made from.
    pub way: usize,
    /// What a front end says beside it: what was left out, and why it may
    /// not be what somebody wants.
    pub notes: Vec<Note>,
}

/// A way the import does not write, and what somebody does by hand instead.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Manual {
    #[error("an MCP bundle at {url}{}, which Mindweaver does not install: download it, check it against its SHA-256, and install it in each agent by hand", sha256.as_deref().map_or_else(String::new, |sha256| format!(" (SHA-256 {sha256})")))]
    Bundle {
        url: Redacted,
        sha256: Option<String>,
    },
    #[error("the Rust crate `{identifier}`{}, whose program the registry does not name: install it with `cargo install`, then add the server with `add mcp --command`", version.as_deref().map_or_else(String::new, |version| format!(" {version}")))]
    Crate {
        identifier: String,
        version: Option<String>,
    },
    #[error("`{identifier}` from `{registry_type}`, a registry this build does not know how to start a package from: install it by hand, then add the server with `add mcp --command`")]
    RegistryType {
        registry_type: String,
        identifier: String,
    },
    #[error("`{identifier}`, a package that serves over {transport}{} once it is started, which no agent starts and reaches in one entry: start it yourself, then add the server with `add mcp --url`", url.as_ref().map_or_else(String::new, |url| format!(" at {url}")))]
    Serves {
        identifier: String,
        transport: String,
        url: Option<Redacted>,
    },
    #[error("a package named {identifier:?}, which no command line takes as one word: a space, a brace, a control character or a leading dash would say something else")]
    Identifier { identifier: String },
    #[error("{url}: carries a credential, which no definition keeps")]
    Credentialed { url: Redacted },
    #[error("a transport `{transport}` this build does not know")]
    Transport { transport: String },
    #[error("{detail}")]
    Unreadable { detail: String },
}

codes!(Manual {
    Self::Bundle { .. } => "mcp_registry_bundle",
    Self::Crate { .. } => "mcp_registry_crate",
    Self::RegistryType { .. } => "mcp_registry_type_unknown",
    Self::Serves { .. } => "mcp_registry_package_serves",
    Self::Identifier { .. } => "mcp_registry_identifier_unfit",
    Self::Credentialed { .. } => "mcp_registry_url_credentialed",
    Self::Transport { .. } => "mcp_registry_transport_unknown",
    Self::Unreadable { .. } => "mcp_registry_way_unreadable",
});

/// What an import says beside the definition it made: never a reason it
/// was not made, each a thing somebody may want to know before writing it.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Note {
    #[error("deprecated by its publisher{}", message.as_deref().map_or_else(String::new, |message| format!(": {message}")))]
    Deprecated { message: Option<String> },
    #[error("`{identifier}` names no version, so an agent runs whatever is latest each time it starts it")]
    Unversioned { identifier: String },
    #[error("{url}: reached over server-sent events, which the MCP specification has replaced, and which Codex and OpenCode do not take")]
    Sse { url: Redacted },
    #[error("{what} is left out, as the registry says it is optional: an agent given a variable nobody set would start the server with nothing there")]
    Optional { what: String },
    #[error("the registry's arguments for the runtime are left out, which the command does not already hold: {}", arguments.join(", "))]
    RuntimeArguments { arguments: Vec<String> },
    #[error("{what} is left out: {why}")]
    LeftOut { what: String, why: &'static str },
}

codes!(Note {
    Self::Deprecated { .. } => "mcp_registry_deprecated",
    Self::Unversioned { .. } => "mcp_registry_unversioned",
    Self::Sse { .. } => "mcp_registry_sse",
    Self::Optional { .. } => "mcp_registry_optional_left_out",
    Self::RuntimeArguments { .. } => "mcp_registry_runtime_arguments_left_out",
    Self::LeftOut { .. } => "mcp_registry_value_left_out",
});

/// Why a server was not made into a definition.
#[derive(Debug, Error)]
pub enum ImportError {
    #[error("`{name}` {version} was deleted from the registry{}", message.as_deref().map_or_else(String::new, |message| format!(": {message}")))]
    Deleted {
        name: UpstreamName,
        version: String,
        message: Option<String>,
    },
    #[error("`{name}` {version} is `{status}` in the registry, a status this build does not know, so it is not imported")]
    Status {
        name: UpstreamName,
        version: String,
        status: String,
    },
    #[error("`{name}` names no package and no remote, so nothing says how to start or reach it")]
    Wayless { name: UpstreamName },
    #[error("`{name}` lists {ways} way(s) to start or reach it, so there is no way {way}")]
    NoSuchWay {
        name: UpstreamName,
        way: usize,
        ways: usize,
    },
    #[error("`{name}` is started or reached in no way Mindweaver writes; by hand: {}", steps.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Manual {
        name: UpstreamName,
        steps: Vec<Manual>,
    },
    #[error("the definition made of `{upstream}` would not be valid as `{name}`: {}", issues.iter().map(ToString::to_string).collect::<Vec<_>>().join("; "))]
    Unfit {
        upstream: UpstreamName,
        name: String,
        issues: Vec<ValidationIssue>,
    },
}

codes!(ImportError {
    Self::Deleted { .. } => "mcp_registry_server_deleted",
    Self::Status { .. } => "mcp_registry_status_unknown",
    Self::Wayless { .. } => "mcp_registry_server_wayless",
    Self::NoSuchWay { .. } => "mcp_registry_way_missing",
    Self::Manual { .. } => "mcp_registry_import_manual",
    Self::Unfit { .. } => "mcp_registry_import_unfit",
});

/// A way as it reads.
enum Parsed {
    Package(wire::Package),
    Remote(wire::Remote),
    Unreadable(String),
}

/// Every way `entry` lists, read one at a time.
fn parsed(entry: &Entry) -> Vec<Parsed> {
    let packages = entry.packages.iter().map(|package| {
        serde_json::from_value::<wire::Package>(package.clone()).map_or_else(
            |error| Parsed::Unreadable(format!("a package that does not read: {error}")),
            Parsed::Package,
        )
    });
    let remotes = entry.remotes.iter().map(|remote| {
        serde_json::from_value::<wire::Remote>(remote.clone()).map_or_else(
            |error| Parsed::Unreadable(format!("a remote that does not read: {error}")),
            Parsed::Remote,
        )
    });
    packages.chain(remotes).collect()
}

/// Every way `entry` lists, as a front end shows them.
pub(super) fn ways(entry: &Entry) -> Vec<Way> {
    parsed(entry)
        .into_iter()
        .map(|parsed| match parsed {
            Parsed::Package(package) => Way::Package {
                registry_type: package.registry_type,
                identifier: package.identifier,
                version: package.version,
                transport: package.transport.kind,
            },
            Parsed::Remote(remote) => Way::Remote {
                transport: remote.kind,
                url: Redacted::new(&remote.url),
            },
            Parsed::Unreadable(detail) => Way::Unreadable { detail },
        })
        .collect()
}

/// `entry` made into a definition named `name`, or after the server, from
/// the way `way` names, or from the first of them that can be.
///
/// The first that can be, in this order: a remote over streamable HTTP,
/// which nothing has to be installed for, runs nothing on the machine and
/// every agent reaches; then a package, as the registry lists them; then a
/// remote over server-sent events. A way that makes a definition `validate`
/// would refuse is passed over for the next, and said if none is made.
pub(super) fn import(
    entry: &Entry,
    provenance: &Provenance,
    way: Option<usize>,
    name: Option<&str>,
) -> Result<Imported, ImportError> {
    match &entry.status {
        Status::Active | Status::Deprecated => {}
        Status::Deleted => {
            return Err(ImportError::Deleted {
                name: entry.name.clone(),
                version: entry.version.clone(),
                message: entry.status_message.clone(),
            })
        }
        Status::Other(status) => {
            return Err(ImportError::Status {
                name: entry.name.clone(),
                version: entry.version.clone(),
                status: status.clone(),
            })
        }
    }
    let parsed = parsed(entry);
    if parsed.is_empty() {
        return Err(ImportError::Wayless {
            name: entry.name.clone(),
        });
    }
    let order: Vec<usize> = match way {
        Some(way) if way < parsed.len() => vec![way],
        Some(way) => {
            return Err(ImportError::NoSuchWay {
                name: entry.name.clone(),
                way,
                ways: parsed.len(),
            })
        }
        None => {
            let mut order: Vec<usize> = (0..parsed.len()).collect();
            order.sort_by_key(|at| match &parsed[*at] {
                Parsed::Remote(remote) if remote.kind == "streamable-http" => 0,
                Parsed::Package(_) => 1,
                Parsed::Remote(_) => 2,
                Parsed::Unreadable(_) => 3,
            });
            order
        }
    };
    let name = name.map_or_else(|| named_after(&entry.name), str::to_owned);

    let mut steps: Vec<(usize, Manual)> = Vec::new();
    let mut unfit = None;
    for at in order {
        let mut drafting = Drafting::new(&name);
        let transport = match &parsed[at] {
            Parsed::Package(package) => drafting.package(package),
            Parsed::Remote(remote) => drafting.remote(remote),
            Parsed::Unreadable(detail) => Err(Manual::Unreadable {
                detail: detail.clone(),
            }),
        };
        let transport = match transport {
            Ok(transport) => transport,
            Err(manual) => {
                steps.push((at, manual));
                continue;
            }
        };
        match drafting.finish(entry, provenance, &name, at, transport) {
            Ok(imported) => return Ok(imported),
            Err(issues) => {
                unfit.get_or_insert(ImportError::Unfit {
                    upstream: entry.name.clone(),
                    name: name.clone(),
                    issues,
                });
            }
        }
    }
    if let Some(unfit) = unfit {
        return Err(unfit);
    }
    steps.sort_by_key(|(at, _)| *at);
    Err(ImportError::Manual {
        name: entry.name.clone(),
        steps: steps.into_iter().map(|(_, manual)| manual).collect(),
    })
}

/// The name a definition of `upstream` is given unless told another: the
/// last word of its namespace, its owner, then the server's own name
/// after the slash, unless that starts with the owner already
/// (`ai.clawfetch/mcp` is `clawfetch-mcp`, and
/// `io.github.buildkite/buildkite-mcp-server` is `buildkite-mcp-server`).
///
/// The owner before it because a server's own name is so often a word
/// every publisher uses, `mcp` or `server`, that two imports would be one
/// definition otherwise. Each character no agent keys a server by is made
/// a hyphen (`Knapcode.SampleMcpServer` is `Knapcode-SampleMcpServer`),
/// and the name is no longer than a name may be.
fn named_after(upstream: &UpstreamName) -> String {
    let (namespace, own) = upstream.rsplit_once('/').unwrap_or(("", upstream));
    let owner = namespace.rsplit('.').next().unwrap_or_default();
    let whole = if owner.is_empty()
        || own
            .to_ascii_lowercase()
            .starts_with(&owner.to_ascii_lowercase())
    {
        own.to_owned()
    } else {
        format!("{owner}-{own}")
    };
    let name: String = whole
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .take(MAX_NAME_SEGMENT_LEN)
        .collect();
    let name = name.trim_matches('-');
    if name.is_empty() {
        String::from("server")
    } else {
        name.to_owned()
    }
}

/// How a server is started or reached, as the import writes it.
enum Transport {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    Endpoint {
        events: bool,
        url: String,
        headers: Vec<(String, String)>,
    },
}

/// An input the import declares.
struct Declared {
    name: String,
    kind: InputKind,
    description: Option<String>,
    secret: bool,
    variable: String,
}

/// What the registry says of something to supply.
#[derive(Clone, Copy)]
struct Asked<'a> {
    description: Option<&'a str>,
    secret: bool,
    /// What is written when it is no secret: a value the registry fixes,
    /// or its default.
    fixed: Option<&'a str>,
}

impl<'a> Asked<'a> {
    /// What `input` says of itself.
    fn of(input: &'a wire::Input) -> Self {
        Asked {
            description: input.description.as_deref(),
            secret: input.is_secret,
            fixed: input
                .value
                .as_deref()
                .or(input.default.as_deref())
                .filter(|fixed| !fixed.is_empty()),
        }
    }
}

/// A definition being written: its inputs as they are declared, and what is
/// said beside it.
struct Drafting {
    /// The definition's name in capitals, what a variable named for it
    /// starts with.
    prefix: String,
    inputs: Vec<Declared>,
    notes: Vec<Note>,
}

impl Drafting {
    fn new(name: &str) -> Self {
        let mut prefix = capitals(name);
        if prefix.starts_with(|c: char| c.is_ascii_digit()) {
            prefix.insert(0, '_');
        }
        Drafting {
            prefix,
            inputs: Vec::new(),
            notes: Vec::new(),
        }
    }

    /// The command that starts `package`, or the step somebody takes by
    /// hand instead.
    fn package(&mut self, package: &wire::Package) -> Result<Transport, Manual> {
        let registry_type = package.registry_type.to_ascii_lowercase();
        let identifier = package.identifier.trim();
        let version = package
            .version
            .as_deref()
            .map(str::trim)
            .filter(|version| !version.is_empty());
        match registry_type.as_str() {
            "npm" | "pypi" | "nuget" | "oci" => {}
            "mcpb" => {
                return Err(Manual::Bundle {
                    url: Redacted::new(identifier),
                    sha256: package.file_sha256.clone(),
                })
            }
            "cargo" => {
                return Err(Manual::Crate {
                    identifier: identifier.to_owned(),
                    version: version.map(str::to_owned),
                })
            }
            _ => {
                return Err(Manual::RegistryType {
                    registry_type: package.registry_type.clone(),
                    identifier: identifier.to_owned(),
                })
            }
        }
        match package.transport.kind.as_str() {
            "stdio" => {}
            "streamable-http" | "sse" => {
                return Err(Manual::Serves {
                    identifier: identifier.to_owned(),
                    transport: package.transport.kind.clone(),
                    url: package.transport.url.as_deref().map(Redacted::new),
                })
            }
            other => {
                return Err(Manual::Transport {
                    transport: other.to_owned(),
                })
            }
        }
        for word in std::iter::once(identifier).chain(version) {
            if !is_one_word(word) {
                return Err(Manual::Identifier {
                    identifier: word.to_owned(),
                });
            }
        }

        let env = self.environment(&package.environment_variables);
        let mut args = Vec::new();
        for argument in &package.package_arguments {
            self.argument(argument, &mut args);
        }
        let (command, mut words, pinned) = match registry_type.as_str() {
            "npm" => (
                "npx",
                vec![
                    String::from("-y"),
                    version.map_or_else(|| identifier.to_owned(), |v| format!("{identifier}@{v}")),
                ],
                version.is_some(),
            ),
            "pypi" => (
                "uvx",
                vec![
                    version.map_or_else(|| identifier.to_owned(), |v| format!("{identifier}=={v}"))
                ],
                version.is_some(),
            ),
            "nuget" => (
                "dnx",
                vec![version.map_or_else(|| identifier.to_owned(), |v| format!("{identifier}@{v}"))],
                version.is_some(),
            ),
            _ => {
                // The tag is the identifier's own since the 2025-09-29
                // schema; one written before it gave the version apart.
                let tagged = is_tagged(identifier);
                let image = match version {
                    Some(version) if !tagged => format!("{identifier}:{version}"),
                    _ => identifier.to_owned(),
                };
                let mut words: Vec<String> = ["run", "-i", "--rm"].map(String::from).to_vec();
                for (key, _) in &env {
                    words.push(String::from("-e"));
                    words.push(key.clone());
                }
                words.push(image);
                ("docker", words, tagged || version.is_some())
            }
        };
        if !pinned {
            self.notes.push(Note::Unversioned {
                identifier: identifier.to_owned(),
            });
        }
        let left: Vec<String> = package
            .runtime_arguments
            .iter()
            .map(spoken)
            .filter(|said| {
                !said.is_empty()
                    && !said
                        .iter()
                        .all(|word| word == command || words.contains(word))
            })
            .map(|said| said.join(" "))
            .collect();
        if !left.is_empty() {
            self.notes.push(Note::RuntimeArguments { arguments: left });
        }
        words.extend(args);
        Ok(Transport::Stdio {
            command: command.to_owned(),
            args: words,
            env,
        })
    }

    /// Where `remote` is reached, or the step somebody takes by hand
    /// instead.
    fn remote(&mut self, remote: &wire::Remote) -> Result<Transport, Manual> {
        let events = match remote.kind.as_str() {
            "streamable-http" => false,
            "sse" => true,
            other => {
                return Err(Manual::Transport {
                    transport: other.to_owned(),
                })
            }
        };
        if carries_credentials(&remote.url) {
            return Err(Manual::Credentialed {
                url: Redacted::new(&remote.url),
            });
        }
        if events {
            self.notes.push(Note::Sse {
                url: Redacted::new(&remote.url),
            });
        }
        // A part of the address is needed whatever the registry says of
        // it: no address is reached without it.
        let url = self.template(
            &remote.url,
            &remote.variables,
            InputKind::UrlVar,
            Asked {
                description: None,
                secret: false,
                fixed: None,
            },
        );
        let mut headers: Vec<(String, String)> = Vec::new();
        for header in &remote.headers {
            let Some(key) = header.name.as_deref().map(str::trim) else {
                self.left_out("a header with no name", "nothing says what to send it as");
                continue;
            };
            let what = format!("the header `{key}`");
            if key.is_empty() || !key.bytes().all(is_header_byte) {
                self.left_out(&what, "it is no name a header is sent under");
                continue;
            }
            if headers
                .iter()
                .any(|(held, _)| held.eq_ignore_ascii_case(key))
            {
                self.left_out(&what, "the registry names it twice, and the first is kept");
                continue;
            }
            // A header's `{variable}` is declared beside it, or, as
            // servers write them, among the remote's beside its URL.
            let mut variables = remote.variables.clone();
            variables.extend(header.variables.clone());
            if let Some(value) = self.key_value(header, &variables, InputKind::Header, key) {
                headers.push((key.to_owned(), value));
            }
        }
        Ok(Transport::Endpoint {
            events,
            url,
            headers,
        })
    }

    /// The environment a package's program is given, each variable the
    /// registry names that is written.
    fn environment(&mut self, variables: &[wire::KeyValue]) -> Vec<(String, String)> {
        let mut env: Vec<(String, String)> = Vec::new();
        for variable in variables {
            let Some(key) = variable.name.as_deref().map(str::trim) else {
                self.left_out(
                    "an environment variable with no name",
                    "nothing says what to set",
                );
                continue;
            };
            let what = format!("the environment variable `{key}`");
            if key.is_empty() || key.contains(['=', '\0']) {
                self.left_out(&what, "it is no name a variable is set under");
                continue;
            }
            if env.iter().any(|(held, _)| held == key) {
                self.left_out(&what, "the registry names it twice, and the first is kept");
                continue;
            }
            if let Some(value) = self.key_value(variable, &variable.variables, InputKind::Env, key)
            {
                env.push((key.to_owned(), value));
            }
        }
        env
    }

    /// What is written of the environment variable or header `key`, or none
    /// when it is left out, each `{variable}` in its value declared in
    /// `variables`.
    fn key_value(
        &mut self,
        held: &wire::KeyValue,
        variables: &BTreeMap<String, wire::Input>,
        kind: InputKind,
        key: &str,
    ) -> Option<String> {
        let input = &held.input;
        let what = match kind {
            InputKind::Env => format!("the environment variable `{key}`"),
            _ => format!("the header `{key}`"),
        };
        // The input a whole value is, and the variable it is read from: an
        // environment variable's own name, which every agent passes on as
        // it is, and otherwise the input's.
        let whole = |drafting: &Drafting, wanted: &str| -> (String, String) {
            match kind {
                InputKind::Env if VariableName::from(key).is_portable() => {
                    (key.to_owned(), key.to_owned())
                }
                InputKind::Env => (key.to_owned(), drafting.variable(key)),
                _ => (wanted.to_owned(), drafting.variable(wanted)),
            }
        };
        match input.value.as_deref().filter(|value| !value.is_empty()) {
            Some(value) if holds_placeholder(value, variables) => {
                if !input.is_required && !asks_required(value, variables) {
                    self.notes.push(Note::Optional { what });
                    return None;
                }
                if let Some(sole) = sole_placeholder(value, variables) {
                    let declared = variables.get(sole);
                    let asked = Asked {
                        description: declared
                            .and_then(|declared| declared.description.as_deref())
                            .or(input.description.as_deref()),
                        secret: declared.is_some_and(|declared| declared.is_secret)
                            || input.is_secret,
                        fixed: declared.and_then(|declared| Asked::of(declared).fixed),
                    };
                    let (name, variable) = whole(self, sole);
                    return Some(self.supplied(&name, kind, variable, asked));
                }
                Some(self.template(value, variables, kind, Asked::of(input)))
            }
            // What the registry fixes and calls no secret, written as it is.
            Some(value) if !input.is_secret => Some(value.to_owned()),
            // Nothing written, or a secret's value, which never is.
            _ => {
                if !input.is_required {
                    self.notes.push(Note::Optional { what });
                    return None;
                }
                let asked = Asked {
                    description: input.description.as_deref(),
                    secret: input.is_secret,
                    fixed: input.default.as_deref().filter(|fixed| !fixed.is_empty()),
                };
                let (name, variable) = whole(self, key);
                Some(self.supplied(&name, kind, variable, asked))
            }
        }
    }

    /// `argument`, written into `args` as the registry says it, or left
    /// out and said.
    fn argument(&mut self, argument: &wire::Argument, args: &mut Vec<String>) {
        let input = &argument.input;
        let flag = match argument.kind.as_deref() {
            Some("positional") => None,
            Some("named") => match argument.name.as_deref().map(str::trim) {
                Some(flag) if !flag.is_empty() => Some(flag),
                _ => {
                    self.left_out("a named argument with no name", "nothing says what to pass");
                    return;
                }
            },
            other => {
                let what = format!("an argument of the type `{}`", other.unwrap_or_default());
                self.left_out(&what, "this build knows only `positional` and `named`");
                return;
            }
        };
        let hint = argument
            .value_hint
            .as_deref()
            .filter(|hint| !hint.trim().is_empty());
        let what = match (flag, hint) {
            (Some(flag), _) => format!("the argument `{flag}`"),
            (None, Some(hint)) => format!("the argument `<{hint}>`"),
            (None, None) => String::from("an argument"),
        };
        let value = match input.value.as_deref().filter(|value| !value.is_empty()) {
            Some(value) if holds_placeholder(value, &argument.variables) => {
                if !input.is_required && !asks_required(value, &argument.variables) {
                    self.notes.push(Note::Optional { what });
                    return;
                }
                self.template(value, &argument.variables, InputKind::Arg, Asked::of(input))
            }
            Some(value) if !input.is_secret => value.to_owned(),
            _ => {
                if !input.is_required {
                    self.notes.push(Note::Optional { what });
                    return;
                }
                let wanted = hint
                    .or_else(|| flag.map(|flag| flag.trim_start_matches('-')))
                    .filter(|wanted| !wanted.is_empty())
                    .unwrap_or("argument");
                let variable = self.variable(wanted);
                let asked = Asked {
                    description: input.description.as_deref(),
                    secret: input.is_secret,
                    fixed: input.default.as_deref().filter(|fixed| !fixed.is_empty()),
                };
                self.supplied(wanted, InputKind::Arg, variable, asked)
            }
        };
        args.extend(flag.map(str::to_owned));
        args.push(value);
    }

    /// `text` with each placeholder in it written as an input of `kind`'s,
    /// described by `variables` where they name it and by `outer`, the
    /// value it is in, where they do not; every other byte as it is.
    fn template(
        &mut self,
        text: &str,
        variables: &BTreeMap<String, wire::Input>,
        kind: InputKind,
        outer: Asked,
    ) -> String {
        let mut written = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                break;
            };
            let named = &after[..close];
            if !is_placeholder(named, variables) {
                written.push_str(&rest[..=open]);
                rest = after;
                continue;
            }
            let before = &rest[..open];
            written.push_str(before.strip_suffix('$').unwrap_or(before));
            let declared = variables.get(named);
            let asked = Asked {
                description: declared
                    .and_then(|declared| declared.description.as_deref())
                    .or(outer.description),
                secret: declared.is_some_and(|declared| declared.is_secret) || outer.secret,
                fixed: declared.and_then(|declared| Asked::of(declared).fixed),
            };
            let variable = self.variable(named);
            written.push_str(&self.supplied(named, kind, variable, asked));
            rest = &after[close + 1..];
        }
        written.push_str(rest);
        written
    }

    /// What is written where somebody supplies a value: what the registry
    /// fixes, when it is no secret, and otherwise the placeholder of an
    /// input named `wanted`, of `kind`, read from `variable`.
    ///
    /// An input of that name, kind and variable is declared once, wherever
    /// it is written; one of that name and another kind or variable is
    /// given the name with a number after it, so two never share one.
    fn supplied(
        &mut self,
        wanted: &str,
        kind: InputKind,
        variable: String,
        asked: Asked,
    ) -> String {
        if !asked.secret {
            if let Some(fixed) = asked.fixed {
                return fixed.to_owned();
            }
        }
        let base = writable(wanted);
        let mut name = base.clone();
        for number in 2.. {
            match self.inputs.iter_mut().find(|held| held.name == name) {
                None => {
                    self.inputs.push(Declared {
                        name: name.clone(),
                        kind,
                        description: asked.description.map(str::to_owned),
                        secret: asked.secret,
                        variable,
                    });
                    break;
                }
                Some(held) if held.kind == kind && held.variable == variable => {
                    held.secret |= asked.secret;
                    if held.description.is_none() {
                        held.description = asked.description.map(str::to_owned);
                    }
                    break;
                }
                Some(_) => name = format!("{base}-{number}"),
            }
        }
        format!("{{{name}}}")
    }

    /// The variable an input named `wanted` is read from: `wanted` itself
    /// when it is spelled as a variable already, in capitals, and
    /// otherwise the definition's name and `wanted`, in capitals.
    fn variable(&self, wanted: &str) -> String {
        let spelled = wanted.starts_with(|c: char| c.is_ascii_uppercase() || c == '_')
            && wanted
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_');
        if spelled {
            wanted.to_owned()
        } else {
            format!("{}_{}", self.prefix, capitals(wanted))
        }
    }

    fn left_out(&mut self, what: &str, why: &'static str) {
        self.notes.push(Note::LeftOut {
            what: what.to_owned(),
            why,
        });
    }

    /// The definition, held to every rule `add` holds one to, or what
    /// `validate` would call it invalid for.
    fn finish(
        self,
        entry: &Entry,
        provenance: &Provenance,
        name: &str,
        way: usize,
        transport: Transport,
    ) -> Result<Imported, Vec<ValidationIssue>> {
        let description = [Some(entry.description.as_str()), entry.title.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|text| !text.is_empty())
            .unwrap_or(entry.name.as_str());
        let mut text = String::from(
            "# An MCP server imported from an MCP registry: each agent's own entry\n\
             # for it is written from this file, and `show` says how each agent is\n\
             # given it.\n",
        );
        let _ = writeln!(text, "format = {MCP_FORMAT}");
        let _ = writeln!(text, "name = {}", quoted(name));
        let _ = writeln!(text, "description = {}", quoted(description));
        text.push('\n');
        match &transport {
            Transport::Stdio { command, args, env } => {
                text.push_str(
                    "# How an agent starts it: the program, what it is started with, and what\n\
                     # its environment is given beside the agent's own.\n\
                     [transport]\n\
                     type = \"stdio\"\n",
                );
                let _ = writeln!(text, "command = {}", quoted(command));
                let items: Vec<String> = args.iter().map(|arg| quoted(arg)).collect();
                let _ = writeln!(text, "args = [{}]", items.join(", "));
                if !env.is_empty() {
                    let _ = writeln!(text, "env = {}", inline(env));
                }
            }
            Transport::Endpoint {
                events,
                url,
                headers,
            } => {
                let (over, word) = if *events {
                    ("over server-sent events", "sse")
                } else {
                    ("over streamable HTTP", "http")
                };
                let _ = write!(
                    text,
                    "# Where an agent reaches it, {over}, and what every request to it\n\
                     # carries.\n\
                     [transport]\n\
                     type = \"{word}\"\n"
                );
                let _ = writeln!(text, "url = {}", quoted(url));
                if !headers.is_empty() {
                    let _ = writeln!(text, "headers = {}", inline(headers));
                }
            }
        }
        for (at, input) in self.inputs.iter().enumerate() {
            text.push('\n');
            if at == 0 {
                text.push_str(
                    "# What is supplied when an agent starts it, each written between braces\n\
                     # where it goes and read from the environment variable it names: no value\n\
                     # is written here, and a secret never.\n",
                );
            }
            text.push_str("[[inputs]]\n");
            let _ = writeln!(text, "name = {}", quoted(&input.name));
            let _ = writeln!(text, "kind = {}", quoted(input.kind.slug()));
            if let Some(description) = &input.description {
                let _ = writeln!(text, "description = {}", quoted(description));
            }
            text.push_str("required = true\n");
            if input.secret {
                text.push_str("secret = true\n");
            }
            let _ = writeln!(text, "env_var = {}", quoted(&input.variable));
        }
        text.push_str(
            "\n# Where it came from: the registry, what it names the server, and the\n\
             # version imported.\n\
             [provenance]\n",
        );
        let _ = writeln!(text, "registry = {}", quoted(provenance.registry));
        let _ = writeln!(text, "upstream_name = {}", quoted(&entry.name));
        let _ = writeln!(text, "version = {}", quoted(&entry.version));
        let _ = writeln!(text, "fetched_at = {}", provenance.fetched_at);

        // Every value is written through `quoted` and `inline`, which write
        // what TOML reads back as it was (`given_test` holds them to it),
        // and the time is the clock's, written by `time::written`.
        let definition = McpDefinition::parse(&text).unwrap_or_else(|error| {
            unreachable!("an import's text reads as a definition: {error}")
        });
        let issues = artifact::mcp_strict_issues(&definition, name, &format!("{name}.toml"), &[]);
        if !issues.is_empty() {
            return Err(issues);
        }
        let mut notes = Vec::new();
        if entry.status == Status::Deprecated {
            notes.push(Note::Deprecated {
                message: entry.status_message.clone(),
            });
        }
        notes.extend(self.notes);
        Ok(Imported {
            name: name.to_owned(),
            text,
            definition,
            way,
            notes,
        })
    }
}

/// What a runtime argument says on a command line, word by word.
fn spoken(argument: &wire::Argument) -> Vec<String> {
    argument
        .name
        .iter()
        .chain(
            argument
                .input
                .value
                .as_ref()
                .or(argument.value_hint.as_ref()),
        )
        .map(|word| word.trim().to_owned())
        .filter(|word| !word.is_empty())
        .collect()
}

/// Whether `named`, between braces, is something to supply: a name the
/// `variables` beside it declare, or one written as an input's may be.
fn is_placeholder(named: &str, variables: &BTreeMap<String, wire::Input>) -> bool {
    variables.contains_key(named)
        || (!named.is_empty()
            && named
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'))
}

/// Whether `value` holds anything to supply.
fn holds_placeholder(value: &str, variables: &BTreeMap<String, wire::Input>) -> bool {
    placeholders(value, variables).next().is_some()
}

/// Whether `value` holds something to supply that its `variables` say is
/// required: a value is as required as the most required part of it.
fn asks_required(value: &str, variables: &BTreeMap<String, wire::Input>) -> bool {
    placeholders(value, variables).any(|named| {
        variables
            .get(named)
            .is_some_and(|declared| declared.is_required)
    })
}

/// Each name between braces in `value` that is something to supply, in the
/// order written.
fn placeholders<'v>(
    value: &'v str,
    variables: &'v BTreeMap<String, wire::Input>,
) -> impl Iterator<Item = &'v str> {
    let mut rest = value;
    std::iter::from_fn(move || loop {
        let open = rest.find('{')?;
        let after = &rest[open + 1..];
        let close = after.find('}')?;
        let named = &after[..close];
        if is_placeholder(named, variables) {
            rest = &after[close + 1..];
            return Some(named);
        }
        rest = after;
    })
}

/// The one thing to supply that is `value` whole, `{name}` or `${name}`,
/// when it is.
fn sole_placeholder<'v>(
    value: &'v str,
    variables: &BTreeMap<String, wire::Input>,
) -> Option<&'v str> {
    let inner = value
        .strip_prefix("${")
        .or_else(|| value.strip_prefix('{'))?
        .strip_suffix('}')?;
    (!inner.contains(['{', '}']) && is_placeholder(inner, variables)).then_some(inner)
}

/// `wanted` as an input's name may be written: each character no
/// placeholder holds made an underscore.
fn writable(wanted: &str) -> String {
    let name: String = wanted
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() {
        String::from("value")
    } else {
        name
    }
}

/// `text` in capitals, each character no variable's name holds made an
/// underscore.
fn capitals(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

/// Whether `word` reaches a program as the one word it is: not empty, no
/// space, brace or control character, and no leading dash, which the
/// runtime before it would read as an option of its own.
fn is_one_word(word: &str) -> bool {
    !word.is_empty()
        && !word.starts_with('-')
        && !word
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '{' || c == '}')
}

/// Whether an image's reference names its tag or its digest, after the
/// last `/` (a registry's port is before it).
fn is_tagged(image: &str) -> bool {
    image
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains([':', '@']))
}

/// Whether `byte` may stand in a header's name, a token as HTTP has it.
fn is_header_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

#[cfg(test)]
#[path = "import_test.rs"]
mod tests;
