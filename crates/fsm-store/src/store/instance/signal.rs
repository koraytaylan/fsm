//! Delivering a signal: the one place two instances that are not parent and
//! child touch each other.
//!
//! A delivery that fails is journaled as attempted rather than dropped,
//! because the sender's audit trail must show what it tried; and the sender's
//! pending entry clears in every case, because a signal is fire-and-forget by
//! design — a sender that needs an answer models the target signalling back.
//!
//! Plan 0010 task 5002.

use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::{InstanceState, Status};
use fsm_core::record::{RecordKind, microsteps_value};
use fsm_core::step::{Outcome, step};

use crate::store::{ErrorObj, Store};

impl Store {
    pub fn signal_deliver(
        &mut self,
        sender_id: &str,
        signal_id: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.signal_deliver_on(
            &mut crate::clock::GlobalClock,
            sender_id,
            signal_id,
            request_id,
        )
    }

    pub fn signal_deliver_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        sender_id: &str,
        signal_id: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(replay) =
            self.claim_request(request_id, Self::fp_signal(sender_id, signal_id))?
        {
            return replay;
        }
        let sender = self.state.instances.get(sender_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", sender_id).request_id(request_id)
        })?;
        let Some(signal) = sender.signals.get(signal_id).cloned() else {
            let pending: Vec<Value> = sender.signals.keys().cloned().map(Value::Str).collect();
            return Err(ErrorObj::new("req/field_unknown", "unknown signal id")
                .hint("use an id from signals_pending")
                .details(Value::Obj(BTreeMap::from([(
                    "signals_pending".into(),
                    Value::Arr(pending),
                )])))
                .request_id(request_id));
        };
        // Self-delivery is always a modelling mistake: `raise` is the
        // construct the author wanted, and it costs no record.
        if signal.target_instance_id == sender_id {
            return Err(
                ErrorObj::new("req/signal_target", "a signal addressed to its own sender")
                    .hint("use raise to deliver an event to this instance inside its own macrostep")
                    .request_id(request_id),
            );
        }

        let commit_ts = clock.now_ms();
        let seq = self.journal.last_seq + 1;
        let target_id = signal.target_instance_id.clone();
        let payload = Value::Obj(
            signal
                .payload
                .iter()
                .map(|(name, value)| (name.clone(), Value::Str(value.canonical_string())))
                .collect(),
        );
        let mut sender_after = sender.clone();
        sender_after.signals.remove(signal_id);
        let sender_mid = self
            .state
            .instance_machines
            .get(sender_id)
            .cloned()
            .unwrap_or_default();

        // Everything the target did with it, named rather than lost.
        let mut target_after = None;
        let mut microsteps = None;
        let outcome = match self.state.instance_machines.get(&target_id).cloned() {
            None => "target_missing".to_string(),
            Some(target_mid) => {
                let machine = &self.state.machines[&target_mid];
                let target = &self.state.instances[&target_id];
                if target.status != Status::Running {
                    "target_settled".to_string()
                } else {
                    let mut budget =
                        fsm_core::expr::eval::Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
                    match step(
                        &machine.compiled,
                        &machine.tree,
                        target,
                        &signal.event,
                        &payload,
                        commit_ts,
                        &mut budget,
                    ) {
                        Outcome::Applied(applied) => {
                            let mut post = InstanceState {
                                status: applied.status_after,
                                configuration: applied.configuration_after.clone(),
                                ctx: applied.ctx_after.clone(),
                                history: applied.history_after.clone(),
                                deadlines: applied.deadlines_after.clone(),
                                pending: target.pending.clone(),
                                invocations: applied.invocations_after.clone(),
                                signals: target.signals.clone(),
                            };
                            post.pending.extend(
                                applied
                                    .effects
                                    .iter()
                                    .map(|effect| format!("{target_id}/{seq}/{}", effect.k)),
                            );
                            post.signals
                                .extend(applied.signals.iter().map(|(k, emitted)| {
                                    (format!("{target_id}/{seq}/{k}"), emitted.clone())
                                }));
                            microsteps = microsteps_value(&applied.trace.microsteps);
                            target_after = Some((target_mid, post));
                            "applied".to_string()
                        }
                        Outcome::Ignored => "ignored".to_string(),
                        Outcome::Rejected(rejection) => format!("rejected:{}", rejection.code),
                    }
                }
            }
        };

        let mut body = BTreeMap::new();
        body.insert(
            "sender_instance_id".into(),
            Value::Str(sender_id.to_string()),
        );
        body.insert("signal_id".into(), Value::Str(signal_id.to_string()));
        body.insert("target_instance_id".into(), Value::Str(target_id.clone()));
        body.insert("event".into(), Value::Str(signal.event.clone()));
        body.insert("payload".into(), payload);
        body.insert("outcome".into(), Value::Str(outcome.clone()));
        body.insert("request_id".into(), Value::Str(request_id.to_string()));
        body.insert(
            "sender_state_hash".into(),
            Value::Str(state_hash(&sender_mid, sender_id, seq, &sender_after)),
        );
        if let Some((target_mid, post)) = &target_after {
            body.insert(
                "target_state_hash".into(),
                Value::Str(state_hash(target_mid, &target_id, seq, post)),
            );
        }
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(microsteps) = microsteps {
            body.insert("microsteps".into(), microsteps);
        }
        let record =
            self.append_at_with_root(RecordKind::SignalDelivered, Value::Obj(body), commit_ts)?;
        self.state.instances.insert(sender_id.into(), sender_after);
        if let Some((_, post)) = target_after {
            self.state.instances.insert(target_id.clone(), post);
        }
        for id in [sender_id, target_id.as_str()] {
            self.history.entry(id.into()).or_default().push(record.seq);
        }
        self.note_record(&record);
        let response = delivered_response(
            sender_id,
            signal_id,
            &target_id,
            &signal.event,
            &outcome,
            request_id,
            record.seq,
            false,
        );
        self.commit_dedup(request_id, response.clone(), record.seq);
        self.finish_commit();
        Ok(response)
    }
}

/// The response a delivery gives, warm or replayed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn delivered_response(
    sender_id: &str,
    signal_id: &str,
    target_id: &str,
    event: &str,
    outcome: &str,
    request_id: &str,
    seq: u64,
    duplicate: bool,
) -> Value {
    Value::Obj(BTreeMap::from([
        ("ok".into(), Value::Str("true".into())),
        ("delivered".into(), Value::Bool(outcome == "applied")),
        ("sender_instance_id".into(), Value::Str(sender_id.into())),
        ("signal_id".into(), Value::Str(signal_id.into())),
        ("target_instance_id".into(), Value::Str(target_id.into())),
        ("event".into(), Value::Str(event.into())),
        ("outcome".into(), Value::Str(outcome.into())),
        ("request_id".into(), Value::Str(request_id.into())),
        ("seq".into(), Value::Num(seq.to_string())),
        ("duplicate".into(), Value::Bool(duplicate)),
    ]))
}
