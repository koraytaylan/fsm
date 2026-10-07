//! A native decision pass shares one logical sample across durable operations.

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
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
