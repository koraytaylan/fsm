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
