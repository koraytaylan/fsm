//! Independent prospective handoff values and original-contract refusal cases.

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{AcknowledgedHandoff, ShapeError};

fn candidate() -> Value {
    // Fingerprint derived independently with Python's stdlib SHA-256 over
    // SPEC's handler-contract domain, newline and canonical literal contract.
    parse(
        include_bytes!("fixtures/execution-handoff.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn set(value: &mut Value, field: &str, replacement: Value) {
    let Value::Obj(fields) = value else {
        panic!("literal fixture must be an object");
    };
    fields.insert(field.into(), replacement);
}

#[test]
fn original_material_has_closed_canonical_encoding_and_borrowed_identity() {
    let value = candidate();
    let handoff = AcknowledgedHandoff::from_value(&value).unwrap();
    assert_eq!(canon_bytes(&handoff.to_value()), canon_bytes(&value));
    assert_eq!(handoff.claim().effect(), ("instance", "instance/3/0"));
    assert_eq!(handoff.claim().run_id(), 1);
    assert_eq!(handoff.acknowledgement_seq(), 6);
    assert_eq!(
        handoff.acknowledgement_request_id(),
        "exec-ack-instance/3/0"
    );
    assert_eq!(handoff.event_request_id(), "exec-ev-instance/3/0-docs_ok");
    assert_eq!(
        handoff.handler_contract(),
        value.get("handler_contract").unwrap()
    );
    assert_eq!(handoff.outcome().status(), "ok");
    assert_eq!(handoff.outcome().result(), Some(&Value::Null));
    assert_eq!(
        handoff.original_claim_hash(),
        format!("sha256:{}", "b".repeat(64))
    );
}

#[test]
fn terminal_failure_uses_original_failed_event_without_erasing_failure_class() {
    let mut value = candidate();
    let mut outcome = value.get("outcome").unwrap().clone();
    set(&mut outcome, "status", Value::Str("timeout".into()));
    set(&mut value, "outcome", outcome);
    assert_eq!(
        AcknowledgedHandoff::from_value(&value).unwrap_err(),
        ShapeError("handoff_request")
    );
    set(
        &mut value,
        "event_request_id",
        Value::Str("exec-ev-instance/3/0-note_added".into()),
    );
    let handoff = AcknowledgedHandoff::from_value(&value).unwrap();
    assert_eq!(handoff.outcome().status(), "timeout");
    assert_eq!(handoff.to_value(), value);
    let mut interrupted = handoff.outcome().to_value();
    set(&mut interrupted, "status", Value::Str("interrupted".into()));
    set(&mut value, "outcome", interrupted);
    assert_eq!(
        AcknowledgedHandoff::from_value(&value).unwrap_err(),
        ShapeError("handoff_outcome")
    );
}

#[test]
fn omitted_result_remains_distinct_from_explicit_null() {
    let mut value = candidate();
    let Value::Obj(outcome) = value.get("outcome").unwrap().clone() else {
        panic!("literal outcome must be an object");
    };
    let mut outcome = outcome;
    outcome.remove("result");
    set(&mut value, "outcome", Value::Obj(outcome));
    let handoff = AcknowledgedHandoff::from_value(&value).unwrap();
    assert_eq!(handoff.outcome().result(), None);
    assert_eq!(handoff.to_value(), value);
}

#[test]
fn changed_contract_and_inconsistent_original_retry_are_refused() {
    let mut value = candidate();
    let mut contract = value.get("handler_contract").unwrap().clone();
    set(
        &mut contract,
        "argv",
        Value::Arr(vec![Value::Str("/bin/false".into())]),
    );
    set(&mut value, "handler_contract", contract);
    assert_eq!(
        AcknowledgedHandoff::from_value(&value).unwrap_err(),
        ShapeError("handoff_contract")
    );

    let mut value = candidate();
    let mut claim = value.get("claim").unwrap().clone();
    let mut retry = claim.get("retry").unwrap().clone();
    set(&mut retry, "attempts", Value::Num("4".into()));
    set(&mut claim, "retry", retry);
    set(&mut value, "claim", claim);
    assert_eq!(
        AcknowledgedHandoff::from_value(&value).unwrap_err(),
        ShapeError("handoff_contract")
    );
}

#[test]
fn foreign_request_keys_zero_sequence_unknown_fields_and_bad_hash_are_refused() {
    for (field, replacement) in [
        (
            "acknowledgement_request_id",
            Value::Str("exec-ack-other/3/0".into()),
        ),
        (
            "event_request_id",
            Value::Str("exec-ev-other/3/0-docs_ok".into()),
        ),
        ("acknowledgement_seq", Value::Num("0".into())),
        (
            "original_claim_hash",
            Value::Str(format!("sha256:{}", "B".repeat(64))),
        ),
        ("format", Value::Str("fsm.execution-handoff/2".into())),
        ("unknown", Value::Null),
    ] {
        let mut value = candidate();
        set(&mut value, field, replacement);
        assert!(AcknowledgedHandoff::from_value(&value).is_err(), "{field}");
    }
}

#[test]
fn oversized_candidate_and_outcome_are_refused_before_copying() {
    for length in [64 * 1024, 128 * 1024] {
        let mut value = candidate();
        let mut outcome = value.get("outcome").unwrap().clone();
        set(&mut outcome, "result", Value::Str("x".repeat(length)));
        set(&mut value, "outcome", outcome);
        assert_eq!(
            AcknowledgedHandoff::from_value(&value).unwrap_err(),
            ShapeError("bytes")
        );
    }
}
