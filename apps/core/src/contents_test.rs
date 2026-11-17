//! Unit tests for `contents.rs`: what a front end is handed of an artifact's
//! files, and what it never is.

use std::fs;

use tempfile::TempDir;

use super::*;

/// A project holding the skill `deploy`, with a reference and an asset
/// beside its `SKILL.md`, and the rule `git/commits`.
fn fixture() -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init(dir.path().join("api")).unwrap();
    let skill = skill_folder(&project, "deploy");
    fs::create_dir_all(skill.join("references")).unwrap();
    fs::write(skill.join("SKILL.md"), SKILL).unwrap();
    fs::write(skill.join("references").join("guide.md"), "# Guide\n").unwrap();
    let rules = project.directory_for(Kind::Rule).unwrap().join("git");
    fs::create_dir_all(&rules).unwrap();
    fs::write(
        rules.join("commits.md"),
        "Write the subject in the imperative.\n",
    )
    .unwrap();
    fs::write(rules.join("branches.md"), "Branch from dev.\n").unwrap();
    (dir, project)
}

const SKILL: &str = "---\nname: deploy\ndescription: Ship it\n---\n\n# deploy\n";

fn skill_folder(project: &MindProject, name: &str) -> PathBuf {
    project.directory_for(Kind::Skill).unwrap().join(name)
}

fn code(result: Result<Contents, ContentsError>) -> &'static str {
    result.expect_err("the file was read").code()
}

#[test]
fn a_skills_manifest_comes_back_as_its_text() {
    let (_dir, project) = fixture();

    let contents = Contents::of_project(&project, "deploy", "SKILL.md").unwrap();

    assert_eq!(
        contents,
        Contents {
            kind: Kind::Skill,
            name: "deploy".into(),
            file: String::from("SKILL.md"),
            size: SKILL.len() as u64,
            hash: Some(hash::bytes(SKILL.as_bytes())),
            text: Some(String::from(SKILL)),
            withheld: None,
            manifest: Some(Manifest {
                front_matter: Some(vec![
                    declared("name", "deploy"),
                    declared("description", "Ship it"),
                ]),
                body: String::from("\n# deploy\n"),
            }),
        }
    );
}

#[test]
fn a_file_further_down_a_skill_is_named_by_its_route() {
    let (_dir, project) = fixture();

    let contents = Contents::of_project(&project, "skill:deploy", "references/guide.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some("# Guide\n"));
    assert_eq!(contents.file, "references/guide.md");
}

/// A route is plain names from the artifact's folder, so nothing that would
/// climb out of it, or start somewhere else, reaches the disk.
#[test]
fn a_route_that_is_not_plain_names_is_refused_before_the_disk_is_read() {
    let (_dir, project) = fixture();

    for file in [
        "../deploy/SKILL.md",
        "references/../SKILL.md",
        "./SKILL.md",
        "/etc/passwd",
        "",
        "references/",
        "references//guide.md",
        "references\\guide.md",
    ] {
        assert_eq!(
            code(Contents::of_project(&project, "deploy", file)),
            "contents_route_unfit",
            "{file:?}"
        );
    }
}

#[test]
fn a_rule_is_one_file_named_by_its_own_name() {
    let (_dir, project) = fixture();

    let contents = Contents::of_project(&project, "rule:git/commits", "commits.md").unwrap();

    assert_eq!(contents.kind, Kind::Rule);
    assert_eq!(contents.name, "git/commits");
    assert_eq!(
        contents.text.as_deref(),
        Some("Write the subject in the imperative.\n")
    );
    // Its neighbour is a rule of its own, not a file of this one.
    assert_eq!(
        code(Contents::of_project(&project, "git/commits", "branches.md")),
        "contents_route_unfit"
    );
}

#[test]
fn a_binary_file_is_reported_with_its_size_and_never_read_out() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    fs::write(skill.join("logo.png"), [0x89, b'P', b'N', b'G', 0, 0xff]).unwrap();
    // Not `nul.txt`, which Windows takes for its null device's name.
    fs::write(skill.join("zero.txt"), "plain\0text").unwrap();

    for (file, size) in [("logo.png", 6), ("zero.txt", 10)] {
        let contents = Contents::of_project(&project, "deploy", file).unwrap();
        assert_eq!(contents.text, None, "{file}");
        assert_eq!(contents.withheld, Some(Withheld::Binary), "{file}");
        assert_eq!(contents.size, size, "{file}");
        // Read whole all the same, so what it holds is hashed.
        let bytes = fs::read(skill.join(file)).unwrap();
        assert_eq!(contents.hash, Some(hash::bytes(&bytes)), "{file}");
    }
}

#[test]
fn a_file_over_the_cap_is_reported_with_its_size_and_never_read_out() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    let over = usize::try_from(CAP).unwrap() + 1;
    fs::write(skill.join("huge.md"), "a".repeat(over)).unwrap();
    fs::write(skill.join("full.md"), "a".repeat(over - 1)).unwrap();

    let huge = Contents::of_project(&project, "deploy", "huge.md").unwrap();
    assert_eq!(huge.text, None);
    assert_eq!(huge.withheld, Some(Withheld::TooLarge));
    assert_eq!(huge.size, CAP + 1);
    // Never read whole, so nothing it holds is hashed.
    assert_eq!(huge.hash, None);
    // The cap itself is still read.
    let full = Contents::of_project(&project, "deploy", "full.md").unwrap();
    assert_eq!(
        full.hash,
        Some(hash::bytes("a".repeat(over - 1).as_bytes()))
    );
    assert_eq!(full.text.map(|text| text.len() as u64), Some(CAP));
}

/// The asset a gathered skill might carry to run in a browser comes back as
/// text in a string, with nothing to say it is a page or an image.
#[test]
fn markup_comes_back_as_text_with_no_type_to_render_it_by() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    let page = "<html><script>fetch('/api/commands')</script></html>\n";
    let image = "<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>\n";
    fs::write(skill.join("page.html"), page).unwrap();
    fs::write(skill.join("logo.svg"), image).unwrap();

    for (file, text) in [("page.html", page), ("logo.svg", image)] {
        let contents = Contents::of_project(&project, "deploy", file).unwrap();
        assert_eq!(contents.text.as_deref(), Some(text));
        let json = serde_json::to_value(&contents).unwrap();
        let mut keys: Vec<&str> = json
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["file", "hash", "kind", "manifest", "name", "size", "text", "withheld"]
        );
        assert_eq!(json["manifest"], serde_json::Value::Null, "{file}");
    }
}

#[cfg(unix)]
#[test]
fn a_file_that_links_out_of_its_holder_is_refused() {
    let (dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    fs::write(dir.path().join("secret.env"), "TOKEN=1\n").unwrap();
    std::os::unix::fs::symlink(dir.path().join("secret.env"), skill.join("env.md")).unwrap();

    let error = Contents::of_project(&project, "deploy", "env.md").unwrap_err();

    assert_eq!(error.code(), "contents_outside");
    assert!(error.to_string().contains("secret.env"), "{error}");
}

/// Discovery follows the link to read what the skill declares, so the skill
/// is listed; its file is still not read out.
#[cfg(unix)]
#[test]
fn a_manifest_that_links_out_of_its_holder_is_refused() {
    let (dir, project) = fixture();
    let skill = skill_folder(&project, "linked");
    fs::create_dir_all(&skill).unwrap();
    let outside = dir.path().join("SKILL.md");
    fs::write(&outside, SKILL.replace("deploy", "linked")).unwrap();
    std::os::unix::fs::symlink(&outside, skill.join("SKILL.md")).unwrap();

    assert_eq!(
        code(Contents::of_project(&project, "linked", "SKILL.md")),
        "contents_outside"
    );
}

#[cfg(unix)]
#[test]
fn a_folder_that_links_out_of_its_holder_takes_its_files_with_it() {
    let (dir, project) = fixture();
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("notes.md"), "private\n").unwrap();
    std::os::unix::fs::symlink(&elsewhere, skill_folder(&project, "deploy").join("notes")).unwrap();

    assert_eq!(
        code(Contents::of_project(&project, "deploy", "notes/notes.md")),
        "contents_outside"
    );
}

#[cfg(unix)]
#[test]
fn a_link_that_stays_inside_its_holder_is_followed() {
    let (_dir, project) = fixture();
    let shared = project.root().join("docs");
    fs::create_dir_all(&shared).unwrap();
    fs::write(shared.join("style.md"), "Be brief.\n").unwrap();
    std::os::unix::fs::symlink(
        shared.join("style.md"),
        skill_folder(&project, "deploy").join("style.md"),
    )
    .unwrap();

    let contents = Contents::of_project(&project, "deploy", "style.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some("Be brief.\n"));
}

#[test]
fn nothing_there_and_a_folder_are_no_file_to_read() {
    let (_dir, project) = fixture();

    assert_eq!(
        code(Contents::of_project(&project, "deploy", "missing.md")),
        "contents_missing"
    );
    assert_eq!(
        code(Contents::of_project(&project, "deploy", "references")),
        "contents_not_a_file"
    );
}

#[test]
fn a_reference_names_one_artifact_or_is_refused() {
    let (_dir, project) = fixture();
    let rule = project.directory_for(Kind::Rule).unwrap().join("deploy.md");
    fs::write(rule, "Deploy on Tuesdays.\n").unwrap();

    assert_eq!(
        code(Contents::of_project(&project, "nothing", "SKILL.md")),
        "not_found"
    );
    assert_eq!(
        code(Contents::of_project(&project, "deploy", "SKILL.md")),
        "ambiguous_name"
    );
    assert_eq!(
        Contents::of_project(&project, "skill:deploy", "SKILL.md")
            .unwrap()
            .kind,
        Kind::Skill
    );
    assert_eq!(
        Contents::of_project(&project, "rule:deploy", "deploy.md")
            .unwrap()
            .kind,
        Kind::Rule
    );
}

/// A file that does not read is the one most in need of being looked at, as
/// a snapshot lists it under the name its place gives it.
#[test]
fn a_skill_whose_manifest_does_not_read_can_still_be_read_out() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "broken");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), "no front matter here\n").unwrap();

    let contents = Contents::of_project(&project, "broken", "SKILL.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some("no front matter here\n"));
    // Nothing declared, and all of it the body an agent would be handed.
    assert_eq!(
        contents.manifest,
        Some(Manifest {
            front_matter: None,
            body: String::from("no front matter here\n"),
        })
    );
}

#[test]
fn the_workspaces_own_are_read_from_its_library() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let skill = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), SKILL).unwrap();

    let contents = Contents::of_library(&workspace, "deploy", "SKILL.md").unwrap();

    assert_eq!(contents.text.as_deref(), Some(SKILL));
    assert_eq!(
        serde_json::to_value(&contents).unwrap(),
        serde_json::json!({
            "kind": "skill",
            "name": "deploy",
            "file": "SKILL.md",
            "size": SKILL.len(),
            "hash": hash::bytes(SKILL.as_bytes()),
            "text": SKILL,
            "withheld": null,
            "manifest": {
                "front_matter": [
                    {"key": "name", "value": "deploy"},
                    {"key": "description", "value": "Ship it"},
                ],
                "body": "\n# deploy\n",
            },
        })
    );
}

/// The default workspace's root is a home: a library file that links to
/// a secret kept there leads out of the library, though not out of the
/// root.
#[cfg(unix)]
#[test]
fn a_workspaces_own_file_that_links_out_of_its_library_is_refused() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let skill = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), SKILL).unwrap();
    let ssh = dir.path().join(".ssh");
    fs::create_dir_all(&ssh).unwrap();
    fs::write(ssh.join("id_ed25519"), "PRIVATE KEY\n").unwrap();
    std::os::unix::fs::symlink(ssh.join("id_ed25519"), skill.join("key.md")).unwrap();

    let error = Contents::of_library(&workspace, "deploy", "key.md").unwrap_err();

    assert_eq!(error.code(), "contents_outside");
    assert!(error.to_string().contains("id_ed25519"), "{error}");
}

#[test]
fn what_is_withheld_says_why_in_one_word() {
    assert_eq!(
        serde_json::to_value([Withheld::Binary, Withheld::TooLarge]).unwrap(),
        serde_json::json!(["binary", "too_large"])
    );
}

fn listed(path: &str, folder: bool, size: Option<u64>) -> Listed {
    Listed {
        path: path.to_owned(),
        folder,
        size,
        refused: None,
    }
}

/// The routes each listed entry, with the code of why it is not followed.
fn refusals(files: &Files) -> Vec<(&str, Option<&str>)> {
    files
        .entries
        .iter()
        .map(|entry| {
            (
                entry.path.as_str(),
                entry.refused.as_ref().map(|said| said.code),
            )
        })
        .collect()
}

#[test]
fn a_skill_lists_every_file_by_its_route_a_folder_before_what_it_holds() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    fs::create_dir_all(skill.join("scripts")).unwrap();
    fs::write(skill.join("scripts").join("ship.sh"), "exit 0\n").unwrap();
    fs::write(skill.join(".hidden"), "").unwrap();

    let files = Files::of_project(&project, "deploy").unwrap();

    assert_eq!(
        files,
        Files {
            kind: Kind::Skill,
            name: "deploy".into(),
            manifest: String::from("SKILL.md"),
            entries: vec![
                listed(".hidden", false, Some(0)),
                listed("SKILL.md", false, Some(SKILL.len() as u64)),
                listed("references", true, None),
                listed("references/guide.md", false, Some(8)),
                listed("scripts", true, None),
                listed("scripts/ship.sh", false, Some(7)),
            ],
            truncated: false,
        }
    );
    // Each route is one a file is read by.
    for entry in files.entries.iter().filter(|entry| !entry.folder) {
        Contents::of_project(&project, "deploy", &entry.path).unwrap();
    }
}

#[test]
fn a_rule_lists_its_one_file_by_its_own_name() {
    let (_dir, project) = fixture();

    let files = Files::of_project(&project, "rule:git/commits").unwrap();

    assert_eq!(files.name, "git/commits");
    assert_eq!(files.manifest, "commits.md");
    assert_eq!(files.entries, [listed("commits.md", false, Some(37))]);
}

#[test]
fn a_listing_names_its_artifact_as_a_file_of_it_does() {
    let (_dir, project) = fixture();

    assert_eq!(
        Files::of_project(&project, "nothing").unwrap_err().code(),
        "not_found"
    );
    assert_eq!(
        serde_json::to_value(Files::of_project(&project, "rule:git/commits").unwrap()).unwrap(),
        serde_json::json!({
            "kind": "rule",
            "name": "git/commits",
            "manifest": "commits.md",
            "entries": [
                {"path": "commits.md", "folder": false, "size": 37, "refused": null}
            ],
            "truncated": false,
        })
    );
}

/// A link that leads out is listed with why, and what it leads to is
/// neither listed nor measured; nor is a link back into a folder above it
/// walked, nor one that leads nowhere. The rest of the folder is listed
/// all the same.
#[cfg(unix)]
#[test]
fn a_link_out_back_up_or_nowhere_is_listed_with_why_and_never_followed() {
    use std::os::unix::fs::symlink;

    let (dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    let elsewhere = dir.path().join("elsewhere");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(elsewhere.join("id_ed25519"), "PRIVATE KEY\n").unwrap();
    symlink(&elsewhere, skill.join("keys")).unwrap();
    symlink(&skill, skill.join("references").join("up")).unwrap();
    symlink(skill.join("gone.md"), skill.join("dangling.md")).unwrap();

    let files = Files::of_project(&project, "deploy").unwrap();

    assert_eq!(
        refusals(&files),
        [
            ("SKILL.md", None),
            ("dangling.md", Some("contents_missing")),
            ("keys", Some("contents_outside")),
            ("references", None),
            ("references/guide.md", None),
            ("references/up", Some("contents_cycle")),
        ]
    );
    let keys = &files.entries[2];
    assert_eq!((keys.folder, keys.size), (false, None));
    let said = keys.refused.as_ref().unwrap();
    assert!(said.message.contains("elsewhere"), "{}", said.message);
    assert!(!files
        .entries
        .iter()
        .any(|entry| entry.path.contains("id_ed25519")));
}

/// A name holding a `\\` is no part of a route ([`place`] refuses it), so it
/// is listed with why rather than as a file to ask for, and a folder of
/// such a name is not walked.
#[cfg(unix)]
#[test]
fn a_name_holding_a_backslash_is_listed_with_why_and_never_opened() {
    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    fs::write(skill.join("a\\b.md"), "x\n").unwrap();
    fs::create_dir_all(skill.join("odd\\folder")).unwrap();
    fs::write(skill.join("odd\\folder").join("inside.md"), "y\n").unwrap();

    let files = Files::of_project(&project, "deploy").unwrap();

    assert_eq!(
        refusals(&files),
        [
            ("SKILL.md", None),
            ("a\\b.md", Some("contents_name_unfit")),
            ("odd\\folder", Some("contents_name_unfit")),
            ("references", None),
            ("references/guide.md", None),
        ]
    );
    let odd = &files.entries[1];
    assert_eq!((odd.folder, odd.size), (false, None));
    assert_eq!(
        code(Contents::of_project(&project, "deploy", "a\\b.md")),
        "contents_route_unfit"
    );
    // Every route listed as one to ask for is one a file is read by.
    for entry in files
        .entries
        .iter()
        .filter(|entry| !entry.folder && entry.refused.is_none())
    {
        Contents::of_project(&project, "deploy", &entry.path).unwrap();
    }
}

/// A name that is not UTF-8 has no spelling a file is asked for by: listed
/// as near as UTF-8 writes it, with why. Linux alone, whose file systems
/// take any bytes in a name, where macOS's refuse one that is not UTF-8.
#[cfg(target_os = "linux")]
#[test]
fn a_name_that_is_not_utf8_is_listed_with_why_and_never_opened() {
    use std::os::unix::ffi::OsStrExt;

    let (_dir, project) = fixture();
    let skill = skill_folder(&project, "deploy");
    fs::write(skill.join(OsStr::from_bytes(b"notes-\xff.md")), "x\n").unwrap();

    let files = Files::of_project(&project, "deploy").unwrap();

    assert_eq!(
        refusals(&files),
        [
            ("SKILL.md", None),
            ("notes-\u{fffd}.md", Some("contents_name_unfit")),
            ("references", None),
            ("references/guide.md", None),
        ]
    );
}

/// A folder linked from inside the holder is walked as the folder it leads
/// to, under the route of the link.
#[cfg(unix)]
#[test]
fn a_folder_linked_from_inside_its_holder_is_listed_under_the_link() {
    let (_dir, project) = fixture();
    let shared = project.root().join("docs");
    fs::create_dir_all(&shared).unwrap();
    fs::write(shared.join("style.md"), "Be brief.\n").unwrap();
    std::os::unix::fs::symlink(&shared, skill_folder(&project, "deploy").join("docs")).unwrap();

    let files = Files::of_project(&project, "deploy").unwrap();

    assert!(files.entries.contains(&listed("docs", true, None)));
    assert!(files
        .entries
        .contains(&listed("docs/style.md", false, Some(10))));
}

#[cfg(unix)]
#[test]
fn a_skill_whose_folder_links_out_of_its_holder_is_refused_whole() {
    let (dir, project) = fixture();
    let elsewhere = dir.path().join("linked");
    fs::create_dir_all(&elsewhere).unwrap();
    fs::write(
        elsewhere.join("SKILL.md"),
        SKILL.replace("deploy", "linked"),
    )
    .unwrap();
    std::os::unix::fs::symlink(&elsewhere, skill_folder(&project, "linked")).unwrap();

    assert_eq!(
        Files::of_project(&project, "linked").unwrap_err().code(),
        "contents_outside"
    );
}

#[test]
fn a_skill_holding_more_than_the_cap_is_listed_up_to_it() {
    let (_dir, project) = fixture();
    let many = skill_folder(&project, "deploy").join("many");
    fs::create_dir_all(&many).unwrap();
    for index in 0..FILES_CAP {
        fs::write(many.join(format!("{index:04}.md")), "").unwrap();
    }

    let files = Files::of_project(&project, "deploy").unwrap();

    assert_eq!(files.entries.len(), FILES_CAP);
    assert!(files.truncated);
    assert_eq!(files.entries[0].path, "SKILL.md");
}

/// As a file of one is: the default workspace's root is a home, so a link
/// to a secret kept there leads out of the library.
#[cfg(unix)]
#[test]
fn a_workspaces_own_is_listed_held_to_its_library() {
    let dir = TempDir::new().unwrap();
    let (workspace, _) = WeaverWorkspace::init(dir.path()).unwrap();
    let skill = workspace
        .own()
        .directory_for(Kind::Skill)
        .unwrap()
        .join("deploy");
    fs::create_dir_all(&skill).unwrap();
    fs::write(skill.join("SKILL.md"), SKILL).unwrap();
    let ssh = dir.path().join(".ssh");
    fs::create_dir_all(&ssh).unwrap();
    std::os::unix::fs::symlink(&ssh, skill.join("ssh")).unwrap();

    let files = Files::of_library(&workspace, "deploy").unwrap();

    assert_eq!(
        refusals(&files),
        [("SKILL.md", None), ("ssh", Some("contents_outside"))]
    );
}

fn declared(key: &str, value: &str) -> Declaration {
    Declaration {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

/// Every key, in the order written, the ones the format does not define
/// among them, each value a string whatever YAML read it as.
#[test]
fn a_skills_front_matter_is_every_key_in_order_each_value_as_written() {
    let (_dir, project) = fixture();
    fs::write(
        skill_folder(&project, "deploy").join("SKILL.md"),
        "---\nname: deploy\ndescription: Ship it\nx-team: ops\nallowed-tools: [Bash, Read]\n\
         metadata:\n  owner: you\nversion: 2\nlicense:\n---\nBody\n",
    )
    .unwrap();

    let contents = Contents::of_project(&project, "deploy", "SKILL.md").unwrap();

    assert_eq!(
        contents.manifest.unwrap(),
        Manifest {
            front_matter: Some(vec![
                declared("name", "deploy"),
                declared("description", "Ship it"),
                declared("x-team", "ops"),
                declared("allowed-tools", "- Bash\n- Read"),
                declared("metadata", "owner: you"),
                declared("version", "2"),
                declared("license", "null"),
            ]),
            body: String::from("Body\n"),
        }
    );
}

/// A skill's fences are cut wherever they close, as `validate` reads them,
/// though what is between them is no mapping to show.
#[test]
fn a_skills_front_matter_that_is_no_mapping_declares_nothing_and_is_cut_off() {
    let (_dir, project) = fixture();
    fs::write(
        skill_folder(&project, "deploy").join("SKILL.md"),
        "---\n- a list\n---\nBody\n",
    )
    .unwrap();

    let manifest = Contents::of_project(&project, "deploy", "SKILL.md")
        .unwrap()
        .manifest
        .unwrap();

    assert_eq!(manifest.front_matter, None);
    assert_eq!(manifest.body, "Body\n");
}

/// A rule's fences hold front matter only when they hold a mapping, as
/// every reader of a rule decides: anything else is content, fences and
/// all, which is what an agent reads.
#[test]
fn a_rules_front_matter_is_read_only_where_its_fences_hold_a_mapping() {
    let (_dir, project) = fixture();
    let rules = project.directory_for(Kind::Rule).unwrap().join("git");
    fs::write(
        rules.join("commits.md"),
        "---\npaths: [\"src/**\"]\n---\nSay why.\n",
    )
    .unwrap();
    fs::write(
        rules.join("branches.md"),
        "---\nA rule.\n---\nBranch from dev.\n",
    )
    .unwrap();

    let scoped = Contents::of_project(&project, "git/commits", "commits.md").unwrap();
    let prose = Contents::of_project(&project, "git/branches", "branches.md").unwrap();

    assert_eq!(
        scoped.manifest.unwrap(),
        Manifest {
            front_matter: Some(vec![declared("paths", "- src/**")]),
            body: String::from("Say why.\n"),
        }
    );
    assert_eq!(
        prose.manifest.unwrap(),
        Manifest {
            front_matter: None,
            body: String::from("---\nA rule.\n---\nBranch from dev.\n"),
        }
    );
}

/// Only the file an artifact is read from is cut: front matter in any other
/// is content no agent reads as such.
#[test]
fn no_other_file_of_a_skill_is_cut() {
    let (_dir, project) = fixture();
    fs::write(
        skill_folder(&project, "deploy")
            .join("references")
            .join("guide.md"),
        "---\ntitle: Guide\n---\n",
    )
    .unwrap();

    let guide = Contents::of_project(&project, "deploy", "references/guide.md").unwrap();

    assert_eq!(guide.manifest, None);
}
