use std::collections::BTreeMap;

use fsm_core::analyze::enabled_events;
use fsm_core::expr::eval::Budget;
use fsm_core::hashes::{STATE_FORMAT, configuration_value, state_hash};
use fsm_core::json::Value;
use fsm_core::record::RecordKind;
use fsm_core::replay::ctx_val_json;

use super::json_helpers::enabled_json;
use super::reconstruct::{
    history_entry, insert_configuration_fields, pending_deadlines_value, verify_prefix_hashes,
};
use super::{ErrorObj, Store};

impl Store {
    pub fn instance_view(
        &self,
        instance_id: &str,
        request_id: Option<&str>,
        duplicate: Option<bool>,
    ) -> Result<Value, ErrorObj> {
        let inst = self.state.instances.get(instance_id).ok_or_else(|| {
            let e = ErrorObj::new("req/instance_not_found", instance_id)
                .hint("use a known instance id from details.known_instances")
                .with_store_catalog(self);
            match request_id {
                Some(rid) => e.request_id(rid),
                None => e,
            }
        })?;
        let mid = self
            .state
            .instance_machines
            .get(instance_id)
            .cloned()
            .unwrap_or_default();
        let stored = self.state.machines.get(&mid);
        let mut ctx = BTreeMap::new();
        for (k, v) in &inst.ctx {
            ctx.insert(k.clone(), ctx_val_json(v));
        }
        let mut m = BTreeMap::new();
        m.insert("instance_id".into(), Value::Str(instance_id.into()));
        m.insert("ok".into(), Value::Str("true".into()));
        m.insert("status".into(), Value::Str(inst.status.as_str().into()));
        if let Some(st) = stored {
            insert_configuration_fields(&mut m, &st.tree, &inst.configuration);
            m.insert(
                "deadlines_pending".into(),
                pending_deadlines_value(st, inst),
            );
            let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
            let evs = enabled_events(&st.compiled, &st.tree, inst, &mut bud);
            m.insert("enabled_events".into(), enabled_json(&evs));
            let mut mac = BTreeMap::new();
            mac.insert("machine_id".into(), Value::Str(mid.clone()));
            mac.insert("name".into(), Value::Str(st.compiled.spec.name.clone()));
            m.insert("machine".into(), Value::Obj(mac));
        } else {
            m.insert(
                "configuration".into(),
                configuration_value(&inst.configuration),
            );
            m.insert("deadlines_pending".into(), Value::Arr(vec![]));
            m.insert("enabled_events".into(), Value::Arr(vec![]));
        }
        m.insert("context".into(), Value::Obj(ctx));
        m.insert(
            "effects_pending".into(),
            Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
        );
        m.insert("seq".into(), Value::Num(self.journal.last_seq.to_string()));
        m.insert(
            "state_hash".into(),
            Value::Str(state_hash(&mid, instance_id, self.journal.last_seq, inst)),
        );
        m.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        if let Some(r) = request_id {
            m.insert("request_id".into(), Value::Str(r.into()));
        }
        if let Some(d) = duplicate {
            m.insert("duplicate".into(), Value::Bool(d));
        }
        Ok(Value::Obj(m))
    }

    pub fn history_page(
        &self,
        instance_id: &str,
        from_seq: u64,
        limit: usize,
        include_trace: bool,
        include_rejected: bool,
    ) -> Result<Value, ErrorObj> {
        let limit = limit.min(500);
        let mut entries = Vec::new();
        let mut next_from_seq = None;
        for rec in self.records.iter().filter(|r| {
            r.body.get("instance_id").and_then(Value::as_str) == Some(instance_id)
                && r.seq >= from_seq
        }) {
            if !include_rejected
                && matches!(
                    rec.kind,
                    RecordKind::EventRejected
                        | RecordKind::DeadlineRejected
                        | RecordKind::RequestRejected
                )
            {
                continue;
            }
            if entries.len() >= limit {
                next_from_seq = Some(rec.seq);
                break;
            }
            entries.push(history_entry(self, rec, include_trace)?);
        }
        let mut out = BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.into())),
            ("entries".into(), Value::Arr(entries)),
            (
                "chain_verified".into(),
                Value::Bool(verify_prefix_hashes(&self.records)),
            ),
        ]);
        if let Some(n) = next_from_seq {
            out.insert("next_from_seq".into(), Value::Num(n.to_string()));
        }
        let _ = include_trace;
        Ok(Value::Obj(out))
    }

    pub fn explain_seq(&self, instance_id: &str, seq: u64) -> Result<Value, ErrorObj> {
        let rec = self
            .records
            .iter()
            .find(|r| r.seq == seq)
            .ok_or_else(|| ErrorObj::new("req/field_missing", "seq"))?;
        if rec.body.get("instance_id").and_then(Value::as_str) != Some(instance_id)
            && rec.kind != RecordKind::Genesis
            && rec.kind != RecordKind::MachineDefined
        {
            return Err(ErrorObj::new("req/instance_not_found", instance_id));
        }
        let mut e = history_entry(self, rec, true)?;
        if let Value::Obj(o) = &mut e {
            o.insert(
                "chain_verified".into(),
                Value::Bool(verify_prefix_hashes(&self.records)),
            );
        }
        Ok(e)
    }
}
