//! Actual checkpoint claim hashes, not provisional hashes or native proof.

use super::*;
use fsm_core::record::execution::AcknowledgedHandoff;

struct Directory(std::path::PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn checkpoint_claim_handoff_matches_only_the_final_published_anchor() {
    let (mut store, effect) = pending();
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .expect("home cache root required");
    let cache = std::path::PathBuf::from(home).join(".cache/fsm-handoff-checkpoint-tests");
    std::fs::create_dir_all(&cache).unwrap();
    let directory = (0..)
        .find_map(|sequence| {
            let path = cache.join(format!("checkpoint-{}-{sequence}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => Some(Directory(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => panic!("create checkpoint fixture directory: {error}"),
            }
        })
        .unwrap();
    // An owned cache path makes any unintended memory-store disk write observable.
    store.data_dir = directory.0.clone();
    while store.journal.last_seq < 9999 {
        store
            .annotate(
                "instance",
                &format!("checkpoint-filler-{}", store.journal.last_seq),
                "claim anchor boundary",
            )
            .unwrap();
    }
    let mut candidate = json(include_bytes!(
        "../../../fsm-core/tests/fixtures/execution-handoff.json"
    ));
    let fingerprint = candidate
        .get("claim")
        .unwrap()
        .get("handler_fingerprint")
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned();
    store
        .claim_execution_on(
            &mut FixedClock::new(100, 1),
            ExecutionClaimRequest {
                instance_id: "instance",
                effect_id: &effect,
                handler_fingerprint: &fingerprint,
                retry: &policy(),
                domain: &domain(),
                request_id: "checkpoint-claim",
                expected_seq: None,
            },
        )
        .unwrap();
    let record = store.records.last().unwrap().clone();
    assert_eq!(record.seq, 10_000);
    assert_eq!(record.kind, RecordKind::ExecutionClaimed);
    assert!(record.body.get("state_root").is_some());
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 0);
    let mut body = record.body.as_obj().unwrap().clone();
    body.remove("state_root");
    body.remove("state_root_format");
    let provisional = fsm_core::record::seal(
        record.seq,
        record.ts,
        record.kind,
        Value::Obj(body),
        &record.prev,
    );
    assert_ne!(record.hash, provisional.hash);
    let claim = store
        .state
        .execution
        .claim_for("instance", &effect)
        .unwrap()
        .clone();
    let published = store.current_execution_claim_hash(&claim).unwrap();
    assert_eq!(published, format!("sha256:{}", record.hash));
    let folded =
        fsm_core::replay::fold_with(store.records.clone(), &mut fsm_core::replay::NopSink).unwrap();
    assert!(crate::snapshot::store_states_eq(&store.state, &folded));

    // Preauthenticated fixture evidence exercises journal identity only;
    // no kernel closure or installed host acceptance is inferred.
    stop(&mut store, &claim, "ok", "checkpoint-stop");
    let sequence = store.journal.last_seq + 1;
    let Value::Obj(fields) = &mut candidate else {
        panic!("literal candidate must be an object");
    };
    fields.insert("claim".into(), claim.to_value());
    fields.insert("original_claim_hash".into(), Value::Str(published.clone()));
    fields.insert(
        "acknowledgement_seq".into(),
        Value::Num(sequence.to_string()),
    );
    let handoff = AcknowledgedHandoff::from_value(&candidate).unwrap();
    let stopped = store
        .state
        .execution
        .stopped_for("instance", &effect)
        .unwrap();
    assert!(handoff.matches_acknowledgement(
        &claim,
        stopped,
        &published,
        "exec-ack-instance/3/0",
        sequence
    ));
    let Value::Obj(fields) = &mut candidate else {
        panic!("literal candidate must be an object");
    };
    fields.insert(
        "original_claim_hash".into(),
        Value::Str(format!("sha256:{}", provisional.hash)),
    );
    let provisional_handoff = AcknowledgedHandoff::from_value(&candidate).unwrap();
    assert!(!provisional_handoff.matches_acknowledgement(
        &claim,
        stopped,
        &published,
        "exec-ack-instance/3/0",
        sequence
    ));
}
