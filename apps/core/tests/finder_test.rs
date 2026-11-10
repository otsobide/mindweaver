//! Searching the skill directories (#108): which finders a workspace asks,
//! what each is sent, and what is made of what each answers, through core's
//! client over a fake that keeps the finders' answers in memory, shaped as
//! each service answered a search on 2026-11-10 (`fixtures/finders`).
//!
//! Nothing reaches the network, nor any of the three services: the fake is
//! handed every request, and answers what a test put there or that nobody
//! is there. D-15 has the owner read skills.sh's and claude-plugins.dev's
//! terms before either is asked; no test or run here asks either.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use mindweaver_core::finder::{
    self, Failure, Finder, Found, Progress, Search, SearchError, Watch, FRESH_FOR, PER_FINDER,
};
use mindweaver_core::http::{Cache, Client};
use mindweaver_core::mother::{random, UrlMother, WorkspaceMother};
use mindweaver_core::port::{Http, HttpError, HttpRequest, HttpResponse, Token};
use mindweaver_core::value::Redacted;
use mindweaver_core::WeaverWorkspace;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// The finders, kept in memory
// ---------------------------------------------------------------------------

/// A request as a finder received it.
#[derive(Debug, Clone)]
struct Asked {
    url: String,
    /// Each header, its name in lowercase, and whether it was sent as a
    /// secret.
    headers: Vec<(String, String, bool)>,
}

impl Asked {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(held, _, _)| held == name)
            .map(|(_, value, _)| value.as_str())
    }
}

/// What a path is answered with: the status, the headers and the body.
type Answer = (u16, Vec<(String, String)>, Vec<u8>);

/// Every finder's site: what each path is answered with, by its origin and
/// its path, the query left off, and what was asked, in order.
#[derive(Debug, Default)]
struct Finders {
    answers: Mutex<BTreeMap<String, Answer>>,
    asked: Mutex<Vec<Asked>>,
}

impl Finders {
    fn new() -> Arc<Self> {
        Arc::new(Finders::default())
    }

    /// `finder`, at its own address, answering a search with `body`.
    fn answer(&self, finder: Finder, body: &str) {
        self.answer_at(finder.address(), finder, 200, &[], body);
    }

    /// `finder`, at `base`, answering a search with `status`, `headers` and
    /// `body`.
    fn answer_at(
        &self,
        base: &str,
        finder: Finder,
        status: u16,
        headers: &[(&str, &str)],
        body: &str,
    ) {
        let headers = headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        self.answers.lock().unwrap().insert(
            format!("{base}/{}", path_of(finder)),
            (status, headers, body.as_bytes().to_vec()),
        );
    }

    fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    /// What was asked at `base`.
    fn asked_at(&self, base: &str) -> Vec<Asked> {
        self.asked()
            .into_iter()
            .filter(|asked| asked.url.starts_with(&format!("{base}/")))
            .collect()
    }
}

impl Http for Finders {
    fn get(&self, request: &HttpRequest) -> Result<HttpResponse, HttpError> {
        let url = request.url().to_string();
        self.asked.lock().unwrap().push(Asked {
            url: url.clone(),
            headers: request
                .headers()
                .iter()
                .map(|header| {
                    (
                        header.name().to_owned(),
                        header.value().to_owned(),
                        header.is_secret(),
                    )
                })
                .collect(),
        });
        let path = url.split(['?', '#']).next().unwrap_or(&url);
        match self.answers.lock().unwrap().get(path) {
            Some((status, headers, body)) => Ok(HttpResponse::new(
                *status,
                headers
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
                body.clone(),
            )),
            None => Err(HttpError::Connect {
                url: request.url().clone(),
                detail: Redacted::new("nothing answers there"),
            }),
        }
    }
}

/// Where below its address a finder answers a search.
fn path_of(finder: Finder) -> &'static str {
    match finder {
        Finder::Github => "search/code",
        Finder::SkillsSh => "api/search",
        Finder::ClaudePlugins => "api/skills/search",
    }
}

/// A fixture: a finder's answer as the service shaped one.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/finders")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Each finder answering as its fixture does.
fn every_finder_answering() -> Arc<Finders> {
    let finders = Finders::new();
    finders.answer(Finder::Github, &fixture("github.json"));
    finders.answer(Finder::SkillsSh, &fixture("skills-sh.json"));
    finders.answer(Finder::ClaudePlugins, &fixture("claude-plugins.json"));
    finders
}

// ---------------------------------------------------------------------------
// A workspace listing its finders
// ---------------------------------------------------------------------------

/// A workspace whose marker lists `names` in `finders`, as a person writes
/// them in, by hand.
fn listing(names: &[&str]) -> (TempDir, WeaverWorkspace) {
    let (folder, workspace) = WorkspaceMother::empty();
    let marker = workspace.config_path();
    let text = fs::read_to_string(&marker).unwrap();
    let listed: Vec<String> = names.iter().map(|name| format!("\"{name}\"")).collect();
    let text = text.replacen(
        "loaded = []\n",
        &format!("loaded = []\nfinders = [{}]\n", listed.join(", ")),
        1,
    );
    fs::write(&marker, text).unwrap();
    let workspace = WeaverWorkspace::open(folder.path()).unwrap();
    (folder, workspace)
}

/// The finders `workspace` enables, asked of `finders` with `token`, for
/// `query`.
fn search(
    workspace: &WeaverWorkspace,
    finders: &Arc<Finders>,
    token: Option<&str>,
    query: &str,
) -> Result<Found, SearchError> {
    let enabled = finder::enabled(workspace).asked(&[])?;
    Search::new(Client::through(Arc::clone(finders) as Arc<dyn Http>))
        .with_token(token.map(Token::new))
        .run(query, &enabled)
}

/// Each result as a row: its name, repository, folder and branch, its
/// counts and the finders that found it.
fn rows(found: &Found) -> Vec<String> {
    found
        .hits
        .iter()
        .map(|hit| {
            let finders: Vec<&str> = hit.finders.iter().map(|finder| finder.name()).collect();
            format!(
                "{} {} {} {} {:?} {:?} {}",
                hit.name,
                hit.repository,
                hit.folder(),
                hit.reference.as_deref().unwrap_or("-"),
                hit.installs,
                hit.stars,
                finders.join(",")
            )
        })
        .collect()
}

fn said(failures: &[Failure]) -> Vec<(&'static str, String)> {
    failures
        .iter()
        .map(|failure| (failure.code(), failure.to_string()))
        .collect()
}

// ---------------------------------------------------------------------------
// What each finder answers
// ---------------------------------------------------------------------------

#[test]
fn each_finder_s_answer_is_listed_as_the_skills_it_names_each_once() {
    let (_folder, workspace) = listing(&["github", "skills.sh", "claude-plugins.dev"]);
    let finders = every_finder_answering();

    let found = search(&workspace, &finders, Some("ghp_test"), "pdf").unwrap();

    assert_eq!(
        rows(&found),
        [
            "pdf https://github.com/acme/skills skills/pdf - Some(2263) Some(321) \
             github,skills.sh,claude-plugins.dev",
            "pdf-tools https://github.com/acme/pdf-tools . - None None github",
            "commit-style https://github.com/beta/house-rules . - Some(56) None skills.sh",
            "slides https://github.com/beta/decks skills/slides - None Some(7) claude-plugins.dev",
        ]
    );
    assert_eq!(
        found.asked,
        [Finder::Github, Finder::SkillsSh, Finder::ClaudePlugins]
    );
    assert!(found.skipped.is_empty());
    assert_eq!(
        said(&found.failures),
        [
            (
                "finder_result_dropped",
                String::from(
                    "github: result 3 (`docs/skill.md`): it is the file `skill.md`, not a \
                     `SKILL.md`, so it is not listed"
                )
            ),
            (
                "finder_result_dropped",
                String::from(
                    "skills.sh: result 3 (`broken`): it names no repository, so it is not listed"
                )
            ),
            (
                "finder_result_dropped",
                String::from(
                    "claude-plugins.dev: result 3 (`escape`): its folder climbs out of the \
                     repository, so it is not listed"
                )
            ),
        ]
    );
}

/// A tree address names the branch it was found at, which a take gathers
/// it at.
#[test]
fn a_result_at_a_folder_s_address_keeps_the_branch_it_names() {
    let (_folder, workspace) = listing(&["claude-plugins.dev"]);
    let finders = Finders::new();
    finders.answer(Finder::ClaudePlugins, &fixture("claude-plugins.json"));

    let found = search(&workspace, &finders, None, "pdf").unwrap();

    assert_eq!(found.hits[0].reference.as_deref(), Some("main"));
    assert_eq!(found.hits[0].folder(), "skills/pdf");
    assert_eq!(
        found.hits[0].repository.as_str(),
        "https://github.com/acme/skills"
    );
}

/// The acceptance's first case: an undocumented API that changed is said,
/// and what the others found is listed all the same.
#[test]
fn a_finder_whose_answer_changed_shape_is_a_warning_beside_the_others_results() {
    let (_folder, workspace) = listing(&["skills.sh", "claude-plugins.dev"]);
    let finders = Finders::new();
    finders.answer(Finder::SkillsSh, &fixture("skills-sh-changed.json"));
    finders.answer(Finder::ClaudePlugins, &fixture("claude-plugins.json"));

    let found = search(&workspace, &finders, None, "pdf").unwrap();

    let names: Vec<&str> = found.hits.iter().map(|hit| hit.name.as_str()).collect();
    assert_eq!(names, ["pdf", "slides"]);
    assert!(found
        .hits
        .iter()
        .all(|hit| hit.finders == [Finder::ClaudePlugins]));
    let unreadable: Vec<_> = said(&found.failures)
        .into_iter()
        .filter(|(code, _)| *code == "finder_answer_unreadable")
        .collect();
    assert_eq!(
        unreadable,
        [(
            "finder_answer_unreadable",
            String::from(
                "skills.sh: its answer is not in the shape this build reads, so nothing it \
                 found is listed: missing field `skills`"
            )
        )]
    );
}

/// An answer that is not JSON at all is the client's to say, with its code.
#[test]
fn a_finder_answering_what_is_not_json_is_a_warning_with_its_code() {
    let (_folder, workspace) = listing(&["skills.sh", "claude-plugins.dev"]);
    let finders = Finders::new();
    finders.answer(Finder::SkillsSh, "<html>down for maintenance</html>");
    finders.answer(Finder::ClaudePlugins, &fixture("claude-plugins.json"));

    let found = search(&workspace, &finders, None, "pdf").unwrap();

    assert_eq!(found.hits.len(), 2);
    assert_eq!(found.failures[0].code(), "http_json_invalid");
    assert_eq!(found.failures[0].finder(), Finder::SkillsSh);
}

#[test]
fn a_finder_that_cannot_be_reached_is_a_warning_beside_the_others_results() {
    let (_folder, workspace) = listing(&["skills.sh", "claude-plugins.dev"]);
    let finders = Finders::new();
    finders.answer(Finder::ClaudePlugins, &fixture("claude-plugins.json"));

    let found = search(&workspace, &finders, None, "pdf").unwrap();

    assert_eq!(found.hits.len(), 2);
    assert_eq!(found.failures[0].code(), "http_connect_failed");
    assert!(
        found.failures[0]
            .to_string()
            .starts_with("skills.sh: https://skills.sh/api/search?q=pdf&limit=20: "),
        "{}",
        found.failures[0]
    );
}

/// GitHub answers a search with a `403` once none is left, saying when it
/// resets: a warning by its code, beside the rest.
#[test]
fn a_rate_limited_finder_is_a_warning_saying_when_it_resets() {
    let (_folder, workspace) = listing(&["github", "skills.sh"]);
    let finders = Finders::new();
    finders.answer_at(
        Finder::Github.address(),
        Finder::Github,
        403,
        &[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1800000000"),
        ],
        r#"{"message": "API rate limit exceeded"}"#,
    );
    finders.answer(Finder::SkillsSh, &fixture("skills-sh.json"));

    let found = search(&workspace, &finders, Some("ghp_test"), "pdf").unwrap();

    assert_eq!(found.hits.len(), 2);
    let failure = &found.failures[0];
    assert_eq!(failure.code(), "http_rate_limited");
    assert!(
        failure
            .to_string()
            .starts_with("github: https://api.github.com/search/code?"),
        "{failure}"
    );
    assert!(
        failure.to_string().ends_with(
            ": rate limited (status 403), and lets a request through again at \
             2027-01-15T08:00:00Z"
        ),
        "{failure}"
    );
}

// ---------------------------------------------------------------------------
// Which finders are asked
// ---------------------------------------------------------------------------

/// The acceptance's second case: what the marker does not list is never
/// sent anything, whichever finders answer and whatever token is given.
#[test]
fn a_finder_that_is_not_enabled_is_never_asked() {
    for listed in [
        vec!["claude-plugins.dev"],
        vec!["skills.sh"],
        vec!["github"],
        vec!["github", "skills.sh"],
    ] {
        let (_folder, workspace) = listing(&listed);
        let finders = every_finder_answering();

        search(&workspace, &finders, Some("ghp_test"), &random::word()).unwrap();

        for finder in Finder::ALL {
            let asked = finders.asked_at(finder.address());
            if listed.contains(&finder.name()) {
                assert_eq!(asked.len(), 1, "{finder} of {listed:?}");
            } else {
                assert!(asked.is_empty(), "{finder} of {listed:?}: {asked:?}");
            }
        }
    }
}

/// A marker naming no finders enables GitHub's search alone (D-15), which
/// asks nothing without a token: the two unstable directories are never
/// asked unless somebody lists them.
#[test]
fn a_marker_naming_no_finders_asks_github_alone_and_only_with_a_token() {
    let (_folder, workspace) = WorkspaceMother::empty();
    let enabled = finder::enabled(&workspace);
    assert_eq!(enabled.finders, [Finder::Github]);
    assert!(!enabled.listed);

    let finders = every_finder_answering();
    let refused = search(&workspace, &finders, None, "pdf").unwrap_err();
    assert_eq!(refused.code(), "finder_none_asked");
    assert!(finders.asked().is_empty(), "{:?}", finders.asked());

    let found = search(&workspace, &finders, Some("ghp_test"), "pdf").unwrap();
    assert_eq!(found.asked, [Finder::Github]);
    assert_eq!(finders.asked().len(), 1);
    assert_eq!(finders.asked_at(Finder::Github.address()).len(), 1);
}

#[test]
fn an_empty_list_enables_no_finder_and_asks_nothing() {
    let (_folder, workspace) = listing(&[]);
    let finders = every_finder_answering();

    let refused = search(&workspace, &finders, Some("ghp_test"), "pdf").unwrap_err();

    assert_eq!(refused.code(), "finder_none_enabled");
    assert!(finders.asked().is_empty());
}

#[test]
fn a_name_the_marker_lists_that_is_no_finder_is_said_and_passed_over() {
    let (_folder, workspace) = listing(&["skills.sh", "skillsmp", "skillsmp"]);

    let enabled = finder::enabled(&workspace);

    assert_eq!(enabled.finders, [Finder::SkillsSh]);
    assert_eq!(enabled.unknown.len(), 1);
    assert_eq!(enabled.unknown[0].code(), "finder_unknown");
    assert_eq!(
        enabled.unknown[0].to_string(),
        format!(
            "{}: `skillsmp` is among its finders, and this build knows none by that name, so it \
             is passed over",
            workspace.config_path().display()
        )
    );
}

/// GitHub's code search answers nothing without a token, so without one
/// it is skipped, and sent nothing at all.
#[test]
fn github_without_a_token_is_skipped_and_sent_nothing() {
    let (_folder, workspace) = listing(&["github", "skills.sh"]);
    let finders = every_finder_answering();

    let found = search(&workspace, &finders, None, "pdf").unwrap();

    assert_eq!(found.skipped, [Finder::Github]);
    assert_eq!(found.asked, [Finder::SkillsSh]);
    assert!(finders.asked_at(Finder::Github.address()).is_empty());
}

/// The token goes to GitHub alone, as a secret, which no other finder is
/// sent and nothing keeps; what the others answer is kept, what GitHub
/// answered a token is not.
#[test]
fn a_token_goes_to_github_alone_as_a_secret_and_is_never_kept() {
    let (_folder, workspace) = listing(&["github", "skills.sh", "claude-plugins.dev"]);
    let finders = every_finder_answering();
    let token = format!("ghp_{}", random::hex(36));
    let client = Client::through(Arc::clone(&finders) as Arc<dyn Http>)
        .caching(Cache::of(&workspace, FRESH_FOR));

    Search::new(client)
        .with_token(Some(Token::new(&token)))
        .run("pdf", &Finder::ALL)
        .unwrap();

    let github = finders.asked_at(Finder::Github.address());
    assert_eq!(github.len(), 1);
    let authorization = github[0]
        .headers
        .iter()
        .find(|(name, _, _)| name == "authorization")
        .expect("GitHub is sent the token");
    assert_eq!(authorization.1, format!("Bearer {token}"));
    assert!(authorization.2, "sent as a secret");
    for finder in [Finder::SkillsSh, Finder::ClaudePlugins] {
        for asked in finders.asked_at(finder.address()) {
            assert!(
                asked
                    .headers
                    .iter()
                    .all(|(_, value, _)| !value.contains(&token)),
                "{finder}: {asked:?}"
            );
        }
    }

    let kept: Vec<String> = fs::read_dir(workspace.http_cache_dir())
        .unwrap()
        .map(|entry| fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect();
    assert_eq!(kept.len(), 2, "the two unstable finders' answers alone");
    assert!(kept.iter().all(|text| !text.contains(&token)));
    assert!(kept.iter().all(|text| !text.contains("api.github.com")));
}

/// Read again within a few minutes, a search asks nothing: what a take by
/// a result's number finds where the listing showed it.
#[test]
fn a_search_made_again_soon_is_answered_from_the_cache() {
    let (_folder, workspace) = listing(&["skills.sh", "claude-plugins.dev"]);
    let finders = every_finder_answering();
    let client = Client::through(Arc::clone(&finders) as Arc<dyn Http>)
        .caching(Cache::of(&workspace, FRESH_FOR));
    let search = Search::new(client);
    let asked = [Finder::SkillsSh, Finder::ClaudePlugins];

    let first = search.run("pdf", &asked).unwrap();
    let second = search.run("pdf", &asked).unwrap();

    assert_eq!(rows(&first), rows(&second));
    assert_eq!(finders.asked().len(), 2);
}

// ---------------------------------------------------------------------------
// What is sent
// ---------------------------------------------------------------------------

#[test]
fn each_finder_is_asked_its_first_page_of_the_query() {
    let finders = every_finder_answering();
    let query = "pdf & forms";

    Search::new(Client::through(Arc::clone(&finders) as Arc<dyn Http>))
        .with_token(Some(Token::new("ghp_test")))
        .run(query, &Finder::ALL)
        .unwrap();

    let urls: Vec<String> = finders.asked().into_iter().map(|asked| asked.url).collect();
    assert_eq!(
        urls,
        [
            format!(
                "https://api.github.com/search/code?q=pdf%20%26%20forms%20filename%3ASKILL.md\
                 &per_page={PER_FINDER}"
            ),
            format!("https://skills.sh/api/search?q=pdf%20%26%20forms&limit={PER_FINDER}"),
            format!(
                "https://api.claude-plugins.dev/api/skills/search?q=pdf%20%26%20forms\
                 &limit={PER_FINDER}"
            ),
        ]
    );
    let github = &finders.asked()[0];
    assert_eq!(github.header("accept"), Some("application/vnd.github+json"));
    assert_eq!(github.header("x-github-api-version"), Some("2022-11-28"));
}

#[test]
fn an_empty_query_asks_nothing() {
    let finders = every_finder_answering();

    for query in ["", "   "] {
        let refused = Search::new(Client::through(Arc::clone(&finders) as Arc<dyn Http>))
            .run(query, &Finder::ALL)
            .unwrap_err();
        assert_eq!(refused.code(), "search_query_empty");
    }
    assert!(finders.asked().is_empty());
}

/// A GitHub Enterprise server's API is asked where it answers, and its
/// token is the host's: `gh` is asked for that one.
#[test]
fn a_finder_asked_at_another_address_is_asked_there_alone() {
    let base = format!("https://{}/api/v3", UrlMother::host());
    let finders = Finders::new();
    finders.answer_at(&base, Finder::Github, 200, &[], &fixture("github.json"));
    let search = Search::new(Client::through(Arc::clone(&finders) as Arc<dyn Http>))
        .at(Finder::Github, &format!("{base}/"))
        .unwrap()
        .with_token(Some(Token::new("ghp_test")));

    let found = search.run("pdf", &[Finder::Github]).unwrap();

    assert_eq!(found.hits.len(), 2);
    assert_eq!(finders.asked_at(&base).len(), 1);
    assert!(finders.asked_at(Finder::Github.address()).is_empty());
    let host = base
        .trim_start_matches("https://")
        .trim_end_matches("/api/v3");
    assert_eq!(search.token_host(), host);
}

#[test]
fn github_s_token_is_for_github_com_and_reaches_a_take_there_alone() {
    let search = Search::new(Client::through(Finders::new() as Arc<dyn Http>));
    assert_eq!(search.token_host(), "github.com");

    let found = |repository: &str| finder::Hit {
        name: random::kebab(),
        repository: Redacted::new(repository),
        path: None,
        reference: None,
        installs: None,
        stars: None,
        finders: vec![Finder::SkillsSh],
    };
    assert!(search.token_reaches(&found("https://github.com/acme/skills")));
    for elsewhere in [
        format!("https://{}/acme/skills", UrlMother::host()),
        String::from("https://gitlab.com/acme/skills"),
        String::from("/srv/skills"),
    ] {
        assert!(!search.token_reaches(&found(&elsewhere)), "{elsewhere}");
    }
}

#[test]
fn a_finder_is_asked_over_https_or_on_this_machine_alone() {
    let search = || Search::new(Client::through(Finders::new() as Arc<dyn Http>));
    let host = UrlMother::host();

    for fine in [
        format!("https://{host}"),
        format!("https://{host}/api/v3"),
        String::from("http://127.0.0.1:8080"),
        String::from("http://localhost:9/mirror"),
    ] {
        assert!(search().at(Finder::SkillsSh, &fine).is_ok(), "{fine}");
    }
    for (refused, why) in [
        (format!("http://{host}"), "it is not https"),
        (format!("https://{host}/x?y=1"), "it has a query"),
        (format!("ftp://{host}"), "not an http or https URL"),
        (
            format!("https://me:{}@{host}", UrlMother::secret()),
            "carries a credential",
        ),
    ] {
        let error = search().at(Finder::SkillsSh, &refused).unwrap_err();
        assert_eq!(error.code(), "finder_url_invalid", "{refused}");
        assert!(error.to_string().contains(why), "{refused}: {error}");
    }
}

/// A finder on this machine, a person's own or a test's, may name a folder
/// here; any other may not, which a gather would read whatever it holds.
#[test]
fn a_finder_on_this_machine_may_name_a_folder_on_it() {
    let local = "http://127.0.0.1:4567";
    let answer = r#"{"skills": [{"name": "deploy", "source": "/srv/skills", "installs": 1}]}"#;
    let finders = Finders::new();
    finders.answer_at(local, Finder::SkillsSh, 200, &[], answer);
    finders.answer(Finder::SkillsSh, answer);
    let client = || Client::through(Arc::clone(&finders) as Arc<dyn Http>);

    let here = Search::new(client())
        .at(Finder::SkillsSh, local)
        .unwrap()
        .run("deploy", &[Finder::SkillsSh])
        .unwrap();
    let elsewhere = Search::new(client())
        .run("deploy", &[Finder::SkillsSh])
        .unwrap();

    assert_eq!(here.hits[0].repository.as_str(), "/srv/skills");
    assert!(elsewhere.hits.is_empty());
    assert_eq!(
        elsewhere.failures[0].to_string(),
        "skills.sh: result 1 (`deploy`): it names a folder on this machine, which only a \
         finder on this machine may, so it is not listed"
    );
}

// ---------------------------------------------------------------------------
// Following a search, and stopping it
// ---------------------------------------------------------------------------

#[test]
fn a_search_reports_each_finder_it_asks() {
    let finders = every_finder_answering();
    let steps = Arc::new(Mutex::new(Vec::new()));
    let watch = Watch::new().reporting_to({
        let steps = Arc::clone(&steps);
        move |step| steps.lock().unwrap().push(step)
    });

    Search::new(Client::through(Arc::clone(&finders) as Arc<dyn Http>))
        .watched_by(watch)
        .run("pdf", &[Finder::SkillsSh, Finder::ClaudePlugins])
        .unwrap();

    assert_eq!(
        *steps.lock().unwrap(),
        [
            Progress::Asking {
                finder: Finder::SkillsSh,
                done: 0,
                total: 2
            },
            Progress::Asking {
                finder: Finder::ClaudePlugins,
                done: 1,
                total: 2
            },
        ]
    );
}

/// A cancel stops a search before the next finder is asked: nothing is
/// written, so nothing is left half done.
#[test]
fn a_cancelled_search_asks_no_further_finder() {
    let finders = every_finder_answering();
    let watch = Watch::new();
    let cancelling = watch.clone();
    let watch = watch.reporting_to(move |_| cancelling.cancel());

    let refused = Search::new(Client::through(Arc::clone(&finders) as Arc<dyn Http>))
        .watched_by(watch)
        .run("pdf", &[Finder::SkillsSh, Finder::ClaudePlugins])
        .unwrap_err();

    assert_eq!(refused.code(), "search_cancelled");
    assert_eq!(finders.asked().len(), 1, "{:?}", finders.asked());
    assert!(Progress::Asking {
        finder: Finder::SkillsSh,
        done: 0,
        total: 1
    }
    .cancellable());
}
