//! Native retry disposition preserves the original deadline across writer reopen.

use super::*;

pub(super) fn settle_retry(
    fixture: &mut Fixture,
    effect: &str,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
) {
    use fsm_core::record::execution::{PendingEffect, Settlement};
    let mut store = Store::open(&fixture.store).unwrap();
    let mut pipeline = fsm_execute::run::Pipeline;
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    let before = store.records.len();
    pipeline
        .stop_native(
            &mut store,
            &mut clock,
            claim,
            completion,
            "native-retry-stop",
        )
        .unwrap();
    assert_eq!(
        store
            .state
            .execution
            .settlement_for(claim, PendingEffect::Present)
            .unwrap(),
        Settlement::Attempted
    );
    let settled = settle_owned(fixture, &mut store, &mut clock, claim, completion);
    assert_eq!(settled.get("duplicate"), Some(&Value::Bool(false)));
    let replay = pipeline
        .settle_stopped(
            &mut store,
            &mut clock,
            claim,
            Settlement::Attempted,
            &fsm_execute::rid::attempt_rid(effect, 1),
        )
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(store.records.len(), before + 2);
    assert!(
        store
            .state
            .execution
            .claim_for("instance", effect)
            .is_none()
    );
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", effect)
            .is_none()
    );
    assert!(
        store.state.instances["instance"]
            .pending
            .iter()
            .any(|pending| pending == effect)
    );
    let execution = store.state.execution.clone();
    drop(store);
    let reopened = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(reopened.state.execution, execution);
    assert!(
        reopened.state.instances["instance"]
            .pending
            .iter()
            .any(|pending| pending == effect)
    );
    drop(reopened);
    let successor = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let material = claim.to_value();
    let retry =
        fsm_core::record::execution::RetryPolicy::from_value(material.get("retry").unwrap())
            .unwrap();
    let request = || fsm_store::store::ExecutionClaimRequest {
        instance_id: "instance",
        effect_id: effect,
        handler_fingerprint: material
            .get("handler_fingerprint")
            .unwrap()
            .as_str()
            .unwrap(),
        retry: &retry,
        domain: &successor,
        request_id: "native-retry-successor",
        expected_seq: None,
    };
    let mut store = Store::open(&fixture.store).unwrap();
    let before = store.records.len();
    // Stop consumes 1000 and Attempted consumes 1001; the original policy
    // permits the successor exactly ten milliseconds after settlement.
    let mut early = fsm_store::clock::FixedClock::new(1010, 1);
    let refused = pipeline
        .claim_native(&mut store, &mut early, request())
        .unwrap_err();
    assert_eq!(refused.code, "exec/store");
    assert_eq!(
        refused
            .details
            .as_ref()
            .unwrap()
            .get("code")
            .and_then(Value::as_str),
        Some("store/execution_retry")
    );
    assert_eq!(store.records.len(), before);
    assert_eq!(store.state.execution, execution);
    let mut due = fsm_store::clock::FixedClock::new(1011, 1);
    let mut changed_fingerprint = material
        .get("handler_fingerprint")
        .unwrap()
        .as_str()
        .unwrap()
        .as_bytes()
        .to_vec();
    changed_fingerprint[7] = if changed_fingerprint[7] == b'0' {
        b'1'
    } else {
        b'0'
    };
    let changed_fingerprint = String::from_utf8(changed_fingerprint).unwrap();
    let mut changed_policy = retry.to_value().as_obj().unwrap().clone();
    changed_policy.insert("attempts".into(), Value::Num("3".into()));
    let changed_policy =
        fsm_core::record::execution::RetryPolicy::from_value(&Value::Obj(changed_policy)).unwrap();
    for changed in [
        fsm_store::store::ExecutionClaimRequest {
            handler_fingerprint: &changed_fingerprint,
            ..request()
        },
        fsm_store::store::ExecutionClaimRequest {
            retry: &changed_policy,
            ..request()
        },
    ] {
        let refused = pipeline
            .claim_native(&mut store, &mut due, changed)
            .unwrap_err();
        assert_eq!(refused.code, "exec/store");
        assert_eq!(
            refused
                .details
                .as_ref()
                .unwrap()
                .get("code")
                .and_then(Value::as_str),
            Some("store/execution_contract")
        );
        assert_eq!(store.records.len(), before);
        assert_eq!(store.state.execution, execution);
        for name in ["binding", "launch", "entry", "handoff"] {
            assert_eq!(
                fs::symlink_metadata(fixture.directory.join(format!("{name}-2.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }
    pipeline
        .claim_native(&mut store, &mut due, request())
        .unwrap();
    assert_eq!(store.records.len(), before + 1);
    let next = store.state.execution.claim_for("instance", effect).unwrap();
    assert_eq!(next.run_id(), claim.run_id() + 1);
    let next_material = next.to_value();
    assert_eq!(next_material.get("attempt"), Some(&Value::Num("2".into())));
    assert_eq!(next_material.get("retry"), material.get("retry"));
    assert_eq!(
        next_material.get("handler_fingerprint"),
        material.get("handler_fingerprint")
    );
    assert_eq!(next_material.get("domain"), Some(&successor.to_value()));
    let retained = store.state.execution.clone();
    drop(store);
    assert_eq!(
        Store::open_read_only(&fixture.store)
            .unwrap()
            .state
            .execution,
        retained
    );
}
