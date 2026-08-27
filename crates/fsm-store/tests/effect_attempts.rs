//! An attempt is a record, because a counter in memory is lost by exactly
//! the restart it exists to survive.
//!
//! Plan 0016 task 7401.

// A helper here hands back the store's own `ErrorObj`, as
// `journal_record_bounds.rs` does: boxing it would only make every assertion
// dereference to read a code.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-attempt-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

/// A machine whose transition emits one effect, so an instance has
/// something pending to attempt.
const CASE: &str = r#"{"format":"fsm.machine/1","name":"attempt_case","context":[{"name":"seen","ty":"int","init":"0"}],"effects":[{"name":"notify","fields":[]}],"states":[{"name":"open"},{"name":"held"}],"initial":"open","events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held","emit":[{"effect":"notify","args":{}}]}]}"#;

/// A store with one instance holding one pending effect.
fn pending(dir: &Scratch) -> (Store, String) {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "attempt_case",
            "inst-a",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event(
            "inst-a",
            "push",
            Value::Obj(BTreeMap::new()),
            "push-1",
            None,
        )
        .unwrap();
    let effect = store.state.instances["inst-a"]
        .pending
        .first()
        .cloned()
        .expect("the transition emitted an effect");
    (store, effect)
}

fn attempt(
    store: &mut Store,
    effect: &str,
    n: u64,
    request_id: &str,
) -> Result<Value, fsm_store::store::ErrorObj> {
    store.attempt_effect_on(
        &mut FixedClock::new(2_000, 1),
        "inst-a",
        effect,
        request_id,
        n,
        Some(value(r#"{"exit_code":"1"}"#)),
    )
}

#[test]
fn an_attempt_is_recorded_and_changes_nothing_else() {
    let dir = scratch("records");
    let (mut store, effect) = pending(&dir);
    let before_leaf = store.state.instances["inst-a"]
        .configuration
        .sequential_leaf()
        .map(str::to_string);
    let before_ctx = store.state.instances["inst-a"].ctx.clone();

    let recorded = attempt(&mut store, &effect, 1, "try-1").expect("attempt 1");
    assert_eq!(recorded.get("attempt").and_then(Value::as_num), Some("1"));
    assert_eq!(
        recorded.get("outcome").and_then(Value::as_str),
        Some("failed"),
        "a successful attempt is an ack, so this kind is always failed"
    );

    // The effect is still pending, which is what makes a retry a retry
    // rather than a re-emit.
    assert_eq!(
        store.state.instances["inst-a"].pending,
        vec![effect.clone()]
    );
    assert_eq!(
        store.state.instances["inst-a"]
            .configuration
            .sequential_leaf()
            .map(str::to_string),
        before_leaf
    );
    assert_eq!(store.state.instances["inst-a"].ctx, before_ctx);
    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::EffectAttempted)
            .count(),
        1
    );
}

#[test]
fn attempts_run_one_at_a_time_and_a_gap_is_refused() {
    let dir = scratch("sequence");
    let (mut store, effect) = pending(&dir);
    attempt(&mut store, &effect, 1, "try-1").expect("attempt 1");
    attempt(&mut store, &effect, 2, "try-2").expect("attempt 2 follows 1");

    // A gap would make the derived count unreliable, and an unreliable count
    // is worse than no retry at all: "three tries" would mean something
    // different after a crash.
    let skipped = attempt(&mut store, &effect, 4, "try-4").expect_err("3 is missing");
    assert_eq!(skipped.code, "req/args_invalid");
    assert!(skipped.hint.contains('3'), "{}", skipped.hint);

    // And the same number twice is the same refusal, under a fresh key.
    let repeated = attempt(&mut store, &effect, 1, "try-again").expect_err("1 already happened");
    assert_eq!(repeated.code, "req/args_invalid");
    assert_eq!(store.attempts_for("inst-a", &effect), 2);
}

#[test]
fn the_same_key_replays_and_different_content_conflicts() {
    let dir = scratch("idempotent");
    let (mut store, effect) = pending(&dir);
    let first = attempt(&mut store, &effect, 1, "try-1").unwrap();
    let records = store.records.len();

    let replayed = attempt(&mut store, &effect, 1, "try-1").expect("a replay is not a failure");
    assert_eq!(replayed.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(replayed.get("seq"), first.get("seq"));
    assert_eq!(store.records.len(), records);

    let conflict = store
        .attempt_effect_on(
            &mut FixedClock::new(2_000, 1),
            "inst-a",
            &effect,
            "try-1",
            1,
            Some(value(r#"{"exit_code":"2"}"#)),
        )
        .expect_err("a key means the content it was claimed for");
    assert_eq!(conflict.code, "req/request_id_conflict");
}

#[test]
fn the_cold_path_rebuilds_the_reply_from_the_journal_alone() {
    // The warm path is served by an in-memory response cache, so a
    // same-process retry proves nothing about the case that matters: the
    // one after the restart a retry exists to survive.
    let dir = scratch("cold");
    let (mut store, effect) = pending(&dir);
    let original = attempt(&mut store, &effect, 1, "try-1").unwrap();
    drop(store);

    let mut reopened = Store::open(&dir).expect("reopened");
    let replayed = reopened
        .attempt_effect_on(
            &mut FixedClock::new(3_000, 1),
            "inst-a",
            &effect,
            "try-1",
            1,
            Some(value(r#"{"exit_code":"1"}"#)),
        )
        .expect("the journal alone answers");
    assert_eq!(replayed.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(replayed.get("seq"), original.get("seq"));
    assert_eq!(replayed.get("attempt"), original.get("attempt"));
    assert_eq!(replayed.get("effect_id"), original.get("effect_id"));
    assert_eq!(
        replayed.get("result"),
        original.get("result"),
        "the capture travels with the replay"
    );
    // And no second record was written for it.
    assert_eq!(
        reopened
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::EffectAttempted)
            .count(),
        1
    );
}

#[test]
fn an_attempt_against_a_settled_effect_is_refused_and_journaled() {
    let dir = scratch("settled");
    let (mut store, effect) = pending(&dir);
    store
        .ack_effect_outcome_on(
            &mut FixedClock::new(2_000, 1),
            "inst-a",
            &effect,
            "ack-1",
            "ok",
            None,
        )
        .expect("acked");

    let refused = attempt(&mut store, &effect, 1, "late-try").expect_err("that effect is over");
    assert_eq!(refused.code, "req/field_unknown");
    // Journaled, and the key is claimed, so a retry replays the refusal.
    assert!(
        store
            .records
            .iter()
            .any(|record| record.kind == RecordKind::RequestRejected
                && record.body.get("request_id").and_then(Value::as_str) == Some("late-try")),
        "the refusal is in the audit trail"
    );
    let again = attempt(&mut store, &effect, 1, "late-try").expect_err("replayed");
    assert_eq!(again.code, "req/field_unknown");
}

#[test]
fn the_effect_still_acks_after_its_attempts() {
    let dir = scratch("acks");
    let (mut store, effect) = pending(&dir);
    attempt(&mut store, &effect, 1, "try-1").unwrap();
    attempt(&mut store, &effect, 2, "try-2").unwrap();
    let acked = store
        .ack_effect_outcome_on(
            &mut FixedClock::new(3_000, 1),
            "inst-a",
            &effect,
            "ack-1",
            "ok",
            None,
        )
        .expect("the third attempt worked");
    assert_eq!(acked.get("acked"), Some(&Value::Bool(true)));
    assert!(store.state.instances["inst-a"].pending.is_empty());
    assert_eq!(
        store.attempts_for("inst-a", &effect),
        2,
        "the trail still says how many times it took"
    );
}

#[test]
fn a_journal_with_attempts_folds_to_the_same_state_as_one_without() {
    // The property the whole retry design rests on: an attempt claims its
    // key and changes nothing else.
    let with = scratch("with");
    let (mut store, effect) = pending(&with);
    attempt(&mut store, &effect, 1, "try-1").unwrap();
    attempt(&mut store, &effect, 2, "try-2").unwrap();
    let with_attempts = store.state.instances["inst-a"].clone();
    drop(store);

    let without = scratch("without");
    let (store, _) = pending(&without);
    let without_attempts = store.state.instances["inst-a"].clone();
    drop(store);

    assert_eq!(
        with_attempts.configuration.sequential_leaf(),
        without_attempts.configuration.sequential_leaf()
    );
    assert_eq!(with_attempts.ctx, without_attempts.ctx);
    assert_eq!(with_attempts.pending.len(), without_attempts.pending.len());

    // And the store with attempts reopens: the fold accepts the new kind.
    let reopened = Store::open(&with).expect("a journal with attempt records opens");
    assert_eq!(
        reopened.state.instances["inst-a"].pending.len(),
        with_attempts.pending.len()
    );
}
