//! Measured append latency and open cost, for sizing a writer actor.
//!
//! `Store` is synchronous, single-writer, and fsyncs every record. An embedder
//! on an async runtime needs real numbers to decide how deep to make the queue
//! in front of it, so `docs/EMBEDDING.md` quotes this harness rather than a
//! guess. Re-run it on your own hardware — fsync latency is a property of the
//! disk, not of this code:
//!
//! ```text
//! cargo test -p fsm-store --test append_latency -- --ignored --nocapture
//! ```
//!
//! Ignored by default: it is a measurement, not an assertion, and its timings
//! are meaningless under a loaded test runner.

#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::time::Instant;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::store::Store;

const SPEC: &[u8] = include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

fn tmp(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "fsm-bench-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn percentile(sorted_us: &[u128], p: f64) -> u128 {
    if sorted_us.is_empty() {
        return 0;
    }
    let idx = ((sorted_us.len() - 1) as f64 * p).round() as usize;
    sorted_us[idx]
}

fn report(label: &str, mut samples: Vec<u128>) {
    samples.sort_unstable();
    let n = samples.len();
    let total: u128 = samples.iter().sum();
    println!(
        "{label}: n={n} mean={:.0}us p50={}us p95={}us p99={}us max={}us  (~{:.0} ops/s)",
        total as f64 / n as f64,
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        percentile(&samples, 0.99),
        samples[n - 1],
        1_000_000.0 * n as f64 / total as f64
    );
}

#[test]
#[ignore = "measurement, not an assertion; run explicitly with --nocapture"]
fn append_latency() {
    let n: usize = std::env::var("FSM_BENCH_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000);

    let dir = tmp("append");
    let mut store = Store::open(&dir).unwrap();
    let def = parse(SPEC, &JsonLimits::DEFAULT).unwrap();
    store.define_machine(def, false, false).unwrap();

    // One record per call: instance_created.
    let mut creates = Vec::with_capacity(n);
    for i in 0..n {
        let t = Instant::now();
        store
            .create_instance("case_review", &format!("i{i}"), &format!("c{i}"), None)
            .unwrap();
        creates.push(t.elapsed().as_micros());
    }
    report("create_instance", creates);

    // One record per call: event_applied, the hot path.
    let mut sends = Vec::with_capacity(n);
    for i in 0..n {
        let t = Instant::now();
        store
            .send_event(
                &format!("i{i}"),
                "docs_ok",
                Value::Obj(BTreeMap::new()),
                &format!("s{i}"),
                None,
            )
            .unwrap();
        sends.push(t.elapsed().as_micros());
    }
    report("send_event", sends);

    let records = store.journal.last_seq;
    drop(store);

    // Genuinely cold: snapshots are a disposable cache, so clearing them forces
    // the whole-journal fold an embedder pays after an unclean shutdown.
    let _ = std::fs::remove_dir_all(fsm_store::snapshot::snap_dir(&dir));
    let t = Instant::now();
    let reopened = Store::open(&dir).unwrap();
    let cold = t.elapsed();
    assert!(
        !reopened.opened_from_snapshot,
        "cold measurement must not have used a snapshot"
    );
    println!(
        "cold open (full fold): {records} records in {:.1}ms ({:.1}us/record)",
        cold.as_secs_f64() * 1000.0,
        cold.as_micros() as f64 / records as f64,
    );

    // With a shutdown snapshot the tail is all that is replayed.
    let mut s = reopened;
    s.shutdown_snapshot().unwrap();
    drop(s);
    let t = Instant::now();
    let warm = Store::open(&dir).unwrap();
    let warm_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "warm open: {records} records in {warm_ms:.1}ms (snapshot={}, replayed={})",
        warm.opened_from_snapshot, warm.replayed_records
    );
    drop(warm);
    let _ = std::fs::remove_dir_all(&dir);
}
