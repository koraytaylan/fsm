//! Cold public ticks recover protected original completion before acknowledgement.

use super::*;

pub(super) fn resume(
    fixture: &Fixture,
    completion: &fsm_execute::run::native_client::NativeCompletion,
    mode: &str,
) {
    use super::super::broker_cases::{Daemon, disconnect_cases};
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
    compete_processes(fixture, &script, &parameters);
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
    let records = reopened.records.clone();
    drop(reopened);
    let binding = read_value(&fixture.directory.join("binding-1.json"), true).unwrap();
    let claim =
        fsm_core::record::execution::Claim::from_value(binding.get("claim").unwrap()).unwrap();
    let mut writer = Store::open(&fixture.store).unwrap();
    assert_eq!(
        fsm_execute::run::Pipeline
            .advance_native_settled(
                &mut writer,
                &mut fsm_store::clock::FixedClock::new(4000, 1),
                &claim,
                completion,
                &fsm_execute::rid::ack_rid(claim.effect().1),
            )
            .unwrap(),
        fsm_execute::run::SettleOutcome::AlreadySettled
    );
    assert_eq!(writer.records, records);
    drop(writer);
    drop(daemon);
}

fn compete_processes(fixture: &Fixture, script: &str, parameters: &[u8]) {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let writer = Store::open(&fixture.store).unwrap();
    let records = writer.records.clone();
    let mut hosts: Vec<_> = (0..2)
        .map(|ordinal| {
            super::stopped_host::Host(
                Command::new("/usr/bin/python3")
                    .env("TMPDIR", &fixture.store)
                    .env("FSM_STOPPED_RECOVERY_ORDINAL", ordinal.to_string())
                    .args(["-c", script])
                    .arg(fixture.directory.join("broker"))
                    .arg(std::str::from_utf8(parameters).unwrap())
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .unwrap(),
            )
        })
        .collect();
    let deadline = Instant::now() + Duration::from_secs(7);
    while !(0..2).all(|ordinal| {
        fixture
            .store
            .join(format!("stopped-recovery-ready-{ordinal}"))
            .is_file()
    }) {
        for host in &mut hosts {
            assert!(
                host.0.try_wait().unwrap().is_none(),
                "recovery host exited before barrier"
            );
        }
        assert!(
            Instant::now() < deadline,
            "recovery hosts never reached barrier"
        );
        assert_eq!(
            Store::open_read_only(&fixture.store).unwrap().records,
            records
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        Store::open_read_only(&fixture.store).unwrap().records,
        records
    );
    drop(writer);
    fs::write(fixture.store.join("stopped-recovery-release"), b"release").unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    for host in &mut hosts {
        let status = loop {
            if let Some(status) = host.0.try_wait().unwrap() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "competing recovery host did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        host.0
            .stdout
            .take()
            .unwrap()
            .take(8193)
            .read_to_end(&mut stdout)
            .unwrap();
        host.0
            .stderr
            .take()
            .unwrap()
            .take(8193)
            .read_to_end(&mut stderr)
            .unwrap();
        assert!(stdout.len() <= 8192 && stderr.len() <= 8192);
        assert!(
            status.success(),
            "cold stopped recovery: {} {}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        assert!(String::from_utf8_lossy(&stdout).contains("1 passed; 0 failed; 0 ignored;"));
    }
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
    let original =
        HandlerSpec::from_contract(parameters.get("replacement_template").unwrap(), fingerprint)
            .unwrap();
    let mut competing_table = HandlerTable::default();
    competing_table
        .handlers
        .insert(original.effect.clone(), original);
    let mut competitor = super::handoff_recovery::RecoveryDriver::new(
        &path,
        competing_table,
        if changed { "tick" } else { "tick_with" },
    );
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
    competitor.check_readonly(&path);
    let mut clock = fsm_store::clock::FixedClock::new(2000, 1);
    // A different healthy writer fences this reconstructed recovery host.
    // Helper recovery can progress, but the existing stopped owner must remain.
    let held_writer = Store::open_read_only(&path).unwrap();
    let held_records = held_writer.records.clone();
    let held_execution = held_writer.state.execution.clone();
    let deadline = Instant::now() + Duration::from_secs(7);
    let mut writer_refused = [false; 2];
    loop {
        let outcome = driver.tick_without_writer(&path, &mut clock, 1800);
        let competing_outcome = competitor.tick_without_writer(&path, &mut clock, 1800);
        writer_refused[0] |= outcome.writer_unavailable;
        writer_refused[1] |= competing_outcome.writer_unavailable;
        let snapshot = Store::open_read_only(&path).unwrap();
        assert_eq!(snapshot.records, held_records);
        assert_eq!(snapshot.state.execution, held_execution);
        assert_eq!(
            snapshot
                .state
                .execution
                .claim_for("instance", claim.effect().1),
            Some(&claim)
        );
        assert!(
            snapshot
                .state
                .execution
                .stopped_for("instance", claim.effect().1)
                .is_some()
        );
        assert_eq!(snapshot.state.execution_handoffs.outstanding().count(), 0);
        assert!(
            !outcome
                .lines
                .iter()
                .any(|line| line.starts_with("native-handoff advanced"))
        );
        assert!(
            !competing_outcome
                .lines
                .iter()
                .any(|line| line.starts_with("native-handoff advanced"))
        );
        if writer_refused.iter().all(|refused| *refused) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cold recovery never reached held writer"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(held_writer.records, held_records);
    drop(held_writer);
    let ordinal = std::env::var("FSM_STOPPED_RECOVERY_ORDINAL").unwrap();
    assert!(matches!(ordinal.as_str(), "0" | "1"));
    fs::write(
        path.join(format!("stopped-recovery-ready-{ordinal}")),
        b"ready",
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(7);
    while !path.join("stopped-recovery-release").is_file() {
        assert_eq!(Store::open_read_only(&path).unwrap().records, held_records);
        assert!(
            Instant::now() < deadline,
            "parent did not release recovery barrier"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        driver.tick(&path, &mut clock, 2000);
        competitor.tick(&path, &mut clock, 2000);
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
    competitor.tick(&path, &mut clock, 2001);
    assert_eq!(Store::open_read_only(&path).unwrap().records, records);
    for host in [driver, competitor] {
        let mut host = host.into_paired(&path);
        let request = host.control().stop(ShutdownMode::Drain, 1000).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while request.poll().phase != ExecutorPhase::Stopped {
            host.poll(&mut clock, 2002);
            assert!(
                Instant::now() < deadline,
                "recovered public components did not drain"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let report = request.poll();
        assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
    }
    assert_eq!(Store::open_read_only(&path).unwrap().records, records);
}
