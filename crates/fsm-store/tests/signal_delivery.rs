//! Delivering a signal: one record names both instances, and every terminal
//! outcome is journaled rather than lost.
//!
//! Plan 0010 task 5002.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{NopSink, fold_with};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-signal-{}-{n}", std::process::id()));
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

/// The sender signals `ctx.counterparty` on entering `working`.
fn sender_src(event: &str, with: &str, target: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"sender","states":[{{"name":"idle"}},{{"name":"working","entry":{{"signal":[{{"to":"ctx.counterparty","event":"{event}"{with}}}]}}}}],"initial":"idle","context":[{{"name":"counterparty","ty":"str","init":"{target}"}},{{"name":"batch","ty":"str","init":"b7"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"idle","on":"go","to":"working"}}]}}"#
    )
}

/// The receiver takes `batch_ready` with a `str` field and cascades.
const RECEIVER: &str = r#"{"format":"fsm.machine/1","name":"receiver","states":[{"name":"waiting"},{"name":"holding"},{"name":"settled"},{"name":"done","terminal":true}],"initial":"waiting","context":[{"name":"batch","ty":"str","init":""}],"events":[{"name":"batch_ready","fields":[{"name":"batch","ty":"str"}]},{"name":"finish","fields":[]}],"transitions":[{"from":"waiting","on":"batch_ready","to":"holding","do":[{"target":"batch","value":"evt.batch"}]},{"from":"holding","if":"ctx.batch != \"\"","to":"settled"},{"from":"settled","on":"finish","to":"done"}]}"#;

/// An `ignore` machine with no matching transition.
const IGNORER: &str = r#"{"format":"fsm.machine/1","name":"ignorer","states":[{"name":"waiting"}],"initial":"waiting","on_unhandled":"ignore","context":[],"events":[{"name":"batch_ready","fields":[{"name":"batch","ty":"str"}]}],"transitions":[]}"#;

/// A store with the receiver created (as `inst-recv`) and the sender in
/// `working`, holding one pending signal.
fn ready(
    directory: &TestDirectory,
    receiver: &str,
    event: &str,
    with: &str,
    target: &str,
) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(receiver), false, false)
        .unwrap();
    let name = value(receiver)
        .get("name")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    store
        .create_instance_ctx_on(
            &mut clock,
            &name,
            "inst-recv",
            "c-recv",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .define_machine_on(
            &mut clock,
            value(&sender_src(event, with, target)),
            false,
            false,
        )
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "sender",
            "inst-send",
            "c-send",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("inst-send", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    store
}

fn only_signal(store: &Store) -> String {
    store.state.instances["inst-send"]
        .signals
        .keys()
        .next()
        .cloned()
        .expect("the sender holds one pending signal")
}

fn records_of(store: &Store, kind: RecordKind) -> Vec<&Record> {
    store.records.iter().filter(|r| r.kind == kind).collect()
}

const WITH_BATCH: &str = r#","with":{"batch":"ctx.batch"}"#;

#[test]
fn delivering_advances_the_target_and_clears_the_senders_entry() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, RECEIVER, "batch_ready", WITH_BATCH, "inst-recv");
    let signal_id = only_signal(&store);
    let sender_before = store.state.instances["inst-send"].clone();
    let response = store
        .signal_deliver("inst-send", &signal_id, "sig-1")
        .unwrap();
    assert_eq!(
        response.get("outcome").and_then(Value::as_str),
        Some("applied")
    );

    let record = records_of(&store, RecordKind::SignalDelivered)[0];
    for (field, want) in [
        ("sender_instance_id", "inst-send"),
        ("target_instance_id", "inst-recv"),
        ("event", "batch_ready"),
    ] {
        assert_eq!(record.body.get(field).and_then(Value::as_str), Some(want));
    }
    assert!(record.body.get("target_state_hash").is_some());
    // The target advanced, cascade and all, in this one record.
    let target = &store.state.instances["inst-recv"];
    assert_eq!(target.configuration.sequential_leaf(), Some("settled"));
    assert_eq!(target.ctx["batch"].canonical_string(), "b7");
    assert_eq!(
        record
            .body
            .get("microsteps")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        Some(1),
        "the target's reaction is sealed here too"
    );
    // The sender lost only its pending entry.
    let sender = &store.state.instances["inst-send"];
    assert!(sender.signals.is_empty());
    assert_eq!(sender.configuration, sender_before.configuration);
    assert_eq!(sender.ctx, sender_before.ctx);
}

#[test]
fn every_terminal_outcome_is_journaled_and_clears_the_entry() {
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        // receiver, event, with, target, expected outcome
        (
            RECEIVER,
            "no_such_event",
            "",
            "inst-recv",
            "rejected:req/event_unknown",
        ),
        (
            RECEIVER,
            "batch_ready",
            "",
            "inst-recv",
            "rejected:req/field_missing",
        ),
        (
            RECEIVER,
            "batch_ready",
            WITH_BATCH,
            "inst-nobody",
            "target_missing",
        ),
        (IGNORER, "batch_ready", WITH_BATCH, "inst-recv", "ignored"),
    ];
    for (receiver, event, with, target, expected) in cases {
        let directory = TestDirectory::create();
        let mut store = ready(&directory, receiver, event, with, target);
        let signal_id = only_signal(&store);
        let response = store
            .signal_deliver("inst-send", &signal_id, "sig-1")
            .unwrap_or_else(|e| panic!("{expected}: {e:?}"));
        assert_eq!(
            response.get("outcome").and_then(Value::as_str),
            Some(*expected),
            "{event} → {target}"
        );
        assert_eq!(records_of(&store, RecordKind::SignalDelivered).len(), 1);
        assert!(
            store.state.instances["inst-send"].signals.is_empty(),
            "{expected}: the entry clears whatever happened"
        );
    }

    // A settled target: cancel the receiver first.
    let directory = TestDirectory::create();
    let mut store = ready(&directory, RECEIVER, "batch_ready", WITH_BATCH, "inst-recv");
    store.cancel_instance("inst-recv", "cancel-1").unwrap();
    let signal_id = only_signal(&store);
    let response = store
        .signal_deliver("inst-send", &signal_id, "sig-1")
        .unwrap();
    assert_eq!(
        response.get("outcome").and_then(Value::as_str),
        Some("target_settled")
    );
    assert!(store.state.instances["inst-send"].signals.is_empty());
}

#[test]
fn self_delivery_is_refused_and_journals_nothing() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, RECEIVER, "batch_ready", WITH_BATCH, "inst-send");
    let signal_id = only_signal(&store);
    let before = store.records.len();
    let error = store
        .signal_deliver("inst-send", &signal_id, "sig-1")
        .expect_err("a signal to its own sender");
    assert_eq!(error.code, "req/signal_target");
    assert!(error.hint.contains("raise"), "{}", error.hint);
    assert_eq!(store.records.len(), before, "nothing is journaled");
    assert!(!store.state.instances["inst-send"].signals.is_empty());
}

#[test]
fn a_retry_replays_from_the_journal_after_a_restart() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, RECEIVER, "no_such_event", "", "inst-recv");
    let signal_id = only_signal(&store);
    let first = store
        .signal_deliver("inst-send", &signal_id, "sig-1")
        .unwrap();
    assert_eq!(
        first.get("outcome").and_then(Value::as_str),
        Some("rejected:req/event_unknown")
    );
    let records = store.records.len();
    drop(store);

    let mut reopened = Store::open(directory.path()).unwrap();
    let replayed = reopened
        .signal_deliver("inst-send", &signal_id, "sig-1")
        .unwrap();
    assert_eq!(reopened.records.len(), records, "nothing was written");
    assert_eq!(
        replayed.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    for field in [
        "sender_instance_id",
        "target_instance_id",
        "event",
        "outcome",
        "seq",
    ] {
        assert_eq!(replayed.get(field), first.get(field), "{field}");
    }
}

#[test]
fn the_journal_folds_to_the_same_state() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory, RECEIVER, "batch_ready", WITH_BATCH, "inst-recv");
    let signal_id = only_signal(&store);
    store
        .signal_deliver("inst-send", &signal_id, "sig-1")
        .unwrap();
    let expected = store.state.instances.clone();
    let records = store.records.clone();
    drop(store);
    let folded = fold_with(records, &mut NopSink).expect("the journal folds");
    assert_eq!(folded.instances, expected);
    assert_eq!(
        Store::open(directory.path()).unwrap().state.instances,
        expected
    );
}

#[test]
fn a_read_only_store_refuses_to_deliver() {
    let directory = TestDirectory::create();
    let store = ready(&directory, RECEIVER, "batch_ready", WITH_BATCH, "inst-recv");
    let signal_id = only_signal(&store);
    drop(store);
    let mut reader = Store::open_read_only(directory.path()).unwrap();
    let error = reader
        .signal_deliver("inst-send", &signal_id, "sig-1")
        .expect_err("read-only");
    assert_eq!(error.code, "io/write");
}
