//! Abrupt writer death after durable APIs; fixture proofs do not authenticate native closure.

use super::*;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn legacy_admission_requires_matching_prefix_and_replays_after_cold_reopen() {
    // This tests producer persistence with preauthenticated fixture proof;
    // it does not authenticate a native legacy environment or issue receipts.
    use fsm_core::record::execution::Admission;

    let directory = Directory(std::env::temp_dir().join(format!(
        "fsm-legacy-admission-{}-{}",
        std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    )));
    let journal = directory.0.join("journal");
    std::fs::create_dir_all(&journal).unwrap();
    let segment = journal.join("seg-00000000000000000000.jsonl");
    let historical = include_bytes!("../../tests/fixtures/non_reactive_session.journal");
    std::fs::write(&segment, historical).unwrap();
    std::fs::write(directory.0.join("VERSION"), b"10\n").unwrap();
    let mut store = Store::open(&directory.0).unwrap();
    assert_eq!(store.state.execution.admission(), Admission::Quarantined);
    assert_eq!(std::fs::read(&segment).unwrap(), historical);
    let original_head = format!("sha256:{}", store.journal.last_hash);
    let witness = |previous_head: String| VerifiedQuiescence {
        value: Value::Obj(BTreeMap::from([
            ("domain".into(), domain().to_value()),
            (
                "receipt".into(),
                Value::Str(format!("sha256:{}", "b".repeat(64))),
            ),
            ("previous_head".into(), Value::Str(previous_head.clone())),
        ])),
        previous_head,
    };
    let proof = witness(original_head);
    let stale = witness(format!("sha256:{}", "e".repeat(64)));
    let mut clock = FixedClock::new(100, 1);
    let sequence = store.journal.last_seq;
    assert_eq!(
        store
            .enable_execution_on(&mut clock, &stale, "enable")
            .unwrap_err()
            .code,
        "store/execution_evidence"
    );
    assert_eq!(clock.now, 100);
    assert_eq!(store.journal.last_seq, sequence);
    assert!(!store.state.dedup.contains_key("enable"));
    assert_eq!(store.state.execution.admission(), Admission::Quarantined);
    let response = store
        .enable_execution_on(&mut clock, &proof, "enable")
        .unwrap();
    assert_eq!(store.state.execution.admission(), Admission::Enabled);
    assert_eq!(store.state.execution.run_high_water(), 0);
    assert_eq!(store.journal.last_seq, sequence + 1);
    assert_fold(&store);
    let committed = store.state.clone();
    let hash = store.journal.last_hash.clone();
    drop(store);
    let snapshots = directory.0.join("snapshots");
    if snapshots.exists() {
        std::fs::remove_dir_all(snapshots).unwrap();
    }
    let mut store = Store::open(&directory.0).unwrap();
    assert!(!store.opened_from_snapshot);
    store.last_responses.clear();
    let mut clock = FixedClock::new(500, 1);
    let replay = store
        .enable_execution_on(&mut clock, &proof, "enable")
        .unwrap();
    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(replay.get("execution"), response.get("execution"));
    assert_eq!(clock.now, 500);
    assert_eq!(store.journal.last_seq, sequence + 1);
    assert_eq!(store.journal.last_hash, hash);
    assert!(crate::snapshot::store_states_eq(&committed, &store.state));
    assert_eq!(
        store
            .enable_execution_on(&mut clock, &proof, "enable-again")
            .unwrap_err()
            .code,
        "store/execution_evidence"
    );
    assert!(!store.state.dedup.contains_key("enable-again"));
    assert_eq!(store.journal.last_seq, sequence + 1);
    assert_eq!(store.journal.last_hash, hash);
    assert_eq!(
        std::fs::read(&segment).unwrap().get(..historical.len()),
        Some(historical.as_slice())
    );
    assert_fold(&store);
    assert!(matches!(
        crate::journal_io::verify(&directory.0).health,
        crate::journal_io::JournalHealth::Ok
    ));
}

#[test]
fn stopped_owner_survives_repeated_seals_and_spent_archived_completion_is_stale() {
    let directory = Directory(std::env::temp_dir().join(format!(
        "fsm-stopped-seals-{}-{}", std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    )));
    std::fs::create_dir_all(&directory.0).unwrap();
    let (mut store, effect) = pending_store(Store::open(&directory.0).unwrap());
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    let evidence = proof(&store, &claim);
    let original_hash = evidence.journal_claim.clone();
    stop(&mut store, &claim, "timeout", "stop");
    let stopped = store
        .state
        .execution
        .stopped_for("instance", &effect)
        .unwrap()
        .to_value();
    // Cancellation removes the pending-effect pin without consuming the
    // unresolved stopped owner, so the real base must carry the result.
    store.cancel_instance("instance", "cancel").unwrap();
    for index in 0..2 {
        let archive = directory.0.join(format!("archive-{index}"));
        std::fs::create_dir_all(&archive).unwrap();
        store.seal_and_archive(&archive, None).unwrap();
        crate::archive::verify(&archive).unwrap();
        let base = crate::base::open_from_base(&directory.0, &store.records).unwrap();
        assert_eq!(base.index.execution_claims.get(&1), Some(&original_hash));
        assert_eq!(
            base.state
                .execution
                .stopped_for("instance", &effect)
                .unwrap()
                .to_value(),
            stopped
        );
        drop(store);
        let read_only = Store::open_read_only(&directory.0).unwrap();
        assert_eq!(
            read_only.state.execution.claim_for("instance", &effect),
            Some(&claim)
        );
        assert_eq!(
            read_only
                .state
                .execution
                .stopped_for("instance", &effect)
                .unwrap()
                .to_value(),
            stopped
        );
        drop(read_only);
        store = Store::open(&directory.0).unwrap();
        assert_eq!(store.execution_claim_hash(&claim).unwrap(), original_hash);
        assert_eq!(store.state.execution.run_high_water(), 1);
        assert_eq!(store.state.execution.unresolved().count(), 1);
        assert!(matches!(
            crate::journal_io::verify(&directory.0).health,
            crate::journal_io::JournalHealth::Ok
        ));
    }
    store
        .settle_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim: &claim,
                disposition: Settlement::Interrupted,
                request_id: "consume",
                expected_seq: None,
            },
        )
        .unwrap();
    assert_eq!(store.state.execution.unresolved().count(), 0);
    assert_eq!(store.state.execution.failed_count("instance", &effect), 0);
    let archive = directory.0.join("spent-archive");
    std::fs::create_dir_all(&archive).unwrap();
    store.seal_and_archive(&archive, None).unwrap();
    crate::archive::verify(&archive).unwrap();
    drop(store);
    let mut store = Store::open(&directory.0).unwrap();
    assert!(
        crate::base::open_from_base(&directory.0, &store.records)
            .unwrap()
            .index
            .execution_claims
            .is_empty()
    );
    assert_eq!(
        store.execution_claim_hash(&claim).unwrap_err().code,
        "store/execution_evidence"
    );
    let before = store.state.clone();
    let head = store.journal.last_seq;
    let hash = store.journal.last_hash.clone();
    let outcome =
        StoppedOutcome::from_value(&json(br#"{"status":"timeout","result":null}"#)).unwrap();
    assert_eq!(
        store
            .stop_execution_on(
                &mut FixedClock::new(100, 1),
                ExecutionStopRequest {
                    claim: &claim,
                    proof: &evidence,
                    outcome: &outcome,
                    request_id: "spent-completion",
                    expected_seq: None,
                }
            )
            .unwrap_err()
            .code,
        "store/execution_stale"
    );
    assert!(crate::snapshot::store_states_eq(&before, &store.state));
    assert_eq!(store.journal.last_seq, head);
    assert_eq!(store.journal.last_hash, hash);
    assert!(!store.state.dedup.contains_key("spent-completion"));
    assert!(matches!(
        crate::journal_io::verify(&directory.0).health,
        crate::journal_io::JournalHealth::Ok
    ));
}

fn execution_operation(
    store: &mut Store,
    effect: &str,
    claim: Option<&Claim>,
    stage: &str,
) -> Result<Value, ErrorObj> {
    match stage {
        "claim" => allocate(store, effect, "boundary", &mut FixedClock::new(100, 1)),
        "stop" => {
            let claim = claim.unwrap();
            let evidence = proof(store, claim);
            let outcome =
                StoppedOutcome::from_value(&json(br#"{"status":"timeout","result":null}"#))
                    .unwrap();
            store.stop_execution_on(
                &mut FixedClock::new(100, 1),
                ExecutionStopRequest {
                    claim,
                    proof: &evidence,
                    outcome: &outcome,
                    request_id: "boundary",
                    expected_seq: None,
                },
            )
        }
        "settle" => store.settle_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim: claim.unwrap(),
                disposition: Settlement::Attempted,
                request_id: "boundary",
                expected_seq: None,
            },
        ),
        _ => panic!("unknown execution operation"),
    }
}

#[test]
fn failed_rotation_preserves_execution_state_and_requires_writer_reopen() {
    for stage in ["claim", "stop", "settle"] {
        let directory = Directory(std::env::temp_dir().join(format!(
            "fsm-execution-rotation-failure-{}-{}-{stage}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        )));
        std::fs::create_dir_all(&directory.0).unwrap();
        let (mut store, effect) = pending_store(Store::open(&directory.0).unwrap());
        if stage != "claim" {
            allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
        }
        let claim = store
            .state
            .execution
            .claim_for("instance", &effect)
            .cloned();
        if stage == "settle" {
            stop(&mut store, claim.as_ref().unwrap(), "timeout", "stop");
        }
        let before = store.state.clone();
        let head = store.journal.last_seq;
        let hash = store.journal.last_hash.clone();
        let segment = directory.0.join("journal").join(&store.journal.seg_name);
        let bytes = std::fs::read(&segment).unwrap();
        let obstruction = directory
            .0
            .join("journal")
            .join(format!("seg-{:020}.jsonl", head + 1));
        std::fs::create_dir(&obstruction).unwrap();
        // Accelerate only the rotation threshold; the filesystem refusal and
        // poisoned-writer handling run through the actual production append.
        store.journal.seg_records = u32::MAX;
        assert_eq!(
            execution_operation(&mut store, &effect, claim.as_ref(), stage)
                .unwrap_err()
                .code,
            "io/write"
        );
        assert!(store.journal.poisoned);
        assert!(crate::snapshot::store_states_eq(&store.state, &before));
        assert_eq!(store.journal.last_seq, head);
        assert_eq!(store.journal.last_hash, hash);
        assert_eq!(std::fs::read(&segment).unwrap(), bytes);
        assert!(!store.state.dedup.contains_key("boundary"));
        std::fs::remove_dir(&obstruction).unwrap();
        assert_eq!(
            execution_operation(&mut store, &effect, claim.as_ref(), stage)
                .unwrap_err()
                .code,
            "io/write"
        );
        assert!(crate::snapshot::store_states_eq(&store.state, &before));
        drop(store);
        let read_only = Store::open_read_only(&directory.0).unwrap();
        assert!(crate::snapshot::store_states_eq(&read_only.state, &before));
        drop(read_only);
        let mut store = Store::open(&directory.0).unwrap();
        assert!(!store.journal.poisoned);
        assert!(crate::snapshot::store_states_eq(&store.state, &before));
        execution_operation(&mut store, &effect, claim.as_ref(), stage).unwrap();
        assert_eq!(store.journal.last_seq, head + 1);
        assert!(store.state.dedup.contains_key("boundary"));
        assert_fold(&store);
        assert!(matches!(
            crate::journal_io::verify(&directory.0).health,
            crate::journal_io::JournalHealth::Ok
        ));
    }
}

fn descriptor_failure(failure: &str, descriptor: impl Fn(&Path) -> std::fs::File) {
    for stage in ["claim", "stop", "settle"] {
        let directory = Directory(std::env::temp_dir().join(format!(
            "fsm-execution-descriptor-{failure}-{}-{}-{stage}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
        )));
        std::fs::create_dir_all(&directory.0).unwrap();
        let (mut store, effect) = pending_store(Store::open(&directory.0).unwrap());
        if stage != "claim" {
            allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
        }
        let claim = store
            .state
            .execution
            .claim_for("instance", &effect)
            .cloned();
        if stage == "settle" {
            stop(&mut store, claim.as_ref().unwrap(), "timeout", "stop");
        }
        let before = store.state.clone();
        let head = store.journal.last_seq;
        let hash = store.journal.last_hash.clone();
        let segment = directory.0.join("journal").join(&store.journal.seg_name);
        let bytes = std::fs::read(&segment).unwrap();
        store.journal.replace_writer_for_test(descriptor(&segment));
        assert_eq!(
            execution_operation(&mut store, &effect, claim.as_ref(), stage)
                .unwrap_err()
                .code,
            "io/write"
        );
        assert!(store.journal.poisoned);
        assert!(crate::snapshot::store_states_eq(&store.state, &before));
        assert_eq!(store.journal.last_seq, head);
        assert_eq!(store.journal.last_hash, hash);
        assert_eq!(std::fs::read(&segment).unwrap(), bytes);
        assert!(!store.state.dedup.contains_key("boundary"));
        // A valid descriptor does not clear the unknown writer state: only a
        // fresh authoritative reopen may permit the next operation.
        store.journal.replace_writer_for_test(
            std::fs::OpenOptions::new()
                .append(true)
                .open(&segment)
                .unwrap(),
        );
        assert_eq!(
            execution_operation(&mut store, &effect, claim.as_ref(), stage)
                .unwrap_err()
                .code,
            "io/write"
        );
        assert_eq!(std::fs::read(&segment).unwrap(), bytes);
        drop(store);
        let read_only = Store::open_read_only(&directory.0).unwrap();
        assert!(crate::snapshot::store_states_eq(&read_only.state, &before));
        drop(read_only);
        let mut store = Store::open(&directory.0).unwrap();
        assert!(!store.journal.poisoned);
        assert!(crate::snapshot::store_states_eq(&store.state, &before));
        execution_operation(&mut store, &effect, claim.as_ref(), stage).unwrap();
        assert_eq!(store.journal.last_seq, head + 1);
        assert!(store.state.dedup.contains_key("boundary"));
        assert_fold(&store);
        assert!(matches!(
            crate::journal_io::verify(&directory.0).health,
            crate::journal_io::JournalHealth::Ok
        ));
    }
}

#[test]
fn failed_segment_write_preserves_execution_state_until_writer_reopen() {
    descriptor_failure("write", |segment| std::fs::File::open(segment).unwrap());
}

#[cfg(target_os = "linux")]
#[test]
fn failed_segment_fsync_preserves_execution_state_until_writer_reopen() {
    descriptor_failure("fsync", |_| {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .open("/dev/null")
            .unwrap();
        // Prove the actual OS descriptor accepts writes but refuses fsync;
        // append then exercises the same write_all -> sync_all production path.
        file.write_all(b"fsync-phase-probe").unwrap();
        assert!(file.sync_all().is_err());
        file
    });
}

#[test]
fn execution_writer_child() {
    let Some(path) = std::env::var_os("FSM_PRIVATE_EXECUTION_CRASH_DIR") else {
        return;
    };
    let path = PathBuf::from(path);
    let stage = std::env::var("FSM_PRIVATE_EXECUTION_CRASH_STAGE").unwrap();
    let rotate =
        std::env::var("FSM_PRIVATE_EXECUTION_CRASH_ROTATE").is_ok_and(|value| value == "1");
    let (mut store, effect) = pending_store(Store::open(&path).unwrap());
    if rotate {
        store.journal.force_rotate().unwrap();
    }
    allocate(&mut store, &effect, "claim", &mut FixedClock::new(100, 1)).unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    if stage != "claimed" {
        if rotate {
            store.journal.force_rotate().unwrap();
        }
        let status = match stage.as_str() {
            "acked" => "ok",
            "interrupted" => "interrupted",
            _ => "timeout",
        };
        stop(&mut store, &claim, status, "stop");
    }
    if matches!(stage.as_str(), "attempted" | "acked" | "interrupted") {
        if rotate {
            store.journal.force_rotate().unwrap();
        }
        let disposition = match stage.as_str() {
            "attempted" => Settlement::Attempted,
            "acked" => Settlement::Acked,
            _ => Settlement::Interrupted,
        };
        store
            .settle_execution_on(
                &mut FixedClock::new(100, 1),
                ExecutionSettleRequest {
                    claim: &claim,
                    disposition,
                    request_id: "settle",
                    expected_seq: None,
                },
            )
            .unwrap();
    }
    let ready = Value::Obj(BTreeMap::from([
        ("claim".into(), claim.to_value()),
        ("seq".into(), Value::Num(store.journal.last_seq.to_string())),
        ("hash".into(), Value::Str(store.journal.last_hash.clone())),
    ]));
    std::fs::write(path.join("ready.tmp"), fsm_core::canon::canon_bytes(&ready)).unwrap();
    std::fs::rename(path.join("ready.tmp"), path.join("ready.json")).unwrap();
    // Keep the writer and avoid normal Drop/snapshot shutdown until the parent kills us.
    loop {
        std::thread::park();
    }
}

#[test]
fn abrupt_writer_death_preserves_durable_ownership_and_atomic_settlement() {
    for rotate in [false, true] {
        for stage in ["claimed", "stopped", "attempted", "acked", "interrupted"] {
            let directory = Directory(std::env::temp_dir().join(format!(
            "fsm-execution-death-{}-{}-{stage}", std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
        )));
            std::fs::create_dir_all(&directory.0).unwrap();
            let mut child = Process(
                Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "store::execution::tests::durability::execution_writer_child",
                        "--nocapture",
                    ])
                    .env("FSM_PRIVATE_EXECUTION_CRASH_DIR", &directory.0)
                    .env("FSM_PRIVATE_EXECUTION_CRASH_STAGE", stage)
                    .env(
                        "FSM_PRIVATE_EXECUTION_CRASH_ROTATE",
                        if rotate { "1" } else { "0" },
                    )
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap(),
            );
            let deadline = Instant::now();
            let marker = directory.0.join("ready.json");
            while !marker.exists() {
                assert!(
                    child.0.try_wait().unwrap().is_none(),
                    "writer exited before {stage}"
                );
                assert!(
                    deadline.elapsed() < Duration::from_secs(30),
                    "missing {stage} barrier"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            let ready = parse(&std::fs::read(&marker).unwrap(), &JsonLimits::DEFAULT).unwrap();
            let claim = Claim::from_value(ready.get("claim").unwrap()).unwrap();
            let effect = claim.effect().1.to_owned();
            let head: u64 = ready.get("seq").unwrap().as_num().unwrap().parse().unwrap();
            let hash = ready.get("hash").unwrap().as_str().unwrap();
            child.0.kill().unwrap();
            assert!(!child.0.wait().unwrap().success());
            assert!(
                std::fs::read_dir(directory.0.join("snapshots"))
                    .unwrap()
                    .next()
                    .is_none(),
                "abrupt death must not run clean Drop snapshot publication"
            );
            let read_only = Store::open_read_only(&directory.0).unwrap();
            assert_eq!(read_only.journal.last_seq, head);
            assert_eq!(read_only.journal.last_hash, hash);
            assert_eq!(read_only.state.execution.run_high_water(), 1);
            let owned = matches!(stage, "claimed" | "stopped");
            assert_eq!(
                read_only
                    .state
                    .execution
                    .claim_for("instance", &effect)
                    .is_some(),
                owned
            );
            assert_eq!(
                read_only
                    .state
                    .execution
                    .stopped_for("instance", &effect)
                    .is_some(),
                stage == "stopped"
            );
            drop(read_only);
            let mut store = Store::open(&directory.0).unwrap();
            assert_eq!(
                crate::journal_io::verify(&directory.0).segments.len(),
                if rotate {
                    match stage {
                        "claimed" => 2,
                        "stopped" => 3,
                        _ => 4,
                    }
                } else {
                    1
                }
            );
            let replay =
                allocate(&mut store, &effect, "claim", &mut FixedClock::new(999, 1)).unwrap();
            assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
            assert_eq!(store.journal.last_seq, head);
            if owned {
                assert_eq!(
                    allocate(
                        &mut store,
                        &effect,
                        "successor",
                        &mut FixedClock::new(110, 1)
                    )
                    .unwrap_err()
                    .code,
                    "store/execution_owned"
                );
                assert_eq!(store.journal.last_seq, head);
                assert_eq!(store.state.execution.run_high_water(), 1);
                if stage == "stopped" {
                    let outcome =
                        StoppedOutcome::from_value(&json(br#"{"status":"timeout","result":null}"#))
                            .unwrap();
                    let evidence = proof(&store, &claim);
                    let replay = store
                        .stop_execution_on(
                            &mut FixedClock::new(999, 1),
                            ExecutionStopRequest {
                                claim: &claim,
                                proof: &evidence,
                                outcome: &outcome,
                                request_id: "stop",
                                expected_seq: Some(0),
                            },
                        )
                        .unwrap();
                    assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
                    assert_eq!(store.journal.last_seq, head);
                }
            } else {
                let disposition = match stage {
                    "attempted" => Settlement::Attempted,
                    "acked" => Settlement::Acked,
                    _ => Settlement::Interrupted,
                };
                let replay = store
                    .settle_execution_on(
                        &mut FixedClock::new(999, 1),
                        ExecutionSettleRequest {
                            claim: &claim,
                            disposition,
                            request_id: "settle",
                            expected_seq: Some(0),
                        },
                    )
                    .unwrap();
                assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
                assert_eq!(store.journal.last_seq, head);
                assert_eq!(store.journal.last_hash, hash);
                let before = store.state.clone();
                assert_eq!(
                    store
                        .settle_execution_on(
                            &mut FixedClock::new(999, 1),
                            ExecutionSettleRequest {
                                claim: &claim,
                                disposition,
                                request_id: "consume-again",
                                expected_seq: None,
                            }
                        )
                        .unwrap_err()
                        .code,
                    "store/execution_stale"
                );
                assert!(crate::snapshot::store_states_eq(&before, &store.state));
                assert_eq!(store.journal.last_seq, head);
                assert!(!store.state.dedup.contains_key("consume-again"));
                let status = match stage {
                    "attempted" => "timeout",
                    "acked" => "ok",
                    _ => "interrupted",
                };
                let outcome = StoppedOutcome::from_value(&json(
                    format!(r#"{{"status":"{status}","result":null}}"#).as_bytes(),
                ))
                .unwrap();
                let evidence = proof(&store, &claim);
                assert_eq!(
                    store
                        .stop_execution_on(
                            &mut FixedClock::new(999, 1),
                            ExecutionStopRequest {
                                claim: &claim,
                                proof: &evidence,
                                outcome: &outcome,
                                request_id: "stop-again",
                                expected_seq: None,
                            }
                        )
                        .unwrap_err()
                        .code,
                    "store/execution_stale"
                );
                assert!(crate::snapshot::store_states_eq(&before, &store.state));
                assert!(!store.state.dedup.contains_key("stop-again"));
                assert_eq!(
                    store.state.execution.failed_count("instance", &effect),
                    u32::from(stage == "attempted")
                );
                if stage == "acked" {
                    assert!(!store.state.instances["instance"].pending.contains(&effect));
                    assert_eq!(
                        allocate(
                            &mut store,
                            &effect,
                            "successor",
                            &mut FixedClock::new(110, 1)
                        )
                        .unwrap_err()
                        .code,
                        "store/execution_stale"
                    );
                } else {
                    assert!(store.state.instances["instance"].pending.contains(&effect));
                    if stage == "attempted" {
                        assert_eq!(
                            allocate(&mut store, &effect, "early", &mut FixedClock::new(109, 1))
                                .unwrap_err()
                                .code,
                            "store/execution_retry"
                        );
                    }
                    allocate(
                        &mut store,
                        &effect,
                        "successor",
                        &mut FixedClock::new(110, 1),
                    )
                    .unwrap();
                    let successor = store
                        .state
                        .execution
                        .claim_for("instance", &effect)
                        .unwrap();
                    assert_eq!(successor.run_id(), 2);
                    assert_eq!(
                        successor.to_value().get("attempt").and_then(Value::as_num),
                        Some(if stage == "attempted" { "2" } else { "1" })
                    );
                }
            }
            assert_fold(&store);
            assert!(matches!(
                crate::journal_io::verify(&directory.0).health,
                crate::journal_io::JournalHealth::Ok
            ));
        }
    }
}
