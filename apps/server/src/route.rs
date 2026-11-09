//! The reads a backend serves, as a table of routes, and the routes the
//! server answers itself: what the server matches a request against, and
//! what a description of the API is written from.
//!
//! Which reads there are, what each is called, what it takes and what it
//! answers are the backend's, since each is a rule of its front end's: the
//! server holds none, and learns them as data ([`Backend::routes`]). What it
//! knows is HTTP: a path to match, a method, a query string to read into the
//! values each parameter's [`Shape`] says, and a body. So it can refuse what
//! no route takes before the backend is asked anything, as
//! `deny_unknown_fields` refuses a field, and hand over the rest without
//! knowing what any of it means. Its own routes, a task previewed, run or
//! cancelled, a switch, the transcript and the events, are data too
//! ([`Own`]), so a description walks both and describes nothing by hand.
//!
//! [`Backend::routes`]: crate::Backend::routes

use std::fmt;
use std::io;

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

use crate::code::Condition;

/// Where every route of the API is served, its version in its path.
///
/// A route, a parameter or a field once served is a contract (D-24):
/// dropping or renaming one, or changing what it holds, moves the API to
/// `/api/v2`, and adding one does not. In the path rather than in a header,
/// so a script names the version it was written for where it names
/// everything else, and an old one meets a 404 rather than an answer it
/// misreads.
pub const API: &str = "/api/v1";

/// One route the server answers itself under [`API`], whatever the
/// backend, which no route of the backend's may take.
///
/// Data, as a backend's [`Route`] is, so a description of the API (#218)
/// walks these beside the backend's table, rather than describing the
/// writes by hand beside the handlers that serve them: the server serves
/// each from here ([`Own::ALL`]), by the method and at the path it says, so
/// the two cannot disagree. None takes a parameter. What each is sent and
/// answers is named by what it is ([`Sent`], [`Answer`]) rather than by a
/// type's name, as a backend's route names its own: a body is the
/// backend's `Task` or `Switch`, or the run a cancel names, and an answer
/// carries its `Outcome` or its JSON, types the server never names, so a
/// description names each as the backend's types do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Own {
    /// `POST /tasks/preview`.
    Preview,
    /// `POST /tasks`.
    Run,
    /// `POST /tasks/cancel`.
    Cancel,
    /// `POST /switch`.
    Switch,
    /// `GET /transcript`.
    Transcript,
    /// `GET /events`.
    Events,
    /// `GET /openapi.json`.
    Document,
}

impl Own {
    /// Every one, in the order a description of the API lists them.
    pub const ALL: [Own; 7] = [
        Own::Preview,
        Own::Run,
        Own::Cancel,
        Own::Switch,
        Own::Transcript,
        Own::Events,
        Own::Document,
    ];

    /// How it is asked for: a POST takes its body, and a GET nothing.
    pub fn method(self) -> Method {
        match self {
            Own::Preview | Own::Run | Own::Cancel | Own::Switch => Method::Post,
            Own::Transcript | Own::Events | Own::Document => Method::Get,
        }
    }

    /// Its path under [`API`], from its `/`, as a [`Route::path`] is.
    pub fn path(self) -> &'static str {
        match self {
            Own::Preview => "/tasks/preview",
            Own::Run => "/tasks",
            Own::Cancel => "/tasks/cancel",
            Own::Switch => "/switch",
            Own::Transcript => "/transcript",
            Own::Events => "/events",
            Own::Document => "/openapi.json",
        }
    }

    /// What it answers, in a sentence, for a description of the API to say.
    pub fn about(self) -> &'static str {
        match self {
            Own::Preview => {
                "The command lines a task is, and what of them to confirm before it runs, \
                 with nothing run"
            }
            Own::Run => {
                "Run a task, once every one sent before it has run: its lines, and what \
                 the command said"
            }
            Own::Cancel => {
                "Ask the run under way to stop, by its number as the events tell it: one \
                 that takes a cancel stops where it can, as how it finishes says"
            }
            Own::Switch => {
                "Act on what the body names from now on, once every task sent before it \
                 has run: what is acted on then"
            }
            Own::Transcript => "Every task the session ran, oldest first, with what each said",
            Own::Events => {
                "What happens as it happens, a change on disk or a run starting, going and \
                 finishing, as server-sent events"
            }
            Own::Document => "This description of the API, as an OpenAPI 3.1 document",
        }
    }

    /// What its body holds; none for a GET, which takes none.
    pub fn body(self) -> Option<Sent> {
        match self {
            Own::Preview | Own::Run => Some(Sent::Task),
            Own::Cancel => Some(Sent::Cancel),
            Own::Switch => Some(Sent::Switch),
            Own::Transcript | Own::Events | Own::Document => None,
        }
    }

    /// What it answers with.
    pub fn answers(self) -> Answer {
        match self {
            Own::Preview => Answer::Preview,
            Own::Run => Answer::Ran,
            Own::Cancel => Answer::Cancelling,
            Own::Switch => Answer::Switched,
            Own::Transcript => Answer::Runs,
            Own::Events => Answer::Events,
            Own::Document => Answer::Document,
        }
    }

    /// Every condition of the server's own a request to it may be refused
    /// for, in the order of [`Condition::ALL`]: what a description of the
    /// API lists among its answers, beside what the backend refuses a
    /// preview, a run or a switch for.
    pub fn conditions(self) -> Vec<Condition> {
        conditions(self.method(), &[], false, self.reaches())
    }

    /// What it hands a request on to: a preview the backend, a run and a
    /// switch the writer thread, a cancel the run under way, and the rest
    /// nothing, answered from what the server holds.
    pub(crate) fn reaches(self) -> Reaches {
        match self {
            Own::Preview => Reaches::Backend,
            Own::Run | Own::Switch => Reaches::Writer,
            Own::Cancel => Reaches::Underway,
            Own::Transcript | Own::Events | Own::Document => Reaches::Nothing,
        }
    }
}

/// What the body of one of the server's own routes ([`Own`]) holds: one
/// of the backend's types, which the server never names, or the server's
/// own [`Cancel`](crate::Cancel).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sent {
    /// A task, the backend's [`Task`](crate::Backend::Task).
    Task,
    /// The run to stop, the server's own [`Cancel`](crate::Cancel):
    /// `{"run": 3}`, whatever the backend.
    Cancel,
    /// What to act on from now on, the backend's
    /// [`Switch`](crate::Backend::Switch).
    Switch,
}

/// What one of the server's own routes ([`Own`]) answers with, as JSON
/// unless it says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// What a task would run, the server's [`Preview`](crate::Preview):
    /// `{"lines": [...], "confirm": [...]}`, each of `confirm` the
    /// backend's JSON.
    Preview,
    /// What one run did: `{"lines": [...], "outcome": ...}`, its outcome
    /// the backend's [`Outcome`](crate::Backend::Outcome).
    Ran,
    /// Every run the session made, oldest first: a list of what
    /// [`Answer::Ran`] is.
    Runs,
    /// The run asked to stop, the server's
    /// [`Cancelling`](crate::Cancelling): `{"run": 3, "lines": [...]}`.
    Cancelling,
    /// What the backend acts on after the switch, as its own JSON
    /// ([`Backend::switch`](crate::Backend::switch)).
    Switched,
    /// Server-sent events (`text/event-stream`) rather than JSON: each one
    /// [`Event`](crate::Event), as a `data:` line of JSON.
    Events,
    /// The API described, as an OpenAPI 3.1 document, the bytes the
    /// backend hands over ([`Backend::document`](crate::Backend::document)).
    Document,
}

/// How a route is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// A read named by its parameters.
    Get,
    /// A read that takes a body, a structure no parameter could hold, and
    /// changes nothing all the same: a plan of boxes ticked, a draft to
    /// judge. Never a write, which is a task (D-18).
    Post,
}

impl Method {
    /// As HTTP spells it.
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// What a parameter's values are, and how many of them it takes: what the
/// server reads each into before the backend sees it.
///
/// Four, because a query string carries nothing else a read needs: text, a
/// count, a yes or no, and a list. Each read into the JSON a body would hold
/// for it, so the backend reads parameters and a body alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// One value, as it was sent: a JSON string.
    Text,
    /// One whole number, at least 0: a JSON number.
    Number,
    /// `true` or `false`, and nothing else: a JSON boolean.
    Flag,
    /// Any number of values, the parameter given once for each,
    /// `root=/a&root=/b`, in that order: a JSON array of strings. The form
    /// a browser's form and every HTTP library write a list in, rather
    /// than one value split at a comma a path may hold.
    List,
}

/// One query parameter a route takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    /// Its name, as the query string spells it.
    pub name: &'static str,
    pub shape: Shape,
    /// Whether a request without it is refused: for a [`Shape::List`],
    /// without one value of it.
    pub required: bool,
    /// What it names, in a sentence, for a description of the API to say.
    pub about: &'static str,
}

impl Parameter {
    /// One a request must send.
    pub const fn required(name: &'static str, shape: Shape, about: &'static str) -> Self {
        Parameter {
            name,
            shape,
            required: true,
            about,
        }
    }

    /// One a request may leave out, which the backend then reads as absent.
    pub const fn optional(name: &'static str, shape: Shape, about: &'static str) -> Self {
        Parameter {
            name,
            shape,
            required: false,
            about,
        }
    }
}

/// One read a backend serves under [`API`], and how what a request sends
/// becomes the backend's query.
///
/// Data rather than handlers, so the server matches, reads and refuses by
/// it without a rule of its own, and so a description of the API (#218) is
/// written by walking the same table the server serves: its method, its
/// path, its parameters, its body and what it answers are all here.
pub struct Route<Q> {
    pub method: Method,
    /// Its path under [`API`], from its `/`: `/snapshot`. Words of lowercase
    /// letters, digits, `-` and `_`, between slashes; never a place for an
    /// identifier, which goes in a parameter, since a registry entry or a
    /// reference can hold a `/`.
    pub path: &'static str,
    /// What it answers, in a sentence, for a description of the API to say.
    pub about: &'static str,
    /// The query parameters a [`Method::Get`] takes; any other is
    /// refused. None for a [`Method::Post`], whose body holds what it reads,
    /// so a read has one place for what it is sent.
    pub parameters: Vec<Parameter>,
    /// What a [`Method::Post`]'s body holds, by the name of the backend's
    /// type for it; none for a [`Method::Get`], which takes none.
    pub body: Option<&'static str>,
    /// What it answers with, by the name of the backend's type for it, as
    /// the backend's generated types name it: `Snapshot`, or `Project[]`
    /// for a list of them.
    pub answers: &'static str,
    /// The backend's query for what a request sent ([`Arguments`]), or why
    /// it is none.
    ///
    /// The one thing here the server calls rather than reads: how the
    /// parameters become a query is the backend's, a parameter naming a
    /// holder or a list of roots, and the server only hands them over.
    pub query: fn(Arguments) -> Result<Q, String>,
}

impl<Q> Route<Q> {
    /// Every condition of the server's own a request to it may be refused
    /// for, in the order of [`Condition::ALL`], as [`Own::conditions`]
    /// says of the server's own routes: by its method and its parameters,
    /// beside what the backend refuses its query for.
    ///
    /// What may be, from what the table says: `invalid_parameters` for a
    /// GET whose `query` never fails as for one that does, since the
    /// server cannot see inside a route's `query`.
    pub fn conditions(&self) -> Vec<Condition> {
        conditions(self.method, &self.parameters, true, Reaches::Backend)
    }
}

/// What a route hands a request on to, past the server's own checks,
/// which can fail under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reaches {
    /// Nothing: the server answers from what it holds.
    Nothing,
    /// The backend, on the blocking pool.
    Backend,
    /// The backend, on the writer thread every run and switch takes its
    /// turn on.
    Writer,
    /// The run under way on the writer thread, and what the backend said
    /// a cancel of it does: never queued behind it, which would reach it
    /// only once it is over.
    Underway,
}

/// Every condition a request to a route asked by `method`, taking
/// `parameters`, may be refused for: a read of the backend's (`read`),
/// whose `query` makes what it sent into one, or one of the server's own,
/// handing it on to what `reaches` says.
///
/// Decided for each condition by a match with no wildcard, so a condition
/// added is placed among the routes before it compiles, rather than left
/// out of every description. Each where the server finds it: the guards
/// and the gate before a route is matched, a query string read by its
/// parameters ([`arguments`]), a body by axum's JSON extractor, and what
/// the request is handed on to.
fn conditions(
    method: Method,
    parameters: &[Parameter],
    read: bool,
    reaches: Reaches,
) -> Vec<Condition> {
    let post = method == Method::Post;
    let any = |test: fn(&Parameter) -> bool| parameters.iter().any(test);
    let may = |condition: Condition| match condition {
        // The Host and the Origin before anything, the token before any
        // route; a parameter no route takes, one of the server's own
        // included, and any method but its own at its path, by the route.
        Condition::TokenRequired
        | Condition::UnknownHost
        | Condition::ForeignOrigin
        | Condition::UnknownParameter
        | Condition::MethodNotAllowed => true,
        // A change, as every POST is, sent by cookie; and a body, read as
        // JSON.
        Condition::SameOriginRequired
        | Condition::PanelHeaderRequired
        | Condition::JsonRequired
        | Condition::MalformedJson
        | Condition::InvalidBody
        | Condition::BodyTooLarge
        | Condition::BodyUnreadable
        | Condition::BodyRefused => post,
        Condition::RepeatedParameter => any(|parameter| parameter.shape != Shape::List),
        Condition::MalformedParameter => {
            any(|parameter| matches!(parameter.shape, Shape::Number | Shape::Flag))
        }
        Condition::MissingParameter => any(|parameter| parameter.required),
        Condition::InvalidParameters => read && !post,
        // A path no route names: never one route's.
        Condition::UnknownRoute => false,
        Condition::NotRunning | Condition::Uncancellable => reaches == Reaches::Underway,
        Condition::BackendFailed => reaches != Reaches::Nothing,
        Condition::WriterStopped => reaches == Reaches::Writer,
    };
    Condition::ALL
        .iter()
        .copied()
        .filter(|condition| may(*condition))
        .collect()
}

/// What a request to a route sent, as JSON: an object of its parameters,
/// each as its [`Shape`] reads it and a parameter left out absent, for a
/// [`Method::Get`]; the body as it was sent, for a [`Method::Post`].
#[derive(Debug, Clone, PartialEq)]
pub struct Arguments(Value);

impl Arguments {
    /// What was sent, as the server read it.
    pub fn new(sent: Value) -> Self {
        Arguments(sent)
    }

    /// What was sent, read as `T`, or what keeps it from being one in
    /// serde's words: how a route's [`query`](Route::query) reads its
    /// parameters, into a struct of its own that refuses a field it does
    /// not name.
    pub fn read<T: DeserializeOwned>(self) -> Result<T, String> {
        serde_json::from_value(self.0).map_err(|error| error.to_string())
    }

    /// What was sent, as JSON.
    pub fn into_value(self) -> Value {
        self.0
    }
}

/// Why a query string is not one a route takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unfit {
    /// A parameter it does not take.
    Unknown { name: String },
    /// One that takes one value, given more than once.
    Twice { name: &'static str },
    /// A value its shape does not read.
    Malformed {
        name: &'static str,
        shape: Shape,
        value: String,
    },
    /// One it needs, left out.
    Missing { name: &'static str },
}

impl Unfit {
    /// The condition it is, whose code a refusal of it carries: one for
    /// each way a query string is unfit, so a script tells a misspelled
    /// parameter from a forgotten one without reading the words.
    pub(crate) fn condition(&self) -> Condition {
        match self {
            Unfit::Unknown { .. } => Condition::UnknownParameter,
            Unfit::Twice { .. } => Condition::RepeatedParameter,
            Unfit::Malformed { .. } => Condition::MalformedParameter,
            Unfit::Missing { .. } => Condition::MissingParameter,
        }
    }

    /// Said of the route at `path` that takes `parameters`.
    pub(crate) fn say(&self, path: &str, parameters: &[Parameter]) -> String {
        match self {
            Unfit::Unknown { name } => {
                let taken: Vec<String> = parameters
                    .iter()
                    .map(|parameter| format!("`{}`", parameter.name))
                    .collect();
                match taken.as_slice() {
                    [] => format!("{path} takes no parameter, and was sent `{name}`"),
                    [one] => format!("{path} takes no parameter `{name}`, only {one}"),
                    [first @ .., last] => format!(
                        "{path} takes no parameter `{name}`, only {} or {last}",
                        first.join(", ")
                    ),
                }
            }
            Unfit::Twice { name } => {
                format!("{path} takes one `{name}`, and was sent more than one")
            }
            Unfit::Malformed { name, shape, value } => {
                let wanted = match shape {
                    Shape::Number => "a whole number",
                    Shape::Flag => "`true` or `false`",
                    // Every value is text, and every value a list.
                    Shape::Text | Shape::List => "text",
                };
                format!("{path} takes `{name}` as {wanted}, not `{value}`")
            }
            Unfit::Missing { name } => format!("{path} needs `{name}`, and was sent none"),
        }
    }
}

/// The parameters `query` sends, read as `parameters` say into one JSON
/// object, or the first thing about them no route of these parameters
/// takes.
///
/// Decoded as a browser encodes a form, `+` a space and `%2F` a slash, as
/// the launch link's token is. Every name must be one of `parameters`, as a
/// field must be one a body's type names; each but a [`Shape::List`] given
/// once, since which of two values a read went by would otherwise be the
/// server's guess; and every required one given.
pub(crate) fn arguments(
    parameters: &[Parameter],
    query: Option<&str>,
) -> Result<Map<String, Value>, Unfit> {
    let mut read = Map::new();
    for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()) {
        let Some(parameter) = parameters.iter().find(|parameter| parameter.name == name) else {
            return Err(Unfit::Unknown {
                name: name.into_owned(),
            });
        };
        let malformed = || Unfit::Malformed {
            name: parameter.name,
            shape: parameter.shape,
            value: value.clone().into_owned(),
        };
        let value = match parameter.shape {
            Shape::Text => Value::String(value.clone().into_owned()),
            Shape::Number => Value::from(value.parse::<u64>().map_err(|_| malformed())?),
            Shape::Flag => match value.as_ref() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => return Err(malformed()),
            },
            Shape::List => {
                let values = read
                    .entry(parameter.name)
                    .or_insert_with(|| Value::Array(Vec::new()));
                if let Value::Array(values) = values {
                    values.push(Value::String(value.into_owned()));
                }
                continue;
            }
        };
        if read.insert(parameter.name.to_owned(), value).is_some() {
            return Err(Unfit::Twice {
                name: parameter.name,
            });
        }
    }
    if let Some(missing) = parameters
        .iter()
        .find(|parameter| parameter.required && !read.contains_key(parameter.name))
    {
        return Err(Unfit::Missing { name: missing.name });
    }
    Ok(read)
}

/// Whether `routes` can be served: each path a route's (see
/// [`Route::path`]), none taken twice nor one the server answers itself,
/// no parameter named twice in one route, a GET taking parameters and no
/// body, and a POST a body and no parameters.
///
/// Checked as the server binds rather than left to the router, which
/// would panic on a path taken twice and read `{name}` in one as a place
/// for a value, so a table that cannot be served fails `bind` with why.
pub(crate) fn check<Q>(routes: &[Route<Q>]) -> io::Result<()> {
    let invalid = |why: String| Err(io::Error::new(io::ErrorKind::InvalidInput, why));
    for (at, route) in routes.iter().enumerate() {
        let path = route.path;
        let words = path.strip_prefix('/').map(|rest| rest.split('/'));
        let plain = |word: &str| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
        };
        // First, since one of the server's own is no path a backend's
        // could have, `/openapi.json` among them, and that it is the
        // server's says more.
        if Own::ALL.iter().any(|own| own.path() == path) {
            return invalid(format!(
                "the route `{path}` is one the server answers itself"
            ));
        }
        if !words.is_some_and(|mut words| words.all(plain)) {
            return invalid(format!(
                "the route `{path}` is no path of lowercase words between slashes"
            ));
        }
        if routes[..at].iter().any(|before| before.path == path) {
            return invalid(format!("the route `{path}` is in the table twice"));
        }
        let names = &route.parameters;
        if let Some(twice) = names.iter().enumerate().find(|(at, parameter)| {
            names[..*at]
                .iter()
                .any(|before| before.name == parameter.name)
        }) {
            return invalid(format!(
                "the route `{path}` names its parameter `{}` twice",
                twice.1.name
            ));
        }
        match (route.method, route.body) {
            (Method::Get, Some(_)) => {
                return invalid(format!("the route `{path}` is a GET, which takes no body"));
            }
            (Method::Post, None) => {
                return invalid(format!(
                    "the route `{path}` is a POST, which takes what it reads in its body"
                ));
            }
            (Method::Post, Some(_)) if !route.parameters.is_empty() => {
                return invalid(format!(
                    "the route `{path}` is a POST, which takes what it reads in its body, \
                     not in parameters"
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "route_test.rs"]
mod tests;
