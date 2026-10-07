//! Reopening a genuine stopped run retains exclusion until durable disposition.

use super::*;

pub(super) fn reopen_stopped(
    fixture: &Fixture,
    effect: &str,
    claim: &fsm_core::record::execution::Claim,
    completion: &fsm_execute::run::native_client::NativeCompletion,
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
    Pipeline
        .stop_native(
            &mut store,
            &mut fsm_store::clock::FixedClock::new(1000, 1),
            claim,
            completion,
            &format!("exec-stop-{effect}-{}", claim.run_id()),
        )
        .unwrap();
    assert_eq!(store.records.len(), before + 1);
    assert_eq!(
        store.records.last().unwrap().kind,
        RecordKind::ExecutionStopped
    );
    let records = store.records.clone();
    let state = store.state.clone();
    let head = (store.journal.last_seq, store.journal.last_hash.clone());
    drop(store);

    // This is a writer-reopen boundary, not an executor-kill assertion or a
    // cache-cold replay claim; the privileged fixture supplied real closure.
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
