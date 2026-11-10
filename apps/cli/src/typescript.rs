//! What the panel sends the CLI's backend and is answered with, in
//! TypeScript (#70): the `ts` feature's, as core's types are core's
//! (`mindweaver_core::typescript`).
//!
//! Derived, every type serde reads or writes: a query, a task and what a
//! run produced go through serde's derive, which ts-rs reads, so none needs
//! words of its own, and what they name of core's, a kind, an agent, what
//! went wrong, is written as core writes it. Only `Code`, which serde never
//! sees, is written by hand.

use std::path::PathBuf;

use mindweaver_core::code::CoreCode;
use mindweaver_core::typescript::{docs, lines};
use mindweaver_server::Condition;
use ts_rs::{Config, TypeVisitor, TS};

use crate::backend::{Confirm, Outcome, Planned, Query, Scanned, Switch};
use crate::error::Code;
use crate::progress::Step;
use crate::render::Envelope;
use crate::task::Task;
use crate::CODES;

/// Every type the CLI hands the panel whole, or takes from it, each with
/// every type it names: a query, a task and a switch, which the panel
/// sends; what a scan, a plan, a preview and a run are answered with, the
/// last two what to confirm and what it produced; each step a run under
/// way reports, which the stream carries; the envelope a reading command's
/// `--json` prints; and every code core, the CLI and the server give.
pub fn roots(visitor: &mut impl TypeVisitor) {
    visitor.visit::<Query>();
    visitor.visit::<Task>();
    visitor.visit::<Scanned>();
    visitor.visit::<Planned>();
    visitor.visit::<Confirm>();
    visitor.visit::<Outcome>();
    visitor.visit::<Step>();
    visitor.visit::<Switch>();
    visitor.visit::<Envelope<'static, ()>>();
    visitor.visit::<Code>();
}

/// Every code what a front end hands on can hold, as the union of them:
/// core's ([`CoreCode`]), the CLI's own ([`CODES`]), which reach the panel
/// beside core's in what a run says and in a refusal, and the server's
/// ([`Condition::CODES`]), which every answer under `/api` it gives of its
/// own carries: so any code a refusal hands the panel is one of these.
///
/// Not the type of a warning's `code`, which stays a `string`, as a
/// `Said`'s does: a union is a promise a test of the codes themselves
/// keeps, and no test holds every line a command says to a code listed.
impl TS for Code {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;

    fn docs() -> Option<String> {
        Some(docs(&[
            " Every code a front end is handed for what went wrong: core's, the CLI's",
            " own and the local server's beside them. Unique across the three, and",
            " never renamed once given.",
        ]))
    }

    fn name(_: &Config) -> String {
        "Code".to_owned()
    }

    fn decl(config: &Config) -> String {
        format!("type Code ={};", Self::inline(config))
    }

    fn decl_concrete(config: &Config) -> String {
        Self::decl(config)
    }

    fn inline(config: &Config) -> String {
        lines(
            std::iter::once(CoreCode::name(config)).chain(
                CODES
                    .iter()
                    .flat_map(|(_, codes)| codes.iter())
                    .chain(Condition::CODES)
                    .map(|code| format!("{code:?}")),
            ),
        )
    }

    fn visit_dependencies(visitor: &mut impl TypeVisitor)
    where
        Self: 'static,
    {
        visitor.visit::<CoreCode>();
    }

    fn output_path() -> Option<PathBuf> {
        Some("Code.ts".into())
    }
}

#[cfg(test)]
#[path = "typescript_test.rs"]
mod tests;
