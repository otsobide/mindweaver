//! What the server answers with of its own, as JSON Schema (#217), and its
//! part of the description of the API as an OpenAPI 3.1 document (#218):
//! what the `schema` feature adds, and nothing else builds.
//!
//! The API is described by JSON Schema written from the Rust types (D-24):
//! a front end's for what its backend is sent and answers, and the
//! server's own for what it answers whatever the backend, a preview, a run,
//! the events and a refusal. Derived beside each type's serde derive, as a
//! front end's are, so what a schema says is what serde writes. What the
//! backend hands over as JSON the server carries unread, so each place for
//! it, an item of a preview's `confirm`, a run's `outcome`, a change's
//! scope, is any JSON here: which of the backend's types fills it is the
//! front end's to say, as a description of its routes names its own types.
//!
//! The front end lists these beside its own roots ([`roots`]), since it is
//! its API they are part of; the server depends on no front end's crate, so
//! it names them through a [`Visitor`] of its own.
//!
//! The document is put together by the front end, which knows every type a
//! route names, and the server writes its part of it ([`paths`]): what is
//! HTTP, and the same whatever the backend. Each route of both tables, the
//! backend's and its own, with its method, its parameters, its body, what
//! it answers and every way it may be refused, the token's two ways in and
//! the events as a stream; each type a route names asked of the front end
//! ([`Describe`]), by the name the table gives it or the word the server's
//! own table has for it ([`Sent`], [`Carried`]). So the server spells no
//! type of the front end's, and the front end no rule of HTTP's.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde_json::{json, Map, Value};

use crate::auth::COOKIE_PREFIX;
use crate::guard::PANEL_HEADER;
use crate::route::{Answer, Method, Own, Parameter, Reaches, Route, Sent, Shape};
use crate::{Cancel, Cancelling, Condition, Event, Grounds, Preview, Ran, Refused};

/// What [`roots`] is handed: each type the server answers with or is sent
/// of its own, for whatever is made of its schema.
pub trait Visitor {
    /// A type the server answers with, described as serde writes it.
    fn given<T: JsonSchema + ?Sized>(&mut self);
    /// A type the server is sent, described as serde reads it.
    fn taken<T: JsonSchema + ?Sized>(&mut self);
}

/// Every type the server answers with or is sent of its own, whatever the
/// backend: what a task's preview is ([`Preview`]), what a run did, which
/// the transcript lists too ([`Ran`]), the run a cancel names and what it
/// is answered with ([`Cancel`], [`Cancelling`]), each event the stream
/// tells ([`Event`]), and what every answer under `/api` that is no
/// success holds ([`Refused`]). Everything else it is sent is the
/// backend's.
pub fn roots(visitor: &mut impl Visitor) {
    visitor.given::<Preview>();
    visitor.given::<Ran>();
    visitor.taken::<Cancel>();
    visitor.given::<Cancelling>();
    visitor.given::<Event>();
    visitor.given::<Refused>();
}

/// A place in what the server answers with of its own that holds the
/// backend's JSON, carried unread: its schemas say any JSON there, and a
/// description says which of the backend's types it is by this word, as
/// the server's table names a body by [`Sent`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Carried {
    /// An item of a preview's `confirm`: what the panel has somebody
    /// confirm before the run ([`Preview::confirm`]).
    Confirm,
    /// What running a task produced, a run's `outcome`: the backend's
    /// [`Outcome`](crate::Backend::Outcome).
    Outcome,
    /// A scope of a `changed` event, the backend's word for what a watched
    /// path is of ([`Watched::scope`](crate::Watched::scope)).
    Scope,
    /// What a `progress` event, and the run a stream is told of on its
    /// own, carries: how a run is going, as the backend reported it
    /// ([`Underway::report`](crate::Underway::report)).
    Progress,
    /// What a switch answers with, whole
    /// ([`Backend::switch`](crate::Backend::switch)).
    Switched,
}

/// Which way a type a route names goes: in a body it is sent, described as
/// serde reads it, or in an answer, as serde writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    Sent,
    Answered,
}

/// A route whose request reaches the backend, for the front end to say on
/// which grounds its backend may refuse one ([`Describe::refused`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Called<'a> {
    /// One of its reads, as its table says it.
    Read {
        method: Method,
        path: &'static str,
        parameters: &'a [Parameter],
    },
    /// One of the server's own that hands the backend what it was sent: a
    /// preview, a run or a switch.
    Own(Own),
}

/// What a description of the API asks the front end whose backend it
/// describes: the schema of each type of the front end's a route names,
/// and of the server's own, as the description holds them.
///
/// A schema answered is what the description puts where the type goes, a
/// reference to its definition as a rule, so the front end gathers every
/// definition itself, as it holds the types: the server never names one
/// of the front end's, and holds none of its definitions.
pub trait Describe {
    /// The backend's type a route names `name` ([`Route::body`],
    /// [`Route::answers`], never with the `[]` of a list), the way it goes.
    fn named(&mut self, name: &str, way: Way) -> Value;
    /// What the body of one of the server's own routes holds, as it is
    /// sent: the backend's `Task` or `Switch`, or the server's own
    /// [`Cancel`], which the front end lists among its roots as it lists
    /// every one of the server's.
    fn sent(&mut self, sent: Sent) -> Value;
    /// What the backend hands over at a place the server carries it, as it
    /// is answered.
    fn carried(&mut self, carried: Carried) -> Value;
    /// One of the server's own types, as it answers with it.
    fn own<T: JsonSchema + ?Sized>(&mut self) -> Value;
    /// The codes the backend may refuse a request to `called` with on
    /// `grounds`, as the schema of a refusal's `code`; none when it never
    /// refuses one on them.
    fn refused(&mut self, called: Called<'_>, grounds: Grounds) -> Option<Value>;
}

/// What the server says of itself in a description of the API: what holds
/// of every route, before any is described.
pub const ABOUT: &str = "Served on 127.0.0.1 alone, at the port the system chose unless one \
was named, every route behind the launch token, as a Bearer token or as the cookie the launch \
link sets. Every answer that is no success is a `Refused`, `{\"code\", \"error\"}`, its code \
the word a script acts on and its error the words a person reads: a path under `/api` no \
route names, of another version or of none, is a 404 of `unknown_route`. Every answer under \
`/api` carries `Cache-Control: no-store`, and none grants another origin the right to read it.";

/// The two ways to bring the launch token, as OpenAPI's security schemes:
/// as a Bearer token, which a script sends, and as the cookie the launch
/// link sets, which the panel's page sends.
pub fn schemes() -> Value {
    json!({
        "bearer": {
            "type": "http",
            "scheme": "bearer",
            "description": "The launch token, as `Authorization: Bearer <token>`: how a script \
                            sends it, from the link printed when the server started, or from \
                            wherever the front end hands it on.",
        },
        "cookie": {
            "type": "apiKey",
            "in": "cookie",
            "name": format!("{COOKIE_PREFIX}<port>"),
            "description": format!(
                "The launch token, as the cookie the launch link sets, named `{COOKIE_PREFIX}` \
                 and the server's port: how the panel's page sends it. A change sent with it, \
                 which every POST is, needs `Sec-Fetch-Site: same-origin`, `{PANEL_HEADER}: 1` \
                 and `Content-Type: application/json` beside it, which only the panel's own \
                 page sends."
            ),
        },
    })
}

/// That every route takes either way in ([`schemes`]).
pub fn security() -> Value {
    json!([{"bearer": []}, {"cookie": []}])
}

/// Every route of the backend's table and of the server's own, as
/// OpenAPI's paths, each under its path from the API's version
/// ([`API`](crate::API), which a description names as its server), with
/// what `describe` says of each type a route names.
pub fn paths<Q>(routes: &[Route<Q>], describe: &mut impl Describe) -> Value {
    let mut paths = Map::new();
    for route in routes {
        let mut operation = operation(route.method, route.path, route.about);
        if !route.parameters.is_empty() {
            let parameters = route.parameters.iter().map(parameter).collect();
            operation.insert("parameters".into(), Value::Array(parameters));
        }
        if let Some(name) = route.body {
            let body = listed(name, Way::Sent, describe);
            operation.insert("requestBody".into(), sent(body));
        }
        let answer = listed(route.answers, Way::Answered, describe);
        let said = format!("What it answers with: the backend's `{}`.", route.answers);
        let called = Called::Read {
            method: route.method,
            path: route.path,
            parameters: &route.parameters,
        };
        let mut responses = refusals(&route.conditions(), Some(called), describe);
        responses.insert("200".into(), json_answer(&said, answer));
        operation.insert("responses".into(), Value::Object(responses));
        paths.insert(route.path.into(), item(route.method, operation));
    }
    for own in Own::ALL {
        let mut operation = operation(own.method(), own.path(), own.about());
        if let Some(body) = own.body() {
            let body = describe.sent(body);
            operation.insert("requestBody".into(), sent(body));
        }
        // A cancel reaches the run under way, and hands the backend nothing
        // it was sent, so nothing of it is the backend's to refuse.
        let called =
            matches!(own.reaches(), Reaches::Backend | Reaches::Writer).then_some(Called::Own(own));
        let mut responses = refusals(&own.conditions(), called, describe);
        responses.insert("200".into(), answered(own.answers(), describe));
        operation.insert("responses".into(), Value::Object(responses));
        paths.insert(own.path().into(), item(own.method(), operation));
    }
    Value::Object(paths)
}

/// One parameter, as OpenAPI's Parameter Object: in the query, read into
/// the JSON its shape says, and a list given once for each value, as the
/// server reads one (`form`, exploded), rather than one value split at a
/// comma.
pub fn parameter(parameter: &Parameter) -> Value {
    let schema = match parameter.shape {
        Shape::Text => json!({"type": "string"}),
        Shape::Number => json!({"type": "integer", "minimum": 0}),
        Shape::Flag => json!({"type": "boolean"}),
        Shape::List => json!({"type": "array", "items": {"type": "string"}}),
    };
    let mut described = json!({
        "name": parameter.name,
        "in": "query",
        "required": parameter.required,
        "description": parameter.about,
        "schema": schema,
    });
    if parameter.shape == Shape::List {
        described["style"] = json!("form");
        described["explode"] = json!(true);
    }
    described
}

/// What one of the server's own routes answers with when it answers, as
/// OpenAPI's Response Object: its own type, each place it carries the
/// backend's JSON filled by what fills it, and the events as a stream.
///
/// Filled where the route's answer is said rather than in the server's own
/// definition, which stays the any JSON it carries: an `allOf` of the two,
/// each holding, so the definition of `Preview` is one whatever the
/// backend, and each route says which of the backend's types its answer
/// carries.
pub fn answered(answer: Answer, describe: &mut impl Describe) -> Value {
    match answer {
        Answer::Preview => {
            let confirm = describe.carried(Carried::Confirm);
            let schema = json!({"allOf": [
                describe.own::<Preview>(),
                {"properties": {"confirm": {"items": confirm}}},
            ]});
            let said = "The command lines the task is, each as somebody would type it, and \
                        what of them to confirm before the run.";
            json_answer(said, schema)
        }
        Answer::Ran => json_answer("Its lines, and what running them produced.", ran(describe)),
        Answer::Runs => json_answer(
            "Every run the session made, oldest first.",
            json!({"type": "array", "items": ran(describe)}),
        ),
        Answer::Cancelling => json_answer(
            "The run asked to stop, and its lines: how it finishes says whether it stopped.",
            describe.own::<Cancelling>(),
        ),
        Answer::Switched => json_answer(
            "What the backend acts on from now on.",
            describe.carried(Carried::Switched),
        ),
        Answer::Events => {
            let scope = describe.carried(Carried::Scope);
            let progress = describe.carried(Carried::Progress);
            let event = json!({"allOf": [
                describe.own::<Event>(),
                {"properties": {"scopes": {"items": scope}, "progress": progress}},
            ]});
            json!({
                "description": "Server-sent events, `resync` first, until the server stops.",
                "content": {
                    "text/event-stream": {
                        "schema": {
                            "type": "string",
                            "description": "One event a message, unnamed, its `data:` line the \
                                            event as JSON, as `x-itemSchema` says it, its type \
                                            inside it; and a comment now and then, which \
                                            carries nothing, to keep the connection open.",
                        },
                        // OpenAPI 3.2's `itemSchema`, under the name a 3.1
                        // document may carry it by: 3.1 has no place for
                        // what each item of a stream holds.
                        "x-itemSchema": event,
                    },
                },
            })
        }
        Answer::Document => json_answer(
            "This document.",
            json!({
                "description": "The API described, as an OpenAPI 3.1 document.",
                "type": "object",
            }),
        ),
    }
}

/// What a run is answered with: the server's own, its outcome the
/// backend's, or, when that would not serialize, what the writer thread
/// says of it instead, so the run it was is still answered and recorded.
fn ran(describe: &mut impl Describe) -> Value {
    let outcome = describe.carried(Carried::Outcome);
    let unwritable = json!({
        "description": "Why the backend's outcome could not be answered, written in its place \
                        when it would not serialize.",
        "type": "object",
        "properties": {"error": {"type": "string"}},
        "required": ["error"],
    });
    json!({"allOf": [
        describe.own::<Ran>(),
        {"properties": {"outcome": {"anyOf": [outcome, unwritable]}}},
    ]})
}

/// Each status a request to a route may be refused with, as OpenAPI's
/// Response Objects by the status: every one a [`Refused`], its code one
/// of `conditions` of that status or, when the request reaches the
/// backend (`called`), one the front end says its backend refuses with on
/// the grounds that status is.
fn refusals(
    conditions: &[Condition],
    called: Option<Called<'_>>,
    describe: &mut impl Describe,
) -> Map<String, Value> {
    let mut statuses: BTreeMap<u16, (Vec<&str>, Option<Value>)> = BTreeMap::new();
    for condition in conditions {
        let status = condition.status().as_u16();
        statuses.entry(status).or_default().0.push(condition.code());
    }
    if let Some(called) = called {
        for grounds in Grounds::ALL {
            if let Some(codes) = describe.refused(called, grounds) {
                let status = grounds.status().as_u16();
                statuses.entry(status).or_default().1 = Some(codes);
            }
        }
    }
    let refused = describe.own::<Refused>();
    statuses
        .into_iter()
        .map(|(status, (own, backend))| {
            let own = (!own.is_empty()).then(|| json!({"type": "string", "enum": own}));
            let code = match (own, backend) {
                (Some(own), Some(backend)) => json!({"anyOf": [own, backend]}),
                (Some(codes), None) | (None, Some(codes)) => codes,
                (None, None) => unreachable!("a status is listed for a code"),
            };
            let schema = json!({"allOf": [refused.clone(), {"properties": {"code": code}}]});
            let mut response = json_answer(said(status), schema);
            if let Some(headers) = headers(status) {
                response["headers"] = headers;
            }
            (status.to_string(), response)
        })
        .collect()
}

/// What a refusal of `status` is, in a sentence: which codes it carries is
/// its schema's to say. One of its own for each status a condition or a
/// backend's grounds have, which a test holds; the last for none.
fn said(status: u16) -> &'static str {
    match status {
        400 => "What the request sent could not be read, a parameter or a body, as the code says.",
        401 => "Without the launch token, or with another server's.",
        403 => {
            "Turned away by a guard: a Host the server does not answer to, a page of another \
             origin, or a change by cookie the panel's own page would not send."
        }
        404 => "What the request names is not there, as the front end's code says.",
        405 => {
            "Asked by another method than the one its path takes, which `Allow` names: the \
             answer to every other method here, never to this one."
        }
        409 => {
            "Not the run under way, or one that takes no cancel, as the code says: nothing was \
             asked of any run."
        }
        413 => "A body larger than the server reads.",
        415 => "A body not sent as `application/json`.",
        422 => {
            "Read, and not one to act on: a body that is not what the route reads, or what the \
             front end will not do, as the code says."
        }
        500 => "What the request was handed on to failed, as the code says.",
        _ => "Refused, as the code says.",
    }
}

/// The headers a refusal of `status` carries besides: how to bring the
/// token beside a 401, and the methods the path takes beside a 405.
fn headers(status: u16) -> Option<Value> {
    match status {
        401 => Some(json!({"WWW-Authenticate": {
            "description": "How to bring the token: as a Bearer token.",
            "schema": {"type": "string", "const": "Bearer"},
        }})),
        405 => Some(json!({"Allow": {
            "description": "The methods the path takes.",
            "schema": {"type": "string"},
        }})),
        _ => None,
    }
}

/// An operation, before its body and answers: named for its method and
/// its path, so each has a name a client can be generated with, and its
/// one sentence.
fn operation(method: Method, path: &str, about: &str) -> Map<String, Value> {
    let id = format!(
        "{}{}",
        method.as_str().to_ascii_lowercase(),
        path.replace(['/', '.', '-'], "_")
    );
    let mut operation = Map::new();
    operation.insert("operationId".into(), Value::String(id));
    operation.insert("summary".into(), Value::String(about.into()));
    operation
}

/// A path's one operation, under its method.
fn item(method: Method, operation: Map<String, Value>) -> Value {
    let mut item = Map::new();
    item.insert(
        method.as_str().to_ascii_lowercase(),
        Value::Object(operation),
    );
    Value::Object(item)
}

/// The schema of what a route names `named`: its type, or a list of them
/// for `Type[]`, as a route's table names a list.
fn listed(named: &str, way: Way, describe: &mut impl Describe) -> Value {
    match named.strip_suffix("[]") {
        Some(item) => json!({"type": "array", "items": describe.named(item, way)}),
        None => describe.named(named, way),
    }
}

/// A body of JSON, which every route that takes one needs.
fn sent(schema: Value) -> Value {
    json!({"required": true, "content": {"application/json": {"schema": schema}}})
}

/// An answer of JSON, said in a sentence.
fn json_answer(said: &str, schema: Value) -> Value {
    json!({"description": said, "content": {"application/json": {"schema": schema}}})
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod tests;
