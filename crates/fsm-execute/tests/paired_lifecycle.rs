//! Real writer leases and reader prefixes; native inventory is empty here.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
use fsm_core::json::{JsonLimits, parse};
use fsm_execute::{
    config::HandlerTable,
    service::{ExecutorPhase, PairedNativeExecutor, ShutdownMode},
};
use fsm_store::{
    clock::{Clock, FixedClock},
    store::Store,
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let cache = PathBuf::from(std::env::var_os("TMPDIR").expect("explicit cache"));
        assert!(!cache.starts_with("/tmp"));
        let path = cache.join(format!(
            "paired-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
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
fn fixture(directory: &Directory) -> Store {
    let mut store = Store::open(&directory.0).unwrap();
    let definition=parse(br#"{"format":"fsm.machine/1","name":"quiet","context":[],"events":[],"effects":[],"states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","transitions":[],"deadlines":[{"name":"due","from":"waiting","after":"dur(1, ms)","to":"done"}]}"#, &JsonLimits::DEFAULT).unwrap();
    let mut clock = FixedClock::new(1000, 1);
    store
        .define_machine_on(&mut clock, definition, false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "quiet",
            "instance",
            "create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}
#[test]
fn empty_paired_stop_completes_while_another_actor_holds_the_writer() {
    for mode in [ShutdownMode::Drain, ShutdownMode::Abort] {
        let directory = Directory::new();
        let writer = Store::open(&directory.0).unwrap();
        let mut driver = PairedNativeExecutor::new(&directory.0, HandlerTable::default()).unwrap();
        assert!(driver.control().report().writer_released);
        let request = driver.control().stop(mode, 1000).unwrap();
        driver.poll(&mut FixedClock::new(0, 1), 0);
        let report = request.wait();
        assert_eq!(report.phase, ExecutorPhase::Stopped);
        assert!(report.writer_released && report.helpers_retired && report.inventory_complete);
        assert!(Store::open(&directory.0).is_err());
        drop(writer);
        drop(Store::open(&directory.0).unwrap());
    }
}
#[test]
fn idle_paired_observation_preserves_due_machine_deadlines_and_journal() {
    let directory = Directory::new();
    let writer = fixture(&directory);
    let records = writer.records.clone();
    let state = writer.state.clone();
    let mut driver = PairedNativeExecutor::new(&directory.0, HandlerTable::default()).unwrap();
    driver.poll(&mut FixedClock::new(10000, 1), 10000);
    assert_eq!(driver.control().report().phase, ExecutorPhase::Running);
    drop(writer);
    let cold = Store::open_read_only(&directory.0).unwrap();
    assert_eq!(cold.records, records);
    assert_eq!(cold.state.instances, state.instances);
}
#[test]
fn replaced_physical_store_cannot_confirm_empty_actor_shutdown() {
    let directory = Directory::new();
    drop(Store::open(&directory.0).unwrap());
    let mut driver = PairedNativeExecutor::new(&directory.0, HandlerTable::default()).unwrap();
    let original = directory.0.with_extension("original");
    std::fs::rename(&directory.0, &original).unwrap();
    std::fs::create_dir(&directory.0).unwrap();
    drop(Store::open(&directory.0).unwrap());
    let request = driver.control().stop(ShutdownMode::Abort, 50).unwrap();
    let lines = driver.poll(&mut FixedClock::new(0, 1), 0);
    let report = request.wait();
    // Restore the original inode before assertions, including regression failures.
    std::fs::remove_dir_all(&directory.0).unwrap();
    std::fs::rename(&original, &directory.0).unwrap();
    assert_eq!(report.phase, ExecutorPhase::Uncertain);
    assert!(!report.inventory_complete);
    assert!(
        lines
            .iter()
            .any(|line| line.contains("exec/inflight_deferred"))
    );
    driver.poll(&mut FixedClock::new(0, 1), 0);
    assert_eq!(driver.control().report().phase, ExecutorPhase::Stopped);
}

struct HeldClock {
    ready: Option<mpsc::Sender<()>>,
    release: mpsc::Receiver<()>,
}
impl Clock for HeldClock {
    fn now_ms(&mut self) -> i64 {
        if let Some(ready) = self.ready.take() {
            ready.send(()).unwrap();
            self.release.recv().unwrap();
        }
        10000
    }
}
#[test]
fn held_temporary_writer_cannot_be_reported_released_during_stop() {
    let directory = Directory::new();
    drop(fixture(&directory));
    let mut driver = PairedNativeExecutor::new(&directory.0, HandlerTable::default()).unwrap();
    let control = driver.control();
    let (ready, observed) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        driver.tick(
            &mut HeldClock {
                ready: Some(ready),
                release: wait,
            },
            10000,
        );
        driver
    });
    observed.recv_timeout(Duration::from_secs(3)).unwrap();
    assert!(Store::open(&directory.0).is_err());
    let report = control.stop(ShutdownMode::Abort, 50).unwrap().wait();
    // Release the actual worker before assertions, keeping regression failures safe.
    release.send(()).unwrap();
    let mut driver = worker.join().unwrap();
    assert_eq!(report.phase, ExecutorPhase::Uncertain);
    assert!(!report.writer_released);
    driver.poll(&mut FixedClock::new(10000, 1), 10000);
    assert_eq!(control.report().phase, ExecutorPhase::Stopped);
    drop(Store::open(&directory.0).unwrap());
}
