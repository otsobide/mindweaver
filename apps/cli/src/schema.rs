//! What the CLI's backend is sent and answers with, as JSON Schema (#217):
//! the `schema` feature's, as core's types are core's
//! (`mindweaver_core::schema`).
//!
//! Derived, every type serde reads or writes, beside its TypeScript: a
//! query, a task, a switch and the bodies of the reads that take one, which
//! the backend is sent, are described as serde reads them; what a scan, a
//! plan, a preview and a run are answered with, and the envelope a reading
//! command's `--json` prints, as serde writes them. What they name of
//! core's is described as core describes it. Only `Code`, which serde
//! never sees, is written by hand.
//!
//! Beside them, what is part of the API without being the backend's: the
//! token file `ui --token-file` writes, which a script reads before it
//! sends anything, and the server's own answers, a preview, a run, an event
//! and a refusal (`mindweaver_server::schema::roots`), which the server
//! names and the CLI lists, since it is the API the CLI serves.

use std::borrow::Cow;

use mindweaver_core::code::CoreCode;
use mindweaver_core::schema::{description, schema, Visitor};
use mindweaver_server::Condition;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{json, Value};

use crate::backend::{Confirm, Outcome, Planned, Query, Scanned, Switch};
use crate::commands::ui::Handed;
use crate::error::{CliCode, Code};
use crate::progress::Step;
use crate::render::Envelope;
use crate::routes::{Drafted, Selected};
use crate::task::Task;
use crate::CODES;

/// Every type the CLI's backend is sent or answers with whole, or that is
/// part of the API it serves, each with every type it names: a query, a
/// task, a switch and what a plan and a draft to judge are sent as, each
/// taken; what a scan, a plan, a preview and a run are answered with, the
/// last two what to confirm and what it produced, each step a run under
/// way reports, the envelope a reading command's `--json` prints, the
/// token file a script reads first, every code core, the CLI and the
/// server give, each given, and the server's own, a cancel taken and the
/// rest given.
pub fn roots(visitor: &mut impl Visitor) {
    visitor.taken::<Query>();
    visitor.taken::<Task>();
    visitor.taken::<Switch>();
    visitor.taken::<Selected>();
    visitor.taken::<Drafted>();
    visitor.given::<Scanned>();
    visitor.given::<Planned>();
    visitor.given::<Confirm>();
    visitor.given::<Outcome>();
    visitor.given::<Step>();
    visitor.given::<Envelope<'static, Value>>();
    visitor.given::<Handed<'static>>();
    visitor.given::<Code>();
    mindweaver_server::schema::roots(&mut Server(visitor));
}

/// The server's own roots, handed on to a visitor of core's as roots
/// given: the server names them through a visitor of its own, since it
/// depends on no mindweaver crate.
struct Server<'a, V>(&'a mut V);

impl<V: Visitor> mindweaver_server::schema::Visitor for Server<'_, V> {
    fn given<T: JsonSchema + ?Sized>(&mut self) {
        self.0.given::<T>();
    }

    fn taken<T: JsonSchema + ?Sized>(&mut self) {
        self.0.taken::<T>();
    }
}

/// Every code a front end is handed for what went wrong, as a string that
/// is one of them: core's ([`CoreCode`]) or the CLI's own ([`CliCode`]),
/// which it names as their own definitions, or the server's
/// ([`Condition::CODES`]), as the TypeScript's union says it.
impl JsonSchema for Code {
    fn schema_name() -> Cow<'static, str> {
        "Code".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::Code").into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        schema(json!({
            "description": description(&[
                " Every code a front end is handed for what went wrong: core's, the CLI's",
                " own and the local server's beside them. Unique across the three, and",
                " never renamed once given.",
            ]),
            "anyOf": [
                generator.subschema_for::<CoreCode>(),
                generator.subschema_for::<CliCode>(),
                {"type": "string", "enum": Condition::CODES},
            ],
        }))
    }
}

/// Every code the CLI gives of its own ([`CODES`]), as a string that is
/// one of them, as core's are its [`CoreCode`]: a definition of its own,
/// so a description of the API names what the CLI's backend refuses with
/// as core's and these, never the server's.
impl JsonSchema for CliCode {
    fn schema_name() -> Cow<'static, str> {
        "CliCode".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::CliCode").into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let codes: Vec<&str> = CODES
            .iter()
            .flat_map(|(_, codes)| codes.iter().copied())
            .collect();
        schema(json!({
            "description": description(&[
                " Every code the CLI gives of its own, for what went wrong: unique beside",
                " core's and the local server's, and never renamed once given.",
            ]),
            "type": "string",
            "enum": codes,
        }))
    }
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod tests;
