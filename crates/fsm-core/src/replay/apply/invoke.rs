//! Replaying an invocation: fold derives the child from one record.

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

/// Fold derives the child from this one record.
///
/// There is no separate `instance_created` for a child: its existence,
/// machine, initial configuration, and context are a pure function of this
/// body and the record's `ts`, so replay reconstructs it by running the same
/// `create` the write ran — which, after plan 0009, is a macrostep, so a
/// child whose initial state has an eventless exit reacts on creation exactly
/// as a root instance does. `child_state_hash` is what proves the derivation
/// matched, which is why the child's reaction is not journaled here: it is
/// fully re-derived, and recording a derivable fact in a permanent record is
/// what SPEC's payload discipline exists to avoid.
pub(super) fn apply_instance_invoked(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let parent_id = rec
        .body
        .get("parent_instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let child_id = rec
        .body
        .get("child_instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let slot = rec
        .body
        .get("slot")
        .and_then(Value::as_str)
        .ok_or(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "slot",
        })?;
    let child_mid = rec
        .body
        .get("child_machine_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?
        .to_string();
    if child_id != crate::hashes::child_instance_id(parent_id, slot) {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "child_instance_id",
        });
    }
    let child_machine = st
        .machines
        .get(&child_mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let overrides = overrides_from(
        &child_machine.compiled.spec.context,
        rec.body.get("overrides"),
    )
    .ok_or(ReplayError::FieldMismatch {
        seq: rec.seq,
        field: "overrides",
    })?;
    let created = create(
        &child_machine.compiled,
        &child_machine.tree,
        &overrides,
        rec.ts,
    )
    .map_err(|_| ReplayError::UnknownInstance { seq: rec.seq })?;
    let child = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: created
            .effects
            .iter()
            .map(|effect| format!("{child_id}/0/{}", effect.k))
            .collect(),
        invocations: created.invocations_after,
        signals: BTreeMap::new(),
    };
    if let Some(want) = rec.body.get("child_state_hash").and_then(Value::as_str) {
        let got = state_hash_for_record(rec, &child_mid, child_id, &child).ok_or(
            ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_format",
            },
        )?;
        if got != want {
            return Err(ReplayError::StateHashMismatch {
                seq: rec.seq,
                expected: want.into(),
                found: got,
            });
        }
    }
    // The parent's slot moves Pending -> Running.
    let parent_mid = st
        .instance_machines
        .get(parent_id)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let parent = st
        .instances
        .get_mut(parent_id)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let invocation = parent
        .invocations
        .get_mut(slot)
        .ok_or(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "slot",
        })?;
    if invocation.status != crate::machine::InvokeStatus::Pending {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "slot",
        });
    }
    invocation.status = crate::machine::InvokeStatus::Running;
    let parent_after = parent.clone();
    verify_record_state_hash(rec, &parent_mid, parent_id, &parent_after)?;
    st.instances.insert(child_id.into(), child);
    st.instance_machines.insert(child_id.into(), child_mid);
    claim_request_id(st, rec)?;
    Ok(())
}

/// A returned invocation re-derives the parent's macrostep.
///
/// The payload is the record's, not a fresh projection of the child: the
/// child may have moved on since — nothing stops a cancelled child being
/// cancelled again, or a completed one being read later — and the parent's
/// state was computed from the values that were in hand when the record was
/// written. Re-projecting would make replay a function of the child's
/// present rather than of the journal.
pub(super) fn apply_invocation_returned(
    st: &mut StoreState,
    rec: &Record,
) -> Result<(), ReplayError> {
    let parent_id = rec
        .body
        .get("parent_instance_id")
        .and_then(Value::as_str)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let slot = rec
        .body
        .get("slot")
        .and_then(Value::as_str)
        .ok_or(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "slot",
        })?;
    let mid = st
        .instance_machines
        .get(parent_id)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let machine = st
        .machines
        .get(&mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let parent = st
        .instances
        .get(parent_id)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let payload = payload_from(st, &machine.compiled, slot, rec)?;
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    let outcome = crate::step::deliver_generated(
        &machine.compiled,
        &machine.tree,
        parent,
        &format!("{}{slot}", crate::step::DONE_INVOKE_PREFIX),
        &payload,
        rec.ts,
        &mut budget,
    );
    let Outcome::Applied(applied) = outcome else {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "outcome",
        });
    };
    let mut post = InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after,
        ctx: applied.ctx_after,
        history: applied.history_after,
        deadlines: applied.deadlines_after,
        pending: parent.pending.clone(),
        invocations: applied.invocations_after,
        signals: parent.signals.clone(),
    };
    post.pending.extend(
        applied
            .effects
            .iter()
            .map(|effect| format!("{parent_id}/{}/{}", rec.seq, effect.k)),
    );
    if let Some(invocation) = post.invocations.get_mut(slot) {
        invocation.status = crate::machine::InvokeStatus::Returned;
    }
    verify_record_state_hash(rec, &mid, parent_id, &post)?;
    verify_microsteps(rec, &applied.trace.microsteps)?;
    st.instances.insert(parent_id.into(), post);
    claim_request_id(st, rec)?;
    Ok(())
}

/// The journaled payload, typed against the **child's** declarations.
///
/// The types come from the child machine the journal already names, reached
/// through `child_instance_id`, because the projection's types are the
/// child's — the parent declares only which of them it wants.
fn payload_from(
    st: &StoreState,
    parent: &crate::machine::CompiledMachine,
    slot: &str,
    rec: &Record,
) -> Result<BTreeMap<String, crate::expr::eval::Val>, ReplayError> {
    let mismatch = |field: &'static str| ReplayError::FieldMismatch {
        seq: rec.seq,
        field,
    };
    let returns = parent
        .spec
        .walk_states()
        .into_iter()
        .find_map(|(node, _)| {
            node.invokes
                .iter()
                .find(|invoke| invoke.id == slot)
                .map(|invoke| invoke.returns.clone())
        })
        .ok_or_else(|| mismatch("slot"))?;
    let child_id = rec
        .body
        .get("child_instance_id")
        .and_then(Value::as_str)
        .ok_or_else(|| mismatch("child_instance_id"))?;
    let child_mid = st
        .instance_machines
        .get(child_id)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let child = st
        .machines
        .get(child_mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let journaled = rec
        .body
        .get("payload")
        .and_then(Value::as_obj)
        .ok_or_else(|| mismatch("payload"))?;
    let mut payload = BTreeMap::new();
    for (field, raw) in journaled {
        let child_var = returns
            .iter()
            .find(|(name, _)| name == field)
            .map(|(_, var)| var.as_str())
            .ok_or_else(|| mismatch("payload"))?;
        let declared = child
            .compiled
            .spec
            .context
            .iter()
            .find(|c| c.name == child_var)
            .ok_or_else(|| mismatch("payload"))?;
        let text = raw.as_str().ok_or_else(|| mismatch("payload"))?;
        let value =
            crate::replay::parse_ctx_val(&declared.ty, text).ok_or_else(|| mismatch("payload"))?;
        payload.insert(field.clone(), value);
    }
    Ok(payload)
}
