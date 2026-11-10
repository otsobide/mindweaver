//! Searching the skill directories, and taking what one found: the
//! reference's "Searching the skill directories".
//!
//! No service is asked. Each finder a use case enables is a server on
//! loopback that answers as the service shaped its answer to a search,
//! asked through `--finder-url`, which takes plain http to this machine
//! alone; one the workspace does not list is pointed at it too, so a
//! request that should never be sent would be seen. D-15 has the owner read
//! skills.sh's and claude-plugins.dev's terms before either is asked.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::{Arc, Mutex};

use crate::{skill, Scenario};

/// Where below a finder's address it answers a search.
const GITHUB: &str = "/search/code";
const SKILLS_SH: &str = "/api/search";
const CLAUDE_PLUGINS: &str = "/api/skills/search";

/// What skills.sh answered a search for `pdf`, as it shapes one.
const SKILLS_SH_ANSWER: &str = r#"{
  "query": "pdf",
  "searchType": "fuzzy",
  "skills": [
    {"id": "acme/skills/pdf", "source": "acme/skills", "skillId": "pdf", "name": "pdf", "installs": 2263},
    {"id": "beta/house-rules/commit-style", "source": "beta/house-rules", "skillId": "commit-style", "name": "commit-style", "installs": 56}
  ],
  "count": 2
}"#;

/// What api.claude-plugins.dev answered the same search, as it shapes one.
const CLAUDE_PLUGINS_ANSWER: &str = r#"{
  "skills": [
    {
      "id": "63d64579-47a6-4a5a-9457-ae16e08b7d1d",
      "name": "pdf",
      "namespace": "@acme/skills/pdf",
      "sourceUrl": "https://github.com/acme/skills/tree/main/skills/pdf",
      "author": "acme",
      "stars": 321,
      "installs": 12,
      "metadata": {"repoOwner": "acme", "repoName": "skills", "directoryPath": "skills/pdf"}
    },
    {
      "id": "4f1b4ee9-a8be-4bf4-810f-2fe94b1f55bb",
      "name": "slides",
      "namespace": "@beta/decks/slides",
      "sourceUrl": "https://github.com/beta/decks/tree/main/skills/slides",
      "author": "beta",
      "stars": 7,
      "installs": null,
      "metadata": {"repoOwner": "beta", "repoName": "decks", "directoryPath": "skills/slides"}
    }
  ],
  "total": 2,
  "limit": 20,
  "offset": 0
}"#;

/// What GitHub's code search answered, as its API documents one.
const GITHUB_ANSWER: &str = r#"{
  "total_count": 1,
  "incomplete_results": false,
  "items": [
    {
      "name": "SKILL.md",
      "path": "skills/review/SKILL.md",
      "repository": {"full_name": "acme/tools", "html_url": "https://github.com/acme/tools"}
    }
  ]
}"#;

/// One answer a directory gives: the status, the headers and the body.
#[derive(Clone)]
struct Answer {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Answer {
    fn json(body: &str) -> Self {
        Answer {
            status: 200,
            headers: vec![(
                String::from("content-type"),
                String::from("application/json"),
            )],
            body: body.to_owned(),
        }
    }
}

/// A request a directory was sent: its target, and the login it carried.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Sent {
    target: String,
    authorization: Option<String>,
}

/// The skill directories on loopback, one server answering each finder's
/// search at the path that finder asks, as each service would.
struct Directories {
    address: SocketAddr,
    sent: Arc<Mutex<Vec<Sent>>>,
}

impl Directories {
    fn answering(answers: &[(&str, Answer)]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().expect("where it listens");
        let sent = Arc::<Mutex<Vec<Sent>>>::default();
        let noted = Arc::clone(&sent);
        let answers: Vec<(String, Answer)> = answers
            .iter()
            .map(|(path, answer)| ((*path).to_owned(), answer.clone()))
            .collect();
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
                let target = request
                    .lines()
                    .next()
                    .and_then(|line| line.split(' ').nth(1))
                    .unwrap_or_default()
                    .to_owned();
                let authorization = request.lines().find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("authorization")
                        .then(|| value.trim().to_owned())
                });
                noted.lock().unwrap().push(Sent {
                    target: target.clone(),
                    authorization,
                });
                let path = target.split('?').next().unwrap_or_default();
                let answer = answers
                    .iter()
                    .find(|(at, _)| at == path)
                    .map(|(_, answer)| answer.clone())
                    .unwrap_or(Answer {
                        status: 404,
                        headers: Vec::new(),
                        body: String::new(),
                    });
                let mut head = format!("HTTP/1.1 {} Answered\r\n", answer.status);
                for (name, value) in &answer.headers {
                    head.push_str(&format!("{name}: {value}\r\n"));
                }
                head.push_str(&format!(
                    "content-length: {}\r\nconnection: close\r\n\r\n",
                    answer.body.len()
                ));
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(answer.body.as_bytes());
            }
        });
        Directories { address, sent }
    }

    /// Every finder asked here rather than at its own address: what a
    /// line typed in a use case says first.
    fn here(&self) -> String {
        ["github", "skills.sh", "claude-plugins.dev"]
            .iter()
            .map(|finder| format!("--finder-url {finder}=http://{} ", self.address))
            .collect()
    }

    /// Each request sent, by its path, in order.
    fn asked(&self) -> Vec<String> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|sent| sent.target.split('?').next().unwrap_or_default().to_owned())
            .collect()
    }

    fn sent(&self) -> Vec<Sent> {
        self.sent.lock().unwrap().clone()
    }
}

/// `~/Projects`, its `weaver.toml` listing `finders`, as a person writes the
/// key in by hand, having read each unstable one's terms.
fn listing(finders: &[&str]) -> Scenario {
    let scenario = Scenario::projects(&["collapse"]);
    let marker = "~/Projects/.mindweaver/weaver.toml";
    let listed: Vec<String> = finders.iter().map(|name| format!("\"{name}\"")).collect();
    let text = scenario.read(marker).replacen(
        "loaded = []\n",
        &format!("loaded = []\nfinders = [{}]\n", listed.join(", ")),
        1,
    );
    scenario.file(marker, &text);
    scenario
}

/// The two unstable directories answering a search for `pdf`.
fn both_answering() -> Directories {
    Directories::answering(&[
        (SKILLS_SH, Answer::json(SKILLS_SH_ANSWER)),
        (CLAUDE_PLUGINS, Answer::json(CLAUDE_PLUGINS_ANSWER)),
    ])
}

const SKIPPED: &str =
    "hint: github was not asked, since its search needs a token: --token-env or --gh hands one over\n";

#[test]
fn a_search_lists_what_each_enabled_finder_found_each_skill_once() {
    let scenario = listing(&["github", "skills.sh", "claude-plugins.dev"]);
    let directories = both_answering();

    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather search {}pdf", directories.here()),
        )
        .succeeds()
        .prints(
            "1  pdf           https://github.com/acme/skills#main  skills/pdf     2263 installs  \
             skills.sh, claude-plugins.dev\n\
             2  commit-style  https://github.com/beta/house-rules  .              56 installs    \
             skills.sh\n\
             3  slides        https://github.com/beta/decks#main   skills/slides  7 stars        \
             claude-plugins.dev\n",
        )
        .says(SKIPPED);

    assert_eq!(directories.asked(), [SKILLS_SH, CLAUDE_PLUGINS]);
}

/// D-15's default: no `finders`, GitHub alone, and with no token nothing
/// at all is sent anywhere.
#[test]
fn a_search_with_no_finder_listed_and_no_token_asks_nothing() {
    let scenario = Scenario::projects(&["collapse"]);
    let directories = both_answering();

    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather search {}pdf", directories.here()),
        )
        .exits(1)
        .prints("")
        .says(
            "error: nothing was searched: github needs a token, handed over with --token-env or \
             --gh, and no finder that needs none is enabled\n",
        );

    assert!(directories.asked().is_empty(), "{:?}", directories.sent());
}

/// A finder the marker does not list is never sent anything, though it
/// answers: here, only claude-plugins.dev is.
#[test]
fn a_finder_the_workspace_does_not_list_is_never_asked() {
    let scenario = listing(&["claude-plugins.dev"]);
    let directories = both_answering();

    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather search {}pdf", directories.here()),
        )
        .succeeds()
        .prints(
            "1  pdf     https://github.com/acme/skills#main  skills/pdf     12 installs  \
             claude-plugins.dev\n\
             2  slides  https://github.com/beta/decks#main   skills/slides  7 stars      \
             claude-plugins.dev\n",
        )
        .quiet();

    assert_eq!(directories.asked(), [CLAUDE_PLUGINS]);
}

#[test]
fn naming_a_finder_the_workspace_does_not_list_is_refused() {
    let scenario = listing(&["claude-plugins.dev"]);
    let directories = both_answering();

    scenario
        .run(
            "~/Projects",
            &format!(
                "mindweaver gather search --finder skills.sh {}pdf",
                directories.here()
            ),
        )
        .exits(1)
        .says(
            "error: skills.sh is not enabled, so it was not asked: list it in `finders` in \
             /home/you/Projects/.mindweaver/weaver.toml to search it, having read its terms: its \
             API is unstable and undocumented\n",
        );

    assert!(directories.asked().is_empty());
}

/// A token is read for the one search, sent to GitHub's API alone, as a
/// bearer token, and printed nowhere.
#[test]
fn a_token_handed_over_searches_github_and_goes_nowhere_else() {
    let scenario = listing(&["github", "skills.sh"]);
    let directories = Directories::answering(&[
        (GITHUB, Answer::json(GITHUB_ANSWER)),
        (SKILLS_SH, Answer::json(SKILLS_SH_ANSWER)),
    ]);
    let token = format!("ghp_{}", mindweaver_core::mother::random::hex(36));

    let ran = scenario.run_with(
        "~/Projects",
        &format!(
            "mindweaver gather search --token-env GITHUB_TOKEN {}pdf",
            directories.here()
        ),
        &[("GITHUB_TOKEN", &token)],
    );

    ran.succeeds()
        .prints(
            "1  review        https://github.com/acme/tools        skills/review                 github\n\
             2  pdf           https://github.com/acme/skills       .              2263 installs  skills.sh\n\
             3  commit-style  https://github.com/beta/house-rules  .              56 installs    skills.sh\n",
        )
        .quiet();
    assert_eq!(
        directories.sent(),
        [
            Sent {
                target: String::from("/search/code?q=pdf%20filename%3ASKILL.md&per_page=20"),
                authorization: Some(format!("Bearer {token}")),
            },
            Sent {
                target: String::from("/api/search?q=pdf&limit=20"),
                authorization: None,
            },
        ]
    );
    assert!(!scenario
        .files("~/Projects/.mindweaver/cache/http")
        .iter()
        .any(|file| scenario
            .read(&format!("~/Projects/.mindweaver/cache/http/{file}"))
            .contains(&token)));
}

/// The acceptance's case on a command line: a directory whose answer
/// changed shape is a warning with its code, and the others' results are
/// listed all the same.
#[test]
fn a_finder_whose_answer_changed_shape_is_a_warning_beside_the_others() {
    let scenario = listing(&["skills.sh", "claude-plugins.dev"]);
    let directories = Directories::answering(&[
        (
            SKILLS_SH,
            Answer::json(r#"{"results": [{"slug": "acme/skills/pdf", "weeklyInstalls": 9}]}"#),
        ),
        (CLAUDE_PLUGINS, Answer::json(CLAUDE_PLUGINS_ANSWER)),
    ]);

    scenario
        .run(
            "~/Projects",
            &format!("mindweaver gather search {}pdf", directories.here()),
        )
        .exits(1)
        .prints(
            "1  pdf     https://github.com/acme/skills#main  skills/pdf     12 installs  \
             claude-plugins.dev\n\
             2  slides  https://github.com/beta/decks#main   skills/slides  7 stars      \
             claude-plugins.dev\n",
        )
        .says(
            "warning: skills.sh: its answer is not in the shape this build reads, so nothing it \
             found is listed: missing field `skills`\n",
        );
}

/// GitHub answers a search with a `403` once none is left: a warning by its
/// code, saying when it lets a request through again.
#[test]
fn a_rate_limited_finder_says_when_it_lets_a_search_through_again() {
    let scenario = listing(&["github", "skills.sh"]);
    let directories = Directories::answering(&[
        (
            GITHUB,
            Answer {
                status: 403,
                headers: vec![
                    (String::from("x-ratelimit-remaining"), String::from("0")),
                    (
                        String::from("x-ratelimit-reset"),
                        String::from("1800000000"),
                    ),
                ],
                body: String::from(r#"{"message": "API rate limit exceeded"}"#),
            },
        ),
        (SKILLS_SH, Answer::json(SKILLS_SH_ANSWER)),
    ]);

    let ran = scenario.run_with(
        "~/Projects",
        &format!(
            "mindweaver gather search --token-env GITHUB_TOKEN {}pdf",
            directories.here()
        ),
        &[("GITHUB_TOKEN", "ghp_test")],
    );

    ran.exits(1)
        .prints(
            "1  pdf           https://github.com/acme/skills       .  2263 installs  skills.sh\n\
             2  commit-style  https://github.com/beta/house-rules  .  56 installs    skills.sh\n",
        )
        .says(&format!(
            "warning: github: http://{}/search/code?q=pdf%20filename%3ASKILL.md&per_page=20: \
             rate limited (status 403), and lets a request through again at \
             2027-01-15T08:00:00Z\n",
            directories.address
        ));
}

/// `--take` gathers the result listed by the number, as `gather git` would,
/// saying first the line it amounts to. A finder on this machine may name a
/// folder on it, which is what a use case can gather with no network.
#[test]
fn taking_a_result_gathers_it_as_gather_git_would_saying_the_line_first() {
    let scenario = listing(&["skills.sh"]);
    scenario.source(
        "~/src/house-rules",
        &[(
            "skills/commit-style/SKILL.md",
            &skill("commit-style", "How this repo writes commit messages"),
        )],
    );
    let folder = scenario.at("~/src/house-rules");
    let answer = format!(
        r#"{{"skills": [{{"name": "commit-style", "source": "{}", "installs": 56}}]}}"#,
        folder.display()
    );
    let directories = Directories::answering(&[(SKILLS_SH, Answer::json(&answer))]);

    scenario
        .run(
            "~/Projects",
            &format!(
                "mindweaver gather search --take 1 {}commit",
                directories.here()
            ),
        )
        .succeeds()
        .prints(&format!(
            "taking result 1, commit-style: mindweaver gather git /home/you/src/house-rules\n\
             src-house-rules: /home/you/src/house-rules at {}\n\
             \x20 added  commit-style  How this repo writes commit messages\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged\n",
            scenario.head("~/src/house-rules")
        ))
        .quiet();

    assert!(scenario.exists("~/Projects/.mindweaver/skills"));
}

/// A token handed over for the search is GitHub's: a result elsewhere,
/// here a folder on this machine, is gathered without it, which a gather of
/// a folder given one would refuse.
#[test]
fn a_token_for_the_search_goes_on_to_no_gather_elsewhere_than_github() {
    let scenario = listing(&["github", "skills.sh"]);
    scenario.source(
        "~/src/house-rules",
        &[(
            "skills/commit-style/SKILL.md",
            &skill("commit-style", "How this repo writes commit messages"),
        )],
    );
    let answer = format!(
        r#"{{"skills": [{{"name": "commit-style", "source": "{}"}}]}}"#,
        scenario.at("~/src/house-rules").display()
    );
    let directories = Directories::answering(&[
        (GITHUB, Answer::json(r#"{"items": []}"#)),
        (SKILLS_SH, Answer::json(&answer)),
    ]);

    let ran = scenario.run_with(
        "~/Projects",
        &format!(
            "mindweaver gather search --token-env GITHUB_TOKEN --take 1 {}commit",
            directories.here()
        ),
        &[("GITHUB_TOKEN", "ghp_test")],
    );

    ran.succeeds().quiet();
    assert!(
        ran.stdout.starts_with(
            "taking result 1, commit-style: mindweaver gather git /home/you/src/house-rules\n"
        ),
        "{}",
        ran.stdout
    );
}

#[test]
fn taking_a_result_the_search_did_not_list_gathers_nothing() {
    let scenario = listing(&["skills.sh", "claude-plugins.dev"]);
    let directories = both_answering();

    scenario
        .run(
            "~/Projects",
            &format!(
                "mindweaver gather search --take 4 {}pdf",
                directories.here()
            ),
        )
        .exits(1)
        .prints("")
        .says("error: there is no result 4 to take: the search found 3\n");

    assert!(!scenario.exists("~/Projects/.mindweaver/skills"));
}

/// `--json` prints what was found in the envelope every reading command's
/// does, what the line says on stderr in its warnings too.
#[test]
fn a_search_as_json_lists_each_result_by_the_names_a_script_reads() {
    let scenario = listing(&["github", "claude-plugins.dev"]);
    let directories = both_answering();

    let ran = scenario.run(
        "~/Projects",
        &format!(
            "mindweaver gather search --json {}slides",
            directories.here()
        ),
    );

    ran.succeeds().says(SKIPPED);
    let printed: serde_json::Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        printed,
        serde_json::json!({
            "version": 2,
            "data": {
                "hits": [
                    {
                        "name": "pdf",
                        "repository": "https://github.com/acme/skills",
                        "path": "skills/pdf",
                        "reference": "main",
                        "installs": 12,
                        "stars": 321,
                        "finders": ["claude-plugins.dev"],
                    },
                    {
                        "name": "slides",
                        "repository": "https://github.com/beta/decks",
                        "path": "skills/slides",
                        "reference": "main",
                        "installs": null,
                        "stars": 7,
                        "finders": ["claude-plugins.dev"],
                    },
                ],
                "asked": ["claude-plugins.dev"],
                "skipped": ["github"],
                "failures": [],
            },
            "warnings": [],
            "coded_warnings": [],
        })
    );
}
