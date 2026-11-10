//! The snapshot every front end reads, against a real workspace on disk:
//! what it serializes to, name by name and in order, and what reading it
//! leaves behind, which is nothing.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::slice;

use mindweaver_core::assign;
use mindweaver_core::create::Start;
use mindweaver_core::doctor::Machine;
use mindweaver_core::install::{self, Confirmed, Edits};
use mindweaver_core::known;
use mindweaver_core::ledger::{Action, InstallRecord, Outcome, Target};
use mindweaver_core::lifecycle;
use mindweaver_core::mcp::{Address, Given, Program};
use mindweaver_core::mother::{McpMother, VariableNameMother};
use mindweaver_core::reapply::{self, Change};
use mindweaver_core::resolve::{Inputs, Intent, Reason};
use mindweaver_core::value::VariableName;
use mindweaver_core::view::{Health, Held, Listing, LoadedWorkspace, Project, Snapshot};
use mindweaver_core::{
    create, hash, Agent, AgentHomes, Directories, Kind, MindProject, Reference, Share, SourceKind,
    Starts, State, WeaverWorkspace, MIND_DIR, SETTINGS_JSON_FILE, WEAVER_DIR,
};
use tempfile::TempDir;

use common::skill;

/// Seconds since the epoch every time in the fixture is pinned to, so what
/// the ledger says reads the same on every run.
const AT: i64 = 1_790_000_000;

/// A workspace `hub` in a temporary folder, as a front end finds one:
///
/// - its library holds `review` and the rule `commits`, which it offers;
/// - it links `api`, marked by its settings for Claude and Codex and shared,
///   which also name the folders its skills go in, the ones its agents
///   read; `api` authors `deploy`, received `review` from the library, both
///   copies there as they were written, and received the rule `commits`
///   (D-23) as an install writes it;
/// - it links `gone`, whose directory has gone since;
/// - it loads `team`, beside it, which offers `notes`;
/// - its shelf holds `lint`, gathered from one source, and its log the
///   install of `commits` and one gather.
struct Fixture {
    dir: TempDir,
    hub: PathBuf,
}

fn fixture() -> Fixture {
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    let (mut workspace, _) = WeaverWorkspace::init(&hub).unwrap();
    let library = workspace.own().directory_for(Kind::Skill).unwrap();
    common::write_files(
        &library,
        &[("review/SKILL.md", &skill("review", "Read it"))],
    );

    let directories = Directories::default()
        .with(Kind::Skill, "skills")
        .with(Kind::Rule, "rules");
    let (api, _) = MindProject::init_settings(
        hub.join("api"),
        &[Agent::Claude, Agent::Codex],
        Share::Shared,
        &directories,
    )
    .unwrap();
    workspace.link(&api).unwrap();
    // The folders the standard table sends skills to, in an order of its
    // own: settings that say where, which change nothing else.
    reapply::apply(
        Some(&workspace),
        &api,
        &Change::Targets {
            kind: Kind::Skill,
            folders: Some(vec![
                PathBuf::from(".claude/skills"),
                PathBuf::from(".agents/skills"),
            ]),
        },
        &AgentHomes::default(),
    )
    .unwrap();
    let api = MindProject::open(api.root()).unwrap();
    common::write_files(
        api.root(),
        &[
            ("skills/deploy/SKILL.md", &skill("deploy", "Ship it")),
            (
                ".agents/skills/review/SKILL.md",
                &skill("review", "Read it"),
            ),
            (
                ".claude/skills/review/SKILL.md",
                &skill("review", "Read it"),
            ),
        ],
    );
    create(&workspace.own(), Kind::Rule, "commits", "Write them well").unwrap();
    let parts = assign::install(
        &workspace,
        slice::from_ref(&api),
        &Reference::parse("rule:commits"),
        None,
        workspace.root(),
        &AgentHomes::default(),
        Edits::Kept,
        Confirmed::Yes,
    )
    .unwrap()
    .parts;
    for part in parts {
        assert!(
            part.done
                .as_ref()
                .is_ok_and(|offered| offered.installed.is_ok()),
            "{:?}",
            part.done
        );
    }
    let (gone, _) = MindProject::init(hub.join("gone")).unwrap();
    workspace.link(&gone).unwrap();
    fs::remove_dir_all(gone.root()).unwrap();

    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
    let notes = team.own().directory_for(Kind::Skill).unwrap();
    common::write_files(&notes, &[("notes/SKILL.md", &skill("notes", "Take them"))]);
    workspace.load(&team).unwrap();

    let ledger = workspace.ledger().unwrap();
    ledger
        .installed(
            "api",
            &InstallRecord {
                kind: Kind::Skill,
                name: "review".into(),
                source_id: None,
                origin: Some("workspace".into()),
                content_hash: Some(hash::tree(&library.join("review")).unwrap()),
                targets: [".agents/skills/review", ".claude/skills/review"]
                    .into_iter()
                    .map(|path| Target {
                        agent: None,
                        path: path.to_owned(),
                        content_hash: None,
                    })
                    .collect(),
                entries: Vec::new(),
            },
        )
        .unwrap();
    let url = "https://github.com/acme/skills";
    let source = ledger
        .source_for(SourceKind::Git, url, None, "skills")
        .unwrap();
    let revision = "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3";
    ledger.saw_source(source.id, Some(revision)).unwrap();
    let shelf = format!(".mindweaver/skills/{}/lint", source.directory);
    common::write_files(
        &hub.join(&shelf),
        &[("SKILL.md", &skill("lint", "Tidy it"))],
    );
    ledger
        .record(
            &source,
            Kind::Skill,
            "lint",
            &shelf,
            Some("Tidy it"),
            Some(revision),
        )
        .unwrap();
    ledger
        .log(
            Action::Gather,
            Some(url),
            Outcome::Ok,
            Some("1 added, 0 updated, 0 unchanged, 0 failed"),
        )
        .unwrap();
    drop(ledger);
    at_a_pinned_time(&workspace);
    Fixture { dir, hub }
}

/// Every time the ledger keeps, set to [`AT`].
fn at_a_pinned_time(workspace: &WeaverWorkspace) {
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(&format!(
            "UPDATE artifacts SET gathered_at = {AT}; UPDATE actions SET at = {AT};"
        ))
        .unwrap();
}

/// Inputs naming the workspace at `root`, with the default one in `home`.
fn naming<'a>(root: &'a Path, home: Option<&'a Path>) -> Inputs<'a> {
    Inputs {
        flag: Some(root),
        activated: None,
        start: root,
        home,
        intent: Intent::Read,
    }
}

/// `json` with every spelling of `root` in it as `<root>`, and `id` as
/// `<id>`, so it reads the same wherever the fixture was made.
fn placed(json: &str, root: &Path, id: &str) -> String {
    let escaped = serde_json::to_string(&root.to_string_lossy()).unwrap();
    let escaped = escaped.trim_matches('"');
    let json = json.replace(escaped, "<root>").replace(id, "<id>");
    if cfg!(windows) {
        json.replace("\\\\", "/")
    } else {
        json
    }
}

/// What [`fixture`] serializes to, with its folder as `<root>` and `api`'s
/// id as `<id>`: every name a front end reads, in the order it is written,
/// pinned whole, so a field that goes, moves or changes what it holds shows
/// here before it reaches the panel.
const EXPECTED: &str = r#"{
  "format": 1,
  "workspace": {
    "name": "hub",
    "path": "<root>/hub",
    "reason": "explicit",
    "made": true
  },
  "kinds": [
    "skill",
    "rule",
    "mcp"
  ],
  "agents": [
    "claude",
    "codex",
    "opencode"
  ],
  "states": [
    "current",
    "outdated",
    "edited",
    "missing",
    "orphaned",
    "foreign",
    "authored"
  ],
  "box_states": [
    "current",
    "outdated",
    "edited",
    "missing",
    "orphaned",
    "foreign"
  ],
  "healths": [
    "ok",
    "attention",
    "broken",
    "stale"
  ],
  "summary": {
    "projects": 1,
    "stale": 1,
    "held": {
      "total": 3,
      "kinds": [
        {
          "kind": "skill",
          "count": 2
        },
        {
          "kind": "rule",
          "count": 1
        },
        {
          "kind": "mcp",
          "count": 0
        }
      ],
      "states": [
        {
          "state": "current",
          "count": 2
        },
        {
          "state": "outdated",
          "count": 0
        },
        {
          "state": "edited",
          "count": 0
        },
        {
          "state": "missing",
          "count": 0
        },
        {
          "state": "orphaned",
          "count": 0
        },
        {
          "state": "foreign",
          "count": 0
        },
        {
          "state": "authored",
          "count": 1
        }
      ]
    },
    "library": {
      "total": 2,
      "kinds": [
        {
          "kind": "skill",
          "count": 1
        },
        {
          "kind": "rule",
          "count": 1
        },
        {
          "kind": "mcp",
          "count": 0
        }
      ],
      "states": [
        {
          "state": "current",
          "count": 0
        },
        {
          "state": "outdated",
          "count": 0
        },
        {
          "state": "edited",
          "count": 0
        },
        {
          "state": "missing",
          "count": 0
        },
        {
          "state": "orphaned",
          "count": 0
        },
        {
          "state": "foreign",
          "count": 0
        },
        {
          "state": "authored",
          "count": 0
        }
      ]
    },
    "shelf": {
      "total": 1,
      "kinds": [
        {
          "kind": "skill",
          "count": 1
        },
        {
          "kind": "rule",
          "count": 0
        },
        {
          "kind": "mcp",
          "count": 0
        }
      ],
      "states": [
        {
          "state": "current",
          "count": 0
        },
        {
          "state": "outdated",
          "count": 0
        },
        {
          "state": "edited",
          "count": 0
        },
        {
          "state": "missing",
          "count": 0
        },
        {
          "state": "orphaned",
          "count": 0
        },
        {
          "state": "foreign",
          "count": 0
        },
        {
          "state": "authored",
          "count": 0
        }
      ]
    }
  },
  "library": {
    "origin": "workspace",
    "directories": [
      {
        "kind": "skill",
        "path": ".mindweaver/library/skills"
      },
      {
        "kind": "rule",
        "path": ".mindweaver/library/rules"
      },
      {
        "kind": "mcp",
        "path": ".mindweaver/library/mcp"
      }
    ],
    "artifacts": [
      {
        "kind": "skill",
        "name": "review",
        "summary": "Read it",
        "state": null,
        "agents": [],
        "origin": null,
        "targets": [
          ".mindweaver/library/skills/review"
        ],
        "issues": [],
        "unread": null
      },
      {
        "kind": "rule",
        "name": "commits",
        "summary": "Write them well",
        "state": null,
        "agents": [],
        "origin": null,
        "targets": [
          ".mindweaver/library/rules/commits.md"
        ],
        "issues": [],
        "unread": null
      }
    ],
    "holders": [
      {
        "kind": "skill",
        "name": "review",
        "entry": "api"
      },
      {
        "kind": "rule",
        "name": "commits",
        "entry": "api"
      }
    ],
    "templates": [
      {
        "name": "full",
        "kind": "skill",
        "origin": null
      }
    ],
    "failures": [],
    "servers": []
  },
  "projects": [
    {
      "entry": "api",
      "path": "<root>/hub/api",
      "name": "api",
      "marker": "settings.toml",
      "migrates": false,
      "id": "<id>",
      "agents": [
        "claude",
        "codex"
      ],
      "settings": {
        "agents": [
          "claude",
          "codex"
        ],
        "share": "shared",
        "edit_tracked": false,
        "targets": [
          {
            "kind": "skill",
            "folders": [
              ".claude/skills",
              ".agents/skills"
            ]
          }
        ]
      },
      "stale": null,
      "health": "ok",
      "directories": [
        {
          "kind": "skill",
          "path": "skills"
        },
        {
          "kind": "rule",
          "path": "rules"
        }
      ],
      "artifacts": [
        {
          "kind": "skill",
          "name": "deploy",
          "summary": "Ship it",
          "state": "authored",
          "agents": [],
          "origin": null,
          "targets": [
            "skills/deploy"
          ],
          "issues": [],
          "unread": null
        },
        {
          "kind": "skill",
          "name": "review",
          "summary": "Read it",
          "state": "current",
          "agents": [
            "claude",
            "codex"
          ],
          "origin": "workspace",
          "targets": [
            ".agents/skills/review",
            ".claude/skills/review"
          ],
          "issues": [],
          "unread": null
        },
        {
          "kind": "rule",
          "name": "commits",
          "summary": null,
          "state": "current",
          "agents": [
            "claude",
            "codex"
          ],
          "origin": "workspace",
          "targets": [
            "AGENTS.md"
          ],
          "issues": [],
          "unread": null
        }
      ],
      "entries": [],
      "tally": {
        "total": 3,
        "kinds": [
          {
            "kind": "skill",
            "count": 2
          },
          {
            "kind": "rule",
            "count": 1
          },
          {
            "kind": "mcp",
            "count": 0
          }
        ],
        "states": [
          {
            "state": "current",
            "count": 2
          },
          {
            "state": "outdated",
            "count": 0
          },
          {
            "state": "edited",
            "count": 0
          },
          {
            "state": "missing",
            "count": 0
          },
          {
            "state": "orphaned",
            "count": 0
          },
          {
            "state": "foreign",
            "count": 0
          },
          {
            "state": "authored",
            "count": 1
          }
        ]
      },
      "templates": [
        {
          "name": "full",
          "kind": "skill",
          "origin": null
        }
      ],
      "unreached": [],
      "missed": [],
      "misplaced": [],
      "failures": []
    },
    {
      "entry": "gone",
      "path": "<root>/hub/gone",
      "name": null,
      "marker": null,
      "migrates": false,
      "id": null,
      "agents": [],
      "settings": null,
      "stale": {
        "code": "not_a_project",
        "message": "<root>/hub/gone: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)"
      },
      "health": "stale",
      "directories": [],
      "artifacts": [],
      "entries": [],
      "tally": {
        "total": 0,
        "kinds": [
          {
            "kind": "skill",
            "count": 0
          },
          {
            "kind": "rule",
            "count": 0
          },
          {
            "kind": "mcp",
            "count": 0
          }
        ],
        "states": [
          {
            "state": "current",
            "count": 0
          },
          {
            "state": "outdated",
            "count": 0
          },
          {
            "state": "edited",
            "count": 0
          },
          {
            "state": "missing",
            "count": 0
          },
          {
            "state": "orphaned",
            "count": 0
          },
          {
            "state": "foreign",
            "count": 0
          },
          {
            "state": "authored",
            "count": 0
          }
        ]
      },
      "templates": [],
      "unreached": [],
      "missed": [],
      "misplaced": [],
      "failures": []
    }
  ],
  "loaded": [
    {
      "entry": "../team",
      "path": "<root>/team",
      "name": "team",
      "offers": 1,
      "origin": "load:../team",
      "error": null
    }
  ],
  "shelf": [
    {
      "kind": "skill",
      "name": "lint",
      "path": ".mindweaver/skills/github.com-acme-skills/lint",
      "summary": "Tidy it",
      "revision": "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3",
      "gathered_at": 1790000000,
      "source": {
        "kind": "git",
        "url": "https://github.com/acme/skills",
        "reference": null,
        "subdirectory": "skills",
        "revision": "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3",
        "directory": "github.com-acme-skills",
        "name": "acme-skills"
      },
      "origin": "https://github.com/acme/skills"
    }
  ],
  "sources": [
    {
      "kind": "git",
      "url": "https://github.com/acme/skills",
      "reference": null,
      "subdirectory": "skills",
      "revision": "4f9c2a7e1b3d5f60a8c4e2b6d0f1a3c5e7b9d1f3",
      "directory": "github.com-acme-skills",
      "name": "acme-skills",
      "origin": "https://github.com/acme/skills"
    }
  ],
  "offers": [
    {
      "kind": "skill",
      "name": "review",
      "summary": "Read it",
      "place": "workspace",
      "origin": "workspace"
    },
    {
      "kind": "skill",
      "name": "notes",
      "summary": "Take them",
      "place": "loaded",
      "origin": "load:../team"
    },
    {
      "kind": "skill",
      "name": "lint",
      "summary": "Tidy it",
      "place": "shelf",
      "origin": "https://github.com/acme/skills"
    },
    {
      "kind": "rule",
      "name": "commits",
      "summary": "Write them well",
      "place": "workspace",
      "origin": "workspace"
    }
  ],
  "boxes": [
    {
      "entry": "api",
      "closed": [],
      "held": [
        {
          "kind": "skill",
          "name": "review",
          "origin": "workspace",
          "state": "current",
          "ticked": true
        },
        {
          "kind": "rule",
          "name": "commits",
          "origin": "workspace",
          "state": "current",
          "ticked": true
        }
      ]
    },
    {
      "entry": "gone",
      "closed": [
        {
          "kind": "skill",
          "why": {
            "code": "not_a_project",
            "message": "<root>/hub/gone: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)"
          }
        },
        {
          "kind": "rule",
          "why": {
            "code": "not_a_project",
            "message": "<root>/hub/gone: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)"
          }
        },
        {
          "kind": "mcp",
          "why": {
            "code": "not_a_project",
            "message": "<root>/hub/gone: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)"
          }
        }
      ],
      "held": []
    }
  ],
  "doctor": [
    {
      "entry": "api",
      "name": "api",
      "path": "<root>/hub/api",
      "findings": [
        {
          "code": "doctor_codex_no_git_root",
          "agents": [
            "codex"
          ],
          "path": "<root>/hub/api",
          "message": "<root>/hub/api: in no git work tree, so Codex reads its AGENTS.md only when started in that folder itself",
          "fix": "make the project a git repository (`git init`), or start Codex in that folder"
        }
      ],
      "failures": []
    },
    {
      "entry": "gone",
      "name": null,
      "path": "<root>/hub/gone",
      "findings": [],
      "failures": [
        {
          "code": "not_a_project",
          "message": "<root>/hub/gone: not a mind project (no .mind/settings.toml, .mind/settings.json or .mind/mind.toml)"
        }
      ]
    }
  ],
  "known": [
    {
      "name": "hub",
      "path": "<root>/hub",
      "default": false,
      "remembered": false,
      "in_use": true,
      "projects": 2,
      "stale_projects": 1,
      "stale": null,
      "loaded_as": null,
      "loadable": false
    }
  ],
  "history": [
    {
      "at": 1790000000,
      "action": "gather",
      "target": "https://github.com/acme/skills",
      "outcome": "ok",
      "detail": "1 added, 0 updated, 0 unchanged, 0 failed"
    },
    {
      "at": 1790000000,
      "action": "install",
      "target": "api",
      "outcome": "ok",
      "detail": "commits into api"
    }
  ],
  "warnings": []
}"#;

/// A snapshot's names are a contract, as a report's are: every key, and the
/// order each is written in.
#[test]
fn a_snapshot_serializes_every_name_in_order() {
    let fixture = fixture();
    let api = MindProject::open(fixture.hub.join("api")).unwrap();
    let id = api.settings().unwrap().id.to_string();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();
    let json = serde_json::to_string_pretty(&snapshot).unwrap();

    assert_eq!(placed(&json, fixture.dir.path(), &id), EXPECTED);
}

/// Every path under `root`, hidden ones included, with what each file holds.
fn tree(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path.clone());
                found.insert(path, None);
            } else {
                let bytes = fs::read(&path).unwrap();
                found.insert(path, Some(bytes));
            }
        }
    }
    found
}

/// A front end reads a snapshot at every update, so reading one must leave
/// the disk as it was: no ledger made or rewritten, no scrub, and no default
/// workspace made in a home that has none.
#[test]
fn building_a_snapshot_creates_no_path_and_never_the_home_workspace() {
    let fixture = fixture();
    let home = fixture.dir.path().join("home");
    fs::create_dir_all(&home).unwrap();
    let before = tree(fixture.dir.path());

    let named = Snapshot::read(&naming(&fixture.hub, Some(&home)), &Machine::default())
        .unwrap()
        .unwrap();
    // Nothing named and nothing above: the default one, as it would be,
    // even asked by inputs that mean to write, which a read is not.
    let defaulted = Snapshot::read(
        &Inputs {
            flag: None,
            activated: None,
            start: &home,
            home: Some(&home),
            intent: Intent::Write,
        },
        &Machine::default(),
    )
    .unwrap()
    .unwrap();

    assert_eq!(named.workspace.reason, Reason::Explicit);
    assert_eq!(defaulted.workspace.reason, Reason::Home);
    assert!(!defaulted.workspace.made);
    assert!(!home.join(WEAVER_DIR).exists());
    assert_eq!(tree(fixture.dir.path()), before);
}

/// A row an older build wrote before credentials were refused still holds
/// them until a command that writes opens the ledger; a snapshot is a read,
/// which scrubs nothing, and prints none of them all the same.
#[test]
fn no_url_in_a_snapshot_carries_userinfo_even_from_rows_written_before_the_scrub() {
    let fixture = fixture();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();
    let tainted = "https://me:ghp_t0ken@github.com/acme/skills";
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch(&format!(
            "UPDATE sources SET url = '{tainted}';
             UPDATE installations SET origin = '{tainted}';
             INSERT INTO actions (at, action, target, outcome, detail)
                 VALUES ({AT}, 'gather', '{tainted}', 'failed', '{tainted}: cannot be fetched');"
        ))
        .unwrap();
    let ledger = fs::read(workspace.ledger_path()).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();
    let json = serde_json::to_string(&snapshot).unwrap();

    assert!(!json.contains("ghp_t0ken"), "{json}");
    assert!(!json.contains("me:"), "{json}");
    assert!(json.contains("https://github.com/acme/skills"), "{json}");
    assert_eq!(snapshot.history.len(), 3, "{:?}", snapshot.history);
    assert_eq!(
        fs::read(workspace.ledger_path()).unwrap(),
        ledger,
        "a read scrubbed the ledger"
    );
}

/// The project `name` in [`fixture`], as a snapshot shows it.
fn read(fixture: &Fixture, name: &str) -> Project {
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();
    let project = MindProject::open(fixture.hub.join(name)).unwrap();
    Project::read(&project, Some(&workspace))
}

/// The one row `api` in [`fixture`] lists as `deploy`.
fn deploy(fixture: &Fixture) -> Held {
    let project = read(fixture, "api");
    assert!(project.failures.is_empty(), "{:?}", project.failures);
    let deploy: Vec<&Held> = project
        .artifacts
        .iter()
        .filter(|held| held.name == "deploy")
        .collect();
    assert_eq!(deploy.len(), 1, "{deploy:?}");
    deploy[0].clone()
}

/// A file that does not read is listed where it would have been, by the
/// name its place gives it and in place of a copy of that name in an
/// agent's folder, with why it does not read: the one most in need of
/// opening. Where it is is said on its own, so the copy that reads, whose
/// standing the row keeps, is never taken for it. What no artifact can be
/// named by is a failure of its holder.
#[test]
fn a_file_that_does_not_read_is_listed_by_its_place_with_why() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    fs::write(api.join("skills/deploy/SKILL.md"), "no front matter\n").unwrap();
    common::write_files(
        &api,
        &[(
            ".claude/skills/deploy/SKILL.md",
            &skill("deploy", "Ship it"),
        )],
    );

    let beside = deploy(&fixture);
    fs::remove_dir_all(api.join(".claude/skills/deploy")).unwrap();
    let alone = deploy(&fixture);

    // Where the copy is, by its own name, and the reason alone.
    assert_eq!(
        serde_json::to_value(&beside.unread).unwrap(),
        serde_json::json!({
            "path": "skills/deploy",
            "code": "front_matter_missing",
            "message": "the file does not start with a `---` front matter fence",
        })
    );
    assert_eq!(beside.summary, None);
    // How the name stands is the copy's that reads, somebody's in Claude's
    // folder, and both are where it is held.
    assert_eq!(beside.state, Some(State::Foreign));
    assert_eq!(beside.agents, [Agent::Claude]);
    assert_eq!(beside.targets, [".claude/skills/deploy", "skills/deploy"]);
    // With no copy that reads, nothing stands, and it is held where it is.
    assert_eq!(
        alone.unread.as_ref().map(|unread| unread.path.as_str()),
        Some("skills/deploy")
    );
    assert_eq!(alone.state, None);
    assert_eq!(alone.agents, []);
    assert_eq!(alone.targets, ["skills/deploy"]);
}

/// An install every copy of which has gone is still listed, missing, with
/// where its copies were put, so a front end can say what to put back.
#[test]
fn an_install_nothing_of_which_is_found_says_where_it_went() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    fs::remove_dir_all(api.join(".agents")).unwrap();
    fs::remove_dir_all(api.join(".claude")).unwrap();

    let project = read(&fixture, "api");

    let review = project
        .artifacts
        .iter()
        .find(|held| held.name == "review")
        .expect("listed");
    assert_eq!(review.state, Some(State::Missing));
    assert_eq!(review.agents, []);
    assert_eq!(review.origin.as_deref(), Some("workspace"));
    assert_eq!(
        review.targets,
        [".agents/skills/review", ".claude/skills/review"]
    );
}

/// A project whose installs are as they were put, beside its own work,
/// asks for nothing, and an entry that opens no project is stale: one word
/// each, which a front end sorts and filters by.
#[test]
fn a_project_with_nothing_to_do_is_ok_and_one_that_opens_none_is_stale() {
    let fixture = fixture();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();

    let healths: Vec<Health> = snapshot
        .projects
        .iter()
        .map(|project| project.health)
        .collect();
    assert_eq!(healths, [Health::Ok, Health::Stale]);
}

/// One project is cut from the snapshot by its entry, as the snapshot
/// lists it, a stale one included: and an entry no registry holds, a name
/// or another spelling of a linked folder among them, is refused as an
/// unlink of it is.
#[test]
fn one_project_is_the_snapshots_own_by_its_entry_and_no_other_spelling() {
    let fixture = fixture();
    let snapshot = || {
        Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
            .unwrap()
            .unwrap()
    };
    let listed = snapshot().projects;

    for (entry, at) in [("api", 0), ("gone", 1)] {
        assert_eq!(snapshot().project(entry).unwrap(), listed[at], "{entry}");
    }
    for entry in ["./api", "api/", "hub", ""] {
        let refused = snapshot().project(entry).unwrap_err();
        assert_eq!(refused.code(), "not_registered", "{entry}");
    }
}

/// A project an older marker marks, a `mind.toml` or a `settings.json`, is
/// one `mind migrate` moves, and the snapshot says so of those alone: one
/// its `settings.toml` marks, and an entry that opens no project, have
/// nothing to move.
#[test]
fn a_project_an_older_marker_marks_is_said_to_migrate_and_no_other() {
    let fixture = fixture();
    let mut workspace = WeaverWorkspace::open(&fixture.hub).unwrap();
    let older = fixture.hub.join("older");
    workspace
        .link(&MindProject::init(&older).unwrap().0)
        .unwrap();
    let json = fixture.hub.join("json");
    fs::create_dir_all(json.join(MIND_DIR)).unwrap();
    fs::write(
        json.join(MIND_DIR).join(SETTINGS_JSON_FILE),
        r#"{"version": 3, "id": "0123456789abcdef0123456789abcdef", "name": "json",
            "agents": ["claude"], "share": "shared", "directories": {}}"#,
    )
    .unwrap();
    workspace.link(&MindProject::open(&json).unwrap()).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();

    let migrates: Vec<(Option<&str>, bool)> = snapshot
        .projects
        .iter()
        .map(|project| (project.entry.as_deref(), project.migrates))
        .collect();
    assert_eq!(
        migrates,
        [
            (Some("api"), false),
            (Some("gone"), false),
            (Some("older"), true),
            (Some("json"), true),
        ]
    );
}

/// An edited copy asks for attention, a forced update or an uninstall,
/// and so does what `validate` only warns of; somebody's copy asks for
/// nothing, since it is never Mindweaver's to change.
#[test]
fn an_edited_copy_asks_for_attention_and_somebody_s_asks_for_nothing() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    common::write_files(
        &api,
        &[(
            ".claude/skills/legacy/SKILL.md",
            &skill("legacy", "Kept by hand"),
        )],
    );
    let foreign = read(&fixture, "api");
    fs::write(
        api.join(".claude/skills/review/SKILL.md"),
        skill("review", "Read it twice"),
    )
    .unwrap();
    let edited = read(&fixture, "api");

    assert!(foreign
        .artifacts
        .iter()
        .any(|held| held.name == "legacy" && held.state == Some(State::Foreign)));
    assert_eq!(foreign.health, Health::Ok);
    assert_eq!(edited.health, Health::Attention);
}

/// A file that does not read breaks its project, over anything else it
/// asks for: what to do cannot be told of a file nobody can read.
#[test]
fn a_file_that_does_not_read_breaks_its_project_over_an_edited_copy() {
    let fixture = fixture();
    let api = fixture.hub.join("api");
    fs::write(
        api.join(".claude/skills/review/SKILL.md"),
        skill("review", "Read it twice"),
    )
    .unwrap();
    fs::write(api.join("skills/deploy/SKILL.md"), "no front matter\n").unwrap();

    let project = read(&fixture, "api");

    assert_eq!(project.health, Health::Broken);
}

/// The overview's counts are of directories, as `known` counts them: a
/// second spelling of one is an entry of its own, which `unlink` is given,
/// and one project all the same, whose artifacts count once.
#[test]
fn the_summary_counts_each_directory_once_however_many_entries_spell_it() {
    let fixture = fixture();
    let marker = fixture.hub.join(WEAVER_DIR).join("weaver.toml");
    let text = fs::read_to_string(&marker).unwrap();
    assert!(text.contains("\"gone\""), "{text}");
    fs::write(&marker, text.replacen("\"gone\"", "\"gone\", \"./api\"", 1)).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();

    assert_eq!(snapshot.projects.len(), 3);
    assert_eq!(snapshot.summary.projects, 1);
    assert_eq!(snapshot.summary.stale, 1);
    assert_eq!(snapshot.summary.held.total, 3);
    assert_eq!(
        snapshot.summary.held.total,
        snapshot.projects[0].tally.total
    );
}

/// What a workspace loads is listed on its own as the snapshot lists it,
/// for a front end showing a project that workspace does not link.
#[test]
fn what_a_workspace_loads_is_listed_on_its_own_as_the_snapshot_lists_it() {
    let fixture = fixture();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();

    assert_eq!(LoadedWorkspace::of(&workspace), snapshot.loaded);
    assert_eq!(snapshot.loaded[0].offers, 1);
}

/// Which of the workspaces this machine knows the one a snapshot is of
/// could load is core's to say, beside each: not itself, which a load
/// refuses; not one it loads already, by the entry it loads it by; not one
/// that would not open; any other, the default in home among them.
#[test]
fn each_workspace_this_machine_knows_says_whether_this_one_loads_it_and_could() {
    let fixture = fixture();
    let home = fixture.dir.path().join("home");
    WeaverWorkspace::init(&home).unwrap();
    let kit = fixture.dir.path().join("kit");
    let old = fixture.dir.path().join("old");
    WeaverWorkspace::init(&kit).unwrap();
    WeaverWorkspace::init(&old).unwrap();
    for root in [&fixture.hub, &fixture.dir.path().join("team"), &kit, &old] {
        known::remember(Some(&home), &WeaverWorkspace::open(root).unwrap()).unwrap();
    }
    fs::remove_dir_all(&old).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, Some(&home)), &Machine::default())
        .unwrap()
        .unwrap();

    let stood: Vec<(String, bool, Option<&str>, bool)> = snapshot
        .known
        .iter()
        .map(|known| {
            (
                known
                    .workspace
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                known.workspace.in_use,
                known.loaded_as.as_deref(),
                known.loadable,
            )
        })
        .collect();
    assert_eq!(
        stood,
        [
            (String::from("home"), false, None, true),
            (String::from("hub"), true, None, false),
            (String::from("team"), false, Some("../team"), false),
            (String::from("kit"), false, None, true),
            (String::from("old"), false, None, false),
        ]
    );
    assert!(snapshot.known[4].workspace.stale.is_some());
}

/// A workspace loaded by a second spelling of its folder is loaded all the
/// same, by the first entry that spells it.
#[test]
fn a_workspace_loaded_by_another_spelling_is_said_to_be_loaded_by_the_first() {
    let fixture = fixture();
    let home = fixture.dir.path().join("home");
    WeaverWorkspace::init(&home).unwrap();
    let team = fixture.dir.path().join("team");
    known::remember(Some(&home), &WeaverWorkspace::open(&team).unwrap()).unwrap();
    let marker = fixture.hub.join(WEAVER_DIR).join("weaver.toml");
    let text = fs::read_to_string(&marker).unwrap();
    fs::write(
        &marker,
        text.replacen("\"../team\"", "\"./../team\", \"../team\"", 1),
    )
    .unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, Some(&home)), &Machine::default())
        .unwrap()
        .unwrap();

    let team = snapshot
        .known
        .iter()
        .find(|known| known.workspace.name.as_deref() == Some("team"))
        .unwrap_or_else(|| panic!("{:?}", snapshot.known));
    assert_eq!(team.loaded_as.as_deref(), Some("./../team"));
    assert!(!team.loadable);
}

/// Each origin a snapshot lists beside what is offered is what `--from`
/// takes for it: given back to an install, it names that one offer, which
/// says it with the same words. So a front end that prints it beside a
/// command prints a line that runs.
#[test]
fn every_origin_a_snapshot_lists_names_its_offer_to_from() {
    let fixture = fixture();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();
    let ledger = workspace.ledger_for_reading().unwrap();
    let api = MindProject::open(fixture.hub.join("api")).unwrap();
    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();
    let offered = |name: &str, origin: &str| {
        install::offered(&workspace, &ledger, &api, Kind::Skill, name, Some(origin))
            .unwrap_or_else(|error| panic!("{name} from {origin}: {error}"))
            .origin()
    };

    let [shelved] = snapshot.shelf.as_slice() else {
        panic!("{:?}", snapshot.shelf)
    };
    assert_eq!(
        offered(&shelved.gathered.name, &shelved.origin),
        shelved.origin
    );
    assert_eq!(snapshot.sources[0].origin, shelved.origin);
    assert_eq!(
        offered("review", snapshot.library.origin),
        snapshot.library.origin
    );
    let [team] = snapshot.loaded.as_slice() else {
        panic!("{:?}", snapshot.loaded)
    };
    assert_eq!(offered("notes", &team.origin), team.origin);
}

/// A second spelling of one loaded workspace offers what the first does,
/// under the first's origin, so it lists that origin too: its own would
/// name nothing an install could find.
#[test]
fn a_second_spelling_of_a_load_lists_the_origin_its_offers_go_by() {
    let fixture = fixture();
    let marker = fixture.hub.join(WEAVER_DIR).join("weaver.toml");
    let text = fs::read_to_string(&marker).unwrap();
    assert!(text.contains("\"../team\""), "{text}");
    fs::write(
        &marker,
        text.replacen("\"../team\"", "\"../team\", \"./../team\"", 1),
    )
    .unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();

    let origins: Vec<(&str, &str, usize)> = snapshot
        .loaded
        .iter()
        .map(|loaded| (loaded.entry.as_str(), loaded.origin.as_str(), loaded.offers))
        .collect();
    assert_eq!(
        origins,
        [
            ("../team", "load:../team", 1),
            ("./../team", "load:../team", 1)
        ]
    );
}

/// The projects holding a copy of each of the library's own, as core tells
/// them: each as the kind and name it holds a copy of, and the entry the
/// registry spells the project by.
fn holders(snapshot: &Snapshot) -> Vec<(Kind, &str, &str)> {
    snapshot
        .library
        .holders
        .iter()
        .map(|holder| (holder.kind, holder.name.as_str(), holder.entry.as_str()))
        .collect()
}

/// An install recorded before origins were kept names none, and is a copy
/// of one of the library's own all the same, as `mindweaver remove
/// --everywhere` takes it: its project holds a copy of it, though no word
/// beside the copy says so, so no front end comparing origins could tell.
#[test]
fn a_copy_recorded_before_origins_were_kept_is_a_copy_of_the_librarys_own() {
    let fixture = fixture();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch("UPDATE installations SET origin = NULL WHERE name = 'review';")
        .unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();

    assert_eq!(
        holders(&snapshot),
        [
            (Kind::Skill, "review", "api"),
            (Kind::Rule, "commits", "api")
        ]
    );
    let review = snapshot.projects[0]
        .artifacts
        .iter()
        .find(|held| held.name == "review")
        .unwrap();
    assert_eq!(
        (review.origin.as_deref(), review.state),
        (None, Some(State::Current))
    );
}

/// Such a record may as well be a copy of a loaded workspace's own of that
/// name, so while one offers it the record is taken for neither: saying it
/// is the library's would be a guess.
#[test]
fn a_copy_recorded_before_origins_were_kept_is_nobodys_while_a_load_offers_its_name() {
    let fixture = fixture();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();
    rusqlite::Connection::open(workspace.ledger_path())
        .unwrap()
        .execute_batch("UPDATE installations SET origin = NULL WHERE name = 'review';")
        .unwrap();
    let team = WeaverWorkspace::open(fixture.dir.path().join("team")).unwrap();
    common::write_files(
        &team.own().directory_for(Kind::Skill).unwrap(),
        &[("review/SKILL.md", &skill("review", "Read it"))],
    );

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();

    assert_eq!(holders(&snapshot), [(Kind::Rule, "commits", "api")]);
}

/// A listing of every kind is the snapshot's holders, as they are: what a
/// script prints of a project is what a front end draws of it.
#[test]
fn a_listing_of_every_kind_holds_what_the_snapshot_holds() {
    let fixture = fixture();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();
    let api = MindProject::open(fixture.hub.join("api")).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();
    let listing = Listing::of_workspace(&workspace, &Kind::ALL);
    let one = Listing::of_project(&api, Some(&workspace), &Kind::ALL);

    assert_eq!(listing.library.as_ref(), Some(&snapshot.library));
    assert_eq!(listing.projects, snapshot.projects);
    assert_eq!(one.library, None);
    assert_eq!(one.projects, [read(&fixture, "api")]);
}

/// A listing of one kind reads that kind alone: each holder's rows of it,
/// as the snapshot has them, and its folders and templates of it.
#[test]
fn a_listing_of_one_kind_holds_only_that_kind() {
    let fixture = fixture();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();
    let rules = Listing::of_workspace(&workspace, &[Kind::Rule]);

    let library = rules.library.unwrap();
    assert_eq!(names(&library.artifacts), ["commits"]);
    assert!(library
        .directories
        .iter()
        .all(|folder| folder.kind == Kind::Rule));
    assert!(library
        .templates
        .iter()
        .all(|template| template.kind == Kind::Rule));
    assert_eq!(rules.projects.len(), snapshot.projects.len());
    for (listed, whole) in rules.projects.iter().zip(&snapshot.projects) {
        let of_rules: Vec<Held> = whole
            .artifacts
            .iter()
            .filter(|held| held.kind == Kind::Rule)
            .cloned()
            .collect();
        assert_eq!(listed.artifacts, of_rules);
        assert_eq!(listed.stale, whole.stale);
        assert!(listed
            .directories
            .iter()
            .all(|folder| folder.kind == Kind::Rule));
        // Counted for the kind read, which is the only one a count is of.
        assert_eq!(listed.tally.total, of_rules.len());
        assert_eq!(
            listed
                .tally
                .kinds
                .iter()
                .map(|counted| (counted.kind, counted.count))
                .collect::<Vec<_>>(),
            [(Kind::Rule, of_rules.len())]
        );
    }
    assert_eq!(names(&rules.projects[0].artifacts), ["commits"]);
}

/// A listing of one kind tells each project's health from what it read, as
/// it counts the tally from it: a skill that does not read breaks the
/// project the snapshot holds, and is nothing to a listing of its rules.
#[test]
fn a_listing_of_one_kind_tells_health_from_that_kind_alone() {
    let fixture = fixture();
    fs::write(
        fixture.hub.join("api/skills/deploy/SKILL.md"),
        "no front matter\n",
    )
    .unwrap();
    let workspace = WeaverWorkspace::open(&fixture.hub).unwrap();

    let snapshot = Snapshot::read(&naming(&fixture.hub, None), &Machine::default())
        .unwrap()
        .unwrap();
    let skills = Listing::of_workspace(&workspace, &[Kind::Skill]);
    let rules = Listing::of_workspace(&workspace, &[Kind::Rule]);

    assert_eq!(snapshot.projects[0].name.as_deref(), Some("api"));
    assert_eq!(snapshot.projects[0].health, Health::Broken);
    assert_eq!(skills.projects[0].health, Health::Broken);
    assert_eq!(rules.projects[0].health, Health::Ok);
}

/// The name of each row, in order.
fn names(held: &[Held]) -> Vec<&str> {
    held.iter().map(|held| held.name.as_str()).collect()
}

/// A workspace whose library holds `given` as the MCP server `name`,
/// written by `add` as a command line tells it, and nothing else.
fn serving(name: &str, given: &Given) -> (TempDir, Snapshot) {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    lifecycle::add(
        Some(&workspace),
        &workspace.own(),
        Kind::Mcp,
        name,
        "A server",
        Start::Server(given),
    )
    .unwrap();
    let snapshot = Snapshot::read(&naming(dir.path(), None), &Machine::default())
        .unwrap()
        .unwrap();
    (dir, snapshot)
}

/// What each agent is said to run is what its own entry spells, a secret
/// read from a variable as that agent refers to one, and what each asks
/// before it starts a server is core's word for it; the server reads back
/// as what `add` was told, for a form to start from.
#[test]
fn each_of_the_librarys_servers_says_what_each_agent_runs_and_asks() {
    let variable = VariableNameMother::random();
    let given = Given::Program(Program {
        command: String::from("npx"),
        args: vec![String::from("-y"), String::from("@example/server")],
        env: Vec::new(),
        secret_env: Vec::new(),
        arg_env: vec![variable.clone()],
    });
    let name = McpMother::name();

    let (_dir, snapshot) = serving(&name, &given);

    let [server] = snapshot.library.servers.as_slice() else {
        panic!("{:?}", snapshot.library.servers);
    };
    assert_eq!(server.name, name);
    assert_eq!(server.given.as_ref(), Some(&given));
    let said: Vec<(Agent, Option<&str>, Starts, bool)> = server
        .agents
        .iter()
        .map(|given| {
            (
                given.agent,
                given.runs.as_ref().map(|runs| runs.line.as_str()),
                given.starts,
                given.refused.is_empty() && given.dropped.is_empty(),
            )
        })
        .collect();
    let claude = format!("npx -y @example/server ${{{variable}}}");
    let opencode = format!("npx -y @example/server {{env:{variable}}}");
    assert_eq!(
        said,
        vec![
            (
                Agent::Claude,
                Some(claude.as_str()),
                Starts::OnceApproved,
                true
            ),
            (Agent::Codex, None, Starts::InATrustedProject, false),
            (
                Agent::OpenCode,
                Some(opencode.as_str()),
                Starts::Unasked,
                true
            ),
        ]
    );
    // Codex has no interpolation, so an argument read from a variable is a
    // value its renderer is not handed, and said by its code.
    let codes: Vec<&str> = server.agents[1]
        .refused
        .iter()
        .map(|said| said.code)
        .collect();
    assert_eq!(codes, vec!["codex_mcp_input_in_text"]);
}

/// A server reached over server-sent events is refused the agents that
/// take none, each by the code `show` gives, and no line is said for them;
/// an address is said to be reached, and as the entry spells it.
#[test]
fn a_server_an_agent_cannot_be_given_says_why_and_runs_nothing_there() {
    let given = Given::Address(Address {
        url: String::from("https://example.com/sse"),
        events: true,
        ..Address::default()
    });

    let (_dir, snapshot) = serving(&McpMother::name(), &given);

    let server = &snapshot.library.servers[0];
    let runs: Vec<Option<(bool, &str)>> = server
        .agents
        .iter()
        .map(|given| {
            given
                .runs
                .as_ref()
                .map(|runs| (runs.reaches, runs.line.as_str()))
        })
        .collect();
    let refused: Vec<Vec<&str>> = server
        .agents
        .iter()
        .map(|given| given.refused.iter().map(|said| said.code).collect())
        .collect();
    assert_eq!(
        runs,
        vec![Some((true, "https://example.com/sse")), None, None]
    );
    assert_eq!(
        refused,
        vec![Vec::new(), vec!["sse_unsupported"], vec!["sse_unsupported"]]
    );
}

/// What a front end reads of a server of the library's own, by name: a
/// contract, as every name of the snapshot is.
#[test]
fn a_server_of_the_librarys_own_serializes_every_name_in_order() {
    let given = Given::Program(Program {
        command: String::from("npx"),
        secret_env: vec![VariableName::from("GITHUB_TOKEN")],
        ..Program::default()
    });

    let (_dir, snapshot) = serving("github", &given);

    let json = serde_json::to_value(&snapshot.library.servers[0]).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "name": "github",
            "given": {
                "type": "program",
                "command": "npx",
                "args": [],
                "env": [],
                "secret_env": ["GITHUB_TOKEN"],
                "arg_env": []
            },
            "agents": [
                {
                    "agent": "claude",
                    "refused": [],
                    "dropped": [],
                    "runs": {"reaches": false, "line": "npx"},
                    "starts": "once_approved"
                },
                {
                    "agent": "codex",
                    "refused": [],
                    "dropped": [],
                    "runs": {"reaches": false, "line": "npx"},
                    "starts": "in_a_trusted_project"
                },
                {
                    "agent": "opencode",
                    "refused": [],
                    "dropped": [],
                    "runs": {"reaches": false, "line": "npx"},
                    "starts": "unasked"
                }
            ]
        })
    );
}
