//! History and explain rebuild a record's trace by re-applying it, and a
//! macrostep must be re-applied as the macrostep it was: under the macrostep
//! budget, so a legitimately deep cascade the live write accepted does not
//! reconstruct without a trace.
//!
//! Plan 0009 task 4602.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MACROSTEP_EVAL_TICKS, MAX_EVAL_TICKS, MAX_MICROSTEPS};
use fsm_core::machine::InstanceState;
use fsm_core::record::RecordKind;
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

/// `idle` leaves on `jump` — or after a deadline, or on `trip` with a
/// context change the invariant rejects at quiescence — into a guarded
/// eventless cycle `s → t → s` that runs exactly `MAX_MICROSTEPS` reactions.
/// The guard is deliberately wide: the macrostep costs several times
/// `MAX_EVAL_TICKS`, which is what makes the budget observable.
fn deep_machine() -> String {
    let rounds = MAX_MICROSTEPS / 2;
    // A balanced sum nests seven deep and evaluates a few hundred nodes.
    fn wide(depth: u32) -> String {
        if depth == 0 {
            "1".to_string()
        } else {
            format!("({} + {})", wide(depth - 1), wide(depth - 1))
        }
    }
    let wide = wide(7);
    format!(
        r#"{{"format":"fsm.machine/1","name":"deep","states":[{{"name":"idle"}},{{"name":"waiting"}},{{"name":"s"}},{{"name":"t"}}],"initial":"idle","context":[{{"name":"n","ty":"int","init":"0"}},{{"name":"tripped","ty":"bool","init":"false"}}],"events":[{{"name":"jump","fields":[]}},{{"name":"go","fields":[]}},{{"name":"trip","fields":[]}}],"deadlines":[{{"name":"expire","from":"waiting","after":"dur(1, s)","to":"s"}}],"invariants":[{{"name":"untripped","expr":"ctx.tripped == false"}}],"transitions":[{{"from":"idle","on":"jump","to":"s"}},{{"from":"idle","on":"go","to":"waiting"}},{{"from":"idle","on":"trip","to":"s","do":[{{"target":"tripped","value":"true"}}]}},{{"from":"s","if":"ctx.n < {rounds} and {wide} > 0","to":"t","do":[{{"target":"n","value":"ctx.n + 1"}}]}},{{"from":"t","to":"s"}}]}}"#
    )
}

/// The macrostep the suite reconstructs is one the standard budget cannot
/// run: the pure engine rejects it with `internal/budget`, so a store path
/// that still used that budget would rebuild these records without a trace.
#[test]
fn the_cascade_exceeds_the_standard_budget() {
    let spec =
        parse_machine(&parse(deep_machine().as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let machine = compile(spec).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let created = create(&machine, &tree, &BTreeMap::new(), 0).unwrap();
    let state = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let payload = Value::Obj(BTreeMap::new());
    let mut standard = Budget::new(MAX_EVAL_TICKS);
    let outcome = step(&machine, &tree, &state, "jump", &payload, 0, &mut standard);
    assert!(
        matches!(&outcome, Outcome::Rejected(_))
            && format!("{outcome:?}").contains("internal/budget"),
        "{outcome:?}"
    );
    let mut macrostep = Budget::new(MACROSTEP_EVAL_TICKS);
    let outcome = step(&machine, &tree, &state, "jump", &payload, 0, &mut macrostep);
    let Outcome::Applied(applied) = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(applied.trace.microsteps.len(), MAX_MICROSTEPS as usize);
}

fn session() -> Store {
    let src = deep_machine();
    let spec = parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(1_000, 1_000);
    store
        .define_machine_on(&mut clock, spec, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "deep",
            "inst-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn send(store: &mut Store, event: &str, request_id: &str) {
    let mut clock = FixedClock::new(10_000, 1_000);
    let _ = store.send_event_stamp_on(
        &mut clock,
        "inst-1",
        event,
        &mut Value::Obj(BTreeMap::new()),
        request_id,
        None,
        &[],
    );
}

fn seq_of(store: &Store, kind: RecordKind) -> u64 {
    store.records.iter().find(|r| r.kind == kind).unwrap().seq
}

fn trace_microsteps(entry: &Value) -> usize {
    entry
        .get("trace")
        .and_then(|trace| trace.get("microsteps"))
        .and_then(Value::as_arr)
        .map_or(0, <[Value]>::len)
}

fn history_entry(store: &Store, seq: u64) -> Value {
    let page = store.history_page("inst-1", 0, 500, true, true).unwrap();
    page.get("entries")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .find(|entry| entry.get("seq").and_then(Value::as_num) == Some(&seq.to_string()))
        .cloned()
        .unwrap()
}

#[test]
fn explain_and_history_rebuild_a_full_ceiling_event_macrostep() {
    let mut store = session();
    send(&mut store, "jump", "send-1");
    let seq = seq_of(&store, RecordKind::EventApplied);
    let explained = store.explain_seq("inst-1", seq).unwrap();
    assert_eq!(trace_microsteps(&explained), MAX_MICROSTEPS as usize);
    // The record's own claim rides along beside the rebuilt trace, and each
    // rebuilt microstep is a full section with its candidates and pipeline.
    let claimed = explained.get("microsteps").and_then(Value::as_arr).unwrap();
    assert_eq!(claimed.len(), MAX_MICROSTEPS as usize);
    let first = &explained
        .get("trace")
        .unwrap()
        .get("microsteps")
        .unwrap()
        .as_arr()
        .unwrap()[0];
    assert!(first.get("candidates").is_some() && first.get("pipeline").is_some());
    assert_eq!(
        trace_microsteps(&history_entry(&store, seq)),
        MAX_MICROSTEPS as usize
    );
}

#[test]
fn explain_and_history_rebuild_a_full_ceiling_deadline_macrostep() {
    let mut store = session();
    send(&mut store, "go", "send-1");
    let mut later = FixedClock::new(20_000, 1_000);
    store
        .poll_instance_deadline_on(&mut later, "inst-1", "poll-1", None)
        .unwrap();
    let seq = seq_of(&store, RecordKind::DeadlineApplied);
    assert_eq!(
        trace_microsteps(&store.explain_seq("inst-1", seq).unwrap()),
        MAX_MICROSTEPS as usize
    );
    assert_eq!(
        trace_microsteps(&history_entry(&store, seq)),
        MAX_MICROSTEPS as usize
    );
}

#[test]
fn explain_and_history_rebuild_a_rejection_after_a_full_cascade() {
    let mut store = session();
    send(&mut store, "trip", "send-1");
    let seq = seq_of(&store, RecordKind::EventRejected);
    let explained = store.explain_seq("inst-1", seq).unwrap();
    assert!(explained.get("trace").is_some(), "{explained:?}");
    assert_eq!(trace_microsteps(&explained), MAX_MICROSTEPS as usize);
    assert_eq!(
        trace_microsteps(&history_entry(&store, seq)),
        MAX_MICROSTEPS as usize
    );
}
