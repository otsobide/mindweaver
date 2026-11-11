//! `mindweaver ui` and `mind ui`: the control panel, served from this binary
//! over the CLI's backend until Ctrl-C (D-17).
//!
//! A command as every other is, reached from both binaries through the one
//! parser, with one difference it shares with the TUI: the link is written
//! as it runs rather than built and handed back, since it has to reach the
//! person, and the browser, while the server is up, which is the whole time
//! the command runs. What comes back once it stops is what the TUI hands
//! back when it closes: every line that changed something, with what each
//! said, so the session can be read and done again without the panel.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::task::Poll;
use std::time::{SystemTime, UNIX_EPOCH};

use mindweaver_core::finder::{self, Search};
use mindweaver_core::http::Client;
use mindweaver_core::mcp_registry::Registry;
use mindweaver_server::{Listening, Server};
use serde::Serialize;
use tokio::sync::oneshot;

use super::typed;
use crate::backend::{Log, PanelBackend};
use crate::parse::UiOptions;
use crate::task::Catalogs;
use crate::{CliError, Failure, Lookup, Outcome, Warning};

/// Where the panel's dev server serves it while it is worked on (`make
/// web/dev`), as a browser writes the origin: `server.host` and
/// `server.port` in apps/web/vite.config.ts, which takes no other port
/// (`strictPort`).
const DEV_SERVER: &str = "http://127.0.0.1:5173";

/// Where that dev server's proxy sends `/api` and the launch link (`SERVER`
/// in the same file): the port `--dev` listens on unless told another.
const DEV_PORT: u16 = 7878;

/// How what `--token-file` writes is laid out ([`Handed`]).
const TOKEN_FILE_VERSION: u32 = 1;

/// Serve the panel over the workspace `lookup` resolves from `directory`
/// until Ctrl-C: what `mindweaver ui` and `mind ui` do.
///
/// The workspace is resolved once, here, before anything listens, from the
/// `-C`, `--home`, `-W` and agent homes this command line was given
/// ([`PanelBackend::launch`]), and nothing the browser sends changes it. So
/// a workspace that cannot be used, a port that cannot be taken, or a token
/// file that cannot be written ([`TokenFile::write`]), is said as any
/// command's failure is, and no link is printed. The file is written once
/// the port is taken, which it names, and before anything is served; it is
/// taken away once nothing is ([`TokenFile::take_away`]).
pub(crate) fn ui_command(
    directory: &Path,
    options: &UiOptions,
    lookup: &Lookup,
) -> Result<Outcome, Failure> {
    let token_file = options
        .token_file
        .as_deref()
        .map(|path| typed(directory, path))
        .transpose()?;
    let log = options.log.then(on_stderr);
    let catalogs = catalogs(options)?;
    let mut backend = PanelBackend::launch(directory, lookup)?.asking(catalogs);
    if let Some(log) = &log {
        backend = backend.logging(Arc::clone(log));
    }
    let root = backend.root();
    // A clone keeps what the server's ran, for the transcript.
    let kept = backend.clone();
    let port = port(options);
    let mut server = Server::new(backend).port(port);
    if options.dev {
        server = server.dev_server(DEV_SERVER);
    }
    if let Some(log) = log {
        server = server.log(move |line| log(line));
    }
    let listening = server.bind().map_err(|source| CliError::Unbound {
        port,
        dev: options.dev,
        source,
    })?;
    let url = listening.url();
    let address = listening.address();
    let unserved = |source| CliError::Unserved { source };
    // One thread: every query goes to tokio's blocking pool and every run to
    // the server's own writer thread, so the runtime only moves bytes.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(unserved)?;
    let served = runtime.block_on(async {
        let stops = Stops::listen().map_err(unserved)?;
        // Once a stop is listened for, so one sent as soon as the file is
        // there takes it away as every other does; and before the link, so
        // a script that waits for the link finds the file whole.
        let handed = token_file
            .as_deref()
            .map(|path| TokenFile::write(path, &listening.api(), listening.token()))
            .transpose()?;
        announce(
            &url,
            address,
            &root,
            options,
            &mut io::stdout(),
            &mut io::stderr(),
        )
        .map_err(unserved)?;
        // Off the runtime, beside the server rather than before it: a text
        // browser runs in the foreground until it is closed, and would
        // otherwise wait for a server that never started.
        let no_open = options.no_open;
        tokio::task::spawn_blocking(move || {
            open(&url, no_open, &mut io::stderr(), webbrowser::open);
        });
        serve(listening, stops).await.map_err(unserved)?;
        Ok::<_, CliError>(handed)
    });
    // Without waiting for a browser still open in the foreground, which the
    // runtime would otherwise do before it let go.
    runtime.shutdown_background();
    let handed = served?;
    let mut outcome = Outcome::plain(kept.transcript());
    if let Some(handed) = handed {
        handed.take_away(&mut outcome);
    }
    Ok(outcome)
}

/// The catalogs the panel's searches and adds ask in place of each
/// command's own (`--registry`, `--finder-url`), each held before anything
/// listens to the rules the line naming it holds it to: an address the
/// registry's reader refuses, plain http away from this machine among it,
/// a `FINDER=URL` naming no finder, or an address a finder may not be
/// asked at, refuses the launch with its code, rather than every search
/// sent from the panel afterwards.
pub(crate) fn catalogs(options: &UiOptions) -> Result<Catalogs, CliError> {
    if let Some(registry) = &options.registry {
        Registry::at(registry, Client::new())?;
    }
    let mut search = Search::new(Client::new());
    for at in &options.finder_urls {
        let (finder, base) = finder::addressed(at)?;
        search = search.at(finder, &base)?;
    }
    Ok(Catalogs {
        registry: options.registry.clone(),
        finders: options.finder_urls.clone(),
    })
}

/// The port to listen on: the one named, or else the dev server's proxy's
/// under `--dev`, or else any the system picks.
fn port(options: &UiOptions) -> u16 {
    options
        .port
        .unwrap_or(if options.dev { DEV_PORT } else { 0 })
}

/// Say where the panel is.
///
/// The link alone on stdout, so a script reads it as the first line there,
/// and the only one until the transcript follows it once the panel stops;
/// on stderr, what is served, where, and how it stops: where, because
/// through the dev server the link names the dev server, and the port it
/// must reach is said nowhere else.
fn announce(
    url: &str,
    address: SocketAddr,
    root: &Path,
    options: &UiOptions,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> io::Result<()> {
    writeln!(out, "{url}")?;
    out.flush()?;
    let through = if options.dev {
        format!(", through the panel's dev server at {DEV_SERVER} (`make web/dev`),")
    } else {
        String::new()
    };
    // Said or not, the panel is served: stderr gone is no reason to stop.
    let _ = writeln!(
        err,
        "note: serving the workspace at {} on {address}{through} until Ctrl-C",
        root.display()
    );
    Ok(())
}

/// Where `--log` sends what the server and the backend say: each line to
/// stderr as it is said, after the time of day it was said at
/// ([`time_of_day`]).
///
/// Written as it is said rather than gathered and handed back, as the link
/// is: it is for watching a panel serve, or for reading beside a browser's
/// trace of the same minutes once something took too long (#240), and
/// either needs it while the panel runs. A line stderr will not take is
/// lost rather than a reason to stop, as the note is.
fn on_stderr() -> Log {
    Arc::new(|line: &str| {
        let _ = writeln!(
            io::stderr().lock(),
            "{} {line}",
            time_of_day(SystemTime::now())
        );
    })
}

/// The time of day `at` was, in UTC, to the millisecond: `04:20:59.756Z`.
///
/// The time of day alone, which is what reading a log beside a trace of the
/// same run asks for, where a date would only lengthen every line. In UTC,
/// which needs no timezone to be looked up, and is what a browser's trace
/// and CI's own log say their times in, so a line here is found there by
/// its time as it reads.
fn time_of_day(at: SystemTime) -> String {
    let since = at.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs() % 86_400;
    format!(
        "{:02}:{:02}:{:02}.{:03}Z",
        seconds / 3_600,
        seconds / 60 % 60,
        seconds % 60,
        since.subsec_millis()
    )
}

/// What `--token-file` writes, for a script to read where the API is and
/// the token to send it: a contract, as the envelope's names are, so
/// dropping or renaming a field, or changing what it holds, moves
/// [`TOKEN_FILE_VERSION`], and adding one does not.
///
/// Its schema goes by what it is to the script, the token file, rather
/// than by what this end does, which hands it over.
#[derive(Serialize)]
#[cfg_attr(
    feature = "schema",
    derive(schemars::JsonSchema),
    schemars(rename = "TokenFile")
)]
pub(crate) struct Handed<'a> {
    /// [`TOKEN_FILE_VERSION`], so a script can tell a layout it was not
    /// written for.
    version: u32,
    /// Where every route is served: `http://127.0.0.1:<port>/api/v1`.
    api: &'a str,
    /// The launch token, to send as `Authorization: Bearer <token>`.
    token: &'a str,
}

/// What `--token-file` holds: [`Handed`], a field to a line for whoever
/// opens it, and ended as a text file is.
pub(crate) fn handed(api: &str, token: &str) -> Vec<u8> {
    let handed = Handed {
        version: TOKEN_FILE_VERSION,
        api,
        token,
    };
    let mut text = serde_json::to_vec_pretty(&handed).expect("strings serialize");
    text.push(b'\n');
    text
}

/// The file `--token-file` named, written for this run, and taken away
/// once the panel stops, only while it still holds what was written there:
/// by [`TokenFile::take_away`], which says what kept it, or when it is
/// dropped before that, on a failure once it was written, saying nothing.
///
/// Compared with what was written rather than taken away whatever it
/// holds: the path is somebody's to write as much as the panel's, and once
/// it holds something else that something is theirs, kept as everything
/// somebody wrote is. A copy of what was written, put there by somebody,
/// goes all the same: it holds a token that opens nothing any more.
///
/// No `Debug`, which would print the token it holds.
struct TokenFile {
    path: PathBuf,
    written: Vec<u8>,
    /// Whether [`TokenFile::take_away`] has seen to it, which dropping it
    /// then leaves alone.
    settled: bool,
}

/// What became of a [`TokenFile`] once the panel stopped.
enum Settled {
    /// It held what was written, and is gone.
    Taken,
    /// Somebody took it away first.
    Gone,
    /// It holds something else, or something else is at its path, a link
    /// among them, which is left as it is.
    Kept,
}

impl TokenFile {
    /// Write where the API is and its token to `path`.
    ///
    /// With `create_new`, so whatever is there already, a file, a folder
    /// or a link, is refused and left as it is: never written over, and
    /// never written through, wherever a link would lead, as a temporary
    /// renamed onto the path would replace it. A link among the folders on
    /// the way is followed: the path is the person's own, typed on their
    /// command line, unlike one a repository or a browser names. In one
    /// write, which is done before the link is printed.
    ///
    /// Mode 0600 on Unix, so no account but this one reads the token.
    /// Windows has no mode bits: the file takes the permissions its folder
    /// passes on, which in the account's own folders let in the account,
    /// the system and the administrators, and in a folder others share
    /// them too. An ACL of the file's own is #154's, with the rest of
    /// Windows.
    fn write(path: &Path, api: &str, token: &str) -> Result<Self, CliError> {
        let mut made = OpenOptions::new();
        made.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut made, 0o600);
        let mut file = made.open(path).map_err(|source| match source.kind() {
            io::ErrorKind::AlreadyExists => CliError::TokenFileExists(path.to_owned()),
            io::ErrorKind::NotFound => CliError::TokenFileFolderMissing(path.to_owned()),
            _ => CliError::TokenFileUnwritable {
                path: path.to_owned(),
                source,
            },
        })?;
        let written = handed(api, token);
        let wrote = file.write_all(&written);
        // Closed first, which Windows needs of a file before it is removed.
        drop(file);
        match wrote {
            Ok(()) => Ok(Self {
                path: path.to_owned(),
                written,
                settled: false,
            }),
            Err(source) => {
                // Made a moment ago, and of no use half written.
                let _ = fs::remove_file(path);
                Err(CliError::TokenFileUnwritable {
                    path: path.to_owned(),
                    source,
                })
            }
        }
    }

    /// Take the file away now the panel has stopped, and say in `outcome`
    /// what kept it: a note when it holds something else, which is
    /// somebody's to keep and no fault of the command's, or a warning,
    /// which fails it, when it could not be looked at or taken away, and
    /// a token is perhaps still there.
    fn take_away(mut self, outcome: &mut Outcome) {
        self.settled = true;
        match self.settle() {
            Ok(Settled::Taken | Settled::Gone) => {}
            Ok(Settled::Kept) => outcome.stderr.push(Warning::from(format!(
                "note: {}: changed since the API's address and token were written there, so \
                 it is left as it is",
                self.path.display()
            ))),
            Err(source) => {
                let error = CliError::TokenFileUnremoved {
                    path: self.path.clone(),
                    source,
                };
                outcome
                    .stderr
                    .push(Warning::coded(error.code(), format!("warning: {error}")));
                outcome.ok = false;
            }
        }
    }

    /// Take the file away if it holds what was written.
    ///
    /// Looked at as a link, never through one: a link put at the path is
    /// somebody's, whatever it leads to. By its length first, so a large
    /// file put in its place is never read.
    fn settle(&self) -> io::Result<Settled> {
        let found = match fs::symlink_metadata(&self.path) {
            Ok(found) => found,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settled::Gone),
            Err(error) => return Err(error),
        };
        if !found.is_file() || found.len() != self.written.len() as u64 {
            return Ok(Settled::Kept);
        }
        match fs::read(&self.path) {
            Ok(held) if held == self.written => {}
            Ok(_) => return Ok(Settled::Kept),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settled::Gone),
            Err(error) => return Err(error),
        }
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(Settled::Taken),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settled::Gone),
            Err(error) => Err(error),
        }
    }
}

/// On a failure once the file was written: taken away all the same, with
/// nothing said beside the failure, which is what there is to say.
impl Drop for TokenFile {
    fn drop(&mut self) {
        if !self.settled {
            let _ = self.settle();
        }
    }
}

/// Open the link with `opener`, unless told not to.
///
/// A browser that cannot be opened is said rather than failed on: the link
/// is there to open by hand, as on a machine with no display.
fn open(
    url: &str,
    no_open: bool,
    err: &mut dyn Write,
    opener: impl FnOnce(&str) -> io::Result<()>,
) {
    if no_open {
        return;
    }
    if let Err(error) = opener(url) {
        let _ = writeln!(
            err,
            "warning: no browser could be opened ({error}); open the link above in one"
        );
    }
}

/// Answer requests until the first stop asked for ([`Stops`]), then finish
/// those under way; or, at a second, stop at once.
///
/// Gracefully first, so a run the panel asked for is answered rather than
/// cut off half way, which would leave the disk as a command killed half way
/// does. The second is for whoever would rather have that than wait.
async fn serve(listening: Listening, mut stops: Stops) -> io::Result<()> {
    let (stop, stopped) = oneshot::channel::<()>();
    let serving = tokio::spawn(listening.serve(async {
        let _ = stopped.await;
    }));
    let abort = serving.abort_handle();
    tokio::spawn(async move {
        stops.next().await;
        let _ = stop.send(());
        stops.next().await;
        abort.abort();
    });
    match serving.await {
        Ok(served) => served,
        // Aborted at the second stop, which is one asked for.
        Err(error) if error.is_cancelled() => Ok(()),
        Err(error) => std::panic::resume_unwind(error.into_panic()),
    }
}

/// A stop asked for, listened for from the moment [`Stops::listen`]
/// returns: Ctrl-C, and on Unix the `SIGTERM` that `kill`, `timeout` and a
/// service manager stop a process with, taken as a Ctrl-C.
///
/// The platform's own listener rather than `tokio::signal::ctrl_c`, which
/// installs its handler only once it is first awaited, after the link is
/// out: a Ctrl-C pressed in between would end the process by the default
/// it replaces, rather than the way every other one does.
///
/// A `SIGTERM` too, since a script that serves itself the API runs the
/// panel in the background and stops it as it stops any process: ended by
/// that signal's default, `ui` would leave its token file behind, and the
/// next launch naming that path refused. Not a `SIGHUP`, which a handler
/// would answer even under `nohup`, whose ignoring it replaces: a panel
/// somebody meant to outlive the terminal would stop with it. On Windows
/// the console's Ctrl-C alone; its close and shutdown events are #154's,
/// with the rest of Windows.
struct Stops {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
    #[cfg(windows)]
    interrupt: tokio::signal::windows::CtrlC,
}

impl Stops {
    /// Start listening. Inside the runtime, which delivers them.
    fn listen() -> io::Result<Self> {
        #[cfg(unix)]
        let stops = {
            use tokio::signal::unix::{signal, SignalKind};
            Self {
                interrupt: signal(SignalKind::interrupt())?,
                terminate: signal(SignalKind::terminate())?,
            }
        };
        #[cfg(windows)]
        let stops = Self {
            interrupt: tokio::signal::windows::ctrl_c()?,
        };
        Ok(stops)
    }

    /// The next stop asked for, of either kind; never, once every listener
    /// has stopped, which leaves whatever waits for one waiting rather than
    /// taking it for one. One stopped leaves the other's to wait for.
    async fn next(&mut self) {
        std::future::poll_fn(|context| {
            #[cfg(unix)]
            if let Poll::Ready(Some(())) = self.terminate.poll_recv(context) {
                return Poll::Ready(());
            }
            match self.interrupt.poll_recv(context) {
                Poll::Ready(Some(())) => Poll::Ready(()),
                _ => Poll::Pending,
            }
        })
        .await;
    }
}

#[cfg(test)]
#[path = "ui_test.rs"]
mod tests;
