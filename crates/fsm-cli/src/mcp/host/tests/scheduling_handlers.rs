//! Genuine timeout/retry acceptance pauses the original owner at wait boundaries.
//! Logical eligibility is injected; authority results and closure remain genuine.

use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use fsm_core::{machine::Status, record::RecordKind};
use fsm_execute::{config::HandlerTable, service::OwnedNativeExecutor};

use crate::{clock::Clock, mcp::notify::diagnostic_output::DiagnosticOutput};

use super::super::{
    Handle,
    mailbox::{Mailbox, Next},
    native::{NativeOwner, WaitClock},
};
use super::{FixedClock, Store, held_handlers, value};

struct LogicalClock(Arc<AtomicI64>);

impl Clock for LogicalClock {
    fn now_ms(&mut self) -> i64 {
        self.0.load(Ordering::Acquire)
    }
}

struct ControlledWait {
    epoch: Instant,
    elapsed: Arc<AtomicUsize>,
    boundaries: mpsc::SyncSender<()>,
    proceed: mpsc::Receiver<()>,
}

impl WaitClock for ControlledWait {
    fn now(&self) -> Instant {
        self.epoch + Duration::from_millis(self.elapsed.load(Ordering::Acquire) as u64)
    }

    fn wait(&self, mailbox: &Mailbox, deadline: Instant) -> Next {
        // A rendezvous freezes a complete original owner pass before observation.
        if self.boundaries.send(()).is_err() || self.proceed.recv().is_err() {
            return Next::Stopped;
        }
        mailbox.next_until_with(deadline, || self.now())
    }
}

struct WaitGate {
    elapsed: Arc<AtomicUsize>,
    boundaries: mpsc::Receiver<()>,
    proceed: mpsc::SyncSender<()>,
}

impl WaitGate {
    fn new() -> (Self, ControlledWait) {
        let elapsed = Arc::new(AtomicUsize::new(0));
        let (arrived, boundaries) = mpsc::sync_channel(0);
        let (proceed, released) = mpsc::sync_channel(0);
        (
            Self {
                elapsed: Arc::clone(&elapsed),
                boundaries,
                proceed,
            },
            ControlledWait {
                epoch: Instant::now(),
                elapsed,
                boundaries: arrived,
                proceed: released,
            },
        )
    }

    fn advance(&self, handle: &Handle) {
        self.elapsed.fetch_add(1, Ordering::Release);
        self.proceed.send(()).unwrap();
        handle.mailbox.wake();
        self.boundaries
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
    }

    fn until(&self, handle: &Handle, mut ready: impl FnMut() -> bool) {
        let watchdog = Instant::now() + Duration::from_secs(20);
        while !ready() {
            assert!(Instant::now() < watchdog, "genuine native progress stalled");
            self.advance(handle);
            // Observe asynchronous native I/O; this does not establish logical eligibility.
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn claim_count(store: &Store) -> usize {
    store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionClaimed)
        .count()
}

#[test]
#[ignore = "requires disposable native CI, protected authority timeout and genuine retry handlers"]
fn autonomous_schedule_real_timeout_retry_pins_backoff_without_another_command() {
    let manifest = held_handlers::manifest();
    assert_eq!(
        held_handlers::field(&manifest, "behavior"),
        "schedule-retry"
    );
    let store_path = PathBuf::from(held_handlers::field(&manifest, "store"));
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
    let handlers =
        HandlerTable::parse(&fs::read_to_string(store_path.join("handlers.json")).unwrap())
            .unwrap();
    assert_eq!(handlers.handlers["notify"].retry.backoff_ms, 10);
    let logical = Arc::new(AtomicI64::new(2000));
    let (gate, wait_clock) = WaitGate::new();
    let (owner, handle) = NativeOwner::new(
        OwnedNativeExecutor::new(store, handlers).unwrap(),
        LogicalClock(Arc::clone(&logical)),
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(1),
        10000,
    )
    .unwrap();
    let owner = owner.with_wait_clock(wait_clock);
    let worker = std::thread::spawn(move || owner.run());
    gate.boundaries
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    // No session exists and no command or direct executor tick is submitted.
    gate.until(&handle, || {
        fs::read_to_string(resource.join("root-candidate")).is_ok_and(|pid| !pid.is_empty())
    });
    let first_root = fs::read_to_string(resource.join("root-candidate")).unwrap();
    assert!(!resource.join("root-release").exists());
    gate.until(&handle, || {
        let observed = Store::open_read_only(&store_path).unwrap();
        observed
            .records
            .iter()
            .any(|record| record.kind == RecordKind::ExecutionSettled)
    });
    let first = Store::open_read_only(&store_path).unwrap();
    assert_eq!(claim_count(&first), 1);
    assert_eq!(first.state.execution.unresolved().count(), 0);
    let stopped = first
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionStopped)
        .unwrap();
    assert_eq!(
        stopped
            .body
            .get("outcome")
            .and_then(|outcome| outcome.get("status"))
            .and_then(super::Value::as_str),
        Some("timeout")
    );
    let settled = first
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionSettled)
        .unwrap();
    assert_eq!(settled.ts, 2000);
    assert_eq!(
        settled
            .body
            .get("disposition")
            .and_then(super::Value::as_str),
        Some("attempted")
    );
    let prefix = first.journal.last_seq;
    drop(first);

    logical.store(2009, Ordering::Release);
    for _ in 0..8 {
        gate.advance(&handle);
    }
    let early = Store::open_read_only(&store_path).unwrap();
    assert_eq!(claim_count(&early), 1);
    assert_eq!(early.journal.last_seq, prefix);
    assert_eq!(early.state.instances["held"].pending.len(), 1);
    drop(early);

    logical.store(2010, Ordering::Release);
    gate.until(&handle, || {
        let observed = Store::open_read_only(&store_path).unwrap();
        claim_count(&observed) == 2
            && fs::read_to_string(resource.join("root-candidate"))
                .is_ok_and(|pid| !pid.is_empty() && pid != first_root)
    });
    let retry = Store::open_read_only(&store_path).unwrap();
    let claims = retry
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionClaimed)
        .collect::<Vec<_>>();
    assert_eq!(claims.len(), 2);
    assert_eq!(claims[1].ts, 2010);
    assert_eq!(
        claims[1].body.get("attempt").and_then(super::Value::as_num),
        Some("2")
    );
    drop(retry);
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    gate.until(&handle, || {
        Store::open_read_only(&store_path).unwrap().state.instances["held"].status
            == Status::Completed
    });
    handle.stop();
    drop(gate);
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
    let reopened = Store::open(&store_path).unwrap();
    assert_eq!(claim_count(&reopened), 2);
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    assert_eq!(reopened.state.instances["held"].status, Status::Completed);
    assert_eq!(
        crate::journal_io::verify(&store_path).health,
        crate::journal_io::JournalHealth::Ok
    );
}

const COMPENSATION_MACHINE: &str = r#"{
 "format":"fsm.machine/1","name":"quiet_compensation","context":[],
 "events":[{"name":"done","fields":[]},{"name":"failed","fields":[]}],
 "effects":[{"name":"notify","fields":[]},{"name":"restore","fields":[]}],
 "states":[
   {"name":"working","entry":{"emit":[{"effect":"notify","args":{}}]}},
   {"name":"recovering","entry":{"emit":[{"effect":"restore","args":{}}]}},
   {"name":"restored","terminal":true},{"name":"cleanup_failed","terminal":true}],
 "initial":"working","transitions":[
   {"from":"working","on":"failed","to":"recovering"},
   {"from":"recovering","on":"done","to":"restored"},
   {"from":"recovering","on":"failed","to":"cleanup_failed"}]
}"#;

#[test]
#[ignore = "requires disposable native CI, genuine failure and a protected restore handler"]
fn autonomous_schedule_real_compensation_completes_without_another_command() {
    let manifest = held_handlers::manifest();
    assert_eq!(
        held_handlers::field(&manifest, "behavior"),
        "schedule-compensation"
    );
    let store_path = PathBuf::from(held_handlers::field(&manifest, "store"));
    let resource = PathBuf::from(held_handlers::field(&manifest, "resource"));
    let mut store = Store::open(&store_path).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    store
        .define_machine_on(&mut clock, value(COMPENSATION_MACHINE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "quiet_compensation",
            "held",
            "create-held",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    let handlers =
        HandlerTable::parse(&fs::read_to_string(store_path.join("handlers.json")).unwrap())
            .unwrap();
    assert_eq!(handlers.handlers["notify"].retry.attempts, 1);
    assert!(handlers.handlers.contains_key("restore"));
    let restore_fingerprint = handlers.handlers["restore"].fingerprint();
    let (gate, wait_clock) = WaitGate::new();
    let (owner, handle) = NativeOwner::new(
        OwnedNativeExecutor::new(store, handlers).unwrap(),
        FixedClock::new(2000, 0),
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(1),
        10000,
    )
    .unwrap();
    let worker = std::thread::spawn(move || owner.with_wait_clock(wait_clock).run());
    gate.boundaries
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    // Only wait-clock wakes and genuine completion I/O drive this owner.
    gate.until(&handle, || {
        fs::read_to_string(resource.join("root-candidate")).is_ok_and(|pid| !pid.is_empty())
    });
    let first_root = fs::read_to_string(resource.join("root-candidate")).unwrap();
    assert!(!resource.join("root-release").exists());
    gate.until(&handle, || {
        let observed = Store::open_read_only(&store_path).unwrap();
        claim_count(&observed) == 2
            && fs::read_to_string(resource.join("root-candidate"))
                .is_ok_and(|pid| !pid.is_empty() && pid != first_root)
    });
    let compensating = Store::open_read_only(&store_path).unwrap();
    let claims = compensating
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionClaimed)
        .collect::<Vec<_>>();
    assert_eq!(claims.len(), 2);
    assert_ne!(
        claims[0].body.get("effect_id"),
        claims[1].body.get("effect_id")
    );
    assert_eq!(
        claims[1]
            .body
            .get("handler_fingerprint")
            .and_then(super::Value::as_str),
        Some(restore_fingerprint.as_str())
    );
    assert_eq!(
        claims[1].body.get("attempt").and_then(super::Value::as_num),
        Some("1")
    );
    let first_stopped = compensating
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionStopped)
        .unwrap();
    assert_eq!(
        first_stopped
            .body
            .get("outcome")
            .and_then(|outcome| outcome.get("status"))
            .and_then(super::Value::as_str),
        Some("timeout")
    );
    let first_settled = compensating
        .records
        .iter()
        .find(|record| record.kind == RecordKind::ExecutionSettled)
        .unwrap();
    assert_eq!(
        first_settled
            .body
            .get("disposition")
            .and_then(super::Value::as_str),
        Some("acked")
    );
    assert_eq!(compensating.state.instances["held"].pending.len(), 1);
    drop(compensating);
    for role in ["grandchild", "child", "root"] {
        fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
    }
    gate.until(&handle, || {
        Store::open_read_only(&store_path).unwrap().state.instances["held"].status
            == Status::Completed
    });
    handle.stop();
    drop(gate);
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
    let reopened = Store::open(&store_path).unwrap();
    assert_eq!(reopened.state.instances["held"].status, Status::Completed);
    let view = reopened.instance_view("held", None, None).unwrap();
    assert_eq!(
        view.get("configuration")
            .and_then(|configuration| configuration.get("leaf"))
            .and_then(super::Value::as_str),
        Some("restored")
    );
    assert!(reopened.state.instances["held"].pending.is_empty());
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    assert_eq!(claim_count(&reopened), 2);
    let stopped = reopened
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::ExecutionStopped)
        .collect::<Vec<_>>();
    assert_eq!(stopped.len(), 2);
    assert_eq!(
        stopped[1]
            .body
            .get("outcome")
            .and_then(|outcome| outcome.get("status"))
            .and_then(super::Value::as_str),
        Some("ok")
    );
    assert_eq!(
        crate::journal_io::verify(&store_path).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
#[ignore = "requires disposable native CI and nine genuine independently held completions"]
fn autonomous_schedule_ready_completions_yield_to_admitted_application_within_eight_turns() {
    let manifest = held_handlers::manifest();
    assert_eq!(
        held_handlers::field(&manifest, "behavior"),
        "schedule-queues"
    );
    let path = PathBuf::from(held_handlers::field(&manifest, "store"));
    let resource = PathBuf::from(held_handlers::field(&manifest, "resource"));
    let mut store = Store::open(&path).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    for index in 0..9 {
        let name = format!("queue-{index}");
        let definition = held_handlers::HELD_MACHINE
            .replace("held_completion", &name)
            .replace("notify", &format!("notify-{index}"));
        store
            .define_machine_on(&mut clock, value(&definition), false, false)
            .unwrap();
        store
            .create_instance_ctx_on(
                &mut clock,
                &name,
                &name,
                &name,
                None,
                &std::collections::BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let handlers =
        HandlerTable::parse(&fs::read_to_string(path.join("handlers.json")).unwrap()).unwrap();
    assert_eq!(handlers.max_inflight, 9);
    assert_eq!(handlers.max_inflight_per_instance, 1);
    let (gate, wait_clock) = WaitGate::new();
    let (owner, handle) = NativeOwner::new(
        OwnedNativeExecutor::new(store, handlers).unwrap(),
        clock,
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(1),
        10000,
    )
    .unwrap();
    let worker = std::thread::spawn(move || owner.with_wait_clock(wait_clock).run());
    gate.boundaries
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    gate.until(&handle, || {
        (0..9).all(|index| {
            resource
                .join(format!("run-{index}/root-candidate"))
                .is_file()
        })
    });
    let before = Store::open_read_only(&path).unwrap();
    assert_eq!(claim_count(&before), 9);
    assert_eq!(before.state.execution.unresolved().count(), 9);
    assert!(
        !before
            .records
            .iter()
            .any(|record| record.kind == RecordKind::ExecutionSettled)
    );
    let prefix = before.journal.last_seq;
    drop(before);
    let sessions = (0..4)
        .map(|_| handle.session().unwrap())
        .collect::<Vec<_>>();
    let mut replies = std::collections::VecDeque::new();
    for index in 0..32 {
        replies.push_back(
            sessions[index % 4]
                .submit(super::command(
                    "instance_get",
                    r#"{"instance_id":"queue-0"}"#,
                ))
                .unwrap(),
        );
    }
    // Every original native tree is released; no outcome or closure is invented.
    for index in 0..9 {
        for role in ["grandchild", "child", "root"] {
            fs::write(
                resource.join(format!("run-{index}/{role}-release")),
                b"release",
            )
            .unwrap();
        }
    }
    let watchdog = Instant::now() + Duration::from_secs(20);
    let mut previous = prefix;
    let mut served = 0;
    let completed_while_ready;
    loop {
        assert!(
            Instant::now() < watchdog,
            "completion queue stalled behind ready application requests"
        );
        gate.advance(&handle);
        let outcome = replies
            .pop_front()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        assert!(outcome.result.is_ok());
        let observed = Store::open_read_only(&path).unwrap();
        let settled = observed
            .records
            .iter()
            .filter(|record| {
                record.kind == RecordKind::ExecutionSettled
                    && record.seq > previous
                    && record.seq <= outcome.committed_seq
            })
            .count();
        assert!(
            settled <= 8,
            "more than eight completions preceded an admitted application response"
        );
        previous = outcome.committed_seq;
        replies.push_back(
            sessions[served % 4]
                .submit(super::command(
                    "instance_get",
                    r#"{"instance_id":"queue-0"}"#,
                ))
                .unwrap(),
        );
        served += 1;
        if observed
            .state
            .instances
            .values()
            .all(|instance| instance.status == Status::Completed)
        {
            assert_eq!(claim_count(&observed), 9);
            assert_eq!(
                observed
                    .records
                    .iter()
                    .filter(|record| record.kind == RecordKind::ExecutionSettled)
                    .count(),
                9
            );
            assert_eq!(observed.state.execution.unresolved().count(), 0);
            completed_while_ready = replies.len() == 32;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(served > 0 && completed_while_ready);
    handle.stop();
    drop(gate);
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
    assert_eq!(
        crate::journal_io::verify(&path).health,
        crate::journal_io::JournalHealth::Ok
    );
}
