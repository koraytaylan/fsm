//! A native decision pass shares one logical sample across durable operations.

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

use fsm_execute::{config::HandlerTable, service::OwnedNativeExecutor};

use crate::mcp::{host::native::NativeOwner, notify::diagnostic_output::DiagnosticOutput};
use crate::{
    clock::{Clock, FixedClock},
    store::Store,
};

use super::{Scratch, value};

struct CountingClock(Arc<AtomicUsize>);

impl Clock for CountingClock {
    fn now_ms(&mut self) -> i64 {
        1001 + self.0.fetch_add(1, Ordering::SeqCst) as i64
    }
}

struct ObservationClock {
    logical: Arc<AtomicI64>,
    calls: Arc<AtomicUsize>,
}

impl Clock for ObservationClock {
    fn now_ms(&mut self) -> i64 {
        let now = self.logical.load(Ordering::Acquire);
        self.calls.fetch_add(1, Ordering::Release);
        now
    }
}

struct ManualWaitClock {
    epoch: Instant,
    elapsed: Arc<AtomicUsize>,
    waits: std::sync::mpsc::Sender<Instant>,
}

impl super::super::native::WaitClock for ManualWaitClock {
    fn now(&self) -> Instant {
        self.epoch + Duration::from_millis(self.elapsed.load(Ordering::Acquire) as u64)
    }

    fn wait(
        &self,
        mailbox: &super::super::mailbox::Mailbox,
        deadline: Instant,
    ) -> super::super::mailbox::Next {
        // The barrier follows the complete owner pass, including journal writes.
        let _ = self.waits.send(deadline);
        mailbox.next_until_with(deadline, || self.now())
    }
}

#[test]
fn autonomous_schedule_idle_wait_clock_never_supplies_logical_deadline_time() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(super::interaction::CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "question_case",
            "question",
            "create-question",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let before = store.journal.last_seq;
    let calls = Arc::new(AtomicUsize::new(0));
    let logical = Arc::new(AtomicI64::new(1000));
    let elapsed = Arc::new(AtomicUsize::new(0));
    let epoch = Instant::now();
    let (waits, observed_waits) = std::sync::mpsc::channel();
    let (owner, handle) = NativeOwner::new(
        OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap(),
        ObservationClock {
            logical: Arc::clone(&logical),
            calls: Arc::clone(&calls),
        },
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(250),
        10000,
    )
    .unwrap();
    let owner = owner.with_wait_clock(ManualWaitClock {
        epoch,
        elapsed: Arc::clone(&elapsed),
        waits,
    });
    let worker = std::thread::spawn(move || owner.run());
    // Real time is only a deadlock watchdog; every scheduled wake is injected.
    let observations = (|| {
        let mut observations = Vec::new();
        for step in 0..=10 {
            let deadline = observed_waits.recv_timeout(Duration::from_secs(3))?;
            let observed = Store::open_read_only(&scratch.0).unwrap();
            observations.push((
                deadline.duration_since(epoch).as_millis(),
                calls.load(Ordering::Acquire),
                observed.journal.last_seq,
            ));
            if step == 10 {
                break;
            }
            if step == 9 {
                logical.store(1001, Ordering::Release);
            }
            elapsed.store((step + 1) * 50, Ordering::Release);
            handle.mailbox.wake();
        }
        Ok::<_, std::sync::mpsc::RecvTimeoutError>(observations)
    })();
    handle.stop();
    let exit = worker.join().unwrap();
    assert!(exit.shutdown.writer_released);
    let observations = observations.unwrap();
    for (step, (deadline, samples, sequence)) in observations.iter().enumerate() {
        assert_eq!(*deadline, ((step + 1) * 50) as u128);
        assert_eq!(*samples, [1, 2, 2, 2, 2, 3, 4, 4, 4, 4, 5][step]);
        assert_eq!(*sequence, before + u64::from(step == 10));
    }
    let reopened = Store::open(&scratch.0).unwrap();
    let applied = reopened
        .records
        .iter()
        .filter(|record| record.seq > before)
        .collect::<Vec<_>>();
    assert_eq!(applied.len(), 1);
    assert_eq!(
        applied[0].kind,
        fsm_core::record::RecordKind::DeadlineApplied
    );
    assert_eq!(applied[0].ts, 1001);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
fn autonomous_schedule_long_interval_observes_original_driver_without_deadline_admission() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(super::interaction::CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "question_case",
            "first",
            "first",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let before = store.journal.last_seq;
    let calls = Arc::new(AtomicUsize::new(0));
    let logical = Arc::new(AtomicI64::new(1000));
    let driver = OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap();
    let (owner, handle) = NativeOwner::new(
        driver,
        ObservationClock {
            logical: Arc::clone(&logical),
            calls: Arc::clone(&calls),
        },
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(86400000),
        10000,
    )
    .unwrap();
    let worker = std::thread::spawn(move || owner.run());
    let limit = Instant::now() + Duration::from_secs(3);
    while calls.load(Ordering::Acquire) == 0 && Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(1));
    }
    // Only the first scheduler pass saw 1000; later observations see a due
    // deadline, which they must leave for the separately configured scheduler.
    logical.store(1001, Ordering::Release);
    while calls.load(Ordering::Acquire) < 2 && Instant::now() < limit {
        std::thread::sleep(Duration::from_millis(1));
    }
    // A complete empty inventory must not trigger repeated journal scans.
    std::thread::sleep(Duration::from_millis(150));
    let observed = calls.load(Ordering::Acquire);
    let records = crate::journal_io::load_records(&scratch.0).unwrap();
    // Retire the actual owner before any assertion, including sensitivity failures.
    handle.stop();
    let exit = worker.join().unwrap();
    assert!(exit.shutdown.writer_released);
    assert_eq!(observed, 2, "original driver observation count");
    assert!(records.iter().all(|record| record.seq <= before));
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, before);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
fn execution_host_native_decision_pass_uses_one_sample_for_both_deadline_records() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(super::interaction::CASE), false, false)
        .unwrap();
    for identifier in ["first", "second"] {
        store
            .create_instance_ctx_on(
                &mut clock,
                "question_case",
                identifier,
                identifier,
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let before = store.journal.last_seq;
    let driver = OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap();
    let (mut owner, handle) = NativeOwner::new(
        driver,
        CountingClock(Arc::clone(&calls)),
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(10),
        10000,
    )
    .unwrap();
    let lines = owner.decision_pass();
    let samples = calls.load(Ordering::SeqCst);
    let records = crate::journal_io::load_records(&scratch.0).unwrap();
    // Retire the original owner before assertions, including a guard-removal failure.
    handle.stop();
    let exit = owner.run();
    assert!(exit.shutdown.writer_released);
    assert!(
        !lines.iter().any(|line| line.starts_with("error ")),
        "{lines:?}"
    );
    assert_eq!(samples, 1);
    // The journal loader includes sequence-zero Genesis; select this pass by sequence.
    let applied = records
        .iter()
        .filter(|record| record.seq > before)
        .collect::<Vec<_>>();
    assert_eq!(applied.len(), 2);
    for record in applied {
        assert_eq!(record.kind, fsm_core::record::RecordKind::DeadlineApplied);
        assert_eq!(record.ts, 1001);
    }
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, before + 2);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

fn scheduled_owner<C: Clock>(store: Store, clock: C) -> (NativeOwner<C>, super::super::Handle) {
    NativeOwner::new(
        OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap(),
        clock,
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_millis(86400000),
        10000,
    )
    .unwrap()
}

fn nested_invocations(path: &std::path::Path, depth: usize) -> Store {
    use fsm_core::hashes::{digest_of, machine_id};
    let mut store = Store::open(path).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    let mut child = None;
    for level in (0..=depth).rev() {
        let source = match &child {
            Some(digest) => format!(
                r#"{{"format":"fsm.machine/1","name":"level-{level}","context":[],"events":[],"effects":[],"states":[{{"name":"running","invoke":[{{"id":"next","machine":"{digest}"}}]}},{{"name":"finished","terminal":true}}],"initial":"running","transitions":[{{"from":"running","on":"$done.invoke.next","to":"finished"}}]}}"#
            ),
            None => format!(
                r#"{{"format":"fsm.machine/1","name":"level-{level}","context":[],"events":[],"effects":[],"states":[{{"name":"waiting"}},{{"name":"finished","terminal":true}}],"initial":"waiting","transitions":[],"deadlines":[{{"name":"due","from":"waiting","after":"dur(0, ms)","to":"finished"}}]}}"#
            ),
        };
        let definition = value(&source);
        child = Some(digest_of(&machine_id(&definition)).unwrap().to_string());
        store
            .define_machine_on(&mut clock, definition, false, false)
            .unwrap();
    }
    store
        .create_instance_ctx_on(
            &mut clock,
            "level-0",
            "root",
            "create-root",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

#[test]
fn autonomous_schedule_composition_reaches_quiescence_in_one_decision_without_rpc() {
    let scratch = Scratch::new();
    let store = nested_invocations(&scratch.0, 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut owner, handle) = scheduled_owner(store, CountingClock(Arc::clone(&calls)));
    let lines = owner.decision_pass();
    let samples = calls.load(Ordering::Acquire);
    let observed = Store::open_read_only(&scratch.0).unwrap();
    let status = observed.state.instances["root"].status;
    let records = observed.records.clone();
    drop(observed);
    handle.stop();
    let exit = owner.run();
    assert!(exit.shutdown.writer_released);
    assert!(
        !lines.iter().any(|line| line.starts_with("error ")),
        "{lines:?}"
    );
    assert_eq!(status, fsm_core::machine::Status::Completed);
    assert_eq!(samples, 1);
    assert!(
        records
            .iter()
            .filter(
                |record| record.kind == fsm_core::record::RecordKind::InstanceInvoked
                    || record.kind == fsm_core::record::RecordKind::InvocationReturned
            )
            .all(|record| record.ts == 1001)
    );
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
fn autonomous_schedule_decision_yields_after_eight_progressing_turns() {
    let scratch = Scratch::new();
    let store = deadline_chain(&scratch.0, 10);
    let before = store.journal.last_seq;
    let calls = Arc::new(AtomicUsize::new(0));
    let (mut owner, handle) = scheduled_owner(store, CountingClock(Arc::clone(&calls)));
    let first = owner.decision_pass();
    let first_observed = Store::open_read_only(&scratch.0).unwrap();
    let first_sequence = first_observed.journal.last_seq;
    let first_status = first_observed.state.instances["chain"].status;
    drop(first_observed);
    let mut later = Vec::new();
    for _ in 0..3 {
        later.extend(owner.decision_pass());
    }
    let observed = Store::open_read_only(&scratch.0).unwrap();
    let final_status = observed.state.instances["chain"].status;
    drop(observed);
    handle.stop();
    let exit = owner.run();
    assert!(exit.shutdown.writer_released);
    assert!(
        !first
            .iter()
            .chain(&later)
            .any(|line| line.starts_with("error ")),
        "{first:?} {later:?}"
    );
    assert_eq!(first_sequence - before, 8);
    assert_eq!(first_status, fsm_core::machine::Status::Running);
    assert_eq!(final_status, fsm_core::machine::Status::Completed);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
fn autonomous_schedule_deadline_one_tick_before_and_exact_due_use_logical_time() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(super::interaction::CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "question_case",
            "question",
            "create-question",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let before = store.journal.last_seq;
    let calls = Arc::new(AtomicUsize::new(0));
    let logical = Arc::new(AtomicI64::new(1000));
    let (mut owner, handle) = scheduled_owner(
        store,
        ObservationClock {
            logical: Arc::clone(&logical),
            calls: Arc::clone(&calls),
        },
    );
    let early = owner.decision_pass();
    let early_sequence = Store::open_read_only(&scratch.0).unwrap().journal.last_seq;
    logical.store(1001, Ordering::Release);
    let due = owner.decision_pass();
    let observed = Store::open_read_only(&scratch.0).unwrap();
    let records = observed.records.clone();
    drop(observed);
    let samples = calls.load(Ordering::Acquire);
    handle.stop();
    let exit = owner.run();
    assert!(exit.shutdown.writer_released);
    assert!(
        !early
            .iter()
            .chain(&due)
            .any(|line| line.starts_with("error ")),
        "{early:?} {due:?}"
    );
    assert_eq!(early_sequence, before);
    let applied = records
        .iter()
        .filter(|record| record.seq > before)
        .collect::<Vec<_>>();
    assert_eq!(applied.len(), 1);
    assert_eq!(
        applied[0].kind,
        fsm_core::record::RecordKind::DeadlineApplied
    );
    assert_eq!(applied[0].ts, 1001);
    assert_eq!(samples, 2);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

fn deadline_chain(path: &std::path::Path, count: usize) -> Store {
    let mut states = (0..count)
        .map(|index| format!(r#"{{"name":"stage-{index}"}}"#))
        .collect::<Vec<_>>();
    states.push(format!(r#"{{"name":"stage-{count}","terminal":true}}"#));
    let deadlines = (0..count).map(|index| format!(r#"{{"name":"due-{index}","from":"stage-{index}","after":"dur(0, ms)","to":"stage-{}"}}"#, index + 1)).collect::<Vec<_>>();
    let definition = value(&format!(
        r#"{{"format":"fsm.machine/1","name":"deadline-chain","context":[],"events":[],"effects":[],"states":[{}],"initial":"stage-0","transitions":[],"deadlines":[{}]}}"#,
        states.join(","),
        deadlines.join(",")
    ));
    let mut store = Store::open(path).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, definition, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "deadline-chain",
            "chain",
            "create-chain",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

#[test]
fn autonomous_schedule_ready_application_runs_between_bounded_executor_batches() {
    let scratch = Scratch::new();
    let store = deadline_chain(&scratch.0, 32);
    let before = store.journal.last_seq;
    let calls = Arc::new(AtomicUsize::new(0));
    let (owner, handle) = scheduled_owner(store, CountingClock(Arc::clone(&calls)));
    let session = handle.session().unwrap();
    let replies = (0..8)
        .map(|index| {
            let mut command = super::command("instance_get", r#"{"instance_id":"chain"}"#);
            command.rpc_id = fsm_core::json::Value::Num(index.to_string());
            session.submit(command).unwrap()
        })
        .collect::<Vec<_>>();
    let worker = std::thread::spawn(move || owner.run());
    let outcomes = replies
        .into_iter()
        .map(|reply| reply.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect::<Vec<_>>();
    handle.stop();
    let exit = worker.join().unwrap();
    assert!(exit.shutdown.writer_released);
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
    // No new command is submitted while the owner runs, and its one-day
    // interval cannot drive this progress; each bound offers the queued read.
    for (index, outcome) in outcomes.iter().take(4).enumerate() {
        assert_eq!(outcome.committed_seq, before + ((index + 1) * 8) as u64);
    }
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(
        reopened.state.instances["chain"].status,
        fsm_core::machine::Status::Completed
    );
    assert_eq!(reopened.journal.last_seq, before + 32);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
fn autonomous_schedule_continuously_ready_application_gets_each_bounded_turn() {
    let scratch = Scratch::new();
    let store = deadline_chain(&scratch.0, 128);
    let before = store.journal.last_seq;
    let calls = Arc::new(AtomicUsize::new(0));
    let (owner, handle) = scheduled_owner(store, CountingClock(Arc::clone(&calls)));
    let sessions = (0..4)
        .map(|_| handle.session().unwrap())
        .collect::<Vec<_>>();
    let mut replies = std::collections::VecDeque::new();
    for index in 0..32 {
        replies.push_back(
            sessions[index % 4]
                .submit(super::command("instance_get", r#"{"instance_id":"chain"}"#))
                .unwrap(),
        );
    }
    let worker = std::thread::spawn(move || owner.run());
    let mut outcomes = Vec::new();
    for index in 0..88 {
        let reply = replies.pop_front().unwrap();
        outcomes.push(reply.recv_timeout(Duration::from_secs(5)).unwrap());
        // The initial backlog covers all sixteen progressing decision passes;
        // replenishment additionally exercises admission while the owner runs.
        if index < 56 {
            // Reply delivery can precede original reservation retirement; a
            // full host may therefore legitimately refuse immediate refill.
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let reply = loop {
                match sessions[index % 4]
                    .submit(super::command("instance_get", r#"{"instance_id":"chain"}"#))
                {
                    Ok(reply) => break reply,
                    Err(super::super::AdmissionError::Busy) => {
                        assert!(std::time::Instant::now() < deadline, "refill stayed busy");
                        std::thread::yield_now();
                    }
                    Err(error) => panic!("refill refused: {error:?}"),
                }
            };
            replies.push_back(reply);
        }
    }
    assert!(replies.is_empty());
    handle.stop();
    let exit = worker.join().unwrap();
    assert!(exit.failure.is_none());
    assert!(exit.shutdown.writer_released);
    assert!(outcomes.iter().all(|outcome| outcome.result.is_ok()));
    for (index, outcome) in outcomes.iter().take(16).enumerate() {
        assert_eq!(outcome.committed_seq, before + ((index + 1) * 8) as u64);
    }
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, before + 128);
    assert_eq!(
        reopened.state.instances["chain"].status,
        fsm_core::machine::Status::Completed
    );
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}

#[test]
fn autonomous_schedule_ready_commands_yield_after_eight_without_wait_time() {
    let scratch = Scratch::new();
    let mut store = Store::open(&scratch.0).unwrap();
    let mut clock = FixedClock::new(1000, 0);
    store
        .define_machine_on(&mut clock, value(super::interaction::CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "question_case",
            "question",
            "create-question",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let before = store.journal.last_seq;
    struct AdmissionClock(bool);
    impl Clock for AdmissionClock {
        fn now_ms(&mut self) -> i64 {
            if std::mem::replace(&mut self.0, false) {
                1000
            } else {
                1001
            }
        }
    }
    let (waits, _observed_waits) = std::sync::mpsc::channel();
    let (owner, handle) = NativeOwner::new(
        OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap(),
        AdmissionClock(true),
        DiagnosticOutput::start(std::io::sink()).unwrap(),
        Duration::from_secs(86400),
        10000,
    )
    .unwrap();
    let owner = owner.with_wait_clock(ManualWaitClock {
        epoch: Instant::now(),
        elapsed: Arc::new(AtomicUsize::new(0)),
        waits,
    });
    let sessions = (0..4)
        .map(|_| handle.session().unwrap())
        .collect::<Vec<_>>();
    // Neither the wait clock nor the one-day interval can authorize this poll;
    // the eighth admitted command alone must yield to the logical due deadline.
    let replies = (0..32)
        .map(|index| {
            sessions[index % 4]
                .submit(super::command(
                    "instance_get",
                    r#"{"instance_id":"question"}"#,
                ))
                .unwrap()
        })
        .collect::<Vec<_>>();
    let worker = std::thread::spawn(move || owner.run());
    let outcomes = replies
        .into_iter()
        .map(|reply| reply.recv_timeout(Duration::from_secs(3)).unwrap())
        .collect::<Vec<_>>();
    handle.stop();
    let exit = worker.join().unwrap();
    assert!(exit.failure.is_none() && exit.shutdown.writer_released);
    for (index, outcome) in outcomes.iter().enumerate() {
        assert!(outcome.result.is_ok());
        assert_eq!(outcome.committed_seq, before + u64::from(index >= 8));
    }
    let reopened = Store::open(&scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, before + 1);
    assert_eq!(
        crate::journal_io::verify(&scratch.0).health,
        crate::journal_io::JournalHealth::Ok
    );
}
