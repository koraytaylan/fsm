//! Retained readiness is scheduling metadata, never native settlement authority.

#[cfg(target_os = "linux")]
mod capacity;

use super::*;
use crate::watch::Observation;
use fsm_core::{
    json::{JsonLimits, parse},
    record::execution::AcknowledgedHandoff,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Directory(PathBuf);

impl Directory {
    fn new() -> Self {
        let root = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit task cache required"));
        assert!(!root.starts_with("/tmp"));
        let path = root.join(format!(
            "fsm-retained-readiness-{}-{}",
            std::process::id(),
            DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn retained_handoff_readiness_neither_reads_the_directory_nor_authorizes_retirement() {
    let directory = Directory::new();
    let store = Store::open(&directory.0).unwrap();
    let before = store.records.clone();
    let mut snapshot = Store::open_read_only(&directory.0).unwrap();
    // This caller-owned material deliberately has no durable acknowledgement;
    // scheduling may inspect it, but it cannot retire or publish an outcome.
    let original = AcknowledgedHandoff::from_value(
        &parse(
            include_bytes!("../../../../fsm-core/tests/fixtures/execution-handoff.json"),
            &JsonLimits::DEFAULT,
        )
        .unwrap(),
    )
    .unwrap();
    snapshot.state.execution_handoffs.install(original).unwrap();
    let table = HandlerTable::default();
    let watcher = Watcher::with_handlers(directory.0.clone(), &table);
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new_native().unwrap();
    runner
        .recover_native_owners(&snapshot, &mut Observation::default(), &mut scheduler)
        .unwrap();
    let mut driver =
        OwnedNativeExecutor::from_owned_parts(store, watcher, scheduler, runner).unwrap();
    assert!(driver.has_ready_native_work());
    // The query still works when its recorded path cannot be read; restore the
    // original directory before any writer operation or lifecycle observation.
    let moved = directory.0.with_extension("moved");
    std::fs::rename(&directory.0, &moved).unwrap();
    let ready_without_directory = driver.has_ready_native_work();
    std::fs::rename(&moved, &directory.0).unwrap();
    assert!(ready_without_directory);
    assert_eq!(driver.store_mut().unwrap().records, before);
    assert!(!driver.control().report().writer_released);
    assert_eq!(
        driver
            .runner
            .apply_native(
                driver.store.as_mut().unwrap(),
                &mut fsm_store::clock::FixedClock::new(100, 0),
                &mut driver.pipeline,
                &mut driver.scheduler,
            )
            .unwrap()
            .unwrap_err()
            .code,
        "exec/inflight_deferred"
    );
    assert!(!driver.has_ready_native_work());
    assert_eq!(driver.store_mut().unwrap().records, before);
    drop(snapshot);
    drop(driver);
    assert_eq!(Store::open(&directory.0).unwrap().records, before);
}
