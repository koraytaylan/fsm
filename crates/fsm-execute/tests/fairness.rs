//! Who gets the scarce slots.
//!
//! `7601` established that an ordering exists and is applied before the caps.
//! This file is about *which* ordering, and the property it has to have:
//! ordering candidates by `effect_id` alone would let the
//! lexicographically-first instance take every slot forever — a starvation bug
//! that only shows up in the stores big enough to matter, which is the worst
//! kind to ship.
//!
//! The round-robin is `(position within the instance's own candidate queue,
//! instance_id, effect_id)`, computed from one observation and nothing else.
//! Every row here is a fabricated observation: the scheduler is pure, so there
//! is no store, no subprocess, and no clock.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::expr::eval::Val;
use fsm_execute::config::HandlerTable;
use fsm_execute::effect::PendingEffect;
use fsm_execute::rid::attempt_rid;
use fsm_execute::sched::{Directive, Scheduler, ready_at};
use fsm_execute::watch::{AttemptState, Observation};

const NOW: i64 = 1_700_000_000_000;

fn table(caps: &str) -> HandlerTable {
    HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",{caps}
            "handlers":[{{
                "effect":"notify",
                "argv":["/usr/local/bin/notify","--case","{{case}}"],
                "timeout_ms":30000,
                "retry":{{"attempts":3,"backoff_ms":1000,"max_backoff_ms":8000}},
                "on_ok":{{"event":"notified"}},
                "on_failed":{{"event":"notify_failed"}}
            }}]
        }}"#
    ))
    .expect("the table validates")
}

fn effect(instance: &str, k: u32) -> PendingEffect {
    PendingEffect {
        instance_id: instance.to_string(),
        effect_id: format!("{instance}/3/{k:03}"),
        effect_name: "notify".to_string(),
        args: BTreeMap::from([("case".to_string(), Val::Str("case-9".into()))]),
        emitted_seq: 3,
        k,
    }
}

fn instance_name(index: u32) -> String {
    format!("case-{index:03}")
}

/// One observation holding `(instance, count)` pending effects apiece.
fn observation(shape: &[(u32, u32)]) -> Observation {
    let mut pending = Vec::new();
    for (instance, count) in shape {
        for k in 0..*count {
            pending.push(effect(&instance_name(*instance), k));
        }
    }
    Observation {
        pending,
        ..Observation::default()
    }
}

fn started(directives: &[Directive]) -> Vec<String> {
    directives
        .iter()
        .filter_map(|directive| match directive {
            Directive::Start { effect, .. } => Some(effect.effect_id.clone()),
            _ => None,
        })
        .collect()
}

fn instances_of(effect_ids: &[String]) -> Vec<String> {
    effect_ids
        .iter()
        .map(|effect_id| {
            effect_id
                .split('/')
                .next()
                .expect("an effect id names its instance")
                .to_string()
        })
        .collect()
}

#[test]
fn a_busy_instance_never_crowds_out_the_quiet_ones() {
    // The shape the ordering exists for: `case-000` has a hundred effects
    // queued and sorts first, and nine other instances have one each.
    let mut shape = vec![(0, 100)];
    shape.extend((1..10).map(|instance| (instance, 1)));
    let mut scheduler = Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":2,"#));
    let starts = started(&scheduler.on_observation(&observation(&shape), NOW));
    assert_eq!(starts.len(), 8);

    let touched: BTreeSet<String> = instances_of(&starts).into_iter().collect();
    assert_eq!(
        touched.len(),
        8,
        "eight instances, not eight of one: {starts:?}"
    );
    let busy = starts
        .iter()
        .filter(|effect_id| effect_id.starts_with("case-000/"))
        .count();
    assert_eq!(busy, 1, "the busy instance takes one slot in the round");
}

#[test]
fn nobody_is_starved_across_ten_ticks_with_completions() {
    // Ten instances, one enormous and nine with a single effect each, driven
    // for ten ticks. "With completions" means what it means in the journal:
    // an acked effect leaves its instance's outbox, so the observation the
    // next tick sees is a smaller one. A test that fed the same observation
    // back would be modelling an executor whose acks never land.
    let mut outbox: Vec<PendingEffect> = (0..100).map(|k| effect(&instance_name(0), k)).collect();
    outbox.extend((1..10).map(|instance| effect(&instance_name(instance), 0)));

    let mut scheduler = Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":2,"#));
    let mut served: BTreeMap<String, usize> = BTreeMap::new();
    for tick in 0..10 {
        let observation = Observation {
            pending: outbox.clone(),
            ..Observation::default()
        };
        let starts = started(&scheduler.on_observation(&observation, NOW + tick));
        assert!(!starts.is_empty(), "tick {tick} started nothing");
        // The busy instance is bounded to its per-instance cap every round,
        // which is the whole point: it cannot buy more of the host by having
        // more work queued.
        let from_busy = starts
            .iter()
            .filter(|effect_id| effect_id.starts_with("case-000/"))
            .count();
        assert!(
            from_busy <= 2,
            "tick {tick} gave the busy instance {from_busy}"
        );
        for instance in instances_of(&starts) {
            *served.entry(instance).or_default() += 1;
        }
        for effect_id in &starts {
            scheduler.complete(effect_id);
        }
        outbox.retain(|effect| !starts.contains(&effect.effect_id));
    }
    assert_eq!(served.len(), 10, "every instance was served: {served:?}");
    // The nine quiet instances are done well before the busy one, which still
    // has most of its hundred left: fairness is about turns, not throughput.
    for instance in 1..10 {
        assert_eq!(served[&instance_name(instance)], 1);
    }
    assert!(
        outbox
            .iter()
            .all(|effect| effect.instance_id == instance_name(0)),
        "only the busy instance still has work"
    );
}

#[test]
fn the_quiet_instances_are_all_served_within_two_rounds() {
    // The shape the ordering exists for, stated as the arithmetic it is:
    // eight slots cannot serve ten instances in one tick, so what fairness
    // buys is that the ninth and tenth are served in the *next* one rather
    // than never.
    let mut outbox: Vec<PendingEffect> = (0..100).map(|k| effect(&instance_name(0), k)).collect();
    outbox.extend((1..10).map(|instance| effect(&instance_name(instance), 0)));

    let mut scheduler = Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":2,"#));
    let mut served: BTreeSet<String> = BTreeSet::new();
    for tick in 0..2 {
        let observation = Observation {
            pending: outbox.clone(),
            ..Observation::default()
        };
        let starts = started(&scheduler.on_observation(&observation, NOW + tick));
        served.extend(instances_of(&starts));
        for effect_id in &starts {
            scheduler.complete(effect_id);
        }
        outbox.retain(|effect| !starts.contains(&effect.effect_id));
    }
    assert_eq!(served.len(), 10, "{served:?}");
}

#[test]
fn the_slots_are_spread_evenly_across_a_round() {
    // Ten instances with four each and eight slots: the ordering must not
    // give the low-sorting instances a disproportionate share.
    let shape: Vec<(u32, u32)> = (0..10).map(|instance| (instance, 4)).collect();
    let mut scheduler = Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":4,"#));
    let starts = started(&scheduler.on_observation(&observation(&shape), NOW));
    let mut per_instance: BTreeMap<String, usize> = BTreeMap::new();
    for instance in instances_of(&starts) {
        *per_instance.entry(instance).or_default() += 1;
    }
    assert!(
        per_instance.values().all(|count| *count == 1),
        "eight slots over ten instances is one apiece: {per_instance:?}"
    );
    // And they are the first eight in instance order, deterministically.
    assert_eq!(
        instances_of(&starts),
        (0..8).map(instance_name).collect::<Vec<_>>()
    );
}

#[test]
fn an_instance_at_its_per_instance_cap_loses_its_turn_and_not_the_slot() {
    // Two instances: the first has three effects and a per-instance cap of
    // one, the second has three and is free to take what the first cannot.
    let mut scheduler = Scheduler::new(table(r#""max_inflight":4,"max_inflight_per_instance":1,"#));
    let narrow = observation(&[(0, 3), (1, 3)]);
    let starts = started(&scheduler.on_observation(&narrow, NOW));
    // Four global slots, one per instance, two instances: two starts, and the
    // other two slots are simply unusable this tick — not silently given to
    // the instance that already has its one.
    assert_eq!(starts, ["case-000/3/000", "case-001/3/000"]);
    assert_eq!(
        scheduler
            .capped()
            .expect("four candidates were skipped")
            .deferred,
        4
    );

    // Now the same shape with a third instance that is *not* at its cap: the
    // slot the capped instances cannot use goes to it rather than being lost.
    let mut roomier = Scheduler::new(table(r#""max_inflight":4,"max_inflight_per_instance":1,"#));
    let wider = observation(&[(0, 3), (1, 3), (2, 3), (3, 3)]);
    let starts = started(&roomier.on_observation(&wider, NOW));
    assert_eq!(
        starts.len(),
        4,
        "one apiece from four instances: {starts:?}"
    );
    assert_eq!(
        instances_of(&starts),
        (0..4).map(instance_name).collect::<Vec<_>>()
    );
}

#[test]
fn an_effect_in_backoff_neither_takes_a_position_nor_blocks_its_instance() {
    // `case-000`'s first effect failed a moment ago and is inside its backoff
    // window; its second has never run. The first must not occupy position 0
    // and push the second out of the round.
    let handlers = table(r#""max_inflight":8,"max_inflight_per_instance":2,"#);
    let retry = &handlers.handlers["notify"].retry;
    let due = ready_at(retry, 1, NOW);

    let mut obs = observation(&[(0, 2), (1, 1)]);
    obs.attempts = BTreeMap::from([(
        "case-000/3/000".to_string(),
        AttemptState {
            attempt: 1,
            last_ts: NOW,
        },
    )]);
    obs.claimed_request_ids = BTreeSet::from([attempt_rid("case-000/3/000", 1)]);

    let mut scheduler = Scheduler::new(handlers);
    let starts = started(&scheduler.on_observation(&obs, due - 1));
    // The waiting effect is not a candidate, so its sibling takes position 0
    // for the instance and starts in the same round as `case-001`'s.
    assert_eq!(starts, ["case-000/3/001", "case-001/3/000"]);
    assert_eq!(scheduler.deferred(), ["case-000/3/000"]);
    // Backoff is not a concurrency deferral: nothing was held back by a cap.
    assert_eq!(scheduler.capped(), None);
}

#[test]
fn a_hundred_runs_and_a_fresh_scheduler_produce_the_same_ordering() {
    let handlers = table(r#""max_inflight":8,"max_inflight_per_instance":2,"#);
    let shape: Vec<(u32, u32)> = (0..12).map(|instance| (instance, 5)).collect();
    let observation = observation(&shape);
    let rendered = |directives: &[Directive]| format!("{directives:?}");

    let mut first = Scheduler::new(handlers.clone());
    let expected = rendered(&first.on_observation(&observation, NOW));
    for run in 0..100 {
        let mut scheduler = Scheduler::new(handlers.clone());
        assert_eq!(
            rendered(&scheduler.on_observation(&observation, NOW)),
            expected,
            "run {run} differed"
        );
    }
}

#[test]
fn the_ordering_is_a_function_of_the_observation_and_not_of_tick_history() {
    // Two schedulers, one with a busy past and one fresh, fed the identical
    // observation. If the ordering remembered anything between ticks these
    // would differ — and restart equivalence would be gone with it.
    let handlers = table(r#""max_inflight":8,"max_inflight_per_instance":2,"#);
    let shape: Vec<(u32, u32)> = (0..6).map(|instance| (instance, 4)).collect();
    let observation = observation(&shape);

    let mut experienced = Scheduler::new(handlers.clone());
    for tick in 0..5 {
        let starts = started(&experienced.on_observation(&observation, NOW + tick));
        for effect_id in &starts {
            experienced.complete(effect_id);
        }
    }
    let after_history = started(&experienced.on_observation(&observation, NOW + 5));

    let mut fresh = Scheduler::new(handlers);
    let from_nothing = started(&fresh.on_observation(&observation, NOW + 5));
    assert_eq!(after_history, from_nothing);
}

#[test]
fn one_instance_degenerates_to_effect_id_order() {
    // With nothing to round-robin against, the ordering is exactly `7601`'s.
    let mut scheduler =
        Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":16,"#));
    let starts = started(&scheduler.on_observation(&observation(&[(0, 20)]), NOW));
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted);
    assert_eq!(starts.len(), 8);
    assert_eq!(starts[0], "case-000/3/000");
    assert_eq!(starts[7], "case-000/3/007");
}

#[test]
fn the_round_robin_survives_a_gap_in_one_instances_queue() {
    // Instances do not have to be the same size, and the round has to keep
    // working when one runs out: `case-000` has one effect, `case-001` has
    // three. After position 0 there is nothing left of `case-000`, and the
    // remaining slots must go to `case-001` rather than stalling the round.
    let mut scheduler = Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":8,"#));
    let starts = started(&scheduler.on_observation(&observation(&[(0, 1), (1, 3)]), NOW));
    assert_eq!(
        starts,
        [
            "case-000/3/000",
            "case-001/3/000",
            "case-001/3/001",
            "case-001/3/002",
        ]
    );
}

#[test]
fn more_permanently_busy_instances_than_slots_means_the_last_ones_wait() {
    // The honest limit of a round-robin that is a pure function of one
    // observation. The tie-break at each position is `instance_id`, and an
    // ordering with no memory between ticks cannot rotate it — so ten
    // instances that never run out of work, against eight slots, leaves the
    // two highest-sorting ones waiting until one of the other eight empties.
    //
    // A rotating cursor would close that window and would cost restart
    // equivalence with it: two executors reading the same journal prefix would
    // disagree about whose turn it was. What the round-robin buys is that no
    // instance can convert *more queued work* into *more of the host* — which
    // is the starvation that shows up in real stores. It is not a
    // time-sharing scheduler, and this row says so out loud rather than
    // leaving a reader to discover it.
    let mut outbox: Vec<PendingEffect> = Vec::new();
    for instance in 0..10 {
        for k in 0..10 {
            outbox.push(effect(&instance_name(instance), k));
        }
    }
    let mut scheduler = Scheduler::new(table(r#""max_inflight":8,"max_inflight_per_instance":1,"#));
    let mut served: BTreeSet<String> = BTreeSet::new();
    for tick in 0..5 {
        let observation = Observation {
            pending: outbox.clone(),
            ..Observation::default()
        };
        let starts = started(&scheduler.on_observation(&observation, NOW + tick));
        assert_eq!(starts.len(), 8);
        served.extend(instances_of(&starts));
        for effect_id in &starts {
            scheduler.complete(effect_id);
        }
        outbox.retain(|effect| !starts.contains(&effect.effect_id));
    }
    assert_eq!(
        served,
        (0..8).map(instance_name).collect::<BTreeSet<_>>(),
        "the two highest-sorting instances wait for a queue to empty"
    );
    // And they are not lost: each of the eight is nine effects lighter, so the
    // wait is bounded by the work in front of them rather than open-ended.
    let remaining = outbox
        .iter()
        .filter(|effect| effect.instance_id == instance_name(0))
        .count();
    assert_eq!(remaining, 5);
}
