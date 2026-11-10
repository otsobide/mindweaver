//! A stable code for everything core can say went wrong: one word a front
//! end acts on, where the message is for whoever reads it.
//!
//! The terminal shows a message and nothing more is asked of it. The panel
//! has to tell a workspace another writer holds from a project that has gone,
//! and the server hands it what went wrong as JSON: matching the English
//! would break the first time a message is reworded, which happens whenever a
//! message is made clearer. So every public error type in core has a
//! `code()`, and `ValidationIssue`, what `validate` finds, has one too, as
//! does `install::Refused`, why an install or an uninstall wrote nothing
//! though nothing went wrong (`foreign`, `edited`).
//!
//! - **Unique across core**, not only within one type: a code reaches the
//!   panel without the type it came from, so `workspace_busy` has to mean the
//!   lock whatever command met it. [`ALL`] lists every code, and a test holds
//!   no two alike.
//! - **A variant that carries another error of core's takes its code** when
//!   that error is what went wrong and the variant adds only where, or what
//!   came of it: an install that met a busy lock says `workspace_busy`, as
//!   the lock does, rather than a code of the install's that the panel would
//!   have to map back. A variant that says something the error it carries
//!   does not, a record dropped while its exclude entry stays, has a code of
//!   its own.
//! - **Named for the condition**, in snake_case, in as few words as tell it
//!   from every other: the variant's own name where that is enough
//!   (`offered_twice`), and the subject before it where the bare name says
//!   nothing alone (`exclude_read_failed`, `adopt_unfit`).
//! - **A contract once given**, as a serialized name is: a code is never
//!   renamed, nor moved to another condition, since a front end acting on
//!   it would stop without a word. A new variant gets a new code; rewording
//!   a message changes none.
//!
//! Each type is given its codes with `codes!`, which writes `code()` as a
//! match with no wildcard, so a new variant does not compile until it has
//! one, and the list of the codes it gives from the same arms, so a code
//! cannot be added without the uniqueness test seeing it. Each arm names one
//! variant, which the macro holds to by what it takes: a wildcard or an
//! or-pattern would give several variants one code, listed once, and the
//! test that no two codes are alike would never see it. Writing the match
//! by hand beside a list kept by hand was the other way, and a list can miss
//! a code its match gives without anything failing.

use std::fmt::Display;

use serde::Serialize;

use crate::activate::{ActivateError, UnknownShell};
use crate::adopt::AdoptError;
use crate::agent::UnknownAgent;
use crate::agents_md::{self, AgentsMdError};
use crate::artifact::{ArtifactError, ValidationIssue};
use crate::browse::{BrowseError, BrowseFailure};
use crate::catalog::DiscoveryFailure;
use crate::claude_mcp::{self, ClaudeMcpError};
use crate::claude_md::{self, ClaudeMdError, Fallback};
use crate::claude_rules::{self, ClaudeRulesError};
use crate::codex_mcp::{self, CodexMcpError};
use crate::config_entry::{EntryError, FileError};
use crate::contents::ContentsError;
use crate::create::CreateError;
use crate::doctor::{DoctorFailure, Symptom};
use crate::everywhere::EverywhereError;
use crate::exclude::{self, ExcludeError};
use crate::frontmatter::FrontMatterError;
use crate::gather::{self, GatherError, Leftover, ScrubFailure};
use crate::http::{CacheError, FetchError};
use crate::install::{
    rules, Clash, ConfigUnread, InstallError, LoadFailure, PlanError, Refused, Respelled,
};
use crate::kind::UnknownKind;
use crate::ledger::LedgerError;
use crate::lifecycle::LifecycleError;
use crate::lock::LockError;
use crate::mcp::{Inexpressible, McpError};
use crate::migrate::{MigrateError, NamesFailure, ProjectFailure, Refusal};
use crate::opencode_mcp::{self, OpenCodeMcpError};
use crate::policy::PolicyError;
use crate::port::{GitError, HttpError};
use crate::reapply::{ReapplyError, ReapplyFailure};
use crate::relink::{Duplicate, RelinkError};
use crate::resolve::ResolveError;
use crate::scan::ScanFailure;
use crate::status::StatusFailure;
use crate::template::TemplateError;
use crate::update::{Left, UpdateError};
use crate::value::Redacted;
use crate::workspace::{MisplacedTarget, ProjectWarning, WorkspaceError};

/// Give an error type its `code()` and its `CODES`.
///
/// An enum lists each variant with a code of its own, as a pattern on
/// `Self`, and then, after `carried`, each that takes the code of the error
/// it carries, with the binding that holds it:
///
/// ```text
/// codes!(LedgerError {
///     Self::Open { .. } => "ledger_open_failed",
/// } carried {
///     Self::Lock(error) => error,
/// });
/// ```
///
/// Each arm is `Self::` and one variant's name, with its fields in braces or
/// parentheses when it has any: what an arm may say is spelled out here
/// rather than taken as any pattern, so that `_` or `Self::A | Self::B`,
/// which would give several variants one code and list it once, does not
/// compile.
///
/// A struct, which is one condition, takes one code:
/// `codes!(UnknownKind => "unknown_kind");`. An enum given one that way
/// does not compile either.
///
/// Exported for a front end's own errors, which reach the panel beside
/// core's and are held to the same rules: the CLI's, which also say some of
/// core's conditions in words of their own, without the error core gave.
/// Each of those is listed after `echoed` with the code core gives the
/// condition, which `code()` gives and `CODES` leaves out, being core's, and
/// `ECHOED` lists for a test to find among core's (`code::ALL`):
///
/// ```text
/// codes!(CliError {
///     Self::NoShell => "no_shell",
/// } carried {
///     Self::Workspace(error) => error,
/// } echoed {
///     Self::NotManaged { .. } => "not_managed",
/// });
/// ```
#[macro_export]
macro_rules! codes {
    ($error:ident {
        $(Self::$variant:ident $(($($tuple:tt)*))? $({$($named:tt)*})? => $code:literal,)*
    } $(carried {
        $(Self::$carrier:ident $(($($carrier_tuple:tt)*))? $({$($carrier_named:tt)*})?
            => $carried:ident,)*
    })? $(echoed {
        $(Self::$echo:ident $(($($echo_tuple:tt)*))? $({$($echo_named:tt)*})?
            => $echoed:literal,)*
    })?) => {
        impl $error {
            /// The word for what went wrong, unique among every code core
            /// gives and never changed once given: what a front end acts
            /// on, where the message may be reworded.
            pub fn code(&self) -> &'static str {
                match self {
                    $(Self::$variant $(($($tuple)*))? $({$($named)*})? => $code,)*
                    $($(
                        Self::$carrier $(($($carrier_tuple)*))? $({$($carrier_named)*})?
                            => $carried.code(),
                    )*)?
                    $($(
                        Self::$echo $(($($echo_tuple)*))? $({$($echo_named)*})? => $echoed,
                    )*)?
                }
            }

            /// Every code this type gives of its own, leaving out those
            /// it takes from an error it carries, or echoes: what
            /// `mindweaver_core::code::ALL` gathers for core's.
            pub const CODES: &'static [&'static str] = &[$($code),*];

            $(
                /// The codes of core's conditions this type says in words of
                /// its own, each one core gives already.
                pub const ECHOED: &'static [&'static str] = &[$($echoed),*];
            )?
        }
    };
    ($error:ident => $code:literal) => {
        impl $error {
            /// The word for what went wrong, unique among every code core
            /// gives and never changed once given: what a front end acts
            /// on, where the message may be reworded.
            pub fn code(&self) -> &'static str {
                // A pattern only a struct matches, so that an enum given
                // one code for all its variants does not compile.
                let $error { .. } = self;
                $code
            }

            /// The one code this type gives: what
            /// `mindweaver_core::code::ALL` gathers for core's.
            pub const CODES: &'static [&'static str] = &[$code];
        }
    };
}

pub(crate) use codes;

/// Every type core gives codes to, by the module it is defined in, with the
/// codes it gives of its own.
///
/// What the test that no two codes are alike reads, and what a front end
/// that lists every code core can give, the panel's types among them (#70),
/// starts from. A new error type joins it; a test finds one that did not.
pub const ALL: &[(&str, &[&str])] = &[
    ("activate::ActivateError", ActivateError::CODES),
    ("activate::UnknownShell", UnknownShell::CODES),
    ("adopt::AdoptError", AdoptError::CODES),
    ("agent::UnknownAgent", UnknownAgent::CODES),
    ("agents_md::AgentsMdError", AgentsMdError::CODES),
    ("agents_md::Malformed", agents_md::Malformed::CODES),
    ("agents_md::Warning", agents_md::Warning::CODES),
    ("artifact::ArtifactError", ArtifactError::CODES),
    ("artifact::ValidationIssue", ValidationIssue::CODES),
    ("browse::BrowseError", BrowseError::CODES),
    ("browse::BrowseFailure", BrowseFailure::CODES),
    ("catalog::DiscoveryFailure", DiscoveryFailure::CODES),
    ("claude_mcp::ClaudeMcpError", ClaudeMcpError::CODES),
    ("claude_mcp::Warning", claude_mcp::Warning::CODES),
    ("claude_md::ClaudeMdError", ClaudeMdError::CODES),
    ("claude_md::Fallback", Fallback::CODES),
    ("claude_md::Warning", claude_md::Warning::CODES),
    ("claude_rules::ClaudeRulesError", ClaudeRulesError::CODES),
    ("claude_rules::Warning", claude_rules::Warning::CODES),
    ("codex_mcp::CodexMcpError", CodexMcpError::CODES),
    ("codex_mcp::Warning", codex_mcp::Warning::CODES),
    ("config_entry::EntryError", EntryError::CODES),
    ("config_entry::file::FileError", FileError::CODES),
    ("contents::ContentsError", ContentsError::CODES),
    ("create::CreateError", CreateError::CODES),
    ("doctor::DoctorFailure", DoctorFailure::CODES),
    ("doctor::Symptom", Symptom::CODES),
    ("everywhere::EverywhereError", EverywhereError::CODES),
    ("exclude::ExcludeError", ExcludeError::CODES),
    ("exclude::Malformed", exclude::Malformed::CODES),
    ("frontmatter::FrontMatterError", FrontMatterError::CODES),
    ("gather::Failure", gather::Failure::CODES),
    ("gather::GatherError", GatherError::CODES),
    ("gather::Leftover", Leftover::CODES),
    ("gather::ScrubFailure", ScrubFailure::CODES),
    ("port::GitError", GitError::CODES),
    ("port::HttpError", HttpError::CODES),
    ("http::FetchError", FetchError::CODES),
    ("http::cache::CacheError", CacheError::CODES),
    ("install::InstallError", InstallError::CODES),
    ("install::entries::ConfigUnread", ConfigUnread::CODES),
    ("install::LoadFailure", LoadFailure::CODES),
    ("install::plan::Clash", Clash::CODES),
    ("install::plan::PlanError", PlanError::CODES),
    ("install::Refused", Refused::CODES),
    ("install::Respelled", Respelled::CODES),
    ("install::rules::Warning", rules::Warning::CODES),
    ("install::rules::Withheld", rules::Withheld::CODES),
    ("kind::UnknownKind", UnknownKind::CODES),
    ("ledger::LedgerError", LedgerError::CODES),
    ("lifecycle::LifecycleError", LifecycleError::CODES),
    ("lock::LockError", LockError::CODES),
    ("mcp::McpError", McpError::CODES),
    ("mcp::expressible::Inexpressible", Inexpressible::CODES),
    ("migrate::MigrateError", MigrateError::CODES),
    ("migrate::Refusal", Refusal::CODES),
    ("migrate::project::ProjectFailure", ProjectFailure::CODES),
    ("migrate::names::NamesFailure", NamesFailure::CODES),
    ("opencode_mcp::OpenCodeMcpError", OpenCodeMcpError::CODES),
    ("opencode_mcp::Warning", opencode_mcp::Warning::CODES),
    ("policy::PolicyError", PolicyError::CODES),
    ("reapply::ReapplyError", ReapplyError::CODES),
    ("reapply::ReapplyFailure", ReapplyFailure::CODES),
    ("relink::Duplicate", Duplicate::CODES),
    ("relink::RelinkError", RelinkError::CODES),
    ("resolve::ResolveError", ResolveError::CODES),
    ("scan::ScanFailure", ScanFailure::CODES),
    ("status::StatusFailure", StatusFailure::CODES),
    ("template::TemplateError", TemplateError::CODES),
    ("update::Left", Left::CODES),
    ("update::UpdateError", UpdateError::CODES),
    ("workspace::MisplacedTarget", MisplacedTarget::CODES),
    ("workspace::ProjectWarning", ProjectWarning::CODES),
    ("workspace::WorkspaceError", WorkspaceError::CODES),
];

/// Every code core gives ([`ALL`]), as one type a description of what a
/// front end is handed names: the union of them in the panel's TypeScript,
/// and the strings they are in the API's schema.
///
/// A type with nothing in it, since what it stands for is the list, and
/// here rather than beside either description, so that both name the same
/// one.
#[cfg(any(feature = "ts", feature = "schema"))]
pub struct CoreCode;

/// An error as a serialized type holds one: its code, for a front end to act
/// on, and its message, for whoever reads it.
///
/// Built where the type is serialized, or kept in what a front end reads
/// (the workspace's snapshot), in place of the error, since an error
/// holds what JSON has no shape for (an `io::Error`, a parser's own), and
/// only these two are meant to leave core. Its fields are public for a
/// front end that reads the snapshot as Rust rather than as JSON, the TUI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Said {
    pub code: &'static str,
    /// Written through [`redacted`](crate::url::redacted), as every URL
    /// is: an error can quote one, a source's among them, and what built
    /// the error is no promise that it carries no credential.
    pub message: Redacted,
}

impl Said {
    /// `error`, by `code`, the one its `code()` gives.
    pub(crate) fn of(code: &'static str, error: &impl Display) -> Self {
        Said {
            code,
            message: Redacted::new(&error.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "code_test.rs"]
mod tests;
