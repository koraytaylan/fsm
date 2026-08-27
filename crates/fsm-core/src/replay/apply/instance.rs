//! Replaying the records about one instance's lifecycle: creation,
//! effect acknowledgement, request refusal, cancellation, and notes.

use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::hashes::configuration_value;
use crate::json::Value;
use crate::machine::{InstanceState, Status};
use crate::record::{Record, RecordKind};
use crate::step::{DeadlineOutcome, Outcome, create, poll_deadline, step};
use crate::tree::Tree;

use super::super::ctx::{claim_request_id, overrides_from};
use super::super::deadline::{
    record_deadline, record_next_deadline, verify_deadline, verify_deadline_transition,
};
use super::super::report::enabled_reports_value;
use super::super::verify::{
    expected_deadline_rejected_details, expected_event_rejected_details,
    expected_request_rejected_details, verify_microsteps, verify_record_state_hash,
    verify_rejection,
};
use super::super::{
    DefinitionCompileMode, ReplayError, STATE_ROOT_FORMAT, StoreState, StoredMachine,
    legacy_state_root_at, state_hash_for_record, state_root_at,
};

pub(super) fn apply_instance_created(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let mid = rec
        .body
        .get("machine_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let m = st
        .machines
        .get(mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let overrides = match overrides_from(&m.compiled.spec.context, rec.body.get("overrides")) {
        Some(o) => o,
        None => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "overrides",
            });
        }
    };
    let a = create(&m.compiled, &m.tree, &overrides, rec.ts)
        .map_err(|_| ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = InstanceState {
        status: a.status_after,
        configuration: a.configuration_after,
        ctx: a.ctx_after,
        history: a.history_after,
        deadlines: a.deadlines_after,
        pending: a
            .effects
            .iter()
            .map(|e| format!("{iid}/0/{}", e.k))
            .collect(),
        invocations: a.invocations_after,
        // The same derived ids the write produced: a fold that
        // numbered them differently would not reproduce the state.
        signals: a
            .signals
            .iter()
            .map(|(k, signal)| (format!("{iid}/{}/{k}", rec.seq), signal.clone()))
            .collect(),
    };
    if let Some(want) = rec.body.get("state_hash").and_then(Value::as_str) {
        let got =
            state_hash_for_record(rec, mid, iid, &inst).ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_format",
            })?;
        if got != want {
            return Err(ReplayError::StateHashMismatch {
                seq: rec.seq,
                expected: want.into(),
                found: got,
            });
        }
    }
    if let Some(want) = rec.body.get("leaf").and_then(Value::as_str) {
        if inst.configuration.leaf(None) != Some(want) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "leaf",
            });
        }
    }
    if let Some(want) = rec.body.get("configuration") {
        if want != &configuration_value(&inst.configuration) {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "configuration",
            });
        }
    }
    verify_microsteps(rec, &a.trace.microsteps)?;
    st.instances.insert(iid.into(), inst);
    st.instance_machines.insert(iid.into(), mid.into());
    claim_request_id(st, rec)?;
    Ok(())
}

pub(super) fn apply_effect_acked(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let eid =
        rec.body
            .get("effect_id")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "effect_id",
            })?;
    let inst = st
        .instances
        .get_mut(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    if !inst.pending.iter().any(|p| p == eid) {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "effect_id",
        });
    }
    inst.pending.retain(|p| p != eid);
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let want =
        rec.body
            .get("state_hash")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_hash",
            })?;
    let got = state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
        seq: rec.seq,
        field: "state_format",
    })?;
    if got != want {
        return Err(ReplayError::StateHashMismatch {
            seq: rec.seq,
            expected: want.into(),
            found: got,
        });
    }
    claim_request_id(st, rec)?;
    Ok(())
}

pub(super) fn apply_request_rejected(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let want =
        rec.body
            .get("state_hash")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_hash",
            })?;
    let got = state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
        seq: rec.seq,
        field: "state_format",
    })?;
    if got != want {
        return Err(ReplayError::StateHashMismatch {
            seq: rec.seq,
            expected: want.into(),
            found: got,
        });
    }
    match rec.body.get("operation").and_then(Value::as_str) {
        Some("ack") => {
            let eid = rec.body.get("effect_id").and_then(Value::as_str).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                },
            )?;
            if inst.pending.iter().any(|p| p == eid) {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "effect_id",
                });
            }
            if rec.body.get("code").and_then(Value::as_str) != Some("req/field_unknown") {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "code",
                });
            }
            if rec.body.get("message").and_then(Value::as_str) != Some("unknown effect id") {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "message",
                });
            }
            if rec.body.get("hint").and_then(Value::as_str)
                != Some("use an id from effects_pending")
            {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "hint",
                });
            }
            if rec.body.get("span").is_some() {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "span",
                });
            }
            let details = rec.body.get("details").and_then(Value::as_obj).ok_or(
                ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "details",
                },
            )?;
            let rid = rec.body.get("request_id").and_then(Value::as_str);
            let want = expected_request_rejected_details(rid, &inst.pending);
            if details != &want {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "details",
                });
            }
        }
        Some("poll_deadline") => {
            let machine = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
            let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
            match poll_deadline(&machine.compiled, &machine.tree, inst, rec.ts, &mut budget) {
                DeadlineOutcome::Rejected(rejected) if rejected.deadline.is_none() => {
                    let request_id = rec.body.get("request_id").and_then(Value::as_str);
                    let details =
                        expected_deadline_rejected_details(&rejected.rejection, request_id);
                    verify_rejection(rec, &rejected.rejection, &details)?;
                }
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "outcome",
                    });
                }
            }
        }
        // An invocation refused for the slot's state: decidable from the
        // parent's own slots, so replay re-derives it rather than trusting
        // the record's word for it.
        Some("invoke") => {
            let slot =
                rec.body
                    .get("slot")
                    .and_then(Value::as_str)
                    .ok_or(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "slot",
                    })?;
            let still_pending = inst.invocations.get(slot).is_some_and(|invocation| {
                invocation.status == crate::machine::InvokeStatus::Pending
            });
            if still_pending {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "slot",
                });
            }
            if rec.body.get("code").and_then(Value::as_str) != Some("req/invoke_slot_state") {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "code",
                });
            }
        }
        // A migration the store refused: re-run it and require the same
        // refusal. The record names its target for exactly this reason.
        Some("migrate") => {
            let to_machine_id = rec
                .body
                .get("to_machine_id")
                .and_then(Value::as_str)
                .ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "to_machine_id",
                })?;
            let from = st
                .machines
                .get(&mid)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?
                .clone();
            let to = st
                .machines
                .get(to_machine_id)
                .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?
                .clone();
            let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
            let refused = crate::migrate::apply::migrate(
                &from.compiled,
                &to.compiled,
                &to.tree,
                inst,
                rec.ts,
                &mut budget,
            );
            match refused {
                Err(rejection)
                    if rec.body.get("code").and_then(Value::as_str) == Some(rejection.code) => {}
                _ => {
                    return Err(ReplayError::FieldMismatch {
                        seq: rec.seq,
                        field: "code",
                    });
                }
            }
        }
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "operation",
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}

pub(super) fn apply_instance_cancelled(
    st: &mut StoreState,
    rec: &Record,
) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = st
        .instances
        .get_mut(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    inst.status = Status::Cancelled;
    inst.deadlines.clear();
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let inst = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let want =
        rec.body
            .get("state_hash")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_hash",
            })?;
    let got = state_hash_for_record(rec, &mid, iid, inst).ok_or(ReplayError::FieldMismatch {
        seq: rec.seq,
        field: "state_format",
    })?;
    if got != want {
        return Err(ReplayError::StateHashMismatch {
            seq: rec.seq,
            expected: want.into(),
            found: got,
        });
    }
    claim_request_id(st, rec)?;
    Ok(())
}

pub(super) fn apply_annotated(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid =
        rec.body
            .get("instance_id")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "instance_id",
            })?;
    if !st.instances.contains_key(iid) {
        return Err(ReplayError::UnknownInstance { seq: rec.seq });
    }
    if rec.body.get("note").and_then(Value::as_str).is_none() {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "note",
        });
    }
    claim_request_id(st, rec)?;
    Ok(())
}

/// One failed attempt at an effect.
///
/// It claims its `request_id` and changes nothing else: the effect stays
/// pending and the instance stays where it was. That is what makes a retry a
/// retry rather than a re-emit, and it is the property the whole retry
/// design rests on — a journal with attempt records folds to exactly the
/// state the same journal without them folds to.
pub(super) fn apply_effect_attempted(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let instance_id =
        rec.body
            .get("instance_id")
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "instance_id",
            })?;
    if !st.instances.contains_key(instance_id) {
        return Err(ReplayError::UnknownInstance { seq: rec.seq });
    }
    for field in ["effect_id", "outcome"] {
        if rec.body.get(field).and_then(Value::as_str).is_none() {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: match field {
                    "effect_id" => "effect_id",
                    _ => "outcome",
                },
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}
