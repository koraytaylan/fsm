//! Replaying a signal delivery: two instances, one record.
//!
//! The journaled `outcome` is what the sender's audit trail promises, so it
//! is re-derived rather than trusted: replay applies the same event to the
//! same target and checks that it came out the same way. The sender's own
//! state changes only by losing the pending entry — delivery is not a
//! transition of the sender.
//!
//! Plan 0010 task 5002.

use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::json::Value;
use crate::machine::{InstanceState, Status};
use crate::record::Record;
use crate::step::{Outcome, step};

use super::super::ctx::claim_request_id;
use super::super::verify::verify_record_state_hash;
use super::super::{ReplayError, StoreState};

pub(super) fn apply_signal_delivered(st: &mut StoreState, rec: &Record) -> Result<(), ReplayError> {
    let field = |name: &'static str| {
        rec.body
            .get(name)
            .and_then(Value::as_str)
            .ok_or(ReplayError::FieldMismatch {
                seq: rec.seq,
                field: name,
            })
    };
    let sender_id = field("sender_instance_id")?.to_string();
    let signal_id = field("signal_id")?.to_string();
    let target_id = field("target_instance_id")?.to_string();
    let event = field("event")?.to_string();
    let outcome = field("outcome")?.to_string();
    let payload = rec
        .body
        .get("payload")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));

    // Whatever happened to the target, the sender loses the pending entry:
    // a signal is fire-and-forget, and a sender that needs an answer models
    // the target signalling back.
    let sender_mid = st
        .instance_machines
        .get(&sender_id)
        .cloned()
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    let sender = st
        .instances
        .get_mut(&sender_id)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    sender.signals.remove(&signal_id);
    let sender_after = sender.clone();
    verify_sender_hash(rec, &sender_mid, &sender_id, &sender_after)?;

    let derived = deliver(st, rec, &target_id, &event, &payload)?;
    if derived != outcome {
        return Err(ReplayError::FieldMismatch {
            seq: rec.seq,
            field: "outcome",
        });
    }
    claim_request_id(st, rec)?;
    Ok(())
}

/// The sender's hash lives under its own key, because the record names two
/// instances and `state_hash` would not say which.
fn verify_sender_hash(
    rec: &Record,
    machine_id: &str,
    instance_id: &str,
    state: &InstanceState,
) -> Result<(), ReplayError> {
    let mut aliased = rec.clone();
    if let Value::Obj(body) = &mut aliased.body {
        if let Some(hash) = body.get("sender_state_hash").cloned() {
            body.insert("state_hash".into(), hash);
        }
    }
    verify_record_state_hash(&aliased, machine_id, instance_id, state)
}

/// Re-apply the delivery to the target and name what happened.
fn deliver(
    st: &mut StoreState,
    rec: &Record,
    target_id: &str,
    event: &str,
    payload: &Value,
) -> Result<String, ReplayError> {
    let Some(target_mid) = st.instance_machines.get(target_id).cloned() else {
        return Ok("target_missing".into());
    };
    let machine = st
        .machines
        .get(&target_mid)
        .ok_or(ReplayError::UnknownMachine { seq: rec.seq })?;
    let target = st
        .instances
        .get(target_id)
        .ok_or(ReplayError::UnknownInstance { seq: rec.seq })?;
    if target.status != Status::Running {
        return Ok("target_settled".into());
    }
    let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
    match step(
        &machine.compiled,
        &machine.tree,
        target,
        event,
        payload,
        rec.ts,
        &mut budget,
    ) {
        Outcome::Applied(applied) => {
            let mut post = InstanceState {
                status: applied.status_after,
                configuration: applied.configuration_after,
                ctx: applied.ctx_after,
                history: applied.history_after,
                deadlines: applied.deadlines_after,
                pending: target.pending.clone(),
                invocations: applied.invocations_after,
                signals: target.signals.clone(),
            };
            post.pending.extend(
                applied
                    .effects
                    .iter()
                    .map(|effect| format!("{target_id}/{}/{}", rec.seq, effect.k)),
            );
            post.signals.extend(
                applied
                    .signals
                    .iter()
                    .map(|(k, signal)| (format!("{target_id}/{}/{k}", rec.seq), signal.clone())),
            );
            if let Some(want) = rec.body.get("target_state_hash").and_then(Value::as_str) {
                let got = crate::hashes::state_hash(&target_mid, target_id, rec.seq, &post);
                if got != want {
                    return Err(ReplayError::StateHashMismatch {
                        seq: rec.seq,
                        expected: want.into(),
                        found: got,
                    });
                }
            }
            st.instances.insert(target_id.into(), post);
            Ok("applied".into())
        }
        Outcome::Ignored => Ok("ignored".into()),
        Outcome::Rejected(rejection) => Ok(format!("rejected:{}", rejection.code)),
    }
}
