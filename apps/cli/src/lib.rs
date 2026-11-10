//! Command line interface for Mindweaver, split the way the model is.
//!
//! `mind <cmd>` acts on the mind project you are standing in. `mind weaver
//! <cmd>` acts on the weaver workspace above it, and the `mindweaver` binary is
//! the same tree reached directly, so `mindweaver link x` and `mind weaver link x`
//! are one command with two spellings.
//!
//! The parser, the work and the rendering all live in this library rather
//! than in the binaries so the tests drive the real command surface instead of
//! shelling out, and so both binaries cannot drift apart.
//!
//! Each has a module of its own (`parse`, `commands` with a module per family
//! of commands, and `render`), so that work on one command does not collide
//! with work on another in one long file. What the binaries and the tests use
//! is public: the two screens, and what is re-exported here.

pub mod admit;
#[cfg(feature = "ui")]
pub mod backend;
pub(crate) mod commands;
pub(crate) mod error;
pub(crate) mod homes;
pub mod install;
#[cfg(feature = "schema")]
pub mod openapi;
pub(crate) mod parse;
pub mod progress;
pub(crate) mod render;
#[cfg(feature = "ui")]
pub mod routes;
#[cfg(feature = "schema")]
pub mod schema;
pub mod task;
pub(crate) mod token;
pub mod tui;
#[cfg(feature = "ts")]
pub mod typescript;

pub use commands::read::WORKSPACE;
pub use commands::{run, run_watched, run_weaver_cli, run_weaver_cli_watched};
pub use error::{CliError, CODES};
pub use homes::{agent_homes, default_home, Lookup};
#[cfg(feature = "ui")]
pub use parse::UiOptions;
pub use parse::{
    AgentHomeOptions, AgentNames, AgentsChange, Cli, Command, Credential, Editing, GatherCommand,
    Given, Migrated, RegistryCommand, SearchCredential, ServerOptions, ShareMode, TargetsChange,
    WeaverCli, WeaverCommand, WorkspacesCommand,
};
pub use progress::ProgressLine;
pub use render::{Failure, Outcome, Warning};
