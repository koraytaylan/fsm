//! Replaying the deadline records: applied, rejected, and not-due.

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

pub(super) fn apply_deadline_applied(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let machine = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let instance = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?
        .clone();
    let expected_deadline = record_deadline(rec)?;
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match poll_deadline(
        &machine.compiled,
        &machine.tree,
        &instance,
        rec.ts,
        &mut budget,
    ) {
        DeadlineOutcome::Applied(applied) => {
            verify_deadline(rec, &expected_deadline, &applied.deadline, false)?;
            verify_deadline_transition(rec, &applied.transition)?;
            verify_microsteps(rec, &applied.transition.trace.microsteps)?;
            let mut pending = instance.pending.clone();
            pending.extend(
                applied
                    .transition
                    .effects
                    .iter()
                    .map(|effect| format!("{iid}/{}/{}", rec.seq, effect.k)),
            );
            let new = InstanceState {
                status: applied.transition.status_after,
                configuration: applied.transition.configuration_after,
                ctx: applied.transition.ctx_after,
                history: applied.transition.history_after,
                deadlines: applied.transition.deadlines_after,
                pending,
                invocations: applied.transition.invocations_after,
                signals: BTreeMap::new(),
            };
            verify_record_state_hash(rec, &mid, iid, &new)?;
            st.instances.insert(iid.into(), new);
        }
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "outcome",
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}

pub(super) fn apply_deadline_rejected(
    st: &mut StoreState,
    rec: &Record,
) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let machine = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let instance = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    verify_record_state_hash(rec, &mid, iid, instance)?;
    let expected_deadline = record_deadline(rec)?;
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match poll_deadline(
        &machine.compiled,
        &machine.tree,
        instance,
        rec.ts,
        &mut budget,
    ) {
        DeadlineOutcome::Rejected(rejected) => {
            let selected = rejected
                .deadline
                .as_ref()
                .ok_or(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "deadline",
                })?;
            verify_deadline(rec, &expected_deadline, selected, false)?;
            let request_id = rec.body.get("request_id").and_then(Value::as_str);
            let details = expected_deadline_rejected_details(&rejected.rejection, request_id);
            verify_rejection(rec, &rejected.rejection, &details)?;
        }
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "outcome",
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}

pub(super) fn apply_deadline_not_due(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let iid = rec
        .body
        .get("instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let mid = st
        .instance_machines
        .get(iid)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let machine = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let instance = st
        .instances
        .get(iid)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    verify_record_state_hash(rec, &mid, iid, instance)?;
    let expected_next = record_next_deadline(rec)?;
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match poll_deadline(
        &machine.compiled,
        &machine.tree,
        instance,
        rec.ts,
        &mut budget,
    ) {
        DeadlineOutcome::NotDue { next } => match (&expected_next, &next) {
            (None, None) => {}
            (Some(expected), Some(actual)) => {
                verify_deadline(rec, expected, actual, true)?;
            }
            _ => {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "next_deadline",
                });
            }
        },
        _ => {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "outcome",
            });
        }
    }
    claim_request_id(st, rec)?;
    Ok(())
}
