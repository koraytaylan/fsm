//! Disposable store snapshots: write, verify, keep-3, and open fast-path.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fsm_core::canon::canon_bytes;
use fsm_core::hashes::{configuration_value, domain_hash, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::replay::{
    STATE_ROOT_FORMAT, StoreState, StoredMachine, ctx_val_string, fold_from, parse_ctx_val,
};
use fsm_core::sha256::to_hex;
use fsm_core::spec::{compile_accepted, compile_accepted_historical_unchecked};
use fsm_core::tree::Tree;

use crate::store::ErrorObj;

/// On-disk snapshot format tag. Version 4 persists active configurations and
/// deadline schedules. Snapshots are disposable: an unrecognised format is
/// skipped and the journal is folded instead.
pub const SNAPSHOT_FORMAT: &str = "fsm.snapshot/4";

/// Hash domain for [`SNAPSHOT_FORMAT`]. Kept in lockstep with it.
pub const SNAPSHOT_DOMAIN: &str = "fsm:snapshot:4";

pub fn snap_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("snapshots")
}

fn hash_body(body: &BTreeMap<String, Value>) -> String {
    let mut tmp = body.clone();
    tmp.insert("snapshot_hash".into(), Value::Str(String::new()));
    format!(
        "sha256:{}",
        to_hex(&domain_hash(SNAPSHOT_DOMAIN, &Value::Obj(tmp)))
    )
}

fn status_from(s: &str) -> Option<Status> {
    match s {
        "running" => Some(Status::Running),
        "completed" => Some(Status::Completed),
        "cancelled" => Some(Status::Cancelled),
        _ => None,
    }
}

fn instance_value(id: &str, inst: &InstanceState, mid: &str, seq: u64) -> Value {
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
        "state_hash".into(),
        Value::Str(state_hash(mid, id, seq, inst)),
    );
    Value::Obj(o)
}

fn snapshot_material(state: &StoreState) -> BTreeMap<String, Value> {
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

fn req_obj<'a>(
    obj: &'a BTreeMap<String, Value>,
    k: &str,
) -> Result<&'a BTreeMap<String, Value>, ErrorObj> {
    obj.get(k)
        .and_then(Value::as_obj)
        .ok_or_else(|| ErrorObj::new("io/read", format!("snapshot missing object {k}")))
}

fn configuration_from_value(value: &Value) -> Result<ActiveConfiguration, ErrorObj> {
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
enum SnapshotDefinitionLimits {
    Current,
    Historical,
}

fn journal_uses_historical_definition_limits(records: &[fsm_core::record::Record]) -> bool {
    records.first().is_some_and(|record| {
        record.seq == 0
            && record.kind == fsm_core::record::RecordKind::Genesis
            && fsm_core::record::genesis_uses_historical_definition_limits(&record.body)
    })
}

fn snapshot_to_state_with_definition_limits(
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
            invocations: BTreeMap::new(),
            signals: BTreeMap::new(),
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

fn snapshot_to_state_for_journal(
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

fn snapshot_dedup_matches_journal(base: &StoreState, records: &[fsm_core::record::Record]) -> bool {
    base.dedup.iter().all(|(request_id, slot)| {
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

fn snapshot_bound(
    base: &StoreState,
    record: &fsm_core::record::Record,
    records: &[fsm_core::record::Record],
    definition_limits: SnapshotDefinitionLimits,
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
        && snapshot_dedup_matches_journal(base, records)
        // A snapshot can omit the MachineDefined prefix it materializes. The
        // historical compiler is therefore safe on the fast path only when
        // the authenticated seq0 record carries the exact old limits table.
        && (definition_limits == SnapshotDefinitionLimits::Current
            || journal_uses_historical_definition_limits(records))
}

fn prune_legacy_root_sidecars(data_dir: &Path) -> Result<(), ErrorObj> {
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

fn try_listed_snaps(data_dir: &Path) -> std::io::Result<Vec<(u64, PathBuf)>> {
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
/// store opens use [`open_state`] so a forged cache can never become authority.
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
fn journal_ids_at(
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
            fsm_core::record::RecordKind::InstanceCreated => {
                if let Some(id) = rec.body.get("instance_id").and_then(Value::as_str) {
                    instances.insert(id.into());
                }
            }
            _ => {}
        }
    }
    (machines, instances)
}

pub fn store_states_eq(a: &StoreState, b: &StoreState) -> bool {
    if a.last_seq != b.last_seq || a.last_hash != b.last_hash {
        return false;
    }
    if a.dedup != b.dedup {
        return false;
    }
    if a.instance_machines != b.instance_machines {
        return false;
    }
    if a.machines.len() != b.machines.len() {
        return false;
    }
    for (id, ma) in &a.machines {
        let Some(mb) = b.machines.get(id) else {
            return false;
        };
        if ma.compiled.machine_id != mb.compiled.machine_id || ma.def != mb.def {
            return false;
        }
    }
    if a.instances.len() != b.instances.len() {
        return false;
    }
    for (id, ia) in &a.instances {
        let Some(ib) = b.instances.get(id) else {
            return false;
        };
        if ia.configuration != ib.configuration
            || ia.status != ib.status
            || ia.ctx != ib.ctx
            || ia.history != ib.history
            || ia.deadlines != ib.deadlines
            || ia.pending != ib.pending
        {
            return false;
        }
        let mid = a.instance_machines.get(id).cloned().unwrap_or_default();
        if state_hash(&mid, id, a.last_seq, ia) != state_hash(&mid, id, b.last_seq, ib) {
            return false;
        }
    }
    true
}

fn snapshot_matches_prefix(base: &StoreState, recs: &[fsm_core::record::Record]) -> bool {
    let prefix = recs
        .iter()
        .filter(|record| record.seq <= base.last_seq)
        .cloned();
    let Ok(folded) = fsm_core::replay::fold_with(prefix, &mut fsm_core::replay::NopSink) else {
        return false;
    };
    store_states_eq(base, &folded)
}

#[derive(Debug, Clone, Default)]
pub struct OpenPath {
    pub replayed_records: usize,
    pub used_snapshot: bool,
    pub snapshot_seq: Option<u64>,
}

/// Reconstruct an untrusted snapshot-cache view plus its journal tail.
///
/// This exists only for diagnostics that immediately compare the result with
/// a complete journal fold. It deliberately preserves a self-consistent but
/// divergent cache so `journal replay` can report the first disagreement.
/// Callers MUST NOT use the returned state operationally; [`open_state`] is
/// the authenticated store-open path.
pub fn reconstruct_snapshot_plus_tail(
    data_dir: &Path,
    recs: &[fsm_core::record::Record],
    to_seq: u64,
) -> Result<StoreState, ErrorObj> {
    let journal_last = recs.last().map(|r| r.seq).unwrap_or(0);
    let want = to_seq.min(journal_last);
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok((base, _definition_limits)) = snapshot_to_state_for_journal(&v, recs) else {
            continue;
        };
        if base.last_seq > want {
            continue;
        }
        let Some(rec) = recs.iter().find(|r| r.seq == base.last_seq) else {
            continue;
        };
        if rec.hash != base.last_hash {
            continue;
        }
        let tail: Vec<_> = recs
            .iter()
            .filter(|r| r.seq > base.last_seq && r.seq <= want)
            .cloned()
            .collect();
        return fold_from(base, tail, &mut fsm_core::replay::NopSink)
            .map_err(|e| ErrorObj::new("io/read", format!("{e:?}")));
    }
    let prefix: Vec<_> = recs.iter().filter(|r| r.seq <= want).cloned().collect();
    fsm_core::replay::fold_with(prefix, &mut fsm_core::replay::NopSink)
        .map_err(|e| ErrorObj::new("io/read", format!("{e:?}")))
}

fn open_state_impl(
    data_dir: &Path,
    recs: Vec<fsm_core::record::Record>,
    sink: &mut impl fsm_core::replay::RecordSink,
    may_prune: bool,
) -> Result<(StoreState, OpenPath), fsm_core::replay::ReplayError> {
    // Earlier builds emitted one mutable root file per commit. They are never
    // trust anchors and can be removed opportunistically.
    if may_prune {
        let _ = prune_legacy_root_sidecars(data_dir);
    }
    let journal_last = recs.last().map(|r| r.seq).unwrap_or(0);
    // First pass: prefer a hash-chain-bound snapshot even when a newer
    // clean-shutdown cache exists without a committed root.
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok((base, definition_limits)) = snapshot_to_state_for_journal(&v, &recs) else {
            continue;
        };
        if base.last_seq > journal_last {
            continue;
        }
        let Some(rec) = recs.iter().find(|r| r.seq == base.last_seq) else {
            continue;
        };
        if rec.hash != base.last_hash {
            continue;
        }
        let bound = snapshot_bound(&base, rec, &recs, definition_limits);
        if !bound {
            continue;
        }
        let snap_seq = base.last_seq;
        let tail: Vec<_> = recs
            .iter()
            .filter(|r| r.seq > base.last_seq)
            .cloned()
            .collect();
        let replayed = tail.len();
        let state = fold_from(base, tail, sink)?;
        return Ok((
            state,
            OpenPath {
                replayed_records: replayed,
                used_snapshot: true,
                snapshot_seq: Some(snap_seq),
            },
        ));
    }
    // An unbound snapshot is still a useful disposable cache representation,
    // but it cannot be trusted. Re-fold and compare its complete prefix before
    // using it; this is a correctness fallback, not the fast path.
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok((base, _definition_limits)) = snapshot_to_state_for_journal(&v, &recs) else {
            continue;
        };
        if base.last_seq > journal_last {
            continue;
        }
        let Some(rec) = recs.iter().find(|r| r.seq == base.last_seq) else {
            continue;
        };
        if rec.hash != base.last_hash || !snapshot_matches_prefix(&base, &recs) {
            continue;
        }
        let snap_seq = base.last_seq;
        let tail: Vec<_> = recs
            .iter()
            .filter(|r| r.seq > base.last_seq)
            .cloned()
            .collect();
        let state = fold_from(base, tail, sink)?;
        return Ok((
            state,
            OpenPath {
                replayed_records: recs.len(),
                used_snapshot: true,
                snapshot_seq: Some(snap_seq),
            },
        ));
    }
    let n = recs.len();
    let state = fsm_core::replay::fold_with(recs, sink)?;
    Ok((
        state,
        OpenPath {
            replayed_records: n,
            used_snapshot: false,
            snapshot_seq: None,
        },
    ))
}

/// Fold a verified journal, using a snapshot only after binding or reproducing
/// its complete journal prefix.
///
/// This writer-side path may prune obsolete cache metadata. Inspection code
/// uses the crate-private non-mutating counterpart.
pub fn open_state(
    data_dir: &Path,
    recs: Vec<fsm_core::record::Record>,
    sink: &mut impl fsm_core::replay::RecordSink,
) -> Result<(StoreState, OpenPath), fsm_core::replay::ReplayError> {
    open_state_impl(data_dir, recs, sink, true)
}

pub(crate) fn open_state_read_only(
    data_dir: &Path,
    recs: Vec<fsm_core::record::Record>,
    sink: &mut impl fsm_core::replay::RecordSink,
) -> Result<(StoreState, OpenPath), fsm_core::replay::ReplayError> {
    open_state_impl(data_dir, recs, sink, false)
}
