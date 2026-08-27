//! Restart at every point where composition could be interrupted, and assert
//! what the journal says afterwards.
//!
//! Composition adds edges between instances, and an edge is exactly the thing
//! a crash can leave half-written: a slot with no child, a child with no
//! parent, a signal delivered twice. Every enactment point gets the same
//! restart treatment plan 0008 gave the executor.
//!
//! The word is **restart**, not `kill -9`, for the reason `executor_chaos.rs`
//! gives: signal-kill coverage of the journal itself lives in
//! `crash_harness.rs`, and what this proves is different — that a fresh
//! writer's journal-derived decisions resume correctly with nothing carried
//! in memory. Death is simulated by dropping the `Store` and opening a new
//! one against the same directory.
//!
//! The seeded generator below is a deliberate ~10-line duplication of the
//! xorshift64\* in `chaos.rs`, `proputil.rs`, and `executor_chaos.rs`, for the
//! reason documented there: a bug in one generator must not hide in another.
//!
//! **Budget.** The 45-minute CI ceiling is shared, and `crash_harness.rs`
//! (1,000 spawns per profile) plus `executor_chaos.rs` (200 iterations that
//! each spawn a process) already dominate it. This suite spawns nothing — it
//! is store operations only. Measured on this machine: 15.8 s for 200
//! iterations in debug, 2.6 s in release — a few seconds per CI job rather
//! than a few minutes, so the committed default is the full 200.
//! `FSM_COMPOSITION_CHAOS_ITERS` raises it; `COMPOSITION_CHAOS_SEED` replays
//! exactly one, and both are printed on failure so a red run is a
//! reproduction.
//!
//! Plan 0010 task 5201.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::journal_io::{JournalHealth, verify};
use fsm_core::hashes::{child_instance_id, digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{InvokeStatus, Status};
use fsm_core::record::RecordKind;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

/// Never lower this floor.
const ITERATIONS: u64 = 200;
const BASE_MS: i64 = 1_700_000_000_000;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Where the restart lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeathPoint {
    /// Before the slot is enacted at all.
    BeforeInvoke,
    /// After the child exists, before it is sent anything.
    AfterInvoke,
    /// After the child settles, before its result reaches the parent.
    BeforeReturn,
    /// Between a parent-exit transition and the cascade that cancels its
    /// child — the plan's one two-record window.
    MidCascade,
    /// After a signal is emitted, before it is delivered.
    MidSignal,
}

impl DeathPoint {
    fn of(seed: u64) -> Self {
        match seed % 5 {
            0 => DeathPoint::BeforeInvoke,
            1 => DeathPoint::AfterInvoke,
            2 => DeathPoint::BeforeReturn,
            3 => DeathPoint::MidCascade,
            _ => DeathPoint::MidSignal,
        }
    }
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-composition-chaos-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory {path:?}: {error}"),
            }
        }
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

/// xorshift64\*, kept local on purpose. See the module header.
fn next_seed(state: &mut u64) -> u64 {
    let mut seed = *state;
    seed ^= seed << 13;
    seed ^= seed >> 7;
    seed ^= seed << 17;
    *state = seed;
    seed
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("a fixture machine is valid JSON")
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source)))
        .expect("a machine id names a digest")
        .to_string()
}

/// The audit step at the bottom of every tree: it does its work and reports.
const AUDIT: &str = r#"{"format":"fsm.machine/1","name":"audit","states":[{"name":"reviewing"},{"name":"filed","terminal":true}],"initial":"reviewing","context":[{"name":"finding","ty":"str","init":"open"}],"events":[{"name":"file","fields":[]}],"transitions":[{"from":"reviewing","on":"file","to":"filed","do":[{"target":"finding","value":"\"clear\""}]}]}"#;

/// A review step that delegates to the step below it, and can be told to walk
/// away from the delegation while it is still running.
fn review(name: &str, below: &str, slots: usize, child_field: &str) -> String {
    // The projection names a field the *child* declares: the audit reports
    // `finding`, a review below this one reports `seen`.
    let invokes: Vec<String> = (0..slots)
        .map(|index| {
            format!(
                r#"{{"id":"step{index}","machine":"{below}","returns":{{"finding":"{child_field}"}}}}"#
            )
        })
        .collect();
    let handlers: Vec<String> = (0..slots)
        .map(|index| {
            format!(
                r#",{{"from":"delegating","on":"$done.invoke.step{index}","to":"decided","do":[{{"target":"seen","value":"evt.finding"}}]}}"#
            )
        })
        .collect();
    format!(
        r#"{{"format":"fsm.machine/1","name":"{name}","states":[{{"name":"intake"}},{{"name":"delegating","invoke":[{}]}},{{"name":"decided","terminal":true}}],"initial":"intake","context":[{{"name":"seen","ty":"str","init":""}}],"events":[{{"name":"open","fields":[]}},{{"name":"withdraw","fields":[]}}],"transitions":[{{"from":"intake","on":"open","to":"delegating"}},{{"from":"delegating","on":"withdraw","to":"decided"}}{}]}}"#,
        invokes.join(","),
        handlers.concat()
    )
}

/// A notifier that signals its counterpart when it opens.
const NOTIFIER: &str = r#"{"format":"fsm.machine/1","name":"notifier","states":[{"name":"idle"},{"name":"announced","entry":{"signal":[{"to":"ctx.desk","event":"file"}]}}],"initial":"idle","context":[{"name":"desk","ty":"str","init":"inst-desk"}],"events":[{"name":"open","fields":[]}],"transitions":[{"from":"idle","on":"open","to":"announced"}]}"#;

/// Every invariant this harness defends, checked against a fresh open.
fn assert_coherent(directory: &TestDirectory, seed: u64, point: DeathPoint) {
    let report = verify(directory.path());
    assert!(
        matches!(report.health, JournalHealth::Ok),
        "seed {seed} {point:?}: journal not clean: {:?}",
        report.health
    );
    let store = Store::open(directory.path()).unwrap_or_else(|error| {
        panic!("seed {seed} {point:?}: reopen failed: {error:?}");
    });

    // Exactly one enactment record per slot, in either direction.
    let mut invoked: BTreeSet<(String, String)> = BTreeSet::new();
    let mut returned: BTreeSet<(String, String)> = BTreeSet::new();
    let mut delivered: BTreeSet<(String, String)> = BTreeSet::new();
    for record in &store.records {
        let field = |name: &str| {
            record
                .body
                .get(name)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        match record.kind {
            RecordKind::InstanceInvoked => assert!(
                invoked.insert((field("parent_instance_id"), field("slot"))),
                "seed {seed} {point:?}: a slot was invoked twice"
            ),
            RecordKind::InvocationReturned => assert!(
                returned.insert((field("parent_instance_id"), field("slot"))),
                "seed {seed} {point:?}: a slot returned twice"
            ),
            RecordKind::SignalDelivered => assert!(
                delivered.insert((field("sender_instance_id"), field("signal_id"))),
                "seed {seed} {point:?}: a signal was delivered twice"
            ),
            _ => {}
        }
    }
    for (parent, slot) in &returned {
        assert!(
            invoked.contains(&(parent.clone(), slot.clone())),
            "seed {seed} {point:?}: {parent}/{slot} returned without being invoked"
        );
    }

    for (id, instance) in &store.state.instances {
        // Every child is derivable from the record that created it.
        if let Some((parent, slot)) = store.parent_of(id) {
            assert_eq!(
                child_instance_id(&parent, &slot),
                *id,
                "seed {seed} {point:?}: {id} is not derivable from its own record"
            );
            assert!(invoked.contains(&(parent, slot)));
        }
        // A running slot names a child that exists.
        for (slot, invocation) in &instance.invocations {
            if invocation.status == InvokeStatus::Running {
                let child = child_instance_id(id, slot);
                assert!(
                    store.state.instances.contains_key(&child),
                    "seed {seed} {point:?}: {id}/{slot} is running with no child"
                );
            }
        }
        // Every instance is settled, or waiting on something that exists.
        if instance.status == Status::Running {
            let waiting_on_a_ghost = instance.signals.values().any(|signal| {
                !store
                    .state
                    .instances
                    .contains_key(&signal.target_instance_id)
                    && signal.target_instance_id != *id
            });
            assert!(
                !waiting_on_a_ghost,
                "seed {seed} {point:?}: {id} holds a signal for an instance that does not exist"
            );
        }
    }
}

/// One seeded run: build a tree, die at the seed's point, resume, and assert.
fn run_one(seed: u64) {
    let point = DeathPoint::of(seed);
    let depth = 2 + usize::from(seed.is_multiple_of(3));
    let slots = 1 + usize::from(seed.is_multiple_of(7));
    let directory = TestDirectory::create();
    let mut clock = FixedClock::new(BASE_MS + (seed % 1_000) as i64, 1);

    // Define the tree: audit at the bottom, review steps above it.
    let mut store = Store::open(directory.path()).unwrap();
    store
        .define_machine_on(&mut clock, value(AUDIT), false, false)
        .unwrap();
    let mut below = digest(AUDIT);
    let mut root = String::new();
    for level in 0..depth {
        let name = format!("review{level}");
        // Only the top holds two slots when the seed asks for it: a deeper
        // level with two slots would double the tree on every level.
        let child_field = if level == 0 { "finding" } else { "seen" };
        let source = review(
            &name,
            &below,
            if level + 1 == depth { slots } else { 1 },
            child_field,
        );
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
        below = digest(&source);
        root = name;
    }
    store
        .define_machine_on(&mut clock, value(NOTIFIER), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            &root,
            "inst-case",
            "c-case",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "audit",
            "inst-desk",
            "c-desk",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event(
            "inst-case",
            "open",
            Value::Obj(BTreeMap::new()),
            "open-case",
            None,
        )
        .unwrap();

    if point == DeathPoint::BeforeInvoke {
        drop(store);
        assert_coherent(&directory, seed, point);
        // A fresh writer enacts the slot it inherited.
        let mut resumed = Store::open(directory.path()).unwrap();
        resumed.invoke_child("inst-case", "step0", "inv-0").unwrap();
        drop(resumed);
        assert_coherent(&directory, seed, point);
        return;
    }

    store.invoke_child("inst-case", "step0", "inv-0").unwrap();
    let child = child_instance_id("inst-case", "step0");

    if point == DeathPoint::AfterInvoke {
        drop(store);
        assert_coherent(&directory, seed, point);
        // The same key replays rather than invoking a second child.
        let mut resumed = Store::open(directory.path()).unwrap();
        let replayed = resumed.invoke_child("inst-case", "step0", "inv-0").unwrap();
        assert_eq!(
            replayed.get("duplicate").and_then(Value::as_bool),
            Some(true)
        );
        drop(resumed);
        assert_coherent(&directory, seed, point);
        return;
    }

    if point == DeathPoint::MidCascade {
        // The parent walks away while the child runs; the cascade's second
        // record is dropped, exactly as a crash between the two would.
        store
            .send_event(
                "inst-case",
                "withdraw",
                Value::Obj(BTreeMap::new()),
                "withdraw-1",
                None,
            )
            .unwrap();
        let mut records = store.records.clone();
        let cancel = records
            .iter()
            .rposition(|record| record.kind == RecordKind::InstanceCancelled)
            .expect("the cascade wrote one");
        assert_eq!(cancel, records.len() - 1);
        records.truncate(cancel);
        drop(store);
        let bytes: Vec<u8> = records
            .iter()
            .flat_map(fsm_core::record::Record::to_line)
            .collect();
        fs::write(
            directory
                .path()
                .join("journal/seg-00000000000000000000.jsonl"),
            bytes,
        )
        .unwrap();

        // Coherent, and the child is running but unreferenced.
        assert_coherent(&directory, seed, point);
        let before = fs::metadata(
            directory
                .path()
                .join("journal/seg-00000000000000000000.jsonl"),
        )
        .unwrap()
        .len();
        let mut resumed = Store::open(directory.path()).unwrap();
        assert_eq!(
            fs::metadata(
                directory
                    .path()
                    .join("journal/seg-00000000000000000000.jsonl")
            )
            .unwrap()
            .len(),
            before,
            "seed {seed}: an open wrote records"
        );
        assert_eq!(resumed.state.instances[&child].status, Status::Running);
        let orphans = resumed.orphaned_children();
        assert_eq!(orphans.len(), 1, "seed {seed}: {orphans:?}");
        let records_before = resumed.records.len();
        let cancelled = resumed
            .cancel_orphans_on(&mut clock, &format!("repair-{seed}"))
            .unwrap();
        assert_eq!(cancelled, vec![child.clone()]);
        assert_eq!(resumed.records.len(), records_before + 1);
        assert!(resumed.orphaned_children().is_empty());
        drop(resumed);
        assert_coherent(&directory, seed, point);
        return;
    }

    if point == DeathPoint::MidSignal {
        store
            .create_instance_ctx_on(
                &mut clock,
                "notifier",
                "inst-note",
                "c-note",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        store
            .send_event(
                "inst-note",
                "open",
                Value::Obj(BTreeMap::new()),
                "open-note",
                None,
            )
            .unwrap();
        let signal_id = store.state.instances["inst-note"]
            .signals
            .keys()
            .next()
            .cloned()
            .expect("the entry block emitted one");
        drop(store);
        assert_coherent(&directory, seed, point);

        let mut resumed = Store::open(directory.path()).unwrap();
        let first = resumed
            .signal_deliver("inst-note", &signal_id, "sig-1")
            .unwrap();
        drop(resumed);
        assert_coherent(&directory, seed, point);
        // And a restarted deliverer replays rather than delivering twice.
        let mut again = Store::open(directory.path()).unwrap();
        let replayed = again
            .signal_deliver("inst-note", &signal_id, "sig-1")
            .unwrap();
        assert_eq!(
            replayed.get("duplicate").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(replayed.get("outcome"), first.get("outcome"));
        assert!(again.state.instances["inst-note"].signals.is_empty());
        drop(again);
        assert_coherent(&directory, seed, point);
        return;
    }

    // BeforeReturn: drive the whole tree down to the audit at the bottom,
    // settle it, die, and let a fresh writer walk the results back up.
    let mut chain = vec!["inst-case".to_string(), child.clone()];
    for level in 1..depth {
        let current = chain.last().unwrap().clone();
        store
            .send_event(
                &current,
                "open",
                Value::Obj(BTreeMap::new()),
                &format!("open-{level}"),
                None,
            )
            .unwrap();
        store
            .invoke_child(&current, "step0", &format!("inv-{level}"))
            .unwrap();
        chain.push(child_instance_id(&current, "step0"));
    }
    let audit = chain.last().unwrap().clone();
    store
        .send_event(&audit, "file", Value::Obj(BTreeMap::new()), "file-1", None)
        .unwrap();
    assert_eq!(store.state.instances[&audit].status, Status::Completed);
    drop(store);
    assert_coherent(&directory, seed, point);

    // Every level returns, deepest first, each one settling its own parent.
    let mut resumed = Store::open(directory.path()).unwrap();
    for (level, parent) in chain.iter().take(chain.len() - 1).enumerate().rev() {
        resumed
            .invocation_return(parent, "step0", &format!("ret-{level}"))
            .unwrap_or_else(|error| panic!("seed {seed}: return {parent}: {error:?}"));
    }
    assert_eq!(
        resumed.state.instances["inst-case"].ctx["seen"].canonical_string(),
        "clear",
        "seed {seed}: the audit's finding reached the root through every level"
    );
    drop(resumed);
    assert_coherent(&directory, seed, point);

    // A second return after another restart replays rather than re-delivering.
    let mut third = Store::open(directory.path()).unwrap();
    let replayed = third
        .invocation_return("inst-case", "step0", "ret-0")
        .unwrap();
    assert_eq!(
        replayed.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    drop(third);
    assert_coherent(&directory, seed, point);
}

fn iterations() -> u64 {
    std::env::var("FSM_COMPOSITION_CHAOS_ITERS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map_or(ITERATIONS, |count| count.max(ITERATIONS))
}

#[test]
fn a_restart_at_every_composition_point_leaves_one_record_per_edge() {
    if let Ok(raw) = std::env::var("COMPOSITION_CHAOS_SEED") {
        let seed = raw
            .parse::<u64>()
            .expect("COMPOSITION_CHAOS_SEED is a number");
        run_one(seed);
        return;
    }
    let mut state = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..iterations() {
        let seed = next_seed(&mut state);
        run_one(seed);
    }
}

#[test]
fn the_generator_is_deterministic_so_a_failure_can_be_replayed() {
    let mut first = 0x9E37_79B9_7F4A_7C15;
    let mut second = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..32 {
        assert_eq!(next_seed(&mut first), next_seed(&mut second));
    }
    // And every death point is reachable, so no seed space is dead.
    let mut state = 0x9E37_79B9_7F4A_7C15;
    let mut points = BTreeSet::new();
    for _ in 0..64 {
        points.insert(format!("{:?}", DeathPoint::of(next_seed(&mut state))));
    }
    assert_eq!(points.len(), 5, "{points:?}");
}
