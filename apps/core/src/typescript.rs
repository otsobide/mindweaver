//! What core hands a front end, in TypeScript, for the panel to compile
//! against (#70): what the `ts` feature adds, and nothing else builds.
//!
//! ts-rs derives most of it, beside each type's serde derive and from the
//! same attributes, so a derived type says in TypeScript what serde writes.
//! What serde writes by hand ts-rs cannot read, and it would spell the Rust
//! variants that a hand-written serializer exists not to, so each such type
//! is given its TypeScript from what its serializer writes, beside it:
//!
//! - **A word**, a kind, an agent or a state: the union of the words, from
//!   the function its serializer calls ([`crate::described::words!`]).
//! - **A path or a URL**, written as text whatever it holds: a `string`,
//!   which is what ts-rs says of a `PathBuf` and a `String` already. Each
//!   field written through a function of its own (`serialize_with`) is one
//!   ts-rs reads as its type, so a test holds each such function to what
//!   ts-rs says of the type it takes, and
//!   [`crate::described::field_serializers`] finds every one a field names,
//!   for a test to refuse one it does not hold.
//! - **Anything else**, an issue `validate` finds, a template's origin, a
//!   skill's metadata: from what its serializer writes, beside it, each
//!   held to its serializer by a test.
//!
//! [`roots`] names what is exported, and every type each root names goes
//! with it, into `TS_RS_EXPORT_DIR`, which `.cargo/config.toml` points at
//! the panel. One list rather than `#[ts(export)]` on each root, so the
//! test that finds two types written to one file walks what the export
//! writes, and no other list.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ts_rs::{Config, ExportError, TypeVisitor, TS};

use crate::browse::Folders;
use crate::code::{self, CoreCode};
use crate::contents::{self, Contents};
use crate::draft::Judged;
use crate::finder::SkillsFound;
use crate::known::Switchable;
use crate::marketplace;
use crate::mcp_registry::ServerListing;
use crate::reading::{Shown, Validation};
use crate::update::Updates;
use crate::view::{Listing, Snapshot};

/// Every type core hands a front end whole, each with every type it names:
/// the snapshot, an artifact's files and a file's contents, what an update
/// of every project would do, the workspaces it may switch to, and the
/// folders inside one, which the panel queries; what `list`,
/// `show` and `validate` print, which a script reads; what a search of the
/// skill directories and of an MCP registry print and what `gather
/// marketplace --list --json` prints, which the panel reads back from a
/// run of one; and every code core gives, which a front end compares a
/// code with.
pub fn roots(visitor: &mut impl TypeVisitor) {
    visitor.visit::<Snapshot>();
    visitor.visit::<Updates>();
    visitor.visit::<Listing>();
    visitor.visit::<SkillsFound<'static>>();
    visitor.visit::<ServerListing<'static>>();
    visitor.visit::<Shown>();
    visitor.visit::<Validation>();
    visitor.visit::<contents::Files>();
    visitor.visit::<Contents>();
    visitor.visit::<Judged>();
    visitor.visit::<Switchable>();
    visitor.visit::<Folders>();
    visitor.visit::<marketplace::Listing>();
    visitor.visit::<CoreCode>();
}

/// `words` as the union TypeScript reads one of them as: `"skill" |
/// "rule"`.
pub fn union<'a>(words: impl IntoIterator<Item = &'a str>) -> String {
    words
        .into_iter()
        .map(|word| format!("{word:?}"))
        .collect::<Vec<_>>()
        .join(" | ")
}

/// `members` as a union laid out one to a line, each its own line in a diff:
/// for a union too long to read on one.
pub fn lines(members: impl IntoIterator<Item = String>) -> String {
    members
        .into_iter()
        .map(|member| format!("\n  | {member}"))
        .collect()
}

/// `text`, a line at a time as a doc comment holds it, as the comment ts-rs
/// writes a derived type's documentation as: for a type whose TypeScript is
/// written by hand.
pub fn docs(text: &[&str]) -> String {
    let mut docs = String::from("/**\n");
    for line in text {
        docs.push_str(" *");
        docs.push_str(line);
        docs.push('\n');
    }
    docs.push_str(" */\n");
    docs
}

/// Every code core gives ([`code::ALL`]), as the union of them: what a
/// front end compares the code of what went wrong with, so that a code the
/// panel names is one core gives.
///
/// Not the type of a `Said`'s `code`, which stays a `string`: what a front
/// end hands on holds its own codes beside core's, the CLI's among them.
impl TS for CoreCode {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn docs() -> Option<String> {
        Some(docs(&[
            " Every code core gives, for what went wrong: unique across core, and",
            " never renamed once given.",
        ]))
    }

    fn name(_: &Config) -> String {
        "CoreCode".to_owned()
    }

    fn decl(config: &Config) -> String {
        format!("type CoreCode ={};", Self::inline(config))
    }

    fn decl_concrete(config: &Config) -> String {
        Self::decl(config)
    }

    fn inline(_: &Config) -> String {
        lines(
            code::ALL
                .iter()
                .flat_map(|(_, codes)| codes.iter())
                .map(|code| format!("{code:?}")),
        )
    }

    fn output_path() -> Option<PathBuf> {
        Some("CoreCode.ts".into())
    }
}

/// What an export test hands [`roots`]: it writes each type it visits, with
/// every type that one names, where and as the environment says
/// (`TS_RS_EXPORT_DIR`, `TS_RS_LARGE_INT`).
pub struct Export {
    config: Config,
    failed: Option<ExportError>,
}

impl Export {
    /// An export as `.cargo/config.toml` sets it up.
    pub fn from_env() -> Self {
        Export {
            config: Config::from_env(),
            failed: None,
        }
    }

    /// What went wrong, if anything did: the first failure, after which it
    /// writes nothing more.
    pub fn finish(self) -> Result<(), ExportError> {
        self.failed.map_or(Ok(()), Err)
    }
}

impl TypeVisitor for Export {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        if self.failed.is_none() {
            self.failed = T::export_all(&self.config).err();
        }
    }
}

/// The file each type it visits is written to, with every type it names:
/// for a test to find two types written to one, which the export would
/// either merge into one file, declaring a name twice when the two share
/// it, or write one over the other, a crate's tests after another's.
#[derive(Default)]
pub struct Files {
    /// Each file, and the Rust types written to it, by their paths without
    /// their generics: a generic type is one file whatever it is given.
    written: BTreeMap<PathBuf, Vec<String>>,
}

impl Files {
    /// Each file more than one type is written to, with those types.
    pub fn shared(&self) -> Vec<String> {
        self.written
            .iter()
            .filter(|(_, types)| types.len() > 1)
            .map(|(file, types)| format!("{}: {}", file.display(), types.join(", ")))
            .collect()
    }
}

/// Following what each type names, as the export does.
impl TypeVisitor for Files {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        let Some(file) = T::output_path() else {
            return;
        };
        let name = std::any::type_name::<T>();
        let name = name.split_once('<').map_or(name, |(name, _)| name);
        let types = self.written.entry(file).or_default();
        if types.iter().any(|seen| seen == name) {
            return;
        }
        types.push(name.to_owned());
        T::visit_dependencies(self);
    }
}

#[cfg(test)]
#[path = "typescript_test.rs"]
mod tests;
