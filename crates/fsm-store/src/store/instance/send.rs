use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::hashes::{STATE_FORMAT, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::{InstanceState, Status};
use fsm_core::record::{RecordKind, microsteps_value};
use fsm_core::step::{Outcome, step, validate_event};

use crate::store::reconstruct::{
    insert_configuration_fields, insert_transition_configuration_fields,
};
use crate::store::{ErrorObj, Store};

impl Store {
    pub fn send_event(
        &mut self,
        instance_id: &str,
        event: &str,
        mut payload: Value,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.send_event_stamp(
            instance_id,
            event,
            &mut payload,
            request_id,
            expect_seq,
            &[],
        )
    }

    pub fn send_event_stamp(
        &mut self,
        instance_id: &str,
        event: &str,
        payload: &mut Value,
        request_id: &str,
        expect_seq: Option<u64>,
        stamps: &[&str],
    ) -> Result<Value, ErrorObj> {
        self.send_event_stamp_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            event,
            payload,
            request_id,
            expect_seq,
            stamps,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_event_stamp_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        event: &str,
        payload: &mut Value,
        request_id: &str,
        expect_seq: Option<u64>,
        stamps: &[&str],
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        let request_fp = Self::fp_send(instance_id, event, payload);
        if let Some(r) = self.lookup_request(request_id, &request_fp)? {
            return r;
        }

        // Stamp a candidate rather than the caller's value so every
        // unjournaled rejection remains atomic. A single reservation supplies
        // every absent field and lets us enforce the cap against the exact
        // payload that will be journalled without advancing built-in clocks.
        let mut final_payload = payload.clone();
        let mut reserved_ts = None;
        if let Value::Obj(fields) = &mut final_payload {
            for field in stamps {
                if !fields.contains_key(*field) {
                    let timestamp = *reserved_ts.get_or_insert_with(|| clock.reserve_ms());
                    fields.insert((*field).into(), Value::Str(timestamp.to_string()));
                }
            }
        }
        Self::check_journalled_size("payload", &final_payload, request_id)?;
        if let Some(exp) = expect_seq {
            if exp != self.journal.last_seq {
                let mut d = BTreeMap::new();
                d.insert(
                    "current_seq".into(),
                    Value::Num(self.journal.last_seq.to_string()),
                );
                return Err(ErrorObj::new(
                    "req/seq_mismatch",
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .hint(
                    "re-read the instance, then retry with the same request_id and the current seq",
                )
                .details(Value::Obj(d))
                .request_id(request_id)
                .with_store_catalog(self));
            }
        }
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id)
                    .request_id(request_id)
                    .hint("use a known instance id from details.known_instances")
                    .with_store_catalog(self)
            })?;
        let m = self.state.machines.get(&mid).ok_or_else(|| {
            ErrorObj::new("req/machine_not_found", &mid)
                .request_id(request_id)
                .with_store_catalog(self)
        })?;
        let response_tree = m.tree.clone();
        let inst = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id)
                .request_id(request_id)
                .hint("use a known instance id from details.known_instances")
                .with_store_catalog(self)
        })?;
        let from_configuration = inst.configuration.clone();
        // SPEC step ordering status-gates before event validation. Request
        // shape errors remain unjournaled only while the instance is running;
        // completed/cancelled outcomes depend on durable instance state and
        // must flow through `step` so they are journaled and replayable.
        if inst.status == Status::Running {
            if let Err(r) = validate_event(&m.compiled, event, &final_payload) {
                return Err(ErrorObj::from_rejection(&r).request_id(request_id));
            }
        }
        let commit_ts = reserved_ts
            .map(|timestamp| clock.commit_reserved_ms(timestamp))
            .unwrap_or_else(|| clock.now_ms());
        self.pending_fp = Some(request_fp);
        *payload = final_payload;
        // A macrostep may run the trigger, MAX_MICROSTEPS reactions, and the
        // closing scan; the enabled-event scan in `instance_view` keeps the
        // standard budget because it selects and never applies a pipeline.
        let mut bud = Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
        let out = step(
            &m.compiled,
            &m.tree,
            inst,
            event,
            payload,
            commit_ts,
            &mut bud,
        );
        match out {
            Outcome::Applied(a) => {
                let mut pending = inst.pending.clone();
                pending.extend(
                    a.effects
                        .iter()
                        .map(|e| format!("{instance_id}/{}/{}", self.journal.last_seq + 1, e.k)),
                );
                let new = InstanceState {
                    status: a.status_after,
                    configuration: a.configuration_after.clone(),
                    ctx: a.ctx_after.clone(),
                    history: a.history_after.clone(),
                    deadlines: a.deadlines_after.clone(),
                    pending,
                    invocations: a.invocations_after.clone(),
                    signals: BTreeMap::new(),
                };
                let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &new);
                let mut body = BTreeMap::new();
                body.insert("instance_id".into(), Value::Str(instance_id.into()));
                body.insert("event".into(), Value::Str(event.into()));
                body.insert("payload".into(), payload.clone());
                body.insert("request_id".into(), Value::Str(request_id.into()));
                body.insert("state_hash".into(), Value::Str(sh.clone()));
                body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
                body.insert("source_state".into(), Value::Str(a.source_state.clone()));
                body.insert(
                    "exited".into(),
                    Value::Arr(a.exited.iter().cloned().map(Value::Str).collect()),
                );
                body.insert(
                    "entered".into(),
                    Value::Arr(a.entered.iter().cloned().map(Value::Str).collect()),
                );
                if let Some(microsteps) = microsteps_value(&a.trace.microsteps) {
                    body.insert("microsteps".into(), microsteps);
                }
                let rec = self.append_at_with_root(
                    RecordKind::EventApplied,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.state.instances.insert(instance_id.into(), new);
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(rec.seq);
                self.note_record(&rec);
                let mut resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(o) = &mut resp {
                    o.insert("applied".into(), Value::Bool(true));
                    o.insert("ok".into(), Value::Str("true".into()));
                    insert_configuration_fields(o, &response_tree, &a.configuration_after);
                    let mut tr = BTreeMap::new();
                    tr.insert("source_state".into(), Value::Str(a.source_state.clone()));
                    tr.insert(
                        "transition_idx".into(),
                        Value::Num(a.transition_idx.to_string()),
                    );
                    tr.insert("internal".into(), Value::Bool(a.internal));
                    if let Some(region) = &a.region {
                        tr.insert("region".into(), Value::Str(region.clone()));
                    }
                    insert_transition_configuration_fields(
                        &mut tr,
                        &from_configuration,
                        &a.configuration_after,
                    );
                    tr.insert(
                        "exited".into(),
                        Value::Arr(a.exited.iter().cloned().map(Value::Str).collect()),
                    );
                    tr.insert(
                        "entered".into(),
                        Value::Arr(a.entered.iter().cloned().map(Value::Str).collect()),
                    );
                    o.insert("transition".into(), Value::Obj(tr));
                    o.insert("trace".into(), a.trace.to_value());
                    o.insert(
                        "monitor_flags".into(),
                        Value::Arr(a.monitor_flags.iter().cloned().map(Value::Str).collect()),
                    );
                }
                self.commit_dedup(request_id, resp.clone(), rec.seq);
                self.finish_commit();
                Ok(resp)
            }
            Outcome::Rejected(r) => {
                let inst = self.state.instances.get(instance_id).unwrap().clone();
                let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &inst);
                let mut body = BTreeMap::new();
                body.insert("instance_id".into(), Value::Str(instance_id.into()));
                body.insert("request_id".into(), Value::Str(request_id.into()));
                body.insert("event".into(), Value::Str(event.into()));
                body.insert("payload".into(), payload.clone());
                body.insert("state_hash".into(), Value::Str(sh));
                body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
                body.insert("code".into(), Value::Str(r.code.into()));
                body.insert("message".into(), Value::Str(r.message.clone()));
                body.insert("hint".into(), Value::Str(r.hint.clone()));
                let mut err = ErrorObj::from_rejection(&r);
                if let Ok(view) = self.instance_view(instance_id, Some(request_id), None) {
                    if let Value::Obj(v) = view {
                        if let Some(en) = v.get("enabled_events") {
                            if let Value::Obj(d) = &mut err.details {
                                d.insert("enabled_events".into(), en.clone());
                            }
                        }
                    }
                }
                err.details = match err.details {
                    Value::Obj(mut d) => {
                        d.insert("trace".into(), r.trace.to_value());
                        Value::Obj(d)
                    }
                    other => other,
                };
                err = err.request_id(request_id);
                body.insert("details".into(), err.details.clone());
                if let Value::Obj(d) = &mut err.details {
                    d.insert("machine_id".into(), Value::Str(mid.clone()));
                    d.insert("instance_id".into(), Value::Str(instance_id.into()));
                }
                if let Some((s, e)) = err.span {
                    let mut sp = BTreeMap::new();
                    sp.insert("start".into(), Value::Num(s.to_string()));
                    sp.insert("end".into(), Value::Num(e.to_string()));
                    body.insert("span".into(), Value::Obj(sp));
                }
                let rec = self.append_at_with_root(
                    RecordKind::EventRejected,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.history
                    .entry(instance_id.into())
                    .or_default()
                    .push(rec.seq);
                self.note_record(&rec);
                let slot = self.claimed_slot(rec.seq);
                self.state.dedup.insert(request_id.into(), slot);
                self.last_errors.insert(request_id.into(), err.clone());
                self.finish_commit();
                Err(err)
            }
            Outcome::Ignored => {
                let inst = self.state.instances.get(instance_id).unwrap().clone();
                let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &inst);
                let mut body = BTreeMap::new();
                body.insert("instance_id".into(), Value::Str(instance_id.into()));
                body.insert("request_id".into(), Value::Str(request_id.into()));
                body.insert("event".into(), Value::Str(event.into()));
                body.insert("payload".into(), payload.clone());
                body.insert("state_hash".into(), Value::Str(sh));
                body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
                let rec = self.append_at_with_root(
                    RecordKind::EventIgnored,
                    Value::Obj(body),
                    commit_ts,
                )?;
                self.note_record(&rec);
                let mut resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
                if let Value::Obj(o) = &mut resp {
                    o.insert("ok".into(), Value::Str("true".into()));
                    o.insert("ignored".into(), Value::Bool(true));
                    o.insert("applied".into(), Value::Bool(false));
                    o.insert("seq".into(), Value::Num(rec.seq.to_string()));
                    o.insert("monitor_flags".into(), Value::Arr(vec![]));
                    o.insert("trace".into(), Value::Obj(BTreeMap::new()));
                    o.insert(
                        "transition".into(),
                        Value::Obj({
                            let mut transition = BTreeMap::from([
                                ("transition_idx".into(), Value::Num("-1".into())),
                                ("internal".into(), Value::Bool(false)),
                                ("exited".into(), Value::Arr(vec![])),
                                ("entered".into(), Value::Arr(vec![])),
                            ]);
                            if let Some(leaf) = inst.configuration.sequential_leaf() {
                                transition
                                    .insert("source_state".into(), Value::Str(leaf.to_string()));
                            }
                            insert_transition_configuration_fields(
                                &mut transition,
                                &inst.configuration,
                                &inst.configuration,
                            );
                            transition
                        }),
                    );
                }
                self.commit_dedup(request_id, resp.clone(), rec.seq);
                self.finish_commit();
                Ok(resp)
            }
        }
    }
}
