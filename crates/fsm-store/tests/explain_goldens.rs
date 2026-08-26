//! `explain` and `instance_history` on a non-reactive machine are byte for
//! byte what the pre-change build produced, and on a reactive record they
//! render every reaction microstep as its own section under the trigger's.
//!
//! Plan 0009 task 4701. Regenerate the golden only from a build that
//! predates the change under test:
//! `FSM_REGEN_FIXTURES=1 cargo test -p fsm-store --test explain_goldens`.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/non_reactive_explain.json"
);

const NON_REACTIVE: &str = r#"{"format":"fsm.machine/1","name":"plain","states":[{"name":"a"},{"name":"b"},{"name":"c","terminal":true}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[{"name":"by","ty":"int"}]},{"name":"stop","fields":[]}],"effects":[{"name":"fx","fields":[]}],"deadlines":[{"name":"expire","from":"b","after":"dur(2, s)","to":"c"}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"ctx.n + evt.by"}],"emit":[{"effect":"fx"}]},{"from":"b","on":"stop","to":"c"}]}"#;

/// Creation cascades, `go` cascades through a raise and an eventless
/// transition, and the deadline cascades.
const REACTIVE: &str = r#"{"format":"fsm.machine/1","name":"reactive","states":[{"name":"boot"},{"name":"idle"},{"name":"working","entry":{"raise":[{"event":"tick"}]}},{"name":"ticked"},{"name":"waiting"},{"name":"expired"},{"name":"done","terminal":true}],"initial":"boot","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true},{"name":"settle","fields":[]}],"deadlines":[{"name":"expire","from":"waiting","after":"dur(1, s)","to":"expired"}],"transitions":[{"from":"boot","to":"idle"},{"from":"idle","on":"go","to":"working"},{"from":"working","on":"tick","to":"ticked","do":[{"target":"n","value":"ctx.n + 1"}]},{"from":"ticked","to":"waiting"},{"from":"expired","to":"done"}]}"#;

/// Define, create, send `go` with `payload`, then poll at each of `polls`:
/// every record kind explain can rebuild, on a reactive and a plain machine.
fn drive(src: &str, payload: Value, polls: &[i64]) -> Store {
    let spec = parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let name = spec
        .get("name")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let mut store = Store::open_memory().unwrap();
    let mut clock = FixedClock::new(1_000, 1_000);
    store
        .define_machine_on(&mut clock, spec, false, false)
        .unwrap();
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
    let mut clock = FixedClock::new(10_000, 1_000);
    let mut payload = payload;
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
    for (i, at) in polls.iter().enumerate() {
        let mut clock = FixedClock::new(*at, 1_000);
        store
            .poll_instance_deadline_on(&mut clock, "inst-1", &format!("poll-{}", i + 1), None)
            .unwrap();
    }
    store
}

fn instance_seqs(store: &Store) -> Vec<u64> {
    store
        .records
        .iter()
        .filter(|r| r.body.get("instance_id").and_then(Value::as_str) == Some("inst-1"))
        .map(|r| r.seq)
        .collect()
}

fn seq_of(store: &Store, kind: RecordKind) -> u64 {
    store.records.iter().find(|r| r.kind == kind).unwrap().seq
}

/// Every explain entry of the instance, and its history page with traces.
fn explained(store: &Store) -> Value {
    let explain: Vec<Value> = instance_seqs(store)
        .into_iter()
        .map(|seq| store.explain_seq("inst-1", seq).unwrap())
        .collect();
    Value::Obj(BTreeMap::from([
        ("explain".to_string(), Value::Arr(explain)),
        (
            "history".to_string(),
            store.history_page("inst-1", 0, 500, true, true).unwrap(),
        ),
    ]))
}

fn microsteps_of(entry: &Value) -> Option<&[Value]> {
    entry
        .get("trace")
        .and_then(|trace| trace.get("microsteps"))
        .and_then(Value::as_arr)
}

#[test]
fn non_reactive_explain_and_history_are_byte_identical_to_the_pre_change_build() {
    let store = drive(
        NON_REACTIVE,
        Value::Obj(BTreeMap::from([("by".to_string(), Value::Str("3".into()))])),
        &[11_000, 20_000],
    );
    let value = explained(&store);
    let bytes = canon_bytes(&value);
    if std::env::var_os("FSM_REGEN_FIXTURES").is_some() {
        std::fs::write(FIXTURE, &bytes).unwrap();
    }
    let committed = std::fs::read(FIXTURE).expect("the pre-change explain golden is committed");
    assert!(
        bytes == committed,
        "a non-reactive machine's explain or history output moved"
    );
    let text = String::from_utf8(bytes).unwrap();
    assert!(
        !text.contains("\"microsteps\"") && !text.contains("\"internal_unhandled\""),
        "no reaction key appears anywhere in a non-reactive explain"
    );
}

#[test]
fn a_reactive_explain_renders_every_microstep_as_its_own_section() {
    let store = drive(REACTIVE, Value::Obj(BTreeMap::new()), &[20_000]);
    let applied = store
        .explain_seq("inst-1", seq_of(&store, RecordKind::EventApplied))
        .unwrap();
    let claim = applied.get("microsteps").and_then(Value::as_arr).unwrap();
    assert_eq!(claim.len(), 2, "the record's own claim rides along");
    let sections = microsteps_of(&applied).unwrap();
    let triggers: Vec<(&str, Option<&str>)> = sections
        .iter()
        .map(|s| {
            (
                s.get("trigger").and_then(Value::as_str).unwrap(),
                s.get("event").and_then(Value::as_str),
            )
        })
        .collect();
    assert_eq!(triggers, [("internal", Some("tick")), ("eventless", None)]);
    for section in sections {
        assert!(section.get("candidates").and_then(Value::as_arr).is_some());
        assert!(section.get("pipeline").and_then(Value::as_arr).is_some());
        assert!(section.get("index").is_some() && section.get("source_state").is_some());
    }
    // The trigger's own sections are untouched by the reactions beneath them.
    let trace = applied.get("trace").unwrap();
    assert_eq!(
        trace
            .get("candidates")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        Some(1)
    );
    let deadline = store
        .explain_seq("inst-1", seq_of(&store, RecordKind::DeadlineApplied))
        .unwrap();
    assert_eq!(microsteps_of(&deadline).map(<[Value]>::len), Some(1));
    let created = store
        .explain_seq("inst-1", seq_of(&store, RecordKind::InstanceCreated))
        .unwrap();
    assert_eq!(
        created
            .get("microsteps")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        Some(1),
        "creation's cascade is claimed on its record"
    );
}

#[test]
fn history_with_traces_matches_explain_entry_for_entry() {
    for (src, payload, polls) in [
        (
            NON_REACTIVE,
            Value::Obj(BTreeMap::from([("by".to_string(), Value::Str("3".into()))])),
            &[11_000, 20_000][..],
        ),
        (REACTIVE, Value::Obj(BTreeMap::new()), &[20_000][..]),
    ] {
        let store = drive(src, payload, polls);
        let page = store.history_page("inst-1", 0, 500, true, true).unwrap();
        let entries = page.get("entries").and_then(Value::as_arr).unwrap();
        let seqs = instance_seqs(&store);
        assert_eq!(entries.len(), seqs.len());
        for (entry, seq) in entries.iter().zip(seqs) {
            let mut explain = store.explain_seq("inst-1", seq).unwrap();
            if let Value::Obj(o) = &mut explain {
                o.remove("chain_verified");
            }
            assert_eq!(entry, &explain, "seq {seq}");
        }
    }
}
