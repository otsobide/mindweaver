//! What an MCP server's definition declares about the server.
//!
//! One file per server, `<name>.toml` in a library's `mcp` folder (D-10),
//! which an install will render as one entry in each agent's own config
//! file, keyed by that name (#90 to #94). Only what the file holds lives
//! here. A definition as a thing on disk is an
//! [`Artifact`](crate::artifact::Artifact) of the kind [`Kind::Mcp`], found,
//! shown, validated, renamed and removed as every kind is; this is the half
//! that is specific to MCP servers.
//!
//! [`Kind::Mcp`]: crate::kind::Kind::Mcp
//!
//! Neutral rather than any agent's shape: a subset of the MCP registry's
//! `server.json`, so an entry from the registry maps onto it (#106), with
//! its inputs typed. Writing Claude's, Codex's or OpenCode's own shape would
//! make the file one agent's and every other a translation of it, and
//! storing `server.json` as it is would keep what no agent reads (packages,
//! a registry's metadata) and none of what an input needs to say.
//!
//! ```toml
//! format = 1
//! name = "github"
//! description = "GitHub's issues, pull requests and code"
//!
//! [transport]
//! type = "stdio"
//! command = "npx"
//! args = ["-y", "@modelcontextprotocol/server-github"]
//! env = { GITHUB_PERSONAL_ACCESS_TOKEN = "{token}" }
//!
//! [[inputs]]
//! name = "token"
//! kind = "env"
//! secret = true
//! env_var = "GITHUB_TOKEN"
//! ```
//!
//! An input is what somebody supplies when an agent starts the server, never
//! a value the file holds (D-11): it is written between braces, `{token}`,
//! where its value goes, and its `kind` says where that may be. A brace that
//! names no input is left as it is written, so a JSON argument such as
//! `{"debug":true}` means what it says.
//!
//! The format is held here: a file that reads as one, a transport that
//! names what it needs to start or reach the server, every input named so
//! that its placeholder can be written and read from a variable every agent
//! can refer to, and every input used where its kind says. So are its
//! secrets (D-11): a secret input names the variable it is read from, and no
//! value holds what looks like a token written as it is (`token`). Beside
//! it, which names a server can be given ([`name`]), and what each agent
//! cannot be given of a definition ([`Inexpressible`]), which no `validate`
//! says, since the definition is whole for every other agent.

use std::collections::BTreeMap;

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use thiserror::Error;

use crate::artifact::ValidationIssue;
use crate::code::codes;
use crate::value::{ArtifactName, InputName, Redacted, VariableName};

mod blanked;
mod expressible;
mod given;
pub(crate) mod name;
mod runs;
mod token;

pub use expressible::{Expressibility, Inexpressible};
pub(crate) use given::{inline, quoted, retransported};
pub use given::{Address, Given, Program, Repeated, Unsayable};
pub use runs::{Running, Runs};

/// The format a definition is written in, and the newest this build reads.
///
/// A version of its own rather than a marker's: a definition is an
/// artifact, copied and gathered with no marker beside it, so it has to say
/// what it is in itself. One from a newer format is refused rather than
/// read as this one, which could take a transport it has never heard of for
/// one it knows ([`McpError::FormatTooNew`]).
pub const MCP_FORMAT: u32 = 1;

/// What a definition declares, as written.
///
/// Every key the format does not define is read past, as a skill's front
/// matter is: a definition carrying one is still a server worth listing,
/// and the file keeps it for whoever wrote it.
#[derive(Debug, Clone, PartialEq)]
pub struct McpDefinition {
    /// The name the server is keyed by in an agent's config, and the file's
    /// stem, which `validate` holds it to.
    pub name: ArtifactName,
    /// What the server is for: what a listing shows, and what an agent's
    /// user reads before approving it.
    pub description: String,
    /// How an agent starts the server or reaches it.
    pub transport: Transport,
    /// What somebody supplies for it, in the order written.
    pub inputs: Vec<Input>,
    /// Where it came from, when it was imported from a registry.
    pub provenance: Option<Upstream>,
    /// What each agent is given beside the entry the definition renders,
    /// keyed by the agent's name, as written: passed through untouched, as
    /// a key the format does not define is, for its renderer to add (an
    /// agent's timeout, say). A key naming no agent this build knows is
    /// kept too, for a build that does.
    pub extras: BTreeMap<String, toml::Table>,
}

/// How an agent starts a server, or reaches one already running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    /// A program the agent starts and talks to over its standard streams.
    Stdio(Stdio),
    /// A server at an address, reached over streamable HTTP.
    Http(Endpoint),
    /// A server at an address, reached over server-sent events, which the
    /// MCP specification has since replaced with streamable HTTP and some
    /// agents no longer take.
    Sse(Endpoint),
}

impl Transport {
    /// Which transport it is, as its `type` names it.
    pub const fn kind(&self) -> TransportType {
        match self {
            Transport::Stdio(_) => TransportType::Stdio,
            Transport::Http(_) => TransportType::Http,
            Transport::Sse(_) => TransportType::Sse,
        }
    }

    /// How the format writes it, as `type`.
    pub const fn slug(&self) -> &'static str {
        self.kind().slug()
    }
}

/// Which of the transports a definition names, with nothing it holds: the
/// word its `type` is written as, for what `show` says of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TransportType {
    /// A program, [`Transport::Stdio`].
    Stdio,
    /// An address over streamable HTTP, [`Transport::Http`].
    Http,
    /// An address over server-sent events, [`Transport::Sse`].
    Sse,
}

impl TransportType {
    /// How the format writes it, as `type`.
    pub const fn slug(self) -> &'static str {
        match self {
            TransportType::Stdio => "stdio",
            TransportType::Http => "http",
            TransportType::Sse => "sse",
        }
    }
}

/// By the word the format writes as `type`, rather than derived: a derive
/// spells the variant, and what a report calls a transport is the word a
/// definition is written with, which renaming a variant must not change.
impl Serialize for TransportType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// How an agent starts an MCP server or reaches it, by the word its
    /// definition writes as its transport's `type`.
    TransportType by slug { Stdio, Http, Sse }
}

/// A program an agent starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stdio {
    /// The program, as the agent finds it: a name on its `PATH`, or a path.
    pub command: String,
    /// What it is started with, in order.
    pub args: Vec<String>,
    /// What its environment is given, beside the agent's own.
    pub env: BTreeMap<String, String>,
    /// The folder it is started in, when not the agent's.
    pub cwd: Option<String>,
}

/// A server at an address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Where the server is. Held without any credential its userinfo was
    /// typed with ([`Redacted`]), as every URL core keeps is: a secret goes
    /// in an input, read from the environment (D-11), and #87 says so of a
    /// literal one.
    pub url: Redacted,
    /// What every request to it carries.
    pub headers: BTreeMap<String, String>,
    /// Each input written only in the part of the address
    /// [`url`](Self::url) does not keep, its userinfo, by name: what was
    /// taken out with any credential, kept as the names the file declares
    /// and never as the text around them, so that `validate` says such an
    /// input is written where no agent can be given it, rather than that it
    /// is used nowhere ([`ValidationIssue::InputInUserinfo`]).
    pub withheld: Vec<InputName>,
    /// What each token the address held as written starts with, its
    /// userinfo included: read off the text before it is redacted, as
    /// [`withheld`](Self::withheld) is, since a token typed there went with
    /// it, and kept as what it starts with and never as the text, so that
    /// `validate` says the file holds one ([`ValidationIssue::SecretLiteral`])
    /// without the definition holding it again.
    pub tokens: Vec<&'static str>,
}

/// One value somebody supplies when an agent starts the server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// What the transport writes between braces where its value goes.
    pub name: InputName,
    /// Where it may be written.
    pub kind: InputKind,
    /// What it is, for whoever supplies it.
    pub description: Option<String>,
    /// Whether the server does not start without it.
    pub required: bool,
    /// Whether it is a secret, which no file may hold (D-11).
    pub secret: bool,
    /// The environment variable it is read from.
    pub env_var: Option<VariableName>,
}

/// Where an input may be written in its transport.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    /// In a value of a program's `env`.
    Env,
    /// In a value of an address's `headers`.
    Header,
    /// In one of a program's `args`.
    Arg,
    /// In an address's `url`.
    UrlVar,
}

impl InputKind {
    /// How the format writes it, as an input's `kind`.
    pub const fn slug(self) -> &'static str {
        match self {
            InputKind::Env => "env",
            InputKind::Header => "header",
            InputKind::Arg => "arg",
            InputKind::UrlVar => "url_var",
        }
    }

    /// Where an input of this kind is written, as a phrase.
    pub(crate) const fn place(self) -> &'static str {
        match self {
            InputKind::Env => "a value of the transport's `env`",
            InputKind::Header => "a value of the transport's `headers`",
            InputKind::Arg => "one of the transport's `args`",
            InputKind::UrlVar => "the transport's `url`",
        }
    }
}

/// By the word the format writes as an input's `kind`, rather than derived,
/// for the reason a transport's `type` is: what is read and what is
/// reported are the one word.
impl Serialize for InputKind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.slug())
    }
}

#[cfg(any(feature = "ts", feature = "schema"))]
crate::described::words! {
    /// Where an MCP server's input may be written in its transport, by the
    /// word its definition writes as the input's `kind`.
    InputKind by slug { Env, Header, Arg, UrlVar }
}

/// Where a definition imported from a registry came from, as the registry
/// said it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Upstream {
    /// The registry, by its address.
    pub registry: Option<Redacted>,
    /// What the registry names the server, which is rarely a name an agent
    /// keys it by (`io.github.owner/server`).
    pub upstream_name: Option<String>,
    /// The version the registry gave.
    pub version: Option<String>,
    /// When it was fetched, as written: a TOML date and time, or a string.
    pub fetched_at: Option<String>,
}

impl McpDefinition {
    /// Read a definition from what its file holds.
    ///
    /// Its `format` first, before anything else is read, so that one from
    /// a newer format is refused as that, whatever else it holds, rather
    /// than as a shape this build does not know. Then the shape: what the
    /// format requires and how each key is typed. A transport that names
    /// nothing to start or reach and an input no value uses are read all
    /// the same, for `validate` to say ([`issues`](Self::issues)): the file
    /// is a definition still, only not one an agent could use.
    pub fn parse(text: &str) -> Result<Self, McpError> {
        let table: toml::Table = toml::from_str(text).map_err(McpError::Unparsed)?;
        match table.get("format") {
            None => return Err(McpError::NoFormat),
            Some(toml::Value::Integer(found)) if *found > i64::from(MCP_FORMAT) => {
                return Err(McpError::FormatTooNew { found: *found })
            }
            Some(toml::Value::Integer(found)) if *found == i64::from(MCP_FORMAT) => {}
            Some(other) => {
                return Err(McpError::FormatUnknown {
                    found: other.to_string(),
                })
            }
        }
        let raw: Raw = toml::Value::Table(table)
            .try_into()
            .map_err(McpError::Malformed)?;
        Ok(raw.into())
    }

    /// Everything wrong with it as a definition, in the order it is worth
    /// fixing: a description with nothing in it, a transport that names
    /// nothing to start or reach, a token written as an input's name, which
    /// is then said of it alone, an input whose name no placeholder can be
    /// written of, a token written where its variable's name goes or a
    /// variable not every agent can refer to, a secret read from no
    /// variable at all, a token written as it is in a value of the
    /// transport or of the extras, an input declared twice, and an input no
    /// value uses
    /// where its kind says, each in the order written: one written in the
    /// address's userinfo said as that, since it is written where its kind
    /// says, only in a part no address is kept with.
    ///
    /// Not its name, which is the file's to say and the artifact's to judge
    /// ([`crate::artifact`]). Pure, as every check `validate` makes is.
    pub fn issues(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();
        if self.description.trim().is_empty() {
            issues.push(ValidationIssue::DescriptionEmpty);
        }
        let missing = match &self.transport {
            Transport::Stdio(stdio) => stdio.command.trim().is_empty().then_some("command"),
            Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                endpoint.url.trim().is_empty().then_some("url")
            }
        };
        if let Some(field) = missing {
            issues.push(ValidationIssue::TransportIncomplete {
                transport: self.transport.slug().to_owned(),
                field: field.to_owned(),
            });
        }
        // An input whose name holds a token is said as that alone: every
        // other word about it would print its name whole, wherever the
        // message goes. `add` names an input after the variable it is told
        // to read, so a token pasted there lands in the name as surely as
        // in the variable.
        let hidden: Vec<bool> = self
            .inputs
            .iter()
            .map(|input| !token::found(&input.name, &[]).is_empty())
            .collect();
        for (index, input) in self.inputs.iter().enumerate() {
            let mut named = Vec::new();
            named.extend(token::found(&input.name, &[]).iter().map(|prefix| {
                ValidationIssue::SecretLiteral {
                    at: format!("inputs[{index}].name"),
                    prefix: (*prefix).to_owned(),
                }
            }));
            if !hidden[index] && !input.name.is_writable() {
                named.push(ValidationIssue::InputNameMalformed {
                    name: input.name.to_string(),
                });
            }
            // The value pasted where its variable's name goes, the likeliest
            // way a token reaches this file, and one every renderer would
            // write into its agent's reference: a GitHub token is letters,
            // digits and underscores, so it passes for a variable. Said as
            // a token, and never as a variable no agent can refer to, whose
            // message would print it whole.
            let pasted = input
                .env_var
                .as_ref()
                .map(|variable| token::found(variable, &[]))
                .unwrap_or_default();
            named.extend(pasted.iter().map(|prefix| ValidationIssue::SecretLiteral {
                at: format!("inputs[{index}].env_var"),
                prefix: (*prefix).to_owned(),
            }));
            match &input.env_var {
                _ if hidden[index] => {}
                Some(_) if !pasted.is_empty() => {}
                Some(variable) if !variable.is_portable() => {
                    named.push(ValidationIssue::EnvVarMalformed {
                        name: input.name.to_string(),
                        variable: variable.to_string(),
                    });
                }
                Some(_) => {}
                // What "a secret input must be an environment reference"
                // (D-11) asks of an input: a value is never in the file, so
                // a secret is one only when it says the variable each
                // agent's reference will name. One that names none could
                // reach an agent only as a value written into its entry.
                // An input that is no secret may name none: its value is
                // somebody's to give when it is installed, and may be
                // written as it is.
                None if input.secret => named.push(ValidationIssue::SecretWithoutEnvVar {
                    name: input.name.to_string(),
                }),
                None => {}
            }
            for issue in named {
                if !issues.contains(&issue) {
                    issues.push(issue);
                }
            }
        }
        for (at, prefix) in self.literals() {
            let issue = ValidationIssue::SecretLiteral {
                at,
                prefix: prefix.to_owned(),
            };
            if !issues.contains(&issue) {
                issues.push(issue);
            }
        }
        let shown = || {
            self.inputs
                .iter()
                .zip(&hidden)
                .filter(|(_, hidden)| !**hidden)
                .map(|(input, _)| input)
        };
        let mut seen: Vec<&InputName> = Vec::new();
        for input in shown() {
            if seen.contains(&&input.name) {
                let repeated = ValidationIssue::InputRepeated {
                    name: input.name.to_string(),
                };
                if !issues.contains(&repeated) {
                    issues.push(repeated);
                }
                continue;
            }
            seen.push(&input.name);
        }
        for input in shown() {
            if self.references(input) {
                continue;
            }
            let issue = if self.withholds(input) {
                ValidationIssue::InputInUserinfo {
                    name: input.name.to_string(),
                }
            } else {
                ValidationIssue::InputUnreferenced {
                    name: input.name.to_string(),
                    kind: input.kind,
                }
            };
            if !issues.contains(&issue) {
                issues.push(issue);
            }
        }
        issues
    }

    /// Where each token written as it is stands in the transport or the
    /// extras, by the key that leads to it, and what it starts with: a
    /// program's `args`, then its `env`, or an address's `url`, then its
    /// `headers`, each in the order written, then every text the extras
    /// hold, at any depth, every placeholder of a declared input taken out
    /// first. Not its `command`, which names a program, nor `cwd`.
    ///
    /// The extras too, since each agent's are passed on to its entry as
    /// they are: a token there reaches the agent's config as surely as one
    /// in the transport does.
    fn literals(&self) -> Vec<(String, &'static str)> {
        let placeholders: Vec<String> = self
            .inputs
            .iter()
            .map(|input| written(&input.name))
            .collect();
        let found = |value: &str| token::found(value, &placeholders);
        let mut literals = Vec::new();
        match &self.transport {
            Transport::Stdio(stdio) => {
                for (index, arg) in stdio.args.iter().enumerate() {
                    for prefix in found(arg) {
                        literals.push((format!("transport.args[{index}]"), prefix));
                    }
                }
                for (key, value) in &stdio.env {
                    for prefix in found(value) {
                        literals.push((keyed("transport.env", key), prefix));
                    }
                }
            }
            Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                for prefix in &endpoint.tokens {
                    literals.push((String::from("transport.url"), *prefix));
                }
                for (key, value) in &endpoint.headers {
                    for prefix in found(value) {
                        literals.push((keyed("transport.headers", key), prefix));
                    }
                }
            }
        }
        let mut texts = Vec::new();
        for (agent, table) in &self.extras {
            let at = keyed("extras", agent);
            for (key, value) in table {
                texts_in(keyed(&at, key), value, &mut texts);
            }
        }
        for (at, text) in texts {
            for prefix in found(text) {
                literals.push((at.clone(), prefix));
            }
        }
        literals
    }

    /// Whether `input` is a part of an address written only in its
    /// userinfo, which went with any credential when the address was read.
    fn withholds(&self, input: &Input) -> bool {
        match &self.transport {
            Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                input.kind == InputKind::UrlVar && endpoint.withheld.contains(&input.name)
            }
            Transport::Stdio(_) => false,
        }
    }

    /// Whether some value of the transport writes `input` where its kind
    /// says it may be. Matched on both, so a transport or a kind added has
    /// to say where one is written in the other.
    fn references(&self, input: &Input) -> bool {
        let written = written(&input.name);
        let holds = |value: &str| value.contains(&written);
        match (&self.transport, input.kind) {
            (Transport::Stdio(stdio), InputKind::Env) => stdio.env.values().any(|v| holds(v)),
            (Transport::Stdio(stdio), InputKind::Arg) => stdio.args.iter().any(|v| holds(v)),
            (Transport::Http(endpoint) | Transport::Sse(endpoint), InputKind::Header) => {
                endpoint.headers.values().any(|v| holds(v))
            }
            (Transport::Http(endpoint) | Transport::Sse(endpoint), InputKind::UrlVar) => {
                holds(&endpoint.url)
            }
            // A program has no headers and no address, and an address no
            // environment and no arguments: an input of such a kind has
            // nowhere to be written.
            (Transport::Stdio(_), InputKind::Header | InputKind::UrlVar)
            | (Transport::Http(_) | Transport::Sse(_), InputKind::Env | InputKind::Arg) => false,
        }
    }

    /// The extras the definition gives `agent`, by the name a command line
    /// takes for it.
    pub fn extras_for(&self, agent: crate::agent::Agent) -> Option<&toml::Table> {
        self.extras.get(agent.name())
    }
}

/// `source`, a definition's text, with its `name` set to `name` and, when
/// one is given, its `description` to `description`, every other byte as
/// it was, the comment on each one's line among them: rewritten through
/// `toml_edit`, as a marker is, and read back to make sure the values
/// changed what they were meant to and nothing else.
///
/// What `rename` writes, and what `add` makes of a template: the one way a
/// definition somebody wrote is given another name.
pub(crate) fn declaring(
    source: &str,
    name: &str,
    description: Option<&str>,
) -> Result<String, String> {
    let before = McpDefinition::parse(source).map_err(|error| error.to_string())?;
    let mut document: toml_edit::DocumentMut = source
        .parse()
        .map_err(|error: toml_edit::TomlError| error.to_string())?;
    let mut set = vec![("name", name)];
    set.extend(description.map(|description| ("description", description)));
    for (key, to) in &set {
        let Some(value) = document
            .get_mut(key)
            .and_then(toml_edit::Item::as_value_mut)
        else {
            return Err(format!("its `{key}` is not a value at the top of the file"));
        };
        let decor = value.decor().clone();
        *value = toml_edit::Value::from(*to);
        *value.decor_mut() = decor;
    }
    let rewritten = document.to_string();

    let after = McpDefinition::parse(&rewritten).map_err(|error| error.to_string())?;
    let wanted = McpDefinition {
        name: ArtifactName::from(name),
        description: description.map_or_else(|| before.description.clone(), str::to_owned),
        ..before
    };
    if after != wanted {
        let keys: Vec<String> = set.iter().map(|(key, _)| format!("`{key}`")).collect();
        return Err(format!(
            "rewriting it would change more than its {}",
            keys.join(" and ")
        ));
    }
    Ok(rewritten)
}

/// Why a file is not a definition this build can read.
///
/// Each one refuses the file, as front matter that does not parse refuses a
/// skill: there is no definition to list. What a definition that reads can
/// still get wrong is `validate`'s ([`McpDefinition::issues`]).
#[derive(Debug, Error)]
pub enum McpError {
    #[error("not valid TOML: {0}")]
    Unparsed(#[source] toml::de::Error),
    #[error("names no `format`, so which format it is written in is not known; this build writes format {MCP_FORMAT}")]
    NoFormat,
    #[error(
        "written in format {found}, by a newer Mindweaver; this one reads format {MCP_FORMAT}"
    )]
    FormatTooNew { found: i64 },
    #[error("`format` is {found}, which no definition was ever written in; this build writes format {MCP_FORMAT}")]
    FormatUnknown { found: String },
    #[error("not a definition of format {MCP_FORMAT}: {0}")]
    Malformed(#[source] toml::de::Error),
}

codes!(McpError {
    Self::Unparsed(..) => "mcp_unparsed",
    Self::NoFormat => "mcp_format_missing",
    Self::FormatTooNew { .. } => "mcp_format_too_new",
    Self::FormatUnknown { .. } => "mcp_format_unknown",
    Self::Malformed(..) => "mcp_malformed",
});

/// The file, as serde reads it: every key typed, and nothing judged.
#[derive(Deserialize)]
struct Raw {
    name: String,
    description: String,
    transport: RawTransport,
    #[serde(default)]
    inputs: Vec<RawInput>,
    #[serde(default)]
    provenance: Option<RawUpstream>,
    #[serde(default)]
    extras: BTreeMap<String, toml::Table>,
}

/// A transport by its `type`, each key it needs read as empty when it is
/// left out, so that one missing is `validate`'s to say rather than a file
/// refused.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum RawTransport {
    Stdio {
        #[serde(default)]
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
        #[serde(default)]
        cwd: Option<String>,
    },
    Http {
        #[serde(default)]
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
    Sse {
        #[serde(default)]
        url: String,
        #[serde(default)]
        headers: BTreeMap<String, String>,
    },
}

#[derive(Deserialize)]
struct RawInput {
    name: String,
    kind: InputKind,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    secret: bool,
    #[serde(default)]
    env_var: Option<String>,
}

#[derive(Deserialize)]
struct RawUpstream {
    #[serde(default)]
    registry: Option<String>,
    #[serde(default)]
    upstream_name: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(default, deserialize_with = "moment")]
    fetched_at: Option<String>,
}

/// `fetched_at`, as a string or as the TOML date and time a registry's
/// timestamp is naturally written as, kept as its text.
fn moment<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    match Option::<toml::Value>::deserialize(deserializer)? {
        None => Ok(None),
        Some(toml::Value::String(text)) => Ok(Some(text)),
        Some(toml::Value::Datetime(moment)) => Ok(Some(moment.to_string())),
        Some(_) => Err(de::Error::custom(
            "`fetched_at` is neither a date and time nor a string",
        )),
    }
}

/// What the first token `text` holds starts with, looked for as `validate`
/// looks through a value: for a refusal that would say `text` before any
/// definition holds it (a key `add` is given twice, what is kept of an
/// address typed with a credential), which then says it by that alone,
/// since said whole it would print the secret wherever the message goes.
pub(crate) fn token_in(text: &str) -> Option<&'static str> {
    token::found(text, &[]).first().copied()
}

/// How an input named `name` is written where its value goes.
pub(crate) fn written(name: &InputName) -> String {
    format!("{{{name}}}")
}

/// `text` with each placeholder of an input in `read` written as `spell`
/// spells an agent's reference to the variable it is read from, and every
/// other byte as it is, a brace naming no input among them.
///
/// In one pass from the left, so a reference written in place of one
/// placeholder is never read again as another: Claude's `${A}` holds `{A}`,
/// which another input may be named. One function for every agent that
/// refers to a variable where a value goes, each renderer handing it the
/// agent's spelling, rather than a copy of the pass in each.
pub(crate) fn referring(
    text: &str,
    read: &[(&InputName, &VariableName)],
    spell: impl Fn(&VariableName) -> String,
) -> String {
    let mut written = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        written.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let named = after.find('}').and_then(|close| {
            let name = &after[..close];
            read.iter()
                .find(|(input, _)| input.as_str() == name)
                .map(|(_, variable)| (close, variable))
        });
        match named {
            Some((close, variable)) => {
                written.push_str(&spell(variable));
                rest = &after[close + 1..];
            }
            None => {
                written.push('{');
                rest = after;
            }
        }
    }
    written.push_str(rest);
    written
}

/// `key` in the table `table` names, as TOML spells the path to it: bare
/// when it may be, and quoted when a dot or a space in it would read as
/// another key.
pub(crate) fn keyed(table: &str, key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if bare {
        format!("{table}.{key}")
    } else {
        format!("{table}.{}", toml::Value::String(key.to_owned()))
    }
}

/// Every text `value` holds, at any depth, beside the path that leads to
/// it from `at`, as TOML spells it: a table's keys through [`keyed`], an
/// array's items by their index. Matched on every kind of value, so one
/// TOML adds says whether it holds text.
fn texts_in<'a>(at: String, value: &'a toml::Value, into: &mut Vec<(String, &'a str)>) {
    match value {
        toml::Value::String(text) => into.push((at, text)),
        toml::Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                texts_in(format!("{at}[{index}]"), value, into);
            }
        }
        toml::Value::Table(table) => {
            for (key, value) in table {
                texts_in(keyed(&at, key), value, into);
            }
        }
        toml::Value::Integer(_)
        | toml::Value::Float(_)
        | toml::Value::Boolean(_)
        | toml::Value::Datetime(_) => {}
    }
}

impl From<Raw> for McpDefinition {
    fn from(raw: Raw) -> Self {
        let inputs: Vec<Input> = raw
            .inputs
            .into_iter()
            .map(|input| Input {
                name: InputName::from(input.name),
                kind: input.kind,
                description: input.description,
                required: input.required,
                secret: input.secret,
                env_var: input.env_var.map(VariableName::from),
            })
            .collect();
        // Read off the address as written and what is kept of it, here,
        // where both are at hand: once redacted, which inputs its userinfo
        // held is not known.
        let placeholders: Vec<String> = inputs.iter().map(|input| written(&input.name)).collect();
        let endpoint = |url: String, headers| {
            let kept = Redacted::new(&url);
            let tokens = token::found(&url, &placeholders);
            let withheld = inputs
                .iter()
                .map(|input| &input.name)
                .filter(|name| {
                    let written = written(name);
                    url.contains(&written) && !kept.contains(&written)
                })
                .cloned()
                .collect();
            Endpoint {
                url: kept,
                headers,
                withheld,
                tokens,
            }
        };
        let transport = match raw.transport {
            RawTransport::Stdio {
                command,
                args,
                env,
                cwd,
            } => Transport::Stdio(Stdio {
                command,
                args,
                env,
                cwd,
            }),
            RawTransport::Http { url, headers } => Transport::Http(endpoint(url, headers)),
            RawTransport::Sse { url, headers } => Transport::Sse(endpoint(url, headers)),
        };
        McpDefinition {
            name: ArtifactName::from(raw.name),
            description: raw.description,
            transport,
            inputs,
            provenance: raw.provenance.map(|upstream| Upstream {
                registry: upstream.registry.as_deref().map(Redacted::new),
                upstream_name: upstream.upstream_name,
                version: upstream.version,
                fetched_at: upstream.fetched_at,
            }),
            extras: raw.extras,
        }
    }
}

#[cfg(test)]
#[path = "mcp_test.rs"]
mod tests;
