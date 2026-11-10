//! Object mothers: factories of the domain's values and aggregates for
//! tests, each made the way its rule allows, so that a test says only what
//! it is about and leaves the rest to a mother.
//!
//! Every mother is a unit struct named for what it makes (`ContentHashMother`
//! makes a [`ContentHash`](crate::value::ContentHash)), with `random()` for
//! one that holds whatever a test does not care about, `create(..)` for one
//! holding exactly what it is given, and a named constructor for each shape a
//! test asks for by name. A mother of an aggregate is made of the mothers of
//! the values it holds, as the aggregate is made of them, so a value's rule
//! changes in one place and every test that builds one follows.
//!
//! Random rather than fixed, so a run of the suite tries values no test
//! spells, which is what catches a change that breaks a case nobody thought
//! of: a test drawing from a mother names the value in what it asserts, so a
//! failure says what it was given. What is random is drawn from `std`'s
//! hasher keys ([`random`]), with no dependency kept for it.
//!
//! Compiled for core's own tests and, under the `mother` feature, for the
//! integration tests of core and of the crates beside it, which reach it
//! through a dev-dependency: never into a binary. It lives in `src` rather
//! than beside the integration suites, as the skeleton it follows keeps its
//! mothers, because core's unit tests are compiled inside the crate and would
//! otherwise need a second copy of every one.

pub mod artifact;
pub mod fixture;
pub mod ledger;
pub mod random;
pub mod registry;
pub mod settings;
pub mod url;
pub mod value;

pub use artifact::{McpMother, RuleMother, SkillMother};
pub use fixture::{
    GitRepositoryMother, MindflayerMother, ProjectMother, ShelfMother, WorkspaceMother,
};
pub use ledger::{
    EntryMother, EntryTargetMother, GatheredMother, InstallRecordMother, LicenceMother,
    MovedMother, SourceMother, TakenMother, TargetMother,
};
pub use registry::RegistryMother;
pub use settings::SettingsMother;
pub use url::{Credentialed, UrlMother};
pub use value::{
    ArtifactNameMother, ContentHashMother, InputNameMother, OriginMother, PinMother,
    ProjectIdMother, RedactedMother, RevisionMother, UpstreamNameMother, VariableNameMother,
};

#[cfg(test)]
#[path = "mother_test.rs"]
mod tests;
