//! Returning an invocation: the child's result becomes the parent's event.
//!
//! Plan 0010 task 4902.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{InvokeStatus, Status};
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{NopSink, fold_with};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-return-{}-{n}", std::process::id()));
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

/// The child writes its decision into `outcome` and the amount into
/// `settled`, both of which the parent projects out.
const CHILD: &str = r#"{"format":"fsm.machine/1","name":"reviewer","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[{"name":"amount","ty":{"decimal":"2"},"init":"0.00"},{"name":"outcome","ty":"str","init":"pending"}],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done","do":[{"target":"outcome","value":"\"approved\""}]}]}"#;

fn value(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn child_digest() -> String {
    digest_of(&machine_id(&value(CHILD))).unwrap().to_string()
}

/// A parent that handles the done event, optionally cascading afterwards.
fn parent_src(handler: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"idle"}},{{"name":"await_review","invoke":[{{"id":"review","machine":"{}","with":{{"amount":"ctx.total"}},"returns":{{"decision":"outcome","amount":"amount"}}}}]}},{{"name":"settled"}},{{"name":"closed"}}],"initial":"idle","context":[{{"name":"total","ty":{{"decimal":"2"}},"init":"1.50"}},{{"name":"seen","ty":"str","init":""}},{{"name":"paid","ty":{{"decimal":"2"}},"init":"0.00"}}],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"await_review"}}{handler}]}}"#,
        child_digest()
    )
}

/// Define both machines, create the parent, enter `await_review`, invoke.
fn ready(directory: &TestDirectory, handler: &str) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CHILD), false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, value(&parent_src(handler)), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "parent",
            "p1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("p1", "open", Value::Obj(BTreeMap::new()), "open-1", None)
        .unwrap();
    store.invoke_child("p1", "review", "inv-1").unwrap();
    store
}

fn finish_child(store: &mut Store) {
    let child = child_instance_id("p1", "review");
    store
        .send_event(&child, "finish", Value::Obj(BTreeMap::new()), "fin-1", None)
        .unwrap();
}

fn records_of(store: &Store, kind: RecordKind) -> Vec<&Record> {
    store.records.iter().filter(|r| r.kind == kind).collect()
}

/// The parent takes the done event into `settled`, reading both projected
/// fields.
const READS: &str = r#",{"from":"await_review","on":"$done.invoke.review","to":"settled","do":[{"target":"seen","value":"evt.decision"},{"target":"paid","value":"evt.amount"}]}"#;

#[test]
fn returning_delivers_the_projection_and_marks_the_slot_returned() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, READS);
    finish_child(&mut store);
    let before = store.records.len();
    let response = store.invocation_return("p1", "review", "ret-1").unwrap();
    assert_eq!(
        response.get("outcome").and_then(Value::as_str),
        Some("completed")
    );
    assert_eq!(store.records.len(), before + 1);
    assert_eq!(records_of(&store, RecordKind::InvocationReturned).len(), 1);

    let parent = &store.state.instances["p1"];
    assert_eq!(parent.configuration.sequential_leaf(), Some("settled"));
    assert_eq!(parent.ctx["seen"].canonical_string(), "approved");
    assert_eq!(
        parent.ctx["paid"].canonical_string(),
        "1.50",
        "the child's two-scale decimal survived the projection"
    );
    assert!(
        !parent.invocations.contains_key("review"),
        "this handler left `await_review`, and exiting a state removes its slots"
    );
}

/// A handler that stays in the invoking state: the slot survives the return
/// as `Returned`, which is what lets a parent read the result and then move
/// on in its own time.
#[test]
fn a_parent_that_stays_keeps_the_slot_as_returned() {
    let directory = TestDirectory::create();
    let handler = r#",{"from":"await_review","on":"$done.invoke.review","do":[{"target":"seen","value":"evt.decision"}]}"#;
    let mut store = ready(&directory, handler);
    finish_child(&mut store);
    store.invocation_return("p1", "review", "ret-1").unwrap();
    let parent = &store.state.instances["p1"];
    assert_eq!(parent.configuration.sequential_leaf(), Some("await_review"));
    assert_eq!(parent.ctx["seen"].canonical_string(), "approved");
    assert_eq!(parent.invocations["review"].status, InvokeStatus::Returned);
    // And a second return is refused, naming the status it holds.
    let error = store
        .invocation_return("p1", "review", "ret-2")
        .expect_err("the slot is returned");
    assert_eq!(error.code, "req/invoke_slot_state");
    assert!(error.message.contains("returned"), "{}", error.message);
}

#[test]
fn a_cancelled_child_returns_an_empty_payload_and_the_handler_still_fires() {
    let directory = TestDirectory::create();
    let handler = r#",{"from":"await_review","on":"$done.invoke.review","to":"settled"}"#;
    let mut store = ready(&directory, handler);
    let child = child_instance_id("p1", "review");
    store.cancel_instance(&child, "cancel-1").unwrap();
    store.invocation_return("p1", "review", "ret-1").unwrap();
    let record = records_of(&store, RecordKind::InvocationReturned)[0];
    assert_eq!(
        record.body.get("outcome").and_then(Value::as_str),
        Some("cancelled")
    );
    assert_eq!(
        record
            .body
            .get("payload")
            .and_then(Value::as_obj)
            .map(BTreeMap::len),
        Some(0),
        "a cancelled child projects nothing; the parent's definition decides what that means"
    );
    assert_eq!(
        store.state.instances["p1"].configuration.sequential_leaf(),
        Some("settled")
    );
}

#[test]
fn the_slot_and_the_childs_status_are_both_gates() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, READS);
    // The child is still running.
    let error = store
        .invocation_return("p1", "review", "ret-early")
        .expect_err("the child has not settled");
    assert_eq!(error.code, "req/invoke_slot_state");
    assert!(error.message.contains("still running"), "{}", error.message);

    finish_child(&mut store);
    store.invocation_return("p1", "review", "ret-1").unwrap();
    // This handler left the invoking state, so the slot went with it.
    let error = store
        .invocation_return("p1", "review", "ret-2")
        .expect_err("the slot is gone");
    assert_eq!(error.code, "req/invoke_slot_state");
    assert!(error.message.contains("no such"), "{}", error.message);

    // And a pending slot cannot return either.
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CHILD), false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, value(&parent_src(READS)), false, false)
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
    let error = store
        .invocation_return("p1", "review", "ret-pending")
        .expect_err("the slot is pending");
    assert_eq!(error.code, "req/invoke_slot_state");
}

#[test]
fn a_retry_replays_from_the_journal_after_a_restart() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, READS);
    finish_child(&mut store);
    let first = store.invocation_return("p1", "review", "ret-1").unwrap();
    let records = store.records.len();
    drop(store);

    let mut reopened = Store::open(directory.path()).unwrap();
    let replayed = reopened.invocation_return("p1", "review", "ret-1").unwrap();
    assert_eq!(reopened.records.len(), records, "nothing was written");
    assert_eq!(
        replayed.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    for field in [
        "parent_instance_id",
        "slot",
        "child_instance_id",
        "outcome",
        "seq",
    ] {
        assert_eq!(replayed.get(field), first.get(field), "{field}");
    }
}

#[test]
fn a_cascading_parent_seals_its_whole_reaction_in_the_one_record() {
    let directory = TestDirectory::create();
    let handler = r#",{"from":"await_review","on":"$done.invoke.review","to":"settled","do":[{"target":"seen","value":"evt.decision"}]},{"from":"settled","if":"ctx.seen == \"approved\"","to":"closed"}"#;
    let mut store = ready(&directory, handler);
    finish_child(&mut store);
    store.invocation_return("p1", "review", "ret-1").unwrap();
    let record = records_of(&store, RecordKind::InvocationReturned)[0];
    let microsteps = record
        .body
        .get("microsteps")
        .and_then(Value::as_arr)
        .expect("the reaction is in this record");
    assert_eq!(microsteps.len(), 1);
    assert_eq!(
        store.state.instances["p1"].configuration.sequential_leaf(),
        Some("closed")
    );
}

#[test]
fn a_parent_with_no_handler_still_commits_and_returns() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, "");
    finish_child(&mut store);
    store.invocation_return("p1", "review", "ret-1").unwrap();
    assert_eq!(records_of(&store, RecordKind::InvocationReturned).len(), 1);
    let parent = &store.state.instances["p1"];
    assert_eq!(parent.invocations["review"].status, InvokeStatus::Returned);
    assert_eq!(
        parent.configuration.sequential_leaf(),
        Some("await_review"),
        "nothing handled it, so the parent did not move"
    );
}

#[test]
fn the_journal_folds_to_the_same_state() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, READS);
    finish_child(&mut store);
    store.invocation_return("p1", "review", "ret-1").unwrap();
    let expected = store.state.instances.clone();
    let records = store.records.clone();
    drop(store);
    let folded = fold_with(records, &mut NopSink).expect("the journal folds");
    assert_eq!(folded.instances, expected);
    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(reopened.state.instances, expected);
    assert_eq!(reopened.state.instances["p1"].status, Status::Running);
}
