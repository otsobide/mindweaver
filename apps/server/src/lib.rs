//! The local server behind the web panel.
//!
//! It is transport and nothing else. The browser sends JSON, the server turns
//! it into a front end's own types and hands them to that front end's
//! [`Backend`], and whatever comes back goes out as JSON. What a valid task is,
//! which paths it may name and what running it means are all the backend's
//! business, which in practice is core's: a rule written here would be one a
//! second front end could disagree with.

mod api;
mod auth;
mod backend;
mod code;
mod events;
mod guard;
mod log;
mod panel;
mod route;
#[cfg(feature = "schema")]
pub mod schema;
mod server;
mod token;
mod transcript;
mod underway;
mod watcher;
mod writer;

pub use api::Refused;
pub use backend::{Backend, Grounds, Preview, Refusal, Watched};
pub use code::Condition;
pub use events::{Event, Events, Subscription, CAPACITY};
pub use panel::Panel;
pub use route::{Answer, Arguments, Method, Own, Parameter, Route, Sent, Shape, API};
pub use server::{Listening, Server};
pub use transcript::Ran;
pub use underway::{Cancel, Cancelling, Underway};
pub use watcher::{Watcher, DEBOUNCE};
