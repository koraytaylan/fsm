//! Production-entry regressions for the canonical journal-payload ceiling.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MAX_PAYLOAD_BYTES;
use fsm_core::machine::InstanceState;
use fsm_store::clock::{FixedClock, pin};
use fsm_store::store::{ErrorObj, Store};

const EVENT_MACHINE: &[u8] = br#"{
    "format":"fsm.machine/1",
    "name":"payload_bounds",
    "states":[{"name":"waiting"}],
    "initial":"waiting",
    "context":[{"name":"count","ty":"int","init":"0"}],
    "events":[{"name":"go","fields":[{"name":"text","ty":"str"}]}],
    "transitions":[{
        "from":"waiting",
        "on":"go",
        "do":[{"target":"count","value":"ctx.count + 1"}]
    }]
}"#;

const STAMPED_EVENT_MACHINE: &[u8] = br#"{
    "format":"fsm.machine/1",
    "name":"payload_stamp_bounds",
    "states":[{"name":"waiting"}],
    "initial":"waiting",
    "context":[{"name":"count","ty":"int","init":"0"}],
    "events":[{"name":"go","fields":[
        {"name":"at","ty":"timestamp"},
        {"name":"text","ty":"str"}
    ]}],
    "transitions":[{
        "from":"waiting",
        "on":"go",
        "do":[{"target":"count","value":"ctx.count + 1"}]
    }]
}"#;

const CASE_REVIEW: &[u8] =
    include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-store-payload-bounds-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn store_with_instances(
    directory: &Path,
    definition: &[u8],
    machine_name: &str,
    instance_ids: &[&str],
) -> Store {
    let mut store = Store::open(directory).unwrap();
    let definition = parse(definition, &JsonLimits::DEFAULT).unwrap();
    let mut clock = FixedClock::new(1, 1);
    store
        .define_machine_on(&mut clock, definition, false, false)
        .unwrap();
    for (index, instance_id) in instance_ids.iter().enumerate() {
        store
            .create_instance_ctx_on(
                &mut clock,
                machine_name,
                instance_id,
                &format!("create-{index}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    store
}

fn string_value_with_canonical_size(size: usize) -> Value {
    let value = Value::Str("x".repeat(size.checked_sub(2).unwrap()));
    assert_eq!(canon_bytes(&value).len(), size);
    value
}

fn event_payload_with_canonical_size(size: usize) -> Value {
    let empty = Value::Obj(BTreeMap::from([("text".into(), Value::Str(String::new()))]));
    let overhead = canon_bytes(&empty).len();
    let value = Value::Obj(BTreeMap::from([(
        "text".into(),
        Value::Str("x".repeat(size.checked_sub(overhead).unwrap())),
    )]));
    assert_eq!(canon_bytes(&value).len(), size);
    value
}

fn event_payload_with_final_stamp_size(size: usize, timestamp: i64) -> Value {
    let stamped_empty = Value::Obj(BTreeMap::from([
        ("at".into(), Value::Str(timestamp.to_string())),
        ("text".into(), Value::Str(String::new())),
    ]));
    let stamped_overhead = canon_bytes(&stamped_empty).len();
    let value = Value::Obj(BTreeMap::from([(
        "text".into(),
        Value::Str("x".repeat(size.checked_sub(stamped_overhead).unwrap())),
    )]));
    let mut stamped = value.clone();
    let Value::Obj(stamped_fields) = &mut stamped else {
        unreachable!("helper constructs an object")
    };
    stamped_fields.insert("at".into(), Value::Str(timestamp.to_string()));
    assert_eq!(canon_bytes(&stamped).len(), size);
    value
}

fn assert_payload_error(error: &ErrorObj, field: &str, bytes: usize, request_id: &str) {
    assert_eq!(error.code, "req/payload_too_large");
    assert_eq!(
        error.details.get("field").and_then(Value::as_str),
        Some(field)
    );
    assert_eq!(
        error.details.get("bytes").and_then(Value::as_num),
        Some(bytes.to_string().as_str())
    );
    assert_eq!(
        error.details.get("max_bytes").and_then(Value::as_num),
        Some(MAX_PAYLOAD_BYTES.to_string().as_str())
    );
    assert_eq!(
        error.details.get("request_id").and_then(Value::as_str),
        Some(request_id)
    );
}

fn assert_unchanged(
    store: &Store,
    instance_id: &str,
    before: &InstanceState,
    sequence: u64,
    hash: &str,
    request_id: &str,
) {
    assert_eq!(store.state.instances.get(instance_id), Some(before));
    assert_eq!(store.state.last_seq, sequence);
    assert_eq!(store.state.last_hash, hash);
    assert!(!store.state.dedup.contains_key(request_id));
}

fn assert_journalled_field_size(store: &Store, request_id: &str, field: &str, size: usize) {
    let record = store
        .records
        .iter()
        .find(|record| record.body.get("request_id").and_then(Value::as_str) == Some(request_id))
        .unwrap_or_else(|| panic!("missing journal record for {request_id}"));
    let value = record
        .body
        .get(field)
        .unwrap_or_else(|| panic!("record for {request_id} is missing {field}"));
    assert_eq!(canon_bytes(value).len(), size);
}

#[test]
fn event_payload_accepts_exact_cap_and_rejects_cap_plus_one_without_claiming_request() {
    let directory = TestDirectory::create();
    let mut store = store_with_instances(
        directory.path(),
        EVENT_MACHINE,
        "payload_bounds",
        &["exact", "oversized"],
    );
    let mut clock = FixedClock::new(10, 1);

    let mut exact = event_payload_with_canonical_size(MAX_PAYLOAD_BYTES);
    assert_eq!(canon_bytes(&exact).len(), MAX_PAYLOAD_BYTES);
    store
        .send_event_stamp_on(
            &mut clock,
            "exact",
            "go",
            &mut exact,
            "event-exact",
            None,
            &[],
        )
        .unwrap();
    assert_eq!(
        store.state.instances["exact"].ctx.get("count"),
        Some(&Val::Int(1))
    );

    let before = store.state.instances["oversized"].clone();
    let sequence = store.state.last_seq;
    let hash = store.state.last_hash.clone();
    let mut oversized = event_payload_with_canonical_size(MAX_PAYLOAD_BYTES + 1);
    assert_eq!(canon_bytes(&oversized).len(), MAX_PAYLOAD_BYTES + 1);
    let error = store
        .send_event_stamp_on(
            &mut clock,
            "oversized",
            "go",
            &mut oversized,
            "event-oversized",
            None,
            &[],
        )
        .unwrap_err();
    assert_payload_error(&error, "payload", MAX_PAYLOAD_BYTES + 1, "event-oversized");
    assert_unchanged(
        &store,
        "oversized",
        &before,
        sequence,
        &hash,
        "event-oversized",
    );

    let mut retry = event_payload_with_canonical_size(11);
    store
        .send_event_stamp_on(
            &mut clock,
            "oversized",
            "go",
            &mut retry,
            "event-oversized",
            None,
            &[],
        )
        .unwrap();
    assert_eq!(
        store.state.instances["oversized"].ctx.get("count"),
        Some(&Val::Int(1))
    );
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(
        reopened.state.instances["exact"].ctx.get("count"),
        Some(&Val::Int(1))
    );
    assert_eq!(
        reopened.state.instances["oversized"].ctx.get("count"),
        Some(&Val::Int(1))
    );
    assert_journalled_field_size(&reopened, "event-exact", "payload", MAX_PAYLOAD_BYTES);
    assert!(reopened.state.dedup.contains_key("event-exact"));
    assert!(reopened.state.dedup.contains_key("event-oversized"));
}

#[test]
fn stamped_event_payload_checks_the_final_boundary_atomically() {
    let directory = TestDirectory::create();
    let mut store = store_with_instances(
        directory.path(),
        STAMPED_EVENT_MACHINE,
        "payload_stamp_bounds",
        &["exact", "oversized"],
    );
    let mut clock = FixedClock::new(42_000, 1);

    let mut exact = event_payload_with_final_stamp_size(MAX_PAYLOAD_BYTES, 42_000);
    assert!(canon_bytes(&exact).len() < MAX_PAYLOAD_BYTES);
    store
        .send_event_stamp_on(
            &mut clock,
            "exact",
            "go",
            &mut exact,
            "stamp-exact",
            None,
            &["at", "at"],
        )
        .unwrap();
    assert_eq!(canon_bytes(&exact).len(), MAX_PAYLOAD_BYTES);
    assert_eq!(exact.get("at").and_then(Value::as_str), Some("42000"));
    assert_eq!(clock.now, 42_001, "one append consumes one clock tick");

    let before = store.state.instances["oversized"].clone();
    let sequence = store.state.last_seq;
    let hash = store.state.last_hash.clone();
    let mut oversized = event_payload_with_final_stamp_size(MAX_PAYLOAD_BYTES + 1, 42_001);
    assert!(canon_bytes(&oversized).len() < MAX_PAYLOAD_BYTES);
    let original_payload = oversized.clone();
    let error = store
        .send_event_stamp_on(
            &mut clock,
            "oversized",
            "go",
            &mut oversized,
            "stamp-oversized",
            None,
            &["at"],
        )
        .unwrap_err();
    assert_payload_error(&error, "payload", MAX_PAYLOAD_BYTES + 1, "stamp-oversized");
    assert_eq!(oversized, original_payload);
    assert_eq!(
        clock.now, 42_001,
        "an unjournaled size rejection must not consume the reserved timestamp"
    );
    assert_unchanged(
        &store,
        "oversized",
        &before,
        sequence,
        &hash,
        "stamp-oversized",
    );

    let mut retry = Value::Obj(BTreeMap::from([(
        "text".into(),
        Value::Str("retry".into()),
    )]));
    store
        .send_event_stamp_on(
            &mut clock,
            "oversized",
            "go",
            &mut retry,
            "stamp-oversized",
            None,
            &["at"],
        )
        .unwrap();
    assert_eq!(retry.get("at").and_then(Value::as_str), Some("42001"));
    assert_eq!(clock.now, 42_002);
    assert_eq!(
        store.state.instances["oversized"].ctx.get("count"),
        Some(&Val::Int(1))
    );
    let final_sequence = store.state.last_seq;
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(reopened.state.last_seq, final_sequence);
    assert_eq!(
        reopened.state.instances["exact"].ctx.get("count"),
        Some(&Val::Int(1))
    );
    assert_eq!(
        reopened.state.instances["oversized"].ctx.get("count"),
        Some(&Val::Int(1))
    );
    assert_journalled_field_size(&reopened, "stamp-exact", "payload", MAX_PAYLOAD_BYTES);
    assert!(reopened.state.dedup.contains_key("stamp-exact"));
    assert!(reopened.state.dedup.contains_key("stamp-oversized"));
}

#[test]
fn ack_result_accepts_exact_cap_and_rejects_cap_plus_one_without_claiming_request() {
    let directory = TestDirectory::create();
    let mut store = store_with_instances(
        directory.path(),
        CASE_REVIEW,
        "case_review",
        &["exact", "oversized"],
    );
    let mut clock = FixedClock::new(10, 1);
    for (instance_id, request_id) in [("exact", "send-exact"), ("oversized", "send-over")] {
        let mut payload = Value::Obj(BTreeMap::new());
        store
            .send_event_stamp_on(
                &mut clock,
                instance_id,
                "docs_ok",
                &mut payload,
                request_id,
                None,
                &[],
            )
            .unwrap();
    }
    let exact_effect = store.state.instances["exact"].pending[0].clone();
    let oversized_effect = store.state.instances["oversized"].pending[0].clone();

    let exact = string_value_with_canonical_size(MAX_PAYLOAD_BYTES);
    assert_eq!(canon_bytes(&exact).len(), MAX_PAYLOAD_BYTES);
    store
        .ack_effect_outcome_on(
            &mut clock,
            "exact",
            &exact_effect,
            "ack-exact",
            "ok",
            Some(exact),
        )
        .unwrap();
    assert!(store.state.instances["exact"].pending.is_empty());

    let before = store.state.instances["oversized"].clone();
    let sequence = store.state.last_seq;
    let hash = store.state.last_hash.clone();
    let oversized = string_value_with_canonical_size(MAX_PAYLOAD_BYTES + 1);
    assert_eq!(canon_bytes(&oversized).len(), MAX_PAYLOAD_BYTES + 1);
    let error = store
        .ack_effect_outcome_on(
            &mut clock,
            "oversized",
            &oversized_effect,
            "ack-oversized",
            "ok",
            Some(oversized),
        )
        .unwrap_err();
    assert_payload_error(&error, "result", MAX_PAYLOAD_BYTES + 1, "ack-oversized");
    assert_unchanged(
        &store,
        "oversized",
        &before,
        sequence,
        &hash,
        "ack-oversized",
    );

    store
        .ack_effect_outcome_on(
            &mut clock,
            "oversized",
            &oversized_effect,
            "ack-oversized",
            "ok",
            Some(Value::Str("ok".into())),
        )
        .unwrap();
    assert!(store.state.instances["oversized"].pending.is_empty());
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert!(reopened.state.instances["exact"].pending.is_empty());
    assert!(reopened.state.instances["oversized"].pending.is_empty());
    assert_journalled_field_size(&reopened, "ack-exact", "result", MAX_PAYLOAD_BYTES);
    assert!(reopened.state.dedup.contains_key("ack-exact"));
    assert!(reopened.state.dedup.contains_key("ack-oversized"));
}

#[test]
fn annotation_accepts_exact_cap_and_rejects_cap_plus_one_without_claiming_request() {
    let directory = TestDirectory::create();
    let mut store = store_with_instances(directory.path(), CASE_REVIEW, "case_review", &["case"]);

    let exact = string_value_with_canonical_size(MAX_PAYLOAD_BYTES);
    assert_eq!(canon_bytes(&exact).len(), MAX_PAYLOAD_BYTES);
    let Value::Str(exact) = exact else {
        unreachable!("helper constructs a string")
    };
    let sequence = store.state.last_seq;
    {
        let _clock = pin(20);
        store.annotate("case", "annotation-exact", &exact).unwrap();
    }
    assert_eq!(store.state.last_seq, sequence + 1);

    let before = store.state.instances["case"].clone();
    let sequence = store.state.last_seq;
    let hash = store.state.last_hash.clone();
    let oversized = string_value_with_canonical_size(MAX_PAYLOAD_BYTES + 1);
    assert_eq!(canon_bytes(&oversized).len(), MAX_PAYLOAD_BYTES + 1);
    let Value::Str(oversized) = oversized else {
        unreachable!("helper constructs a string")
    };
    let error = {
        let _clock = pin(21);
        store
            .annotate("case", "annotation-oversized", &oversized)
            .unwrap_err()
    };
    assert_payload_error(
        &error,
        "note",
        MAX_PAYLOAD_BYTES + 1,
        "annotation-oversized",
    );
    assert_unchanged(
        &store,
        "case",
        &before,
        sequence,
        &hash,
        "annotation-oversized",
    );

    {
        let _clock = pin(22);
        store
            .annotate("case", "annotation-oversized", "ok")
            .unwrap();
    }
    let final_sequence = store.state.last_seq;
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(reopened.state.last_seq, final_sequence);
    assert_journalled_field_size(&reopened, "annotation-exact", "note", MAX_PAYLOAD_BYTES);
    assert!(reopened.state.dedup.contains_key("annotation-exact"));
    assert!(reopened.state.dedup.contains_key("annotation-oversized"));
}
