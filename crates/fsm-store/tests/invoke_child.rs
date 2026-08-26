//! Enacting an invocation: one record creates a child, and fold derives the
//! child's whole existence from it.
//!
//! Plan 0010 task 4901.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::InvokeStatus;
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{NopSink, fold_with};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-invoke-{}-{n}", std::process::id()));
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

const CHILD: &str = r#"{"format":"fsm.machine/1","name":"reviewer","states":[{"name":"working"},{"name":"done","terminal":true}],"initial":"working","context":[{"name":"amount","ty":"int","init":"0"},{"name":"outcome","ty":"str","init":"pending"}],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"working","on":"finish","to":"done"}]}"#;

fn value(src: &str) -> Value {
    parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn child_digest() -> String {
    digest_of(&machine_id(&value(CHILD))).unwrap().to_string()
}

/// A parent whose `await_review` state invokes the reviewer.
fn parent_src() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"idle"}},{{"name":"await_review","invoke":[{{"id":"review","machine":"{}","with":{{"amount":"ctx.total"}},"returns":{{"decision":"outcome"}}}}]}},{{"name":"settled"}}],"initial":"idle","context":[{{"name":"total","ty":"int","init":"7"}}],"events":[{{"name":"open","fields":[]}},{{"name":"close","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"await_review"}},{{"from":"await_review","on":"close","to":"settled"}}]}}"#,
        child_digest()
    )
}

/// A store holding both machines with the parent in `await_review`.
fn opened(directory: &TestDirectory) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CHILD), false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, value(&parent_src()), false, false)
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
    store
}

fn records_of(store: &Store, kind: RecordKind) -> Vec<&Record> {
    store.records.iter().filter(|r| r.kind == kind).collect()
}

fn seqs(page: &Value) -> Vec<u64> {
    page.get("entries")
        .and_then(Value::as_arr)
        .unwrap()
        .iter()
        .map(|entry| {
            entry
                .get("seq")
                .and_then(Value::as_num)
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect()
}

#[test]
fn invoking_a_pending_slot_writes_one_record_and_creates_the_child() {
    let directory = TestDirectory::create();
    let mut store = opened(&directory);
    let before = store.records.len();
    let response = store.invoke_child("p1", "review", "inv-1").unwrap();
    let child_id = child_instance_id("p1", "review");
    assert_eq!(
        response.get("child_instance_id").and_then(Value::as_str),
        Some(child_id.as_str())
    );
    assert_eq!(
        response.get("duplicate").and_then(Value::as_bool),
        Some(false)
    );
    assert_eq!(store.records.len(), before + 1, "one record");
    assert_eq!(records_of(&store, RecordKind::InstanceInvoked).len(), 1);
    assert_eq!(
        store.state.instances["p1"].invocations["review"].status,
        InvokeStatus::Running
    );

    // The child exists, at the derived id, with the projection applied over
    // its own inits: `amount` from the parent, `outcome` from its `init`.
    let child = &store.state.instances[&child_id];
    assert_eq!(child.ctx["amount"].canonical_string(), "7");
    assert_eq!(child.ctx["outcome"].canonical_string(), "pending");
    assert_eq!(child.configuration.sequential_leaf(), Some("working"));
    assert_eq!(
        store.state.instance_machines[&child_id],
        machine_id(&value(CHILD))
    );
}

#[test]
fn history_shows_both_sides_and_the_index_agrees_with_the_view() {
    let directory = TestDirectory::create();
    let mut store = opened(&directory);
    store.invoke_child("p1", "review", "inv-1").unwrap();
    let child_id = child_instance_id("p1", "review");
    let invoked_seq = records_of(&store, RecordKind::InstanceInvoked)[0].seq;

    for id in ["p1", child_id.as_str()] {
        let page = store.history_page(id, 0, 500, false, true).unwrap();
        let listed = seqs(&page);
        assert!(
            listed.contains(&invoked_seq),
            "{id}'s history shows the invocation: {listed:?}"
        );
        assert_eq!(
            store.history.get(id).cloned().unwrap_or_default(),
            listed,
            "{id}: the folded index and the view agree"
        );
        // `explain` resolves the seq for both sides rather than reporting a
        // mismatch.
        store.explain_seq(id, invoked_seq).unwrap();
    }
}

#[test]
fn a_retry_replays_from_the_journal_after_a_restart() {
    let directory = TestDirectory::create();
    let mut store = opened(&directory);
    let first = store.invoke_child("p1", "review", "inv-1").unwrap();
    let records = store.records.len();
    drop(store);

    // The cold path: a reopened store has no response cache, so the reply is
    // rebuilt from the record alone.
    let mut reopened = Store::open(directory.path()).unwrap();
    let replayed = reopened.invoke_child("p1", "review", "inv-1").unwrap();
    assert_eq!(reopened.records.len(), records, "nothing was written");
    assert_eq!(
        replayed.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    for field in [
        "parent_instance_id",
        "slot",
        "child_instance_id",
        "child_machine_id",
        "seq",
    ] {
        assert_eq!(replayed.get(field), first.get(field), "{field}");
    }
}

#[test]
fn a_slot_that_is_not_pending_is_refused_and_the_refusal_replays() {
    let directory = TestDirectory::create();
    let mut store = opened(&directory);
    store.invoke_child("p1", "review", "inv-1").unwrap();
    let before = store.records.len();
    let error = store
        .invoke_child("p1", "review", "inv-2")
        .expect_err("the slot is running");
    assert_eq!(error.code, "req/invoke_slot_state");
    assert_eq!(store.records.len(), before + 1, "the refusal is journaled");
    assert_eq!(
        records_of(&store, RecordKind::RequestRejected).len(),
        1,
        "and it claims the key"
    );
    let again = store
        .invoke_child("p1", "review", "inv-2")
        .expect_err("the retry replays the refusal");
    assert_eq!(again.code, error.code);
    assert_eq!(store.records.len(), before + 1, "without writing again");

    let unknown = store
        .invoke_child("p1", "nosuch", "inv-3")
        .expect_err("no such slot");
    assert_eq!(unknown.code, "req/invoke_slot_state");
}

#[test]
fn a_failed_child_creation_journals_nothing_and_leaves_the_slot_pending() {
    // The child's invariant refuses the projection the parent supplies.
    let child = r#"{"format":"fsm.machine/1","name":"picky","states":[{"name":"working"}],"initial":"working","context":[{"name":"amount","ty":"int","init":"0"}],"events":[],"transitions":[],"invariants":[{"name":"small","expr":"ctx.amount < 5","mode":"enforce"}]}"#;
    let digest = digest_of(&machine_id(&value(child))).unwrap().to_string();
    let parent = format!(
        r#"{{"format":"fsm.machine/1","name":"parent","states":[{{"name":"idle"}},{{"name":"await_review","invoke":[{{"id":"review","machine":"{digest}","with":{{"amount":"ctx.total"}}}}]}}],"initial":"idle","context":[{{"name":"total","ty":"int","init":"7"}}],"events":[{{"name":"open","fields":[]}}],"transitions":[{{"from":"idle","on":"open","to":"await_review"}}]}}"#
    );
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(child), false, false)
        .unwrap();
    store
        .define_machine_on(&mut clock, value(&parent), false, false)
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
    let before = store.records.len();
    let error = store
        .invoke_child("p1", "review", "inv-1")
        .expect_err("the child's invariant refuses 7");
    assert_eq!(error.code, "run/invoke_create_failed");
    assert_eq!(store.records.len(), before, "nothing at all was journaled");
    assert_eq!(
        store.state.instances["p1"].invocations["review"].status,
        InvokeStatus::Pending,
        "the slot is still pending, so a corrected retry can enact it"
    );
}

#[test]
fn fold_derives_the_child_from_the_record_alone() {
    let directory = TestDirectory::create();
    let mut store = opened(&directory);
    store.invoke_child("p1", "review", "inv-1").unwrap();
    let child_id = child_instance_id("p1", "review");
    let expected = store.state.instances[&child_id].clone();
    let records = store.records.clone();
    drop(store);

    let folded = fold_with(records, &mut NopSink).expect("the journal folds");
    assert_eq!(
        folded.instances.get(&child_id),
        Some(&expected),
        "the child is reconstructed from the invocation record"
    );
    assert_eq!(
        folded.instances["p1"].invocations["review"].status,
        InvokeStatus::Running
    );
    // And a reopened store agrees, on both open paths.
    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(reopened.state.instances.get(&child_id), Some(&expected));
    assert!(reopened.history.contains_key(&child_id));
}

#[test]
fn a_read_only_store_refuses_to_invoke() {
    let directory = TestDirectory::create();
    let store = opened(&directory);
    drop(store);
    let mut reader = Store::open_read_only(directory.path()).unwrap();
    let error = reader
        .invoke_child("p1", "review", "inv-1")
        .expect_err("read-only");
    assert_eq!(error.code, "io/write");
}

#[test]
fn the_catalogue_rules_fire_at_definition_time() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    let unknown = format!(
        r#"{{"format":"fsm.machine/1","name":"orphan","states":[{{"name":"a","invoke":[{{"id":"review","machine":"{}"}}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#,
        "ab".repeat(32)
    );
    let Err(error) = store.define_machine_on(&mut clock, value(&unknown), false, false) else {
        panic!("the store holds no such machine");
    };
    assert_eq!(error.code, "def/invoke_unknown_machine");

    store
        .define_machine_on(&mut clock, value(CHILD), false, false)
        .unwrap();
    let digest = child_digest();
    let with = |slot: &str, context: &str| {
        format!(
            r#"{{"format":"fsm.machine/1","name":"p","states":[{{"name":"a","invoke":[{slot}]}}],"initial":"a","context":[{context}],"events":[],"transitions":[]}}"#
        )
    };
    let cases = [
        (
            format!(r#"{{"id":"review","machine":"{digest}","with":{{"nosuch":"ctx.total"}}}}"#),
            r#"{"name":"total","ty":"int","init":"1"}"#,
            "def/invoke_unknown_ctx",
        ),
        (
            format!(r#"{{"id":"review","machine":"{digest}","returns":{{"decision":"nosuch"}}}}"#),
            r#"{"name":"total","ty":"int","init":"1"}"#,
            "def/invoke_unknown_ctx",
        ),
        (
            format!(r#"{{"id":"review","machine":"{digest}","with":{{"amount":"ctx.label"}}}}"#),
            r#"{"name":"label","ty":"str","init":""}"#,
            "def/invoke_type",
        ),
    ];
    for (slot, context, code) in cases {
        let Err(error) =
            store.define_machine_on(&mut clock, value(&with(&slot, context)), false, false)
        else {
            panic!("{slot} must be refused");
        };
        assert_eq!(error.code, code, "{slot}");
    }

    // A chain of five machines is one deeper than the graph may be.
    let mut previous = child_digest();
    for level in 0..3 {
        let src = format!(
            r#"{{"format":"fsm.machine/1","name":"level{level}","states":[{{"name":"a","invoke":[{{"id":"down","machine":"{previous}"}}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
        );
        store
            .define_machine_on(&mut clock, value(&src), false, false)
            .unwrap_or_else(|e| panic!("level {level}: {e:?}"));
        previous = digest_of(&machine_id(&value(&src))).unwrap().to_string();
    }
    let too_deep = format!(
        r#"{{"format":"fsm.machine/1","name":"toodeep","states":[{{"name":"a","invoke":[{{"id":"down","machine":"{previous}"}}]}}],"initial":"a","context":[],"events":[],"transitions":[]}}"#
    );
    let Err(error) = store.define_machine_on(&mut clock, value(&too_deep), false, false) else {
        panic!("five machines deep");
    };
    assert_eq!(error.code, "def/invoke_depth");
}
