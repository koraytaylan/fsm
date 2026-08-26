//! Interrupt a cohort migration at every instance boundary and assert what
//! the journal says afterwards.
//!
//! A bulk migration is N idempotent operations, not a transaction, so the
//! question is not "did it finish" but "can it be finished": exactly one
//! record per instance however many times it was interrupted, no instance
//! left half-migrated, and a cohort that completes on the next run.
//!
//! The word is **restart**, not `kill -9`, for the reason `executor_chaos.rs`
//! gives: signal-kill coverage of the journal lives in `crash_harness.rs`,
//! and what this proves is that a fresh writer's derived keys resume
//! correctly with nothing carried in memory.
//!
//! **Budget.** Store operations only, no spawns: 64 seeded interruptions
//! measure at 11.7 s debug on this machine — a few seconds per CI job rather
//! than a few minutes. The committed default is 64;
//! `FSM_MIGRATION_CHAOS_ITERS` raises it and `MIGRATION_CHAOS_SEED` replays
//! one.
//!
//! Plan 0011 task 5601.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::journal_io::{JournalHealth, verify};
use fsm_cli::store::Store;
use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::Status;
use fsm_core::record::RecordKind;
use fsm_store::clock::FixedClock;

/// Never lower this floor.
const ITERATIONS: u64 = 64;
const COHORT: usize = 8;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-migchaos-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn next_seed(state: &mut u64) -> u64 {
    let mut seed = *state;
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    *state = seed;
    seed
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source))).unwrap().to_string()
}

const OLD: &str = r#"{"format":"fsm.machine/1","name":"chaos_v1","states":[{"name":"intake"},{"name":"stuck"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"1"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"stuck"}]}"#;

fn new_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"chaos_v2","states":[{{"name":"intake"}},{{"name":"stuck"}}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"stuck"}}],"supersedes":{{"machine":"{}","states":{{"intake":"intake"}},"context":{{"score":"ctx.score + 1"}}}}}}"#,
        digest(OLD)
    )
}

/// The instances a cohort run would move, in the order it would move them.
fn cohort_ids() -> Vec<String> {
    (0..COHORT)
        .map(|index| format!("inst-{index:02}"))
        .collect()
}

/// The derived key a cohort run uses, which is the whole of its resumability.
fn request_id(instance_id: &str, to_machine_id: &str) -> String {
    format!("migrate-{instance_id}-{to_machine_id}")
}

fn build(directory: &TestDirectory, stuck: usize) -> String {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    for source in [OLD.to_string(), new_source()] {
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
    }
    for (index, id) in cohort_ids().into_iter().enumerate() {
        store
            .create_instance_ctx_on(
                &mut clock,
                "chaos_v1",
                &id,
                &format!("create-{index}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        // The last `stuck` instances move to the leaf the mapping does not
        // cover, so every run has known exclusions to skip.
        if index >= COHORT - stuck {
            store
                .send_event(
                    &id,
                    "go",
                    Value::Obj(BTreeMap::new()),
                    &format!("go-{index}"),
                    None,
                )
                .unwrap();
        }
    }
    store
        .resolve_machine("chaos_v2")
        .unwrap()
        .compiled
        .machine_id
        .clone()
}

/// Migrate up to `limit` instances, then drop the store: a restart.
///
/// This deliberately re-attempts **every** instance, including ones a
/// previous run already moved — a stronger test of the derived key than the
/// cohort command, which selects only instances still on the source machine.
/// A replay is not progress and does not count against the limit.
fn run_until(directory: &TestDirectory, to_machine_id: &str, limit: usize) -> usize {
    let mut store = Store::open(directory.path()).unwrap();
    let mut moved = 0;
    for id in cohort_ids() {
        if moved >= limit {
            break;
        }
        let request = request_id(&id, to_machine_id);
        match store.migrate_instance_on(&mut FixedClock::new(9_000, 1), &id, "chaos_v2", &request) {
            Ok(response) => {
                if response.get("duplicate").and_then(Value::as_bool) != Some(true) {
                    moved += 1;
                }
            }
            // A known exclusion is not progress and not a failure.
            Err(error) => assert_eq!(error.code, "req/migrate_unmapped", "{id}: {error:?}"),
        }
    }
    moved
}

/// Every invariant this harness defends.
fn assert_coherent(directory: &TestDirectory, seed: u64, at: usize) {
    let report = verify(directory.path());
    assert!(
        matches!(report.health, JournalHealth::Ok),
        "seed {seed} at {at}: journal not clean: {:?}",
        report.health
    );
    let store = Store::open(directory.path()).unwrap();
    let mut migrated: BTreeSet<String> = BTreeSet::new();
    for record in &store.records {
        if record.kind != RecordKind::InstanceMigrated {
            continue;
        }
        let id = record
            .body
            .get("instance_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        assert!(
            migrated.insert(id.clone()),
            "seed {seed} at {at}: {id} migrated twice"
        );
    }
    let target = machine_id(&value(&new_source()));
    for id in cohort_ids() {
        let on_new = store.state.instance_machines.get(&id) == Some(&target);
        assert_eq!(
            on_new,
            migrated.contains(&id),
            "seed {seed} at {at}: {id} is half-migrated"
        );
        assert_eq!(
            store.state.instances[&id].status,
            Status::Running,
            "seed {seed} at {at}: {id} settled"
        );
    }
}

fn run_one(seed: u64) {
    let stuck = (seed % 3) as usize;
    let interrupt_at = (seed / 3) as usize % (COHORT - stuck + 1);
    let directory = TestDirectory::create();
    let target = build(&directory, stuck);

    // Interrupt part-way, and assert the store is coherent at the boundary.
    let first = run_until(&directory, &target, interrupt_at);
    assert_coherent(&directory, seed, interrupt_at);

    // A fresh writer finishes the cohort.
    let second = run_until(&directory, &target, usize::MAX);
    assert_coherent(&directory, seed, interrupt_at);
    assert_eq!(
        first + second,
        COHORT - stuck,
        "seed {seed}: the cohort did not finish"
    );

    // A third run has nothing left to do, and writes nothing.
    let store = Store::open(directory.path()).unwrap();
    let records = store.records.len();
    drop(store);
    let third = run_until(&directory, &target, usize::MAX);
    assert_eq!(third, 0, "seed {seed}: a finished cohort moved again");
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(
        store.records.len(),
        records,
        "seed {seed}: a finished cohort wrote again"
    );
    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::InstanceMigrated)
            .count(),
        COHORT - stuck,
        "seed {seed}: one record per migrated instance, no more"
    );
}

fn iterations() -> u64 {
    std::env::var("FSM_MIGRATION_CHAOS_ITERS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map_or(ITERATIONS, |count| count.max(ITERATIONS))
}

#[test]
fn an_interrupted_cohort_finishes_with_one_record_per_instance() {
    if let Ok(raw) = std::env::var("MIGRATION_CHAOS_SEED") {
        run_one(
            raw.parse::<u64>()
                .expect("MIGRATION_CHAOS_SEED is a number"),
        );
        return;
    }
    let mut state = 0xB5AD_4ECE_DA10_1234;
    for _ in 0..iterations() {
        run_one(next_seed(&mut state));
    }
}

#[test]
fn a_replayed_key_is_what_makes_the_resume_free() {
    let directory = TestDirectory::create();
    let target = build(&directory, 0);
    let mut store = Store::open(directory.path()).unwrap();
    let request = request_id("inst-00", &target);
    let first = store
        .migrate_instance_on(
            &mut FixedClock::new(9_000, 1),
            "inst-00",
            "chaos_v2",
            &request,
        )
        .unwrap();
    let records = store.records.len();
    drop(store);
    // A fresh writer re-derives the identical key from journaled content.
    let mut resumed = Store::open(directory.path()).unwrap();
    let again = resumed
        .migrate_instance_on(
            &mut FixedClock::new(9_500, 1),
            "inst-00",
            "chaos_v2",
            &request,
        )
        .unwrap();
    assert_eq!(again.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(again.get("seq"), first.get("seq"));
    assert_eq!(resumed.records.len(), records);
}
