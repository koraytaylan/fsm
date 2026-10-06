//! Root-side verification and retirement for the fresh Runner fixture axis.

use super::*;

pub(super) fn run(fixture: &mut Fixture, binding: &Value, effect: &str, successor: &NativeDomain) {
    disconnect_cases::fresh_handoff(&fixture.directory, binding, successor);
    assert_eq!(
        read_value(&fixture.directory.join("binding-1.json"), true).unwrap(),
        *binding
    );
    let store = Store::open_read_only(&fixture.store).unwrap();
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
            .dedup
            .contains_key(&fsm_execute::rid::ack_rid(effect))
    );
    drop(store);
    disconnect_cases::discard_prepared(&fixture.directory, &successor.to_value());
    fixture.cleanup().unwrap();
}
