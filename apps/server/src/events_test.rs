use std::time::Duration;

use serde_json::json;

use super::*;

/// The next event, or a failed test once long enough has passed that none
/// is coming.
async fn next(subscription: &mut Subscription) -> Option<Event> {
    tokio::time::timeout(Duration::from_secs(10), subscription.next())
        .await
        .expect("an event, or the end, in time")
}

/// What a subscriber hears first while nothing has switched.
const RESYNC: Event = Event::Resync { switches: 0 };

fn changed(scope: &str) -> Event {
    Event::Changed {
        scopes: vec![json!(scope)],
    }
}

#[tokio::test]
async fn a_subscriber_is_told_to_resync_and_then_every_event_in_order() {
    let events = Events::new();
    let mut subscription = events.subscribe();

    events.publish(changed("one"));
    events.publish(changed("two"));

    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(changed("one")));
    assert_eq!(next(&mut subscription).await, Some(changed("two")));
}

#[tokio::test]
async fn a_subscriber_is_told_how_things_are_watched_once_something_has_said() {
    let events = Events::new();
    let mut before = events.subscribe();
    let unwatched = Event::Unwatched {
        reason: String::from("the limit"),
    };
    events.publish(unwatched.clone());
    events.publish(changed("one"));

    let mut after = events.subscribe();

    assert_eq!(next(&mut before).await, Some(RESYNC));
    assert_eq!(next(&mut before).await, Some(unwatched.clone()));
    assert_eq!(next(&mut after).await, Some(RESYNC));
    assert_eq!(next(&mut after).await, Some(unwatched));
    events.publish(Event::Watching);
    assert_eq!(next(&mut after).await, Some(Event::Watching));
}

#[tokio::test]
async fn a_subscriber_that_falls_behind_is_told_to_resync_and_how_things_are_watched() {
    let events = Events::new();
    events.publish(Event::Watching);
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(Event::Watching));

    for each in 0..CAPACITY + 10 {
        events.publish(changed(&each.to_string()));
    }

    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(Event::Watching));
    // Then what the channel still holds, the latest it kept.
    assert_eq!(next(&mut subscription).await, Some(changed("10")));
}

/// A subscriber that missed a switch, subscribing after it, or falling
/// behind past it, learns of it from the count its resync carries, the
/// one every switch told carries.
#[tokio::test]
async fn every_switch_is_counted_and_a_resync_carries_the_count() {
    let events = Events::new();
    let mut heard = events.subscribe();
    assert_eq!(next(&mut heard).await, Some(RESYNC));

    events.switched();
    events.switched();

    assert_eq!(
        next(&mut heard).await,
        Some(Event::Switched { switches: 1 })
    );
    assert_eq!(
        next(&mut heard).await,
        Some(Event::Switched { switches: 2 })
    );
    let mut after = events.subscribe();
    assert_eq!(next(&mut after).await, Some(Event::Resync { switches: 2 }));

    events.switched();
    for each in 0..CAPACITY + 10 {
        events.publish(changed(&each.to_string()));
    }

    assert_eq!(
        next(&mut after).await,
        Some(Event::Resync { switches: 3 }),
        "fallen behind past the switch"
    );
}

/// Run `run` of the lines `gather`, which takes a cancel.
fn started(run: u64) -> Event {
    Event::CommandStarted {
        run,
        lines: vec![String::from("gather")],
        cancellable: true,
    }
}

fn busy(run: u64, cancelling: bool) -> Event {
    Event::Busy {
        run,
        lines: vec![String::from("gather")],
        cancellable: true,
        cancelling,
    }
}

fn progress(run: u64, step: &str) -> Event {
    Event::Progress {
        run,
        progress: json!(step),
    }
}

/// A subscriber that comes while a run is under way, a tab opened or shown
/// again in the middle of a gather, is told what the writer is busy with
/// and the latest step it reported, rather than wait for the next one.
#[tokio::test]
async fn a_subscriber_is_told_of_the_run_under_way_and_its_latest_step() {
    let events = Events::new();
    events.publish(started(4));
    events.publish(progress(4, "connecting"));
    events.publish(progress(4, "receiving"));
    events.publish(Event::Cancelling { run: 4 });

    let mut subscription = events.subscribe();

    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(busy(4, true)));
    assert_eq!(
        next(&mut subscription).await,
        Some(progress(4, "receiving"))
    );
    events.publish(Event::CommandFinished {
        run: 4,
        lines: vec![String::from("gather")],
    });
    let mut after = events.subscribe();
    events.publish(changed("one"));
    assert_eq!(next(&mut after).await, Some(RESYNC));
    assert_eq!(next(&mut after).await, Some(changed("one")));
}

/// What is queued behind a run is told that the writer is busy with it;
/// with nothing under way, nothing is told.
#[tokio::test]
async fn busy_is_told_only_while_a_run_is_under_way() {
    let events = Events::new();
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(RESYNC));

    events.busy();
    events.publish(started(1));
    events.busy();
    // A step of a run not under way, told too late, changes nothing kept.
    events.publish(progress(7, "placing"));
    events.publish(Event::CommandFinished {
        run: 1,
        lines: vec![String::from("gather")],
    });
    events.busy();
    events.publish(changed("after"));

    assert_eq!(next(&mut subscription).await, Some(started(1)));
    assert_eq!(next(&mut subscription).await, Some(busy(1, false)));
    assert_eq!(next(&mut subscription).await, Some(progress(7, "placing")));
    assert!(matches!(
        next(&mut subscription).await,
        Some(Event::CommandFinished { run: 1, .. })
    ));
    assert_eq!(next(&mut subscription).await, Some(changed("after")));
}

#[tokio::test]
async fn a_subscriber_that_falls_behind_is_told_of_the_run_under_way_again() {
    let events = Events::new();
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    events.publish(started(2));

    for each in 0..CAPACITY + 10 {
        events.publish(progress(2, &each.to_string()));
    }

    assert_eq!(next(&mut subscription).await, Some(RESYNC));
    assert_eq!(next(&mut subscription).await, Some(busy(2, false)));
    assert_eq!(
        next(&mut subscription).await,
        Some(progress(2, &(CAPACITY + 9).to_string()))
    );
}

#[tokio::test]
async fn closing_ends_every_subscription_and_every_one_after() {
    let events = Events::new();
    let mut subscription = events.subscribe();
    assert_eq!(next(&mut subscription).await, Some(RESYNC));

    events.close();
    events.publish(changed("late"));

    assert_eq!(next(&mut subscription).await, None);
    assert_eq!(next(&mut events.subscribe()).await, None);
}

#[test]
fn each_event_is_tagged_by_its_type_in_snake_case() {
    let shapes = [
        (
            changed("one"),
            json!({"type": "changed", "scopes": ["one"]}),
        ),
        (
            Event::CommandStarted {
                run: 1,
                lines: vec![String::from("mindweaver link beta")],
                cancellable: false,
            },
            json!({"type": "command_started", "run": 1, "lines": ["mindweaver link beta"],
                   "cancellable": false}),
        ),
        (
            Event::Progress {
                run: 2,
                progress: json!({"stage": "connecting"}),
            },
            json!({"type": "progress", "run": 2, "progress": {"stage": "connecting"}}),
        ),
        (
            Event::Cancelling { run: 2 },
            json!({"type": "cancelling", "run": 2}),
        ),
        (
            Event::Busy {
                run: 2,
                lines: vec![String::from("mindweaver gather git /src")],
                cancellable: true,
                cancelling: false,
            },
            json!({"type": "busy", "run": 2, "lines": ["mindweaver gather git /src"],
                   "cancellable": true, "cancelling": false}),
        ),
        (
            Event::CommandFinished {
                run: 1,
                lines: vec![String::from("mindweaver link beta")],
            },
            json!({"type": "command_finished", "run": 1, "lines": ["mindweaver link beta"]}),
        ),
        (
            Event::Resync { switches: 2 },
            json!({"type": "resync", "switches": 2}),
        ),
        (
            Event::Switched { switches: 3 },
            json!({"type": "switched", "switches": 3}),
        ),
        (Event::Watching, json!({"type": "watching"})),
        (
            Event::Unwatched {
                reason: String::from("the limit"),
            },
            json!({"type": "unwatched", "reason": "the limit"}),
        ),
    ];
    for (event, shape) in shapes {
        assert_eq!(serde_json::to_value(&event).unwrap(), shape);
    }
}
