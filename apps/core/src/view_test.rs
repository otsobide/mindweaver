//! Unit tests for `view.rs`: what a snapshot never does, which only a test
//! inside the crate can count.

use std::fs;

use tempfile::TempDir;

use super::*;
use crate::scan;

/// A workspace linking one project beside it, which holds a skill of its
/// own, and a project nobody linked, which a scan would find.
fn fixture() -> TempDir {
    let dir = TempDir::new().unwrap();
    let (mut workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let (project, _) = MindProject::init(dir.path().join("api")).unwrap();
    let skill = project.directory_for(Kind::Skill).unwrap().join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(
        skill.join("SKILL.md"),
        "---\nname: deploy\ndescription: Ship it\n---\n\n# deploy\n",
    )
    .unwrap();
    workspace.link(&project).unwrap();
    MindProject::init(dir.path().join("web")).unwrap();
    dir
}

/// Inputs naming the workspace at `root` and no default.
fn naming(root: &Path) -> Inputs<'_> {
    Inputs {
        flag: Some(root),
        activated: None,
        start: root,
        home: None,
        intent: Intent::Read,
    }
}

/// A live update reads a snapshot every time something changes, and a walk
/// of the disk is no price to pay for one: what could be linked is a query
/// of its own. Counted on this thread, where the snapshot is built.
#[test]
fn building_a_snapshot_runs_no_scan() {
    let dir = fixture();
    let before = scan::scans();

    let snapshot = Snapshot::read(&naming(dir.path()), &Machine::default())
        .unwrap()
        .unwrap();

    assert_eq!(scan::scans(), before, "the snapshot scanned");
    assert_eq!(snapshot.projects.len(), 1);
    // The counter counts: a scan of the same workspace is one.
    let workspace = WeaverWorkspace::open(dir.path()).unwrap();
    let found = scan::scan(&workspace, &scan::Request::default());
    assert_eq!(scan::scans(), before + 1);
    assert!(found.found.iter().any(|found| !found.linked));
}

/// What a source supplied says of its licences is counted by the words a
/// listing says each in, most first and then by those words: one `MIT` in
/// a manifest and one in a file are one licence, a file whose line could
/// not be read is said by its name, an artifact that says nothing is
/// `unknown`, and one gathered before licences were kept is in no count.
#[test]
fn a_source_counts_the_licences_what_it_supplied_says_by_their_words() {
    use crate::mother::{ArtifactNameMother, GatheredMother, LicenceMother, SourceMother};

    let source = SourceMother::create("https://github.com/acme/skills", None);
    // A row of its own, which a mother's id drawn at random may not be.
    let other = Source {
        id: source.id + 1,
        ..SourceMother::create("https://github.com/other/skills", None)
    };
    let with = |licence: Option<Licence>, from: &Source| Gathered {
        licence,
        ..GatheredMother::create(Kind::Skill, ArtifactNameMother::random(), from.clone())
    };
    let shelf = [
        with(Some(LicenceMother::manifest("MIT")), &source),
        with(Some(Licence::Unknown), &source),
        with(Some(LicenceMother::file("LICENSE", Some("MIT"))), &source),
        with(Some(LicenceMother::file("COPYING-NOTICE", None)), &source),
        with(None, &source),
        with(Some(LicenceMother::manifest("Apache-2.0")), &other),
    ];

    let listed = ShelfSource::all(vec![source.clone(), other], &shelf);

    let counted = |listed: &ShelfSource| -> Vec<(String, usize)> {
        listed
            .licences
            .iter()
            .map(|each| (each.licence.clone(), each.count))
            .collect()
    };
    assert_eq!(
        counted(&listed[0]),
        [
            (String::from("MIT"), 2),
            (String::from("COPYING-NOTICE"), 1),
            (String::from("unknown"), 1),
        ]
    );
    assert_eq!(counted(&listed[1]), [(String::from("Apache-2.0"), 1)]);
    assert!(ShelfSource::all(vec![source], &[])[0].licences.is_empty());
    assert_eq!(
        serde_json::to_value(&listed[1].licences).unwrap(),
        serde_json::json!([{"licence": "Apache-2.0", "count": 1}])
    );
}
