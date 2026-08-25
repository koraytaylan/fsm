//! The scheduler is pure, so every row here is a fabricated observation and a
//! chosen `now_ms` — no store, no subprocess, no clock. What is being pinned
//! is that the decisions come from journaled facts, which is why a fresh
//! process reaches the same ones.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::expr::eval::Val;
use fsm_core::json::Value;
use fsm_execute::config::HandlerTable;
use fsm_execute::effect::PendingEffect;
use fsm_execute::rid::{ack_rid, event_rid, poll_rid};
use fsm_execute::run::KillReason;
use fsm_execute::sched::{Directive, Scheduler};
use fsm_execute::watch::{DueDeadline, Observation, SettledEffect};

const NOW: i64 = 1_700_000_000_000;

fn table() -> HandlerTable {
    HandlerTable::parse(
        r#"{
            "format":"fsm.handlers/1",
            "handlers":[
                {
                    "effect":"assign_reviewer",
                    "argv":["/usr/local/bin/assign-reviewer","--case","{case}","--quiet"],
                    "timeout_ms":30000,
                    "on_ok":{"event":"assigned","payload":{"channel":"desk"},"stamps":["at"]},
                    "on_failed":{"event":"assignment_failed"}
                },
                {
                    "effect":"archive_case",
                    "argv":["/usr/local/bin/archive-case","--case","{case}"],
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

fn observation_with_pending(effects: Vec<PendingEffect>) -> Observation {
    Observation {
        pending: effects,
        ..Observation::default()
    }
}

fn settled(effect_id: &str, name: &str, outcome: &str) -> SettledEffect {
    SettledEffect {
        instance_id: effect_id.split('/').next().unwrap().to_string(),
        effect_id: effect_id.to_string(),
        effect_name: name.to_string(),
        outcome: outcome.to_string(),
        seq: 7,
    }
}

fn claimed(keys: &[String]) -> BTreeSet<String> {
    keys.iter().cloned().collect()
}

#[test]
fn a_pending_effect_with_a_handler_starts_once_with_substituted_argv() {
    let mut scheduler = Scheduler::new(table());
    let observation = observation_with_pending(vec![effect("case-1/3/0", "assign_reviewer")]);

    let directives = scheduler.on_observation(&observation, NOW);
    assert_eq!(directives.len(), 1);
    match &directives[0] {
        Directive::Start {
            effect,
            argv,
            timeout_ms,
        } => {
            assert_eq!(effect.effect_id, "case-1/3/0");
            assert_eq!(
                argv,
                &[
                    "/usr/local/bin/assign-reviewer",
                    "--case",
                    "case-9",
                    "--quiet"
                ]
            );
            assert_eq!(*timeout_ms, 30_000);
        }
        other => panic!("expected a Start, got {other:?}"),
    }

    // Still running: the same observation must not start a second process.
    assert!(scheduler.on_observation(&observation, NOW + 1).is_empty());
}

#[test]
fn an_already_claimed_ack_key_stops_a_fresh_process_from_re_running_the_work() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        pending: vec![effect("case-1/3/0", "assign_reviewer")],
        claimed_request_ids: claimed(&[ack_rid("case-1/3/0")]),
        ..Observation::default()
    };
    // Empty in-flight map, exactly as after a restart: the journal is what
    // says this effect has already been settled.
    assert!(scheduler.on_observation(&observation, NOW).is_empty());
}

#[test]
fn a_pending_effect_with_no_handler_is_reported_once_and_never_run() {
    let mut scheduler = Scheduler::new(table());
    let observation = observation_with_pending(vec![effect("case-1/3/0", "call_supplier")]);

    assert!(scheduler.on_observation(&observation, NOW).is_empty());
    assert_eq!(scheduler.unhandled(), ["case-1/3/0"]);

    assert!(scheduler.on_observation(&observation, NOW + 1).is_empty());
    assert!(
        scheduler.unhandled().is_empty(),
        "the stall is logged once, not on every tick"
    );
}

#[test]
fn a_settled_effect_sends_the_advance_its_handler_declares() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        settled: vec![settled("case-1/3/0", "assign_reviewer", "ok")],
        ..Observation::default()
    };

    let directives = scheduler.on_observation(&observation, NOW);
    assert_eq!(directives.len(), 1);
    match &directives[0] {
        Directive::SendEvent {
            instance_id,
            effect_id,
            event,
            payload,
            stamps,
            request_id,
        } => {
            assert_eq!(instance_id, "case-1");
            assert_eq!(effect_id, "case-1/3/0");
            assert_eq!(event, "assigned");
            assert_eq!(
                payload,
                &Value::Obj(BTreeMap::from([(
                    "channel".into(),
                    Value::Str("desk".into())
                )]))
            );
            assert_eq!(stamps, &["at"]);
            assert_eq!(request_id, &event_rid("case-1/3/0", "assigned"));
        }
        other => panic!("expected a SendEvent, got {other:?}"),
    }
}

#[test]
fn a_failed_outcome_uses_the_failure_advance() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        settled: vec![settled("case-1/3/0", "assign_reviewer", "failed")],
        ..Observation::default()
    };
    match &scheduler.on_observation(&observation, NOW)[0] {
        Directive::SendEvent { event, .. } => assert_eq!(event, "assignment_failed"),
        other => panic!("expected a SendEvent, got {other:?}"),
    }
}

#[test]
fn an_advance_that_already_landed_is_not_sent_again() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        settled: vec![settled("case-1/3/0", "assign_reviewer", "ok")],
        claimed_request_ids: claimed(&[event_rid("case-1/3/0", "assigned")]),
        ..Observation::default()
    };
    assert!(scheduler.on_observation(&observation, NOW).is_empty());
}

#[test]
fn an_advance_the_engine_declines_is_parked_until_the_journal_moves() {
    // A declined advance claims no key, so the ack stays outstanding and this
    // rule would re-derive the same directive on every tick — and every one of
    // those ticks would open the writer, fold, and snapshot on drop.
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        to_seq: 7,
        settled: vec![settled("case-1/3/0", "assign_reviewer", "ok")],
        ..Observation::default()
    };
    assert_eq!(scheduler.on_observation(&observation, NOW).len(), 1);

    scheduler.park_advance("case-1/3/0", "assigned", 7);
    assert!(
        scheduler.on_observation(&observation, NOW + 1).is_empty(),
        "nothing was journaled, so nothing could have changed the guard"
    );

    // A record landed: the guard may read context some other event changed.
    let moved = Observation {
        to_seq: 8,
        ..observation
    };
    assert_eq!(scheduler.on_observation(&moved, NOW + 2).len(), 1);
}

#[test]
fn a_handler_declaring_no_advance_leaves_the_instance_where_it_is() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        settled: vec![settled("case-1/3/0", "archive_case", "ok")],
        ..Observation::default()
    };
    assert!(scheduler.on_observation(&observation, NOW).is_empty());
}

#[test]
fn a_due_deadline_is_polled_once_under_its_derived_key() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        due_deadlines: vec![DueDeadline {
            instance_id: "case-1".into(),
            deadline_name: "review_timeout".into(),
            due_ms: NOW - 5,
        }],
        ..Observation::default()
    };

    let directives = scheduler.on_observation(&observation, NOW);
    assert_eq!(directives.len(), 1);
    match &directives[0] {
        Directive::PollDeadline {
            instance_id,
            deadline,
            due_ms,
            request_id,
        } => {
            assert_eq!(instance_id, "case-1");
            assert_eq!(deadline, "review_timeout");
            assert_eq!(*due_ms, NOW - 5);
            assert_eq!(request_id, &poll_rid("case-1", "review_timeout", NOW - 5));
        }
        other => panic!("expected a PollDeadline, got {other:?}"),
    }

    // A poll that was decided but never journaled is decided again: the
    // driver, not the decision, is what records that one landed. Marking it
    // here would silence the deadline for the life of the process whenever a
    // tick could not take the writer.
    assert_eq!(scheduler.on_observation(&observation, NOW + 1).len(), 1);

    scheduler.poll_issued("case-1", "review_timeout", NOW - 5);
    assert!(
        scheduler.on_observation(&observation, NOW + 2).is_empty(),
        "a poll that landed is not repeated"
    );
}

#[test]
fn a_deadline_whose_poll_is_already_journaled_is_not_polled_again() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        due_deadlines: vec![DueDeadline {
            instance_id: "case-1".into(),
            deadline_name: "review_timeout".into(),
            due_ms: NOW - 5,
        }],
        claimed_request_ids: claimed(&[poll_rid("case-1", "review_timeout", NOW - 5)]),
        ..Observation::default()
    };
    assert!(scheduler.on_observation(&observation, NOW).is_empty());
}

#[test]
fn a_rescheduled_deadline_is_a_new_observation_and_is_polled_again() {
    let mut scheduler = Scheduler::new(table());
    let first = Observation {
        due_deadlines: vec![DueDeadline {
            instance_id: "case-1".into(),
            deadline_name: "review_timeout".into(),
            due_ms: NOW - 5,
        }],
        ..Observation::default()
    };
    assert_eq!(scheduler.on_observation(&first, NOW).len(), 1);
    scheduler.poll_issued("case-1", "review_timeout", NOW - 5);
    let second = Observation {
        due_deadlines: vec![DueDeadline {
            instance_id: "case-1".into(),
            deadline_name: "review_timeout".into(),
            due_ms: NOW + 500,
        }],
        ..Observation::default()
    };
    assert_eq!(scheduler.on_observation(&second, NOW + 600).len(), 1);
}

#[test]
fn a_cancelled_instance_kills_its_in_flight_handler_exactly_once() {
    let mut scheduler = Scheduler::new(table());
    let running = observation_with_pending(vec![effect("case-1/3/0", "assign_reviewer")]);
    assert_eq!(scheduler.on_observation(&running, NOW).len(), 1);

    let cancelled = Observation {
        pending: vec![effect("case-1/3/0", "assign_reviewer")],
        cancellations: vec!["case-1".into()],
        ..Observation::default()
    };
    let directives = scheduler.on_observation(&cancelled, NOW + 1);
    assert_eq!(
        directives,
        [Directive::Kill {
            effect_id: "case-1/3/0".into(),
            reason: KillReason::Cancelled,
        }]
    );

    // The watcher reports a cancellation on the scan that observes it, and the
    // scheduler will not re-direct a kill it has already directed.
    assert!(
        scheduler
            .on_observation(&cancelled, NOW + 2)
            .iter()
            .all(|directive| !matches!(directive, Directive::Kill { .. }))
    );
}

#[test]
fn a_run_past_its_timeout_is_killed_exactly_once_with_that_reason() {
    let mut scheduler = Scheduler::new(table());
    let observation = observation_with_pending(vec![effect("case-1/3/0", "assign_reviewer")]);
    assert_eq!(scheduler.on_observation(&observation, NOW).len(), 1);

    // The deadline is `now + timeout_ms`, so the instant it lands is not yet
    // past it.
    assert!(
        scheduler
            .on_observation(&observation, NOW + 30_000)
            .is_empty()
    );

    let directives = scheduler.on_observation(&observation, NOW + 30_001);
    assert_eq!(
        directives,
        [Directive::Kill {
            effect_id: "case-1/3/0".into(),
            reason: KillReason::Timeout,
        }]
    );
    assert!(
        scheduler
            .on_observation(&observation, NOW + 60_000)
            .is_empty()
    );
}

#[test]
fn a_cancel_and_a_timeout_together_report_the_cancel() {
    let mut scheduler = Scheduler::new(table());
    let observation = observation_with_pending(vec![effect("case-1/3/0", "assign_reviewer")]);
    scheduler.on_observation(&observation, NOW);
    let both = Observation {
        pending: vec![effect("case-1/3/0", "assign_reviewer")],
        cancellations: vec!["case-1".into()],
        ..Observation::default()
    };
    match &scheduler.on_observation(&both, NOW + 90_000)[0] {
        Directive::Kill { reason, .. } => assert_eq!(*reason, KillReason::Cancelled),
        other => panic!("expected a Kill, got {other:?}"),
    }
}

#[test]
fn an_argv_that_cannot_be_built_is_reported_rather_than_dropped() {
    // The handler's argv names `{case}`; this emit produced no such argument,
    // so the run has failed before it began. Silently skipping it would leave
    // the effect pending forever with no diagnostic anywhere.
    let mut scheduler = Scheduler::new(table());
    let mut effect = effect("case-1/3/0", "assign_reviewer");
    effect.args = BTreeMap::new();
    let observation = observation_with_pending(vec![effect]);

    assert!(scheduler.on_observation(&observation, NOW).is_empty());
    let unstartable = scheduler.unstartable();
    assert_eq!(unstartable.len(), 1);
    assert_eq!(unstartable[0].effect.effect_id, "case-1/3/0");
    assert_eq!(unstartable[0].error.code, "exec/config");
}

#[test]
fn a_pending_effect_whose_key_is_already_claimed_is_reported_as_stalled() {
    let mut scheduler = Scheduler::new(table());
    let observation = Observation {
        pending: vec![effect("case-1/0/0", "assign_reviewer")],
        claimed_request_ids: claimed(&[ack_rid("case-1/0/0")]),
        ..Observation::default()
    };
    assert!(scheduler.on_observation(&observation, NOW).is_empty());
    assert_eq!(scheduler.stalled(), ["case-1/0/0"]);
    // Reported once, not on every tick.
    assert!(scheduler.on_observation(&observation, NOW + 1).is_empty());
    assert!(scheduler.stalled().is_empty());
}

#[test]
fn an_effect_advanced_by_another_writer_is_not_also_killed_in_the_same_tick() {
    // A human acks the effect from the CLI while this executor's handler is
    // still running, and the run then passes its deadline. Both rules match;
    // one directive per effect per tick, and the kill waits a tick.
    let mut scheduler = Scheduler::new(table());
    let running = observation_with_pending(vec![effect("case-1/3/0", "assign_reviewer")]);
    assert_eq!(scheduler.on_observation(&running, NOW).len(), 1);

    let acked_elsewhere = Observation {
        settled: vec![settled("case-1/3/0", "assign_reviewer", "ok")],
        ..Observation::default()
    };
    let directives = scheduler.on_observation(&acked_elsewhere, NOW + 90_000);
    assert_eq!(directives.len(), 1);
    assert!(matches!(directives[0], Directive::SendEvent { .. }));

    let after = Observation {
        claimed_request_ids: claimed(&[event_rid("case-1/3/0", "assigned")]),
        ..Observation::default()
    };
    assert_eq!(
        scheduler.on_observation(&after, NOW + 90_001),
        [Directive::Kill {
            effect_id: "case-1/3/0".into(),
            reason: KillReason::Timeout,
        }]
    );
}

#[test]
fn completing_a_run_lets_the_next_observation_act_on_the_effect_again() {
    let mut scheduler = Scheduler::new(table());
    let observation = observation_with_pending(vec![effect("case-1/3/0", "assign_reviewer")]);
    assert_eq!(scheduler.on_observation(&observation, NOW).len(), 1);
    assert!(scheduler.on_observation(&observation, NOW).is_empty());

    // A settle that failed to journal leaves the effect pending; clearing the
    // in-flight entry is what lets the next tick retry rather than wedge.
    scheduler.complete("case-1/3/0");
    assert_eq!(scheduler.on_observation(&observation, NOW).len(), 1);
}

#[test]
fn a_fresh_scheduler_resumes_the_advance_a_killed_one_never_sent() {
    // The predecessor acked and died. Its successor has an empty in-flight map
    // and reads the same journal: the ack key is claimed, the event key is
    // not.
    let mut successor = Scheduler::new(table());
    let after_ack = Observation {
        settled: vec![settled("case-1/3/0", "assign_reviewer", "ok")],
        claimed_request_ids: claimed(&[ack_rid("case-1/3/0")]),
        ..Observation::default()
    };
    let directives = successor.on_observation(&after_ack, NOW);
    assert_eq!(directives.len(), 1);
    assert!(matches!(directives[0], Directive::SendEvent { .. }));
}

#[test]
fn the_same_observation_and_time_always_produce_the_same_directives() {
    let observation = Observation {
        pending: vec![
            effect("case-1/3/0", "assign_reviewer"),
            effect("case-2/4/0", "archive_case"),
        ],
        settled: vec![settled("case-3/5/0", "assign_reviewer", "ok")],
        due_deadlines: vec![DueDeadline {
            instance_id: "case-4".into(),
            deadline_name: "review_timeout".into(),
            due_ms: NOW - 1,
        }],
        ..Observation::default()
    };
    let first = Scheduler::new(table()).on_observation(&observation, NOW);
    let second = Scheduler::new(table()).on_observation(&observation, NOW);
    assert_eq!(first, second);
    assert_eq!(first.len(), 4, "two starts, one advance, one poll");
}
