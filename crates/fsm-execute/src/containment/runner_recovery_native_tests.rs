//! Reopening a genuine stopped run retains exclusion until durable disposition.

use super::*;

pub(super) fn reopen_stopped(
    fixture: &Fixture,
    effect: &str,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
    mode: &str,
) -> (Store, usize) {
    use fsm_core::record::RecordKind;
    use fsm_execute::run::Pipeline;

    let mut store = Store::open(&fixture.store).unwrap();
    let before = store.records.len();
    assert!(
        store
            .state
            .execution
            .stopped_for("instance", effect)
            .is_none()
    );
    let hash = store.current_execution_claim_hash(claim).unwrap();
    assert!(completion.proof().matches_claim(claim, &hash));
    completion.proof().check_store(&fixture.store).unwrap();
    if matches!(
        mode,
        "process-recover-stopped-kill" | "process-recover-acked-kill"
    ) {
        drop(store);
        super::stopped_host::kill_after_publication(fixture, mode);
        store = Store::open(&fixture.store).unwrap();
    } else {
        Pipeline
            .stop_native(
                &mut store,
                &mut fsm_store::clock::FixedClock::new(1000, 1),
                claim,
                completion,
                &format!("exec-stop-{effect}-{}", claim.run_id()),
            )
            .unwrap();
    }
    if mode == "process-recover-acked-kill" {
        assert_eq!(store.records.len(), before + 2);
        assert_eq!(store.records[before].kind, RecordKind::ExecutionStopped);
        assert_eq!(store.records[before + 1].kind, RecordKind::ExecutionSettled);
        let records = store.records.clone();
        drop(store);
        let reopened = Store::open(&fixture.store).unwrap();
        assert_eq!(reopened.records, records);
        assert_eq!(reopened.state.execution.unresolved().count(), 0);
        assert_eq!(reopened.state.execution_handoffs.outstanding().count(), 1);
        let handoff = reopened
            .state
            .execution_handoffs
            .outstanding()
            .next()
            .unwrap();
        assert_eq!(handoff.claim(), claim);
        assert_eq!(
            handoff.handler_contract(),
            &completion.handler().contract_value()
        );
        assert!(
            !reopened.state.instances["instance"]
                .pending
                .contains(&effect.to_owned())
        );
        assert_eq!(
            reopened.state.instances["instance"]
                .configuration
                .sequential_leaf(),
            Some("docs_review")
        );
        return (reopened, before);
    }
    assert_eq!(store.records.len(), before + 1);
    assert_eq!(
        store.records.last().unwrap().kind,
        RecordKind::ExecutionStopped
    );
    let records = store.records.clone();
    let state = store.state.clone();
    let head = (store.journal.last_seq, store.journal.last_hash.clone());
    drop(store);

    // The dedicated killed-host case additionally proves process death after
    // the append; other modes retain their writer-reopen boundary.
    let mut reopened = Store::open(&fixture.store).unwrap();
    assert_eq!(reopened.records, records);
    assert!(fsm_store::snapshot::store_states_eq(
        &reopened.state,
        &state
    ));
    assert_eq!(
        (
            reopened.journal.last_seq,
            reopened.journal.last_hash.clone()
        ),
        head
    );
    assert_eq!(
        reopened.state.execution.claim_for("instance", effect),
        Some(claim)
    );
    assert_eq!(
        reopened
            .state
            .execution
            .stopped_for("instance", effect)
            .unwrap()
            .outcome(),
        completion.stopped_outcome()
    );
    assert!(
        reopened.state.instances["instance"]
            .pending
            .iter()
            .any(|id| id == effect)
    );
    let Err(error) = Pipeline.start_native(&mut reopened, claim, Duration::from_secs(1)) else {
        panic!("a durable stopped result authorized another native launch");
    };
    assert_eq!(error.code, "exec/inflight_deferred");
    assert_eq!(reopened.records, records);
    assert_eq!(reopened.current_execution_claim_hash(claim).unwrap(), hash);
    (reopened, before)
}
