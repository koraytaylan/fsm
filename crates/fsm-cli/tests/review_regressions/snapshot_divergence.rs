use std::collections::BTreeMap;
use std::process::Command;

use fsm_cli::store::Store;
use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};

use crate::harness::{case, fsm_bin, gate, tmp};

#[test]
fn write_snapshot_propagates_dir_sync() {
    let _g = gate();
    let dir = tmp("wsnp");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    let state = store.state.clone();
    drop(store);
    let snap = dir.join("snapshots");
    let _ = std::fs::remove_dir_all(&snap);
    std::fs::write(&snap, b"not-a-directory").unwrap();
    let err = fsm_cli::snapshot::write_snapshot(&dir, &state).unwrap_err();
    assert_eq!(err.code, "io/write", "{err:?}");
}

fn reseal_snapshot(o: &mut BTreeMap<String, Value>) {
    // The state root serializes a dedup entry as its claiming seq alone, while
    // the snapshot carries the whole slot (seq plus request fingerprint).
    // Project one onto the other so a resealed snapshot stays self-consistent.
    let dedup_root: BTreeMap<String, Value> = o
        .get("dedup")
        .and_then(Value::as_obj)
        .map(|d| {
            d.iter()
                .map(|(rid, slot)| {
                    let seq = slot
                        .get("seq")
                        .cloned()
                        .unwrap_or_else(|| Value::Num("0".into()));
                    (rid.clone(), seq)
                })
                .collect()
        })
        .unwrap_or_default();
    let root_material = Value::Obj(BTreeMap::from([
        ("seq".into(), o.get("seq").unwrap().clone()),
        ("machines".into(), o.get("machines").unwrap().clone()),
        ("instances".into(), o.get("instances").unwrap().clone()),
        ("dedup".into(), Value::Obj(dedup_root)),
    ]));
    let root = format!(
        "sha256:{}",
        fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
            fsm_core::replay::STATE_ROOT_DOMAIN,
            &root_material,
        ))
    );
    o.insert("state_root".into(), Value::Str(root));
    o.insert("snapshot_hash".into(), Value::Str(String::new()));
    let hash = format!(
        "sha256:{}",
        fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
            fsm_store::snapshot::SNAPSHOT_DOMAIN,
            &Value::Obj(o.clone()),
        ))
    );
    o.insert("snapshot_hash".into(), Value::Str(hash));
}

fn rewrite_snap_strip_dedup(dir: &std::path::Path, rid: &str, snap_seq: u64) {
    let path = keep_only_snap_seq(dir, snap_seq);
    let bytes = std::fs::read(&path).unwrap();
    let v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    let mut o = v.as_obj().unwrap().clone();
    if let Some(Value::Obj(d)) = o.get_mut("dedup") {
        d.remove(rid);
    }
    reseal_snapshot(&mut o);
    std::fs::write(&path, fsm_core::canon::canon_bytes(&Value::Obj(o))).unwrap();
}

#[test]
fn stripped_dedup_snapshot_and_mutable_sidecars_cannot_reexecute() {
    let _g = gate();
    let dir = tmp("stripd");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let seq = store.journal.last_seq;
    assert!(store.state.dedup.contains_key("c1"));
    let snap_seq = store.journal.last_seq;
    drop(store);
    rewrite_snap_strip_dedup(&dir, "c1", snap_seq);
    let snap_path = keep_only_snap_seq(&dir, snap_seq);
    let forged = parse(&std::fs::read(&snap_path).unwrap(), &JsonLimits::DEFAULT).unwrap();
    let forged_root = forged.get("state_root").and_then(Value::as_str).unwrap();
    fsm_cli::snapshot::commit_state_root(&dir, snap_seq, forged_root).unwrap();
    let old_sidecar = dir.join("journal").join(format!("root-{snap_seq:020}"));
    std::fs::write(&old_sidecar, format!("{forged_root}\n")).unwrap();
    let mut store = Store::open(&dir).unwrap();
    assert!(
        store.state.dedup.contains_key("c1"),
        "open must fall back to journal fold"
    );
    assert_eq!(store.journal.last_seq, seq);
    let again = store.create_instance("case_review", "i1", "c1", None);
    assert!(again.is_ok(), "{again:?}");
    let v = again.unwrap();
    assert_eq!(v.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(store.journal.last_seq, seq, "retry must not append");
    assert!(
        !old_sidecar.exists(),
        "legacy unbounded sidecars should be removed, not trusted"
    );
}

#[test]
fn journal_replay_disagrees_on_stripped_dedup_snapshot() {
    let _g = gate();
    let dir = tmp("jrpd");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let snap_seq = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    let last_seq = store.journal.last_seq;
    drop(store);
    rewrite_snap_strip_dedup(&dir, "c1", snap_seq);
    let snap_path = std::fs::read_dir(dir.join("snapshots"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .unwrap();
    let parsed = parse(&std::fs::read(&snap_path).unwrap(), &JsonLimits::DEFAULT).unwrap();
    fsm_cli::snapshot::snapshot_to_state(&parsed).expect("rewritten snap must parse");
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let last = recs.last().unwrap().seq;
    let live = fsm_cli::snapshot::reconstruct_snapshot_plus_tail(&dir, &recs, last).unwrap();
    assert!(
        !live.dedup.contains_key("c1"),
        "reconstruct should keep stripped snap dedup {:?}",
        live.dedup
    );
    let (_, div) = replay_disagreement(&dir);
    assert_eq!(div, snap_seq, "dedup responsible seq");
    assert_ne!(div, last_seq, "dedup must not be last_seq");
    let _ = last;
}

#[test]
fn snapshot_binding_skips_prefix_replay() {
    let _g = gate();
    let dir = tmp("fastp");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let mid = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s2", None)
        .unwrap();
    let last = store.journal.last_seq;
    drop(store);
    for (seq, path) in fsm_cli::snapshot::listed_snaps(&dir) {
        if seq != mid {
            let _ = std::fs::remove_file(path);
        }
    }
    let store = Store::open(&dir).unwrap();
    assert!(store.opened_from_snapshot, "expected snapshot fast path");
    assert_eq!(store.opened_snapshot_seq, Some(mid));
    assert_eq!(store.replayed_records, (last - mid) as usize);
    assert!(store.replayed_records > 0);
    assert!(store.replayed_records < store.records.len());
}

fn replay_disagreement(dir: &std::path::Path) -> (String, u64) {
    let bin = fsm_bin();
    let out = Command::new(&bin)
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.contains("\"agreement\":false"), "{stdout}");
    assert_ne!(out.status.code(), Some(0), "{stdout}");
    let v = parse(stdout.trim().as_bytes(), &JsonLimits::DEFAULT).expect(&stdout);
    let seq = v
        .get("first_divergent_seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| panic!("numeric first_divergent_seq missing: {stdout}"));
    (stdout, seq)
}

#[test]
fn journal_replay_disagrees_on_context_divergent_snapshot() {
    let _g = gate();
    let dir = tmp("ctxdiv");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let snap_seq = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    let last_seq = store.journal.last_seq;
    drop(store);
    rewrite_snap_context(&dir, snap_seq);
    let (out1, div) = replay_disagreement(&dir);
    assert_eq!(div, snap_seq, "responsible seq must be the snapshot seq");
    assert_ne!(div, last_seq, "must not report the tail last_seq");
    let (out2, div2) = replay_disagreement(&dir);
    assert_eq!(div2, snap_seq);
    assert_eq!(out1, out2, "two CLI replay runs must match");
}

fn keep_only_snap_seq(dir: &std::path::Path, snap_seq: u64) -> std::path::PathBuf {
    let snaps = fsm_cli::snapshot::listed_snaps(dir);
    let mut keep = None;
    for (seq, path) in &snaps {
        if *seq == snap_seq && keep.is_none() {
            keep = Some(path.clone());
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
    keep.expect("midstream snapshot")
}

fn rewrite_snap_context(dir: &std::path::Path, snap_seq: u64) {
    let path = keep_only_snap_seq(dir, snap_seq);
    let bytes = std::fs::read(&path).unwrap();
    let v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    let mut o = v.as_obj().unwrap().clone();
    let seq: u64 = o
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if let Some(Value::Obj(insts)) = o.get_mut("instances") {
        let keys: Vec<String> = insts.keys().cloned().collect();
        for id in keys {
            let Some(Value::Obj(inst)) = insts.get_mut(&id) else {
                continue;
            };
            let mid = inst
                .get("machine_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let leaf = inst
                .get("configuration")
                .and_then(|configuration| configuration.get("leaf"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let status = match inst.get("status").and_then(Value::as_str) {
                Some("completed") => fsm_core::machine::Status::Completed,
                Some("cancelled") => fsm_core::machine::Status::Cancelled,
                _ => fsm_core::machine::Status::Running,
            };
            if let Some(Value::Obj(ctx)) = inst.get_mut("context") {
                ctx.insert("visits".into(), Value::Str("99".into()));
            }
            let mut ctx = BTreeMap::new();
            if let Some(c) = inst.get("context").and_then(Value::as_obj) {
                for (k, val) in c {
                    if let Some(s) = val.as_str()
                        && let Ok(n) = s.parse::<i64>()
                    {
                        ctx.insert(k.clone(), Val::Int(n));
                    }
                }
            }
            let mut history = BTreeMap::new();
            if let Some(h) = inst.get("history").and_then(Value::as_obj) {
                for (k, val) in h {
                    if let Some(s) = val.as_str() {
                        history.insert(k.clone(), s.to_string());
                    }
                }
            }
            let pending = inst
                .get("pending")
                .and_then(Value::as_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let deadlines = inst
                .get("deadlines")
                .and_then(Value::as_obj)
                .map(|deadlines| {
                    deadlines
                        .iter()
                        .filter_map(|(name, due_ms)| {
                            due_ms
                                .as_num()
                                .and_then(|value| value.parse::<i64>().ok())
                                .map(|due_ms| (name.clone(), due_ms))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let st = fsm_core::machine::InstanceState {
                status,
                configuration: fsm_core::machine::ActiveConfiguration::Sequential { leaf },
                ctx,
                history,
                deadlines,
                pending,
            };
            inst.insert(
                "state_hash".into(),
                Value::Str(fsm_core::hashes::state_hash(&mid, &id, seq, &st)),
            );
        }
    }
    reseal_snapshot(&mut o);
    std::fs::write(&path, fsm_core::canon::canon_bytes(&Value::Obj(o))).unwrap();
}

#[test]
fn journal_replay_disagrees_on_pending_and_history_divergent_snapshots() {
    let _g = gate();
    for kind in ["pending", "history"] {
        let dir = tmp(kind);
        let mut store = Store::open(&dir).unwrap();
        store.define_machine(case(), false, false).unwrap();
        store
            .create_instance("case_review", "i1", "c1", None)
            .unwrap();
        store.shutdown_snapshot().unwrap();
        let snap_seq = store.journal.last_seq;
        store
            .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
            .unwrap();
        let last_seq = store.journal.last_seq;
        drop(store);
        rewrite_snap_field(&dir, kind, snap_seq);
        let (_, div) = replay_disagreement(&dir);
        assert_eq!(div, snap_seq, "{kind} responsible seq");
        assert_ne!(div, last_seq, "{kind} must not be last_seq");
    }
}

fn rewrite_snap_field(dir: &std::path::Path, kind: &str, snap_seq: u64) {
    let path = keep_only_snap_seq(dir, snap_seq);
    let bytes = std::fs::read(&path).unwrap();
    let v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    let mut o = v.as_obj().unwrap().clone();
    let seq: u64 = o
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    if let Some(Value::Obj(insts)) = o.get_mut("instances") {
        let keys: Vec<String> = insts.keys().cloned().collect();
        for id in keys {
            let Some(Value::Obj(inst)) = insts.get_mut(&id) else {
                continue;
            };
            let mid = inst
                .get("machine_id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let leaf = inst
                .get("configuration")
                .and_then(|configuration| configuration.get("leaf"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let status = match inst.get("status").and_then(Value::as_str) {
                Some("completed") => fsm_core::machine::Status::Completed,
                Some("cancelled") => fsm_core::machine::Status::Cancelled,
                _ => fsm_core::machine::Status::Running,
            };
            if kind == "pending" {
                inst.insert(
                    "pending".into(),
                    Value::Arr(vec![Value::Str("ghost/1/0".into())]),
                );
            }
            if kind == "history" {
                inst.insert(
                    "history".into(),
                    Value::Obj(BTreeMap::from([(
                        "in_review".into(),
                        Value::Str("docs_review".into()),
                    )])),
                );
            }
            let mut ctx = BTreeMap::new();
            if let Some(c) = inst.get("context").and_then(Value::as_obj) {
                for (k, val) in c {
                    if let Some(s) = val.as_str()
                        && let Ok(n) = s.parse::<i64>()
                    {
                        ctx.insert(k.clone(), Val::Int(n));
                    }
                }
            }
            let mut history = BTreeMap::new();
            if let Some(h) = inst.get("history").and_then(Value::as_obj) {
                for (k, val) in h {
                    if let Some(s) = val.as_str() {
                        history.insert(k.clone(), s.to_string());
                    }
                }
            }
            let pending = inst
                .get("pending")
                .and_then(Value::as_arr)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let deadlines = inst
                .get("deadlines")
                .and_then(Value::as_obj)
                .map(|deadlines| {
                    deadlines
                        .iter()
                        .filter_map(|(name, due_ms)| {
                            due_ms
                                .as_num()
                                .and_then(|value| value.parse::<i64>().ok())
                                .map(|due_ms| (name.clone(), due_ms))
                        })
                        .collect()
                })
                .unwrap_or_default();
            let st = fsm_core::machine::InstanceState {
                status,
                configuration: fsm_core::machine::ActiveConfiguration::Sequential { leaf },
                ctx,
                history,
                deadlines,
                pending,
            };
            inst.insert(
                "state_hash".into(),
                Value::Str(fsm_core::hashes::state_hash(&mid, &id, seq, &st)),
            );
        }
    }
    reseal_snapshot(&mut o);
    std::fs::write(&path, fsm_core::canon::canon_bytes(&Value::Obj(o))).unwrap();
}

#[test]
fn journal_replay_ignores_caches_on_migratable_store() {
    let _g = gate();
    let dir = tmp("jrmig");
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let snap_seq = store.journal.last_seq;
    store
        .send_event("i1", "docs_ok", Value::Obj(BTreeMap::new()), "s1", None)
        .unwrap();
    drop(store);
    rewrite_snap_strip_dedup(&dir, "c1", snap_seq);
    std::fs::write(dir.join("VERSION"), "5\n").unwrap();
    let out = Command::new(fsm_bin())
        .args([
            "--data-dir",
            dir.to_str().unwrap(),
            "--json",
            "journal",
            "replay",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    assert!(stdout.contains("\"agreement\":true"), "{stdout}");
    assert!(stdout.contains("\"snapshots_ignored\":true"), "{stdout}");
    assert_eq!(
        std::fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        "5",
        "replay must not migrate"
    );
}
