//! Enacting an invocation: one record creates a child.
//!
//! The pure core wrote the intent into the parent's state (plan 0010 task
//! 4802); this is the shell making it so, in the shape `emit`/`effect_ack`
//! already has. One record, one fsync, one atomic outcome — fold derives the
//! child's whole existence from that record, which is why composition needs
//! no group-commit concept.
//!
//! Plan 0010 task 4901.

use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, child_instance_id, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::{InstanceState, InvokeStatus, Status};
use fsm_core::record::RecordKind;
use fsm_core::step::{DONE_INVOKE_PREFIX, Outcome, create, deliver_generated};

use crate::store::{ErrorObj, Store};

impl Store {
    pub fn invoke_child(
        &mut self,
        parent_id: &str,
        slot: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.invoke_child_on(&mut crate::clock::GlobalClock, parent_id, slot, request_id)
    }

    pub fn invoke_child_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        parent_id: &str,
        slot: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(replay) = self.claim_request(request_id, Self::fp_invoke(parent_id, slot))? {
            return replay;
        }
        let parent = self.state.instances.get(parent_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", parent_id).request_id(request_id)
        })?;
        let Some(invocation) = parent.invocations.get(slot) else {
            return Err(self.refuse_slot(parent_id, slot, request_id, "no such invocation slot"));
        };
        if invocation.status != InvokeStatus::Pending {
            let held = invocation.status.as_str();
            return Err(self.refuse_slot(
                parent_id,
                slot,
                request_id,
                &format!("slot is {held}, not pending"),
            ));
        }
        let child_machine_digest = invocation.child_machine_id.clone();
        let overrides = invocation.overrides.clone();
        let child_id = child_instance_id(parent_id, slot);
        let child_mid = self
            .state
            .machines
            .keys()
            .find(|machine_id| {
                fsm_core::hashes::digest_of(machine_id) == Some(child_machine_digest.as_str())
            })
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/machine_not_found", child_machine_digest.clone())
                    .request_id(request_id)
                    .hint("define the child machine before invoking it")
            })?;
        let child_machine = &self.state.machines[&child_mid];
        let commit_ts = clock.now_ms();
        // A failed child creation fails the whole operation and journals
        // nothing, mirroring SPEC's rule that `run/create_failed` is
        // unjournaled: the slot stays `Pending` and the caller may correct
        // the definition and retry under the same key.
        let created = create(
            &child_machine.compiled,
            &child_machine.tree,
            &overrides,
            commit_ts,
        )
        .map_err(|rejection| {
            let inner = ErrorObj::from_rejection(&rejection);
            ErrorObj::new(
                "run/invoke_create_failed",
                format!(
                    "creating the child for slot {slot} failed: {}",
                    inner.message
                ),
            )
            .hint(inner.hint.clone())
            .details(inner.details.clone())
            .request_id(request_id)
        })?;
        let child = InstanceState {
            status: created.status_after,
            configuration: created.configuration_after.clone(),
            ctx: created.ctx_after.clone(),
            history: created.history_after.clone(),
            deadlines: created.deadlines_after.clone(),
            pending: created
                .effects
                .iter()
                .map(|effect| format!("{child_id}/0/{}", effect.k))
                .collect(),
            invocations: created.invocations_after.clone(),
            signals: BTreeMap::new(),
        };
        let seq = self.journal.last_seq + 1;
        let mut parent_after = self.state.instances[parent_id].clone();
        parent_after
            .invocations
            .get_mut(slot)
            .expect("the slot was Pending a moment ago")
            .status = InvokeStatus::Running;
        let parent_mid = self
            .state
            .instance_machines
            .get(parent_id)
            .cloned()
            .unwrap_or_default();
        let mut body = BTreeMap::new();
        body.insert(
            "parent_instance_id".into(),
            Value::Str(parent_id.to_string()),
        );
        body.insert("slot".into(), Value::Str(slot.to_string()));
        body.insert("child_instance_id".into(), Value::Str(child_id.clone()));
        body.insert("child_machine_id".into(), Value::Str(child_mid.clone()));
        body.insert(
            "overrides".into(),
            Value::Obj(
                overrides
                    .iter()
                    .map(|(name, value)| (name.clone(), Value::Str(value.canonical_string())))
                    .collect(),
            ),
        );
        body.insert("request_id".into(), Value::Str(request_id.to_string()));
        body.insert(
            "state_hash".into(),
            Value::Str(state_hash(&parent_mid, parent_id, seq, &parent_after)),
        );
        body.insert(
            "child_state_hash".into(),
            Value::Str(state_hash(&child_mid, &child_id, seq, &child)),
        );
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        let record =
            self.append_at_with_root(RecordKind::InstanceInvoked, Value::Obj(body), commit_ts)?;
        self.state.instances.insert(parent_id.into(), parent_after);
        self.state.instances.insert(child_id.clone(), child);
        self.state
            .instance_machines
            .insert(child_id.clone(), child_mid.clone());
        for id in [parent_id, child_id.as_str()] {
            self.history.entry(id.into()).or_default().push(record.seq);
        }
        self.note_record(&record);
        let response = invoked_response(
            parent_id, slot, &child_id, &child_mid, request_id, record.seq, false,
        );
        self.commit_dedup(request_id, response.clone(), record.seq);
        self.finish_commit();
        Ok(response)
    }

    /// A slot that is not `Pending` is refused the way `ack_effect` refuses a
    /// settled effect: a journaled `request_rejected` claims the key, so the
    /// retry replays the same benign refusal instead of acting late.
    fn refuse_slot(
        &mut self,
        parent_id: &str,
        slot: &str,
        request_id: &str,
        message: &str,
    ) -> ErrorObj {
        let slots: Vec<Value> = self
            .state
            .instances
            .get(parent_id)
            .map(|parent| {
                parent
                    .invocations
                    .iter()
                    .map(|(id, invocation)| {
                        Value::Obj(BTreeMap::from([
                            ("slot".into(), Value::Str(id.clone())),
                            (
                                "status".into(),
                                Value::Str(invocation.status.as_str().into()),
                            ),
                        ]))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let details = Value::Obj(BTreeMap::from([
            ("slots".into(), Value::Arr(slots.clone())),
            ("request_id".into(), Value::Str(request_id.into())),
        ]));
        let error = ErrorObj::new("req/invoke_slot_state", message)
            .hint("invoke a slot that is pending; a running slot is already enacted and a returned one is finished")
            .details(details.clone())
            .request_id(request_id);
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(parent_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("code".into(), Value::Str("req/invoke_slot_state".into()));
        body.insert("message".into(), Value::Str(message.into()));
        body.insert("hint".into(), Value::Str(error.hint.clone()));
        body.insert("details".into(), details);
        body.insert("operation".into(), Value::Str("invoke".into()));
        body.insert("slot".into(), Value::Str(slot.into()));
        if let Some(parent) = self.state.instances.get(parent_id) {
            let mid = self
                .state
                .instance_machines
                .get(parent_id)
                .cloned()
                .unwrap_or_default();
            body.insert(
                "state_hash".into(),
                Value::Str(state_hash(
                    &mid,
                    parent_id,
                    self.journal.last_seq + 1,
                    parent,
                )),
            );
            body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        }
        let Ok(record) = self.append_rec(
            RecordKind::RequestRejected,
            Value::Obj(body),
            &mut crate::clock::GlobalClock,
        ) else {
            return error;
        };
        self.note_record(&record);
        self.last_errors.insert(request_id.into(), error.clone());
        let claimed = self.claimed_slot(record.seq);
        self.state.dedup.insert(request_id.into(), claimed);
        self.finish_commit();
        error
    }
}

/// The response an enacted invocation returns, warm or replayed.
pub(crate) fn invoked_response(
    parent_id: &str,
    slot: &str,
    child_id: &str,
    child_machine_id: &str,
    request_id: &str,
    seq: u64,
    duplicate: bool,
) -> Value {
    Value::Obj(BTreeMap::from([
        ("ok".into(), Value::Str("true".into())),
        ("invoked".into(), Value::Bool(true)),
        ("parent_instance_id".into(), Value::Str(parent_id.into())),
        ("slot".into(), Value::Str(slot.into())),
        ("child_instance_id".into(), Value::Str(child_id.into())),
        (
            "child_machine_id".into(),
            Value::Str(child_machine_id.into()),
        ),
        ("status".into(), Value::Str("running".into())),
        ("request_id".into(), Value::Str(request_id.into())),
        ("seq".into(), Value::Num(seq.to_string())),
        ("duplicate".into(), Value::Bool(duplicate)),
    ]))
}

impl Store {
    pub fn invocation_return(
        &mut self,
        parent_id: &str,
        slot: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.invocation_return_on(&mut crate::clock::GlobalClock, parent_id, slot, request_id)
    }

    /// Deliver a settled child's result to its parent.
    ///
    /// A separate journaled operation for the same reason an effect ack is:
    /// a state change caused by something outside the instance must be a
    /// record somebody can point at, not a side effect of reading.
    pub fn invocation_return_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        parent_id: &str,
        slot: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(replay) = self.claim_request(request_id, Self::fp_return(parent_id, slot))? {
            return replay;
        }
        let parent = self.state.instances.get(parent_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", parent_id).request_id(request_id)
        })?;
        let Some(invocation) = parent.invocations.get(slot) else {
            return Err(self.refuse_slot(parent_id, slot, request_id, "no such invocation slot"));
        };
        if invocation.status != InvokeStatus::Running {
            let held = invocation.status.as_str();
            return Err(self.refuse_slot(
                parent_id,
                slot,
                request_id,
                &format!("slot is {held}, not running: only an enacted slot can return"),
            ));
        }
        let child_id = child_instance_id(parent_id, slot);
        let Some(child) = self.state.instances.get(&child_id) else {
            return Err(self.refuse_slot(
                parent_id,
                slot,
                request_id,
                "the child instance is missing",
            ));
        };
        let outcome = match child.status {
            Status::Completed => "completed",
            Status::Cancelled => "cancelled",
            Status::Running => {
                return Err(self.refuse_slot(
                    parent_id,
                    slot,
                    request_id,
                    "the child is still running: cancel it or wait for it to complete",
                ));
            }
        };
        let parent_mid = self
            .state
            .instance_machines
            .get(parent_id)
            .cloned()
            .unwrap_or_default();
        let machine = self
            .state
            .machines
            .get(&parent_mid)
            .ok_or_else(|| ErrorObj::new("req/machine_not_found", parent_mid.clone()))?;
        // The projection is read out of the child's final context. A
        // cancelled child is skipped: the parent's definition decides what
        // cancellation means, through a declared field, rather than the
        // engine deciding for it. `outcome` stays out of the payload —
        // injecting an engine-chosen field would break the shape the child's
        // declarations promised.
        let returns = machine
            .compiled
            .spec
            .walk_states()
            .into_iter()
            .find_map(|(node, _)| {
                node.invokes
                    .iter()
                    .find(|invoke| invoke.id == slot)
                    .map(|invoke| invoke.returns.clone())
            })
            .unwrap_or_default();
        let mut payload = BTreeMap::new();
        if outcome == "completed" {
            for (field, child_var) in &returns {
                if let Some(value) = child.ctx.get(child_var) {
                    payload.insert(field.clone(), value.clone());
                }
            }
        }
        let commit_ts = clock.now_ms();
        let mut budget = fsm_core::expr::eval::Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
        let event = format!("{DONE_INVOKE_PREFIX}{slot}");
        let applied = match deliver_generated(
            &machine.compiled,
            &machine.tree,
            parent,
            &event,
            &payload,
            commit_ts,
            &mut budget,
        ) {
            Outcome::Applied(applied) => applied,
            Outcome::Ignored => {
                return Err(ErrorObj::new(
                    "req/invoke_slot_state",
                    "the parent ignores its own generated event",
                )
                .request_id(request_id));
            }
            Outcome::Rejected(rejection) => {
                // Nothing is handling it, or the handler failed. A discard is
                // plan 0009's rule and applies; a real failure is the
                // parent's, and the slot stays `Running` for a retry.
                if rejection.code != "run/unhandled" {
                    return Err(ErrorObj::from_rejection(&rejection).request_id(request_id));
                }
                unhandled_applied(parent)
            }
        };
        let seq = self.journal.last_seq + 1;
        let mut post = InstanceState {
            status: applied.status_after,
            configuration: applied.configuration_after.clone(),
            ctx: applied.ctx_after.clone(),
            history: applied.history_after.clone(),
            deadlines: applied.deadlines_after.clone(),
            pending: parent.pending.clone(),
            invocations: applied.invocations_after.clone(),
            signals: parent.signals.clone(),
        };
        post.pending.extend(
            applied
                .effects
                .iter()
                .map(|effect| format!("{parent_id}/{seq}/{}", effect.k)),
        );
        if let Some(invocation) = post.invocations.get_mut(slot) {
            invocation.status = InvokeStatus::Returned;
        }
        let mut body = BTreeMap::new();
        body.insert(
            "parent_instance_id".into(),
            Value::Str(parent_id.to_string()),
        );
        body.insert("slot".into(), Value::Str(slot.to_string()));
        body.insert("child_instance_id".into(), Value::Str(child_id.clone()));
        body.insert("outcome".into(), Value::Str(outcome.into()));
        body.insert(
            "payload".into(),
            Value::Obj(
                payload
                    .iter()
                    .map(|(name, value)| (name.clone(), Value::Str(value.canonical_string())))
                    .collect(),
            ),
        );
        body.insert("request_id".into(), Value::Str(request_id.to_string()));
        body.insert(
            "state_hash".into(),
            Value::Str(state_hash(&parent_mid, parent_id, seq, &post)),
        );
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(microsteps) = fsm_core::record::microsteps_value(&applied.trace.microsteps) {
            body.insert("microsteps".into(), microsteps);
        }
        let record =
            self.append_at_with_root(RecordKind::InvocationReturned, Value::Obj(body), commit_ts)?;
        self.state.instances.insert(parent_id.into(), post);
        for id in [parent_id, child_id.as_str()] {
            self.history.entry(id.into()).or_default().push(record.seq);
        }
        self.note_record(&record);
        let response = returned_response(
            parent_id, slot, &child_id, outcome, request_id, record.seq, false,
        );
        self.commit_dedup(request_id, response.clone(), record.seq);
        self.finish_commit();
        Ok(response)
    }
}

/// A parent with no handler for its own done event: plan 0009 discards the
/// event, so the macrostep is a no-op over the parent's state and the record
/// still commits with the slot `Returned`.
fn unhandled_applied(parent: &InstanceState) -> fsm_core::step::Applied {
    fsm_core::step::Applied {
        configuration_after: parent.configuration.clone(),
        ctx_after: parent.ctx.clone(),
        history_after: parent.history.clone(),
        deadlines_after: parent.deadlines.clone(),
        invocations_after: parent.invocations.clone(),
        cancelled_children: Vec::new(),
        effects: Vec::new(),
        monitor_flags: Vec::new(),
        status_after: parent.status,
        internal: true,
        region: None,
        source_state: String::new(),
        transition_idx: 0,
        exited: Vec::new(),
        entered: Vec::new(),
        trace: fsm_core::trace::DecisionTrace::default(),
    }
}

/// The response a returned invocation gives, warm or replayed.
pub(crate) fn returned_response(
    parent_id: &str,
    slot: &str,
    child_id: &str,
    outcome: &str,
    request_id: &str,
    seq: u64,
    duplicate: bool,
) -> Value {
    Value::Obj(BTreeMap::from([
        ("ok".into(), Value::Str("true".into())),
        ("returned".into(), Value::Bool(true)),
        ("parent_instance_id".into(), Value::Str(parent_id.into())),
        ("slot".into(), Value::Str(slot.into())),
        ("child_instance_id".into(), Value::Str(child_id.into())),
        ("outcome".into(), Value::Str(outcome.into())),
        ("status".into(), Value::Str("returned".into())),
        ("request_id".into(), Value::Str(request_id.into())),
        ("seq".into(), Value::Num(seq.to_string())),
        ("duplicate".into(), Value::Bool(duplicate)),
    ]))
}
