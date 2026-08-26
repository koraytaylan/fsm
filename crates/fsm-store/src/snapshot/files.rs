//! Writing, pruning, and listing snapshot cache files.
use crate::store::ErrorObj;
use fsm_core::canon::canon_bytes;
use fsm_core::hashes::state_hash;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::StoreState;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::decode::*;
use super::encode::*;

/// Durably install and verify a bounded disposable snapshot cache.
///
/// Callers that use this low-level helper directly must provide the same
/// single-writer exclusion as [`crate::store::Store`]. An over-cap snapshot is
/// refused before any cache path is changed; authoritative journal state is
/// unaffected.
pub fn write_snapshot(data_dir: &Path, state: &StoreState) -> Result<PathBuf, ErrorObj> {
    if state.last_seq == 0 {
        return Err(ErrorObj::new("io/write", "no records to snapshot"));
    }
    let body = state_to_snapshot(state);
    let bytes = canon_bytes(&body);
    if bytes.len() > crate::PERSISTENCE_READ_CAP {
        let mut details = BTreeMap::new();
        details.insert("bytes".into(), Value::Num(bytes.len().to_string()));
        details.insert(
            "max_bytes".into(),
            Value::Num(crate::PERSISTENCE_READ_CAP.to_string()),
        );
        return Err(ErrorObj::new(
            "io/write",
            format!(
                "snapshot is {} bytes; the limit is {} bytes",
                bytes.len(),
                crate::PERSISTENCE_READ_CAP
            ),
        )
        .hint("the journal remains authoritative; this oversized snapshot cache was not installed")
        .details(Value::Obj(details)));
    }
    prune_legacy_root_sidecars(data_dir)?;
    let dir = snap_dir(data_dir);
    crate::ensure_persistence_directory(&dir)
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let seq = state.last_seq;
    let nonce = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let tmp = dir.join(format!("snap-{seq}-{nonce}.tmp"));
    crate::write_durable(&tmp, &bytes).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let dest = dir.join(format!("snap-{seq}.json"));
    let final_path = if dest.exists() {
        dir.join(format!("snap-{seq}-{nonce}.json"))
    } else {
        dest
    };
    fs::rename(&tmp, &final_path).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    crate::sync_dir(&dir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let back = crate::read_regular_file_capped(&final_path, crate::PERSISTENCE_READ_CAP)
        .map_err(|e| ErrorObj::new("io/read", e.to_string()))?;
    let parsed =
        parse(&back, &JsonLimits::DEFAULT).map_err(|e| ErrorObj::new("io/read", e.message))?;
    let reloaded = match snapshot_to_state(&parsed) {
        Ok(state) => state,
        Err(current_error) => {
            // A migrated historical-genesis store can legitimately contain a
            // definition above the current aggregate evaluation ceiling. Retry
            // only after re-reading and verifying the journal chain and
            // matching its seq0 body against the exact historical limits
            // object.
            let records = crate::journal_io::load_records(data_dir)
                .map_err(|error| ErrorObj::new("io/read", error))?;
            if !journal_uses_historical_definition_limits(&records) {
                return Err(current_error);
            }
            snapshot_to_state_with_definition_limits(&parsed, SnapshotDefinitionLimits::Historical)?
        }
    };
    if reloaded.last_seq != state.last_seq || reloaded.last_hash != state.last_hash {
        return Err(ErrorObj::new("io/read", "snapshot reload mismatch"));
    }
    for (id, inst) in &state.instances {
        let other = reloaded
            .instances
            .get(id)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot missing instance"))?;
        let mid = state.instance_machines.get(id).cloned().unwrap_or_default();
        if state_hash(&mid, id, state.last_seq, inst)
            != state_hash(&mid, id, reloaded.last_seq, other)
        {
            return Err(ErrorObj::new("io/read", "snapshot instance hash mismatch"));
        }
    }
    prune_old(data_dir)?;
    Ok(final_path)
}

/// Retain the three newest snapshot cache files.
///
/// This mutating helper requires the caller to provide writer exclusion.
pub fn prune_old(data_dir: &Path) -> Result<(), ErrorObj> {
    let snaps =
        try_listed_snaps(data_dir).map_err(|error| ErrorObj::new("io/write", error.to_string()))?;
    for (_, p) in snaps.into_iter().skip(3) {
        fs::remove_file(&p).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    }
    let dir = snap_dir(data_dir);
    if crate::persistence_directory_exists(&dir)
        .map_err(|error| ErrorObj::new("io/write", error.to_string()))?
    {
        crate::sync_dir(&dir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    }
    Ok(())
}

#[allow(dead_code)]
pub(super) fn journal_ids_at(
    recs: &[fsm_core::record::Record],
    seq: u64,
) -> (
    std::collections::BTreeSet<String>,
    std::collections::BTreeSet<String>,
) {
    let mut machines = std::collections::BTreeSet::new();
    let mut instances = std::collections::BTreeSet::new();
    for rec in recs.iter().filter(|r| r.seq <= seq) {
        match rec.kind {
            fsm_core::record::RecordKind::MachineDefined => {
                if let Some(id) = rec.body.get("machine_id").and_then(Value::as_str) {
                    machines.insert(id.into());
                }
            }
            // Every kind's own answer to "which instances is this about":
            // a child exists because an `instance_invoked` record says so,
            // and a snapshot that did not know it would lose it.
            fsm_core::record::RecordKind::InstanceCreated
            | fsm_core::record::RecordKind::InstanceInvoked => {
                for id in fsm_core::record::instances_touched(rec) {
                    instances.insert(id.into());
                }
            }
            _ => {}
        }
    }
    (machines, instances)
}

/// The invocation slots a snapshot carries, typed against the machine whose
/// instance they belong to.
pub(super) fn invocations_from(
    io: &BTreeMap<String, Value>,
    machine: &fsm_core::machine::CompiledMachine,
) -> Result<BTreeMap<String, fsm_core::machine::Invocation>, ErrorObj> {
    let bad = || ErrorObj::new("io/read", "snapshot invocation slot");
    let Some(slots) = io.get("invocations").and_then(Value::as_obj) else {
        return Ok(BTreeMap::new());
    };
    let mut out = BTreeMap::new();
    for (slot, entry) in slots {
        let field = |name: &str| entry.get(name).and_then(Value::as_str);
        let status = match field("status") {
            Some("pending") => fsm_core::machine::InvokeStatus::Pending,
            Some("running") => fsm_core::machine::InvokeStatus::Running,
            Some("returned") => fsm_core::machine::InvokeStatus::Returned,
            _ => return Err(bad()),
        };
        let declared = machine
            .spec
            .walk_states()
            .into_iter()
            .find_map(|(node, _)| node.invokes.iter().find(|i| i.id == *slot).cloned())
            .ok_or_else(bad)?;
        let mut overrides = BTreeMap::new();
        if let Some(values) = entry.get("overrides").and_then(Value::as_obj) {
            for (name, raw) in values {
                // The projection must be one the slot declares; a snapshot
                // naming a field the definition does not have is a cache to
                // discard, not a state to trust.
                if !declared.with.iter().any(|(field, _)| field == name) {
                    return Err(bad());
                }
                let text = raw.as_str().ok_or_else(bad)?;
                // The projection's own type is the child's; a snapshot is a
                // cache, so the value is carried as written and re-derived
                // from the journal if it cannot be read back.
                overrides.insert(
                    name.clone(),
                    fsm_core::replay::parse_ctx_val(&fsm_core::spec::TySpec::Str, text)
                        .ok_or_else(bad)?,
                );
            }
        }
        out.insert(
            slot.clone(),
            fsm_core::machine::Invocation {
                child_machine_id: field("child_machine_id").unwrap_or_default().to_string(),
                status,
                overrides,
            },
        );
    }
    Ok(out)
}

/// The undelivered signals a snapshot carries.
pub(super) fn signals_from(
    io: &BTreeMap<String, Value>,
) -> Result<BTreeMap<String, fsm_core::machine::PendingSignal>, ErrorObj> {
    let bad = || ErrorObj::new("io/read", "snapshot signal");
    let Some(signals) = io.get("signals").and_then(Value::as_obj) else {
        return Ok(BTreeMap::new());
    };
    let mut out = BTreeMap::new();
    for (id, entry) in signals {
        let field = |name: &str| entry.get(name).and_then(Value::as_str);
        let mut payload = BTreeMap::new();
        if let Some(values) = entry.get("payload").and_then(Value::as_obj) {
            for (name, raw) in values {
                let text = raw.as_str().ok_or_else(bad)?;
                payload.insert(
                    name.clone(),
                    fsm_core::replay::parse_ctx_val(&fsm_core::spec::TySpec::Str, text)
                        .ok_or_else(bad)?,
                );
            }
        }
        out.insert(
            id.clone(),
            fsm_core::machine::PendingSignal {
                target_instance_id: field("target_instance_id").unwrap_or_default().to_string(),
                event: field("event").unwrap_or_default().to_string(),
                payload,
            },
        );
    }
    Ok(out)
}
