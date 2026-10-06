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
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 2);
    let store = Store::open_read_only(&fixture.store).unwrap();
    let mut effects = std::collections::BTreeSet::new();
    for allocation in [1, 2] {
        let binding = read_value(
            &fixture.directory.join(format!("binding-{allocation}.json")),
            true,
        )
        .unwrap();
        let claim =
            fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
        assert_eq!(
            number(&claim.domain().to_value(), "allocation").unwrap(),
            allocation
        );
        assert!(effects.insert(claim.effect().1.to_owned()));
        let domain = claim.domain().to_value();
        fixture.groups.push((
            cgroup(&origin(&fixture.directory).unwrap(), allocation).unwrap(),
            domain.get("cgroup").unwrap().clone(),
        ));
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
    }
    drop(store);
    fixture.cleanup().unwrap();
}

pub(super) fn run(fixture: &mut Fixture, binding: &Value, effect: &str, successor: &NativeDomain) {
    run_kind(fixture, binding, effect, successor, false, false, false);
}

pub(super) fn cold(fixture: &mut Fixture, binding: &Value, effect: &str, successor: &NativeDomain) {
    run_kind(fixture, binding, effect, successor, true, false, false);
}

pub(super) fn conflict(
    fixture: &mut Fixture,
    binding: &Value,
    effect: &str,
    successor: &NativeDomain,
) {
    run_kind(fixture, binding, effect, successor, true, true, false);
}

pub(super) fn rejected(
    fixture: &mut Fixture,
    binding: &Value,
    effect: &str,
    successor: &NativeDomain,
) {
    run_kind(fixture, binding, effect, successor, true, false, true);
}

fn run_kind(
    fixture: &mut Fixture,
    binding: &Value,
    effect: &str,
    successor: &NativeDomain,
    cold: bool,
    conflicting: bool,
    rejected: bool,
) {
    if rejected {
        disconnect_cases::rejected_handoff(&fixture.directory, binding, successor);
    } else if conflicting {
        disconnect_cases::conflicting_handoff(&fixture.directory, binding, successor);
    } else if cold {
        disconnect_cases::cold_handoff(&fixture.directory, binding, successor);
    } else {
        disconnect_cases::fresh_handoff(&fixture.directory, binding, successor);
    }
    assert_eq!(
        read_value(&fixture.directory.join("binding-1.json"), true).unwrap(),
        *binding
    );
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 2);
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
    if conflicting || rejected {
        assert_eq!(store.state.execution_handoffs.outstanding().count(), 1);
    } else if cold {
        assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
    }
    drop(store);
    disconnect_cases::discard_prepared(&fixture.directory, &successor.to_value());
    fixture.cleanup().unwrap();
}

pub(super) fn competition(fixture: &mut Fixture, table: &Value) {
    let competitor = fixture.prepare();
    assert_eq!(number(&competitor, "allocation").unwrap(), 1);
    disconnect_cases::permit_operator_store(&fixture.store);
    disconnect_cases::competing_admission(&fixture.directory, table, &competitor);
    assert_eq!(number(&fixture.counter(), "last_allocation").unwrap(), 2);
    for allocation in [1, 2] {
        for name in ["binding", "launch", "entry", "handoff"] {
            assert_eq!(
                fs::symlink_metadata(fixture.directory.join(format!("{name}-{allocation}.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
    }
    let closed = read_value(&fixture.directory.join("closed-2.json"), true).unwrap();
    assert_eq!(
        closed.get("format").and_then(Value::as_str),
        Some("fsm.native-domain-closed/1")
    );
    let loser = NativeDomain::from_value(closed.get("domain").unwrap()).unwrap();
    assert_eq!(number(&loser.to_value(), "allocation").unwrap(), 2);
    assert!(
        !cgroup(&origin(&fixture.directory).unwrap(), 2)
            .unwrap()
            .exists()
    );
    let store = Store::open_read_only(&fixture.store).unwrap();
    let claims: Vec<_> = store.state.execution.unresolved().collect();
    assert_eq!(claims.len(), 1);
    assert_eq!(claims[0].0.domain().to_value(), competitor);
    assert!(
        store.state.instances["instance"]
            .pending
            .contains(&claims[0].0.effect().1.to_owned())
    );
    assert!(
        !store
            .state
            .dedup
            .contains_key(&fsm_execute::rid::ack_rid(claims[0].0.effect().1))
    );
    drop(store);
    // Fixture teardown is not a journal ownership settlement.
    disconnect_cases::discard_prepared(&fixture.directory, &competitor);
    fixture.cleanup().unwrap();
}
