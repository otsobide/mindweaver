//! Gathering from a Claude Code plugin marketplace (#103, D-13), against
//! real workspaces and real repositories made in process: a catalog
//! recorded and listed, a plugin of it gathered as a source of its own only
//! when chosen, each of a repository of its own fetched at its pin.
//!
//! A plugin a catalog names on GitHub is fetched through a stand-in for the
//! forge ([`Forges`]), which hands a repository on the disk for each address
//! it knows and fails every other, so nothing here reaches the network.

mod common;

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use common::{commit, skill, url, workspace, write_files};
use mindweaver_core::adapter::gix::Gix;
use mindweaver_core::gather::{GatherError, Progress, Request as GitRequest, Watch};
use mindweaver_core::ledger::{Ledger, SourceKind};
use mindweaver_core::marketplace::{
    self, Extra, Gathered, MarketplaceError, NotGathered, Request, Took,
};
use mindweaver_core::mother::{GitRepositoryMother, MarketplaceMother, SkillMother};
use mindweaver_core::port::{Cloned, GitError, Remote, Token};
use mindweaver_core::value::{Pin, SourceName};
use mindweaver_core::{Kind, Licence, WeaverWorkspace};
use serde_json::json;
use tempfile::TempDir;

/// What one fetch was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Asked {
    url: String,
    pin: Option<Pin>,
    token: bool,
}

/// A stand-in for the forges a catalog names: each address it knows is a
/// repository on the disk, fetched with gix, as the forge would answer it;
/// a path is fetched as it is; anything else fails, as a repository nobody
/// has does. Every fetch is noted, with whether a token went with it.
#[derive(Debug, Default)]
struct Forges {
    at: HashMap<String, String>,
    asked: Mutex<Vec<Asked>>,
}

impl Forges {
    fn serving(at: &[(&str, &TempDir)]) -> Arc<Self> {
        Arc::new(Self {
            at: at
                .iter()
                .map(|(address, repository)| ((*address).to_owned(), url(repository)))
                .collect(),
            asked: Mutex::new(Vec::new()),
        })
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }
}

impl Remote for Forges {
    fn fetch(
        &self,
        url: &str,
        reference: Option<&str>,
        pin: Option<&Pin>,
        into: &Path,
        watch: &Watch,
        token: Option<&Token>,
    ) -> Result<Cloned, GitError> {
        self.asked.lock().unwrap().push(Asked {
            url: url.to_owned(),
            pin: pin.cloned(),
            token: token.is_some(),
        });
        let from = match self.at.get(url) {
            Some(path) => path.clone(),
            None if marketplace::on_this_machine(url) => url.to_owned(),
            None => {
                return Err(GitError::Fetch {
                    url: url.to_owned(),
                    detail: String::from("no such repository"),
                })
            }
        };
        Gix.fetch(&from, reference, pin, into, watch, None)
    }

    fn scrub(&self, clone: &Path) -> Result<bool, GitError> {
        Gix.scrub(clone)
    }
}

/// The commit `repository` is at now, as a pin.
fn head_of(repository: &TempDir) -> Pin {
    let opened = gix::open_opts(repository.path(), gix::open::Options::isolated()).unwrap();
    Pin::parse(&opened.head_id().unwrap().to_string()).unwrap()
}

fn gather(workspace: &WeaverWorkspace, ledger: &Ledger, request: &Request) -> Gathered {
    marketplace::gather(workspace, ledger, request).expect("the marketplace is gathered")
}

/// The names of what the shelf holds of the source called `source`.
fn shelved_by(ledger: &Ledger, source: &str) -> Vec<String> {
    let mut names: Vec<String> = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .filter(|gathered| gathered.source.name.as_str() == source)
        .map(|gathered| gathered.name.to_string())
        .collect();
    names.sort();
    names
}

fn name(text: &str) -> SourceName {
    SourceName::parse(text).unwrap()
}

/// What came of the plugin `entry`.
fn took<'a>(gathered: &'a Gathered, entry: &str) -> &'a Took {
    &gathered
        .plugins
        .iter()
        .find(|plugin| plugin.entry == entry)
        .unwrap_or_else(|| panic!("{entry} was not chosen: {:?}", gathered.plugins))
        .took
}

/// Everything under `folder`, a route each, for a test that holds a folder
/// to be as it was.
fn files_under(folder: &Path) -> Vec<String> {
    fn walk(root: &Path, at: &Path, found: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(at) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            found.push(path.strip_prefix(root).unwrap().display().to_string());
            if path.is_dir() {
                walk(root, &path, found);
            }
        }
    }
    let mut found = Vec::new();
    walk(folder, folder, &mut found);
    found.sort();
    found
}

// ---------------------------------------------------------------------------
// Listing
// ---------------------------------------------------------------------------

/// The acceptance's first case: a catalog modelled on anthropics/skills is
/// recorded as a source of its own kind, called what it calls itself, its
/// clone kept, and listed, with no plugin gathered.
#[test]
fn listing_a_marketplace_records_it_by_its_catalogs_name_and_gathers_no_plugin() {
    let catalog = MarketplaceMother::anthropic_skills();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let listed = gather(&workspace, &ledger, &Request::listing(url(&catalog)));

    assert_eq!(
        listed.listing.name,
        name(MarketplaceMother::ANTHROPIC_SKILLS)
    );
    assert_eq!(listed.listing.revision, Some(head_of(&catalog).revision()));
    let entries: Vec<&str> = listed
        .listing
        .catalog
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(entries, ["document-skills", "example-skills"]);
    assert!(listed.plugins.is_empty());
    assert!(!listed.unconfirmed);

    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 1, "{sources:?}");
    assert_eq!(sources[0].kind, SourceKind::Marketplace);
    assert_eq!(sources[0].name, name(MarketplaceMother::ANTHROPIC_SKILLS));
    assert_eq!(sources[0].revision, Some(head_of(&catalog).revision()));
    assert!(workspace
        .cache_dir()
        .join(&sources[0].directory)
        .join(marketplace::CATALOG)
        .is_file());
    assert!(ledger.gathered().unwrap().is_empty());
    assert!(!workspace.gathered_dir(Kind::Skill).exists());
}

/// The acceptance's second case: a catalog of more than a hundred plugins,
/// each a repository of its own, is listed by fetching the catalog alone.
#[test]
fn a_catalog_of_more_than_a_hundred_plugins_is_listed_by_fetching_the_catalog_alone() {
    let count = 100 + mindweaver_core::mother::random::between(1, 40);
    let catalog = MarketplaceMother::listing(count);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let forges = Arc::new(Forges::default());

    let listed = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).through(forges.clone()),
    );

    assert_eq!(listed.listing.catalog.entries.len(), count);
    assert_eq!(
        forges.asked(),
        [Asked {
            url: url(&catalog),
            pin: None,
            token: false,
        }],
        "nothing but the catalog is fetched"
    );
    assert_eq!(ledger.sources().unwrap().len(), 1);
}

/// The acceptance's third case: every plugin of a catalog listing more than
/// the bound, without a yes, writes nothing: no source, no clone, no shelf,
/// nothing left in the scratch folder, and nothing logged.
#[test]
fn every_plugin_of_a_long_catalog_without_a_yes_writes_nothing() {
    let catalog = MarketplaceMother::listing(marketplace::ASKED_BEYOND + 1);
    let (dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let before = files_under(dir.path());
    let forges = Arc::new(Forges::default());

    let asked = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog))
            .all(false)
            .through(forges.clone()),
    );

    assert!(asked.unconfirmed);
    assert!(asked.plugins.is_empty());
    assert_eq!(
        asked.listing.catalog.entries.len(),
        marketplace::ASKED_BEYOND + 1
    );
    assert_eq!(forges.asked().len(), 1, "the catalog alone is fetched");
    assert!(ledger.sources().unwrap().is_empty());
    assert!(ledger.history(10).unwrap().is_empty());
    // But for the lock it took, the scratch folder its run's own folder was
    // made in, gone again, and what SQLite keeps beside a ledger it reads.
    let written = |path: &String| {
        !["lock", "tmp", "-shm", "-wal"]
            .iter()
            .any(|kept| path.ends_with(kept))
    };
    let after: Vec<String> = files_under(dir.path())
        .into_iter()
        .filter(written)
        .collect();
    let before: Vec<String> = before.into_iter().filter(written).collect();
    assert_eq!(after, before);
    assert!(files_under(&workspace.scratch_dir()).is_empty());

    // The read a front end asks first says the same, and writes nothing
    // either.
    let read = marketplace::read(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).all(false),
    )
    .unwrap();
    assert!(read.catalog.asks_first());
    assert!(ledger.sources().unwrap().is_empty());
}

/// A repository with no catalog is no marketplace, and nothing of it is
/// recorded.
#[test]
fn a_repository_with_no_catalog_is_refused_and_nothing_is_recorded() {
    let repository = GitRepositoryMother::with_skills(&["deploy"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let refused =
        marketplace::gather(&workspace, &ledger, &Request::listing(url(&repository))).unwrap_err();

    assert_eq!(refused.code(), "marketplace_catalog_missing", "{refused}");
    assert!(
        refused.to_string().contains("mindweaver gather git"),
        "{refused}"
    );
    assert!(ledger.sources().unwrap().is_empty());
}

/// `--name` calls the marketplace something else, and a name another
/// source has is refused before anything is recorded.
#[test]
fn a_marketplace_is_called_what_name_says_and_a_name_taken_is_refused() {
    let catalog = MarketplaceMother::anthropic_skills();
    let other = GitRepositoryMother::with_skills(&["deploy"]);
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    mindweaver_core::gather::gather(
        &workspace,
        &ledger,
        &GitRequest::git(url(&other)).named(Some("taken")),
    )
    .unwrap();

    let refused = marketplace::gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).named(Some("taken")),
    )
    .unwrap_err();
    assert_eq!(refused.code(), "source_name_taken", "{refused}");
    assert_eq!(ledger.sources().unwrap().len(), 1);

    let listed = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).named(Some("anthropic")),
    );
    assert_eq!(listed.listing.name, name("anthropic"));
}

// ---------------------------------------------------------------------------
// Gathering plugins
// ---------------------------------------------------------------------------

/// A plugin of anthropics/skills, its whole repository, takes the skills
/// its entry lists and no other, as a source of its own named after the
/// marketplace and the entry, its child, at the marketplace's commit.
#[test]
fn a_plugin_of_anthropics_skills_takes_the_skills_its_entry_lists_and_no_other() {
    let catalog = MarketplaceMother::anthropic_skills();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let gathered = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).plugins(vec![String::from("document-skills")]),
    );

    let Took::Gathered(report) = took(&gathered, "document-skills") else {
        panic!("{:?}", gathered.plugins);
    };
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    let child = format!("{}.document-skills", MarketplaceMother::ANTHROPIC_SKILLS);
    assert_eq!(report.name, name(&child));
    assert_eq!(report.revision, Some(head_of(&catalog).revision()));
    let mut expected: Vec<String> = MarketplaceMother::DOCUMENT_SKILLS
        .iter()
        .map(|skill| (*skill).to_owned())
        .collect();
    expected.sort();
    assert_eq!(shelved_by(&ledger, &child), expected);

    let sources = ledger.sources().unwrap();
    assert_eq!(sources.len(), 2, "{sources:?}");
    let plugin = &sources[1];
    assert_eq!(plugin.kind, SourceKind::Git);
    assert_eq!(
        plugin.parent,
        Some(name(MarketplaceMother::ANTHROPIC_SKILLS))
    );
    assert_eq!(plugin.entry.as_deref(), Some("document-skills"));
    assert_eq!(plugin.subdirectory, ".");
    // In the marketplace's own repository, so read from its clone: the
    // plugin has no clone of its own.
    assert!(!workspace.cache_dir().join(&plugin.directory).exists());
    let entry = gathered.listing.catalog.entry("document-skills").unwrap();
    assert_eq!(entry.gathered, Some(name(&child)));
    assert_eq!(
        gathered
            .listing
            .catalog
            .entry("example-skills")
            .unwrap()
            .gathered,
        None
    );

    // Each document skill bundles the licence it is under, which is its
    // own and no plugin's.
    for artifact in ledger.gathered().unwrap() {
        assert!(
            matches!(&artifact.licence, Some(Licence::Manifest { .. })),
            "{artifact:?}"
        );
    }
}

/// A plugin named that the catalog does not list is refused whole, before
/// any plugin is gathered or the marketplace recorded.
#[test]
fn a_plugin_the_catalog_does_not_list_is_refused_before_any_is_gathered() {
    let catalog = MarketplaceMother::anthropic_skills();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let refused = marketplace::gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).plugins(vec![
            String::from("document-skills"),
            String::from("spreadsheets"),
        ]),
    )
    .unwrap_err();

    assert_eq!(refused.code(), "marketplace_plugin_unknown", "{refused}");
    assert!(
        refused.to_string().contains("`spreadsheets` is"),
        "{refused}"
    );
    assert!(ledger.sources().unwrap().is_empty());
    assert!(ledger.gathered().unwrap().is_empty());
}

/// The repositories of the catalog mixing every kind of place a plugin can
/// be: the marketplace, keeping two of its own, a monorepo one is a folder
/// of, a GitHub repository another is pinned to a commit of, and the pin.
struct Mixed {
    catalog: TempDir,
    mono: TempDir,
    deploy: TempDir,
    pin: Pin,
}

/// A team's catalog, mixing a plugin kept beside it (`./plugins/formatter`,
/// which keeps hooks and agents beside its skills), one named bare under
/// its `pluginRoot` that is one skill, a folder of a monorepo
/// (`git-subdir`), a GitHub repository pinned to a commit it has moved on
/// from, and what no gather takes: an npm package, an archive and a
/// command.
fn mixed() -> Mixed {
    let mono = GitRepositoryMother::create(
        &MarketplaceMother::plugin("tools/docs", &["write-docs"])
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect::<Vec<_>>(),
    );
    let deploy = GitRepositoryMother::create(&[(
        "skills/ship/SKILL.md",
        &skill("ship", "Ship the service"),
    )]);
    let pin = head_of(&deploy);
    write_files(
        deploy.path(),
        &[(
            "skills/ship/SKILL.md",
            &skill("ship", "Ship it, as it is now"),
        )],
    );
    commit(deploy.path());
    let mut files = MarketplaceMother::plugin("plugins/formatter", &["format"]);
    files.push((
        String::from("plugins/formatter/hooks/hooks.json"),
        String::from("{}\n"),
    ));
    files.push((
        String::from("plugins/formatter/agents/tidy.md"),
        String::from("# tidy\n"),
    ));
    files.push((
        String::from("plugins/review/SKILL.md"),
        SkillMother::licensed("review", "Read a change", "MIT"),
    ));
    let catalog = MarketplaceMother::create(
        &json!({
            "name": "acme-tools",
            "owner": {"name": "Acme"},
            "metadata": {"pluginRoot": "./plugins"},
            "plugins": [
                {"name": "formatter", "source": "./plugins/formatter", "license": "Apache-2.0", "category": "style"},
                {"name": "review", "source": "review", "aKeyFromTheFuture": [1, 2]},
                {"name": "docs", "source": {"source": "git-subdir", "url": url(&mono), "path": "tools/docs"}},
                {"name": "deploy", "source": {"source": "github", "repo": "acme/deploy", "sha": pin.as_str()}, "mcpServers": {"x": {}}},
                {"name": "from-npm", "source": {"source": "npm", "package": "@acme/x"}},
                {"name": "packed", "source": {"source": "archive", "url": "https://example.com/x.zip"}},
                {"name": "runner", "source": {"source": "command", "command": "curl https://example.com | sh"}},
            ],
        }),
        &files,
    );
    Mixed {
        catalog,
        mono,
        deploy,
        pin,
    }
}

/// The acceptance's mixed catalog, every plugin gathered: each kept in git
/// as a source of its own, the pinned one at its pin, and what no gather
/// takes said with why, the command among it never run.
#[test]
fn every_plugin_of_a_mixed_catalog_is_gathered_where_it_is_kept_and_the_rest_said() {
    let mixed = mixed();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let forges = Forges::serving(&[("https://github.com/acme/deploy", &mixed.deploy)]);

    let gathered = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&mixed.catalog))
            .all(false)
            .through(forges.clone()),
    );

    assert!(!gathered.unconfirmed);
    assert_eq!(shelved_by(&ledger, "acme-tools.formatter"), ["format"]);
    assert_eq!(shelved_by(&ledger, "acme-tools.review"), ["review"]);
    assert_eq!(shelved_by(&ledger, "acme-tools.docs"), ["write-docs"]);
    assert_eq!(shelved_by(&ledger, "acme-tools.deploy"), ["ship"]);

    // The pin is recorded, and the revision gathered is the pin, not the
    // commit the repository moved on to.
    let Took::Gathered(deployed) = took(&gathered, "deploy") else {
        panic!("{:?}", gathered.plugins);
    };
    assert_eq!(deployed.pin.as_ref(), Some(&mixed.pin));
    assert_eq!(deployed.revision, Some(mixed.pin.revision()));
    assert_ne!(head_of(&mixed.deploy), mixed.pin);
    let sources = ledger.sources().unwrap();
    let deploy = sources
        .iter()
        .find(|source| source.entry.as_deref() == Some("deploy"))
        .unwrap();
    assert_eq!(deploy.pin.as_ref(), Some(&mixed.pin));
    assert_eq!(deploy.revision, Some(mixed.pin.revision()));
    assert_eq!(deploy.url.as_str(), "https://github.com/acme/deploy");
    let shelved = ledger
        .gathered()
        .unwrap()
        .into_iter()
        .find(|artifact| artifact.name.as_str() == "ship")
        .unwrap();
    assert_eq!(shelved.revision, Some(mixed.pin.revision()));
    assert_eq!(shelved.summary.as_deref(), Some("Ship the service"));
    assert!(forges.asked().contains(&Asked {
        url: String::from("https://github.com/acme/deploy"),
        pin: Some(mixed.pin.clone()),
        token: false,
    }));

    // A folder of a monorepo, fetched into a clone of its own.
    let docs = sources
        .iter()
        .find(|source| source.entry.as_deref() == Some("docs"))
        .unwrap();
    assert_eq!(docs.url.as_str(), url(&mixed.mono));
    assert_eq!(docs.subdirectory, "tools/docs");
    assert!(workspace.cache_dir().join(&docs.directory).is_dir());

    // Listed straight after the marketplace, by their entries.
    let listed: Vec<String> = sources
        .iter()
        .map(|source| source.name.to_string())
        .collect();
    assert_eq!(
        listed,
        [
            "acme-tools",
            "acme-tools.deploy",
            "acme-tools.docs",
            "acme-tools.formatter",
            "acme-tools.review"
        ]
    );

    // What beside its skills a plugin keeps or declares is said and left.
    let left = |entry: &str| {
        gathered
            .plugins
            .iter()
            .find(|plugin| plugin.entry == entry)
            .unwrap()
            .left
            .clone()
    };
    assert_eq!(left("formatter"), [Extra::Agents, Extra::Hooks]);
    assert_eq!(left("deploy"), [Extra::McpServers]);
    assert!(left("review").is_empty());

    // What no gather takes, said with why, and no failure of `--all`'s.
    let refused: Vec<(&str, &str)> = gathered
        .plugins
        .iter()
        .filter_map(|plugin| match &plugin.took {
            Took::NotGathered(why) => Some((plugin.entry.as_str(), why.code())),
            _ => None,
        })
        .collect();
    assert_eq!(
        refused,
        [
            ("from-npm", "marketplace_npm_not_gathered"),
            ("packed", "marketplace_archive_not_gathered"),
            ("runner", "marketplace_command_refused"),
        ]
    );
    assert!(gathered.plugins.iter().all(|plugin| !plugin.fails()));
}

/// The acceptance's command entry: named, it is refused with a message
/// saying it is never run, which fails the gather, and the plugin beside it
/// is gathered all the same.
#[test]
fn a_command_entry_named_is_refused_with_a_message_and_the_rest_gathered() {
    let mixed = mixed();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let gathered = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&mixed.catalog))
            .plugins(vec![String::from("runner"), String::from("formatter")]),
    );

    let runner = &gathered.plugins[0];
    assert_eq!(runner.entry, "runner");
    let Took::NotGathered(why @ NotGathered::Command { .. }) = &runner.took else {
        panic!("{:?}", runner.took);
    };
    assert_eq!(
        why.to_string(),
        "runner: its source is a command to run, which Mindweaver never runs: only a plugin kept \
         in git is gathered"
    );
    assert!(runner.fails(), "named, it fails the gather");
    assert!(matches!(took(&gathered, "formatter"), Took::Gathered(_)));
    assert!(ledger
        .sources()
        .unwrap()
        .iter()
        .all(|source| source.entry.as_deref() != Some("runner")));
}

/// One plugin that cannot be fetched is said, and forgotten, being nothing
/// on the shelf; the one beside it is gathered.
#[test]
fn a_plugin_that_cannot_be_fetched_is_said_and_costs_none_beside_it() {
    let review = GitRepositoryMother::one_skill("review");
    let catalog = MarketplaceMother::create(
        &MarketplaceMother::catalog(
            "acme-tools",
            &[
                json!({"name": "gone", "source": {"source": "github", "repo": "acme/gone"}}),
                json!({"name": "review", "source": {"source": "url", "url": url(&review)}}),
            ],
        ),
        &[],
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let gathered = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog))
            .all(true)
            .through(Arc::new(Forges::default())),
    );

    let Took::Failed(error) = took(&gathered, "gone") else {
        panic!("{:?}", gathered.plugins);
    };
    assert_eq!(error.code(), "git_fetch_failed", "{error}");
    assert!(gathered.plugins[0].fails());
    assert_eq!(gathered.plugins[0].name, None, "forgotten, being nothing");
    assert!(matches!(took(&gathered, "review"), Took::Gathered(_)));
    let names: Vec<String> = ledger
        .sources()
        .unwrap()
        .iter()
        .map(|source| source.name.to_string())
        .collect();
    assert_eq!(names, ["acme-tools", "acme-tools.review"]);
}

/// A plugin's licence is what each skill of it declaring none is under; a
/// skill that declares one keeps its own. The plugin's own manifest says
/// it when its entry does not.
#[test]
fn a_skill_declaring_no_licence_is_under_its_plugins() {
    let mut files = MarketplaceMother::plugin("plugins/formatter", &["format"]);
    files.push((
        String::from("plugins/formatter/skills/lint/SKILL.md"),
        SkillMother::licensed("lint", "Tidy the code", "MIT"),
    ));
    files.extend(MarketplaceMother::plugin("plugins/review", &["review"]));
    files.push((
        String::from("plugins/review/.claude-plugin/plugin.json"),
        String::from(r#"{"name": "review", "license": "MPL-2.0"}"#),
    ));
    let catalog = MarketplaceMother::create(
        &MarketplaceMother::catalog(
            "acme-tools",
            &[
                json!({"name": "formatter", "source": "./plugins/formatter", "license": "Apache-2.0"}),
                json!({"name": "review", "source": "./plugins/review"}),
            ],
        ),
        &files,
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).all(true),
    );

    let licence = |skill: &str| {
        ledger
            .gathered()
            .unwrap()
            .into_iter()
            .find(|artifact| artifact.name.as_str() == skill)
            .unwrap()
            .licence
            .unwrap()
    };
    assert_eq!(
        licence("format"),
        Licence::Plugin {
            text: String::from("Apache-2.0")
        }
    );
    assert_eq!(
        licence("lint"),
        Licence::Manifest {
            text: String::from("MIT")
        }
    );
    assert_eq!(
        licence("review"),
        Licence::Plugin {
            text: String::from("MPL-2.0")
        }
    );
}

/// A folder an entry lists that holds no skill is said, and the skills it
/// lists that are there are taken.
#[test]
fn a_folder_an_entry_lists_that_holds_no_skill_is_said_beside_the_rest() {
    let catalog = MarketplaceMother::create(
        &MarketplaceMother::catalog(
            "acme-tools",
            &[json!({
                "name": "tools",
                "source": "./",
                "skills": ["./skills/format", "./skills/gone", "../../etc"],
            })],
        ),
        &MarketplaceMother::plugin(".", &["format"]),
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();

    let gathered = gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog)).plugins(vec![String::from("tools")]),
    );

    let Took::Gathered(report) = took(&gathered, "tools") else {
        panic!("{:?}", gathered.plugins);
    };
    assert_eq!(shelved_by(&ledger, "acme-tools.tools"), ["format"]);
    let codes: Vec<&str> = report
        .failures
        .iter()
        .map(|failure| failure.code())
        .collect();
    assert_eq!(
        codes,
        ["gathered_listed_missing", "gathered_listed_missing"]
    );
}

/// A catalog fetched from elsewhere names no repository of this machine:
/// such a plugin is not gathered, and said.
#[test]
fn a_catalog_from_elsewhere_reaches_no_repository_on_this_machine() {
    let private = GitRepositoryMother::one_skill("private");
    let catalog = MarketplaceMother::create(
        &MarketplaceMother::catalog(
            "acme-tools",
            &[json!({"name": "near", "source": {"source": "url", "url": url(&private)}})],
        ),
        &[],
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let forges = Forges::serving(&[("https://github.com/acme/tools", &catalog)]);

    let gathered = gather(
        &workspace,
        &ledger,
        &Request::listing("acme/tools")
            .plugins(vec![String::from("near")])
            .through(forges.clone()),
    );

    let Took::NotGathered(why) = took(&gathered, "near") else {
        panic!("{:?}", gathered.plugins);
    };
    assert_eq!(why.code(), "marketplace_plugin_local");
    assert_eq!(forges.asked().len(), 1, "the catalog alone is fetched");
    assert!(ledger.gathered().unwrap().is_empty());
}

/// A token goes to a plugin on the marketplace's own host alone: one
/// handed over for GitHub never reaches a plugin kept elsewhere.
#[test]
fn a_token_goes_only_to_the_plugins_on_the_marketplaces_host() {
    let deploy = GitRepositoryMother::one_skill("deploy");
    let lint = GitRepositoryMother::one_skill("lint");
    let catalog = MarketplaceMother::create(
        &MarketplaceMother::catalog(
            "acme-tools",
            &[
                json!({"name": "deploy", "source": {"source": "github", "repo": "acme/deploy"}}),
                json!({"name": "lint", "source": {"source": "url", "url": "https://gitlab.com/acme/lint.git"}}),
            ],
        ),
        &[],
    );
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let forges = Forges::serving(&[
        ("https://github.com/acme/tools", &catalog),
        ("https://github.com/acme/deploy", &deploy),
        ("https://gitlab.com/acme/lint.git", &lint),
    ]);

    gather(
        &workspace,
        &ledger,
        &Request::listing("acme/tools")
            .all(true)
            .with_token(Some(Token::new("not-a-real-token")))
            .through(forges.clone()),
    );

    let tokens: Vec<(String, bool)> = forges
        .asked()
        .into_iter()
        .map(|asked| (asked.url, asked.token))
        .collect();
    assert_eq!(
        tokens,
        [
            (String::from("https://github.com/acme/tools"), true),
            (String::from("https://github.com/acme/deploy"), true),
            (String::from("https://gitlab.com/acme/lint.git"), false),
        ]
    );
}

/// A cancel before the marketplace is recorded leaves the workspace as it
/// was: nothing recorded, nothing in the cache.
#[test]
fn a_cancel_before_anything_is_placed_leaves_the_workspace_as_it_was() {
    let catalog = MarketplaceMother::anthropic_skills();
    let (_dir, workspace) = workspace();
    let ledger = workspace.ledger().unwrap();
    let watch = Watch::new();
    let cancel = watch.clone();
    let watch = watch.reporting_to(move |step: Progress| {
        if matches!(step, Progress::CheckingOut { .. }) {
            cancel.cancel();
        }
    });

    let refused = marketplace::gather(
        &workspace,
        &ledger,
        &Request::listing(url(&catalog))
            .plugins(vec![String::from("document-skills")])
            .watched_by(watch),
    )
    .unwrap_err();

    assert!(
        matches!(refused, MarketplaceError::Gather(GatherError::Cancelled)),
        "{refused:?}"
    );
    assert!(ledger.sources().unwrap().is_empty());
    assert!(ledger.history(10).unwrap().is_empty());
    assert!(fs::read_dir(workspace.cache_dir())
        .map(|entries| entries.count() == 0)
        .unwrap_or(true));
}
