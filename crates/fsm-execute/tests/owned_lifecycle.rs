//! Downstream proof of control independence and actual writer release.
#![cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]

use fsm_execute::{
    config::HandlerTable,
    service::{ExecutorPhase, OwnedNativeExecutor, ShutdownMode},
};
use fsm_store::{clock::FixedClock, store::Store};
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
        let directory = root.join(format!(
            "fsm-owned-lifecycle-{}-{}",
            std::process::id(),
            DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&directory).unwrap();
        Self(directory)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn empty_shutdown_releases_the_actual_writer_before_reporting_stopped() {
    for mode in [ShutdownMode::Drain, ShutdownMode::Abort] {
        let directory = Directory::new();
        let store = Store::open(&directory.0).unwrap();
        let mut executor = OwnedNativeExecutor::new(store, HandlerTable::default()).unwrap();
        let control = executor.control();
        let Err(error) = Store::open(&directory.0) else {
            panic!("owned writer must hold its lock")
        };
        assert_eq!(error.code, "store/lock");
        let request = control.stop(mode, 10000).unwrap();
        assert!(request.poll().admission_closed);
        assert!(!request.poll().writer_released);
        assert!(executor.poll(&mut FixedClock::new(0, 1), 0).is_empty());
        let report = request.wait();
        assert_eq!(report.phase, ExecutorPhase::Stopped);
        assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
        assert!(report.unresolved_run_ids.is_empty());
        assert_eq!(report.unclaimed_reservations, Some(0));
        assert!(executor.store_mut().is_none());
        drop(Store::open(&directory.0).unwrap());
    }
}

#[test]
fn idle_worker_cannot_block_a_control_deadline_or_release_its_writer() {
    let directory = Directory::new();
    let executor =
        OwnedNativeExecutor::new(Store::open(&directory.0).unwrap(), HandlerTable::default())
            .unwrap();
    let request = executor.control().stop(ShutdownMode::Abort, 1).unwrap();
    let report = request.wait();
    assert_eq!(report.phase, ExecutorPhase::Uncertain);
    assert!(!report.writer_released && !report.inventory_complete);
    let Err(error) = Store::open(&directory.0) else {
        panic!("unobserved writer must remain held")
    };
    assert_eq!(error.code, "store/lock");
    drop(executor);
    // Drop releases ordinary resources but cannot publish guaranteed Stopped.
    assert_eq!(request.poll().phase, ExecutorPhase::Uncertain);
}
