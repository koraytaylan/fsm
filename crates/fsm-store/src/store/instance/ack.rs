use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, state_hash};
use fsm_core::json::Value;
use fsm_core::record::RecordKind;

use crate::store::{ErrorObj, Store};

impl Store {
    pub fn ack_effect(
        &mut self,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        self.ack_effect_outcome(instance_id, effect_id, request_id, "ok", None)
    }

    pub fn ack_effect_outcome(
        &mut self,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
        outcome: &str,
        result: Option<Value>,
    ) -> Result<Value, ErrorObj> {
        self.ack_effect_outcome_on(
            &mut crate::clock::GlobalClock,
            instance_id,
            effect_id,
            request_id,
            outcome,
            result,
        )
    }

    pub fn ack_effect_outcome_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
        outcome: &str,
        result: Option<Value>,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(
            request_id,
            Self::fp_ack(instance_id, effect_id, outcome, result.as_ref()),
        )? {
            return r;
        }
        if let Some(v) = result.as_ref() {
            Self::check_journalled_size("result", v, request_id)?;
        }
        if outcome != "ok" && outcome != "failed" {
            return Err(
                ErrorObj::new("req/args_invalid", "outcome must be ok or failed")
                    .request_id(request_id),
            );
        }
        let inst = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
        })?;
        if !inst.pending.iter().any(|p| p == effect_id) {
            let listed = inst.pending.clone();
            let mut body = BTreeMap::new();
            body.insert("request_id".into(), Value::Str(request_id.into()));
            body.insert("instance_id".into(), Value::Str(instance_id.into()));
            body.insert("code".into(), Value::Str("req/field_unknown".into()));
            body.insert("message".into(), Value::Str("unknown effect id".into()));
            body.insert(
                "hint".into(),
                Value::Str("use an id from effects_pending".into()),
            );
            let mut det = BTreeMap::new();
            det.insert(
                "pending".into(),
                Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
            );
            det.insert("request_id".into(), Value::Str(request_id.into()));
            body.insert("details".into(), Value::Obj(det));
            body.insert("operation".into(), Value::Str("ack".into()));
            body.insert("effect_id".into(), Value::Str(effect_id.into()));
            let mid = self
                .state
                .instance_machines
                .get(instance_id)
                .cloned()
                .unwrap_or_default();
            let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, inst);
            body.insert("state_hash".into(), Value::Str(sh));
            body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
            let rec = self.append_rec(RecordKind::RequestRejected, Value::Obj(body), clock)?;
            self.note_record(&rec);
            let mut details = BTreeMap::new();
            details.insert(
                "pending".into(),
                Value::Arr(listed.into_iter().map(Value::Str).collect()),
            );
            let err = ErrorObj::new("req/field_unknown", "unknown effect id")
                .hint("use an id from effects_pending")
                .details(Value::Obj(details))
                .request_id(request_id);
            self.last_errors.insert(request_id.into(), err.clone());
            let slot = self.claimed_slot(rec.seq);
            self.state.dedup.insert(request_id.into(), slot);
            self.finish_commit();
            return Err(err);
        }
        let pending: Vec<String> = inst
            .pending
            .iter()
            .filter(|p| *p != effect_id)
            .cloned()
            .collect();
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .ok_or_else(|| {
                ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
            })?;
        let mut post = inst.clone();
        post.pending.clone_from(&pending);
        let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &post);
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("effect_id".into(), Value::Str(effect_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("outcome".into(), Value::Str(outcome.into()));
        body.insert("state_hash".into(), Value::Str(sh));
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(res) = result.clone() {
            body.insert("result".into(), res);
        }
        let rec = self.append_rec(RecordKind::EffectAcked, Value::Obj(body), clock)?;
        self.state.instances.insert(instance_id.into(), post);
        self.note_record(&rec);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        let mut m = BTreeMap::new();
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("acked".into(), Value::Bool(true));
        m.insert("instance_id".into(), Value::Str(instance_id.into()));
        m.insert("effect_id".into(), Value::Str(effect_id.into()));
        m.insert("outcome".into(), Value::Str(outcome.into()));
        m.insert("request_id".into(), Value::Str(request_id.into()));
        m.insert("duplicate".into(), Value::Bool(false));
        m.insert("seq".into(), Value::Num(rec.seq.to_string()));
        m.insert(
            "effects_pending".into(),
            Value::Arr(pending.into_iter().map(Value::Str).collect()),
        );
        if let Some(res) = result {
            m.insert("result".into(), res);
        }
        let resp = Value::Obj(m);
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
        Ok(resp)
    }
}
