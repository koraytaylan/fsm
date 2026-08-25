//! Derived keys are what make a restarted executor safe. These rows pin both
//! halves of that claim: the derivations are pure and distinct, and the store
//! really does replay one and refuse the other.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_execute::rid::{ack_rid, event_rid, poll_rid};
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-execute-{test_name}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory {path:?}: {error}"),
            }
        }
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

fn effect_machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"order_confirmation",
            "context":[{"name":"order_id","ty":"str","init":"order-0"}],
            "events":[{"name":"submit","fields":[]},{"name":"confirmed","fields":[]}],
            "effects":[{"name":"request_confirmation","fields":[{"name":"order","ty":"str"}]}],
            "states":[
                {"name":"placed"},
                {"name":"awaiting_confirmation","entry":{"emit":[
                    {"effect":"request_confirmation","args":{"order":"ctx.order_id"}}
                ]}},
                {"name":"confirmed_state","terminal":true}
            ],
            "initial":"placed",
            "transitions":[
                {"from":"placed","on":"submit","to":"awaiting_confirmation"},
                {"from":"awaiting_confirmation","on":"confirmed","to":"confirmed_state"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn result(text: &str) -> Value {
    Value::Obj(BTreeMap::from([
        ("status".into(), Value::Num("0".into())),
        ("stdout".into(), Value::Str(text.into())),
    ]))
}

/// A store holding one instance with one pending effect.
fn store_with_pending_effect(directory: &TestDirectory) -> (Store, FixedClock, String) {
    let mut store = Store::open(directory.path()).expect("open writer");
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, effect_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation",
            "order-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "order-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    let effect_id = store.state.instances["order-1"].pending[0].clone();
    (store, clock, effect_id)
}

fn acked_records(store: &Store) -> usize {
    store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::EffectAcked)
        .count()
}

#[test]
fn every_derivation_is_a_pure_function_of_its_inputs() {
    for _ in 0..4 {
        assert_eq!(ack_rid("order-1/3/0"), "exec-ack-order-1/3/0");
        assert_eq!(
            event_rid("order-1/3/0", "confirmed"),
            "exec-ev-order-1/3/0-confirmed"
        );
        assert_eq!(
            poll_rid("order-1", "review_timeout", 1_700_000_000_000),
            "exec-poll-7-order-1-review_timeout-1700000000000"
        );
    }
}

#[test]
fn keys_differ_whenever_the_observation_differs() {
    assert_ne!(ack_rid("order-1/3/0"), ack_rid("order-1/3/1"));
    assert_ne!(ack_rid("order-1/3/0"), ack_rid("order-2/3/0"));
    assert_ne!(
        event_rid("order-1/3/0", "confirmed"),
        event_rid("order-1/3/0", "confirmation_failed")
    );
    // A deadline that becomes due again is a new observation, so it gets a new
    // key rather than replaying the previous poll's outcome.
    assert_ne!(
        poll_rid("order-1", "review_timeout", 1_000),
        poll_rid("order-1", "review_timeout", 2_000)
    );
    assert_ne!(
        poll_rid("order-1", "review_timeout", 1_000),
        poll_rid("order-1", "escalation_timeout", 1_000)
    );
    // The parts are concatenated, so the instance id is length-prefixed: these
    // two would otherwise compose the same key and silence one deadline.
    assert_ne!(
        poll_rid("order-1", "expire", 1_000),
        poll_rid("order", "1-expire", 1_000)
    );
}

#[test]
fn a_key_is_unique_per_instance_sequence_and_emit_index() {
    let mut seen = BTreeSet::new();
    for instance in ["order-1", "order-2"] {
        for seq in 1..6u64 {
            for k in 0..3u32 {
                let effect_id = format!("{instance}/{seq}/{k}");
                assert!(seen.insert(ack_rid(&effect_id)), "{effect_id} collided");
                for event in ["confirmed", "confirmation_failed"] {
                    assert!(
                        seen.insert(event_rid(&effect_id, event)),
                        "{effect_id}/{event} collided"
                    );
                }
            }
        }
    }
}

#[test]
fn re_issuing_a_derived_ack_replays_instead_of_writing_twice() {
    let directory = TestDirectory::create("rid-duplicate");
    let (mut store, mut clock, effect_id) = store_with_pending_effect(&directory);
    let key = ack_rid(&effect_id);

    let first = store
        .ack_effect_outcome_on(
            &mut clock,
            "order-1",
            &effect_id,
            &key,
            "ok",
            Some(result("sent")),
        )
        .unwrap();
    assert_eq!(first.get("duplicate"), Some(&Value::Bool(false)));
    assert_eq!(acked_records(&store), 1);

    // The same derived key with the same content: the store replays it. This
    // is the restarted executor's path — it recomputes the key and the
    // captured output and re-issues both.
    let second = store
        .ack_effect_outcome_on(
            &mut clock,
            "order-1",
            &effect_id,
            &key,
            "ok",
            Some(result("sent")),
        )
        .unwrap();
    assert_eq!(second.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(acked_records(&store), 1, "no second effect_acked record");
}

#[test]
fn the_same_key_with_a_different_result_is_refused_not_replayed() {
    let directory = TestDirectory::create("rid-conflict");
    let (mut store, mut clock, effect_id) = store_with_pending_effect(&directory);
    let key = ack_rid(&effect_id);
    store
        .ack_effect_outcome_on(
            &mut clock,
            "order-1",
            &effect_id,
            &key,
            "ok",
            Some(result("sent")),
        )
        .unwrap();

    // Two writers racing one effect capture different output. The loser is
    // refused rather than silently overwriting the journaled outcome.
    let error = store
        .ack_effect_outcome_on(
            &mut clock,
            "order-1",
            &effect_id,
            &key,
            "ok",
            Some(result("sent twice")),
        )
        .unwrap_err();
    assert_eq!(error.code, "req/request_id_conflict");
    assert_eq!(acked_records(&store), 1);
}

#[test]
fn the_journal_itself_records_that_the_write_already_happened() {
    let directory = TestDirectory::create("rid-dedup");
    let (mut store, mut clock, effect_id) = store_with_pending_effect(&directory);
    let key = ack_rid(&effect_id);
    store
        .ack_effect_outcome_on(
            &mut clock,
            "order-1",
            &effect_id,
            &key,
            "ok",
            Some(result("sent")),
        )
        .unwrap();
    drop(store);

    // A fresh process keeps nothing in memory; what it knows, it reads. The
    // derived key is in the folded dedup map of a read-only open, which is the
    // fact every "have I already written this?" decision rests on.
    let reopened = Store::open_read_only(directory.path()).unwrap();
    assert!(reopened.state.dedup.contains_key(&key));
    assert!(
        !reopened
            .state
            .dedup
            .contains_key(&event_rid(&effect_id, "confirmed")),
        "the advance was never sent, so its key is unclaimed"
    );
}
