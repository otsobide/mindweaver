//! Mothers of what the ledger records and hands back ([`crate::ledger`]),
//! each an aggregate of the value mothers, as each record is of the values.

use crate::agent::Agent;
use crate::kind::Kind;
use crate::ledger::{
    Entry, EntryTarget, Gathered, InstallRecord, Moved, Source, SourceKind, Target,
};
use crate::mother::random;
use crate::mother::url::UrlMother;
use crate::mother::value::{
    ArtifactNameMother, ContentHashMother, OriginMother, RedactedMother, RevisionMother,
};
use crate::value::{ArtifactName, ContentHash, Origin, Redacted, SourceName};

/// Makes [`Source`]s, as the ledger hands one back.
pub struct SourceMother;

impl SourceMother {
    /// A git source, at a branch or not, last gathered at some revision.
    pub fn random() -> Source {
        let reference = random::chance().then(random::word);
        Self::create(&UrlMother::clean(), reference.as_deref())
    }

    /// A git source at `url`, asked for at `reference`.
    pub fn create(url: &str, reference: Option<&str>) -> Source {
        Source {
            id: random::between(1, 1000) as i64,
            kind: SourceKind::Git,
            url: Redacted::new(url),
            reference: reference.map(str::to_owned),
            subdirectory: String::from(crate::gather::DEFAULT_SUBDIRECTORY),
            revision: Some(RevisionMother::random()),
            directory: random::word(),
            name: SourceName::derived(&crate::url::canonical(url)),
        }
    }
}

/// Makes [`Gathered`] artifacts.
pub struct GatheredMother;

impl GatheredMother {
    /// A skill gathered from some source.
    pub fn random() -> Gathered {
        Self::create(
            Kind::Skill,
            ArtifactNameMother::random(),
            SourceMother::random(),
        )
    }

    /// `name`, of `kind`, gathered from `source`.
    pub fn create(kind: Kind, name: ArtifactName, source: Source) -> Gathered {
        Gathered {
            kind,
            path: format!(".mindweaver/{}/{}/{name}", kind.slug(), source.directory),
            name,
            summary: random::chance().then(|| random::text(30)),
            revision: source.revision.clone(),
            gathered_at: random::between(1_600_000_000, 1_900_000_000) as i64,
            source,
        }
    }
}

/// Makes [`Target`]s, the copies an installation wrote.
pub struct TargetMother;

impl TargetMother {
    /// A copy of a skill named `name` for one agent or another.
    pub fn random(name: &str) -> Target {
        let agent = random::pick(&Agent::ALL);
        Self::create(Some(agent), &format!(".{}/skills/{name}", agent.name()))
    }

    /// A copy at `path`, for `agent` or for no one agent.
    pub fn create(agent: Option<Agent>, path: &str) -> Target {
        Target {
            agent: agent.map(|agent| agent.name().to_owned()),
            path: path.to_owned(),
            content_hash: None,
        }
    }

    /// A copy at `path` that holds bytes of its own, as each of a rule's
    /// does.
    pub fn holding(agent: Option<Agent>, path: &str, hash: ContentHash) -> Target {
        Target {
            content_hash: Some(hash),
            ..Self::create(agent, path)
        }
    }
}

/// Makes [`EntryTarget`]s, the entries an MCP server's installation wrote.
pub struct EntryTargetMother;

impl EntryTargetMother {
    /// An entry of the server `name` for one agent or another, in the file
    /// that agent reads its servers from in a project holding none of
    /// OpenCode's other files.
    pub fn random(name: &str) -> EntryTarget {
        Self::of(random::pick(&Agent::ALL), name)
    }

    /// The entry of the server `name` for `agent`, in its file, holding
    /// what some definition rendered.
    pub fn of(agent: Agent, name: &str) -> EntryTarget {
        let file = match agent {
            Agent::Claude => crate::claude_mcp::FILE,
            Agent::Codex => crate::codex_mcp::FILE,
            Agent::OpenCode => crate::opencode_mcp::FILE,
        };
        Self::create(agent, file, name, ContentHashMother::random())
    }

    /// Exactly what it is given.
    pub fn create(agent: Agent, file: &str, name: &str, fingerprint: ContentHash) -> EntryTarget {
        EntryTarget {
            agent,
            file: file.to_owned(),
            name: ArtifactName::from(name),
            fingerprint,
        }
    }
}

/// Makes [`InstallRecord`]s.
pub struct InstallRecordMother;

impl InstallRecordMother {
    /// A skill installed from somewhere, with a copy for each of some agents.
    pub fn random() -> InstallRecord {
        Self::skill(&ArtifactNameMother::random())
    }

    /// The skill `name`, installed from the workspace's own, a copy in each
    /// agent's folder.
    pub fn skill(name: &str) -> InstallRecord {
        Self::create(
            Kind::Skill,
            name,
            Some(OriginMother::random()),
            Some(ContentHashMother::random()),
            Agent::ALL
                .iter()
                .map(|agent| {
                    TargetMother::create(Some(*agent), &format!(".{}/skills/{name}", agent.name()))
                })
                .collect(),
        )
    }

    /// The rule `route`, installed as its section of the block and its file
    /// for Claude, each holding bytes of its own.
    pub fn rule(route: &str) -> InstallRecord {
        Self::create(
            Kind::Rule,
            route,
            Some(OriginMother::workspace()),
            Some(ContentHashMother::random()),
            vec![
                TargetMother::holding(None, "AGENTS.md", ContentHashMother::random()),
                TargetMother::holding(
                    Some(Agent::Claude),
                    &format!(".claude/rules/mindweaver/{route}.md"),
                    ContentHashMother::random(),
                ),
            ],
        )
    }

    /// The MCP server `name`, installed from the workspace's own as an entry
    /// in each agent's config file, and no copy.
    pub fn mcp(name: &str) -> InstallRecord {
        InstallRecord {
            entries: Agent::ALL
                .iter()
                .map(|agent| EntryTargetMother::of(*agent, name))
                .collect(),
            ..Self::create(
                Kind::Mcp,
                name,
                Some(OriginMother::workspace()),
                None,
                Vec::new(),
            )
        }
    }

    /// A record a build from before schema 3 wrote: neither where it came
    /// from nor a hash of what it wrote.
    pub fn before_hashes(name: &str) -> InstallRecord {
        InstallRecord {
            origin: None,
            content_hash: None,
            ..Self::skill(name)
        }
    }

    /// Exactly what it is given, from no shelf source.
    pub fn create(
        kind: Kind,
        name: &str,
        origin: Option<Origin>,
        content_hash: Option<ContentHash>,
        targets: Vec<Target>,
    ) -> InstallRecord {
        InstallRecord {
            kind,
            name: ArtifactName::from(name),
            source_id: None,
            origin,
            content_hash,
            targets,
            entries: Vec::new(),
        }
    }
}

/// Makes [`Moved`] installations, as a project's migration hands the ledger
/// where each one's copies are now.
pub struct MovedMother;

impl MovedMother {
    /// The skill `name`, its copies now in each agent's folder.
    pub fn skill(name: &str) -> Moved {
        Moved {
            kind: Kind::Skill,
            name: ArtifactName::from(name),
            targets: InstallRecordMother::skill(name).targets,
            content_hash: Some(ContentHashMother::random()),
        }
    }
}

/// Makes [`Entry`]s of the action log, as the ledger hands one back.
pub struct EntryMother;

impl EntryMother {
    /// Something that happened to something, successfully or not.
    pub fn random() -> Entry {
        Entry {
            at: random::between(1_600_000_000, 1_900_000_000) as i64,
            action: random::pick(&["gather", "install", "uninstall", "relink", "adopt"]).to_owned(),
            target: Some(RedactedMother::url()),
            outcome: random::pick(&["ok", "failed"]).to_owned(),
            detail: random::chance().then(RedactedMother::random),
        }
    }
}

/// Makes the routes the ledger files a project's records by: from the
/// workspace, `/` separated, or `.` for a project at its root (D-03).
pub struct RouteMother;

impl RouteMother {
    /// A route a workspace may link a project at.
    pub fn random() -> String {
        match random::between(0, 4) {
            0 => String::from("."),
            1 => format!("{}/{}", random::kebab(), random::kebab()),
            _ => random::kebab(),
        }
    }

    /// `count` routes, no two alike, for a test of what one project's
    /// records keep from another's.
    pub fn distinct(count: usize) -> Vec<String> {
        let mut routes: Vec<String> = Vec::with_capacity(count);
        while routes.len() < count {
            let route = Self::random();
            if !routes.contains(&route) {
                routes.push(route);
            }
        }
        routes
    }
}

impl InstallRecordMother {
    /// Any record a store may be handed: a skill, a rule or an MCP server,
    /// under a name of any script, from somewhere or from before origins
    /// were kept, with a hash or without, and none or several copies, each
    /// for one agent or for none and holding a hash of its own or not; and
    /// an MCP server's with an entry for some of the agents, one each.
    ///
    /// Wider than [`random`](Self::random), which is a skill as an install
    /// writes one today: a store keeps what it is handed, and a record an
    /// older build wrote is one it is handed too.
    pub fn any() -> InstallRecord {
        let kind = random::pick(&Kind::ALL);
        let name = match random::between(0, 2) {
            0 => ArtifactNameMother::random(),
            1 => ArtifactNameMother::route(),
            _ => ArtifactName::from(format!("{}{}", random::kebab(), random::text(8))),
        };
        let targets = (0..random::between(0, 4))
            .map(|copy| {
                let agent = random::chance().then(|| random::pick(&Agent::ALL));
                let folder = agent.map_or("agents", Agent::name);
                let path = format!(".{folder}/{}/{name}-{copy}", kind.folder());
                if random::chance() {
                    TargetMother::holding(agent, &path, ContentHashMother::random())
                } else {
                    TargetMother::create(agent, &path)
                }
            })
            .collect();
        let entries = match kind {
            Kind::Mcp => Agent::ALL
                .into_iter()
                .filter(|_| random::chance())
                .map(|agent| EntryTargetMother::of(agent, &name))
                .collect(),
            Kind::Skill | Kind::Rule => Vec::new(),
        };
        InstallRecord {
            kind,
            name,
            source_id: None,
            origin: random::chance().then(OriginMother::random),
            content_hash: random::chance().then(ContentHashMother::random),
            targets,
            entries,
        }
    }
}
