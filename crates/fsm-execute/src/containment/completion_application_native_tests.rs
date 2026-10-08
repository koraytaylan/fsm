//! Genuine opaque completions must still match current durable application ownership.

use super::*;
use fsm_execute::run::Pipeline;
use fsm_store::clock::FixedClock;

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn application_refuses_completion_for_another_journal_claim() {
    run(Application::RetainedExecution);
}

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn settlement_refuses_completion_for_another_journal_claim() {
    run(Application::PipelineSettlement);
}

#[test]
#[ignore = "requires installed production gate and writable provisioned root cgroups"]
fn handoff_refuses_completion_for_another_journal_claim() {
    run(Application::HandoffReplay);
}

#[derive(Clone, Copy)]
enum Application {
    RetainedExecution,
    PipelineSettlement,
    HandoffReplay,
}

fn run(application: Application) {
    let mut fixture = if matches!(application, Application::HandoffReplay) {
        let table = fsm_core::json::parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"on_ok":{"event":"docs_ok"},"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#,
            &fsm_core::json::JsonLimits::DEFAULT).unwrap();
        Fixture::new_for_table(table)
    } else {
        Fixture::new()
    };
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let (binding, _) = claim_binding(&fixture, &domain);
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let hash = binding.get("journal_claim").unwrap().as_str().unwrap();
    bind(&fixture.directory, &binding).unwrap();
    let response = object([
        ("format", Value::Str("fsm.native-response/1".into())),
        ("ok", Value::Bool(true)),
        ("result", runner::execute(&fixture.directory, 1).unwrap()),
    ]);
    let original = NativeCompletion::verify(&response, &claim, hash).unwrap();
    let mut writer = Store::open(&fixture.store).unwrap();
    let mut clock = FixedClock::new(1000, 1);
    Pipeline
        .stop_native(
            &mut writer,
            &mut clock,
            &claim,
            &original,
            "application-proof-stop",
        )
        .unwrap();

    let closure = fixture.directory.join("closure-1-1.json");
    let attestation = fixture.directory.join("result-1-1.json");
    let original_closure = fs::read(&closure).unwrap();
    let original_attestation = fs::read(&attestation).unwrap();
    let closure_identity = physical_identity(&closure);
    let attestation_identity = physical_identity(&attestation);
    let wrong_hash = format!("sha256:{}", "0".repeat(64));
    assert_ne!(hash, wrong_hash);
    // Fault only genuine retired-run records, then use the public verifier;
    // no private proof construction or in-memory store mutation is involved.
    for path in [&closure, &attestation] {
        let mut material = read_value(path, true).unwrap().as_obj().unwrap().clone();
        material.insert("journal_claim".into(), Value::Str(wrong_hash.clone()));
        write_same_file(path, &canon_bytes(&Value::Obj(material)));
    }
    let wrong = NativeCompletion::verify(&response, &claim, &wrong_hash).unwrap();
    assert!(wrong.proof().matches_claim(&claim, &wrong_hash));
    assert!(!wrong.proof().matches_claim(&claim, hash));
    write_same_file(&closure, &original_closure);
    write_same_file(&attestation, &original_attestation);
    assert_eq!(physical_identity(&closure), closure_identity);
    assert_eq!(physical_identity(&attestation), attestation_identity);
    assert_eq!(writer.current_execution_claim_hash(&claim).unwrap(), hash);
    let acknowledgement = fsm_execute::rid::ack_rid(claim.effect().1);
    if matches!(application, Application::HandoffReplay) {
        Pipeline
            .settle_native_stopped(&mut writer, &mut clock, &claim, &original)
            .unwrap();
        assert_eq!(writer.state.execution_handoffs.outstanding().count(), 1);
        assert!(
            !writer
                .state
                .dedup
                .contains_key(&fsm_execute::rid::event_rid(claim.effect().1, "docs_ok"))
        );
    }
    let records = writer.records.clone();
    let state = writer.state.clone();
    match application {
        Application::RetainedExecution => {
            let mut execution =
                NativeExecution::from_completion(&claim, &wrong_hash, wrong).unwrap();
            match execution.settle(&mut writer, &mut clock) {
                Err(error)
                    if error.code == "exec/inflight_deferred"
                        && error.message
                            == "matching original native completion or durable settlement is not proven" =>
                    {}
                Err(error) => panic!(
                    "native application deferred mismatched closure to downstream settlement: {}",
                    error.message
                ),
                Ok(_) => panic!("native application accepted closure for another journal claim"),
            }
            assert!(execution.progress().retained);
        }
        Application::HandoffReplay => {
            match Pipeline.advance_native_settled(
                &mut writer,
                &mut clock,
                &claim,
                &wrong,
                &acknowledgement,
            ) {
                Err(error) => {
                    assert_eq!(error.code, "exec/inflight_deferred");
                    assert_eq!(
                        error.message,
                        "original native terminal settlement is not proven"
                    );
                }
                Ok(_) => panic!("native handoff advanced closure for another journal claim"),
            }
        }
        Application::PipelineSettlement => {
            match Pipeline.settle_native_stopped(&mut writer, &mut clock, &claim, &wrong) {
                Err(error) => {
                    assert_eq!(error.code, "exec/inflight_deferred");
                    assert_eq!(
                        error.message,
                        "matching stopped native ownership is not proven"
                    );
                }
                Ok(_) => panic!("native settlement accepted closure for another journal claim"),
            }
        }
    }
    assert_eq!(writer.records, records);
    assert!(fsm_store::snapshot::store_states_eq(&writer.state, &state));
    // The exact genuine completion still settles the same original stopped
    // outcome once the mismatched opaque candidate has been refused.
    if matches!(application, Application::HandoffReplay) {
        assert_eq!(
            Pipeline
                .advance_native_settled(
                    &mut writer,
                    &mut clock,
                    &claim,
                    &original,
                    &acknowledgement
                )
                .unwrap(),
            fsm_execute::run::SettleOutcome::Advanced
        );
        assert_eq!(writer.state.execution_handoffs.outstanding().count(), 0);
        assert!(
            writer
                .state
                .dedup
                .contains_key(&fsm_execute::rid::event_rid(claim.effect().1, "docs_ok"))
        );
    } else {
        Pipeline
            .settle_native_stopped(&mut writer, &mut clock, &claim, &original)
            .unwrap();
    }
    assert!(
        writer
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    drop(writer);
    fixture.cleanup().unwrap();
}
