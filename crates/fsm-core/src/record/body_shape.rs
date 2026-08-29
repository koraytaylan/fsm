//! Which body fields each record kind requires.
//!
//! Split out of `record.rs` because it is a second subject: that module models
//! the record envelope — its kinds, its canonical bytes, its hash chain — and
//! this one answers a different question about the same bytes, "does the body
//! of a `deadline_not_due` carry what a `deadline_not_due` must carry". The
//! two change for different reasons and grow at different rates.
//!
//! Bodies are **not** closed. Every check here validates the fields a kind
//! requires, never the absence of others, which is what lets
//! `fsm-store`'s commit path inject a `state_root` into whatever record lands
//! on a 10 000th sequence without that record's kind knowing about it.

use crate::json::Value;

use super::{RecordKind, legacy_limits_value, limits_value};

fn is_hex64(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn is_state_hash(v: Option<&Value>) -> bool {
    v.and_then(Value::as_str)
        .and_then(|s| s.strip_prefix("sha256:"))
        .is_some_and(is_hex64)
}

fn req_str(body: &Value, k: &str) -> bool {
    body.get(k).and_then(Value::as_str).is_some()
}

fn req_i64(body: &Value, k: &str) -> bool {
    body.get(k)
        .and_then(Value::as_num)
        .is_some_and(|raw| raw.parse::<i64>().is_ok())
}

fn req_u32(body: &Value, k: &str) -> bool {
    body.get(k)
        .and_then(Value::as_num)
        .is_some_and(|raw| raw.parse::<u32>().is_ok())
}

fn req_str_arr(body: &Value, k: &str) -> bool {
    body.get(k)
        .and_then(Value::as_arr)
        .is_some_and(|values| values.iter().all(|value| value.as_str().is_some()))
}

/// A journaled `microsteps` array, if present: entries indexed from 1 in
/// order, each `eventless` or `internal` (the latter naming its `event`),
/// with the trigger's fields. An empty array is malformed by the absence
/// rule above.
fn microsteps_ok(body: &Value) -> bool {
    let Some(microsteps) = body.get("microsteps") else {
        return true;
    };
    let Some(entries) = microsteps.as_arr() else {
        return false;
    };
    if entries.is_empty() {
        return false;
    }
    entries.iter().enumerate().all(|(position, entry)| {
        let index_ok = entry
            .get("index")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse::<u32>().ok())
            == Some(position as u32 + 1);
        let trigger_ok = match entry.get("trigger").and_then(Value::as_str) {
            Some("eventless") => entry.get("event").is_none(),
            Some("internal") => req_str(entry, "event"),
            _ => false,
        };
        index_ok
            && trigger_ok
            && req_str(entry, "source_state")
            && req_u32(entry, "transition_idx")
            && req_str_arr(entry, "exited")
            && req_str_arr(entry, "entered")
    })
}

fn genesis_limits_ok(value: Option<&Value>) -> bool {
    value.is_some_and(|value| value == &limits_value() || value == &legacy_limits_value())
}

/// A record may declare any state format this build can verify: the current
/// one, or the v2 that predates the composition fields. An absent field is
/// the historical v1 and is only allowed where the field is optional.
fn state_format_ok(body: &Value, required: bool) -> bool {
    match body.get("state_format") {
        Some(Value::Str(format)) => {
            format == crate::hashes::STATE_FORMAT || format == crate::hashes::STATE_FORMAT_V2
        }
        None => !required,
        Some(_) => false,
    }
}

fn root_format_ok(body: &Value) -> bool {
    match body.get("state_root_format") {
        Some(Value::Str(format)) => {
            format == "fsm.state-root/3" && is_state_hash(body.get("state_root"))
        }
        None => true,
        Some(_) => false,
    }
}

fn deadline_identity_ok(body: &Value) -> bool {
    req_str(body, "deadline") && req_u32(body, "deadline_idx") && req_i64(body, "due_ms")
}

fn span_ok(v: Option<&Value>) -> bool {
    match v {
        None => true,
        Some(Value::Obj(o)) => {
            o.get("start")
                .and_then(Value::as_num)
                .is_some_and(|raw| raw.parse::<u32>().is_ok())
                && o.get("end")
                    .and_then(Value::as_num)
                    .is_some_and(|raw| raw.parse::<u32>().is_ok())
        }
        Some(_) => false,
    }
}

fn rejection_ok(body: &Value) -> bool {
    req_str(body, "code")
        && req_str(body, "message")
        && req_str(body, "hint")
        && body.get("details").and_then(Value::as_obj).is_some()
        && span_ok(body.get("span"))
}

pub(super) fn body_ok(kind: RecordKind, body: &Value) -> bool {
    let shape_ok = match kind {
        RecordKind::Genesis => {
            body.get("format").and_then(Value::as_str) == Some("fsm.journal/1")
                && genesis_limits_ok(body.get("limits"))
                && body.get("created_ts").and_then(Value::as_num).is_some()
        }
        RecordKind::MachineDefined => req_str(body, "machine_id") && body.get("def").is_some(),
        RecordKind::InstanceCreated => {
            req_str(body, "instance_id")
                && req_str(body, "machine_id")
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && body.get("overrides").and_then(Value::as_obj).is_some()
                && match body.get("state_format") {
                    Some(_) => body.get("configuration").and_then(Value::as_obj).is_some(),
                    None => req_str(body, "leaf"),
                }
                && microsteps_ok(body)
        }
        RecordKind::EventApplied => {
            req_str(body, "instance_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && req_str_arr(body, "exited")
                && req_str_arr(body, "entered")
                && req_str(body, "source_state")
                && microsteps_ok(body)
        }
        RecordKind::EventRejected => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && is_state_hash(body.get("state_hash"))
                && rejection_ok(body)
        }
        RecordKind::EventIgnored => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "event")
                && body.get("payload").is_some()
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::DeadlineApplied => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && deadline_identity_ok(body)
                && is_state_hash(body.get("state_hash"))
                && req_str_arr(body, "exited")
                && req_str_arr(body, "entered")
                && req_str(body, "source_state")
                && microsteps_ok(body)
        }
        RecordKind::DeadlineRejected => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && deadline_identity_ok(body)
                && is_state_hash(body.get("state_hash"))
                && rejection_ok(body)
        }
        RecordKind::DeadlineNotDue => {
            let next_ok = match (
                body.get("next_deadline"),
                body.get("next_deadline_idx"),
                body.get("next_due_ms"),
            ) {
                (None, None, None) => true,
                (Some(Value::Str(_)), Some(idx), Some(due)) => {
                    idx.as_num().is_some_and(|raw| raw.parse::<u32>().is_ok())
                        && due.as_num().is_some_and(|raw| raw.parse::<i64>().is_ok())
                }
                _ => false,
            };
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && next_ok
        }
        RecordKind::EffectAcked => {
            req_str(body, "instance_id")
                && req_str(body, "effect_id")
                && req_str(body, "request_id")
                && matches!(
                    body.get("outcome").and_then(Value::as_str),
                    Some("ok") | Some("failed")
                )
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::RequestRejected => {
            req_str(body, "request_id")
                && req_str(body, "instance_id")
                && rejection_ok(body)
                && req_str(body, "operation")
                && is_state_hash(body.get("state_hash"))
                && (body.get("operation").and_then(Value::as_str) != Some("ack")
                    || req_str(body, "effect_id"))
        }
        RecordKind::InstanceCancelled => {
            req_str(body, "instance_id")
                && req_str(body, "request_id")
                && req_str(body, "reason")
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::InstanceMigrated => {
            req_str(body, "instance_id")
                && req_str(body, "from_machine_id")
                && req_str(body, "to_machine_id")
                && req_str(body, "request_id")
                && body.get("configuration_before").is_some()
                && body.get("configuration_after").is_some()
                && body
                    .get("dropped_history")
                    .and_then(Value::as_arr)
                    .is_some()
                && body
                    .get("rescheduled_deadlines")
                    .and_then(Value::as_arr)
                    .is_some()
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::SignalDelivered => {
            req_str(body, "sender_instance_id")
                && req_str(body, "signal_id")
                && req_str(body, "target_instance_id")
                && req_str(body, "event")
                && req_str(body, "request_id")
                && req_str(body, "outcome")
                && body.get("payload").and_then(Value::as_obj).is_some()
                && is_state_hash(body.get("sender_state_hash"))
                && body
                    .get("target_state_hash")
                    .is_none_or(|hash| is_state_hash(Some(hash)))
        }
        RecordKind::InvocationReturned => {
            req_str(body, "parent_instance_id")
                && req_str(body, "slot")
                && req_str(body, "child_instance_id")
                && req_str(body, "request_id")
                && matches!(
                    body.get("outcome").and_then(Value::as_str),
                    Some("completed") | Some("cancelled")
                )
                && body.get("payload").and_then(Value::as_obj).is_some()
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::InstanceInvoked => {
            req_str(body, "parent_instance_id")
                && req_str(body, "slot")
                && req_str(body, "child_instance_id")
                && req_str(body, "child_machine_id")
                && req_str(body, "request_id")
                && is_state_hash(body.get("state_hash"))
                && is_state_hash(body.get("child_state_hash"))
                && body.get("overrides").and_then(Value::as_obj).is_some()
        }
        RecordKind::Annotated => {
            req_str(body, "instance_id") && req_str(body, "request_id") && req_str(body, "note")
        }
        RecordKind::EffectAttempted => {
            req_str(body, "instance_id")
                && req_str(body, "effect_id")
                && req_str(body, "request_id")
                // Always `failed`: a successful attempt is an ack.
                && body.get("outcome").and_then(Value::as_str) == Some("failed")
                && body
                    .get("attempt")
                    .and_then(Value::as_num)
                    .and_then(|attempt| attempt.parse::<u64>().ok())
                    .is_some_and(|attempt| attempt >= 1)
                && is_state_hash(body.get("state_hash"))
        }
        RecordKind::StateCheckpoint => is_state_hash(body.get("state_root")),
    };

    let current_only = matches!(
        kind,
        RecordKind::DeadlineApplied | RecordKind::DeadlineRejected | RecordKind::DeadlineNotDue
    );
    let current_root_ok = body.get("state_root").is_none()
        || body.get("state_format").is_none()
        || body.get("state_root_format").and_then(Value::as_str) == Some("fsm.state-root/3");
    shape_ok && state_format_ok(body, current_only) && root_format_ok(body) && current_root_ok
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::json::Value;
    use crate::record::{Record, RecordError, RecordKind, seal, verify_line, zeros};

    fn hex_hash() -> String {
        format!("sha256:{}", "ab".repeat(32))
    }

    fn verify_kind(kind: RecordKind, body: BTreeMap<String, Value>) -> Result<Record, RecordError> {
        let rec = seal(1, 1, kind, Value::Obj(body), &zeros());
        verify_line(&rec.to_line(), 1, &zeros())
    }

    #[test]
    fn body_schema_requires_typed_fields() {
        let mut applied = BTreeMap::new();
        applied.insert("instance_id".into(), Value::Str("i".into()));
        applied.insert("event".into(), Value::Str("go".into()));
        applied.insert("payload".into(), Value::Obj(BTreeMap::new()));
        applied.insert("request_id".into(), Value::Str("r".into()));
        applied.insert("state_hash".into(), Value::Str(hex_hash()));
        applied.insert("exited".into(), Value::Arr(vec![]));
        applied.insert("entered".into(), Value::Arr(vec![]));
        applied.insert("source_state".into(), Value::Str("s".into()));
        assert!(verify_kind(RecordKind::EventApplied, applied.clone()).is_ok());

        let mut missing = applied.clone();
        missing.remove("exited");
        assert!(matches!(
            verify_kind(RecordKind::EventApplied, missing),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut bad_hash = applied;
        bad_hash.insert(
            "state_hash".into(),
            Value::Str(
                "sha256:zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".into(),
            ),
        );
        assert!(matches!(
            verify_kind(RecordKind::EventApplied, bad_hash),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut ack = BTreeMap::new();
        ack.insert("instance_id".into(), Value::Str("i".into()));
        ack.insert("effect_id".into(), Value::Num("7".into()));
        ack.insert("request_id".into(), Value::Str("r".into()));
        assert!(matches!(
            verify_kind(RecordKind::EffectAcked, ack),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut rejected = BTreeMap::new();
        rejected.insert("instance_id".into(), Value::Str("i".into()));
        rejected.insert("request_id".into(), Value::Str("r".into()));
        rejected.insert("event".into(), Value::Str("go".into()));
        rejected.insert("payload".into(), Value::Obj(BTreeMap::new()));
        rejected.insert("state_hash".into(), Value::Str(hex_hash()));
        assert!(matches!(
            verify_kind(RecordKind::EventRejected, rejected),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut ann = BTreeMap::new();
        ann.insert("instance_id".into(), Value::Num("1".into()));
        ann.insert("request_id".into(), Value::Str("r".into()));
        ann.insert("note".into(), Value::Str("n".into()));
        assert!(matches!(
            verify_kind(RecordKind::Annotated, ann),
            Err(RecordError::BodyInvalid { .. })
        ));
    }

    #[test]
    fn deadline_body_schemas_require_current_formats_and_complete_identity() {
        let mut applied = BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("r".into())),
            ("deadline".into(), Value::Str("expire".into())),
            ("deadline_idx".into(), Value::Num("0".into())),
            ("due_ms".into(), Value::Num("42".into())),
            ("state_hash".into(), Value::Str(hex_hash())),
            (
                "state_format".into(),
                Value::Str(crate::hashes::STATE_FORMAT.into()),
            ),
            ("exited".into(), Value::Arr(vec![Value::Str("wait".into())])),
            (
                "entered".into(),
                Value::Arr(vec![Value::Str("done".into())]),
            ),
            ("source_state".into(), Value::Str("wait".into())),
        ]);
        assert!(verify_kind(RecordKind::DeadlineApplied, applied.clone()).is_ok());
        applied.remove("state_format");
        assert!(matches!(
            verify_kind(RecordKind::DeadlineApplied, applied),
            Err(RecordError::BodyInvalid { .. })
        ));

        let mut idle = BTreeMap::from([
            ("instance_id".into(), Value::Str("i".into())),
            ("request_id".into(), Value::Str("r".into())),
            ("state_hash".into(), Value::Str(hex_hash())),
            (
                "state_format".into(),
                Value::Str(crate::hashes::STATE_FORMAT.into()),
            ),
            ("next_deadline".into(), Value::Str("expire".into())),
            ("next_deadline_idx".into(), Value::Num("0".into())),
            ("next_due_ms".into(), Value::Num("42".into())),
        ]);
        assert!(verify_kind(RecordKind::DeadlineNotDue, idle.clone()).is_ok());
        idle.remove("next_due_ms");
        assert!(matches!(
            verify_kind(RecordKind::DeadlineNotDue, idle),
            Err(RecordError::BodyInvalid { .. })
        ));
    }
}
