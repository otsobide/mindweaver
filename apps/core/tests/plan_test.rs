//! A batch of assignments planned in core (#78): which offer of a name a
//! project holds, and what a selection of boxes amounts to.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::slice;

use common::{repository, skill};
use mindweaver_core::doctor::Machine;
use mindweaver_core::gather::{self, Request};
use mindweaver_core::install::{self, Candidate, Cell, Change, Plan};
use mindweaver_core::ledger::Ledger;
use mindweaver_core::resolve::{Reason, Resolved};
use mindweaver_core::view::Snapshot;
use mindweaver_core::{
    create, Agent, AgentHomes, Directories, Kind, MindProject, Share, State, WeaverWorkspace,
    MIND_DIR, SETTINGS_FILE,
};
use tempfile::TempDir;

/// Under one directory: `home`, a workspace with a skill of its own,
/// `deploy`, linking a project for each of `names`, each naming every agent;
/// and `team`, a workspace it loads, with a `deploy` of its own too.
struct Fixture {
    dir: TempDir,
    workspace: WeaverWorkspace,
    team: WeaverWorkspace,
    projects: Vec<MindProject>,
}

fn given(names: &[&str]) -> Fixture {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path().join("home")).unwrap();
    let mut projects = Vec::new();
    for name in names {
        let root = dir.path().join("home").join(name);
        fs::create_dir(&root).unwrap();
        let (project, _) =
            MindProject::init_settings(&root, &Agent::ALL, Share::Shared, &Directories::default())
                .unwrap();
        workspace.link(&project).unwrap();
        projects.push(project);
    }
    create(&workspace.own(), Kind::Skill, "deploy", "Ship it").unwrap();
    let (team, _) = WeaverWorkspace::init(dir.path().join("team")).unwrap();
    create(&team.own(), Kind::Skill, "deploy", "Ship it the team's way").unwrap();
    workspace.load(&team).unwrap();
    Fixture {
        dir,
        workspace,
        team,
        projects,
    }
}

impl Fixture {
    /// Every skill `project` is offered, as the install screen lists them.
    fn surveyed(&self, project: usize) -> Vec<Candidate> {
        install::survey(
            &self.workspace,
            &self.reading(),
            &self.projects[project],
            Kind::Skill,
        )
        .unwrap()
    }

    /// The ledger as a read opens it, making none.
    fn reading(&self) -> Ledger {
        self.workspace.ledger_for_reading().unwrap()
    }

    /// Install `name` into `project` from `from`.
    fn install(&self, project: usize, name: &str, from: &str) {
        let project = &self.projects[project];
        let ledger = self.workspace.ledger().unwrap();
        let candidate = install::offered(
            &self.workspace,
            &ledger,
            project,
            Kind::Skill,
            name,
            Some(from),
        )
        .unwrap();
        install::install(
            &self.workspace,
            &ledger,
            project,
            &candidate,
            &AgentHomes::default(),
        )
        .unwrap();
    }

    /// Each offer `project` is surveyed for, by its origin, and whether the
    /// project holds it.
    fn held(&self, project: usize) -> Vec<(String, bool)> {
        let candidates = self.surveyed(project);
        let held = install::holding(
            &self.workspace,
            &self.reading(),
            &self.projects[project],
            &candidates,
        )
        .unwrap();
        candidates
            .iter()
            .map(|candidate| candidate.origin().into_string())
            .zip(held)
            .collect()
    }
}

/// Write `name`'s manifest in `workspace`'s own folder of skills anew.
fn rewrite(workspace: &WeaverWorkspace, name: &str, description: &str) {
    let folder = workspace.own().directory_for(Kind::Skill).unwrap();
    fs::write(folder.join(name).join("SKILL.md"), skill(name, description)).unwrap();
}

fn pairs(held: &[(&str, bool)]) -> Vec<(String, bool)> {
    held.iter()
        .map(|(origin, held)| ((*origin).to_owned(), *held))
        .collect()
}

// ---------------------------------------------------------------------------
// Which offer of a name a project holds
// ---------------------------------------------------------------------------

#[test]
fn a_name_the_project_does_not_hold_ticks_none_of_its_offers() {
    let fixture = given(&["alpha"]);

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", false)])
    );
}

#[test]
fn an_installation_ticks_the_offer_it_came_from_though_its_copy_is_neither_offers_now() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "load:../team");
    // The team moves on, so the copy is behind what it came from, and holds
    // what neither place offers: the content alone would tell neither, and
    // tick the first.
    rewrite(&fixture.team, "deploy", "Ship it the team's new way");
    let candidates = fixture.surveyed(0);
    assert_eq!(candidates[1].standing, install::Standing::Outdated);

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", true)])
    );
}

#[test]
fn an_installation_ticks_the_offer_it_came_from_though_another_offers_the_same() {
    let fixture = given(&["alpha"]);
    // Byte for byte the same in both places, so the content alone would
    // tick the first.
    rewrite(&fixture.workspace, "deploy", "Ship it");
    rewrite(&fixture.team, "deploy", "Ship it");
    fixture.install(0, "deploy", "load:../team");

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", true)])
    );
}

#[test]
fn somebodys_copy_ticks_every_offer_of_its_name() {
    let fixture = given(&["alpha"]);
    let theirs = fixture.projects[0].root().join(".claude/skills/deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Mine")).unwrap();

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", true), ("load:../team", true)])
    );
}

#[test]
fn an_installation_whose_copies_have_gone_ticks_none_of_its_offers() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "workspace");
    for folder in [".claude/skills", ".agents/skills"] {
        fs::remove_dir_all(fixture.projects[0].root().join(folder).join("deploy")).unwrap();
    }

    assert_eq!(
        fixture.held(0),
        pairs(&[("workspace", false), ("load:../team", false)])
    );
}

#[test]
fn an_installation_nothing_it_came_from_offers_ticks_the_first_offer_of_its_name() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "load:../team");
    fs::remove_dir_all(
        fixture
            .team
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join("deploy"),
    )
    .unwrap();

    // Still there, so its box is still ticked, for unticking it to take it
    // out: the only box its name has.
    assert_eq!(fixture.held(0), pairs(&[("workspace", true)]));
}

// ---------------------------------------------------------------------------
// What a selection amounts to, read off the disk
// ---------------------------------------------------------------------------

fn cell(project: &str, reference: &str, origin: &str, wanted: bool) -> Cell {
    Cell {
        project: project.to_owned(),
        reference: reference.to_owned(),
        origin: origin.into(),
        wanted,
    }
}

impl Fixture {
    fn planned(&self, selection: &[Cell]) -> Plan {
        install::plan(&self.workspace, selection).unwrap()
    }

    /// Change the manifest of `project`'s copies of `name`, as somebody
    /// editing an installed skill does.
    fn edit(&self, project: usize, name: &str) {
        for folder in [".claude/skills", ".agents/skills"] {
            let manifest = self.projects[project]
                .root()
                .join(folder)
                .join(name)
                .join("SKILL.md");
            fs::write(manifest, skill(name, "Mine now")).unwrap();
        }
    }
}

/// The code of each conflict, and of each box locked, in order.
fn codes(plan: &Plan) -> (Vec<&str>, Vec<&str>) {
    (
        plan.conflicts
            .iter()
            .map(|conflict| conflict.why.code)
            .collect(),
        plan.locked.iter().map(|locked| locked.why.code).collect(),
    )
}

fn owned(projects: &[&str]) -> Vec<String> {
    projects.iter().map(|&project| project.to_owned()).collect()
}

/// Every file under `dir` with what it holds, to tell that nothing changed.
fn files(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut found = BTreeMap::new();
    let mut folders = vec![dir.to_path_buf()];
    while let Some(folder) = folders.pop() {
        for entry in fs::read_dir(&folder).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                folders.push(entry.path());
            } else {
                found.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    found
}

#[test]
fn boxes_read_off_the_disk_plan_an_install_and_an_uninstall_naming_each_project_by_its_entry() {
    let fixture = given(&["alpha", "beta"]);
    fixture.install(0, "deploy", "workspace");

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", false),
        cell("beta", "skill:deploy", "load:../team", true),
        // Not held from there: asked to be as it is.
        cell("beta", "skill:deploy", "workspace", false),
    ]);

    assert_eq!(
        plan.changes,
        [
            Change::Uninstall {
                projects: owned(&["alpha"]),
                kind: Kind::Skill,
                name: "deploy".into(),
            },
            Change::Install {
                projects: owned(&["beta"]),
                kind: Kind::Skill,
                name: "deploy".into(),
                from: "load:../team".into(),
            },
        ]
    );
    assert_eq!(codes(&plan), (vec![], vec![]));
}

#[test]
fn an_offer_asked_for_in_place_of_the_one_a_project_holds_replaces_it() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "workspace");

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", false),
        cell("alpha", "skill:deploy", "load:../team", true),
    ]);

    assert_eq!(
        plan.changes,
        [Change::Install {
            projects: owned(&["alpha"]),
            kind: Kind::Skill,
            name: "deploy".into(),
            from: "load:../team".into(),
        }]
    );
}

#[test]
fn an_installation_nothing_offers_any_more_is_asked_away_by_the_origin_it_came_from() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "load:../team");
    fs::remove_dir_all(
        fixture
            .team
            .own()
            .directory_for(Kind::Skill)
            .unwrap()
            .join("deploy"),
    )
    .unwrap();

    let plan = fixture.planned(&[cell("alpha", "skill:deploy", "load:../team", false)]);

    assert_eq!(
        plan.changes,
        [Change::Uninstall {
            projects: owned(&["alpha"]),
            kind: Kind::Skill,
            name: "deploy".into(),
        }]
    );
}

#[test]
fn a_credential_typed_into_a_box_is_its_origin_as_printed_and_never_said_back() {
    let fixture = given(&["alpha"]);
    let source = repository(&[("skills/lint/SKILL.md", &skill("lint", "Check it"))]);
    // A URL, for the credential to have somewhere to be: a `file://` one,
    // so the gather needs no network.
    let address = format!("file://{}", source.path().display());
    let ledger = fixture.workspace.ledger().unwrap();
    gather::gather(&fixture.workspace, &ledger, &Request::git(address.clone())).unwrap();
    let typed = address.replacen("file://", "file://user:secret@", 1);

    let plan = fixture.planned(&[
        cell("alpha", "skill:lint", &typed, true),
        // The same offer, as every listing prints it: no clash.
        cell("alpha", "skill:lint", &address, true),
        cell("alpha", "skill:review", &typed, true),
    ]);

    assert_eq!(
        plan.changes,
        [Change::Install {
            projects: owned(&["alpha"]),
            kind: Kind::Skill,
            name: "lint".into(),
            from: address.clone().into(),
        }]
    );
    assert_eq!(codes(&plan), (vec![], vec!["not_offered"]));
    let json = serde_json::to_string(&plan).unwrap();
    assert!(!json.contains("secret"), "{json}");
    // As JSON spells it, which escapes a Windows path's backslashes.
    let origin = serde_json::to_string(&address).unwrap();
    assert!(json.contains(&format!(r#""origin":{origin}"#)), "{json}");
}

#[test]
fn a_bare_name_is_the_one_kind_its_origin_offers_it_as() {
    let fixture = given(&["alpha"]);
    create(
        &fixture.workspace.own(),
        Kind::Rule,
        "deploy",
        "How we ship",
    )
    .unwrap();

    let plan = fixture.planned(&[
        // The team offers no rule of the name.
        cell("alpha", "deploy", "load:../team", true),
        cell("alpha", "rule:deploy", "workspace", true),
    ]);

    assert_eq!(
        plan.changes,
        [
            Change::Install {
                projects: owned(&["alpha"]),
                kind: Kind::Skill,
                name: "deploy".into(),
                from: "load:../team".into(),
            },
            Change::Install {
                projects: owned(&["alpha"]),
                kind: Kind::Rule,
                name: "deploy".into(),
                from: "workspace".into(),
            },
        ]
    );
}

#[test]
fn a_plan_is_a_read_that_writes_nothing_and_makes_no_ledger() {
    let fixture = given(&["alpha", "beta"]);
    let before = files(fixture.dir.path());
    assert!(!fixture.workspace.ledger_path().exists());

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", true),
        cell("beta", "skill:deploy", "load:../team", true),
    ]);

    assert_eq!(plan.changes.len(), 2, "{plan:?}");
    assert!(!fixture.workspace.ledger_path().exists());
    assert_eq!(files(fixture.dir.path()), before);
}

#[test]
fn a_project_the_workspace_does_not_link_refuses_the_whole_plan() {
    let fixture = given(&["alpha"]);

    for entry in ["gamma", "--yes", "../team", "-C"] {
        let error = install::plan(
            &fixture.workspace,
            &[
                cell("alpha", "skill:deploy", "workspace", true),
                cell(entry, "skill:deploy", "workspace", true),
            ],
        )
        .unwrap_err();

        assert_eq!(error.code(), "not_registered", "{entry}: {error}");
    }
}

// ---------------------------------------------------------------------------
// Each conflict, and each lock, read off the disk
// ---------------------------------------------------------------------------

#[test]
fn one_name_asked_for_from_two_places_clashes() {
    let fixture = given(&["alpha"]);

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", true),
        cell("alpha", "skill:deploy", "load:../team", true),
    ]);

    assert_eq!(plan.changes, []);
    assert_eq!(codes(&plan), (vec!["wanted_twice"], vec![]));
}

#[test]
fn one_box_asked_both_ways_clashes() {
    let fixture = given(&["alpha"]);

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", true),
        cell("alpha", "skill:deploy", "workspace", false),
    ]);

    assert_eq!(plan.changes, []);
    assert_eq!(codes(&plan), (vec!["asked_both_ways"], vec![]));
}

#[test]
fn somebodys_copy_is_locked_as_foreign_when_asked_away() {
    let fixture = given(&["alpha"]);
    let theirs = fixture.projects[0].root().join(".claude/skills/deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Mine")).unwrap();

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", false),
        // There, whichever is asked for: as it is.
        cell("alpha", "skill:deploy", "load:../team", true),
    ]);

    assert_eq!(plan.changes, []);
    assert_eq!(codes(&plan), (vec![], vec!["foreign"]));
}

#[test]
fn a_copy_edited_since_it_was_installed_is_locked_as_edited() {
    let fixture = given(&["alpha"]);
    fixture.install(0, "deploy", "workspace");
    fixture.edit(0, "deploy");

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", false),
        cell("alpha", "skill:deploy", "load:../team", true),
    ]);

    assert_eq!(plan.changes, []);
    assert_eq!(codes(&plan), (vec![], vec!["edited", "edited"]));
}

#[test]
fn an_offer_its_origin_does_not_make_is_locked_as_not_offered() {
    let fixture = given(&["alpha"]);

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "load:../elsewhere", true),
        cell("alpha", "skill:review", "workspace", true),
        // Asked away from where nothing is held: as it is.
        cell("alpha", "skill:review", "load:../team", false),
    ]);

    assert_eq!(plan.changes, []);
    assert_eq!(codes(&plan), (vec![], vec!["not_offered", "not_offered"]));
    assert_eq!(
        plan.locked[0].why.message,
        "nothing called `deploy` can be installed from load:../elsewhere: gather it, load a \
         workspace that has it with `mindweaver load`, or add it to the workspace with `mindweaver add`"
    );
}

#[test]
fn a_held_offer_asked_away_beside_an_offer_nothing_offers_is_held_back_with_it() {
    let fixture = given(&["alpha", "beta"]);
    fixture.install(0, "deploy", "workspace");
    fixture.install(1, "deploy", "workspace");

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", false),
        cell("alpha", "skill:deploy", "load:../elsewhere", true),
        // Asked to be as it is: nothing to hold back.
        cell("alpha", "skill:deploy", "load:../team", false),
        cell("beta", "skill:deploy", "workspace", false),
    ]);

    // Planned without the box asked for, the one asked away would take
    // `deploy` out of alpha, which asked for another in its place.
    assert_eq!(
        plan.changes,
        [Change::Uninstall {
            projects: owned(&["beta"]),
            kind: Kind::Skill,
            name: "deploy".into(),
        }]
    );
    assert_eq!(codes(&plan), (vec![], vec!["not_offered", "not_offered"]));
    let locked: Vec<&str> = plan
        .locked
        .iter()
        .map(|locked| locked.cell.origin.as_str())
        .collect();
    assert_eq!(locked, ["workspace", "load:../elsewhere"]);
    assert_eq!(plan.locked[0].why, plan.locked[1].why);
}

#[test]
fn a_project_with_nowhere_to_put_a_kind_is_locked_as_install_says() {
    let fixture = given(&["alpha"]);
    let root = fixture.dir.path().join("home/gamma");
    fs::create_dir_all(root.join(MIND_DIR)).unwrap();
    fs::write(
        root.join(MIND_DIR).join(SETTINGS_FILE),
        "version = 5\nid = \"0123456789abcdef0123456789abcdef\"\nname = \"gamma\"\n\
         agents = []\nshare = \"shared\"\n",
    )
    .unwrap();
    let mut workspace = fixture.workspace.clone();
    workspace.link(&MindProject::open(&root).unwrap()).unwrap();

    let plan = install::plan(
        &workspace,
        &[
            cell("gamma", "skill:deploy", "workspace", true),
            cell("alpha", "skill:deploy", "workspace", true),
        ],
    )
    .unwrap();

    assert_eq!(codes(&plan), (vec![], vec!["no_target"]));
    assert_eq!(plan.changes.len(), 1, "{plan:?}");
}

#[test]
fn a_project_that_does_not_open_is_locked_by_why_it_does_not() {
    let fixture = given(&["alpha", "beta"]);
    fs::remove_dir_all(fixture.projects[1].root().join(MIND_DIR)).unwrap();
    let opening = fixture
        .workspace
        .members()
        .into_iter()
        .find(|member| member.entry == Path::new("beta"))
        .unwrap()
        .project
        .unwrap_err()
        .code();

    let plan = fixture.planned(&[
        cell("alpha", "skill:deploy", "workspace", true),
        cell("beta", "skill:deploy", "workspace", true),
    ]);

    assert_eq!(codes(&plan), (vec![], vec![opening]));
    assert_eq!(plan.changes.len(), 1, "{plan:?}");
}

#[test]
fn a_bare_name_two_kinds_are_offered_as_from_one_origin_is_locked_for_the_reference_to_say_which() {
    let fixture = given(&["alpha"]);
    create(
        &fixture.workspace.own(),
        Kind::Rule,
        "deploy",
        "How we ship",
    )
    .unwrap();

    let plan = fixture.planned(&[cell("alpha", "deploy", "workspace", true)]);

    assert_eq!(plan.changes, []);
    assert_eq!(codes(&plan), (vec![], vec!["offered_as_kinds"]));
}

// ---------------------------------------------------------------------------
// The boxes a snapshot shows
// ---------------------------------------------------------------------------

impl Fixture {
    /// The workspace as a front end reads it.
    fn snapshot(&self) -> Snapshot {
        let resolved = Resolved {
            workspace: WeaverWorkspace::open(self.workspace.root()).unwrap(),
            reason: Reason::Explicit,
        };
        Snapshot::of(&resolved, None, &Machine::default())
    }

    /// Each box of `kind` `name` in the project at `entry`, as a snapshot
    /// shows it: by the origin of each offer, its state and whether it is
    /// ticked, none for an empty one.
    fn shown(&self, entry: &str, name: &str) -> Vec<(String, Option<State>, bool)> {
        let snapshot = self.snapshot();
        let boxes = snapshot
            .boxes
            .iter()
            .find(|boxes| boxes.entry == entry)
            .unwrap();
        snapshot
            .offers
            .iter()
            .filter(|offered| offered.kind == Kind::Skill && offered.name == name)
            .map(|offered| {
                let held = boxes.held.iter().find(|held| {
                    held.kind == offered.kind
                        && held.name == offered.name
                        && held.origin == offered.origin
                });
                (
                    offered.origin.to_string(),
                    held.map(|held| held.state),
                    held.is_some_and(|held| held.ticked),
                )
            })
            .collect()
    }
}

/// A snapshot ticks the box the install screen ticks, each of its offers
/// standing as the name does: so the panel, which draws the snapshot, and
/// the screen, which surveys each project, open on the same boxes.
#[test]
fn a_snapshot_ticks_the_boxes_the_install_screen_ticks() {
    let fixture = given(&["alpha", "beta", "gamma", "delta"]);
    fixture.install(0, "deploy", "load:../team");
    let theirs = fixture.projects[1].root().join(".claude/skills/deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Mine")).unwrap();
    fixture.install(2, "deploy", "workspace");
    fixture.edit(2, "deploy");
    fixture.install(3, "deploy", "workspace");
    fs::remove_dir_all(fixture.projects[3].root().join(".claude/skills/deploy")).unwrap();

    let current = Some(State::Current);
    assert_eq!(
        fixture.shown("alpha", "deploy"),
        [
            (String::from("workspace"), current, false),
            (String::from("load:../team"), current, true),
        ]
    );
    let foreign = Some(State::Foreign);
    assert_eq!(
        fixture.shown("beta", "deploy"),
        [
            (String::from("workspace"), foreign, true),
            (String::from("load:../team"), foreign, true),
        ]
    );
    let edited = Some(State::Edited);
    assert_eq!(
        fixture.shown("gamma", "deploy"),
        [
            (String::from("workspace"), edited, true),
            (String::from("load:../team"), edited, false),
        ]
    );
    let missing = Some(State::Missing);
    assert_eq!(
        fixture.shown("delta", "deploy"),
        [
            (String::from("workspace"), missing, false),
            (String::from("load:../team"), missing, false),
        ]
    );
    for (project, entry) in ["alpha", "beta", "gamma", "delta"].into_iter().enumerate() {
        let screen: Vec<(String, bool)> = fixture
            .shown(entry, "deploy")
            .into_iter()
            .map(|(origin, _, ticked)| (origin, ticked))
            .collect();
        assert_eq!(screen, fixture.held(project), "{entry}");
    }
}

/// Each box asked to be as the snapshot shows it asks for nothing: a box a
/// matrix leaves as it opened is no change, whatever stands in it,
/// somebody's copy and an edited one included. Each on its own, as a front
/// end sends only the boxes somebody changed: somebody's copy ticks every
/// offer of its name, which asked for all at once is a name asked of a
/// project from two places.
#[test]
fn each_box_asked_as_a_snapshot_shows_it_plans_nothing() {
    let fixture = given(&["alpha", "beta", "gamma"]);
    fixture.install(0, "deploy", "load:../team");
    let theirs = fixture.projects[1].root().join(".agents/skills/deploy");
    fs::create_dir_all(&theirs).unwrap();
    fs::write(theirs.join("SKILL.md"), skill("deploy", "Mine")).unwrap();
    fixture.install(2, "deploy", "workspace");
    fixture.edit(2, "deploy");
    let snapshot = fixture.snapshot();

    let mut asked = 0;
    for boxes in &snapshot.boxes {
        for offered in &snapshot.offers {
            let ticked = boxes.held.iter().any(|held| {
                held.kind == offered.kind
                    && held.name == offered.name
                    && held.origin == offered.origin
                    && held.ticked
            });
            let shown = Cell::of(
                &boxes.entry,
                offered.kind,
                &offered.name,
                &offered.origin,
                ticked,
            );
            assert_eq!(
                fixture.planned(slice::from_ref(&shown)),
                Plan::default(),
                "{shown:?}"
            );
            asked += 1;
        }
    }
    assert_eq!(asked, 3 * 2, "{snapshot:?}");
}

/// A kind a project has nowhere to put closes its boxes, by the code a
/// plan locks one of them by: the panel says why before anything is asked.
#[test]
fn a_kind_a_project_has_nowhere_to_put_closes_its_boxes_by_the_code_a_plan_locks_them_by() {
    let mut fixture = given(&["alpha"]);
    let root = fixture.dir.path().join("home").join("bare");
    fs::create_dir(&root).unwrap();
    let (bare, _) =
        MindProject::init_settings(&root, &[], Share::Shared, &Directories::default()).unwrap();
    fixture.workspace.link(&bare).unwrap();
    let snapshot = fixture.snapshot();

    let boxes = snapshot
        .boxes
        .iter()
        .find(|boxes| boxes.entry == "bare")
        .unwrap();
    let closed: Vec<(Kind, &str)> = boxes
        .closed
        .iter()
        .map(|closed| (closed.kind, closed.why.code))
        .collect();
    assert_eq!(
        closed,
        [
            (Kind::Skill, "no_target"),
            (Kind::Rule, "no_target"),
            (Kind::Mcp, "no_target"),
        ]
    );
    assert_eq!(boxes.held, []);
    let plan = fixture.planned(&[cell("bare", "skill:deploy", "workspace", true)]);
    assert_eq!(codes(&plan), (vec![], vec!["no_target"]));
    assert_eq!(plan.locked[0].why, boxes.closed[0].why);
}

/// Somebody's MCP server of the name in a file an agent of the project
/// reads ticks its box, there whichever is asked for, and locks it asked
/// away by the code an uninstall refuses it with, as a copy nobody
/// installed does; the projects with none go ahead.
#[test]
fn an_mcp_server_somebody_keeps_in_an_agents_file_locks_its_box_as_foreign() {
    let fixture = given(&["alpha", "beta"]);
    let folder = fixture.workspace.own().directory_for(Kind::Mcp).unwrap();
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("github.toml"),
        mindweaver_core::mother::McpMother::plain_program("github"),
    )
    .unwrap();
    fs::write(
        fixture.projects[0].root().join(".mcp.json"),
        "{\n  \"mcpServers\": {\n    \"github\": {\n      \"command\": \"theirs\"\n    }\n  }\n}\n",
    )
    .unwrap();

    let plan = fixture.planned(&[
        cell("alpha", "mcp:github", "workspace", false),
        cell("beta", "mcp:github", "workspace", true),
    ]);

    assert_eq!(
        plan.changes,
        [Change::Install {
            projects: owned(&["beta"]),
            kind: Kind::Mcp,
            name: "github".into(),
            from: "workspace".into(),
        }]
    );
    assert_eq!(codes(&plan), (vec![], vec!["foreign"]));
    assert_eq!(
        plan.locked[0].cell,
        cell("alpha", "mcp:github", "workspace", false)
    );
    // And the snapshot's box says it is somebody's, as the plan locks it.
    let snapshot = fixture.snapshot();
    let held: Vec<(&str, State)> = snapshot
        .boxes
        .iter()
        .flat_map(|boxes| {
            boxes
                .held
                .iter()
                .filter(|held| held.kind == Kind::Mcp)
                .map(move |held| (boxes.entry.as_str(), held.state))
        })
        .collect();
    assert_eq!(held, [("alpha", State::Foreign)]);
}
