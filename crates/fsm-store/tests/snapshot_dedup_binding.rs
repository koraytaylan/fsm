//! Snapshot fast-path proof for the request-id fingerprint ledger.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::clock::FixedClock;
use fsm_store::snapshot::{listed_snaps, snapshot_to_state, state_to_snapshot};
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-store-snapshot-dedup-{}-{sequence}",
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

fn machine_definition() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"snapshot_dedup",
            "context":[],
            "events":[],
            "states":[{"name":"waiting"}],
            "initial":"waiting",
            "transitions":[]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("snapshot fixture is valid JSON")
}

#[test]
fn bound_snapshot_cannot_forge_a_request_fingerprint() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).expect("open empty store");
    let mut define_clock = FixedClock::new(1, 1);
    store
        .define_machine_on(&mut define_clock, machine_definition(), false, false)
        .expect("define machine");
    let mut create_clock = FixedClock::new(2, 1);
    store
        .create_instance_ctx_on(
            &mut create_clock,
            "snapshot_dedup",
            "case-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .expect("create instance");
    let mut snapshot_clock = FixedClock::new(3, 1);
    store
        .shutdown_snapshot_on(&mut snapshot_clock)
        .expect("write bound snapshot");
    drop(store);

    // `shutdown_snapshot_on` writes the bound snapshot and `Drop` may add a
    // second clean-shutdown cache at the same sequence. Forge every disposable
    // cache so no authentic sibling can mask the candidate under test.
    let snapshot_paths: Vec<_> = listed_snaps(directory.path())
        .into_iter()
        .map(|(_, path)| path)
        .collect();
    assert!(!snapshot_paths.is_empty(), "snapshot exists");
    let forged_fingerprint = format!("sha256:{}", "0".repeat(64));
    for snapshot_path in &snapshot_paths {
        let snapshot = parse(
            &fs::read(snapshot_path).expect("read snapshot"),
            &JsonLimits::DEFAULT,
        )
        .expect("parse snapshot");
        let mut forged_state = snapshot_to_state(&snapshot).expect("materialize snapshot");
        forged_state
            .dedup
            .get_mut("create-1")
            .expect("request slot")
            .fp = Some(forged_fingerprint.clone());
        let forged_snapshot = state_to_snapshot(&forged_state);
        fs::write(snapshot_path, canon_bytes(&forged_snapshot)).expect("rewrite snapshot cache");
    }

    let mut reopened = Store::open(directory.path()).expect("fall back to journal fold");
    assert!(
        !reopened.opened_from_snapshot,
        "a fingerprint not authenticated by its claiming record is not a fast-path anchor"
    );
    assert_ne!(
        reopened.state.dedup["create-1"].fp,
        Some(forged_fingerprint)
    );
    let seq = reopened.state.last_seq;
    let duplicate = reopened
        .create_instance_ctx_on(
            &mut FixedClock::new(999, 1),
            "snapshot_dedup",
            "case-1",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .expect("identical retry replays instead of conflicting");
    assert_eq!(
        duplicate.get("duplicate").and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(reopened.state.last_seq, seq, "retry must not append");
}
