//! Unit tests for `schema.rs`: what the server answers with of its own,
//! held to what serde writes of it.

use schemars::generate::SchemaSettings;
use schemars::Schema;
use serde::Serialize;
use serde_json::{json, Value};

use super::*;
use crate::Condition;

/// Each root's schema as a file of it would hold it, by its name.
#[derive(Default)]
struct Given(Vec<(String, Schema)>);

impl Visitor for Given {
    fn given<T: JsonSchema + ?Sized>(&mut self) {
        let schema = SchemaSettings::draft2020_12()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<T>();
        self.0.push((T::schema_name().into_owned(), schema));
    }

    fn taken<T: JsonSchema + ?Sized>(&mut self) {
        let schema = SchemaSettings::draft2020_12()
            .for_deserialize()
            .into_generator()
            .into_root_schema_for::<T>();
        self.0.push((T::schema_name().into_owned(), schema));
    }
}

/// The schema of the root named `name`.
fn schema(name: &str) -> Schema {
    let mut given = Given::default();
    roots(&mut given);
    given
        .0
        .into_iter()
        .find(|(named, _)| named == name)
        .unwrap_or_else(|| panic!("no root {name}"))
        .1
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

/// That what serde writes of `value` holds to the root named `name`.
fn holds_to(name: &str, value: &impl Serialize) {
    let json = serde_json::to_value(value).unwrap();
    if let Err(error) = holds(&schema(name), &json) {
        panic!("{json} is no {name}: {error}");
    }
}

/// That `value` does not hold to the root named `name`.
fn refused_by(name: &str, value: &Value) {
    assert!(
        holds(&schema(name), value).is_err(),
        "{value} holds to {name}"
    );
}

/// One event of each kind, matched with no wildcard so a kind added is
/// added here before it compiles.
fn every_event() -> Vec<Event> {
    fn _every(event: Event) {
        match event {
            Event::Changed { .. }
            | Event::CommandStarted { .. }
            | Event::Progress { .. }
            | Event::Cancelling { .. }
            | Event::Busy { .. }
            | Event::CommandFinished { .. }
            | Event::Switched { .. }
            | Event::Resync { .. }
            | Event::Watching
            | Event::Unwatched { .. } => {}
        }
    }
    let lines = vec!["mindweaver link /home/you/blog".to_owned()];
    vec![
        Event::Changed {
            scopes: vec![json!({"type": "workspace"}), json!("anything")],
        },
        Event::CommandStarted {
            run: 1,
            lines: lines.clone(),
            cancellable: true,
        },
        Event::Progress {
            run: 1,
            progress: json!({"stage": "connecting"}),
        },
        Event::Cancelling { run: 1 },
        Event::Busy {
            run: 1,
            lines: lines.clone(),
            cancellable: true,
            cancelling: false,
        },
        Event::CommandFinished { run: 1, lines },
        Event::Switched { switches: 2 },
        Event::Resync { switches: 0 },
        Event::Watching,
        Event::Unwatched {
            reason: "the system's limit on watches was reached".to_owned(),
        },
    ]
}

#[test]
fn the_server_s_roots_are_a_preview_a_run_a_cancel_an_event_and_a_refusal() {
    let mut given = Given::default();
    roots(&mut given);
    let names: Vec<&str> = given.0.iter().map(|(name, _)| name.as_str()).collect();

    assert_eq!(
        names,
        ["Preview", "Ran", "Cancel", "Cancelling", "Event", "Refused"]
    );
}

/// A cancel names a run by its number and nothing else, and is answered
/// with that run's number and lines.
#[test]
fn a_cancel_and_what_it_is_answered_with_hold_to_their_schemas() {
    holds_to(
        "Cancelling",
        &Cancelling {
            run: 3,
            lines: vec!["mindweaver gather git https://example.com/skills".to_owned()],
        },
    );
    holds(&schema("Cancel"), &json!({"run": 3})).unwrap();
    refused_by("Cancel", &json!({"run": -1}));
    refused_by("Cancel", &json!({"run": 3, "all": true}));
    refused_by("Cancel", &json!({}));
    refused_by("Cancelling", &json!({"run": 3}));
}

#[test]
fn every_event_serde_writes_holds_to_its_schema() {
    for event in every_event() {
        holds_to("Event", &event);
    }
}

/// An event serde never writes: a type that is none, one without what its
/// type carries, and a count that is no count.
#[test]
fn an_event_serde_never_writes_is_refused() {
    refused_by("Event", &json!({"type": "nope"}));
    refused_by("Event", &json!({"type": "changed"}));
    refused_by("Event", &json!({"type": "switched", "switches": -1}));
    refused_by("Event", &json!({"scopes": []}));
    refused_by("Event", &json!({"type": "progress", "progress": {}}));
    refused_by(
        "Event",
        &json!({"type": "command_started", "lines": [], "cancellable": false}),
    );
}

/// The backend's JSON, which the server carries unread, is any JSON in
/// the server's own schema: what fills it is the front end's to say.
#[test]
fn a_preview_and_a_run_hold_to_their_schemas_whatever_the_backend_hands_over() {
    holds_to(
        "Preview",
        &Preview {
            lines: vec!["mindweaver unlink gone".to_owned()],
            confirm: vec![json!({"line": 0, "caution": "unlinks"}), json!(1)],
            cancellable: false,
        },
    );
    holds_to(
        "Preview",
        &Preview {
            lines: Vec::new(),
            confirm: Vec::new(),
            cancellable: true,
        },
    );
    holds_to(
        "Ran",
        &Ran {
            lines: vec!["mindweaver unlink gone".to_owned()],
            outcome: json!({"ok": true, "runs": []}),
        },
    );
    refused_by("Preview", &json!({"lines": ["mindweaver unlink gone"]}));
    refused_by(
        "Preview",
        &json!({"lines": [], "confirm": [], "cancellable": "no"}),
    );
    refused_by(
        "Ran",
        &json!({"lines": "mindweaver unlink gone", "outcome": null}),
    );
}

#[test]
fn a_refusal_of_every_condition_holds_to_its_schema() {
    for condition in Condition::ALL {
        holds_to(
            "Refused",
            &Refused {
                code: condition.code(),
                error: format!("{condition:?}"),
            },
        );
    }
    refused_by("Refused", &json!({"error": "no code"}));
    refused_by("Refused", &json!({"code": 404, "error": "a number"}));
}

/// A front end's part of a description whose every schema says what it
/// was asked for, so a test reads back where each goes; its backend
/// refusing on `refusing` alone, whatever the route.
#[derive(Default)]
struct Asked {
    refusing: Vec<Grounds>,
    /// Every route the backend was asked about, by its path.
    called: Vec<String>,
}

impl Describe for Asked {
    fn named(&mut self, name: &str, way: Way) -> Value {
        json!({ "named": name, "way": format!("{way:?}") })
    }

    fn sent(&mut self, sent: Sent) -> Value {
        json!({ "sent": format!("{sent:?}") })
    }

    fn carried(&mut self, carried: Carried) -> Value {
        json!({ "carried": format!("{carried:?}") })
    }

    fn own<T: JsonSchema + ?Sized>(&mut self) -> Value {
        json!({ "own": T::schema_name() })
    }

    fn refused(&mut self, called: Called<'_>, grounds: Grounds) -> Option<Value> {
        let path = match called {
            Called::Read { path, .. } => path,
            Called::Own(own) => own.path(),
        };
        self.called.push(path.to_owned());
        self.refusing
            .contains(&grounds)
            .then(|| json!({ "backend": format!("{grounds:?}") }))
    }
}

/// A read of the backend's, by its method, its path and what it answers
/// with, taking `parameters` or, for a POST, a body of `Drafted`.
fn read(
    method: Method,
    path: &'static str,
    parameters: Vec<Parameter>,
    answers: &'static str,
) -> Route<()> {
    Route {
        method,
        path,
        about: "what it reads",
        parameters,
        body: (method == Method::Post).then_some("Drafted"),
        answers,
        query: |_| Ok(()),
    }
}

/// The table every description test here describes: a GET taking one
/// parameter of each shape, one answering a list, and a POST.
fn table() -> Vec<Route<()>> {
    vec![
        read(
            Method::Get,
            "/files",
            vec![
                Parameter::required("reference", Shape::Text, "what to read"),
                Parameter::optional("depth", Shape::Number, "how deep"),
                Parameter::optional("hidden", Shape::Flag, "whether hidden ones too"),
                Parameter::optional("root", Shape::List, "where to look"),
            ],
            "Files",
        ),
        read(Method::Get, "/projects", Vec::new(), "Project[]"),
        read(Method::Post, "/validate", Vec::new(), "Judged"),
    ]
}

/// The operation at `path` of what [`paths`] writes of [`table`] with
/// `describe`, by its one method.
fn operation_at(described: &Value, path: &str) -> Value {
    let item = described[path]
        .as_object()
        .unwrap_or_else(|| panic!("no path {path}: {described}"));
    assert_eq!(item.len(), 1, "{path}: {item:?}");
    item.values().next().unwrap().clone()
}

#[test]
fn a_read_is_described_by_its_method_its_parameters_its_body_and_its_answer() {
    let described = paths(&table(), &mut Asked::default());

    let files = &described["/files"]["get"];
    assert_eq!(files["operationId"], "get_files");
    assert_eq!(files["summary"], "what it reads");
    assert_eq!(
        files["parameters"],
        json!([
            {"name": "reference", "in": "query", "required": true,
             "description": "what to read", "schema": {"type": "string"}},
            {"name": "depth", "in": "query", "required": false,
             "description": "how deep", "schema": {"type": "integer", "minimum": 0}},
            {"name": "hidden", "in": "query", "required": false,
             "description": "whether hidden ones too", "schema": {"type": "boolean"}},
            {"name": "root", "in": "query", "required": false,
             "description": "where to look", "style": "form", "explode": true,
             "schema": {"type": "array", "items": {"type": "string"}}},
        ])
    );
    assert!(files.get("requestBody").is_none());
    assert_eq!(
        files["responses"]["200"]["content"]["application/json"]["schema"],
        json!({"named": "Files", "way": "Answered"})
    );
    let projects = operation_at(&described, "/projects");
    assert!(projects.get("parameters").is_none());
    assert_eq!(
        projects["responses"]["200"]["content"]["application/json"]["schema"],
        json!({"type": "array", "items": {"named": "Project", "way": "Answered"}})
    );
    let validate = &described["/validate"]["post"];
    assert_eq!(validate["operationId"], "post_validate");
    assert_eq!(
        validate["requestBody"],
        json!({"required": true, "content": {"application/json": {
            "schema": {"named": "Drafted", "way": "Sent"},
        }}})
    );
}

/// The code a refusal of `status` from the operation `operation` may
/// carry, as its schema says it beside the server's own `Refused`.
fn code(operation: &Value, status: &str) -> Value {
    let schema = &operation["responses"][status]["content"]["application/json"]["schema"];
    assert_eq!(schema["allOf"][0], json!({"own": "Refused"}), "{status}");
    schema["allOf"][1]["properties"]["code"].clone()
}

#[test]
fn every_status_a_read_may_be_refused_with_carries_the_codes_of_that_status() {
    let mut asked = Asked {
        refusing: Grounds::ALL.to_vec(),
        ..Asked::default()
    };
    let described = paths(&table(), &mut asked);
    let files = operation_at(&described, "/files");
    let validate = operation_at(&described, "/validate");
    let statuses = |operation: &Value| -> Vec<String> {
        operation["responses"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    };

    assert_eq!(
        statuses(&files),
        ["200", "400", "401", "403", "404", "405", "422", "500"]
    );
    assert_eq!(
        code(&files, "400"),
        json!({"type": "string", "enum": [
            "unknown_parameter", "repeated_parameter", "malformed_parameter",
            "missing_parameter", "invalid_parameters",
        ]})
    );
    assert_eq!(code(&files, "404"), json!({"backend": "Absent"}));
    assert_eq!(code(&files, "422"), json!({"backend": "Unfit"}));
    assert_eq!(
        code(&files, "500"),
        json!({"type": "string", "enum": ["backend_failed"]})
    );
    assert_eq!(
        statuses(&validate),
        ["200", "400", "401", "403", "404", "405", "413", "415", "422", "500"]
    );
    assert_eq!(
        code(&validate, "422"),
        json!({"anyOf": [
            {"type": "string", "enum": ["invalid_body"]},
            {"backend": "Unfit"},
        ]})
    );
    assert_eq!(
        files["responses"]["401"]["headers"]["WWW-Authenticate"]["schema"],
        json!({"type": "string", "const": "Bearer"})
    );
    assert!(files["responses"]["405"]["headers"]["Allow"].is_object());
}

/// A backend that never refuses on some grounds has no status of them
/// listed, but where the server's own conditions have it too.
#[test]
fn a_status_only_the_backend_gives_is_listed_only_when_it_may_give_it() {
    let mut asked = Asked {
        refusing: vec![Grounds::Unfit],
        ..Asked::default()
    };
    let described = paths(&table(), &mut asked);

    let files = operation_at(&described, "/files");
    assert!(files["responses"].get("404").is_none());
    assert_eq!(code(&files, "422"), json!({"backend": "Unfit"}));
}

/// The backend is asked about each route that hands it a request, its
/// reads, a preview, a run and a switch, and about no other: not a cancel,
/// which reaches the run under way and hands the backend nothing sent.
#[test]
fn only_a_route_that_reaches_the_backend_is_refused_on_its_grounds() {
    let mut asked = Asked {
        refusing: Grounds::ALL.to_vec(),
        ..Asked::default()
    };
    let described = paths(&table(), &mut asked);

    asked.called.dedup();
    assert_eq!(
        asked.called,
        [
            "/files",
            "/projects",
            "/validate",
            "/tasks/preview",
            "/tasks",
            "/switch"
        ]
    );
    for own in Own::ALL {
        let operation = operation_at(&described, own.path());
        let listed: Vec<&String> = operation["responses"].as_object().unwrap().keys().collect();
        let backend = asked.called.iter().any(|path| path == own.path());
        assert_eq!(listed.contains(&&"404".to_owned()), backend, "{own:?}");
    }
}

/// What the server answers with of its own, each place it carries the
/// backend's JSON filled by the word for it, and the events a stream.
#[test]
fn the_servers_own_answers_say_what_the_backend_fills_them_with() {
    let described = paths(&[] as &[Route<()>], &mut Asked::default());
    let answer = |path: &str| {
        operation_at(&described, path)["responses"]["200"]["content"]["application/json"]["schema"]
            .clone()
    };
    let ran = json!({"allOf": [
        {"own": "Ran"},
        {"properties": {"outcome": {"anyOf": [
            {"carried": "Outcome"},
            {
                "description": "Why the backend's outcome could not be answered, written in \
                                its place when it would not serialize.",
                "type": "object",
                "properties": {"error": {"type": "string"}},
                "required": ["error"],
            },
        ]}}},
    ]});

    assert_eq!(
        answer("/tasks/preview"),
        json!({"allOf": [
            {"own": "Preview"},
            {"properties": {"confirm": {"items": {"carried": "Confirm"}}}},
        ]})
    );
    assert_eq!(answer("/tasks"), ran);
    assert_eq!(
        answer("/transcript"),
        json!({"type": "array", "items": ran})
    );
    assert_eq!(answer("/switch"), json!({"carried": "Switched"}));
    assert_eq!(answer("/tasks/cancel"), json!({"own": "Cancelling"}));
    let events = &operation_at(&described, "/events")["responses"]["200"]["content"];
    assert_eq!(
        events["text/event-stream"]["x-itemSchema"],
        json!({"allOf": [
            {"own": "Event"},
            {"properties": {
                "scopes": {"items": {"carried": "Scope"}},
                "progress": {"carried": "Progress"},
            }},
        ]})
    );
    assert_eq!(events["text/event-stream"]["schema"]["type"], "string");
    assert_eq!(answer("/openapi.json")["type"], "object");
    for (path, body) in [
        ("/tasks/preview", "Task"),
        ("/tasks", "Task"),
        ("/tasks/cancel", "Cancel"),
        ("/switch", "Switch"),
    ] {
        assert_eq!(
            operation_at(&described, path)["requestBody"]["content"]["application/json"]["schema"],
            json!({ "sent": body }),
            "{path}"
        );
    }
}

/// Every status a condition of the server's or the grounds of a backend's
/// refusal is answered with is said in words of its own, not the words
/// for none.
#[test]
fn every_status_a_refusal_is_answered_with_is_said_in_words_of_its_own() {
    let fallback = said(0);
    let statuses = Condition::ALL
        .iter()
        .map(|condition| condition.status())
        .chain(Grounds::ALL.iter().map(|grounds| grounds.status()));

    for status in statuses {
        assert_ne!(said(status.as_u16()), fallback, "{status}");
    }
}

/// The two ways in, which every route takes: either holds.
#[test]
fn the_token_is_brought_as_a_bearer_token_or_as_the_launch_links_cookie() {
    let schemes = schemes();

    assert_eq!(schemes["bearer"]["type"], "http");
    assert_eq!(schemes["bearer"]["scheme"], "bearer");
    assert_eq!(schemes["cookie"]["type"], "apiKey");
    assert_eq!(schemes["cookie"]["in"], "cookie");
    assert_eq!(security(), json!([{"bearer": []}, {"cookie": []}]));
}
