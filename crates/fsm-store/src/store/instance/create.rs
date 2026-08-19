use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_core::hashes::{STATE_FORMAT, configuration_value, machine_id, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::InstanceState;
use fsm_core::record::RecordKind;
use fsm_core::step::create;

use crate::store::{ErrorObj, Store};

impl Store {
    pub fn create_instance(
        &mut self,
        machine_ref: &str,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
    ) -> Result<Value, ErrorObj> {
        self.create_instance_ctx(
            machine_ref,
            instance_id,
            request_id,
            expect_seq,
            &BTreeMap::new(),
            &[],
        )
    }

    pub fn create_instance_ctx(
        &mut self,
        machine_ref: &str,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
        overrides: &BTreeMap<String, Val>,
        tags: &[String],
    ) -> Result<Value, ErrorObj> {
        self.create_instance_ctx_on(
            &mut crate::clock::GlobalClock,
            machine_ref,
            instance_id,
            request_id,
            expect_seq,
            overrides,
            tags,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create_instance_ctx_on(
        &mut self,
        clock: &mut dyn crate::clock::Clock,
        machine_ref: &str,
        instance_id: &str,
        request_id: &str,
        expect_seq: Option<u64>,
        overrides: &BTreeMap<String, Val>,
        tags: &[String],
    ) -> Result<Value, ErrorObj> {
        self.ensure_writable()?;
        if let Some(r) = self.claim_request(
            request_id,
            Self::fp_create(machine_ref, instance_id, overrides, tags),
        )? {
            return r;
        }
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
        let mid = {
            let m = self
                .resolve_machine(machine_ref)
                .map_err(|e| e.request_id(request_id))?;
            machine_id(&m.def)
        };
        let m = self.state.machines.get(&mid).ok_or_else(|| {
            ErrorObj::new("req/machine_not_found", machine_ref)
                .request_id(request_id)
                .with_store_catalog(self)
        })?;
        let commit_ts = clock.now_ms();
        let a = create(&m.compiled, &m.tree, overrides, commit_ts).map_err(|r| {
            let mut e = ErrorObj::from_rejection(&r)
                .request_id(request_id)
                .with_store_catalog(self);
            if let Value::Obj(d) = &mut e.details {
                d.insert("machine".into(), Value::Str(machine_ref.into()));
                d.insert("machine_id".into(), Value::Str(mid.clone()));
                d.insert(
                    "context_fields".into(),
                    Value::Arr(
                        m.compiled
                            .spec
                            .context
                            .iter()
                            .map(|c| {
                                Value::Obj(BTreeMap::from([
                                    ("name".into(), Value::Str(c.name.clone())),
                                    ("type".into(), Value::Str(c.ty.to_ty().to_string())),
                                    ("init".into(), Value::Str(c.init.clone())),
                                ]))
                            })
                            .collect(),
                    ),
                );
                let created: Vec<Value> = self
                    .state
                    .instance_machines
                    .values()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .filter(|id| id != &mid)
                    .map(Value::Str)
                    .collect();
                if !created.is_empty() {
                    d.insert("known_machines".into(), Value::Arr(created));
                }
            }
            e
        })?;
        let pending: Vec<String> = a
            .effects
            .iter()
            .map(|e| format!("{instance_id}/0/{}", e.k))
            .collect();
        let inst = InstanceState {
            status: a.status_after,
            configuration: a.configuration_after.clone(),
            ctx: a.ctx_after.clone(),
            history: a.history_after.clone(),
            deadlines: a.deadlines_after.clone(),
            pending: pending.clone(),
        };
        let sh = state_hash(&mid, instance_id, self.journal.last_seq + 1, &inst);
        let mut ov = BTreeMap::new();
        for (k, v) in overrides {
            ov.insert(k.clone(), Value::Str(v.canonical_string()));
        }
        let mut body = BTreeMap::new();
        body.insert("instance_id".into(), Value::Str(instance_id.into()));
        body.insert("machine_id".into(), Value::Str(mid.clone()));
        body.insert("request_id".into(), Value::Str(request_id.into()));
        body.insert("state_hash".into(), Value::Str(sh.clone()));
        body.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        body.insert(
            "configuration".into(),
            configuration_value(&inst.configuration),
        );
        body.insert("overrides".into(), Value::Obj(ov));
        if !tags.is_empty() {
            body.insert(
                "tags".into(),
                Value::Arr(tags.iter().cloned().map(Value::Str).collect()),
            );
        }
        let rec =
            self.append_at_with_root(RecordKind::InstanceCreated, Value::Obj(body), commit_ts)?;
        self.state.instances.insert(instance_id.into(), inst);
        self.state.instance_machines.insert(instance_id.into(), mid);
        if !tags.is_empty() {
            self.tags.insert(instance_id.into(), tags.to_vec());
        }
        self.history
            .entry(instance_id.into())
            .or_default()
            .push(rec.seq);
        self.note_record(&rec);
        let resp = self.instance_view(instance_id, Some(request_id), Some(false))?;
        self.commit_dedup(request_id, resp.clone(), rec.seq);
        self.finish_commit();
        Ok(resp)
    }
}
