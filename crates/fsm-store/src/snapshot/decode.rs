//! Reading a snapshot cache value back into a store state.
use crate::store::ErrorObj;
use fsm_core::hashes::state_hash;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::InstanceState;
use fsm_core::replay::{STATE_ROOT_FORMAT, StoreState, StoredMachine, parse_ctx_val};
use fsm_core::spec::{compile_accepted, compile_accepted_historical_unchecked};
use fsm_core::tree::Tree;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::encode::*;
use super::files::*;

pub(super) fn snapshot_to_state_with_definition_limits(
    v: &Value,
    definition_limits: SnapshotDefinitionLimits,
) -> Result<StoreState, ErrorObj> {
    let obj = v
        .as_obj()
        .ok_or_else(|| ErrorObj::new("io/read", "snapshot not an object"))?;
    if obj.get("format").and_then(Value::as_str) != Some(SNAPSHOT_FORMAT) {
        return Err(ErrorObj::new("io/read", "bad snapshot format"));
    }
    if obj.get("state_root_format").and_then(Value::as_str) != Some(STATE_ROOT_FORMAT) {
        return Err(ErrorObj::new("io/read", "bad snapshot state_root_format"));
    }
    let committed_root = obj
        .get("state_root")
        .and_then(Value::as_str)
        .filter(|root| !root.is_empty())
        .ok_or_else(|| ErrorObj::new("io/read", "snapshot missing state_root"))?
        .to_string();
    let want = hash_body(obj);
    let got = obj
        .get("snapshot_hash")
        .and_then(Value::as_str)
        .ok_or_else(|| ErrorObj::new("io/read", "snapshot missing snapshot_hash"))?;
    if got != want {
        return Err(ErrorObj::new("io/read", "snapshot hash mismatch"));
    }
    let seq: u64 = obj
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ErrorObj::new("io/read", "snapshot missing seq"))?;
    let last_hash = obj
        .get("last_hash")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ErrorObj::new("io/read", "snapshot missing last_hash"))?
        .to_string();
    let mut st = StoreState {
        last_seq: seq,
        last_hash,
        ..StoreState::default()
    };
    for (id, def) in req_obj(obj, "machines")? {
        let compiled = match definition_limits {
            SnapshotDefinitionLimits::Current => compile_accepted(def),
            SnapshotDefinitionLimits::Historical => compile_accepted_historical_unchecked(def),
        }
        .map_err(ErrorObj::from_findings)?;
        if compiled.machine_id != *id {
            return Err(ErrorObj::new("io/read", "snapshot machine id mismatch"));
        }
        let tree = Tree::for_machine(&compiled.spec);
        st.machines.insert(
            id.clone(),
            StoredMachine {
                def: def.clone(),
                compiled,
                tree,
            },
        );
    }
    for (rid, slotv) in req_obj(obj, "dedup")? {
        let so = slotv
            .as_obj()
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot dedup slot"))?;
        let n: u64 = so
            .get("seq")
            .and_then(Value::as_num)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot dedup seq"))?;
        if n == 0 || n > seq {
            return Err(ErrorObj::new("io/read", "snapshot dedup out of bounds"));
        }
        let fp = match so.get("fp") {
            None => None,
            Some(v) => Some(
                v.as_str()
                    .ok_or_else(|| ErrorObj::new("io/read", "snapshot dedup fp"))?
                    .to_string(),
            ),
        };
        st.dedup
            .insert(rid.clone(), fsm_core::replay::RequestSlot { seq: n, fp });
    }
    for (id, iv) in req_obj(obj, "instances")? {
        let io = iv
            .as_obj()
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance not object"))?;
        let mid = io
            .get("machine_id")
            .and_then(Value::as_str)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing machine_id"))?
            .to_string();
        let stored = st
            .machines
            .get(&mid)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance unknown machine"))?;
        let configuration =
            configuration_from_value(io.get("configuration").ok_or_else(|| {
                ErrorObj::new("io/read", "snapshot instance missing configuration")
            })?)?;
        let status = io
            .get("status")
            .and_then(Value::as_str)
            .and_then(status_from)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing status"))?;
        let cobj = io
            .get("context")
            .and_then(Value::as_obj)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing context"))?;
        let declared: std::collections::BTreeSet<&str> = stored
            .compiled
            .spec
            .context
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        for k in cobj.keys() {
            if !declared.contains(k.as_str()) {
                return Err(ErrorObj::new("io/read", "snapshot context extra key"));
            }
        }
        let mut ctx = BTreeMap::new();
        for decl in &stored.compiled.spec.context {
            let raw = cobj
                .get(&decl.name)
                .and_then(Value::as_str)
                .ok_or_else(|| ErrorObj::new("io/read", "snapshot context incomplete"))?;
            let val = parse_ctx_val(&decl.ty, raw)
                .ok_or_else(|| ErrorObj::new("io/read", "snapshot context type"))?;
            ctx.insert(decl.name.clone(), val);
        }
        let hobj = io
            .get("history")
            .and_then(Value::as_obj)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing history"))?;
        let mut history = BTreeMap::new();
        for (k, raw) in hobj {
            let s = raw
                .as_str()
                .ok_or_else(|| ErrorObj::new("io/read", "snapshot history binding"))?;
            history.insert(k.clone(), s.to_string());
        }
        let pending = io
            .get("pending")
            .and_then(Value::as_arr)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing pending"))?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ErrorObj::new("io/read", "snapshot pending id"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let deadline_object = io
            .get("deadlines")
            .and_then(Value::as_obj)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing deadlines"))?;
        let deadlines = deadline_object
            .iter()
            .map(|(name, due)| {
                let due_ms = due
                    .as_num()
                    .and_then(|raw| raw.parse::<i64>().ok())
                    .ok_or_else(|| ErrorObj::new("io/read", "snapshot deadline timestamp"))?;
                Ok((name.clone(), due_ms))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let inst = InstanceState {
            status,
            configuration,
            ctx,
            history,
            deadlines,
            pending,
            invocations: invocations_from(io, &stored.compiled)?,
            signals: signals_from(io)?,
        };
        stored
            .tree
            .validate_instance_state(&stored.compiled, &inst)
            .map_err(|error| {
                ErrorObj::new(
                    "io/read",
                    format!("snapshot invalid instance state: {}", error.detail()),
                )
            })?;
        let want_h = io
            .get("state_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing state_hash"))?;
        let have = state_hash(&mid, id, seq, &inst);
        if have != want_h {
            return Err(ErrorObj::new("io/read", "snapshot state hash mismatch"));
        }
        st.instance_machines.insert(id.clone(), mid);
        st.instances.insert(id.clone(), inst);
    }
    if committed_root != materialize_state_root(&st) {
        return Err(ErrorObj::new("io/read", "snapshot state_root mismatch"));
    }
    Ok(st)
}

/// Decode a snapshot using the current definition-admission limits.
///
/// Historical definitions are accepted only by store-open paths that also
/// authenticate the exact legacy genesis in the hash-chained journal.
pub fn snapshot_to_state(v: &Value) -> Result<StoreState, ErrorObj> {
    snapshot_to_state_with_definition_limits(v, SnapshotDefinitionLimits::Current)
}

pub(super) fn snapshot_to_state_for_journal(
    value: &Value,
    records: &[fsm_core::record::Record],
) -> Result<(StoreState, SnapshotDefinitionLimits), ErrorObj> {
    match snapshot_to_state(value) {
        Ok(state) => Ok((state, SnapshotDefinitionLimits::Current)),
        Err(current_error) => {
            if !journal_uses_historical_definition_limits(records) {
                return Err(current_error);
            }
            snapshot_to_state_with_definition_limits(value, SnapshotDefinitionLimits::Historical)
                .map(|state| (state, SnapshotDefinitionLimits::Historical))
        }
    }
}

/// Canonical hash of the complete materialized store state.
pub fn materialize_state_root(state: &StoreState) -> String {
    materialize_state_root_at(state, state.last_seq)
}

/// Root committed by a checkpoint at `seq`. The record hash is deliberately
/// excluded from the material so the root can be placed in that record.
pub fn materialize_state_root_at(state: &StoreState, seq: u64) -> String {
    fsm_core::replay::state_root_at(state, seq)
}

/// Compatibility helper for callers of the former sidecar API. The single
/// bounded file is never consulted when selecting a snapshot.
///
/// This mutating helper requires the caller to provide writer exclusion.
pub fn commit_state_root(data_dir: &Path, seq: u64, root: &str) -> Result<(), ErrorObj> {
    if data_dir.as_os_str() == "<memory>" {
        return Ok(());
    }
    let jdir = data_dir.join("journal");
    crate::ensure_persistence_directory(&jdir)
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let dest = jdir.join("legacy-snapshot-root");
    let tmp = jdir.join("legacy-snapshot-root.tmp");
    crate::write_durable(&tmp, format!("{seq}\t{root}\n").as_bytes())
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    fs::rename(&tmp, &dest).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    crate::sync_dir(&jdir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    Ok(())
}

pub fn load_state_root(data_dir: &Path, seq: u64) -> Option<String> {
    let journal = data_dir.join("journal");
    if !crate::persistence_directory_exists(&journal).ok()? {
        return None;
    }
    let p = journal.join("legacy-snapshot-root");
    let s = crate::read_regular_string_capped(&p, crate::PERSISTENCE_READ_CAP).ok()?;
    let (stored_seq, root) = s.trim().split_once('\t')?;
    (stored_seq.parse::<u64>().ok()? == seq).then(|| root.to_string())
}

/// Whether every dedup entry a cache carries agrees with the record that
/// claimed it.
///
/// `sealed_floor` is the first sequence the live journal holds. An entry
/// claimed below it has no record here to agree with — the record is in the
/// archive — and it does not need one: the seal committed a root over exactly
/// those fingerprints, and the base file was checked against it before this
/// ran. Checking only what the live records can answer is what lets the
/// snapshot fast path survive a seal.
pub(super) fn snapshot_dedup_matches_journal(
    base: &StoreState,
    records: &[fsm_core::record::Record],
    sealed_floor: u64,
) -> bool {
    base.dedup.iter().all(|(request_id, slot)| {
        if slot.seq < sealed_floor {
            return true;
        }
        let Ok(index) = records.binary_search_by_key(&slot.seq, |record| record.seq) else {
            return false;
        };
        let record = &records[index];
        if record.body.get("request_id").and_then(Value::as_str) != Some(request_id.as_str()) {
            return false;
        }
        match record.body.get("request_fp") {
            None => slot.fp.is_none(),
            Some(Value::Str(fingerprint)) => slot.fp.as_deref() == Some(fingerprint.as_str()),
            Some(_) => false,
        }
    })
}

pub(super) fn snapshot_bound(
    base: &StoreState,
    record: &fsm_core::record::Record,
    records: &[fsm_core::record::Record],
    definition_limits: SnapshotDefinitionLimits,
    sealed_floor: u64,
) -> bool {
    let root = materialize_state_root(base);
    record.body.get("state_root").and_then(Value::as_str) == Some(root.as_str())
        && record
            .body
            .get("state_root_format")
            .and_then(Value::as_str)
            == Some(STATE_ROOT_FORMAT)
        // `fsm.state-root/3` binds each request id to its claiming sequence.
        // The hash-chained claiming record binds the fingerprint itself, so a
        // fast-path snapshot must agree with that record before its dedup
        // ledger is trusted.
        && snapshot_dedup_matches_journal(base, records, sealed_floor)
        // A snapshot can omit the MachineDefined prefix it materializes. The
        // historical compiler is therefore safe on the fast path only when
        // the authenticated seq0 record carries the exact old limits table.
        // A sealed journal's genesis is in the archive, so the live records
        // cannot answer this — the base carries the discriminator instead, and
        // a cache that needs the historical compiler is refused on a sealed
        // store rather than admitted on a guess.
        && (definition_limits == SnapshotDefinitionLimits::Current
            || (sealed_floor == 0 && journal_uses_historical_definition_limits(records)))
}

pub(super) fn prune_legacy_root_sidecars(data_dir: &Path) -> Result<(), ErrorObj> {
    let jdir = data_dir.join("journal");
    if !crate::persistence_directory_exists(&jdir)
        .map_err(|error| ErrorObj::new("io/write", error.to_string()))?
    {
        return Ok(());
    }
    let entries = fs::read_dir(&jdir).map_err(|error| {
        ErrorObj::new(
            "io/write",
            format!("read journal directory {}: {error}", jdir.display()),
        )
    })?;
    let mut removed = false;
    for entry in entries {
        let entry = entry.map_err(|error| {
            ErrorObj::new(
                "io/write",
                format!(
                    "read journal directory entry in {}: {error}",
                    jdir.display()
                ),
            )
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(suffix) = name.strip_prefix("root-") else {
            continue;
        };
        let digits = suffix.strip_suffix(".tmp").unwrap_or(suffix);
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            fs::remove_file(entry.path()).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
            removed = true;
        }
    }
    if removed {
        crate::sync_dir(&jdir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    }
    Ok(())
}

pub(super) fn try_listed_snaps(data_dir: &Path) -> std::io::Result<Vec<(u64, PathBuf)>> {
    let dir = snap_dir(data_dir);
    if !crate::persistence_directory_exists(&dir)? {
        return Ok(Vec::new());
    }
    let rd = fs::read_dir(&dir)?;
    let mut out = Vec::new();
    for ent in rd {
        let ent = ent?;
        let p = ent.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.starts_with("snap-") || !name.ends_with(".json") {
            continue;
        }
        let mid = name.trim_start_matches("snap-").trim_end_matches(".json");
        let seq = mid
            .split('-')
            .next()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        out.push((seq, p));
    }
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    Ok(out)
}

/// List snapshot cache files without following a symlinked snapshot directory.
/// An inaccessible or invalid cache directory is equivalent to no snapshots;
/// the authenticated journal remains authoritative.
pub fn listed_snaps(data_dir: &Path) -> Vec<(u64, PathBuf)> {
    try_listed_snaps(data_dir).unwrap_or_default()
}

/// Decode the newest self-consistent current-format snapshot cache, if any.
///
/// This helper does not authenticate the result against a journal. Operational
/// store opens use [`open::open_state`](super::open::open_state) so a forged cache can never become authority.
pub fn load_newest_valid(data_dir: &Path) -> Option<(u64, StoreState)> {
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        if let Ok(st) = snapshot_to_state(&v) {
            return Some((st.last_seq, st));
        }
    }
    None
}
