//! The `LedgerStore` contract: what every store behind the ledger's port
//! promises a caller, run once for each store there is.
//!
//! Each test is a function of a [`Store`], and `every_store!` runs it against
//! SQLite in memory, SQLite on a file, and SQLite on a file closed and opened
//! again between every write and the read that checks it, so that what a
//! store keeps is what it hands back after a restart too. A new store joins
//! by a constructor of `Store` and a line in `every_store!`: nothing here
//! names SQLite, a table or a query, which `adapter/sqlite_test.rs` keeps to
//! itself, so the suite holds any store to what the port says rather than to
//! how one store happens to keep it.
//!
//! What is drawn from the mothers is new each run, and named in what is
//! asserted, so a failure says what it was given.

use std::collections::BTreeSet;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use mindweaver_core::adapter::SystemClock;
use mindweaver_core::ledger::{
    Action, Carried, Forgotten, InstallRecord, LedgerError, Moved, Outcome, Source, Target,
};
use mindweaver_core::mother::ledger::RouteMother;
use mindweaver_core::mother::{
    random, ArtifactNameMother, ContentHashMother, EntryTargetMother, InstallRecordMother,
    McpMother, OriginMother, ProjectIdMother, RedactedMother, RevisionMother, TargetMother,
    UrlMother,
};
use mindweaver_core::value::{ArtifactName, Origin, Redacted, SourceName};
use mindweaver_core::{Agent, Clock, Kind, Ledger, ProjectId, SourceKind, LEDGER_FILE};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// The stores
// ---------------------------------------------------------------------------

/// A store under the contract, as a test holds it: the ledger over it, and
/// how what it keeps is opened again.
///
/// Through `Deref`, a test calls the port's methods on it as a caller calls
/// them on a [`Ledger`], and [`reopen`](Self::reopen) between a write and
/// the read that checks it, which a store kept nowhere but its connection
/// takes as nothing to do.
struct Store {
    /// Declared first so it is dropped first: a connection closed before
    /// the folder it is in goes.
    ledger: Option<Ledger>,
    open: Box<dyn Fn() -> Ledger>,
    reopens: bool,
    clock: Option<Arc<dyn Clock>>,
    kept_at: PathBuf,
    _dir: Option<TempDir>,
}

impl Store {
    fn in_memory() -> Self {
        Self::kept(None, PathBuf::from(":memory:"), false, || {
            Ledger::in_memory().expect("open a ledger in memory")
        })
    }

    fn on_file() -> Self {
        Self::file(false)
    }

    fn reopened() -> Self {
        Self::file(true)
    }

    fn file(reopens: bool) -> Self {
        let dir = TempDir::new().expect("make a temporary folder");
        let path = dir.path().join(LEDGER_FILE);
        let opened = path.clone();
        Self::kept(Some(dir), path, reopens, move || {
            Ledger::open(&opened).expect("open the ledger's file")
        })
    }

    fn kept(
        dir: Option<TempDir>,
        kept_at: PathBuf,
        reopens: bool,
        open: impl Fn() -> Ledger + 'static,
    ) -> Self {
        Store {
            ledger: Some(open()),
            open: Box::new(open),
            reopens,
            clock: None,
            kept_at,
            _dir: dir,
        }
    }

    /// Stamp by `clock` from now on, and again each time it is reopened:
    /// the clock is the caller's, not something the store keeps.
    fn stamped_by(mut self, clock: Arc<dyn Clock>) -> Self {
        let ledger = self.take().with_clock(Arc::clone(&clock));
        self.ledger = Some(ledger);
        self.clock = Some(clock);
        self
    }

    /// Close the store and open what it kept again, for one that keeps it
    /// somewhere: what a later command finds.
    fn reopen(&mut self) {
        if !self.reopens {
            return;
        }
        // Closed before it is opened again, as a process that ends closes it.
        self.ledger = None;
        let ledger = (self.open)();
        self.ledger = Some(match &self.clock {
            Some(clock) => ledger.with_clock(Arc::clone(clock)),
            None => ledger,
        });
    }

    /// The ledger itself, for a test that moves it elsewhere.
    fn take(&mut self) -> Ledger {
        self.ledger.take().expect("the store is open")
    }
}

impl Deref for Store {
    type Target = Ledger;

    fn deref(&self) -> &Ledger {
        self.ledger.as_ref().expect("the store is open")
    }
}

/// Every test named, as a `#[test]` of its own against `$store`.
macro_rules! contract {
    ($store:ident: $($test:ident),* $(,)?) => {
        mod $store {
            $(
                #[test]
                fn $test() {
                    super::$test(super::Store::$store());
                }
            )*
        }
    };
}

/// Every test named, against every store. A function left out of the list
/// is never called, which the compiler says.
macro_rules! every_store {
    ($($test:ident),* $(,)?) => {
        contract!(in_memory: $($test),*);
        contract!(on_file: $($test),*);
        contract!(reopened: $($test),*);
    };
}

every_store![
    a_store_says_where_it_is_kept,
    a_new_store_records_nothing,
    a_refusal_names_where_the_store_is_kept_and_records_nothing,
    every_installation_reads_back_as_it_was_recorded,
    an_installation_from_before_hashes_reads_back_with_neither_origin_nor_hash,
    each_copy_keeps_the_hash_of_its_own_it_was_given,
    an_installation_recording_no_copy_still_reads_back,
    a_servers_installation_reads_back_with_every_entry_it_wrote_and_no_copy,
    installing_a_server_again_replaces_its_entries_whole,
    forgetting_a_servers_installation_takes_its_entries,
    an_installation_belongs_to_its_project_and_its_kind_alone,
    installing_a_name_again_replaces_its_record_and_its_copies_whole,
    installations_and_records_are_in_name_order_and_copies_in_path_order_each_once,
    forgetting_an_installation_says_whether_there_was_one_and_takes_its_copies,
    an_installation_from_a_source_points_at_it_until_the_source_is_forgotten,
    forgetting_a_copy_of_a_name_never_installed_forgets_nothing,
    forgetting_a_copy_the_installation_never_wrote_forgets_nothing,
    forgetting_each_copy_in_turn_forgets_the_installation_with_the_last,
    a_skill_recording_no_copy_is_forgotten_whole_by_a_change_at_any_path,
    a_rule_recording_no_copy_outlives_a_change_at_any_path,
    a_servers_installation_outlives_a_change_at_any_path_with_its_entries,
    a_rules_last_copy_forgotten_takes_its_installation_with_it,
    renaming_a_copy_of_a_name_never_installed_carries_nothing,
    renaming_a_copy_the_installation_never_wrote_carries_nothing,
    a_renamed_last_copy_takes_its_installation_to_the_new_name,
    a_renamed_copy_among_others_is_an_installation_of_its_own_from_the_same_place,
    a_copy_renamed_onto_a_name_installed_already_is_forgotten_and_that_name_untouched,
    the_last_copy_renamed_onto_a_name_installed_already_takes_its_installation,
    retargeting_moves_the_copies_and_keeps_the_rest_of_the_record,
    retargeting_to_no_copy_forgets_and_a_name_never_installed_is_nothing,
    a_migration_moves_each_installation_fills_a_missing_hash_and_logs_each_line,
    a_migration_passes_over_what_is_not_recorded_and_forgets_what_has_no_copy_left,
    a_migration_that_cannot_be_recorded_records_none_of_it,
    a_render_records_what_it_left_and_forgets_the_rest_of_its_kind_alone,
    a_render_of_nothing_forgets_every_installation_of_its_kind,
    a_render_notes_each_file_it_made_and_forgets_each_it_did_not,
    a_render_that_cannot_be_recorded_leaves_the_last_render_whole,
    whether_mindweaver_made_a_file_is_kept_per_project_and_path,
    a_source_is_registered_once_and_found_by_its_coordinate,
    every_coordinate_is_a_source_of_its_own_with_a_folder_of_its_own,
    a_source_remembers_the_revision_it_was_last_gathered_at,
    forgetting_a_source_takes_what_it_brought_and_frees_its_folder,
    every_source_is_listed_by_url_then_branch_then_folder_even_with_nothing_gathered,
    a_gathered_artifact_reads_back_with_its_source,
    gathering_a_name_again_from_one_source_replaces_it_and_from_another_adds_it,
    what_was_gathered_is_listed_by_name_then_source,
    a_shelf_moved_moves_every_path_under_it_and_nothing_else,
    no_credential_typed_into_a_url_comes_back_from_the_store,
    a_url_typed_with_a_credential_and_without_it_is_one_source,
    nothing_stored_through_the_port_is_left_to_scrub,
    every_spelling_of_one_repository_is_one_source,
    a_source_is_found_by_its_name_and_takes_another_when_renamed,
    a_project_is_known_by_its_id_at_the_route_it_was_last_seen_at,
    an_id_seen_at_another_route_moves_there_unless_it_was_last_seen_at_a_stale_one,
    a_route_seen_holding_another_id_forgets_the_one_it_held,
    a_route_holds_whatever_an_installation_or_a_made_file_records_there,
    a_relink_that_writes_moves_every_record_and_file_to_the_new_route,
    a_relink_whose_write_fails_moves_nothing_and_logs_nothing,
    a_relink_drops_what_another_project_left_at_the_new_route,
    the_log_reads_back_newest_first_as_far_as_it_is_asked,
    the_log_keeps_what_each_action_was_about_and_what_it_said,
    everything_a_store_stamps_is_stamped_by_the_clock_it_is_handed,
    each_write_reads_the_clock_when_it_is_made,
    a_store_handed_no_clock_stamps_by_the_systems,
    a_store_is_read_on_a_thread_of_its_own,
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A clock stopped at one moment.
#[derive(Debug)]
struct Stopped(i64);

impl Clock for Stopped {
    fn now(&self) -> i64 {
        self.0
    }
}

/// A clock that moves on a second each time it is read, so every write it
/// stamps is told from the one before without waiting for a second to pass.
#[derive(Debug)]
struct Ticking(AtomicI64);

impl Ticking {
    fn from(start: i64) -> Arc<Self> {
        Arc::new(Ticking(AtomicI64::new(start)))
    }

    /// What it will say next, without saying it.
    fn next(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
}

impl Clock for Ticking {
    fn now(&self) -> i64 {
        self.0.fetch_add(1, Ordering::SeqCst)
    }
}

/// Every action the log takes, for a test to draw from.
const ACTIONS: [Action; 5] = [
    Action::Gather,
    Action::Install,
    Action::Uninstall,
    Action::Relink,
    Action::Adopt,
];

/// `record` with its copies in path order and its entries in agent order:
/// what a store hands back is the same copies and entries, and in which
/// order it lists them the port leaves to it.
fn sorted(mut record: InstallRecord) -> InstallRecord {
    record.targets.sort_by(|a, b| a.path.cmp(&b.path));
    record.entries.sort_by_key(|entry| entry.agent);
    record
}

/// What the store records of `name` in `project`, its copies in path order.
fn recorded(store: &Store, project: &str, kind: Kind, name: &str) -> Option<InstallRecord> {
    store.installation(project, kind, name).unwrap().map(sorted)
}

/// A skill `name` with one copy at each of `paths`, each for no one agent.
fn skill_at(name: &str, paths: &[&str]) -> InstallRecord {
    InstallRecordMother::create(
        Kind::Skill,
        name,
        Some(OriginMother::random()),
        Some(ContentHashMother::random()),
        paths
            .iter()
            .map(|path| TargetMother::create(None, path))
            .collect(),
    )
}

/// A source registered for `url` at no branch, in the default folder.
fn source(store: &Store, url: &str) -> Source {
    store
        .source_for(
            SourceKind::Git,
            url,
            None,
            mindweaver_core::DEFAULT_SUBDIRECTORY,
        )
        .unwrap()
}

/// The paths of `record`'s copies, in path order.
fn paths_of(record: &InstallRecord) -> Vec<String> {
    let mut paths: Vec<String> = record.targets.iter().map(|t| t.path.clone()).collect();
    paths.sort();
    paths
}

/// `count` names for records of one project and kind, no two alike.
fn distinct_names(count: usize) -> Vec<ArtifactName> {
    let mut names: Vec<ArtifactName> = Vec::with_capacity(count);
    while names.len() < count {
        let name = ArtifactNameMother::random();
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

// ---------------------------------------------------------------------------
// Where it is kept
// ---------------------------------------------------------------------------

fn a_store_says_where_it_is_kept(mut store: Store) {
    store.reopen();

    assert_eq!(store.path(), store.kept_at);
}

fn a_new_store_records_nothing(mut store: Store) {
    store.reopen();
    let route = RouteMother::random();

    for kind in Kind::ALL {
        assert_eq!(
            store.installations(&route, kind).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(store.records(&route, kind).unwrap(), []);
        assert_eq!(store.copies(&route, kind).unwrap(), Vec::<String>::new());
    }
    assert!(!store.holds(&route).unwrap(), "{route}");
    assert!(!store.created(&route, "AGENTS.md").unwrap(), "{route}");
    assert_eq!(store.project_at(&route).unwrap(), None, "{route}");
    assert_eq!(store.known_projects().unwrap(), []);
    assert_eq!(store.sources().unwrap(), []);
    assert_eq!(store.gathered().unwrap(), []);
    assert_eq!(store.history(100).unwrap(), []);
    assert_eq!(store.tainted_sources().unwrap(), []);
    assert_eq!(store.scrub_log().unwrap(), 0);
}

fn a_refusal_names_where_the_store_is_kept_and_records_nothing(mut store: Store) {
    // A source the store never registered, which nothing it lists could
    // name: an artifact of it, and an installation pointing at it.
    let unregistered = mindweaver_core::mother::SourceMother::random();
    let name = ArtifactNameMother::random();
    let mut pointing = InstallRecordMother::skill(&name);
    pointing.source_id = Some(unregistered.id);

    let refusals = [
        store
            .record(
                &unregistered,
                Kind::Skill,
                &name,
                &format!("skills/{name}"),
                None,
                None,
            )
            .unwrap_err(),
        store.installed("collapse", &pointing).unwrap_err(),
    ];

    store.reopen();
    for refusal in refusals {
        assert_eq!(refusal.path(), store.path(), "{refusal}");
        assert!(
            LedgerError::CODES.contains(&refusal.code()),
            "{refusal}: {}",
            refusal.code()
        );
    }
    assert_eq!(store.gathered().unwrap(), []);
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &name), None);
}

// ---------------------------------------------------------------------------
// Installations
// ---------------------------------------------------------------------------

fn every_installation_reads_back_as_it_was_recorded(mut store: Store) {
    let routes = RouteMother::distinct(5);
    let mut written: Vec<(String, InstallRecord)> = Vec::new();
    for _ in 0..50 {
        let record = InstallRecordMother::any();
        let route = random::pick(&routes);
        // A name is one installation a project and kind, the last written.
        written.retain(|(at, kept)| {
            !(*at == route && kept.kind == record.kind && kept.name == record.name)
        });
        store.installed(&route, &record).unwrap();
        written.push((route, record));
    }

    store.reopen();
    for (route, record) in &written {
        assert_eq!(
            recorded(&store, route, record.kind, &record.name).as_ref(),
            Some(&sorted(record.clone())),
            "at {route}"
        );
        let listed: Vec<InstallRecord> = store
            .records(route, record.kind)
            .unwrap()
            .into_iter()
            .map(sorted)
            .collect();
        assert!(
            listed.contains(&sorted(record.clone())),
            "{record:?} is not among {listed:?}"
        );
    }
}

fn an_installation_from_before_hashes_reads_back_with_neither_origin_nor_hash(mut store: Store) {
    let name = ArtifactNameMother::random();
    let before = InstallRecordMother::before_hashes(&name);

    store.installed("collapse", &before).unwrap();

    store.reopen();
    let read = recorded(&store, "collapse", Kind::Skill, &name).expect("recorded");
    assert_eq!(read.origin, None, "{name}");
    assert_eq!(read.content_hash, None, "{name}");
    assert_eq!(read, sorted(before));
}

fn each_copy_keeps_the_hash_of_its_own_it_was_given(mut store: Store) {
    let route = ArtifactNameMother::route();
    let mut rule = InstallRecordMother::rule(&route);
    // And one beside them holding the tree the installation's hash names.
    rule.targets.push(TargetMother::create(
        Some(Agent::Codex),
        ".codex/rules/x.md",
    ));

    store.installed("collapse", &rule).unwrap();

    store.reopen();
    let read = recorded(&store, "collapse", Kind::Rule, &route).expect("recorded");
    for target in &rule.targets {
        let kept = read
            .targets
            .iter()
            .find(|copy| copy.path == target.path)
            .unwrap_or_else(|| panic!("{} is not among {read:?}", target.path));
        assert_eq!(kept, target);
    }
}

fn an_installation_recording_no_copy_still_reads_back(mut store: Store) {
    let name = ArtifactNameMother::random();
    for kind in Kind::ALL {
        let mut record = InstallRecordMother::skill(&name);
        record.kind = kind;
        record.targets.clear();
        store.installed("collapse", &record).unwrap();

        store.reopen();
        assert_eq!(
            recorded(&store, "collapse", kind, &name),
            Some(record.clone())
        );
        assert_eq!(store.records("collapse", kind).unwrap(), [record]);
        assert_eq!(
            store.installations("collapse", kind).unwrap(),
            [name.as_str()]
        );
        assert_eq!(
            store.copies("collapse", kind).unwrap(),
            Vec::<String>::new()
        );
    }
    assert!(store.holds("collapse").unwrap());
}

fn a_servers_installation_reads_back_with_every_entry_it_wrote_and_no_copy(mut store: Store) {
    let name = McpMother::name();
    let server = InstallRecordMother::mcp(&name);

    store.installed("collapse", &server).unwrap();

    store.reopen();
    assert_eq!(
        recorded(&store, "collapse", Kind::Mcp, &name),
        Some(server.clone()),
        "{name}"
    );
    assert_eq!(store.records("collapse", Kind::Mcp).unwrap(), [server]);
    // An entry is no copy: nothing of a server is recorded by a path.
    assert_eq!(
        store.copies("collapse", Kind::Mcp).unwrap(),
        Vec::<String>::new()
    );
    assert!(store.holds("collapse").unwrap());
}

fn installing_a_server_again_replaces_its_entries_whole(mut store: Store) {
    let name = McpMother::name();
    let first = InstallRecordMother::mcp(&name);
    let again = InstallRecord {
        entries: vec![EntryTargetMother::of(Agent::Codex, &name)],
        ..first.clone()
    };

    store.installed("collapse", &first).unwrap();
    store.installed("collapse", &again).unwrap();

    store.reopen();
    let read = recorded(&store, "collapse", Kind::Mcp, &name).expect("recorded");
    assert_eq!(read.entries, again.entries, "{name}");
    assert_ne!(read.entries[0].fingerprint, first.entries[1].fingerprint);
}

fn forgetting_a_servers_installation_takes_its_entries(mut store: Store) {
    let name = McpMother::name();
    let server = InstallRecordMother::mcp(&name);
    store.installed("collapse", &server).unwrap();

    assert!(store.uninstalled("collapse", Kind::Mcp, &name).unwrap());
    // Recorded again with none, none of the old ones come back with it.
    let bare = InstallRecord {
        entries: Vec::new(),
        ..server
    };
    store.installed("collapse", &bare).unwrap();

    store.reopen();
    assert_eq!(
        recorded(&store, "collapse", Kind::Mcp, &name),
        Some(bare),
        "{name}"
    );
}

fn an_installation_belongs_to_its_project_and_its_kind_alone(mut store: Store) {
    let [here, there] = <[String; 2]>::try_from(RouteMother::distinct(2)).unwrap();
    let name = ArtifactNameMother::random();
    let skill = InstallRecordMother::skill(&name);
    let rule = InstallRecordMother::rule(&name);

    store.installed(&here, &skill).unwrap();

    store.reopen();
    assert_eq!(
        recorded(&store, &there, Kind::Skill, &name),
        None,
        "{there}"
    );
    assert_eq!(recorded(&store, &here, Kind::Rule, &name), None, "{here}");
    assert_eq!(
        store.installations(&there, Kind::Skill).unwrap(),
        Vec::<String>::new()
    );
    assert_eq!(
        store.copies(&here, Kind::Rule).unwrap(),
        Vec::<String>::new()
    );
    assert!(!store.holds(&there).unwrap(), "{there}");

    // One name, a skill and a rule, in one project: two installations.
    store.installed(&here, &rule).unwrap();
    store.reopen();
    assert_eq!(
        recorded(&store, &here, Kind::Skill, &name),
        Some(sorted(skill.clone()))
    );
    assert_eq!(
        recorded(&store, &here, Kind::Rule, &name),
        Some(sorted(rule.clone()))
    );

    assert!(store.uninstalled(&here, Kind::Rule, &name).unwrap());
    store.reopen();
    assert_eq!(
        recorded(&store, &here, Kind::Skill, &name),
        Some(sorted(skill))
    );
}

fn installing_a_name_again_replaces_its_record_and_its_copies_whole(mut store: Store) {
    let name = ArtifactNameMother::random();
    let first = skill_at(&name, &[&format!(".claude/skills/{name}"), "skills/kept"]);
    let mut again = skill_at(&name, &["skills/kept", &format!(".agents/skills/{name}")]);
    again.origin = Some(OriginMother::random());

    store.installed("collapse", &first).unwrap();
    store.installed("collapse", &again).unwrap();

    store.reopen();
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &name),
        Some(sorted(again.clone()))
    );
    assert_eq!(
        store.installations("collapse", Kind::Skill).unwrap(),
        [name.as_str()]
    );
    assert_eq!(
        store.copies("collapse", Kind::Skill).unwrap(),
        paths_of(&again)
    );
}

fn installations_and_records_are_in_name_order_and_copies_in_path_order_each_once(
    mut store: Store,
) {
    let names = distinct_names(random::between(5, 20));
    let mut skills = Vec::new();
    let mut rules = Vec::new();
    for name in &names {
        let skill = InstallRecordMother::skill(name);
        // Every rule's section is in one `AGENTS.md`, shared by them all.
        let rule = InstallRecordMother::rule(name);
        store.installed("collapse", &skill).unwrap();
        store.installed("collapse", &rule).unwrap();
        skills.push(skill);
        rules.push(rule);
    }

    store.reopen();
    for (kind, written) in [(Kind::Skill, &skills), (Kind::Rule, &rules)] {
        let mut in_order: Vec<String> = names.iter().map(|name| name.to_string()).collect();
        in_order.sort();
        assert_eq!(store.installations("collapse", kind).unwrap(), in_order);
        let records: Vec<String> = store
            .records("collapse", kind)
            .unwrap()
            .into_iter()
            .map(|record| record.name.into_string())
            .collect();
        assert_eq!(records, in_order);
        let copies: BTreeSet<String> = written.iter().flat_map(paths_of).collect();
        assert_eq!(
            store.copies("collapse", kind).unwrap(),
            copies.into_iter().collect::<Vec<_>>(),
            "{kind}"
        );
    }
}

fn forgetting_an_installation_says_whether_there_was_one_and_takes_its_copies(mut store: Store) {
    let [gone, kept] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    let forgotten = InstallRecordMother::skill(&gone);
    let other = InstallRecordMother::skill(&kept);
    store.installed("collapse", &forgotten).unwrap();
    store.installed("collapse", &other).unwrap();

    assert!(
        store.uninstalled("collapse", Kind::Skill, &gone).unwrap(),
        "{gone}"
    );
    assert!(
        !store.uninstalled("collapse", Kind::Skill, &gone).unwrap(),
        "{gone}"
    );
    assert!(
        !store.uninstalled("collapse", Kind::Rule, &kept).unwrap(),
        "{kept}"
    );

    store.reopen();
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &gone), None);
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &kept),
        Some(sorted(other.clone()))
    );
    assert_eq!(
        store.copies("collapse", Kind::Skill).unwrap(),
        paths_of(&other)
    );
}

fn an_installation_from_a_source_points_at_it_until_the_source_is_forgotten(mut store: Store) {
    let gathered_from = source(&store, &UrlMother::https());
    let name = ArtifactNameMother::random();
    let mut record = InstallRecordMother::skill(&name);
    record.source_id = Some(gathered_from.id);
    record.origin = Some(gathered_from.origin());
    store.installed("collapse", &record).unwrap();

    store.reopen();
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &name).and_then(|r| r.source_id),
        Some(gathered_from.id)
    );

    assert!(store.forget_source(gathered_from.id).unwrap());
    store.reopen();
    let read = recorded(&store, "collapse", Kind::Skill, &name).expect("still recorded");
    assert_eq!(read.source_id, None, "{name}");
    assert_eq!(read.origin, Some(gathered_from.origin()), "{name}");
    assert_eq!(read.targets, sorted(record).targets);
}

// ---------------------------------------------------------------------------
// One copy forgotten
// ---------------------------------------------------------------------------

fn forgetting_a_copy_of_a_name_never_installed_forgets_nothing(mut store: Store) {
    let name = ArtifactNameMother::random();
    let other = InstallRecordMother::skill(&format!("{name}-other"));
    store.installed("collapse", &other).unwrap();

    for kind in Kind::ALL {
        assert_eq!(
            store
                .uninstalled_copy("collapse", kind, &name, &format!(".claude/skills/{name}"))
                .unwrap(),
            Forgotten::Nothing,
            "{kind} {name}"
        );
    }

    store.reopen();
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &other.name),
        Some(sorted(other))
    );
}

fn forgetting_a_copy_the_installation_never_wrote_forgets_nothing(mut store: Store) {
    let name = ArtifactNameMother::random();
    let skill = InstallRecordMother::skill(&name);
    store.installed("collapse", &skill).unwrap();

    let forgotten = store
        .uninstalled_copy("collapse", Kind::Skill, &name, "somewhere/else")
        .unwrap();

    store.reopen();
    assert_eq!(forgotten, Forgotten::Nothing, "{name}");
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &name),
        Some(sorted(skill))
    );
}

fn forgetting_each_copy_in_turn_forgets_the_installation_with_the_last(mut store: Store) {
    let name = ArtifactNameMother::random();
    let skill = InstallRecordMother::skill(&name);
    store.installed("collapse", &skill).unwrap();
    let mut left = sorted(skill.clone());

    for (n, target) in skill.targets.iter().enumerate() {
        let forgotten = store
            .uninstalled_copy("collapse", Kind::Skill, &name, &target.path)
            .unwrap();
        left.targets.retain(|copy| copy.path != target.path);

        store.reopen();
        if n + 1 < skill.targets.len() {
            assert_eq!(forgotten, Forgotten::Copy, "{}", target.path);
            // The rest of the record is what was installed, untouched.
            assert_eq!(
                recorded(&store, "collapse", Kind::Skill, &name),
                Some(left.clone())
            );
        } else {
            assert_eq!(forgotten, Forgotten::Installation, "{}", target.path);
            assert_eq!(recorded(&store, "collapse", Kind::Skill, &name), None);
        }
    }
    assert!(!store.holds("collapse").unwrap());
}

fn a_skill_recording_no_copy_is_forgotten_whole_by_a_change_at_any_path(mut store: Store) {
    let [forgotten, renamed] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    for name in [&forgotten, &renamed] {
        let mut before_targets = InstallRecordMother::before_hashes(name);
        before_targets.targets.clear();
        store.installed("collapse", &before_targets).unwrap();
    }

    let by_forgetting = store
        .uninstalled_copy("collapse", Kind::Skill, &forgotten, "skills/anything")
        .unwrap();
    let by_renaming = store
        .renamed_copy(
            "collapse",
            Kind::Skill,
            &renamed,
            "skills/anything",
            "elsewhere",
            "skills/elsewhere",
        )
        .unwrap();

    store.reopen();
    assert_eq!(by_forgetting, Forgotten::Installation, "{forgotten}");
    assert_eq!(by_renaming, Carried::Forgotten, "{renamed}");
    assert_eq!(
        store.installations("collapse", Kind::Skill).unwrap(),
        Vec::<String>::new()
    );
}

fn a_rule_recording_no_copy_outlives_a_change_at_any_path(mut store: Store) {
    let route = ArtifactNameMother::route();
    let mut reaching_nobody = InstallRecordMother::rule(&route);
    reaching_nobody.targets.clear();
    store.installed("collapse", &reaching_nobody).unwrap();

    let forgotten = store
        .uninstalled_copy("collapse", Kind::Rule, &route, "AGENTS.md")
        .unwrap();
    let carried = store
        .renamed_copy(
            "collapse",
            Kind::Rule,
            &route,
            "AGENTS.md",
            "moved",
            "AGENTS.md",
        )
        .unwrap();

    store.reopen();
    assert_eq!(forgotten, Forgotten::Nothing, "{route}");
    assert_eq!(carried, Carried::Nothing, "{route}");
    assert_eq!(
        recorded(&store, "collapse", Kind::Rule, &route),
        Some(reaching_nobody)
    );
}

fn a_servers_installation_outlives_a_change_at_any_path_with_its_entries(mut store: Store) {
    let name = McpMother::name();
    let server = InstallRecordMother::mcp(&name);
    store.installed("collapse", &server).unwrap();
    let renamed = format!("{name}x");

    // Where the project's own definition of the name is, and the file one of
    // its entries is in: a change at either reaches no entry it wrote.
    for at in [format!("mcp/{name}.toml"), server.entries[0].file.clone()] {
        let forgotten = store
            .uninstalled_copy("collapse", Kind::Mcp, &name, &at)
            .unwrap();
        let carried = store
            .renamed_copy(
                "collapse",
                Kind::Mcp,
                &name,
                &at,
                &renamed,
                &format!("mcp/{renamed}.toml"),
            )
            .unwrap();
        assert_eq!(forgotten, Forgotten::Nothing, "{name} at {at}");
        assert_eq!(carried, Carried::Nothing, "{name} at {at}");
    }

    store.reopen();
    assert_eq!(
        recorded(&store, "collapse", Kind::Mcp, &name),
        Some(server),
        "{name}"
    );
    assert_eq!(recorded(&store, "collapse", Kind::Mcp, &renamed), None);
}

fn a_rules_last_copy_forgotten_takes_its_installation_with_it(mut store: Store) {
    let route = ArtifactNameMother::route();
    let rule = InstallRecordMother::rule(&route);
    store.installed("collapse", &rule).unwrap();

    let first = store
        .uninstalled_copy("collapse", Kind::Rule, &route, &rule.targets[0].path)
        .unwrap();
    let last = store
        .uninstalled_copy("collapse", Kind::Rule, &route, &rule.targets[1].path)
        .unwrap();

    store.reopen();
    assert_eq!(
        (first, last),
        (Forgotten::Copy, Forgotten::Installation),
        "{route}"
    );
    assert_eq!(recorded(&store, "collapse", Kind::Rule, &route), None);
}

// ---------------------------------------------------------------------------
// One copy renamed
// ---------------------------------------------------------------------------

fn renaming_a_copy_of_a_name_never_installed_carries_nothing(mut store: Store) {
    let name = ArtifactNameMother::random();

    for kind in Kind::ALL {
        let carried = store
            .renamed_copy("collapse", kind, &name, "skills/a", "b", "skills/b")
            .unwrap();
        assert_eq!(carried, Carried::Nothing, "{kind} {name}");
    }

    store.reopen();
    assert!(!store.holds("collapse").unwrap());
}

fn renaming_a_copy_the_installation_never_wrote_carries_nothing(mut store: Store) {
    let name = ArtifactNameMother::random();
    let skill = InstallRecordMother::skill(&name);
    store.installed("collapse", &skill).unwrap();

    let carried = store
        .renamed_copy(
            "collapse",
            Kind::Skill,
            &name,
            "somewhere/else",
            "b",
            "skills/b",
        )
        .unwrap();

    store.reopen();
    assert_eq!(carried, Carried::Nothing, "{name}");
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &name),
        Some(sorted(skill))
    );
    assert_eq!(recorded(&store, "collapse", Kind::Skill, "b"), None);
}

fn a_renamed_last_copy_takes_its_installation_to_the_new_name(mut store: Store) {
    let gathered_from = source(&store, &UrlMother::https());
    let [name, to] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    let mut skill = InstallRecordMother::skill(&name);
    skill.source_id = Some(gathered_from.id);
    skill.targets = vec![TargetMother::holding(
        Some(Agent::Claude),
        &format!(".claude/skills/{name}"),
        ContentHashMother::random(),
    )];
    store.installed("collapse", &skill).unwrap();
    let to_path = format!(".claude/skills/{to}");

    let carried = store
        .renamed_copy(
            "collapse",
            Kind::Skill,
            &name,
            &skill.targets[0].path,
            &to,
            &to_path,
        )
        .unwrap();

    store.reopen();
    assert_eq!(carried, Carried::Moved, "{name} to {to}");
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &name), None);
    let moved = InstallRecord {
        name: to.clone(),
        targets: vec![Target {
            path: to_path,
            ..skill.targets[0].clone()
        }],
        ..skill
    };
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &to), Some(moved));
}

fn a_renamed_copy_among_others_is_an_installation_of_its_own_from_the_same_place(mut store: Store) {
    let [name, to] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    let skill = InstallRecordMother::skill(&name);
    store.installed("collapse", &skill).unwrap();
    let renamed = random::pick(&skill.targets);
    let to_path = format!("renamed/{to}");

    let carried = store
        .renamed_copy("collapse", Kind::Skill, &name, &renamed.path, &to, &to_path)
        .unwrap();

    store.reopen();
    assert_eq!(carried, Carried::Moved, "{} to {to_path}", renamed.path);
    let mut left = sorted(skill.clone());
    left.targets.retain(|copy| copy.path != renamed.path);
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &name), Some(left));
    let own = InstallRecord {
        name: to.clone(),
        targets: vec![Target {
            path: to_path,
            ..renamed.clone()
        }],
        ..skill
    };
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &to), Some(own));
}

fn a_copy_renamed_onto_a_name_installed_already_is_forgotten_and_that_name_untouched(
    mut store: Store,
) {
    let [name, taken] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    let skill = InstallRecordMother::skill(&name);
    let holder = InstallRecordMother::skill(&taken);
    store.installed("collapse", &skill).unwrap();
    store.installed("collapse", &holder).unwrap();
    let renamed = random::pick(&skill.targets);

    let carried = store
        .renamed_copy(
            "collapse",
            Kind::Skill,
            &name,
            &renamed.path,
            &taken,
            &format!("renamed/{taken}"),
        )
        .unwrap();

    store.reopen();
    assert_eq!(carried, Carried::Forgotten, "{} onto {taken}", renamed.path);
    let mut left = sorted(skill);
    left.targets.retain(|copy| copy.path != renamed.path);
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &name), Some(left));
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &taken),
        Some(sorted(holder))
    );
}

fn the_last_copy_renamed_onto_a_name_installed_already_takes_its_installation(mut store: Store) {
    let [name, taken] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    let skill = skill_at(&name, &[&format!("skills/{name}")]);
    let holder = InstallRecordMother::skill(&taken);
    store.installed("collapse", &skill).unwrap();
    store.installed("collapse", &holder).unwrap();

    let carried = store
        .renamed_copy(
            "collapse",
            Kind::Skill,
            &name,
            &skill.targets[0].path,
            &taken,
            &format!("skills/{taken}"),
        )
        .unwrap();

    store.reopen();
    assert_eq!(carried, Carried::Forgotten, "{name} onto {taken}");
    assert_eq!(recorded(&store, "collapse", Kind::Skill, &name), None);
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &taken),
        Some(sorted(holder))
    );
}

// ---------------------------------------------------------------------------
// Copies moved
// ---------------------------------------------------------------------------

fn retargeting_moves_the_copies_and_keeps_the_rest_of_the_record(mut store: Store) {
    let record = InstallRecordMother::any();
    store.installed("collapse", &record).unwrap();
    let moved: Vec<Target> = (0..random::between(1, 3))
        .map(|n| {
            TargetMother::create(
                random::chance().then_some(Agent::OpenCode),
                &format!("moved/{n}"),
            )
        })
        .collect();

    let retargeted = store
        .retargeted("collapse", record.kind, &record.name, &moved)
        .unwrap();

    store.reopen();
    assert!(retargeted, "{record:?}");
    let expected = InstallRecord {
        targets: moved,
        ..record.clone()
    };
    assert_eq!(
        recorded(&store, "collapse", record.kind, &record.name),
        Some(sorted(expected))
    );
}

fn retargeting_to_no_copy_forgets_and_a_name_never_installed_is_nothing(mut store: Store) {
    let record = InstallRecordMother::any();
    store.installed("collapse", &record).unwrap();

    let forgotten = store
        .retargeted("collapse", record.kind, &record.name, &[])
        .unwrap();
    let nothing = store
        .retargeted(
            "collapse",
            record.kind,
            &record.name,
            &[TargetMother::create(None, "anywhere")],
        )
        .unwrap();

    store.reopen();
    assert!(forgotten, "{record:?}");
    assert!(!nothing, "{record:?}");
    assert_eq!(
        recorded(&store, "collapse", record.kind, &record.name),
        None
    );
}

// ---------------------------------------------------------------------------
// A project's migration
// ---------------------------------------------------------------------------

fn a_migration_moves_each_installation_fills_a_missing_hash_and_logs_each_line(mut store: Store) {
    let [before, hashed] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    let unhashed = InstallRecordMother::before_hashes(&before);
    let kept = InstallRecordMother::skill(&hashed);
    store.installed("collapse", &unhashed).unwrap();
    store.installed("collapse", &kept).unwrap();
    let id = ProjectIdMother::random();
    let moves = [moved_skill(&before), moved_skill(&hashed)];
    let lines: Vec<(Action, String)> = (0..random::between(1, 4))
        .map(|_| {
            (
                random::pick(&ACTIONS),
                RedactedMother::random().into_string(),
            )
        })
        .collect();

    store.migrated("collapse", &id, &moves, &lines).unwrap();

    store.reopen();
    let filled = recorded(&store, "collapse", Kind::Skill, &before).expect("still recorded");
    assert_eq!(filled.content_hash, moves[0].content_hash, "{before}");
    assert_eq!(filled.targets, sorted_targets(&moves[0].targets));
    assert_eq!(filled.origin, None, "{before}");
    let own = recorded(&store, "collapse", Kind::Skill, &hashed).expect("still recorded");
    assert_eq!(
        own.content_hash, kept.content_hash,
        "{hashed} keeps its own"
    );
    assert_eq!(own.targets, sorted_targets(&moves[1].targets));
    assert_eq!(store.project_at("collapse").unwrap(), Some(id));
    let history = store.history(lines.len()).unwrap();
    let said: Vec<(String, Option<Redacted>, String, Option<Redacted>)> = history
        .into_iter()
        .rev()
        .map(|entry| (entry.action, entry.target, entry.outcome, entry.detail))
        .collect();
    let expected: Vec<_> = lines
        .iter()
        .map(|(action, detail)| {
            (
                action.slug().to_owned(),
                Some(Redacted::new("collapse")),
                Outcome::Ok.slug().to_owned(),
                Some(Redacted::new(detail)),
            )
        })
        .collect();
    assert_eq!(said, expected);
}

/// Where the skill `name`'s copies are once its project has moved: under
/// `moved/`, so the move shows, with a hash for a record from before hashes
/// were kept. `MovedMother::skill` names the copies an install writes
/// today, which a store given them would seem to leave where they were.
fn moved_skill(name: &str) -> Moved {
    let mut moved = mindweaver_core::mother::MovedMother::skill(name);
    moved.targets = moved
        .targets
        .into_iter()
        .map(|target| Target {
            path: format!("moved/{}", target.path),
            ..target
        })
        .collect();
    moved
}

fn sorted_targets(targets: &[Target]) -> Vec<Target> {
    let mut sorted = targets.to_vec();
    sorted.sort_by(|a, b| a.path.cmp(&b.path));
    sorted
}

fn a_migration_passes_over_what_is_not_recorded_and_forgets_what_has_no_copy_left(
    mut store: Store,
) {
    let [never, emptied] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    store
        .installed("collapse", &InstallRecordMother::skill(&emptied))
        .unwrap();
    let mut gone = moved_skill(&emptied);
    gone.targets.clear();

    store
        .migrated(
            "collapse",
            &ProjectIdMother::random(),
            &[moved_skill(&never), gone],
            &[],
        )
        .unwrap();

    store.reopen();
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &never),
        None,
        "{never}"
    );
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &emptied),
        None,
        "{emptied}"
    );
    assert_eq!(store.history(10).unwrap(), []);
}

fn a_migration_that_cannot_be_recorded_records_none_of_it(mut store: Store) {
    let [first, second] = <[ArtifactName; 2]>::try_from(distinct_names(2)).unwrap();
    let one = InstallRecordMother::before_hashes(&first);
    let two = InstallRecordMother::skill(&second);
    store.installed("collapse", &one).unwrap();
    store.installed("collapse", &two).unwrap();
    // The second move names one copy twice, which no record can hold, after
    // the first was taken in.
    let mut twice = moved_skill(&second);
    twice.targets.push(twice.targets[0].clone());

    let refused = store.migrated(
        "collapse",
        &ProjectIdMother::random(),
        &[moved_skill(&first), twice],
        &[(Action::Install, String::from("collapse moved"))],
    );

    store.reopen();
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &first),
        Some(sorted(one))
    );
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &second),
        Some(sorted(two))
    );
    assert_eq!(store.project_at("collapse").unwrap(), None);
    assert_eq!(store.history(10).unwrap(), []);
}

// ---------------------------------------------------------------------------
// A render of a kind
// ---------------------------------------------------------------------------

fn a_render_records_what_it_left_and_forgets_the_rest_of_its_kind_alone(mut store: Store) {
    let routes = distinct_names(4);
    let [kept, dropped, added, elsewhere] = <[ArtifactName; 4]>::try_from(routes).unwrap();
    let skill = InstallRecordMother::skill(&dropped);
    let other_project = InstallRecordMother::rule(&elsewhere);
    store.installed("collapse", &skill).unwrap();
    store.installed("tanukeys", &other_project).unwrap();
    store
        .rendered(
            "collapse",
            Kind::Rule,
            &[
                InstallRecordMother::rule(&kept),
                InstallRecordMother::rule(&dropped),
            ],
            &[],
        )
        .unwrap();
    let changed = InstallRecordMother::rule(&kept);
    let new = InstallRecordMother::rule(&added);

    store
        .rendered("collapse", Kind::Rule, &[changed.clone(), new.clone()], &[])
        .unwrap();

    store.reopen();
    let records: Vec<InstallRecord> = store
        .records("collapse", Kind::Rule)
        .unwrap()
        .into_iter()
        .map(sorted)
        .collect();
    let mut expected = vec![sorted(changed), sorted(new)];
    expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    assert_eq!(records, expected);
    // Another kind in the project, and the kind in another, are not the
    // render's.
    assert_eq!(
        recorded(&store, "collapse", Kind::Skill, &dropped),
        Some(sorted(skill))
    );
    assert_eq!(
        recorded(&store, "tanukeys", Kind::Rule, &elsewhere),
        Some(sorted(other_project))
    );
}

fn a_render_of_nothing_forgets_every_installation_of_its_kind(mut store: Store) {
    let names = distinct_names(random::between(1, 5));
    let rules: Vec<InstallRecord> = names.iter().map(|n| InstallRecordMother::rule(n)).collect();
    store.rendered("collapse", Kind::Rule, &rules, &[]).unwrap();
    // Installed one by one too, which a render of the kind forgets as well.
    store
        .installed(
            "collapse",
            &InstallRecordMother::rule(&format!("{}-x", names[0])),
        )
        .unwrap();

    store.rendered("collapse", Kind::Rule, &[], &[]).unwrap();

    store.reopen();
    assert_eq!(store.records("collapse", Kind::Rule).unwrap(), []);
    assert_eq!(
        store.copies("collapse", Kind::Rule).unwrap(),
        Vec::<String>::new()
    );
}

fn a_render_notes_each_file_it_made_and_forgets_each_it_did_not(mut store: Store) {
    let rule = InstallRecordMother::rule(&ArtifactNameMother::route());
    store
        .rendered(
            "collapse",
            Kind::Rule,
            std::slice::from_ref(&rule),
            &[("AGENTS.md", true), ("CLAUDE.md", true)],
        )
        .unwrap();

    store.reopen();
    assert!(store.created("collapse", "AGENTS.md").unwrap());
    assert!(store.created("collapse", "CLAUDE.md").unwrap());

    store
        .rendered("collapse", Kind::Rule, &[rule], &[("CLAUDE.md", false)])
        .unwrap();

    store.reopen();
    assert!(
        store.created("collapse", "AGENTS.md").unwrap(),
        "a file the render did not name"
    );
    assert!(!store.created("collapse", "CLAUDE.md").unwrap());
}

fn a_render_that_cannot_be_recorded_leaves_the_last_render_whole(mut store: Store) {
    let names = distinct_names(3);
    let rules: Vec<InstallRecord> = names.iter().map(|n| InstallRecordMother::rule(n)).collect();
    store
        .rendered("collapse", Kind::Rule, &rules, &[("AGENTS.md", true)])
        .unwrap();
    // A render naming one copy twice, after another record it would have
    // changed and before the file it would have forgotten.
    let changed = InstallRecordMother::rule(&names[0]);
    let mut twice = InstallRecordMother::rule(&names[1]);
    twice.targets.push(twice.targets[0].clone());

    let refused = store.rendered(
        "collapse",
        Kind::Rule,
        &[changed, twice],
        &[("AGENTS.md", false)],
    );

    store.reopen();
    assert!(refused.is_err(), "{refused:?}");
    let records: Vec<InstallRecord> = store
        .records("collapse", Kind::Rule)
        .unwrap()
        .into_iter()
        .map(sorted)
        .collect();
    let mut expected: Vec<InstallRecord> = rules.into_iter().map(sorted).collect();
    expected.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));
    assert_eq!(records, expected);
    assert!(store.created("collapse", "AGENTS.md").unwrap());
}

fn whether_mindweaver_made_a_file_is_kept_per_project_and_path(mut store: Store) {
    let [here, there] = <[String; 2]>::try_from(RouteMother::distinct(2)).unwrap();
    store.set_created(&here, "AGENTS.md", true).unwrap();
    store.set_created(&here, "AGENTS.md", true).unwrap();

    store.reopen();
    assert!(store.created(&here, "AGENTS.md").unwrap(), "{here}");
    assert!(!store.created(&here, "CLAUDE.md").unwrap(), "{here}");
    assert!(!store.created(&there, "AGENTS.md").unwrap(), "{there}");

    store.set_created(&here, "AGENTS.md", false).unwrap();
    // Forgetting what was never noted is nothing to refuse.
    store.set_created(&there, "AGENTS.md", false).unwrap();

    store.reopen();
    assert!(!store.created(&here, "AGENTS.md").unwrap(), "{here}");
    assert!(!store.created(&there, "AGENTS.md").unwrap(), "{there}");
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

/// A coordinate a source is registered by: a URL, a branch or none, and a
/// folder.
fn coordinate() -> (String, Option<String>, String) {
    (
        UrlMother::clean(),
        random::chance().then(random::word),
        random::pick(&[
            String::from(mindweaver_core::DEFAULT_SUBDIRECTORY),
            random::word(),
        ]),
    )
}

fn a_source_is_registered_once_and_found_by_its_coordinate(mut store: Store) {
    let (url, reference, folder) = coordinate();
    assert_eq!(
        store
            .source(SourceKind::Git, &url, reference.as_deref(), &folder)
            .unwrap(),
        None,
        "{url}"
    );

    let registered = store
        .source_for(SourceKind::Git, &url, reference.as_deref(), &folder)
        .unwrap();

    store.reopen();
    assert_eq!(registered.kind, SourceKind::Git);
    assert_eq!(registered.url, url.as_str());
    assert_eq!(registered.reference, reference);
    assert_eq!(registered.subdirectory, folder);
    assert_eq!(registered.revision, None, "never gathered yet");
    assert!(!registered.directory.is_empty(), "{registered:?}");
    assert_eq!(
        store
            .source(SourceKind::Git, &url, reference.as_deref(), &folder)
            .unwrap(),
        Some(registered.clone())
    );
    // Its folder is chosen once, and is where its files already are.
    assert_eq!(
        store
            .source_for(SourceKind::Git, &url, reference.as_deref(), &folder)
            .unwrap(),
        registered
    );
    assert_eq!(store.sources().unwrap(), [registered]);
}

fn every_coordinate_is_a_source_of_its_own_with_a_folder_of_its_own(mut store: Store) {
    // Every form of URL, and one URL at two branches and in two folders:
    // many reduce to one readable name, and none may share a folder. A
    // form that spells another's repository is that one's source
    // ([`every_spelling_of_one_repository_is_one_source`]), and left out.
    let mut identities = BTreeSet::new();
    let mut coordinates: Vec<(String, Option<String>, String)> = UrlMother::every_clean()
        .into_iter()
        .filter(|url| identities.insert(mindweaver_core::url::canonical(url)))
        .map(|url| (url, None, String::from("skills")))
        .collect();
    let url = UrlMother::https();
    coordinates.push((url.clone(), None, String::from("skills")));
    coordinates.push((
        url.clone(),
        Some(String::from("main")),
        String::from("skills"),
    ));
    coordinates.push((
        url.clone(),
        Some(String::from("next")),
        String::from("skills"),
    ));
    coordinates.push((url, None, String::from("agents")));
    let registered: Vec<Source> = coordinates
        .iter()
        .map(|(url, reference, folder)| {
            store
                .source_for(SourceKind::Git, url, reference.as_deref(), folder)
                .unwrap()
        })
        .collect();

    store.reopen();
    let ids: BTreeSet<i64> = registered.iter().map(|source| source.id).collect();
    let folders: BTreeSet<&str> = registered.iter().map(|s| s.directory.as_str()).collect();
    assert_eq!(ids.len(), registered.len(), "{registered:#?}");
    assert_eq!(folders.len(), registered.len(), "{registered:#?}");
    for ((url, reference, folder), source) in coordinates.iter().zip(&registered) {
        assert_eq!(
            store
                .source(SourceKind::Git, url, reference.as_deref(), folder)
                .unwrap()
                .as_ref(),
            Some(source)
        );
    }
    assert_eq!(store.sources().unwrap().len(), registered.len());
}

fn a_source_remembers_the_revision_it_was_last_gathered_at(mut store: Store) {
    let registered = source(&store, &UrlMother::https());
    let first = RevisionMother::random();
    let second = RevisionMother::random();

    store
        .saw_source(registered.id, Some(first.as_str()))
        .unwrap();
    store.reopen();
    let read = store.sources().unwrap();
    assert_eq!(read[0].revision.as_ref(), Some(&first));

    store
        .saw_source(registered.id, Some(second.as_str()))
        .unwrap();
    store.reopen();
    assert_eq!(store.sources().unwrap()[0].revision.as_ref(), Some(&second));

    store.saw_source(registered.id, None).unwrap();
    // One the store never registered changes nothing, and is no failure.
    store
        .saw_source(registered.id + 1000, Some(first.as_str()))
        .unwrap();
    store.reopen();
    assert_eq!(store.sources().unwrap(), [registered]);
}

fn forgetting_a_source_takes_what_it_brought_and_frees_its_folder(mut store: Store) {
    let url = UrlMother::https();
    let forgotten = source(&store, &url);
    let kept = source(&store, &UrlMother::https());
    for from in [&forgotten, &kept] {
        let name = ArtifactNameMother::random();
        store
            .record(
                from,
                Kind::Skill,
                &name,
                &format!("skills/{name}"),
                None,
                None,
            )
            .unwrap();
    }

    assert!(store.forget_source(forgotten.id).unwrap());
    assert!(!store.forget_source(forgotten.id).unwrap());

    store.reopen();
    assert_eq!(
        store.source(SourceKind::Git, &url, None, "skills").unwrap(),
        None
    );
    assert_eq!(store.sources().unwrap(), std::slice::from_ref(&kept));
    let gathered = store.gathered().unwrap();
    assert!(
        gathered
            .iter()
            .all(|artifact| artifact.source.id == kept.id),
        "{gathered:#?}"
    );
    assert_eq!(gathered.len(), 1);
    // A gather cancelled after registering it leaves nothing holding its
    // folder, so the next source given that name has it.
    assert_eq!(source(&store, &url).directory, forgotten.directory);
}

fn every_source_is_listed_by_url_then_branch_then_folder_even_with_nothing_gathered(
    mut store: Store,
) {
    let mut registered = Vec::new();
    for _ in 0..random::between(5, 15) {
        let (url, reference, folder) = coordinate();
        registered.push(
            store
                .source_for(SourceKind::Git, &url, reference.as_deref(), &folder)
                .unwrap(),
        );
    }
    let with_artifact = random::pick(&registered);
    store
        .record(
            &with_artifact,
            Kind::Skill,
            "deploy",
            "skills/deploy",
            None,
            None,
        )
        .unwrap();

    store.reopen();
    registered.sort_by(|a, b| {
        (&a.url, &a.reference, &a.subdirectory).cmp(&(&b.url, &b.reference, &b.subdirectory))
    });
    registered.dedup();
    assert_eq!(store.sources().unwrap(), registered);
}

// ---------------------------------------------------------------------------
// What was gathered
// ---------------------------------------------------------------------------

fn a_gathered_artifact_reads_back_with_its_source(store: Store) {
    let at = random::between(1_600_000_000, 1_900_000_000) as i64;
    let mut store = store.stamped_by(Arc::new(Stopped(at)));
    let from = source(&store, &UrlMother::https());
    let revision = RevisionMother::random();
    store.saw_source(from.id, Some(revision.as_str())).unwrap();
    let name = ArtifactNameMother::random();
    let path = format!(".mindweaver/skills/{}/{name}", from.directory);
    let summary = random::chance().then(|| random::text(30));

    store
        .record(
            &from,
            Kind::Skill,
            &name,
            &path,
            summary.as_deref(),
            Some(revision.as_str()),
        )
        .unwrap();

    store.reopen();
    let gathered = store.gathered().unwrap();
    assert_eq!(gathered.len(), 1, "{gathered:#?}");
    let artifact = &gathered[0];
    assert_eq!(artifact.kind, Kind::Skill);
    assert_eq!(artifact.name, name);
    assert_eq!(artifact.path, path);
    assert_eq!(artifact.summary, summary);
    assert_eq!(artifact.revision.as_ref(), Some(&revision));
    assert_eq!(artifact.gathered_at, at);
    assert_eq!(
        artifact.source,
        Source {
            revision: Some(revision),
            ..from
        }
    );
}

fn gathering_a_name_again_from_one_source_replaces_it_and_from_another_adds_it(mut store: Store) {
    let first = source(&store, &UrlMother::https());
    let second = source(&store, &UrlMother::https());
    let name = ArtifactNameMother::random();
    store
        .record(&first, Kind::Skill, &name, "old/path", Some("Old"), None)
        .unwrap();
    let revision = RevisionMother::random();

    store
        .record(
            &first,
            Kind::Skill,
            &name,
            "new/path",
            None,
            Some(revision.as_str()),
        )
        .unwrap();
    store
        .record(
            &second,
            Kind::Skill,
            &name,
            "other/path",
            Some("Other"),
            None,
        )
        .unwrap();

    store.reopen();
    let gathered = store.gathered().unwrap();
    let of = |from: &Source| {
        gathered
            .iter()
            .filter(|artifact| artifact.source.id == from.id)
            .collect::<Vec<_>>()
    };
    assert_eq!(gathered.len(), 2, "{gathered:#?}");
    let replaced = of(&first);
    assert_eq!(replaced.len(), 1, "{gathered:#?}");
    assert_eq!(replaced[0].path, "new/path");
    assert_eq!(replaced[0].summary, None);
    assert_eq!(replaced[0].revision.as_ref(), Some(&revision));
    assert_eq!(of(&second)[0].summary.as_deref(), Some("Other"));
}

fn a_shelf_moved_moves_every_path_under_it_and_nothing_else(mut store: Store) {
    let from = source(&store, &UrlMother::https());
    let names = distinct_names(3);
    let under = format!(".{}/skills/{}", random::word(), random::word());
    store
        .record(
            &from,
            Kind::Skill,
            &names[0],
            &format!("{under}/a"),
            None,
            None,
        )
        .unwrap();
    store
        .record(
            &from,
            Kind::Rule,
            &names[1],
            &format!("{under}/b.md"),
            None,
            None,
        )
        .unwrap();
    // Beside it, spelled to look like it: a prefix of characters is not
    // a folder.
    let beside = format!("{under}_beside/c");
    store
        .record(&from, Kind::Skill, &names[2], &beside, None, None)
        .unwrap();

    let moved = store
        .shelf_moved(&format!("{under}/"), "elsewhere/")
        .unwrap();

    store.reopen();
    assert_eq!(moved, 2);
    let mut paths: Vec<String> = store
        .gathered()
        .unwrap()
        .into_iter()
        .map(|gathered| gathered.path)
        .collect();
    paths.sort();
    let mut expected = vec![
        String::from("elsewhere/a"),
        String::from("elsewhere/b.md"),
        beside,
    ];
    expected.sort();
    assert_eq!(paths, expected);
    assert_eq!(store.shelf_moved("nowhere/", "anywhere/").unwrap(), 0);
}

fn what_was_gathered_is_listed_by_name_then_source(mut store: Store) {
    let sources: Vec<Source> = (0..3)
        .map(|_| source(&store, &UrlMother::https()))
        .collect();
    let names = distinct_names(random::between(3, 8));
    let mut written: Vec<(String, String)> = Vec::new();
    for name in &names {
        for from in random::some_of(&sources) {
            store
                .record(
                    &from,
                    Kind::Skill,
                    name,
                    &format!("{}/{name}", from.directory),
                    None,
                    None,
                )
                .unwrap();
            written.push((name.to_string(), from.url.to_string()));
        }
    }

    store.reopen();
    written.sort();
    let listed: Vec<(String, String)> = store
        .gathered()
        .unwrap()
        .into_iter()
        .map(|artifact| {
            (
                artifact.name.into_string(),
                artifact.source.url.into_string(),
            )
        })
        .collect();
    assert_eq!(listed, written);
}

// ---------------------------------------------------------------------------
// Credentials
// ---------------------------------------------------------------------------

fn no_credential_typed_into_a_url_comes_back_from_the_store(mut store: Store) {
    let every = UrlMother::every_credentialed();
    for (n, url) in every.iter().enumerate() {
        let quoted = UrlMother::quoted(url);
        let project = format!("project-{n}");
        let registered = store
            .source_for(SourceKind::Git, &url.typed, Some("main"), "skills")
            .unwrap();
        store
            .record(
                &registered,
                Kind::Skill,
                "deploy",
                "skills/deploy",
                None,
                None,
            )
            .unwrap();
        let mut record = InstallRecordMother::skill("deploy");
        record.source_id = Some(registered.id);
        record.origin = Some(Origin::from(format!("{}#main", url.typed)));
        store.installed(&project, &record).unwrap();
        let mut rule = InstallRecordMother::rule("style");
        rule.origin = Some(Origin::from(url.typed.as_str()));
        store
            .rendered(&project, Kind::Rule, std::slice::from_ref(&rule), &[])
            .unwrap();
        store
            .log(
                random::pick(&ACTIONS),
                Some(url.typed.as_str()),
                Outcome::Failed,
                Some(quoted.typed.as_str()),
            )
            .unwrap();
        store
            .migrated(
                &project,
                &ProjectIdMother::random(),
                &[],
                &[(Action::Install, quoted.typed.clone())],
            )
            .unwrap();
    }

    store.reopen();
    let mut read = vec![
        format!("{:?}", store.sources().unwrap()),
        format!("{:?}", store.gathered().unwrap()),
        format!("{:?}", store.history(1000).unwrap()),
        format!("{:?}", store.tainted_sources().unwrap()),
    ];
    for (n, url) in every.iter().enumerate() {
        let project = format!("project-{n}");
        let found = store
            .source(SourceKind::Git, &url.typed, Some("main"), "skills")
            .unwrap()
            .unwrap_or_else(|| panic!("{} is not found by what was typed", url.clean));
        assert_eq!(found.url, url.clean.as_str());
        let origin = recorded(&store, &project, Kind::Skill, "deploy")
            .and_then(|record| record.origin)
            .expect("recorded with an origin");
        assert_eq!(origin, format!("{}#main", url.clean));
        let rule = recorded(&store, &project, Kind::Rule, "style").expect("rendered");
        assert_eq!(rule.origin.as_deref(), Some(url.clean.as_str()));
        read.push(format!("{found:?}"));
        read.push(format!(
            "{:?}",
            store.records(&project, Kind::Skill).unwrap()
        ));
        read.push(format!(
            "{:?}",
            store.records(&project, Kind::Rule).unwrap()
        ));
    }
    let read = read.join("\n");
    for url in &every {
        assert!(
            !read.contains(&url.secret),
            "{} is in what was read:\n{read}",
            url.secret
        );
    }
    let history = store.history(1000).unwrap();
    for url in &every {
        assert!(
            history
                .iter()
                .any(|entry| entry.target.as_deref() == Some(url.clean.as_str())),
            "{} is not what the log says it was about",
            url.clean
        );
    }
}

fn a_url_typed_with_a_credential_and_without_it_is_one_source(mut store: Store) {
    let url = UrlMother::credentialed();

    let typed = store
        .source_for(SourceKind::Git, &url.typed, None, "skills")
        .unwrap();
    let clean = store
        .source_for(SourceKind::Git, &url.clean, None, "skills")
        .unwrap();

    store.reopen();
    assert_eq!(typed, clean, "{}", url.clean);
    assert_eq!(store.sources().unwrap(), [clean]);
    assert!(!typed.directory.contains(&url.secret), "{typed:?}");
}

fn nothing_stored_through_the_port_is_left_to_scrub(mut store: Store) {
    let mut registered = Vec::new();
    for url in UrlMother::every_credentialed() {
        registered.push(source(&store, &url.typed));
        store
            .log(Action::Gather, Some(url.typed.as_str()), Outcome::Ok, None)
            .unwrap();
    }
    for url in UrlMother::every_clean() {
        registered.push(source(&store, &url));
    }

    store.reopen();
    assert_eq!(store.tainted_sources().unwrap(), []);
    for kept in &registered {
        store.scrub_source(kept.id).unwrap();
    }
    let never = registered.iter().map(|s| s.id).max().unwrap() + 1;
    store.scrub_source(never).unwrap();
    assert_eq!(store.scrub_log().unwrap(), 0);

    store.reopen();
    // A form that spells another's repository registered into it.
    let mut ids = BTreeSet::new();
    registered.retain(|source| ids.insert(source.id));
    registered.sort_by(|a, b| {
        (&a.url, &a.reference, &a.subdirectory).cmp(&(&b.url, &b.reference, &b.subdirectory))
    });
    assert_eq!(store.sources().unwrap(), registered);
}

fn every_spelling_of_one_repository_is_one_source(mut store: Store) {
    let (owner, repository) = (random::word(), random::word());
    let first = format!("https://github.com/{owner}/{repository}");
    let spellings = [
        format!("git@github.com:{owner}/{repository}.git"),
        format!("ssh://git@github.com/{owner}/{repository}"),
        format!(
            "https://github.com/{}/{repository}.git/",
            owner.to_uppercase()
        ),
        format!("{owner}/{repository}"),
    ];

    let registered = store
        .source_for(SourceKind::Git, &first, None, "skills")
        .unwrap();

    store.reopen();
    // Registered as it was first typed, and named for its repository.
    assert_eq!(registered.url, first.as_str());
    assert_eq!(registered.name, format!("{owner}-{repository}"));
    for spelling in &spellings {
        assert_eq!(
            store
                .source(SourceKind::Git, spelling, None, "skills")
                .unwrap()
                .as_ref(),
            Some(&registered),
            "{spelling}"
        );
        assert_eq!(
            store
                .source_for(SourceKind::Git, spelling, None, "skills")
                .unwrap(),
            registered,
            "{spelling}"
        );
    }
    // At a branch, the remote's HEAD being one too, it is a place of its own.
    let main = store
        .source_for(SourceKind::Git, &spellings[0], Some("main"), "skills")
        .unwrap();
    assert_ne!(main.id, registered.id);
    assert_eq!(main.name, format!("{owner}-{repository}-2"));
    store.reopen();
    assert_eq!(store.sources().unwrap().len(), 2);
}

fn a_source_is_found_by_its_name_and_takes_another_when_renamed(mut store: Store) {
    let first = source(&store, &UrlMother::https());
    let second = source(&store, &UrlMother::https());
    let name = SourceName::parse(&format!("{}-{}", random::word(), random::word())).unwrap();

    store.rename_source(first.id, &name).unwrap();

    store.reopen();
    let renamed = Source {
        name: name.clone(),
        ..first.clone()
    };
    assert_eq!(store.source_named(&name).unwrap(), Some(renamed.clone()));
    assert_eq!(store.source_named(&first.name).unwrap(), None);
    assert_eq!(
        store.source_named(&second.name).unwrap(),
        Some(second.clone())
    );
    // A name two sources would hold is refused, and the store is as it was.
    let refused = store.rename_source(second.id, &name).unwrap_err();
    assert_eq!(refused.code(), "ledger_query_failed");
    store.reopen();
    assert_eq!(store.source_named(&name).unwrap(), Some(renamed));
}

// ---------------------------------------------------------------------------
// Projects known by their id
// ---------------------------------------------------------------------------

fn a_project_is_known_by_its_id_at_the_route_it_was_last_seen_at(mut store: Store) {
    let routes = RouteMother::distinct(random::between(2, 6));
    let seen: Vec<(ProjectId, String)> = routes
        .iter()
        .map(|route| (ProjectIdMother::random(), route.clone()))
        .collect();

    let learned = store.knew(&seen, &[]).unwrap();
    let again = store.knew(&seen, &[]).unwrap();

    store.reopen();
    assert_eq!(learned, seen.len());
    assert_eq!(again, 0, "nothing new the second time");
    for (id, route) in &seen {
        assert_eq!(
            store.project_at(route).unwrap().as_ref(),
            Some(id),
            "{route}"
        );
    }
    let mut in_order = seen.clone();
    in_order.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(store.known_projects().unwrap(), in_order);
    assert_eq!(store.project_at("never-seen").unwrap(), None);
}

fn an_id_seen_at_another_route_moves_there_unless_it_was_last_seen_at_a_stale_one(
    mut store: Store,
) {
    let [first, moved, stale, kept] = <[String; 4]>::try_from(RouteMother::distinct(4)).unwrap();
    let travelling = ProjectIdMother::random();
    let waiting = ProjectIdMother::random();
    store
        .knew(
            &[
                (travelling.clone(), first.clone()),
                (waiting.clone(), stale.clone()),
            ],
            &[],
        )
        .unwrap();

    let learned = store
        .knew(
            &[
                (travelling.clone(), moved.clone()),
                (waiting.clone(), kept.clone()),
            ],
            std::slice::from_ref(&stale),
        )
        .unwrap();

    store.reopen();
    assert_eq!(learned, 1, "only {travelling} moved");
    assert_eq!(store.project_at(&first).unwrap(), None, "{first}");
    assert_eq!(store.project_at(&moved).unwrap(), Some(travelling));
    // What finds the stale entry's project once it has moved, until a
    // relink takes it from there.
    assert_eq!(store.project_at(&stale).unwrap(), Some(waiting));
    assert_eq!(store.project_at(&kept).unwrap(), None, "{kept}");
}

fn a_route_seen_holding_another_id_forgets_the_one_it_held(mut store: Store) {
    let route = RouteMother::random();
    let before = ProjectIdMother::random();
    let after = ProjectIdMother::random();
    store.knew(&[(before.clone(), route.clone())], &[]).unwrap();

    let learned = store.knew(&[(after.clone(), route.clone())], &[]).unwrap();

    store.reopen();
    assert_eq!(learned, 1);
    assert_eq!(store.project_at(&route).unwrap(), Some(after.clone()));
    assert_eq!(store.known_projects().unwrap(), [(after, route)]);
}

fn a_route_holds_whatever_an_installation_or_a_made_file_records_there(mut store: Store) {
    let [installed, made, neither] = <[String; 3]>::try_from(RouteMother::distinct(3)).unwrap();
    let record = InstallRecordMother::any();
    store.installed(&installed, &record).unwrap();
    store.set_created(&made, "AGENTS.md", true).unwrap();
    // An id alone is no record of the project's.
    store
        .knew(&[(ProjectIdMother::random(), neither.clone())], &[])
        .unwrap();

    store.reopen();
    assert!(store.holds(&installed).unwrap(), "{installed}");
    assert!(store.holds(&made).unwrap(), "{made}");
    assert!(!store.holds(&neither).unwrap(), "{neither}");

    store
        .uninstalled(&installed, record.kind, &record.name)
        .unwrap();
    store.set_created(&made, "AGENTS.md", false).unwrap();
    store.reopen();
    assert!(!store.holds(&installed).unwrap(), "{installed}");
    assert!(!store.holds(&made).unwrap(), "{made}");
}

// ---------------------------------------------------------------------------
// A relink
// ---------------------------------------------------------------------------

fn a_relink_that_writes_moves_every_record_and_file_to_the_new_route(mut store: Store) {
    let [old, new] = <[String; 2]>::try_from(RouteMother::distinct(2)).unwrap();
    let id = ProjectIdMother::random();
    store.knew(&[(id.clone(), old.clone())], &[]).unwrap();
    let skills: Vec<InstallRecord> = distinct_names(random::between(0, 4))
        .iter()
        .map(|name| InstallRecordMother::skill(name))
        .collect();
    let rules: Vec<InstallRecord> = distinct_names(random::between(0, 3))
        .iter()
        .map(|name| InstallRecordMother::rule(name))
        .collect();
    for record in skills.iter().chain(&rules) {
        store.installed(&old, record).unwrap();
    }
    store.set_created(&old, "AGENTS.md", true).unwrap();
    store.reopen();
    let history = store.history(100).unwrap().len();

    let moved = store
        .relinked(&old, &new, &id, || Ok::<_, std::io::Error>("marker"))
        .unwrap();

    store.reopen();
    assert_eq!(moved.unwrap(), ("marker", skills.len() + rules.len()));
    for record in skills.iter().chain(&rules) {
        assert_eq!(
            recorded(&store, &old, record.kind, &record.name),
            None,
            "{old}"
        );
        assert_eq!(
            recorded(&store, &new, record.kind, &record.name),
            Some(sorted(record.clone()))
        );
    }
    assert!(store.created(&new, "AGENTS.md").unwrap(), "{new}");
    assert!(!store.created(&old, "AGENTS.md").unwrap(), "{old}");
    assert!(!store.holds(&old).unwrap(), "{old}");
    assert_eq!(store.project_at(&new).unwrap(), Some(id.clone()));
    assert_eq!(store.project_at(&old).unwrap(), None, "{old}");
    let log = store.history(100).unwrap();
    assert_eq!(log.len(), history + 1);
    assert_eq!(log[0].action, Action::Relink.slug());
    assert_eq!(log[0].outcome, Outcome::Ok.slug());
    assert_eq!(log[0].target.as_deref(), Some(new.as_str()));
}

fn a_relink_whose_write_fails_moves_nothing_and_logs_nothing(mut store: Store) {
    let [old, new] = <[String; 2]>::try_from(RouteMother::distinct(2)).unwrap();
    let id = ProjectIdMother::random();
    store.knew(&[(id.clone(), old.clone())], &[]).unwrap();
    let record = InstallRecordMother::any();
    store.installed(&old, &record).unwrap();
    store.set_created(&old, "AGENTS.md", true).unwrap();
    // What another project left at the new route, which a relink that went
    // ahead would have dropped.
    let left = InstallRecordMother::any();
    store.installed(&new, &left).unwrap();

    let refused = store
        .relinked(&old, &new, &id, || {
            Err::<(), _>("the marker would not change")
        })
        .unwrap();

    store.reopen();
    assert_eq!(refused, Err("the marker would not change"));
    assert_eq!(
        recorded(&store, &old, record.kind, &record.name),
        Some(sorted(record))
    );
    assert_eq!(
        recorded(&store, &new, left.kind, &left.name),
        Some(sorted(left))
    );
    assert!(store.created(&old, "AGENTS.md").unwrap());
    assert_eq!(store.project_at(&old).unwrap(), Some(id));
    assert_eq!(store.project_at(&new).unwrap(), None, "{new}");
    assert_eq!(store.history(10).unwrap(), []);
}

fn a_relink_drops_what_another_project_left_at_the_new_route(mut store: Store) {
    let [old, new] = <[String; 2]>::try_from(RouteMother::distinct(2)).unwrap();
    let id = ProjectIdMother::random();
    let other = ProjectIdMother::random();
    store
        .knew(
            &[(id.clone(), old.clone()), (other.clone(), new.clone())],
            &[],
        )
        .unwrap();
    let left = InstallRecordMother::any();
    store.installed(&new, &left).unwrap();
    store.set_created(&new, "CLAUDE.md", true).unwrap();
    let record = InstallRecordMother::skill(&format!("{}-moved", left.name));
    store.installed(&old, &record).unwrap();

    let moved = store.relinked(&old, &new, &id, || Ok::<_, ()>(())).unwrap();

    store.reopen();
    assert_eq!(moved, Ok(((), 1)));
    assert_eq!(
        recorded(&store, &new, left.kind, &left.name),
        None,
        "{left:?}"
    );
    assert!(!store.created(&new, "CLAUDE.md").unwrap(), "{new}");
    assert_eq!(
        recorded(&store, &new, Kind::Skill, &record.name),
        Some(sorted(record))
    );
    assert_eq!(store.known_projects().unwrap(), [(id, new)]);
}

// ---------------------------------------------------------------------------
// The action log
// ---------------------------------------------------------------------------

fn the_log_reads_back_newest_first_as_far_as_it_is_asked(mut store: Store) {
    let count = random::between(5, 30);
    let mut written = Vec::new();
    for n in 0..count {
        let detail = format!("entry {n}");
        store
            .log(
                random::pick(&ACTIONS),
                None,
                Outcome::Ok,
                Some(detail.as_str()),
            )
            .unwrap();
        written.push(detail);
    }
    written.reverse();

    store.reopen();
    for limit in [0, 1, count / 2, count, count + 10] {
        let read: Vec<String> = store
            .history(limit)
            .unwrap()
            .into_iter()
            .map(|entry| entry.detail.expect("said something").into_string())
            .collect();
        assert_eq!(read, written[..limit.min(count)], "the latest {limit}");
    }
}

fn the_log_keeps_what_each_action_was_about_and_what_it_said(mut store: Store) {
    let mut written = Vec::new();
    for _ in 0..random::between(10, 40) {
        let action = random::pick(&ACTIONS);
        let outcome = random::pick(&[Outcome::Ok, Outcome::Failed]);
        let target = random::chance().then(|| random::text(40));
        let detail = random::chance().then(|| random::text(60));
        store
            .log(action, target.as_deref(), outcome, detail.as_deref())
            .unwrap();
        written.push((action, target, outcome, detail));
    }

    store.reopen();
    let history = store.history(written.len()).unwrap();
    assert_eq!(history.len(), written.len());
    for (entry, (action, target, outcome, detail)) in history.iter().rev().zip(&written) {
        assert_eq!(entry.action, action.slug());
        assert_eq!(entry.outcome, outcome.slug());
        assert_eq!(
            entry.target,
            target.as_deref().map(Redacted::new),
            "{target:?}"
        );
        assert_eq!(
            entry.detail,
            detail.as_deref().map(Redacted::new),
            "{detail:?}"
        );
    }
}

fn everything_a_store_stamps_is_stamped_by_the_clock_it_is_handed(store: Store) {
    let at = random::between(1_000_000_000, 2_000_000_000) as i64;
    let mut store = store.stamped_by(Arc::new(Stopped(at)));
    let from = source(&store, &UrlMother::https());
    store
        .record(&from, Kind::Skill, "deploy", "skills/deploy", None, None)
        .unwrap();
    store
        .log(Action::Gather, Some("deploy"), Outcome::Ok, None)
        .unwrap();
    let old = RouteMother::random();
    let id = ProjectIdMother::random();
    store
        .migrated(&old, &id, &[], &[(Action::Install, String::from("moved"))])
        .unwrap();
    store
        .relinked(&old, &format!("{old}-moved"), &id, || Ok::<_, ()>(()))
        .unwrap()
        .unwrap();

    store.reopen();
    assert_eq!(store.gathered().unwrap()[0].gathered_at, at);
    let stamps: Vec<i64> = store.history(10).unwrap().iter().map(|e| e.at).collect();
    assert_eq!(
        stamps,
        [at, at, at],
        "a log line, a migration's and a relink's"
    );
}

fn each_write_reads_the_clock_when_it_is_made(store: Store) {
    let start = random::between(1_700_000_000, 1_800_000_000) as i64;
    let clock = Ticking::from(start);
    let mut store = store.stamped_by(clock.clone());
    let count = random::between(3, 10);
    let mut told = Vec::new();
    for n in 0..count {
        told.push(clock.next());
        store
            .log(
                random::pick(&ACTIONS),
                None,
                Outcome::Ok,
                Some(format!("{n}").as_str()),
            )
            .unwrap();
    }
    let from = source(&store, &UrlMother::https());
    let before_record = clock.next();
    store
        .record(&from, Kind::Skill, "deploy", "skills/deploy", None, None)
        .unwrap();

    store.reopen();
    let stamps: Vec<i64> = store
        .history(count)
        .unwrap()
        .iter()
        .rev()
        .map(|e| e.at)
        .collect();
    assert_eq!(stamps, told, "each line stamped as it was written");
    let gathered_at = store.gathered().unwrap()[0].gathered_at;
    assert!(
        (before_record..clock.next()).contains(&gathered_at),
        "{gathered_at} is not a time the clock told the record"
    );
}

fn a_store_handed_no_clock_stamps_by_the_systems(mut store: Store) {
    let before = SystemClock.now();
    store.log(Action::Install, None, Outcome::Ok, None).unwrap();
    let after = SystemClock.now();

    store.reopen();
    let at = store.history(1).unwrap()[0].at;
    assert!(
        (before..=after).contains(&at),
        "{at} is not between {before} and {after}"
    );
}

fn a_store_is_read_on_a_thread_of_its_own(mut store: Store) {
    let record = InstallRecordMother::any();
    store.installed("collapse", &record).unwrap();
    store.reopen();
    let ledger = store.take();
    let (kind, name) = (record.kind, record.name.clone());

    let read = std::thread::spawn(move || ledger.installation("collapse", kind, &name).unwrap())
        .join()
        .expect("the thread read the store");

    assert_eq!(read.map(sorted), Some(sorted(record)));
}
