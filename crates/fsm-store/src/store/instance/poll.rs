use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::hashes::{STATE_FORMAT, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::InstanceState;
use fsm_core::record::RecordKind;
use fsm_core::step::{DeadlineOutcome, poll_deadline};

use crate::store::reconstruct::insert_transition_configuration_fields;
use crate::store::{ErrorObj, Store};

impl Store {
    /// Poll and apply at most one due deadline using the process clock.
    pub fn poll_instance_deadline(
        &mut self,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.poll_instance_deadline_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            request_id,
            expect_seq,
        )
    }

    /// Injected-clock form of [`Store::poll_instance_deadline`].
    pub fn poll_instance_deadline_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(result) = self.claim_request(request_id, Self::fp_poll_deadline(instance_id))? {
            return result;
        }
        if let Some(expected) = expect_seq {
            if expected != self.journal.last_seq {
                return Err(ErrorObj::new(
                    "req/seq_mismatch",
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .hint(
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .details(Value::Obj(BTreeMap::from([(
                    "current_seq".into(),
                    Value::Num(self.journal.last_seq.to_string()),
                )])))
                .request_id(request_id));
            }
        }
        let machine_id = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id)
                    .request_id(request_id)
                    .with_store_catalog(self)
            })?;
        let machine = self.state.machines.get(&machine_id).ok_or_else(|| {
            ErrorObj::new("req/machine_not_found", &machine_id).request_id(request_id)
        })?;
        let instance = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
        })?;
        let before_configuration = instance.configuration.clone();
        let commit_ts = clock.now_ms();
        let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        let outcome = poll_deadline(
            &machine.compiled,
            &machine.tree,
            instance,
            commit_ts,
            &mut budget,
        );
        match outcome {
            DeadlineOutcome::Applied(applied) => {
                let transition = applied.transition;
                let mut pending = instance.pending.clone();
                pending.extend(transition.effects.iter().map(|effect| {
                    format!("{instance_id}/{}/{}", self.journal.last_seq + 1, effect.k)
                }));
                let next_state = InstanceState {
                    status: transition.status_after,
                    configuration: transition.configuration_after.clone(),
                    ctx: transition.ctx_after.clone(),
                    history: transition.history_after.clone(),
                    deadlines: transition.deadlines_after.clone(),
                    pending,
                };
                let state_hash = state_hash(
                    &machine_id,
                    instance_id,
                    self.journal.last_seq + 1,
                    &next_state,
                );
                let body = Value::Obj(BTreeMap::from([
                    ("instance_id".into(), Value::Str(instance_id.into())),
                    ("request_id".into(), Value::Str(request_id.into())),
                    ("deadline".into(), Value::Str(applied.deadline.name.clone())),
                    (
                        "deadline_idx".into(),
                        Value::Num(applied.deadline.deadline_idx.to_string()),
                    ),
                    (
                        "due_ms".into(),
                        Value::Num(applied.deadline.due_ms.to_string()),
                    ),
                    ("state_hash".into(), Value::Str(state_hash)),
                    ("state_format".into(), Value::Str(STATE_FORMAT.into())),
                    (
                        "source_state".into(),
                        Value::Str(transition.source_state.clone()),
                    ),
                    (
                        "exited".into(),
                        Value::Arr(transition.exited.iter().cloned().map(Value::Str).collect()),
                    ),
                    (
                        "entered".into(),
                        Value::Arr(transition.entered.iter().cloned().map(Value::Str).collect()),
                    ),
                ]));
                let record =
                    self.append_at_with_root(RecordKind::DeadlineApplied, body, commit_ts)?;
                self.state.instances.insert(instance_id.into(), next_state);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
                self.note_record(&record);
                let mut response =
                    self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(output) = &mut response {
                    output.insert("deadline_applied".into(), Value::Bool(true));
                    output.insert("deadline_not_due".into(), Value::Bool(false));
                    output.insert("deadline".into(), Value::Str(applied.deadline.name));
                    output.insert(
                        "deadline_idx".into(),
                        Value::Num(applied.deadline.deadline_idx.to_string()),
                    );
                    output.insert(
                        "due_ms".into(),
                        Value::Str(applied.deadline.due_ms.to_string()),
                    );
                    let mut transition_value = BTreeMap::from([
                        (
                            "source_state".into(),
                            Value::Str(transition.source_state.clone()),
                        ),
                        (
                            "deadline_idx".into(),
                            Value::Num(transition.transition_idx.to_string()),
                        ),
                        ("internal".into(), Value::Bool(false)),
                        (
                            "exited".into(),
                            Value::Arr(transition.exited.iter().cloned().map(Value::Str).collect()),
                        ),
                        (
                            "entered".into(),
                            Value::Arr(
                                transition.entered.iter().cloned().map(Value::Str).collect(),
                            ),
                        ),
                    ]);
                    if let Some(region) = &transition.region {
                        transition_value.insert("region".into(), Value::Str(region.clone()));
                    }
                    insert_transition_configuration_fields(
                        &mut transition_value,
                        &before_configuration,
                        &transition.configuration_after,
                    );
                    output.insert("transition".into(), Value::Obj(transition_value));
                    output.insert("trace".into(), transition.trace.to_value());
                    output.insert(
                        "monitor_flags".into(),
                        Value::Arr(
                            transition
                                .monitor_flags
                                .iter()
                                .cloned()
                                .map(Value::Str)
                                .collect(),
                        ),
                    );
                }
                self.commit_dedup(request_id, response.clone(), record.seq);
                self.finish_commit();
                Ok(response)
            }
            DeadlineOutcome::NotDue { next } => {
                let unchanged = instance.clone();
                let state_hash = state_hash(
                    &machine_id,
                    instance_id,
                    self.journal.last_seq + 1,
                    &unchanged,
                );
                let mut body = BTreeMap::from([
                    ("instance_id".into(), Value::Str(instance_id.into())),
                    ("request_id".into(), Value::Str(request_id.into())),
                    ("state_hash".into(), Value::Str(state_hash)),
                    ("state_format".into(), Value::Str(STATE_FORMAT.into())),
                ]);
                if let Some(next) = &next {
                    body.insert("next_deadline".into(), Value::Str(next.name.clone()));
                    body.insert(
                        "next_deadline_idx".into(),
                        Value::Num(next.deadline_idx.to_string()),
                    );
                    body.insert("next_due_ms".into(), Value::Num(next.due_ms.to_string()));
                }
                let record = self.append_at_with_root(
                    RecordKind::DeadlineNotDue,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.note_record(&record);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
                let mut response =
                    self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(output) = &mut response {
                    output.insert("deadline_applied".into(), Value::Bool(false));
                    output.insert("deadline_not_due".into(), Value::Bool(true));
                    if let Some(next) = next {
                        output.insert("next_deadline".into(), Value::Str(next.name));
                        output.insert(
                            "next_deadline_idx".into(),
                            Value::Num(next.deadline_idx.to_string()),
                        );
                        output.insert("next_due_ms".into(), Value::Str(next.due_ms.to_string()));
                    }
                }
                self.commit_dedup(request_id, response.clone(), record.seq);
                self.finish_commit();
                Ok(response)
            }
            DeadlineOutcome::Rejected(rejected) => {
                let rejection = rejected.rejection;
                let unchanged = instance.clone();
                let state_hash = state_hash(
                    &machine_id,
                    instance_id,
                    self.journal.last_seq + 1,
                    &unchanged,
                );
                let mut error = ErrorObj::from_rejection(&rejection).request_id(request_id);
                let mut body = BTreeMap::from([
                    ("instance_id".into(), Value::Str(instance_id.into())),
                    ("request_id".into(), Value::Str(request_id.into())),
                    ("state_hash".into(), Value::Str(state_hash)),
                    ("state_format".into(), Value::Str(STATE_FORMAT.into())),
                    ("code".into(), Value::Str(rejection.code.into())),
                    ("message".into(), Value::Str(rejection.message.clone())),
                    ("hint".into(), Value::Str(rejection.hint.clone())),
                    ("details".into(), error.details.clone()),
                ]);
                let kind = if let Some(deadline) = rejected.deadline {
                    body.insert("deadline".into(), Value::Str(deadline.name));
                    body.insert(
                        "deadline_idx".into(),
                        Value::Num(deadline.deadline_idx.to_string()),
                    );
                    body.insert("due_ms".into(), Value::Num(deadline.due_ms.to_string()));
                    RecordKind::DeadlineRejected
                } else {
                    body.insert("operation".into(), Value::Str("poll_deadline".into()));
                    RecordKind::RequestRejected
                };
                if let Some((start, end)) = rejection.span {
                    body.insert(
                        "span".into(),
                        Value::Obj(BTreeMap::from([
                            ("start".into(), Value::Num(start.to_string())),
                            ("end".into(), Value::Num(end.to_string())),
                        ])),
                    );
                }
                if let Value::Obj(details) = &mut error.details {
                    details.insert("machine_id".into(), Value::Str(machine_id));
                    details.insert("instance_id".into(), Value::Str(instance_id.into()));
                }
                let record = self.append_at_with_root(kind, Value::Obj(body), commit_ts)?;
                self.note_record(&record);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(record.seq);
                let slot = self.claimed_slot(record.seq);
                self.state.dedup.insert(request_id.into(), slot);
                self.last_errors.insert(request_id.into(), error.clone());
                self.finish_commit();
                Err(error)
            }
        }
    }
}
