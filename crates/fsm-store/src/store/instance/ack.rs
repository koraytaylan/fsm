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

    /// Record one failed attempt at an effect, leaving it pending.
    ///
    /// A retry counter in process memory is lost by exactly the restart it
    /// exists to survive, and two executors holding their own counters
    /// would disagree about how many attempts had happened. So an attempt
    /// is a record: the count is derived from the journal, a fresh process
    /// reaches the conclusion its killed predecessor did, and the audit
    /// trail can answer "how many times did we try, and when" without
    /// inference.
    pub fn attempt_effect_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
        attempt: u64,
        result: Option<Value>,
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(replay) = self.claim_request(
            request_id,
            Self::fp_attempt(instance_id, effect_id, attempt, result.as_ref()),
        )? {
            return replay;
        }
        if let Some(value) = result.as_ref() {
            Self::check_journalled_size("result", value, request_id)?;
        }
        let instance = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("req/instance_not_found", instance_id).request_id(request_id)
        })?;
        // An attempt against an effect that is not pending is the same
        // mistake as acking one: the caller is talking about something that
        // is over.
        if !instance.pending.iter().any(|pending| pending == effect_id) {
            return Err(self.reject_unknown_effect(clock, instance_id, effect_id, request_id));
        }
        // Strictly `last + 1`. A gap makes the derived count unreliable, and
        // an unreliable count is worse than no retry at all: it would make a
        // policy of "three tries" mean something different after a crash.
        let attempts = self.attempts_for(instance_id, effect_id);
        if attempt != attempts + 1 {
            return Err(ErrorObj::new(
                "req/args_invalid",
                format!("attempt {attempt} follows {attempts}"),
            )
            .hint(format!(
                "the next attempt for this effect is {}",
                attempts + 1
            ))
            .request_id(request_id));
        }

        let machine_id = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        // The instance is unchanged, so the hash is over the state as it
        // stands: an attempt moves nothing.
        let state_hash = state_hash(
            &machine_id,
            instance_id,
            self.journal.last_seq + 1,
            instance,
        );
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("effect_id".into(), Value::Str(effect_id.into()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("attempt".into(), Value::Num(attempt.to_string()));
        // Always failed: a successful attempt is an ack, which is why
        // counting these gives the failed count directly.
        body.insert("outcome".into(), Value::Str("failed".into()));
        body.insert("state_hash".into(), Value::Str(state_hash));
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(value) = result.clone() {
            body.insert("result".into(), value);
        }
        let record = self.append_rec(RecordKind::EffectAttempted, Value::Obj(body), clock)?;
        self.note_record(&record);
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(record.seq);
        let response = attempted_response(
            instance_id,
            effect_id,
            request_id,
            attempt,
            result,
            record.seq,
            false,
        );
        self.commit_dedup(request_id, response.clone(), record.seq);
        self.finish_commit();
        Ok(response)
    }

    /// Journal the refusal an effect nobody is waiting on earns, claim the
    /// key, and hand back the error.
    ///
    /// One writer for both operations: an attempt and an ack are refused for
    /// the same reason and must be refused in the same words, or a caller
    /// would have to learn two vocabularies for one mistake.
    fn reject_unknown_effect(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        instance_id: &str,
        effect_id: &str,
        request_id: &str,
    ) -> ErrorObj {
        let pending = self
            .state
            .instances
            .get(instance_id)
            .map(|instance| instance.pending.clone())
            .unwrap_or_default();
        let mut body = BTreeMap::new();
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("code".into(), Value::Str("req/field_unknown".into()));
        body.insert("message".into(), Value::Str("unknown effect id".into()));
        body.insert(
            "hint".into(),
            Value::Str("use an id from effects_pending".into()),
        );
        let mut details = BTreeMap::new();
        details.insert(
            "pending".into(),
            Value::Arr(pending.iter().cloned().map(Value::Str).collect()),
        );
        details.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("details".into(), Value::Obj(details.clone()));
        body.insert("operation".into(), Value::Str("ack".into()));
        body.insert("effect_id".into(), Value::Str(effect_id.into()));
        let machine_id = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        if let Some(instance) = self.state.instances.get(instance_id) {
            let hash = state_hash(
                &machine_id,
                instance_id,
                self.journal.last_seq + 1,
                instance,
            );
            body.insert("state_hash".into(), Value::Str(hash));
            body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        }
        let error = ErrorObj::new("req/field_unknown", "unknown effect id")
            .hint("use an id from effects_pending")
            .details(Value::Obj(BTreeMap::from([(
                "pending".to_string(),
                Value::Arr(pending.into_iter().map(Value::Str).collect()),
            )])))
            .request_id(request_id);
        let Ok(record) = self.append_rec(RecordKind::RequestRejected, Value::Obj(body), clock)
        else {
            return error;
        };
        self.note_record(&record);
        self.last_errors.insert(request_id.into(), error.clone());
        let slot = self.claimed_slot(record.seq);
        self.state.dedup.insert(request_id.into(), slot);
        self.finish_commit();
        error
    }

    /// How many failed attempts this effect has already had.
    ///
    /// Derived, never remembered.
    pub fn attempts_for(&self, instance_id: &str, effect_id: &str) -> u64 {
        self.records
            .iter()
            .filter(|record| record.kind == RecordKind::EffectAttempted)
            .filter(|record| {
                record.body.get("instance_id").and_then(Value::as_str) == Some(instance_id)
                    && record.body.get("effect_id").and_then(Value::as_str) == Some(effect_id)
            })
            .count() as u64
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
            return Err(self.reject_unknown_effect(clock, instance_id, effect_id, request_id));
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

/// The response one attempt gives, warm or replayed.
pub(crate) fn attempted_response(
    instance_id: &str,
    effect_id: &str,
    request_id: &str,
    attempt: u64,
    result: Option<Value>,
    seq: u64,
    duplicate: bool,
) -> Value {
    let mut out = BTreeMap::from([
        ("ok".to_string(), Value::Str("true".into())),
        ("attempted".to_string(), Value::Bool(true)),
        ("instance_id".to_string(), Value::Str(instance_id.into())),
        ("effect_id".to_string(), Value::Str(effect_id.into())),
        ("attempt".to_string(), Value::Num(attempt.to_string())),
        ("outcome".to_string(), Value::Str("failed".into())),
        ("request_id".to_string(), Value::Str(request_id.into())),
        ("duplicate".to_string(), Value::Bool(duplicate)),
        ("seq".to_string(), Value::Num(seq.to_string())),
    ]);
    if let Some(value) = result {
        out.insert("result".to_string(), value);
    }
    Value::Obj(out)
}
