//! A journaled `microsteps` array is only worth writing if replay re-derives
//! it and compares — in both directions.
//!
//! Plan 0009 task 4602. Journals are built by hand from the pure engine, as
//! the other replay proofs do, so nothing here depends on the store.

use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, configuration_value, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MAX_MICROSTEPS;
use fsm_core::machine::{CompiledMachine, InstanceState};
use fsm_core::record::{Record, RecordKind, limits_value, microsteps_value, seal, zeros};
use fsm_core::replay::{NopSink, ReplayError, fold_with};
use fsm_core::step::{Applied, Outcome, create, step};
use fsm_core::tree::Tree;

/// Creation cascades (`boot → idle`), `go` cascades through a raise and an
/// eventless transition.
const REACTIVE: &str = r#"{"format":"fsm.machine/1","name":"reactive","states":[{"name":"boot"},{"name":"idle"},{"name":"working","entry":{"raise":[{"event":"tick"}]}},{"name":"ticked"},{"name":"waiting"}],"initial":"boot","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true}],"transitions":[{"from":"boot","to":"idle"},{"from":"idle","on":"go","to":"working"},{"from":"working","on":"tick","to":"ticked","do":[{"target":"n","value":"ctx.n + 1"}]},{"from":"ticked","to":"waiting"}]}"#;

const PLAIN: &str = r#"{"format":"fsm.machine/1","name":"plain","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"}]}"#;

struct Journal {
    records: Vec<Record>,
    machine_id: String,
    machine: CompiledMachine,
    tree: Tree,
    state: InstanceState,
}

fn push(records: &mut Vec<Record>, ts: i64, kind: RecordKind, body: BTreeMap<String, Value>) {
    let seq = records.len() as u64;
    let previous = records.last().map(|r| r.hash.clone()).unwrap_or_else(zeros);
    records.push(seal(seq, ts, kind, Value::Obj(body), &previous));
}

fn instance(applied: &Applied, pending: Vec<String>) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after.clone(),
        ctx: applied.ctx_after.clone(),
        history: applied.history_after.clone(),
        deadlines: applied.deadlines_after.clone(),
        pending,
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

/// Genesis, the definition, and a creation record whose `microsteps` come
/// from the real engine.
fn journal(src: &str) -> Journal {
    let definition = parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let machine = fsm_core::spec::compile_accepted(&definition).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    let machine_id = machine.machine_id.clone();
    let created = create(&machine, &tree, &BTreeMap::new(), 100).unwrap();
    let state = instance(&created, Vec::new());
    let mut records = Vec::new();
    push(
        &mut records,
        0,
        RecordKind::Genesis,
        BTreeMap::from([
            ("format".into(), Value::Str("fsm.journal/1".into())),
            ("created_ts".into(), Value::Num("0".into())),
            ("limits".into(), limits_value()),
        ]),
    );
    push(
        &mut records,
        1,
        RecordKind::MachineDefined,
        BTreeMap::from([
            ("machine_id".into(), Value::Str(machine_id.clone())),
            ("def".into(), definition),
        ]),
    );
    let mut body = BTreeMap::from([
        ("instance_id".into(), Value::Str("i".into())),
        ("machine_id".into(), Value::Str(machine_id.clone())),
        ("request_id".into(), Value::Str("create".into())),
        (
            "state_hash".into(),
            Value::Str(state_hash(&machine_id, "i", 2, &state)),
        ),
        ("state_format".into(), Value::Str(STATE_FORMAT.into())),
        (
            "configuration".into(),
            configuration_value(&state.configuration),
        ),
        ("overrides".into(), Value::Obj(BTreeMap::new())),
    ]);
    if let Some(microsteps) = microsteps_value(&created.trace.microsteps) {
        body.insert("microsteps".into(), microsteps);
    }
    push(&mut records, 100, RecordKind::InstanceCreated, body);
    Journal {
        records,
        machine_id,
        machine,
        tree,
        state,
    }
}

/// Append an `event_applied` for `go` at the journal's next seq.
fn append_go(journal: &mut Journal) -> Applied {
    let mut budget = fsm_core::expr::eval::Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
    let out = match step(
        &journal.machine,
        &journal.tree,
        &journal.state,
        "go",
        &Value::Obj(BTreeMap::new()),
        200,
        &mut budget,
    ) {
        Outcome::Applied(applied) => applied,
        other => panic!("{other:?}"),
    };
    let seq = journal.records.len() as u64;
    let pending: Vec<String> = out
        .effects
        .iter()
        .map(|e| format!("i/{seq}/{}", e.k))
        .collect();
    let next = instance(&out, pending);
    let mut body = BTreeMap::from([
        ("instance_id".into(), Value::Str("i".into())),
        ("event".into(), Value::Str("go".into())),
        ("payload".into(), Value::Obj(BTreeMap::new())),
        ("request_id".into(), Value::Str("send".into())),
        (
            "state_hash".into(),
            Value::Str(state_hash(&journal.machine_id, "i", seq, &next)),
        ),
        ("state_format".into(), Value::Str(STATE_FORMAT.into())),
        ("source_state".into(), Value::Str(out.source_state.clone())),
        (
            "exited".into(),
            Value::Arr(out.exited.iter().cloned().map(Value::Str).collect()),
        ),
        (
            "entered".into(),
            Value::Arr(out.entered.iter().cloned().map(Value::Str).collect()),
        ),
    ]);
    if let Some(microsteps) = microsteps_value(&out.trace.microsteps) {
        body.insert("microsteps".into(), microsteps);
    }
    push(&mut journal.records, 200, RecordKind::EventApplied, body);
    journal.state = next;
    out
}

/// Re-seal every record from `from` on so a tampered body keeps a valid chain;
/// this exercises replay, not the hash chain.
fn reseal(records: &mut [Record], from: usize) {
    for seq in from..records.len() {
        let previous = if seq == 0 {
            zeros()
        } else {
            records[seq - 1].hash.clone()
        };
        let record = &records[seq];
        records[seq] = seal(
            record.seq,
            record.ts,
            record.kind,
            record.body.clone(),
            &previous,
        );
    }
}

fn with_body(records: &mut [Record], seq: usize, edit: impl FnOnce(&mut BTreeMap<String, Value>)) {
    let mut body = records[seq].body.as_obj().unwrap().clone();
    edit(&mut body);
    records[seq].body = Value::Obj(body);
    reseal(records, seq);
}

#[test]
fn a_reactive_journal_folds_and_reproduces_every_state_hash() {
    let mut journal = journal(REACTIVE);
    let out = append_go(&mut journal);
    assert_eq!(
        out.trace.microsteps.len(),
        2,
        "the raise and the eventless transition"
    );
    let folded = fold_with(journal.records.clone(), &mut NopSink).unwrap();
    assert_eq!(folded.instances["i"], journal.state);
    assert_eq!(folded.last_seq, 3);
}

#[test]
fn a_tampered_entry_fails_naming_the_seq_and_the_microstep() {
    let mut journal = journal(REACTIVE);
    append_go(&mut journal);
    with_body(&mut journal.records, 3, |body| {
        let Some(Value::Arr(microsteps)) = body.get_mut("microsteps") else {
            panic!("the cascading record carries microsteps");
        };
        let Value::Obj(second) = &mut microsteps[1] else {
            panic!("a microstep entry is an object");
        };
        second.insert(
            "entered".into(),
            Value::Arr(vec![Value::Str("idle".into())]),
        );
    });
    assert_eq!(
        fold_with(journal.records.clone(), &mut NopSink).unwrap_err(),
        ReplayError::MicrostepMismatch { seq: 3, index: 2 }
    );
}

#[test]
fn deleting_the_key_from_a_cascading_record_fails() {
    let mut cascading = journal(REACTIVE);
    append_go(&mut cascading);
    with_body(&mut cascading.records, 3, |body| {
        body.remove("microsteps");
    });
    assert_eq!(
        fold_with(cascading.records.clone(), &mut NopSink).unwrap_err(),
        ReplayError::MicrostepMismatch { seq: 3, index: 1 }
    );
    // The same for creation's own cascade.
    let mut fresh = journal(REACTIVE);
    with_body(&mut fresh.records, 2, |body| {
        body.remove("microsteps");
    });
    assert_eq!(
        fold_with(fresh.records.clone(), &mut NopSink).unwrap_err(),
        ReplayError::MicrostepMismatch { seq: 2, index: 1 }
    );
}

#[test]
fn a_spurious_key_on_a_non_reactive_record_fails() {
    let mut journal = journal(PLAIN);
    append_go(&mut journal);
    assert!(journal.records[3].body.get("microsteps").is_none());
    with_body(&mut journal.records, 3, |body| {
        body.insert(
            "microsteps".into(),
            Value::Arr(vec![Value::Obj(BTreeMap::from([
                ("index".into(), Value::Num("1".into())),
                ("trigger".into(), Value::Str("eventless".into())),
                ("source_state".into(), Value::Str("b".into())),
                ("transition_idx".into(), Value::Num("0".into())),
                ("exited".into(), Value::Arr(vec![Value::Str("b".into())])),
                ("entered".into(), Value::Arr(vec![Value::Str("a".into())])),
            ]))]),
        );
    });
    assert_eq!(
        fold_with(journal.records.clone(), &mut NopSink).unwrap_err(),
        ReplayError::MicrostepMismatch { seq: 3, index: 1 }
    );
}

#[test]
fn a_full_ceiling_macrostep_replays_under_the_macrostep_budget() {
    let states: Vec<String> = (0..=MAX_MICROSTEPS)
        .map(|i| format!(r#"{{"name":"s{i}"}}"#))
        .collect();
    let transitions: Vec<String> = (0..MAX_MICROSTEPS)
        .map(|i| format!(r#"{{"from":"s{i}","to":"s{}"}}"#, i + 1))
        .collect();
    let src = format!(
        r#"{{"format":"fsm.machine/1","name":"deep","states":[{}],"initial":"s0","context":[],"events":[],"transitions":[{}]}}"#,
        states.join(","),
        transitions.join(",")
    );
    let journal = journal(&src);
    let created = &journal.records[2];
    assert_eq!(
        created
            .body
            .get("microsteps")
            .and_then(Value::as_arr)
            .unwrap()
            .len(),
        MAX_MICROSTEPS as usize
    );
    let folded = fold_with(journal.records.clone(), &mut NopSink).unwrap();
    assert_eq!(
        folded.instances["i"].configuration.sequential_leaf(),
        Some("s64")
    );
}
