//! Genuine acknowledgement recovery uses the original event contract.

use super::*;

pub(super) fn resume_original_event(
    fixture: &Fixture,
    effect: &str,
    completion: &fsm_execute::run::native_client::NativeCompletion,
    mode: &str,
) {
    use super::super::broker_cases::{Daemon, disconnect_cases};
    use fsm_execute::rid::event_rid;
    use std::process::{Command, Stdio};

    let snapshot = Store::open_read_only(&fixture.store).unwrap();
    let before = snapshot.records.len();
    assert_eq!(snapshot.state.execution.unresolved().count(), 0);
    let original = snapshot
        .state
        .execution_handoffs
        .outstanding()
        .next()
        .unwrap();
    assert_eq!(
        original.handler_contract(),
        &completion.handler().contract_value()
    );
    assert_eq!(original.event_request_id(), event_rid(effect, "docs_ok"));
    let retained_claim = original.claim().clone();
    assert_eq!(
        snapshot.state.instances["instance"]
            .configuration
            .sequential_leaf(),
        Some("docs_review")
    );
    drop(snapshot);
    let counter = fixture.counter();

    disconnect_cases::permit_operator_store(&fixture.store);
    super::super::super::super::broker_endpoint::provision(&fixture.directory, 65534).unwrap();
    let daemon = Daemon::ready(&fixture.directory, 1);
    disconnect_cases::install_supervisor(&fixture.directory);
    let script = disconnect_cases::SUPERVISOR.replace(
        "supervisor_probe::owned_request",
        "runner_cases::handoff_recovery::recovered_event",
    );
    let parameters = fsm_core::canon::canon_bytes(&object([
        ("effect", Value::Str(effect.into())),
        (
            "changed",
            Value::Bool(matches!(
                mode,
                "process-recover-changed" | "process-recover-public-tick-with"
            )),
        ),
        (
            "host",
            Value::Str(
                match mode {
                    "process-recover-public-tick" => "tick",
                    "process-recover-public-tick-with" => "tick_with",
                    _ => "paired",
                }
                .into(),
            ),
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
        "unprivileged handoff recovery failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed; 0 ignored;"));
    assert_eq!(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .filter(|line| *line == "FSM_NATIVE_HANDOFF_RECOVERED")
            .count(),
        1
    );
    assert_eq!(fixture.counter(), counter);
    let reopened = Store::open_read_only(&fixture.store).unwrap();
    assert_eq!(reopened.records.len(), before + 1);
    assert_eq!(reopened.state.execution_handoffs.outstanding().count(), 0);
    let delivered = reopened.records.clone();
    drop(reopened);
    // An independently retained original completion must retire after the
    // other host's accepted event, even though that event is now disabled.
    let mut writer = Store::open(&fixture.store).unwrap();
    assert_eq!(
        fsm_execute::run::Pipeline
            .advance_native_settled(
                &mut writer,
                &mut fsm_store::clock::FixedClock::new(3000, 1),
                &retained_claim,
                completion,
                &fsm_execute::rid::ack_rid(effect),
            )
            .unwrap(),
        fsm_execute::run::SettleOutcome::AlreadySettled
    );
    assert_eq!(writer.records, delivered);
    drop(writer);
    drop(daemon);
}

#[test]
#[ignore = "invoked as the configured unprivileged operator by the native recovery fixture"]
fn recovered_event() {
    use fsm_core::{
        json::{JsonLimits, parse},
        record::RecordKind,
    };
    use fsm_execute::{
        config::{HandlerSpec, HandlerTable},
        rid::event_rid,
        service::{ExecutorPhase, ShutdownMode},
    };
    use fsm_store::clock::FixedClock;

    assert_eq!(fs::metadata("/proc/self").unwrap().uid(), 65534);
    let encoded = std::env::var("FSM_NATIVE_TEST_BINDING").unwrap();
    assert!(encoded.len() <= 8192);
    let parameters = parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    let effect = parameters.get("effect").and_then(Value::as_str).unwrap();
    let changed = parameters.get("changed").and_then(Value::as_bool).unwrap();
    let host = parameters.get("host").and_then(Value::as_str).unwrap();
    let path = PathBuf::from(std::env::var_os("FSM_NATIVE_TEST_STORE").unwrap());
    let snapshot = Store::open_read_only(&path).unwrap();
    let before = snapshot.records.len();
    let original = snapshot
        .state
        .execution_handoffs
        .outstanding()
        .next()
        .unwrap();
    let material = original.claim().to_value();
    let fingerprint = material
        .get("handler_fingerprint")
        .and_then(Value::as_str)
        .unwrap();
    let original_handler =
        HandlerSpec::from_contract(original.handler_contract(), fingerprint).unwrap();
    drop(snapshot);

    let mut table = HandlerTable::default();
    if changed {
        let mut handler = original_handler.clone();
        handler.argv = vec!["/fixture-handler-must-not-run".into()];
        handler.on_ok.as_mut().unwrap().event = "withdraw".into();
        assert_ne!(handler.fingerprint(), original_handler.fingerprint());
        table.handlers.insert(handler.effect.clone(), handler);
    }
    // Reconstruct the real standalone paired driver from durable state, with
    // no retained completion object supplied to it and no original table.
    let mut driver = RecoveryDriver::new(&path, table, host);
    driver.check_readonly(&path);
    let mut clock = FixedClock::new(2000, 1);
    let lines = driver.tick(&path, &mut clock, 2000);
    assert!(
        lines
            .iter()
            .any(|line| line.starts_with("native-handoff advanced")),
        "{lines:?}"
    );
    let settled = Store::open_read_only(&path).unwrap();
    assert_eq!(settled.records.len(), before + 1);
    assert_eq!(
        settled.records.last().unwrap().kind,
        RecordKind::EventApplied
    );
    assert_eq!(
        settled.state.instances["instance"]
            .configuration
            .sequential_leaf(),
        Some("risk_review")
    );
    assert!(
        settled
            .state
            .dedup
            .contains_key(&event_rid(effect, "docs_ok"))
    );
    assert!(
        !settled
            .state
            .dedup
            .contains_key(&event_rid(effect, "withdraw"))
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
    // This marker is parsed by the parent; it follows all production-driver
    // and reopened-journal assertions rather than standing in for them.
    #[allow(clippy::print_stdout)]
    {
        println!("\nFSM_NATIVE_HANDOFF_RECOVERED");
    }
}

enum RecoveryDriver {
    Paired(Box<fsm_execute::service::PairedNativeExecutor>),
    Public(Box<PublicTick>),
}

struct PublicTick {
    watcher: fsm_execute::watch::Watcher,
    scheduler: fsm_execute::sched::Scheduler,
    runner: fsm_execute::run::Runner,
    pipeline: fsm_execute::run::Pipeline,
    borrowed: bool,
}

impl RecoveryDriver {
    fn new(path: &Path, table: fsm_execute::config::HandlerTable, host: &str) -> Self {
        match host {
            "paired" => {
                Self::Paired(fsm_execute::service::PairedNativeExecutor::new(path, table).unwrap())
            }
            "tick" | "tick_with" => Self::Public(Box::new(PublicTick {
                watcher: fsm_execute::watch::Watcher::with_handlers(path.to_path_buf(), &table),
                scheduler: fsm_execute::sched::Scheduler::new(table),
                runner: fsm_execute::run::Runner::new_native().unwrap(),
                pipeline: fsm_execute::run::Pipeline,
                borrowed: host == "tick_with",
            })),
            _ => panic!("unknown native recovery host"),
        }
    }

    fn check_readonly(&mut self, path: &Path) {
        if let Self::Public(parts) = self {
            let mut readonly = Store::open_read_only(path).unwrap();
            let records = readonly.records.clone();
            let lines = fsm_execute::service::tick_with(
                &mut parts.watcher,
                &mut parts.scheduler,
                &mut parts.runner,
                &mut parts.pipeline,
                &mut readonly,
                &mut fsm_store::clock::FixedClock::new(1500, 1),
                1500,
            );
            assert!(
                !lines
                    .iter()
                    .any(|line| line.starts_with("native-handoff advanced"))
            );
            assert_eq!(readonly.records, records);
            assert_eq!(Store::open_read_only(path).unwrap().records, records);
            assert_eq!(readonly.state.execution_handoffs.outstanding().count(), 1);
        }
    }

    fn tick(
        &mut self,
        path: &Path,
        clock: &mut dyn fsm_store::clock::Clock,
        now: i64,
    ) -> Vec<String> {
        match self {
            Self::Paired(driver) => driver.tick(clock, now),
            Self::Public(parts) if parts.borrowed => {
                let mut writer = Store::open(path).unwrap();
                fsm_execute::service::tick_with(
                    &mut parts.watcher,
                    &mut parts.scheduler,
                    &mut parts.runner,
                    &mut parts.pipeline,
                    &mut writer,
                    clock,
                    now,
                )
            }
            Self::Public(parts) => fsm_execute::service::tick(
                &mut parts.watcher,
                &mut parts.scheduler,
                &mut parts.runner,
                &mut parts.pipeline,
                path,
                clock,
                now,
            ),
        }
    }

    fn into_paired(self, path: &Path) -> fsm_execute::service::PairedNativeExecutor {
        match self {
            Self::Paired(driver) => *driver,
            Self::Public(parts) => fsm_execute::service::PairedNativeExecutor::from_owned_parts(
                Store::open_read_only(path).unwrap(),
                parts.watcher,
                parts.scheduler,
                parts.runner,
            )
            .unwrap(),
        }
    }
}
