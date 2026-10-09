//! Genuine marker acceptance; executed only by the protected native coordinator.

use super::*;
use fsm_execute::{
    config::Advance,
    run::{Pipeline, Runner},
    sched::Scheduler,
    service::{tick_reporting, tick_with},
    watch::Watcher,
};
use std::{
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    time::{Duration, Instant},
};

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn standalone_native_refusal_preserves_work_and_repair_starts_original_handler() {
    observe(false);
}

#[test]
#[ignore = "requires disposable native CI and exact staged process/MCP fixture"]
fn borrowed_native_refusal_preserves_work_and_repair_starts_original_handler() {
    observe(true);
}

fn observe(borrowed: bool) {
    let manifest = manifest();
    let store_path = PathBuf::from(manifest.get("store").unwrap().as_str().unwrap());
    let resource = PathBuf::from(manifest.get("resource").unwrap().as_str().unwrap());
    assert!(!resource.join("root-candidate").exists());
    assert!(!resource.join("root-release").exists());
    let mut table =
        HandlerTable::parse(&fs::read_to_string(store_path.join("handlers.json")).unwrap())
            .unwrap();
    let mut restore = table.handlers["notify"].clone();
    restore.effect = "restore".into();
    restore.on_ok = Some(Advance {
        event: "undeclared".into(),
        payload: Value::Obj(BTreeMap::new()),
        stamps: Vec::new(),
    });
    table.handlers.insert("restore".into(), restore);
    let mut clock = FixedClock::new(2000, 0);
    let mut store = Store::open(&store_path).unwrap();
    store
        .define_machine_on(
            &mut clock,
            parse(
                br#"{
      "format":"fsm.machine/1","name":"native-admission","context":[],
      "events":[{"name":"done","fields":[]}],
      "effects":[{"name":"notify","fields":[]},{"name":"restore","fields":[]}],
      "states":[{"name":"running","entry":{"emit":[{"effect":"notify"}]}},
                {"name":"compensating","entry":{"emit":[{"effect":"restore"}]}},
                {"name":"finished","terminal":true}],
      "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
    }"#,
                &JsonLimits::DEFAULT,
            )
            .unwrap(),
            false,
            false,
        )
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "native-admission",
            "original",
            "create-original",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let records = store.records.clone();
    let state = store.state.clone();
    let effect_id = store.state.instances["original"].pending[0].clone();
    drop(store);
    let mut watcher = Watcher::with_handlers(store_path.clone(), &table);
    let mut scheduler = Scheduler::new(table.clone());
    let mut runner = Runner::new_native().unwrap();
    let tick = |watcher: &mut Watcher,
                scheduler: &mut Scheduler,
                runner: &mut Runner,
                clock: &mut FixedClock| {
        if borrowed {
            let mut store = Store::open(&store_path).unwrap();
            tick_with(
                watcher,
                scheduler,
                runner,
                &mut Pipeline,
                &mut store,
                clock,
                2000,
            )
        } else {
            let outcome = tick_reporting(
                watcher,
                scheduler,
                runner,
                &mut Pipeline,
                &store_path,
                clock,
                2000,
            );
            assert!(!outcome.writer_unavailable);
            outcome.lines
        }
    };
    for _ in 0..3 {
        let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
        assert!(
            lines
                .iter()
                .any(|line| line == "error exec/contract_invalid"),
            "{lines:?}"
        );
        assert!(
            !resource.join("root-candidate").exists(),
            "incompatible later step started the first real handler"
        );
        let current = Store::open_read_only(&store_path).unwrap();
        assert_eq!(current.records, records);
        assert!(fsm_store::snapshot::store_states_eq(&state, &current.state));
        assert!(scheduler.inflight_effect(&effect_id).is_none());
    }
    table.handlers.get_mut("restore").unwrap().on_ok = None;
    scheduler = Scheduler::new(table.clone());
    watcher = Watcher::with_handlers(store_path.clone(), &table);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !resource.join("root-candidate").is_file() {
        let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
        assert!(
            Instant::now() < deadline,
            "repaired original handler never entered: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    fs::write(resource.join("root-release"), b"release").unwrap();
    loop {
        let lines = tick(&mut watcher, &mut scheduler, &mut runner, &mut clock);
        let current = Store::open_read_only(&store_path).unwrap();
        if current.state.instances["original"].status == fsm_core::machine::Status::Completed
            && current.state.execution.unresolved().count() == 0
            && runner.local_native_claims().next().is_none()
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "repaired handler did not settle and retire: {lines:?}"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        fsm_store::journal_io::verify(&store_path).health,
        fsm_store::journal_io::JournalHealth::Ok
    );
}

fn manifest() -> Value {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    let path = PathBuf::from(
        std::env::var_os("FSM_CONTRACT_NATIVE_MANIFEST").expect("Root coordinator manifest"),
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
