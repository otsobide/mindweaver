//! The [`LedgerStore`] a build keeps a workspace's records in: SQLite, in
//! `.mindweaver/mindweaver.db`, every query and every step of its schema.
//!
//! What it stores, and the promises it keeps of it (a URL kept redacted, a
//! write whole or not at all, a schema refused when it is newer than this
//! build reads), are the port's and [`crate::ledger`]'s; how, here. Why
//! SQLite is in docs/architecture.md ("Why the ledger is SQLite").

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::backup::{Backup, StepResult};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior};

use crate::adapter::SystemClock;
use crate::agent::Agent;
use crate::kind::Kind;
use crate::ledger::{
    Action, Carried, Entry, EntryTarget, Forgotten, FromSource, Gathered, InstallRecord,
    LedgerError, Merged, Moved, Outcome, Source, SourceKind, Taken, Target,
};
use crate::licence::Licence;
use crate::port::{Clock, LedgerStore};
use crate::settings::ProjectId;
use crate::url::{canonical, carries_credentials, redacted};
use crate::value::{ArtifactName, ContentHash, Origin, Pin, Redacted, Revision, SourceName};
use crate::workspace::RESERVED_IN_CACHE;

/// The schema version this build writes.
///
/// Refused if the file on disk is newer, for the same reason a marker file is:
/// an old binary silently misreading a newer database is the failure that has
/// no error message.
pub const SCHEMA_VERSION: u32 = 9;

/// The SQLite store, over one connection to the ledger's file, or to one in
/// memory.
#[derive(Debug)]
pub struct Sqlite {
    connection: Connection,
    path: PathBuf,
    /// What every row it writes is stamped by.
    clock: Arc<dyn Clock>,
}

impl Sqlite {
    /// [`Ledger::open`](crate::ledger::Ledger::open), over this store.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, LedgerError> {
        Self::open_knowing(path, Vec::new)
    }

    /// [`Ledger::open_knowing`](crate::ledger::Ledger::open_knowing), over this store.
    pub fn open_knowing(
        path: impl Into<PathBuf>,
        known: impl FnOnce() -> Vec<(ProjectId, String)>,
    ) -> Result<Self, LedgerError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| LedgerError::Create {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let connection = Connection::open(&path).map_err(|source| LedgerError::Open {
            path: path.clone(),
            source: source.into(),
        })?;
        // A connection that finds another one writing waits for it, as long
        // as a writer waits for the workspace, rather than failing with
        // "database is locked": the CLI, the TUI and the server can each have
        // the ledger open. rusqlite waits five seconds of its own accord, but
        // as a library's default; set here, the wait is Mindweaver's.
        connection
            .busy_timeout(crate::lock::WAIT)
            .map_err(|source| LedgerError::Open {
                path: path.clone(),
                source: source.into(),
            })?;
        let ledger = Self {
            connection,
            path,
            clock: Arc::new(SystemClock),
        };
        ledger.migrate(known)?;
        // And write-ahead logging, so that readers and a writer do not wait
        // for each other at all: a reader goes on reading the ledger as the
        // last write left it while the next is made. It needs every process
        // using the file on one machine, sharing memory through
        // `mindweaver.db-shm`, so a workspace on a network filesystem used
        // from two machines at once is beyond it, as it is beyond the
        // workspace lock.
        //
        // Only once `migrate` has accepted the version: the mode is kept in
        // the file's header, so switching first would change a ledger a newer
        // build wrote before refusing it. And never a reason to refuse the
        // ledger, since every mode keeps it whole: a filesystem that cannot
        // keep the log stays in the journal it had, where readers and writers
        // take turns instead, and a switch that finds another connection
        // writing fails at once, without the busy wait, because waiting there
        // could deadlock with it. That one is left to the next open.
        let _ = ledger
            .connection
            .pragma_update_and_check(None, "journal_mode", "wal", |_| Ok(()));
        Ok(ledger)
    }

    /// [`Ledger::open_existing`](crate::ledger::Ledger::open_existing), over this store.
    pub fn open_existing(path: impl Into<PathBuf>) -> Result<Option<Self>, LedgerError> {
        Self::open_existing_knowing(path, Vec::new)
    }

    /// [`Ledger::open_existing_knowing`](crate::ledger::Ledger::open_existing_knowing), over this store.
    pub fn open_existing_knowing(
        path: impl Into<PathBuf>,
        known: impl FnOnce() -> Vec<(ProjectId, String)>,
    ) -> Result<Option<Self>, LedgerError> {
        let path = path.into();
        if !path.is_file() {
            return Ok(None);
        }
        // Without `SQLITE_OPEN_CREATE`, so a file removed since the check is
        // an error to report rather than an empty database left in its place.
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection =
            Connection::open_with_flags(&path, flags).map_err(|source| LedgerError::Open {
                path: path.clone(),
                source: source.into(),
            })?;
        // Waiting for a writer as `open` does, but in whatever journal the
        // file keeps: changing it is a write, and a read makes none but a
        // migration.
        connection
            .busy_timeout(crate::lock::WAIT)
            .map_err(|source| LedgerError::Open {
                path: path.clone(),
                source: source.into(),
            })?;
        let ledger = Self {
            connection,
            path,
            clock: Arc::new(SystemClock),
        };
        match ledger.migrate(known) {
            Ok(()) => Ok(Some(ledger)),
            Err(error) if cannot_make_the_log(&ledger.path, &error) => {
                Self::open_immutable(&ledger.path).map(Some).ok_or(error)
            }
            Err(error) => Err(error),
        }
    }

    /// A ledger that keeps a write-ahead log, read where the log's files
    /// cannot be made, as a file nobody is changing.
    ///
    /// SQLite makes `mindweaver.db-wal` and `mindweaver.db-shm` to read a
    /// ledger that keeps its log, and takes them away when the last
    /// connection closes. So in a folder this process may read and not write
    /// (a sandboxed agent's view of the workspace, a read-only mount,
    /// somebody else's workspace), reading fails though nothing is to be
    /// written, where the journal the ledger had before let it be read.
    /// Called only when there is no log, which is when everything is in the
    /// file itself, and SQLite's `immutable` then reads it without the log or
    /// any lock. What somebody who can write there writes goes into a log
    /// this does not see, so this reads the ledger as their last checkpoint
    /// left it; a checkpoint made between two of its reads is the one thing
    /// that could change the file under it.
    ///
    /// `None` when it cannot be opened that way either, or migrating it would
    /// need a write, and the error that sent it here is the one to tell.
    fn open_immutable(path: &Path) -> Option<Self> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let connection = Connection::open_with_flags(immutable_uri(path)?, flags).ok()?;
        let ledger = Self {
            connection,
            path: path.to_path_buf(),
            clock: Arc::new(SystemClock),
        };
        ledger.migrate(Vec::new).ok()?;
        Some(ledger)
    }

    /// [`Ledger::in_memory`](crate::ledger::Ledger::in_memory), over this store.
    pub fn in_memory() -> Result<Self, LedgerError> {
        let connection = Connection::open_in_memory().map_err(|source| LedgerError::Open {
            path: PathBuf::from(":memory:"),
            source: source.into(),
        })?;
        let ledger = Self {
            connection,
            path: PathBuf::from(":memory:"),
            clock: Arc::new(SystemClock),
        };
        ledger.migrate(Vec::new)?;
        Ok(ledger)
    }

    /// Seconds since the Unix epoch, by its clock.
    fn now(&self) -> i64 {
        self.clock.now()
    }

    /// Bring an empty or older database up to [`SCHEMA_VERSION`].
    ///
    /// `user_version` rather than a table of our own: SQLite carries the
    /// integer in the file header, so reading it costs nothing and it cannot
    /// itself be missing from a database that was half created.
    ///
    /// A database that already holds something is [backed up](Self::back_up)
    /// first, whichever step it is about to take. A step to come is one more
    /// block below, and the copy is taken for it without its asking.
    ///
    /// `known` is asked only by the step to schema 5, for each project's id
    /// and route ([`open_knowing`](Self::open_knowing)), and not at all by a
    /// ledger already there, so opening a current one reads no project.
    fn migrate(&self, known: impl FnOnce() -> Vec<(ProjectId, String)>) -> Result<(), LedgerError> {
        self.connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|source| self.failed("enabling foreign keys", source))?;
        // Zeroes what a row leaves behind when it is rewritten or deleted.
        // Without it a URL the scrub rewrites would keep its credentials in
        // the file's free space, readable by anything that reads the bytes
        // rather than the rows, until SQLite happened to reuse the page.
        self.connection
            .pragma_update(None, "secure_delete", true)
            .map_err(|source| self.failed("turning on secure delete", source))?;

        // Read before taking the write lock, so that opening a database that
        // is already current (every open but the first after an upgrade)
        // writes nothing, and still works on a file it may only read.
        let found = self.schema_version()?;
        if found == SCHEMA_VERSION {
            return Ok(());
        }

        // Every step and the stamp in one transaction. Run one statement at a
        // time, a step that fails, or a process that dies halfway, would leave
        // a database half migrated under its old number, which the next open
        // would try to migrate again and fail on; this way it is left as it
        // was.
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(|source| self.failed("starting the migration", source))?;
        // Read again under the lock, because another process may have
        // migrated it in the meantime.
        let found = self.schema_version()?;
        if found == SCHEMA_VERSION {
            return Ok(());
        }
        // Copied under the lock too, not before it: a copy taken after the
        // first read could be of what another process has since migrated,
        // and would replace that process's copy of the old version with one of
        // the new. Holding the lock, this is the one process migrating, and
        // the file is at the version just read. Version 0 is a database just
        // created, with nothing in it to keep.
        if found > 0 {
            self.back_up(found)?;
        }

        // One block per version, applied in order, so a database created two
        // versions ago arrives at the same schema as one created today.
        if found < 1 {
            transaction
                .execute_batch(SCHEMA_V1)
                .map_err(|source| self.failed("creating the schema", source))?;
        }
        if found < 2 {
            transaction
                .execute_batch(SCHEMA_V2)
                .map_err(|source| self.failed("adding the installations table", source))?;
        }
        if found < 3 {
            transaction
                .execute_batch(SCHEMA_V3)
                .map_err(|source| self.failed("giving installations their targets", source))?;
        }
        if found < 4 {
            transaction.execute_batch(SCHEMA_V4).map_err(|source| {
                self.failed("giving each copy a hash and noting the files made", source)
            })?;
        }
        if found < 5 {
            let failed = |source| self.failed("naming each project by its id", source);
            transaction.execute_batch(SCHEMA_V5).map_err(failed)?;
            // What no SQL in the file can know: which id the project at a
            // route holds. Each is taken once, the first for an id or a
            // route two of them claim, which the caller leaves out anyway,
            // since two projects holding one id are a copy to tell apart,
            // not a record to fill in.
            for (id, route) in known() {
                let taken = transaction
                    .execute(
                        "INSERT OR IGNORE INTO projects (id, last_route) VALUES (?1, ?2)",
                        rusqlite::params![id.as_str(), route],
                    )
                    .map_err(failed)?;
                if taken > 0 {
                    transaction
                        .execute(
                            "UPDATE installations SET project_id = ?1 WHERE project = ?2",
                            rusqlite::params![id.as_str(), route],
                        )
                        .map_err(failed)?;
                }
            }
        }
        if found < 6 {
            transaction.execute_batch(SCHEMA_V6).map_err(|source| {
                self.failed("giving installations the entries they write", source)
            })?;
        }
        if found < 7 {
            let failed =
                |source| self.failed("naming each source and knowing it by its URL", source);
            // What no SQL can work out of a URL, handed to the step as
            // values, as the step to schema 5 is handed each project's id.
            let spelled = spelled_sources(&transaction).map_err(failed)?;
            seven(&transaction, &spelled).map_err(failed)?;
        }
        if found < 8 {
            transaction.execute_batch(SCHEMA_V8).map_err(|source| {
                self.failed(
                    "giving each gathered artifact where it came from and its licence",
                    source,
                )
            })?;
        }
        if found < 9 {
            transaction.execute_batch(SCHEMA_V9).map_err(|source| {
                self.failed("knowing a marketplace's plugins by their entries", source)
            })?;
        }

        transaction
            .pragma_update(None, "user_version", SCHEMA_VERSION)
            .map_err(|source| self.failed("stamping the schema version", source))?;
        transaction
            .commit()
            .map_err(|source| self.failed("finishing the migration", source))
    }

    /// Copy the database, at schema `version`, to [`backup_path`] before a
    /// step changes it.
    ///
    /// So that a step that turns out wrong, or a build that misreads what it
    /// wrote, leaves the file it started from to go back to: the copy opens at
    /// the old version, and an older Mindweaver reads it as it always did. One
    /// copy per version, replaced if an earlier attempt at the same step left
    /// one, since both are of the same file at the same version and the newer
    /// is the one the step is about to change. Nothing is migrated unless the
    /// copy is whole.
    ///
    /// Taken by SQLite's online backup rather than by copying the file:
    /// opening and closing the file outside SQLite while this connection
    /// holds it would drop the connection's locks on Unix, where they belong
    /// to the process. Read through a connection of its own, because this
    /// one holds the migration's write lock and SQLite will not copy from a
    /// connection in the middle of writing; reading beside that lock is
    /// allowed. Written to a temporary beside it and renamed, so an
    /// interrupted copy is never mistaken for a whole one, and made with the
    /// database's permissions before anything is written to it, so a ledger
    /// kept private is never readable through its copy, not even while the
    /// copy is being written.
    ///
    /// Called with the write lock held, which is what makes the temporary's
    /// one fixed name safe: no other process is copying, so whatever is
    /// there is what an interrupted attempt left.
    fn back_up(&self, version: u32) -> Result<(), LedgerError> {
        let backup = backup_path(&self.path, version);
        let failed = |source: io::Error| LedgerError::Backup {
            path: self.path.clone(),
            backup: backup.clone(),
            source,
        };
        let mut name = backup.file_name().unwrap_or_default().to_os_string();
        name.push(".tmp");
        let temporary = backup.with_file_name(name);

        // What an interrupted attempt left, its journal included, which
        // SQLite would otherwise replay into the fresh copy.
        for leftover in [temporary.clone(), journal_of(&temporary)] {
            match fs::remove_file(&leftover) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(failed(error)),
                _ => {}
            }
        }
        if let Err(error) = copy_database(&self.path, &temporary, version) {
            let _ = fs::remove_file(&temporary);
            let _ = fs::remove_file(journal_of(&temporary));
            return Err(failed(error));
        }
        // Exactly the database's once it is whole: it was made with the
        // owner's read and write added, and less whatever the umask took.
        if let Ok(metadata) = fs::metadata(&self.path) {
            let _ = fs::set_permissions(&temporary, metadata.permissions());
        }
        fs::rename(&temporary, &backup).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            failed(error)
        })
    }

    /// The schema version on disk, refused when it is newer than this build's.
    fn schema_version(&self) -> Result<u32, LedgerError> {
        let found: u32 = self
            .connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(|source| self.failed("reading the schema version", source))?;
        if found > SCHEMA_VERSION {
            return Err(LedgerError::Version {
                path: self.path.clone(),
                found,
            });
        }
        Ok(found)
    }

    /// A readable directory name no other source has taken, and none the
    /// cache keeps for core's own use ([`RESERVED_IN_CACHE`]).
    fn free_directory(&self, wanted: &str) -> Result<String, LedgerError> {
        for attempt in 1.. {
            let candidate = if attempt == 1 {
                wanted.to_owned()
            } else {
                format!("{wanted}-{attempt}")
            };
            if RESERVED_IN_CACHE.contains(&candidate.as_str()) {
                continue;
            }
            let taken = self
                .connection
                .query_row(
                    "SELECT 1 FROM sources WHERE directory = ?1
                     UNION ALL SELECT 1 FROM merged_sources WHERE directory = ?1",
                    [&candidate],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|source| self.failed("checking a source directory", source))?
                .is_some();
            if !taken {
                return Ok(candidate);
            }
        }
        unreachable!("the loop returns as soon as a name is free")
    }

    /// `wanted`, or the first of it numbered that no source is called.
    fn free_name(&self, wanted: &SourceName) -> Result<SourceName, LedgerError> {
        for attempt in 1.. {
            let candidate = if attempt == 1 {
                wanted.clone()
            } else {
                wanted.numbered(attempt)
            };
            if self.source_named(&candidate)?.is_none() {
                return Ok(candidate);
            }
        }
        unreachable!("the loop returns as soon as a name is free")
    }

    fn failed(&self, doing: &str, source: rusqlite::Error) -> LedgerError {
        LedgerError::Query {
            path: self.path.clone(),
            doing: doing.to_owned(),
            source: source.into(),
        }
    }
}

impl LedgerStore for Sqlite {
    fn stamp_by(&mut self, clock: Arc<dyn Clock>) {
        self.clock = clock;
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn source(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Option<Source>, LedgerError> {
        self.connection
            .query_row(
                &format!(
                    "SELECT {SOURCE_COLUMNS} FROM sources
                      WHERE kind = ?1 AND canonical_url = ?2 AND reference_key = ?3
                        AND subdirectory = ?4 AND parent_id IS NULL"
                ),
                rusqlite::params![
                    kind.slug(),
                    canonical(url),
                    reference.unwrap_or_default(),
                    subdirectory
                ],
                read_source,
            )
            .optional()
            .map_err(|source| self.failed("looking up a source", source))
    }

    fn source_for(
        &self,
        kind: SourceKind,
        url: &str,
        reference: Option<&str>,
        subdirectory: &str,
    ) -> Result<Source, LedgerError> {
        let url = redacted(url);
        if let Some(existing) = self.source(kind, &url, reference, subdirectory)? {
            return Ok(existing);
        }

        // What a dry run says it would be given, by the same rule.
        let (name, directory) = self.would_register(&url)?;
        let known_by = canonical(&url);
        let at = self.now();
        self.connection
            .execute(
                "INSERT INTO sources
                     (kind, url, reference, subdirectory, revision, directory, first_seen, last_seen,
                      name, canonical_url, reference_key)
                 VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    kind.slug(),
                    url.as_ref(),
                    reference,
                    subdirectory,
                    directory,
                    at,
                    name.as_str(),
                    known_by,
                    reference.unwrap_or_default()
                ],
            )
            .map_err(|source| self.failed("registering a source", source))?;

        Ok(Source {
            id: self.connection.last_insert_rowid(),
            kind,
            url: Redacted::new(&url),
            reference: reference.map(str::to_owned),
            subdirectory: subdirectory.to_owned(),
            revision: None,
            directory,
            name,
            pin: None,
            parent_id: None,
            parent: None,
            entry: None,
        })
    }

    fn child_for(
        &self,
        parent: &Source,
        entry: &str,
        url: &str,
        subdirectory: &str,
    ) -> Result<Source, LedgerError> {
        let failed = |source| self.failed("registering a marketplace's plugin", source);
        let url = redacted(url);
        let found = self
            .connection
            .query_row(
                &format!(
                    "SELECT {SOURCE_COLUMNS} FROM sources WHERE parent_id = ?1 AND entry = ?2"
                ),
                rusqlite::params![parent.id, entry],
                read_source,
            )
            .optional()
            .map_err(failed)?;
        if let Some(mut found) = found {
            // Where the catalog says it is today, which its entry, not its
            // place, is the identity of.
            if found.url != url.as_ref() || found.subdirectory != subdirectory {
                self.connection
                    .execute(
                        "UPDATE sources SET url = ?2, canonical_url = ?3, subdirectory = ?4
                          WHERE id = ?1",
                        rusqlite::params![found.id, url.as_ref(), canonical(&url), subdirectory],
                    )
                    .map_err(failed)?;
                found.url = Redacted::new(&url);
                found.subdirectory = subdirectory.to_owned();
            }
            return Ok(found);
        }

        // Called after its marketplace and its entry, `acme-tools.deploy`,
        // and filed beside the marketplace's folder the same way, so the
        // shelf reads as the catalog does.
        let wanted = SourceName::parse(&format!("{}.{entry}", parent.name))
            .unwrap_or_else(|| SourceName::derived(&canonical(&url)));
        let name = self.free_name(&wanted)?;
        let directory = self.free_directory(&format!("{}.{}", parent.directory, slug(entry)))?;
        let at = self.now();
        self.connection
            .execute(
                "INSERT INTO sources
                     (kind, url, reference, subdirectory, revision, directory, first_seen, last_seen,
                      name, canonical_url, reference_key, parent_id, entry)
                 VALUES (?1, ?2, NULL, ?3, NULL, ?4, ?5, ?5, ?6, ?7, '', ?8, ?9)",
                rusqlite::params![
                    SourceKind::Git.slug(),
                    url.as_ref(),
                    subdirectory,
                    directory,
                    at,
                    name.as_str(),
                    canonical(&url),
                    parent.id,
                    entry
                ],
            )
            .map_err(failed)?;
        Ok(Source {
            id: self.connection.last_insert_rowid(),
            kind: SourceKind::Git,
            url: Redacted::new(&url),
            reference: None,
            subdirectory: subdirectory.to_owned(),
            revision: None,
            directory,
            name,
            pin: None,
            parent_id: Some(parent.id),
            parent: Some(parent.name.clone()),
            entry: Some(entry.to_owned()),
        })
    }

    fn children(&self, parent: &Source) -> Result<Vec<Source>, LedgerError> {
        let failed = |source| self.failed("listing a marketplace's plugins", source);
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT {SOURCE_COLUMNS} FROM sources WHERE parent_id = ?1 ORDER BY entry"
            ))
            .map_err(failed)?;
        let rows = statement
            .query_map([parent.id], read_source)
            .map_err(failed)?;
        rows.collect::<Result<_, _>>().map_err(failed)
    }

    fn would_register(&self, url: &str) -> Result<(SourceName, String), LedgerError> {
        let url = redacted(url);
        Ok((
            self.free_name(&SourceName::derived(&canonical(&url)))?,
            self.free_directory(&slug(&url))?,
        ))
    }

    fn set_source_folder(&self, id: i64, subdirectory: &str) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "UPDATE sources SET subdirectory = ?2 WHERE id = ?1",
                rusqlite::params![id, subdirectory],
            )
            .map(|_| ())
            .map_err(|source| self.failed("moving a source to another folder", source))
    }

    fn source_named(&self, name: &SourceName) -> Result<Option<Source>, LedgerError> {
        self.connection
            .query_row(
                &format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE name = ?1"),
                [name.as_str()],
                read_source,
            )
            .optional()
            .map_err(|source| self.failed("looking up a source by its name", source))
    }

    fn rename_source(&self, id: i64, name: &SourceName) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "UPDATE sources SET name = ?2 WHERE id = ?1",
                rusqlite::params![id, name.as_str()],
            )
            .map(|_| ())
            .map_err(|source| self.failed("renaming a source", source))
    }

    fn merged(&self) -> Result<Vec<Merged>, LedgerError> {
        let failed = |source| self.failed("reading the sources made one", source);
        let pending: Vec<(String, Option<String>, String, i64)> = {
            let mut statement = self
                .connection
                .prepare(
                    "SELECT url, reference, directory, into_id FROM merged_sources ORDER BY id",
                )
                .map_err(failed)?;
            let rows = statement
                .query_map([], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
                })
                .map_err(failed)?;
            rows.collect::<Result<_, _>>().map_err(failed)?
        };
        let mut merged = Vec::new();
        for (url, reference, directory, into) in pending {
            merged.push(Merged {
                url: Redacted::new(&url),
                reference,
                directory,
                into: self
                    .connection
                    .query_row(
                        &format!("SELECT {SOURCE_COLUMNS} FROM sources WHERE id = ?1"),
                        [into],
                        read_source,
                    )
                    .map_err(failed)?,
            });
        }
        Ok(merged)
    }

    fn cleared_merge(&self, directory: &str) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "DELETE FROM merged_sources WHERE directory = ?1",
                [directory],
            )
            .map(|_| ())
            .map_err(|source| self.failed("forgetting a source made one", source))
    }

    fn saw_source(
        &self,
        id: i64,
        revision: Option<&str>,
        pin: Option<&Pin>,
    ) -> Result<(), LedgerError> {
        self.connection
            .execute(
                "UPDATE sources SET revision = ?2, last_seen = ?3, pin = ?4 WHERE id = ?1",
                rusqlite::params![id, revision, self.now(), pin.map(Pin::as_str)],
            )
            .map(|_| ())
            .map_err(|source| self.failed("updating a source", source))
    }

    fn forget_source(&self, id: i64) -> Result<bool, LedgerError> {
        self.connection
            .execute("DELETE FROM sources WHERE id = ?1", [id])
            .map(|deleted| deleted > 0)
            .map_err(|source| self.failed("forgetting a source", source))
    }

    fn record(&self, source: &Source, taken: &Taken) -> Result<(), LedgerError> {
        let (licence_text, licence_file) = match &taken.licence {
            Licence::Manifest { text } | Licence::Plugin { text } => (Some(text.as_str()), None),
            Licence::File { file, text } => (text.as_deref(), Some(file.as_str())),
            Licence::Unknown => (None, None),
        };
        self.connection
            .execute(
                "INSERT INTO artifacts (kind, name, source_id, path, summary, revision, gathered_at,
                                        upstream_path, content_hash, licence_from, licence,
                                        licence_file)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT (kind, name, source_id) DO UPDATE SET
                     path = excluded.path,
                     summary = excluded.summary,
                     revision = excluded.revision,
                     gathered_at = excluded.gathered_at,
                     upstream_path = excluded.upstream_path,
                     content_hash = excluded.content_hash,
                     licence_from = excluded.licence_from,
                     licence = excluded.licence,
                     licence_file = excluded.licence_file",
                rusqlite::params![
                    taken.kind.slug(),
                    taken.name,
                    source.id,
                    taken.path,
                    taken.summary,
                    taken.revision,
                    self.now(),
                    taken.upstream_path,
                    taken.content_hash,
                    taken.licence.slug(),
                    licence_text,
                    licence_file
                ],
            )
            .map(|_| ())
            .map_err(|error| self.failed("recording an artifact", error))
    }

    fn forget_gathered(
        &self,
        source: &Source,
        kind: Kind,
        name: &str,
    ) -> Result<bool, LedgerError> {
        self.connection
            .execute(
                "DELETE FROM artifacts WHERE source_id = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![source.id, kind.slug(), name],
            )
            .map(|deleted| deleted > 0)
            .map_err(|error| self.failed("forgetting an artifact", error))
    }

    fn shelf_moved(&self, from: &str, to: &str) -> Result<usize, LedgerError> {
        // Compared as a prefix of bytes, never with LIKE, whose `%` and `_`
        // a folder's name may hold.
        self.connection
            .execute(
                "UPDATE artifacts SET path = ?2 || substr(path, length(?1) + 1)
                  WHERE substr(path, 1, length(?1)) = ?1",
                rusqlite::params![from, to],
            )
            .map_err(|error| self.failed("moving what was gathered", error))
    }

    fn gathered(&self) -> Result<Vec<Gathered>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT a.kind, a.name, a.path, a.summary, a.revision, a.gathered_at,
                        s.id, s.kind, s.url, s.reference, s.subdirectory, s.revision, s.directory,
                        s.name, s.pin, a.upstream_path, a.content_hash, a.licence_from, a.licence,
                        a.licence_file, s.parent_id, s.entry, p.name
                   FROM artifacts a
                   JOIN sources s ON s.id = a.source_id
                   LEFT JOIN sources p ON p.id = s.parent_id
                  ORDER BY a.kind, a.name, s.url",
            )
            .map_err(|source| self.failed("listing what was gathered", source))?;

        // The two slug columns are read as the strings they are and turned
        // into types afterwards, so a row naming a kind this build has never
        // heard of is skipped rather than failing the whole listing. Opening
        // refuses a newer database outright, so such a row was edited by hand.
        let rows = statement
            .query_map([], |row| {
                Ok(Row {
                    kind: row.get(0)?,
                    name: row.get(1)?,
                    path: row.get(2)?,
                    summary: row.get(3)?,
                    revision: row.get(4)?,
                    gathered_at: row.get(5)?,
                    source_id: row.get(6)?,
                    source_kind: row.get(7)?,
                    url: row.get(8)?,
                    reference: row.get(9)?,
                    subdirectory: row.get(10)?,
                    source_revision: row.get(11)?,
                    directory: row.get(12)?,
                    source_name: row.get(13)?,
                    pin: row.get(14)?,
                    upstream_path: row.get(15)?,
                    content_hash: row.get(16)?,
                    licence: read_licence(row.get(17)?, row.get(18)?, row.get(19)?),
                    parent_id: row.get(20)?,
                    entry: row.get(21)?,
                    parent: row.get(22)?,
                })
            })
            .map_err(|source| self.failed("listing what was gathered", source))?;

        let mut found = Vec::new();
        for row in rows {
            let row = row.map_err(|source| self.failed("reading a gathered artifact", source))?;
            let (Ok(kind), Some(source_kind)) = (
                row.kind.parse::<Kind>(),
                SourceKind::parse(&row.source_kind),
            ) else {
                continue;
            };
            found.push(Gathered {
                kind,
                name: ArtifactName::from(row.name),
                path: row.path,
                summary: row.summary,
                revision: row.revision,
                gathered_at: row.gathered_at,
                source: Source {
                    id: row.source_id,
                    kind: source_kind,
                    url: Redacted::new(&row.url),
                    reference: row.reference,
                    subdirectory: row.subdirectory,
                    revision: row.source_revision,
                    directory: row.directory,
                    name: SourceName::recorded(row.source_name),
                    pin: row.pin.map(Pin::recorded),
                    parent_id: row.parent_id,
                    parent: row.parent.map(SourceName::recorded),
                    entry: row.entry,
                },
                upstream_path: row.upstream_path,
                content_hash: row.content_hash,
                licence: row.licence,
            });
        }
        // By kind in report order, a kind's declaration order, which the
        // query sorts by its slug in the alphabet's: stable, so each kind
        // keeps the query's order by name, then source. Nothing told the
        // two apart while skills were all a shelf held (#111).
        found.sort_by_key(|gathered| gathered.kind);
        Ok(found)
    }

    fn sources(&self) -> Result<Vec<Source>, LedgerError> {
        let failed = |source| self.failed("listing the sources", source);
        let mut statement = self
            .connection
            .prepare(&format!("SELECT {SOURCE_COLUMNS} FROM sources ORDER BY id"))
            .map_err(failed)?;
        let rows = statement
            .query_map([], |row| {
                let slug: String = row.get(1)?;
                Ok(SourceKind::parse(&slug).map(|_| read_source(row)))
            })
            .map_err(failed)?;
        let mut sources = Vec::new();
        for row in rows {
            if let Some(source) = row.map_err(failed)? {
                sources.push(source.map_err(failed)?);
            }
        }
        // Each marketplace's plugins straight after it, by their entries,
        // so a listing reads as the catalog's tree: what a refresh or a
        // removal of it reaches is what is under it.
        let (mut listed, children): (Vec<Source>, Vec<Source>) = sources
            .into_iter()
            .partition(|source| source.parent_id.is_none());
        listed.sort_by(|a: &Source, b: &Source| {
            (&a.url, &a.reference, &a.subdirectory).cmp(&(&b.url, &b.reference, &b.subdirectory))
        });
        let mut ordered = Vec::with_capacity(listed.len() + children.len());
        for source in listed {
            let id = source.id;
            ordered.push(source);
            let mut under: Vec<&Source> = children
                .iter()
                .filter(|child| child.parent_id == Some(id))
                .collect();
            under.sort_by(|a, b| a.entry.cmp(&b.entry));
            ordered.extend(under.into_iter().cloned());
        }
        Ok(ordered)
    }

    fn installed_from(&self, source: &Source) -> Result<Vec<FromSource>, LedgerError> {
        let failed = |error| self.failed("reading what was installed from a source", error);
        let mut statement = self
            .connection
            .prepare(
                "SELECT project, kind, name FROM installations
                  WHERE source_id = ?1
                  ORDER BY project, kind, name",
            )
            .map_err(failed)?;
        let rows = statement
            .query_map([source.id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(failed)?;
        let mut installed = Vec::new();
        for row in rows {
            let (project, kind, name) = row.map_err(failed)?;
            // A kind this build has never heard of is passed over, as a
            // listing of the shelf passes one over.
            if let Ok(kind) = kind.parse::<Kind>() {
                installed.push(FromSource {
                    project,
                    kind,
                    name: ArtifactName::from(name),
                });
            }
        }
        // In report order of the kinds rather than by their words.
        installed.sort_by(|a, b| (&a.project, a.kind, &a.name).cmp(&(&b.project, b.kind, &b.name)));
        Ok(installed)
    }

    fn installed(&self, project: &str, installation: &InstallRecord) -> Result<(), LedgerError> {
        let failed = |source| self.failed("recording an installation", source);
        // An origin spells a source's URL, so it goes in as every URL does.
        let origin = installation.origin.as_deref().map(redacted);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let id: i64 = transaction
            .query_row(
                "INSERT INTO installations
                     (project, kind, name, source_id, origin, content_hash, installed_at,
                      project_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                         (SELECT id FROM projects WHERE last_route = ?1))
                 ON CONFLICT (project, kind, name) DO UPDATE SET
                     source_id = excluded.source_id,
                     origin = excluded.origin,
                     content_hash = excluded.content_hash,
                     installed_at = excluded.installed_at,
                     project_id = excluded.project_id
                 RETURNING id",
                rusqlite::params![
                    project,
                    installation.kind.slug(),
                    installation.name,
                    installation.source_id,
                    origin.as_deref(),
                    installation.content_hash,
                    self.now()
                ],
                |row| row.get(0),
            )
            .map_err(failed)?;
        transaction
            .execute(
                "DELETE FROM installation_targets WHERE installation_id = ?1",
                [id],
            )
            .map_err(failed)?;
        insert_targets(&transaction, id, &installation.targets).map_err(failed)?;
        replace_entries(&transaction, id, &installation.entries).map_err(failed)?;
        transaction.commit().map_err(failed)
    }

    fn rendered(
        &self,
        project: &str,
        kind: Kind,
        records: &[InstallRecord],
        files: &[(&str, bool)],
    ) -> Result<(), LedgerError> {
        let failed = |source| self.failed("recording what a render wrote", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let recorded: Vec<(i64, String)> = {
            let mut statement = transaction
                .prepare("SELECT id, name FROM installations WHERE project = ?1 AND kind = ?2")
                .map_err(failed)?;
            let rows = statement
                .query_map(rusqlite::params![project, kind.slug()], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })
                .map_err(failed)?;
            rows.collect::<Result<_, _>>().map_err(failed)?
        };
        for (id, name) in recorded {
            if !records.iter().any(|record| record.name == name) {
                transaction
                    .execute("DELETE FROM installations WHERE id = ?1", [id])
                    .map_err(failed)?;
            }
        }
        for record in records {
            let origin = record.origin.as_deref().map(redacted);
            let id: i64 = transaction
                .query_row(
                    "INSERT INTO installations
                         (project, kind, name, source_id, origin, content_hash, installed_at,
                          project_id)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
                             (SELECT id FROM projects WHERE last_route = ?1))
                     ON CONFLICT (project, kind, name) DO UPDATE SET
                         source_id = excluded.source_id,
                         origin = excluded.origin,
                         content_hash = excluded.content_hash,
                         project_id = excluded.project_id
                     RETURNING id",
                    rusqlite::params![
                        project,
                        kind.slug(),
                        record.name,
                        record.source_id,
                        origin.as_deref(),
                        record.content_hash,
                        self.now()
                    ],
                    |row| row.get(0),
                )
                .map_err(failed)?;
            transaction
                .execute(
                    "DELETE FROM installation_targets WHERE installation_id = ?1",
                    [id],
                )
                .map_err(failed)?;
            insert_targets(&transaction, id, &record.targets).map_err(failed)?;
            replace_entries(&transaction, id, &record.entries).map_err(failed)?;
        }
        for (path, created) in files {
            set_created_in(&transaction, self.now(), project, path, *created).map_err(failed)?;
        }
        transaction.commit().map_err(failed)
    }

    fn installation(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
    ) -> Result<Option<InstallRecord>, LedgerError> {
        let failed = |source| self.failed("reading an installation", source);
        let Some((id, source_id, origin, content_hash)) = self
            .connection
            .query_row(
                "SELECT id, source_id, origin, content_hash FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get(3)?,
                    ))
                },
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(None);
        };
        // And comes out as every URL does, for one schema 3 filled in from a
        // source's URL as an older build stored it, until the scrub rewrites it.
        let origin = origin.map(|text| Origin::from(redacted(&text)));

        let mut statement = self
            .connection
            .prepare(
                "SELECT agent, path, content_hash FROM installation_targets
                  WHERE installation_id = ?1 ORDER BY path",
            )
            .map_err(failed)?;
        let targets = statement
            .query_map([id], |row| {
                Ok(Target {
                    agent: row.get(0)?,
                    path: row.get(1)?,
                    content_hash: row.get(2)?,
                })
            })
            .map_err(failed)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(failed)?;
        let entries = entries_of(
            &self.connection,
            "installation_entries.installation_id = ?1",
            [id],
        )
        .map_err(failed)?
        .into_iter()
        .map(|(_, entry)| entry)
        .collect();

        Ok(Some(InstallRecord {
            kind,
            name: ArtifactName::from(name),
            source_id,
            origin,
            content_hash,
            targets,
            entries,
        }))
    }

    fn uninstalled(&self, project: &str, kind: Kind, name: &str) -> Result<bool, LedgerError> {
        self.connection
            .execute(
                "DELETE FROM installations WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
            )
            .map(|rows| rows > 0)
            .map_err(|source| self.failed("forgetting an installation", source))
    }

    fn uninstalled_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
    ) -> Result<Forgotten, LedgerError> {
        let failed = |source| self.failed("forgetting an installed copy", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let Some(id) = transaction
            .query_row(
                "SELECT id FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(Forgotten::Nothing);
        };
        let copy = transaction
            .execute(
                "DELETE FROM installation_targets
                  WHERE installation_id = ?1 AND path = ?2",
                rusqlite::params![id, path],
            )
            .map_err(failed)?;
        let left: i64 = transaction
            .query_row(
                "SELECT count(*) FROM installation_targets WHERE installation_id = ?1",
                [id],
                |row| row.get(0),
            )
            .map_err(failed)?;
        let forgotten = if left == 0 && (copy > 0 || stale_without_copies(kind)) {
            transaction
                .execute("DELETE FROM installations WHERE id = ?1", [id])
                .map_err(failed)?;
            Forgotten::Installation
        } else if copy > 0 {
            Forgotten::Copy
        } else {
            Forgotten::Nothing
        };
        transaction.commit().map_err(failed)?;
        Ok(forgotten)
    }

    fn renamed_copy(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        path: &str,
        to_name: &str,
        to_path: &str,
    ) -> Result<Carried, LedgerError> {
        let failed = |source| self.failed("recording a renamed copy", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        let Some(id) = transaction
            .query_row(
                "SELECT id FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), name],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(failed)?
        else {
            return Ok(Carried::Nothing);
        };
        let count = |transaction: &Transaction<'_>| -> rusqlite::Result<i64> {
            transaction.query_row(
                "SELECT count(*) FROM installation_targets WHERE installation_id = ?1",
                [id],
                |row| row.get(0),
            )
        };
        let copy: Option<(Option<String>, Option<String>)> = transaction
            .query_row(
                "SELECT agent, content_hash FROM installation_targets
                  WHERE installation_id = ?1 AND path = ?2",
                rusqlite::params![id, path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(failed)?;
        let Some((agent, held)) = copy else {
            if stale_without_copies(kind) && count(&transaction).map_err(failed)? == 0 {
                transaction
                    .execute("DELETE FROM installations WHERE id = ?1", [id])
                    .map_err(failed)?;
                transaction.commit().map_err(failed)?;
                return Ok(Carried::Forgotten);
            }
            return Ok(Carried::Nothing);
        };
        transaction
            .execute(
                "DELETE FROM installation_targets
                  WHERE installation_id = ?1 AND path = ?2",
                rusqlite::params![id, path],
            )
            .map_err(failed)?;
        let last = count(&transaction).map_err(failed)? == 0;
        let taken = transaction
            .query_row(
                "SELECT 1 FROM installations
                  WHERE project = ?1 AND kind = ?2 AND name = ?3",
                rusqlite::params![project, kind.slug(), to_name],
                |_| Ok(()),
            )
            .optional()
            .map_err(failed)?
            .is_some();
        let carried = if taken {
            if last {
                transaction
                    .execute("DELETE FROM installations WHERE id = ?1", [id])
                    .map_err(failed)?;
            }
            Carried::Forgotten
        } else {
            let to = if last {
                transaction
                    .execute(
                        "UPDATE installations SET name = ?1 WHERE id = ?2",
                        rusqlite::params![to_name, id],
                    )
                    .map_err(failed)?;
                id
            } else {
                transaction
                    .query_row(
                        "INSERT INTO installations
                             (project, kind, name, source_id, origin, content_hash, installed_at,
                              project_id)
                         SELECT project, kind, ?1, source_id, origin, content_hash, installed_at,
                                project_id
                           FROM installations WHERE id = ?2
                         RETURNING id",
                        rusqlite::params![to_name, id],
                        |row| row.get::<_, i64>(0),
                    )
                    .map_err(failed)?
            };
            transaction
                .execute(
                    "INSERT INTO installation_targets (installation_id, agent, path, content_hash)
                     VALUES (?1, ?2, ?3, ?4)",
                    rusqlite::params![to, agent, to_path, held],
                )
                .map_err(failed)?;
            Carried::Moved
        };
        transaction.commit().map_err(failed)?;
        Ok(carried)
    }

    fn retargeted(
        &self,
        project: &str,
        kind: Kind,
        name: &str,
        targets: &[Target],
    ) -> Result<bool, LedgerError> {
        let failed = |source| self.failed("recording where an installation's copies are", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        if retarget_in(&transaction, project, kind, name, targets)
            .map_err(failed)?
            .is_none()
        {
            return Ok(false);
        }
        transaction.commit().map_err(failed)?;
        Ok(true)
    }

    fn migrated(
        &self,
        project: &str,
        id: &ProjectId,
        moved: &[Moved],
        log: &[(Action, String)],
    ) -> Result<(), LedgerError> {
        let failed = |source| self.failed("recording a project's move to its settings", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;
        know_in(&transaction, id, project, &[]).map_err(failed)?;
        for one in moved {
            let id = retarget_in(&transaction, project, one.kind, &one.name, &one.targets)
                .map_err(failed)?;
            if let (Some(id), Some(hash)) = (id, &one.content_hash) {
                transaction
                    .execute(
                        "UPDATE installations SET content_hash = ?1
                          WHERE id = ?2 AND content_hash IS NULL",
                        rusqlite::params![hash, id],
                    )
                    .map_err(failed)?;
            }
        }
        for (action, detail) in log {
            insert_log(
                &transaction,
                self.now(),
                *action,
                Some(project),
                Outcome::Ok,
                Some(detail),
            )
            .map_err(failed)?;
        }
        transaction.commit().map_err(failed)
    }

    fn installations(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT name FROM installations
                  WHERE project = ?1 AND kind = ?2 ORDER BY name",
            )
            .map_err(|source| self.failed("reading installations", source))?;
        let rows = statement
            .query_map(rusqlite::params![project, kind.slug()], |row| row.get(0))
            .map_err(|source| self.failed("reading installations", source))?;
        rows.collect::<Result<Vec<String>, _>>()
            .map_err(|source| self.failed("reading installations", source))
    }

    fn records(&self, project: &str, kind: Kind) -> Result<Vec<InstallRecord>, LedgerError> {
        let failed = |source| self.failed("reading the installations", source);
        let mut statement = self
            .connection
            .prepare(
                "SELECT installations.id, installations.name, installations.source_id,
                        installations.origin, installations.content_hash,
                        installation_targets.agent, installation_targets.path,
                        installation_targets.content_hash
                   FROM installations
                   LEFT JOIN installation_targets
                     ON installation_targets.installation_id = installations.id
                  WHERE installations.project = ?1 AND installations.kind = ?2
                  ORDER BY installations.name, installation_targets.path",
            )
            .map_err(failed)?;
        let rows = statement
            .query_map(rusqlite::params![project, kind.slug()], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<ContentHash>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<ContentHash>>(7)?,
                ))
            })
            .map_err(failed)?;
        let mut records: Vec<(i64, InstallRecord)> = Vec::new();
        for row in rows {
            let (id, name, source_id, origin, content_hash, agent, path, held) =
                row.map_err(failed)?;
            // One row a copy, an installation's together, since a name is
            // one installation a project and the rows come in name order.
            if records.last().is_none_or(|(last, _)| *last != id) {
                records.push((
                    id,
                    InstallRecord {
                        kind,
                        name: ArtifactName::from(name),
                        source_id,
                        // Out as every URL comes out, as `installation` has it.
                        origin: origin.map(|text| Origin::from(redacted(&text))),
                        content_hash,
                        targets: Vec::new(),
                        entries: Vec::new(),
                    },
                ));
            }
            // None for an installation recording no copy, which the join
            // still gives a row.
            if let (Some(path), Some((_, record))) = (path, records.last_mut()) {
                record.targets.push(Target {
                    agent,
                    path,
                    content_hash: held,
                });
            }
        }
        // Asked apart rather than joined beside the targets, which would
        // give a row for each pair of a copy and an entry.
        let entries = entries_of(
            &self.connection,
            "installations.project = ?1 AND installations.kind = ?2",
            rusqlite::params![project, kind.slug()],
        )
        .map_err(failed)?;
        for (id, entry) in entries {
            if let Some((_, record)) = records.iter_mut().find(|(at, _)| *at == id) {
                record.entries.push(entry);
            }
        }
        Ok(records.into_iter().map(|(_, record)| record).collect())
    }

    fn copies(&self, project: &str, kind: Kind) -> Result<Vec<String>, LedgerError> {
        let failed = |source| self.failed("reading the copies installed", source);
        let mut statement = self
            .connection
            .prepare(
                "SELECT DISTINCT installation_targets.path
                   FROM installation_targets
                   JOIN installations
                     ON installations.id = installation_targets.installation_id
                  WHERE installations.project = ?1 AND installations.kind = ?2
                  ORDER BY installation_targets.path",
            )
            .map_err(failed)?;
        let rows = statement
            .query_map(rusqlite::params![project, kind.slug()], |row| row.get(0))
            .map_err(failed)?;
        rows.collect::<Result<Vec<String>, _>>().map_err(failed)
    }

    fn created(&self, project: &str, path: &str) -> Result<bool, LedgerError> {
        self.connection
            .query_row(
                "SELECT 1 FROM created_files WHERE project = ?1 AND path = ?2",
                rusqlite::params![project, path],
                |_| Ok(()),
            )
            .optional()
            .map(|found| found.is_some())
            .map_err(|source| self.failed("reading which files Mindweaver made", source))
    }

    fn set_created(&self, project: &str, path: &str, created: bool) -> Result<(), LedgerError> {
        set_created_in(&self.connection, self.now(), project, path, created)
            .map_err(|source| self.failed("noting which files Mindweaver made", source))
    }

    fn project_at(&self, route: &str) -> Result<Option<ProjectId>, LedgerError> {
        self.connection
            .query_row(
                "SELECT id FROM projects WHERE last_route = ?1",
                [route],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map(|id| id.as_deref().and_then(ProjectId::parse))
            .map_err(|source| self.failed("reading which project a route holds", source))
    }

    fn known_projects(&self) -> Result<Vec<(ProjectId, String)>, LedgerError> {
        let failed = |source| self.failed("reading the projects known by their id", source);
        let mut statement = self
            .connection
            .prepare("SELECT id, last_route FROM projects ORDER BY last_route")
            .map_err(failed)?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(failed)?;
        let mut known = Vec::new();
        for row in rows {
            let (id, route) = row.map_err(failed)?;
            if let Some(id) = ProjectId::parse(&id) {
                known.push((id, route));
            }
        }
        Ok(known)
    }

    fn holds(&self, route: &str) -> Result<bool, LedgerError> {
        self.connection
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM installations WHERE project = ?1)
                     OR EXISTS (SELECT 1 FROM created_files WHERE project = ?1)",
                [route],
                |row| row.get::<_, bool>(0),
            )
            .map_err(|source| self.failed("reading what is recorded of a project", source))
    }

    fn knew(&self, seen: &[(ProjectId, String)], stale: &[String]) -> Result<usize, LedgerError> {
        let failed = |source| self.failed("recording which project each route holds", source);
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(failed)?;
        let mut learned = 0;
        for (id, route) in seen {
            if know_in(&transaction, id, route, stale).map_err(failed)? {
                learned += 1;
            }
        }
        transaction.commit().map_err(failed)?;
        Ok(learned)
    }

    fn relinked_by(
        &self,
        old: &str,
        new: &str,
        id: &ProjectId,
        write: &mut dyn FnMut() -> bool,
    ) -> Result<Option<usize>, LedgerError> {
        let failed = |source| self.failed("moving a project's records to where it is", source);
        let transaction =
            Transaction::new_unchecked(&self.connection, TransactionBehavior::Immediate)
                .map_err(failed)?;
        transaction
            .execute(
                "DELETE FROM installations WHERE project = ?1 AND project_id IS NOT ?2",
                rusqlite::params![new, id.as_str()],
            )
            .map_err(failed)?;
        transaction
            .execute("DELETE FROM created_files WHERE project = ?1", [new])
            .map_err(failed)?;
        transaction
            .execute(
                "DELETE FROM projects WHERE last_route = ?1 AND id != ?2",
                rusqlite::params![new, id.as_str()],
            )
            .map_err(failed)?;
        // Known before the records name it, which they may only once it is.
        transaction
            .execute(
                "INSERT INTO projects (id, last_route) VALUES (?1, ?2)
                 ON CONFLICT (id) DO UPDATE SET last_route = excluded.last_route",
                rusqlite::params![id.as_str(), new],
            )
            .map_err(failed)?;
        let moved = transaction
            .execute(
                "UPDATE installations SET project = ?2, project_id = ?3 WHERE project = ?1",
                rusqlite::params![old, new, id.as_str()],
            )
            .map_err(failed)?;
        transaction
            .execute(
                "UPDATE created_files SET project = ?2 WHERE project = ?1",
                rusqlite::params![old, new],
            )
            .map_err(failed)?;
        insert_log(
            &transaction,
            self.now(),
            Action::Relink,
            Some(new),
            Outcome::Ok,
            Some(&format!("{old} to {new}")),
        )
        .map_err(failed)?;
        if !write() {
            // Dropped, so nothing above is kept.
            return Ok(None);
        }
        transaction.commit().map_err(failed)?;
        Ok(Some(moved))
    }

    fn tainted_sources(&self) -> Result<Vec<Source>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT {SOURCE_COLUMNS} FROM sources WHERE instr(url, '@') > 0 ORDER BY id"
            ))
            .map_err(|source| self.failed("looking for credentials in sources", source))?;
        let rows = statement
            .query_map([], |row| {
                let stored: String = row.get(2)?;
                Ok((carries_credentials(&stored), read_source(row)?))
            })
            .map_err(|source| self.failed("looking for credentials in sources", source))?;

        let mut tainted = Vec::new();
        for row in rows {
            let (carries, source) =
                row.map_err(|source| self.failed("looking for credentials in sources", source))?;
            if carries {
                tainted.push(source);
            }
        }
        Ok(tainted)
    }

    fn scrub_source(&self, id: i64) -> Result<(), LedgerError> {
        let failed = |source| self.failed("rewriting a source without credentials", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;

        let Some(stored) = transaction
            .query_row("SELECT url FROM sources WHERE id = ?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(failed)?
        else {
            return Ok(());
        };
        let clean = redacted(&stored);
        if clean == stored {
            return Ok(());
        }

        let origins: Vec<(i64, String)> = {
            let mut statement = transaction
                .prepare(
                    "SELECT id, origin FROM installations
                      WHERE source_id = ?1 AND instr(origin, '@') > 0",
                )
                .map_err(failed)?;
            let rows = statement
                .query_map([id], |row| Ok((row.get(0)?, row.get(1)?)))
                .map_err(failed)?;
            rows.collect::<Result<_, _>>().map_err(failed)?
        };
        for (installation, origin) in origins {
            let clean = redacted(&origin);
            if clean != origin {
                transaction
                    .execute(
                        "UPDATE installations SET origin = ?2 WHERE id = ?1",
                        rusqlite::params![installation, clean.as_ref()],
                    )
                    .map_err(failed)?;
            }
        }
        transaction
            .execute(
                "UPDATE sources SET url = ?2 WHERE id = ?1",
                rusqlite::params![id, clean.as_ref()],
            )
            .map_err(failed)?;
        transaction.commit().map_err(failed)
    }

    fn scrub_log(&self) -> Result<usize, LedgerError> {
        let failed = |source| self.failed("rewriting the action log without credentials", source);
        let transaction = self.connection.unchecked_transaction().map_err(failed)?;

        let rows: Vec<(i64, Option<String>, Option<String>)> = {
            let mut statement = transaction
                .prepare(
                    "SELECT id, target, detail FROM actions
                      WHERE instr(target, '@') > 0 OR instr(detail, '@') > 0",
                )
                .map_err(failed)?;
            let rows = statement
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .map_err(failed)?;
            rows.collect::<Result<_, _>>().map_err(failed)?
        };

        let mut scrubbed = 0;
        for (id, target, detail) in rows {
            let clean_target = target.as_deref().map(redacted);
            let clean_detail = detail.as_deref().map(redacted);
            if clean_target.as_deref() == target.as_deref()
                && clean_detail.as_deref() == detail.as_deref()
            {
                continue;
            }
            transaction
                .execute(
                    "UPDATE actions SET target = ?2, detail = ?3 WHERE id = ?1",
                    rusqlite::params![id, clean_target.as_deref(), clean_detail.as_deref()],
                )
                .map_err(failed)?;
            scrubbed += 1;
        }
        transaction.commit().map_err(failed)?;
        Ok(scrubbed)
    }

    fn log(
        &self,
        action: Action,
        target: Option<&str>,
        outcome: Outcome,
        detail: Option<&str>,
    ) -> Result<(), LedgerError> {
        insert_log(
            &self.connection,
            self.now(),
            action,
            target,
            outcome,
            detail,
        )
        .map_err(|source| self.failed("writing to the action log", source))
    }

    fn history(&self, limit: usize) -> Result<Vec<Entry>, LedgerError> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT at, action, target, outcome, detail
                   FROM actions ORDER BY id DESC LIMIT ?1",
            )
            .map_err(|source| self.failed("reading the action log", source))?;
        let rows = statement
            .query_map([limit as i64], |row| {
                let target: Option<String> = row.get(2)?;
                let detail: Option<String> = row.get(4)?;
                Ok(Entry {
                    at: row.get(0)?,
                    action: row.get(1)?,
                    target: target.map(|text| Redacted::new(&text)),
                    outcome: row.get(3)?,
                    detail: detail.map(|text| Redacted::new(&text)),
                })
            })
            .map_err(|source| self.failed("reading the action log", source))?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|source| self.failed("reading the action log", source))
    }
}

/// [`Ledger::knew`](crate::ledger::Ledger::knew) of one project, through `connection`, which a caller
/// holds a transaction on: whether the ledger did not know `id` at `route`
/// already.
fn know_in(
    connection: &Connection,
    id: &ProjectId,
    route: &str,
    stale: &[String],
) -> rusqlite::Result<bool> {
    let at: Option<String> = connection
        .query_row(
            "SELECT last_route FROM projects WHERE id = ?1",
            [id.as_str()],
            |row| row.get(0),
        )
        .optional()?;
    match at {
        Some(at) if at == route || stale.contains(&at) => return Ok(false),
        Some(_) => {
            connection.execute("DELETE FROM projects WHERE id = ?1", [id.as_str()])?;
        }
        None => {}
    }
    connection.execute("DELETE FROM projects WHERE last_route = ?1", [route])?;
    connection.execute(
        "INSERT INTO projects (id, last_route) VALUES (?1, ?2)",
        rusqlite::params![id.as_str(), route],
    )?;
    connection.execute(
        "UPDATE installations SET project_id = ?1 WHERE project = ?2",
        rusqlite::params![id.as_str(), route],
    )?;
    Ok(true)
}

/// Give the installation of `name` the copies `targets` in place of those
/// it records, or forget it when `targets` is empty, through `connection`,
/// which a caller holds a transaction on: its id, or `None` when nothing
/// records it.
fn retarget_in(
    connection: &Connection,
    project: &str,
    kind: Kind,
    name: &str,
    targets: &[Target],
) -> rusqlite::Result<Option<i64>> {
    let Some(id) = connection
        .query_row(
            "SELECT id FROM installations
              WHERE project = ?1 AND kind = ?2 AND name = ?3",
            rusqlite::params![project, kind.slug(), name],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    else {
        return Ok(None);
    };
    if targets.is_empty() {
        connection.execute("DELETE FROM installations WHERE id = ?1", [id])?;
        return Ok(Some(id));
    }
    connection.execute(
        "DELETE FROM installation_targets WHERE installation_id = ?1",
        [id],
    )?;
    insert_targets(connection, id, targets)?;
    Ok(Some(id))
}

/// Whether an installation of `kind` recording no copy is forgotten whole by
/// a change at a path in its project that reached none of its copies, as
/// [`uninstalled_copy`](LedgerStore::uninstalled_copy) and
/// [`renamed_copy`](LedgerStore::renamed_copy) are asked.
///
/// A skill's is one only a build from before installations had targets
/// wrote, held to a change by name, as records were. A rule's is one a build
/// writes, for a rule that reaches no agent the project names, and an MCP
/// server's never records a copy, since what it writes is an entry in each
/// agent's config file (D-10), which no change at a path reaches: either way
/// the change reached nothing it installed, and it stays. Forgetting a
/// server's would take its entries' record with it, and leave what
/// Mindweaver wrote there somebody's, never updated or taken out again. On
/// every kind with no wildcard, so a kind added says which it is.
fn stale_without_copies(kind: Kind) -> bool {
    match kind {
        Kind::Skill => true,
        Kind::Rule | Kind::Mcp => false,
    }
}

/// Record `targets` as the copies of the installation `id`, through
/// `connection`, which a caller holds a transaction on, and which holds none
/// of them yet.
fn insert_targets(connection: &Connection, id: i64, targets: &[Target]) -> rusqlite::Result<()> {
    for target in targets {
        connection.execute(
            "INSERT INTO installation_targets (installation_id, agent, path, content_hash)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![id, target.agent, target.path, target.content_hash],
        )?;
    }
    Ok(())
}

/// Record `entries` as the entries of the installation `id`, in place of
/// those it recorded, through `connection`, which a caller holds a
/// transaction on.
fn replace_entries(
    connection: &Connection,
    id: i64,
    entries: &[EntryTarget],
) -> rusqlite::Result<()> {
    connection.execute(
        "DELETE FROM installation_entries WHERE installation_id = ?1",
        [id],
    )?;
    for entry in entries {
        connection.execute(
            "INSERT INTO installation_entries
                 (installation_id, agent, file, name, fingerprint)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                id,
                entry.agent.name(),
                entry.file,
                entry.name,
                entry.fingerprint
            ],
        )?;
    }
    Ok(())
}

/// Every entry of the installations `filter` picks, a condition on
/// `installations` and `installation_entries` taking `params`, each beside
/// its installation's id, in agent order: one naming an agent this build
/// does not know is passed over, as a row naming a kind it does not know is.
fn entries_of(
    connection: &Connection,
    filter: &str,
    params: impl rusqlite::Params,
) -> rusqlite::Result<Vec<(i64, EntryTarget)>> {
    let mut statement = connection.prepare(&format!(
        "SELECT installation_entries.installation_id, installation_entries.agent,
                installation_entries.file, installation_entries.name,
                installation_entries.fingerprint
           FROM installation_entries
           JOIN installations ON installations.id = installation_entries.installation_id
          WHERE {filter}
          ORDER BY installation_entries.installation_id, installation_entries.agent"
    ))?;
    let rows = statement.query_map(params, |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, ContentHash>(4)?,
        ))
    })?;
    let mut entries = Vec::new();
    for row in rows {
        let (id, agent, file, name, fingerprint) = row?;
        let Ok(agent) = agent.parse::<Agent>() else {
            continue;
        };
        entries.push((
            id,
            EntryTarget {
                agent,
                file,
                name: ArtifactName::from(name),
                fingerprint,
            },
        ));
    }
    entries.sort_by(|(a, one), (b, other)| (a, one.agent).cmp(&(b, other.agent)));
    Ok(entries)
}

/// [`Ledger::set_created`](crate::ledger::Ledger::set_created), through `connection`, which may hold a
/// transaction a caller keeps it in.
fn set_created_in(
    connection: &Connection,
    at: i64,
    project: &str,
    path: &str,
    created: bool,
) -> rusqlite::Result<()> {
    if created {
        connection.execute(
            "INSERT INTO created_files (project, path, created_at) VALUES (?1, ?2, ?3)
             ON CONFLICT (project, path) DO NOTHING",
            rusqlite::params![project, path, at],
        )?;
    } else {
        connection.execute(
            "DELETE FROM created_files WHERE project = ?1 AND path = ?2",
            rusqlite::params![project, path],
        )?;
    }
    Ok(())
}

/// Append one entry to the action log through `connection`, every URL in
/// what it is about and what it says [`redacted`], as [`Ledger::log`](crate::ledger::Ledger::log) does.
fn insert_log(
    connection: &Connection,
    at: i64,
    action: Action,
    target: Option<&str>,
    outcome: Outcome,
    detail: Option<&str>,
) -> rusqlite::Result<()> {
    let target = target.map(redacted);
    let detail = detail.map(redacted);
    connection
        .execute(
            "INSERT INTO actions (at, action, target, outcome, detail)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![
                at,
                action.slug(),
                target.as_deref(),
                outcome.slug(),
                detail.as_deref()
            ],
        )
        .map(|_| ())
}

/// One joined row, before its slugs mean anything.
struct Row {
    kind: String,
    name: String,
    path: String,
    summary: Option<String>,
    revision: Option<Revision>,
    gathered_at: i64,
    source_id: i64,
    source_kind: String,
    url: String,
    reference: Option<String>,
    subdirectory: String,
    source_revision: Option<Revision>,
    directory: String,
    source_name: String,
    pin: Option<String>,
    upstream_path: Option<String>,
    content_hash: Option<ContentHash>,
    licence: Option<Licence>,
    parent_id: Option<i64>,
    entry: Option<String>,
    parent: Option<String>,
}

/// Where a ledger at `path` is copied before a schema step takes it on from
/// `version`: beside it, as `mindweaver.db.v2.bak` for version 2.
///
/// Named after the version it holds rather than numbered or dated, so which
/// build reads a copy is in its name, and a ledger brought forward by several
/// builds in turn keeps one copy for each version it had, side by side.
pub fn backup_path(path: &Path, version: u32) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".v{version}.bak"));
    path.with_file_name(name)
}

/// Copy the database at `from`, at schema `version`, into a new one at `to`,
/// closed once it is whole.
fn copy_database(from: &Path, to: &Path, version: u32) -> Result<(), io::Error> {
    // Read-only and never created: a file gone since it was opened is an
    // error, not an empty database to copy.
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let source = Connection::open_with_flags(from, flags).map_err(io::Error::other)?;
    create_like(to, from)?;
    let mut copy = Connection::open(to).map_err(io::Error::other)?;
    {
        let copying = Backup::new(&source, &mut copy).map_err(io::Error::other)?;
        // Every page in one step, so the copy is one moment of the database
        // rather than pages from either side of a write.
        match copying.step(-1).map_err(io::Error::other)? {
            StepResult::Done => {}
            other => {
                return Err(io::Error::other(format!(
                    "something else was writing to it ({other:?}), so try again"
                )))
            }
        }
    }
    // What the copy is for is opening at the old version, so that is checked
    // rather than assumed: a file that is not the one being migrated is no
    // way back from migrating it.
    let copied: u32 = copy
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(io::Error::other)?;
    if copied != version {
        return Err(io::Error::other(format!(
            "the copy is at schema version {copied}, not {version}, so it is not of the file being migrated"
        )));
    }
    Ok(())
}

/// Create `path` empty, no more readable than the file at `like`, for SQLite
/// to copy that file into.
///
/// Made with that mode rather than given it once the copy is written: SQLite
/// creates a file anyone the umask allows may read, and whoever opened it
/// before its mode changed could read the whole ledger through that handle
/// afterwards. The owner may always read and write it, which SQLite needs
/// and nobody else gains by. SQLite opens an empty file as an empty
/// database, and gives the journal it keeps beside it the same mode.
fn create_like(path: &Path, like: &Path) -> Result<(), io::Error> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode((fs::metadata(like)?.permissions().mode() & 0o777) | 0o600);
    }
    // Elsewhere a new file takes its folder's access rules, and the one
    // flag left, read-only, is given to the copy once it is whole.
    #[cfg(not(unix))]
    let _ = like;
    options.open(path).map(drop)
}

/// The rollback journal SQLite keeps beside a database while writing it.
/// Whether `error` is SQLite failing to make the files a write-ahead log
/// needs beside the ledger at `path`, with no log there to read.
fn cannot_make_the_log(path: &Path, error: &LedgerError) -> bool {
    let LedgerError::Query { source, .. } = error else {
        return false;
    };
    let Some(rusqlite::Error::SqliteFailure(failure, _)) = source.downcast_ref() else {
        return false;
    };
    let mut log = path.file_name().unwrap_or_default().to_os_string();
    log.push("-wal");
    matches!(
        failure.code,
        rusqlite::ErrorCode::ReadOnly | rusqlite::ErrorCode::CannotOpen
    ) && !path.with_file_name(log).exists()
}

/// `path` as the URI SQLite opens as [`immutable`](Sqlite::open_immutable):
/// `None` for one that is not UTF-8, which a URI cannot spell.
///
/// Only what SQLite reads as part of the URI's syntax is escaped (`%`, and
/// `?` and `#`, which would end the path), since it takes everything else
/// as it is. An absolute path goes after an empty authority, `file:///`, so
/// one that starts with two slashes is not read as naming a host; on Windows
/// a slash goes before it too (`file:///C:/...`), which SQLite drops again.
fn immutable_uri(path: &Path) -> Option<String> {
    let mut uri = String::from("file:");
    if path.is_absolute() {
        uri.push_str("//");
        if cfg!(windows) {
            uri.push('/');
        }
    }
    for character in path.to_str()?.chars() {
        match character {
            '%' => uri.push_str("%25"),
            '?' => uri.push_str("%3F"),
            '#' => uri.push_str("%23"),
            other => uri.push(other),
        }
    }
    uri.push_str("?immutable=1");
    Some(uri)
}

fn journal_of(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push("-journal");
    path.with_file_name(name)
}

/// The columns of `sources` [`read_source`] reads, in its order.
const SOURCE_COLUMNS: &str =
    "id, kind, url, reference, subdirectory, revision, directory, name, pin, parent_id, entry,
     (SELECT parent.name FROM sources AS parent WHERE parent.id = sources.parent_id)";

/// What a `SELECT` of [`SOURCE_COLUMNS`] maps to.
fn read_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<Source> {
    let slug: String = row.get(1)?;
    // No path of ours can write a kind this build does not know: opening
    // refuses a newer database outright. A row edited by hand still can, so it
    // is reported rather than guessed at.
    let kind = SourceKind::parse(&slug).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            format!("unknown source kind `{slug}`").into(),
        )
    })?;
    let url: String = row.get(2)?;
    Ok(Source {
        id: row.get(0)?,
        kind,
        url: Redacted::new(&url),
        reference: row.get(3)?,
        subdirectory: row.get(4)?,
        revision: row.get(5)?,
        directory: row.get(6)?,
        name: SourceName::recorded(row.get(7)?),
        pin: row.get::<_, Option<String>>(8)?.map(Pin::recorded),
        parent_id: row.get(9)?,
        entry: row.get(10)?,
        parent: row.get::<_, Option<String>>(11)?.map(SourceName::recorded),
    })
}

/// A licence as an artifact's row holds it, in the three columns schema 8
/// gave it: `None` for a row from before them, and for one whose `from`
/// was edited by hand into a word this build does not know, which is no
/// licence anybody declared.
fn read_licence(
    from: Option<String>,
    text: Option<String>,
    file: Option<String>,
) -> Option<Licence> {
    match (from?.as_str(), file) {
        ("manifest", _) => Some(Licence::Manifest {
            text: text.unwrap_or_default(),
        }),
        ("file", Some(file)) => Some(Licence::File { file, text }),
        // A plugin's, which a skill a marketplace's plugin holds is under
        // when it declares none (#103), kept in the column a manifest's is.
        ("plugin", _) => Some(Licence::Plugin {
            text: text.unwrap_or_default(),
        }),
        ("unknown", _) => Some(Licence::Unknown),
        _ => None,
    }
}

/// A URL as a readable directory name.
///
/// The scheme, any credentials and a trailing `.git` carry nothing a person
/// browsing `.mindweaver/skills/` wants to read, and anything that is not a
/// plain character becomes a hyphen, so the result is a name every filesystem
/// accepts and nobody has to quote.
fn slug(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    // `git@github.com:acme/skills` and `https://user:token@host/x` both put
    // what is worth keeping after the last `@`.
    let rest = rest.rsplit_once('@').map_or(rest, |(_, rest)| rest);
    let rest = rest.strip_suffix(".git").unwrap_or(rest);

    let mut out = String::new();
    for c in rest.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        String::from("source")
    } else {
        trimmed.to_owned()
    }
}

/// A hash as the ledger stores it: the text it is written as, in a `TEXT`
/// column. Here, in the ledger, rather than beside the value: the value
/// knows nothing of SQLite, and a store of another kind would say how it
/// keeps one in its own module, as this one does.
impl rusqlite::ToSql for ContentHash {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
    }
}

/// A name as the ledger stores it, the text it is.
impl rusqlite::ToSql for ArtifactName {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
    }
}

/// A name as the ledger holds it: any text, as a name read from anywhere is.
impl rusqlite::types::FromSql for ArtifactName {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        String::column_result(value).map(ArtifactName::from)
    }
}

/// A revision as the ledger stores it, the text the remote named it by.
impl rusqlite::ToSql for Revision {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(rusqlite::types::ToSqlOutput::from(self.as_str()))
    }
}

/// A revision as the ledger holds it.
impl rusqlite::types::FromSql for Revision {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        String::column_result(value).map(Revision::from)
    }
}

/// A hash as the ledger holds it, taken as written ([`ContentHash::recorded`]).
impl rusqlite::types::FromSql for ContentHash {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        String::column_result(value).map(ContentHash::recorded)
    }
}

/// The first schema.
///
/// Timestamps are Unix seconds. `ON DELETE CASCADE` on the artifacts means
/// forgetting a source forgets what it brought, which is the only sane reading
/// of dropping one.
const SCHEMA_V1: &str = "
CREATE TABLE sources (
    id           INTEGER PRIMARY KEY,
    kind         TEXT    NOT NULL,
    url          TEXT    NOT NULL,
    reference    TEXT,
    subdirectory TEXT    NOT NULL,
    revision     TEXT,
    directory    TEXT    NOT NULL UNIQUE,
    first_seen   INTEGER NOT NULL,
    last_seen    INTEGER NOT NULL,
    UNIQUE (kind, url, reference, subdirectory)
);

CREATE TABLE artifacts (
    id          INTEGER PRIMARY KEY,
    kind        TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
    path        TEXT    NOT NULL,
    summary     TEXT,
    revision    TEXT,
    gathered_at INTEGER NOT NULL,
    UNIQUE (kind, name, source_id)
);

CREATE TABLE actions (
    id      INTEGER PRIMARY KEY,
    at      INTEGER NOT NULL,
    action  TEXT    NOT NULL,
    target  TEXT,
    outcome TEXT    NOT NULL,
    detail  TEXT
);

CREATE INDEX artifacts_by_name ON artifacts (kind, name);
CREATE INDEX actions_by_time ON actions (at);
";

/// The second schema: what has been put into a mind project.
///
/// `project` is stored relative to the workspace, the way a registered project
/// is, so moving the workspace and its projects together keeps the record
/// true. `source_id` is nulled rather than cascaded when a source is
/// forgotten: the file is still in the project, and saying "installed, origin
/// no longer known" is truer than pretending it was never installed.
const SCHEMA_V2: &str = "
CREATE TABLE installations (
    id           INTEGER PRIMARY KEY,
    project      TEXT    NOT NULL,
    kind         TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    source_id    INTEGER REFERENCES sources(id) ON DELETE SET NULL,
    path         TEXT    NOT NULL,
    installed_at INTEGER NOT NULL,
    UNIQUE (project, kind, name)
);

CREATE INDEX installations_by_project ON installations (project);
";

/// The third schema: one installation, a copy in each place its agents read
/// (D-07), and enough about what it wrote to tell it apart later (D-04).
///
/// An installation keeps one row, and every copy it wrote is a target of it:
/// installed, updated and removed together, since an agent that reads two of
/// the copies (OpenCode reads `.agents/skills` and `.claude/skills`) must
/// never see two versions. A target's `path` is relative to the project and
/// `/` separated, as the installation's own was, which goes: kept in both
/// places, the two could disagree. `agent` names the one agent a copy is for,
/// and is empty for a copy no one agent owns: the folder a v2 project's
/// marker names, or `.agents/skills`, which two agents read. One row per path,
/// so such a copy is never recorded twice.
///
/// `origin` is where the installation came from, spelled as `--from` takes
/// it, which the workspace's own and a loaded workspace's had no other way to
/// say: their `source_id` is empty. `content_hash` is [`crate::hash::tree`]
/// of what was written, once for the installation rather than once a target,
/// because every target is written from one copy and is compared with it.
///
/// What version 2 recorded becomes one target at the path it had. Its origin
/// is filled in where it points at a source, spelled as `--from` takes a
/// source's offer at this version (the address, with `#` and the branch when
/// one was asked for), and here rather than through the code that spells it
/// now, so a later change to that spelling cannot change what this step
/// writes. An install from the workspace or a load gets none, since version 2
/// kept nothing that says which. No hash is made up for a copy nobody hashed
/// when it was written: D-04 reads an empty one as current only while the
/// copy matches its offer byte for byte.
const SCHEMA_V3: &str = "
CREATE TABLE installation_targets (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT,
    path            TEXT    NOT NULL,
    UNIQUE (installation_id, path)
);

INSERT INTO installation_targets (installation_id, agent, path)
    SELECT id, NULL, path FROM installations;

ALTER TABLE installations DROP COLUMN path;
ALTER TABLE installations ADD COLUMN origin TEXT;
ALTER TABLE installations ADD COLUMN content_hash TEXT;

UPDATE installations
   SET origin = (SELECT s.url || coalesce('#' || s.reference, '')
                   FROM sources s
                  WHERE s.id = installations.source_id)
 WHERE source_id IS NOT NULL;
";

/// The fourth schema: what installing a rule writes (#45).
///
/// A rule reaches a project marked by its settings as a section of the block
/// in its `AGENTS.md` and, for Claude, as a file in its rules folder
/// (`.claude/rules/mindweaver`), which hold different bytes: the section is the
/// rule without its front matter, the file the rule as written. So a target
/// gains a `content_hash` of its own, which is what tells a section or a file
/// edited since from the one written (D-04). A skill's copies leave it empty:
/// every one of them holds the tree the installation's `content_hash` names,
/// which is why version 3 kept one hash an installation.
///
/// `created_files` names each file Mindweaver made in a project, by its
/// route from the project, rather than wrote into: an `AGENTS.md` that was
/// not there until a rule was installed. The block is taken out of a file
/// with the last rule, and only a file Mindweaver made goes with it, so
/// whether it made the file is kept until then. A table of its own rather
/// than a column of an installation's, since one file serves every rule of
/// the project, and outlives each of them but the last.
///
/// Nothing older is filled in: no build before this one installed a rule,
/// and every target it recorded is a skill's.
const SCHEMA_V4: &str = "
ALTER TABLE installation_targets ADD COLUMN content_hash TEXT;

CREATE TABLE created_files (
    project    TEXT    NOT NULL,
    path       TEXT    NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE (project, path)
);
";

/// The fifth schema: a project known by its id, so that one that moves is
/// found again (#57).
///
/// Every record names its project by its route from the workspace, which
/// moving the project changes: its entry goes stale, and its records name a
/// place nothing is at. `projects` keeps the id ([`ProjectId`]) the ledger
/// last saw at each route, one id a route and one route an id, so a project
/// found somewhere else holding an id the ledger last saw at a stale entry
/// is that entry's project, moved, and `mindweaver relink` takes its records
/// along ([`Ledger::relinked`](crate::ledger::Ledger::relinked)). `installations.project_id` says which id
/// each record was made for, the route's when it was written, emptied when
/// the ledger stops knowing that id and moved with it.
///
/// The route stays the key, rather than giving way to the id: a project a
/// `mind.toml` marks has no id, and keeps being found by its route as
/// before, and so does one whose id nothing has recorded yet.
///
/// What a version before it held is filled in by the step, but not from
/// the file, which kept no id: from the projects the workspace links, each
/// opened for the id its settings hold ([`Ledger::open_knowing`](crate::ledger::Ledger::open_knowing)), the one
/// fact no SQL could know. A project a `mind.toml` marks, an entry that
/// would not open, and two projects holding one id get none, and their
/// records keep their route alone.
const SCHEMA_V5: &str = "
CREATE TABLE projects (
    id         TEXT    PRIMARY KEY,
    last_route TEXT    NOT NULL UNIQUE
);

ALTER TABLE installations
    ADD COLUMN project_id TEXT REFERENCES projects(id) ON UPDATE CASCADE ON DELETE SET NULL;

CREATE INDEX installations_by_project_id ON installations (project_id);
";

/// The sixth schema: the entries an MCP server's installation writes into
/// its agents' config files (#93).
///
/// An MCP server is installed as one entry in each agent's own config file
/// (D-10), `.mcp.json`, `.codex/config.toml` or OpenCode's, never as a
/// copy, so its installation has no target: an entry is found by the key it
/// is under in a file every server of the project shares, not by a path of
/// its own, and a copy's `path` would name the same file for every server.
/// `installation_entries` keeps one row an entry: the agent it is for, the
/// file, as a route from the project, the key, and the `fingerprint` of
/// what was written there ([`crate::hash::fingerprint`]), which is what
/// tells an entry untouched from one edited since (D-04) and the one thing
/// that says it is Mindweaver's, since an entry can carry no mark. One
/// entry an agent, since each agent reads one file, and gone with the
/// installation, as its targets go.
///
/// Where the server's definition came from is the installation's `origin`,
/// which every installation has kept since schema 3 and which an entry is
/// held to what is offered now by, so this step adds nothing for it.
///
/// Nothing older is filled in: no build before this one installed an MCP
/// server, and every installation it recorded is a skill's or a rule's.
const SCHEMA_V6: &str = "
CREATE TABLE installation_entries (
    installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
    agent           TEXT    NOT NULL,
    file            TEXT    NOT NULL,
    name            TEXT    NOT NULL,
    fingerprint     TEXT    NOT NULL,
    UNIQUE (installation_id, agent)
);
";

/// The seventh schema: a gathered source is called by a name and known by
/// its URL however that is spelled (#98, D-13, closing #7).
///
/// A source was its URL as typed, so `https://github.com/acme/skills` and
/// `git@github.com:acme/skills.git` were two sources of one repository,
/// each offering every skill under an origin of its own, and the `UNIQUE`
/// of version 1 held nothing at the remote's HEAD, where `reference` is
/// `NULL`, which SQLite never finds equal to another. `canonical_url` is
/// the URL every spelling of it is ([`crate::url::canonical`]), which holds
/// no credential, and `reference_key` the branch, empty for the remote's
/// HEAD, so the identity behind `sources_by_identity` is never `NULL`.
/// `name` is what a listing shows and `--from` takes ([`SourceName`]), one
/// a source. `pin` and `parent_id` are for what a source is pinned to (#99)
/// and the marketplace it is listed by (#103): no build of this schema
/// reads or writes either, and a gather writes `pin` from schema 8 on.
///
/// The columns go on the end of the table rather than into a new one,
/// which would have to be copied in under foreign keys that cascade a
/// dropped `sources` into every artifact. The old `UNIQUE` stays, and holds
/// nothing the new index does not.
///
/// `merged_sources` holds each row the step made one with another
/// ([`Merged`]): the URL it was gathered from without a credential, its
/// branch, the folder name it had and the source it is one with, until the
/// first command that writes has said it and removed its folders
/// ([`LedgerStore::merged`]). Kept in the file rather than handed back by
/// the opening that took the step, which is a read as often as not and says
/// nothing; and while a row is there its folder name is given to no new
/// source, whose clone would otherwise be removed as the merge's. Forgotten
/// with the source it is one with, which `gather forget` removes only once
/// it has opened the ledger to write, and so said the merge.
const SCHEMA_V7: &str = "
ALTER TABLE sources ADD COLUMN name TEXT NOT NULL DEFAULT '';
ALTER TABLE sources ADD COLUMN canonical_url TEXT NOT NULL DEFAULT '';
ALTER TABLE sources ADD COLUMN reference_key TEXT NOT NULL DEFAULT '';
ALTER TABLE sources ADD COLUMN pin TEXT;
ALTER TABLE sources ADD COLUMN parent_id INTEGER REFERENCES sources(id) ON DELETE CASCADE;

UPDATE sources SET reference_key = coalesce(reference, '');

CREATE TABLE merged_sources (
    id        INTEGER PRIMARY KEY,
    directory TEXT    NOT NULL UNIQUE,
    url       TEXT    NOT NULL,
    reference TEXT,
    into_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE
);
";

/// What the seventh schema holds once every row has its identity and name.
const SCHEMA_V7_INDEXES: &str = "
CREATE UNIQUE INDEX sources_by_identity
    ON sources (kind, canonical_url, reference_key, subdirectory);
CREATE UNIQUE INDEX sources_by_name ON sources (name);
";

/// What the step to schema 7 is handed of one source: what no SQL can work
/// out of its URL, by today's rules.
struct Spelled {
    id: i64,
    /// The URL every spelling of its repository is ([`canonical`]), the
    /// rule a gather finds a source by.
    identity: String,
    /// The name a gather would give it ([`SourceName::derived`]), before
    /// it is numbered for one another source takes first.
    name: String,
    /// Its URL without a credential ([`redacted`]).
    clean: String,
}

/// What [`seven`] is handed of every source a ledger of an earlier schema
/// holds, read through `transaction` before the step changes anything.
///
/// Worked out here rather than inside the step, which writes it in with SQL
/// of its own, as the step to schema 5 writes the ids it is handed: no SQL
/// can tell that two URLs are one repository's, nor take a credential out
/// of one. It is what today's rules say of each row, and once written it is
/// what a gather finds the row by: a build that changes a rule takes a step
/// of its own to write the rows again.
fn spelled_sources(transaction: &Transaction<'_>) -> rusqlite::Result<Vec<Spelled>> {
    let mut statement = transaction.prepare("SELECT id, url FROM sources ORDER BY id")?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get::<_, String>(1)?)))?;
    rows.map(|row| {
        row.map(|(id, url)| {
            let identity = canonical(&url);
            Spelled {
                id,
                name: SourceName::derived(&identity).to_string(),
                clean: redacted(&url).into_owned(),
                identity,
            }
        })
    })
    .collect()
}

/// The step to schema 7, through `transaction`, `spelled` being what no SQL
/// can work out of each source's URL ([`spelled_sources`]).
///
/// What a version before it held is filled in by its own SQL from what it
/// is handed, as the step to schema 5 writes in the ids it is handed: each
/// row's identity, and its name, in the order the sources were first
/// gathered, the later of two that would take one numbered.
///
/// Rows that turn out to be one place, two spellings of one URL at one
/// branch from one folder, become one before the identity is made unique,
/// as the scrub of #25 makes two one whose URLs only a credential told
/// apart: the one whose shelf was filled most recently is kept, since its
/// folder holds the newer files, and one whose gather failed before it took
/// anything loses however recently it was tried. What the other gathered
/// goes with it, and every installation from it is from the kept one now,
/// its origin rewritten to the kept one's as version 3 spelled an origin,
/// without the credential the kept one's URL may hold, so an update finds
/// where it came from offered there and no row is given a credential it
/// did not hold. Each row that went is kept in `merged_sources`
/// ([`Merged`]), for the first command that writes to say and to remove its
/// folders, which nothing points into any more.
///
/// A credential is left where the scrub finds it, which every command that
/// writes runs ([`crate::gather::scrub`]): it rewrites a clone before its
/// row, and a row this rewrote first would no longer say that its clone
/// still holds the credential. The identity, the name and an origin this
/// writes never hold one.
fn seven(transaction: &Transaction<'_>, spelled: &[Spelled]) -> rusqlite::Result<()> {
    transaction.execute_batch(SCHEMA_V7)?;
    let of = |id: i64| {
        spelled
            .iter()
            .find(|spelled| spelled.id == id)
            .ok_or(rusqlite::Error::QueryReturnedNoRows)
    };
    for spelled in spelled {
        transaction.execute(
            "UPDATE sources SET canonical_url = ?2 WHERE id = ?1",
            rusqlite::params![spelled.id, spelled.identity],
        )?;
    }

    // Each identity's rows together, the freshest first: the newest
    // harvest, a shelf never filled last, then the last attempt, then the
    // later row.
    let ranked: Vec<(i64, [String; 4])> = {
        let mut statement = transaction.prepare(
            "SELECT id, kind, canonical_url, reference_key, subdirectory
               FROM sources
              ORDER BY kind, canonical_url, reference_key, subdirectory,
                       (SELECT MAX(gathered_at) FROM artifacts WHERE source_id = sources.id) DESC,
                       last_seen DESC, id DESC",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get(0)?,
                [row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?],
            ))
        })?;
        rows.collect::<Result<_, _>>()?
    };
    // Each row that goes, as it was, and the row it is one with.
    let mut pairs: Vec<(i64, Option<String>, String, i64)> = Vec::new();
    let mut kept: Option<&(i64, [String; 4])> = None;
    for row in &ranked {
        match kept {
            Some((into, identity)) if *identity == row.1 => {
                let (reference, directory) = transaction.query_row(
                    "SELECT reference, directory FROM sources WHERE id = ?1",
                    [row.0],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                pairs.push((row.0, reference, directory, *into));
            }
            _ => kept = Some(row),
        }
    }
    for (gone, reference, directory, into) in &pairs {
        // Said as it was gathered, which a script may still name it by.
        transaction.execute(
            "INSERT INTO merged_sources (directory, url, reference, into_id)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![directory, of(*gone)?.clean, reference, into],
        )?;
        let reference: Option<String> = transaction.query_row(
            "SELECT reference FROM sources WHERE id = ?1",
            [into],
            |row| row.get(0),
        )?;
        let clean = &of(*into)?.clean;
        let origin = match reference {
            Some(reference) => format!("{clean}#{reference}"),
            None => clean.clone(),
        };
        transaction.execute(
            "UPDATE installations SET source_id = ?1, origin = ?3 WHERE source_id = ?2",
            rusqlite::params![into, gone, origin],
        )?;
        // What it gathered goes with it, by the cascade.
        transaction.execute("DELETE FROM sources WHERE id = ?1", [gone])?;
    }

    let remaining: Vec<i64> = {
        let mut statement = transaction.prepare("SELECT id FROM sources ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut taken: Vec<String> = Vec::new();
    for id in remaining {
        let wanted = &of(id)?.name;
        let name = std::iter::once(wanted.clone())
            .chain((2..).map(|n| format!("{wanted}-{n}")))
            .find(|name| !taken.contains(name))
            .expect("one of endlessly many names is free");
        transaction.execute(
            "UPDATE sources SET name = ?2 WHERE id = ?1",
            rusqlite::params![id, name],
        )?;
        taken.push(name);
    }
    transaction.execute_batch(SCHEMA_V7_INDEXES)
}

/// The eighth schema: where each gathered artifact came from, what it held
/// and the licence it says it is under (#99).
///
/// An artifact kept the commit it was taken at (`revision`), and nothing
/// else of where it came from: an audit asking which folder of which
/// repository a skill on the shelf is, whether the shelf still holds what
/// was gathered, and under what licence, had the files to read and nothing
/// to hold them to. `upstream_path` is its folder in the source, from the
/// repository's root, `/` separated; `content_hash` is [`crate::hash::tree`]
/// of its copy on the shelf, the recipe an install keeps of what it wrote;
/// and the licence is three columns, as [`Licence`] is told: `licence_from`
/// (`manifest`, `file` or `unknown`), `licence`, what it says, and
/// `licence_file`, the file it was read from. The source's `pin`, which the
/// step to schema 7 added and nothing wrote, is written by every gather
/// from this version on, so this step adds nothing for it.
///
/// Nothing older is filled in: each is read off what a gather fetched and
/// placed, which a row from before this version kept none of and no SQL
/// can read, so a row stays as it was, its new columns empty, until its
/// source is gathered again. Its `upstream_path` alone could be spelled
/// from its source's folder and its own, and is not, so the three are
/// there together or not at all.
const SCHEMA_V8: &str = "
ALTER TABLE artifacts ADD COLUMN upstream_path TEXT;
ALTER TABLE artifacts ADD COLUMN content_hash TEXT;
ALTER TABLE artifacts ADD COLUMN licence_from TEXT;
ALTER TABLE artifacts ADD COLUMN licence TEXT;
ALTER TABLE artifacts ADD COLUMN licence_file TEXT;
";

/// The ninth schema: a marketplace's plugins, each a source of its own
/// listed by it (#103, D-13).
///
/// A Claude Code marketplace is a source of kind `marketplace`, its
/// catalog's repository, and each plugin gathered from it a source whose
/// `parent_id`, the column schema 7 made for it, names the marketplace.
/// `entry` is what the plugin is called in the catalog: what it is, since
/// where it is, its URL and its folder, is whatever the catalog says today,
/// and two entries of one catalog may both be its whole repository (as
/// anthropics/skills lists its skills twice, in two plugins). So a plugin
/// is known by its marketplace and its entry, unique behind
/// `sources_by_entry`, and the identity a gather finds a source by,
/// `sources_by_identity`, holds of the sources no marketplace lists alone:
/// a plugin at a repository somebody also gathers with `gather git` is two
/// sources, each called by a name of its own.
///
/// A plugin's `reference` is left `NULL`, its branch being the catalog's to
/// say each time, as its pin is, rather than part of where what was
/// installed from it came from: a catalog that moves a plugin to another
/// tag leaves its installations found where they were. That is also what
/// keeps version 1's `UNIQUE (kind, url, reference, subdirectory)`, which
/// no step can drop without copying the table, from holding two plugins
/// at one repository and folder to one row: SQLite finds no two `NULL`s
/// equal.
///
/// Nothing older is filled in: no build before this one gathered from a
/// marketplace, so every source it recorded is listed by none.
const SCHEMA_V9: &str = "
ALTER TABLE sources ADD COLUMN entry TEXT;

DROP INDEX sources_by_identity;
CREATE UNIQUE INDEX sources_by_identity
    ON sources (kind, canonical_url, reference_key, subdirectory)
    WHERE parent_id IS NULL;
CREATE UNIQUE INDEX sources_by_entry ON sources (parent_id, entry)
    WHERE parent_id IS NOT NULL;
";

#[cfg(test)]
#[path = "sqlite_test.rs"]
mod tests;
