//! The scheduler's retry rules, which are all derived from the journal.
//!
//! Every row here is a fabricated observation and a chosen `now_ms`: no
//! store, no subprocess, no clock. What is pinned is that a fresh process
//! fed the same records reaches the same conclusion its killed predecessor
//! did.
//!
//! Plan 0016 task 7403.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::expr::eval::Val;
use fsm_execute::config::HandlerTable;
use fsm_execute::effect::PendingEffect;
use fsm_execute::rid::attempt_rid;
use fsm_execute::sched::{Directive, Scheduler, backoff_for, ready_at};
use fsm_execute::watch::{AttemptState, Observation};

const NOW: i64 = 1_700_000_000_000;

/// One handler that retries three times with a 1-second base, and one that
/// retries only timeouts.
fn table() -> HandlerTable {
    HandlerTable::parse(
        r#"{
            "format":"fsm.handlers/1",
            "handlers":[
                {
                    "effect":"notify",
                    "argv":["/usr/local/bin/notify","--case","{case}"],
                    "timeout_ms":30000,
                    "retry":{"attempts":3,"backoff_ms":1000,"max_backoff_ms":8000}
                },
                {
                    "effect":"timeouts_only",
                    "argv":["/usr/local/bin/slow"],
                    "timeout_ms":1000,
                    "retry":{"attempts":4,"on":["timeout"]}
                },
                {
                    "effect":"once",
                    "argv":["/usr/local/bin/once"],
                    "timeout_ms":1000
                }
            ]
        }"#,
    )
    .unwrap()
}

fn effect(effect_id: &str, name: &str) -> PendingEffect {
    PendingEffect {
        instance_id: effect_id.split('/').next().unwrap().to_string(),
        effect_id: effect_id.to_string(),
        effect_name: name.to_string(),
        args: BTreeMap::from([("case".to_string(), Val::Str("case-9".into()))]),
        emitted_seq: 3,
        k: 0,
    }
}

/// An observation with one pending effect and whatever attempt state.
fn observation(
    effect_id: &str,
    name: &str,
    attempts: &[(&str, AttemptState)],
    claimed: &[String],
) -> Observation {
    Observation {
        pending: vec![effect(effect_id, name)],
        attempts: attempts
            .iter()
            .map(|(id, state)| ((*id).to_string(), *state))
            .collect(),
        claimed_request_ids: claimed.iter().cloned().collect::<BTreeSet<_>>(),
        ..Observation::default()
    }
}

fn started(directives: &[Directive]) -> Option<u32> {
    directives.iter().find_map(|directive| match directive {
        Directive::Start { attempt, .. } => Some(*attempt),
        _ => None,
    })
}

#[test]
fn an_effect_nobody_has_tried_starts_as_attempt_one() {
    let mut scheduler = Scheduler::new(table());
    let directives = scheduler.on_observation(&observation("case-1/3/0", "notify", &[], &[]), NOW);
    assert_eq!(started(&directives), Some(1));
}

#[test]
fn the_next_attempt_waits_for_its_backoff_and_then_starts() {
    let retry = &table().handlers["notify"].retry;
    let after_one = AttemptState {
        attempt: 1,
        last_ts: NOW,
    };
    let ready = ready_at(retry, 1, NOW);
    assert_eq!(ready, NOW + 1_000, "the first wait is the base");

    // Inside the window: no directive at all. That is what makes backoff
    // free — the executor does not sleep and does not hold a slot.
    let mut scheduler = Scheduler::new(table());
    let directives = scheduler.on_observation(
        &observation("case-1/3/0", "notify", &[("case-1/3/0", after_one)], &[]),
        ready - 1,
    );
    assert!(
        directives.is_empty(),
        "an effect inside its backoff window produced {directives:?}"
    );

    // At the deadline, attempt 2.
    let mut scheduler = Scheduler::new(table());
    let directives = scheduler.on_observation(
        &observation("case-1/3/0", "notify", &[("case-1/3/0", after_one)], &[]),
        ready,
    );
    assert_eq!(started(&directives), Some(2));
}

#[test]
fn the_wait_doubles_and_stops_at_the_ceiling() {
    let retry = &table().handlers["notify"].retry;
    assert_eq!(backoff_for(retry, 1), 1_000);
    assert_eq!(backoff_for(retry, 2), 2_000);
    assert_eq!(backoff_for(retry, 3), 4_000);
    assert_eq!(backoff_for(retry, 4), 8_000);
    assert_eq!(
        backoff_for(retry, 9),
        8_000,
        "a handler whose dependency is down for an hour must not back off for a week"
    );
}

#[test]
fn the_last_attempt_is_the_last() {
    let table = table();
    let attempts = table.handlers["notify"].retry.attempts;
    let state = AttemptState {
        attempt: attempts - 1,
        last_ts: NOW,
    };
    let mut scheduler = Scheduler::new(table.clone());
    let directives = scheduler.on_observation(
        &observation("case-1/3/0", "notify", &[("case-1/3/0", state)], &[]),
        NOW + 1_000_000,
    );
    assert_eq!(started(&directives), Some(attempts), "the last try");

    // And after it, nothing: the ack path takes over.
    let exhausted = AttemptState {
        attempt: attempts,
        last_ts: NOW,
    };
    let mut scheduler = Scheduler::new(table);
    let directives = scheduler.on_observation(
        &observation("case-1/3/0", "notify", &[("case-1/3/0", exhausted)], &[]),
        NOW + 1_000_000,
    );
    assert!(
        started(&directives).is_none(),
        "an exhausted effect must not be started again: {directives:?}"
    );
}

#[test]
fn a_fresh_scheduler_reaches_the_same_conclusion() {
    // Restart equivalence: the count comes from the journal, so a process
    // that has never seen this effect starts the attempt its predecessor
    // would have.
    let state = AttemptState {
        attempt: 2,
        last_ts: NOW,
    };
    let observation = observation("case-1/3/0", "notify", &[("case-1/3/0", state)], &[]);
    let now = NOW + 1_000_000;

    let mut warm = Scheduler::new(table());
    let _ = warm.on_observation(&observation, now);

    let mut fresh = Scheduler::new(table());
    let directives = fresh.on_observation(&observation, now);
    assert_eq!(
        started(&directives),
        Some(3),
        "a fresh scheduler must start attempt 3, not attempt 1"
    );
}

#[test]
fn an_attempt_whose_key_is_claimed_is_not_started_again() {
    // The key was claimed, so the attempt was journaled: this is a restart
    // re-deriving it, and acting again could never be recorded.
    let state = AttemptState {
        attempt: 1,
        last_ts: NOW,
    };
    let mut scheduler = Scheduler::new(table());
    let directives = scheduler.on_observation(
        &observation(
            "case-1/3/0",
            "notify",
            &[("case-1/3/0", state)],
            &[attempt_rid("case-1/3/0", 2)],
        ),
        NOW + 1_000_000,
    );
    assert!(
        started(&directives).is_none(),
        "a claimed attempt key must stop the start: {directives:?}"
    );
}

#[test]
fn a_handler_without_a_retry_block_still_starts_exactly_once() {
    let mut scheduler = Scheduler::new(table());
    let directives = scheduler.on_observation(&observation("case-1/3/0", "once", &[], &[]), NOW);
    assert_eq!(started(&directives), Some(1));

    // And after that one attempt, nothing.
    let state = AttemptState {
        attempt: 1,
        last_ts: NOW,
    };
    let mut scheduler = Scheduler::new(table());
    let directives = scheduler.on_observation(
        &observation("case-1/3/0", "once", &[("case-1/3/0", state)], &[]),
        NOW + 1_000_000,
    );
    assert!(started(&directives).is_none(), "{directives:?}");
}

#[test]
fn the_class_list_decides_what_is_retried_at_all() {
    let table = table();
    let timeouts_only = &table.handlers["timeouts_only"].retry;
    assert!(timeouts_only.retries("timeout"));
    assert!(
        !timeouts_only.retries("nonzero_exit"),
        "a class the table did not name is not retried, whatever the budget"
    );
    assert!(
        !timeouts_only.retries("cancelled"),
        "and the one kill that means stop is never retryable — the config \
         gate refuses the class, and this is the run-time half of the rule"
    );

    let once = &table.handlers["once"].retry;
    for class in fsm_execute::config::FAILURE_CLASSES {
        assert!(
            !once.retries(class),
            "a handler with one attempt retries nothing: {class}"
        );
    }
}
