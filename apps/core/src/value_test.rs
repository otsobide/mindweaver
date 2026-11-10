use super::*;

#[test]
fn a_hash_parsed_in_capitals_is_the_one_the_recipe_writes() {
    let written = crate::hash::bytes(b"deploy");
    let given = ContentHash::parse(&written.to_ascii_uppercase()).unwrap();

    assert_eq!(given, written);
    assert_eq!(given.as_str(), written.as_str());
}

#[test]
fn a_text_that_is_no_hash_is_not_parsed_as_one() {
    let digest = crate::hash::bytes(b"deploy");

    assert_eq!(ContentHash::parse(""), None);
    assert_eq!(ContentHash::parse(&digest[1..]), None);
    assert_eq!(ContentHash::parse(&format!("{digest}  SKILL.md")), None);
    assert_eq!(ContentHash::parse(&"g".repeat(ContentHash::LEN)), None);
}

#[test]
fn a_redacted_text_holds_no_credential_whatever_it_was_made_from() {
    let url = Redacted::new("https://user:token@example.com/skills.git");

    assert_eq!(url, "https://example.com/skills.git");
    assert!(!crate::url::carries_credentials(&url));
}

#[test]
fn an_origin_is_written_without_the_credential_it_was_built_with() {
    let origin = Origin::from("https://user:token@example.com/skills.git#main");

    assert_eq!(origin, "https://user:token@example.com/skills.git#main");
    assert_eq!(
        serde_json::to_value(&origin).unwrap(),
        serde_json::json!("https://example.com/skills.git#main")
    );
}

#[test]
fn a_value_reads_as_the_text_it_holds() {
    let name = ArtifactName::from("deploy");

    assert_eq!(format!("{name}"), "deploy");
    assert_eq!(format!("{name:?}"), format!("{:?}", "deploy"));
    assert_eq!(
        serde_json::to_value(&name).unwrap(),
        serde_json::json!("deploy")
    );
    assert!(name.starts_with("dep"));
    assert_eq!(Path::new("skills").join(&name), Path::new("skills/deploy"));
}

#[test]
fn values_sort_as_the_text_they_hold() {
    let mut names: Vec<ArtifactName> = ["b", "a", "B"].map(ArtifactName::from).into();
    names.sort();

    assert_eq!(names, ["B", "a", "b"]);
}

// What only the crate can make: a hash as the recipe computed it and as the
// ledger holds it, neither of which a caller outside can reach, so their
// rules are held here, beside the type, rather than in `tests/value_test.rs`.

#[test]
fn a_hash_as_the_ledger_holds_it_is_kept_exactly_as_it_was_written() {
    use crate::mother::ContentHashMother;

    for _ in 0..100 {
        let digits = ContentHashMother::digits().to_ascii_uppercase();

        let recorded = ContentHash::recorded(digits.clone());

        // Not judged again, so not lowercased: a row in capitals somebody
        // edited by hand reads as a hash no recipe writes, and its copy as
        // edited, rather than as the hash it would parse to.
        assert_eq!(recorded.as_str(), digits);
        assert_ne!(Some(recorded), ContentHash::parse(&digits));
    }
}

#[test]
fn a_row_holding_something_other_than_a_hash_is_still_read_as_one_no_copy_holds() {
    use crate::mother::ContentHashMother;

    for (why, text) in ContentHashMother::not_hashes() {
        let recorded = ContentHash::recorded(text.clone());

        assert_eq!(recorded.as_str(), text, "{why}");
        assert_ne!(
            recorded,
            ContentHashMother::random(),
            "{why}: equal to a hash the recipe wrote"
        );
    }
}

#[test]
fn what_the_recipes_compute_is_what_parse_reads_back() {
    let computed = ContentHash::computed("ab".repeat(ContentHash::LEN / 2));

    assert_eq!(ContentHash::parse(&computed), Some(computed.clone()));
}

#[cfg(debug_assertions)]
#[test]
#[should_panic]
fn a_computed_hash_of_the_wrong_length_is_caught_in_a_debug_build() {
    ContentHash::computed(String::from("abc"));
}

#[test]
fn a_new_id_is_drawn_from_two_random_numbers_written_in_full() {
    for _ in 0..200 {
        let id = ProjectId::generate();

        // Two numbers of sixteen digits each, zero-padded: an id whose number
        // happened to be small is still 32 digits, never a shorter one parse
        // would refuse.
        assert_eq!(id.as_str().len(), ProjectId::LEN, "{id}");
        assert!(ProjectId::parse(id.as_str()).is_some(), "{id}");
    }
}

#[test]
fn redacted_is_the_one_constructor_and_it_redacts_through_url_redacted() {
    use crate::mother::UrlMother;

    for url in UrlMother::every_credentialed() {
        assert_eq!(Redacted::new(&url.typed), redacted(&url.typed).as_ref());
    }
}

#[test]
fn a_source_name_is_one_plain_word_and_none_of_the_others_from_takes() {
    for name in [
        "acme-skills",
        "Acme_Skills.2",
        "a",
        "v1.0",
        "x--y",
        ".hidden",
    ] {
        assert_eq!(
            SourceName::parse(name).map(SourceName::into_string),
            Some(name.to_owned()),
            "{name}"
        );
    }
    for refused in [
        "",
        ".",
        "..",
        "-skills",
        "workspace",
        "acme/skills",
        "load:team",
        "acme skills",
        "skills#main",
        "café",
        "a@b",
    ] {
        assert!(SourceName::parse(refused).is_none(), "{refused}");
    }
}

#[test]
fn a_source_is_named_after_its_owner_and_repository() {
    for (url, name) in [
        ("https://github.com/acme/skills", "acme-skills"),
        ("https://gitlab.com/acme/platform/skills", "platform-skills"),
        ("ssh://example.com:2222/acme/skills.git", "acme-skills"),
        ("example.com:acme/skills.git", "acme-skills"),
        ("https://example.com/Team%20One/skills", "team-20one-skills"),
        ("/srv/git/skills", "git-skills"),
        ("/skills", "skills"),
        ("./skills/repo", "skills-repo"),
        ("/srv/.hidden", "srv-.hidden"),
        ("/srv/-x-", "srv--x"),
        ("C:\\git\\skills", "git-skills"),
        ("https://example.com", "source"),
        ("/", "source"),
        ("/workspace", "workspace-2"),
    ] {
        let derived = SourceName::derived(url);
        assert_eq!(derived, name, "{url}");
        assert!(
            SourceName::parse(&derived).is_some(),
            "{url} gave {derived}"
        );
    }
    assert_eq!(
        SourceName::derived("https://github.com/acme/skills").numbered(2),
        "acme-skills-2"
    );
}
