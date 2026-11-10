//! A stable code for everything the server answers under `/api` that is no
//! success of its own making: one word a script acts on, beside the words a
//! person reads.
//!
//! The status says which family a refusal is of, and the message what went
//! wrong in detail, the route, the parameter, the header, what was expected
//! and what came. Neither is enough for a program: several conditions share
//! a status (a 403 is a Host, an Origin or a header the panel's page sends),
//! and the message is reworded whenever it is made clearer. So every answer
//! the server gives that is no success carries a code beside its message,
//! as every refusal of a backend's does, and every error core gives.
//!
//! The server's codes are its own, written here once. Core's `codes!`
//! would be the natural tool, but the server depends on no mindweaver crate
//! (`tests/crate_test.rs`), so it mirrors its rules instead:
//!
//! - **One code per condition**, named for it, in snake_case, in as few
//!   words as tell it from every other: `token_required`, `unknown_route`.
//! - **Written as a match with no wildcard**, by `codes!`, which takes one
//!   variant per arm and writes [`Condition::code`], [`Condition::status`],
//!   [`Condition::ALL`] and [`Condition::CODES`] from the same arms: a new
//!   condition does not compile until it has a code and a status, and
//!   cannot have one the list leaves out.
//! - **Unique beside core's and the CLI's.** A code reaches a client
//!   without the crate it came from, so `invalid_body` must mean the same
//!   whoever said it. The server cannot see the other two lists, so the
//!   CLI, which depends on all three crates, holds them apart in a test
//!   (`error_test.rs`).
//! - **A contract once given**, as a serialized name is: a code is never
//!   renamed, nor moved to another condition. Rewording a message changes
//!   none.
//!
//! A backend's refusal carries its own front end's code
//! ([`Refusal`](crate::Refusal)), never one of these: the server gives a
//! code only to a condition it found itself.

use axum::http::StatusCode;

/// Give [`Condition`] its codes and statuses, from one arm per variant:
///
/// ```text
/// codes!(Condition {
///     Self::TokenRequired => (UNAUTHORIZED, "token_required"),
/// });
/// ```
///
/// Each arm is `Self::` and one variant's name, the conditions having no
/// fields: what an arm may say is spelled out rather than taken as any
/// pattern, so that `_` or `Self::A | Self::B`, which would give several
/// conditions one code, does not compile. The status is named as
/// [`StatusCode`]'s constant is.
macro_rules! codes {
    ($condition:ident {
        $(Self::$variant:ident => ($status:ident, $code:literal),)*
    }) => {
        impl $condition {
            /// Every condition, in the order a description of the API lists
            /// them.
            pub const ALL: &'static [$condition] = &[$($condition::$variant),*];

            /// Every code the server gives of its own, in the order of
            /// [`ALL`](Self::ALL).
            pub const CODES: &'static [&'static str] = &[$($code),*];

            /// The word for the condition, unique among every code core,
            /// the CLI and the server give, and never changed once given:
            /// what a client acts on, where the message may be reworded.
            pub fn code(self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)*
                }
            }

            /// The status the server answers it with.
            pub fn status(self) -> StatusCode {
                match self {
                    $(Self::$variant => StatusCode::$status,)*
                }
            }
        }
    };
}

/// Why the server answers a request under `/api` with no success, when the
/// server rather than the backend found it: the guards, the token, a route
/// that is not there or not asked that way, what a request sends that a
/// route cannot read, and a backend or a thread that failed.
///
/// Conditions without the words for them: each place that finds one says
/// what it found, the route, the parameter or the header named, so the
/// message holds the detail while the code stays the one word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Condition {
    /// A request under `/api` without the launch token, as the cookie its
    /// link sets or as a Bearer token, or with one that is not this
    /// server's: one condition, since either way the request does not hold
    /// the token, and what to do is the same, open the link printed.
    TokenRequired,
    /// A request whose Host is not one the server answers to: not exactly
    /// one, or one naming another name or another port.
    UnknownHost,
    /// A request sent from a page of another origin, as its Origin says.
    ForeignOrigin,
    /// A change sent with the cookie without `Sec-Fetch-Site:
    /// same-origin`: from a page on another port of loopback, or from a
    /// script, which sends the token as Bearer instead.
    SameOriginRequired,
    /// A change sent with the cookie without `X-Mindweaver: 1`
    /// ([`PANEL_HEADER`](crate::guard::PANEL_HEADER)), which the panel's own
    /// requests carry. Its code spells the header as Mindflayer named it,
    /// `X-Mindflayer`, as it was given, since a code once given is never
    /// renamed (D-26).
    PanelHeaderRequired,
    /// A body not sent as `application/json`: refused by the guard on a
    /// change by cookie and by the JSON extractor alike, one condition
    /// whichever finds it.
    JsonRequired,
    /// A body that is not JSON at all.
    MalformedJson,
    /// JSON that is not what the route reads: a task, a switch or a read's
    /// body with a field missing, one it does not take, or a value of
    /// another type, and a POST read whose route's `query` cannot read
    /// what it was sent.
    InvalidBody,
    /// A body larger than the server reads.
    BodyTooLarge,
    /// A body that could not be read whole, the connection failing under
    /// it.
    BodyUnreadable,
    /// A body the JSON extractor refused in a way the server does not know
    /// by name: one axum adds later, since its rejection is
    /// `#[non_exhaustive]`.
    BodyRefused,
    /// A query parameter the route does not take.
    UnknownParameter,
    /// A parameter that takes one value, given more than once.
    RepeatedParameter,
    /// A value its parameter's shape does not read: a number that is not
    /// one, a flag that is neither `true` nor `false`.
    MalformedParameter,
    /// A parameter the route needs, left out.
    MissingParameter,
    /// A GET whose parameters, each read, its route's `query` cannot make
    /// a query of.
    InvalidParameters,
    /// A path under `/api` that no route names: a typo, a version not
    /// served, or the panel's old unversioned endpoints.
    UnknownRoute,
    /// A method the route at that path does not take; the answer's `Allow`
    /// says which it does.
    MethodNotAllowed,
    /// A cancel of a run that is not under way: one over already, one not
    /// started yet, or a number no run was given. A run is stopped only
    /// while it runs, so one that ended is told apart from one that will
    /// not stop, and nothing queued behind it is stopped in its place.
    NotRunning,
    /// A cancel of the run under way, which takes none: its preview said
    /// none of its lines stops when asked, so it runs to its end, rather
    /// than the cancel be passed on to do nothing.
    Uncancellable,
    /// The backend failed while answering, its call panicking, so there
    /// is no answer to give: a query, a preview, a run, a switch or what a
    /// cancel does.
    BackendFailed,
    /// The thread every run and switch goes through has stopped, so
    /// nothing can be queued on it.
    WriterStopped,
}

codes!(Condition {
    Self::TokenRequired => (UNAUTHORIZED, "token_required"),
    Self::UnknownHost => (FORBIDDEN, "unknown_host"),
    Self::ForeignOrigin => (FORBIDDEN, "foreign_origin"),
    Self::SameOriginRequired => (FORBIDDEN, "same_origin_required"),
    Self::PanelHeaderRequired => (FORBIDDEN, "x_mindflayer_required"),
    Self::JsonRequired => (UNSUPPORTED_MEDIA_TYPE, "json_required"),
    Self::MalformedJson => (BAD_REQUEST, "malformed_json"),
    Self::InvalidBody => (UNPROCESSABLE_ENTITY, "invalid_body"),
    Self::BodyTooLarge => (PAYLOAD_TOO_LARGE, "body_too_large"),
    Self::BodyUnreadable => (BAD_REQUEST, "body_unreadable"),
    Self::BodyRefused => (BAD_REQUEST, "body_refused"),
    Self::UnknownParameter => (BAD_REQUEST, "unknown_parameter"),
    Self::RepeatedParameter => (BAD_REQUEST, "repeated_parameter"),
    Self::MalformedParameter => (BAD_REQUEST, "malformed_parameter"),
    Self::MissingParameter => (BAD_REQUEST, "missing_parameter"),
    Self::InvalidParameters => (BAD_REQUEST, "invalid_parameters"),
    Self::UnknownRoute => (NOT_FOUND, "unknown_route"),
    Self::MethodNotAllowed => (METHOD_NOT_ALLOWED, "method_not_allowed"),
    Self::NotRunning => (CONFLICT, "not_running"),
    Self::Uncancellable => (CONFLICT, "uncancellable"),
    Self::BackendFailed => (INTERNAL_SERVER_ERROR, "backend_failed"),
    Self::WriterStopped => (INTERNAL_SERVER_ERROR, "writer_stopped"),
});

#[cfg(test)]
#[path = "code_test.rs"]
mod tests;
