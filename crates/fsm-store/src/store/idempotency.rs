use std::collections::BTreeMap;
use std::fs;

use fsm_core::expr::eval::Val;
use fsm_core::json::Value;
use fsm_core::record::RecordKind;

use super::reconstruct::{
    fold_prefix, reconstruct_applied, reconstruct_deadline_applied, reconstruct_ignored, view_at,
};
use super::{ErrorObj, Store};

impl Store {
    /// Look up a `request_id` against the idempotency ledger without claiming
    /// an unused key.
    ///
    /// `Ok(None)` means the key is unclaimed. `Ok(Some(_))` is the original
    /// outcome, replayed. `Err` is either the original error replayed, or
    /// `req/request_id_conflict` when the key was claimed by a *different*
    /// request.
    #[allow(clippy::type_complexity)]
    pub(super) fn lookup_request(
        &mut self,
        request_id: &str,
        fp: &str,
    ) -> Result<Option<Result<Value, ErrorObj>>, ErrorObj> {
        self.pending_fp = None;
        if let Some(slot) = self.state.dedup.get(request_id) {
            if let Some(prev) = slot.fp.as_deref() {
                if prev != fp {
                    let claimed_seq = slot.seq;
                    return Err(self.request_id_conflict(request_id, claimed_seq));
                }
            }
        }
        Ok(self.replay_request(request_id))
    }

    /// Resolve a `request_id` against the idempotency ledger and stage its
    /// fingerprint for the records the operation will append.
    ///
    /// `Ok(None)` means the key is unclaimed and the caller should proceed;
    /// the fingerprint is stashed for the records the operation will append.
    /// `Ok(Some(_))` is the original outcome, replayed. `Err` is either the
    /// original error replayed, or `req/request_id_conflict` when the key was
    /// claimed by a *different* request — a reuse must never return an
    /// unrelated outcome as though it were this request's.
    #[allow(clippy::type_complexity)]
    pub(super) fn claim_request(
        &mut self,
        request_id: &str,
        fp: String,
    ) -> Result<Option<Result<Value, ErrorObj>>, ErrorObj> {
        let replay = self.lookup_request(request_id, &fp)?;
        if replay.is_none() {
            self.pending_fp = Some(fp);
        }
        Ok(replay)
    }

    /// The key is taken by different content. Point at the record that claimed
    /// it so the caller can see what they actually sent, and say plainly that
    /// the remedy is a new key — not a retry.
    fn request_id_conflict(&self, request_id: &str, claimed_seq: u64) -> ErrorObj {
        let mut d = BTreeMap::new();
        d.insert("claimed_by_seq".into(), Value::Num(claimed_seq.to_string()));
        if let Some(rec) = self.records.iter().find(|r| r.seq == claimed_seq) {
            d.insert(
                "claimed_by".into(),
                Value::Str(rec.kind.as_str().to_string()),
            );
            for field in ["instance_id", "event", "deadline", "effect_id"] {
                if let Some(v) = rec.body.get(field) {
                    d.insert(format!("original_{field}"), v.clone());
                }
            }
        }
        ErrorObj::new(
            "req/request_id_conflict",
            "request_id already used for a different request",
        )
        .hint("this request_id is an idempotency key held by different content; send this request with a NEW request_id, and retry the original one only to replay its outcome")
        .details(Value::Obj(d))
        .request_id(request_id)
    }

    fn replay_request(&self, request_id: &str) -> Option<Result<Value, ErrorObj>> {
        if let Some(mut r) = self.last_responses.get(request_id).cloned() {
            if let Value::Obj(o) = &mut r {
                o.insert("duplicate".into(), Value::Bool(true));
            }
            return Some(Ok(r));
        }
        if let Some(e) = self.last_errors.get(request_id) {
            return Some(Err(e.clone().mark_duplicate()));
        }
        if !self.state.dedup.contains_key(request_id) {
            return None;
        }
        let rec = self
            .records
            .iter()
            .rev()
            .find(|r| r.body.get("request_id").and_then(Value::as_str) == Some(request_id))?;
        if matches!(
            rec.kind,
            RecordKind::EventRejected | RecordKind::DeadlineRejected | RecordKind::RequestRejected
        ) {
            let code = rec
                .body
                .get("code")
                .and_then(Value::as_str)
                .unwrap_or("run/unhandled");
            let message = rec
                .body
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or(code);
            let hint = rec
                .body
                .get("hint")
                .and_then(Value::as_str)
                .unwrap_or(message);
            let mut err = ErrorObj::new(code, message).hint(hint);
            if let Some(d) = rec.body.get("details") {
                err.details = d.clone();
            }
            let include_instance_identity = matches!(
                rec.kind,
                RecordKind::EventRejected | RecordKind::DeadlineRejected
            ) || (rec.kind == RecordKind::RequestRejected
                && rec.body.get("operation").and_then(Value::as_str) == Some("poll_deadline"));
            if include_instance_identity {
                if let Value::Obj(d) = &mut err.details {
                    if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                        d.insert("instance_id".into(), Value::Str(iid.into()));
                        if let Some(mid) = self.state.instance_machines.get(iid) {
                            d.insert("machine_id".into(), Value::Str(mid.clone()));
                        }
                    }
                }
            }
            if let Some(sp) = rec.body.get("span").and_then(Value::as_obj) {
                if let (Some(s), Some(e)) = (
                    sp.get("start")
                        .and_then(Value::as_num)
                        .and_then(|n| n.parse().ok()),
                    sp.get("end")
                        .and_then(Value::as_num)
                        .and_then(|n| n.parse().ok()),
                ) {
                    err.span = Some((s, e));
                }
            }
            return Some(Err(err.request_id(request_id).mark_duplicate()));
        }
        if rec.kind == RecordKind::EffectAcked {
            let mut m = BTreeMap::new();
            m.insert("ok".into(), Value::Str("true".into()));
            m.insert("acked".into(), Value::Bool(true));
            if let Some(v) = rec.body.get("instance_id") {
                m.insert("instance_id".into(), v.clone());
            }
            if let Some(v) = rec.body.get("effect_id") {
                m.insert("effect_id".into(), v.clone());
            }
            if let Some(v) = rec.body.get("outcome") {
                m.insert("outcome".into(), v.clone());
            }
            m.insert("request_id".into(), Value::Str(request_id.into()));
            if let Some(v) = rec.body.get("result") {
                m.insert("result".into(), v.clone());
            }
            m.insert("duplicate".into(), Value::Bool(true));
            m.insert("seq".into(), Value::Num(rec.seq.to_string()));
            if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                    if let Some(inst) = folded.instances.get(iid) {
                        m.insert(
                            "effects_pending".into(),
                            Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
                        );
                    }
                }
            }
            return Some(Ok(Value::Obj(m)));
        }
        if rec.kind == RecordKind::Annotated {
            let mut m = BTreeMap::new();
            m.insert("ok".into(), Value::Str("true".into()));
            if let Some(n) = rec.body.get("note") {
                m.insert("note".into(), n.clone());
            }
            m.insert("request_id".into(), Value::Str(request_id.into()));
            m.insert("duplicate".into(), Value::Bool(true));
            return Some(Ok(Value::Obj(m)));
        }
        if rec.kind == RecordKind::EventIgnored {
            if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                    if let Some(mut v) = reconstruct_ignored(&folded, rec, iid, request_id) {
                        if let Value::Obj(o) = &mut v {
                            o.insert("duplicate".into(), Value::Bool(true));
                        }
                        return Some(Ok(v));
                    }
                }
            }
        }
        if rec.kind == RecordKind::DeadlineNotDue {
            if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
                if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                    if let Ok(mut value) =
                        view_at(&folded, iid, Some(request_id), Some(true), rec.seq)
                    {
                        if let Value::Obj(output) = &mut value {
                            output.insert("deadline_applied".into(), Value::Bool(false));
                            output.insert("deadline_not_due".into(), Value::Bool(true));
                            for field in ["next_deadline", "next_deadline_idx"] {
                                if let Some(field_value) = rec.body.get(field) {
                                    output.insert(field.into(), field_value.clone());
                                }
                            }
                            if let Some(next_due_ms) =
                                rec.body.get("next_due_ms").and_then(Value::as_num)
                            {
                                output.insert(
                                    "next_due_ms".into(),
                                    Value::Str(next_due_ms.to_string()),
                                );
                            }
                        }
                        return Some(Ok(value));
                    }
                }
            }
        }
        if let Some(iid) = rec.body.get("instance_id").and_then(Value::as_str) {
            if rec.kind == RecordKind::EventApplied {
                if let Ok(pre) = fold_prefix(&self.records, rec.seq.saturating_sub(1)) {
                    if let Some(mut v) = reconstruct_applied(&pre, rec, iid, request_id) {
                        if let Value::Obj(o) = &mut v {
                            o.insert("duplicate".into(), Value::Bool(true));
                        }
                        return Some(Ok(v));
                    }
                }
            }
            if rec.kind == RecordKind::DeadlineApplied {
                if let Ok(pre) = fold_prefix(&self.records, rec.seq.saturating_sub(1)) {
                    if let Some(mut value) =
                        reconstruct_deadline_applied(&pre, rec, iid, request_id)
                    {
                        if let Value::Obj(output) = &mut value {
                            output.insert("duplicate".into(), Value::Bool(true));
                        }
                        return Some(Ok(value));
                    }
                }
            }
            if let Ok(folded) = fold_prefix(&self.records, rec.seq) {
                if let Ok(mut v) = view_at(&folded, iid, Some(request_id), Some(true), rec.seq) {
                    if let Value::Obj(o) = &mut v {
                        o.insert("duplicate".into(), Value::Bool(true));
                        if rec.kind == RecordKind::EventApplied {
                            o.insert("applied".into(), Value::Bool(true));
                        } else if rec.kind == RecordKind::DeadlineApplied {
                            o.insert("deadline_applied".into(), Value::Bool(true));
                        }
                    }
                    return Some(Ok(v));
                }
            }
        }
        Some(Ok(Value::Obj(BTreeMap::from([
            ("ok".into(), Value::Str("true".into())),
            ("duplicate".into(), Value::Bool(true)),
        ]))))
    }

    /// Fingerprint of a `create` request. The machine reference is fingerprinted
    /// as written, not as resolved: resolution happens after this check, and a
    /// retry resends the same text. A different spelling of the same machine is
    /// therefore a conflict — strict, but never silently wrong.
    pub(super) fn fp_create(
        machine_ref: &str,
        instance_id: &str,
        overrides: &BTreeMap<String, Val>,
        tags: &[String],
    ) -> String {
        let ov = overrides
            .iter()
            .map(|(k, v)| (k.clone(), Value::Str(fsm_core::replay::ctx_val_string(v))))
            .collect();
        fsm_core::hashes::request_fp(
            "create",
            &BTreeMap::from([
                ("machine".into(), Value::Str(machine_ref.into())),
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("overrides".into(), Value::Obj(ov)),
                (
                    "tags".into(),
                    Value::Arr(tags.iter().cloned().map(Value::Str).collect()),
                ),
            ]),
        )
    }

    /// Fingerprint of a `send` request, over the payload **as received** —
    /// before stamp fields are filled in, so an honest retry of a stamped send
    /// still matches.
    pub(super) fn fp_send(instance_id: &str, event: &str, payload: &Value) -> String {
        fsm_core::hashes::request_fp(
            "send",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("event".into(), Value::Str(event.into())),
                ("payload".into(), payload.clone()),
            ]),
        )
    }

    pub(super) fn fp_poll_deadline(instance_id: &str) -> String {
        fsm_core::hashes::request_fp(
            "poll_deadline",
            &BTreeMap::from([("instance_id".into(), Value::Str(instance_id.into()))]),
        )
    }

    pub(super) fn fp_ack(
        instance_id: &str,
        effect_id: &str,
        outcome: &str,
        result: Option<&Value>,
    ) -> String {
        fsm_core::hashes::request_fp(
            "ack",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("effect_id".into(), Value::Str(effect_id.into())),
                ("outcome".into(), Value::Str(outcome.into())),
                ("result".into(), result.cloned().unwrap_or(Value::Null)),
            ]),
        )
    }

    pub(super) fn fp_cancel(instance_id: &str, reason: &str) -> String {
        fsm_core::hashes::request_fp(
            "cancel",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("reason".into(), Value::Str(reason.into())),
            ]),
        )
    }

    pub(super) fn fp_annotate(instance_id: &str, note: &str) -> String {
        fsm_core::hashes::request_fp(
            "annotate",
            &BTreeMap::from([
                ("instance_id".into(), Value::Str(instance_id.into())),
                ("note".into(), Value::Str(note.into())),
            ]),
        )
    }

    pub fn allocate_request_id(&mut self) -> Result<String, ErrorObj> {
        self.ensure_writable()?;
        let path = self.data_dir.join("alloc");
        let n = crate::read_regular_string_capped(&path, crate::PERSISTENCE_READ_CAP)
            .ok()
            .and_then(|s| {
                let t = s.trim();
                if t.is_empty() {
                    None
                } else {
                    t.parse::<u64>().ok()
                }
            })
            .unwrap_or(self.journal.last_seq);
        let mut next = n
            .saturating_add(1)
            .max(self.journal.last_seq.saturating_add(1));
        loop {
            let cand = format!("req-{}-{next}", self.journal.last_seq);
            if !self.state.dedup.contains_key(&cand) {
                let tmp = self.data_dir.join("alloc.tmp");
                crate::write_durable(&tmp, format!("{next}\n").as_bytes())
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                fs::rename(&tmp, &path).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                crate::sync_dir(&self.data_dir)
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
                return Ok(cand);
            }
            next += 1;
        }
    }

    /// Record which request claimed the key, so a later reuse with different
    /// content is a conflict rather than a replay of this outcome.
    pub(super) fn claimed_slot(&self, seq: u64) -> fsm_core::replay::RequestSlot {
        fsm_core::replay::RequestSlot {
            seq,
            fp: self.pending_fp.clone(),
        }
    }

    pub(super) fn commit_dedup(&mut self, request_id: &str, resp: Value, seq: u64) {
        let slot = self.claimed_slot(seq);
        self.state.dedup.insert(request_id.into(), slot);
        self.last_responses.insert(request_id.into(), resp);
        self.state.last_seq = self.journal.last_seq;
        self.state.last_hash = self.journal.last_hash.clone();
    }
}
