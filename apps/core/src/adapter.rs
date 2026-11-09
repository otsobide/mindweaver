//! The adapters: what implements each of core's ports ([`crate::port`]) with
//! a library or the system, the one place a build says which.
//!
//! Each is the default its port is given wherever nothing else is handed
//! over, as D-14 chose for HTTP: [`gix::Gix`] fetches a source,
//! [`git::SystemGit`] sends a backup, and [`SystemClock`] tells the time. Another adapter is written beside them
//! against the same port and handed to what takes the port, a gather's
//! [`Request::through`](crate::gather::Request::through) or the ledger's
//! [`with_clock`](crate::ledger::Ledger::with_clock); nothing behind the port
//! changes. A test's fake is one more adapter, and lives with the test.

pub mod git;
pub mod gix;
pub mod sqlite;

use std::time::{SystemTime, UNIX_EPOCH};

use crate::port::Clock;

/// The operating system's clock, as the [`Clock`] the ledger stamps with.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    /// An integer rather than a formatted timestamp: it needs no date
    /// library, it sorts and compares as itself, and SQLite renders it for
    /// anyone reading the file by hand with `datetime(at, 'unixepoch')`. A
    /// clock set before 1970 reads as the epoch itself.
    fn now(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs() as i64)
    }
}

#[cfg(test)]
#[path = "adapter_test.rs"]
mod tests;
