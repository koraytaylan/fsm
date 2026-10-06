//! Historical snapshot/6 is a disposable cache, never reinterpreted as /7.

#[test]
fn historical_snapshot_v6_is_skipped() {
    let value = fsm_core::json::parse(
        include_bytes!("fixtures/snapshot_v6_parallel.json"),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap();
    assert!(fsm_store::snapshot::snapshot_to_state(&value).is_err());
}
