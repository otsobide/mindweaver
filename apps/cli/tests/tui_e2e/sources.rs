//! What a workspace takes from elsewhere, from the screen: a repository
//! gathered onto its shelf, watched while it runs, and another workspace
//! loaded, then forgotten.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::{collapse_and_tanukeys, skill, Key, Scenario};

/// `~/Projects` as [`collapse_and_tanukeys`] has it, and `~/src/team`, a
/// workspace of its own offering `review`.
fn beside_team() -> Scenario {
    let scenario = collapse_and_tanukeys();
    scenario
        .folder("~/src/team")
        .given("~/src/team", "mindweaver init")
        .given(
            "~/src/team",
            "mindweaver add skill review \"Read a change\"",
        );
    scenario
}

#[test]
fn g_gathers_from_a_repository_and_the_header_counts_what_it_put_on_the_shelf() {
    let scenario = collapse_and_tanukeys();
    scenario.source(
        "~/src/acme-skills",
        &[("skills/lint/SKILL.md", &skill("lint", "Lint the code"))],
    );
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("nothing on the shelf");

    terminal
        .types("g")
        .shows("Gather skills from a git repository")
        .types("../src/acme-skills")
        .shows("mindweaver gather git ../src/acme-skills")
        .presses(Key::Enter)
        .shows("lint")
        .shows("1 on the shelf");
    assert_eq!(
        scenario
            .files("~/Projects/.mindweaver/skills")
            .iter()
            .filter(|file| file.ends_with("/lint/SKILL.md"))
            .count(),
        1
    );

    terminal.presses(Key::Escape).types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        format!(
            "$ mindweaver gather git ../src/acme-skills\n\
             src-acme-skills: ../src/acme-skills at {}\n\
             \x20 added  lint  Lint the code\n\
             \n\
             1 skill: 1 added, 0 updated, 0 unchanged",
            scenario.head("~/src/acme-skills")
        )
    );
    scenario
        .run("~/Projects", "mindweaver install -p tanukeys lint")
        .succeeds();
}

#[test]
fn capital_l_loads_another_workspace_and_its_own_are_offered_from_then_on() {
    let scenario = beside_team();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("collapse  2");

    terminal
        .types("L")
        .shows("Load")
        .types("../src/team")
        .shows("mindweaver load ../src/team")
        .presses(Key::Enter)
        .shows("1 loaded");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver load ../src/team\n",
            "loaded team as ../src/team: 1 skill to install\n",
            "  into the workspace at /home/you/Projects",
        )
    );
    scenario
        .run("~/Projects", "mindweaver install -p tanukeys review")
        .succeeds()
        .prints("installed review into tanukeys, from load:../src/team\n");
}

#[test]
fn enter_on_a_loaded_workspace_in_the_load_form_unloads_it_once_asked() {
    let scenario = beside_team();
    scenario.given("~/Projects", "mindweaver load ../src/team");
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("1 loaded");

    terminal
        .types("L")
        .shows("team · 1 to install")
        .presses(Key::Enter)
        .shows("Unload");
    terminal.types("y").no_longer_shows("1 loaded");

    terminal.types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        concat!(
            "$ mindweaver unload ../src/team\n",
            "unloaded ../src/team\n",
            "  from the workspace at /home/you/Projects",
        )
    );
    scenario
        .run("~/Projects", "mindweaver load")
        .succeeds()
        .prints(concat!(
            "nothing loaded\n",
            "  offer another workspace's own with `mindweaver load <path>`\n",
            "  in the workspace at /home/you/Projects\n",
        ));
}

/// A git server on loopback that holds every request it reads until it is
/// told to answer, then answers that there is no such repository: a gather
/// that waits on it for as long as a test needs, as one waits on a slow
/// network, and stops the moment the server answers.
struct Holding {
    address: SocketAddr,
    /// Told once for every request read, so a test knows the gather is
    /// waiting on the answer rather than still on its way.
    read: Receiver<()>,
    answer: Arc<(Mutex<bool>, Condvar)>,
}

impl Holding {
    fn listening() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let address = listener.local_addr().expect("where it listens");
        let (told, read) = mpsc::channel();
        let answer = Arc::new((Mutex::new(false), Condvar::new()));
        let answering = Arc::clone(&answer);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let told = told.clone();
                let answering = Arc::clone(&answering);
                std::thread::spawn(move || hold(stream, &told, &answering));
            }
        });
        Self {
            address,
            read,
            answer,
        }
    }

    fn url(&self) -> String {
        format!("http://{}/acme/skills.git", self.address)
    }

    /// Wait until the gather has asked, and is waiting on the answer.
    fn asked(&self) {
        self.read
            .recv_timeout(Duration::from_secs(20))
            .expect("the gather asks the server within 20s");
    }

    /// Answer what is held, and anything asked from now on.
    fn answers(&self) {
        let (answered, waiting) = &*self.answer;
        *answered.lock().unwrap() = true;
        waiting.notify_all();
    }
}

impl Drop for Holding {
    fn drop(&mut self) {
        self.answers();
    }
}

/// Read one request on `stream`, say so, and answer it once told to.
fn hold(mut stream: TcpStream, told: &Sender<()>, answer: &(Mutex<bool>, Condvar)) {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    while !request.windows(4).any(|end| end == b"\r\n\r\n") {
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return,
            Ok(read) => request.extend_from_slice(&buffer[..read]),
        }
    }
    let _ = told.send(());
    let (answered, waiting) = answer;
    let mut answered = answered.lock().unwrap();
    while !*answered {
        answered = waiting.wait(answered).unwrap();
    }
    let _ = stream
        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
}

#[test]
fn esc_while_a_gather_waits_on_the_server_is_taken_once_it_answers_and_nothing_is_placed() {
    let scenario = collapse_and_tanukeys();
    let server = Holding::listening();
    let url = server.url();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("nothing on the shelf");

    terminal
        .types("g")
        .shows("Gather skills from a git repository")
        .types(&url)
        .presses(Key::Enter)
        .shows("esc cancels");
    server.asked();
    // Asked, and said to be asked, while the gather waits on the network,
    // which no cancel cuts short: it is taken once the server answers.
    terminal
        .presses(Key::Escape)
        .shows("cancelling, before anything is placed");
    server.answers();
    terminal
        .shows("cancelled, with nothing put on the shelf")
        .shows("Gather skills from a git repository");

    terminal
        .presses(Key::Escape)
        .shows("nothing on the shelf")
        .types("q");
    let closed = terminal.exits();
    assert_eq!(
        closed.shown.trim_end(),
        format!(
            "$ mindweaver gather git {url}\n\
             error: cancelled, with nothing put on the shelf"
        )
    );
    // Nothing placed, and the source left unregistered, as a cancel leaves
    // the shelf and the ledger.
    scenario
        .run("~/Projects", "mindweaver gather list")
        .succeeds()
        .prints(concat!(
            "nothing gathered yet\n",
            "  take some with `mindweaver gather git <url>`, or offer a workspace's own with \
             `mindweaver load <path>`\n",
        ));
    assert!(!scenario.exists("~/Projects/.mindweaver/skills"));
    // The folder clones go in was made before the gather connected; the
    // clone it stopped is gone from it.
    assert_eq!(
        scenario.files("~/Projects/.mindweaver/cache"),
        Vec::<String>::new()
    );
}

#[test]
fn ctrl_c_while_a_cancel_waits_on_the_server_closes_the_screen_with_the_gathers_line() {
    let scenario = collapse_and_tanukeys();
    let server = Holding::listening();
    let url = server.url();
    let mut terminal = scenario.open("~/Projects", "mindweaver");
    terminal.shows("nothing on the shelf");

    terminal
        .types("g")
        .shows("Gather skills from a git repository")
        .types(&url)
        .presses(Key::Enter)
        .shows("esc cancels");
    server.asked();
    // Cancelled first, and said to be: leaving draws nothing more, so a
    // Ctrl-C alone gives a test nothing to wait on before the server
    // answers, which it would then race. That the key cancels by itself is
    // the screen's own suite's (`leaving_while_a_gather_runs_cancels_it_and_waits_for_it`).
    terminal
        .presses(Key::Escape)
        .shows("cancelling, before anything is placed")
        .presses(Key::CtrlC);
    // Left with the gather stopped, not abandoned: its line reaches the
    // transcript once the server lets it go.
    server.answers();
    let closed = terminal.exits();

    assert!(closed.status.success());
    assert_eq!(
        closed.shown.trim_end(),
        format!(
            "$ mindweaver gather git {url}\n\
             error: cancelled, with nothing put on the shelf"
        )
    );
}
