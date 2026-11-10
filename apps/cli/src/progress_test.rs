use serde_json::json;

use super::*;

/// Bytes written somewhere a test can read them back from.
#[derive(Clone, Default)]
struct Written(Arc<Mutex<Vec<u8>>>);

impl Write for Written {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Written {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

#[test]
fn each_step_reads_as_what_is_counted_and_how_far_it_has_got() {
    let cases = [
        (Progress::Connecting, "connecting"),
        (
            Progress::Receiving {
                done: 120,
                total: Some(340),
            },
            "receiving objects 120/340",
        ),
        (
            Progress::Resolving {
                done: 7,
                total: None,
            },
            "resolving deltas 7",
        ),
        (
            Progress::CheckingOut {
                done: 0,
                total: None,
            },
            "checking out files",
        ),
        (
            Progress::Harvesting { done: 1, total: 3 },
            "reading folders 1/3",
        ),
        (
            Progress::Placing { done: 0, total: 2 },
            "placing on the shelf 0/2",
        ),
    ];
    for (progress, words) in cases {
        assert_eq!(said(progress), words);
    }
}

#[test]
fn the_line_is_rewritten_in_place_and_wiped_at_the_end() {
    let written = Written::default();
    let line = ProgressLine::to(written.clone());
    let watch = line.watch();

    watch.report(Progress::Receiving {
        done: 12,
        total: Some(340),
    });
    watch.report(Progress::Harvesting { done: 1, total: 3 });
    line.clear();

    // The shorter line covers what is left of the longer one with spaces,
    // and the wipe covers the last.
    assert_eq!(
        written.text(),
        "\rreceiving objects 12/340\rreading folders 1/3     \r                   \r"
    );
}

#[test]
fn a_line_that_drew_nothing_wipes_nothing() {
    let written = Written::default();
    let line = ProgressLine::to(written.clone());

    line.clear();

    assert_eq!(written.text(), "");
}

#[test]
fn a_line_that_is_not_drawn_hands_out_a_watch_that_reports_to_nobody() {
    let line = ProgressLine { drawn: None };
    let watch = line.watch();

    watch.report(Progress::Connecting);
    line.clear();

    // Still a watch that can be cancelled, as every one is.
    watch.cancel();
    assert!(watch.is_cancelled());
}

#[test]
fn a_step_says_its_stage_its_count_its_words_and_whether_a_cancel_still_stops_it() {
    let cases = [
        (
            Progress::Connecting,
            json!({"stage": "connecting", "done": null, "total": null,
                   "said": "connecting", "cancellable": true}),
        ),
        (
            Progress::Receiving {
                done: 120,
                total: Some(340),
            },
            json!({"stage": "receiving", "done": 120, "total": 340,
                   "said": "receiving objects 120/340", "cancellable": true}),
        ),
        (
            Progress::Resolving {
                done: 7,
                total: None,
            },
            json!({"stage": "resolving", "done": 7, "total": null,
                   "said": "resolving deltas 7", "cancellable": true}),
        ),
        (
            Progress::CheckingOut {
                done: 0,
                total: None,
            },
            json!({"stage": "checking_out", "done": 0, "total": null,
                   "said": "checking out files", "cancellable": true}),
        ),
        (
            Progress::Harvesting { done: 1, total: 3 },
            json!({"stage": "harvesting", "done": 1, "total": 3,
                   "said": "reading folders 1/3", "cancellable": true}),
        ),
        (
            Progress::Placing { done: 0, total: 2 },
            json!({"stage": "placing", "done": 0, "total": 2,
                   "said": "placing on the shelf 0/2", "cancellable": false}),
        ),
    ];
    for (progress, step) in cases {
        assert_eq!(serde_json::to_value(Step::of(progress)).unwrap(), step);
    }
}

/// What a watch paced at `pace` hands on of `steps`, in words.
fn handed(pace: Duration, steps: &[Progress]) -> Vec<String> {
    let handed = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&handed);
    let watch = paced(move |step| kept.lock().unwrap().push(step.said), pace);
    for step in steps {
        watch.report(*step);
    }
    let handed = handed.lock().unwrap().clone();
    handed
}

/// A burst of a stage is handed on by its first step and its last, so no
/// stage goes unsaid and none is left short of its end, however long the
/// pace; with none, every step is.
#[test]
fn a_burst_of_steps_is_handed_on_by_each_stage_s_first_and_last() {
    let steps: Vec<Progress> = std::iter::once(Progress::Connecting)
        .chain((0..=200).map(|done| Progress::Harvesting { done, total: 200 }))
        .chain((0..=3).map(|done| Progress::Placing { done, total: 3 }))
        .collect();

    assert_eq!(
        handed(Duration::from_secs(3600), &steps),
        [
            "connecting",
            "reading folders 0/200",
            "reading folders 200/200",
            "placing on the shelf 0/3",
            "placing on the shelf 3/3",
        ]
    );
    assert_eq!(handed(Duration::ZERO, &steps).len(), steps.len());
    assert_eq!(PACE, Duration::from_millis(100));
}

/// What the watch is cancelled by is the watch's, as every one's is.
#[test]
fn a_reported_watch_is_cancelled_as_any_is() {
    let watch = reported(|_| {});

    watch.clone().cancel();

    assert!(watch.is_cancelled());
}
