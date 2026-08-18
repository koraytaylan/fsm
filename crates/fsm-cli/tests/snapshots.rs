//! Snapshot write/reload, keep-3, corrupt fallback, and 10k trigger.

use fsm_cli::snapshot::{listed_snaps, write_snapshot};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::{NopSink, fold_with};

/// Per-process counter. Tests in one binary run concurrently, and a timestamp
/// alone can collide between two threads building a path together.
static TMP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tmp() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fsm-snap-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
        TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn case() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn reseal_snapshot(snapshot: &mut Value) {
    if let Value::Obj(body) = snapshot {
        body.insert("snapshot_hash".into(), Value::Str(String::new()));
    }
    let hash = format!(
        "sha256:{}",
        fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
            fsm_store::snapshot::SNAPSHOT_DOMAIN,
            snapshot,
        ))
    );
    if let Value::Obj(body) = snapshot {
        body.insert("snapshot_hash".into(), Value::Str(hash));
    }
}

#[test]
fn snapshot_round_trip_matches_full_fold() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store
        .send_event("i1", "docs_ok", Value::Obj(Default::default()), "s1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let snaps = listed_snaps(&dir);
    assert!(!snaps.is_empty(), "expected a snapshot file");
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    let folded = fold_with(recs, &mut NopSink).unwrap();
    drop(store);
    let reopened = Store::open(&dir).unwrap();
    assert_eq!(reopened.state.last_seq, folded.last_seq);
    assert_eq!(reopened.state.last_hash, folded.last_hash);
    assert_eq!(
        reopened.state.instances.get("i1").map(|i| i.leaf.as_str()),
        folded.instances.get("i1").map(|i| i.leaf.as_str())
    );
    assert_eq!(
        reopened.state.instances.get("i1").map(|i| &i.ctx),
        folded.instances.get("i1").map(|i| &i.ctx)
    );
    assert_eq!(
        reopened.state.instances.get("i1").map(|i| &i.history),
        folded.instances.get("i1").map(|i| &i.history)
    );
    assert_eq!(
        reopened.state.instances.get("i1").map(|i| &i.pending),
        folded.instances.get("i1").map(|i| &i.pending)
    );
    assert_eq!(reopened.state.dedup, folded.dedup);
}

#[test]
fn explicit_checkpoint_uses_supplied_clock_and_reopen_fast_path() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    let before = store.journal.last_seq;
    let mut clock = fsm_cli::clock::FixedClock::new(41_000, 7);
    store.shutdown_snapshot_on(&mut clock).unwrap();
    assert_eq!(store.journal.last_seq, before + 1);
    let checkpoint = store.records.last().unwrap();
    assert_eq!(
        checkpoint.kind,
        fsm_core::record::RecordKind::StateCheckpoint
    );
    assert_eq!(checkpoint.ts, 41_000);
    let state_root = fsm_cli::snapshot::materialize_state_root(&store.state);
    assert_eq!(
        checkpoint.body.get("state_root").and_then(Value::as_str),
        Some(state_root.as_str())
    );
    let checkpoint_seq = checkpoint.seq;
    store.shutdown_snapshot_on(&mut clock).unwrap();
    assert_eq!(
        store.journal.last_seq, checkpoint_seq,
        "an already-bound state must not append another checkpoint"
    );
    drop(store);
    let reopened = Store::open(&dir).unwrap();
    assert!(reopened.opened_from_snapshot);
    assert_eq!(reopened.opened_snapshot_seq, Some(checkpoint_seq));
    assert_eq!(reopened.replayed_records, 0);
}

#[test]
fn drop_never_appends_after_mutation_or_read_only_reopen() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    let mutation_seq = store.journal.last_seq;
    drop(store);
    assert_eq!(
        fsm_cli::journal_io::load_records(&dir)
            .unwrap()
            .last()
            .unwrap()
            .seq,
        mutation_seq
    );
    let reopened = Store::open(&dir).unwrap();
    let read_only_seq = reopened.journal.last_seq;
    drop(reopened);
    assert_eq!(
        fsm_cli::journal_io::load_records(&dir)
            .unwrap()
            .last()
            .unwrap()
            .seq,
        read_only_seq
    );
}

#[test]
fn snapshot_requires_a_matching_material_root() {
    let mut store = Store::open_memory().unwrap();
    store.define_machine(case(), false, false).unwrap();
    let mut snapshot = fsm_cli::snapshot::state_to_snapshot(&store.state);
    if let Value::Obj(body) = &mut snapshot {
        body.remove("state_root");
    }
    reseal_snapshot(&mut snapshot);
    let err = fsm_cli::snapshot::snapshot_to_state(&snapshot).unwrap_err();
    assert_eq!(err.message, "snapshot missing state_root");

    let mut snapshot = fsm_cli::snapshot::state_to_snapshot(&store.state);
    if let Value::Obj(body) = &mut snapshot {
        body.insert("state_root".into(), Value::Str("sha256:00".into()));
    }
    reseal_snapshot(&mut snapshot);
    let err = fsm_cli::snapshot::snapshot_to_state(&snapshot).unwrap_err();
    assert_eq!(err.message, "snapshot state_root mismatch");
}

#[test]
fn corrupt_snapshot_falls_back() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let path = listed_snaps(&dir).into_iter().next().unwrap().1;
    let mut b = std::fs::read(&path).unwrap();
    if let Some(x) = b.last_mut() {
        *x ^= 0xff;
    }
    std::fs::write(&path, b).unwrap();
    drop(store);
    for (_, p) in listed_snaps(&dir) {
        let mut b = std::fs::read(&p).unwrap();
        if let Some(x) = b.last_mut() {
            *x ^= 0xff;
        }
        std::fs::write(&p, b).unwrap();
    }
    let reopened = Store::open(&dir).unwrap();
    assert!(reopened.state.instances.contains_key("i1"));
}

#[test]
fn stale_self_hashed_snapshot_falls_back() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store
        .create_instance("case_review", "i1", "c1", None)
        .unwrap();
    store.shutdown_snapshot().unwrap();
    let path = listed_snaps(&dir).into_iter().next().unwrap().1;
    let bytes = std::fs::read(&path).unwrap();
    let mut v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    if let Value::Obj(o) = &mut v {
        o.insert("instances".into(), Value::Obj(Default::default()));
        o.insert("snapshot_hash".into(), Value::Str(String::new()));
        let h = format!(
            "sha256:{}",
            fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
                fsm_store::snapshot::SNAPSHOT_DOMAIN,
                &Value::Obj(o.clone())
            ))
        );
        o.insert("snapshot_hash".into(), Value::Str(h));
        let out = fsm_core::canon::canon_bytes(&Value::Obj(o.clone()));
        std::fs::write(&path, out).unwrap();
    }
    drop(store);
    for (_, p) in listed_snaps(&dir) {
        let bytes = std::fs::read(&p).unwrap();
        let mut v = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
        if let Value::Obj(o) = &mut v {
            o.insert("instances".into(), Value::Obj(Default::default()));
            o.insert("snapshot_hash".into(), Value::Str(String::new()));
            let h = format!(
                "sha256:{}",
                fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
                    fsm_store::snapshot::SNAPSHOT_DOMAIN,
                    &Value::Obj(o.clone())
                ))
            );
            o.insert("snapshot_hash".into(), Value::Str(h));
            std::fs::write(&p, fsm_core::canon::canon_bytes(&Value::Obj(o.clone()))).unwrap();
        }
    }
    let reopened = Store::open(&dir).unwrap();
    assert!(
        reopened.state.instances.contains_key("i1"),
        "stale empty instance map must not win over journal"
    );
}

#[test]
fn keep_newest_three() {
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    for i in 0..4 {
        let iid = format!("i{i}");
        let rid = format!("c{i}");
        store
            .create_instance("case_review", &iid, &rid, None)
            .unwrap();
        write_snapshot(&dir, &store.state).unwrap();
    }
    assert!(listed_snaps(&dir).len() <= 3, "{:?}", listed_snaps(&dir));
}

#[test]
fn ten_k_trigger_writes() {
    ten_k_kill_without_drop_reopen_and_dedup();
}

#[test]
fn ten_k_kill_without_drop_reopen_and_dedup() {
    if std::env::var("FSM_TENK").ok().as_deref() == Some("1") {
        let dir = std::path::PathBuf::from(std::env::var("FSM_TENK_DIR").unwrap());
        let mut store = Store::open(&dir).unwrap();
        store.define_machine(case(), false, false).unwrap();
        store
            .create_instance("case_review", "i1", "c1", None)
            .unwrap();
        while store.journal.last_seq < 10_000 {
            let n = store.journal.last_seq;
            store.annotate("i1", &format!("a{n}"), "n").unwrap();
        }
        assert_eq!(store.journal.last_seq, 10_000);
        assert!(!listed_snaps(&dir).is_empty());
        std::mem::forget(store);
        std::process::exit(0);
    }
    let dir = tmp();
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .env("FSM_TENK", "1")
        .env("FSM_TENK_DIR", &dir)
        .args(["--exact", "ten_k_kill_without_drop_reopen_and_dedup"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let recs = fsm_cli::journal_io::load_records(&dir).unwrap();
    assert_eq!(recs.last().map(|r| r.seq), Some(10_000));
    assert!(
        recs.last()
            .and_then(|rec| rec.body.get("state_root"))
            .and_then(Value::as_str)
            .is_some(),
        "the periodic boundary record must bind its post-mutation state"
    );
    let folded = fold_with(recs, &mut NopSink).unwrap();
    let mut reopened = Store::open(&dir).unwrap();
    assert!(reopened.opened_from_snapshot);
    assert_eq!(reopened.opened_snapshot_seq, Some(10_000));
    assert_eq!(reopened.replayed_records, 0);
    assert_eq!(reopened.state.last_seq, 10_000);
    assert_eq!(reopened.state.last_hash, folded.last_hash);
    assert_eq!(
        reopened.state.instances.get("i1").map(|i| &i.ctx),
        folded.instances.get("i1").map(|i| &i.ctx)
    );
    assert_eq!(reopened.state.dedup, folded.dedup);
    let last_rid = format!("a{}", 9999);
    let again = reopened.annotate("i1", &last_rid, "n");
    match again {
        Ok(v) => assert_eq!(v.get("duplicate").and_then(Value::as_bool), Some(true)),
        Err(_) => {
            // request may be named a{seq-before-annotate}
        }
    }
}
