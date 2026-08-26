//! Replaying a migration: the instance moves onto the machine the record
//! names, and the fold follows it there.
//!
//! An instance's records legitimately span two definitions, so replay tracks
//! the current machine **per instance** rather than assuming the one its
//! creation named. Everything the record claims is re-derived by running the
//! same pure migration at the record's own timestamp — no clock, no
//! ambiguity.
//!
//! Plan 0011 tasks 5501 (fold) and 5502 (claims).

use crate::expr::eval::Budget;
use crate::json::Value;
use crate::record::Record;

use super::super::verify::verify_record_state_hash;
use super::super::{ReplayError, StoreState};

pub(super) fn apply_instance_migrated(
    st: &mut StoreState,
    rec: &Record,
) -> Result<(), ReplayError> {
    let field = |name: &'static str| {
        rec.body
            .get(name)
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: name,
            })
    };
    let instance_id = field("instance_id")?.to_string();
    let from_machine_id = field("from_machine_id")?.to_string();
    let to_machine_id = field("to_machine_id")?.to_string();

    // The record names the machine the instance was on; if the fold disagrees,
    // the journal is not the one this record was written against.
    if st.instance_machines.get(&instance_id) != Some(&from_machine_id) {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "from_machine_id",
        });
    }
    let from = st
        .machines
        .get(&from_machine_id)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?
        .clone();
    let to = st
        .machines
        .get(&to_machine_id)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?
        .clone();
    let state = st
        .instances
        .get(&instance_id)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?
        .clone();

    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    let migrated = crate::migrate::apply::migrate(
        &from.compiled,
        &to.compiled,
        &to.tree,
        &state,
        rec.ts,
        &mut budget,
    )
    .map_err(|_| ReplayError::FieldMismatch {
        seq: rec.seq,
        field: "migration",
    })?;

    verify_claims(rec, &migrated.report)?;
    verify_record_state_hash(rec, &to_machine_id, &instance_id, &migrated.state)?;
    st.instances.insert(instance_id.clone(), migrated.state);
    st.instance_machines.insert(instance_id, to_machine_id);
    super::super::ctx::claim_request_id(st, rec)?;
    Ok(())
}

/// The report fields a migration journals are claims, so they are recomputed
/// and compared rather than trusted.
fn verify_claims(
    rec: &Record,
    report: &crate::migrate::apply::MigrationReport,
) -> Result<(), ReplayError> {
    let dropped: Vec<Value> = report
        .dropped_history
        .iter()
        .cloned()
        .map(Value::Str)
        .collect();
    if rec.body.get("dropped_history") != Some(&Value::Arr(dropped)) {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "dropped_history",
        });
    }
    if rec.body.get("rescheduled_deadlines")
        != Some(&crate::migrate::apply::rescheduled_value(
            &report.rescheduled_deadlines,
        ))
    {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "rescheduled_deadlines",
        });
    }
    let settled = report.settled.as_ref().ok_or(ReplayError::FieldMismatch {
        seq: rec.seq,
        field: "configuration_after",
    })?;
    if rec.body.get("configuration_after") != Some(&crate::hashes::configuration_value(settled)) {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "configuration_after",
        });
    }
    super::verify_microsteps(rec, &report.microsteps)
}
