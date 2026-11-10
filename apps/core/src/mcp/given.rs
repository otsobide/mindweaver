//! What `add` is told of an MCP server it writes a definition for: how an
//! agent starts the server or reaches it, as a command line's options say,
//! and the definition's text written from them.
//!
//! Every value is a name or a literal the definition holds as it is, as
//! #86's model has it. A literal is written as typed (`--env K=V`,
//! `--header K=V`), and judged as every value of a transport is, so one
//! shaped like a token is refused. Every other option declares an input,
//! read from the variable it names and never given a value (D-11), and
//! named after that variable, so its placeholder says which one: `{VAR}`.
//! Which of those is a secret is said here, once ([`Program`],
//! [`Address`]): what reaches a server through its environment or a
//! header is, and what goes into its arguments or its address is not,
//! being what a process list, a log or a proxy shows, and what Codex has
//! no way to read a secret into.
//!
//! Nothing here judges what it writes. The text is read back and held to
//! the one strict function `validate` and `rename` hold a definition to
//! ([`crate::create`]), so `add` refuses exactly what `validate` would call
//! invalid. What no definition can hold at all, two transports, or an
//! option of one given with the other, no value of this type can say: a
//! front end's grammar refuses it before one is made.
//!
//! The other way round, a definition is read back as what `add` would be
//! told to write it ([`Given::of`]), when the options can say all of it:
//! what a form changing one starts from, and what `edit` writes from
//! again ([`retransported`]), every other key of the file kept.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::mcp::{written, Input, InputKind, McpDefinition, Transport};
use crate::value::{InputName, VariableName};

/// How an agent starts an MCP server, or reaches one: a program or an
/// address, never both.
///
/// In JSON, what a front end sends a command line's options as, and what
/// the snapshot says a definition is told as: tagged by `type` inside the
/// object, `{"type": "program", "command": "npx", ...}`, as a task is, each
/// pair as an array of its two words. A field it does not have is refused,
/// as a task's is: it is a write, and one misspelled would be dropped.
/// Every secret is the name of the variable it is read from, and no field
/// holds a secret's value, there being none for it to hold.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Given {
    /// A program an agent starts, `--command`.
    Program(Program),
    /// A server at an address, `--url`.
    Address(Address),
}

/// A program an agent starts, and what it is started with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Program {
    /// The program, as the agent finds it (`--command`).
    pub command: String,
    /// What it is started with, in order (`--arg`), before every argument
    /// read from a variable.
    pub args: Vec<String>,
    /// What its environment is given, each value as written (`--env
    /// K=V`), before every secret.
    pub env: Vec<(String, String)>,
    /// Each variable passed on to its environment under its own name, a
    /// secret (`--secret-env VAR`): the shape every agent can give one,
    /// Codex's `env_vars` among them.
    pub secret_env: Vec<VariableName>,
    /// Each argument read from a variable, after those written as they are
    /// (`--arg-env VAR`). No secret: a program's arguments are what a
    /// process list shows, and Codex reads no secret into one.
    pub arg_env: Vec<VariableName>,
}

/// A server at an address, and what every request to it carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Address {
    /// Where the server is, as typed (`--url`): checked for a credential
    /// before anything is written, since the file would keep one, and
    /// never kept or said whole once one is found.
    pub url: String,
    /// Whether it is reached over server-sent events rather than streamable
    /// HTTP (`--sse`).
    pub events: bool,
    /// What every request carries, each value as written (`--header K=V`),
    /// before every secret.
    pub headers: Vec<(String, String)>,
    /// Each header whose whole value is read from a variable, a secret
    /// (`--secret-header H=VAR`): what Codex reads one into
    /// (`env_http_headers`).
    pub secret_headers: Vec<(String, VariableName)>,
    /// The variable `Authorization: Bearer` is followed by, a secret
    /// (`--bearer-env VAR`): what Codex reads one into
    /// (`bearer_token_env_var`).
    pub bearer: Option<VariableName>,
    /// Each part of the address read from a variable, by the name the
    /// address writes between braces where it goes (`--url-var NAME=VAR`).
    /// No secret: an address is what a log or a proxy shows, and Codex
    /// reads no secret into one.
    pub url_vars: Vec<(InputName, VariableName)>,
}

/// The header a bearer token is sent in.
const AUTHORIZATION: &str = "Authorization";

/// A key of a transport's table given more than once, which no TOML file
/// can hold: the table, as the definition names it, and the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repeated {
    pub table: &'static str,
    pub key: String,
}

/// A name given beside a value that holds `=`, which no command line can
/// say: the option that takes the pair, as a line spells it, and the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsayable {
    pub option: &'static str,
    pub key: String,
}

impl Given {
    /// The first key of `env` or `headers` given twice, by an option that
    /// writes a value and one that reads it, or by one option twice.
    ///
    /// Exactly as given: two headers that differ by case are two keys to
    /// the file, which holds them both, and to `validate`, which says
    /// nothing of them.
    pub fn repeated(&self) -> Option<Repeated> {
        let (table, keys): (&'static str, Vec<&str>) = match self {
            Given::Program(program) => (
                "env",
                program
                    .env
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .chain(program.secret_env.iter().map(VariableName::as_str))
                    .collect(),
            ),
            Given::Address(address) => (
                "headers",
                address
                    .headers
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .chain(address.secret_headers.iter().map(|(key, _)| key.as_str()))
                    .chain(address.bearer.iter().map(|_| AUTHORIZATION))
                    .collect(),
            ),
        };
        keys.iter().enumerate().find_map(|(at, key)| {
            keys[..at].contains(key).then(|| Repeated {
                table,
                key: (*key).to_owned(),
            })
        })
    }

    /// The first name given beside a value that holds `=`: a setting's
    /// variable, a header's name, a secret header's, or a part of the
    /// address's. Each option of a pair takes it as `NAME=VALUE` and ends
    /// the name at the first `=` (`--env X=Y=Z` sets `X`), so a line
    /// would say another pair than the one given, and what a request sent
    /// would be judged as one definition and written as another. Only a
    /// request can give one, never a command line; and no variable's,
    /// header's or input's name holds one.
    pub fn unsayable(&self) -> Option<Unsayable> {
        let pairs: Vec<(&'static str, &str)> = match self {
            Given::Program(program) => program
                .env
                .iter()
                .map(|(key, _)| ("--env", key.as_str()))
                .collect(),
            Given::Address(address) => address
                .headers
                .iter()
                .map(|(key, _)| ("--header", key.as_str()))
                .chain(
                    address
                        .secret_headers
                        .iter()
                        .map(|(header, _)| ("--secret-header", header.as_str())),
                )
                .chain(
                    address
                        .url_vars
                        .iter()
                        .map(|(name, _)| ("--url-var", name.as_str())),
                )
                .collect(),
        };
        pairs
            .into_iter()
            .find(|(_, key)| key.contains('='))
            .map(|(option, key)| Unsayable {
                option,
                key: key.to_owned(),
            })
    }

    /// What `add` would be told to write `definition`'s transport and
    /// inputs as they are: none when it holds what no option says.
    ///
    /// Every input declared as `add` declares one, named after the
    /// variable it is read from but for a part of an address, which is
    /// named in it, with no description and not said to be required; a
    /// secret passed on to a program's environment under its variable's
    /// name, a secret header's whole value, `Authorization: Bearer` and a
    /// secret, an argument read from a variable after every argument
    /// written as it is, and a part of the address; and nothing else: no
    /// folder to start a program in, no input written anywhere else, and
    /// no placeholder in a value written as it is. Then writing it again
    /// from what this answers gives an agent what the file gives it now,
    /// its keys in the order `add` writes them.
    ///
    /// None rather than the part the options can say: a form starting
    /// from that would write the rest away. Such a definition is changed
    /// as text, whole.
    pub fn of(definition: &McpDefinition) -> Option<Given> {
        let inputs = &definition.inputs;
        if inputs
            .iter()
            .any(|input| input.description.is_some() || input.required)
        {
            return None;
        }
        // Each input, once, by its place among them.
        let mut used = vec![false; inputs.len()];
        let mut take = |found: Option<usize>| -> Option<&Input> {
            let at = found?;
            (!used[at]).then(|| {
                used[at] = true;
                &inputs[at]
            })
        };
        // An input as `add` declares one read from the variable it is
        // named after.
        let as_named = |input: &Input, kind: InputKind, secret: bool| {
            input.kind == kind
                && input.secret == secret
                && input
                    .env_var
                    .as_ref()
                    .is_some_and(|variable| variable.as_str() == input.name.as_str())
        };
        let placeholders: Vec<String> = inputs.iter().map(|input| written(&input.name)).collect();
        let plain = |text: &str| !placeholders.iter().any(|written| text.contains(written));
        let given = match &definition.transport {
            Transport::Stdio(stdio) => {
                if stdio.cwd.is_some() {
                    return None;
                }
                let read_at = |text: &str, kind: InputKind, secret: bool| {
                    inputs.iter().position(|input| {
                        written(&input.name) == text && as_named(input, kind, secret)
                    })
                };
                let mut env = Vec::new();
                let mut secret_env = Vec::new();
                for (key, value) in &stdio.env {
                    match read_at(value, InputKind::Env, true) {
                        Some(at) if inputs[at].name.as_str() == key => {
                            secret_env.push((at, take(Some(at))?.name.as_str().into()));
                        }
                        _ if plain(value) => env.push((key.clone(), value.clone())),
                        _ => return None,
                    }
                }
                let written_as_is = stdio
                    .args
                    .iter()
                    .take_while(|arg| read_at(arg, InputKind::Arg, false).is_none())
                    .count();
                let (args, read) = stdio.args.split_at(written_as_is);
                if !args.iter().all(|arg| plain(arg)) {
                    return None;
                }
                let mut arg_env = Vec::new();
                for arg in read {
                    arg_env.push(
                        take(read_at(arg, InputKind::Arg, false))?
                            .name
                            .as_str()
                            .into(),
                    );
                }
                // In the order the inputs are declared, as `add` declares
                // them: the order an agent is told nothing of.
                secret_env.sort_by_key(|(at, _): &(usize, VariableName)| *at);
                Given::Program(Program {
                    command: stdio.command.clone(),
                    args: args.to_vec(),
                    env,
                    secret_env: secret_env
                        .into_iter()
                        .map(|(_, variable)| variable)
                        .collect(),
                    arg_env,
                })
            }
            Transport::Http(endpoint) | Transport::Sse(endpoint) => {
                if !endpoint.withheld.is_empty() {
                    return None;
                }
                let secret_at = |variable: &str| {
                    inputs.iter().position(|input| {
                        input.name.as_str() == variable && as_named(input, InputKind::Header, true)
                    })
                };
                let mut headers = Vec::new();
                let mut secret_headers = Vec::new();
                let mut bearer = None;
                for (key, value) in &endpoint.headers {
                    let whole = value
                        .strip_prefix('{')
                        .and_then(|rest| rest.strip_suffix('}'))
                        .and_then(secret_at);
                    let token = (key == AUTHORIZATION)
                        .then(|| value.strip_prefix("Bearer {")?.strip_suffix('}'))
                        .flatten()
                        .and_then(secret_at);
                    match (whole, token) {
                        (Some(at), _) => {
                            secret_headers.push((
                                at,
                                key.clone(),
                                take(Some(at))?.name.as_str().into(),
                            ));
                        }
                        (None, Some(at)) if bearer.is_none() => {
                            bearer = Some(VariableName::from(take(Some(at))?.name.as_str()));
                        }
                        _ if plain(value) => headers.push((key.clone(), value.clone())),
                        _ => return None,
                    }
                }
                let mut url_vars = Vec::new();
                for (at, input) in inputs.iter().enumerate() {
                    if input.kind != InputKind::UrlVar || input.secret || used[at] {
                        continue;
                    }
                    let variable = input.env_var.clone()?;
                    if !endpoint.url.contains(&written(&input.name)) {
                        return None;
                    }
                    used[at] = true;
                    url_vars.push((input.name.clone(), variable));
                }
                let url_read: Vec<String> =
                    url_vars.iter().map(|(name, _)| written(name)).collect();
                let others = placeholders
                    .iter()
                    .filter(|written| !url_read.contains(written));
                if others
                    .clone()
                    .any(|written| endpoint.url.contains(written.as_str()))
                {
                    return None;
                }
                secret_headers.sort_by_key(|(at, _, _): &(usize, String, VariableName)| *at);
                Given::Address(Address {
                    url: endpoint.url.to_string(),
                    events: matches!(definition.transport, Transport::Sse(_)),
                    headers,
                    secret_headers: secret_headers
                        .into_iter()
                        .map(|(_, header, variable)| (header, variable))
                        .collect(),
                    bearer,
                    url_vars,
                })
            }
        };
        used.iter().all(|used| *used).then_some(given)
    }

    /// The text of a definition of `name`, described as `description`,
    /// written from what is given: a template of its own, its comments
    /// saying what each part is for, as a marker's say what each key is,
    /// and every value quoted as TOML quotes it, so that one holding a
    /// quote or a line break reads back as it was typed.
    ///
    /// A key given twice ([`repeated`](Self::repeated)) is written twice,
    /// so the text does not read: ask that first.
    pub fn text(&self, name: &str, description: &str) -> String {
        let mut text = String::from(
            "# An MCP server, as Mindweaver keeps it: each agent's own entry for it\n\
             # is written from this file, and `show` says how each agent is given it.\n",
        );
        let _ = writeln!(text, "format = {}", super::MCP_FORMAT);
        let _ = writeln!(text, "name = {}", quoted(name));
        let _ = writeln!(text, "description = {}", quoted(description));
        text.push('\n');
        let inputs = match self {
            Given::Program(program) => program.write(&mut text),
            Given::Address(address) => address.write(&mut text),
        };
        for (at, input) in inputs.iter().enumerate() {
            text.push('\n');
            if at == 0 {
                text.push_str(
                    "# What is supplied when an agent starts it, each written between braces\n\
                     # where it goes and read from the environment variable it names: no value\n\
                     # is written here, and a secret never.\n",
                );
            }
            input.write(&mut text);
        }
        text
    }
}

/// `text`, a definition's file, with how its server is started or reached
/// and what is supplied for it, its `transport` and `inputs`, as `given`
/// says them, written as `add` writes them; every other key, its name, its
/// description, its provenance and its extras, kept as it was, the
/// comments on them with it, through `toml_edit`.
///
/// What `edit` writes when it is told a server's transport rather than
/// its contents: a form changing how a server is started has the options'
/// words, and a definition written again whole from them would drop what
/// the file holds beside them. Refused, with why, when the file does not
/// read as a definition, whose transport cannot be told apart from the
/// rest then.
///
/// `text` as it is when `given` says what it holds already, its
/// `transport` and `inputs` the same values to TOML: a form starts from
/// what [`Given::of`] reads back, `env` and `headers` in the order of
/// their keys, which is no order an agent reads, so a change told nothing
/// new rewrites nothing, a layout or a comment of somebody's in those
/// tables included, and `edit` writes nothing.
pub(crate) fn retransported(text: &str, given: &Given) -> Result<String, String> {
    let before = McpDefinition::parse(text).map_err(|error| error.to_string())?;
    let mut document: toml_edit::DocumentMut = text
        .parse()
        .map_err(|error: toml_edit::TomlError| error.to_string())?;
    let rewritten = given.text(&before.name, &before.description);
    let written: toml_edit::DocumentMut = rewritten
        .parse()
        .map_err(|error: toml_edit::TomlError| error.to_string())?;
    if let (Ok(held), Ok(told)) = (started(text), started(&rewritten)) {
        if held == told {
            return Ok(text.to_owned());
        }
    }
    // Where the transport stood, so the new one stands there, its inputs
    // after it, and not below the extras, which a file reads as written
    // after them: each table is given the one place, which `toml_edit`
    // lays out in the order they are inserted, before any table after it.
    let at = document
        .get("transport")
        .and_then(toml_edit::Item::as_table)
        .and_then(toml_edit::Table::position)
        .unwrap_or(isize::MAX);
    document.remove("transport");
    document.remove("inputs");
    for key in ["transport", "inputs"] {
        let Some(item) = written.get(key) else {
            continue;
        };
        let mut item = item.clone();
        match &mut item {
            toml_edit::Item::Table(table) => table.set_position(Some(at)),
            toml_edit::Item::ArrayOfTables(tables) => {
                for table in tables.iter_mut() {
                    table.set_position(Some(at));
                }
            }
            toml_edit::Item::None | toml_edit::Item::Value(_) => {}
        }
        document.insert(key, item);
    }
    Ok(document.to_string())
}

/// What a definition's `text` says of how its server is started or reached
/// and of what is supplied for it, its `transport` and `inputs`, as TOML
/// reads their values: a table's keys in no order.
fn started(text: &str) -> Result<(Option<toml::Value>, Option<toml::Value>), toml::de::Error> {
    let mut table: toml::Table = toml::from_str(text)?;
    Ok((table.remove("transport"), table.remove("inputs")))
}

/// An input a definition declares, as `add` writes it.
struct Declared<'a> {
    name: &'a str,
    kind: &'static str,
    secret: bool,
    variable: &'a VariableName,
}

impl Declared<'_> {
    fn write(&self, text: &mut String) {
        text.push_str("[[inputs]]\n");
        let _ = writeln!(text, "name = {}", quoted(self.name));
        let _ = writeln!(text, "kind = {}", quoted(self.kind));
        if self.secret {
            text.push_str("secret = true\n");
        }
        let _ = writeln!(text, "env_var = {}", quoted(self.variable));
    }
}

impl Program {
    /// Its transport, written into `text`, and the inputs it declares.
    fn write<'a>(&'a self, text: &mut String) -> Vec<Declared<'a>> {
        text.push_str(
            "# How an agent starts it: the program, what it is started with, and what\n\
             # its environment is given beside the agent's own.\n\
             [transport]\n\
             type = \"stdio\"\n",
        );
        let _ = writeln!(text, "command = {}", quoted(&self.command));
        let args: Vec<String> = self
            .args
            .iter()
            .cloned()
            .chain(self.arg_env.iter().map(placeholder))
            .collect();
        if !args.is_empty() {
            let items: Vec<String> = args.iter().map(|arg| quoted(arg)).collect();
            let _ = writeln!(text, "args = [{}]", items.join(", "));
        }
        let env: Vec<(String, String)> = self
            .env
            .iter()
            .cloned()
            .chain(
                self.secret_env
                    .iter()
                    .map(|variable| (variable.to_string(), placeholder(variable))),
            )
            .collect();
        if !env.is_empty() {
            let _ = writeln!(text, "env = {}", inline(&env));
        }
        self.secret_env
            .iter()
            .map(|variable| Declared {
                name: variable.as_str(),
                kind: "env",
                secret: true,
                variable,
            })
            .chain(self.arg_env.iter().map(|variable| Declared {
                name: variable.as_str(),
                kind: "arg",
                secret: false,
                variable,
            }))
            .collect()
    }
}

impl Address {
    /// Its transport, written into `text`, and the inputs it declares.
    fn write<'a>(&'a self, text: &mut String) -> Vec<Declared<'a>> {
        let (over, word) = if self.events {
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
        let _ = writeln!(text, "url = {}", quoted(&self.url));
        let headers: Vec<(String, String)> = self
            .headers
            .iter()
            .cloned()
            .chain(
                self.secret_headers
                    .iter()
                    .map(|(header, variable)| (header.clone(), placeholder(variable))),
            )
            .chain(self.bearer.iter().map(|variable| {
                (
                    AUTHORIZATION.to_owned(),
                    format!("Bearer {}", placeholder(variable)),
                )
            }))
            .collect();
        if !headers.is_empty() {
            let _ = writeln!(text, "headers = {}", inline(&headers));
        }
        self.secret_headers
            .iter()
            .map(|(_, variable)| Declared {
                name: variable.as_str(),
                kind: "header",
                secret: true,
                variable,
            })
            .chain(self.bearer.iter().map(|variable| Declared {
                name: variable.as_str(),
                kind: "header",
                secret: true,
                variable,
            }))
            .chain(self.url_vars.iter().map(|(name, variable)| Declared {
                name: name.as_str(),
                kind: "url_var",
                secret: false,
                variable,
            }))
            .collect()
    }
}

/// Where an input named after `variable` is written: `{VAR}`.
fn placeholder(variable: &VariableName) -> String {
    written(&InputName::from(variable.as_str()))
}

/// `text` as a TOML string, quoted and escaped as TOML writes one.
pub(crate) fn quoted(text: &str) -> String {
    toml::Value::String(text.to_owned()).to_string()
}

/// `entries` as an inline table, in the order given: each key bare where a
/// TOML key may be, and quoted where a dot or a space would read as
/// another.
pub(crate) fn inline(entries: &[(String, String)]) -> String {
    let entries: Vec<String> = entries
        .iter()
        .map(|(key, value)| format!("{} = {}", bare_or_quoted(key), quoted(value)))
        .collect();
    format!("{{ {} }}", entries.join(", "))
}

/// `key` as TOML spells it: bare when it may be, quoted otherwise.
fn bare_or_quoted(key: &str) -> String {
    let bare = !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if bare {
        key.to_owned()
    } else {
        quoted(key)
    }
}

#[cfg(test)]
#[path = "given_test.rs"]
mod tests;
