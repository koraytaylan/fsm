//! The record-kind dispatcher: every kind's applier, one module per
//! subject.
//!
//! An instance written before a format bump keeps its records and its hashes
//! forever: every state-bearing record carries the `state_format` it was
//! written under, and verification picks the function that format names.
//! There is no moment at which an old hash is recomputed under a new format,
//! and no reader ever guesses from a record's age.

use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::hashes::configuration_value;
use crate::json::Value;
use crate::machine::{InstanceState, Status};
use crate::record::{Record, RecordKind};
use crate::step::{DeadlineOutcome, Outcome, create, poll_deadline, step};
use crate::tree::Tree;

use super::ctx::{claim_request_id, overrides_from};
use super::deadline::{
    record_deadline, record_next_deadline, verify_deadline, verify_deadline_transition,
};
use super::report::enabled_reports_value;
use super::verify::{
    expected_deadline_rejected_details, expected_event_rejected_details,
    expected_request_rejected_details, verify_microsteps, verify_record_state_hash,
    verify_rejection,
};
use super::{
    DefinitionCompileMode, ReplayError, STATE_ROOT_FORMAT, StoreState, StoredMachine,
    legacy_state_root_at, state_hash_for_record, state_root_at,
};

mod deadline_records;
mod event;
mod instance;
mod invoke;

use deadline_records::{apply_deadline_applied, apply_deadline_not_due, apply_deadline_rejected};
use event::{apply_event_applied, apply_event_rejected_or_ignored};
use instance::{
    apply_annotated, apply_effect_acked, apply_instance_cancelled, apply_instance_created,
    apply_request_rejected,
};
use invoke::{apply_instance_invoked, apply_invocation_returned};

pub(super) fn apply(
    st: &mut StoreState,
    rec: &Record,
    compile_mode: DefinitionCompileMode,
) -> Result<(), ReplayError> {
    let applied = match rec.kind {
        RecordKind::Genesis => Ok(()),
        RecordKind::MachineDefined => apply_machine_defined(st, rec, compile_mode),
        RecordKind::InstanceCreated => apply_instance_created(st, rec),
        RecordKind::EventApplied => apply_event_applied(st, rec),
        RecordKind::EventRejected | RecordKind::EventIgnored => {
            apply_event_rejected_or_ignored(st, rec, compile_mode)
        }
        RecordKind::DeadlineApplied => apply_deadline_applied(st, rec),
        RecordKind::DeadlineRejected => apply_deadline_rejected(st, rec),
        RecordKind::DeadlineNotDue => apply_deadline_not_due(st, rec),
        RecordKind::EffectAcked => apply_effect_acked(st, rec),
        RecordKind::InstanceInvoked => apply_instance_invoked(st, rec),
        RecordKind::InvocationReturned => apply_invocation_returned(st, rec),
        RecordKind::RequestRejected => apply_request_rejected(st, rec),
        RecordKind::InstanceCancelled => apply_instance_cancelled(st, rec),
        RecordKind::Annotated => apply_annotated(st, rec),
        RecordKind::StateCheckpoint => Ok(()),
    };
    applied?;
    if let Some(root) = rec.body.get("state_root") {
        let want = root.as_str().ok_or(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "state_root",
        })?;
        let found = match rec.body.get("state_root_format").and_then(Value::as_str) {
            Some(STATE_ROOT_FORMAT) => state_root_at(st, rec.seq),
            None => legacy_state_root_at(st, rec.seq),
            Some(_) => {
                return Err(ReplayError::FieldMismatch {
                    seq: rec.seq,
                    field: "state_root_format",
                });
            }
        };
        if want != found {
            return Err(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: "state_root",
            });
        }
    }
    Ok(())
}

fn apply_machine_defined(
    st: &mut StoreState,
    rec: &Record,
    compile_mode: DefinitionCompileMode,
) -> Result<(), ReplayError> {
    let def = rec
        .body
        .get("def")
        .cloned()
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    // A definition that invokes is compiled against the machines already
    // folded, exactly as `define_machine` compiled it against the ones the
    // store held: a done-invoke payload types against the child's
    // declarations, and without them the parent would not compile at all.
    let catalogue: crate::spec::Catalogue = st
        .machines
        .iter()
        .filter_map(|(machine_id, stored)| {
            crate::hashes::digest_of(machine_id)
                .map(|digest| (digest.to_string(), stored.compiled.spec.clone()))
        })
        .collect();
    let compiled = match compile_mode {
        DefinitionCompileMode::Current => {
            crate::spec::compile_accepted_with_catalogue(&def, &catalogue)
        }
        DefinitionCompileMode::HistoricalPersistence => {
            crate::spec::compile_accepted_historical_unchecked(&def)
        }
    }
    .map_err(|_| ReplayError::UnknownMachine { seq: rec.seq })?;
    let tree = Tree::for_machine(&compiled.spec);
    let id = rec
        .body
        .get("machine_id")
        .and_then(Value::as_str)
        .unwrap_or(&compiled.machine_id)
        .to_string();
    if id != compiled.machine_id {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "machine_id",
        });
    }
    st.machines.insert(
        id,
        StoredMachine {
            def,
            compiled,
            tree,
        },
    );
    Ok(())
}
