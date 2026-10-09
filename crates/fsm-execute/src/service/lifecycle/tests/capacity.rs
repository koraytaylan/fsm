//! Durable pending-effect observations at the original worker reservation ceiling.
use super::super::*;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::clock::FixedClock;
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    time::{Duration, Instant},
};

const MACHINE: &[u8] = br#"{
 "format":"fsm.machine/1","name":"async_completion","context":[],
 "events":[{"name":"done","fields":[]}],"effects":[{"name":"notify","fields":[]}],
 "states":[{"name":"running","entry":{"emit":[{"effect":"notify","args":{}}]}},
           {"name":"finished","terminal":true}],
 "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
}"#;

#[test]
#[ignore = "requires disposable native CI and protected real handler manifest"]
fn completion_capacity_keeps_effect_pending_until_original_reservations_release() {
    let manifest = manifest();
    let path = PathBuf::from(field(&manifest, "store"));
    let resource = PathBuf::from(field(&manifest, "resource"));
    let mut store = Store::open(&path).unwrap();
    let mut clock = FixedClock::new(2000, 0);
    store
        .define_machine_on(
            &mut clock,
            parse(MACHINE, &JsonLimits::DEFAULT).unwrap(),
            false,
            false,
        )
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "async_completion",
            "held",
            "create-held",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let handlers =
        HandlerTable::parse(&fs::read_to_string(path.join("handlers.json")).unwrap()).unwrap();
    let mut driver = OwnedNativeExecutor::new(store, handlers).unwrap();
    driver.enable_worker_polling();
    let budget = driver.workers.as_ref().unwrap().clone();
    let reservations = crate::run::native_client::worker::exhaust_budget(&budget);
    let records = driver.store_mut().unwrap().records.clone();
    for _ in 0..10 {
        let _ = driver.tick(&mut clock, 2000);
        assert_eq!(driver.store_mut().unwrap().records, records);
        assert_eq!(
            driver.store_mut().unwrap().state.instances["held"]
                .pending
                .len(),
            1
        );
        assert_eq!(
            driver
                .store_mut()
                .unwrap()
                .state
                .execution
                .unresolved()
                .count(),
            0
        );
        assert_eq!(budget.reserved(), 128);
        assert_eq!(budget.charged_bytes(), 2 * 1024 * 1024 * 1024);
        assert!(!resource.join("root-candidate").exists());
    }
    drop(reservations);
    assert_eq!(budget.reserved(), 0);
    let deadline = Instant::now() + Duration::from_secs(12);
    while !resource.join("root-candidate").is_file() {
        let _ = driver.tick(&mut clock, 2000);
        assert!(
            Instant::now() < deadline,
            "queued effect did not resume after capacity returned"
        );
        assert!(budget.reserved() <= 128);
        assert!(budget.charged_bytes() <= 2 * 1024 * 1024 * 1024);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        driver
            .store_mut()
            .unwrap()
            .state
            .execution
            .unresolved()
            .count(),
        1
    );
    assert_eq!(
        driver.store_mut().unwrap().state.instances["held"]
            .pending
            .len(),
        1
    );
    let control = driver.control();
    let request = control.stop(ShutdownMode::Abort, 10000).unwrap();
    let deadline = Instant::now() + Duration::from_secs(12);
    while control.report().phase != ExecutorPhase::Stopped {
        let _ = driver.poll(&mut clock, 2000);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    let report = request.wait();
    assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
    assert_eq!(budget.reserved(), 0);
    assert!(!resource.join("root-release").exists());
    let verification = fsm_store::journal_io::verify(&path);
    assert_eq!(
        verification.health,
        fsm_store::journal_io::JournalHealth::Ok
    );
    let reopened = Store::open(&path).unwrap();
    assert_eq!(verification.records, reopened.records.len() as u64);
    assert_eq!(reopened.state.execution.unresolved().count(), 0);
    for kind in [
        fsm_core::record::RecordKind::ExecutionClaimed,
        fsm_core::record::RecordKind::ExecutionStopped,
        fsm_core::record::RecordKind::ExecutionSettled,
    ] {
        assert_eq!(
            reopened
                .records
                .iter()
                .filter(|record| record.kind == kind)
                .count(),
            1
        );
    }
}

fn field<'a>(manifest: &'a Value, name: &str) -> &'a str {
    manifest
        .get(name)
        .and_then(Value::as_str)
        .expect("protected fixture field")
}

fn manifest() -> Value {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let path = PathBuf::from(
        std::env::var_os("FSM_COMPLETION_NATIVE_MANIFEST").expect("Root coordinator manifest"),
    );
    assert!(path.is_absolute());
    for parent in path.ancestors().skip(1) {
        let metadata = fs::symlink_metadata(parent).unwrap();
        assert!(metadata.is_dir());
        assert_eq!(metadata.uid(), 0);
        assert_eq!(metadata.mode() & 0o022, 0);
    }
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(0o400000)
        .open(&path)
        .unwrap();
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 65_536);
    assert_eq!(
        (metadata.uid(), metadata.mode() & 0o7777, metadata.nlink()),
        (0, 0o444, 1)
    );
    let mut encoded = Vec::new();
    Read::by_ref(&mut file)
        .take(65_537)
        .read_to_end(&mut encoded)
        .unwrap();
    assert!(encoded.len() <= 65_536);
    let current = fs::symlink_metadata(path).unwrap();
    assert_eq!(
        (metadata.dev(), metadata.ino()),
        (current.dev(), current.ino())
    );
    parse(&encoded, &JsonLimits::DEFAULT).unwrap()
}
