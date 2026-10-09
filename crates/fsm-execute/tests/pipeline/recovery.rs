//! Acknowledged outcome recovery preserves event keys across receiver migration.

use super::*;

fn acknowledged_confirmation(test_name: &str) -> (Fixture, PendingEffect) {
    let mut fixture = Fixture::awaiting_confirmation(test_name);
    let effect = fixture.pending_effect(0);
    fixture
        .store
        .ack_effect_outcome_on(
            &mut fixture.clock,
            "order-1",
            &effect.effect_id,
            &ack_rid(&effect.effect_id),
            "ok",
            None,
        )
        .unwrap();
    (fixture, effect)
}

fn migrate_outcome_receiver(fixture: &mut Fixture, name: &str, ty: Value) {
    let Value::Obj(mut document) = machine() else {
        unreachable!()
    };
    document.insert("name".into(), Value::Str(name.into()));
    let digest =
        fsm_core::hashes::digest_of(&fixture.store.state.instance_machines["order-1"]).unwrap();
    document.insert("supersedes".into(), parse(format!(
        r#"{{"machine":"{digest}","states":{{"awaiting_confirmation":"awaiting_confirmation"}},"context":{{"order_id":"ctx.order_id","approved":"ctx.approved"}}}}"#
    ).as_bytes(), &JsonLimits::DEFAULT).unwrap());
    if matches!(ty, Value::Obj(_)) {
        document.insert(
            "enums".into(),
            parse(br#"{"StampRange":["0","2000"]}"#, &JsonLimits::DEFAULT).unwrap(),
        );
    }
    let Value::Arr(events) = document.get_mut("events").unwrap() else {
        unreachable!()
    };
    let Value::Obj(event) = events
        .iter_mut()
        .find(|event| event.get("name").and_then(Value::as_str) == Some("confirmed"))
        .unwrap()
    else {
        unreachable!()
    };
    let Value::Arr(fields) = event.get_mut("fields").unwrap() else {
        unreachable!()
    };
    let Value::Obj(field) = &mut fields[0] else {
        unreachable!()
    };
    field.insert("ty".into(), ty);
    fixture
        .store
        .define_machine_on(&mut fixture.clock, Value::Obj(document), false, false)
        .unwrap();
    fixture
        .store
        .migrate_instance_on(&mut fixture.clock, "order-1", name, name)
        .unwrap();
}

fn refused_recovery_preserves_key_then_repairs(ty: Value, code: &str, test_name: &str) {
    let (mut fixture, effect) = acknowledged_confirmation(test_name);
    migrate_outcome_receiver(&mut fixture, "incompatible_receiver", ty);
    fixture.clock.now = 2000;
    let advance = handler("request_confirmation").on_ok.unwrap();
    let state = fixture.store.state.clone();
    let records = fixture.store.records.clone();
    let request_id = event_rid(&effect.effect_id, &advance.event);
    let result = Pipeline.advance_only(
        &mut fixture.store,
        &mut fixture.clock,
        &effect.effect_id,
        "order-1",
        &advance,
    );
    assert!(
        !fixture.store.state.dedup.contains_key(&request_id),
        "refused recovery consumed the derived event key"
    );
    let error = result.unwrap_err();
    assert_eq!(error.code, code);
    assert!(fsm_store::snapshot::store_states_eq(
        &state,
        &fixture.store.state
    ));
    assert_eq!(fixture.store.records, records);
    assert_eq!(fixture.records_of_kind(RecordKind::EffectAcked).len(), 1);
    migrate_outcome_receiver(
        &mut fixture,
        "repaired_receiver",
        Value::Str("timestamp".into()),
    );
    assert_eq!(
        Pipeline
            .advance_only(
                &mut fixture.store,
                &mut fixture.clock,
                &effect.effect_id,
                "order-1",
                &advance
            )
            .unwrap(),
        SettleOutcome::Advanced
    );
    let records = fixture.store.records.clone();
    assert_eq!(
        Pipeline
            .advance_only(
                &mut fixture.store,
                &mut fixture.clock,
                &effect.effect_id,
                "order-1",
                &advance
            )
            .unwrap(),
        SettleOutcome::AckedNoAdvance
    );
    assert_eq!(fixture.store.records, records);
    let reopened = Store::open_read_only(&fixture.store.data_dir).unwrap();
    assert!(fsm_store::snapshot::store_states_eq(
        &fixture.store.state,
        &reopened.state
    ));
    assert_eq!(fixture.records_of_kind(RecordKind::EffectAcked).len(), 1);
}

#[test]
fn acknowledged_recovery_refuses_invalid_migrated_payload_without_consuming_event_key() {
    refused_recovery_preserves_key_then_repairs(
        Value::Str("bool".into()),
        "exec/contract_invalid",
        "pipe-recovery-invalid-receiver",
    );
}

#[test]
fn acknowledged_recovery_refuses_unknown_stamp_family_without_consuming_event_key() {
    refused_recovery_preserves_key_then_repairs(
        parse(br#"{"enum":"StampRange"}"#, &JsonLimits::DEFAULT).unwrap(),
        "exec/contract_unknown",
        "pipe-recovery-unknown-stamps",
    );
}

#[test]
fn cancelled_acknowledged_recovery_suppresses_invalid_payload_without_writing() {
    let (mut fixture, effect) = acknowledged_confirmation("pipe-recovery-cancelled");
    fixture
        .store
        .cancel_instance_reason_on(
            &mut fixture.clock,
            "order-1",
            "cancel",
            "operator stopped it",
        )
        .unwrap();
    let mut advance = handler("request_confirmation").on_ok.unwrap();
    advance.payload = Value::Obj(BTreeMap::from([("at".into(), Value::Bool(true))]));
    let state = fixture.store.state.clone();
    let records = fixture.store.records.clone();
    assert_eq!(
        Pipeline
            .advance_only(
                &mut fixture.store,
                &mut fixture.clock,
                &effect.effect_id,
                "order-1",
                &advance
            )
            .unwrap(),
        SettleOutcome::AckedNoAdvance
    );
    assert!(fsm_store::snapshot::store_states_eq(
        &state,
        &fixture.store.state
    ));
    assert_eq!(fixture.store.records, records);
}
