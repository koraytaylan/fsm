//! The public driver is observed with genuine barrier-held native handlers.
//! Ignored cases require the protected disposable-CI coordinator, not local setup.

#[cfg(target_os = "linux")]
mod native {
    use std::{
        collections::BTreeMap,
        fs,
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
        path::PathBuf,
        time::{Duration, Instant},
    };

    use fsm_core::json::{JsonLimits, Value, parse};
    use fsm_execute::{
        config::HandlerTable,
        run::native_client::{NativeExecution, NativeRun},
        service::{ExecutorPhase, OwnedNativeExecutor, ShutdownMode},
    };
    use fsm_store::{clock::FixedClock, store::Store};

    const MACHINE: &[u8] = br#"{
     "format":"fsm.machine/1","name":"async_completion","context":[],
     "events":[{"name":"done","fields":[]}],"effects":[{"name":"notify","fields":[]}],
     "states":[{"name":"running","entry":{"emit":[{"effect":"notify","args":{}}]}},
               {"name":"finished","terminal":true}],
     "initial":"running","transitions":[{"from":"running","on":"done","to":"finished"}]
    }"#;

    #[test]
    #[ignore = "requires disposable native CI and exact staged process fixture"]
    fn async_completion_process_dispatch_poll_and_stop_do_not_wait_for_release() {
        observe("process", CompletionMode::AbortHeld);
    }

    #[test]
    #[ignore = "requires disposable native CI and exact staged MCP fixture"]
    fn async_completion_mcp_dispatch_poll_and_stop_do_not_wait_for_release() {
        observe("mcp", CompletionMode::AbortHeld);
    }

    #[test]
    #[ignore = "requires disposable native CI and exact staged process fixture"]
    fn async_completion_process_repeated_polling_settles_once() {
        observe("process", CompletionMode::Release);
    }

    #[test]
    #[ignore = "requires disposable native CI and exact staged MCP fixture"]
    fn async_completion_mcp_repeated_polling_settles_once() {
        observe("mcp", CompletionMode::Release);
    }

    enum CompletionMode {
        AbortHeld,
        Release,
    }

    fn observe(kind: &str, mode: CompletionMode) {
        let manifest = manifest();
        assert_eq!(field(&manifest, "kind"), kind);
        let store_path = PathBuf::from(field(&manifest, "store"));
        let resource = PathBuf::from(field(&manifest, "resource"));
        assert!(!resource.join("root-release").exists());
        let mut store = Store::open(&store_path).unwrap();
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
            HandlerTable::parse(&fs::read_to_string(store_path.join("handlers.json")).unwrap())
                .unwrap();
        let mut driver = OwnedNativeExecutor::new(store, handlers).unwrap();
        driver.enable_worker_polling();
        let control = driver.control();
        let deadline = Instant::now() + Duration::from_secs(12);
        while !resource.join("root-candidate").is_file() {
            let entered = Instant::now();
            let _ = driver.tick(&mut clock, 2000);
            assert!(
                entered.elapsed() < Duration::from_secs(2),
                "dispatch waited on a held handler"
            );
            assert!(!resource.join("root-release").exists());
            assert!(
                Instant::now() < deadline,
                "real handler did not reach its barrier"
            );
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
        let prefix = driver.store_mut().unwrap().journal.last_seq;
        let original = driver
            .store_mut()
            .unwrap()
            .state
            .execution
            .unresolved()
            .next()
            .unwrap()
            .0
            .clone();
        let original_hash = driver
            .store_mut()
            .unwrap()
            .current_execution_claim_hash(&original)
            .unwrap();

        // Invoke the public completion boundary on the same writer owner while
        // the actual process/MCP conversation is still held outside that owner.
        for _ in 0..3 {
            let entered = Instant::now();
            let _ = driver.poll(&mut clock, 2000);
            assert!(
                entered.elapsed() < Duration::from_secs(2),
                "completion polling waited on release"
            );
            assert!(!resource.join("root-release").exists());
            assert_eq!(driver.store_mut().unwrap().journal.last_seq, prefix);
        }
        if matches!(mode, CompletionMode::Release) {
            for role in ["grandchild", "child", "root"] {
                fs::write(resource.join(format!("{role}-release")), b"release").unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(12);
            while driver.store_mut().unwrap().state.instances["held"].status
                != fsm_core::machine::Status::Completed
            {
                let _ = driver.tick(&mut clock, 2000);
                assert!(Instant::now() < deadline, "completion did not settle");
                std::thread::sleep(Duration::from_millis(5));
            }
            let settled = driver.store_mut().unwrap().journal.last_seq;
            for _ in 0..3 {
                let _ = driver.tick(&mut clock, 2000);
                let _ = driver.poll(&mut clock, 2000);
                assert_eq!(driver.store_mut().unwrap().journal.last_seq, settled);
            }
        }
        let entered = Instant::now();
        let request = control.stop(ShutdownMode::Abort, 10000).unwrap();
        assert!(
            entered.elapsed() < Duration::from_secs(2),
            "stop request waited on release"
        );
        assert!(control.report().admission_closed);
        assert_eq!(
            resource.join("root-release").exists(),
            matches!(mode, CompletionMode::Release)
        );
        let deadline = Instant::now() + Duration::from_secs(12);
        while control.report().phase != ExecutorPhase::Stopped {
            let entered = Instant::now();
            let _ = driver.poll(&mut clock, 2000);
            assert!(
                entered.elapsed() < Duration::from_secs(2),
                "shutdown polling waited on the handler"
            );
            assert!(
                Instant::now() < deadline,
                "native abort did not retire the original attempt"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let report = request.wait();
        assert!(report.inventory_complete && report.helpers_retired && report.writer_released);
        assert!(driver.store_mut().is_none());
        assert_eq!(
            resource.join("root-release").exists(),
            matches!(mode, CompletionMode::Release)
        );
        let verified = fsm_store::journal_io::verify(&store_path);
        assert_eq!(verified.health, fsm_store::journal_io::JournalHealth::Ok);
        let mut reopened = Store::open(&store_path).unwrap();
        assert_eq!(verified.records, reopened.records.len() as u64);
        assert_eq!(reopened.state.execution.unresolved().count(), 0);
        if matches!(mode, CompletionMode::Release) {
            assert!(reopened.state.instances["held"].pending.is_empty());
            assert_eq!(
                reopened.state.instances["held"].status,
                fsm_core::machine::Status::Completed
            );
            let mut recovered =
                NativeRun::recover(&original, &original_hash, Duration::from_secs(10)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(12);
            let completion = loop {
                if let Some(completion) = recovered.poll().unwrap() {
                    break completion;
                }
                assert!(
                    Instant::now() < deadline,
                    "original completion recovery timed out"
                );
                std::thread::sleep(Duration::from_millis(5));
            };
            assert!(recovered.reap().unwrap());
            let mut retained =
                NativeExecution::from_completion(&original, &original_hash, completion).unwrap();
            let records = reopened.records.clone();
            let mut reader = Store::open_read_only(&store_path).unwrap();
            assert_eq!(
                retained.settle(&mut reader, &mut clock).unwrap_err().code,
                "exec/mode"
            );
            assert!(retained.progress().retained && retained.completion().is_some());
            assert_eq!(reader.records, records);
            for _ in 0..3 {
                retained.settle(&mut reopened, &mut clock).unwrap();
                assert_eq!(reopened.records, records);
                assert!(!retained.progress().retained);
            }
        }
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
}
