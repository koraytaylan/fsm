use std::collections::BTreeMap;

use fsm_cli::store::Store;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{Scratch, case, gate, tmp};

#[test]
fn override_survives_reopen() {
    let _g = gate();
    let dir = tmp("ov");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    let mut ov = BTreeMap::new();
    ov.insert("visits".into(), Val::Int(2));
    s.create_instance_ctx("case_review", "i1", "r1", None, &ov, &[])
        .unwrap();
    drop(s);
    let s2 = Store::open(&dir).unwrap();
    let inst = s2.state.instances.get("i1").unwrap();
    assert_eq!(inst.ctx.get("visits"), Some(&Val::Int(2)));
}

fn enum_ctx() -> Value {
    parse(
        include_bytes!("../../../fsm-core/tests/fixtures/machines/enum_ctx.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn premium() -> Val {
    Val::Enum {
        ty: "Tier".into(),
        variant: "premium".into(),
    }
}

/// `canonical_string` writes an enum qualified (`Tier.premium`); the journal
/// reader must strip that prefix rather than take the whole string as the
/// variant. When it did not, creating an instance with an enum override wrote
/// a journal that could not be folded back — the store failed to open at all,
/// with `store/state_hash_mismatch`, and every later command was locked out.
/// `override_survives_reopen` missed it because `visits` is an `int`.
#[test]
fn enum_override_survives_reopen() {
    let _g = gate();
    let dir = tmp("enum-ov");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(enum_ctx(), false, false).unwrap();
    let mut ov = BTreeMap::new();
    ov.insert("level".into(), premium());
    s.create_instance_ctx("enum_ctx", "i1", "r1", None, &ov, &[])
        .unwrap();
    let before = s.state.instances.get("i1").unwrap().clone();
    drop(s);

    let s2 = Store::open(&dir).expect("store must reopen after an enum override");
    let after = s2.state.instances.get("i1").unwrap();
    assert_eq!(after.ctx.get("level"), Some(&premium()));
    assert_eq!(
        after.ctx, before.ctx,
        "folded context must equal the live context"
    );
}

/// The same round-trip, reached through a `do` block instead of an override,
/// so the fold path is covered for enums written by the engine itself.
#[test]
fn enum_set_by_transition_survives_reopen() {
    let _g = gate();
    let dir = tmp("enum-set");
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(enum_ctx(), false, false).unwrap();
    s.create_instance("enum_ctx", "i1", "r1", None).unwrap();
    s.send_event("i1", "upgrade", Value::Obj(BTreeMap::new()), "r2", None)
        .unwrap();
    assert_eq!(
        s.state.instances.get("i1").unwrap().ctx.get("level"),
        Some(&premium())
    );
    drop(s);

    let s2 = Store::open(&dir).expect("store must reopen after an enum set");
    assert_eq!(
        s2.state.instances.get("i1").unwrap().ctx.get("level"),
        Some(&premium())
    );
}

// --- request_id is an idempotency key, not a free-form label -----------------
//
// Reusing a `request_id` for different content used to return the *original*
// outcome with `duplicate: true`. A driver whose ids derive from (task, event)
// rather than per-attempt would see `applied: true` for an event that never
// landed, and the instance would sit in its old state while the driver believed
// it had advanced. Standard idempotency-key semantics: same key + different
// params is a conflict, not a replay.

fn conflict_fixture(tag: &str) -> (Scratch, Store) {
    let dir = tmp(tag);
    let mut s = Store::open(&dir).unwrap();
    s.define_machine(case(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    (dir, s)
}

fn empty() -> Value {
    Value::Obj(BTreeMap::new())
}

fn err_code(r: Result<Value, fsm_cli::store::ErrorObj>) -> String {
    match r {
        Ok(v) => panic!("expected an error, got {v:?}"),
        Err(e) => e.code,
    }
}

#[test]
fn reused_request_id_with_a_different_event_is_a_conflict() {
    let _g = gate();
    let (_dir, mut s) = conflict_fixture("conflict-evt");
    s.send_event("i1", "docs_ok", empty(), "R", None).unwrap();
    let configuration_after_first = s.state.instances.get("i1").unwrap().configuration.clone();

    // A different event under the same key must not be answered with the first
    // event's outcome.
    let e = err_code(s.send_event("i1", "note_added", empty(), "R", None));
    assert_eq!(e, "req/request_id_conflict");
    assert_eq!(
        s.state.instances.get("i1").unwrap().configuration,
        configuration_after_first,
        "a conflicting send must not advance the instance"
    );
}

#[test]
fn reused_request_id_with_a_different_payload_is_a_conflict() {
    let _g = gate();
    let (_dir, mut s) = conflict_fixture("conflict-payload");
    // Two `docs_ok` to reach risk_review, where `scored` is accepted.
    s.send_event("i1", "docs_ok", empty(), "s1", None).unwrap();
    s.send_event("i1", "docs_ok", empty(), "s2", None).unwrap();
    let mut p1 = BTreeMap::new();
    p1.insert("score".into(), Value::Str("10".into()));
    s.send_event("i1", "scored", Value::Obj(p1), "R", None)
        .unwrap();

    let mut p2 = BTreeMap::new();
    p2.insert("score".into(), Value::Str("99".into()));
    let e = err_code(s.send_event("i1", "scored", Value::Obj(p2), "R", None));
    assert_eq!(e, "req/request_id_conflict");
}

#[test]
fn reused_request_id_across_operations_is_a_conflict() {
    let _g = gate();
    let (_dir, mut s) = conflict_fixture("conflict-op");
    s.send_event("i1", "docs_ok", empty(), "R", None).unwrap();
    // Same key, same instance, different operation entirely.
    let e = err_code(s.cancel_instance_reason("i1", "R", "stop"));
    assert_eq!(e, "req/request_id_conflict");
    assert_eq!(
        s.state.instances.get("i1").unwrap().status,
        fsm_core::machine::Status::Running,
        "a conflicting cancel must not cancel the instance"
    );
}

#[test]
fn reused_request_id_on_a_different_instance_is_a_conflict() {
    let _g = gate();
    let (_dir, mut s) = conflict_fixture("conflict-inst");
    s.create_instance("case_review", "i2", "c2", None).unwrap();
    s.send_event("i1", "docs_ok", empty(), "R", None).unwrap();
    let e = err_code(s.send_event("i2", "docs_ok", empty(), "R", None));
    assert_eq!(e, "req/request_id_conflict");
}

/// The other half of the contract: an honest retry — byte-identical request,
/// same key — must still replay, or the fix would have broken idempotency.
#[test]
fn identical_retry_still_replays() {
    let _g = gate();
    let (_dir, mut s) = conflict_fixture("retry-ok");
    let first = s.send_event("i1", "docs_ok", empty(), "R", None).unwrap();
    let again = s.send_event("i1", "docs_ok", empty(), "R", None).unwrap();
    assert_eq!(
        again.get("duplicate"),
        Some(&Value::Bool(true)),
        "retry must be marked as a replay"
    );
    assert_eq!(
        again.get("state_hash"),
        first.get("state_hash"),
        "replay must return the original outcome"
    );
}

/// `expect_seq` is a concurrency precondition, not request content. A caller
/// that re-reads the instance and retries with a refreshed `expect_seq` is
/// retrying the same request, so it must replay rather than conflict.
#[test]
fn expect_seq_is_not_part_of_the_fingerprint() {
    let _g = gate();
    let (_dir, mut s) = conflict_fixture("retry-seq");
    let at = s.journal.last_seq;
    s.send_event("i1", "docs_ok", empty(), "R", Some(at))
        .unwrap();
    let again = s
        .send_event("i1", "docs_ok", empty(), "R", Some(s.journal.last_seq))
        .unwrap();
    assert_eq!(again.get("duplicate"), Some(&Value::Bool(true)));
}

/// Fingerprints live in the journal records, so the ledger survives a restart.
#[test]
fn request_id_conflict_survives_reopen() {
    let _g = gate();
    let (dir, mut s) = conflict_fixture("conflict-reopen");
    s.send_event("i1", "docs_ok", empty(), "R", None).unwrap();
    drop(s);

    let mut s2 = Store::open(&dir).unwrap();
    assert_eq!(
        err_code(s2.send_event("i1", "note_added", empty(), "R", None)),
        "req/request_id_conflict",
        "the fingerprint must be rebuilt by the fold, not held only in memory"
    );
    let again = s2.send_event("i1", "docs_ok", empty(), "R", None).unwrap();
    assert_eq!(
        again.get("duplicate"),
        Some(&Value::Bool(true)),
        "an honest retry must still replay after a reopen"
    );
}

/// A rejected request also claims its key. Reusing that key for different
/// content must conflict rather than replay the rejection.
#[test]
fn a_rejected_request_claims_its_key_too() {
    let _g = gate();
    let (_dir, mut s) = conflict_fixture("conflict-rejected");
    // `resume` is unhandled at the initial leaf: rejected, but journalled.
    assert_eq!(
        err_code(s.send_event("i1", "resume", empty(), "R", None)),
        "run/unhandled"
    );
    assert_eq!(
        err_code(s.send_event("i1", "docs_ok", empty(), "R", None)),
        "req/request_id_conflict"
    );
    // ...and retrying the rejected request replays its rejection.
    assert_eq!(
        err_code(s.send_event("i1", "resume", empty(), "R", None)),
        "run/unhandled"
    );
}
