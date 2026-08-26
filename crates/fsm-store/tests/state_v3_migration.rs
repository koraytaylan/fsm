//! The `fsm.state/3` migration: an instance written before this plan keeps
//! its records and its hashes forever.
//!
//! Composition puts `invocations` and `signals` in the state, so every
//! instance's hash moves — including instances that will never invoke
//! anything — and the only thing standing between that and an unreadable
//! store is the per-record discriminator the format already carries.
//!
//! Plan 0010 task 4904.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::hashes::{STATE_FORMAT, STATE_FORMAT_V2, digest_of, machine_id, state_hash_v2};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordKind, verify_line, zeros};
use fsm_core::replay::{NopSink, fold_with, state_root_at};
use fsm_store::clock::FixedClock;
use fsm_store::journal_io::STORE_VERSION;
use fsm_store::snapshot::SNAPSHOT_FORMAT;
use fsm_store::store::Store;

/// A journal the pre-composition build wrote: every state-bearing record
/// carries `fsm.state/2`.
const V2_JOURNAL: &[u8] = include_bytes!("fixtures/non_reactive_session.journal");

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-v3mig-{}-{n}", std::process::id()));
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

fn lay_out(directory: &TestDirectory, version: &str, bytes: &[u8]) {
    let journal = directory.path().join("journal");
    fs::create_dir_all(&journal).unwrap();
    fs::write(journal.join("seg-00000000000000000000.jsonl"), bytes).unwrap();
    fs::write(directory.path().join("VERSION"), format!("{version}\n")).unwrap();
}

fn version_of(directory: &TestDirectory) -> String {
    fs::read_to_string(directory.path().join("VERSION"))
        .unwrap()
        .trim()
        .to_string()
}

fn records(bytes: &[u8]) -> Vec<Record> {
    let mut previous = zeros();
    let mut out = Vec::new();
    for (seq, line) in bytes
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .enumerate()
    {
        let record = verify_line(line, seq as u64, &previous).expect("a committed record verifies");
        previous = record.hash.clone();
        out.push(record);
    }
    out
}

#[test]
fn a_version_eight_store_opens_folds_and_is_stamped_forward() {
    let directory = TestDirectory::create();
    lay_out(&directory, "8", V2_JOURNAL);
    let before = fs::read(
        directory
            .path()
            .join("journal/seg-00000000000000000000.jsonl"),
    )
    .unwrap();
    let store = Store::open(directory.path()).expect("a v8 store migrates");
    assert_eq!(version_of(&directory), STORE_VERSION);
    assert_eq!(
        fs::read(
            directory
                .path()
                .join("journal/seg-00000000000000000000.jsonl")
        )
        .unwrap(),
        before,
        "interior records are never rewritten"
    );
    assert!(!store.state.instances.is_empty());
    drop(store);
    // And re-opening a store already stamped forward changes nothing.
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(version_of(&directory), STORE_VERSION);
    drop(store);
}

#[test]
fn every_record_verifies_under_the_format_it_declares() {
    let folded = records(V2_JOURNAL);
    let state_bearing: Vec<&Record> = folded
        .iter()
        .filter(|record| record.body.get("state_hash").is_some())
        .collect();
    assert!(!state_bearing.is_empty());
    for record in &state_bearing {
        assert_eq!(
            record.body.get("state_format").and_then(Value::as_str),
            Some(STATE_FORMAT_V2),
            "the fixture is what the pre-composition build wrote"
        );
    }
    fold_with(folded.clone(), &mut NopSink).expect("v2 records verify under v2");

    // A v2 record whose hash was computed under v3 does not verify: the
    // discriminator is load-bearing, not decorative.
    let mut tampered = folded.clone();
    let victim = tampered
        .iter_mut()
        .find(|record| record.kind == RecordKind::InstanceCreated)
        .expect("the session creates an instance");
    if let Value::Obj(body) = &mut victim.body {
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.to_string()));
    }
    assert!(
        fold_with(tampered, &mut NopSink).is_err(),
        "a v2 hash does not verify as v3"
    );
}

#[test]
fn a_mixed_journal_verifies_each_record_under_its_own_format() {
    let directory = TestDirectory::create();
    lay_out(&directory, "8", V2_JOURNAL);
    let mut store = Store::open(directory.path()).unwrap();
    let v2_records = store.records.len();
    // Continue the same session under the current build: new records carry
    // v3, old ones keep v2, and the whole journal folds. The fixture's
    // instance has completed, so the continuation is a fresh one on the
    // machine the migrated journal already holds.
    let machine = store
        .state
        .machines
        .keys()
        .next()
        .cloned()
        .expect("the migrated journal holds its machine");
    store
        .create_instance_ctx_on(
            &mut FixedClock::new(50_000, 1),
            &machine,
            "after-migration",
            "after-migration",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let formats: Vec<Option<&str>> = store
        .records
        .iter()
        .filter(|record| record.body.get("state_hash").is_some())
        .map(|record| record.body.get("state_format").and_then(Value::as_str))
        .collect();
    assert!(formats.contains(&Some(STATE_FORMAT_V2)) && formats.contains(&Some(STATE_FORMAT)));
    let all = store.records.clone();
    assert!(all.len() > v2_records);
    drop(store);
    fold_with(all, &mut NopSink).expect("a mixed journal folds");
    Store::open(directory.path()).expect("and reopens");
}

#[test]
fn a_store_with_neither_invocations_nor_signals_folds_to_two_empty_maps() {
    let directory = TestDirectory::create();
    lay_out(&directory, "8", V2_JOURNAL);
    let store = Store::open(directory.path()).unwrap();
    for (id, instance) in &store.state.instances {
        assert!(
            instance.invocations.is_empty() && instance.signals.is_empty(),
            "{id} invokes and signals nothing"
        );
    }
    // Its v3 hash differs from the v2 hash of the same logical state: the
    // reason this migration exists at all.
    let (id, instance) = store.state.instances.iter().next().expect("one instance");
    let mid = store.state.instance_machines[id].clone();
    let seq = store.state.last_seq;
    assert_ne!(
        state_hash_v2(&mid, id, seq, instance),
        fsm_core::hashes::state_hash(&mid, id, seq, instance)
    );
}

#[test]
fn a_failed_fold_refuses_and_leaves_the_version_alone() {
    let directory = TestDirectory::create();
    let mut corrupted = records(V2_JOURNAL);
    let victim = corrupted
        .iter_mut()
        .find(|record| record.kind == RecordKind::InstanceCreated)
        .expect("the session creates an instance");
    if let Value::Obj(body) = &mut victim.body {
        body.insert(
            "state_hash".into(),
            Value::Str(format!("sha256:{}", "f".repeat(64))),
        );
    }
    let bytes: Vec<u8> = corrupted.iter().flat_map(Record::to_line).collect();
    lay_out(&directory, "8", &bytes);
    assert!(
        Store::open(directory.path()).is_err(),
        "a journal that does not fold is refused"
    );
    assert_eq!(version_of(&directory), "8", "and its VERSION is untouched");
}

#[test]
fn an_older_snapshot_is_ignored_and_the_state_re_derived() {
    let directory = TestDirectory::create();
    lay_out(&directory, "8", V2_JOURNAL);
    let store = Store::open(directory.path()).unwrap();
    let expected_root = state_root_at(&store.state, store.state.last_seq);
    let instances = store.state.instances.clone();
    drop(store);

    // A snapshot from the previous format, laid beside a current journal.
    let snapshots = directory.path().join("snapshots");
    fs::create_dir_all(&snapshots).unwrap();
    let stale = format!(
        r#"{{"format":"fsm.snapshot/4","body":{{}},"snapshot_hash":"sha256:{}"}}"#,
        "0".repeat(64)
    );
    fs::write(snapshots.join("snap-00000000000000000004.json"), stale).unwrap();

    let reopened = Store::open(directory.path()).expect("a stale snapshot is skipped");
    assert_eq!(
        reopened.state.instances, instances,
        "re-derived from the journal"
    );
    assert_eq!(
        state_root_at(&reopened.state, reopened.state.last_seq),
        expected_root
    );
    assert_eq!(SNAPSHOT_FORMAT, "fsm.snapshot/5");
}

#[test]
fn an_unknown_version_is_still_refused_and_never_reinterpreted() {
    let directory = TestDirectory::create();
    lay_out(&directory, "99", V2_JOURNAL);
    let Err(error) = Store::open(directory.path()) else {
        panic!("an unknown version is refused");
    };
    assert_eq!(error.code, "store/version_mismatch");
    assert_eq!(version_of(&directory), "99");
}

#[test]
fn a_snapshot_carries_a_composed_stores_children_and_slots() {
    // The snapshot's known-instance set is derived from the records, and a
    // child exists because an `instance_invoked` record says so.
    let child = r#"{"format":"fsm.machine/1","name":"leaf","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;
    let child_value = parse(child.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let digest = digest_of(&machine_id(&child_value)).unwrap().to_string();
    let parent = format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{digest}"}}]}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}}]}}"#
    );
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, child_value, false, false)
        .unwrap();
    store
        .define_machine_on(
            &mut clock,
            parse(parent.as_bytes(), &JsonLimits::DEFAULT).unwrap(),
            false,
            false,
        )
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "parent",
            "p1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("p1", "open", Value::Obj(BTreeMap::new()), "open-1", None)
        .unwrap();
    store.invoke_child("p1", "down", "inv-1").unwrap();
    let expected = store.state.instances.clone();
    store.shutdown_snapshot().unwrap();
    drop(store);

    let reopened = Store::open(directory.path()).expect("reopen from the snapshot");
    assert_eq!(
        reopened.state.instances, expected,
        "the child and the parent's slot both survived the snapshot"
    );
    let child_id = fsm_core::hashes::child_instance_id("p1", "down");
    assert!(reopened.state.instances.contains_key(&child_id));
    assert_eq!(
        reopened.state.instances["p1"].invocations["down"].status,
        fsm_core::machine::InvokeStatus::Running
    );
}
