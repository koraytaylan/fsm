//! Startup closes a genuine abandoned runner through the public paired driver.

use super::super::broker_cases::{Daemon, disconnect_cases};
use super::*;

pub(super) struct Session<'fixture> {
    fixture: &'fixture Fixture,
    _daemon: Daemon,
    operator_ready: std::cell::Cell<bool>,
}

impl<'fixture> Session<'fixture> {
    pub(super) fn new(fixture: &'fixture Fixture) -> Self {
        super::super::super::super::broker_endpoint::provision(&fixture.directory, 65534).unwrap();
        let daemon = Daemon::ready(&fixture.directory, 1);
        disconnect_cases::install_supervisor(&fixture.directory);
        Self {
            fixture,
            _daemon: daemon,
            operator_ready: std::cell::Cell::new(false),
        }
    }

    pub(super) fn refuse_live_runner(&self) {
        let entry = self.fixture.directory.join("entry-1.json");
        let original_entry = fs::read(&entry).unwrap();
        self.run("refuse_live_runner", "FSM_NATIVE_LIVE_RUNNER_REFUSED");
        assert_unresolved(self.fixture, self.fixture_effect().as_str());
        assert_eq!(fs::read(entry).unwrap(), original_entry);
        for name in ["closing-1.json", "closed-1.json"] {
            assert!(fs::symlink_metadata(self.fixture.directory.join(name)).is_err());
        }
    }

    fn fixture_effect(&self) -> String {
        Store::open_read_only(&self.fixture.store)
            .unwrap()
            .state
            .execution
            .unresolved()
            .next()
            .unwrap()
            .0
            .effect()
            .1
            .to_owned()
    }

    pub(super) fn resume(&self) {
        self.run("recover_orphan", "FSM_NATIVE_ORPHAN_RECOVERED");
        assert_eq!(
            Store::open_read_only(&self.fixture.store)
                .unwrap()
                .state
                .execution
                .unresolved()
                .count(),
            0
        );
    }

    pub(super) fn resume_via_cli(&self) {
        super::super::workflow_cases::stage_artifact(
            &self.fixture.directory.join("recovery-cli"),
            "FSM_NATIVE_WORKFLOW_CLI_ARTIFACT",
            "FSM_NATIVE_WORKFLOW_CLI_SHA256",
        );
        self.run("recover_orphan_cli", "FSM_NATIVE_ORPHAN_RECOVERED");
        assert_eq!(
            Store::open_read_only(&self.fixture.store)
                .unwrap()
                .state
                .execution
                .unresolved()
                .count(),
            0
        );
    }

    pub(super) fn refuse_pre_run_owner(&self) {
        self.run("refuse_pre_run_owner", "FSM_NATIVE_PRE_RUN_OWNER_REFUSED");
    }

    pub(super) fn refuse_pre_run_identity(&self) {
        self.run(
            "refuse_pre_run_identity",
            "FSM_NATIVE_PRE_RUN_IDENTITY_REFUSED",
        );
    }

    pub(super) fn refuse_pre_run_boot(&self) {
        self.run("refuse_pre_run_boot", "FSM_NATIVE_PRE_RUN_BOOT_REFUSED");
    }

    fn run(&self, case: &str, marker: &str) {
        use std::process::{Command, Stdio};

        let fixture = self.fixture;
        // Finish root claim setup before granting this fixed fixture to its
        // operator; repeated recovery calls must not re-grant operator files.
        if !self.operator_ready.get() {
            disconnect_cases::permit_operator_store(&fixture.store);
            self.operator_ready.set(true);
        }
        let counter = fixture.counter();
        let script = disconnect_cases::SUPERVISOR.replace(
            "supervisor_probe::owned_request",
            &format!("runner_cases::orphan_recovery::{case}"),
        );
        let stdout_path = fixture.store.join(format!("{case}.stdout"));
        let stderr_path = fixture.store.join(format!("{case}.stderr"));
        let mut child = Command::new("/usr/bin/python3")
            .env("TMPDIR", &fixture.store)
            .env(
                "FSM_NATIVE_ORPHAN_CLI",
                fixture.directory.join("recovery-cli"),
            )
            .args(["-c", &script])
            .arg(fixture.directory.join("broker"))
            .stdin(Stdio::null())
            .stdout(Stdio::from(fs::File::create(&stdout_path).unwrap()))
            .stderr(Stdio::from(fs::File::create(&stderr_path).unwrap()))
            .spawn()
            .unwrap();
        let deadline = Instant::now()
            + Duration::from_secs(if case == "recover_orphan_cli" { 22 } else { 12 });
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
        assert_eq!(stdout.matches(marker).count(), 1);
        assert_eq!(fixture.counter(), counter);
    }
}

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn refuse_pre_run_owner() {
    refuse_pre_run(
        "native shutdown refused: original preparation owner remains active or lease locking is unavailable",
    );
    #[allow(clippy::print_stdout)] // Frozen marker consumed by the root fixture.
    {
        println!("\nFSM_NATIVE_PRE_RUN_OWNER_REFUSED");
    }
}

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn refuse_pre_run_identity() {
    refuse_pre_run("native shutdown refused: native owner lease protection or identity differs");
    #[allow(clippy::print_stdout)] // Frozen marker consumed by the root fixture.
    {
        println!("\nFSM_NATIVE_PRE_RUN_IDENTITY_REFUSED");
    }
}

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn refuse_pre_run_boot() {
    refuse_pre_run("native discovery operator, boot or authority differs");
    #[allow(clippy::print_stdout)] // Frozen marker consumed by the root fixture.
    {
        println!("\nFSM_NATIVE_PRE_RUN_BOOT_REFUSED");
    }
}

fn refuse_pre_run(expected_message: &str) {
    use fsm_store::clock::FixedClock;

    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 65534);
    let path = PathBuf::from(std::env::var_os("FSM_NATIVE_TEST_STORE").unwrap());
    let mut writer = Store::open(&path).unwrap();
    let records = writer.records.clone();
    let ownership = writer.state.execution.clone();
    let run = writer
        .state
        .execution
        .unresolved()
        .next()
        .unwrap()
        .0
        .run_id();
    if expected_message
        == "native shutdown refused: original preparation owner remains active or lease locking is unavailable"
    {
        refuse_copied_pre_run_store(&path, run, &records);
    }
    let error = fsm_execute::service::reconcile_run(
        &mut writer,
        &mut FixedClock::new(2000, 1),
        run,
        Duration::from_secs(3),
    )
    .unwrap_err();
    assert_eq!(error.code, "exec/inflight_deferred");
    assert_eq!(error.message, expected_message);
    assert_eq!(writer.records, records);
    assert_eq!(writer.state.execution, ownership);
    drop(writer);
    assert_eq!(Store::open_read_only(&path).unwrap().records, records);
}

fn refuse_copied_pre_run_store(path: &Path, run_id: u64, records: &[fsm_core::record::Record]) {
    use fsm_store::clock::FixedClock;

    let copied = super::super::supervisor_probe::fresh_handoff::copy_store(path);
    let mut writer = Store::open(&copied).unwrap();
    assert_eq!(writer.records, records);
    let ownership = writer.state.execution.clone();
    let error = fsm_execute::service::reconcile_run(
        &mut writer,
        &mut FixedClock::new(2000, 1),
        run_id,
        Duration::from_secs(3),
    )
    .unwrap_err();
    assert_eq!(error.code, "exec/inflight_deferred");
    assert_eq!(error.message, "native discovery store registration missing");
    assert_eq!(writer.records, records);
    assert_eq!(writer.state.execution, ownership);
    drop(writer);
    assert_eq!(Store::open_read_only(&copied).unwrap().records, records);
    assert_eq!(Store::open_read_only(path).unwrap().records, records);
}

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn refuse_live_runner() {
    use fsm_execute::config::HandlerTable;
    use fsm_execute::service::{ExecutorPhase, ShutdownMode};
    use fsm_store::clock::FixedClock;

    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 65534);
    let path = PathBuf::from(std::env::var_os("FSM_NATIVE_TEST_STORE").unwrap());
    let records = Store::open_read_only(&path).unwrap().records.clone();
    let mut driver =
        super::handoff_recovery::RecoveryDriver::new(&path, HandlerTable::default(), "paired")
            .into_paired(&path);
    let mut clock = FixedClock::new(1500, 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut lines = driver.tick(&mut clock, 1500);
        lines.extend(driver.poll(&mut clock, 1500));
        assert_eq!(Store::open_read_only(&path).unwrap().records, records);
        if lines.iter().any(|line| {
            line.contains("original runner remains active or lease locking is unavailable")
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "startup did not report live-runner lease refusal: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let request = driver.control().stop(ShutdownMode::Drain, 1000).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        driver.poll(&mut clock, 1501);
        let report = request.poll();
        if report.phase == ExecutorPhase::Stopped {
            assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
            break;
        }
        assert!(
            Instant::now() < deadline,
            "refused startup helper did not retire: {report:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(Store::open_read_only(&path).unwrap().records, records);
    let mut writer = Store::open(&path).unwrap();
    let ownership = writer.state.execution.clone();
    let run_id = ownership.unresolved().next().unwrap().0.run_id();
    let refusal = fsm_execute::service::reconcile_run(
        &mut writer,
        &mut clock,
        run_id,
        Duration::from_secs(3),
    )
    .unwrap_err();
    assert_eq!(refusal.code, "exec/inflight_deferred");
    assert_eq!(
        refusal.message,
        "native shutdown refused: original runner remains active or lease locking is unavailable"
    );
    assert_eq!(writer.records, records);
    assert_eq!(writer.state.execution, ownership);
    drop(writer);
    assert_eq!(Store::open_read_only(&path).unwrap().records, records);
    #[allow(clippy::print_stdout)]
    {
        println!("\nFSM_NATIVE_LIVE_RUNNER_REFUSED");
    }
}

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn recover_orphan() {
    recover_orphan_with(None);
}

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn recover_orphan_cli() {
    recover_orphan_with(Some(PathBuf::from(
        std::env::var_os("FSM_NATIVE_ORPHAN_CLI").unwrap(),
    )));
}

fn reconcile_orphan_cli(
    cli: &Path,
    path: &Path,
    run_id: u64,
    records: &[fsm_core::record::Record],
) {
    use std::process::{Command, Stdio};
    let output_path = path.join("orphan-cli.stdout");
    let error_path = path.join("orphan-cli.stderr");
    let mut child = Command::new(cli)
        .args(["--json", "--data-dir"])
        .arg(path)
        .args([
            "execute",
            "reconcile",
            "--run-id",
            &run_id.to_string(),
            "--timeout-ms",
            "8000",
        ])
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output_path).unwrap())
        .stderr(fs::File::create(&error_path).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().unwrap();
            panic!("orphan MCP CLI reconciliation exceeded its bound; authority retained");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(fs::metadata(&output_path).unwrap().len() <= 8192);
    assert!(fs::metadata(&error_path).unwrap().len() <= 8192);
    assert!(
        status.success(),
        "orphan MCP CLI refused: {}",
        String::from_utf8_lossy(&fs::read(&error_path).unwrap())
    );
    let response = fsm_core::json::parse(
        &fs::read(&output_path).unwrap(),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(response.get("duplicate"), Some(&Value::Bool(false)));
    assert_eq!(
        response.get("execution").unwrap().get("run_id"),
        Some(&Value::Num(run_id.to_string()))
    );
    assert_eq!(
        response.get("execution").unwrap().get("disposition"),
        Some(&Value::Str("interrupted".into()))
    );
    let observed = Store::open_read_only(path).unwrap();
    assert_eq!(&observed.records[..records.len()], records);
    assert_eq!(observed.records.len(), records.len() + 2);
    assert_eq!(observed.state.execution.unresolved().count(), 0);
}

fn recover_orphan_with(cli: Option<PathBuf>) {
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
    if let Some(cli) = cli {
        reconcile_orphan_cli(&cli, &path, original_run, &records);
    }
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
    let mut writer = Store::open(&path).unwrap();
    for _ in 0..2 {
        let replay = fsm_execute::service::reconcile_run(
            &mut writer,
            &mut clock,
            original_run,
            Duration::from_secs(1),
        )
        .unwrap();
        assert_eq!(replay.get("duplicate"), Some(&Value::Bool(true)));
        assert_eq!(
            replay.get("execution").unwrap().get("disposition"),
            Some(&Value::Str("interrupted".into()))
        );
        assert_eq!(writer.records, records);
        assert_eq!(writer.state.execution.unresolved().count(), 0);
    }
    drop(writer);
    #[allow(clippy::print_stdout)]
    {
        println!("\nFSM_NATIVE_ORPHAN_RECOVERED");
    }
}
