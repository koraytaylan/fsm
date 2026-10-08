//! Independent caller of guarded native reconciliation; no native proof implied.

#[cfg(target_os = "linux")]
#[test]
fn reconciliation_refuses_memory_store_without_changing_observed_journal() {
    use fsm_core::json::{JsonLimits, parse};
    use fsm_core::record::execution::Claim;
    use fsm_execute::run::native_client::NativeShutdown;
    use fsm_store::store::Store;
    use std::time::Duration;

    let fixture = parse(
        include_bytes!("../../fsm-core/tests/fixtures/execution-handoff.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let claim = Claim::from_value(fixture.get("claim").unwrap()).unwrap();
    let store = Store::open_memory().unwrap();
    let observed = store.journal.last_seq;
    let result = NativeShutdown::start_reconciliation(&store, &claim, Duration::from_secs(1));
    assert_eq!(
        result.err().unwrap(),
        "native shutdown requires a supported verified durable store"
    );
    assert_eq!(store.journal.last_seq, observed);
    assert!(store.state.execution.unresolved().next().is_none());
}
