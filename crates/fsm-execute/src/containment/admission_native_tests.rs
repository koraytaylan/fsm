//! Native writer-held startup refusal; no synthetic cleanup acceptance.

use super::{Fixture, claim_binding};
use fsm_core::json::Value;
use fsm_core::machine::Status;
use fsm_core::record::execution::NativeDomain;
use fsm_store::store::Store;
use std::fs;

pub(super) fn bound_claim(fixture: &Fixture, binding: &Value, effect: &str) {
    {
        let claim =
            fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
        let mut snapshot = Store::open_read_only(&fixture.store).unwrap();
        let hash = binding.get("journal_claim").unwrap().as_str().unwrap();
        let material = claim.to_value();
        let retry =
            fsm_core::record::execution::RetryPolicy::from_value(material.get("retry").unwrap())
                .unwrap();
        let mut memory = Store::open_memory().unwrap();
        let mut pipeline = fsm_execute::run::Pipeline;
        for refused_store in [&mut memory, &mut snapshot] {
            let records = refused_store.records.len();
            let state = refused_store.state.clone();
            let refusal = pipeline
                .claim_native(
                    refused_store,
                    &mut fsm_store::clock::FixedClock::new(100, 1),
                    fsm_store::store::ExecutionClaimRequest {
                        instance_id: "instance",
                        effect_id: effect,
                        handler_fingerprint: material
                            .get("handler_fingerprint")
                            .unwrap()
                            .as_str()
                            .unwrap(),
                        retry: &retry,
                        domain: claim.domain(),
                        request_id: "native-nondurable-claim",
                        expected_seq: None,
                    },
                )
                .unwrap_err();
            assert_eq!(refusal.code, "exec/mode");
            assert!(refusal.message.contains("healthy durable writer"));
            assert_eq!(refused_store.records.len(), records);
            assert!(fsm_store::snapshot::store_states_eq(
                &refused_store.state,
                &state
            ));
        }
        let before_records = snapshot.records.len();
        let mut pipeline = fsm_execute::run::Pipeline;
        let refusal =
            match pipeline.start_native(&mut snapshot, &claim, std::time::Duration::from_secs(1)) {
                Err(error) => error,
                Ok(_) => panic!("read-only native launch started a helper"),
            };
        assert_eq!(refusal.code, "exec/mode");
        assert_eq!(snapshot.records.len(), before_records);
        assert_eq!(
            snapshot.state.execution.claim_for("instance", effect),
            Some(&claim)
        );
        assert!(fs::symlink_metadata(fixture.directory.join("binding-1.json")).is_err());
        assert!(fs::symlink_metadata(fixture.directory.join("launch-1.json")).is_err());
        super::super::super::verify_claim(&snapshot, &claim, hash).unwrap();
        let mut execution = snapshot
            .state
            .execution
            .to_value()
            .as_obj()
            .unwrap()
            .clone();
        execution.insert("admission".into(), Value::Str("quarantined".into()));
        snapshot.state.execution =
            fsm_core::record::execution::ExecutionState::from_value(&Value::Obj(execution))
                .unwrap();
        assert_eq!(
            snapshot.state.execution.claim_for("instance", effect),
            Some(&claim)
        );
        assert_eq!(
            super::super::super::verify_claim(&snapshot, &claim, hash).unwrap_err(),
            "claim execution admission is quarantined"
        );
        // This is a read-only in-memory guard control, not a journal mutation
        // or evidence that an actual migrated native environment is quiescent.
    }
    {
        let claim =
            fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
        let mut writer = Store::open(&fixture.store).unwrap();
        let before = writer.state.execution.clone();
        let count = writer.records.len();
        let mut stale = claim.to_value().as_obj().unwrap().clone();
        stale.insert(
            "run_id".into(),
            Value::Num((claim.run_id() + 1).to_string()),
        );
        let stale = fsm_core::record::execution::Claim::from_value(&Value::Obj(stale)).unwrap();
        let mut pipeline = fsm_execute::run::Pipeline;
        match pipeline.start_native(&mut writer, &stale, std::time::Duration::from_secs(1)) {
            Err(error) => {
                assert_eq!(error.code, "exec/store");
                assert_eq!(
                    error
                        .details
                        .as_ref()
                        .unwrap()
                        .get("code")
                        .and_then(Value::as_str),
                    Some("store/execution_stale")
                );
            }
            Ok(_) => panic!("stale claim started a native helper"),
        }
        assert_eq!(writer.records.len(), count);
        assert_eq!(writer.state.execution, before);
        let obstruction = fixture
            .store
            .join("journal")
            .join(format!("seg-{:020}.jsonl", writer.journal.last_seq + 1));
        fs::create_dir(&obstruction).unwrap();
        writer.journal.seg_records = u32::MAX;
        assert_eq!(
            writer
                .ack_effect("instance", effect, "native-poison-control")
                .unwrap_err()
                .code,
            "io/write"
        );
        assert!(writer.journal.poisoned);
        let mut pipeline = fsm_execute::run::Pipeline;
        match pipeline.start_native(&mut writer, &claim, std::time::Duration::from_secs(1)) {
            Err(error) => {
                assert_eq!(error.code, "exec/mode");
                assert!(error.message.contains("healthy durable writer"));
            }
            Ok(_) => panic!("poisoned writer started a native helper"),
        }
        assert_eq!(writer.records.len(), count);
        assert_eq!(writer.state.execution, before);
        assert!(
            writer.state.instances["instance"]
                .pending
                .iter()
                .any(|pending| pending == effect)
        );
        assert!(fs::symlink_metadata(fixture.directory.join("binding-1.json")).is_err());
        assert!(fs::symlink_metadata(fixture.directory.join("launch-1.json")).is_err());
        fs::remove_dir(&obstruction).unwrap();
        drop(writer);
        let reopened = Store::open_read_only(&fixture.store).unwrap();
        assert!(!reopened.journal.poisoned);
        assert_eq!(reopened.state.execution, before);
    }
}

pub(super) fn removed_pending() {
    for cancellation in [false, true] {
        let mut fixture = Fixture::new();
        let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
        let (binding, effect) = claim_binding(&fixture, &domain);
        let claim =
            fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
        let mut writer = Store::open(&fixture.store).unwrap();
        if cancellation {
            writer
                .cancel_instance("instance", "native-before-bind-cancel")
                .unwrap();
        } else {
            writer
                .ack_effect("instance", &effect, "native-before-bind-ack")
                .unwrap();
        }
        assert_eq!(
            writer.state.instances["instance"].pending.contains(&effect),
            cancellation
        );
        assert_eq!(
            writer.state.instances["instance"].status,
            if cancellation {
                Status::Cancelled
            } else {
                Status::Running
            }
        );
        assert_eq!(
            writer.state.execution.claim_for("instance", &effect),
            Some(&claim)
        );
        let before = writer.state.execution.clone();
        let count = writer.records.len();
        let mut pipeline = fsm_execute::run::Pipeline;
        match pipeline.start_native(&mut writer, &claim, std::time::Duration::from_secs(1)) {
            Err(error) => assert_eq!(error.code, "exec/inflight_deferred"),
            Ok(_) => panic!("removed pending effect started a native helper"),
        }
        assert_eq!(writer.records.len(), count);
        assert_eq!(writer.state.execution, before);
        for name in ["binding", "launch", "entry", "handoff"] {
            assert_eq!(
                fs::symlink_metadata(fixture.directory.join(format!("{name}-1.json")))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::NotFound
            );
        }
        drop(writer);
        let reopened = Store::open_read_only(&fixture.store).unwrap();
        assert_eq!(reopened.state.execution, before);
        assert_eq!(
            reopened.state.instances["instance"]
                .pending
                .contains(&effect),
            cancellation
        );
        assert_eq!(
            reopened.state.instances["instance"].status,
            if cancellation {
                Status::Cancelled
            } else {
                Status::Running
            }
        );
        drop(reopened);
        // Exact test-owned empty domains may be removed by fixture teardown;
        // this creates no production closure receipt or claim clearance.
        fixture.cleanup().unwrap();
    }
}
