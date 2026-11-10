//! Gathering skills from a git repository onto the workspace's shelf, and
//! installing from there: the reference's "Gathering skills from
//! elsewhere".
//!
//! A source is named by its path from the workspace, as the reference names
//! one by its URL: a table pads every row to its longest source, which a
//! temporary folder's path would make as long as the system's says.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};

use crate::{skill, Scenario};

const SOURCE: &str = "~/src/acme-skills";

/// A skill's file declaring the licence `license`, as its author writes it.
fn licensed(name: &str, description: &str, license: &str) -> String {
    format!(
        "---\nname: {name}\ndescription: {description}\nlicense: {license}\n---\n\n# {name}\n\n\
         Steps.\n"
    )
}

/// `~/Projects` linking `collapse`, beside `~/src/acme-skills`, a repository
/// offering two skills, the first declaring its licence and the second
/// saying nothing of one.
fn acme() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &licensed(
                    "commit-style",
                    "How this repo writes commit messages",
                    "MIT",
                ),
            ),
            (
                "skills/ddd-reviewer/SKILL.md",
                &skill(
                    "ddd-reviewer",
                    "Review a change against the DDD layering rules",
                ),
            ),
        ],
    );
    scenario
}

#[test]
fn gathering_a_repository_puts_its_skills_on_the_shelf_and_says_each() {
    let scenario = acme();

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  commit-style  How this repo writes commit messages\n\
             \x20 added  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 2 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert!(
        !scenario.exists("~/Projects/collapse/.claude"),
        "nothing is written into a project"
    );
}

#[test]
fn gathering_again_says_what_moved() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .change(
            SOURCE,
            &[(
                "skills/commit-style/SKILL.md",
                &format!(
                    "{}Keep the subject short.\n",
                    skill("commit-style", "How this repo writes commit messages")
                ),
            )],
        );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 updated    commit-style  How this repo writes commit messages\n\
             \x20 unchanged  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 0 added, 1 updated, 1 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();
}

#[test]
fn a_skill_breaking_the_format_is_gathered_with_a_warning() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/other-rules",
        &[(
            "skills/pdf--forms/SKILL.md",
            &skill("pdf--forms", "Fill in PDF forms"),
        )],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/other-rules")
        .exits(1)
        .prints(&format!(
            "src-other-rules: ../src/other-rules at {}\n\
             \x20 added  pdf--forms  Fill in PDF forms\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head("~/src/other-rules")
        ))
        .says(
            "warning: /home/you/Projects/.mindweaver/skills/..-src-other-rules/pdf--forms/SKILL.md: \
             `pdf--forms` is not usable as a name: it has to be lowercase letters and digits, \
             joined by single hyphens\n",
        );
}

#[test]
fn gather_list_says_what_the_shelf_holds_and_from_where() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "commit-style  src-acme-skills  MIT      How this repo writes commit messages\n",
            "ddd-reviewer  src-acme-skills  unknown  Review a change against the DDD layering \
             rules\n",
        ))
        .quiet();
}

#[test]
fn gathering_at_a_pin_takes_that_commit_and_says_the_source_is_pinned() {
    let scenario = acme();
    let pin = scenario.commit(SOURCE);
    scenario.change(
        SOURCE,
        &[(
            "skills/ddd-reviewer/SKILL.md",
            &skill("ddd-reviewer", "Review a change, as it is now"),
        )],
    );

    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {pin}"),
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}, pinned\n\
             \x20 added  commit-style  How this repo writes commit messages\n\
             \x20 added  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 2 added, 0 updated, 0 unchanged\n",
            &pin[..7]
        ))
        .quiet();
}

#[test]
fn gather_list_shows_the_pin_of_a_pinned_source_beside_its_name() {
    let scenario = acme();
    let pin = scenario.commit(SOURCE);
    scenario
        .source(
            "~/src/other-rules",
            &[(
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "Conventional commits, strictly"),
            )],
        )
        .given(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {pin}"),
        )
        .given("~/Projects", "mindweaver gather git ../src/other-rules");

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(&format!(
            "commit-style  src-acme-skills  pinned {short}  MIT      How this repo writes commit \
             messages\n\
             commit-style  src-other-rules                  unknown  Conventional commits, strictly\n\
             ddd-reviewer  src-acme-skills  pinned {short}  unknown  Review a change against the \
             DDD layering rules\n",
            short = &pin[..7]
        ))
        .quiet();
}

#[test]
fn gathering_again_without_the_pin_follows_the_branch_again() {
    let scenario = acme();
    let pin = scenario.commit(SOURCE);
    scenario.given(
        "~/Projects",
        &format!("mindweaver gather git ../src/acme-skills --pin {pin}"),
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 unchanged  commit-style  How this repo writes commit messages\n\
             \x20 unchanged  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 0 added, 0 updated, 2 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();
    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "commit-style  src-acme-skills  MIT      How this repo writes commit messages\n",
            "ddd-reviewer  src-acme-skills  unknown  Review a change against the DDD layering \
             rules\n",
        ));
}

#[test]
fn a_pin_that_is_no_commit_in_full_is_refused_before_anything_is_fetched() {
    let scenario = acme();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --pin a1b2c3d",
        )
        .exits(1)
        .prints("")
        .says(
            "error: `a1b2c3d` cannot pin a source: it has to be a commit in full, 40 hexadecimal \
             digits, not a commit shortened, a branch or a tag\n",
        );

    assert!(!scenario.exists("~/Projects/.mindweaver/cache/..-src-acme-skills"));
}

#[test]
fn a_pin_the_repository_does_not_have_is_refused_and_nothing_is_gathered() {
    let scenario = acme();
    let missing = "0123456789abcdef0123456789abcdef01234567";

    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {missing}"),
        )
        .exits(1)
        .prints("")
        .says(&format!(
            "error: ../src/acme-skills: has no commit {missing} that its default branch reaches, \
             so nothing is gathered at it\n"
        ));

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "nothing gathered yet\n",
            "  take some with `mindweaver gather git <url>`, or offer a workspace's own with \
             `mindweaver load <path>`\n",
        ));
}

#[test]
fn a_gathered_skill_installs_from_the_source_it_came_from() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver install -p collapse commit-style")
        .succeeds()
        .prints("installed commit-style into collapse, from ../src/acme-skills\n")
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"),
        scenario.read("~/src/acme-skills/skills/commit-style/SKILL.md")
    );
}

#[test]
fn a_gathered_skill_the_format_refuses_is_not_installed() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/other-rules",
        &[(
            "skills/pdf--forms/SKILL.md",
            &skill("pdf--forms", "Fill in PDF forms"),
        )],
    );
    // Gathered with its warning, which a step building the scenario would
    // refuse: the warning is the use case before this one.
    scenario
        .run("~/Projects", "mindweaver gather git ../src/other-rules")
        .exits(1);

    scenario
        .run("~/Projects", "mindweaver install -p collapse pdf--forms")
        .exits(1)
        .prints("")
        .says(
            "error: `pdf--forms` cannot be installed: `pdf--forms` is not usable as a name: \
             it has to be lowercase letters and digits, joined by single hyphens\n",
        );

    assert!(!scenario.exists("~/Projects/collapse/.claude"));
}

#[test]
fn a_url_carrying_a_token_is_refused_before_anything_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git https://me:ghp_t0ken@github.com/acme/private-skills",
        )
        .exits(1)
        .prints("")
        .says(
            "error: https://github.com/acme/private-skills: a password, a token or, over \
             http(s), a user name in the URL is refused, because gathering stores and shows the \
             URL: gather it without one, and hand the token over with --token-env or --gh, or \
             configure a git credential helper that has it\n",
        );

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "nothing gathered yet\n",
            "  take some with `mindweaver gather git <url>`, or offer a workspace's own with \
             `mindweaver load <path>`\n",
        ));
}

#[test]
fn mindweaver_validate_reads_the_shelf_beside_the_library_and_the_projects() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run("~/Projects", "mindweaver validate")
        .succeeds()
        .prints(concat!(
            "commit-style: ok\n",
            "ddd-reviewer: ok\n",
            "\n",
            "2 skills checked, 0 invalid\n",
        ))
        .quiet();
}

// ---------------------------------------------------------------------------
// Wherever a repository keeps its skills (#101)
// ---------------------------------------------------------------------------

/// `~/Projects` linking `collapse`, beside `~/src/acme-skills`, a
/// repository keeping a skill in `skills/`, one where Claude reads them and
/// one in a plugin.
fn spread() -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "How this repo writes commit messages"),
            ),
            (
                ".claude/skills/lint/SKILL.md",
                &skill("lint", "Tidy the code"),
            ),
            (
                "plugins/docs/skills/pdf/SKILL.md",
                &skill("pdf", "Read PDFs"),
            ),
        ],
    );
    scenario
}

#[test]
fn gathering_takes_skills_from_every_place_a_repository_keeps_them() {
    let scenario = spread();

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  commit-style  How this repo writes commit messages\n\
             \x20 added  lint          Tidy the code\n\
             \x20 added  pdf           Read PDFs\n\
             \n\
             3 skills: 3 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert_eq!(
        scenario.files("~/Projects/.mindweaver/skills"),
        [
            "..-src-acme-skills/commit-style/SKILL.md",
            "..-src-acme-skills/lint/SKILL.md",
            "..-src-acme-skills/pdf/SKILL.md",
        ]
    );
}

#[test]
fn a_dry_run_says_what_would_be_taken_and_from_where_and_writes_nothing() {
    let scenario = spread();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --dry-run",
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 would add  commit-style  skills/commit-style      How this repo writes commit messages\n\
             \x20 would add  lint          .claude/skills/lint      Tidy the code\n\
             \x20 would add  pdf           plugins/docs/skills/pdf  Read PDFs\n\
             \n\
             3 skills: 3 to add, 0 to update, 0 unchanged\n\
             nothing gathered: run it without --dry-run to gather\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    for untouched in ["skills", "cache", "mindweaver.db"] {
        assert!(
            !scenario.exists(&format!("~/Projects/.mindweaver/{untouched}")),
            "{untouched}"
        );
    }
}

#[test]
fn a_dry_run_after_a_gather_says_what_would_move() {
    let scenario = spread();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .change(
            SOURCE,
            &[(
                ".claude/skills/lint/SKILL.md",
                &format!("{}Fix what it can.\n", skill("lint", "Tidy the code")),
            )],
        );
    let shelf = scenario.read("~/Projects/.mindweaver/skills/..-src-acme-skills/lint/SKILL.md");

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --dry-run",
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 would update  lint          .claude/skills/lint      Tidy the code\n\
             \x20 unchanged     commit-style  skills/commit-style      How this repo writes commit messages\n\
             \x20 unchanged     pdf           plugins/docs/skills/pdf  Read PDFs\n\
             \n\
             3 skills: 0 to add, 1 to update, 2 unchanged\n\
             nothing gathered: run it without --dry-run to gather\n",
            scenario.head(SOURCE)
        ))
        .quiet();

    assert_eq!(
        scenario.read("~/Projects/.mindweaver/skills/..-src-acme-skills/lint/SKILL.md"),
        shelf
    );
}

#[test]
fn a_skill_md_at_the_root_makes_the_repository_one_skill() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        "~/src/deploy-skill",
        &[
            ("SKILL.md", &skill("deploy", "Ship the service")),
            ("scripts/run.sh", "echo ship\n"),
            ("examples/inner/SKILL.md", &skill("inner", "An example")),
        ],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/deploy-skill")
        .succeeds()
        .prints(&format!(
            "src-deploy-skill: ../src/deploy-skill at {}\n\
             \x20 added  deploy  Ship the service\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head("~/src/deploy-skill")
        ))
        .quiet();

    assert_eq!(
        scenario.files("~/Projects/.mindweaver/skills"),
        [
            "..-src-deploy-skill/deploy/SKILL.md",
            "..-src-deploy-skill/deploy/examples/inner/SKILL.md",
            "..-src-deploy-skill/deploy/scripts/run.sh",
        ]
    );
}

#[test]
fn two_skills_one_source_calls_the_same_are_neither_gathered() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "skills/ops/deploy/SKILL.md",
                &skill("deploy", "The ops one"),
            ),
            (
                "skills/web/deploy/SKILL.md",
                &skill("deploy", "The web one"),
            ),
            ("skills/review/SKILL.md", &skill("review", "Read a change")),
        ],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .exits(1)
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  review  Read a change\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .says(
            "warning: skills/ops/deploy: not taken, since skills/web/deploy in the same source \
             is called `deploy` too; gather the one wanted with `--path`\n\
             warning: skills/web/deploy: not taken, since skills/ops/deploy in the same source \
             is called `deploy` too; gather the one wanted with `--path`\n",
        );
}

#[test]
fn a_skill_s_claude_plugin_folder_is_left_off_the_shelf_with_a_note() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source(
        SOURCE,
        &[
            (
                "plugins/ops/skills/deploy/SKILL.md",
                &skill("deploy", "Ship the service"),
            ),
            ("plugins/ops/skills/deploy/.claude-plugin/plugin.json", "{}"),
        ],
    );

    scenario
        .run("~/Projects", "mindweaver gather git ../src/acme-skills")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  deploy  Ship the service\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .says(
            "note: deploy was gathered without its .claude-plugin, since Claude would load the \
             skill as a plugin, with the hooks and MCP servers it declares\n",
        );

    assert_eq!(
        scenario.files("~/Projects/.mindweaver/skills"),
        ["..-src-acme-skills/deploy/SKILL.md"]
    );
}

#[test]
fn a_repository_holding_no_skill_says_where_it_looked() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.source("~/src/notes", &[("README.md", "Just notes.\n")]);

    scenario
        .run("~/Projects", "mindweaver gather git ../src/notes")
        .exits(1)
        .prints("")
        .says(
            "error: ../src/notes: no skill found in the repository: looked for a `SKILL.md` \
             there, in each folder there, and in each folder of `skills`, \
             `skills/<namespace>`, `.agents/skills`, `.claude/skills` and \
             `plugins/<plugin>/skills`; name the folder that holds them with `--path`\n",
        );
}

#[test]
fn path_gathers_only_the_folder_it_names() {
    let scenario = spread();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --path .claude/skills",
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  lint  Tidy the code\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head(SOURCE)
        ))
        .quiet();
}

#[test]
fn a_branch_given_twice_over_is_refused_before_anything_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git acme/skills@v1 --ref v2",
        )
        .exits(1)
        .prints("")
        .says("error: the URL names the branch `v1`, and `--ref` names `v2`: give it once\n");

    assert!(!scenario.exists("~/Projects/.mindweaver/cache"));
}

// ---------------------------------------------------------------------------
// A private repository: the reference's paragraphs on one, in "Gathering skills
// from elsewhere"
// ---------------------------------------------------------------------------

/// A private repository's server on loopback: whatever it is asked, it
/// asks who is asking (`401`, `WWW-Authenticate: Basic`), and notes each
/// login it was sent, `None` for a request that sent none. Over plain http,
/// where a gather never sends a credential, which is what it holds.
struct Guarded {
    address: SocketAddr,
    sent: Arc<Mutex<Vec<Option<String>>>>,
}

impl Guarded {
    fn listening() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().expect("where it listens");
        let sent = Arc::<Mutex<Vec<Option<String>>>>::default();
        let noted = Arc::clone(&sent);
        std::thread::spawn(move || {
            for mut stream in listener.incoming().flatten() {
                let mut request = Vec::new();
                let mut buffer = [0; 1024];
                while !request.windows(4).any(|end| end == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(read) => request.extend_from_slice(&buffer[..read]),
                    }
                }
                let request = String::from_utf8_lossy(&request).into_owned();
                noted.lock().unwrap().push(request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_owned())
                }));
                let _ = stream.write_all(
                    b"HTTP/1.1 401 Unauthorized\r\n\
                      WWW-Authenticate: Basic realm=\"private\"\r\n\
                      Content-Length: 0\r\n\
                      Connection: close\r\n\r\n",
                );
            }
        });
        Self { address, sent }
    }

    fn url(&self) -> String {
        format!("http://{}/acme/private-skills.git", self.address)
    }
}

/// An https URL on loopback where nothing listens: a fetch from it fails at
/// once, with no network, after whatever a gather does with a token first.
fn nowhere_over_https() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("where it listened").port();
    drop(listener);
    format!("https://127.0.0.1:{port}/acme/private-skills.git")
}

/// A token as a front end is handed one, drawn for one test.
fn a_token() -> String {
    mindweaver_core::mother::random::hex(40)
}

/// `gh` as a person logged in to `host` has it: `gh auth token --hostname
/// <host>` prints `token`, and anything else is `gh`'s error. Every line it
/// is run with is noted in `~/gh-asked`.
fn gh_logged_in(scenario: &Scenario, host: &str, token: &str) {
    scenario.program(
        "~/bin/gh",
        &format!(
            "#!/bin/sh\n\
             printf '%s\\n' \"$*\" >> \"$HOME/gh-asked\"\n\
             if [ \"$*\" = 'auth token --hostname {host}' ]; then\n\
             \x20 printf '%s\\n' '{token}'\n\
             \x20 exit 0\n\
             fi\n\
             echo \"no oauth token found for $4\" >&2\n\
             exit 1\n"
        ),
    );
}

#[test]
fn a_private_repository_nothing_logs_into_fails_saying_how_to_hand_over_a_token() {
    let scenario = Scenario::projects(&["collapse"]);
    let server = Guarded::listening();
    // What a prompt would run, were one ever reached: it leaves a mark.
    scenario.program(
        "~/bin/askpass",
        "#!/bin/sh\ntouch \"$HOME/asked\"\necho hunter2\n",
    );
    let askpass = scenario.at("~/bin/askpass").display().to_string();
    let line = format!("mindweaver gather git {}", server.url());

    scenario
        .run_with(
            "~/Projects",
            &line,
            &[
                ("GIT_ASKPASS", &askpass),
                ("SSH_ASKPASS", &askpass),
                ("GIT_TERMINAL_PROMPT", "1"),
            ],
        )
        .exits(1)
        .prints("")
        .says(&format!(
            "error: {}: needs credentials, and gathering never prompts for them: configure a git \
             credential helper, use ssh-agent for an ssh URL, or hand it a token with --token-env \
             or --gh\n",
            server.url()
        ));

    assert_eq!(*server.sent.lock().unwrap(), [None]);
    assert!(!scenario.exists("~/asked"), "a prompt was reached");
}

#[test]
fn a_token_variable_that_holds_nothing_is_said_before_anything_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git acme/private-skills --token-env GITHUB_TOKEN",
        )
        .exits(1)
        .prints("")
        .says(
            "error: $GITHUB_TOKEN holds no token: set it for this command, or leave --token-env \
             out to log in as git does\n",
        );

    assert!(!scenario.exists("~/Projects/.mindweaver/cache"));
}

#[test]
fn a_token_for_a_repository_reached_over_ssh_is_refused_before_anything_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);
    let token = a_token();

    let ran = scenario.run_with(
        "~/Projects",
        "mindweaver gather git git@github.com:acme/private-skills.git --token-env GITHUB_TOKEN",
        &[("GITHUB_TOKEN", &token)],
    );

    ran.exits(1).prints("").says(
        "error: git@github.com:acme/private-skills.git: reached over ssh, which logs in with keys \
         of its own, so a token would be for nothing: leave --token-env and --gh out, or gather \
         its https URL\n",
    );
    assert!(!scenario.exists("~/Projects/.mindweaver/cache"));
}

#[test]
fn a_token_for_a_repository_over_plain_http_is_refused_and_never_sent() {
    let scenario = Scenario::projects(&["collapse"]);
    let server = Guarded::listening();
    let token = a_token();
    let line = format!(
        "mindweaver gather git {} --token-env GITHUB_TOKEN",
        server.url()
    );

    scenario
        .run_with("~/Projects", &line, &[("GITHUB_TOKEN", &token)])
        .exits(1)
        .prints("")
        .says(&format!(
            "error: {}: not fetched over https, the one way a token is sent: leave --token-env \
             and --gh out, or gather its https URL\n",
            server.url()
        ));

    assert_eq!(*server.sent.lock().unwrap(), []);
}

#[test]
fn gh_with_no_token_for_the_repository_s_host_is_said_and_nothing_is_fetched() {
    let scenario = Scenario::projects(&["collapse"]);
    gh_logged_in(&scenario, "gitlab.com", &a_token());

    scenario
        .run_on_path(
            "~/Projects",
            "mindweaver gather git acme/private-skills --gh",
            &[],
            &["~/bin"],
        )
        .exits(1)
        .prints("")
        .says(
            "error: `gh` has no token for github.com: log in with `gh auth login --hostname \
             github.com`, or name a variable holding a token with --token-env\n",
        );

    assert_eq!(
        scenario.read("~/gh-asked"),
        "auth token --hostname github.com\n"
    );
    assert!(!scenario.exists("~/Projects/.mindweaver/cache"));
}

#[test]
fn gh_asked_for_where_there_is_no_gh_is_said() {
    let scenario = Scenario::projects(&["collapse"]);
    scenario.folder("~/bin");

    scenario
        .run_on_path(
            "~/Projects",
            "mindweaver gather git acme/private-skills --gh",
            &[],
            &["~/bin"],
        )
        .exits(1)
        .prints("")
        .says(
            "error: --gh asks GitHub's `gh` for the token, and there is no `gh` on the PATH: \
             install it and log in with `gh auth login`, or name a variable holding a token with \
             --token-env\n",
        );
}

#[test]
fn gh_is_never_asked_for_a_repository_no_token_goes_to() {
    let scenario = Scenario::projects(&["collapse"]);
    gh_logged_in(&scenario, "github.com", &a_token());

    scenario
        .run_on_path(
            "~/Projects",
            "mindweaver gather git git@github.com:acme/private-skills.git --gh",
            &[],
            &["~/bin"],
        )
        .exits(1)
        .prints("")
        .says(
            "error: git@github.com:acme/private-skills.git: reached over ssh, which logs in with \
             keys of its own, so a token would be for nothing: leave --token-env and --gh out, or \
             gather its https URL\n",
        );

    assert!(!scenario.exists("~/gh-asked"));
}

/// The token `gh` gives reaches the fetch, a dry run's too, and nothing a
/// gather says or leaves holds it: the repository is on this machine,
/// where nothing answers, so the fetch fails once it has the token.
#[test]
fn the_token_gh_gives_is_handed_to_the_fetch_and_kept_nowhere() {
    let scenario = Scenario::projects(&["collapse"]);
    let token = a_token();
    gh_logged_in(&scenario, "127.0.0.1", &token);
    let url = nowhere_over_https();

    for line in [
        format!("mindweaver gather git {url} --gh"),
        format!("mindweaver gather git {url} --dry-run --gh"),
    ] {
        scenario
            .run_on_path("~/Projects", &line, &[], &["~/bin"])
            .exits(1)
            .prints("")
            .says(&format!(
                "error: {url}: cannot be fetched: An IO error occurred when talking to the server\n"
            ));
    }

    assert_eq!(
        scenario.read("~/gh-asked"),
        "auth token --hostname 127.0.0.1\nauth token --hostname 127.0.0.1\n"
    );
    let kept: Vec<String> = scenario
        .files("~/Projects")
        .into_iter()
        .filter(|file| {
            std::fs::read(scenario.at(&format!("~/Projects/{file}")))
                .is_ok_and(|bytes| String::from_utf8_lossy(&bytes).contains(&token))
        })
        .collect();
    assert_eq!(kept, Vec::<String>::new());
}

// ---------------------------------------------------------------------------
// Managing the sources (#100): the reference's "Managing the sources"
// ---------------------------------------------------------------------------

/// `seconds` since the Unix epoch, as `gather sources` says a time: its day
/// and minute in UTC. Counted a year and a month at a time, rather than by
/// the reckoning the command uses, so the two are held to each other.
fn utc(seconds: i64) -> String {
    let leap = |year: i64| (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let (mut days, into) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let mut year = 1970;
    while days >= if leap(year) { 366 } else { 365 } {
        days -= if leap(year) { 366 } else { 365 };
        year += 1;
    }
    let lengths = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= lengths[month] {
        days -= lengths[month];
        month += 1;
    }
    format!(
        "{year:04}-{:02}-{:02} {:02}:{:02} UTC",
        month + 1,
        days + 1,
        into / 3_600,
        into % 3_600 / 60
    )
}

/// When the source called `name` was last gathered, as `gather sources
/// --json` says it, in `gather sources`'s words: no run makes it the same.
fn gathered_when(scenario: &Scenario, name: &str) -> String {
    let ran = scenario.run("~/Projects", "mindweaver gather sources --json");
    ran.succeeds();
    let envelope: serde_json::Value = serde_json::from_str(&ran.stdout).expect("JSON");
    let source = envelope["data"]
        .as_array()
        .expect("a list of sources")
        .iter()
        .find(|source| source["name"] == name)
        .unwrap_or_else(|| panic!("no source {name} in {envelope:#}"));
    utc(source["gathered_at"]
        .as_i64()
        .expect("when it was gathered"))
}

/// [`acme`], gathered, with `commit-style` installed into `collapse`.
fn installed_from_acme() -> Scenario {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .given("~/Projects", "mindweaver install -p collapse commit-style");
    scenario
}

#[test]
fn gather_sources_lists_each_source_by_name_with_what_the_shelf_holds_and_when() {
    let scenario = acme();
    let pin = scenario.commit(SOURCE);
    scenario
        .source(
            "~/src/other-rules",
            &[(
                "agents/commit-style/SKILL.md",
                &skill("commit-style", "Conventional commits, strictly"),
            )],
        )
        .given(
            "~/Projects",
            &format!("mindweaver gather git ../src/acme-skills --pin {pin}"),
        )
        .given(
            "~/Projects",
            "mindweaver gather git ../src/other-rules --path agents --name other-rules",
        );
    let (acme, other) = (
        gathered_when(&scenario, "src-acme-skills"),
        gathered_when(&scenario, "other-rules"),
    );

    scenario
        .run("~/Projects", "mindweaver gather sources")
        .succeeds()
        .prints(&format!(
            "src-acme-skills  ../src/acme-skills               pinned {short}  2 skills  \
             gathered {acme}\n\
             other-rules      ../src/other-rules  path agents                  1 skill   \
             gathered {other}\n",
            short = &pin[..7]
        ))
        .quiet();
}

#[test]
fn gather_sources_with_none_says_how_to_gather_one() {
    let scenario = Scenario::projects(&["collapse"]);

    scenario
        .run("~/Projects", "mindweaver gather sources")
        .succeeds()
        .prints("no sources yet\n  take some with `mindweaver gather git <url>`\n")
        .quiet();
}

#[test]
fn gather_rename_calls_a_source_something_else_and_from_takes_the_new_name() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");

    scenario
        .run(
            "~/Projects",
            "mindweaver gather rename src-acme-skills acme-skills",
        )
        .succeeds()
        .prints("renamed source src-acme-skills to acme-skills, gathered from ../src/acme-skills\n")
        .quiet();

    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "commit-style  acme-skills  MIT      How this repo writes commit messages\n",
            "ddd-reviewer  acme-skills  unknown  Review a change against the DDD layering rules\n",
        ));
    scenario
        .run(
            "~/Projects",
            "mindweaver install -p collapse commit-style --from acme-skills",
        )
        .succeeds()
        .prints("installed commit-style into collapse, from ../src/acme-skills\n")
        .quiet();
}

#[test]
fn gather_rename_refuses_a_name_another_source_has_or_none_can() {
    let scenario = acme();
    scenario
        .source(
            "~/src/other-rules",
            &[(
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "Conventional commits, strictly"),
            )],
        )
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .given("~/Projects", "mindweaver gather git ../src/other-rules");

    scenario
        .run(
            "~/Projects",
            "mindweaver gather rename src-acme-skills src-other-rules",
        )
        .exits(1)
        .prints("")
        .says(
            "error: `src-other-rules` is the name of the source at ../src/other-rules already: \
             give this one another\n",
        );
    scenario
        .run(
            "~/Projects",
            "mindweaver gather rename src-acme-skills acme/skills",
        )
        .exits(1)
        .prints("")
        .says(
            "error: `acme/skills` cannot name a source: it has to be letters, digits, `.`, `_` \
             and `-`, not starting with `-`, and neither `.`, `..` nor `workspace`\n",
        );
    scenario
        .run("~/Projects", "mindweaver gather rename acme team")
        .exits(1)
        .prints("")
        .says(
            "error: no source is called `acme`: `mindweaver gather sources` lists them by name\n",
        );
}

#[test]
fn gather_update_says_what_each_source_added_updated_and_removed() {
    let scenario = acme();
    scenario
        .source(
            "~/src/other-rules",
            &[(
                "skills/commit-style/SKILL.md",
                &skill("commit-style", "Conventional commits, strictly"),
            )],
        )
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .given("~/Projects", "mindweaver gather git ../src/other-rules")
        .change(
            SOURCE,
            &[
                (
                    "skills/commit-style/SKILL.md",
                    &licensed(
                        "commit-style",
                        "How this repo writes commit messages, briefly",
                        "MIT",
                    ),
                ),
                ("skills/lint/SKILL.md", &skill("lint", "Tidy the code")),
            ],
        )
        .drop_from(SOURCE, &["skills/ddd-reviewer"]);

    scenario
        .run("~/Projects", "mindweaver gather update --all")
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {acme}\n\
             \x20 added    lint          Tidy the code\n\
             \x20 updated  commit-style  How this repo writes commit messages, briefly\n\
             \x20 removed  ddd-reviewer  Review a change against the DDD layering rules\n\
             \n\
             2 skills: 1 added, 1 updated, 0 unchanged, 1 removed\n\
             \n\
             src-other-rules: ../src/other-rules at {other}\n\
             \x20 unchanged  commit-style  Conventional commits, strictly\n\
             \n\
             1 skill: 0 added, 0 updated, 1 unchanged\n",
            acme = scenario.head(SOURCE),
            other = scenario.head("~/src/other-rules"),
        ))
        .quiet();

    assert!(!scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills/ddd-reviewer"));
    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "commit-style  src-acme-skills  MIT      How this repo writes commit messages, briefly\n",
            "commit-style  src-other-rules  unknown  Conventional commits, strictly\n",
            "lint          src-acme-skills  unknown  Tidy the code\n",
        ));
}

#[test]
fn after_gather_update_an_install_from_a_source_that_moved_on_is_outdated() {
    let scenario = installed_from_acme();
    scenario.change(
        SOURCE,
        &[(
            "skills/commit-style/SKILL.md",
            &licensed(
                "commit-style",
                "How this repo writes commit messages, briefly",
                "MIT",
            ),
        )],
    );
    scenario.given("~/Projects", "mindweaver gather update src-acme-skills");

    scenario
        .run("~/Projects", "mindweaver status")
        .succeeds()
        .prints("commit-style  outdated  claude,codex,opencode  ../src/acme-skills\n")
        .quiet();
}

/// A source on this machine is fetched by its path, which no token goes
/// to: `--gh` asks `gh` for none, and the update says why that source was
/// not gathered, beside the rest.
#[test]
fn gather_update_with_gh_refuses_it_for_a_source_no_token_goes_to_and_asks_gh_nothing() {
    let scenario = acme();
    scenario.given("~/Projects", "mindweaver gather git ../src/acme-skills");
    gh_logged_in(&scenario, "github.com", &a_token());

    scenario
        .run_on_path(
            "~/Projects",
            "mindweaver gather update --all --gh",
            &[],
            &["~/bin"],
        )
        .exits(1)
        .prints("")
        .says(
            "warning: src-acme-skills: ../src/acme-skills: not fetched over https, the one way a \
             token is sent: leave --token-env and --gh out, or gather its https URL\n",
        );

    assert!(!scenario.exists("~/gh-asked"));
}

#[test]
fn gather_update_of_a_name_no_source_has_gathers_nothing() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .change(
            SOURCE,
            &[("skills/lint/SKILL.md", &skill("lint", "Tidy the code"))],
        );

    scenario
        .run(
            "~/Projects",
            "mindweaver gather update src-acme-skills acme-skills",
        )
        .exits(1)
        .prints("")
        .says(
            "error: no source is called `acme-skills`: `mindweaver gather sources` lists them \
             by name\n",
        );

    assert!(!scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills/lint"));
}

#[test]
fn gathering_a_source_again_takes_off_the_shelf_what_it_no_longer_has() {
    let scenario = acme();
    scenario
        .given("~/Projects", "mindweaver gather git ../src/acme-skills")
        .drop_from(SOURCE, &["skills/ddd-reviewer"]);

    scenario
        .run(
            "~/Projects",
            "mindweaver gather git ../src/acme-skills --dry-run",
        )
        .succeeds()
        .prints(&format!(
            "src-acme-skills: ../src/acme-skills at {}\n\
             \x20 would remove  ddd-reviewer  skills/ddd-reviewer  Review a change against the \
             DDD layering rules\n\
             \x20 unchanged     commit-style  skills/commit-style  How this repo writes commit \
             messages\n\
             \n\
             1 skill: 0 to add, 0 to update, 1 unchanged, 1 to remove\n\
             nothing gathered: run it without --dry-run to gather\n",
            scenario.head(SOURCE)
        ))
        .quiet();
    assert!(scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills/ddd-reviewer"));
}

#[test]
fn gather_remove_without_yes_says_what_it_would_take_and_leave_orphaned() {
    let scenario = installed_from_acme();

    scenario
        .run("~/Projects", "mindweaver gather remove src-acme-skills")
        .succeeds()
        .prints(concat!(
            "would forget the source src-acme-skills, gathered from ../src/acme-skills\n",
            "would remove its 2 skills from the shelf, at \
             /home/you/Projects/.mindweaver/skills/..-src-acme-skills\n",
            "would remove its clone, at /home/you/Projects/.mindweaver/cache/..-src-acme-skills\n",
            "would leave commit-style installed in collapse, orphaned\n",
            "nothing removed: add --yes to go ahead\n",
        ))
        .quiet();

    assert!(scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills/commit-style"));
    assert!(scenario.exists("~/Projects/.mindweaver/cache/..-src-acme-skills"));
    scenario
        .run("~/Projects", "mindweaver status")
        .succeeds()
        .prints("commit-style  current  claude,codex,opencode  ../src/acme-skills\n");
}

#[test]
fn gather_remove_yes_forgets_the_source_and_leaves_what_was_installed_orphaned() {
    let scenario = installed_from_acme();

    scenario
        .run(
            "~/Projects",
            "mindweaver gather remove src-acme-skills --yes",
        )
        .succeeds()
        .prints(concat!(
            "forgot the source src-acme-skills, gathered from ../src/acme-skills\n",
            "removed its 2 skills from the shelf, at \
             /home/you/Projects/.mindweaver/skills/..-src-acme-skills\n",
            "removed its clone, at /home/you/Projects/.mindweaver/cache/..-src-acme-skills\n",
            "left commit-style installed in collapse, orphaned\n",
        ))
        .says(
            "note: nothing offers what was installed from it any more; `mindweaver uninstall \
             --orphaned --all-projects` takes out what nothing offers\n",
        );

    assert!(!scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills"));
    assert!(!scenario.exists("~/Projects/.mindweaver/cache/..-src-acme-skills"));
    assert!(scenario.exists("~/Projects/collapse/.claude/skills/commit-style/SKILL.md"));
    scenario
        .run("~/Projects", "mindweaver status")
        .succeeds()
        .prints("commit-style  orphaned  claude,codex,opencode  ../src/acme-skills\n");
    scenario
        .run("~/Projects", "mindweaver gather sources")
        .succeeds()
        .prints("no sources yet\n  take some with `mindweaver gather git <url>`\n");
}

#[test]
fn gather_remove_of_a_name_no_source_has_removes_nothing() {
    let scenario = installed_from_acme();

    scenario
        .run("~/Projects", "mindweaver gather remove acme --yes")
        .exits(1)
        .prints("")
        .says(
            "error: no source is called `acme`: `mindweaver gather sources` lists them by name\n",
        );

    assert!(scenario.exists("~/Projects/.mindweaver/skills/..-src-acme-skills/commit-style"));
}

#[test]
fn a_token_from_a_variable_and_from_gh_at_once_is_the_grammar_s_to_refuse() {
    let scenario = Scenario::projects(&["collapse"]);

    let ran = scenario.run(
        "~/Projects",
        "mindweaver gather git acme/private-skills --token-env GITHUB_TOKEN --gh",
    );

    ran.exits(2).prints("");
    assert!(
        ran.stderr
            .starts_with("error: the argument '--token-env <VARIABLE>' cannot be used with '--gh'"),
        "{ran}"
    );
}
