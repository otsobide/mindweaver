//! Gathering from a Claude Code plugin marketplace (#103): the reference's
//! "Gathering from a Claude Code marketplace".
//!
//! The marketplace is a folder on this machine, named by its path from the
//! workspace as every source here is, and so are the repositories its
//! catalog names, which a catalog on this machine may name: what the
//! reference shows on GitHub, with no network.

use crate::{skill, Scenario};

const MARKETPLACE: &str = "~/src/acme-tools";
const DEPLOY: &str = "~/src/acme-deploy";
const MONO: &str = "~/src/acme-mono";

/// `~/Projects` linking `collapse`, beside a marketplace keeping two
/// plugins of its own, one with hooks beside its skill and one named bare
/// under its `pluginRoot`, and naming a folder of a monorepo, a repository
/// pinned to a commit it has moved on from, an npm package and a command:
/// the catalog of the reference's examples, with the pin the deploy
/// plugin is held to.
fn acme() -> (Scenario, String) {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        MONO,
        &[(
            "tools/docs/skills/write-docs/SKILL.md",
            &skill("write-docs", "Write the docs"),
        )],
    );
    scenario.source(
        DEPLOY,
        &[("skills/ship/SKILL.md", &skill("ship", "Ship the service"))],
    );
    let pin = scenario.commit(DEPLOY);
    scenario.change(
        DEPLOY,
        &[(
            "skills/ship/SKILL.md",
            &skill("ship", "Ship it, as it is now"),
        )],
    );
    let ran = scenario.at("~/ran");
    let catalog = serde_json::json!({
        "name": "acme-tools",
        "owner": {"name": "Acme"},
        "metadata": {"pluginRoot": "./plugins"},
        "plugins": [
            {"name": "formatter", "source": "./plugins/formatter", "description": "Format the code", "license": "Apache-2.0"},
            {"name": "review", "source": "review", "description": "Read a change"},
            {"name": "docs", "source": {"source": "git-subdir", "url": "../src/acme-mono", "path": "tools/docs"}, "description": "Write the docs"},
            {"name": "deploy", "source": {"source": "url", "url": "../src/acme-deploy", "sha": pin}, "description": "Ship the service"},
            {"name": "from-npm", "source": {"source": "npm", "package": "@acme/x"}, "description": "A package from npm"},
            {"name": "runner", "source": {"source": "command", "command": format!("touch {}", ran.display())}, "description": "Run a script"},
        ],
    });
    scenario.source(
        MARKETPLACE,
        &[
            (".claude-plugin/marketplace.json", &catalog.to_string()),
            (
                "plugins/formatter/skills/format/SKILL.md",
                &skill("format", "Format the code"),
            ),
            ("plugins/formatter/hooks/hooks.json", "{}\n"),
            ("plugins/review/SKILL.md", &skill("review", "Read a change")),
        ],
    );
    (scenario, pin)
}

/// `rows` as a listing lays them out: each column but the last padded to
/// its longest, two spaces apart, and nothing hanging off a line's end.
fn padded(rows: &[Vec<String>], indent: &str) -> String {
    let widths: Vec<usize> = (0..rows[0].len())
        .map(|column| rows.iter().map(|row| row[column].len()).max().unwrap())
        .collect();
    rows.iter()
        .map(|row| {
            let line: Vec<String> = row
                .iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect();
            format!("{indent}{}\n", line.join("  ").trim_end())
        })
        .collect()
}

/// The listing of [`acme`]'s catalog, with `standing` for each plugin, in
/// the order the catalog lists them.
fn listed(scenario: &Scenario, pin: &str, standing: [&str; 6]) -> String {
    let pinned = format!("pinned {}", &pin[..7]);
    let rows: Vec<Vec<String>> = [
        ["formatter", "./plugins/formatter", "", "Format the code"],
        ["review", "./plugins/review", "", "Read a change"],
        [
            "docs",
            "../src/acme-mono, path tools/docs",
            "",
            "Write the docs",
        ],
        ["deploy", "../src/acme-deploy", &pinned, "Ship the service"],
        ["from-npm", "npm @acme/x", "", "A package from npm"],
        ["runner", "command", "", "Run a script"],
    ]
    .iter()
    .zip(standing)
    .map(|(row, standing)| {
        vec![
            row[0].to_owned(),
            row[1].to_owned(),
            row[2].to_owned(),
            standing.to_owned(),
            row[3].to_owned(),
        ]
    })
    .collect();
    let mut text = format!(
        "acme-tools: ../src/acme-tools at {}\n",
        scenario.head(MARKETPLACE)
    );
    text.push_str(&padded(&rows, "  "));
    let gathered = standing
        .iter()
        .filter(|said| said.starts_with("gathered"))
        .count();
    text.push_str(&format!(
        "\n6 plugins listed, {}\n",
        match gathered {
            0 => String::from("none gathered"),
            gathered => format!("{gathered} of them gathered"),
        }
    ));
    text
}

#[test]
fn listing_a_marketplace_records_it_and_lists_its_plugins_gathering_none() {
    let (scenario, pin) = acme();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather marketplace ../src/acme-tools --list",
        )
        .succeeds()
        .prints(&listed(
            &scenario,
            &pin,
            ["", "", "", "", "not gathered", "not gathered"],
        ))
        .quiet();

    assert!(scenario.exists("~/Projects/.mindweaver/cache/..-src-acme-tools"));
    assert!(!scenario.exists("~/Projects/.mindweaver/skills"));
    assert!(!scenario.exists("~/Projects/.mindweaver/cache/..-src-acme-tools.deploy"));
}

#[test]
fn gathering_plugins_by_name_takes_each_as_a_source_of_its_own_at_its_pin() {
    let (scenario, pin) = acme();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather marketplace ../src/acme-tools --plugin formatter --plugin deploy",
        )
        .succeeds()
        .prints(&format!(
            "acme-tools: ../src/acme-tools at {marketplace}, 6 plugins listed\n\
             \n\
             acme-tools.formatter: ../src/acme-tools at {marketplace}\n\
             \x20 added  format  Format the code\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n\
             \n\
             acme-tools.deploy: ../src/acme-deploy at {pinned}, pinned\n\
             \x20 added  ship  Ship the service\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            marketplace = scenario.head(MARKETPLACE),
            pinned = &pin[..7],
        ))
        .says("note: formatter: its hooks are not gathered: only a plugin's skills are\n");

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(&padded(
            &[
                vec![
                    String::from("format"),
                    String::from("acme-tools.formatter"),
                    String::new(),
                    String::from("Apache-2.0"),
                    String::from("Format the code"),
                ],
                vec![
                    String::from("ship"),
                    String::from("acme-tools.deploy"),
                    format!("pinned {}", &pin[..7]),
                    String::from("unknown"),
                    String::from("Ship the service"),
                ],
            ],
            "",
        ))
        .quiet();
    scenario
        .run(
            "~/Projects",
            "mindweaver install -p collapse format --from acme-tools.formatter",
        )
        .succeeds()
        .prints("installed format into collapse, from ../src/acme-tools\n")
        .quiet();
}

#[test]
fn every_plugin_gathered_says_what_no_gather_takes_and_never_runs_a_command() {
    let (scenario, pin) = acme();

    let ran = scenario.run(
        "~/Projects",
        "mindweaver gather marketplace ../src/acme-tools --all",
    );

    ran.succeeds().says(
        "note: formatter: its hooks are not gathered: only a plugin's skills are\n\
         note: from-npm: an npm package, which is not gathered: only a plugin kept in git is\n\
         note: runner: its source is a command to run, which Mindweaver never runs: only a \
         plugin kept in git is gathered\n",
    );
    assert!(!scenario.exists("~/ran"), "the command is never run");
    scenario
        .run(
            "~/Projects",
            "mindweaver gather marketplace ../src/acme-tools --list",
        )
        .succeeds()
        .prints(&listed(
            &scenario,
            &pin,
            [
                "gathered as acme-tools.formatter",
                "gathered as acme-tools.review",
                "gathered as acme-tools.docs",
                "gathered as acme-tools.deploy",
                "not gathered",
                "not gathered",
            ],
        ))
        .quiet();
}

#[test]
fn a_command_entry_named_is_refused_with_a_message_and_never_run() {
    let (scenario, _) = acme();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather marketplace ../src/acme-tools --plugin runner",
        )
        .exits(1)
        .prints(&format!(
            "acme-tools: ../src/acme-tools at {}, 6 plugins listed\n",
            scenario.head(MARKETPLACE)
        ))
        .says(
            "warning: runner: its source is a command to run, which Mindweaver never runs: only \
             a plugin kept in git is gathered\n",
        );
    assert!(!scenario.exists("~/ran"));
}

#[test]
fn a_plugin_the_catalog_does_not_list_is_refused_before_any_is_gathered() {
    let (scenario, _) = acme();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather marketplace ../src/acme-tools --plugin formatter --plugin lint",
        )
        .exits(1)
        .prints("")
        .says(
            "error: `lint` is not listed by the catalog of ../src/acme-tools, so nothing is \
             gathered: `mindweaver gather marketplace --list` lists what it has\n",
        );
    assert!(!scenario.exists("~/Projects/.mindweaver/skills"));
}

/// A catalog of `count` plugins, each a GitHub repository of its own, at
/// `~/src/directory`.
fn directory(scenario: &Scenario, count: usize) {
    let plugins: Vec<serde_json::Value> = (0..count)
        .map(|n| {
            serde_json::json!({
                "name": format!("plugin-{n:02}"),
                "source": {"source": "github", "repo": format!("acme/plugin-{n:02}")},
                "description": format!("Plugin {n}"),
            })
        })
        .collect();
    scenario.source(
        "~/src/directory",
        &[(
            ".claude-plugin/marketplace.json",
            &serde_json::json!({"name": "directory", "plugins": plugins}).to_string(),
        )],
    );
}

#[test]
fn every_plugin_of_a_long_catalog_without_yes_is_said_and_nothing_written() {
    let scenario = Scenario::projects(&["collapse"]);
    directory(&scenario, 26);
    let before = scenario.files("~/Projects/.mindweaver");

    let ran = scenario.run(
        "~/Projects",
        "mindweaver gather marketplace ../src/directory --all",
    );

    ran.succeeds().quiet();
    let mut expected = format!(
        "directory: ../src/directory at {}\n",
        scenario.head("~/src/directory")
    );
    for n in 0..26 {
        expected.push_str(&format!(
            "  plugin-{n:02}  github acme/plugin-{n:02}  Plugin {n}\n"
        ));
    }
    expected.push_str(
        "\n26 plugins listed, none gathered\n\
         26 plugins to gather, more than 25 at once: nothing gathered: add --yes to gather every \
         one, or name the ones wanted with --plugin\n",
    );
    ran.prints(&expected);
    // But for the lock a gather takes, and what SQLite keeps beside a
    // ledger it reads.
    let written = |file: &String| {
        !(file.starts_with("tmp/")
            || file == "lock"
            || file.ends_with("-shm")
            || file.ends_with("-wal"))
    };
    let after: Vec<String> = scenario
        .files("~/Projects/.mindweaver")
        .into_iter()
        .filter(written)
        .collect();
    let before: Vec<String> = before.into_iter().filter(written).collect();
    assert_eq!(after, before);
    assert!(!scenario.exists("~/Projects/.mindweaver/cache"));
}

#[test]
fn gather_sources_shows_a_marketplace_with_its_plugins_under_it() {
    let (scenario, pin) = acme();
    scenario.given(
        "~/Projects",
        "mindweaver gather marketplace ../src/acme-tools --plugin review --plugin deploy",
    );
    let sources = scenario.run("~/Projects", "mindweaver gather sources --json");
    let envelope: serde_json::Value = serde_json::from_str(&sources.stdout).unwrap();
    let when = |name: &str| {
        let at = envelope["data"]
            .as_array()
            .unwrap()
            .iter()
            .find(|source| source["name"] == name)
            .unwrap()["gathered_at"]
            .as_i64()
            .unwrap();
        crate::gathering::utc(at)
    };
    assert_eq!(envelope["data"][0]["kind"], "marketplace");
    assert_eq!(envelope["data"][1]["parent"], "acme-tools");
    assert_eq!(envelope["data"][1]["entry"], "deploy");

    let row = |cells: [&str; 6]| -> Vec<String> { cells.map(str::to_owned).to_vec() };
    let pinned = format!("pinned {}", &pin[..7]);
    let (deploy, formatter) = (
        format!("gathered {}", when("acme-tools.deploy")),
        format!("gathered {}", when("acme-tools.review")),
    );
    scenario
        .run("~/Projects", "mindweaver gather sources")
        .succeeds()
        .prints(&padded(
            &[
                row(["acme-tools", "../src/acme-tools", "", "", "marketplace", ""]),
                row([
                    "  acme-tools.deploy",
                    "../src/acme-deploy",
                    "",
                    &pinned,
                    "1 skill",
                    &deploy,
                ]),
                row([
                    "  acme-tools.review",
                    "../src/acme-tools",
                    "path plugins/review",
                    "",
                    "1 skill",
                    &formatter,
                ]),
            ],
            "",
        ))
        .quiet();
}

#[test]
fn gather_update_of_a_marketplace_gathers_again_each_plugin_of_it_gathered() {
    let (scenario, pin) = acme();
    scenario
        .given(
            "~/Projects",
            "mindweaver gather marketplace ../src/acme-tools --plugin review --plugin deploy",
        )
        .change(
            MARKETPLACE,
            &[(
                "plugins/review/SKILL.md",
                &skill("review", "Read a change, closely"),
            )],
        );

    scenario
        .run("~/Projects", "mindweaver gather update acme-tools")
        .succeeds()
        .prints(&format!(
            "acme-tools: ../src/acme-tools at {marketplace}, 6 plugins listed\n\
             \n\
             acme-tools.deploy: ../src/acme-deploy at {pinned}, pinned\n\
             \x20 unchanged  ship  Ship the service\n\
             \n\
             1 skill: 0 added, 0 updated, 1 unchanged\n\
             \n\
             acme-tools.review: ../src/acme-tools at {marketplace}\n\
             \x20 updated  review  Read a change, closely\n\
             \n\
             1 skill: 0 added, 1 updated, 0 unchanged\n",
            marketplace = scenario.head(MARKETPLACE),
            pinned = &pin[..7],
        ))
        .quiet();
}

#[test]
fn gather_remove_of_a_marketplace_forgets_each_plugin_of_it_too() {
    let (scenario, _) = acme();
    scenario
        .given(
            "~/Projects",
            "mindweaver gather marketplace ../src/acme-tools --plugin review --plugin deploy",
        )
        .given("~/Projects", "mindweaver install -p collapse review");

    scenario
        .run("~/Projects", "mindweaver gather remove acme-tools")
        .succeeds()
        .prints(
            "would forget the source acme-tools, gathered from ../src/acme-tools\n\
             would remove its clone, at /home/you/Projects/.mindweaver/cache/..-src-acme-tools\n\
             would forget its plugin acme-tools.deploy, gathered from ../src/acme-deploy\n\
             would remove its 1 skill from the shelf, at \
             /home/you/Projects/.mindweaver/skills/..-src-acme-tools.deploy\n\
             would remove its clone, at \
             /home/you/Projects/.mindweaver/cache/..-src-acme-tools.deploy\n\
             would forget its plugin acme-tools.review, gathered from ../src/acme-tools\n\
             would remove its 1 skill from the shelf, at \
             /home/you/Projects/.mindweaver/skills/..-src-acme-tools.review\n\
             would leave review installed in collapse, orphaned\n\
             nothing removed: add --yes to go ahead\n",
        )
        .quiet();

    scenario
        .run("~/Projects", "mindweaver gather remove acme-tools --yes")
        .succeeds()
        .says(
            "note: nothing offers what was installed from it any more; `mindweaver uninstall \
             --orphaned --all-projects` takes out what nothing offers\n",
        );
    scenario
        .run("~/Projects", "mindweaver gather sources")
        .succeeds()
        .prints("no sources yet\n  take some with `mindweaver gather git <url>`\n");
    for gone in [
        "~/Projects/.mindweaver/cache/..-src-acme-tools",
        "~/Projects/.mindweaver/cache/..-src-acme-tools.deploy",
        "~/Projects/.mindweaver/skills/..-src-acme-tools.review",
        "~/Projects/.mindweaver/skills/..-src-acme-tools.deploy",
    ] {
        assert!(!scenario.exists(gone), "{gone}");
    }
}

#[test]
fn the_listing_as_json_holds_each_plugin_where_it_is_and_whether_it_is_gathered() {
    let (scenario, pin) = acme();
    scenario.given(
        "~/Projects",
        "mindweaver gather marketplace ../src/acme-tools --plugin review",
    );

    let ran = scenario.run(
        "~/Projects",
        "mindweaver gather marketplace ../src/acme-tools --list --json",
    );

    ran.succeeds().quiet();
    let envelope: serde_json::Value = serde_json::from_str(&ran.stdout).unwrap();
    let data = &envelope["data"];
    assert_eq!(data["name"], "acme-tools");
    assert_eq!(data["url"], "../src/acme-tools");
    assert_eq!(data["catalog"]["name"], "acme-tools");
    let entries = data["catalog"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 6);
    assert_eq!(entries[1]["name"], "review");
    assert_eq!(entries[1]["gathered"], "acme-tools.review");
    assert_eq!(entries[3]["source"]["type"], "url");
    assert_eq!(entries[3]["source"]["sha"], pin);
    assert_eq!(entries[5]["source"], serde_json::json!({"type": "command"}));
    assert_eq!(entries[5]["gatherable"], false);
}
