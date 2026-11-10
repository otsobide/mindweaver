//! The engine every Mindweaver front end is built on.
//!
//! Mindweaver works at two levels. A **mind project** carries a `.mind`, the
//! way a repository carries a `.git`, and holds skills and rules in the
//! directories its marker names (`skills/` and `rules/` by default). A
//! **weaver workspace** carries a `.mindweaver` that references the mind
//! projects it manages, so their artifacts can be handled together. Both live
//! in [`workspace`], and what a Mindflayer build wrote under its own names is
//! read beside them ([`names`]).
//!
//! Nothing in here knows about a terminal or a window. The CLI in `apps/cli`,
//! and whatever front end comes after it, decides how to render what these
//! types return, so the two can never disagree about what a skill is.

pub mod activate;
pub mod adapter;
pub mod adopt;
pub mod agent;
pub mod agents_md;
pub mod artifact;
pub mod assign;
pub mod backup;
pub mod browse;
pub mod catalog;
pub mod claude_mcp;
pub mod claude_md;
pub mod claude_rules;
pub mod code;
pub mod codex_mcp;
pub mod config_entry;
pub mod contents;
pub mod copy;
pub mod create;
#[cfg(any(feature = "ts", feature = "schema"))]
pub mod described;
pub mod doctor;
pub mod draft;
pub mod everywhere;
pub mod exclude;
pub mod export;
pub mod finder;
pub mod follow;
pub mod frontmatter;
pub mod gather;
pub mod hash;
pub mod http;
pub mod install;
pub mod kind;
pub mod known;
pub mod ledger;
pub mod licence;
pub mod lifecycle;
mod lines;
pub mod lock;
pub mod marketplace;
pub mod mcp;
pub mod migrate;
#[cfg(any(test, feature = "mother"))]
pub mod mother;
pub mod names;
pub mod opencode_mcp;
pub mod paths;
pub mod policy;
pub mod port;
pub mod reading;
pub mod reapply;
pub mod relink;
pub mod resolve;
pub mod rule;
pub mod scan;
#[cfg(feature = "schema")]
pub mod schema;
pub mod settings;
pub mod skill;
pub mod sources;
pub mod status;
pub mod template;
#[cfg(feature = "ts")]
pub mod typescript;
pub mod update;
pub mod url;
pub mod value;
pub mod view;
pub mod watch;
pub mod workspace;

pub use activate::{ActivateError, Shell, UnknownShell};
pub use agent::{Agent, AgentHomes, AgentTarget, Starts, TargetTable, UnknownAgent};
pub use artifact::{Artifact, ArtifactCopy, ArtifactError, Declared, Provenance, ValidationIssue};
pub use catalog::{Catalog, DiscoveryFailure, Reference, QUALIFIER};
pub use create::{create, create_from, CreateError};
pub use exclude::{ExcludeError, ExcludeFile, Excluded};
pub use frontmatter::{Document, FrontMatterError};
pub use gather::{gather, GatherError, Report, Request, Source, DEFAULT_SUBDIRECTORY};
pub use install::{install, survey, uninstall, Candidate, InstallError, Standing};
pub use kind::{Kind, Layout, UnknownKind};
pub use known::{Known, KnownWorkspace};
pub use ledger::{Ledger, LedgerError, SourceKind, FLAYER_LEDGER_FILE, LEDGER_FILE};
pub use licence::Licence;
pub use lifecycle::{LifecycleError, Target};
pub use lock::{LockError, WorkspaceLock};
pub use mcp::{McpDefinition, McpError, MCP_FORMAT};
pub use names::Names;
pub use port::{Clock, Http, Remote};
pub use reapply::{Change, ReapplyError};
pub use resolve::{resolve, resolve_except, Inputs, Intent, Reason, ResolveError, Resolved};
pub use rule::{Lenient, RuleManifest};
pub use scan::{scan, Found, Scan, ScanFailure};
pub use settings::{ProjectId, Settings, Share, Targets, SETTINGS_FILE, SETTINGS_JSON_FILE};
pub use skill::{
    Compatibility, Metadata, SkillManifest, MAX_COMPATIBILITY_LEN, MAX_DESCRIPTION_LEN,
    MAX_NAME_SEGMENT_LEN,
};
pub use status::{ArtifactStatus, Missed, ProjectStatus, State, Status, StatusFailure, Unreached};
pub use template::{Template, TemplateError, TEMPLATES_DIR};
pub use workspace::{
    Directories, Initialization, Loaded, Marker, MarkerFile, Member, MindConfig, MindProject,
    Misplaced, MisplacedTarget, ProjectConfig, ProjectWarning, Registration, WeaverConfig,
    WeaverWorkspace, WorkspaceError, CACHE_DIR, DIRECTORIES_VERSION, FLAYER_CONFIG, FLAYER_DIR,
    FLAYER_MIND_IGNORE, LIBRARY_DIR, MIND_CONFIG, MIND_DIR, MIND_IGNORE, SETTINGS_TOML_VERSION,
    SETTINGS_VERSION, TARGETS_VERSION, WEAVER_CONFIG, WEAVER_DIR,
};
