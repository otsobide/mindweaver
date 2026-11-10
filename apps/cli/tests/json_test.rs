//! The reading commands' `--json`: `list`, `show`, `validate`, `templates`
//! and `gather list`, read back into structs of this suite's own, each held
//! to the entries the text lists, the warnings it says, with their codes,
//! and on stderr as it says them, and the exit status it has.

// Every field of the structs below is named so that one renamed or added is
// refused, which is what they are for: not every test reads every one.
#![allow(dead_code)]

mod common;

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use common::*;

// ---------------------------------------------------------------------------
// What `list --json` prints, field by field: the snapshot's holders. Nothing
// else is allowed, so a field renamed or added fails here first.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Listing {
    library: Option<Library>,
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Library {
    origin: String,
    directories: Vec<Directory>,
    artifacts: Vec<Held>,
    holders: Vec<LibraryHolder>,
    templates: Vec<Template>,
    failures: Vec<Said>,
    servers: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryHolder {
    kind: String,
    name: String,
    entry: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    entry: Option<String>,
    path: String,
    name: Option<String>,
    marker: Option<String>,
    migrates: bool,
    id: Option<String>,
    agents: Vec<String>,
    settings: Option<Settings>,
    stale: Option<Said>,
    health: String,
    directories: Vec<Directory>,
    artifacts: Vec<Held>,
    entries: Vec<Value>,
    tally: Value,
    templates: Vec<Template>,
    unreached: Vec<Value>,
    missed: Vec<Value>,
    misplaced: Vec<Said>,
    failures: Vec<Said>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    agents: Vec<String>,
    share: String,
    edit_tracked: bool,
    targets: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Directory {
    kind: String,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Held {
    kind: String,
    name: String,
    summary: Option<String>,
    state: Option<String>,
    agents: Vec<String>,
    origin: Option<String>,
    targets: Vec<String>,
    issues: Vec<Value>,
    unread: Option<Unread>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Unread {
    path: String,
    code: String,
    message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Template {
    name: String,
    kind: String,
    origin: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Said {
    code: String,
    message: String,
}

/// Every row a listing names, as `holder kind name`, `workspace` the holder
/// of the library's own, in the order it lists them: holder by holder.
fn entries(listing: &Listing) -> Vec<String> {
    let own = listing
        .library
        .iter()
        .flat_map(|library| library.artifacts.iter().map(|held| ("workspace", held)));
    let projects = listing.projects.iter().flat_map(|project| {
        let name = project.name.as_deref().unwrap_or("?");
        project.artifacts.iter().map(move |held| (name, held))
    });
    own.chain(projects)
        .map(|(holder, held)| format!("{holder} {} {}", held.kind, held.name))
        .collect()
}

/// The first `columns` words of every row a table prints, sorted.
fn rows(stdout: &str, columns: usize) -> Vec<String> {
    let mut rows: Vec<String> = stdout
        .lines()
        .map(|line| {
            line.split_whitespace()
                .take(columns)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    rows.sort();
    rows
}

/// [`entries`], sorted as [`rows`] is, to hold one against the other.
fn sorted(mut entries: Vec<String>) -> Vec<String> {
    entries.sort();
    entries
}

/// Each warning line an outcome said on stderr, without the `warning: `
/// the envelope leaves out.
fn warned(outcome: &Outcome) -> Vec<&str> {
    outcome
        .stderr
        .iter()
        .filter_map(|line| line.text.strip_prefix("warning: "))
        .collect()
}

/// A workspace whose library holds the skill `tidy`, linking `alpha`, which
/// authors the skill `alpha` and the rule `git/commits` and received `tidy`
/// from the library, and `beta`, which authors the skill `beta`.
fn workspace_with_rows() -> TempDir {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    write_rule(&alpha, "git/commits.md", "# Commits\n\nSay why.\n");
    weaver(dir.path(), &["add", "skill", "tidy", "Tidy it"]).unwrap();
    weaver(dir.path(), &["install", "-p", "alpha", "tidy"]).unwrap();
    dir
}

#[test]
fn weaver_list_as_json_holds_the_entries_the_text_lists() {
    let dir = workspace_with_rows();

    let text = weaver(dir.path(), &["list"]).unwrap();
    let json = weaver(dir.path(), &["list", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert_eq!(
        entries(&listing),
        [
            "workspace skill tidy",
            "alpha skill alpha",
            "alpha skill tidy",
            "alpha rule git/commits",
            "beta skill beta",
        ]
    );
    assert_eq!(sorted(entries(&listing)), rows(&text.stdout, 3));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert!(json.stderr.is_empty(), "{:?}", json.stderr);
    assert!(json.ok && text.ok);
    // As the snapshot holds them: how each stands, from where, read by whom.
    let alpha = &listing.projects[0];
    assert_eq!(alpha.entry.as_deref(), Some("alpha"));
    let tidy = alpha
        .artifacts
        .iter()
        .find(|held| held.name == "tidy")
        .unwrap();
    assert_eq!(tidy.state.as_deref(), Some("current"));
    assert_eq!(tidy.origin.as_deref(), Some("workspace"));
    assert_eq!(tidy.summary.as_deref(), Some("Tidy it"));
    let library = listing.library.as_ref().unwrap();
    assert_eq!(library.artifacts[0].state, None);
    // What a copy installed from it names, and what `--from` takes for it.
    assert_eq!(tidy.origin.as_deref(), Some(library.origin.as_str()));
    // The projects holding a copy of one of its own, as core tells them:
    // alpha's `tidy` was installed from it, and nobody's own is taken for
    // one.
    let holders: Vec<(&str, &str, &str)> = library
        .holders
        .iter()
        .map(|holder| {
            (
                holder.kind.as_str(),
                holder.name.as_str(),
                holder.entry.as_str(),
            )
        })
        .collect();
    assert_eq!(holders, [("skill", "tidy", "alpha")]);
}

#[test]
fn a_kind_narrows_what_list_as_json_holds_as_it_narrows_the_text() {
    let dir = workspace_with_rows();

    let text = weaver(dir.path(), &["list", "rules"]).unwrap();
    let json = weaver(dir.path(), &["list", "rules", "--json"]).unwrap();

    let (listing, _): (Listing, _) = enveloped(&json);
    assert_eq!(entries(&listing), ["alpha rule git/commits"]);
    assert_eq!(text.stdout, "git/commits  Commits\n");
    for project in &listing.projects {
        assert!(project
            .templates
            .iter()
            .all(|template| template.kind == "rule"));
        assert!(project
            .directories
            .iter()
            .all(|folder| folder.kind == "rule"));
    }
}

#[test]
fn mind_list_as_json_is_weaver_list_of_its_project() {
    let dir = workspace_with_rows();
    let alpha = dir.path().join("alpha");

    let here = mind(&alpha, &["list", "--json"]).unwrap();
    let named = weaver(dir.path(), &["list", "-p", "alpha", "--json"]).unwrap();
    let text = mind(&alpha, &["list"]).unwrap();

    assert_eq!(here, named);
    let (listing, _): (Listing, _) = enveloped(&here);
    assert!(listing.library.is_none());
    assert_eq!(listing.projects.len(), 1);
    assert_eq!(sorted(entries(&listing)), {
        let mut rows: Vec<String> = rows(&text.stdout, 2)
            .into_iter()
            .map(|row| format!("alpha {row}"))
            .collect();
        rows.sort();
        rows
    });
}

/// A file that does not read is a warning in the envelope, with the code
/// stderr gives it, and on stderr as the text says it, and fails the JSON
/// as it fails the text; the snapshot lists it all the same, by its place,
/// saying why.
#[test]
fn an_artifact_that_does_not_read_is_a_warning_and_fails_list_as_json_too() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    write_skill(&alpha, "broken", "no front matter\n");

    for (text, json) in [
        (
            weaver(dir.path(), &["list"]).unwrap(),
            weaver(dir.path(), &["list", "--json"]).unwrap(),
        ),
        (
            mind(&alpha, &["list"]).unwrap(),
            mind(&alpha, &["list", "--json"]).unwrap(),
        ),
    ] {
        let (listing, warnings): (Listing, _) = enveloped(&json);
        assert_eq!(warnings, warned(&text));
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("broken"), "{warnings:?}");
        assert_eq!(warning_codes(&json), stderr_codes(&text));
        assert_eq!(
            warning_codes(&json),
            [Some(String::from("front_matter_missing"))]
        );
        assert_eq!(json.stderr, text.stderr);
        assert!(!text.ok);
        assert_eq!(json.ok, text.ok);
        let broken = listing
            .projects
            .iter()
            .flat_map(|project| &project.artifacts)
            .find(|held| held.name == "broken")
            .expect("listed by its place");
        let unread = broken.unread.as_ref().unwrap();
        assert_eq!(unread.code, "front_matter_missing");
        assert_eq!(unread.path, "skills/broken");
    }
}

/// An entry that opens no project is a warning, as the text says it, and a
/// project of its own in the data, saying why.
#[test]
fn a_stale_entry_is_a_warning_and_a_project_saying_why_in_list_as_json() {
    let dir = workspace_with_two();
    fs::remove_dir_all(dir.path().join("beta")).unwrap();

    let text = weaver(dir.path(), &["list"]).unwrap();
    let json = weaver(dir.path(), &["list", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert_eq!(warnings, warned(&text));
    assert_eq!(warning_codes(&json), stderr_codes(&text));
    assert_eq!(json.stderr, text.stderr);
    assert_eq!(listing.projects.len(), 2);
    let stale = listing.projects[1].stale.as_ref().expect("stale");
    assert_eq!(listing.projects[1].entry.as_deref(), Some("beta"));
    assert_eq!(stale.code, "not_a_project");
    assert!(!json.ok && !text.ok);
}

/// A hint is advice for whoever reads the terminal, so it stays on stderr
/// under `--json`, outside the envelope, and fails nothing.
#[test]
fn the_hint_about_an_unmade_default_workspace_stays_on_stderr() {
    let home = TempDir::new().unwrap();
    let elsewhere = TempDir::new().unwrap();

    let json = weaver_at_home(elsewhere.path(), home.path(), &["list", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert!(listing.projects.is_empty());
    assert!(warnings.is_empty());
    assert_eq!(json.stderr.len(), 1, "{:?}", json.stderr);
    assert!(json.stderr[0].text.starts_with("hint: "));
    assert!(json.ok);
    assert!(!home.path().join(".mindweaver").exists());
}

/// `mind list --json` reads how each stands against the workspace `mind`
/// consults, which the text never reads, so one that cannot be read fails
/// neither: the project is read alone, saying why among its `failures`,
/// with the text's warnings, none, and its exit status.
#[test]
fn a_workspace_above_that_cannot_be_read_leaves_mind_list_as_json_as_the_text() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    let marker = dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG);
    let mut written = fs::read_to_string(&marker).unwrap();
    written.push_str("garbage = [\n");
    fs::write(&marker, written).unwrap();

    let text = mind(&alpha, &["list"]).unwrap();
    let json = mind(&alpha, &["list", "--json"]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert_eq!(entries(&listing), ["alpha skill alpha"]);
    assert_eq!(listing.projects[0].entry, None);
    let failures = &listing.projects[0].failures;
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(failures[0].message.contains(&marker.display().to_string()));
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(json.stderr, text.stderr);
    assert!(text.ok && json.ok);
}

/// One `-W` names that is no workspace, which `mind list` never reads, is
/// no refusal of its JSON either: why is among the project's `failures`.
#[test]
fn a_workspace_named_that_is_none_leaves_mind_list_as_json_as_the_text() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    write_skill(dir.path(), "deploy", &skill_file("deploy", "Ship it"));
    let nowhere = TempDir::new().unwrap();

    let named = nowhere.path().to_string_lossy();
    let text = mind(dir.path(), &["list", "-W", &named]).unwrap();
    let json = mind(dir.path(), &["list", "--json", "-W", &named]).unwrap();

    let (listing, warnings): (Listing, _) = enveloped(&json);
    assert_eq!(rows(&text.stdout, 1), ["deploy"]);
    assert_eq!(listing.projects[0].artifacts[0].name, "deploy");
    let failures = &listing.projects[0].failures;
    assert_eq!(failures.len(), 1, "{failures:?}");
    assert!(
        failures[0].message.contains(&*named),
        "{}",
        failures[0].message
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(json.stderr, text.stderr);
    assert!(text.ok && json.ok);
}

// ---------------------------------------------------------------------------
// What `validate --json` prints, field by field.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Validation {
    artifacts: Vec<Checked>,
    invalid: usize,
    unreadable: usize,
    differing: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Checked {
    kind: String,
    name: String,
    path: String,
    project: String,
    issues: Vec<Value>,
    invalid: bool,
    copies_differ: bool,
}

/// The name of each artifact a `validate` table judges, in its order: every
/// line naming one, without the kind or the holder a qualified label adds.
fn judged(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .take_while(|line| !line.is_empty())
        .filter(|line| !line.starts_with(' '))
        .map(|line| {
            let (label, _) = line.rsplit_once(": ").unwrap();
            let label = label.split(" (").next().unwrap();
            let name = label
                .split_once(':')
                .map_or(label, |(kind, name)| match kind {
                    "skill" | "rule" => name,
                    _ => label,
                });
            name.to_owned()
        })
        .collect()
}

/// The name of each artifact a `validate --json` checked, in its order.
fn checked(validation: &Validation) -> Vec<String> {
    validation
        .artifacts
        .iter()
        .map(|checked| checked.name.clone())
        .collect()
}

#[test]
fn validate_as_json_checks_the_entries_the_text_checks() {
    let dir = workspace_with_rows();
    write_skill(
        &dir.path().join("beta"),
        "deploy",
        &skill_file("Deployment", "Ship it"),
    );

    let text = weaver(dir.path(), &["validate"]).unwrap();
    let json = weaver(dir.path(), &["validate", "--json"]).unwrap();

    let (validation, warnings): (Validation, _) = enveloped(&json);
    assert_eq!(checked(&validation), judged(&text.stdout));
    assert_eq!(
        checked(&validation),
        ["Deployment", "alpha", "beta", "tidy", "tidy", "git/commits"]
    );
    let kinds: Vec<&str> = validation
        .artifacts
        .iter()
        .map(|checked| checked.kind.as_str())
        .collect();
    assert_eq!(kinds, ["skill", "skill", "skill", "skill", "skill", "rule"]);
    assert!(text
        .stdout
        .ends_with("5 skills and 1 rule checked, 1 invalid\n"));
    assert_eq!(validation.invalid, 1);
    assert_eq!((validation.unreadable, validation.differing), (0, 0));
    let deployment = &validation.artifacts[0];
    assert!(deployment.invalid);
    assert!(deployment.path.ends_with("deploy"), "{}", deployment.path);
    assert_eq!(
        deployment.issues[0]["code"],
        Value::from("name_not_kebab_case")
    );
    // An artifact that does not pass fails the JSON as it fails the text,
    // though nothing is said on stderr.
    assert!(warnings.is_empty());
    assert!(text.stderr.is_empty() && json.stderr.is_empty());
    assert!(!text.ok);
    assert_eq!(json.ok, text.ok);
}

#[test]
fn mind_validate_as_json_is_weaver_validate_of_its_project() {
    let dir = workspace_with_rows();

    let here = mind(&dir.path().join("alpha"), &["validate", "rules", "--json"]).unwrap();
    let named = weaver(dir.path(), &["validate", "rules", "-p", "alpha", "--json"]).unwrap();

    assert_eq!(here, named);
    let (validation, _): (Validation, _) = enveloped(&here);
    assert_eq!(checked(&validation), ["git/commits"]);
    assert!(here.ok);
}

/// A file that does not read is a warning, as stderr says it, counted as
/// unreadable, and fails the JSON as it fails the text.
#[test]
fn an_artifact_that_does_not_read_is_a_warning_and_fails_validate_as_json_too() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    write_skill(&alpha, "broken", "no front matter\n");

    let text = mind(&alpha, &["validate"]).unwrap();
    let json = mind(&alpha, &["validate", "--json"]).unwrap();

    let (validation, warnings): (Validation, _) = enveloped(&json);
    assert_eq!(checked(&validation), judged(&text.stdout));
    assert_eq!(warnings, warned(&text));
    assert_eq!(
        warning_codes(&json),
        [Some(String::from("front_matter_missing"))]
    );
    assert_eq!(warning_codes(&json), stderr_codes(&text));
    assert_eq!(json.stderr, text.stderr);
    assert_eq!(validation.unreadable, 1);
    assert!(
        text.stdout.ends_with(", 1 file unreadable\n"),
        "{}",
        text.stdout
    );
    assert!(!text.ok);
    assert_eq!(json.ok, text.ok);
}

#[test]
fn validate_as_json_names_what_it_could_not_find_as_the_text_does() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());

    let failure = mind(dir.path(), &["validate", "absent", "--json"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::UnknownArtifact(ref name) if name == "absent"));
}

// ---------------------------------------------------------------------------
// What `show --json` prints, field by field.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Shown {
    kind: String,
    name: String,
    path: String,
    project: String,
    file: String,
    summary: Option<String>,
    description: Option<String>,
    allowed_tools: Option<Vec<String>>,
    license: Option<String>,
    compatibility: Option<String>,
    metadata: Option<serde_json::Map<String, Value>>,
    paths: Option<Vec<String>>,
    transport: Option<Transport>,
    inputs: Option<Vec<Input>>,
    expressibility: Option<Vec<Expressibility>>,
    body: String,
}

/// How an MCP server is started or reached, as `show --json` prints it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Transport {
    #[serde(rename = "type")]
    transport: String,
    command: Option<String>,
    args: Vec<String>,
    env: BTreeMap<String, String>,
    cwd: Option<String>,
    url: Option<String>,
    headers: BTreeMap<String, String>,
}

/// One input of an MCP server, as `show --json` prints it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    name: String,
    kind: String,
    description: Option<String>,
    required: bool,
    secret: bool,
    env_var: Option<String>,
}

/// What one agent cannot be given of an MCP server, as `show --json`
/// prints it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expressibility {
    agent: String,
    refused: Vec<Said>,
    dropped: Vec<Said>,
}

#[test]
fn show_as_json_holds_what_the_text_shows() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    write_skill(
        dir.path(),
        "deploy",
        "---\nname: deploy\ndescription: Ship it\nallowed-tools: Bash, Read\nlicense: MIT\n\
         compatibility: Requires git\nmetadata:\n  author: acme\n---\n\nRun the pipeline.\n",
    );

    let text = mind(dir.path(), &["show", "deploy"]).unwrap();
    let json = mind(dir.path(), &["show", "deploy", "--json"]).unwrap();

    let (shown, warnings): (Vec<Shown>, _) = enveloped(&json);
    assert_eq!(shown.len(), 1);
    let deploy = &shown[0];
    let file = PathBuf::from(&deploy.file);
    assert_eq!(
        text.stdout,
        format!(
            "{}\n{}\n\n{}\n\nallowed-tools: {}\nlicense: {}\ncompatibility: {}\nmetadata:\n  author: {}\n\n{}\n",
            deploy.name,
            file.display(),
            deploy.description.as_deref().unwrap(),
            deploy.allowed_tools.as_ref().unwrap().join(", "),
            deploy.license.as_deref().unwrap(),
            deploy.compatibility.as_deref().unwrap(),
            deploy.metadata.as_ref().unwrap()["author"].as_str().unwrap(),
            deploy.body.trim(),
        )
    );
    assert_eq!(deploy.kind, "skill");
    assert_eq!(
        file,
        dir.path().join("skills").join("deploy").join("SKILL.md")
    );
    assert_eq!(deploy.paths, None);
    assert!(deploy.transport.is_none() && deploy.inputs.is_none());
    assert!(deploy.expressibility.is_none());
    assert!(warnings.is_empty());
    assert!(json.ok && text.ok);
}

#[test]
fn show_as_json_says_what_each_agent_cannot_be_given_of_a_server_as_the_text_does() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("events.toml"),
        mindweaver_core::mother::McpMother::sse("events"),
    )
    .unwrap();

    let text = weaver(dir.path(), &["show", "mcp:events"]).unwrap();
    let json = weaver(dir.path(), &["show", "mcp:events", "--json"]).unwrap();

    let (shown, warnings): (Vec<Shown>, _) = enveloped(&json);
    assert!(warnings.is_empty(), "{warnings:?}");
    let report = shown[0].expressibility.as_ref().expect("a server's report");
    let agents: Vec<&str> = report.iter().map(|one| one.agent.as_str()).collect();
    assert_eq!(agents, ["claude", "codex", "opencode"]);
    for one in report {
        assert!(one.dropped.is_empty(), "{one:?}");
        let codes: Vec<&str> = one.refused.iter().map(|said| said.code.as_str()).collect();
        let expected: &[&str] = if one.agent == "claude" {
            &[]
        } else {
            &["sse_unsupported"]
        };
        assert_eq!(codes, expected, "{}", one.agent);
        for said in &one.refused {
            assert!(
                text.stdout.contains(&format!("    - {}\n", said.message)),
                "{}",
                text.stdout
            );
        }
    }
    assert!(json.ok && text.ok);
}

#[test]
fn show_as_json_lays_out_a_servers_transport_and_inputs_as_the_text_does() {
    let dir = TempDir::new().unwrap();
    weaver(dir.path(), &["init"]).unwrap();
    let folder = own_directory_for(dir.path(), Kind::Mcp);
    fs::create_dir_all(&folder).unwrap();
    fs::write(
        folder.join("linear.toml"),
        mindweaver_core::mother::McpMother::http("linear"),
    )
    .unwrap();

    let text = weaver(dir.path(), &["show", "mcp:linear"]).unwrap();
    let json = weaver(dir.path(), &["show", "mcp:linear", "--json"]).unwrap();

    let (shown, warnings): (Vec<Shown>, _) = enveloped(&json);
    assert!(warnings.is_empty(), "{warnings:?}");
    let transport = shown[0].transport.as_ref().expect("a server's transport");
    assert_eq!(transport.transport, "http");
    assert_eq!(
        (&transport.command, &transport.cwd),
        (&None, &None),
        "{transport:?}"
    );
    assert!(transport.args.is_empty() && transport.env.is_empty());
    let url = transport.url.as_deref().expect("an address");
    assert!(
        text.stdout
            .contains(&format!("transport: http\n  url: {url}\n  headers:\n")),
        "{}",
        text.stdout
    );
    for (header, value) in &transport.headers {
        assert!(
            text.stdout.contains(&format!("    {header}: {value}\n")),
            "{}",
            text.stdout
        );
    }
    let inputs = shown[0].inputs.as_ref().expect("a server's inputs");
    let said: Vec<(&str, &str, bool)> = inputs
        .iter()
        .map(|input| (input.name.as_str(), input.kind.as_str(), input.secret))
        .collect();
    assert_eq!(
        said,
        [("tenant", "url_var", false), ("token", "header", true)]
    );
    for input in inputs {
        let variable = input.env_var.as_deref().expect("a variable");
        assert!(
            text.stdout
                .contains(&format!("  {}: {}", input.name, input.kind))
                && text.stdout.contains(&format!("read from ${variable}\n")),
            "{}",
            text.stdout
        );
        assert!(input.required && input.description.is_none(), "{input:?}");
    }
    assert!(json.ok && text.ok);
}

#[test]
fn show_as_json_holds_every_artifact_of_the_name_the_text_shows() {
    let dir = workspace_with_two();
    write_skill(
        &dir.path().join("beta"),
        "alpha",
        &skill_file("alpha", "The other one"),
    );
    write_rule(&dir.path().join("beta"), "alpha.md", "# A rule\n");

    let text = weaver(dir.path(), &["show", "alpha"]).unwrap();
    let json = weaver(dir.path(), &["show", "alpha", "--json"]).unwrap();

    let (shown, _): (Vec<Shown>, _) = enveloped(&json);
    let said: Vec<(&str, &str, &str)> = shown
        .iter()
        .map(|shown| {
            let holder = Path::new(&shown.project).file_name().unwrap();
            (
                shown.kind.as_str(),
                shown.name.as_str(),
                holder.to_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        said,
        [
            ("skill", "alpha", "alpha"),
            ("skill", "alpha", "beta"),
            ("rule", "alpha", "beta"),
        ]
    );
    assert_eq!(text.stdout.matches("\n---\n").count(), shown.len() - 1);
    assert_eq!(shown[2].summary.as_deref(), Some("A rule"));
    assert_eq!(shown[2].metadata, None);
}

#[test]
fn mind_show_as_json_is_weaver_show_of_its_project() {
    let dir = workspace_with_rows();

    let here = mind(
        &dir.path().join("alpha"),
        &["show", "git/commits", "--json"],
    )
    .unwrap();
    let named = weaver(
        dir.path(),
        &["show", "git/commits", "-p", "alpha", "--json"],
    )
    .unwrap();

    assert_eq!(here, named);
    let (shown, _): (Vec<Shown>, _) = enveloped(&here);
    assert_eq!(shown[0].body, "# Commits\n\nSay why.\n");
}

/// A file that does not read beside the one shown is a warning, as the
/// text says it, and fails the JSON as it fails the text.
#[test]
fn an_artifact_that_does_not_read_is_a_warning_beside_show_as_json() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    write_skill(&alpha, "broken", "no front matter\n");

    let text = mind(&alpha, &["show", "alpha"]).unwrap();
    let json = mind(&alpha, &["show", "alpha", "--json"]).unwrap();

    let (shown, warnings): (Vec<Shown>, _) = enveloped(&json);
    assert_eq!(shown.len(), 1);
    assert_eq!(warnings, warned(&text));
    assert_eq!(warning_codes(&json), stderr_codes(&text));
    assert_eq!(json.stderr, text.stderr);
    assert_eq!(warnings.len(), 1);
    assert!(!text.ok);
    assert_eq!(json.ok, text.ok);
}

#[test]
fn show_as_json_names_what_it_could_not_find_as_the_text_does() {
    let dir = workspace_with_two();

    let failure = weaver(dir.path(), &["show", "absent", "--json"]).unwrap_err();

    assert!(matches!(*failure.error, CliError::UnknownArtifact(ref name) if name == "absent"));
}

// ---------------------------------------------------------------------------
// What `templates --json` and `gather list --json` print, field by field.
// ---------------------------------------------------------------------------

#[test]
fn templates_as_json_holds_the_templates_the_text_lists() {
    let dir = TempDir::new().unwrap();
    new_project(dir.path());
    for (route, contents) in [
        (".mind/templates/runbook/SKILL.md", "---\n---\n"),
        (".mind/templates/policy.md", "## Why\n"),
    ] {
        let path = dir.path().join(route);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    let text = mind(dir.path(), &["templates"]).unwrap();
    let json = mind(dir.path(), &["templates", "--json"]).unwrap();

    let (templates, warnings): (Vec<Template>, _) = enveloped(&json);
    let listed: Vec<String> = templates
        .iter()
        .map(|template| format!("{} {}", template.name, template.kind))
        .collect();
    assert_eq!(sorted(listed.clone()), rows(&text.stdout, 2));
    assert_eq!(listed, ["full skill", "runbook skill", "policy rule"]);
    // Built in is in no folder; the rest are where they are, whole.
    assert_eq!(templates[0].origin, None);
    assert_eq!(
        templates[2].origin.as_deref().map(PathBuf::from),
        Some(dir.path().join(".mind/templates/policy.md"))
    );
    assert!(warnings.is_empty());
    assert!(json.ok && text.ok);
}

#[test]
fn weaver_templates_as_json_is_mind_templates_of_its_project() {
    let dir = workspace_with_two();

    let here = mind(&dir.path().join("alpha"), &["templates", "--json"]).unwrap();
    let named = weaver(dir.path(), &["templates", "-p", "alpha", "--json"]).unwrap();

    assert_eq!(here, named);
}

/// A workspace `mind templates` consults that cannot be read is a warning
/// beside the templates, said in the envelope and on stderr as the text
/// says it there, and fails the JSON as it fails the text.
#[test]
fn a_workspace_above_that_cannot_be_read_is_a_warning_beside_templates_as_json() {
    let dir = workspace_with_two();
    let alpha = dir.path().join("alpha");
    let marker = dir.path().join(WEAVER_DIR).join(WEAVER_CONFIG);
    let mut written = fs::read_to_string(&marker).unwrap();
    written.push_str("garbage = [\n");
    fs::write(&marker, written).unwrap();

    let text = mind(&alpha, &["templates"]).unwrap();
    let json = mind(&alpha, &["templates", "--json"]).unwrap();

    let (_, warnings): (Vec<Template>, _) = enveloped(&json);
    assert_eq!(warnings, warned(&text));
    assert_eq!(warnings.len(), 1);
    assert_eq!(warning_codes(&json), stderr_codes(&text));
    assert_eq!(json.stderr, text.stderr);
    assert!(!text.ok);
    assert_eq!(json.ok, text.ok);
}

/// An offer, as `gather list --json` prints one: the place that offers it,
/// by `place`, and what that place holds of it, every field of each named,
/// as in every report this suite reads. `gather list` offers from a loaded
/// workspace or the shelf, never the workspace's own, which `list` holds.
#[derive(Debug, Deserialize)]
#[serde(tag = "place", rename_all = "snake_case", deny_unknown_fields)]
enum Offer {
    Loaded {
        kind: String,
        name: String,
        summary: Option<String>,
        path: String,
        entry: String,
        workspace: String,
        origin: String,
    },
    Shelf {
        kind: String,
        name: String,
        path: String,
        summary: Option<String>,
        revision: Option<String>,
        gathered_at: i64,
        source: Source,
        origin: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    kind: String,
    url: String,
    reference: Option<String>,
    subdirectory: String,
    revision: Option<String>,
    directory: String,
    name: String,
}

impl Offer {
    /// The `place` it was tagged with.
    fn place(&self) -> &str {
        match self {
            Offer::Loaded { .. } => "loaded",
            Offer::Shelf { .. } => "shelf",
        }
    }

    /// What every place says of what it offers: its kind, name, summary
    /// and origin.
    fn said(&self) -> (&str, &str, Option<&str>, &str) {
        match self {
            Offer::Loaded {
                kind,
                name,
                summary,
                origin,
                ..
            }
            | Offer::Shelf {
                kind,
                name,
                summary,
                origin,
                ..
            } => (kind, name, summary.as_deref(), origin),
        }
    }

    fn kind(&self) -> &str {
        self.said().0
    }

    fn name(&self) -> &str {
        self.said().1
    }

    fn summary(&self) -> Option<&str> {
        self.said().2
    }

    fn origin(&self) -> &str {
        self.said().3
    }

    /// What the text names where it comes from by: a source's name, and
    /// the origin of anything else.
    fn listed_from(&self) -> &str {
        match self {
            Offer::Shelf { source, .. } => &source.name,
            Offer::Loaded { origin, .. } => origin,
        }
    }
}

/// A workspace that loads `team`, beside it, which offers its own skill
/// `notes`, and whose shelf holds `deploy`, gathered from a repository.
fn workspace_offering() -> (TempDir, TempDir) {
    let source = repository(&[("skills/deploy/SKILL.md", &skill_file("deploy", "Ship it"))]);
    let dir = TempDir::new().unwrap();
    let hub = dir.path().join("hub");
    let team = dir.path().join("team");
    weaver(&hub, &["init"]).unwrap();
    weaver(&team, &["init"]).unwrap();
    weaver(&team, &["add", "skill", "notes", "Take notes"]).unwrap();
    weaver(&hub, &["load", "../team"]).unwrap();
    let url = source.path().to_string_lossy().into_owned();
    weaver(&hub, &["gather", "git", &url]).unwrap();
    (dir, source)
}

#[test]
fn gather_list_as_json_holds_the_offers_the_text_lists() {
    let (dir, source) = workspace_offering();
    let hub = dir.path().join("hub");

    let text = weaver(&hub, &["gather", "list"]).unwrap();
    let json = weaver(&hub, &["gather", "list", "--json"]).unwrap();

    let (offers, warnings): (Vec<Offer>, _) = enveloped(&json);
    let listed: Vec<String> = offers
        .iter()
        .map(|offer| format!("{} {}", offer.name(), offer.listed_from()))
        .collect();
    assert_eq!(sorted(listed), rows(&text.stdout, 2));
    let places: Vec<(&str, &str)> = offers
        .iter()
        .map(|offer| (offer.place(), offer.name()))
        .collect();
    assert_eq!(places, [("loaded", "notes"), ("shelf", "deploy")]);
    assert_eq!(offers[1].origin(), source.path().to_string_lossy());
    assert_eq!(offers[0].summary(), Some("Take notes"));
    assert!(offers.iter().all(|offer| offer.kind() == "skill"));
    assert!(warnings.is_empty());
    assert!(json.ok && text.ok);
}

/// A loaded workspace that has gone is a warning beside what the rest
/// offer, in the envelope as on stderr, and fails the JSON as the text.
#[test]
fn a_loaded_workspace_gone_is_a_warning_beside_gather_list_as_json() {
    let (dir, _source) = workspace_offering();
    let hub = dir.path().join("hub");
    fs::remove_dir_all(dir.path().join("team")).unwrap();

    let text = weaver(&hub, &["gather", "list"]).unwrap();
    let json = weaver(&hub, &["gather", "list", "--json"]).unwrap();

    let (offers, warnings): (Vec<Offer>, _) = enveloped(&json);
    assert_eq!(offers.len(), 1);
    assert_eq!(warnings, warned(&text));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert_eq!(warning_codes(&json), stderr_codes(&text));
    assert!(warning_codes(&json)[0].is_some());
    assert_eq!(json.stderr, text.stderr);
    assert!(!text.ok);
    assert_eq!(json.ok, text.ok);
}
