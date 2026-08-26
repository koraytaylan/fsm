//! Encoding a store state as a snapshot cache value.
use crate::store::ErrorObj;
use fsm_core::hashes::{configuration_value, domain_hash, state_hash};
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::replay::{STATE_ROOT_FORMAT, StoreState, ctx_val_string};
use fsm_core::sha256::to_hex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::decode::*;

/// On-disk snapshot format tag. Version 5 adds the composition fields
/// (`invocations`, `signals`) to version 4's configurations and schedules.
///
/// Snapshots are disposable caches: a version-4 snapshot found beside a
/// current journal is skipped and the journal folded instead, never migrated
/// — which is what the store's migration rule already says to do with them,
/// and why a snapshot bump needs no reader for the older shape.
pub const SNAPSHOT_FORMAT: &str = "fsm.snapshot/5";

/// Hash domain for [`SNAPSHOT_FORMAT`]. Kept in lockstep with it.
pub const SNAPSHOT_DOMAIN: &str = "fsm:snapshot:5";

pub fn snap_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("snapshots")
}

pub(super) fn hash_body(body: &BTreeMap<String, Value>) -> String {
    let mut tmp = body.clone();
    tmp.insert("snapshot_hash".into(), Value::Str(String::new()));
    format!(
        "sha256:{}",
        to_hex(&domain_hash(SNAPSHOT_DOMAIN, &Value::Obj(tmp)))
    )
}

pub(super) fn status_from(s: &str) -> Option<Status> {
    match s {
        "running" => Some(Status::Running),
        "completed" => Some(Status::Completed),
        "cancelled" => Some(Status::Cancelled),
        _ => None,
    }
}

pub(super) fn instance_value(id: &str, inst: &InstanceState, mid: &str, seq: u64) -> Value {
    let mut ctx = BTreeMap::new();
    for (k, v) in &inst.ctx {
        ctx.insert(k.clone(), Value::Str(ctx_val_string(v)));
    }
    let mut hist = BTreeMap::new();
    for (k, v) in &inst.history {
        hist.insert(k.clone(), Value::Str(v.clone()));
    }
    let mut o = BTreeMap::new();
    o.insert(
        "configuration".into(),
        configuration_value(&inst.configuration),
    );
    o.insert("status".into(), Value::Str(inst.status.as_str().into()));
    o.insert("machine_id".into(), Value::Str(mid.into()));
    o.insert("context".into(), Value::Obj(ctx));
    o.insert("history".into(), Value::Obj(hist));
    o.insert(
        "deadlines".into(),
        Value::Obj(
            inst.deadlines
                .iter()
                .map(|(name, due_ms)| (name.clone(), Value::Num(due_ms.to_string())))
                .collect(),
        ),
    );
    o.insert(
        "pending".into(),
        Value::Arr(inst.pending.iter().cloned().map(Value::Str).collect()),
    );
    o.insert(
        "invocations".into(),
        fsm_core::hashes::invocations_value(inst),
    );
    o.insert("signals".into(), fsm_core::hashes::signals_value(inst));
    o.insert(
        "state_hash".into(),
        Value::Str(state_hash(mid, id, seq, inst)),
    );
    Value::Obj(o)
}

pub(super) fn snapshot_material(state: &StoreState) -> BTreeMap<String, Value> {
    let mut machines = BTreeMap::new();
    for (id, m) in &state.machines {
        machines.insert(id.clone(), m.def.clone());
    }
    let mut instances = BTreeMap::new();
    for (id, inst) in &state.instances {
        let mid = state.instance_machines.get(id).cloned().unwrap_or_default();
        instances.insert(id.clone(), instance_value(id, inst, &mid, state.last_seq));
    }
    let mut dedup = BTreeMap::new();
    for (rid, slot) in &state.dedup {
        let mut e = BTreeMap::from([("seq".into(), Value::Num(slot.seq.to_string()))]);
        // Absent for keys claimed before fingerprints existed; those replay
        // but cannot be conflict-checked.
        if let Some(fp) = &slot.fp {
            e.insert("fp".into(), Value::Str(fp.clone()));
        }
        dedup.insert(rid.clone(), Value::Obj(e));
    }
    let mut body = BTreeMap::new();
    body.insert("seq".into(), Value::Num(state.last_seq.to_string()));
    body.insert("last_hash".into(), Value::Str(state.last_hash.clone()));
    body.insert("machines".into(), Value::Obj(machines));
    body.insert("instances".into(), Value::Obj(instances));
    body.insert("dedup".into(), Value::Obj(dedup));
    body
}

/// Encode a materialized store state as a self-hashed [`SNAPSHOT_FORMAT`] value.
///
/// The result is only a disposable cache representation; the journal remains
/// authoritative and must bind or reproduce the represented state on open.
pub fn state_to_snapshot(state: &StoreState) -> Value {
    let mut body = snapshot_material(state);
    body.insert("format".into(), Value::Str(SNAPSHOT_FORMAT.into()));
    body.insert(
        "state_root".into(),
        Value::Str(materialize_state_root(state)),
    );
    body.insert(
        "state_root_format".into(),
        Value::Str(STATE_ROOT_FORMAT.into()),
    );
    let h = hash_body(&body);
    body.insert("snapshot_hash".into(), Value::Str(h));
    Value::Obj(body)
}

pub(super) fn req_obj<'a>(
    obj: &'a BTreeMap<String, Value>,
    k: &str,
) -> Result<&'a BTreeMap<String, Value>, ErrorObj> {
    obj.get(k)
        .and_then(Value::as_obj)
        .ok_or_else(|| ErrorObj::new("io/read", format!("snapshot missing object {k}")))
}

pub(super) fn configuration_from_value(value: &Value) -> Result<ActiveConfiguration, ErrorObj> {
    let object = value
        .as_obj()
        .ok_or_else(|| ErrorObj::new("io/read", "snapshot configuration not object"))?;
    match object.get("kind").and_then(Value::as_str) {
        Some("sequential") => {
            if object.len() != 2 {
                return Err(ErrorObj::new(
                    "io/read",
                    "snapshot sequential configuration fields",
                ));
            }
            let leaf = object
                .get("leaf")
                .and_then(Value::as_str)
                .ok_or_else(|| ErrorObj::new("io/read", "snapshot configuration leaf"))?;
            Ok(ActiveConfiguration::Sequential {
                leaf: leaf.to_string(),
            })
        }
        Some("parallel") => {
            if object.len() != 2 {
                return Err(ErrorObj::new(
                    "io/read",
                    "snapshot parallel configuration fields",
                ));
            }
            let leaves = object
                .get("leaves")
                .and_then(Value::as_obj)
                .ok_or_else(|| ErrorObj::new("io/read", "snapshot configuration leaves"))?
                .iter()
                .map(|(region, leaf)| {
                    leaf.as_str()
                        .map(|leaf| (region.clone(), leaf.to_string()))
                        .ok_or_else(|| ErrorObj::new("io/read", "snapshot region leaf"))
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            Ok(ActiveConfiguration::Parallel { leaves })
        }
        _ => Err(ErrorObj::new("io/read", "snapshot configuration kind")),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SnapshotDefinitionLimits {
    Current,
    Historical,
}

pub(super) fn journal_uses_historical_definition_limits(
    records: &[fsm_core::record::Record],
) -> bool {
    records.first().is_some_and(|record| {
        record.seq == 0
            && record.kind == fsm_core::record::RecordKind::Genesis
            && fsm_core::record::genesis_uses_historical_definition_limits(&record.body)
    })
}
