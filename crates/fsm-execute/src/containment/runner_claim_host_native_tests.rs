//! Actual caller death after a fresh durable claim and before native binding.

use super::*;
use std::io::{Read, Write};
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, Stdio};

pub(super) fn run() {
    use fsm_core::{
        json::{JsonLimits, parse},
        record::execution::{Claim, Settlement, StoppedOutcome},
    };
    use fsm_store::store::ExecutionStopRequest;
    let table = parse(br#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","kind":"process","argv":["/fixture-must-never-launch"],"timeout_ms":100,"retry":{"attempts":2,"backoff_ms":10,"max_backoff_ms":10,"on":["timeout"]}}]}"#, &JsonLimits::DEFAULT).unwrap();
    let mut fixture = Fixture::new_for_table(table);
    let domain = NativeDomain::from_value(&fixture.prepare()).unwrap();
    super::super::broker_cases::disconnect_cases::install_fixture_binary(
        &fixture.directory,
        "claim-host-test",
    );
    let mut host = super::stopped_host::Host(
        Command::new(fixture.directory.join("claim-host-test"))
            .args([
                "--exact",
                "authority::allocator::native_tests::runner_cases::claim_host::claim_then_wait",
                "--ignored",
            ])
            .env("FSM_CLAIM_HOST_STORE", &fixture.store)
            .env("FSM_CLAIM_HOST_AUTHORITY", &fixture.directory)
            .env(
                "FSM_CLAIM_HOST_DOMAIN",
                String::from_utf8(fsm_core::canon::canon_bytes(&domain.to_value())).unwrap(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let ready = fixture.directory.join("claim-host-ready");
    while !ready.exists() {
        assert!(
            host.0.try_wait().unwrap().is_none(),
            "claim host exited before publication"
        );
        assert!(Instant::now() < deadline, "claim host publication deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(matches!(Store::open(&fixture.store), Err(error) if error.code == "store/lock"));
    host.0.kill().unwrap();
    loop {
        if let Some(status) = host.0.try_wait().unwrap() {
            assert_eq!(status.signal(), Some(9));
            break;
        }
        assert!(Instant::now() < deadline, "claim host kill deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let binding = read_value(&ready, true).unwrap();
    let claim = Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let mut writer = Store::open(&fixture.store).unwrap();
    assert_eq!(
        writer
            .state
            .execution
            .claim_for("instance", claim.effect().1),
        Some(&claim)
    );
    assert_eq!(
        writer.current_execution_claim_hash(&claim).unwrap(),
        text(&binding, "journal_claim").unwrap()
    );
    let records = writer.records.clone();
    assert_eq!(
        records.last().unwrap().kind,
        fsm_core::record::RecordKind::ExecutionClaimed
    );
    let successor = NativeDomain::from_value(&fixture.prepare()).unwrap();
    let material = claim.to_value();
    let retry =
        fsm_core::record::execution::RetryPolicy::from_value(material.get("retry").unwrap())
            .unwrap();
    let refusal = fsm_execute::run::Pipeline
        .claim_native(
            &mut writer,
            &mut fsm_store::clock::FixedClock::new(500, 1),
            fsm_store::store::ExecutionClaimRequest {
                instance_id: "instance",
                effect_id: claim.effect().1,
                handler_fingerprint: text(&material, "handler_fingerprint").unwrap(),
                retry: &retry,
                domain: &successor,
                request_id: "claim-host-competitor",
                expected_seq: None,
            },
        )
        .unwrap_err();
    assert_eq!(refusal.store_code(), Some("store/execution_owned"));
    assert_eq!(writer.records, records);
    for prefix in ["binding", "launch", "entry", "handoff"] {
        assert!(!fixture.directory.join(format!("{prefix}-1.json")).exists());
    }
    drop(writer);
    bind(&fixture.directory, &binding).unwrap();
    closure::complete(&fixture.directory, 1).unwrap();
    let proof = VerifiedClosure::read(
        &fixture
            .directory
            .join(format!("closure-1-{}.json", claim.run_id())),
    )
    .unwrap();
    assert!(proof.matches_claim(&claim, text(&binding, "journal_claim").unwrap()));
    let mut writer = Store::open(&fixture.store).unwrap();
    let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
    writer
        .stop_execution_on(
            &mut clock,
            ExecutionStopRequest {
                claim: &claim,
                proof: &proof,
                outcome: &StoppedOutcome::from_value(&object([(
                    "status",
                    Value::Str("interrupted".into()),
                )]))
                .unwrap(),
                request_id: "claim-host-stop",
                expected_seq: None,
            },
        )
        .unwrap();
    fsm_execute::run::Pipeline
        .settle_stopped(
            &mut writer,
            &mut clock,
            &claim,
            Settlement::Interrupted,
            "claim-host-interrupted",
        )
        .unwrap();
    let settled = writer.records.clone();
    assert_eq!(settled.len(), records.len() + 2);
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    assert!(
        writer.state.instances["instance"]
            .pending
            .contains(&claim.effect().1.to_owned())
    );
    assert!(
        !writer
            .state
            .dedup
            .contains_key(&fsm_execute::rid::ack_rid(claim.effect().1))
    );
    drop(writer);
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        settled
    );
    for prefix in ["launch", "entry", "handoff"] {
        assert!(!fixture.directory.join(format!("{prefix}-1.json")).exists());
    }
    fixture.cleanup().unwrap();
}

#[test]
#[ignore = "invoked only by the independent after-claim native crash fixture"]
fn claim_then_wait() {
    use fsm_core::json::{JsonLimits, parse};
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 0);
    let directory = PathBuf::from(std::env::var_os("FSM_CLAIM_HOST_AUTHORITY").unwrap());
    let store_path = PathBuf::from(std::env::var_os("FSM_CLAIM_HOST_STORE").unwrap());
    let domain = NativeDomain::from_value(
        &parse(
            std::env::var("FSM_CLAIM_HOST_DOMAIN").unwrap().as_bytes(),
            &JsonLimits::DEFAULT,
        )
        .unwrap(),
    )
    .unwrap();
    let (binding, _) = super::super::claim_binding_paths(&directory, &store_path, &domain);
    let _writer = Store::open(&store_path).unwrap();
    let mut ready = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("claim-host-ready"))
        .unwrap();
    ready
        .write_all(&fsm_core::canon::canon_bytes(&binding))
        .unwrap();
    ready.sync_all().unwrap();
    std::io::stdin().read_exact(&mut [0]).unwrap();
    panic!("parent must kill the original claim host before binding");
}
