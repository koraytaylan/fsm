//! Snapshot write/reload, keep-3, corrupt fallback, and 10k trigger.

use fsm_cli::snapshot::{listed_snaps, load_newest_valid, write_snapshot};
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
    let reopened = Store::open(&dir).unwrap();
    assert!(reopened.state.instances.contains_key("i1"));
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
    let dir = tmp();
    let mut store = Store::open(&dir).unwrap();
    store.define_machine(case(), false, false).unwrap();
    store.state.last_seq = 10_000;
    store.journal.last_seq = 10_000;
    store.maybe_snapshot().unwrap();
    assert!(
        !listed_snaps(&dir).is_empty() || load_newest_valid(&dir).is_some(),
        "10k trigger should write"
    );
}
