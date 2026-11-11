//! What core hands a front end, and takes from one, as JSON Schema (#217):
//! what the `schema` feature adds, and nothing else builds.
//!
//! The local server's API is described by JSON Schema written from the Rust
//! types (D-24), as the panel's TypeScript is (`typescript`):
//! schemars derives most of it, beside each type's serde derive and from
//! the same attributes, so a derived type's schema says what serde writes
//! or reads. What serde writes by hand schemars cannot read, and it would
//! spell the Rust variants that a hand-written serializer exists not to, so
//! each such type is given its schema from what its serializer writes,
//! beside it, as its TypeScript is:
//!
//! - **A word**, a kind, an agent or a state: a string, one of the words,
//!   from the function its serializer calls ([`crate::described::words!`]),
//!   which writes its TypeScript too.
//! - **A path or a URL**, written as text whatever it holds: a string,
//!   which is what schemars says of a `PathBuf` and a `String` already, so
//!   a test holds each function a field is written through to the schema of
//!   the type it takes ([`crate::described::field_serializers`]).
//! - **Anything else**, an issue `validate` finds, a project's id, a
//!   template's origin, a skill's metadata, every code core gives: from
//!   what its serializer writes, beside it, each held to its serializer by
//!   a test.
//!
//! Each root is described the way the API uses it ([`Visitor`]): what it
//! is answered with as serde writes it, every field there and an `Option`
//! written as `null`, open to a field it does not name ([`given_settings`]);
//! what it is sent as serde reads it, an `Option` that may be left out left
//! out of `required`, closed where serde refuses a field it does not know
//! ([`taken_settings`]). [`roots`] names each, and
//! [`Schemas`] writes each into a file of its own, every type it names in
//! its `$defs`, in JSON Schema 2020-12, which OpenAPI 3.1 takes as it is.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use schemars::generate::SchemaSettings;
use schemars::transform::RecursiveTransform;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{json, Value};

use crate::browse::Folders;
use crate::code::{self, CoreCode};
use crate::contents::{self, Contents};
use crate::draft::Judged;
use crate::finder::SkillsFound;
use crate::known::Switchable;
use crate::mcp_registry::ServerListing;
use crate::reading::{Shown, Validation};
use crate::update::Updates;
use crate::view::{Library, Listing, Project, Snapshot};

/// What a list of roots is handed: each type the API takes or gives whole,
/// by the way it goes, for whatever is made of its schema.
///
/// Two ways, since serde's derive writes and reads one type differently: a
/// field that is an `Option` is always written, as `null` when it is none,
/// and may be left out of what is read. A schema of one way for both would
/// either call a field written every time optional, or refuse a request
/// that leaves out what serde reads as none.
pub trait Visitor {
    /// A type the API answers with, described as serde writes it.
    fn given<T: JsonSchema + ?Sized>(&mut self);
    /// A type the API is sent, described as serde reads it.
    fn taken<T: JsonSchema + ?Sized>(&mut self);
}

/// Every type core hands a front end whole, each with every type it names:
/// what the panel's TypeScript exports (`typescript::roots`), the
/// snapshot, an artifact's files and a file's contents, what an update of
/// every project would do, the workspaces it may switch to, the folders
/// inside one, what `list`, `show` and `validate` print, what a search of
/// the skill directories and of an MCP registry print, which a run of the
/// panel's answers with, and every code core gives; and beside them the
/// snapshot's projects, one project of it and its library, which the API
/// answers with alone. Each is given, as serde writes it: core is sent
/// nothing, a front end reading what it is sent into types of its own.
pub fn roots(visitor: &mut impl Visitor) {
    visitor.given::<Snapshot>();
    visitor.given::<Project>();
    visitor.given::<Library>();
    visitor.given::<Updates>();
    visitor.given::<Listing>();
    visitor.given::<SkillsFound<'static>>();
    visitor.given::<ServerListing<'static>>();
    visitor.given::<Shown>();
    visitor.given::<Validation>();
    visitor.given::<contents::Files>();
    visitor.given::<Contents>();
    visitor.given::<Judged>();
    visitor.given::<Switchable>();
    visitor.given::<Folders>();
    visitor.given::<CoreCode>();
}

/// How a root the API answers with is described: as serde writes it, and
/// open to a field its objects do not name.
///
/// schemars closes an object whose type refuses a field it does not know
/// (`deny_unknown_fields`) whichever way it describes it, and a type the
/// API is sent can be answered too: a task, a holder and a box are in what
/// a plan is answered with. Closed there, a field added to one, which moves
/// no version, would make a client that checks answers refuse it.
pub fn given_settings() -> SchemaSettings {
    SchemaSettings::draft2020_12()
        .for_serialize()
        .with_transform(RecursiveTransform(open))
}

/// How a root the API is sent is described: as serde reads it, closed
/// where serde refuses a field it does not know.
pub fn taken_settings() -> SchemaSettings {
    SchemaSettings::draft2020_12().for_deserialize()
}

/// `schema` no longer refusing a field it does not name, as schemars
/// writes that refusal for `deny_unknown_fields`: `additionalProperties`,
/// or `unevaluatedProperties` beside a flattened field, set to `false`. A
/// map's `additionalProperties`, the schema of its values, is kept.
fn open(schema: &mut Schema) {
    for keyword in ["additionalProperties", "unevaluatedProperties"] {
        if schema.get(keyword) == Some(&Value::Bool(false)) {
            schema.remove(keyword);
        }
    }
}

/// Every root a visit names, each with its schema whole, as its file holds
/// it, by the name its type goes by: what the export writes, and how a
/// schema is found by the name of its type, as a route names what it is
/// sent and what it answers.
#[derive(Debug, Default)]
pub struct Schemas {
    /// Each name, with every root visited under it: more than one is two
    /// types written to one file, which [`Schemas::shared`] says.
    named: BTreeMap<String, Vec<Schema>>,
}

impl Schemas {
    /// The schema of the root named `name`, as its file holds it.
    pub fn get(&self, name: &str) -> Option<&Schema> {
        self.named.get(name).and_then(|schemas| schemas.first())
    }

    /// Every root's name, in order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.named.keys().map(String::as_str)
    }

    /// Each name more than one root was visited under, which would be
    /// written to one file, one over the other.
    pub fn shared(&self) -> Vec<String> {
        self.named
            .iter()
            .filter(|(_, schemas)| schemas.len() > 1)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Write each root's schema into `folder`, as `<name>.json`: laid out
    /// two spaces a level, its keys in order, and ended as a text file is,
    /// so a change to one reads as a diff of the lines it changed.
    pub fn write(&self, folder: &Path) -> io::Result<()> {
        fs::create_dir_all(folder)?;
        for (name, schemas) in &self.named {
            for schema in schemas {
                let mut text = serde_json::to_string_pretty(schema)?;
                text.push('\n');
                fs::write(folder.join(format!("{name}.json")), text)?;
            }
        }
        Ok(())
    }

    fn add<T: JsonSchema + ?Sized>(&mut self, settings: SchemaSettings) {
        let schema = settings.into_generator().into_root_schema_for::<T>();
        self.named
            .entry(T::schema_name().into_owned())
            .or_default()
            .push(schema);
    }
}

/// Each root in JSON Schema 2020-12, its own schema at the top and every
/// type it names in `$defs`.
impl Visitor for Schemas {
    fn given<T: JsonSchema + ?Sized>(&mut self) {
        self.add::<T>(given_settings());
    }

    fn taken<T: JsonSchema + ?Sized>(&mut self) {
        self.add::<T>(taken_settings());
    }
}

/// Every root a visit names, as one description of the whole API holds
/// them: a generator for each way, whose definitions every root it visits
/// shares, as an OpenAPI document's components are shared by its routes.
///
/// What finds two types going by one name, which schemars tells apart
/// without a word, by a number it adds to the second's name: a file of one
/// root shows that only when both types are in it, and a description of
/// the whole API holds every type of every crate at once.
#[derive(Debug)]
pub struct Together {
    given: SchemaGenerator,
    taken: SchemaGenerator,
}

impl Default for Together {
    fn default() -> Self {
        Together {
            given: given_settings().into_generator(),
            taken: taken_settings().into_generator(),
        }
    }
}

impl Together {
    /// Each name schemars made up for a type whose own was taken: one that
    /// is another type's name with a number after it.
    pub fn renamed(&self) -> Vec<String> {
        [&self.given, &self.taken]
            .into_iter()
            .flat_map(|generator| {
                let defined = generator.definitions();
                defined
                    .keys()
                    .filter(|name| {
                        let stem = name.trim_end_matches(|c: char| c.is_ascii_digit());
                        stem.len() < name.len() && defined.contains_key(stem)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

/// Each root a definition of its own, beside every type it names.
impl Visitor for Together {
    fn given<T: JsonSchema + ?Sized>(&mut self) {
        self.given.subschema_for::<T>();
    }

    fn taken<T: JsonSchema + ?Sized>(&mut self) {
        self.taken.subschema_for::<T>();
    }
}

/// What a type's documentation says, as schemars writes a derived type's
/// `description`: each line without the space after its `///`, the lines
/// joined as written. For a type whose schema is written by hand, so its
/// description reads as a derived one's does.
pub fn description(lines: &[&str]) -> String {
    lines
        .iter()
        .map(|line| line.strip_prefix(' ').unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// The schema of a type serde writes as one of `words`: a string, one of
/// them, with `docs` as its description ([`crate::described::words!`]).
pub fn words<'a>(docs: &[&str], words: impl IntoIterator<Item = &'a str>) -> Schema {
    let words: Vec<&str> = words.into_iter().collect();
    schema(json!({
        "description": description(docs),
        "type": "string",
        "enum": words,
    }))
}

/// `value` as the schema it is: an object, as every schema written here is.
pub fn schema(value: Value) -> Schema {
    Schema::try_from(value).expect("a schema written by hand is an object")
}

/// Every code core gives ([`code::ALL`]), as a string that is one of them:
/// what a description of the API names when it says which codes a refusal
/// can carry.
impl JsonSchema for CoreCode {
    fn schema_name() -> Cow<'static, str> {
        "CoreCode".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::CoreCode").into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let codes: Vec<&str> = code::ALL
            .iter()
            .flat_map(|(_, codes)| codes.iter().copied())
            .collect();
        schema(json!({
            "description": "Every code core gives, for what went wrong: unique across core, \
                            and never renamed once given.",
            "type": "string",
            "enum": codes,
        }))
    }
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod tests;
