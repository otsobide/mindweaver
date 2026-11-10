//! What an agent is given to start or reach an MCP server, as its own entry
//! spells it: what an install says of each agent before it writes anything
//! (#95).
//!
//! Read off each renderer's entry ([`crate::claude_mcp::Entry::runs`],
//! [`crate::codex_mcp::Entry::runs`], [`crate::opencode_mcp::Entry::runs`])
//! rather than off the definition: an input reaches Claude as `${VAR}` and
//! OpenCode as `{env:VAR}`, and Codex is given a program's arguments as
//! they are, so what each agent will run is what its entry holds, and a
//! front end spelling it again from the definition would say what no agent
//! is given.

use std::fmt;

use serde::Serialize;

use crate::config_entry::Value;
use crate::mcp::Transport;

/// The program an agent starts, or the address it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Runs {
    /// A program: what the agent runs, and each argument after it, as the
    /// entry spells them.
    Program { command: String, args: Vec<String> },
    /// An address, as the entry spells it, which holds no credential: the
    /// definition's goes through [`crate::url::redacted`].
    Address { url: String },
}

impl Runs {
    /// What the object `entry` holds as a program, its command under
    /// `command` and its arguments under `args`, or as an address under
    /// `url`: none when it holds neither, which no renderer writes.
    pub(crate) fn of(entry: &Value) -> Option<Runs> {
        if let Some(url) = member(entry, "url").and_then(text) {
            return Some(Runs::Address {
                url: url.to_owned(),
            });
        }
        let command = member(entry, "command").and_then(text)?;
        Some(Runs::Program {
            command: command.to_owned(),
            args: words(member(entry, "args"))?,
        })
    }

    /// What `transport` starts or reaches as a definition writes it, each
    /// input between its braces where it goes: what a front end shows of a
    /// definition no agent holds yet, an import from a registry before it
    /// is written (#107). What an agent is given is its own entry's, read
    /// by [`of`](Self::of), since each agent spells an input its own way.
    pub fn of_transport(transport: &Transport) -> Runs {
        match transport {
            Transport::Stdio(stdio) => Runs::Program {
                command: stdio.command.clone(),
                args: stdio.args.clone(),
            },
            Transport::Http(endpoint) | Transport::Sse(endpoint) => Runs::Address {
                url: endpoint.url.to_string(),
            },
        }
    }

    /// What the object `entry` holds as a program whose command and
    /// arguments are one array under `command`, OpenCode's shape, or as an
    /// address under `url`.
    pub(crate) fn of_line(entry: &Value) -> Option<Runs> {
        if let Some(url) = member(entry, "url").and_then(text) {
            return Some(Runs::Address {
                url: url.to_owned(),
            });
        }
        let mut line = words(member(entry, "command"))?.into_iter();
        Some(Runs::Program {
            command: line.next()?,
            args: line.collect(),
        })
    }
}

/// What an agent runs or reaches, as a front end shows it: whether it is
/// an address rather than a program, and the line, as [`Runs`] displays
/// it, each word a shell would split quoted.
///
/// The line as core writes it rather than the words for a front end to
/// join, which would be the shell's quoting spelled again in TypeScript,
/// and could show a line no shell reads as the program it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Running {
    /// Whether it reaches an address, which it then is, rather than
    /// starting a program.
    pub reaches: bool,
    /// The program as a shell line that runs it, or the address.
    pub line: String,
}

impl Runs {
    /// What a front end shows of it.
    pub fn shown(&self) -> Running {
        Running {
            reaches: matches!(self, Runs::Address { .. }),
            line: self.to_string(),
        }
    }
}

/// A program as a shell line that runs it, each word that a shell would
/// split or read as something else quoted, so an argument holding a space
/// reads as the one argument it is; an address as it is.
///
/// A reference to a variable, `${VAR}` or `{env:VAR}`, is left unquoted:
/// it is what the agent fills in, and quoting it would read as though it
/// were given the text.
impl fmt::Display for Runs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Runs::Address { url } => f.write_str(url),
            Runs::Program { command, args } => {
                f.write_str(&quoted(command))?;
                for arg in args {
                    write!(f, " {}", quoted(arg))?;
                }
                Ok(())
            }
        }
    }
}

/// `word` as one word of a shell line: as it is when a shell reads it as
/// itself ([`bare`]), and otherwise between single quotes, a single quote
/// in it closed, escaped and opened again.
fn quoted(word: &str) -> String {
    if bare(word) {
        return word.to_owned();
    }
    format!("'{}'", word.replace('\'', r"'\''"))
}

/// Whether a shell reads `word` as itself: it holds only what no shell
/// splits or expands, braces only around a reference to a variable
/// (`${VAR}`, `{env:VAR}`), which is what the agent fills in, and a `~`
/// only where no shell expands one, past the first character and not after
/// a `=` or a `:`, where bash expands it in a word that reads as an
/// assignment. Braces anywhere else are quoted, since `{a,b}` is two words
/// to bash, and an empty word, which is none.
fn bare(word: &str) -> bool {
    let plain = |c: char| c.is_ascii_alphanumeric() || "-_./:@%+=,^".contains(c);
    let named = |name: &str| {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    };
    let mut rest = word;
    let mut before: Option<char> = None;
    while let Some(c) = rest.chars().next() {
        if let Some(inside) = rest
            .strip_prefix("${")
            .or_else(|| rest.strip_prefix("{env:"))
        {
            let Some(end) = inside.find('}') else {
                return false;
            };
            if !named(&inside[..end]) {
                return false;
            }
            rest = &inside[end + 1..];
            before = Some('}');
            continue;
        }
        let tilde = c == '~' && before.is_some_and(|before| before != '=' && before != ':');
        if !(plain(c) || tilde) {
            return false;
        }
        rest = &rest[c.len_utf8()..];
        before = Some(c);
    }
    !word.is_empty()
}

/// The member `key` of the object `value`.
fn member<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(members) => members
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value),
        _ => None,
    }
}

/// The text `value` holds, when it is text.
fn text(value: &Value) -> Option<&str> {
    match value {
        Value::String(text) => Some(text),
        _ => None,
    }
}

/// Each word of the array `value`, none when there is no array, as Codex's
/// entry leaves `args` out when it holds none; refused when one is no text.
fn words(value: Option<&Value>) -> Option<Vec<String>> {
    match value {
        None => Some(Vec::new()),
        Some(Value::Array(words)) => words
            .iter()
            .map(|word| text(word).map(str::to_owned))
            .collect(),
        Some(_) => None,
    }
}

#[cfg(test)]
#[path = "runs_test.rs"]
mod tests;
