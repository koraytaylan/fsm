//! Downstream construction and use of the pure claim-era ownership API.

use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_core::record::execution::{
    Admission, Claim, Closure, ExecutionState, FailureClass, FileIdentity, NativeDomain,
    PendingEffect, RetryPolicy, Settlement, Stopped, StoppedOutcome,
};

#[test]
fn external_caller_can_construct_stop_settle_and_restore_execution_ownership() {
    let domain = NativeDomain::new(
        "0123456789abcdef0123456789abcdef".into(),
        1,
        "01234567-89ab-cdef-0123-456789abcdef".into(),
        FileIdentity::new(0, 42).unwrap(),
        FileIdentity::new(8, 43).unwrap(),
        1,
    )
    .unwrap();
    let policy = RetryPolicy::new(2, 10, 20, vec![FailureClass::Timeout]).unwrap();
    let metadata = Value::Obj(BTreeMap::from([
        ("run_id".into(), Value::Num("1".into())),
        ("instance_id".into(), Value::Str("instance".into())),
        ("effect_id".into(), Value::Str("effect".into())),
        ("attempt".into(), Value::Num("1".into())),
        (
            "handler_fingerprint".into(),
            Value::Str(format!("sha256:{}", "a".repeat(64))),
        ),
        ("retry".into(), policy.to_value()),
        ("domain".into(), domain.to_value()),
    ]));
    let claim = Claim::from_value(&metadata).unwrap();
    assert_eq!(claim.effect(), ("instance", "effect"));
    let mut state = ExecutionState::new(Admission::Enabled);
    state
        .claim(claim.clone(), PendingEffect::Present, 100)
        .unwrap();
    let closure =
        Closure::new(claim.run_id(), domain, format!("sha256:{}", "b".repeat(64))).unwrap();
    let outcome = StoppedOutcome::from_value(&Value::Obj(BTreeMap::from([(
        "status".into(),
        Value::Str("ok".into()),
    )])))
    .unwrap();
    state.stop(&claim, Stopped::new(closure, outcome)).unwrap();
    let mut restored = ExecutionState::from_value(&state.to_value()).unwrap();
    assert!(restored.stopped_for("instance", "effect").is_some());
    let mut projected = fsm_core::replay::StoreState {
        execution: restored.clone(),
        ..fsm_core::replay::StoreState::default()
    };
    assert_eq!(projected.execution.unresolved().count(), 1);
    projected.retain_pending_execution();
    assert_eq!(
        projected.execution, restored,
        "unresolved ownership survives removal of its effect"
    );
    restored
        .settle(&claim, Settlement::Acked, PendingEffect::Present, 100)
        .unwrap();
    assert!(restored.claim_for("instance", "effect").is_none());
    assert_eq!(restored.next_run_id().unwrap(), 2);
}
