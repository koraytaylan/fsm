//! Startup closes a genuine abandoned runner through the public paired driver.

use super::super::broker_cases::{Daemon, disconnect_cases};
use super::*;

pub(super) struct Session<'fixture> {
    fixture: &'fixture Fixture,
    _daemon: Daemon,
    operator_ready: std::cell::Cell<bool>,
    cli_ready: std::cell::Cell<bool>,
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
            cli_ready: std::cell::Cell::new(false),
        }
    }

    pub(super) fn refuse_live_runner(&self) {
        self.ensure_cli();
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

    fn ensure_cli(&self) {
        if self.cli_ready.get() {
            return;
        }
        super::super::workflow_cases::stage_artifact(
            &self.fixture.directory.join("recovery-cli"),
            "FSM_NATIVE_WORKFLOW_CLI_ARTIFACT",
            "FSM_NATIVE_WORKFLOW_CLI_SHA256",
        );
        self.cli_ready.set(true);
    }

    pub(super) fn resume_via_cli(&self) {
        self.ensure_cli();
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
        self.ensure_cli();
        self.run("refuse_pre_run_owner", "FSM_NATIVE_PRE_RUN_OWNER_REFUSED");
    }

    pub(super) fn refuse_pre_run_identity(&self) {
        self.ensure_cli();
        self.run(
            "refuse_pre_run_identity",
            "FSM_NATIVE_PRE_RUN_IDENTITY_REFUSED",
        );
    }

    pub(super) fn refuse_pre_run_boot(&self) {
        self.ensure_cli();
        self.run("refuse_pre_run_boot", "FSM_NATIVE_PRE_RUN_BOOT_REFUSED");
    }

    pub(super) fn refuse_pre_run_socket(&self) {
        self.ensure_cli();
        self.run("refuse_pre_run_socket", "FSM_NATIVE_PRE_RUN_SOCKET_REFUSED");
    }

    pub(super) fn refuse_changed_domain(&self) {
        self.ensure_cli();
        self.run("refuse_changed_domain", "FSM_NATIVE_CHANGED_DOMAIN_REFUSED");
    }

    pub(super) fn refuse_pre_run_reused_cgroup(&self) {
        self.ensure_cli();
        self.run(
            "refuse_pre_run_reused_cgroup",
            "FSM_NATIVE_REUSED_CGROUP_REFUSED",
        );
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
            + Duration::from_secs(
                if matches!(case, "recover_orphan_cli" | "refuse_live_runner") {
                    22
                } else {
                    12
                },
            );
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
        if case == "recover_orphan_cli" {
            let responses: Vec<_> = stdout
                .lines()
                .filter_map(|line| line.strip_prefix("FSM_NATIVE_ORPHAN_CLI_RESPONSE "))
                .collect();
            assert_eq!(responses.len(), 1);
            #[allow(clippy::print_stdout)]
            // Verified public response retained by native acceptance logs.
            {
                println!("\nFSM_NATIVE_ORPHAN_CLI_RESPONSE {}", responses[0]);
            }
        }
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

#[test]
#[ignore = "configured unprivileged supervisor child inside enrolled native fixture"]
fn refuse_pre_run_socket() {
    refuse_pre_run("native discovery socket identity or access differs");
    #[allow(clippy::print_stdout)] // Frozen marker consumed by the root fixture.
    {
        println!("\nFSM_NATIVE_PRE_RUN_SOCKET_REFUSED");
    }
}

#[test]
#[ignore = "configured unprivileged operator inside an orphaned native fixture"]
fn refuse_changed_domain() {
    refuse_pre_run("native shutdown refused: claimed closure original protected binding differs");
    #[allow(clippy::print_stdout)] // Frozen marker consumed by the root fixture.
    {
        println!("\nFSM_NATIVE_CHANGED_DOMAIN_REFUSED");
    }
}

#[test]
#[ignore = "configured unprivileged operator inside a physically reused cgroup fixture"]
fn refuse_pre_run_reused_cgroup() {
    refuse_pre_run("native shutdown refused: native cgroup identity differs");
    #[allow(clippy::print_stdout)] // Frozen marker consumed by the Root fixture.
    {
        println!("\nFSM_NATIVE_REUSED_CGROUP_REFUSED");
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
    refuse_original_cli(&path, run, expected_message, &records);
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
    refuse_original_cli(
        &copied,
        run_id,
        "native discovery store registration missing",
        records,
    );
    assert_eq!(Store::open_read_only(path).unwrap().records, records);
}

fn refuse_original_cli(
    path: &Path,
    run_id: u64,
    expected_message: &str,
    records: &[fsm_core::record::Record],
) {
    use std::process::{Command, Stdio};

    let ownership = Store::open_read_only(path).unwrap().state.execution.clone();
    let output = path.join("identity-reconcile.stdout");
    let errors = path.join("identity-reconcile.stderr");
    let executable = PathBuf::from(std::env::var_os("FSM_NATIVE_ORPHAN_CLI").unwrap());
    let mut child = Command::new(executable)
        .args(["--json", "--data-dir"])
        .arg(path)
        .args([
            "execute",
            "reconcile",
            "--run-id",
            &run_id.to_string(),
            "--timeout-ms",
            "1000",
        ])
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output).unwrap())
        .stderr(fs::File::create(&errors).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            child.wait().unwrap();
            panic!("original identity CLI refusal exceeded its bound; authority retained");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(!status.success());
    assert!(fs::metadata(output).unwrap().len() == 0);
    assert!(fs::metadata(&errors).unwrap().len() <= 8192);
    let response = fsm_core::json::parse(
        &fs::read(errors).unwrap(),
        &fsm_core::json::JsonLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(
        response.get("code").and_then(Value::as_str),
        Some("exec/inflight_deferred")
    );
    assert_eq!(
        response.get("message").and_then(Value::as_str),
        Some(expected_message)
    );
    assert_eq!(
        response.get("hint").and_then(Value::as_str),
        Some(
            "retain the original run; recover original results or restore its authority facilities before retrying reconciliation"
        )
    );
    let observed = Store::open_read_only(path).unwrap();
    assert_eq!(observed.records, records);
    assert_eq!(observed.state.execution, ownership);
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
    refuse_original_cli(
        &path,
        run_id,
        "native shutdown refused: original runner remains active or lease locking is unavailable",
        &records,
    );
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

    // Submit both production callers before observing either result; the store
    // writer must serialize first closure with historical duplicate recovery.
    let mut callers = [0, 1].map(|index| {
        let output = path.join(format!("orphan-cli-{index}.stdout"));
        let errors = path.join(format!("orphan-cli-{index}.stderr"));
        let child = Command::new(cli)
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
            .stdout(fs::File::create(&output).unwrap())
            .stderr(fs::File::create(&errors).unwrap())
            .spawn()
            .unwrap();
        (child, output, errors)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut statuses = [None, None];
    loop {
        for (index, (child, _, _)) in callers.iter_mut().enumerate() {
            if statuses[index].is_none() {
                statuses[index] = child.try_wait().unwrap();
            }
        }
        if statuses.iter().all(Option::is_some) {
            break;
        }
        if Instant::now() >= deadline {
            // Reap only these fixture-owned callers; retain native authority
            // because a caller timeout establishes no tree closure.
            for (index, (child, _, _)) in callers.iter_mut().enumerate() {
                if statuses[index].is_none() {
                    let _ = child.kill();
                    child.wait().unwrap();
                }
            }
            panic!("orphan CLI race exceeded its bound; authority retained");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut closure = None;
    for ((_, output, errors), status) in callers.into_iter().zip(statuses) {
        assert!(fs::metadata(&output).unwrap().len() <= 8192);
        assert!(fs::metadata(&errors).unwrap().len() <= 8192);
        let status = status.unwrap();
        if !status.success() {
            assert_eq!(status.code(), Some(4));
            assert!(fs::read(&output).unwrap().is_empty());
            let refusal = fsm_core::json::parse(
                &fs::read(&errors).unwrap(),
                &fsm_core::json::JsonLimits::DEFAULT,
            )
            .unwrap();
            assert_eq!(refusal.get("code"), Some(&Value::Str("store/lock".into())));
            continue;
        }
        assert!(fs::read(&errors).unwrap().is_empty());
        let response = fsm_core::json::parse(
            &fs::read(&output).unwrap(),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        assert_eq!(
            response.get("execution").unwrap().get("run_id"),
            Some(&Value::Num(run_id.to_string())),
        );
        assert_eq!(
            response.get("execution").unwrap().get("disposition"),
            Some(&Value::Str("interrupted".into())),
        );
        match response.get("duplicate") {
            Some(Value::Bool(false)) => {
                assert!(
                    closure.replace(response).is_none(),
                    "two callers committed first closure"
                );
            }
            Some(Value::Bool(true)) => {}
            other => panic!("orphan CLI race duplicate classification differs: {other:?}"),
        }
    }
    let response = closure.expect("one production caller must commit first closure");
    let observed = Store::open_read_only(path).unwrap();
    assert_eq!(&observed.records[..records.len()], records);
    assert_eq!(observed.records.len(), records.len() + 2);
    assert_eq!(observed.state.execution.unresolved().count(), 0);
    #[allow(clippy::print_stdout)] // Only the verified first-closure response is retained.
    {
        println!(
            "\nFSM_NATIVE_ORPHAN_CLI_RESPONSE {}",
            String::from_utf8(fsm_core::canon::canon_bytes(&response)).unwrap()
        );
    }
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
