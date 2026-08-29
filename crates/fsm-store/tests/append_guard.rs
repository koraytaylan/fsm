//! A **guard** on append and cold-open cost. Its sibling
//! `append_latency.rs` is a **measurement**.
//!
//! The distinction is the design, and conflating the two produces either a
//! flaky gate or a number nobody reads. A measurement reports figures for a
//! human — `docs/RELEASE.md` runs it by hand and quotes its table in
//! `docs/EMBEDDING.md`, and it stays `#[ignore]`d because its timings are
//! meaningless under a loaded test runner. A guard asserts a bound and fails.
//! This one runs by default, and it exists because plan 0017 changes both the
//! path an append takes and the way a store opens, and an unguarded cost is
//! exactly what regresses under that kind of change.
//!
//! # Why the bounds look loose
//!
//! CI is a shared, noisy, variable machine across three operating systems, and
//! a tight bound there produces flakes. A flaky performance test is deleted
//! within a month, so this one is deliberately sized to catch a **collapse** —
//! an accidental rescan per append, a fold that stopped being linear — and not
//! a ten-percent drift. Noticing a drift is the measurement's job.
//!
//! The append bound adapts to the hardware instead of guessing at it. An
//! append is dominated by one `fsync`, whose cost belongs to the disk and not
//! to this code, so the guard measures a bare `write` + `sync_all` on the same
//! filesystem in the same run and allows a multiple of it — with an absolute
//! floor for a filesystem where `fsync` is free, which is where a pure
//! multiple would be meaningless. Cold open does not `fsync` at all; it reads
//! and folds, so it gets a flat per-record ceiling.

// A guard that skips must say so. A silent skip on a filesystem that cannot be
// measured is indistinguishable from a pass, which is the one thing a guard
// must never be.
#![allow(clippy::print_stdout)]

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::store::Store;

const SPEC: &[u8] = include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

/// Instances created and events sent, each producing one journal record.
///
/// Measured on this host (Linux, 16 cores, `/tmp` on tmpfs) at 1 000
/// iterations per phase: 5.1 s in the debug profile and 0.7 s in release. At
/// the committed 200 the debug cost is well under a second, which fits beside
/// `crash_harness.rs` and `executor_chaos.rs` inside the 45-minute per-job
/// ceiling. `FSM_APPEND_GUARD_N` raises it for a real measurement run, so the
/// guard and the harness are one code path with two budgets.
const DEFAULT_ITERATIONS: usize = 200;

/// The environment variable that raises the iteration count.
const ITERATIONS_VARIABLE: &str = "FSM_APPEND_GUARD_N";

/// How many bare `fsync`s one append is allowed to cost.
///
/// An append writes one record line, `fsync`s the segment, folds the record
/// into the in-memory state, and hashes it into the chain. Everything but the
/// `fsync` is microseconds of CPU. Twelve is wide enough that a busy runner
/// does not fail and narrow enough that an append which grew a second
/// synchronous write, or a per-append walk of the journal, does not fit.
const APPEND_FSYNC_MULTIPLE: u128 = 12;

/// The append ceiling where `fsync` is free, in microseconds.
///
/// On tmpfs `sync_all` returns in a couple of microseconds, so
/// `APPEND_FSYNC_MULTIPLE` alone would assert something near zero. Measured on
/// this host (Linux, `/tmp` on tmpfs) the median append was 430 µs in the
/// debug profile and 59 µs in release; 4 000 µs leaves roughly nine times the
/// debug median. Raising this constant means saying why an append got slower.
const APPEND_FLOOR_MICROSECONDS: u128 = 4_000;

/// The cold-open ceiling, in microseconds per folded record.
///
/// A cold open reads every segment and folds every record; there is no
/// `fsync` in it, so it is CPU and sequential-read bound and a flat ceiling is
/// the honest shape. Measured on this host: 478 µs per record in the debug
/// profile and 58 µs in release, folding 2 001 records. 4 000 µs leaves eight
/// times the debug figure, which a fold that stopped being linear in the
/// record count would not fit inside.
const COLD_OPEN_CEILING_MICROSECONDS_PER_RECORD: u128 = 4_000;

static GUARD_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// A directory this test owns and removes.
///
/// The standing rule after a temporary-directory leak once exhausted this
/// host's inode table — which reads as a broken toolchain rather than a leaky
/// test — is that anything written goes somewhere the test removes.
struct GuardDirectory {
    path: PathBuf,
}

impl GuardDirectory {
    /// `None` when the environment cannot host a measurement at all.
    fn create() -> Option<Self> {
        let root = std::env::var_os("FSM_BENCH_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        let sequence = GUARD_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = root.join(format!(
            "fsm-append-guard-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&path).ok()?;
        Some(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for GuardDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn iterations_from(configured: Option<&str>) -> usize {
    configured
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .unwrap_or(DEFAULT_ITERATIONS)
}

fn iterations() -> usize {
    iterations_from(std::env::var(ITERATIONS_VARIABLE).ok().as_deref())
}

fn median(mut samples: Vec<u128>) -> u128 {
    // The median, not the mean: one scheduler stall on a shared runner moves a
    // mean and is exactly the noise this guard must not fail on.
    assert!(!samples.is_empty(), "no samples to summarize");
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// The median cost of one bare `write` + `sync_all` on this filesystem.
///
/// This is the part of an append that belongs to the disk rather than to this
/// code, and measuring it in the same run is what lets the append bound adapt
/// to hardware instead of guessing at it.
fn bare_fsync_median_microseconds(directory: &Path) -> Option<u128> {
    const PROBES: usize = 32;
    let path = directory.join("fsync-probe");
    let payload = vec![b'x'; 512];
    let mut samples = Vec::with_capacity(PROBES);
    for _ in 0..PROBES {
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()?;
        let start = Instant::now();
        file.write_all(&payload).ok()?;
        file.sync_all().ok()?;
        samples.push(start.elapsed().as_micros());
    }
    let _ = fs::remove_file(&path);
    Some(median(samples))
}

/// The bound an append is held to, given what a bare `fsync` costs here.
fn append_ceiling_microseconds(bare_fsync_microseconds: u128) -> u128 {
    (bare_fsync_microseconds * APPEND_FSYNC_MULTIPLE).max(APPEND_FLOOR_MICROSECONDS)
}

/// The comparison itself, kept out of the measurement so the guard can be
/// proved falsifiable by driving its own bound rather than by slowing a store.
fn check(label: &str, measured: u128, ceiling: u128, iterations: usize) -> Result<(), String> {
    if measured <= ceiling {
        return Ok(());
    }
    Err(format!(
        "{label} regressed: measured {measured}us, ceiling {ceiling}us, over {iterations} iterations. \
         A performance failure with no numbers cannot be triaged from a CI log, so they are all here. \
         If this is a deliberate cost, move the named constant in append_guard.rs and say why in the commit."
    ))
}

#[test]
fn append_and_cold_open_stay_inside_their_ceilings() {
    let Some(directory) = GuardDirectory::create() else {
        println!(
            "SKIP append_guard: no writable directory under FSM_BENCH_ROOT or the system \
             temporary directory, so there is nothing meaningful to measure here."
        );
        return;
    };
    let Some(bare_fsync) = bare_fsync_median_microseconds(directory.path()) else {
        println!(
            "SKIP append_guard: this filesystem would not accept a write-and-sync probe, so the \
             append bound has no baseline to adapt to."
        );
        return;
    };
    let count = iterations();
    let data_directory = directory.path().join("store");
    fs::create_dir(&data_directory).expect("guard directory is writable");

    let mut store = Store::open(&data_directory).expect("a fresh store opens");
    let definition = parse(SPEC, &JsonLimits::DEFAULT).expect("the committed machine parses");
    store
        .define_machine(definition, false, false)
        .expect("the committed machine is definable");

    let mut creates = Vec::with_capacity(count);
    for index in 0..count {
        let start = Instant::now();
        store
            .create_instance(
                "case_review",
                &format!("i{index}"),
                &format!("c{index}"),
                None,
            )
            .expect("create_instance succeeds");
        creates.push(start.elapsed().as_micros());
    }
    let mut sends = Vec::with_capacity(count);
    for index in 0..count {
        let start = Instant::now();
        store
            .send_event(
                &format!("i{index}"),
                "docs_ok",
                Value::Obj(BTreeMap::new()),
                &format!("s{index}"),
                None,
            )
            .expect("send_event succeeds");
        sends.push(start.elapsed().as_micros());
    }
    let records = store.journal.last_seq;
    drop(store);

    let ceiling = append_ceiling_microseconds(bare_fsync);
    let mut failures = Vec::new();
    for (label, samples) in [("create_instance", creates), ("send_event", sends)] {
        if let Err(message) = check(label, median(samples), ceiling, count) {
            failures.push(message);
        }
    }

    // Genuinely cold: a snapshot is a disposable cache, and clearing it forces
    // the whole-journal fold an embedder pays after an unclean shutdown. That
    // fold is what sealing exists to bound, so it is what this guard watches.
    let _ = fs::remove_dir_all(fsm_store::snapshot::snap_dir(&data_directory));
    let start = Instant::now();
    let reopened = Store::open(&data_directory).expect("the store reopens");
    let cold = start.elapsed().as_micros();
    assert!(
        !reopened.opened_from_snapshot,
        "the cold-open measurement used a snapshot, so it measured the wrong path"
    );
    drop(reopened);
    let per_record = cold / u128::from(records.max(1));
    if let Err(message) = check(
        "cold open per folded record",
        per_record,
        COLD_OPEN_CEILING_MICROSECONDS_PER_RECORD,
        count,
    ) {
        failures.push(format!("{message} ({records} records folded in {cold}us)"));
    }

    assert!(
        failures.is_empty(),
        "append guard failed with a bare fsync median of {bare_fsync}us:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_guard_fails_when_its_ceiling_is_below_the_measurement() {
    // Driving the bound directly, rather than slowing a store down: a guard
    // that cannot be made to fail is not a guard, and proving it by making the
    // real thing slow would be a test of the slowdown.
    let failure = check("append", 5_000, 4_000, 200).expect_err("5000us must not fit under 4000us");
    assert!(
        check("append", 4_000, 4_000, 200).is_ok(),
        "the bound is inclusive"
    );
    assert!(check("append", 3_999, 4_000, 200).is_ok());
    assert!(
        failure.contains("5000us"),
        "the measured value is missing: {failure}"
    );
    assert!(
        failure.contains("4000us"),
        "the ceiling is missing: {failure}"
    );
    assert!(
        failure.contains("200 iterations"),
        "the iteration count is missing: {failure}"
    );
}

#[test]
fn the_append_ceiling_adapts_to_the_filesystem_above_its_floor() {
    // Where fsync is free the floor applies; where it is expensive the
    // multiple does, so one constant does not have to cover both worlds.
    assert_eq!(append_ceiling_microseconds(2), APPEND_FLOOR_MICROSECONDS);
    assert_eq!(append_ceiling_microseconds(0), APPEND_FLOOR_MICROSECONDS);
    assert_eq!(append_ceiling_microseconds(1_000), 12_000);
    assert!(
        append_ceiling_microseconds(APPEND_FLOOR_MICROSECONDS) > APPEND_FLOOR_MICROSECONDS,
        "a filesystem whose bare fsync already costs the floor must not be held to the floor"
    );
}

#[test]
fn the_iteration_count_comes_from_the_environment_or_the_committed_default() {
    // Read through a pure function rather than by setting the variable: in
    // edition 2024 `set_var` is unsafe, and a test that mutates the process
    // environment is a test that cannot run beside another one.
    assert_eq!(iterations_from(None), DEFAULT_ITERATIONS);
    assert_eq!(iterations_from(Some("5000")), 5_000);
    assert_eq!(iterations_from(Some("0")), DEFAULT_ITERATIONS);
    assert_eq!(iterations_from(Some("not a number")), DEFAULT_ITERATIONS);
    assert_eq!(ITERATIONS_VARIABLE, "FSM_APPEND_GUARD_N");
}

#[test]
fn the_guard_directory_is_removed_with_the_guard() {
    let path = {
        let directory = GuardDirectory::create().expect("a temporary directory is creatable");
        let path = directory.path().to_path_buf();
        fs::write(path.join("leftover"), b"x").expect("the directory is writable");
        assert!(path.is_dir());
        path
    };
    assert!(
        !path.exists(),
        "the guard left {path:?} behind; a leaked temporary directory once exhausted this host's inode table"
    );
}
