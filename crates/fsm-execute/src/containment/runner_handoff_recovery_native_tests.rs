//! Genuine acknowledgement recovery uses the original event contract.

use super::*;

pub(super) fn resume_original_event(
    fixture: &Fixture,
    effect: &str,
    completion: &fsm_execute::run::native_client::NativeCompletion,
    changed: bool,
) {
    use fsm_core::record::RecordKind;
    use fsm_execute::{
        config::HandlerTable,
        rid::event_rid,
        service::{ExecutorPhase, PairedNativeExecutor, ShutdownMode},
    };
    use fsm_store::clock::FixedClock;

    let snapshot = Store::open_read_only(&fixture.store).unwrap();
    let before = snapshot.records.len();
    assert_eq!(snapshot.state.execution.unresolved().count(), 0);
    let original = snapshot
        .state
        .execution_handoffs
        .outstanding()
        .next()
        .unwrap();
    assert_eq!(
        original.handler_contract(),
        &completion.handler().contract_value()
    );
    assert_eq!(original.event_request_id(), event_rid(effect, "docs_ok"));
    assert_eq!(
        snapshot.state.instances["instance"]
            .configuration
            .sequential_leaf(),
        Some("docs_review")
    );
    drop(snapshot);
    let counter = fixture.counter();

    let mut table = HandlerTable::default();
    if changed {
        let mut handler = completion.handler().clone();
        handler.argv = vec!["/fixture-handler-must-not-run".into()];
        handler.on_ok.as_mut().unwrap().event = "withdraw".into();
        assert_ne!(handler.fingerprint(), completion.handler().fingerprint());
        table.handlers.insert(handler.effect.clone(), handler);
    }
    // Reconstruct the real standalone paired driver from durable state, with
    // no retained completion object supplied to it and no original table.
    let mut driver = PairedNativeExecutor::new(&fixture.store, table).unwrap();
    let mut clock = FixedClock::new(2000, 1);
    let lines = driver.tick(&mut clock, 2000);
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("native-handoff advanced")),
        "{lines:?}"
    );
    let settled = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(settled.records.len(), before + 1);
    assert_eq!(
        settled.records.last().unwrap().kind,
        RecordKind::EventApplied
    );
    assert_eq!(
        settled.state.instances["instance"]
            .configuration
            .sequential_leaf(),
        Some("risk_review")
    );
    assert!(
        settled
            .state
            .dedup
            .contains_key(&event_rid(effect, "docs_ok"))
    );
    assert!(
        !settled
            .state
            .dedup
            .contains_key(&event_rid(effect, "withdraw"))
    );
    assert_eq!(settled.state.execution_handoffs.outstanding().count(), 0);
    assert_eq!(fixture.counter(), counter);
    let records = settled.records.clone();
    drop(settled);
    driver.tick(&mut clock, 2001);
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    let request = driver.control().stop(ShutdownMode::Drain, 1000).unwrap();
    driver.poll(&mut clock, 2002);
    let report = request.poll();
    assert_eq!(report.phase, ExecutorPhase::Stopped);
    assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
}
