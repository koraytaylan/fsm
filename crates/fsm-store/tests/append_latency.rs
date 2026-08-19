//! Measured append latency and open cost, for sizing a writer actor.
//!
//! `Store` is synchronous, single-writer, and fsyncs every record. An embedder
//! on an async runtime needs real numbers to decide how deep to make the queue
//! in front of it, so `docs/EMBEDDING.md` quotes this harness rather than a
//! guess. Re-run it on your own hardware — fsync latency is a property of the
//! disk, not of this code:
//!
//! ```text
//! FSM_BENCH_ROOT=/path/on/filesystem-under-test \
//!   cargo +stable test --release -p fsm-store --test append_latency -- --ignored --nocapture
//! ```
//!
//! `FSM_BENCH_ROOT` must name an existing directory on the persistence
//! filesystem being measured. The harness creates and removes one uniquely
//! named child beneath it; it never removes the caller-provided root.
//!
//! Ignored by default: it is a measurement, not an assertion, and its timings
//! are meaningless under a loaded test runner.

#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::store::Store;

const SPEC: &[u8] = include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");
static BENCH_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct BenchDirectory {
    path: PathBuf,
}

impl BenchDirectory {
    fn create(tag: &str) -> Self {
        let configured = std::env::var_os("FSM_BENCH_ROOT").unwrap_or_else(|| {
            panic!(
                "FSM_BENCH_ROOT must name an existing directory on the persistence filesystem to measure"
            )
        });
        let root = fs::canonicalize(PathBuf::from(configured))
            .unwrap_or_else(|error| panic!("canonicalize FSM_BENCH_ROOT: {error}"));
        assert!(root.is_dir(), "FSM_BENCH_ROOT is not a directory: {root:?}");

        let sequence = BENCH_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!("fsm-bench-{tag}-{}-{sequence}", std::process::id()));
        fs::create_dir(&path)
            .unwrap_or_else(|error| panic!("create benchmark directory {path:?}: {error}"));
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for BenchDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
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

    let directory = BenchDirectory::create("append");
    let dir = directory.path();
    println!("benchmark data directory: {}", dir.display());
    let mut store = Store::open(dir).unwrap();
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
    let _ = fs::remove_dir_all(fsm_store::snapshot::snap_dir(dir));
    let t = Instant::now();
    let reopened = Store::open(dir).unwrap();
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
    let warm = Store::open(dir).unwrap();
    let warm_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "warm open: {records} records in {warm_ms:.1}ms (snapshot={}, replayed={})",
        warm.opened_from_snapshot, warm.replayed_records
    );
    drop(warm);
}
