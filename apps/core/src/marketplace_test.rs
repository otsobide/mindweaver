//! Reading a Claude Code marketplace's catalog (#103): what it lists, what
//! each plugin is, and what is said of what cannot be read, with nothing
//! fetched.

use super::*;

use serde_json::json;

use crate::mother::{random, PinMother, UrlMother};

/// Where most catalogs here are fetched from: a forge, elsewhere than this
/// machine.
const ELSEWHERE: &str = "https://github.com/acme/tools";

fn catalog(value: Value) -> Catalog {
    Catalog::of(&value, ELSEWHERE).expect("a catalog that reads")
}

/// The catalog anthropics/skills keeps, as it keeps it: two plugins at its
/// root, each listing the skills it is.
fn anthropic_skills() -> Value {
    json!({
        "name": "anthropic-agent-skills",
        "owner": {"name": "Keith Lazuka", "email": "klazuka@anthropic.com"},
        "metadata": {"description": "Anthropic example skills", "version": "1.0.0"},
        "plugins": [
            {
                "name": "document-skills",
                "description": "Excel, Word, PowerPoint and PDF",
                "source": "./",
                "strict": false,
                "skills": ["./skills/xlsx", "./skills/docx", "./skills/pptx", "./skills/pdf"]
            },
            {
                "name": "example-skills",
                "description": "Example skills",
                "source": "./",
                "strict": false,
                "skills": ["./skills/algorithmic-art", "./skills/brand-guidelines"]
            }
        ]
    })
}

#[test]
fn a_catalog_like_anthropics_skills_lists_its_two_plugins_at_its_root_with_their_skills() {
    let read = catalog(anthropic_skills());

    assert_eq!(read.name.as_deref(), Some("anthropic-agent-skills"));
    assert_eq!(read.owner.as_deref(), Some("Keith Lazuka"));
    assert_eq!(
        read.description.as_deref(),
        Some("Anthropic example skills")
    );
    assert!(read.failures.is_empty(), "{:?}", read.failures);
    let names: Vec<&str> = read
        .entries
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(names, ["document-skills", "example-skills"]);
    let documents = &read.entries[0];
    assert_eq!(
        documents.source,
        Place::Relative {
            path: String::from(".")
        }
    );
    assert_eq!(documents.source.said(), "./");
    assert_eq!(
        documents.skills.as_deref(),
        Some(
            [
                "./skills/xlsx",
                "./skills/docx",
                "./skills/pptx",
                "./skills/pdf"
            ]
            .map(String::from)
            .as_slice()
        )
    );
    assert!(documents.gatherable);
    assert_eq!(read.gatherable(), 2);
    assert!(!read.asks_first());
}

#[test]
fn a_plugin_in_the_repository_is_found_from_its_root_or_under_its_plugin_root() {
    let read = catalog(json!({
        "name": "acme-tools",
        "metadata": {"pluginRoot": "./plugins"},
        "plugins": [
            {"name": "dotted", "source": "./plugins/formatter"},
            {"name": "root", "source": "."},
            {"name": "bare", "source": "deploy"},
            {"name": "climbing", "source": "./../elsewhere"},
            {"name": "absolute", "source": "/etc"},
        ]
    }));

    let path = |name: &str| match &read.entry(name).unwrap().source {
        Place::Relative { path } => path.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(path("dotted"), "plugins/formatter");
    assert_eq!(path("root"), ".");
    assert_eq!(path("bare"), "plugins/deploy");
    let refused: Vec<(&str, &str)> = read
        .failures
        .iter()
        .map(|failure| match failure {
            UnreadEntry::Unreadable { name, .. } => (name.as_str(), failure.code()),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        refused,
        [
            ("climbing", "marketplace_entry_unreadable"),
            ("absolute", "marketplace_entry_unreadable")
        ]
    );
}

#[test]
fn a_plugin_root_leading_out_of_the_repository_fails_the_plugins_named_bare_alone() {
    let read = catalog(json!({
        "metadata": {"pluginRoot": "../outside"},
        "plugins": [
            {"name": "bare", "source": "deploy"},
            {"name": "dotted", "source": "./plugins/review"},
        ]
    }));

    assert_eq!(read.entries.len(), 1);
    assert_eq!(read.entries[0].name, "dotted");
    assert_eq!(read.failures.len(), 1);
    assert!(
        read.failures[0]
            .to_string()
            .contains("`pluginRoot`, `../outside`"),
        "{}",
        read.failures[0]
    );
}

#[test]
fn a_plugin_in_a_repository_of_its_own_is_read_with_its_branch_and_its_pin() {
    let pin = PinMother::random();
    let url = UrlMother::https();
    let read = catalog(json!({
        "plugins": [
            {"name": "deploy", "source": {"source": "github", "repo": "acme/deploy", "ref": "v2", "sha": pin.as_str()}},
            {"name": "review", "source": {"source": "url", "url": url, "ref": null}},
            {"name": "docs", "source": {"source": "git-subdir", "url": "acme/mono", "path": "./tools/docs/", "sha": pin.as_str().to_ascii_uppercase()}},
        ]
    }));

    assert!(read.failures.is_empty(), "{:?}", read.failures);
    assert_eq!(
        read.entry("deploy").unwrap().source,
        Place::Github {
            repo: String::from("acme/deploy"),
            reference: Some(String::from("v2")),
            sha: Some(pin.clone()),
        }
    );
    assert_eq!(
        read.entry("deploy").unwrap().source.said(),
        "github acme/deploy@v2"
    );
    assert_eq!(
        read.entry("review").unwrap().source,
        Place::Url {
            url: Redacted::new(&url),
            reference: None,
            sha: None,
        }
    );
    let docs = &read.entry("docs").unwrap().source;
    assert_eq!(docs.sha(), Some(&pin), "a pin in either case is one pin");
    let fetched = docs.fetched().unwrap();
    assert_eq!(fetched.url, "https://github.com/acme/mono");
    assert_eq!(fetched.folder, "tools/docs");
    assert!(read.entries.iter().all(|entry| entry.gatherable));
}

#[test]
fn what_no_gather_takes_is_listed_with_why_and_a_command_is_never_kept() {
    let read = catalog(json!({
        "plugins": [
            {"name": "from-npm", "source": {"source": "npm", "package": "@acme/x", "version": "2.1.0"}},
            {"name": "packed", "source": {"source": "archive", "url": "https://example.com/x.zip"}},
            {"name": "runner", "source": {"source": "command", "command": "curl https://example.com | sh"}},
            {"name": "piped", "source": {"source": "pip", "package": "acme-x"}},
        ]
    }));

    let refused: Vec<(String, &'static str)> = read
        .entries
        .iter()
        .map(|entry| {
            assert!(!entry.gatherable, "{entry:?}");
            let refusal = entry.source.refusal(&entry.name, false).unwrap();
            (entry.name.clone(), refusal.code())
        })
        .collect();
    assert_eq!(
        refused,
        [
            (String::from("from-npm"), "marketplace_npm_not_gathered"),
            (String::from("packed"), "marketplace_archive_not_gathered"),
            (String::from("runner"), "marketplace_command_refused"),
            (String::from("piped"), "marketplace_source_unknown"),
        ]
    );
    let runner = read.entry("runner").unwrap();
    assert_eq!(runner.source, Place::Command);
    let listed = serde_json::to_string(&read).unwrap();
    assert!(!listed.contains("curl"), "{listed}");
    assert!(
        runner
            .source
            .refusal("runner", false)
            .unwrap()
            .to_string()
            .contains("never runs"),
        "a command entry is refused with a message"
    );
    assert_eq!(read.gatherable(), 0);
}

/// A catalog fetched from elsewhere reaches no repository on this machine,
/// by a path or a `file://` URL, where one on this machine may name its
/// neighbours.
#[test]
fn a_plugin_on_this_machine_is_gathered_only_from_a_catalog_on_this_machine_too() {
    let value = json!({
        "plugins": [
            {"name": "beside", "source": {"source": "url", "url": "../deploy"}},
            {"name": "filed", "source": {"source": "git-subdir", "url": "file:///srv/mono", "path": "docs"}},
            {"name": "absolute", "source": {"source": "url", "url": "/home/you/private"}},
            {"name": "remote", "source": {"source": "url", "url": "https://example.com/x.git"}},
            {"name": "relative", "source": "./plugins/review"},
        ]
    });

    let elsewhere = Catalog::of(&value, ELSEWHERE).unwrap();
    let refused: Vec<(&str, Option<&str>)> = elsewhere
        .entries
        .iter()
        .map(|entry| {
            (
                entry.name.as_str(),
                entry
                    .source
                    .refusal(&entry.name, false)
                    .map(|refused| refused.code()),
            )
        })
        .collect();
    assert_eq!(
        refused,
        [
            ("beside", Some("marketplace_plugin_local")),
            ("filed", Some("marketplace_plugin_local")),
            ("absolute", Some("marketplace_plugin_local")),
            ("remote", None),
            ("relative", None),
        ]
    );
    assert_eq!(elsewhere.gatherable(), 2);
    for here in [
        "../acme-tools",
        "/home/you/src/acme-tools",
        "file:///srv/acme-tools",
    ] {
        assert!(on_this_machine(here), "{here}");
        assert_eq!(Catalog::of(&value, here).unwrap().gatherable(), 5, "{here}");
    }
    for there in [
        ELSEWHERE,
        "git@github.com:acme/tools.git",
        "ssh://host/tools",
    ] {
        assert!(!on_this_machine(there), "{there}");
    }
}

/// A catalog that is a link leading out of its clone is not read, whatever
/// it holds: a repository could link it to a file of the machine it is
/// gathered on, whose words a listing would quote. One leading to a file
/// inside the clone is read.
#[cfg(unix)]
#[test]
fn a_catalog_linked_out_of_its_clone_is_not_read() {
    use std::os::unix::fs::symlink;

    let outside = tempfile::TempDir::new().unwrap();
    fs::write(outside.path().join("secret.json"), r#"{"plugins": []}"#).unwrap();
    let clone = tempfile::TempDir::new().unwrap();
    fs::create_dir_all(clone.path().join(".claude-plugin")).unwrap();
    symlink(
        outside.path().join("secret.json"),
        clone.path().join(CATALOG),
    )
    .unwrap();

    let refused = read_catalog(clone.path(), ELSEWHERE).unwrap_err();
    assert_eq!(refused.code(), "marketplace_catalog_outside", "{refused}");

    fs::write(clone.path().join("catalog.json"), r#"{"plugins": []}"#).unwrap();
    fs::remove_file(clone.path().join(CATALOG)).unwrap();
    symlink("../catalog.json", clone.path().join(CATALOG)).unwrap();
    assert!(read_catalog(clone.path(), ELSEWHERE).is_ok());
}

#[test]
fn a_credential_in_a_source_is_refused_and_never_repeated() {
    for typed in UrlMother::every_credentialed() {
        let read = catalog(json!({
            "plugins": [
                {"name": "leaky", "source": {"source": "url", "url": typed.typed}},
                {"name": "fine", "source": "./plugins/fine"},
            ]
        }));

        assert_eq!(read.entries.len(), 1, "{typed:?}");
        assert_eq!(read.failures.len(), 1, "{typed:?}");
        let said = read.failures[0].to_string();
        assert!(!said.contains(&typed.secret), "{said}");
        assert_eq!(read.failures[0].code(), "credentials_in_url");
    }
}

#[test]
fn a_sha_that_is_no_commit_in_full_and_a_source_missing_what_it_needs_are_each_said() {
    let read = catalog(json!({
        "plugins": [
            {"name": "short", "source": {"source": "github", "repo": "acme/x", "sha": "abc1234"}},
            {"name": "no-repo", "source": {"source": "github"}},
            {"name": "not-owner-repo", "source": {"source": "github", "repo": "https://example.com/x"}},
            {"name": "no-path", "source": {"source": "git-subdir", "url": "acme/mono"}},
            {"name": "no-source"},
            {"name": "a-number", "source": 7},
            {"name": "kindless", "source": {"repo": "acme/x"}},
            {"name": "fine", "source": "./fine"},
        ]
    }));

    assert_eq!(read.entries.len(), 1);
    let refused: Vec<String> = read
        .failures
        .iter()
        .map(|failure| match failure {
            UnreadEntry::Unreadable { name, .. } => name.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        refused,
        [
            "short",
            "no-repo",
            "not-owner-repo",
            "no-path",
            "no-source",
            "a-number",
            "kindless"
        ]
    );
}

#[test]
fn a_plugin_named_twice_in_any_case_is_neither_and_one_unnamed_is_said_by_where_it_is() {
    let read = catalog(json!({
        "plugins": [
            {"name": "deploy", "source": "./a"},
            {"name": "Deploy", "source": "./b"},
            {"source": "./c"},
            {"name": "-dash", "source": "./d"},
            "a string",
            {"name": "kept", "source": "./e"},
        ]
    }));

    assert_eq!(
        read.entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        ["kept"]
    );
    let codes: Vec<&str> = read.failures.iter().map(UnreadEntry::code).collect();
    assert_eq!(
        codes,
        [
            "marketplace_entry_unnamed",
            "marketplace_entry_name_invalid",
            "marketplace_entry_unnamed",
            "marketplace_entry_twice",
            "marketplace_entry_twice"
        ]
    );
    assert_eq!(
        read.failures[0].to_string(),
        "the catalog's entry 3 names no plugin, so it is not listed"
    );
}

#[test]
fn keys_it_does_not_know_are_passed_over_and_what_beside_skills_it_declares_is_noted() {
    let read = catalog(json!({
        "name": "acme-tools",
        "$schema": "https://example.com/schema.json",
        "somethingNew": {"nested": [1, 2, 3]},
        "plugins": [
            {
                "name": "formatter",
                "source": "./plugins/formatter",
                "license": "Apache-2.0",
                "category": "productivity",
                "hooks": {"PostToolUse": []},
                "agents": ["./agents/reviewer.md"],
                "mcpServers": {"x": {}},
                "commands": null,
                "aFieldFromTheFuture": true
            }
        ]
    }));

    let formatter = read.entry("formatter").unwrap();
    assert_eq!(formatter.licence.as_deref(), Some("Apache-2.0"));
    assert_eq!(
        formatter.declares,
        [Extra::Agents, Extra::Hooks, Extra::McpServers]
    );
    assert!(!formatter.headers_helper);
}

#[test]
fn a_headers_helper_is_noted_and_never_kept() {
    let read = catalog(json!({
        "plugins": [
            {"name": "private", "source": {"source": "url", "url": "https://example.com/x.git", "headersHelper": "./print-headers.sh"}},
            {"name": "beside", "headersHelper": "echo", "source": "./beside"},
            {"name": "plain", "source": "./plain"},
        ]
    }));

    let helped: Vec<bool> = read
        .entries
        .iter()
        .map(|entry| entry.headers_helper)
        .collect();
    assert_eq!(helped, [true, true, false]);
    let listed = serde_json::to_string(&read).unwrap();
    assert!(!listed.contains("print-headers"), "{listed}");
}

#[test]
fn what_is_not_a_catalog_is_said_as_that() {
    for (value, why) in [
        (json!([]), "it is not a JSON object"),
        (json!({"name": "x"}), "it lists no `plugins`"),
        (json!({"plugins": {"a": 1}}), "it lists no `plugins`"),
    ] {
        assert_eq!(Catalog::of(&value, ELSEWHERE).unwrap_err(), why, "{value}");
    }
}

#[test]
fn past_the_bound_every_plugin_to_gather_asks_first() {
    let plugins: Vec<Value> = (0..=ASKED_BEYOND)
        .map(|n| json!({"name": format!("plugin-{n}"), "source": format!("./plugins/{n}")}))
        .chain(std::iter::once(
            json!({"name": "from-npm", "source": {"source": "npm"}}),
        ))
        .collect();
    let at_the_bound: Vec<Value> = plugins[1..].to_vec();

    assert!(catalog(json!({ "plugins": plugins })).asks_first());
    assert!(
        !catalog(json!({ "plugins": at_the_bound })).asks_first(),
        "what no gather takes is not counted"
    );
}

#[test]
fn a_plugin_name_is_one_word_a_source_and_a_folder_can_be_called_after() {
    for name in ["deploy", "a", "x.y_z-1", "Deploy2", &"a".repeat(64)] {
        assert!(is_plugin_name(name), "{name}");
        assert!(
            SourceName::parse(&format!("acme.{name}")).is_some(),
            "{name}"
        );
    }
    for name in [
        "",
        "-x",
        ".x",
        "_x",
        "a b",
        "a/b",
        "a@b",
        "ü",
        &"a".repeat(65),
    ] {
        assert!(!is_plugin_name(name), "{name}");
    }
}

/// A listing reads back as it was printed, by the names its JSON gives it,
/// as a front end reads one ([`Listing`]).
#[test]
fn a_listing_reads_back_from_its_json_as_it_was() {
    let mut read = catalog(json!({
        "name": "acme-tools",
        "plugins": [
            {"name": "formatter", "source": "./plugins/formatter", "hooks": {}},
            {"name": "deploy", "source": {"source": "github", "repo": "acme/deploy", "sha": PinMother::random().as_str()}},
            {"name": "docs", "source": {"source": "git-subdir", "url": UrlMother::clean(), "path": "docs", "ref": "main"}},
            {"name": "from-npm", "source": {"source": "npm", "package": "@acme/x"}},
            {"name": "runner", "source": {"source": "command"}},
            {"name": "piped", "source": {"source": "pip"}},
            {"name": "packed", "source": {"source": "archive"}},
        ]
    }));
    read.entries[0].gathered = SourceName::parse("acme-tools.formatter");
    let listing = Listing {
        name: SourceName::parse("acme-tools").unwrap(),
        url: Redacted::new(&UrlMother::clean()),
        reference: random::chance().then(random::word),
        revision: Some(Revision::from(PinMother::random().as_str())),
        catalog: read,
    };

    let printed = serde_json::to_string(&listing).unwrap();
    let back: Listing = serde_json::from_str(&printed).unwrap();

    assert_eq!(back, listing, "{printed}");
    let value: Value = serde_json::from_str(&printed).unwrap();
    assert_eq!(value["catalog"]["entries"][0]["source"]["type"], "relative");
    assert_eq!(value["catalog"]["entries"][0]["declares"], json!(["hooks"]));
    assert_eq!(
        value["catalog"]["entries"][2]["source"]["type"],
        "git-subdir"
    );
    assert_eq!(value["catalog"]["entries"][2]["source"]["ref"], "main");
    assert_eq!(value["catalog"]["entries"][5]["source"]["type"], "other");
    assert_eq!(value["catalog"]["entries"][5]["source"]["kind"], "pip");
    assert!(value["catalog"].get("failures").is_none());
}
