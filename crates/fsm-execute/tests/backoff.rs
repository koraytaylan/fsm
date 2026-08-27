//! The wait between attempts: derived, deterministic, and bounded.
//!
//! Plan 0016 task 7501.

use std::collections::BTreeMap;

use fsm_execute::config::{HandlerTable, Retry};
use fsm_execute::effect::PendingEffect;
use fsm_execute::sched::{Directive, Scheduler, backoff_for, ready_at};
use fsm_execute::watch::{AttemptState, Observation};

const NOW: i64 = 1_700_000_000_000;

fn retry(backoff_ms: i64, max_backoff_ms: i64) -> Retry {
    Retry {
        attempts: 16,
        backoff_ms,
        max_backoff_ms,
        on: fsm_execute::config::FAILURE_CLASSES
            .iter()
            .map(|class| (*class).to_string())
            .collect(),
    }
}

fn table() -> HandlerTable {
    HandlerTable::parse(
        r#"{
            "format":"fsm.handlers/1",
            "handlers":[{
                "effect":"notify",
                "argv":["/usr/local/bin/notify"],
                "timeout_ms":30000,
                "retry":{"attempts":5,"backoff_ms":1000,"max_backoff_ms":60000}
            }]
        }"#,
    )
    .unwrap()
}

fn observation(attempt: u32, last_ts: i64) -> Observation {
    Observation {
        pending: vec![PendingEffect {
            instance_id: "case-1".into(),
            effect_id: "case-1/3/0".into(),
            effect_name: "notify".into(),
            args: BTreeMap::new(),
            emitted_seq: 3,
            k: 0,
        }],
        attempts: if attempt == 0 {
            BTreeMap::new()
        } else {
            BTreeMap::from([("case-1/3/0".to_string(), AttemptState { attempt, last_ts })])
        },
        ..Observation::default()
    }
}

fn starts(directives: &[Directive]) -> bool {
    directives
        .iter()
        .any(|directive| matches!(directive, Directive::Start { .. }))
}

#[test]
fn the_wait_doubles_from_the_records_own_timestamp() {
    let retry = retry(1_000, 60_000);
    assert_eq!(ready_at(&retry, 1, NOW), NOW + 1_000);
    assert_eq!(ready_at(&retry, 2, NOW), NOW + 2_000);
    assert_eq!(ready_at(&retry, 3, NOW), NOW + 4_000);
    // Each from *its own* record's timestamp, not from a running total.
    assert_eq!(ready_at(&retry, 2, NOW + 5_000), NOW + 7_000);
}

#[test]
fn the_ceiling_wins_and_keeps_winning() {
    let retry = retry(1_000, 60_000);
    assert_eq!(backoff_for(&retry, 7), 60_000, "2^6 * 1000 is over the cap");
    assert_eq!(backoff_for(&retry, 10), 60_000);
    assert_eq!(backoff_for(&retry, 16), 60_000);
}

#[test]
fn an_overflowing_multiply_saturates_instead_of_landing_in_the_past() {
    // A large base against a high attempt overflows a naive multiply, and an
    // overflowed deadline in the past would turn backoff into a busy loop —
    // the exact opposite of what it is for.
    let retry = retry(i64::MAX / 4, i64::MAX / 2);
    for attempt in 1..=16 {
        let wait = backoff_for(&retry, attempt);
        assert!(wait > 0, "attempt {attempt} produced a wait of {wait}");
        assert!(
            wait <= retry.max_backoff_ms,
            "attempt {attempt} exceeded the ceiling"
        );
        let due = ready_at(&retry, attempt, NOW);
        assert!(due > NOW, "attempt {attempt} is due in the past: {due}");
    }
}

#[test]
fn a_deadline_is_a_deadline_to_the_millisecond() {
    let table = table();
    let due = ready_at(&table.handlers["notify"].retry, 1, NOW);

    let mut scheduler = Scheduler::new(table.clone());
    assert!(
        !starts(&scheduler.on_observation(&observation(1, NOW), due - 1)),
        "one millisecond early is early"
    );
    let mut scheduler = Scheduler::new(table);
    assert!(
        starts(&scheduler.on_observation(&observation(1, NOW), due)),
        "and exactly on it is due"
    );
}

#[test]
fn a_deadline_far_in_the_past_is_due_now() {
    // Computed from the record's timestamp rather than from process start:
    // an executor that comes up an hour later does not restart the wait.
    let mut scheduler = Scheduler::new(table());
    let directives = scheduler.on_observation(&observation(1, NOW - 3_600_000), NOW);
    assert!(starts(&directives), "{directives:?}");
}

#[test]
fn a_hundred_runs_produce_the_same_directives() {
    // No jitter, and no randomness of any kind: the restart-equivalence
    // property requires that the same observation and the same `now_ms`
    // produce the same directives.
    let table = table();
    let due = ready_at(&table.handlers["notify"].retry, 2, NOW);
    let observation = observation(2, NOW);
    let rendered = |directives: &[Directive]| format!("{directives:?}");

    let mut first = Scheduler::new(table.clone());
    let expected = rendered(&first.on_observation(&observation, due));
    for run in 0..100 {
        let mut scheduler = Scheduler::new(table.clone());
        let directives = scheduler.on_observation(&observation, due);
        assert_eq!(rendered(&directives), expected, "run {run} differed");
    }
}

#[test]
fn a_tick_that_is_waiting_says_so_with_identifiers_only() {
    let handlers = table();
    let due = ready_at(&handlers.handlers["notify"].retry, 1, NOW);
    let mut scheduler = Scheduler::new(handlers);
    let directives = scheduler.on_observation(&observation(1, NOW), due - 1);
    assert!(directives.is_empty());
    assert_eq!(
        scheduler.deferred(),
        ["case-1/3/0"],
        "an operator watching a quiet tick must be able to tell waiting from idle"
    );
    // Identifiers only: nothing about the handler, the wait, or the host.
    let reported = format!("{:?}", scheduler.deferred());
    for leak in ["/usr/local/bin", "ms", "pid"] {
        assert!(!reported.contains(leak), "{reported} leaks {leak}");
    }

    // And once it starts, the deferral is not still being reported.
    let mut scheduler = Scheduler::new(table());
    let _ = scheduler.on_observation(&observation(1, NOW), due);
    assert!(scheduler.deferred().is_empty());
}
