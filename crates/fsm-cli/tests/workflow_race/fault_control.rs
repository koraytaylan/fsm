//! Actual stop observations and authenticated interruption shared by fault cases.
use super::*;

pub(super) fn abort_with_uncertainty(directory: &Directory, root: &Path, output: &Path) {
    stop_with_uncertainty(directory, root, output, "abort");
}

pub(super) fn drain_with_uncertainty(directory: &Directory, root: &Path, output: &Path) {
    stop_with_uncertainty(directory, root, output, "drain");
}

fn stop_with_uncertainty(directory: &Directory, root: &Path, output: &Path, mode: &str) {
    let entry = directory.1.as_ref().unwrap();
    let mut stop = Command::new(directory.executable())
        .env("HOME", text(entry, "home"))
        .args(["--json", "--data-dir"])
        .arg(directory.store())
        .args([
            "execute",
            "stop",
            "--mode",
            mode,
            "--timeout-ms",
            "1000",
            "--control-dir",
        ])
        .arg(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(output).unwrap())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    let mut observed_owner = Value::Null;
    loop {
        if let Ok(owner) = fsm_cli::local_control::observe(root, &directory.store(), 50)
            && owner.get("admission_closed") == Some(&Value::Bool(true))
        {
            observed_owner = owner;
        }
        if let Some(status) = stop.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(Instant::now() < until, "fault control exceeded its bound");
        std::thread::sleep(Duration::from_millis(5));
    }
    let report = parse(&fs::read(output).unwrap(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        report.get("code").and_then(Value::as_str),
        Some("exec/inflight_deferred")
    );
    let details = report.get("details").unwrap();
    if details.get("phase").is_none() {
        // The entire CLI exchange shares the caller's deadline, so a terminal
        // owner report may arrive after that exchange retires; it must leave
        // every unconfirmed transport fact unknown rather than fabricate it.
        for fact in [
            "admission_closed",
            "native_cleanup_confirmed",
            "writer_released",
        ] {
            assert_eq!(details.get(fact), Some(&Value::Null));
        }
    } else {
        assert_eq!(
            details.get("phase").and_then(Value::as_str),
            Some("uncertain")
        );
        assert_eq!(details.get("admission_closed"), Some(&Value::Bool(true)));
    }
    // Independently interrogate the surviving original incarnation: a bounded
    // transport refusal alone cannot prove that it accepted stop or closed admission.
    // Embedded shutdown may retire its endpoint as the terminal deadline
    // expires; retain a genuine in-flight observation rather than infer stop
    // acceptance from endpoint absence or the CLI's transport uncertainty.
    if let Ok(owner) = fsm_cli::local_control::observe(root, &directory.store(), 250) {
        observed_owner = owner;
    }
    assert!(matches!(
        observed_owner.get("phase").and_then(Value::as_str),
        Some("draining" | "stopping" | "uncertain")
    ));
    assert_eq!(
        observed_owner.get("admission_closed"),
        Some(&Value::Bool(true))
    );
}

pub(super) fn reconcile_original_closure(
    directory: &Directory,
    claim: &fsm_core::record::execution::Claim,
    records: &[fsm_core::record::Record],
) -> Store {
    let snapshot = Store::open_read_only(&directory.store()).unwrap();
    assert_eq!(snapshot.records, records);
    let instance = snapshot.state.instances[claim.effect().0].clone();
    let mut shutdown = fsm_execute::run::native_client::NativeShutdown::start(
        &snapshot,
        claim,
        Duration::from_secs(3),
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        let proof = shutdown.poll().unwrap();
        if proof.is_some() && shutdown.reap().unwrap() {
            break;
        }
        assert!(
            Instant::now() < until,
            "original closure reconciliation exceeded its bound"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut writer = Store::open(&directory.store()).unwrap();
    shutdown
        .settle_interrupted(&mut writer, &mut fsm_store::clock::GlobalClock)
        .unwrap();
    assert_original_interruption(writer, claim, records, instance)
}

pub(super) fn reconcile_original_closure_via_cli(
    directory: &Directory,
    claim: &fsm_core::record::execution::Claim,
    records: &[fsm_core::record::Record],
) -> Store {
    let snapshot = Store::open_read_only(&directory.store()).unwrap();
    assert_eq!(snapshot.records, records);
    let instance = snapshot.state.instances[claim.effect().0].clone();
    drop(snapshot);
    let mut settled = None;
    let mut competing_writer = Some(Store::open(&directory.store()).unwrap());
    for ordinal in 0..3 {
        let output_path = directory.0.join(format!("reconcile-stdout-{ordinal}"));
        let error_path = directory.0.join(format!("reconcile-stderr-{ordinal}"));
        let mut reconciliation = Command::new(directory.executable())
            .args(["--json", "--data-dir"])
            .arg(directory.store())
            .args([
                "execute",
                "reconcile",
                "--run-id",
                &claim.run_id().to_string(),
                "--timeout-ms",
                "8000",
            ])
            .stdin(Stdio::null())
            .stdout(fs::File::create(&output_path).unwrap())
            .stderr(fs::File::create(&error_path).unwrap())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(12);
        let status = loop {
            if let Some(status) = reconciliation.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                // Retire only the child owned by this fixture, retaining native
                // authority evidence for the supervisor's authenticated cleanup.
                let _ = reconciliation.kill();
                reconciliation.wait().unwrap();
                panic!("original-run CLI reconciliation exceeded its fixture deadline");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(fs::metadata(&output_path).unwrap().len() <= 8192);
        assert!(fs::metadata(&error_path).unwrap().len() <= 8192);
        if ordinal == 0 {
            assert!(!status.success());
            let refusal = parse(&fs::read(&error_path).unwrap(), &JsonLimits::DEFAULT).unwrap();
            assert_eq!(refusal.get("code"), Some(&Value::Str("store/lock".into())));
            assert!(fs::read(&output_path).unwrap().is_empty());
            assert_eq!(
                Store::open_read_only(&directory.store()).unwrap().records,
                records
            );
            assert_eq!(competing_writer.as_ref().unwrap().records, records);
            drop(competing_writer.take());
            record_reconciliation_transcript(ordinal, claim.run_id(), status, refusal);
            continue;
        }
        assert!(
            status.success(),
            "original-run CLI reconciliation refused: {}",
            String::from_utf8_lossy(&fs::read(&error_path).unwrap())
        );
        let response = parse(&fs::read(&output_path).unwrap(), &JsonLimits::DEFAULT).unwrap();
        assert_eq!(response.get("duplicate"), Some(&Value::Bool(ordinal > 1)));
        assert_eq!(
            response.get("execution").unwrap().get("run_id"),
            Some(&Value::Num(claim.run_id().to_string()))
        );
        assert_eq!(
            response.get("execution").unwrap().get("disposition"),
            Some(&Value::Str("interrupted".into()))
        );
        let observed = Store::open_read_only(&directory.store()).unwrap();
        if let Some(original) = &settled {
            assert_eq!(&observed.records, original);
        } else {
            assert_eq!(observed.records.len(), records.len() + 2);
            settled = Some(observed.records.clone());
        }
        record_reconciliation_transcript(ordinal, claim.run_id(), status, response);
    }
    let writer = Store::open(&directory.store()).unwrap();
    assert_original_interruption(writer, claim, records, instance)
}

fn record_reconciliation_transcript(
    ordinal: u64,
    run_id: u64,
    status: std::process::ExitStatus,
    response: Value,
) {
    let transcript = Value::Obj(BTreeMap::from([
        ("ordinal".into(), Value::Num(ordinal.to_string())),
        ("run_id".into(), Value::Num(run_id.to_string())),
        ("success".into(), Value::Bool(status.success())),
        ("response".into(), response),
    ]));
    // Only verified fixture responses reach the retained native transcript;
    // the command uses the original store and exact run with an 8000-ms bound.
    writeln!(
        std::io::stdout().lock(),
        "FSM_NATIVE_RECONCILE_TRANSCRIPT {}",
        String::from_utf8(canon_bytes(&transcript)).unwrap()
    )
    .unwrap();
}

fn assert_original_interruption(
    writer: Store,
    claim: &fsm_core::record::execution::Claim,
    records: &[fsm_core::record::Record],
    instance: fsm_core::machine::InstanceState,
) -> Store {
    assert_eq!(&writer.records[..records.len()], records);
    assert_eq!(writer.records.len(), records.len() + 2);
    assert_eq!(
        writer.records[records.len()].kind,
        RecordKind::ExecutionStopped
    );
    assert_eq!(
        writer.records[records.len() + 1].kind,
        RecordKind::ExecutionSettled
    );
    assert_eq!(writer.state.instances[claim.effect().0], instance);
    assert!(
        writer.state.instances[claim.effect().0]
            .pending
            .iter()
            .any(|effect| effect == claim.effect().1)
    );
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    writer
}

pub(super) fn live(identity: &(u32, String)) -> bool {
    let stat = match fs::read_to_string(format!("/proc/{}/stat", identity.0)) {
        Ok(stat) => stat,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return false,
        Err(error) => panic!("original process observation failed: {error}"),
    };
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .collect();
    fields[19] == identity.1 && fields[0] != "Z"
}
