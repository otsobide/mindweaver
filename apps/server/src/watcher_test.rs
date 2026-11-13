use std::fs;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use notify::event::{AccessKind, AccessMode, CreateKind, Flag, ModifyKind};
use serde_json::json;
use tempfile::TempDir;

use super::*;
use crate::events::Subscription;

fn watched(path: &Path, scope: &str) -> Watched {
    Watched {
        path: path.to_path_buf(),
        scope: json!(scope),
    }
}

/// A folder holding a skills folder with one skill, the skill's own deep
/// folders, an `AGENTS.md`, and no `.claude` yet.
fn tree() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("skills/deploy/scripts/deep")).unwrap();
    fs::write(dir.path().join("AGENTS.md"), "# Here\n").unwrap();
    dir
}

/// The acceptance's test on the path list: every folder watched is one the
/// list names, or the folder above one, each on its own; nothing below a
/// folder named is, however deep the tree there goes, and no watch is a
/// recursive one, which would cover the whole tree under the project's
/// folder, watched for its `AGENTS.md`.
#[test]
fn only_the_folders_named_and_those_above_them_are_watched() {
    let dir = tree();
    let root = dir.path();
    let skills = root.join("skills");
    let list = Arc::new(Mutex::new(vec![
        watched(&skills, "project"),
        watched(&skills.join("deploy"), "project"),
        watched(&root.join("AGENTS.md"), "project"),
        watched(&root.join(".claude").join("skills"), "project"),
    ]));
    let mut state = state(Stand::default(), &list, &Events::new());

    state.apply(&list.lock().unwrap().clone(), &[]);

    assert_eq!(
        state.watches.set,
        [root.to_path_buf(), skills.clone(), skills.join("deploy")]
    );
    assert!(state.watches.recursive.is_empty());
}

/// The scopes a change to `path` reaches under `plan`, by their names.
fn reached(plan: &Plan, path: PathBuf) -> Vec<serde_json::Value> {
    plan.reached(&path)
        .into_iter()
        .map(|scope| plan.scopes[scope].clone())
        .collect()
}

#[test]
fn a_change_counts_for_a_path_named_or_one_directly_in_a_folder_named() {
    let dir = tree();
    let root = dir.path();
    let skills = root.join("skills");
    let list = [
        watched(&skills, "project"),
        watched(&root.join("AGENTS.md"), "agents"),
        watched(&root.join(".claude").join("skills"), "claude"),
    ];
    let plan = Plan::of(&list, Path::is_dir);
    let reached = |path: PathBuf| reached(&plan, path);

    assert_eq!(reached(skills.join("lint")), [json!("project")]);
    assert_eq!(reached(skills.clone()), [json!("project")]);
    assert_eq!(reached(root.join("AGENTS.md")), [json!("agents")]);
    // The first folder on the way to one not there yet, being made.
    assert_eq!(reached(root.join(".claude")), [json!("claude")]);
    // Below a folder's own entries, or beside a file named: nothing.
    assert!(reached(skills.join("deploy").join("SKILL.md")).is_empty());
    assert!(reached(root.join("README.md")).is_empty());
}

/// `rm -rf` of a project, or `mv` of a folder in it, is told by the one
/// folder it took, which is above what is named rather than named: it is a
/// change to each path below it.
#[test]
fn a_change_to_a_folder_above_paths_named_reaches_their_scopes() {
    let dir = tree();
    let root = dir.path();
    let skills = root.join("skills");
    let list = [
        watched(&skills.join("deploy"), "deploy"),
        watched(&root.join("AGENTS.md"), "agents"),
    ];
    let plan = Plan::of(&list, Path::is_dir);
    let reached = |path: PathBuf| reached(&plan, path);

    assert_eq!(reached(skills.clone()), [json!("deploy")]);
    assert_eq!(
        reached(root.to_path_buf()),
        [json!("deploy"), json!("agents")]
    );
    assert_eq!(
        reached(root.parent().unwrap().to_path_buf()),
        [json!("deploy"), json!("agents")]
    );
    // Beside them, a folder holding nothing named.
    assert!(reached(root.join("src")).is_empty());
    assert!(reached(skills.join("lint")).is_empty());
}

/// What sets the watches in a test: it records each, and refuses every one
/// past `room` as the system refuses one past its limit.
#[derive(Default)]
struct Stand {
    set: Vec<PathBuf>,
    /// Each of them asked for with everything below it.
    recursive: Vec<PathBuf>,
    dropped: Vec<PathBuf>,
    room: Option<usize>,
}

impl Watches for Stand {
    fn watch(&mut self, folder: &Path, mode: RecursiveMode) -> notify::Result<()> {
        if self.room.is_some_and(|room| self.set.len() >= room) {
            return Err(notify::Error::new(ErrorKind::MaxFilesWatch).add_path(folder.into()));
        }
        self.set.push(folder.to_path_buf());
        if mode == RecursiveMode::Recursive {
            self.recursive.push(folder.to_path_buf());
        }
        Ok(())
    }

    fn unwatch(&mut self, folder: &Path) -> notify::Result<()> {
        self.dropped.push(folder.to_path_buf());
        Ok(())
    }
}

/// A watcher's state over `stand`, naming what `list` holds whenever asked.
fn state(stand: Stand, list: &Arc<Mutex<Vec<Watched>>>, events: &Events) -> State<Stand> {
    let named = Arc::clone(list);
    State {
        watches: stand,
        paths: Box::new(move || named.lock().unwrap().clone()),
        plan: Plan::default(),
        watched: BTreeSet::new(),
        real: BTreeMap::new(),
        events: events.clone(),
        said: None,
    }
}

/// Everything told since `subscription` last read, but its first word to
/// ask for everything.
async fn told(events: &Events, subscription: &mut Subscription) -> Vec<Event> {
    // A step of no run under way: told as it is, and kept by nothing.
    let end = Event::Progress {
        run: 0,
        progress: json!("the end"),
    };
    events.publish(end.clone());
    let mut seen = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), subscription.next())
            .await
            .unwrap()
            .unwrap();
        if event == end {
            return seen;
        }
        if !matches!(event, Event::Resync { .. }) {
            seen.push(event);
        }
    }
}

fn change(kind: EventKind, path: PathBuf) -> DebouncedEvent {
    DebouncedEvent::new(notify::Event::new(kind).add_path(path), Instant::now())
}

#[tokio::test]
async fn the_limit_is_told_once_what_fits_is_watched_and_room_made_is_told_too() {
    let dir = tree();
    let root = dir.path();
    let skills = root.join("skills");
    let list = Arc::new(Mutex::new(vec![
        watched(&skills, "project"),
        watched(&skills.join("deploy"), "project"),
    ]));
    let events = Events::new();
    let mut subscription = events.subscribe();
    let mut state = state(
        Stand {
            room: Some(2),
            ..Stand::default()
        },
        &list,
        &events,
    );

    state.apply(&list.lock().unwrap().clone(), &[]);
    // A second change, still at the limit, says nothing new.
    state.changed(&[change(EventKind::Create(CreateKind::Any), skills.join("x"))]);

    let limit = Event::Unwatched {
        reason: String::from(if cfg!(target_os = "linux") {
            "the system's limit on watched folders is reached (fs.inotify.max_user_watches)"
        } else {
            "the system's limit on watched folders is reached"
        }),
    };
    assert_eq!(
        told(&events, &mut subscription).await,
        [
            limit,
            Event::Changed {
                scopes: vec![json!("project")]
            }
        ]
    );
    assert_eq!(state.watches.set.len(), 2);

    // Fewer folders named, as when a project is unlinked: all are watched.
    list.lock().unwrap().pop();
    state.changed(&[change(EventKind::Create(CreateKind::Any), skills.join("y"))]);

    assert_eq!(
        told(&events, &mut subscription).await,
        [
            Event::Watching,
            Event::Changed {
                scopes: vec![json!("project")]
            }
        ]
    );
}

/// An install puts a copy in place by a rename over the folder, and the
/// watch stays with the folder it was set on, now gone: set it again.
#[tokio::test]
async fn a_folder_an_event_names_is_watched_again() {
    let dir = tree();
    let skills = dir.path().join("skills");
    let deploy = skills.join("deploy");
    let list = Arc::new(Mutex::new(vec![
        watched(&skills, "project"),
        watched(&deploy, "project"),
    ]));
    let events = Events::new();
    let mut state = state(Stand::default(), &list, &events);
    state.apply(&list.lock().unwrap().clone(), &[]);
    let set = state.watches.set.len();

    state.changed(&[change(
        EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::To)),
        deploy.clone(),
    )]);

    assert_eq!(state.watches.dropped, std::slice::from_ref(&deploy));
    assert_eq!(state.watches.set.len(), set + 1);
    assert_eq!(state.watches.set.last(), Some(&deploy));
}

/// notify drops every watch below a folder removed or renamed away, and
/// one renamed back within the debounce is where it was: every folder
/// below the one an event names is set again, not only that one.
#[tokio::test]
async fn every_folder_below_one_an_event_names_is_watched_again() {
    let dir = tree();
    let skills = dir.path().join("skills");
    let deploy = skills.join("deploy");
    let list = Arc::new(Mutex::new(vec![watched(&deploy, "project")]));
    let events = Events::new();
    let mut state = state(Stand::default(), &list, &events);
    state.apply(&list.lock().unwrap().clone(), &[]);
    assert_eq!(state.watches.set, [skills.clone(), deploy.clone()]);

    state.changed(&[change(
        EventKind::Modify(ModifyKind::Name(notify::event::RenameMode::To)),
        skills.clone(),
    )]);

    assert_eq!(state.watches.dropped, [skills.clone(), deploy.clone()]);
    assert_eq!(state.watches.set[2..], [skills, deploy]);
}

/// A snapshot opens every file it reads: were that a change, each snapshot
/// would set off the next.
#[tokio::test]
async fn a_file_opened_or_closed_unwritten_is_no_change() {
    let dir = tree();
    let agents = dir.path().join("AGENTS.md");
    let list = Arc::new(Mutex::new(vec![watched(&agents, "project")]));
    let events = Events::new();
    let mut subscription = events.subscribe();
    let mut state = state(Stand::default(), &list, &events);
    state.apply(&list.lock().unwrap().clone(), &[]);

    state.changed(&[
        change(
            EventKind::Access(AccessKind::Open(AccessMode::Any)),
            agents.clone(),
        ),
        change(
            EventKind::Access(AccessKind::Close(AccessMode::Read)),
            agents.clone(),
        ),
    ]);

    assert_eq!(told(&events, &mut subscription).await, [Event::Watching]);
}

/// The system dropped events it could not hold: anything may have changed.
#[tokio::test]
async fn events_the_system_dropped_change_every_scope() {
    let dir = tree();
    let list = Arc::new(Mutex::new(vec![
        watched(&dir.path().join("skills"), "one"),
        watched(&dir.path().join("AGENTS.md"), "two"),
    ]));
    let events = Events::new();
    let mut subscription = events.subscribe();
    let mut state = state(Stand::default(), &list, &events);
    state.apply(&list.lock().unwrap().clone(), &[]);

    state.changed(&[DebouncedEvent::new(
        notify::Event::new(EventKind::Other).set_flag(Flag::Rescan),
        Instant::now(),
    )]);

    assert_eq!(
        told(&events, &mut subscription).await,
        [
            Event::Watching,
            Event::Changed {
                scopes: vec![json!("one"), json!("two")]
            }
        ]
    );
}

#[test]
fn a_folder_that_cannot_be_watched_is_named_once_with_why() {
    let folder = Path::new("/somewhere/skills");
    let denied = notify::Error::io(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        .add_path(folder.to_path_buf());

    let Event::Unwatched { reason } = unwatched(folder, &denied) else {
        panic!("not unwatched");
    };

    assert_eq!(
        reason,
        format!(
            "{} cannot be watched: {}",
            folder.display(),
            std::io::Error::from(std::io::ErrorKind::PermissionDenied)
        )
    );
}

/// macOS's FSEvents names a change by the real path of where it happened,
/// `/private/var/...` for a folder watched as `/var/...`: it is told by the
/// path listed, as inotify names it. A link in the listed path stands in
/// for `/var` here, on every system.
#[cfg(unix)]
#[tokio::test]
async fn a_change_named_by_its_real_path_is_told_by_the_path_listed() {
    let dir = tree();
    let link = TempDir::new().unwrap();
    let through = link.path().join("var");
    std::os::unix::fs::symlink(dir.path(), &through).unwrap();
    let skills = through.join("skills");
    let list = Arc::new(Mutex::new(vec![watched(&skills, "project")]));
    let events = Events::new();
    let mut subscription = events.subscribe();
    let mut state = state(Stand::default(), &list, &events);
    state.apply(&list.lock().unwrap().clone(), &[]);

    let real = fs::canonicalize(dir.path()).unwrap().join("skills");
    state.changed(&[change(
        EventKind::Create(CreateKind::File),
        real.join("new"),
    )]);

    assert_eq!(
        told(&events, &mut subscription).await,
        [
            Event::Watching,
            Event::Changed {
                scopes: vec![json!("project")]
            }
        ]
    );
    assert_eq!(state.as_listed(&real), skills);
}
