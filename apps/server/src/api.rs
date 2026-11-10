//! The JSON endpoints under `/api/v1`, and what answers every other path
//! under `/api`.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::rejection::{BytesRejection, FailedToBufferBody, JsonRejection};
use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::http::{Method, Uri};
use axum::response::sse::{self, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, on, post, MethodFilter, MethodRouter};
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::code::Condition;
use crate::events::Events;
use crate::route::{self, Arguments, Own, Route};
use crate::transcript::Transcript;
use crate::underway::Cancel;
use crate::watcher::Watcher;
use crate::writer::{Uncancelled, Writer};
use crate::{Backend, Refusal};

/// What every handler under `/api` shares.
pub(crate) struct Api<B: Backend> {
    pub(crate) backend: Arc<B>,
    pub(crate) writer: Writer<B>,
    pub(crate) transcript: Transcript,
    pub(crate) events: Events,
    /// Held for as long as the routes are, and by the writer's thread, which
    /// has it watch anew after a switch; dropped with both, which stops it.
    pub(crate) _watcher: Arc<Watcher>,
}

/// A read one of the backend's routes serves ([`Route`]), as the method
/// it is asked by: a GET by its parameters, a POST by its body.
pub(crate) fn read<B: Backend>(route: Route<B::Query>) -> MethodRouter<Arc<Api<B>>> {
    let route = Arc::new(route);
    match route.method {
        route::Method::Get => get(move |State(api): State<Arc<Api<B>>>, uri: Uri| {
            let route = Arc::clone(&route);
            async move { answer(&api, &route, &uri, None).await }
        }),
        route::Method::Post => post(
            move |State(api): State<Arc<Api<B>>>,
                  uri: Uri,
                  body: Result<Json<Value>, JsonRejection>| {
                let route = Arc::clone(&route);
                async move { answer(&api, &route, &uri, Some(body)).await }
            },
        ),
    }
}

/// One of the server's own routes ([`Own`]), its handler asked by the
/// method the table says rather than one spelled again here, so a
/// description written from the table says how it is served.
pub(crate) fn own<B: Backend>(own: Own) -> MethodRouter<Arc<Api<B>>> {
    let method = match own.method() {
        route::Method::Get => MethodFilter::GET,
        route::Method::Post => MethodFilter::POST,
    };
    match own {
        Own::Preview => on(method, preview::<B>),
        Own::Run => on(method, run::<B>),
        Own::Cancel => on(method, cancel::<B>),
        Own::Switch => on(method, switch::<B>),
        Own::Transcript => on(method, transcript::<B>),
        Own::Events => on(method, events::<B>),
        Own::Document => on(method, document::<B>),
    }
}

/// What `route` answers to a request for `uri`, with `body` for a POST:
/// its parameters read as the table says, every one it does not take
/// refused, the backend's query made of them by the route, and that query
/// answered.
///
/// What a GET sends that the route cannot read is a 400, as a query string
/// is refused anywhere (`invalid_parameters`); what a POST's body holds that
/// it cannot read is a 422, as a body a task cannot be read from is, since
/// the request arrived whole (`invalid_body`). The route's own words come
/// after its path, so a script learns which of its requests they are
/// about.
async fn answer<B: Backend>(
    api: &Api<B>,
    route: &Route<B::Query>,
    uri: &Uri,
    body: Option<Result<Json<Value>, JsonRejection>>,
) -> Response {
    let path = uri.path();
    let parameters = match route::arguments(&route.parameters, uri.query()) {
        Ok(parameters) => Value::Object(parameters),
        Err(unfit) => return error(unfit.condition(), &unfit.say(path, &route.parameters)),
    };
    let (sent, unfit) = match body {
        None => (parameters, Condition::InvalidParameters),
        Some(Ok(Json(body))) => (body, Condition::InvalidBody),
        Some(Err(rejection)) => return refused(rejection),
    };
    match (route.query)(Arguments::new(sent)) {
        Ok(query) => ask(api, query).await,
        Err(message) => error(unfit, &format!("{path}: {message}")),
    }
}

/// The backend's answer to `query`, on the blocking pool, because a query
/// reads the disk and the runtime's own threads are for moving bytes: one
/// slow read there would stall every other request waiting on that thread.
async fn ask<B: Backend>(api: &Api<B>, query: B::Query) -> Response {
    let backend = Arc::clone(&api.backend);
    match tokio::task::spawn_blocking(move || backend.query(query)).await {
        Ok(Ok(answer)) => Json(answer).into_response(),
        Ok(Err(refusal)) => refused_by(refusal),
        Err(_) => error(
            Condition::BackendFailed,
            "the backend failed while answering the query",
        ),
    }
}

/// `POST /api/v1/tasks/preview`: the command lines a task is, and what of
/// them the panel has somebody confirm, with nothing run. On the blocking
/// pool for the same reason as a query: working out a line can mean reading
/// the disk.
pub(crate) async fn preview<B: Backend>(
    State(api): State<Arc<Api<B>>>,
    uri: Uri,
    body: Result<Json<B::Task>, JsonRejection>,
) -> Response {
    if let Some(refusal) = unasked(&uri) {
        return refusal;
    }
    let task = match body {
        Ok(Json(task)) => task,
        Err(rejection) => return refused(rejection),
    };
    let backend = Arc::clone(&api.backend);
    match tokio::task::spawn_blocking(move || backend.preview(&task)).await {
        Ok(Ok(preview)) => Json(preview).into_response(),
        Ok(Err(refusal)) => refused_by(refusal),
        Err(_) => error(
            Condition::BackendFailed,
            "the backend failed while previewing the task",
        ),
    }
}

/// `POST /api/v1/tasks`: run a task, once every task that arrived before it
/// has run, and say what its lines were and what came of them; or, when the
/// backend refused it, why nothing of it ran.
pub(crate) async fn run<B: Backend>(
    State(api): State<Arc<Api<B>>>,
    uri: Uri,
    body: Result<Json<B::Task>, JsonRejection>,
) -> Response {
    if let Some(refusal) = unasked(&uri) {
        return refusal;
    }
    let task = match body {
        Ok(Json(task)) => task,
        Err(rejection) => return refused(rejection),
    };
    match api.writer.run(task).await {
        Ok(Ok(ran)) => Json(ran).into_response(),
        Ok(Err(refusal)) => refused_by(refusal),
        Err(failed) => error(failed.condition, failed.message),
    }
}

/// `POST /api/v1/tasks/cancel`: ask run `run` to stop, the one under way,
/// and say which run that was and its lines; or why it was not asked.
///
/// Never queued on the writer thread, which the run holds: it reaches the
/// run where it is kept for a cancel, and hands it to what the backend said
/// one does. Whether the run stops, and where, is the run's, which its
/// finish and its outcome say: a gather past the point where it can stop
/// places what it read and finishes. Refused, with nothing asked of
/// anything, for a run that is not the one under way (`not_running`, a
/// 409), one over already among them, so a cancel that crosses the end of
/// its run never stops the one queued behind; and for the run under way
/// when its preview said it takes none (`uncancellable`, a 409), rather
/// than pass on a cancel that would do nothing.
pub(crate) async fn cancel<B: Backend>(
    State(api): State<Arc<Api<B>>>,
    uri: Uri,
    body: Result<Json<Cancel>, JsonRejection>,
) -> Response {
    if let Some(refusal) = unasked(&uri) {
        return refusal;
    }
    let Cancel { run } = match body {
        Ok(Json(cancel)) => cancel,
        Err(rejection) => return refused(rejection),
    };
    match api.writer.cancel(run).await {
        Ok(cancelling) => Json(cancelling).into_response(),
        Err(Uncancelled::NotRunning { under_way: None }) => error(
            Condition::NotRunning,
            &format!(
                "run {run} is not under way, and nothing is: a run is stopped only while it runs"
            ),
        ),
        Err(Uncancelled::NotRunning {
            under_way: Some(under_way),
        }) => error(
            Condition::NotRunning,
            &format!(
                "run {run} is not under way, run {under_way} is: a run is stopped only while \
                 it runs"
            ),
        ),
        Err(Uncancelled::Uncancellable) => error(
            Condition::Uncancellable,
            &format!(
                "run {run} takes no cancel: none of its lines stops when asked, so it runs to \
                 its end"
            ),
        ),
        Err(Uncancelled::Failed) => error(
            Condition::BackendFailed,
            "the backend failed while asking the run to stop",
        ),
    }
}

/// `POST /api/v1/switch`: act on what the body names from now on, once every
/// task that arrived before it has run, and say what that is; or, when the
/// backend refused it, why nothing changed.
///
/// On the writer thread, as a run is, so it never lands while a task runs.
/// Nothing is kept in the transcript, which holds runs to replay: a switch
/// is no command, and every line the backend runs names what it acts on.
pub(crate) async fn switch<B: Backend>(
    State(api): State<Arc<Api<B>>>,
    uri: Uri,
    body: Result<Json<B::Switch>, JsonRejection>,
) -> Response {
    if let Some(refusal) = unasked(&uri) {
        return refusal;
    }
    let to = match body {
        Ok(Json(to)) => to,
        Err(rejection) => return refused(rejection),
    };
    match api.writer.switch(to).await {
        Ok(Ok(answer)) => Json(answer).into_response(),
        Ok(Err(refusal)) => refused_by(refusal),
        Err(failed) => error(failed.condition, failed.message),
    }
}

/// `GET /api/v1/transcript`: every task the session ran, oldest first.
pub(crate) async fn transcript<B: Backend>(State(api): State<Arc<Api<B>>>, uri: Uri) -> Response {
    if let Some(refusal) = unasked(&uri) {
        return refusal;
    }
    Json(api.transcript.entries()).into_response()
}

/// `GET /api/v1/events`: every event from now on ([`crate::Event`]), as
/// server-sent events, one `data:` line of JSON each, `resync` first.
///
/// Server-sent events rather than a WebSocket: the stream goes one way, a
/// browser's `EventSource` reconnects by itself, and it is a plain GET, so
/// the gate holds it to the token as it holds every other read, the cookie
/// included. Each event unnamed, its type inside the JSON, so the panel
/// reads them all through `onmessage` rather than a listener per name.
///
/// A comment every 15 seconds while nothing happens (`KeepAlive`), so a
/// proxy between, the panel's dev server's among them, does not take the
/// connection for one that has died. The stream ends when the bus is
/// closed, which a server stopping does first: until then it is a request
/// under way, which a graceful stop would wait for without end.
pub(crate) async fn events<B: Backend>(State(api): State<Arc<Api<B>>>, uri: Uri) -> Response {
    if let Some(refusal) = unasked(&uri) {
        return refusal;
    }
    let stream =
        futures_util::stream::unfold(api.events.subscribe(), |mut subscription| async move {
            let event = subscription.next().await?;
            // Words, numbers and the backend's JSON: nothing in it can fail.
            let frame = sse::Event::default()
                .json_data(&event)
                .expect("an event serializes");
            Some((Ok::<_, Infallible>(frame), subscription))
        });
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// `GET /api/v1/openapi.json`: the API described, as the backend hands the
/// document over ([`Backend::document`]), its bytes as they are.
///
/// Behind the token as every route is, though it says nothing of the
/// workspace: the server answers nobody without it, so what it serves is
/// one rule rather than a list of what may be read without.
pub(crate) async fn document<B: Backend>(uri: Uri) -> Response {
    if let Some(refusal) = unasked(&uri) {
        return refusal;
    }
    ([(CONTENT_TYPE, "application/json")], B::document()).into_response()
}

/// The refusal of a parameter sent to a route of the server's own, which
/// takes none, as a backend's route refuses one it does not take: so a
/// script's misspelling is said rather than passed over, under every route
/// alike. None when none was sent.
fn unasked(uri: &Uri) -> Option<Response> {
    route::arguments(&[], uri.query())
        .err()
        .map(|unfit| error(unfit.condition(), &unfit.say(uri.path(), &[])))
}

/// Whether a path is the API's: `/api` and everything under it, where every
/// answer is JSON, refusals included.
pub(crate) fn is_api(path: &str) -> bool {
    path == "/api" || path.starts_with("/api/")
}

/// Any other path under `/api`: a 404 a script can read, rather than the
/// panel's page, which is what a path outside `/api` that names no file gets.
pub(crate) async fn missing(method: Method, uri: Uri) -> Response {
    error(
        Condition::UnknownRoute,
        &format!("no such endpoint: {method} {}", uri.path()),
    )
}

/// A known path asked for with a method it does not take, the methods it
/// does take in `Allow`, which the router adds.
pub(crate) async fn not_allowed(method: Method, uri: Uri) -> Response {
    error(
        Condition::MethodNotAllowed,
        &format!("{} does not take {method}", uri.path()),
    )
}

/// A body that is not the JSON a handler expects, in the same shape as every
/// other error here rather than axum's plain text, with axum's words for
/// what it found and the code of the condition it is.
pub(crate) fn refused(rejection: JsonRejection) -> Response {
    error(unread(&rejection), &rejection.body_text())
}

/// The condition each of axum's ways to refuse a JSON body is, answered
/// with the status axum gives it, so a description of the API lists each
/// beforehand.
///
/// The rejection is `#[non_exhaustive]`, and so is what it carries when the
/// body could not be read, so this match is the one place among the
/// server's codes with a fallback. Whatever keeps a body from being read
/// whole is `body_unreadable`, however axum comes to say it; a rejection
/// axum adds later is `body_refused`, a 400 with a code of its own, rather
/// than an answer with no code, or with one that says something else.
fn unread(rejection: &JsonRejection) -> Condition {
    match rejection {
        JsonRejection::JsonDataError(_) => Condition::InvalidBody,
        JsonRejection::JsonSyntaxError(_) => Condition::MalformedJson,
        JsonRejection::MissingJsonContentType(_) => Condition::JsonRequired,
        JsonRejection::BytesRejection(BytesRejection::FailedToBufferBody(
            FailedToBufferBody::LengthLimitError(_),
        )) => Condition::BodyTooLarge,
        JsonRejection::BytesRejection(_) => Condition::BodyUnreadable,
        _ => Condition::BodyRefused,
    }
}

/// What every answer under `/api` that is no success holds, whoever turned
/// the request away, the server or the backend: `{"code": "...", "error":
/// "..."}`, an object of these two fields and no other.
///
/// One type for both ([`error`], [`refused_by`]) rather than an object
/// spelled where each is answered, so the shape a script reads every
/// refusal by is said once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Refused {
    /// The one word a script acts on, which never changes when the words
    /// are made clearer: a [`Condition`]'s, or the backend's own.
    pub code: &'static str,
    /// What happened, for a person: the route, the parameter or the header,
    /// what was expected and what came.
    pub error: String,
}

/// What the backend refused, as [`Refused`], its code the backend's own: a
/// 404 when what the request named is not there, and a 422 when it is not
/// one to act on, as a body the server cannot read is. Which of the two,
/// the backend says ([`Grounds`](crate::Grounds)); the server never reads
/// the code to tell.
pub(crate) fn refused_by(refusal: Refusal) -> Response {
    let refused = Refused {
        code: refusal.code,
        error: refusal.message,
    };
    (refusal.grounds.status(), Json(refused)).into_response()
}

/// A condition of the server's own, as [`Refused`], with the status it is
/// answered with: the words of what was found, the route, the parameter or
/// the header, beside the one word a script acts on.
pub(crate) fn error(condition: Condition, message: &str) -> Response {
    let refused = Refused {
        code: condition.code(),
        error: message.to_owned(),
    };
    (condition.status(), Json(refused)).into_response()
}
