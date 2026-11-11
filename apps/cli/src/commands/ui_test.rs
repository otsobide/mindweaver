use super::*;

use tempfile::TempDir;

const LINK: &str = "http://127.0.0.1:4000/?token=abc";

/// What the same launch hands a script through `--token-file`.
const API: &str = "http://127.0.0.1:4000/api/v1";
const TOKEN: &str = "abc";

fn options(dev: bool) -> UiOptions {
    UiOptions {
        dev,
        ..UiOptions::default()
    }
}

/// What `announce` printed on each stream.
fn announced(options: &UiOptions) -> (String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    announce(
        LINK,
        SocketAddr::from(([127, 0, 0, 1], 4000)),
        Path::new("/work"),
        options,
        &mut out,
        &mut err,
    )
    .unwrap();
    (
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// The link `open` asked for, if any, and what it said on stderr; asked of
/// a stand-in answering `answer`, so no test starts a browser.
fn opened(no_open: bool, answer: io::Result<()>) -> (Option<String>, String) {
    let mut err = Vec::new();
    let mut asked = None;
    open(LINK, no_open, &mut err, |url| {
        asked = Some(url.to_owned());
        answer
    });
    (asked, String::from_utf8(err).unwrap())
}

#[test]
fn the_link_is_printed_alone_on_stdout_and_where_it_serves_on_stderr() {
    let (out, err) = announced(&options(false));

    assert_eq!(out, format!("{LINK}\n"));
    assert_eq!(
        err,
        "note: serving the workspace at /work on 127.0.0.1:4000 until Ctrl-C\n"
    );
}

#[test]
fn through_the_dev_server_the_note_says_where() {
    let (_, err) = announced(&options(true));

    assert_eq!(
        err,
        "note: serving the workspace at /work on 127.0.0.1:4000, through the panel's dev \
         server at http://127.0.0.1:5173 (`make web/dev`), until Ctrl-C\n"
    );
}

#[test]
fn the_link_is_opened_unless_told_not_to() {
    assert_eq!(
        opened(false, Ok(())),
        (Some(LINK.to_owned()), String::new())
    );
    assert_eq!(opened(true, Ok(())), (None, String::new()));
}

/// As on a machine with no display: the link is there to open by hand.
#[test]
fn a_browser_that_cannot_be_opened_is_said_and_nothing_fails() {
    let (_, err) = opened(
        false,
        Err(io::Error::new(io::ErrorKind::NotFound, "no display")),
    );

    assert_eq!(
        err,
        "warning: no browser could be opened (no display); open the link above in one\n"
    );
}

/// What `--registry` and `--finder-url` name is what the panel's searches
/// and adds ask, as typed; none of either, the official registry and each
/// finder's own address.
#[test]
fn the_catalogs_the_panel_asks_are_the_ones_its_launch_names() {
    let named = UiOptions {
        registry: Some(String::from("http://127.0.0.1:4100")),
        finder_urls: vec![
            String::from("skills.sh=http://localhost:4200"),
            String::from("claude-plugins.dev=https://mirror.example"),
        ],
        ..UiOptions::default()
    };

    assert_eq!(
        catalogs(&named).unwrap(),
        Catalogs {
            registry: named.registry.clone(),
            finders: named.finder_urls.clone(),
        }
    );
    assert_eq!(
        catalogs(&UiOptions::default()).unwrap(),
        Catalogs::default()
    );
}

/// An address a line asking it would refuse refuses the launch, with the
/// code the line would give, before anything is served: plain http away
/// from this machine, which anything on the way could answer for, and a
/// `FINDER=URL` naming no finder.
#[test]
fn a_catalog_a_line_would_refuse_refuses_the_launch() {
    let refused = |options: UiOptions| catalogs(&options).unwrap_err().code();

    assert_eq!(
        refused(UiOptions {
            registry: Some(String::from("http://registry.example")),
            ..UiOptions::default()
        }),
        "mcp_registry_base_insecure"
    );
    assert_eq!(
        refused(UiOptions {
            finder_urls: vec![String::from("skills.sh=http://mirror.example")],
            ..UiOptions::default()
        }),
        "finder_url_invalid"
    );
    assert_eq!(
        refused(UiOptions {
            finder_urls: vec![String::from("nobody=https://mirror.example")],
            ..UiOptions::default()
        }),
        "finder_url_unnamed"
    );
}

/// The dev server's proxy looks on 7878, so `--dev` alone is enough for
/// `make web/dev`; a port named, 0 included, is the port.
#[test]
fn the_port_is_the_one_named_or_the_dev_servers_proxys_or_any() {
    let named = |port, dev| {
        super::port(&UiOptions {
            port,
            no_open: true,
            dev,
            ..UiOptions::default()
        })
    };

    assert_eq!(named(None, false), 0);
    assert_eq!(named(None, true), 7878);
    assert_eq!(named(Some(9000), false), 9000);
    assert_eq!(named(Some(9000), true), 9000);
    assert_eq!(named(Some(0), true), 0);
}

// ---------------------------------------------------------------------------
// The log
// ---------------------------------------------------------------------------

/// A moment of a run on CI whose trace the log was made to be read beside:
/// 04:20:59.756, UTC, on 5 November 2026.
const MOMENT: u64 = 1_793_852_459_756;

#[test]
fn a_logged_line_is_said_after_its_time_of_day_in_utc() {
    let at = |millis: u64| time_of_day(UNIX_EPOCH + std::time::Duration::from_millis(millis));

    assert_eq!(at(MOMENT), "04:20:59.756Z");
    assert_eq!(at(0), "00:00:00.000Z");
    assert_eq!(at(86_400_000 - 1), "23:59:59.999Z");
    assert_eq!(
        at(86_400_000),
        "00:00:00.000Z",
        "a day later, the same time"
    );
    assert_eq!(at(MOMENT + 3_600_000 * 20), "00:20:59.756Z");
}

/// Before 1970, which no clock should say, is said as midnight then rather
/// than refused: a line of the log is no reason for a panel to stop.
#[test]
fn a_time_before_1970_is_said_as_its_start() {
    let before = UNIX_EPOCH - std::time::Duration::from_secs(1);

    assert_eq!(time_of_day(before), "00:00:00.000Z");
}

// ---------------------------------------------------------------------------
// The token file
// ---------------------------------------------------------------------------

/// `TokenFile::write` of [`API`] and [`TOKEN`] at `path`, which must be
/// refused, and why.
fn refused(path: &Path) -> CliError {
    match TokenFile::write(path, API, TOKEN) {
        Ok(_) => panic!("{}: written", path.display()),
        Err(error) => error,
    }
}

/// Every name a script reads pinned, as the envelope's are: the version,
/// where the API is and the token, a field to a line.
#[test]
fn the_token_file_holds_its_version_where_the_api_is_and_the_token() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("api.json");

    let _written = TokenFile::write(&path, API, TOKEN).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\n  \"version\": 1,\n  \"api\": \"http://127.0.0.1:4000/api/v1\",\n  \
         \"token\": \"abc\"\n}\n"
    );
}

#[cfg(unix)]
#[test]
fn no_account_but_its_owners_may_read_the_token_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("api.json");

    let _written = TokenFile::write(&path, API, TOKEN).unwrap();

    let mode = fs::metadata(&path).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600, "{mode:o}");
}

/// A file somebody wrote and a folder alike: refused by the one code, and
/// left as they were.
#[test]
fn what_is_at_the_path_already_is_refused_with_its_code_and_left_as_it_was() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("api.json");
    fs::write(&file, "somebody's\n").unwrap();
    let folder = dir.path().join("run");
    fs::create_dir(&folder).unwrap();

    for path in [&file, &folder] {
        let error = refused(path);

        assert_eq!(error.code(), "token_file_exists", "{error}");
        assert_eq!(
            error.to_string(),
            format!(
                "{}: already exists, and is left alone, so the panel was not served: \
                 --token-file writes a file of its own, never over one",
                path.display()
            )
        );
    }
    assert_eq!(fs::read_to_string(&file).unwrap(), "somebody's\n");
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
}

/// A link at the path is something there, wherever it leads: nothing is
/// written through it, to a file that is there or to one that is not.
#[cfg(unix)]
#[test]
fn a_link_at_the_path_is_refused_and_nothing_is_written_where_it_leads() {
    let dir = TempDir::new().unwrap();
    let kept = dir.path().join("kept");
    fs::write(&kept, "somebody's\n").unwrap();
    let nowhere = dir.path().join("nowhere");
    let to_kept = dir.path().join("to-kept.json");
    std::os::unix::fs::symlink(&kept, &to_kept).unwrap();
    let to_nowhere = dir.path().join("to-nowhere.json");
    std::os::unix::fs::symlink(&nowhere, &to_nowhere).unwrap();

    for path in [&to_kept, &to_nowhere] {
        let error = refused(path);

        assert_eq!(error.code(), "token_file_exists", "{error}");
    }
    assert_eq!(fs::read_to_string(&kept).unwrap(), "somebody's\n");
    assert!(
        fs::symlink_metadata(&nowhere).is_err(),
        "nothing made there"
    );
}

#[test]
fn a_folder_not_there_is_refused_with_its_code_and_none_is_made() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("run").join("api.json");

    let error = refused(&path);

    assert_eq!(error.code(), "token_file_folder_missing", "{error}");
    assert_eq!(
        error.to_string(),
        format!(
            "{}: its folder does not exist, so the panel was not served: \
             --token-file makes no folder",
            path.display()
        )
    );
    assert!(!dir.path().join("run").exists());
}

/// A file on the way where a folder should be, which Unix tells from a
/// folder not there, and Windows does not.
#[cfg(unix)]
#[test]
fn a_path_through_a_file_is_refused_as_unwritable() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("notes");
    fs::write(&file, "somebody's\n").unwrap();
    let path = file.join("api.json");

    let error = refused(&path);

    assert_eq!(error.code(), "token_file_unwritable", "{error}");
    assert!(
        error.to_string().starts_with(&format!(
            "{}: the API's address and token cannot be written there, so the panel was \
             not served: ",
            path.display()
        )),
        "{error}"
    );
    assert_eq!(fs::read_to_string(&file).unwrap(), "somebody's\n");
}

/// A [`TokenFile`] written at `api.json` in a folder of its own, and the
/// file's path.
fn written() -> (TempDir, PathBuf, TokenFile) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("api.json");
    let file = TokenFile::write(&path, API, TOKEN).unwrap();
    (dir, path, file)
}

/// What taking `file` away said, and whether the command still went well.
fn taken_away(file: TokenFile) -> (Vec<Warning>, bool) {
    let mut outcome = Outcome::plain(String::new());
    file.take_away(&mut outcome);
    (outcome.stderr, outcome.ok)
}

#[test]
fn the_token_file_is_taken_away_with_nothing_said() {
    let (_dir, path, file) = written();

    assert_eq!(taken_away(file), (Vec::new(), true));
    assert!(fs::symlink_metadata(&path).is_err(), "still there");
}

/// Changed, longer or shorter or the same length, it is somebody's: left
/// as they wrote it, and said, with nothing gone wrong.
#[test]
fn a_token_file_changed_since_is_left_as_it_is_and_said() {
    let same_length = String::from_utf8(handed(API, TOKEN))
        .unwrap()
        .replace(TOKEN, "xyz");
    for changed in ["{}\n", same_length.as_str()] {
        let (_dir, path, file) = written();
        fs::write(&path, changed).unwrap();

        let (said, ok) = taken_away(file);

        assert_eq!(
            said,
            [Warning::from(format!(
                "note: {}: changed since the API's address and token were written there, so it \
                 is left as it is",
                path.display()
            ))]
        );
        assert!(ok);
        assert_eq!(fs::read_to_string(&path).unwrap(), changed);
    }
}

/// A link put in its place is somebody's, even to what was written: left,
/// and what it leads to as well.
#[cfg(unix)]
#[test]
fn a_link_put_in_its_place_is_left_whatever_it_leads_to() {
    let (dir, path, file) = written();
    let copy = dir.path().join("copy.json");
    fs::rename(&path, &copy).unwrap();
    std::os::unix::fs::symlink(&copy, &path).unwrap();

    let (said, ok) = taken_away(file);

    assert_eq!(said.len(), 1, "{said:?}");
    assert!(said[0].text.starts_with("note: "), "{said:?}");
    assert!(ok);
    assert!(fs::symlink_metadata(&path).unwrap().is_symlink());
    assert_eq!(fs::read(&copy).unwrap(), handed(API, TOKEN));
}

#[test]
fn a_token_file_taken_away_already_is_nothing_to_say() {
    let (_dir, path, file) = written();
    fs::remove_file(&path).unwrap();

    assert_eq!(taken_away(file), (Vec::new(), true));
}

/// As when the panel fails once the file is written: it goes all the same,
/// and only while it holds what was written.
#[test]
fn dropped_before_it_was_taken_away_the_token_file_goes_all_the_same() {
    let (_dir, path, file) = written();

    drop(file);

    assert!(fs::symlink_metadata(&path).is_err(), "still there");
    let (_dir, path, file) = written();
    fs::write(&path, "{}\n").unwrap();
    drop(file);
    assert_eq!(fs::read_to_string(&path).unwrap(), "{}\n");
}

/// A folder it may no longer change keeps the file: a warning with its
/// code, which fails the command, since a token is left behind. Not where
/// the folder's mode keeps nobody out, as for root, which is said.
#[cfg(unix)]
#[test]
fn a_token_file_that_cannot_be_taken_away_is_a_warning_with_its_code() {
    use std::os::unix::fs::PermissionsExt as _;
    let (dir, path, file) = written();
    let mode = |mode| fs::set_permissions(dir.path(), fs::Permissions::from_mode(mode)).unwrap();
    mode(0o500);
    if fs::write(dir.path().join("probe"), "").is_ok() {
        mode(0o700);
        eprintln!("skipped: a folder's mode keeps this account out of nothing");
        return;
    }

    let (said, ok) = taken_away(file);

    mode(0o700);
    assert_eq!(said.len(), 1, "{said:?}");
    assert_eq!(said[0].code, Some("token_file_unremoved"));
    assert!(
        said[0].text.starts_with(&format!(
            "warning: {}: could not be taken away once the panel stopped: ",
            path.display()
        )),
        "{said:?}"
    );
    assert!(!ok);
    assert!(path.is_file());
}
