//! Atomic persistence fixtures; stopped evidence here is preauthenticated,
//! and these tests make no installed native closure or host acceptance claim.

use super::*;
use fsm_core::record::execution::AcknowledgedHandoff;
use std::path::PathBuf;

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .expect("home cache root required");
        let cache = PathBuf::from(home).join(".cache/fsm-handoff-ledger-tests");
        std::fs::create_dir_all(&cache).unwrap();
        (0..)
            .find_map(|sequence| {
                let path = cache.join(format!("handoff-{}-{sequence}", std::process::id()));
                match std::fs::create_dir(&path) {
                    Ok(()) => Some(Self(path)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                    Err(error) => panic!("create handoff directory: {error}"),
                }
            })
            .unwrap()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stopped(directory: &Directory) -> (Store, Claim, AcknowledgedHandoff) {
    let (mut store, effect) = pending_store(Store::open(&directory.0).unwrap());
    let literal = AcknowledgedHandoff::from_value(&json(include_bytes!(
        "../../../fsm-core/tests/fixtures/execution-handoff.json"
    )))
    .unwrap();
    let claim_value = literal.claim().to_value();
    let fingerprint = claim_value
        .get("handler_fingerprint")
        .unwrap()
        .as_str()
        .unwrap();
    store
        .claim_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionClaimRequest {
                instance_id: "instance",
                effect_id: &effect,
                handler_fingerprint: fingerprint,
                retry: &policy(),
                domain: &domain(),
                request_id: "claim",
                expected_seq: None,
            },
        )
        .unwrap();
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    stop(&mut store, &claim, "ok", "stop");
    let handoff = AcknowledgedHandoff::new(
        &claim,
        &store.current_execution_claim_hash(&claim).unwrap(),
        literal.handler_contract(),
        literal.outcome(),
        &format!("exec-ack-{effect}"),
        store.journal.last_seq + 1,
    )
    .unwrap();
    (store, claim, handoff)
}

fn acknowledge(store: &mut Store, claim: &Claim, handoff: &AcknowledgedHandoff) -> Value {
    store
        .settle_execution_with_handoff_on(
            &mut FixedClock::new(100, 1),
            ExecutionSettleRequest {
                claim,
                disposition: Settlement::Acked,
                request_id: handoff.acknowledgement_request_id(),
                expected_seq: None,
            },
            handoff,
        )
        .unwrap()
}

#[test]
fn atomic_handoff_survives_cache_cold_replay_and_two_seals_until_exact_event() {
    let directory = Directory::new();
    let (mut store, claim, handoff) = stopped(&directory);
    let response = acknowledge(&mut store, &claim, &handoff);
    assert_eq!(
        response.get("execution").unwrap().get("handoff"),
        Some(&handoff.to_value())
    );
    assert!(
        store
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    assert_eq!(
        store
            .state
            .execution_handoffs
            .outstanding()
            .collect::<Vec<_>>(),
        vec![&handoff]
    );
    assert_eq!(
        store.records.last().unwrap().seq,
        handoff.acknowledgement_seq()
    );
    let mut without_obligation = store.state.clone();
    without_obligation.execution_handoffs = Default::default();
    assert_ne!(
        fsm_core::replay::state_root_at(&store.state, store.state.last_seq),
        fsm_core::replay::state_root_at(&without_obligation, store.state.last_seq),
    );
    assert_eq!(
        fsm_core::replay::state_root_at_v4(&store.state, store.state.last_seq),
        fsm_core::replay::state_root_at_v4(&without_obligation, store.state.last_seq),
    );
    let duplicate = acknowledge(&mut store, &claim, &handoff);
    assert_eq!(duplicate.get("duplicate"), Some(&Value::Bool(true)));
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 1);
    store
        .shutdown_snapshot_on(&mut FixedClock::new(200, 1))
        .unwrap();
    drop(store);
    let inspected = Store::open_read_only(&directory.0).unwrap();
    assert!(inspected.opened_from_snapshot);
    assert_eq!(
        inspected
            .state
            .execution_handoffs
            .outstanding()
            .collect::<Vec<_>>(),
        vec![&handoff]
    );
    drop(inspected);
    std::fs::remove_dir_all(directory.0.join("snapshots")).unwrap();
    let mut store = Store::open(&directory.0).unwrap();
    assert!(!store.opened_from_snapshot);
    for index in 0..2 {
        let archive = directory.0.join(format!("archive-{index}"));
        std::fs::create_dir(&archive).unwrap();
        store.seal_and_archive(&archive, None).unwrap();
        crate::archive::verify(&archive).unwrap();
        drop(store);
        store = Store::open(&directory.0).unwrap();
        assert_eq!(
            store
                .state
                .execution_handoffs
                .outstanding()
                .collect::<Vec<_>>(),
            vec![&handoff]
        );
        assert_eq!(store.state.execution.unresolved().count(), 0);
    }
    store
        .send_event(
            claim.effect().0,
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            handoff.event_request_id(),
            None,
        )
        .unwrap();
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
    drop(store);
    let store = Store::open_read_only(&directory.0).unwrap();
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
}

#[test]
fn accepted_foreign_event_key_does_not_retire_original_obligation() {
    let directory = Directory::new();
    let (mut store, claim, handoff) = stopped(&directory);
    acknowledge(&mut store, &claim, &handoff);
    store
        .send_event(
            claim.effect().0,
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "foreign-event",
            None,
        )
        .unwrap();
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 1);
    drop(store);
    let store = Store::open_read_only(&directory.0).unwrap();
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 1);
}

#[test]
fn valid_shaped_foreign_anchor_refuses_before_acknowledgement_publication() {
    let directory = Directory::new();
    let (mut store, claim, handoff) = stopped(&directory);
    let mut material = handoff.to_value().as_obj().unwrap().clone();
    material.insert(
        "original_claim_hash".into(),
        Value::Str(format!("sha256:{}", "a".repeat(64))),
    );
    let foreign = AcknowledgedHandoff::from_value(&Value::Obj(material)).unwrap();
    let before = store.state.clone();
    let mut clock = FixedClock::new(200, 1);
    assert_eq!(
        store
            .settle_execution_with_handoff_on(
                &mut clock,
                ExecutionSettleRequest {
                    claim: &claim,
                    disposition: Settlement::Acked,
                    request_id: foreign.acknowledgement_request_id(),
                    expected_seq: None,
                },
                &foreign
            )
            .unwrap_err()
            .code,
        "store/execution_evidence"
    );
    assert_eq!(clock.now, 200);
    assert!(crate::snapshot::store_states_eq(&before, &store.state));
    assert!(
        !store
            .state
            .dedup
            .contains_key(handoff.acknowledgement_request_id())
    );
    assert_eq!(store.state.execution_handoffs.outstanding().count(), 0);
}

#[test]
fn torn_acknowledgement_and_event_repair_preserve_only_complete_obligations() {
    for event_tail in [false, true] {
        let directory = Directory::new();
        let (mut store, claim, handoff) = stopped(&directory);
        acknowledge(&mut store, &claim, &handoff);
        if event_tail {
            store
                .send_event(
                    claim.effect().0,
                    "docs_ok",
                    Value::Obj(BTreeMap::new()),
                    handoff.event_request_id(),
                    None,
                )
                .unwrap();
        }
        drop(store);
        let segment = directory.0.join("journal/seg-00000000000000000000.jsonl");
        let bytes = std::fs::read(&segment).unwrap();
        assert_eq!(bytes.last(), Some(&b'\n'));
        let start = bytes[..bytes.len() - 1]
            .iter()
            .rposition(|byte| *byte == b'\n')
            .unwrap()
            + 1;
        let cut = start + (bytes.len() - start) / 2;
        std::fs::OpenOptions::new()
            .write(true)
            .open(&segment)
            .unwrap()
            .set_len(cut as u64)
            .unwrap();
        assert!(Store::open(&directory.0).is_err());
        crate::journal_io::repair_truncate_torn_tail(&directory.0).unwrap();
        let store = Store::open_read_only(&directory.0).unwrap();
        assert_eq!(
            store.state.execution_handoffs.outstanding().count(),
            usize::from(event_tail)
        );
        assert_eq!(
            store
                .state
                .execution
                .claim_for(claim.effect().0, claim.effect().1)
                .is_some(),
            !event_tail
        );
        assert_eq!(
            store.state.instances[claim.effect().0]
                .pending
                .contains(&claim.effect().1.to_string()),
            !event_tail
        );
    }
}

#[test]
fn failed_handoff_append_preserves_stopped_owner_until_authoritative_reopen() {
    let mut failures = vec!["write", "rotation"];
    if cfg!(unix) {
        failures.push("fsync");
    }
    for failure in failures {
        let directory = Directory::new();
        let (mut store, claim, handoff) = stopped(&directory);
        let before = store.state.clone();
        let segment = directory.0.join("journal").join(&store.journal.seg_name);
        let bytes = std::fs::read(&segment).unwrap();
        let obstruction = directory
            .0
            .join("journal")
            .join(format!("seg-{:020}.jsonl", store.journal.last_seq + 1));
        match failure {
            "write" => store
                .journal
                .replace_writer_for_test(std::fs::File::open(&segment).unwrap()),
            "fsync" => store.journal.replace_writer_for_test(
                std::fs::OpenOptions::new()
                    .write(true)
                    .open("/dev/null")
                    .unwrap(),
            ),
            "rotation" => {
                std::fs::create_dir(&obstruction).unwrap();
                store.journal.seg_records = u32::MAX;
            }
            _ => unreachable!(),
        }
        let result = store.settle_execution_with_handoff_on(
            &mut FixedClock::new(200, 1),
            ExecutionSettleRequest {
                claim: &claim,
                disposition: Settlement::Acked,
                request_id: handoff.acknowledgement_request_id(),
                expected_seq: None,
            },
            &handoff,
        );
        assert_eq!(result.unwrap_err().code, "io/write", "{failure}");
        assert!(store.journal.poisoned);
        assert!(crate::snapshot::store_states_eq(&store.state, &before));
        assert_eq!(std::fs::read(&segment).unwrap(), bytes);
        assert!(
            !store
                .state
                .dedup
                .contains_key(handoff.acknowledgement_request_id())
        );
        if failure == "rotation" {
            std::fs::remove_dir(&obstruction).unwrap();
        }
        drop(store);
        let inspected = Store::open_read_only(&directory.0).unwrap();
        assert!(crate::snapshot::store_states_eq(&inspected.state, &before));
        drop(inspected);
        let mut store = Store::open(&directory.0).unwrap();
        acknowledge(&mut store, &claim, &handoff);
        assert_eq!(store.state.execution_handoffs.outstanding().count(), 1);
        assert_eq!(store.state.execution.unresolved().count(), 0);
    }
}
