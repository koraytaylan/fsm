//! Cold public ticks recover protected original completion before acknowledgement.

use super::*;

pub(super) fn resume(
    fixture: &Fixture,
    completion: &fsm_execute::run::native_client::NativeCompletion,
    mode: &str,
) {
    use super::super::broker_cases::{Daemon, disconnect_cases};
    use std::process::{Command, Stdio};
    let before = Store::open_read_only(&fixture.store).unwrap().records.len();
    let counter = fixture.counter();
    disconnect_cases::permit_operator_store(&fixture.store);
    super::super::super::super::broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    let daemon = Daemon::ready(&fixture.directory, 1);
    disconnect_cases::install_supervisor(&fixture.directory);
    let script = disconnect_cases::SUPERVISOR.replace(
        "supervisor_probe::owned_request",
        "runner_cases::stopped_recovery::recover_stopped",
    );
    let parameters = fsm_core::canon::canon_bytes(&object([
        (
            "changed",
            Value::Bool(mode == "process-recover-stopped-changed"),
        ),
        (
            "replacement_template",
            completion.handler().contract_value(),
        ),
    ]));
    let output = Command::new("/usr/bin/python3")
        .env("TMPDIR", &fixture.store)
        .args(["-c", &script])
        .arg(fixture.directory.join("broker"))
        .arg(std::str::from_utf8(&parameters).unwrap())
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(output.stdout.len() <= 8192 && output.stderr.len() <= 8192);
    assert!(
        output.status.success(),
        "cold stopped recovery: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed; 0 ignored;"));
    assert_eq!(fixture.counter(), counter);
    let reopened = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(reopened.records.len(), before + 2);
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    assert_eq!(reopened.state.execution_handoffs.outstanding().count(), 0);
    assert_eq!(
        reopened.state.instances["instance"]
            .configuration
            .sequential_leaf(),
        Some("risk_review")
    );
    drop(reopened);
    drop(daemon);
}

#[test]
#[ignore = "invoked only as the configured operator after a stopped-host crash"]
fn recover_stopped() {
    use fsm_core::{
        json::{JsonLimits, parse},
        record::RecordKind,
    };
    use fsm_execute::{
        config::{HandlerSpec, HandlerTable},
        rid::event_rid,
        service::{ExecutorPhase, ShutdownMode},
    };
    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 65534);
    let parameters = parse(
        std::env::var("FSM_NATIVE_TEST_BINDING").unwrap().as_bytes(),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let changed = parameters.get("changed").and_then(Value::as_bool).unwrap();
    let path = PathBuf::from(std::env::var_os("FSM_NATIVE_TEST_STORE").unwrap());
    let snapshot = Store::open_read_only(&path).unwrap();
    let before = snapshot.records.len();
    let (claim, stopped) = snapshot.state.execution.unresolved().next().unwrap();
    assert_eq!(stopped.unwrap().outcome().status(), "ok");
    assert_eq!(snapshot.state.execution_handoffs.outstanding().count(), 0);
    let claim = claim.clone();
    let material = claim.to_value();
    let fingerprint = text(&material, "handler_fingerprint").unwrap();
    drop(snapshot);
    let mut table = HandlerTable::default();
    if changed {
        let mut replacement = HandlerSpec::from_contract(
            parameters.get("replacement_template").unwrap(),
            fingerprint,
        )
        .unwrap();
        replacement.argv = vec!["/fixture-must-never-start".into()];
        replacement.on_ok.as_mut().unwrap().event = "withdraw".into();
        assert_ne!(replacement.fingerprint(), fingerprint);
        table
            .handlers
            .insert(replacement.effect.clone(), replacement);
    }
    let mut driver = super::handoff_recovery::RecoveryDriver::new(
        &path,
        table,
        if changed { "tick_with" } else { "tick" },
    );
    driver.check_readonly(&path);
    let mut clock = fsm_store::clock::FixedClock::new(2000, 1);
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        driver.tick(&path, &mut clock, 2000);
        let snapshot = Store::open_read_only(&path).unwrap();
        if snapshot.state.execution.unresolved().count() == 0
            && snapshot.state.execution_handoffs.outstanding().count() == 0
        {
            assert_eq!(snapshot.records.len(), before + 2);
            assert_eq!(snapshot.records[before].kind, RecordKind::ExecutionSettled);
            assert_eq!(snapshot.records[before + 1].kind, RecordKind::EventApplied);
            assert!(
                snapshot
                    .state
                    .dedup
                    .contains_key(&event_rid(claim.effect().1, "docs_ok"))
            );
            assert!(
                !snapshot
                    .state
                    .dedup
                    .contains_key(&event_rid(claim.effect().1, "withdraw"))
            );
            break;
        }
        assert!(Instant::now() < deadline, "cold stopped recovery deadline");
        std::thread::sleep(Duration::from_millis(5));
    }
    let records = Store::open_read_only(&path).unwrap().records.clone();
    driver.tick(&path, &mut clock, 2001);
    assert_eq!(Store::open_read_only(&path).unwrap().records, records);
    let mut driver = driver.into_paired(&path);
    let request = driver.control().stop(ShutdownMode::Drain, 1000).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while request.poll().phase != ExecutorPhase::Stopped {
        driver.poll(&mut clock, 2002);
        assert!(
            Instant::now() < deadline,
            "recovered public components did not drain"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let report = request.poll();
    assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
}
