//! The cancel cascade and the orphans it can leave behind.
//!
//! This is the one place the composition plan writes two records for one
//! request, so the window between them is tested rather than denied.
//!
//! Plan 0010 task 4903.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::Status;
use fsm_core::record::{Record, RecordKind};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-cascade-{}-{n}", std::process::id()));
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

fn value(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(src: &str) -> String {
    digest_of(&machine_id(&value(src))).unwrap().to_string()
}

/// A leaf machine that just runs.
const LEAF: &str = r#"{"format":"fsm.machine/1","name":"leaf","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;

/// A machine that invokes `child_digest` from `busy` and can leave it.
fn waiter(name: &str, child_digest: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"idle"}},{{"name":"busy","invoke":[{{"id":"down","machine":"{child_digest}"}}]}},{{"name":"elsewhere"}}],"initial":"idle","context":[],"events":[{{"name":"open","fields":[]}},{{"name":"leave","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"busy"}},{{"from":"busy","on":"leave","to":"elsewhere"}}]}}"#
    )
}

fn records_of(store: &Store, kind: RecordKind) -> Vec<&Record> {
    store.records.iter().filter(|r| r.kind == kind).collect()
}

/// A store with `depth` levels of waiters over a leaf, each invoked, so the
/// root has a running chain beneath it. Returns the instance ids, root first.
fn chain(directory: &TestDirectory, depth: usize) -> (Store, Vec<String>) {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(LEAF), false, false)
        .unwrap();
    // Built bottom-up so each waiter invokes the one below it: waiter0 over
    // the leaf, waiter1 over waiter0, and the last one is the root.
    let mut below = digest(LEAF);
    let mut names = Vec::new();
    for level in 0..depth {
        let name = format!("waiter{level}");
        let src = waiter(&name, &below);
        store
            .define_machine_on(&mut clock, value(&src), false, false)
            .unwrap();
        below = digest(&src);
        names.push(name);
    }
    let root = names.last().expect("at least one waiter").clone();
    store
        .create_instance_ctx_on(
            &mut clock,
            &root,
            "root",
            "c-root",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let mut ids = vec!["root".to_string()];
    let mut current = "root".to_string();
    for level in 0..depth {
        store
            .send_event(
                &current,
                "open",
                Value::Obj(BTreeMap::new()),
                &format!("open-{level}"),
                None,
            )
            .unwrap();
        store
            .invoke_child(&current, "down", &format!("inv-{level}"))
            .unwrap();
        current = child_instance_id(&current, "down");
        ids.push(current.clone());
    }
    (store, ids)
}

#[test]
fn leaving_an_invoking_state_cancels_the_running_child() {
    let directory = TestDirectory::create();
    let (mut store, ids) = chain(&directory, 1);
    let child = ids[1].clone();
    let before = store.records.len();
    store
        .send_event(
            "root",
            "leave",
            Value::Obj(BTreeMap::new()),
            "leave-1",
            None,
        )
        .unwrap();
    assert_eq!(store.records.len(), before + 2, "the event and the cancel");
    assert_eq!(store.state.instances[&child].status, Status::Cancelled);
    let cancelled = records_of(&store, RecordKind::InstanceCancelled);
    assert_eq!(cancelled.len(), 1);
    assert_eq!(
        cancelled[0].body.get("reason").and_then(Value::as_str),
        Some("parent-exit:root/down")
    );
    assert!(
        !store.state.instances["root"]
            .invocations
            .contains_key("down"),
        "the slot went with the state"
    );
}

#[test]
fn cancelling_a_parent_cancels_the_whole_chain_depth_first() {
    let directory = TestDirectory::create();
    let (mut store, ids) = chain(&directory, 3);
    assert_eq!(ids.len(), 4, "root plus three descendants");
    store.cancel_instance("root", "cancel-1").unwrap();
    for id in &ids {
        assert_eq!(
            store.state.instances[id].status,
            Status::Cancelled,
            "{id} is cancelled"
        );
    }
    assert_eq!(
        records_of(&store, RecordKind::InstanceCancelled).len(),
        4,
        "one record each, the root's own included"
    );
}

#[test]
fn a_settled_child_is_not_re_cancelled() {
    let directory = TestDirectory::create();
    let (mut store, ids) = chain(&directory, 1);
    let child = ids[1].clone();
    store
        .send_event(&child, "finish", Value::Obj(BTreeMap::new()), "fin-1", None)
        .unwrap();
    assert_eq!(store.state.instances[&child].status, Status::Completed);
    let before = records_of(&store, RecordKind::InstanceCancelled).len();
    store.cancel_instance("root", "cancel-1").unwrap();
    assert_eq!(
        records_of(&store, RecordKind::InstanceCancelled).len(),
        before + 1,
        "only the root's own cancellation"
    );
    assert_eq!(store.state.instances[&child].status, Status::Completed);
}

#[test]
fn the_crash_window_leaves_an_orphan_that_doctor_reports_and_repair_settles() {
    let directory = TestDirectory::create();
    let (mut store, ids) = chain(&directory, 1);
    let child = ids[1].clone();
    store
        .send_event(
            "root",
            "leave",
            Value::Obj(BTreeMap::new()),
            "leave-1",
            None,
        )
        .unwrap();
    // Drop the child's cancellation, as a crash between the two records
    // would: the parent's event survived, the cascade did not.
    let mut records = store.records.clone();
    let cancel = records
        .iter()
        .rposition(|record| record.kind == RecordKind::InstanceCancelled)
        .expect("the cascade wrote one");
    assert_eq!(cancel, records.len() - 1, "it is the last record");
    records.truncate(cancel);
    drop(store);
    let bytes: Vec<u8> = records.iter().flat_map(Record::to_line).collect();
    let segment = directory
        .path()
        .join("journal")
        .join("seg-00000000000000000000.jsonl");
    fs::write(&segment, bytes).unwrap();

    let before = fs::read(&segment).unwrap().len();
    let mut reopened = Store::open(directory.path()).unwrap();
    assert_eq!(
        fs::read(&segment).unwrap().len(),
        before,
        "an open writes nothing, orphans or not"
    );
    assert_eq!(
        reopened.state.instances[&child].status,
        Status::Running,
        "running, and unreferenced: nothing is corrupt"
    );
    let orphans = reopened.orphaned_children();
    assert_eq!(orphans.len(), 1);
    assert_eq!(
        orphans[0].get("instance_id").and_then(Value::as_str),
        Some(child.as_str())
    );

    let records = reopened.records.len();
    let cancelled = reopened
        .cancel_orphans_on(&mut FixedClock::new(9_000, 1), "repair-1")
        .unwrap();
    assert_eq!(cancelled, vec![child.clone()]);
    assert_eq!(reopened.records.len(), records + 1, "one record each");
    assert_eq!(reopened.state.instances[&child].status, Status::Cancelled);
    assert_eq!(
        records_of(&reopened, RecordKind::InstanceCancelled)
            .last()
            .and_then(|r| r.body.get("reason"))
            .and_then(Value::as_str),
        Some("orphan")
    );
    assert!(reopened.orphaned_children().is_empty());

    // A clean store repairs nothing.
    let records = reopened.records.len();
    assert!(
        reopened
            .cancel_orphans_on(&mut FixedClock::new(9_500, 1), "repair-2")
            .unwrap()
            .is_empty()
    );
    assert_eq!(reopened.records.len(), records);
}

#[test]
fn a_directly_cancelled_child_returns_cancelled_to_its_parent() {
    let directory = TestDirectory::create();
    let (mut store, ids) = chain(&directory, 1);
    let child = ids[1].clone();
    store.cancel_instance(&child, "cancel-child").unwrap();
    let response = store.invocation_return("root", "down", "ret-1").unwrap();
    assert_eq!(
        response.get("outcome").and_then(Value::as_str),
        Some("cancelled"),
        "nothing about being a child removes an instance's ordinary operations"
    );
}
