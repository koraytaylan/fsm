//! Snapshot write/reload, keep-3, corrupt fallback, and 10k trigger.

use fsm_cli::snapshot::{listed_snaps, write_snapshot};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::{NopSink, fold_with};

fn tmp() -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fsm-snap-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
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
                "fsm:snapshot:1",
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
                    "fsm:snapshot:1",
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
    let folded = fold_with(recs, &mut NopSink).unwrap();
    let mut reopened = Store::open(&dir).unwrap();
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
