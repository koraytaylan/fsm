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

/// How many instance views this process has rendered.
///
/// A view is the expensive read in this store: it scans enabled events,
/// which evaluates every guard leaving the current configuration. One per
/// `instance_get` is the price of the answer; one per row of a listing is a
/// listing that gets slow exactly when a store gets interesting. Counting
/// them is how a test can say which of the two is happening.
static VIEWS_RENDERED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The number of instance views rendered so far in this process.
pub fn views_rendered() -> u64 {
    VIEWS_RENDERED.load(std::sync::atomic::Ordering::Relaxed)
}

impl Store {
    pub fn instance_view(
        &self,
        instance_id: &str,
        request_id: Option<&str>,
        duplicate: Option<bool>,
    ) -> Result<Value, ErrorObj> {
        VIEWS_RENDERED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
            // A scan selects and never applies a pipeline: the standard budget.
            let mut bud = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
            let evs = enabled_events(&st.compiled, &st.tree, inst, &mut bud);
            m.insert("enabled_events".into(), enabled_json(&evs));
            let internal: Vec<Value> = st
                .compiled
                .spec
                .events
                .iter()
                .filter(|event| event.internal)
                .map(|event| Value::Str(event.name.clone()))
                .collect();
            if !internal.is_empty() {
                m.insert("internal_events".into(), Value::Arr(internal));
            }
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
        // The tree, from the derived indexes rather than a read-time scan.
        m.insert("parent".into(), self.parent_value(instance_id));
        m.insert("children".into(), Value::Arr(self.children_of(instance_id)));
        m.insert(
            "created_seq".into(),
            Value::Num(self.created_seq(instance_id).to_string()),
        );
        m.insert(
            "machine_history".into(),
            Value::Arr(self.machine_history(instance_id)),
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

    /// The instance as every surface reports it: the view plus its history
    /// bindings.
    ///
    /// The tool and the `fsm://instance/{id}` resource both call this, so
    /// they cannot disagree about what an instance looks like — which is the
    /// only way two surfaces stay identical as a view grows fields.
    pub fn instance_report(&self, instance_id: &str) -> Result<Value, ErrorObj> {
        let mut view = self.instance_view(instance_id, None, None)?;
        if let (Value::Obj(fields), Some(instance)) =
            (&mut view, self.state.instances.get(instance_id))
        {
            fields.insert(
                "history".into(),
                Value::Obj(
                    instance
                        .history
                        .iter()
                        .map(|(owner, leaf)| (owner.clone(), Value::Str(leaf.clone())))
                        .collect(),
                ),
            );
        }
        Ok(view)
    }

    /// The definitions this instance has been on, oldest first, each with the
    /// seq from which it applied.
    ///
    /// A reader sees that an instance has changed definitions without paging
    /// its journal — and a migrated instance's pre-migration records are
    /// legible only against the definition named beside them.
    pub fn machine_history(&self, instance_id: &str) -> Vec<Value> {
        let mut out = Vec::new();
        for record in &self.records {
            let field = |name: &str| record.body.get(name).and_then(Value::as_str);
            let entry = match record.kind {
                RecordKind::InstanceCreated if field("instance_id") == Some(instance_id) => {
                    field("machine_id").map(|id| (id.to_string(), record.seq))
                }
                RecordKind::InstanceInvoked if field("child_instance_id") == Some(instance_id) => {
                    field("child_machine_id").map(|id| (id.to_string(), record.seq))
                }
                RecordKind::InstanceMigrated if field("instance_id") == Some(instance_id) => {
                    field("to_machine_id").map(|id| (id.to_string(), record.seq))
                }
                _ => None,
            };
            if let Some((machine_id, from_seq)) = entry {
                out.push(Value::Obj(BTreeMap::from([
                    ("machine_id".into(), Value::Str(machine_id)),
                    ("from_seq".into(), Value::Num(from_seq.to_string())),
                ])));
            }
        }
        out
    }

    /// The record that brought an instance into existence: an
    /// `instance_created` for a root, an `instance_invoked` for a child.
    ///
    /// Read from the history index, whose first entry is that record by
    /// construction — nothing can touch an instance before it exists — so
    /// ordering by it includes children, which ordering by a scan for
    /// `instance_created` silently would not.
    pub fn created_seq(&self, instance_id: &str) -> u64 {
        self.history
            .get(instance_id)
            .and_then(|seqs| seqs.first())
            .copied()
            .unwrap_or(0)
    }

    /// The parent and slot that invoked this instance, or null for a root.
    fn parent_value(&self, instance_id: &str) -> Value {
        match self.parents.get(instance_id) {
            None => Value::Null,
            Some((parent, slot)) => Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str(parent.clone())),
                ("slot".into(), Value::Str(slot.clone())),
            ])),
        }
    }

    /// Every slot this instance holds, with the child it names.
    ///
    /// Derived from the instance's own `invocations`, so a slot appears from
    /// the moment it is pending — before any child exists — and a reader can
    /// see what the instance is about to wait for.
    fn children_of(&self, instance_id: &str) -> Vec<Value> {
        let Some(instance) = self.state.instances.get(instance_id) else {
            return Vec::new();
        };
        instance
            .invocations
            .iter()
            .map(|(slot, invocation)| {
                let child_id = fsm_core::hashes::child_instance_id(instance_id, slot);
                let status = self
                    .state
                    .instances
                    .get(&child_id)
                    .map(|child| child.status.as_str().to_string());
                let mut entry = BTreeMap::from([
                    ("slot".into(), Value::Str(slot.clone())),
                    ("child_instance_id".into(), Value::Str(child_id)),
                    (
                        "child_machine_id".into(),
                        Value::Str(invocation.child_machine_id.clone()),
                    ),
                    (
                        "invocation_status".into(),
                        Value::Str(invocation.status.as_str().into()),
                    ),
                ]);
                // Absent while the slot is pending: there is no child yet, and
                // an invented "running" would be a lie a reader would act on.
                if let Some(status) = status {
                    entry.insert("status".into(), Value::Str(status));
                }
                Value::Obj(entry)
            })
            .collect()
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
            // Every kind's own answer to "which instances is this about":
            // a composition record names a parent and a child, and neither
            // is called `instance_id`.
            fsm_core::record::instances_touched(r).contains(&instance_id) && r.seq >= from_seq
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
        if !fsm_core::record::instances_touched(rec).contains(&instance_id)
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
