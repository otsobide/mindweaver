//! A terminal for the screen to run in, and what it shows.
//!
//! The TUI is the built binary, started from a folder of the scenario with
//! the environment every command there gets, in a pseudo-terminal of a size
//! of its own, as a shell in a terminal window starts it: the leader of a
//! session of its own, with that terminal as its controlling one, so
//! crossterm reads its size and its keys there, never the developer's
//! terminal, nor none at all under CI. What it draws is fed to a VT100
//! parser (`vt100`), and a test reads the screen as rows of text, paths in
//! them read as `/home`, as a person reads the window.
//!
//! The screen draws when it is ready and redraws when something changes, so
//! a test waits for what it expects to show (`Terminal::shows`), bounded,
//! and fails with the screen as it was when the wait ran out.

use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::os::unix::process::CommandExt;
use std::process::{Child, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use rustix::pty::{self, OpenptFlags};
use rustix::termios::{self, Winsize};

use crate::Scenario;

/// The window: wide enough for the reference's screen, as it is drawn there.
pub const ROWS: u16 = 24;
pub const COLUMNS: u16 = 120;

/// How long anything a test waits for has to show: long past what a draw
/// takes, for a machine under load, and still bounded.
const PATIENCE: Duration = Duration::from_secs(20);

/// A pseudo-terminal's side the test keeps, closed in every command it
/// starts: Linux opens it so, where macOS's `posix_openpt` takes no
/// `O_CLOEXEC`, and the flag is set as soon as it is open.
fn openpt() -> OwnedFd {
    #[cfg(target_os = "linux")]
    let master = pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC)
        .expect("open a pseudo-terminal");
    #[cfg(not(target_os = "linux"))]
    let master = {
        let master =
            pty::openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).expect("open a pseudo-terminal");
        rustix::io::fcntl_setfd(&master, rustix::io::FdFlags::CLOEXEC)
            .expect("close it in what is started");
        master
    };
    master
}

impl Scenario {
    /// `line`, typed in `at`, run in a terminal of its own.
    pub fn open(&self, at: &str, line: &str) -> Terminal<'_> {
        self.open_with(at, line, &[])
    }

    /// `line` run in a terminal of its own, with `env` beside the
    /// environment every command here gets: a variable the use case is
    /// about, such as the editor `e` hands the terminal to.
    pub fn open_with(&self, at: &str, line: &str, env: &[(&str, &str)]) -> Terminal<'_> {
        let master = openpt();
        pty::grantpt(&master).expect("grant it");
        pty::unlockpt(&master).expect("unlock it");
        let name = pty::ptsname(&master, Vec::new()).expect("name its other end");
        let slave: OwnedFd = File::options()
            .read(true)
            .write(true)
            .open(name.to_str().expect("a terminal's name is text"))
            .expect("open the terminal's other end")
            .into();
        // Sized through the screen's side: Linux takes it on either, and
        // macOS refuses it on the side the test keeps (ENOTTY).
        termios::tcsetwinsize(
            &slave,
            Winsize {
                ws_row: ROWS,
                ws_col: COLUMNS,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
        )
        .expect("size the window");

        let mut env = env.to_vec();
        env.push(("TERM", "xterm-256color"));
        let mut command = self.command(at, line, &env);
        command
            .stdin(Stdio::from(slave.try_clone().unwrap()))
            .stdout(Stdio::from(slave.try_clone().unwrap()))
            .stderr(Stdio::from(slave));
        // SAFETY: setsid and the ioctl are async-signal-safe, and touch no
        // memory of the parent's: what a shell does before it runs a job.
        unsafe {
            command.pre_exec(|| {
                rustix::process::setsid()?;
                rustix::process::ioctl_tiocsctty(std::io::stdin())?;
                Ok(())
            });
        }
        let child = command.spawn().expect("start the screen");
        // The command, and the copies of the terminal it held, go now: the
        // terminal's other end is the screen's alone, so a read here ends
        // once the screen has exited.
        drop(command);

        let master = File::from(master);
        let mut reading = master.try_clone().expect("a reader of the terminal");
        let mut answering = master.try_clone().expect("a writer of the answers");
        let (sent, received) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0; 8192];
            let mut asked = Asked::default();
            // An error once the screen has gone: EIO on Linux.
            while let Ok(read @ 1..) = reading.read(&mut buffer) {
                asked.answer(&buffer[..read], &mut answering);
                if sent.send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
        });
        Terminal {
            scenario: self,
            line: line.to_owned(),
            child,
            keyboard: master,
            drawn: received,
            parser: vt100::Parser::new(ROWS, COLUMNS, 0),
        }
    }
}

/// What a terminal says when it is asked where its cursor is (`ESC [ 6 n`,
/// a device status report), as every terminal a person uses answers it:
/// ratatui asks before it clears the screen, which the screen does when it
/// takes its window back from an editor, and waits two seconds for the
/// answer before it gives up drawing.
///
/// Answered here, on the thread that reads what the screen writes, rather
/// than where a test waits, so an answer never depends on whether a test is
/// waiting when the question is asked; with a parser of its own, fed
/// everything, so it says where the cursor is when it was asked.
struct Asked {
    tracking: vt100::Parser,
    /// The end of the last read, when it could be the start of a question
    /// the next read finishes.
    partial: Vec<u8>,
}

/// Where is the cursor?
const CURSOR_ASKED: &[u8] = b"\x1b[6n";

impl Default for Asked {
    fn default() -> Self {
        Self {
            tracking: vt100::Parser::new(ROWS, COLUMNS, 0),
            partial: Vec::new(),
        }
    }
}

impl Asked {
    /// Take in what the screen wrote, and answer each question in it.
    fn answer(&mut self, written: &[u8], terminal: &mut File) {
        let mut rest = std::mem::take(&mut self.partial);
        rest.extend_from_slice(written);
        let mut at = 0;
        while let Some(found) = rest[at..]
            .windows(CURSOR_ASKED.len())
            .position(|window| window == CURSOR_ASKED)
        {
            let end = at + found + CURSOR_ASKED.len();
            self.tracking.process(&rest[at..end]);
            let (row, column) = self.tracking.screen().cursor_position();
            // One-based, as a terminal counts.
            let _ = write!(terminal, "\x1b[{};{}R", row + 1, column + 1);
            at = end;
        }
        // A question cut in two by the read is answered once it is whole.
        let kept = (1..CURSOR_ASKED.len())
            .rev()
            .find(|&length| rest[at..].ends_with(&CURSOR_ASKED[..length]))
            .unwrap_or(0);
        let cut = rest.len() - kept;
        self.tracking.process(&rest[at..cut]);
        self.partial = rest[cut..].to_vec();
    }
}

/// The screen running in a terminal, killed when the test lets go of it
/// unless it was quit.
pub struct Terminal<'a> {
    scenario: &'a Scenario,
    line: String,
    child: Child,
    /// Where keys are typed.
    keyboard: File,
    /// What the screen draws, as it draws it.
    drawn: Receiver<Vec<u8>>,
    parser: vt100::Parser,
}

impl Terminal<'_> {
    /// Type `keys`, as a person does: each byte as a terminal sends it.
    pub fn types(&mut self, keys: &str) -> &mut Self {
        self.keyboard
            .write_all(keys.as_bytes())
            .expect("type into the terminal");
        self
    }

    /// Press a key a terminal sends as a sequence of its own.
    pub fn presses(&mut self, key: Key) -> &mut Self {
        self.types(key.bytes());
        if let Key::Escape = key {
            // An escape the screen reads together with the key after it is
            // that key with Alt held, as in any terminal: a person's next key
            // comes later than this, and so does a test's.
            std::thread::sleep(Duration::from_millis(100));
        }
        self
    }

    /// Wait until the screen shows `expected` on one of its rows, and
    /// return the screen as it is then.
    pub fn shows(&mut self, expected: &str) -> &mut Self {
        self.until(
            |screen| screen.lines().any(|row| row.contains(expected)),
            expected,
        )
    }

    /// Wait until the rows from `first` down are exactly `expected`: what a
    /// part of the screen is once it is drawn.
    pub fn draws(&mut self, first: u16, expected: &[&str]) -> &mut Self {
        let end = first + u16::try_from(expected.len()).expect("rows of a screen");
        let wanted: Vec<String> = expected.iter().map(|row| (*row).to_owned()).collect();
        let started = Instant::now();
        loop {
            while self.take_in(Duration::ZERO) {}
            let drawn = self.rows(first..end);
            if drawn == wanted {
                return self;
            }
            if started.elapsed() > PATIENCE {
                panic!(
                    "`{}` did not draw rows {first}..{end} within {PATIENCE:?}.\n--- expected\n{}\n--- drawn\n{}\n--- the screen\n{self}",
                    self.line,
                    wanted.join("\n"),
                    drawn.join("\n"),
                );
            }
            self.take_in(Duration::from_millis(50));
        }
    }

    /// Wait until no row of the screen shows `gone`.
    pub fn no_longer_shows(&mut self, gone: &str) -> &mut Self {
        self.until(
            |screen| !screen.lines().any(|row| row.contains(gone)),
            &format!("no row showing {gone:?}"),
        )
    }

    /// The screen's row `row`, from 0 at the top, its trailing spaces cut.
    pub fn row(&self, row: u16) -> String {
        self.screen()
            .lines()
            .nth(usize::from(row))
            .unwrap_or_default()
            .to_owned()
    }

    /// The rows in `range`, each as [`row`](Self::row) gives it.
    pub fn rows(&self, range: std::ops::Range<u16>) -> Vec<String> {
        range.map(|row| self.row(row)).collect()
    }

    /// Every row of the screen, each with its trailing spaces cut, paths
    /// read as `/home`.
    pub fn screen(&self) -> String {
        let screen = self.parser.screen();
        let rows: Vec<String> = screen
            .rows(0, COLUMNS)
            .map(|row| row.trim_end().to_owned())
            .collect();
        self.scenario.seen(rows.join("\n").as_bytes())
    }

    /// Wait until the screen exits, and say how it did and what the
    /// terminal shows once it has given its window back: the transcript it
    /// prints on the way out, under where it was started.
    pub fn exits(mut self) -> Closed {
        let started = Instant::now();
        let status = loop {
            self.take_in(Duration::from_millis(20));
            if let Some(status) = self.child.try_wait().expect("wait for the screen") {
                break status;
            }
            assert!(
                started.elapsed() < PATIENCE,
                "`{}` still running {PATIENCE:?} on:\n{}",
                self.line,
                self.screen()
            );
        };
        // Whatever it wrote before it went, to the last byte.
        while self.take_in(Duration::from_millis(50)) {}
        assert!(
            !self.parser.screen().alternate_screen(),
            "`{}` left the alternate screen on:\n{}",
            self.line,
            self.screen()
        );
        Closed {
            status,
            shown: self.screen(),
        }
    }

    /// Wait until the screen has given the window back to what it handed
    /// the terminal to, an editor: off the alternate screen, the window as
    /// the shell left it, with whatever the editor wrote since.
    pub fn steps_aside(&mut self) -> &mut Self {
        self.until_drawn(
            |terminal| !terminal.parser.screen().alternate_screen(),
            "the screen stepping aside",
        )
    }

    /// Wait until the screen has taken its window back, once what it handed
    /// the terminal to has closed.
    pub fn comes_back(&mut self) -> &mut Self {
        self.until_drawn(
            |terminal| terminal.parser.screen().alternate_screen(),
            "the screen drawn again",
        )
    }

    /// Wait until `ready` holds of the screen, for at most [`PATIENCE`].
    fn until(&mut self, ready: impl Fn(&str) -> bool, waiting_for: &str) -> &mut Self {
        self.until_drawn(|terminal| ready(&terminal.screen()), waiting_for)
    }

    /// Wait until `ready` holds of the terminal, for at most [`PATIENCE`].
    fn until_drawn(&mut self, ready: impl Fn(&Self) -> bool, waiting_for: &str) -> &mut Self {
        let started = Instant::now();
        loop {
            // What it drew already first, so a screen that has moved on
            // since the last wait is not read as it was.
            while self.take_in(Duration::ZERO) {}
            if ready(self) {
                return self;
            }
            if started.elapsed() > PATIENCE {
                panic!(
                    "`{}` did not show {waiting_for:?} within {PATIENCE:?}; the screen:\n{}",
                    self.line, self
                );
            }
            self.take_in(Duration::from_millis(50));
        }
    }

    /// Feed the parser what the screen drew within `wait`; whether it drew
    /// anything.
    fn take_in(&mut self, wait: Duration) -> bool {
        let next = if wait.is_zero() {
            self.drawn.try_recv().map_err(|_| RecvTimeoutError::Timeout)
        } else {
            self.drawn.recv_timeout(wait)
        };
        match next {
            Ok(bytes) => {
                self.parser.process(&bytes);
                while let Ok(more) = self.drawn.try_recv() {
                    self.parser.process(&more);
                }
                true
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => false,
        }
    }
}

impl fmt::Display for Terminal<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "+{}+", "-".repeat(usize::from(COLUMNS)))?;
        for row in self.screen().lines() {
            writeln!(f, "|{row}")?;
        }
        write!(f, "+{}+", "-".repeat(usize::from(COLUMNS)))
    }
}

impl Drop for Terminal<'_> {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// A key a terminal sends as an escape sequence, or as a control byte.
#[derive(Clone, Copy)]
pub enum Key {
    Enter,
    Escape,
    Tab,
    Up,
    Down,
    Right,
    Left,
    Backspace,
    CtrlC,
    /// Clears the line being typed, as at a shell prompt.
    CtrlU,
}

impl Key {
    fn bytes(self) -> &'static str {
        match self {
            Key::Enter => "\r",
            Key::Escape => "\x1b",
            Key::Tab => "\t",
            Key::Up => "\x1b[A",
            Key::Down => "\x1b[B",
            Key::Right => "\x1b[C",
            Key::Left => "\x1b[D",
            Key::Backspace => "\x7f",
            Key::CtrlC => "\x03",
            Key::CtrlU => "\x15",
        }
    }
}

/// How the screen ended, and what the terminal showed after it.
pub struct Closed {
    pub status: ExitStatus,
    /// The terminal's rows once the screen gave its window back.
    pub shown: String,
}
