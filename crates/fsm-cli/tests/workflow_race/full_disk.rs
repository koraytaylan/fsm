//! Actual ENOSPC through production stop, with durable uncertainty and recovery.
use super::*;
use fsm_core::expr::eval::Val;
use fsm_store::store::VerifiedClosure;
use std::os::unix::fs::MetadataExt;

pub(super) fn restart_after_full_disk(
    directory: &Directory,
    client: &mut Client,
    original: Option<&mut Competitor>,
) -> Competitor {
    assert!(
        directory.1.is_some(),
        "bounded mounted native fixture required"
    );
    let marker = directory.resource().join("tree-live");
    let until = Instant::now() + Duration::from_secs(8);
    let identifiers = loop {
        if let Ok(bytes) = fs::read_to_string(&marker) {
            let ids: Vec<u32> = bytes
                .split_whitespace()
                .filter_map(|id| id.parse().ok())
                .collect();
            if ids.len() == 2 {
                break ids;
            }
        }
        assert!(Instant::now() < until, "full-disk tree did not enter");
        std::thread::sleep(Duration::from_millis(5));
    };
    let identities: Vec<_> = identifiers.into_iter().map(process_identity).collect();
    let marker_identity = fs::symlink_metadata(&marker).unwrap();
    align_journal(directory, client, original.is_some());
    let snapshot = Store::open_read_only(&directory.store()).unwrap();
    assert_eq!(snapshot.state.execution.unresolved().count(), 1);
    let claim = snapshot
        .state
        .execution
        .unresolved()
        .next()
        .unwrap()
        .0
        .clone();
    let hash = snapshot.current_execution_claim_hash(&claim).unwrap();
    let records = snapshot.records.clone();
    assert_eq!(snapshot.journal.seg_bytes % 4096, 0);
    drop(snapshot);
    let filler = directory.store().join("owned-enospc-filler");
    fill_owned_volume(&filler);
    assert!(identities.iter().all(live));
    let entry = directory.1.as_ref().unwrap();
    let root = original.as_ref().map_or_else(
        || PathBuf::from(text(entry, "home")).join(".cache/fsm/control"),
        |owner| owner.root.clone(),
    );
    let output = directory.resource().join("full-disk-stop.json");
    let mut stop = Command::new(directory.executable())
        .env("HOME", text(entry, "home"))
        .args(["--json", "--data-dir"])
        .arg(directory.store())
        .args([
            "execute",
            "stop",
            "--mode",
            "abort",
            "--timeout-ms",
            "1000",
            "--control-dir",
        ])
        .arg(&root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&output).unwrap())
        .spawn()
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = stop.try_wait().unwrap() {
            assert!(!status.success());
            break;
        }
        assert!(
            Instant::now() < until,
            "full-disk control exceeded its bound"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let report = parse(&fs::read(&output).unwrap(), &JsonLimits::DEFAULT).unwrap();
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
    let owner = fsm_cli::local_control::observe(&root, &directory.store(), 1000).unwrap();
    assert_eq!(
        owner.get("phase").and_then(Value::as_str),
        Some("uncertain")
    );
    assert_eq!(owner.get("admission_closed"), Some(&Value::Bool(true)));
    let domain = claim.domain().to_value();
    let receipt = PathBuf::from("/var/lib/fsm-containment")
        .join(text(&domain, "namespace"))
        .join(format!(
            "authority-{}",
            domain.get("generation").unwrap().as_num().unwrap()
        ))
        .join(format!(
            "closure-{}-{}.json",
            domain.get("allocation").unwrap().as_num().unwrap(),
            claim.run_id()
        ));
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        let observed = Store::open_read_only(&directory.store()).unwrap();
        assert_eq!(
            observed.records, records,
            "ENOSPC must preserve the durable prefix"
        );
        assert_eq!(observed.state.execution.unresolved().count(), 1);
        assert_eq!(
            fs::read_to_string(directory.resource().join("calls")).unwrap(),
            "check_prerequisite\n"
        );
        if let Ok(proof) = VerifiedClosure::read(&receipt) {
            assert!(proof.matches_claim(&claim, &hash));
            proof.check_store(&directory.store()).unwrap();
            assert!(identities.iter().all(|identity| !live(identity)));
            break;
        }
        assert!(
            Instant::now() < until,
            "native closure did not complete independently of ENOSPC"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    // Retire only the original fixture-owned process, before freeing space;
    // its unresolved claim requires authenticated operator reconciliation.
    if let Some(owner) = original {
        owner.child.kill().unwrap();
        owner.child.wait().unwrap();
    } else {
        client.process.kill().unwrap();
        client.process.wait().unwrap();
    }
    fs::remove_file(filler).unwrap();
    let writer = reconcile_original_closure(directory, &claim, &records);
    let matched = fs::symlink_metadata(&marker).unwrap();
    assert_eq!(
        (matched.dev(), matched.ino()),
        (marker_identity.dev(), marker_identity.ino())
    );
    fs::remove_file(marker).unwrap();
    fs::write(directory.resource().join("tree-release"), b"successor only").unwrap();
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    *client = Client::start_mode(directory, ExecutionMode::Standalone);
    drop(writer);
    start(directory, "after-full-disk")
}

fn reconcile_original_closure(
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

fn align_journal(directory: &Directory, client: &mut Client, standalone: bool) {
    // Valid ready instances have no effects; their ordinary context overrides
    // align the segment so stop cannot succeed inside an already allocated page.
    let mut padding = 1;
    for index in 0..8 {
        let before = Store::open_read_only(&directory.store())
            .unwrap()
            .journal
            .seg_bytes;
        let request = format!("disk-{index:02}");
        if standalone {
            let until = Instant::now() + Duration::from_secs(3);
            let mut writer = loop {
                match Store::open(&directory.store()) {
                    Ok(writer) => break writer,
                    Err(error) if error.code == "store/lock" => {
                        assert!(Instant::now() < until);
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("alignment writer: {error:?}"),
                }
            };
            writer
                .create_instance_ctx(
                    "discovered_workflow",
                    &format!("inst-{request}"),
                    &request,
                    None,
                    &BTreeMap::from([("resource".into(), Val::Str("x".repeat(padding)))]),
                    &[],
                )
                .unwrap();
        } else {
            client.call(
                "instance_create",
                object([
                    ("machine", string("discovered_workflow")),
                    ("request_id", string(&request)),
                    (
                        "context",
                        object([("resource", string(&"x".repeat(padding)))]),
                    ),
                ]),
            );
        }
        let after = Store::open_read_only(&directory.store())
            .unwrap()
            .journal
            .seg_bytes;
        if after.is_multiple_of(4096) {
            return;
        }
        let overhead = after - before - padding as u64;
        padding = ((4096 - (after + overhead) % 4096) % 4096) as usize;
    }
    panic!("valid context records did not align the full-disk journal");
}

fn fill_owned_volume(path: &Path) {
    let mut filler = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .unwrap();
    for _ in 0..=512 {
        match filler.write_all(&[0; 4096]) {
            Ok(()) => {}
            Err(error) => {
                assert_eq!(error.raw_os_error(), Some(28), "actual ENOSPC required");
                return;
            }
        }
    }
    panic!("full-disk fixture exceeded its two-MiB volume cap");
}

fn live(identity: &(u32, String)) -> bool {
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

#[test]
#[ignore = "requires root-provisioned two-MiB noswap store and genuine native tree"]
fn standalone_full_disk_stop_preserves_claim_and_recovers() {
    run_scenario_mode(
        "full-disk",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Standalone,
    );
}

#[test]
#[ignore = "requires root-provisioned two-MiB noswap store and genuine native tree"]
fn embedded_full_disk_stop_preserves_claim_and_recovers() {
    run_scenario_mode(
        "full-disk-embedded",
        "succeeded",
        &[
            "check_prerequisite",
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        ExecutionMode::Embedded,
    );
}
