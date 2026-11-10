//! Bringing MCP servers from the registry into the library: the
//! reference's "From the MCP Registry".
//!
//! No registry is asked. Each use case serves the one on loopback
//! (`common/mcp_registry.rs`), answering what the official registry
//! answered when #106 recorded it, through `--registry`, which takes plain
//! http to this machine alone; the reference shows each line without it,
//! as a person types it against the official one.

use crate::mcp_registry::Registry;
use crate::Scenario;

/// `line`, its `gather mcp <command>` told to ask `registry`.
fn asking(registry: &Registry, line: &str) -> String {
    let (head, rest) = line
        .split_once(" mcp search ")
        .map(|(head, rest)| (format!("{head} mcp search"), rest))
        .or_else(|| {
            line.split_once(" mcp add ")
                .map(|(head, rest)| (format!("{head} mcp add"), rest))
        })
        .unwrap_or_else(|| panic!("`{line}` is no line of gather mcp"));
    format!("{head} --registry {} {rest}", registry.url())
}

/// `~/Projects`, and the registry on loopback.
fn projects() -> (Scenario, Registry) {
    (Scenario::projects(&["collapse"]), Registry::recorded())
}

/// The definition the reference shows, written by an import from the
/// official registry, its provenance's registry and time those of the
/// one `text` was written by on loopback, now.
fn as_recorded(text: &str, registry: &Registry) -> String {
    text.replacen(
        &format!("registry = \"{}\"", registry.url()),
        "registry = \"https://registry.modelcontextprotocol.io\"",
        1,
    )
    .lines()
    .map(|line| {
        if line.starts_with("fetched_at = ") {
            String::from("fetched_at = 2026-11-10T12:00:00Z\n")
        } else {
            format!("{line}\n")
        }
    })
    .collect()
}

const LIBRARY: &str = "~/Projects/.mindweaver/library/mcp";

#[test]
fn a_search_lists_each_server_whose_name_holds_the_query_with_its_version_status_and_ways() {
    let (scenario, registry) = projects();

    scenario
        .run(
            "~/Projects",
            &asking(&registry, "mindweaver gather mcp search airtable"),
        )
        .succeeds()
        .prints(
            "ai.waystation/airtable                           0.3.1   active  http, sse\n\
             com.airtable/mcp                                 0.1.0   active  http\n\
             com.insightfulpipe/airtable                      1.0.0   active  http\n\
             com.mcparmory/airtable                           1.0.3   active  pypi, oci\n\
             io.github.Automations-Project/airtable-user-mcp  2.1.1   active  npm\n\
             io.github.domdomegg/airtable-mcp-server          1.14.0  active  npm, oci, mcpb\n\
             io.github.mrfentmen/airtable-mcp                 1.1.0   active  npm\n\
             io.github.pipeworx-io/airtable                   0.2.1   active  npm, http\n",
        )
        .quiet();

    assert_eq!(registry.paths(), ["/v0.1/servers"; 3]);
}

#[test]
fn a_search_too_short_for_the_registry_asks_nothing() {
    let (scenario, registry) = projects();

    scenario
        .run(
            "~/Projects",
            &asking(&registry, "mindweaver gather mcp search ab"),
        )
        .exits(1)
        .prints("")
        .says("error: `ab` is too short to search for: the registry takes 3 characters or more\n");

    assert!(registry.asked().is_empty());
}

/// The acceptance: without `--yes`, what an agent would run and nothing
/// written.
#[test]
fn adding_a_server_says_what_an_agent_would_run_and_writes_nothing_until_yes() {
    let (scenario, registry) = projects();

    scenario
        .run(
            "~/Projects",
            &asking(
                &registry,
                "mindweaver gather mcp add io.github.buildkite/buildkite-mcp-server",
            ),
        )
        .succeeds()
        .prints(
            "io.github.buildkite/buildkite-mcp-server 0.7.0, active\n  \
             runs: docker run -i --rm -e BUILDKITE_API_TOKEN \
             ghcr.io/buildkite/buildkite-mcp-server:0.7.0\n  \
             reads $BUILDKITE_API_TOKEN (a secret) where the agents run\n\
             would create mcp buildkite-mcp-server at \
             /home/you/Projects/.mindweaver/library/mcp/buildkite-mcp-server.toml\n\
             nothing written: add --yes to go ahead\n",
        )
        .quiet();

    assert!(!scenario.exists(LIBRARY));
}

/// With `--yes`, the definition, and nothing started: every program the
/// definition names is one on a PATH of the test's that would say it ran.
#[test]
fn adding_it_with_yes_writes_its_definition_and_starts_nothing() {
    let (scenario, registry) = projects();
    let ran = "~/ran";
    let tattle = format!(
        "#!/bin/sh\necho \"$0 $*\" >> {}\n",
        scenario.at(ran).display()
    );
    for program in ["docker", "npx", "uvx", "dnx"] {
        scenario.program(&format!("~/bin/{program}"), &tattle);
    }

    scenario
        .run_on_path(
            "~/Projects",
            &asking(
                &registry,
                "mindweaver gather mcp add io.github.buildkite/buildkite-mcp-server --yes",
            ),
            &[],
            &["~/bin"],
        )
        .succeeds()
        .prints(
            "io.github.buildkite/buildkite-mcp-server 0.7.0, active\n  \
             runs: docker run -i --rm -e BUILDKITE_API_TOKEN \
             ghcr.io/buildkite/buildkite-mcp-server:0.7.0\n  \
             reads $BUILDKITE_API_TOKEN (a secret) where the agents run\n\
             created mcp buildkite-mcp-server at \
             /home/you/Projects/.mindweaver/library/mcp/buildkite-mcp-server.toml\n",
        )
        .quiet();

    assert!(!scenario.exists(ran), "{}", scenario.read(ran));
    assert_eq!(
        as_recorded(
            &scenario.read(&format!("{LIBRARY}/buildkite-mcp-server.toml")),
            &registry
        ),
        r#"# An MCP server imported from an MCP registry: each agent's own entry
# for it is written from this file, and `show` says how each agent is
# given it.
format = 1
name = "buildkite-mcp-server"
description = "MCP server exposing Buildkite API data (pipelines, builds, jobs, tests) to AI tooling and editors."

# How an agent starts it: the program, what it is started with, and what
# its environment is given beside the agent's own.
[transport]
type = "stdio"
command = "docker"
args = ["run", "-i", "--rm", "-e", "BUILDKITE_API_TOKEN", "ghcr.io/buildkite/buildkite-mcp-server:0.7.0"]
env = { BUILDKITE_API_TOKEN = "{BUILDKITE_API_TOKEN}" }

# What is supplied when an agent starts it, each written between braces
# where it goes and read from the environment variable it names: no value
# is written here, and a secret never.
[[inputs]]
name = "BUILDKITE_API_TOKEN"
kind = "env"
description = "Buildkite API token for authentication. Get one from https://buildkite.com/user/api-access-tokens"
required = true
secret = true
env_var = "BUILDKITE_API_TOKEN"

# Where it came from: the registry, what it names the server, and the
# version imported.
[provenance]
registry = "https://registry.modelcontextprotocol.io"
upstream_name = "io.github.buildkite/buildkite-mcp-server"
version = "0.7.0"
fetched_at = 2026-11-10T12:00:00Z
"#
    );
    scenario
        .run("~/Projects", "mindweaver validate mcp")
        .succeeds()
        .prints("buildkite-mcp-server: ok\n\n1 MCP server checked, 0 invalid\n");
}

/// A server listing more than one way says which it writes, the first an
/// agent reaches with nothing installed, and `--way` writes another.
#[test]
fn a_server_listing_several_ways_says_which_it_writes_and_way_writes_another() {
    let (scenario, registry) = projects();

    scenario
        .run(
            "~/Projects",
            &asking(&registry, "mindweaver gather mcp add ai.biel/biel-ai"),
        )
        .succeeds()
        .prints(
            "ai.biel/biel-ai 2.0.0, active\n  \
             way 1 of 2 (http); --way writes another: 2 sse\n  \
             reaches: https://mcp.biel.ai/v2/{project_slug}/mcp\n  \
             reads $BIEL_AI_PROJECT_SLUG where the agents run\n\
             would create mcp biel-ai at /home/you/Projects/.mindweaver/library/mcp/biel-ai.toml\n\
             nothing written: add --yes to go ahead\n",
        )
        .quiet();
    scenario
        .run(
            "~/Projects",
            &asking(
                &registry,
                "mindweaver gather mcp add ai.biel/biel-ai --way 2",
            ),
        )
        .succeeds()
        .prints(
            "ai.biel/biel-ai 2.0.0, active\n  \
             way 2 of 2 (sse); --way writes another: 1 http\n  \
             reaches: https://mcp.biel.ai/sse?project_slug={project_slug}&domain={domain}\n  \
             reads $BIEL_AI_PROJECT_SLUG and $BIEL_AI_DOMAIN where the agents run\n\
             would create mcp biel-ai at /home/you/Projects/.mindweaver/library/mcp/biel-ai.toml\n\
             nothing written: add --yes to go ahead\n",
        )
        .says(
            "note: https://mcp.biel.ai/sse?project_slug={project_slug}&domain={domain}: reached \
             over server-sent events, which the MCP specification has replaced, and which Codex \
             and OpenCode do not take\n",
        );
}

#[test]
fn a_deprecated_server_is_added_saying_why() {
    let (scenario, registry) = projects();

    scenario
        .run(
            "~/Projects",
            &asking(
                &registry,
                "mindweaver gather mcp add ai.agentlookups/greenlight --yes",
            ),
        )
        .succeeds()
        .prints(
            "ai.agentlookups/greenlight 1.0.0, deprecated\n  \
             reaches: https://solar.agentlookups.ai/mcp\n\
             created mcp agentlookups-greenlight at \
             /home/you/Projects/.mindweaver/library/mcp/agentlookups-greenlight.toml\n",
        )
        .says(
            "note: deprecated by its publisher: GreenLight closed on 2026-09-28 because its \
             answers could not be kept correct; the server answers 410 Gone. Notice: \
             https://solar.agentlookups.ai/\n",
        );
}

/// What no command of its registry's starts is a manual step, said with
/// what to do instead, and nothing is written.
#[test]
fn a_server_mindweaver_writes_no_way_of_is_a_manual_step_and_nothing_is_written() {
    let (scenario, registry) = projects();

    scenario
        .run(
            "~/Projects",
            &asking(
                &registry,
                "mindweaver gather mcp add app.internetmonitor/internet-monitor --yes",
            ),
        )
        .exits(1)
        .prints("")
        .says(
            "error: `app.internetmonitor/internet-monitor` is started or reached in no way \
             Mindweaver writes; by hand: an MCP bundle at \
             https://github.com/andrscrrn/internet-monitor-mcp/releases/download/v1.0.2/internet-monitor.mcpb \
             (SHA-256 6414b553daf4ab512828dcc53143c1170bfdb30fde001388731329e6acf3df52), which \
             Mindweaver does not install: download it, check it against its SHA-256, and \
             install it in each agent by hand\n",
        );

    assert!(!scenario.exists(LIBRARY));
}

#[test]
fn a_version_the_registry_does_not_hold_is_refused() {
    let (scenario, registry) = projects();

    scenario
        .run(
            "~/Projects",
            &asking(
                &registry,
                "mindweaver gather mcp add ai.clawfetch/mcp --version 9.9.9 --yes",
            ),
        )
        .exits(1)
        .says("error: the registry holds no version 9.9.9 of `ai.clawfetch/mcp`\n");

    assert!(!scenario.exists(LIBRARY));
}

/// A server is added once: one of its name already in the library is
/// refused, dry run and all, as `add` refuses one, and left as it is.
#[test]
fn a_server_already_in_the_library_is_refused_and_left_as_it_is() {
    let (scenario, registry) = projects();
    scenario.given(
        "~/Projects",
        &asking(
            &registry,
            "mindweaver gather mcp add ai.clawfetch/mcp --yes",
        ),
    );
    let before = scenario.read(&format!("{LIBRARY}/clawfetch-mcp.toml"));

    for line in [
        "mindweaver gather mcp add ai.clawfetch/mcp",
        "mindweaver gather mcp add ai.clawfetch/mcp --yes",
    ] {
        scenario
            .run("~/Projects", &asking(&registry, line))
            .exits(1)
            .prints("")
            .says(
                "error: /home/you/Projects/.mindweaver/library/mcp/clawfetch-mcp.toml: already \
                 exists, and is left alone\n",
            );
    }
    assert_eq!(
        scenario.read(&format!("{LIBRARY}/clawfetch-mcp.toml")),
        before
    );
}
