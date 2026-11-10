//! Unit tests for `finder.rs`, kept beside it rather than inside it: what a
//! finder's result resolves to, how two finders' results are folded into
//! one, and what each refusal says. Asking finders through a client, and
//! which of them are asked, is `tests/finder_test.rs`.

use super::*;

use crate::mother::{random, UrlMother};

/// A result as a finder lists it, at `address`.
fn listed(address: &str) -> Listed {
    Listed {
        name: Some(random::kebab()),
        address: Some(address.to_owned()),
        ..Listed::default()
    }
}

fn hit(repository: &str, name: &str, path: Option<&str>) -> Hit {
    Hit {
        name: name.to_owned(),
        repository: Redacted::new(repository),
        path: path.map(str::to_owned),
        reference: None,
        installs: None,
        stars: None,
        finders: Vec::new(),
    }
}

#[test]
fn every_finder_is_named_as_the_marker_and_a_command_line_name_it() {
    for finder in Finder::ALL {
        assert_eq!(Finder::named(finder.name()), Some(finder));
        assert_eq!(finder.to_string(), finder.name());
        assert_eq!(
            serde_json::to_string(&finder).unwrap(),
            format!("\"{}\"", finder.name())
        );
        assert_eq!(
            serde_json::from_str::<Finder>(&format!("\"{}\"", finder.name())).unwrap(),
            finder
        );
    }
    assert_eq!(Finder::named("GitHub"), None);
    assert_eq!(Finder::named(&random::kebab()), None);
}

/// D-15: GitHub's is documented and needs a token; the other two are
/// documented by nobody, and need none.
#[test]
fn only_github_needs_a_token_and_only_the_others_are_unstable() {
    assert!(Finder::Github.needs_token());
    assert!(!Finder::Github.is_unstable());
    for finder in [Finder::SkillsSh, Finder::ClaudePlugins] {
        assert!(!finder.needs_token(), "{finder}");
        assert!(finder.is_unstable(), "{finder}");
    }
    assert_eq!(DEFAULT_FINDERS, &[Finder::Github]);
}

#[test]
fn a_finder_s_address_is_its_name_an_equals_sign_and_the_url() {
    let url = format!("https://{}/api/v3", UrlMother::host());

    assert_eq!(
        addressed(&format!("github={url}")).unwrap(),
        (Finder::Github, url.clone())
    );
    assert_eq!(
        addressed(&format!("skills.sh={url}=x")).unwrap(),
        (Finder::SkillsSh, format!("{url}=x"))
    );
    for given in [url.clone(), format!("gitlab={url}"), String::from("github")] {
        let refused = addressed(&given).unwrap_err();
        assert_eq!(refused.code(), "finder_url_unnamed", "{given}");
    }
}

#[test]
fn a_named_finder_must_be_enabled_and_some_finder_must_be() {
    let enabled = Enabled {
        finders: vec![Finder::SkillsSh],
        listed: true,
        unknown: Vec::new(),
        config: PathBuf::from("/w/.mindweaver/weaver.toml"),
    };

    assert_eq!(enabled.asked(&[]).unwrap(), vec![Finder::SkillsSh]);
    assert_eq!(
        enabled.asked(&[Finder::SkillsSh]).unwrap(),
        vec![Finder::SkillsSh]
    );
    let refused = enabled.asked(&[Finder::ClaudePlugins]).unwrap_err();
    assert_eq!(refused.code(), "finder_not_enabled");
    assert_eq!(
        refused.to_string(),
        "claude-plugins.dev is not enabled, so it was not asked: list it in `finders` in \
         /w/.mindweaver/weaver.toml to search it, having read its terms: its API is unstable \
         and undocumented"
    );

    let none = Enabled {
        finders: Vec::new(),
        ..enabled
    };
    assert_eq!(none.asked(&[]).unwrap_err().code(), "finder_none_enabled");
}

#[test]
fn an_address_carrying_a_credential_is_dropped_without_repeating_it() {
    for credentialed in UrlMother::every_credentialed() {
        let (_, why) = listed(&credentialed.typed).resolved(false).unwrap_err();

        assert_eq!(why, "its repository's address carries a credential");
        assert!(!why.contains(&credentialed.secret), "{why}");
    }
}

#[test]
fn a_folder_on_this_machine_is_taken_from_a_finder_on_this_machine_alone() {
    for folder in ["/home/someone/skills", "./skills", "file:///srv/skills"] {
        let (_, why) = listed(folder).resolved(false).unwrap_err();
        assert_eq!(
            why,
            "it names a folder on this machine, which only a finder on this machine may"
        );

        let hit = listed(folder).resolved(true).unwrap();
        assert_eq!(hit.repository.as_str(), url::canonical(folder), "{folder}");
    }
}

#[test]
fn a_folder_that_climbs_out_of_its_repository_is_dropped() {
    for path in ["../outside", "skills/../../x", ".."] {
        let mut result = listed("acme/skills");
        result.path = Some(path.to_owned());
        let (_, why) = result.resolved(false).unwrap_err();

        assert_eq!(why, "its folder climbs out of the repository", "{path}");
    }
}

/// `owner/repo` and a forge's address of a folder are read as `gather git`
/// reads them, and listed by the URL every spelling of the repository is.
#[test]
fn an_address_is_read_as_a_gather_reads_it_and_listed_by_its_canonical_url() {
    let hit = listed("https://github.com/Acme/Skills/tree/main/skills/pdf")
        .resolved(false)
        .unwrap();
    assert_eq!(hit.repository.as_str(), "https://github.com/acme/skills");
    assert_eq!(hit.path.as_deref(), Some("skills/pdf"));
    assert_eq!(hit.reference.as_deref(), Some("main"));
    assert_eq!(hit.folder(), "skills/pdf");

    let hit = listed("acme/skills").resolved(false).unwrap();
    assert_eq!(hit.repository.as_str(), "https://github.com/acme/skills");
    assert_eq!(hit.path, None);
    assert_eq!(hit.reference, None);
    assert_eq!(hit.folder(), ".");

    let mut root = listed("git@github.com:acme/skills.git");
    root.path = Some(String::from("./"));
    assert_eq!(root.resolved(false).unwrap().path, None);
}

#[test]
fn a_result_with_no_name_is_named_after_its_folder_or_its_repository() {
    let mut nameless = listed("acme/skills");
    nameless.name = None;
    nameless.path = Some(String::from("skills/pdf"));
    assert_eq!(nameless.resolved(false).unwrap().name, "pdf");

    let mut nameless = listed("acme/skills");
    nameless.name = None;
    assert_eq!(nameless.resolved(false).unwrap().name, "skills");
}

/// What a finder says reaches a terminal: a carriage return in a name
/// would let a row overwrite the one above it.
#[test]
fn a_name_is_made_printable_and_an_address_with_control_characters_dropped() {
    let mut named = listed("acme/skills");
    named.name = Some(String::from("pdf\rgone\u{1b}[2J"));
    assert_eq!(named.resolved(false).unwrap().name, "pdf gone [2J");

    let (_, why) = listed("acme/skills\n").resolved(false).unwrap_err();
    assert_eq!(
        why,
        "its repository has a control character or a space at either end"
    );
    let (_, why) = listed("").resolved(false).unwrap_err();
    assert_eq!(why, "it names no repository");
}

#[test]
fn one_skill_two_finders_name_is_listed_once_with_both() {
    let mut found = Found::default();
    let repository = "https://github.com/acme/skills";
    found.add(Finder::Github, hit(repository, "pdf", Some("skills/pdf")));
    found.add(
        Finder::ClaudePlugins,
        Hit {
            stars: Some(321),
            ..hit(repository, "pdf-tools", Some("skills/pdf"))
        },
    );
    found.add(
        Finder::SkillsSh,
        Hit {
            installs: Some(9),
            ..hit(repository, "pdf", None)
        },
    );

    assert_eq!(found.hits.len(), 1, "{:?}", found.hits);
    let hit = &found.hits[0];
    assert_eq!(hit.name, "pdf");
    assert_eq!(
        hit.finders,
        vec![Finder::Github, Finder::ClaudePlugins, Finder::SkillsSh]
    );
    assert_eq!((hit.installs, hit.stars), (Some(9), Some(321)));
}

/// A finder that does not say where a skill is lists every one of a
/// repository by its name: by its repository alone, each would be folded
/// into the first.
#[test]
fn skills_of_one_repository_with_no_folder_are_told_apart_by_their_names() {
    let mut found = Found::default();
    let repository = "https://github.com/acme/skills";
    found.add(Finder::SkillsSh, hit(repository, "pdf", None));
    found.add(Finder::SkillsSh, hit(repository, "docx", None));
    found.add(Finder::SkillsSh, hit(repository, "pdf", None));

    let names: Vec<&str> = found.hits.iter().map(|hit| hit.name.as_str()).collect();
    assert_eq!(names, ["pdf", "docx"]);
    assert_eq!(found.hits[0].finders, vec![Finder::SkillsSh]);
}

/// Found first with no folder, then with one: the folder is kept, so a take
/// gathers what was found rather than the whole repository.
#[test]
fn a_result_with_no_folder_takes_the_folder_another_finder_names() {
    let mut found = Found::default();
    let repository = "https://github.com/acme/skills";
    found.add(Finder::SkillsSh, hit(repository, "pdf", None));
    found.add(
        Finder::ClaudePlugins,
        Hit {
            reference: Some(String::from("main")),
            ..hit(repository, "pdf", Some("skills/pdf"))
        },
    );

    assert_eq!(found.hits.len(), 1);
    assert_eq!(found.hits[0].path.as_deref(), Some("skills/pdf"));
    assert_eq!(found.hits[0].reference.as_deref(), Some("main"));
}

#[test]
fn a_result_is_taken_by_the_number_it_is_listed_at() {
    let mut found = Found::default();
    found.add(Finder::SkillsSh, hit("https://github.com/a/b", "one", None));
    found.add(Finder::SkillsSh, hit("https://github.com/a/b", "two", None));

    assert_eq!(found.take(1).unwrap().name, "one");
    assert_eq!(found.take(2).unwrap().name, "two");
    for missing in [0, 3] {
        let refused = found.take(missing).unwrap_err();
        assert_eq!(refused.code(), "search_result_missing");
        assert_eq!(
            refused.to_string(),
            format!("there is no result {missing} to take: the search found 2")
        );
    }
}

/// The names a search goes out under in `--json`, one by one: a contract,
/// as every reading command's are.
#[test]
fn what_a_search_found_serializes_under_names_of_its_own() {
    let mut found = Found {
        asked: vec![Finder::SkillsSh],
        skipped: vec![Finder::Github],
        ..Found::default()
    };
    found.add(
        Finder::SkillsSh,
        Hit {
            installs: Some(3),
            ..hit("https://github.com/acme/skills", "pdf", None)
        },
    );
    found.failures.push(Failure::Unreadable {
        finder: Finder::ClaudePlugins,
        detail: Redacted::new("missing field `skills`"),
    });

    assert_eq!(
        serde_json::to_value(&found).unwrap(),
        serde_json::json!({
            "hits": [{
                "name": "pdf",
                "repository": "https://github.com/acme/skills",
                "path": null,
                "reference": null,
                "installs": 3,
                "stars": null,
                "finders": ["skills.sh"],
            }],
            "asked": ["skills.sh"],
            "skipped": ["github"],
            "failures": [{
                "code": "finder_answer_unreadable",
                "message": "claude-plugins.dev: its answer is not in the shape this build \
                            reads, so nothing it found is listed: missing field `skills`",
            }],
        })
    );
}

/// What a front end reads back of a search's JSON is the result it was
/// given, its repository redacted again on the way in.
#[test]
fn a_result_reads_back_as_it_went_out() {
    let hit = Hit {
        reference: Some(String::from("main")),
        stars: Some(7),
        finders: vec![Finder::ClaudePlugins],
        ..hit(
            "https://github.com/beta/decks",
            "slides",
            Some("skills/slides"),
        )
    };

    let read: Hit = serde_json::from_value(serde_json::to_value(&hit).unwrap()).unwrap();
    assert_eq!(read, hit);

    let credentialed = UrlMother::credentialed();
    let mut sent = serde_json::to_value(&hit).unwrap();
    sent["repository"] = serde_json::Value::String(credentialed.typed.clone());
    let read: Hit = serde_json::from_value(sent).unwrap();
    assert!(!read.repository.contains(&credentialed.secret));
}

#[test]
fn a_host_is_read_without_its_login_or_its_port() {
    let host = UrlMother::host();
    assert_eq!(
        host_of(&format!("https://{host}/api/v3")),
        Some(host.clone())
    );
    assert_eq!(
        host_of(&format!("https://{}:8443/", host.to_ascii_uppercase())),
        Some(host.clone())
    );
    assert_eq!(host_of("http://[::1]:9/x"), Some(String::from("[::1]")));
    assert_eq!(host_of("acme/skills"), None);
}

#[test]
fn only_localhost_and_loopback_addresses_are_this_machine() {
    for here in [
        "http://localhost:8080",
        "http://127.0.0.1:1",
        "http://127.8.9.10/x",
        "http://[::1]:9/",
    ] {
        assert!(on_this_machine(here), "{here}");
    }
    for elsewhere in [
        format!("http://{}", UrlMother::host()),
        String::from("http://10.0.0.1"),
        String::from("http://localhost.example.com"),
        String::from("http://127.0.0.1.example.com"),
    ] {
        assert!(!on_this_machine(&elsewhere), "{elsewhere}");
    }
}

#[test]
fn each_search_error_gives_its_own_code_listed_in_order() {
    let config = PathBuf::from("/w/.mindweaver/weaver.toml");
    let every = [
        (SearchError::Empty, "search_query_empty"),
        (
            SearchError::NoneEnabled {
                config: config.clone(),
            },
            "finder_none_enabled",
        ),
        (
            SearchError::NotEnabled {
                finder: Finder::Github,
                config,
            },
            "finder_not_enabled",
        ),
        (
            SearchError::NoneAsked {
                skipped: vec![Finder::Github],
            },
            "finder_none_asked",
        ),
        (
            SearchError::Unaddressed {
                given: Redacted::new("x"),
            },
            "finder_url_unnamed",
        ),
        (
            SearchError::Address {
                finder: Finder::SkillsSh,
                url: Redacted::new("http://example.com"),
                why: String::from("no"),
            },
            "finder_url_invalid",
        ),
        (
            SearchError::NoSuchResult { asked: 2, found: 1 },
            "search_result_missing",
        ),
        (SearchError::Cancelled, "search_cancelled"),
    ];

    for (error, code) in &every {
        assert_eq!(error.code(), *code, "{error:?}");
    }
    let given: Vec<&str> = every.iter().map(|(_, code)| *code).collect();
    assert_eq!(SearchError::CODES, given.as_slice());
}

#[test]
fn nothing_asked_for_want_of_a_token_says_how_to_hand_one_over() {
    let refused = SearchError::NoneAsked {
        skipped: vec![Finder::Github],
    };

    assert_eq!(
        refused.to_string(),
        "nothing was searched: github needs a token, handed over with --token-env or --gh, \
         and no finder that needs none is enabled"
    );
}

#[test]
fn a_finder_that_failed_says_the_code_of_why() {
    let error = FetchError::RateLimited {
        url: Redacted::new("https://api.github.com/search/code"),
        status: 403,
        resets: None,
    };
    let (code, said) = (error.code(), error.to_string());

    let failure = Failure::Failed {
        finder: Finder::Github,
        error,
    };

    assert_eq!(failure.code(), code);
    assert_eq!(failure.to_string(), format!("github: {said}"));
    assert_eq!(failure.finder(), Finder::Github);
    assert_eq!(
        Failure::CODES,
        &["finder_answer_unreadable", "finder_result_dropped"]
    );
}
