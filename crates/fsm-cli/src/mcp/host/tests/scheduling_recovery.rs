//! A genuine acknowledgement crash reopens the private owner without client input.
//! The protected journal barrier is active only in the original child process.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

use fsm_core::{machine::Status, record::RecordKind};
use fsm_execute::{config::HandlerTable, service::OwnedNativeExecutor};

use crate::{
    mcp::{host::native::NativeOwner, notify::diagnostic_output::DiagnosticOutput},
    store::Store,
};

use super::{FixedClock, Value, held_handlers, value};

const OBSERVER: &str = "mcp::host::tests::scheduling_recovery::autonomous_schedule_reopened_acknowledgement_advances_without_rpc";

fn owner(path: &Path) -> (NativeOwner<FixedClock>, super::super::Handle) {
    let store = Store::open(path).unwrap();
    let handlers =
        HandlerTable::parse(&fs::read_to_string(path.join("handlers.json")).unwrap()).unwrap();
    NativeOwner::new(
        OwnedNativeExecutor::new(store, handlers).unwrap(),
        FixedClock::new(2000, 0),
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(5),
        10000,
    )
    .unwrap()
}

fn count(store: &Store, kind: RecordKind) -> usize {
    store
        .records
        .iter()
        .filter(|record| record.kind == kind)
        .count()
}

fn wait_until(mut ready: impl FnMut() -> bool) {
    let watchdog = Instant::now() + Duration::from_secs(12);
    while !ready() {
        assert!(
            Instant::now() < watchdog,
            "genuine acknowledgement recovery stalled"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires disposable native CI, protected acknowledgement cut and genuine original closure"]
fn autonomous_schedule_reopened_acknowledgement_advances_without_rpc() {
    let manifest = held_handlers::manifest();
    assert_eq!(
        held_handlers::field(&manifest, "behavior"),
        "schedule-recovery"
    );
    let store_path = PathBuf::from(held_handlers::field(&manifest, "store"));
    if std::env::var("FSM_SCHEDULING_CRASH_ACTOR").as_deref() == Ok("1") {
        // This child is the original private owner, never a second writer.
        let (owner, _handle) = owner(&store_path);
        owner.run();
        panic!("original owner returned before the acknowledgement crash");
    }
    let resource = PathBuf::from(held_handlers::field(&manifest, "resource"));
    let mut store = Store::open(&store_path).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    store
        .define_machine_on(&mut clock, value(held_handlers::HELD_MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "held_completion",
            "held",
            "create-held",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    let before = store.journal.last_seq;
    drop(store);
    let mut original = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            OBSERVER,
            "--ignored",
            "--nocapture",
            "--color",
            "never",
        ])
        .env("FSM_SCHEDULING_CRASH_ACTOR", "1")
        .env(
            "FSM_LIFECYCLE_JOURNAL_CUT",
            Path::new(held_handlers::field(&manifest, "authority"))
                .join("crash-journal-barrier.json"),
        )
        .env("TMPDIR", &store_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(store_path.join("original-private-owner.stderr")).unwrap())
        .spawn()
        .unwrap();
    wait_until(|| {
        assert!(
            original.try_wait().unwrap().is_none(),
            "original owner exited before handler entry"
        );
        resource.join("root-candidate").is_file()
    });
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    wait_until(|| {
        assert!(
            original.try_wait().unwrap().is_none(),
            "original owner exited before protected cut"
        );
        store_path.join("journal-cut-ready.json").is_file()
    });
    let cut = value(&fs::read_to_string(store_path.join("journal-cut-ready.json")).unwrap());
    assert_eq!(cut.get("cut").and_then(Value::as_str), Some("acked"));
    assert_eq!(
        cut.get("pid").and_then(Value::as_num),
        Some(original.id().to_string().as_str())
    );
    let frozen = Store::open_read_only(&store_path).unwrap();
    assert_eq!(
        cut.get("seq").and_then(Value::as_num),
        Some(frozen.journal.last_seq.to_string().as_str())
    );
    assert_eq!(count(&frozen, RecordKind::ExecutionClaimed), 1);
    assert_eq!(count(&frozen, RecordKind::ExecutionStopped), 1);
    assert_eq!(count(&frozen, RecordKind::ExecutionSettled), 1);
    assert_eq!(count(&frozen, RecordKind::EventApplied), 0);
    assert_eq!(frozen.state.instances["held"].status, Status::Running);
    assert!(frozen.state.instances["held"].pending.is_empty());
    assert_eq!(frozen.state.execution_handoffs.outstanding().count(), 1);
    let prefix = frozen.journal.last_seq;
    drop(frozen);
    original.kill().unwrap();
    assert!(!original.wait().unwrap().success());
    assert_eq!(
        crate::journal_io::verify(&store_path).health,
        crate::journal_io::JournalHealth::Ok
    );
    assert_eq!(
        Store::open_read_only(&store_path).unwrap().journal.last_seq,
        prefix
    );
    // No crash hook, session, command, or direct tick is used by the successor.
    let (reopened, handle) = owner(&store_path);
    let worker = std::thread::spawn(move || reopened.run());
    wait_until(|| {
        Store::open_read_only(&store_path).unwrap().state.instances["held"].status
            == Status::Completed
    });
    handle.stop();
    wait_until(|| worker.is_finished());
    let exit = worker.join().unwrap();
    assert!(exit.failure.is_none());
    assert_eq!(
        exit.shutdown.phase,
        fsm_execute::service::ExecutorPhase::Stopped
    );
    assert!(
        exit.shutdown.writer_released
            && exit.shutdown.inventory_complete
            && exit.shutdown.helpers_retired
    );
    assert_eq!(exit.diagnostics.dropped(), 0);
    let completed = Store::open(&store_path).unwrap();
    assert_eq!(completed.journal.last_seq, prefix + 1);
    assert!(prefix > before);
    for kind in [
        RecordKind::ExecutionClaimed,
        RecordKind::ExecutionStopped,
        RecordKind::ExecutionSettled,
        RecordKind::EventApplied,
    ] {
        assert_eq!(count(&completed, kind), 1);
    }
    assert_eq!(completed.state.execution.unresolved().count(), 0);
    assert_eq!(completed.state.execution_handoffs.outstanding().count(), 0);
    assert_eq!(completed.state.instances["held"].status, Status::Completed);
    assert_eq!(completed.records.last().unwrap().ts, 2000);
    assert_eq!(
        crate::journal_io::verify(&store_path).health,
        crate::journal_io::JournalHealth::Ok
    );
}
