//! The API the panel's backend serves, described as an OpenAPI 3.1
//! document (#218, D-24): what the `schema` feature adds beside the
//! schemas, and nothing else builds.
//!
//! Put together here, where both tables and every type are known: the
//! server writes what is HTTP of each route of its own table and of the
//! backend's ([`mindweaver_server::schema::paths`]), and asks this module
//! for the schema of each type a route names ([`Describe`]), each found by
//! the name the route gives it among the roots core, the CLI and the
//! server list, as a file of `docs/api/schemas` is. Walking the very
//! tables that are served, rather than a description beside the handlers,
//! so the two cannot disagree; and written into `docs/api/openapi.json`
//! by a test (`make api/openapi`), committed, and served as those bytes
//! (`PanelBackend::document`), so a plain build has it without compiling
//! schemars.
//!
//! Every type a route names is a definition of `components.schemas`, the
//! way the route uses it ([`Way`]), and every place the server carries the
//! backend's JSON is said of the CLI's types there, by the server's word
//! for it ([`Carried`]). A type both sent and answered whose two ways
//! differ, as a task does, sent closed and with what serde reads as none
//! left out of `required`, answered open with every field there, is two
//! definitions, the one sent named with `Sent` after it ([`SENT`]); one
//! described alike both ways, a word, is one.

use std::collections::{BTreeMap, BTreeSet};

use mindweaver_core::code::CoreCode;
use mindweaver_core::known::Switchable;
use mindweaver_core::schema::{self as core, given_settings, taken_settings, Visitor};
use mindweaver_server::schema::{self as server, Called, Carried, Describe, Way};
use mindweaver_server::{Backend, Cancel, Grounds, Method, Sent, API};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{json, Map, Value};

use crate::backend::{Confirm, Outcome, PanelBackend, Switch};
use crate::error::CliCode;
use crate::progress::Step;
use crate::task::{Holder, Task};

/// What the document is in, as the versions of OpenAPI go: 3.1, whose
/// schemas are JSON Schema 2020-12, as the API's are.
pub const OPENAPI: &str = "3.1.1";

/// Where every definition is, in the document, as OpenAPI keeps them.
const COMPONENTS: &str = "/components/schemas";

/// Where a definition of what the API is sent is put while the document is
/// written, apart from those it answers with, until each is named: so a
/// reference to one is told from a reference to a type of the same name
/// answered, which schemars would otherwise write alike.
const PENDING: &str = "/components/sent";

/// What follows the name of a type sent, when it is answered too and
/// described otherwise.
pub const SENT: &str = "Sent";

/// The API described, as the document `docs/api/openapi.json` holds it.
pub fn document() -> Value {
    let mut described = Described::default();
    let paths = server::paths(&PanelBackend::routes(), &mut described);
    let (schemas, names) = described.components();
    let version = API.rsplit_once("/v").map_or(API, |(_, version)| version);
    let about = format!(
        "The HTTP API `mindweaver ui` serves, for scripts and tools on this machine as much as \
         for the web panel, its first client. A read is a GET naming what it reads in its \
         parameters, and a read that takes a structure a POST that changes nothing; every \
         change is a task, the command lines it is, previewed and then run, so the API does \
         nothing the command line cannot. A script that did not start the panel finds where \
         the API is and its token in the file `mindweaver ui --token-file` writes. A route, a \
         parameter or a field once served is a contract: dropping or renaming one, or \
         changing what it holds, moves the API to `/api/v2`, and adding one does not, so an \
         answer's schema leaves room for a field it does not name. Written from the Rust \
         that serves it, never by hand.\n\n{}",
        server::ABOUT
    );
    let document = json!({
        "openapi": OPENAPI,
        "info": {
            "title": "Mindweaver",
            "version": version,
            "description": about,
            "license": {"name": "GPL-3.0-only", "identifier": "GPL-3.0-only"},
        },
        "jsonSchemaDialect": "https://json-schema.org/draft/2020-12/schema",
        "servers": [{
            "url": API,
            "description": "Where `mindweaver ui` serves it, beside this document: \
                            `http://127.0.0.1:<port>/api/v1`, at the port its link names.",
        }],
        "security": server::security(),
        "paths": paths,
        "components": {
            "schemas": schemas,
            "securitySchemes": server::schemes(),
        },
    });
    named(&document, &names)
}

/// One root, by the way it goes and how to put its definition in a
/// generator, which answers where it is.
struct Root {
    way: Way,
    define: fn(&mut SchemaGenerator) -> Schema,
}

/// Every root of core's, the CLI's and the server's, by its name: what a
/// route names a type by, as a file of `docs/api/schemas` is named.
#[derive(Default)]
struct Roots(BTreeMap<String, Root>);

impl Roots {
    fn add<T: JsonSchema + ?Sized>(&mut self, way: Way) {
        let root = Root {
            way,
            define: SchemaGenerator::subschema_for::<T>,
        };
        self.0.entry(T::schema_name().into_owned()).or_insert(root);
    }
}

impl Visitor for Roots {
    fn given<T: JsonSchema + ?Sized>(&mut self) {
        self.add::<T>(Way::Answered);
    }

    fn taken<T: JsonSchema + ?Sized>(&mut self) {
        self.add::<T>(Way::Sent);
    }
}

/// What the document's definitions are gathered in as the server asks for
/// them: a generator for each way, as `docs/api/schemas` describes each
/// root, whose definitions every route shares.
struct Described {
    answered: SchemaGenerator,
    sent: SchemaGenerator,
    roots: Roots,
}

impl Default for Described {
    fn default() -> Self {
        let mut answered = given_settings();
        answered.definitions_path = COMPONENTS.into();
        let mut sent = taken_settings();
        sent.definitions_path = PENDING.into();
        let mut roots = Roots::default();
        core::roots(&mut roots);
        crate::schema::roots(&mut roots);
        Described {
            answered: answered.into_generator(),
            sent: sent.into_generator(),
            roots,
        }
    }
}

impl Described {
    fn generator(&mut self, way: Way) -> &mut SchemaGenerator {
        match way {
            Way::Sent => &mut self.sent,
            Way::Answered => &mut self.answered,
        }
    }

    /// Every definition the routes named, as `components.schemas` holds
    /// them, and what each sent is named there.
    ///
    /// A definition sent that is described as the one answered of its
    /// name is that one, once each it names is too; any other of a name
    /// answered is named apart, with [`SENT`] after it. Found by narrowing
    /// the names taken alike until none is dropped, since a definition
    /// sent naming one named apart names another than the one answered.
    fn components(mut self) -> (Value, BTreeMap<String, String>) {
        let mut answered = self.answered.take_definitions(true);
        let sent = self.sent.take_definitions(true);
        let mut alike: BTreeSet<String> = sent
            .keys()
            .filter(|name| answered.contains_key(*name))
            .cloned()
            .collect();
        let names = |alike: &BTreeSet<String>| -> BTreeMap<String, String> {
            sent.keys()
                .map(|name| {
                    let apart = answered.contains_key(name) && !alike.contains(name);
                    let named = if apart {
                        format!("{name}{SENT}")
                    } else {
                        name.clone()
                    };
                    (name.clone(), named)
                })
                .collect()
        };
        loop {
            let named = names(&alike);
            let before = alike.len();
            alike.retain(|name| self::named(&sent[name], &named) == answered[name]);
            if alike.len() == before {
                break;
            }
        }
        let names = names(&alike);
        for (name, schema) in &sent {
            if alike.contains(name) {
                continue;
            }
            let named = &names[name];
            assert!(
                !answered.contains_key(named),
                "{named} would name two definitions, the one answered and {name} as it is sent"
            );
            answered.insert(named.clone(), self::named(schema, &names));
        }
        (Value::Object(answered), names)
    }
}

impl Describe for Described {
    /// As the root of that name, which goes the way the route uses it: a
    /// route names no type that is not one, nor sends one only answered.
    fn named(&mut self, name: &str, way: Way) -> Value {
        let root = self
            .roots
            .0
            .get(name)
            .unwrap_or_else(|| panic!("a route names {name}, which is no root"));
        assert_eq!(root.way, way, "a route names {name} {way:?}");
        let define = root.define;
        define(self.generator(way)).to_value()
    }

    /// A task and a switch, as `POST /tasks/preview`, `/tasks` and
    /// `/switch` read them into the backend's types, and the run a cancel
    /// names, as `POST /tasks/cancel` reads it into the server's own.
    fn sent(&mut self, sent: Sent) -> Value {
        match sent {
            Sent::Task => self.sent.subschema_for::<Task>(),
            Sent::Cancel => self.sent.subschema_for::<Cancel>(),
            Sent::Switch => self.sent.subschema_for::<Switch>(),
        }
        .to_value()
    }

    /// What the CLI's backend hands over at each place: an item of what to
    /// confirm, a run's outcome, the holder a watched path is of, as a
    /// query names it, each step a run under way reports, and the row of
    /// the workspace switched to, or none when the list it is read from no
    /// longer holds it.
    fn carried(&mut self, carried: Carried) -> Value {
        match carried {
            Carried::Confirm => self.answered.subschema_for::<Confirm>(),
            Carried::Outcome => self.answered.subschema_for::<Outcome>(),
            Carried::Scope => self.answered.subschema_for::<Holder>(),
            Carried::Progress => self.answered.subschema_for::<Step>(),
            Carried::Switched => self.answered.subschema_for::<Option<Switchable>>(),
        }
        .to_value()
    }

    fn own<T: JsonSchema + ?Sized>(&mut self) -> Value {
        self.answered.subschema_for::<T>().to_value()
    }

    /// Core's codes and the CLI's own, never the server's. As absent only
    /// for a read whose parameters name what it reads (`Absence`): they
    /// are part of the address of what it reads, so one naming nothing
    /// makes the address name nothing. Whatever a body names, and anything
    /// else, is unfit.
    fn refused(&mut self, called: Called<'_>, grounds: Grounds) -> Option<Value> {
        let may = match grounds {
            Grounds::Absent => matches!(
                called,
                Called::Read { method: Method::Get, parameters, .. } if !parameters.is_empty()
            ),
            Grounds::Unfit => true,
        };
        may.then(|| {
            json!({"anyOf": [
                self.answered.subschema_for::<CoreCode>().to_value(),
                self.answered.subschema_for::<CliCode>().to_value(),
            ]})
        })
    }
}

/// `value` with every reference to a definition sent pointing at the name
/// it is given in `components.schemas`.
fn named(value: &Value, names: &BTreeMap<String, String>) -> Value {
    match value {
        Value::Object(object) => Value::Object(
            object
                .iter()
                .map(|(key, value)| {
                    let pending = value
                        .as_str()
                        .filter(|_| key == "$ref")
                        .and_then(|reference| reference.strip_prefix(&format!("#{PENDING}/")));
                    let value = match pending {
                        Some(name) => {
                            let named = names
                                .get(name)
                                .unwrap_or_else(|| panic!("no definition sent of {name}"));
                            Value::String(format!("#{COMPONENTS}/{named}"))
                        }
                        None => named(value, names),
                    };
                    (key.clone(), value)
                })
                .collect::<Map<String, Value>>(),
        ),
        Value::Array(list) => Value::Array(list.iter().map(|value| named(value, names)).collect()),
        _ => value.clone(),
    }
}

#[cfg(test)]
#[path = "openapi_test.rs"]
mod tests;
