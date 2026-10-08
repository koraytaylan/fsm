//! Startup closes a genuine abandoned runner through the public paired driver.

use super::*;

pub(super) fn resume(fixture: &Fixture) {
    use super::super::broker_cases::{Daemon, disconnect_cases};
    use std::process::{Command, Stdio};

    let counter = fixture.counter();
    disconnect_cases::permit_operator_store(&fixture.store);
    super::super::super::super::broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    let daemon = Daemon::ready(&fixture.directory, 1);
    disconnect_cases::install_supervisor(&fixture.directory);
    let script = disconnect_cases::SUPERVISOR.replace(
        "supervisor_probe::owned_request",
        "runner_cases::orphan_recovery::recover_orphan",
    );
    let stdout_path = fixture.store.join("orphan-recovery.stdout");
    let stderr_path = fixture.store.join("orphan-recovery.stderr");
    let mut child = Command::new("/usr/bin/python3")
        .env("TMPDIR", &fixture.store)
        .args(["-c", &script])
        .arg(fixture.directory.join("broker"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(fs::File::create(&stdout_path).unwrap()))
        .stderr(Stdio::from(fs::File::create(&stderr_path).unwrap()))
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("owned startup orphan recovery child timed out; authority retained");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    for path in [&stdout_path, &stderr_path] {
        assert!(fs::metadata(path).unwrap().len() <= 8192);
    }
    let stdout = fs::read_to_string(stdout_path).unwrap();
    let stderr = fs::read_to_string(stderr_path).unwrap();
    assert!(
        status.success(),
        "startup orphan recovery failed: {stdout}{stderr}"
    );
    assert_eq!(stdout.matches("FSM_NATIVE_ORPHAN_RECOVERED").count(), 1);
    assert_eq!(fixture.counter(), counter);
    assert_eq!(
        Store::open_read_only(&fixture.store)
            .unwrap()
            .state
            .execution
            .unresolved()
            .count(),
        0
    );
    drop(daemon);
}

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn recover_orphan() {
    use fsm_core::record::RecordKind;
    use fsm_execute::config::HandlerTable;
    use fsm_execute::service::{ExecutorPhase, ShutdownMode};
    use fsm_store::clock::FixedClock;

    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 65534);
    let path = PathBuf::from(std::env::var_os("FSM_NATIVE_TEST_STORE").unwrap());
    let snapshot = Store::open_read_only(&path).unwrap();
    let records = snapshot.records.clone();
    let configuration = snapshot.state.instances["instance"].configuration.clone();
    let original_run = snapshot
        .state
        .execution
        .unresolved()
        .next()
        .unwrap()
        .0
        .run_id();
    assert_eq!(snapshot.state.execution.unresolved().count(), 1);
    assert!(
        snapshot
            .state
            .execution
            .unresolved()
            .next()
            .unwrap()
            .1
            .is_none()
    );
    drop(snapshot);
    let mut driver =
        super::handoff_recovery::RecoveryDriver::new(&path, HandlerTable::default(), "paired");
    let mut clock = FixedClock::new(2000, 1);
    let deadline = Instant::now() + Duration::from_secs(7);
    loop {
        driver.tick(&path, &mut clock, 2000);
        if Store::open_read_only(&path)
            .unwrap()
            .state
            .execution
            .unresolved()
            .count()
            == 0
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "startup never settled original orphan"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let settled = Store::open_read_only(&path).unwrap();
    assert_eq!(&settled.records[..records.len()], records.as_slice());
    assert_eq!(settled.records.len(), records.len() + 2);
    assert_eq!(
        settled.records[records.len()].kind,
        RecordKind::ExecutionStopped
    );
    assert_eq!(
        settled.records.last().unwrap().kind,
        RecordKind::ExecutionSettled
    );
    for record in &settled.records[records.len()..] {
        assert_eq!(number(&record.body, "run_id").unwrap(), original_run);
    }
    assert_eq!(
        settled.records[records.len()]
            .body
            .get("outcome")
            .unwrap()
            .get("status"),
        Some(&Value::Str("interrupted".into()))
    );
    assert_eq!(
        settled.records.last().unwrap().body.get("disposition"),
        Some(&Value::Str("interrupted".into()))
    );
    assert_eq!(
        settled.state.instances["instance"].configuration,
        configuration
    );
    assert_eq!(settled.state.execution_handoffs.outstanding().count(), 0);
    let records = settled.records.clone();
    drop(settled);
    driver.tick(&path, &mut clock, 2001);
    assert_eq!(Store::open_read_only(&path).unwrap().records, records);
    let mut driver = driver.into_paired(&path);
    let request = driver.control().stop(ShutdownMode::Drain, 1000).unwrap();
    driver.poll(&mut clock, 2002);
    let report = request.poll();
    assert_eq!(report.phase, ExecutorPhase::Stopped);
    assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
    #[allow(clippy::print_stdout)]
    {
        println!("\nFSM_NATIVE_ORPHAN_RECOVERED");
    }
}
