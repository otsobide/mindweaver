use std::sync::atomic::AtomicUsize;

use serde_json::json;

use super::*;

/// A run whose reports are kept, for a test to read.
fn kept() -> (Underway, Arc<Mutex<Vec<Value>>>) {
    let reports = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&reports);
    let underway = Underway::new(7, move |progress| sink.lock().unwrap().push(progress));
    (underway, reports)
}

#[test]
fn a_report_reaches_whoever_follows_until_the_run_is_over() {
    let (underway, reports) = kept();

    underway.report(json!({"stage": "connecting"}));
    underway.end();
    underway.report(json!({"stage": "placing"}));

    assert_eq!(underway.run(), 7);
    assert_eq!(*reports.lock().unwrap(), [json!({"stage": "connecting"})]);
}

#[test]
fn a_cancel_does_what_the_backend_said_one_does_each_time_it_is_asked() {
    let (underway, _) = kept();
    let done = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&done);
    underway.on_cancel(move || {
        counted.fetch_add(1, Ordering::SeqCst);
    });

    assert!(!underway.is_cancelled());
    underway.clone().cancel();
    underway.cancel();

    assert!(underway.is_cancelled());
    assert_eq!(done.load(Ordering::SeqCst), 2);
}

/// A cancel that arrives before the backend says what one does is not
/// lost: it is done as soon as the backend says.
#[test]
fn a_cancel_asked_before_the_backend_says_what_one_does_is_done_once_it_says() {
    let (underway, _) = kept();
    underway.cancel();
    let done = Arc::new(AtomicBool::new(false));
    let set = Arc::clone(&done);

    underway.on_cancel(move || set.store(true, Ordering::SeqCst));

    assert!(done.load(Ordering::SeqCst));
}

#[test]
fn a_run_nobody_follows_reports_to_nobody_and_is_numbered_0() {
    let underway = Underway::unfollowed();

    underway.report(json!("anything"));

    assert_eq!(underway.run(), 0);
    assert!(!underway.is_cancelled());
}

#[test]
fn a_hook_that_panics_is_caught_and_said() {
    let (underway, _) = kept();
    underway.on_cancel(|| panic!("the backend's cancel panics, as asked"));

    assert!(!underway.cancel_caught());
    assert!(underway.is_cancelled());
}

#[test]
fn a_cancel_is_read_from_its_run_alone() {
    let read: Cancel = serde_json::from_value(json!({"run": 3})).unwrap();

    assert_eq!(read, Cancel { run: 3 });
    assert!(serde_json::from_value::<Cancel>(json!({"run": 3, "force": true})).is_err());
    assert!(serde_json::from_value::<Cancel>(json!({"run": -1})).is_err());
    assert!(serde_json::from_value::<Cancel>(json!({})).is_err());
    assert_eq!(
        serde_json::to_value(Cancelling {
            run: 3,
            lines: vec![String::from(
                "mindweaver gather git https://example.com/skills"
            )],
        })
        .unwrap(),
        json!({"run": 3, "lines": ["mindweaver gather git https://example.com/skills"]})
    );
}
