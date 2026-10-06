//! Independent bounded handoff fixtures; synthetic records supply no native authority.

use fsm_core::hashes::request_fp;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{AcknowledgedHandoff, HandoffState};
use fsm_core::record::{RecordKind, seal, zeros};
use std::collections::BTreeMap;

fn handoff() -> AcknowledgedHandoff {
    AcknowledgedHandoff::from_value(
        &parse(
            include_bytes!("fixtures/execution-handoff.json"),
            &JsonLimits::DEFAULT,
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn collection_roundtrip_preserves_original_material_and_refuses_duplicates() {
    let original = handoff();
    let mut state = HandoffState::default();
    state.install(original.clone()).unwrap();
    let encoded = state.to_value();
    assert_eq!(HandoffState::from_value(&encoded).unwrap(), state);
    assert!(state.install(original).is_err());
    assert_eq!(state.to_value(), encoded);
    let Value::Arr(mut duplicate) = encoded else {
        panic!("ordered collection");
    };
    duplicate.push(duplicate[0].clone());
    assert!(HandoffState::from_value(&Value::Arr(duplicate)).is_err());
}

#[test]
fn only_exact_accepted_event_retires_the_original_obligation() {
    let original = handoff();
    let payload = Value::Obj(BTreeMap::new());
    let body = BTreeMap::from([
        (
            "instance_id".into(),
            Value::Str(original.claim().effect().0.into()),
        ),
        ("event".into(), Value::Str("docs_ok".into())),
        ("payload".into(), payload.clone()),
        (
            "request_id".into(),
            Value::Str(original.event_request_id().into()),
        ),
        (
            "request_fp".into(),
            Value::Str(request_fp(
                "send",
                &BTreeMap::from([
                    (
                        "instance_id".into(),
                        Value::Str(original.claim().effect().0.into()),
                    ),
                    ("event".into(), Value::Str("docs_ok".into())),
                    ("payload".into(), payload),
                ]),
            )),
        ),
    ]);
    for (key, value) in [
        ("instance_id", Value::Str("foreign".into())),
        ("request_id", Value::Str("foreign".into())),
        ("event", Value::Str("note_added".into())),
        (
            "request_fp",
            Value::Str(format!("sha256:{}", "a".repeat(64))),
        ),
        (
            "payload",
            Value::Obj(BTreeMap::from([("changed".into(), Value::Bool(true))])),
        ),
    ] {
        let mut state = HandoffState::default();
        state.install(original.clone()).unwrap();
        let mut changed = body.clone();
        changed.insert(key.into(), value);
        state.accept_event_record(&seal(
            7,
            100,
            RecordKind::EventApplied,
            Value::Obj(changed),
            &zeros(),
        ));
        assert_eq!(state.outstanding().count(), 1, "foreign {key}");
    }
    for kind in [RecordKind::EventRejected, RecordKind::RequestRejected] {
        let mut state = HandoffState::default();
        state.install(original.clone()).unwrap();
        state.accept_event_record(&seal(7, 100, kind, Value::Obj(body.clone()), &zeros()));
        assert_eq!(state.outstanding().count(), 1);
    }
    let mut state = HandoffState::default();
    state.install(original).unwrap();
    state.accept_event_record(&seal(
        7,
        100,
        RecordKind::EventApplied,
        Value::Obj(body),
        &zeros(),
    ));
    assert_eq!(state.outstanding().count(), 0);
}

#[test]
fn collection_rejects_entry_and_aggregate_byte_overflow_before_installation() {
    let original = handoff().to_value();
    let mut entries = Vec::new();
    for run in 1..=4097 {
        let mut value = original.as_obj().unwrap().clone();
        let mut claim = value["claim"].as_obj().unwrap().clone();
        let effect = format!("instance/{run}/0");
        claim.insert("run_id".into(), Value::Num(run.to_string()));
        claim.insert("effect_id".into(), Value::Str(effect.clone()));
        value.insert("claim".into(), Value::Obj(claim));
        value.insert(
            "acknowledgement_request_id".into(),
            Value::Str(format!("exec-ack-{effect}")),
        );
        value.insert(
            "event_request_id".into(),
            Value::Str(format!("exec-ev-{effect}-docs_ok")),
        );
        entries.push(Value::Obj(value));
    }
    assert!(HandoffState::from_value(&Value::Arr(entries)).is_err());
    let excessive = Value::Arr(vec![Value::Str("x".repeat(8 * 1024 * 1024))]);
    assert_eq!(HandoffState::from_value(&excessive).unwrap_err().0, "bytes");
}
