//! Unit tests for `licence.rs`, kept beside it rather than inside it.

use super::*;

use serde_json::json;
use tempfile::TempDir;

use crate::mother::{random, SkillMother};

/// A skill's folder holding `manifest` as its `SKILL.md` and `files`
/// beside it, and the skill read from it.
fn skill(manifest: &str, files: &[(&str, &[u8])]) -> (TempDir, Artifact) {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("deploy");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("SKILL.md"), manifest).unwrap();
    for (name, bytes) in files {
        let path = folder.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let artifact = Artifact::skill(&folder, dir.path()).unwrap();
    (dir, artifact)
}

fn licence_of(manifest: &str, files: &[(&str, &[u8])]) -> Licence {
    let (dir, artifact) = skill(manifest, files);
    Licence::of(&artifact, &dir.path().join("deploy"))
}

fn plain() -> String {
    SkillMother::create("deploy", "Ship it")
}

#[test]
fn the_licence_a_manifest_declares_is_kept_as_written_whatever_a_file_beside_it_says() {
    for _ in 0..20 {
        let declared = SkillMother::licence();
        let manifest = SkillMother::licensed("deploy", "Ship it", &declared);

        let licence = licence_of(&manifest, &[("LICENSE", b"GNU GENERAL PUBLIC LICENSE\n")]);

        assert_eq!(licence, Licence::Manifest { text: declared });
    }
}

#[test]
fn a_licence_written_empty_declares_none_and_the_file_beside_it_is_read() {
    for empty in ["\"\"", "\"  \""] {
        let manifest = SkillMother::licensed("deploy", "Ship it", empty);

        let licence = licence_of(&manifest, &[("LICENSE", b"MIT License\n")]);

        assert_eq!(
            licence,
            Licence::File {
                file: String::from("LICENSE"),
                text: Some(String::from("MIT License")),
            },
            "{empty}"
        );
    }
}

#[test]
fn a_licence_file_is_read_for_its_first_line_that_holds_anything_as_written() {
    let licence = licence_of(
        &plain(),
        &[(
            "LICENSE.txt",
            b"\xEF\xBB\xBF\n   \n                                 Apache License\n  Version 2.0\n",
        )],
    );

    assert_eq!(
        licence,
        Licence::File {
            file: String::from("LICENSE.txt"),
            text: Some(String::from("Apache License")),
        }
    );
}

#[test]
fn every_spelling_of_a_licence_file_is_one_and_nothing_else_is() {
    for name in [
        "LICENSE",
        "LICENCE",
        "license",
        "Licence.md",
        "LICENSE.txt",
        "LICENSE-MIT",
        "license-apache.md",
    ] {
        let licence = licence_of(&plain(), &[(name, b"Some licence\n")]);
        assert_eq!(
            licence,
            Licence::File {
                file: name.to_owned(),
                text: Some(String::from("Some licence")),
            },
            "{name}"
        );
    }
    for name in [
        "COPYING",
        "LICENSEE",
        "LICENSES",
        "MY-LICENSE",
        "README.md",
        "docs/LICENSE",
    ] {
        assert_eq!(
            licence_of(&plain(), &[(name, b"Some licence\n")]),
            Licence::Unknown,
            "{name}"
        );
    }
}

#[test]
fn a_folder_called_license_is_no_licence_file() {
    let (dir, artifact) = skill(&plain(), &[("LICENSE/MIT", b"MIT License\n")]);

    assert_eq!(
        Licence::of(&artifact, &dir.path().join("deploy")),
        Licence::Unknown
    );
}

#[test]
fn of_several_licence_files_the_first_by_name_is_read() {
    let licence = licence_of(
        &plain(),
        &[
            ("LICENSE-MIT", b"MIT License\n"),
            ("LICENSE-APACHE", b"Apache License\n"),
            ("LICENSE.md", b"# Both\n"),
        ],
    );

    assert_eq!(
        licence,
        Licence::File {
            file: String::from("LICENSE-APACHE"),
            text: Some(String::from("Apache License")),
        }
    );
}

#[test]
fn a_licence_file_with_nothing_readable_is_named_by_its_file_alone() {
    for (why, bytes) in [
        ("empty", b"".as_slice()),
        ("only space", b"\n \t\n\n".as_slice()),
        ("not text", b"\xFF\xFE\x00binary\n".as_slice()),
    ] {
        let licence = licence_of(&plain(), &[("LICENSE", bytes)]);

        assert_eq!(
            licence,
            Licence::File {
                file: String::from("LICENSE"),
                text: None,
            },
            "{why}"
        );
        assert_eq!(licence.said(), "LICENSE", "{why}");
    }
}

#[test]
fn a_line_past_what_is_read_or_kept_is_not_all_kept() {
    let long = "A".repeat(KEPT * 3);
    let licence = licence_of(&plain(), &[("LICENSE", long.as_bytes())]);
    assert_eq!(
        licence,
        Licence::File {
            file: String::from("LICENSE"),
            text: Some("A".repeat(KEPT)),
        }
    );

    let far = format!("{}MIT License\n", "\n".repeat(READ as usize));
    let licence = licence_of(&plain(), &[("LICENSE", far.as_bytes())]);
    assert_eq!(
        licence,
        Licence::File {
            file: String::from("LICENSE"),
            text: None,
        }
    );
}

#[test]
fn a_skill_that_says_nothing_of_its_licence_is_unknown_and_said_so() {
    let licence = licence_of(&plain(), &[("README.md", b"MIT License\n")]);

    assert_eq!(licence, Licence::Unknown);
    assert_eq!(licence.said(), "unknown");
}

#[test]
fn a_licence_is_written_tagged_by_where_it_was_read_beside_what_it_said() {
    let text = random::text(20);

    assert_eq!(
        serde_json::to_value(Licence::Manifest { text: text.clone() }).unwrap(),
        json!({"from": "manifest", "text": text})
    );
    assert_eq!(
        serde_json::to_value(Licence::File {
            file: String::from("LICENSE"),
            text: Some(text.clone()),
        })
        .unwrap(),
        json!({"from": "file", "file": "LICENSE", "text": text})
    );
    assert_eq!(
        serde_json::to_value(Licence::File {
            file: String::from("LICENSE"),
            text: None,
        })
        .unwrap(),
        json!({"from": "file", "file": "LICENSE", "text": null})
    );
    assert_eq!(
        serde_json::to_value(Licence::Unknown).unwrap(),
        json!({"from": "unknown"})
    );
    for licence in [
        Licence::Manifest { text: text.clone() },
        Licence::File {
            file: String::from("LICENSE"),
            text: None,
        },
        Licence::Unknown,
    ] {
        assert_eq!(
            serde_json::to_value(&licence).unwrap()["from"],
            json!(licence.slug())
        );
    }
}
