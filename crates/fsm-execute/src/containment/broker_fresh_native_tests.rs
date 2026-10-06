//! Root-side verification and retirement for the fresh Runner fixture axis.

use super::*;

pub(super) fn admission(fixture: &mut Fixture, table: &Value) {
    disconnect_cases::permit_operator_store(&fixture.store);
    disconnect_cases::fresh_admission(&fixture.directory, table);
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 1);
    let binding = read_value(&fixture.directory.join("binding-1.json"), true).unwrap();
    let claim =
        fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let domain = claim.domain().to_value();
    fixture.groups.push((
        cgroup(&origin(&fixture.directory).unwrap(), 1).unwrap(),
        domain.get("cgroup").unwrap().clone(),
    ));
    let store = Store::open_read_only(&fixture.store).unwrap();
    assert!(
        store
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    assert!(
        store
            .state
            .dedup
            .contains_key(&fsm_execute::rid::ack_rid(claim.effect().1))
    );
    assert!(
        store
            .state
            .dedup
            .contains_key(&fsm_execute::rid::event_rid(claim.effect().1, "docs_ok"))
    );
    drop(store);
    fixture.cleanup().unwrap();
}

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
