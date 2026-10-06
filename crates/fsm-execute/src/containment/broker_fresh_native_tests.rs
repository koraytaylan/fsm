//! Root-side verification and retirement for the fresh Runner fixture axis.

use super::*;

pub(super) fn cancellation(fixture: &mut Fixture, table: &Value) {
    disconnect_cases::permit_operator_store(&fixture.store);
    disconnect_cases::cancel_admission(&fixture.directory, table);
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 1);
    for name in ["binding", "launch", "entry", "handoff"] {
        assert_eq!(
            fs::symlink_metadata(fixture.directory.join(format!("{name}-1.json")))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
    }
    let closed = read_value(&fixture.directory.join("closed-1.json"), true).unwrap();
    assert_eq!(
        closed.get("format").and_then(Value::as_str),
        Some("fsm.native-domain-closed/1")
    );
    let domain = NativeDomain::from_value(closed.get("domain").unwrap()).unwrap();
    assert_eq!(number(&domain.to_value(), "allocation").unwrap(), 1);
    assert!(
        !cgroup(&origin(&fixture.directory).unwrap(), 1)
            .unwrap()
            .exists()
    );
    fixture.cleanup().unwrap();
}

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
