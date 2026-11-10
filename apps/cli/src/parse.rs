//! The command lines `mind` and `mindweaver` take, as clap types.
//!
//! Only the grammar, apart from the work, so the whole surface reads in one
//! place. The binaries parse with these types and so does every line the TUI
//! runs, which is why a feature cannot reach the TUI without a command.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use clap::builder::{PossibleValuesParser, StringValueParser, TypedValueParser};
use clap::error::ErrorKind;
use clap::{ArgGroup, Args, Parser, Subcommand};
use mindweaver_core::mcp::{self, Address, Program};
use mindweaver_core::scan::DEFAULT_DEPTH;
use mindweaver_core::value::{InputName, VariableName};
use mindweaver_core::{Agent, Kind, Share, Shell, DEFAULT_SUBDIRECTORY};

/// Manage the agent skills in a mind project.
//
// `about` is spelled out rather than taken from the crate description, which
// is one line for a package that ships two binaries and so can only be right
// for one of them. `propagate_version` puts `--version` on the nested
// subcommands too, so `mind weaver --version` answers instead of erroring
// while `mindweaver --version` works.
#[derive(Debug, Parser)]
#[command(
    name = "mind",
    version,
    propagate_version = true,
    about = "Manage the agent skills in a mind project",
    long_about = None
)]
pub struct Cli {
    /// What to do [default: open the TUI on this project].
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Directory to work in [default: the current directory].
    ///
    /// Named `directory` rather than `path` on purpose: a global argument
    /// shares a namespace with every subcommand's own arguments, and `path`
    /// is exactly what a subcommand taking one would call it.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    pub directory: Option<PathBuf>,

    /// Where the default workspace lives, used when none is found above
    /// [default: $MINDWEAVER_HOME, then your home directory; empty for none].
    #[arg(long = "home", value_name = "DIR", global = true)]
    pub home: Option<String>,

    // A string rather than a path for the reason `home` is one: empty is an
    // answer, and a path cannot spell it.
    /// The workspace to use, by its directory or its `.mindweaver`, wherever
    /// you are [default: $MINDWEAVER_WORKSPACE, then the one at or above
    /// here, then the default one; empty to name none and ignore
    /// $MINDWEAVER_WORKSPACE].
    #[arg(short = 'W', long = "workspace", value_name = "DIR", global = true)]
    pub workspace: Option<String>,

    #[command(flatten)]
    pub agent_homes: AgentHomeOptions,
}

/// Where each agent keeps what it reads for every project: global options on
/// both binaries, which `agent_homes` decides beside `default_home`.
///
/// Options, rather than variables read where a check needs them, so a test
/// can pin all four to none the way it pins `--home=`. Strings for the same
/// reason `--home` is one: empty means none, and a path cannot spell it.
//
// Each field is named after its flag rather than after its agent: a global
// argument shares a namespace with every subcommand's own arguments, and
// `agents` is what an option naming a project's agents would be called.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
#[command(next_help_heading = "Agent homes")]
pub struct AgentHomeOptions {
    /// Claude Code's configuration folder [default: $CLAUDE_CONFIG_DIR, then
    /// ~/.claude; empty for none].
    #[arg(long = "claude-config-dir", value_name = "DIR", global = true)]
    pub claude_config_dir: Option<String>,

    /// Codex's home [default: $CODEX_HOME, then ~/.codex; empty for none].
    #[arg(long = "codex-home", value_name = "DIR", global = true)]
    pub codex_home: Option<String>,

    /// OpenCode's configuration folder [default: $OPENCODE_CONFIG_DIR, then
    /// ~/.config/opencode; empty for none].
    #[arg(long = "opencode-config-dir", value_name = "DIR", global = true)]
    pub opencode_config_dir: Option<String>,

    /// The folder several agents share for your own skills [default:
    /// ~/.agents; empty for none].
    #[arg(long = "agents-dir", value_name = "DIR", global = true)]
    pub agents_dir: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Create a mind project here.
    Init {
        /// The agents that work in this project, comma-separated [default:
        /// claude,codex,opencode].
        ///
        /// What is installed into the project is put where each of them
        /// reads it.
        //
        // Every agent by default, so a project made without thinking about
        // it reaches whichever one is opened in it. The default is spelled
        // in the text because clap would show it space-separated, which is
        // not how the option takes it.
        #[arg(
            long,
            value_name = "AGENTS",
            value_delimiter = ',',
            value_parser = agents(),
            default_values_t = Agent::ALL,
            hide_default_value = true
        )]
        agents: Vec<Agent>,

        /// Leave `.mind` for the team to commit, and what is installed into
        /// the project with it, rather than keeping both out of git.
        #[arg(long)]
        shared: bool,

        /// Where this project keeps its skills [default: `skills`].
        ///
        /// Relative to the project. Point it wherever the agents that read
        /// them already look, such as `.claude/skills`.
        #[arg(long, value_name = "DIR")]
        skills: Option<PathBuf>,

        /// Where this project keeps its rules [default: `rules`].
        #[arg(long, value_name = "DIR")]
        rules: Option<PathBuf>,

        /// Give the project here an id of its own, keeping everything else:
        /// for a copy made with its `.mind`, which holds the id of the
        /// project it was copied from.
        //
        // An option of `init` rather than a command of its own: the id is
        // what `init` gave the project, and this gives it again.
        #[arg(long, conflicts_with_all = ["agents", "shared", "skills", "rules"])]
        new_id: bool,
    },

    /// Create a skill, a rule or an MCP server in this project.
    Add {
        /// What to create: `skill`, `rule` or `mcp`.
        kind: Kind,

        /// Its name: `deploy`, or a route like `git/no-force-push` for a rule.
        name: String,

        /// One line saying what it is: a skill's description, a rule's
        /// opening line, what an MCP server is for.
        description: String,

        /// Start from a template instead: `full`, or one of yours in
        /// `.mind/templates` or the workspace's `.mindweaver/templates`.
        #[arg(long, value_name = "NAME", conflicts_with = "ServerOptions")]
        template: Option<String>,

        // Boxed, being most of what a line can say: every other command
        // would otherwise be as large.
        #[command(flatten)]
        server: Box<ServerOptions>,
    },

    /// Open one artifact in your editor, or replace what it holds.
    Edit {
        /// Its name, or `kind:name` when one name belongs to two kinds.
        reference: String,

        #[command(flatten)]
        editing: Editing,
    },

    /// Give one artifact a new name, on disk and in what it declares.
    #[command(alias = "mv")]
    Rename {
        /// Its name now, or `kind:name`.
        reference: String,

        /// The name it should have.
        to: String,
    },

    /// Delete one artifact from this project.
    #[command(alias = "rm")]
    Remove {
        /// Its name, or `kind:name`.
        reference: String,

        /// Delete it. Without this nothing is deleted, and the command says
        /// what would be.
        #[arg(short, long)]
        yes: bool,
    },

    /// List the templates `mind add --template` can start from here.
    Templates {
        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// List this project's artifacts.
    #[command(alias = "ls")]
    List {
        /// Only this kind: `skills`, `rules` or `mcp` [default: every kind].
        kind: Option<Kind>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Show one artifact in full.
    Show {
        /// Its name, or `kind:name` when one name belongs to two kinds.
        reference: String,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Check this project's artifacts against what an agent requires.
    Validate {
        /// A kind (`rules`), or one artifact by name [default: everything].
        target: Option<String>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Say what this project holds: whether what Mindweaver installed is
    /// current, outdated, edited, missing or orphaned, what is somebody's
    /// and what is the project's own, which agents read each, and where
    /// what was installed came from.
    Status {
        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Find what keeps this project's agents from what it gives them,
    /// which no agent says: a Codex project nothing trusts, an
    /// AGENTS.override.md, a CLAUDE.local.md, a skill at user scope, an
    /// MCP server's variable that is not set. Reads files alone, and fixes
    /// nothing.
    Doctor {
        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Register this project with the weaver workspace above it.
    Link,

    /// Drop this project from the weaver workspace above it.
    Unlink,

    /// Offer other weaver workspaces' own skills and rules to the workspace
    /// above this project, read live from where they are; on its own, list
    /// what it loads.
    Load {
        /// Each workspace's directory, the one holding `.mindweaver`
        /// [default: list the loaded ones].
        #[arg(value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Stop the workspace above this project offering a loaded workspace's.
    Unload {
        /// Each workspace's directory, as loaded. It need not still exist.
        #[arg(required = true, value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Put a skill, a rule or an MCP server into this project: one of the
    /// workspace's own, a loaded workspace's, or one from its shelf.
    Install {
        /// Its name: a skill's, a server's, or a rule's route like
        /// `git/commits`; `skill:<name>`, `rule:<route>` or `mcp:<name>`
        /// when one name is two kinds.
        name: String,

        /// Where it comes from, when two places offer that name: `workspace`,
        /// a shelf source's name (as `gather list` shows it) or URL, or a
        /// loaded workspace's path or name.
        #[arg(long, value_name = "ORIGIN")]
        from: Option<String>,

        /// Write over a copy changed since it was installed, which is
        /// otherwise left alone.
        #[arg(long)]
        force: bool,

        /// Install an MCP server that comes from elsewhere than the
        /// workspace's own library. Without this nothing is written, and
        /// the command says what each agent would run.
        #[arg(short, long)]
        yes: bool,
    },

    /// Take a skill, a rule or an MCP server Mindweaver installed back out
    /// of this project.
    Uninstall {
        /// Its name, or `kind:name` when one name is two kinds.
        #[arg(required_unless_present = "orphaned")]
        name: Option<String>,

        /// Every installation nothing offers any more where it came from,
        /// instead of one by name.
        #[arg(long, conflicts_with = "name")]
        orphaned: bool,

        /// Take out a copy changed since it was installed too, which is
        /// otherwise left alone with every other copy.
        #[arg(long)]
        force: bool,
    },

    /// Bring what Mindweaver installed into this project up to date with
    /// where it came from, and put back the copies that have gone.
    ///
    /// A copy changed since it was installed is left alone, with every
    /// other copy of it, and said. Without --yes nothing changes, and the
    /// command says what would.
    Update {
        /// Only these, each by its name, or `kind:name` when one name is
        /// both [default: everything installed].
        #[arg(value_name = "NAME")]
        names: Vec<String>,

        /// Write over a copy changed since it was installed, which is
        /// otherwise left alone.
        #[arg(long)]
        force: bool,

        /// Make the change. Without this nothing changes, and the command
        /// says what would.
        #[arg(short, long)]
        yes: bool,
    },

    /// Change which agents work in this project, and carry what is
    /// installed into it along: a copy wherever an agent it now names
    /// reads, and none left where no agent it names does.
    ///
    /// `mind agents claude codex` names exactly those; `add` and `remove`
    /// change the ones it names. Without --yes nothing changes, and the
    /// command says what would.
    //
    // The usage is spelled out because clap's would offer agents before
    // `add` too, which it then refuses.
    #[command(
        subcommand_negates_reqs = true,
        override_usage = "mind agents [OPTIONS] <AGENT>...\n       mind agents <add|remove> [OPTIONS] <AGENT>..."
    )]
    Agents {
        #[command(subcommand)]
        change: Option<AgentsChange>,

        #[command(flatten)]
        named: AgentNames,
    },

    /// Keep what is installed into this project to this checkout, or leave
    /// it for the team to commit, with `.mind` itself.
    ///
    /// Without --yes nothing changes, and the command says what would.
    Share {
        #[command(flatten)]
        mode: ShareMode,
    },

    /// Install a kind into folders of this project's own, for every agent
    /// it names, instead of where each agent reads, or send it back there,
    /// and carry what is installed along.
    ///
    /// A folder no agent reads reaches one only once it is pointed there.
    /// Without --yes nothing changes, and the command says what would.
    Targets {
        #[command(subcommand)]
        change: TargetsChange,
    },

    /// Bring this project, made by an older Mindweaver and marked by a
    /// mind.toml, to a settings.toml, and what is installed into it to where
    /// its agents read; or one marked by a settings.json to a settings.toml
    /// saying the same.
    ///
    /// Without --yes nothing changes, and the command says what would.
    Migrate {
        /// Move the workspace this project's commands use, made by a
        /// Mindflayer build, to Mindweaver's names, as `mindweaver migrate
        /// --names` does there.
        #[arg(long, conflicts_with_all = ["agents", "local", "shared"])]
        names: bool,

        #[command(flatten)]
        settings: Migrated,

        /// Make the change. Without this nothing changes, and the command
        /// says what would.
        #[arg(short, long)]
        yes: bool,
    },

    /// Open the TUI on this project. What `mind` does on its own.
    Tui,

    /// Open the control panel in a browser, over the workspace a command run
    /// here would use: what `mindweaver ui` does.
    #[cfg(feature = "ui")]
    Ui(UiOptions),

    /// Act on the weaver workspace instead. Also reachable as `mindweaver <cmd>`.
    ///
    /// On its own it opens the TUI, as `mindweaver` on its own does.
    Weaver {
        #[command(subcommand)]
        command: Option<WeaverCommand>,
    },
}

/// The `mindweaver` binary: a shortcut into the workspace half of `mind`.
#[derive(Debug, Parser)]
#[command(
    name = "mindweaver",
    version,
    propagate_version = true,
    about = "Manage the mind projects a weaver workspace orchestrates",
    long_about = None
)]
pub struct WeaverCli {
    /// What to do [default: open the TUI].
    #[command(subcommand)]
    pub command: Option<WeaverCommand>,

    /// Directory to work in [default: the current directory].
    ///
    /// Named `directory` rather than `path` on purpose: a global argument
    /// shares a namespace with every subcommand's own arguments, and `path`
    /// is exactly what a subcommand taking one would call it.
    #[arg(short = 'C', long = "directory", value_name = "DIR", global = true)]
    pub directory: Option<PathBuf>,

    /// Where the default workspace lives, used when none is found above
    /// [default: $MINDWEAVER_HOME, then your home directory; empty for none].
    #[arg(long = "home", value_name = "DIR", global = true)]
    pub home: Option<String>,

    // A string rather than a path for the reason `home` is one: empty is an
    // answer, and a path cannot spell it.
    /// The workspace to use, by its directory or its `.mindweaver`, wherever
    /// you are [default: $MINDWEAVER_WORKSPACE, then the one at or above
    /// here, then the default one; empty to name none and ignore
    /// $MINDWEAVER_WORKSPACE].
    #[arg(short = 'W', long = "workspace", value_name = "DIR", global = true)]
    pub workspace: Option<String>,

    #[command(flatten)]
    pub agent_homes: AgentHomeOptions,
}

#[derive(Debug, Subcommand)]
pub enum WeaverCommand {
    /// Create a weaver workspace here, or in the directory given.
    Init {
        /// The directory to make it in, made as well when it is not there
        /// [default: the directory the command works in].
        ///
        /// So a workspace kept for one purpose can be made from anywhere, the
        /// way `python -m venv DIR` makes a virtualenv, and then activated.
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,

        /// Where the workspace keeps its own skills [default:
        /// `.mindweaver/library/skills`].
        ///
        /// Relative to the workspace. They are the ones it shares with every
        /// project it manages, kept by default in its library, where no
        /// agent looks. Any folder inside the workspace will do except the
        /// shelf and the cache, a folder inside them, or one holding them,
        /// such as `.mindweaver` itself.
        #[arg(long, value_name = "DIR")]
        skills: Option<PathBuf>,

        /// Where the workspace keeps its own rules [default:
        /// `.mindweaver/library/rules`].
        #[arg(long, value_name = "DIR")]
        rules: Option<PathBuf>,
    },

    /// Print the shell code that activates a workspace for every command the
    /// shell runs, as a Python virtualenv's `activate` does:
    /// `eval "$(mindweaver activate DIR)"`. `mindweaver-deactivate` undoes it.
    ///
    /// It writes nothing, and prints no code when there is nothing to
    /// activate, so a failed `eval` changes nothing either.
    Activate {
        /// The workspace, by its directory or its `.mindweaver` [default: the
        /// one a command run here would use].
        #[arg(value_name = "DIR")]
        path: Option<PathBuf>,

        /// The shell to write it for [default: the one $SHELL names].
        #[arg(long, value_name = "SHELL", value_parser = shells())]
        shell: Option<Shell>,
    },

    /// Say which workspace a command run here would use, and why: named with
    /// -W, activated, found walking up, or the default one.
    Which,

    /// List the weaver workspaces this machine knows: the default one, each
    /// one `mindweaver init` has made or been run on since the default was made,
    /// and the one in use, marked with `*`.
    ///
    /// `mindweaver workspaces forget <path>` takes one out of the list.
    #[command(args_conflicts_with_subcommands = true)]
    Workspaces {
        #[command(subcommand)]
        command: Option<WorkspacesCommand>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Create a skill, a rule or an MCP server: the workspace's own, or a
    /// project's.
    Add {
        /// A project this workspace manages, by name or path, instead of the
        /// workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// What to create: `skill`, `rule` or `mcp`.
        kind: Kind,

        /// Its name: `deploy`, or a route like `git/no-force-push` for a rule.
        name: String,

        /// One line saying what it is: a skill's description, a rule's
        /// opening line, what an MCP server is for.
        description: String,

        /// Start from a template instead: `full`, or one in
        /// `.mindweaver/templates` or the project's `.mind/templates`.
        #[arg(long, value_name = "NAME", conflicts_with = "ServerOptions")]
        template: Option<String>,

        // Boxed, being most of what a line can say: every other command
        // would otherwise be as large.
        #[command(flatten)]
        server: Box<ServerOptions>,
    },

    /// Open one of the workspace's own artifacts, or a project's, in your
    /// editor, or replace what it holds.
    Edit {
        /// Its name, or `kind:name` when one name belongs to two kinds.
        reference: String,

        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        #[command(flatten)]
        editing: Editing,
    },

    /// Give one artifact a new name, on disk and in what it declares.
    #[command(alias = "mv")]
    Rename {
        /// Its name now, or `kind:name`.
        reference: String,

        /// The name it should have.
        to: String,

        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,
    },

    /// Delete one of the workspace's own artifacts, or a project's.
    ///
    /// One of the workspace's own installed into its projects leaves its
    /// copies there orphaned; --everywhere takes them out first.
    #[command(alias = "rm")]
    Remove {
        /// Its name, or `kind:name`.
        reference: String,

        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Uninstall it from every project this workspace links first,
        /// leaving a copy changed since it was installed, and somebody's own
        /// of that name.
        #[arg(long, conflicts_with = "project")]
        everywhere: bool,

        /// Delete it. Without this nothing is deleted, and the command says
        /// what would be.
        #[arg(short, long)]
        yes: bool,
    },

    /// List the templates `mindweaver add --template` can start from.
    Templates {
        /// A project this workspace manages, instead of the workspace itself.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// List the workspace's own artifacts and every managed project's.
    #[command(alias = "ls")]
    List {
        /// Only this kind: `skills`, `rules` or `mcp` [default: every kind].
        kind: Option<Kind>,

        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Show one artifact in full, wherever in the workspace it is.
    Show {
        /// Its name, or `kind:name` when one name belongs to two kinds.
        reference: String,

        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Check the workspace's own artifacts, every managed project's, and what
    /// is on the shelf.
    Validate {
        /// A kind (`rules`), or one artifact by name [default: everything].
        target: Option<String>,

        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Say what every managed project holds: whether what Mindweaver
    /// installed is current, outdated, edited, missing or orphaned, what is
    /// somebody's and what is the project's own, which agents read each,
    /// and where what was installed came from.
    Status {
        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Find what keeps each managed project's agents from what it gives
    /// them, which no agent says: a Codex project nothing trusts, an
    /// AGENTS.override.md, a CLAUDE.local.md, a skill at user scope, an
    /// MCP server's variable that is not set. Reads files alone, and fixes
    /// nothing.
    Doctor {
        /// Only this project.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Register one or more mind projects with this workspace.
    Link {
        /// Each project's directory, the one holding `.mind`.
        #[arg(required = true, value_name = "PROJECT")]
        projects: Vec<PathBuf>,
    },

    /// Drop one or more registered mind projects.
    Unlink {
        /// Each project's directory, as registered. It need not still exist.
        #[arg(required = true, value_name = "PROJECT")]
        projects: Vec<PathBuf>,
    },

    /// Follow a project that moved: point its entry at the directory it is
    /// in now, and keep everything installed into it.
    ///
    /// The directory must hold the id the workspace last saw where the
    /// project was, which only the project, or a copy of it, holds:
    /// `mindweaver scan` names each one it finds as `moved`.
    Relink {
        /// The project's directory, as registered, which holds no project
        /// any more.
        #[arg(value_name = "OLD")]
        old: PathBuf,

        /// The directory the project is in now, the one holding `.mind`.
        #[arg(value_name = "NEW")]
        new: PathBuf,
    },

    /// Offer other weaver workspaces' own skills and rules to this
    /// workspace's projects, read live from where they are.
    ///
    /// Into the workspace at or above here that is not one being loaded, or
    /// else the default one in your home: so `mindweaver load .` from inside a
    /// workspace full of skills and rules loads it into the one that manages
    /// you. On its own, it lists what this workspace loads and what each
    /// offers.
    Load {
        /// Each workspace's directory, the one holding `.mindweaver`
        /// [default: list the loaded ones].
        #[arg(value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Stop offering a loaded workspace's skills and rules.
    Unload {
        /// Each workspace's directory, as loaded. It need not still exist.
        #[arg(required = true, value_name = "WORKSPACE")]
        workspaces: Vec<PathBuf>,
    },

    /// Open the minds screen: every mind project and repository under this
    /// workspace, ticked where it is linked, to link and unlink in one pass.
    ///
    /// Given folders, it looks for what to link under those instead, as
    /// `mindweaver scan` does, and still lists every project the workspace links.
    Minds {
        /// Each folder to look under, itself included [default: the
        /// workspace's root].
        #[arg(value_name = "DIR")]
        roots: Vec<PathBuf>,
    },

    /// Look for projects and repositories to link: under this workspace, or
    /// under each folder given.
    ///
    /// It links nothing, and prints each path as `mindweaver link` run from here
    /// takes it.
    Scan {
        /// Each folder to look under, itself included [default: the
        /// workspace's root].
        #[arg(value_name = "DIR")]
        roots: Vec<PathBuf>,

        /// How many levels below each folder to look. However deep, a scan
        /// that has listed too many folders stops, and says so.
        #[arg(long, value_name = "N", default_value_t = DEFAULT_DEPTH)]
        depth: usize,

        /// List what is not linked first, most worth linking first, each
        /// with why: a mind project with an id, then one without, then a git
        /// repository holding files an agent reads, then any other.
        #[arg(long)]
        suggest: bool,

        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },

    /// Collect artifacts from elsewhere onto this workspace's shelf.
    #[command(subcommand)]
    Gather(GatherCommand),

    /// Put skills, rules and MCP servers into the projects this workspace
    /// manages: the workspace's own, a loaded workspace's, or its shelf's.
    ///
    /// On its own it opens a screen for all of them at once. Given a name and
    /// projects, it installs that one into each, without the screen.
    Install {
        /// A project to install into, by name or path; once for each.
        #[arg(short = 'p', long = "project", value_name = "PROJECT")]
        projects: Vec<String>,

        /// Every project the workspace links.
        #[arg(long, conflicts_with = "projects")]
        all_projects: bool,

        /// Its name: a skill's, a server's, or a rule's route like
        /// `git/commits`; `skill:<name>`, `rule:<route>` or `mcp:<name>`
        /// when one name is two kinds [default: open the screen].
        name: Option<String>,

        /// Where it comes from, when two places offer that name: `workspace`,
        /// a shelf source's name (as `gather list` shows it) or URL, or a
        /// loaded workspace's path or name.
        #[arg(long, value_name = "ORIGIN")]
        from: Option<String>,

        /// Write over a copy changed since it was installed, which is
        /// otherwise left alone.
        #[arg(long, requires = "name")]
        force: bool,

        /// Install an MCP server that comes from elsewhere than the
        /// workspace's own library. Without this nothing is written, and
        /// the command says what each agent would run.
        #[arg(short, long, requires = "name")]
        yes: bool,
    },

    /// Take a skill, a rule or an MCP server Mindweaver installed back out
    /// of projects.
    Uninstall {
        /// A project to take it out of, by name or path; once for each.
        #[arg(short = 'p', long = "project", value_name = "PROJECT")]
        projects: Vec<String>,

        /// Every project the workspace links that has it.
        #[arg(long, conflicts_with = "projects")]
        all_projects: bool,

        /// Its name, or `kind:name` when one name is two kinds.
        #[arg(required_unless_present = "orphaned")]
        name: Option<String>,

        /// Every installation nothing offers any more where it came from,
        /// instead of one by name.
        #[arg(long, conflicts_with = "name")]
        orphaned: bool,

        /// Take out a copy changed since it was installed too, which is
        /// otherwise left alone with every other copy.
        #[arg(long)]
        force: bool,
    },

    /// Bring what Mindweaver installed into projects up to date with where
    /// it came from, and put back the copies that have gone, as `mind
    /// update` does in one.
    ///
    /// A copy changed since it was installed is left alone, with every
    /// other copy of it, and said. Without --yes nothing changes, and the
    /// command says what would.
    Update {
        /// A project to update, by name or path; once for each.
        #[arg(short = 'p', long = "project", value_name = "PROJECT")]
        projects: Vec<String>,

        /// Every project the workspace links.
        #[arg(long, conflicts_with = "projects")]
        all: bool,

        /// Only these, each by its name, or `kind:name` when one name is
        /// both [default: everything installed].
        #[arg(value_name = "NAME")]
        names: Vec<String>,

        /// Write over a copy changed since it was installed, which is
        /// otherwise left alone.
        #[arg(long)]
        force: bool,

        /// Make the change. Without this nothing changes, and the command
        /// says what would.
        #[arg(short, long)]
        yes: bool,
    },

    /// Bring a skill somebody keeps in a project's .claude/skills or
    /// .agents/skills into this workspace's library, and record the
    /// project's copies as installed from there, so updates reach them.
    ///
    /// Not a byte of the project is written. Without --yes nothing is, and
    /// the command says what would be.
    Adopt {
        /// The project it is in, by name or path.
        #[arg(short, long, value_name = "PROJECT")]
        project: String,

        /// Its name, as `mindweaver status` lists it `foreign`.
        reference: String,

        /// Adopt it. Without this nothing is written, and the command says
        /// what would be.
        #[arg(short, long)]
        yes: bool,
    },

    /// Change which agents work in a project, and carry what is installed
    /// into it along, as `mind agents` does there.
    ///
    /// Without --yes nothing changes, and the command says what would.
    #[command(
        subcommand_negates_reqs = true,
        override_usage = "mindweaver agents -p <PROJECT> [OPTIONS] <AGENT>...\n       mindweaver agents -p <PROJECT> <add|remove> [OPTIONS] <AGENT>..."
    )]
    Agents {
        /// The project, by name or path. Needed, and taken on either side of
        /// `add` or `remove`.
        #[arg(short, long, value_name = "PROJECT", global = true)]
        project: Option<String>,

        #[command(subcommand)]
        change: Option<AgentsChange>,

        #[command(flatten)]
        named: AgentNames,
    },

    /// Keep what is installed into a project to this checkout, or leave it
    /// for the team to commit, as `mind share` does there.
    ///
    /// Without --yes nothing changes, and the command says what would.
    Share {
        /// The project, by name or path.
        #[arg(short, long, value_name = "PROJECT")]
        project: String,

        #[command(flatten)]
        mode: ShareMode,
    },

    /// Install a kind into folders of a project's own, or send it back to
    /// where each agent reads, as `mind targets` does there.
    ///
    /// Without --yes nothing changes, and the command says what would.
    #[command(
        override_usage = "mindweaver targets -p <PROJECT> set [OPTIONS] <KIND> <DIR>...\n       mindweaver targets -p <PROJECT> reset [OPTIONS] <KIND>"
    )]
    Targets {
        /// The project, by name or path. Needed, and taken on either side of
        /// `set` or `reset`.
        #[arg(short, long, value_name = "PROJECT", global = true)]
        project: Option<String>,

        #[command(subcommand)]
        change: TargetsChange,
    },

    /// Bring a workspace made by an older Mindweaver, or the projects it
    /// links, into the layout a new one has.
    ///
    /// Without --yes nothing changes, and the command says what would.
    //
    // A flag per migration rather than a bare `migrate` that does all of
    // them, so the next one is asked for by name and never run by a line
    // typed for this one; and one at a time, so a line says which it runs.
    // Not required as a group, so a line naming none is told what there is
    // to migrate rather than shown clap's usage.
    #[command(group(
        ArgGroup::new("what")
            .args(["library", "names", "project", "all_projects"])
            .multiple(false)
    ))]
    Migrate {
        /// Move the workspace's own skills and rules from the folders its
        /// marker names into its library, `.mindweaver/library`, and point
        /// the marker there, keeping it as it was beside it.
        #[arg(long, conflicts_with_all = ["agents", "local", "shared"])]
        library: bool,

        /// Move a workspace a Mindflayer build made to Mindweaver's names:
        /// its .mindflayer, flayer.toml and mindflayer.db to .mindweaver,
        /// weaver.toml and mindweaver.db, and in each project it links the
        /// markers Mindflayer wrote and Claude's rules folder.
        //
        // Named for what moves, the names written on the disk, beside
        // `--library`, named for where the workspace's own go: not
        // `--mindweaver`, which names the product and would be wrong again
        // after another rename, nor `--rename`, which reads as the command
        // that renames an artifact.
        #[arg(long, conflicts_with_all = ["agents", "local", "shared"])]
        names: bool,

        /// Move one project the workspace links, by name or path, from its
        /// mind.toml or settings.json to a settings.toml, as `mind migrate`
        /// does there.
        #[arg(short, long, value_name = "PROJECT")]
        project: Option<String>,

        /// Move every project the workspace links that a mind.toml or a
        /// settings.json marks.
        #[arg(long)]
        all_projects: bool,

        #[command(flatten)]
        settings: Migrated,

        /// Make the change. Without this nothing changes, and the command
        /// says what would.
        #[arg(short, long)]
        yes: bool,
    },

    /// Open the TUI: every managed project, and a key for each command.
    ///
    /// What `mindweaver` does on its own. Everything it changes, it changes by
    /// running one of these commands, and it prints them when it closes.
    Tui,

    /// Open the control panel in a browser, served on 127.0.0.1 until
    /// Ctrl-C.
    ///
    /// It prints the link that opens it, which carries the token that lets
    /// the browser in, and opens it. Everything the panel changes, it
    /// changes by running one of these commands, each shown before it runs.
    ///
    /// What the panel asks the server is an HTTP API under /api/v1, there
    /// for scripts on this machine too: with --no-open and --token-file, it
    /// serves them with no browser.
    #[cfg(feature = "ui")]
    Ui(UiOptions),
}

/// How `ui` serves the panel.
#[cfg(feature = "ui")]
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct UiOptions {
    /// The port to listen on, on 127.0.0.1 [default: one the system picks;
    /// 7878 with --dev, where the dev server's proxy looks].
    //
    // An option rather than a number defaulting to 0, so that --dev can
    // tell a port nobody named from one somebody did.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,

    /// Print the link without opening a browser.
    #[arg(long)]
    pub no_open: bool,

    /// Serve the panel through its dev server (`make web/dev`, on
    /// 127.0.0.1:5173): take requests from its pages too, and open the panel
    /// there.
    #[arg(long)]
    pub dev: bool,

    /// Write where the API is and the token to PATH, as JSON, for a script
    /// to read: a new file only your account can read, never one already
    /// there, taken away when the panel stops unless it was changed. With
    /// --no-open, the way to serve the API to scripts with no browser.
    //
    // A path somebody names rather than one of Mindweaver's own, in its
    // home say: several panels may be served at once, each with a token of
    // its own, and only whoever starts one knows which script it is for.
    #[arg(long, value_name = "PATH")]
    pub token_file: Option<PathBuf>,

    /// Say on stderr, as it happens, every request answered and how long
    /// it took, how long each change waited its turn and ran, and how long
    /// each of its commands took, of it waiting for another writer to let
    /// go of a workspace: each line after the time of day, in UTC. A
    /// request is said by its path and query, so a read's line names the
    /// folders and entries it asked for; never the token, a header, nor
    /// the body a change sent, and so not which commands it ran.
    //
    // Its path and query, as the request log has always said them, since
    // which read took long is what the log is read for; the token alone is
    // taken out of them (`log::redact`, in the server).
    //
    // On stderr, which the person who started the panel already reads and
    // can send anywhere, rather than to a file of its own, which would be
    // one more path to refuse when something is there. Off unless asked
    // for: a line a request is too many for a terminal somebody is using.
    #[arg(long)]
    pub log: bool,
}

/// How `edit` changes an artifact, at either level: in an editor, by
/// replacing what it holds with contents given whole, or, for an MCP
/// server, by writing how it is started or reached again from the options
/// `add mcp` takes.
///
/// One struct for both binaries' `edit`, so the two cannot come to take
/// them differently, as [`ShareMode`] is for `share`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
pub struct Editing {
    /// The editor to open it in [default: $VISUAL, then $EDITOR, then vi].
    #[arg(long, value_name = "COMMAND", conflicts_with = "ServerOptions")]
    pub editor: Option<String>,

    /// Replace what it holds (a skill's SKILL.md, or a rule's file) with
    /// what FILE holds, or standard input with `-`, instead of opening an
    /// editor. Nothing is written unless it passes the rules `add` keeps.
    //
    // A path rather than the text itself, which a command line holds badly
    // and a process listing shows to every user.
    #[arg(
        long,
        value_name = "FILE",
        conflicts_with_all = ["editor", "ServerOptions"]
    )]
    pub contents: Option<PathBuf>,

    /// With --contents: replace it only while it holds what it held when
    /// it was read, whose SHA-256 this is (64 hex digits), so a change made
    /// since is not written over.
    #[arg(long, value_name = "SHA256", requires = "contents")]
    pub if_unchanged: Option<String>,

    /// For an MCP server, how an agent starts it or reaches it, given
    /// again as `add mcp` is given it, instead of opening an editor: what
    /// its definition says of its transport and its inputs is written
    /// again from these, and everything else it holds is kept.
    //
    // Boxed, as `add`'s are, being most of what a line can say. The
    // options of `add mcp` rather than a file of TOML, so a front end with
    // a form sends what the form asks and never writes the format itself.
    #[command(flatten)]
    pub server: Box<ServerOptions>,
}

/// Where `edit --contents` takes what the artifact is to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Given<'a> {
    /// Standard input, named `-` as `cat` and git name it.
    Input,
    /// A file, as it was typed.
    File(&'a Path),
}

impl Editing {
    /// Where `--contents` takes the contents from; none when the edit is
    /// an editor's.
    pub fn given(&self) -> Option<Given<'_>> {
        self.contents.as_deref().map(|contents| {
            if contents == Path::new("-") {
                Given::Input
            } else {
                Given::File(contents)
            }
        })
    }
}

/// How an agent starts an MCP server or reaches it, for `add mcp`: a
/// program, `--command` and what it is started with, or an address,
/// `--url` and what a request to it carries.
///
/// One struct for both binaries' `add`, as [`Editing`] is for `edit`. Every
/// value is a name or a literal the definition holds as it is: `--env` and
/// `--header` a value written as typed, every other option a variable an
/// input is read from, never its value. What no definition can hold, both
/// transports, or an option of one with the other or with a template that
/// gives one, is refused here, as grammar; whatever the definition then
/// holds is core's to judge ([`mindweaver_core::create::create_server`]).
///
/// Each option of one transport conflicts with the other's as well as
/// requiring its own: clap passes over what an option requires when that
/// conflicts with something given, so `--header` with `--command` would
/// otherwise be taken, and dropped. What says how a server is started
/// otherwise conflicts with every option here at once, by the group clap
/// gives the struct, `ServerOptions`: `add`'s `--template`, and `edit`'s
/// editor and contents, so `--arg` with `--template` is refused rather
/// than dropped, and one struct serves both commands.
#[derive(Debug, Clone, Default, PartialEq, Eq, Args)]
#[command(next_help_heading = "MCP server")]
pub struct ServerOptions {
    /// The program an agent starts the server with.
    #[arg(long, value_name = "COMMAND")]
    pub command: Option<String>,

    /// An argument the program is started with; again for each, in order.
    //
    // Led by a dash as often as not (`-y`, `--stdio`), so taken as a value
    // whatever it starts with: `--arg=-y` is never needed.
    #[arg(
        long = "arg",
        value_name = "ARG",
        allow_hyphen_values = true,
        requires = "command",
        conflicts_with = "url"
    )]
    pub args: Vec<String>,

    /// A variable the program's environment is given, its value written as
    /// it is.
    #[arg(
        long,
        value_name = "KEY=VALUE",
        value_parser = Pair,
        requires = "command",
        conflicts_with = "url"
    )]
    pub env: Vec<(String, String)>,

    /// A secret the program's environment is given, read from the variable
    /// VAR when an agent starts it and passed on under that name; never its
    /// value.
    #[arg(
        long = "secret-env",
        value_name = "VAR",
        requires = "command",
        conflicts_with = "url"
    )]
    pub secret_env: Vec<String>,

    /// An argument read from the variable VAR when an agent starts the
    /// program, after those --arg gives. No secret: a process list shows
    /// it.
    #[arg(
        long = "arg-env",
        value_name = "VAR",
        requires = "command",
        conflicts_with = "url"
    )]
    pub arg_env: Vec<String>,

    /// The address an agent reaches the server at, over streamable HTTP.
    #[arg(long, value_name = "URL", conflicts_with = "command")]
    pub url: Option<String>,

    /// Reach the address over server-sent events instead.
    #[arg(long, requires = "url", conflicts_with = "command")]
    pub sse: bool,

    /// A header every request carries, its value written as it is.
    #[arg(
        long = "header",
        value_name = "NAME=VALUE",
        value_parser = Pair,
        requires = "url",
        conflicts_with = "command"
    )]
    pub headers: Vec<(String, String)>,

    /// A secret header, its whole value read from the variable VAR when an
    /// agent starts the server; never its value.
    #[arg(
        long = "secret-header",
        value_name = "NAME=VAR",
        value_parser = Pair,
        requires = "url",
        conflicts_with = "command"
    )]
    pub secret_headers: Vec<(String, String)>,

    /// A secret sent as `Authorization: Bearer` and the token, read from
    /// the variable VAR; never its value.
    #[arg(
        long = "bearer-env",
        value_name = "VAR",
        requires = "url",
        conflicts_with = "command"
    )]
    pub bearer_env: Option<String>,

    /// A part of the address, written `{NAME}` in the URL, read from the
    /// variable VAR. No secret: an address is what a log shows.
    #[arg(
        long = "url-var",
        value_name = "NAME=VAR",
        value_parser = Pair,
        requires = "url",
        conflicts_with = "command"
    )]
    pub url_vars: Vec<(String, String)>,
}

impl ServerOptions {
    /// What core is told of the server's transport: none when neither a
    /// program nor an address is given, and then the grammar has let no
    /// other option of either through.
    pub fn given(&self) -> Option<mcp::Given> {
        if let Some(command) = &self.command {
            return Some(mcp::Given::Program(Program {
                command: command.clone(),
                args: self.args.clone(),
                env: self.env.clone(),
                secret_env: self.secret_env.iter().map(VariableName::from).collect(),
                arg_env: self.arg_env.iter().map(VariableName::from).collect(),
            }));
        }
        let url = self.url.as_ref()?;
        Some(mcp::Given::Address(Address {
            url: url.clone(),
            events: self.sse,
            headers: self.headers.clone(),
            secret_headers: self
                .secret_headers
                .iter()
                .map(|(header, variable)| (header.clone(), VariableName::from(variable)))
                .collect(),
            bearer: self.bearer_env.as_deref().map(VariableName::from),
            url_vars: self
                .url_vars
                .iter()
                .map(|(name, variable)| (InputName::from(name), VariableName::from(variable)))
                .collect(),
        }))
    }
}

/// What `workspaces` does to the list besides showing it.
#[derive(Debug, Subcommand)]
pub enum WorkspacesCommand {
    /// Stop knowing a workspace: every entry the default workspace keeps for
    /// it, however it is spelled.
    Forget {
        /// The workspace's directory, the one holding `.mindweaver`. It need
        /// not still exist.
        #[arg(value_name = "PATH")]
        path: PathBuf,
    },
}

/// How `agents` changes the agents a project names, when it does not name
/// them all.
#[derive(Debug, Subcommand)]
pub enum AgentsChange {
    /// Name these agents as well.
    Add(AgentNames),
    /// Stop naming these agents.
    #[command(alias = "rm")]
    Remove(AgentNames),
}

/// The agents an `agents` line names, and whether to go ahead.
#[derive(Debug, Args)]
pub struct AgentNames {
    /// Each agent: claude, codex or opencode, space or comma-separated.
    #[arg(
        required = true,
        value_name = "AGENT",
        value_delimiter = ',',
        value_parser = agents()
    )]
    pub agents: Vec<Agent>,

    /// Make the change. Without this nothing changes, and the command says
    /// what would.
    #[arg(short, long)]
    pub yes: bool,
}

/// How `targets` changes where a project installs a kind.
#[derive(Debug, Subcommand)]
pub enum TargetsChange {
    /// Install a kind in these folders, for every agent the project names,
    /// in place of where it went.
    Set {
        /// The kind: `skills`.
        #[arg(value_name = "KIND")]
        kind: Kind,

        /// Each folder, relative to the project: inside it, and outside
        /// `.mind`.
        #[arg(required = true, value_name = "DIR")]
        folders: Vec<String>,

        /// Make the change. Without this nothing changes, and the command
        /// says what would.
        #[arg(short, long)]
        yes: bool,
    },
    /// Install a kind where each agent the project names reads it again.
    Reset {
        /// The kind: `skills`.
        #[arg(value_name = "KIND")]
        kind: Kind,

        /// Make the change. Without this nothing changes, and the command
        /// says what would.
        #[arg(short, long)]
        yes: bool,
    },
}

/// What a project's settings are to say once it is moved from its
/// `mind.toml`, which says neither: the agents that work in it, and whether
/// what is installed into it is the team's. A `settings.json` says both, and
/// keeps what it says.
#[derive(Debug, Args)]
pub struct Migrated {
    /// The agents that work in the project, comma-separated [default:
    /// claude,codex,opencode].
    ///
    /// What is installed into it is copied where each of them reads it. A
    /// project a settings.json marks keeps the agents and the share mode it
    /// names.
    #[arg(
        long,
        value_name = "AGENTS",
        value_delimiter = ',',
        value_parser = agents(),
        default_values_t = Agent::ALL,
        hide_default_value = true
    )]
    pub agents: Vec<Agent>,

    /// Keep `.mind`, and what is installed into the project, out of git for
    /// this checkout.
    #[arg(long, conflicts_with = "shared")]
    pub local: bool,

    /// Leave `.mind`, and what is installed into the project, for the team
    /// to commit, as a project from before settings was [default].
    //
    // The default spelled out as a flag too, so a line can say it.
    #[arg(long)]
    pub shared: bool,
}

impl Migrated {
    /// The share mode asked for: shared unless `--local`, since a project
    /// from before settings was committed by design (D-02).
    pub fn share(&self) -> Share {
        if self.local {
            Share::Local
        } else {
            Share::Shared
        }
    }
}

/// What `share` sets: the share mode, and whether a tracked file may be
/// edited, written together.
#[derive(Debug, Args)]
pub struct ShareMode {
    /// `local`, kept out of git for this checkout alone, or `shared`, left
    /// for the team to commit.
    #[arg(value_name = "MODE", value_parser = shares())]
    pub share: Share,

    /// Let a file git tracks (AGENTS.md, CLAUDE.md, an MCP config) be edited
    /// to reach an agent in a local project. Off unless given.
    #[arg(long)]
    pub edit_tracked: bool,

    /// Make the change. Without this nothing changes, and the command says
    /// what would.
    #[arg(short, long)]
    pub yes: bool,
}

/// Where a gather takes artifacts from.
///
/// A subcommand per method rather than a `--from` flag: each one takes
/// different arguments, and the next one (a directory, an archive) should have
/// to say so rather than overloading a URL.
#[derive(Debug, Subcommand)]
pub enum GatherCommand {
    /// Take skills from a git repository.
    Git {
        /// The repository to clone.
        url: String,

        /// The folder inside the repository to take skills from.
        #[arg(long, value_name = "DIR", default_value = DEFAULT_SUBDIRECTORY)]
        path: String,

        /// A branch or tag to take, instead of the repository's default.
        #[arg(long = "ref", value_name = "REF")]
        reference: Option<String>,

        /// What to call the source, instead of its owner and repository
        /// (`acme-skills`): what `gather list` shows and `--from` takes.
        ///
        /// Letters, digits, `.`, `_` and `-`. Given for a source gathered
        /// before, it is called this from now on.
        #[arg(long, value_name = "NAME")]
        name: Option<String>,
    },

    /// List what can be installed from the shelf and from loaded workspaces,
    /// and where each artifact comes from.
    #[command(alias = "ls")]
    List {
        /// Print it as JSON, in the envelope every reading command's --json
        /// shares.
        #[arg(long)]
        json: bool,
    },
}

/// `NAME=VALUE`, split at its first `=`: what `--env`, `--header`,
/// `--secret-header` and `--url-var` take. The name before it, since no
/// variable's or header's name holds one, and the value after it whole,
/// whatever it holds, another `=` included.
///
/// A parser of its own rather than a function clap wraps, which says a
/// value it refuses whole: one given no `=` is as likely a secret whose
/// name was left out (`--env $TOKEN`) or a header written as curl takes one
/// (`--header "Authorization: Bearer ..."`), and said, it would land
/// wherever the error goes, a terminal or a CI log. So it is never said.
#[derive(Debug, Clone, Copy)]
struct Pair;

impl TypedValueParser for Pair {
    type Value = (String, String);

    fn parse_ref(
        &self,
        cmd: &clap::Command,
        arg: Option<&clap::Arg>,
        value: &OsStr,
    ) -> Result<Self::Value, clap::Error> {
        let text = StringValueParser::new().parse_ref(cmd, arg, value)?;
        let Some((name, value)) = text.split_once('=') else {
            let option = arg.map(ToString::to_string).unwrap_or_default();
            let refused = clap::Error::raw(
                ErrorKind::ValueValidation,
                format!(
                    "invalid value for '{option}': give it as a name, `=` and what goes with \
                     it (what was given is not repeated, since it may be a secret)"
                ),
            );
            return Err(refused.format(&mut cmd.clone()));
        };
        Ok((name.to_owned(), value.to_owned()))
    }
}

/// What `--shell` takes, from core's list rather than spelled again here, as
/// possible values so `--help` lists them and a typo is refused with them.
fn shells() -> impl TypedValueParser<Value = Shell> {
    PossibleValuesParser::new(Shell::ALL.map(Shell::name))
        .map(|name| name.parse::<Shell>().expect("every listed shell parses"))
}

/// What `--agents` takes, each from core's list, for the reason `--shell`
/// does.
fn agents() -> impl TypedValueParser<Value = Agent> {
    PossibleValuesParser::new(Agent::ALL.map(Agent::name))
        .map(|name| name.parse::<Agent>().expect("every listed agent parses"))
}

/// What `share` takes, each share mode by the word its settings spell it
/// with, for the reason `--shell` takes core's list.
fn shares() -> impl TypedValueParser<Value = Share> {
    PossibleValuesParser::new(Share::ALL.map(Share::name))
        .map(|name| Share::named(&name).expect("every listed share mode is one"))
}
