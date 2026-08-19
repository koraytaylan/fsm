use std::collections::BTreeMap;

use crate::json::Value;
use crate::machine::InstanceState;
use crate::record::Record;
use crate::step::Rejection;

use super::{ReplayError, state_hash_for_record};

pub(super) fn verify_record_state_hash(
    record: &Record,
    machine_id: &str,
    instance_id: &str,
    state: &InstanceState,
) -> Result<(), ReplayError> {
    let expected = record
        .body
        .get("state_hash")
        .and_then(Value::as_str)
        .ok_or(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "state_hash",
        })?;
    let actual = state_hash_for_record(record, machine_id, instance_id, state).ok_or(
        ReplayError::FieldMismatch {
            seq: record.seq,
            field: "state_format",
        },
    )?;
    if actual != expected {
        return Err(ReplayError::StateHashMismatch {
            seq: record.seq,
            expected: expected.into(),
            found: actual,
        });
    }
    Ok(())
}

pub(super) fn verify_rejection(
    record: &Record,
    rejection: &Rejection,
    expected_details: &BTreeMap<String, Value>,
) -> Result<(), ReplayError> {
    for (field, expected) in [
        ("code", rejection.code),
        ("message", rejection.message.as_str()),
        ("hint", rejection.hint.as_str()),
    ] {
        if record.body.get(field).and_then(Value::as_str) != Some(expected) {
            return Err(ReplayError::FieldMismatch {
                seq: record.seq,
                field,
            });
        }
    }
    if record.body.get("details").and_then(Value::as_obj) != Some(expected_details) {
        return Err(ReplayError::FieldMismatch {
            seq: record.seq,
            field: "details",
        });
    }
    match (record.body.get("span"), rejection.span) {
        (None, None) => {}
        (Some(Value::Obj(span)), Some((start, end))) => {
            if span.get("start").and_then(Value::as_num) != Some(&start.to_string())
                || span.get("end").and_then(Value::as_num) != Some(&end.to_string())
            {
                return Err(ReplayError::FieldMismatch {
                    seq: record.seq,
                    field: "span",
                });
            }
        }
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: record.seq,
                field: "span",
            });
        }
    }
    Ok(())
}

pub(super) fn expected_deadline_rejected_details(
    rejection: &Rejection,
    request_id: Option<&str>,
) -> BTreeMap<String, Value> {
    let mut details = BTreeMap::new();
    if let Some(block) = &rejection.block {
        details.insert("block".into(), Value::Str(block.clone()));
    }
    if let Some(cause) = rejection.cause {
        details.insert("cause".into(), Value::Str(cause.into()));
    }
    if let Some(source_state) = &rejection.source_state {
        details.insert("source_state".into(), Value::Str(source_state.clone()));
    }
    if let Some(transition_idx) = rejection.transition_idx {
        details.insert(
            "transition_idx".into(),
            Value::Num(transition_idx.to_string()),
        );
    }
    details.insert("trace".into(), rejection.trace.to_value());
    if let Some(request_id) = request_id {
        details.insert("request_id".into(), Value::Str(request_id.into()));
    }
    details
}

pub(super) fn expected_event_rejected_details(
    r: &crate::step::Rejection,
    rid: Option<&str>,
    enabled: Value,
) -> BTreeMap<String, Value> {
    let mut d = BTreeMap::new();
    if let Some(b) = &r.block {
        d.insert("block".into(), Value::Str(b.clone()));
    }
    if let Some(c) = r.cause {
        d.insert("cause".into(), Value::Str(c.into()));
    }
    if let Some(s) = &r.source_state {
        d.insert("source_state".into(), Value::Str(s.clone()));
    }
    if let Some(idx) = r.transition_idx {
        d.insert("transition_idx".into(), Value::Num(idx.to_string()));
    }
    d.insert("trace".into(), r.trace.to_value());
    if let Some(rid) = rid {
        d.insert("request_id".into(), Value::Str(rid.into()));
    }
    d.insert("enabled_events".into(), enabled);
    d
}

pub(super) fn expected_request_rejected_details(
    rid: Option<&str>,
    pending: &[String],
) -> BTreeMap<String, Value> {
    let mut d = BTreeMap::new();
    d.insert(
        "pending".into(),
        Value::Arr(pending.iter().cloned().map(Value::Str).collect()),
    );
    if let Some(rid) = rid {
        d.insert("request_id".into(), Value::Str(rid.into()));
    }
    d
}
