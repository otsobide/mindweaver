//! Unit tests for `lifecycle.rs`, kept beside it rather than inside it.
//!
//! The integration suite finds, edits, renames and removes artifacts in
//! projects; these hold the helpers each change is made of to their rules
//! one at a time: where an artifact lives, which artifact a file is, the
//! name line rewritten and nothing else, what contents are fit to be
//! written, what is inside a kind's folder, and which folders a removal
//! prunes.

use super::*;

use tempfile::TempDir;

use crate::mother::random;
use crate::mother::{ProjectMother, SkillMother};
use crate::settings::Share;

const DRAWS: usize = 200;

fn project() -> (TempDir, MindProject) {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::create(&dir.path().join("app"), &[], Share::Shared);
    (dir, project)
}

fn target(kind: Kind, name: &str, path: PathBuf) -> Target {
    Target {
        kind,
        name: ArtifactName::from(name),
        path,
        artifact: None,
    }
}

fn skill_at(project: &MindProject, name: &str) -> Target {
    let folder = project.directory_for(Kind::Skill).unwrap().join(name);
    target(Kind::Skill, name, folder)
}

fn rule_at(project: &MindProject, name: &str) -> Target {
    let file = project
        .directory_for(Kind::Rule)
        .unwrap()
        .join(format!("{name}.md"));
    target(Kind::Rule, name, file)
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn a_skill_is_opened_at_its_manifest_and_a_rule_at_itself() {
    let skill = target(Kind::Skill, "deploy", PathBuf::from("/p/skills/deploy"));
    let rule = target(Kind::Rule, "git/x", PathBuf::from("/p/rules/git/x.md"));

    assert_eq!(skill.file(), Path::new("/p/skills/deploy/SKILL.md"));
    assert_eq!(rule.file(), Path::new("/p/rules/git/x.md"));
}

#[test]
fn a_target_is_qualified_by_its_kind_s_slug_and_a_colon() {
    for _ in 0..DRAWS {
        let kind = random::pick(&Kind::ALL);
        let name = random::pick(&[
            random::kebab(),
            format!("{}/{}", random::kebab(), random::kebab()),
        ]);
        let qualified = target(kind, &name, PathBuf::from("/p")).qualified_name();

        assert_eq!(qualified, format!("{}:{name}", kind.slug()));
        assert_eq!(
            Reference::parse(&qualified).kind(),
            Some(kind),
            "{qualified}"
        );
        assert_eq!(Reference::parse(&qualified).name(), name, "{qualified}");
    }
}

#[test]
fn a_skill_lives_in_a_folder_of_its_name_and_a_rule_in_a_file_of_its_route() {
    let directory = Path::new("/p/kind");

    assert_eq!(
        place_in(directory, Kind::Skill, "deploy"),
        Path::new("/p/kind/deploy")
    );
    assert_eq!(
        place_in(directory, Kind::Rule, "deploy"),
        Path::new("/p/kind/deploy.md")
    );
    assert_eq!(
        place_in(directory, Kind::Rule, "git/no-force"),
        Path::new("/p/kind/git/no-force.md")
    );
}

#[test]
fn an_artifact_s_place_is_in_the_folder_its_project_keeps_the_kind_in() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::with_mcp(&dir.path().join("app"), &[], Share::Shared);
    for kind in Kind::ALL {
        let name = random::kebab();

        assert_eq!(
            place(&project, kind, &name),
            Some(place_in(&project.directory_for(kind).unwrap(), kind, &name)),
            "{kind:?} {name}"
        );
    }
}

#[test]
fn a_kind_the_project_keeps_nowhere_has_no_place() {
    let dir = TempDir::new().unwrap();
    let (project, _) = MindProject::init_settings(
        dir.path(),
        &[],
        Share::Shared,
        &crate::workspace::Directories::default().with(Kind::Skill, "skills"),
    )
    .unwrap();

    assert_eq!(place(&project, Kind::Rule, "lint"), None);
    assert!(place(&project, Kind::Skill, "deploy").is_some());
}

/// The place a name gives, read back, is that name: what lets a file that
/// does not read still be named from where it sits.
#[test]
fn the_file_at_any_artifact_s_place_identifies_it() {
    let dir = TempDir::new().unwrap();
    let project = ProjectMother::with_mcp(&dir.path().join("app"), &[], Share::Shared);
    for _ in 0..DRAWS {
        let kind = random::pick(&Kind::ALL);
        let name = match kind {
            Kind::Skill | Kind::Mcp => random::kebab(),
            Kind::Rule => crate::mother::ArtifactNameMother::route().into_string(),
        };
        let file = file_of(kind, &place(&project, kind, &name).unwrap());

        assert_eq!(
            identify(&project, &file),
            Some((kind, name.clone())),
            "{file:?}"
        );
    }
}

#[test]
fn a_skill_is_identified_by_its_manifest_alone() {
    let (_dir, project) = project();
    let skills = project.directory_for(Kind::Skill).unwrap();

    assert_eq!(
        identify(&project, &skills.join("deploy").join("SKILL.md")),
        Some((Kind::Skill, String::from("deploy")))
    );
    for other in [
        skills.join("deploy"),
        skills.join("deploy").join("README.md"),
        skills.join("deploy").join("scripts").join("SKILL.md"),
        skills.join("SKILL.md"),
        skills.clone(),
    ] {
        assert_eq!(identify(&project, &other), None, "{other:?}");
    }
}

#[test]
fn a_rule_is_identified_by_its_extension_in_any_case() {
    let (_dir, project) = project();
    let rules = project.directory_for(Kind::Rule).unwrap();

    assert_eq!(
        identify(&project, &rules.join("git").join("no-force.MD")),
        Some((Kind::Rule, String::from("git/no-force")))
    );
    for other in [rules.join("notes.txt"), rules.join("README"), rules.clone()] {
        assert_eq!(identify(&project, &other), None, "{other:?}");
    }
}

#[test]
fn a_file_outside_every_folder_the_project_keeps_is_no_artifact() {
    let (dir, project) = project();

    assert_eq!(identify(&project, &dir.path().join("elsewhere.md")), None);
    assert_eq!(identify(&project, &project.root().join("README.md")), None);
}

#[cfg(unix)]
#[test]
fn a_skill_whose_folder_is_not_utf_8_is_not_identified() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let (_dir, project) = project();
    let folder = project
        .directory_for(Kind::Skill)
        .unwrap()
        .join(OsStr::from_bytes(b"caf\xe9"));

    assert_eq!(identify(&project, &folder.join("SKILL.md")), None);
}

#[test]
fn an_artifact_is_held_where_its_file_is() {
    let dir = TempDir::new().unwrap();
    let skill = dir.path().join("deploy");
    let rule = dir.path().join("lint.md");

    assert!(!holds(Kind::Skill, &skill));
    fs::create_dir_all(&skill).unwrap();
    assert!(
        !holds(Kind::Skill, &skill),
        "a folder with no manifest holds none"
    );
    write(&skill.join("SKILL.md"), "x");
    assert!(holds(Kind::Skill, &skill));

    assert!(!holds(Kind::Rule, &rule));
    fs::create_dir_all(&rule).unwrap();
    assert!(!holds(Kind::Rule, &rule), "a folder is no rule's file");
}

#[test]
fn a_rename_rewrites_the_name_line_and_every_other_byte_stays() {
    let source =
        "---\nname: deploy\n# a comment\ndescription: Ship it\nlicense: MIT\n---\n\n# Deploy\n";

    assert_eq!(
        rewrite_name(source, "release").unwrap(),
        "---\nname: release\n# a comment\ndescription: Ship it\nlicense: MIT\n---\n\n# Deploy\n"
    );
}

#[test]
fn any_name_rewritten_reads_back_as_that_name_and_nothing_else_moves() {
    for _ in 0..DRAWS {
        let (name, source) = SkillMother::random();
        let to = random::kebab();

        // As YAML writes it: a name of digits alone, which a kebab name
        // may be, is quoted so it stays a string.
        let spelled = serde_yaml_ng::to_string(&to).unwrap();

        let rewritten = rewrite_name(&source, &to).unwrap();

        assert_eq!(
            rewritten,
            source.replacen(
                &format!("name: {name}\n"),
                &format!("name: {}\n", spelled.trim_end()),
                1
            ),
            "{name} to {to}"
        );
        let front = frontmatter::split(&rewritten).unwrap().front_matter;
        assert_eq!(SkillManifest::parse(front).unwrap().name, to);
    }
}

#[test]
fn a_rename_keeps_crlf_endings_on_the_name_line() {
    let source = "---\r\nname: deploy\r\ndescription: Ship it\r\n---\r\nBody.\r\n";

    assert_eq!(
        rewrite_name(source, "release").unwrap(),
        "---\r\nname: release\r\ndescription: Ship it\r\n---\r\nBody.\r\n"
    );
}

/// Serialized, so a name YAML would read as another type stays a string.
#[test]
fn a_name_yaml_would_read_as_something_else_is_written_quoted() {
    let source = SkillMother::create("deploy", "Ship it");
    for to in ["true", "123", "null", "1e3"] {
        let rewritten = rewrite_name(&source, to).unwrap();
        let front = frontmatter::split(&rewritten).unwrap().front_matter;

        assert_eq!(SkillManifest::parse(front).unwrap().name, to, "{rewritten}");
        assert!(!rewritten.contains(&format!("name: {to}\n")), "{rewritten}");
    }
}

#[test]
fn a_name_that_carries_on_below_its_key_is_not_rewritten() {
    for source in [
        "---\nname: >\n  deploy\ndescription: Ship it\n---\n",
        "---\nname: |\n  deploy\ndescription: Ship it\n---\n",
        "---\nname:\n  deploy\ndescription: Ship it\n---\n",
    ] {
        let refused = rewrite_name(source, "release").unwrap_err();

        assert_eq!(
            refused, "its `name` is not on a line of its own",
            "{source:?}"
        );
    }
}

#[test]
fn a_manifest_with_no_front_matter_or_none_that_reads_is_not_rewritten() {
    for source in [
        SkillMother::without_front_matter("deploy"),
        String::from("---\nname: [deploy\n---\n"),
    ] {
        assert!(rewrite_name(&source, "release").is_err(), "{source:?}");
    }
}

#[test]
fn contents_that_are_no_text_are_unfit_for_any_artifact() {
    let (_dir, project) = project();
    for target in [skill_at(&project, "deploy"), rule_at(&project, "lint")] {
        let refused = fit(&target, b"\xff\xfe\x00 not text").unwrap_err();

        assert!(
            matches!(refused, LifecycleError::NotText { .. }),
            "{refused:?}"
        );
        assert_eq!(refused.code(), "edit_contents_not_text");
    }
}

#[test]
fn a_skill_s_contents_that_name_its_folder_and_keep_the_rules_are_fit() {
    let (_dir, project) = project();
    for _ in 0..DRAWS {
        let (name, text) = SkillMother::random();

        assert!(
            fit(&skill_at(&project, &name), text.as_bytes()).is_ok(),
            "{text}"
        );
    }
}

#[test]
fn a_skill_s_contents_without_front_matter_cannot_be_read_as_a_skill_s() {
    let (_dir, project) = project();
    let text = SkillMother::without_front_matter("deploy");

    let refused = fit(&skill_at(&project, "deploy"), text.as_bytes()).unwrap_err();

    assert!(
        matches!(refused, LifecycleError::Unparsed { .. }),
        "{refused:?}"
    );
}

/// The folder is the name an agent finds a skill by: contents calling it
/// anything else are pointed at the rename that moves both.
#[test]
fn a_skill_s_contents_naming_it_other_than_its_folder_are_refused() {
    let (_dir, project) = project();
    let text = SkillMother::create("release", "Ship it");

    let refused = fit(&skill_at(&project, "deploy"), text.as_bytes()).unwrap_err();

    assert!(
        matches!(&refused, LifecycleError::NameChanged { name, to } if name == "deploy" && to == "release"),
        "{refused:?}"
    );
}

#[test]
fn a_skill_s_contents_breaking_a_strict_rule_are_unfit() {
    let (_dir, project) = project();
    let text = SkillMother::with_long_description("deploy");

    let refused = fit(&skill_at(&project, "deploy"), text.as_bytes()).unwrap_err();

    assert!(
        matches!(refused, LifecycleError::ContentsUnfit { .. }),
        "{refused:?}"
    );
    assert_eq!(refused.code(), "edit_contents_unfit");
}

#[test]
fn a_rule_s_contents_that_say_something_are_fit() {
    let (_dir, project) = project();
    for contents in [
        crate::mother::RuleMother::random(),
        crate::mother::RuleMother::scoped("rust", &["src/**/*.rs"]),
    ] {
        assert!(
            fit(&rule_at(&project, "lint"), contents.as_bytes()).is_ok(),
            "{contents}"
        );
    }
}

#[test]
fn a_rule_s_contents_with_nothing_to_say_are_unfit() {
    let (_dir, project) = project();
    for contents in ["", "\n\n", "---\npaths:\n  - \"x\"\n---\n"] {
        let refused = fit(&rule_at(&project, "lint"), contents.as_bytes()).unwrap_err();

        assert!(
            matches!(refused, LifecycleError::ContentsUnfit { .. }),
            "{contents:?} gave {refused:?}"
        );
    }
}

#[test]
fn a_target_inside_its_kind_s_folder_is_there_to_change() {
    let (_dir, project) = project();
    for target in [skill_at(&project, "deploy"), rule_at(&project, "git/x")] {
        assert_eq!(
            inside(&project, &target).unwrap(),
            project.directory_for(target.kind).unwrap()
        );
    }
}

/// By components, as a path is compared: a sibling folder whose name only
/// begins with the kind's is outside it, and the kind's folder itself is
/// nothing to delete.
#[test]
fn a_target_at_its_kind_s_folder_or_beside_it_is_outside() {
    let (_dir, project) = project();
    let skills = project.directory_for(Kind::Skill).unwrap();
    let mut beside = skills.clone().into_os_string();
    beside.push("-old");
    for path in [
        skills.clone(),
        PathBuf::from(beside).join("deploy"),
        project.root().join("deploy"),
    ] {
        let refused = inside(&project, &target(Kind::Skill, "deploy", path.clone())).unwrap_err();

        assert!(
            matches!(&refused, LifecycleError::Outside { path: at, directory } if at == &path && directory == &skills),
            "{path:?} gave {refused:?}"
        );
        assert_eq!(refused.code(), "change_outside");
    }
}

#[test]
fn something_is_there_once_anything_is_even_a_link_to_nothing() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("x");

    assert!(!occupied(&path));
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("nowhere"), &path).unwrap();
        assert!(occupied(&path));
        assert!(!path.exists(), "a link to nothing");
    }
}

#[test]
fn pruning_takes_the_empty_folders_up_to_the_kind_s_folder_and_keeps_it() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("rules");
    let deepest = root.join("a").join("b").join("c");
    fs::create_dir_all(&deepest).unwrap();

    prune(&root, Some(&deepest));

    assert!(!root.join("a").exists());
    assert!(root.is_dir(), "the kind's folder stays");
}

#[test]
fn pruning_stops_at_the_first_folder_holding_something() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("rules");
    let deepest = root.join("a").join("b").join("c");
    fs::create_dir_all(&deepest).unwrap();
    write(&root.join("a").join("kept.md"), "x");

    prune(&root, Some(&deepest));

    assert!(!root.join("a").join("b").exists());
    assert!(root.join("a").join("kept.md").is_file());
}

#[test]
fn pruning_from_outside_the_kind_s_folder_or_from_nowhere_removes_nothing() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().join("rules");
    let outside = dir.path().join("other").join("empty");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&outside).unwrap();

    prune(&root, Some(&outside));
    prune(&root, None);
    prune(&root, Some(&root));

    assert!(outside.is_dir());
    assert!(root.is_dir());
}

#[test]
fn removing_a_skill_deletes_every_file_it_holds_at_any_depth() {
    let dir = TempDir::new().unwrap();
    let folder = dir.path().join("deploy");
    let count = random::between(1, 6);
    write(&folder.join("SKILL.md"), "x");
    for n in 1..count {
        write(&folder.join("scripts").join(format!("{n}.sh")), "x");
    }
    fs::create_dir_all(folder.join("empty")).unwrap();

    assert_eq!(files(&target(Kind::Skill, "deploy", folder)), count);
}

/// A folder that cannot be listed, or is not there, still counts as the
/// one thing a removal deletes.
#[test]
fn a_rule_and_a_skill_that_cannot_be_listed_are_one_file() {
    let dir = TempDir::new().unwrap();

    assert_eq!(
        files(&target(Kind::Rule, "lint", dir.path().join("lint.md"))),
        1
    );
    assert_eq!(
        files(&target(Kind::Skill, "gone", dir.path().join("gone"))),
        1
    );
}

#[test]
fn no_workspace_is_no_library() {
    for kind in Kind::ALL {
        assert!(!library(
            None,
            kind,
            Path::new("/home/you/.mindweaver/skills")
        ));
    }
}

#[test]
fn the_workspace_s_own_folder_of_a_kind_it_keeps_apart_is_the_library() {
    let (_dir, workspace) = crate::mother::WorkspaceMother::empty();
    for kind in workspace.own_kinds() {
        let folder = workspace.own().directory_for(kind).unwrap();

        assert!(library(Some(&workspace), kind, &folder), "{kind:?}");
        assert!(
            !library(Some(&workspace), kind, &folder.join("x")),
            "{kind:?}"
        );
    }
}

#[test]
fn a_file_leading_out_of_its_kind_s_folder_through_a_link_is_never_written_at() {
    #[cfg(unix)]
    {
        let (dir, project) = project();
        let target = skill_at(&project, "deploy");
        fs::create_dir_all(&target.path).unwrap();
        let elsewhere = dir.path().join("bashrc");
        write(&elsewhere, "export X=1\n");
        std::os::unix::fs::symlink(&elsewhere, target.file()).unwrap();
        let directory = project.directory_for(Kind::Skill).unwrap();

        let refused = written_at(None, &project, &target, &directory).unwrap_err();

        assert!(
            matches!(refused, LifecycleError::LeadsOut { .. }),
            "{refused:?}"
        );
        assert_eq!(refused.code(), "edit_leads_outside");
        assert_eq!(fs::read_to_string(&elsewhere).unwrap(), "export X=1\n");
    }
}

#[test]
fn a_file_inside_every_bound_is_written_at_where_it_resolves() {
    let (_dir, project) = project();
    let target = rule_at(&project, "lint");
    write(&target.path, "# Lint\n");
    let directory = project.directory_for(Kind::Rule).unwrap();

    assert_eq!(
        written_at(None, &project, &target, &directory).unwrap(),
        crate::paths::canonical(&target.path).unwrap()
    );
}

#[test]
fn a_file_that_is_not_there_cannot_be_written_at() {
    let (_dir, project) = project();
    let target = rule_at(&project, "gone");
    let directory = project.directory_for(Kind::Rule).unwrap();

    let refused = written_at(None, &project, &target, &directory).unwrap_err();

    assert!(matches!(refused, LifecycleError::Io { .. }), "{refused:?}");
    assert_eq!(refused.code(), "change_failed");
}

#[test]
fn an_ambiguous_name_says_every_artifact_it_names() {
    let error = LifecycleError::Ambiguous {
        typed: String::from("deploy"),
        matches: vec![String::from("skill:deploy"), String::from("rule:deploy")],
    };

    assert_eq!(
        error.to_string(),
        "`deploy` names more than one thing (skill:deploy, rule:deploy), so say which: by kind, \
         as in `skill:name`, or by folder"
    );
    assert_eq!(error.code(), "ambiguous_name");
}

#[test]
fn a_stale_edit_says_the_hash_the_file_holds_now_and_the_one_given() {
    let found = crate::mother::ContentHashMother::random();
    let expected = crate::mother::ContentHashMother::digits();
    let error = LifecycleError::Stale {
        path: PathBuf::from("/p/rules/lint.md"),
        expected: expected.clone(),
        found: found.clone(),
    };

    assert_eq!(
        error.to_string(),
        format!(
            "/p/rules/lint.md: changed since it was read, its SHA-256 now {found} and not \
             {expected}, so it is not written over; read it again"
        )
    );
    assert_eq!(error.code(), "edit_stale");
}

#[test]
fn every_lifecycle_error_of_its_own_has_a_code_and_they_are_all_listed() {
    let codes = LifecycleError::CODES;
    let mut unique = codes.to_vec();
    unique.sort_unstable();
    unique.dedup();

    assert_eq!(unique.len(), codes.len(), "{codes:?}");
    assert_eq!(codes.len(), 27, "{codes:?}");
    for code in [
        "not_found",
        "edit_transport_not_mcp",
        "edit_server_unreadable",
        "change_received_mcp",
        "change_outside",
        "edit_stale",
        "edit_leads_outside",
        "rename_mcp_nested",
        "rename_definition_unreadable",
        "edit_definition_unreadable",
        "edit_mcp_name_changed",
    ] {
        assert!(codes.contains(&code), "{code}");
    }
}

#[test]
fn a_lifecycle_error_carrying_another_of_core_s_gives_its_code() {
    let error = LifecycleError::from(WorkspaceError::NotAProject {
        path: PathBuf::from("/p"),
    });

    assert_eq!(error.code(), "not_a_project");
}
