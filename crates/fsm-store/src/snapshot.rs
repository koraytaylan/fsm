//! Disposable store snapshots: write, verify, keep-3, and open fast-path.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fsm_core::canon::canon_bytes;
use fsm_core::hashes::{domain_hash, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{InstanceState, Status};
use fsm_core::replay::{StoreState, StoredMachine, ctx_val_string, fold_from, parse_ctx_val};
use fsm_core::sha256::to_hex;
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

use crate::store::ErrorObj;

/// On-disk snapshot format tag. Bumped to `/3` when dedup entries gained the
/// request fingerprint. Snapshots are disposable: an unrecognised format is
/// skipped and the journal is folded instead.
pub const SNAPSHOT_FORMAT: &str = "fsm.snapshot/3";

/// Hash domain for [`SNAPSHOT_FORMAT`]. Kept in lockstep with it.
pub const SNAPSHOT_DOMAIN: &str = "fsm:snapshot:3";

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
    o.insert("leaf".into(), Value::Str(inst.leaf.clone()));
    o.insert("status".into(), Value::Str(inst.status.as_str().into()));
    o.insert("machine_id".into(), Value::Str(mid.into()));
    o.insert("context".into(), Value::Obj(ctx));
    o.insert("history".into(), Value::Obj(hist));
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

pub fn state_to_snapshot(state: &StoreState) -> Value {
    let mut body = snapshot_material(state);
    body.insert("format".into(), Value::Str(SNAPSHOT_FORMAT.into()));
    body.insert(
        "state_root".into(),
        Value::Str(materialize_state_root(state)),
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

pub fn snapshot_to_state(v: &Value) -> Result<StoreState, ErrorObj> {
    let obj = v
        .as_obj()
        .ok_or_else(|| ErrorObj::new("io/read", "snapshot not an object"))?;
    if obj.get("format").and_then(Value::as_str) != Some(SNAPSHOT_FORMAT) {
        return Err(ErrorObj::new("io/read", "bad snapshot format"));
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
        let compiled = compile_accepted(def).map_err(ErrorObj::from_findings)?;
        if compiled.machine_id != *id {
            return Err(ErrorObj::new("io/read", "snapshot machine id mismatch"));
        }
        let tree = Tree::build(&compiled.spec.states);
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
        let leaf = io
            .get("leaf")
            .and_then(Value::as_str)
            .ok_or_else(|| ErrorObj::new("io/read", "snapshot instance missing leaf"))?
            .to_string();
        if stored.tree.id(&leaf).is_none() {
            return Err(ErrorObj::new("io/read", "snapshot instance unknown leaf"));
        }
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
            let Some(owner) = stored.tree.id(k) else {
                return Err(ErrorObj::new("io/read", "snapshot history unknown owner"));
            };
            let Some(bound) = stored.tree.id(s) else {
                return Err(ErrorObj::new(
                    "io/read",
                    "snapshot history unknown descendant",
                ));
            };
            let mut walk = Some(bound);
            let mut under = owner == bound;
            while let Some(n) = walk {
                if n == owner {
                    under = true;
                    break;
                }
                walk = stored.tree.parent[n as usize];
            }
            if !under {
                return Err(ErrorObj::new("io/read", "snapshot history not descendant"));
            }
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
        let inst = InstanceState {
            status,
            leaf,
            ctx,
            history,
            pending,
        };
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
pub fn commit_state_root(data_dir: &Path, seq: u64, root: &str) -> Result<(), ErrorObj> {
    if data_dir.as_os_str() == "<memory>" {
        return Ok(());
    }
    let jdir = data_dir.join("journal");
    fs::create_dir_all(&jdir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let dest = jdir.join("legacy-snapshot-root");
    let tmp = jdir.join("legacy-snapshot-root.tmp");
    fs::write(&tmp, format!("{seq}\t{root}\n"))
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let f = fs::File::open(&tmp).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    f.sync_all()
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    fs::rename(&tmp, &dest).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let df = fs::File::open(&jdir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    df.sync_all()
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    Ok(())
}

pub fn load_state_root(data_dir: &Path, seq: u64) -> Option<String> {
    let p = data_dir.join("journal").join("legacy-snapshot-root");
    let s = fs::read_to_string(p).ok()?;
    let (stored_seq, root) = s.trim().split_once('\t')?;
    (stored_seq.parse::<u64>().ok()? == seq).then(|| root.to_string())
}

fn snapshot_bound(base: &StoreState, rec: &fsm_core::record::Record) -> bool {
    let root = materialize_state_root(base);
    rec.body.get("state_root").and_then(Value::as_str) == Some(root.as_str())
}

fn prune_legacy_root_sidecars(data_dir: &Path) -> Result<(), ErrorObj> {
    let jdir = data_dir.join("journal");
    let Ok(entries) = fs::read_dir(&jdir) else {
        return Ok(());
    };
    let mut removed = false;
    for entry in entries.flatten() {
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
        let dir = fs::File::open(&jdir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        dir.sync_all()
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    }
    Ok(())
}

pub fn listed_snaps(data_dir: &Path) -> Vec<(u64, PathBuf)> {
    let dir = snap_dir(data_dir);
    let Ok(rd) = fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ent in rd.flatten() {
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
    out
}

pub fn load_newest_valid(data_dir: &Path) -> Option<(u64, StoreState)> {
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = fs::read(&path) else {
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

pub fn write_snapshot(data_dir: &Path, state: &StoreState) -> Result<PathBuf, ErrorObj> {
    if state.last_seq == 0 {
        return Err(ErrorObj::new("io/write", "no records to snapshot"));
    }
    prune_legacy_root_sidecars(data_dir)?;
    let dir = snap_dir(data_dir);
    fs::create_dir_all(&dir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let body = state_to_snapshot(state);
    let bytes = canon_bytes(&body);
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
    fs::write(&tmp, &bytes).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let f = fs::File::open(&tmp).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    f.sync_all()
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let dest = dir.join(format!("snap-{seq}.json"));
    let final_path = if dest.exists() {
        dir.join(format!("snap-{seq}-{nonce}.json"))
    } else {
        dest
    };
    fs::rename(&tmp, &final_path).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let df = fs::File::open(&dir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    df.sync_all()
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    let back = fs::read(&final_path).map_err(|e| ErrorObj::new("io/read", e.to_string()))?;
    let parsed =
        parse(&back, &JsonLimits::DEFAULT).map_err(|e| ErrorObj::new("io/read", e.message))?;
    let reloaded = snapshot_to_state(&parsed)?;
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

pub fn prune_old(data_dir: &Path) -> Result<(), ErrorObj> {
    let snaps = listed_snaps(data_dir);
    for (_, p) in snaps.into_iter().skip(3) {
        fs::remove_file(&p).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
    }
    let dir = snap_dir(data_dir);
    if dir.exists() {
        let df = fs::File::open(&dir).map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
        df.sync_all()
            .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
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
        if ia.leaf != ib.leaf
            || ia.status != ib.status
            || ia.ctx != ib.ctx
            || ia.history != ib.history
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

pub fn reconstruct_snapshot_plus_tail(
    data_dir: &Path,
    recs: &[fsm_core::record::Record],
    to_seq: u64,
) -> Result<StoreState, ErrorObj> {
    let journal_last = recs.last().map(|r| r.seq).unwrap_or(0);
    let want = to_seq.min(journal_last);
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok(base) = snapshot_to_state(&v) else {
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

pub fn open_state(
    data_dir: &Path,
    recs: Vec<fsm_core::record::Record>,
    sink: &mut impl fsm_core::replay::RecordSink,
) -> Result<(StoreState, OpenPath), fsm_core::replay::ReplayError> {
    // Earlier builds emitted one mutable root file per commit. They are never
    // trust anchors and can be removed opportunistically.
    let _ = prune_legacy_root_sidecars(data_dir);
    let journal_last = recs.last().map(|r| r.seq).unwrap_or(0);
    // First pass: prefer a hash-chain-bound snapshot even when a newer
    // clean-shutdown cache exists without a committed root.
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok(base) = snapshot_to_state(&v) else {
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
        let bound = snapshot_bound(&base, rec);
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
        let Ok(bytes) = fs::read(&path) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok(base) = snapshot_to_state(&v) else {
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
