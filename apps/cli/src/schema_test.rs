//! Unit tests for `schema.rs`: the export of the API's schemas the CLI
//! gives, and what they say held to what serde writes, the answers the
//! panel's tests render among it.

use std::fs;
use std::path::Path;

use mindweaver_core::known::Switchable;
use mindweaver_core::schema::{self as core, Schemas, Together};
use mindweaver_server::Backend;

use super::*;
use crate::backend::PanelBackend;
use crate::commands::ui::handed;

/// Where the API's schemas are written, as core's export writes them:
/// `docs/api/schemas` in the repository, whichever folder cargo was
/// started in.
const FOLDER: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/api/schemas");

/// The export of the CLI's schemas, which `make api/schemas` runs with the
/// feature beside core's: every root the CLI lists, the server's own
/// among them, each into a file of its own.
#[test]
fn export_schemas() {
    let mut schemas = Schemas::default();
    roots(&mut schemas);

    schemas.write(Path::new(FOLDER)).unwrap();
}

/// Every root of core's, the CLI's and the server's, as one description
/// of the API names them.
fn every_root() -> Schemas {
    let mut schemas = Schemas::default();
    core::roots(&mut schemas);
    roots(&mut schemas);
    schemas
}

/// Across the crates, since all three write into one folder: a root of the
/// CLI's and one of core's of one name would be written one over the other.
#[test]
fn no_two_roots_of_any_crate_are_written_to_one_file() {
    assert_eq!(every_root().shared(), Vec::<String>::new());
}

/// Every type every root names at once, as a description of the whole API
/// holds them, where schemars would give a second type of a name taken a
/// number of its own without a word.
#[test]
fn no_two_types_of_any_crate_go_by_one_name() {
    let mut together = Together::default();
    core::roots(&mut together);
    roots(&mut together);

    assert_eq!(together.renamed(), Vec::<String>::new());
}

/// What a route is sent and answers with is named by its type's name,
/// `[]` after it for a list of them: each a root, so a description of the
/// API finds the schema of every route's body and answer by it.
#[test]
fn every_type_a_route_is_sent_or_answers_with_is_a_root() {
    let schemas = every_root();
    for route in PanelBackend::routes() {
        for named in route.body.into_iter().chain([route.answers]) {
            let name = named.strip_suffix("[]").unwrap_or(named);
            assert!(
                schemas.get(name).is_some(),
                "{} {}: {named} is no root",
                route.method,
                route.path
            );
        }
    }
}

/// Whether `value` is one `schema` describes, and what keeps it from being
/// one when it is not, as a JSON Schema 2020-12 validator says it, with no
/// loader at all: every schema holds what it names in its own `$defs`.
fn holds(schema: &Schema, value: &Value) -> Result<(), String> {
    let mut compiler = boon::Compiler::new();
    compiler.use_loader(Box::new(boon::SchemeUrlLoader::new()));
    compiler
        .add_resource("file:///schema.json", schema.as_value().clone())
        .unwrap();
    let mut compiled = boon::Schemas::new();
    let index = compiler
        .compile("file:///schema.json", &mut compiled)
        .unwrap_or_else(|error| panic!("{error:#}"));
    compiled
        .validate(value, index)
        .map_err(|error| format!("{error:#}"))
}

/// `T`'s schema as its file holds it, the way it is given.
fn given<T: JsonSchema + ?Sized>() -> Schema {
    core::given_settings()
        .into_generator()
        .into_root_schema_for::<T>()
}

/// Every root given, of core's, the CLI's and the server's: what the API
/// answers with, each as its file holds it.
#[derive(Default)]
struct Given(Schemas);

impl Visitor for Given {
    fn given<T: JsonSchema + ?Sized>(&mut self) {
        self.0.given::<T>();
    }

    fn taken<T: JsonSchema + ?Sized>(&mut self) {}
}

/// Where `schema`, at `at`, refuses a field it does not name, as schemars
/// writes that for a type serde refuses one in.
fn closings(schema: &Value, at: &str) -> Vec<String> {
    match schema {
        Value::Object(object) => object
            .iter()
            .flat_map(|(key, value)| {
                let below = format!("{at}/{key}");
                let closes = ["additionalProperties", "unevaluatedProperties"]
                    .contains(&key.as_str())
                    && value == &Value::Bool(false);
                let mut found = closings(value, &below);
                if closes {
                    found.insert(0, below);
                }
                found
            })
            .collect(),
        Value::Array(list) => list
            .iter()
            .enumerate()
            .flat_map(|(index, value)| closings(value, &format!("{at}/{index}")))
            .collect(),
        _ => Vec::new(),
    }
}

/// An answer stays open to a field it does not name, since one added moves
/// no version: every root given, of every crate, a task in what a plan is
/// answered with among them, which serde holds closed where it is sent.
#[test]
fn no_answer_of_any_crate_refuses_a_field_it_does_not_name() {
    let mut given = Given::default();
    core::roots(&mut given);
    roots(&mut given);
    let closing: Vec<String> = given
        .0
        .names()
        .flat_map(|name| closings(given.0.get(name).unwrap().as_value(), name))
        .collect();

    assert_eq!(closing, Vec::<String>::new());
    let planned = given.0.get("Planned").unwrap();
    assert!(planned.pointer("/$defs/Task").is_some());
    let mut taken = Schemas::default();
    taken.taken::<Task>();
    assert_ne!(
        closings(taken.get("Task").unwrap().as_value(), "Task"),
        Vec::<String>::new()
    );
}

#[test]
fn every_code_a_front_end_is_handed_holds_to_its_schema_and_nothing_else_does() {
    let schema = given::<Code>();
    let every = mindweaver_core::code::ALL
        .iter()
        .chain(CODES)
        .flat_map(|(_, codes)| codes.iter())
        .chain(Condition::CODES);

    for code in every {
        let code = Value::from(*code);
        if let Err(error) = holds(&schema, &code) {
            panic!("{code}: {error}");
        }
    }
    assert!(holds(&schema, &Value::from("nope")).is_err());
}

/// The CLI's own codes, and neither core's nor the server's, which a
/// description of the API lists beside core's as what the CLI's backend
/// refuses with.
#[test]
fn every_code_the_cli_gives_of_its_own_holds_to_its_schema_and_no_other_does() {
    let schema = given::<CliCode>();
    let own = CODES.iter().flat_map(|(_, codes)| codes.iter());
    let others = mindweaver_core::code::ALL
        .iter()
        .flat_map(|(_, codes)| codes.iter())
        .chain(Condition::CODES);

    for code in own {
        let code = Value::from(*code);
        if let Err(error) = holds(&schema, &code) {
            panic!("{code}: {error}");
        }
    }
    for code in others {
        assert!(holds(&schema, &Value::from(*code)).is_err(), "{code}");
    }
}

/// What `--token-file` writes, the bytes a script reads, as its schema
/// says them; and a file without the token, which it never writes, not.
#[test]
fn the_token_file_holds_to_its_schema() {
    let schema = given::<Handed<'static>>();
    let written: Value =
        serde_json::from_slice(&handed("http://127.0.0.1:4000/api/v1", "abc")).unwrap();

    assert_eq!(schema.get("title"), Some(&Value::from("TokenFile")));
    holds(&schema, &written).unwrap();
    let mut tokenless = written;
    tokenless.as_object_mut().unwrap().remove("token");
    assert!(holds(&schema, &tokenless).is_err());
}

/// Each root of the panel's types has a schema of the name its file has,
/// so what the panel is handed is described for every other client too.
/// With the `ts` feature beside `schema`, as `make api/schemas` builds.
#[cfg(feature = "ts")]
#[test]
fn every_root_the_panel_s_types_export_has_a_schema_of_its_name() {
    use std::collections::BTreeSet;

    struct Exported(BTreeSet<String>);

    impl ts_rs::TypeVisitor for Exported {
        fn visit<T: ts_rs::TS + 'static + ?Sized>(&mut self) {
            let file = T::output_path().expect("a root is written to a file");
            let name = file.file_stem().unwrap().to_string_lossy().into_owned();
            self.0.insert(name);
        }
    }

    let mut exported = Exported(BTreeSet::new());
    mindweaver_core::typescript::roots(&mut exported);
    crate::typescript::roots(&mut exported);
    let schemas = every_root();
    let described: BTreeSet<String> = schemas.names().map(str::to_owned).collect();

    assert_eq!(
        exported.0.difference(&described).collect::<Vec<_>>(),
        Vec::<&String>::new()
    );
}

/// The fixtures the panel's views render, written from the real backend
/// (`tests/fixtures_test.rs`, `make web/fixtures`) and committed beside the
/// panel's tests.
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/tests/fixtures");

/// The keywords whose values are schemas, or lists or maps of schemas: what
/// [`closed`] follows, so a field's name is never taken for a keyword.
const BELOW: [&str; 2] = ["items", "not"];
const LISTS: [&str; 4] = ["oneOf", "anyOf", "allOf", "prefixItems"];
const MAPS: [&str; 2] = ["properties", "$defs"];

/// `schema` with every object it describes by its fields closed to any
/// other field, as serde writes no field its type does not name.
///
/// What the schemas say of an answer leaves it open, so that a field added
/// to one, which moves no version, does not make a client that checks
/// answers refuse it. Closed here, so a field serde writes that a schema
/// does not name fails rather than pass as one more allowed: a schema that
/// says less than serde writes is caught, as well as one that says other.
/// Only where the object is described by its fields alone, as every one
/// is here, since a closed object beside a `oneOf` would refuse what the
/// branch it matches names.
fn closed(schema: &Value) -> Value {
    let Value::Object(object) = schema else {
        return schema.clone();
    };
    let mut closed = serde_json::Map::new();
    for (key, value) in object {
        let value = match (key.as_str(), value) {
            (key, value) if BELOW.contains(&key) => self::closed(value),
            (key, Value::Array(list)) if LISTS.contains(&key) => {
                Value::Array(list.iter().map(self::closed).collect())
            }
            (key, Value::Object(map)) if MAPS.contains(&key) => Value::Object(
                map.iter()
                    .map(|(name, below)| (name.clone(), self::closed(below)))
                    .collect(),
            ),
            ("additionalProperties", Value::Object(_)) => self::closed(value),
            _ => value.clone(),
        };
        closed.insert(key.clone(), value);
    }
    let combined = LISTS.iter().any(|list| closed.contains_key(*list));
    if closed.contains_key("properties") && !combined {
        closed
            .entry("additionalProperties")
            .or_insert(Value::Bool(false));
    }
    Value::Object(closed)
}

/// What a fixture holds is held to the schemas of every root, as written
/// and closed.
struct Fixtures {
    schemas: Schemas,
    /// What each route answers with and is sent, by its path.
    routes: Vec<(String, &'static str, Option<&'static str>)>,
}

impl Fixtures {
    fn new() -> Self {
        Fixtures {
            schemas: every_root(),
            routes: PanelBackend::routes()
                .into_iter()
                .map(|route| (route.path.to_owned(), route.answers, route.body))
                .collect(),
        }
    }

    /// That `value`, in the fixture `at`, holds to the root `name` as its
    /// file says it, and closed, or to a list of them for `name[]`: as
    /// written too, since an object opened to a field it does not name could
    /// match two members of a `oneOf` where closed it matches one.
    fn is(&self, at: &str, name: &str, value: &Value) {
        if let Some(item) = name.strip_suffix("[]") {
            let items = value
                .as_array()
                .unwrap_or_else(|| panic!("{at}: {value} is no list of {item}"));
            for (index, value) in items.iter().enumerate() {
                self.is(&format!("{at}/{index}"), item, value);
            }
            return;
        }
        let schema = self
            .schemas
            .get(name)
            .unwrap_or_else(|| panic!("{at}: {name} is no root"));
        if let Err(error) = holds(schema, value) {
            panic!("{at}: what the fixture holds is no {name}: {error}");
        }
        let schema = Schema::try_from(closed(schema.as_value())).unwrap();
        if let Err(error) = holds(&schema, value) {
            panic!("{at}: what the fixture holds is no {name} closed: {error}");
        }
    }

    /// A read as the panel asks it, `{"query", "answer"}`: the query as the
    /// backend reads it, the answer as its route says, and for a route
    /// that takes a body, the query but its type as the body.
    fn asked(&self, at: &str, asked: &Value) {
        let query = &asked["query"];
        self.is(&format!("{at}/query"), "Query", query);
        let kind = query["type"].as_str().unwrap();
        let (_, answers, body) = self
            .routes
            .iter()
            .find(|(path, ..)| path.strip_prefix('/') == Some(kind))
            .unwrap_or_else(|| panic!("{at}: no route reads {kind}"));
        self.is(&format!("{at}/answer"), answers, &asked["answer"]);
        if let Some(body) = body {
            let mut sent = query.clone();
            sent.as_object_mut().unwrap().remove("type");
            self.is(&format!("{at}/query"), body, &sent);
        }
    }

    /// A task previewed and run, `{"task", "preview", "ran"}`: the task as
    /// the backend reads it, the server's preview and run, and in them what
    /// the backend handed over, what to confirm and what the run produced.
    fn ran(&self, at: &str, ran: &Value) {
        self.is(&format!("{at}/task"), "Task", &ran["task"]);
        self.is(&format!("{at}/preview"), "Preview", &ran["preview"]);
        self.is(
            &format!("{at}/preview/confirm"),
            "Confirm[]",
            &ran["preview"]["confirm"],
        );
        self.is(&format!("{at}/ran"), "Ran", &ran["ran"]);
        self.is(
            &format!("{at}/ran/outcome"),
            "Outcome",
            &ran["ran"]["outcome"],
        );
    }

    /// A refusal, its `{"code", "error"}` as every answer under `/api` that
    /// is no success holds it, beside what was sent, under `sent`.
    fn refused(&self, at: &str, refused: &Value, sent: &str, name: &str) {
        self.is(&format!("{at}/{sent}"), name, &refused[sent]);
        let body = json!({"code": refused["code"], "error": refused["error"]});
        self.is(at, "Refused", &body);
    }

    /// Each item of `list`, in the fixture `at`, held by `each`.
    fn each(&self, at: &str, list: &Value, each: impl Fn(&Self, &str, &Value)) {
        let items = list.as_array().unwrap_or_else(|| panic!("{at} is no list"));
        for (index, item) in items.iter().enumerate() {
            each(self, &format!("{at}/{index}"), item);
        }
    }

    /// The fixture `name`, by what each part of it is an answer to.
    fn hold(&self, name: &str, fixture: &Value) {
        match name {
            "workspace" | "candidates" | "unmade" | "assigning" | "updating" | "serving" => {
                self.is(name, "Snapshot", fixture);
            }
            "library" | "scan" | "plans" | "updates" | "drafts" | "servers" => {
                self.each(name, fixture, Self::asked);
            }
            "commands" | "applied" | "loads" | "updated" | "authored" | "served" => {
                self.each(name, fixture, Self::ran);
            }
            // A task previewed, and run where running it reaches no
            // repository: one previewed alone holds no run.
            "kept" => self.each(name, fixture, |fixtures, at, kept| {
                if kept["ran"].is_null() {
                    fixtures.is(&format!("{at}/task"), "Task", &kept["task"]);
                    fixtures.is(&format!("{at}/preview"), "Preview", &kept["preview"]);
                } else {
                    fixtures.ran(at, kept);
                }
            }),
            // What a run reports as it goes, each `progress` event's.
            "steps" => self.each(name, fixture, |fixtures, at, step| {
                fixtures.is(at, "Step", step);
            }),
            "folders" => {
                self.each(&format!("{name}/walked"), &fixture["walked"], Self::asked);
                let at = format!("{name}/refused");
                self.refused(&at, &fixture["refused"], "query", "Query");
            }
            "switches" => {
                self.asked(&format!("{name}/workspaces"), &fixture["workspaces"]);
                let switched = &fixture["switched"];
                let at = format!("{name}/switched");
                self.is(&format!("{at}/to"), "Switch", &switched["to"]);
                self.is(&format!("{at}/answer"), "Switchable", &switched["answer"]);
                self.is(&format!("{at}/snapshot"), "Snapshot", &switched["snapshot"]);
                self.asked(&format!("{at}/workspaces"), &switched["workspaces"]);
                let at = format!("{name}/refused");
                self.refused(&at, &fixture["refused"], "to", "Switch");
            }
            // What `mindweaver validate` prints, as text, which the editor's
            // verdict is held to word for word: no answer of the API's.
            "validated" => {}
            other => panic!(
                "{other}.json is a fixture this test does not know: say here what each \
                 part of it is an answer to"
            ),
        }
    }
}

/// Every fixture the panel's views render holds to the schema of what it is
/// an answer to, or was sent as: so the schemas are held to what the real
/// backend and server answer, not only to values a test made, and a
/// fixture this test does not know fails it.
#[test]
fn every_fixture_the_panel_renders_holds_to_the_schema_of_what_it_answers() {
    let fixtures = Fixtures::new();
    let mut names: Vec<String> = fs::read_dir(FIXTURES)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(names.len() > 10, "{names:?}");

    for file in names {
        let name = file
            .strip_suffix(".json")
            .unwrap_or_else(|| panic!("{file} is no JSON fixture"));
        let text = fs::read_to_string(Path::new(FIXTURES).join(&file)).unwrap();
        let fixture: Value = serde_json::from_str(&text).unwrap();
        fixtures.hold(name, &fixture);
    }
}

/// What closes the schemas, which a test that closed nothing would pass
/// unseen: a field the schema does not name holds open and is refused
/// closed, in a list and in a definition alike, and a map, whose keys are
/// any, stays open.
#[test]
fn a_closed_schema_refuses_a_field_it_does_not_name() {
    let schema = given::<Switchable>();
    let mut row = json!({
        "name": "kit", "path": "/home/you/kit", "default": false, "remembered": true,
        "in_use": false, "projects": 0, "stale_projects": 0, "stale": null,
        "launched": false, "reason": "explicit", "switchable": true,
    });
    holds(&schema, &row).unwrap();
    let closed_schema = Schema::try_from(closed(schema.as_value())).unwrap();
    holds(&closed_schema, &row).unwrap();

    row["extra"] = json!(1);
    holds(&schema, &row).unwrap();
    assert!(holds(&closed_schema, &row).is_err());
    let map = json!({"type": "object", "additionalProperties": {"type": "string"}});
    assert_eq!(closed(&map), map);
}
