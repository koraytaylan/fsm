//! The macrostep record shape and its compatibility anchor.
//!
//! Plan 0009 task 4601. One optional key, `microsteps`, carries the whole
//! reaction; its **absence** on a non-reactive machine is what keeps every
//! existing store's bytes — and this suite proves the bytes did not move by
//! replaying a fixed session against a journal the pre-change build wrote.
//! Regenerate that journal only from a build that predates the key:
//! `FSM_REGEN_FIXTURES=1 cargo test -p fsm-store --test macrostep_records`.

use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MAX_MICROSTEPS, MAX_PAYLOAD_BYTES};
use fsm_core::record::{Record, RecordKind, limits_value, verify_line, zeros};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/non_reactive_session.journal"
);

const NON_REACTIVE: &str = r#"{"format":"fsm.machine/1","name":"plain","states":[{"name":"a"},{"name":"b"},{"name":"c","terminal":true}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[{"name":"by","ty":"int"}]},{"name":"stop","fields":[]}],"effects":[{"name":"fx","fields":[]}],"deadlines":[{"name":"expire","from":"b","after":"dur(2, s)","to":"c"}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"ctx.n + evt.by"}],"emit":[{"effect":"fx"}]},{"from":"b","on":"stop","to":"c"}]}"#;

/// A machine whose creation cascades, whose event cascades, and whose
/// deadline cascades: every reactive record kind in one definition.
const REACTIVE: &str = r#"{"format":"fsm.machine/1","name":"reactive","states":[{"name":"boot"},{"name":"idle"},{"name":"working","entry":{"raise":[{"event":"tick"}]}},{"name":"ticked"},{"name":"waiting"},{"name":"expired"},{"name":"done","terminal":true}],"initial":"boot","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true},{"name":"settle","fields":[]}],"deadlines":[{"name":"expire","from":"waiting","after":"dur(1, s)","to":"expired"}],"transitions":[{"from":"boot","to":"idle"},{"from":"idle","on":"go","to":"working"},{"from":"working","on":"tick","to":"ticked","do":[{"target":"n","value":"ctx.n + 1"}]},{"from":"ticked","to":"waiting"},{"from":"expired","to":"done"}]}"#;

fn spec(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn session(src: &str) -> Store {
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(1_000, 1_000);
    store
        .define_machine_on(&mut clock, spec(src), false, false)
        .unwrap();
    let name = spec(src)
        .get("name")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    store
        .create_instance_ctx_on(
            &mut clock,
            &name,
            "inst-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn body_of(store: &Store, kind: RecordKind) -> Vec<&Value> {
    store
        .records
        .iter()
        .filter(|r| r.kind == kind)
        .map(|r| &r.body)
        .collect()
}

fn journal_lines(store: &Store) -> Vec<u8> {
    store.records.iter().flat_map(Record::to_line).collect()
}

#[test]
fn a_non_reactive_session_writes_the_bytes_the_pre_change_build_wrote() {
    let mut store = session(NON_REACTIVE);
    let mut clock = FixedClock::new(10_000, 1_000);
    let mut payload = Value::Obj(BTreeMap::from([("by".to_string(), Value::Str("3".into()))]));
    store
        .send_event_stamp_on(
            &mut clock,
            "inst-1",
            "go",
            &mut payload,
            "send-1",
            None,
            &[],
        )
        .unwrap();
    store
        .poll_instance_deadline_on(&mut clock, "inst-1", "poll-1", None)
        .unwrap();
    let mut late = FixedClock::new(20_000, 1_000);
    store
        .poll_instance_deadline_on(&mut late, "inst-1", "poll-2", None)
        .unwrap();
    let bytes = journal_lines(&store);
    if std::env::var_os("FSM_REGEN_FIXTURES").is_some() {
        std::fs::write(FIXTURE, &bytes).unwrap();
    }
    let committed = std::fs::read(FIXTURE).expect("the pre-change journal fixture is committed");
    assert!(
        comparable(&bytes) == comparable(&committed),
        "a non-reactive machine's journal bytes moved; the microsteps key must stay absent"
    );
    // And the committed journal itself still folds, under the format its own
    // records declare.
    let mut previous = zeros();
    let mut records = Vec::new();
    for (seq, line) in committed
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        let record = verify_line(line, seq as u64, &previous).expect("a committed record verifies");
        previous = record.hash.clone();
        records.push(record);
    }
    fsm_core::replay::fold_with(records, &mut fsm_core::replay::NopSink)
        .expect("the pre-change journal still folds under the format its records declare");
    for kind in [
        RecordKind::InstanceCreated,
        RecordKind::EventApplied,
        RecordKind::DeadlineApplied,
    ] {
        for body in body_of(&store, kind) {
            assert!(body.get("microsteps").is_none(), "{kind:?} carries no key");
        }
    }
}

#[test]
fn a_reactive_session_records_the_cascade_in_every_kind() {
    let mut store = session(REACTIVE);
    let created = body_of(&store, RecordKind::InstanceCreated)[0].clone();
    let microsteps = created
        .get("microsteps")
        .and_then(Value::as_arr)
        .expect("creation cascaded");
    assert_eq!(microsteps.len(), 1);
    assert_eq!(
        microsteps[0].get("index").and_then(Value::as_num),
        Some("1")
    );
    assert_eq!(
        microsteps[0].get("trigger").and_then(Value::as_str),
        Some("eventless")
    );
    assert!(microsteps[0].get("event").is_none());
    assert_eq!(
        microsteps[0].get("source_state").and_then(Value::as_str),
        Some("boot")
    );

    let mut clock = FixedClock::new(10_000, 1_000);
    store
        .send_event_stamp_on(
            &mut clock,
            "inst-1",
            "go",
            &mut Value::Obj(BTreeMap::new()),
            "send-1",
            None,
            &[],
        )
        .unwrap();
    let applied = body_of(&store, RecordKind::EventApplied)[0].clone();
    let microsteps = applied.get("microsteps").and_then(Value::as_arr).unwrap();
    let triggers: Vec<(&str, Option<&str>)> = microsteps
        .iter()
        .map(|m| {
            (
                m.get("trigger").and_then(Value::as_str).unwrap(),
                m.get("event").and_then(Value::as_str),
            )
        })
        .collect();
    assert_eq!(triggers, [("internal", Some("tick")), ("eventless", None)]);
    assert_eq!(
        microsteps[0]
            .get("exited")
            .and_then(Value::as_arr)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        microsteps[1]
            .get("entered")
            .and_then(Value::as_arr)
            .unwrap()[0]
            .as_str(),
        Some("waiting")
    );
    // The record's top-level fields describe the trigger only.
    assert_eq!(
        applied.get("source_state").and_then(Value::as_str),
        Some("idle")
    );
    assert_eq!(
        applied.get("entered").and_then(Value::as_arr).unwrap()[0].as_str(),
        Some("working")
    );

    let mut later = FixedClock::new(20_000, 1_000);
    store
        .poll_instance_deadline_on(&mut later, "inst-1", "poll-1", None)
        .unwrap();
    let deadline = body_of(&store, RecordKind::DeadlineApplied)[0].clone();
    assert_eq!(
        deadline.get("deadline_idx").and_then(Value::as_num),
        Some("0")
    );
    let microsteps = deadline.get("microsteps").and_then(Value::as_arr).unwrap();
    assert_eq!(microsteps.len(), 1);
    assert_eq!(
        microsteps[0]
            .get("entered")
            .and_then(Value::as_arr)
            .unwrap()[0]
            .as_str(),
        Some("done")
    );
    // Every record still verifies as a canonical, well-formed line.
    let mut prev = zeros();
    for (seq, record) in store.records.iter().enumerate() {
        let verified = verify_line(&record.to_line(), seq as u64, &prev).unwrap();
        prev = verified.hash;
    }
}

#[test]
fn a_full_cascade_stays_under_the_payload_ceiling() {
    let states: Vec<String> = (0..=MAX_MICROSTEPS)
        .map(|i| format!(r#"{{"name":"long_state_name_number_{i}"}}"#))
        .collect();
    let transitions: Vec<String> = (0..MAX_MICROSTEPS)
        .map(|i| {
            format!(
                r#"{{"from":"long_state_name_number_{i}","to":"long_state_name_number_{}"}}"#,
                i + 1
            )
        })
        .collect();
    let src = format!(
        r#"{{"format":"fsm.machine/1","name":"deep","states":[{}],"initial":"long_state_name_number_0","context":[],"events":[],"transitions":[{}]}}"#,
        states.join(","),
        transitions.join(",")
    );
    let store = session(&src);
    let created = &store
        .records
        .iter()
        .find(|r| r.kind == RecordKind::InstanceCreated)
        .unwrap();
    let microsteps = created
        .body
        .get("microsteps")
        .and_then(Value::as_arr)
        .unwrap();
    assert_eq!(microsteps.len(), MAX_MICROSTEPS as usize);
    let bytes = fsm_core::canon::canon_bytes(&created.body);
    assert!(bytes.len() < MAX_PAYLOAD_BYTES, "{} bytes", bytes.len());
}

#[test]
fn a_journaled_empty_or_malformed_microsteps_key_is_refused() {
    let mut store = session(NON_REACTIVE);
    let mut clock = FixedClock::new(10_000, 1_000);
    let mut payload = Value::Obj(BTreeMap::from([("by".to_string(), Value::Str("3".into()))]));
    store
        .send_event_stamp_on(
            &mut clock,
            "inst-1",
            "go",
            &mut payload,
            "send-1",
            None,
            &[],
        )
        .unwrap();
    let applied = store
        .records
        .iter()
        .find(|r| r.kind == RecordKind::EventApplied)
        .unwrap();
    let reseal = |body: Value| {
        let record =
            fsm_core::record::seal(applied.seq, applied.ts, applied.kind, body, &applied.prev);
        verify_line(&record.to_line(), applied.seq, &applied.prev)
    };
    let mut empty = applied.body.as_obj().unwrap().clone();
    empty.insert("microsteps".into(), Value::Arr(vec![]));
    assert!(
        reseal(Value::Obj(empty)).is_err(),
        "empty is malformed: absent, never empty"
    );
    let mut bad_index = applied.body.as_obj().unwrap().clone();
    bad_index.insert(
        "microsteps".into(),
        Value::Arr(vec![Value::Obj(BTreeMap::from([
            ("index".into(), Value::Num("0".into())),
            ("trigger".into(), Value::Str("eventless".into())),
            ("source_state".into(), Value::Str("a".into())),
            ("transition_idx".into(), Value::Num("0".into())),
            ("exited".into(), Value::Arr(vec![])),
            ("entered".into(), Value::Arr(vec![])),
        ]))]),
    );
    assert!(reseal(Value::Obj(bad_index)).is_err(), "indices start at 1");
    let mut internal_without_event = applied.body.as_obj().unwrap().clone();
    internal_without_event.insert(
        "microsteps".into(),
        Value::Arr(vec![Value::Obj(BTreeMap::from([
            ("index".into(), Value::Num("1".into())),
            ("trigger".into(), Value::Str("internal".into())),
            ("source_state".into(), Value::Str("a".into())),
            ("transition_idx".into(), Value::Num("0".into())),
            ("exited".into(), Value::Arr(vec![])),
            ("entered".into(), Value::Arr(vec![])),
        ]))]),
    );
    assert!(
        reseal(Value::Obj(internal_without_event)).is_err(),
        "an internal entry names its event"
    );
}

#[test]
fn state_hash_commits_the_state_after_the_whole_macrostep() {
    let store = session(REACTIVE);
    let created = store
        .records
        .iter()
        .find(|r| r.kind == RecordKind::InstanceCreated)
        .unwrap();
    let instance = store.state.instances.get("inst-1").unwrap();
    assert_eq!(
        instance.configuration.sequential_leaf(),
        Some("idle"),
        "past the creation cascade"
    );
    let machine_id = created
        .body
        .get("machine_id")
        .and_then(Value::as_str)
        .unwrap();
    let by_hand = fsm_core::hashes::state_hash(machine_id, "inst-1", created.seq, instance);
    assert_eq!(
        created.body.get("state_hash").and_then(Value::as_str),
        Some(by_hand.as_str())
    );
}

#[test]
fn the_genesis_limits_block_has_not_moved() {
    let keys: Vec<String> = limits_value().as_obj().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        [
            "max_ctx_vars",
            "max_deadlines",
            "max_def_bytes",
            "max_emits_per_block",
            "max_enums",
            "max_eval_ticks",
            "max_events",
            "max_fields",
            "max_history",
            "max_invariants",
            "max_nesting",
            "max_regions",
            "max_sets_per_block",
            "max_states",
            "max_transitions",
            "max_transitions_per_cell",
            "max_variants",
        ]
    );
}

/// A state hash is a format-versioned value, and plan 0010's composition
/// fields moved every one of them from `fsm.state/2` to `fsm.state/3` — with
/// a per-record discriminator, so the committed journal below still folds and
/// still verifies under v2. That bump is deliberate and orthogonal to this
/// suite's claim, so the comparison holds the two format-versioned fields
/// aside and pins every other byte.
fn without_state_hashes(line: &[u8]) -> Vec<u8> {
    let Ok(mut value) = parse(line, &JsonLimits::DEFAULT) else {
        return line.to_vec();
    };
    fn scrub(value: &mut Value) {
        if let Value::Obj(fields) = value {
            for (name, inner) in fields.iter_mut() {
                // `hash` and `prev` chain the body, so they move with any
                // field in it; holding them aside is the same statement as
                // holding the state hash aside, one level out.
                if name.ends_with("state_hash")
                    || name == "state_format"
                    || name == "state_root"
                    || name == "hash"
                    || name == "prev"
                {
                    *inner = Value::Str("<format-versioned>".into());
                } else {
                    scrub(inner);
                }
            }
        }
        if let Value::Arr(items) = value {
            for item in items {
                scrub(item);
            }
        }
    }
    scrub(&mut value);
    fsm_core::canon::canon_bytes(&value)
}

/// Both journals, line by line, with those fields held aside.
fn comparable(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(without_state_hashes)
        .collect()
}
