//! Store transitions with preauthenticated fixture proofs; no native-authentication claim.

use super::*;
use crate::clock::FixedClock;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::record::execution::Closure;

fn json(bytes: &[u8]) -> Value {
    parse(bytes, &JsonLimits::DEFAULT).unwrap()
}

fn domain() -> NativeDomain {
    NativeDomain::from_value(&json(br#"{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}"#)).unwrap()
}
fn policy() -> RetryPolicy {
    RetryPolicy::from_value(&json(
        br#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]}"#,
    ))
    .unwrap()
}
fn pending() -> (Store, String) {
    let mut store = Store::open_memory().unwrap();
    store
        .define_machine(
            json(include_bytes!(
                "../../../fsm-core/tests/fixtures/machines/case_review.json"
            )),
            false,
            false,
        )
        .unwrap();
    store
        .create_instance("case_review", "instance", "create", None)
        .unwrap();
    store
        .send_event(
            "instance",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send",
            None,
        )
        .unwrap();
    let effect = store.state.instances["instance"].pending[0].clone();
    (store, effect)
}
fn allocate(
    store: &mut Store,
    effect: &str,
    request_id: &str,
    clock: &mut FixedClock,
) -> Result<Value, ErrorObj> {
    store.claim_execution_on(clock, ExecutionClaimRequest {
        instance_id: "instance", effect_id: effect,
        handler_fingerprint: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        domain: &domain(), retry: &policy(), request_id, expected_seq: None,
    })
}
fn proof(store: &Store, claim: &Claim) -> VerifiedClosure {
    VerifiedClosure {
        closure: Closure::new(
            claim.run_id(),
            claim.domain().clone(),
            format!("sha256:{}", "b".repeat(64)),
        )
        .unwrap(),
        journal_claim: store.execution_claim_hash(claim).unwrap(),
    }
}
fn stop(store: &mut Store, claim: &Claim, status: &str, request_id: &str) -> Value {
    let outcome = StoppedOutcome::from_value(&json(
        format!(r#"{{"status":"{status}","result":null}}"#).as_bytes(),
    ))
    .unwrap();
    store
        .stop_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionStopRequest {
                claim,
                proof: &proof(store, claim),
                outcome: &outcome,
                request_id,
                expected_seq: None,
            },
        )
        .unwrap()
}
fn assert_fold(store: &Store) {
    let replay = fsm_core::replay::fold_with(store.records.clone(), &mut NopSink).unwrap();
    assert!(crate::snapshot::store_states_eq(&store.state, &replay));
}

#[test]
fn durable_stop_excludes_successor_until_single_failed_settlement_and_retry_deadline() {
    let (mut store, effect) = pending();
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    let stopped = stop(&mut store, &claim, "timeout", "stop");
    assert_fold(&store);
    let before = store.journal.last_seq;
    assert_eq!(
        allocate(&mut store, &effect, "overlap", &mut FixedClock::new(200, 1))
            .unwrap_err()
            .code,
        "store/execution_owned"
    );
    assert_eq!(store.journal.last_seq, before);
    store.last_responses.clear();
    let replay = stop(&mut store, &claim, "timeout", "stop");
    assert_eq!(replay.get("execution"), stopped.get("execution"));
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    store
        .settle_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim: &claim,
                disposition: Settlement::Attempted,
                request_id: "settle",
                expected_seq: None,
            },
        )
        .unwrap();
    assert_eq!(store.state.execution.failed_count("instance", &effect), 1);
    assert_fold(&store);
    let before = store.journal.last_seq;
    let mut clock = FixedClock::new(109, 1);
    assert_eq!(
        allocate(&mut store, &effect, "retry", &mut clock)
            .unwrap_err()
            .code,
        "store/execution_retry"
    );
    assert_eq!(clock.now, 109);
    assert_eq!(store.journal.last_seq, before);
    clock.now = 110;
    allocate(&mut store, &effect, "retry", &mut clock).unwrap();
    assert_eq!(store.state.execution.run_high_water(), 2);
    let successor = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    assert_eq!(
        successor.to_value().get("attempt"),
        Some(&Value::Num("2".into()))
    );
    let before = store.journal.last_seq;
    assert_eq!(
        store
            .settle_execution_on(
                &mut clock,
                ExecutionSettleRequest {
                    claim: &claim,
                    disposition: Settlement::Attempted,
                    request_id: "consume-twice",
                    expected_seq: None
                }
            )
            .unwrap_err()
            .code,
        "store/execution_stale"
    );
    assert_eq!(store.journal.last_seq, before);
    assert_fold(&store);
}

#[test]
fn settlement_ack_is_one_record_and_cold_replay_cannot_consume_again() {
    let (mut store, effect) = pending();
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    stop(&mut store, &claim, "ok", "stop");
    let before = store.journal.last_seq;
    let first = store
        .settle_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim: &claim,
                disposition: Settlement::Acked,
                request_id: "settle",
                expected_seq: None,
            },
        )
        .unwrap();
    assert_eq!(store.journal.last_seq, before + 1);
    assert!(!store.state.instances["instance"].pending.contains(&effect));
    assert!(
        store
            .state
            .execution
            .claim_for("instance", &effect)
            .is_none()
    );
    store.last_responses.clear();
    let replay = store
        .settle_execution_on(
            &mut FixedClock::new(101, 1),
            ExecutionSettleRequest {
                claim: &claim,
                disposition: Settlement::Acked,
                request_id: "settle",
                expected_seq: Some(before),
            },
        )
        .unwrap();
    assert_eq!(replay.get("execution"), first.get("execution"));
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(store.journal.last_seq, before + 1);
    assert_fold(&store);
}

#[test]
fn wrong_original_claim_receipt_refuses_without_mutation() {
    let (mut store, effect) = pending();
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    let mut proof = proof(&store, &claim);
    proof.journal_claim = format!("sha256:{}", "f".repeat(64));
    let outcome = StoppedOutcome::from_value(&json(br#"{"status":"ok"}"#)).unwrap();
    let before = store.journal.last_seq;
    assert_eq!(
        store
            .stop_execution_on(
                &mut FixedClock::new(100, 1),
                ExecutionStopRequest {
                    claim: &claim,
                    proof: &proof,
                    outcome: &outcome,
                    request_id: "wrong-proof",
                    expected_seq: None
                }
            )
            .unwrap_err()
            .code,
        "store/execution_evidence"
    );
    assert_eq!(store.journal.last_seq, before);
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", &effect)
            .is_none()
    );
    assert!(!store.state.dedup.contains_key("wrong-proof"));
}

#[test]
fn cancellation_drops_settled_retry_ledger_without_dropping_unresolved_claim() {
    let (mut store, effect) = pending();
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    stop(&mut store, &claim, "timeout", "stop");
    store
        .settle_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim: &claim,
                disposition: Settlement::Attempted,
                request_id: "settle",
                expected_seq: None,
            },
        )
        .unwrap();
    assert_eq!(store.state.execution.failed_count("instance", &effect), 1);
    store.cancel_instance("instance", "cancel").unwrap();
    assert_eq!(store.state.execution.failed_count("instance", &effect), 0);
    assert_fold(&store);
    let before = store.journal.last_seq;
    assert_eq!(
        allocate(
            &mut store,
            &effect,
            "cancelled-claim",
            &mut FixedClock::new(110, 1)
        )
        .unwrap_err()
        .code,
        "store/execution_stale"
    );
    assert_eq!(store.journal.last_seq, before);
}

#[test]
fn exact_outcome_budget_survives_stop_snapshot_and_base_without_releasing_ownership() {
    use fsm_core::canon::canon_bytes;
    let (mut store, effect) = pending();
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    let mut outcome = BTreeMap::from([
        ("status".into(), Value::Str("ok".into())),
        ("result".into(), Value::Str(String::new())),
    ]);
    let overhead = canon_bytes(&Value::Obj(outcome.clone())).len();
    // A control character costs six canonical bytes; the non-ASCII glyph costs two.
    let result = format!("\u{0001}é{}", "x".repeat(65536 - overhead - 8));
    outcome.insert("result".into(), Value::Str(result.clone()));
    let exact = Value::Obj(outcome.clone());
    assert_eq!(canon_bytes(&exact).len(), 65536);
    outcome.insert("result".into(), Value::Str(format!("{result}x")));
    let oversized = Value::Obj(outcome);
    assert_eq!(canon_bytes(&oversized).len(), 65537);
    assert_eq!(
        StoppedOutcome::from_value(&oversized).unwrap_err().0,
        "bytes"
    );
    let typed = StoppedOutcome::from_value(&exact).unwrap();
    let before = store.journal.last_seq;
    store
        .stop_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionStopRequest {
                claim: &claim,
                proof: &proof(&store, &claim),
                outcome: &typed,
                request_id: "exact-stop",
                expected_seq: Some(before),
            },
        )
        .unwrap();
    assert_eq!(store.journal.last_seq, before + 1);
    assert_eq!(
        store
            .state
            .execution
            .stopped_for("instance", &effect)
            .unwrap()
            .outcome()
            .to_value(),
        exact
    );
    assert_eq!(
        allocate(
            &mut store,
            &effect,
            "successor",
            &mut FixedClock::new(101, 1)
        )
        .unwrap_err()
        .code,
        "store/execution_owned"
    );
    assert_fold(&store);

    let snapshot = crate::snapshot::state_to_snapshot(&store.state);
    let restored = crate::snapshot::snapshot_to_state(&snapshot).unwrap();
    assert!(crate::snapshot::store_states_eq(&store.state, &restored));
    let mut index = crate::base::BaseIndex::default();
    index
        .execution_claims
        .insert(claim.run_id(), store.execution_claim_hash(&claim).unwrap());
    let roots = crate::base::base_roots(&store.state, &index);
    let base = crate::base::encode(&store.state, &index, crate::base::DefinitionLimits::Current);
    let (restored, _) = crate::base::decode(&base, &roots).unwrap();
    assert!(crate::snapshot::store_states_eq(&store.state, &restored));

    let Value::Obj(mut execution) = store.state.execution.to_value() else {
        panic!("execution must be an object");
    };
    let Some(Value::Arr(claims)) = execution.get_mut("claims") else {
        panic!("claims must be an array");
    };
    let Value::Obj(entry) = &mut claims[0] else {
        panic!("claim entry must be an object");
    };
    let Some(Value::Obj(stopped)) = entry.get_mut("stopped") else {
        panic!("stopped result must be an object");
    };
    stopped.insert("outcome".into(), oversized.clone());
    let Value::Obj(mut snapshot) = snapshot else {
        panic!("snapshot must be an object")
    };
    snapshot.insert("execution".into(), Value::Obj(execution.clone()));
    snapshot.insert("snapshot_hash".into(), Value::Str(String::new()));
    let hash = fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
        "fsm:snapshot:6",
        &Value::Obj(snapshot.clone()),
    ));
    snapshot.insert("snapshot_hash".into(), Value::Str(format!("sha256:{hash}")));
    assert_eq!(
        crate::snapshot::snapshot_to_state(&Value::Obj(snapshot))
            .unwrap_err()
            .message,
        "invalid execution field: bytes"
    );
    let Value::Obj(mut base) = base else {
        panic!("base must be an object")
    };
    base.insert("execution".into(), Value::Obj(execution));
    assert_eq!(
        crate::base::decode(&Value::Obj(base), &roots)
            .unwrap_err()
            .message,
        "base state file: invalid execution field: bytes"
    );
    let mut records = store.records.clone();
    let last = records.pop().unwrap();
    let Value::Obj(mut body) = last.body else {
        panic!("stop body must be an object")
    };
    body.insert("outcome".into(), oversized);
    records.push(fsm_core::record::seal(
        last.seq,
        last.ts,
        last.kind,
        Value::Obj(body),
        &last.prev,
    ));
    assert!(fsm_core::replay::fold_with(records, &mut NopSink).is_err());
}
