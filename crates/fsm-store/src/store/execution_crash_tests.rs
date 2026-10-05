//! Abrupt writer death after durable APIs; fixture proofs do not authenticate native closure.

use super::*;
use std::path::PathBuf;
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
