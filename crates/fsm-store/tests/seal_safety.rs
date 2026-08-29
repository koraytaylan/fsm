//! The idempotency keys a seal carries, the keys it drops, and the three
//! independent reasons a dropped key can never be applied twice.
//!
//! Plan 0017 task 7903.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::Status;
use fsm_core::record::{Record, RecordKind, seal, zeros};
use fsm_core::replay::{RequestSlot, StoreState};
use fsm_store::base::DefinitionLimits;
use fsm_store::seal_safety::carry_at_cut;
use fsm_store::store::Store;

const CASE_REVIEW: &[u8] =
    include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A directory name no other run of this binary can produce.
///
/// A process id alone is not unique enough: a full `--workspace` run spawns
/// thousands of short-lived processes, ids get reused, and a reused id names a
/// directory a previous run may still be finishing with — which surfaces as a
/// `store/lock` naming *this* process. `crash_harness.rs` learned the same
/// thing and pins it with a test; this is that idiom.
fn invocation_tag() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    )
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(tag: &str) -> Self {
        let index = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-seal-safety-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
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

fn definition() -> Value {
    parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses")
}

/// A store with one running instance and one cancelled one, each holding keys.
fn store_with_a_live_and_a_settled_instance(directory: &TestDirectory) -> Store {
    let mut store = Store::open(directory.path()).expect("a fresh store opens");
    store
        .define_machine(definition(), false, false)
        .expect("the machine is definable");
    for instance in ["live", "settled"] {
        store
            .create_instance("case_review", instance, &format!("create-{instance}"), None)
            .expect("create succeeds");
        store
            .send_event(
                instance,
                "docs_ok",
                Value::Obj(BTreeMap::new()),
                &format!("send-{instance}"),
                None,
            )
            .expect("send succeeds");
    }
    store
        .cancel_instance("settled", "cancel-settled")
        .expect("cancel succeeds");
    store
}

fn decide(store: &Store) -> fsm_store::seal_safety::CarryDecision {
    carry_at_cut(&store.state, &store.records, DefinitionLimits::Current)
        .expect("a cut over a small store is admissible")
}

#[test]
fn a_cut_at_the_head_of_a_store_with_a_live_instance_succeeds() {
    // The case the first version of this rule got wrong, and the case every
    // real seal is: a cut sits at or near the head, so every key of every
    // running instance is below it. A rule that dropped those, or refused when
    // they existed, would refuse every seal a live store could ever ask for.
    let directory = TestDirectory::create("head");
    let store = store_with_a_live_and_a_settled_instance(&directory);
    let decision = decide(&store);
    assert!(
        decision.carried.contains_key("create-live"),
        "a live instance's creation key was dropped"
    );
    assert!(
        decision.carried.contains_key("send-live"),
        "a live instance's event key was dropped"
    );
    for (request_id, slot) in &decision.carried {
        assert!(
            slot.seq <= store.state.last_seq,
            "{request_id} was carried for the wrong reason: it is above the cut"
        );
    }
}

#[test]
fn a_settled_instances_keys_are_dropped_and_a_live_ones_are_carried() {
    let directory = TestDirectory::create("partition");
    let store = store_with_a_live_and_a_settled_instance(&directory);
    let decision = decide(&store);
    // The partition itself, not just the counts: a rule that carried the right
    // number of the wrong keys would pass a count assertion.
    for key in ["create-live", "send-live"] {
        assert!(decision.carried.contains_key(key), "{key} was not carried");
        assert!(!decision.dropped.contains(key), "{key} was also dropped");
    }
    for key in ["create-settled", "send-settled", "cancel-settled"] {
        assert!(decision.dropped.contains(key), "{key} was not dropped");
        assert!(
            !decision.carried.contains_key(key),
            "{key} was also carried"
        );
    }
    assert_eq!(decision.carried_count(), 2);
    assert_eq!(decision.dropped_count(), 3);
    assert_eq!(
        decision.carried_count() + decision.dropped_count(),
        store.state.dedup.len(),
        "the partition lost or invented a key"
    );
}

#[test]
fn a_cut_over_only_settled_instances_carries_nothing() {
    let directory = TestDirectory::create("settled");
    let mut store = Store::open(directory.path()).expect("a fresh store opens");
    store
        .define_machine(definition(), false, false)
        .expect("the machine is definable");
    store
        .create_instance("case_review", "done", "create-done", None)
        .expect("create succeeds");
    store
        .cancel_instance("done", "cancel-done")
        .expect("cancel succeeds");
    let decision = decide(&store);
    assert_eq!(decision.carried_count(), 0);
    assert_eq!(decision.dropped_count(), store.state.dedup.len());
}

#[test]
fn a_key_claimed_by_a_machine_definition_names_no_instance_and_is_droppable() {
    let directory = TestDirectory::create("machine");
    let mut store = Store::open(directory.path()).expect("a fresh store opens");
    store
        .define_machine(definition(), false, false)
        .expect("the machine is definable");
    let decision = decide(&store);
    // `machine_defined` carries no instance at all, so nothing keeps its key
    // alive. Re-issuing it is idempotent by content hash, which is the closure.
    assert_eq!(decision.carried_count(), 0);
}

// ---------------------------------------------------------------------------
// Hand-built records: the attributions a field probe gets wrong
// ---------------------------------------------------------------------------

fn hash(byte: u8) -> String {
    format!("sha256:{}", format!("{byte:02x}").repeat(32))
}

/// A record at `seq` of `kind` with `body`, chained onto a plausible parent.
fn record(seq: u64, kind: RecordKind, body: BTreeMap<String, Value>) -> Record {
    seal(seq, seq as i64, kind, Value::Obj(body), &zeros())
}

/// A state carrying `instances` with the given statuses and one dedup entry.
fn state_with(instances: &[(&str, Status)], dedup: &[(&str, u64)], last_seq: u64) -> StoreState {
    let mut state = StoreState {
        last_seq,
        ..StoreState::default()
    };
    for (id, status) in instances {
        let mut instance = fsm_core::machine::InstanceState {
            status: *status,
            configuration: fsm_core::machine::ActiveConfiguration::Sequential {
                leaf: "intake".into(),
            },
            ctx: BTreeMap::new(),
            history: BTreeMap::new(),
            deadlines: BTreeMap::new(),
            pending: Vec::new(),
            invocations: BTreeMap::new(),
            signals: BTreeMap::new(),
        };
        instance.status = *status;
        state.instances.insert((*id).into(), instance);
    }
    for (request_id, seq) in dedup {
        state.dedup.insert(
            (*request_id).into(),
            RequestSlot {
                seq: *seq,
                fp: None,
            },
        );
    }
    state
}

/// The partition alone, skipping the base-size check that needs real machines.
fn partition_of(state: &StoreState, records: &[Record]) -> (Vec<String>, Vec<String>) {
    let decision = carry_at_cut(state, records, DefinitionLimits::Current)
        .expect("a state with no machines fits any base");
    (
        decision.carried.keys().cloned().collect(),
        decision.dropped.iter().cloned().collect(),
    )
}

#[test]
fn an_invocation_key_is_attributed_to_the_child_instance() {
    // The exact defect a `body.get("instance_id")` probe produces, and it fails
    // silently: `instance_invoked` has no `instance_id` field at all, so a
    // probe would judge an invoked child's keys unattached and drop every one.
    let records = vec![record(
        5,
        RecordKind::InstanceInvoked,
        BTreeMap::from([
            ("parent_instance_id".into(), Value::Str("parent".into())),
            ("slot".into(), Value::Str("review".into())),
            ("child_instance_id".into(), Value::Str("child".into())),
            ("child_machine_id".into(), Value::Str("m".into())),
            ("request_id".into(), Value::Str("invoke-key".into())),
            ("state_hash".into(), Value::Str(hash(0x11))),
            ("child_state_hash".into(), Value::Str(hash(0x22))),
            ("overrides".into(), Value::Obj(BTreeMap::new())),
        ]),
    )];
    // Only the child is live; the parent has settled.
    let state = state_with(
        &[("parent", Status::Completed), ("child", Status::Running)],
        &[("invoke-key", 5)],
        9,
    );
    let (carried, dropped) = partition_of(&state, &records);
    assert_eq!(carried, vec!["invoke-key".to_string()]);
    assert!(dropped.is_empty());
}

#[test]
fn a_signal_key_is_carried_when_either_instance_it_names_is_live() {
    // `signal_delivered` is the only record naming two instances that are not
    // parent and child. Either one being live keeps the key.
    let body = |sender: &str, target: &str| {
        BTreeMap::from([
            ("sender_instance_id".into(), Value::Str(sender.into())),
            ("signal_id".into(), Value::Str("s".into())),
            ("target_instance_id".into(), Value::Str(target.into())),
            ("event".into(), Value::Str("resume".into())),
            ("request_id".into(), Value::Str("signal-key".into())),
            ("outcome".into(), Value::Str("delivered".into())),
            ("payload".into(), Value::Obj(BTreeMap::new())),
            ("sender_state_hash".into(), Value::Str(hash(0x33))),
        ])
    };
    let records = vec![record(
        7,
        RecordKind::SignalDelivered,
        body("sender", "target"),
    )];

    for (sender_status, target_status, expected_carried) in [
        (Status::Running, Status::Completed, true),
        (Status::Completed, Status::Running, true),
        (Status::Completed, Status::Cancelled, false),
    ] {
        let state = state_with(
            &[("sender", sender_status), ("target", target_status)],
            &[("signal-key", 7)],
            9,
        );
        let (carried, dropped) = partition_of(&state, &records);
        assert_eq!(
            carried.is_empty(),
            !expected_carried,
            "sender {sender_status:?} target {target_status:?} carried {carried:?}"
        );
        assert_eq!(dropped.is_empty(), expected_carried);
    }
}

#[test]
fn a_key_claimed_above_the_cut_is_carried_whatever_it_names() {
    let records = vec![record(
        12,
        RecordKind::Annotated,
        BTreeMap::from([
            ("instance_id".into(), Value::Str("gone".into())),
            ("request_id".into(), Value::Str("above".into())),
            ("note".into(), Value::Str("n".into())),
        ]),
    )];
    let state = state_with(&[("gone", Status::Completed)], &[("above", 12)], 9);
    let (carried, dropped) = partition_of(&state, &records);
    assert_eq!(carried, vec!["above".to_string()]);
    assert!(dropped.is_empty());
}

#[test]
fn a_key_whose_claiming_record_is_absent_is_carried_rather_than_dropped() {
    // Carrying too much risks only the size limit; dropping too much risks a
    // request applied twice. When the record set cannot answer, carry.
    let state = state_with(&[], &[("orphan", 3)], 9);
    let (carried, dropped) = partition_of(&state, &[]);
    assert_eq!(carried, vec!["orphan".to_string()]);
    assert!(dropped.is_empty());
}

// ---------------------------------------------------------------------------
// The three closures, against a real store
// ---------------------------------------------------------------------------

#[test]
fn an_event_to_a_settled_instance_is_refused_by_its_terminal_status() {
    // Closure one. The key was dropped; presenting a *fresh* key for the same
    // work is refused anyway, so replaying the dropped one changes nothing.
    let directory = TestDirectory::create("terminal");
    let mut store = store_with_a_live_and_a_settled_instance(&directory);
    let error = store
        .send_event(
            "settled",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "a-key-never-seen",
            None,
        )
        .expect_err("a settled instance refuses an event");
    assert!(
        error.code.starts_with("run/") || error.code.starts_with("req/"),
        "unexpected code {}",
        error.code
    );
}

#[test]
fn re_issuing_a_dropped_create_key_collides_with_the_instance_that_exists() {
    // Closure two. Every surface derives `inst-<request_id>`, so re-issuing a
    // creation request produces the same instance id — and `create` refuses an
    // id that exists rather than replacing it. Before this task the store had
    // no such check: a dropped key would have let the same request silently
    // reset the instance it had already made. The closure is a fact now, not a
    // hope about callers, which is what a proof obligation on the seal means.
    let directory = TestDirectory::create("create");
    let mut store = store_with_a_live_and_a_settled_instance(&directory);
    let before = store.state.instances.len();
    let error = store
        .create_instance("case_review", "settled", "a-different-key", None)
        .expect_err("creating an instance that exists is refused");
    assert_eq!(error.code, "req/instance_exists");
    assert!(
        error.hint.contains("original request_id"),
        "the hint must point at the replay that is the correct retry: {}",
        error.hint
    );
    assert_eq!(
        store.state.instances.len(),
        before,
        "a refused create still made an instance"
    );
    // The same refusal for a *running* instance: the guard is about identity,
    // not about status, so a create can never reset live work either.
    let error = store
        .create_instance("case_review", "live", "another-key", None)
        .expect_err("creating over a running instance is refused");
    assert_eq!(error.code, "req/instance_exists");
    assert_eq!(
        store.state.instances["live"].status,
        Status::Running,
        "a refused create disturbed the instance it named"
    );
}

#[test]
fn re_issuing_a_dropped_machine_key_is_idempotent_by_content_hash() {
    // Closure three. A definition is content-addressed, so re-adding it under
    // any key at all is the same machine and writes no second record.
    let directory = TestDirectory::create("machine-idempotent");
    let mut store = Store::open(directory.path()).expect("a fresh store opens");
    store
        .define_machine(definition(), false, false)
        .expect("the machine is definable");
    let records_before = store.records.len();
    let machines_before = store.state.machines.len();
    store
        .define_machine(definition(), false, false)
        .expect("re-adding the same definition succeeds");
    assert_eq!(
        store.state.machines.len(),
        machines_before,
        "the same definition became a second machine"
    );
    assert_eq!(
        store.records.len(),
        records_before,
        "the same definition wrote a second record"
    );
}

// ---------------------------------------------------------------------------
// The one refusal
// ---------------------------------------------------------------------------

#[test]
fn a_carried_set_too_large_for_a_base_file_is_refused_on_size() {
    // Constructed directly rather than by generating a store that large: the
    // property is the ceiling, not the time it takes to reach it.
    let mut state = state_with(&[("live", Status::Running)], &[], 9);
    // One live instance, and enough keys naming it that the encoded base must
    // exceed the persistence unit ceiling.
    let mut records = Vec::new();
    // Long keys rather than many: the property under test is the ceiling, and
    // reaching it with 4 KiB keys costs four thousand records instead of a
    // hundred thousand.
    for index in 0..4_200u64 {
        let request_id = format!("key-{index:07}-{}", "p".repeat(4_096));
        state.dedup.insert(
            request_id.clone(),
            RequestSlot {
                seq: index + 1,
                fp: Some(format!("sha256:{}", "f".repeat(64))),
            },
        );
        records.push(record(
            index + 1,
            RecordKind::Annotated,
            BTreeMap::from([
                ("instance_id".into(), Value::Str("live".into())),
                ("request_id".into(), Value::Str(request_id)),
                ("note".into(), Value::Str("n".into())),
            ]),
        ));
    }
    let error = carry_at_cut(&state, &records, DefinitionLimits::Current)
        .expect_err("an oversized carried set is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert!(
        error.hint.contains("earlier") && error.hint.contains("settle"),
        "the hint must name both remedies: {}",
        error.hint
    );
    assert!(
        error.message.contains("ceiling"),
        "the refusal must say it is a size limit: {}",
        error.message
    );
}

#[test]
fn the_decision_takes_no_lock_and_writes_nothing() {
    // A monitoring session must be able to ask this while a writer holds the
    // store, which is what makes `--dry-run` a read-only question.
    let directory = TestDirectory::create("readonly");
    let store = store_with_a_live_and_a_settled_instance(&directory);
    let before: Vec<_> = fs::read_dir(directory.path())
        .expect("the directory is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    // The writer is still open and holding the lock while this runs.
    let decision = decide(&store);
    assert!(decision.carried_count() + decision.dropped_count() > 0);
    let after: Vec<_> = fs::read_dir(directory.path())
        .expect("the directory is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(before, after, "the decision changed the data directory");
}
