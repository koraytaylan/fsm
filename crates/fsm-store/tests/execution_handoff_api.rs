//! External store API refuses caller-owned handoff material without actual ownership.

#[test]
fn standalone_handoff_candidate_cannot_publish_an_acknowledgement() {
    use fsm_core::record::execution::{AcknowledgedHandoff, Settlement};
    use fsm_store::clock::FixedClock;
    use fsm_store::store::{ExecutionSettleRequest, Store};
    let value = fsm_core::json::parse(
        include_bytes!("../../fsm-core/tests/fixtures/execution-handoff.json"),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap();
    let candidate = AcknowledgedHandoff::from_value(&value).unwrap();
    let mut store = Store::open_memory().unwrap();
    let head = store.journal.last_seq;
    let hash = store.journal.last_hash.clone();
    let records = store.records.len();
    let error = store
        .settle_execution_with_handoff_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim: candidate.claim(),
                disposition: Settlement::Acked,
                request_id: candidate.acknowledgement_request_id(),
                expected_seq: None,
            },
            &candidate,
        )
        .unwrap_err();
    assert_eq!(error.code, "store/execution_owned");
    assert_eq!(store.journal.last_seq, head);
    assert_eq!(store.journal.last_hash, hash);
    assert_eq!(store.records.len(), records);
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
    assert!(
        !store
            .state
            .dedup
            .contains_key(candidate.acknowledgement_request_id())
    );
}
