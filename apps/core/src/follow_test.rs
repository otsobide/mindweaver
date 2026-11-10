//! Unit tests for `follow.rs`, kept beside it rather than inside it.

use std::sync::Mutex;

use super::*;
use crate::{backup, gather};

#[test]
fn a_watch_reports_each_step_to_its_sink_in_order() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let watch = gather::Watch::new().reporting_to({
        let seen = Arc::clone(&seen);
        move |progress| seen.lock().unwrap().push(progress)
    });

    watch.report(gather::Progress::Connecting);
    watch.report(gather::Progress::Harvesting { done: 1, total: 2 });

    assert_eq!(
        *seen.lock().unwrap(),
        [
            gather::Progress::Connecting,
            gather::Progress::Harvesting { done: 1, total: 2 }
        ]
    );
}

#[test]
fn a_watch_with_no_sink_reports_to_nobody() {
    // Nothing to assert beyond not panicking: a run nobody follows still
    // reports, and has to be able to.
    gather::Watch::new().report(gather::Progress::Connecting);
}

#[test]
fn a_cancel_reaches_every_clone_of_a_watch() {
    let watch = gather::Watch::new();
    let kept = watch.clone();
    assert!(!watch.is_cancelled());

    kept.cancel();

    assert!(watch.is_cancelled());
    assert!(format!("{watch:?}").contains("cancelled: true"));
}

/// What a front end that follows both kinds of run holds a step as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Either {
    Gather(gather::Progress),
    Backup(backup::Progress),
}

#[test]
fn a_watch_relaying_to_another_tells_its_sink_each_step_in_its_words() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let held: Watch<Either> = Watch::new().reporting_to({
        let seen = Arc::clone(&seen);
        move |step| seen.lock().unwrap().push(step)
    });

    held.relaying(Either::Gather)
        .report(gather::Progress::Connecting);
    held.relaying(Either::Backup)
        .report(backup::Progress::Pushing);

    assert_eq!(
        *seen.lock().unwrap(),
        [
            Either::Gather(gather::Progress::Connecting),
            Either::Backup(backup::Progress::Pushing),
        ]
    );
}

#[test]
fn a_cancel_reaches_a_relaying_watch_and_back() {
    let held: Watch<Either> = Watch::new();
    let backup = held.relaying(Either::Backup);
    let gather = held.relaying(Either::Gather);

    held.cancel();
    assert!(backup.is_cancelled() && gather.is_cancelled());

    let other: Watch<Either> = Watch::new();
    other.relaying(Either::Gather).cancel();
    assert!(other.is_cancelled());
}

#[test]
fn a_watch_relaying_to_one_nobody_follows_reports_to_nobody() {
    let held: Watch<Either> = Watch::new();

    let relaying = held.relaying(Either::Backup);
    relaying.report(backup::Progress::CheckingOut);

    assert!(format!("{relaying:?}").contains("reporting: false"));
}
