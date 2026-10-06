//! Claim-era record folding; native authentication happens before publication.

use crate::json::Value;
use crate::record::execution::{
    AcknowledgedHandoff, Admission, Claim, Closure, ExecutionState, PendingEffect, Settlement,
    Stopped, StoppedOutcome,
};
use crate::record::{Record, RecordKind};

use super::super::{ReplayError, StoreState};
use super::claim_request_id;
use super::instance::{apply_effect_acked, apply_effect_attempted};
use super::verify_record_state_hash;

fn mismatch(record: &Record, field: &'static str) -> ReplayError {
    ReplayError::FieldMismatch {
        seq: record.seq,
        field,
    }
}

pub(super) fn apply_genesis(state: &mut StoreState, record: &Record) -> Result<(), ReplayError> {
    let admission = match record.body.get("execution_admission") {
        None => Admission::Quarantined,
        Some(Value::Str(value)) if value == "enabled" => Admission::Enabled,
        Some(_) => return Err(mismatch(record, "execution_admission")),
    };
    if record.seq != 0
        || state.last_seq != 0
        || !state.machines.is_empty()
        || state.execution.run_high_water() != 0
    {
        return Err(mismatch(record, "genesis"));
    }
    state.execution = ExecutionState::new(admission);
    state.execution_handoffs = Default::default();
    Ok(())
}

pub(super) fn apply_claimed(state: &mut StoreState, record: &Record) -> Result<(), ReplayError> {
    ensure_request(record)?;
    let claim = Claim::from_body(&record.body).map_err(|_| mismatch(record, "claim"))?;
    let (instance_id, effect_id) = claim.effect();
    let pending = pending(state, instance_id, effect_id, record)?;
    state
        .execution
        .claim(claim.clone(), pending, record.ts)
        .map_err(|error| mismatch(record, error.0))?;
    state
        .execution
        .attach_claim_record_hash(&claim, &format!("sha256:{}", record.hash))
        .map_err(|error| mismatch(record, error.0))?;
    claim_request_id(state, record)
}

fn owned(state: &StoreState, record: &Record) -> Result<Claim, ReplayError> {
    let instance_id = field(record, "instance_id")?;
    let effect_id = field(record, "effect_id")?;
    let run_id = record
        .body
        .get("run_id")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse::<u64>().ok())
        .ok_or_else(|| mismatch(record, "run_id"))?;
    let claim = state
        .execution
        .claim_for(instance_id, effect_id)
        .ok_or_else(|| mismatch(record, "owned"))?;
    if claim.run_id() != run_id {
        return Err(mismatch(record, "run_id"));
    }
    Ok(claim.clone())
}

fn field<'a>(record: &'a Record, name: &'static str) -> Result<&'a str, ReplayError> {
    record
        .body
        .get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| mismatch(record, name))
}

fn pending(
    state: &StoreState,
    instance_id: &str,
    effect_id: &str,
    record: &Record,
) -> Result<PendingEffect, ReplayError> {
    let instance = state
        .instances
        .get(instance_id)
        .ok_or(ReplayError::UnknownInstance { seq: record.seq })?;
    Ok(
        if instance.status == crate::machine::Status::Running
            && instance.pending.iter().any(|pending| pending == effect_id)
        {
            PendingEffect::Present
        } else {
            PendingEffect::Absent
        },
    )
}

pub(super) fn apply_stopped(state: &mut StoreState, record: &Record) -> Result<(), ReplayError> {
    ensure_request(record)?;
    let claim = owned(state, record)?;
    if record.body.get("handler_fingerprint") != claim.to_value().get("handler_fingerprint") {
        return Err(mismatch(record, "handler_fingerprint"));
    }
    let closure = Closure::from_value(
        record
            .body
            .get("closure")
            .ok_or_else(|| mismatch(record, "closure"))?,
    )
    .map_err(|_| mismatch(record, "closure"))?;
    let outcome = StoppedOutcome::from_value(
        record
            .body
            .get("outcome")
            .ok_or_else(|| mismatch(record, "outcome"))?,
    )
    .map_err(|_| mismatch(record, "outcome"))?;
    state
        .execution
        .stop(&claim, Stopped::new(closure, outcome))
        .map_err(|error| mismatch(record, error.0))?;
    claim_request_id(state, record)
}

pub(super) fn apply_settled(state: &mut StoreState, record: &Record) -> Result<(), ReplayError> {
    ensure_request(record)?;
    let claim = owned(state, record)?;
    let disposition = match field(record, "disposition")? {
        "acked" => Settlement::Acked,
        "attempted" => Settlement::Attempted,
        "interrupted" => Settlement::Interrupted,
        _ => return Err(mismatch(record, "disposition")),
    };
    let (instance_id, effect_id) = claim.effect();
    let observed = pending(state, instance_id, effect_id, record)?;
    let handoff = record
        .body
        .get("handoff")
        .map(AcknowledgedHandoff::from_value)
        .transpose()
        .map_err(|_| mismatch(record, "handoff"))?;
    if let Some(handoff) = &handoff {
        let stopped = state
            .execution
            .stopped_for(instance_id, effect_id)
            .ok_or_else(|| mismatch(record, "not_stopped"))?;
        let hash = state
            .execution
            .claim_record_hash(&claim)
            .ok_or_else(|| mismatch(record, "claim_hash"))?;
        if disposition != Settlement::Acked
            || !handoff.matches_acknowledgement(
                &claim,
                stopped,
                hash,
                field(record, "request_id")?,
                record.seq,
            )
        {
            return Err(mismatch(record, "handoff"));
        }
    }
    let stopped = state
        .execution
        .settle(&claim, disposition, observed, record.ts)
        .map_err(|error| mismatch(record, error.0))?;
    if disposition == Settlement::Interrupted {
        if ["outcome", "result", "attempt"]
            .iter()
            .any(|field| record.body.get(field).is_some())
        {
            return Err(mismatch(record, "disposition"));
        }
        let machine_id = state
            .instance_machines
            .get(instance_id)
            .ok_or(ReplayError::UnknownMachine { seq: record.seq })?;
        let instance = state
            .instances
            .get(instance_id)
            .ok_or(ReplayError::UnknownInstance { seq: record.seq })?;
        verify_record_state_hash(record, machine_id, instance_id, instance)?;
        return claim_request_id(state, record);
    }
    let outcome = if stopped.outcome().status() == "ok" {
        "ok"
    } else {
        "failed"
    };
    if field(record, "outcome")? != outcome
        || record.body.get("result") != stopped.outcome().result()
    {
        return Err(mismatch(record, "outcome"));
    }
    let mut derived = record.clone();
    match disposition {
        Settlement::Acked => {
            derived.kind = RecordKind::EffectAcked;
            apply_effect_acked(state, &derived)?;
            if let Some(handoff) = handoff {
                state
                    .execution_handoffs
                    .install(handoff)
                    .map_err(|error| mismatch(record, error.0))?;
            }
            Ok(())
        }
        Settlement::Attempted => {
            if record.body.get("attempt") != claim.to_value().get("attempt") {
                return Err(mismatch(record, "attempt"));
            }
            let machine_id = state
                .instance_machines
                .get(instance_id)
                .ok_or(ReplayError::UnknownMachine { seq: record.seq })?;
            let instance = state
                .instances
                .get(instance_id)
                .ok_or(ReplayError::UnknownInstance { seq: record.seq })?;
            verify_record_state_hash(record, machine_id, instance_id, instance)?;
            derived.kind = RecordKind::EffectAttempted;
            apply_effect_attempted(state, &derived)
        }
        Settlement::Interrupted => Err(mismatch(record, "disposition")),
    }
}

pub(super) fn apply_enabled(state: &mut StoreState, record: &Record) -> Result<(), ReplayError> {
    ensure_request(record)?;
    let previous_head = field(record, "previous_head")?;
    if previous_head != format!("sha256:{}", record.prev) {
        return Err(mismatch(record, "previous_head"));
    }
    let evidence = record
        .body
        .get("quiescence")
        .and_then(Value::as_obj)
        .ok_or_else(|| mismatch(record, "quiescence"))?;
    if evidence.len() != 3
        || evidence.get("previous_head").and_then(Value::as_str) != Some(previous_head)
    {
        return Err(mismatch(record, "quiescence"));
    }
    crate::record::execution::NativeDomain::from_value(
        evidence
            .get("domain")
            .ok_or_else(|| mismatch(record, "domain"))?,
    )
    .map_err(|_| mismatch(record, "domain"))?;
    let receipt = evidence
        .get("receipt")
        .and_then(Value::as_str)
        .and_then(|receipt| receipt.strip_prefix("sha256:"))
        .ok_or_else(|| mismatch(record, "receipt"))?;
    if receipt.len() != 64
        || !receipt
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(mismatch(record, "receipt"));
    }
    state
        .execution
        .enable()
        .map_err(|error| mismatch(record, error.0))?;
    claim_request_id(state, record)
}

fn ensure_request(record: &Record) -> Result<(), ReplayError> {
    if field(record, "request_id")?.len() > 4096 {
        return Err(mismatch(record, "request_id"));
    }
    let fingerprint = field(record, "request_fp")?
        .strip_prefix("sha256:")
        .ok_or_else(|| mismatch(record, "request_fp"))?;
    if fingerprint.len() != 64
        || !fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(mismatch(record, "request_fp"));
    }
    Ok(())
}
