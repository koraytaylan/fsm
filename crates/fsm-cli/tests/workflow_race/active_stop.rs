//! Actual live-tree shutdown through production control with quiet protocol input.
use super::fault_control::live;
use super::*;
use fsm_execute::service::ShutdownMode;
use std::os::unix::fs::MetadataExt;

pub(super) fn restart_after_stop(
    directory: &Directory,
    client: &mut Client,
    original: Option<&mut Competitor>,
    failures: &str,
) -> Competitor {
    assert!(directory.1.is_some(), "genuine native fixture required");
    let completes = failures.starts_with("active-stop-complete");
    let times_out = failures.starts_with("active-stop-timeout");
    let marker = directory.resource().join("tree-live");
    let until = Instant::now() + Duration::from_secs(8);
    let identities = loop {
        if let Ok(bytes) = fs::read_to_string(&marker) {
            let identifiers: Vec<u32> = bytes
                .split_whitespace()
                .map(|pid| pid.parse().unwrap())
                .collect();
            if identifiers.len() == 2 {
                break identifiers
                    .into_iter()
                    .map(process_identity)
                    .collect::<Vec<_>>();
            }
        }
        assert!(Instant::now() < until, "original tree never entered");
        std::thread::sleep(Duration::from_millis(5));
    };
    let marker_identity = fs::symlink_metadata(&marker).unwrap();
    let snapshot = Store::open_read_only(&directory.store()).unwrap();
    assert_eq!(snapshot.state.execution.unresolved().count(), 1);
    let instance = snapshot.state.instances["inst-run"].clone();
    let entry = directory.1.as_ref().unwrap();
    let root = original.as_ref().map_or_else(
        || PathBuf::from(text(entry, "home")).join(".cache/fsm/control"),
        |owner| owner.root.clone(),
    );
    assert!(identities.iter().all(live));
    super::upgrade::inspect(directory, "retained");
    // No protocol frames or EOF after admission: only the external control
    // transport may wake the production embedded lifecycle pump.
    assert!(client.input.is_some());
    let draining = failures.contains("drain").then(|| {
        let root = root.clone();
        let store = directory.store();
        std::thread::spawn(move || {
            fsm_cli::local_control::stop(&root, &store, ShutdownMode::Drain, 10000)
        })
    });
    if draining.is_some() {
        let until = Instant::now() + Duration::from_secs(2);
        loop {
            let owner = fsm_cli::local_control::observe(&root, &directory.store(), 100).unwrap();
            if owner.get("phase").and_then(Value::as_str) == Some("draining") {
                assert_eq!(owner.get("admission_closed"), Some(&Value::Bool(true)));
                assert!(identities.iter().all(live));
                assert_eq!(
                    Store::open_read_only(&directory.store()).unwrap().records,
                    snapshot.records
                );
                break;
            }
            assert!(Instant::now() < until, "drain never closed admission");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    let output = directory.resource().join("active-stop.json");
    let mut stop = Command::new(super::upgrade::operator(directory))
        .env("HOME", text(entry, "home"))
        .args(["--json", "--data-dir"])
        .arg(directory.store())
        .args([
            "execute",
            "stop",
            "--mode",
            if completes || times_out {
                "drain"
            } else {
                "abort"
            },
            "--timeout-ms",
            "5000",
            "--control-dir",
        ])
        .arg(&root)
        .stdin(Stdio::null())
        .stdout(fs::File::create(&output).unwrap())
        .stderr(fs::File::create(directory.resource().join("active-stop.stderr")).unwrap())
        .spawn()
        .unwrap();
    if completes {
        fs::write(
            directory.resource().join("tree-release"),
            b"original completion",
        )
        .unwrap();
    }
    let until = Instant::now() + Duration::from_secs(7);
    loop {
        if let Some(status) = stop.try_wait().unwrap() {
            assert!(status.success(), "actual healthy stop failed");
            break;
        }
        assert!(Instant::now() < until, "healthy stop exceeded its bound");
        std::thread::sleep(Duration::from_millis(5));
    }
    let report = parse(&fs::read(output).unwrap(), &JsonLimits::DEFAULT).unwrap();
    assert_eq!(report.get("phase").and_then(Value::as_str), Some("stopped"));
    for fact in [
        "admission_closed",
        "helpers_retired",
        "inventory_complete",
        "writer_released",
    ] {
        assert_eq!(report.get(fact), Some(&Value::Bool(true)));
    }
    assert_eq!(report.get("unresolved_run_ids"), Some(&value("[]")));
    assert_eq!(report.get("timed_out"), Some(&Value::Bool(false)));
    super::upgrade::record_stop(directory, &report);
    if let Some(draining) = draining {
        assert!(
            draining.is_finished(),
            "original drain transport did not retire"
        );
        assert_eq!(
            draining
                .join()
                .unwrap()
                .unwrap()
                .get("phase")
                .and_then(Value::as_str),
            Some("stopped")
        );
    }
    let process = original.map_or(&mut client.process, |owner| &mut owner.child);
    loop {
        if let Some(status) = process.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < until, "original owner did not retire");
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(identities.iter().all(|identity| !live(identity)));
    super::upgrade::inspect(directory, "drained");
    let writer = Store::open(&directory.store()).unwrap();
    assert_eq!(&writer.records[..snapshot.records.len()], snapshot.records);
    assert_eq!(
        writer.records.len(),
        snapshot.records.len() + if completes { 3 } else { 2 }
    );
    assert_eq!(
        writer.records[snapshot.records.len()].kind,
        RecordKind::ExecutionStopped
    );
    assert_eq!(
        writer.records[snapshot.records.len() + 1].kind,
        RecordKind::ExecutionSettled
    );
    if completes {
        assert_eq!(
            writer.records[snapshot.records.len() + 2].kind,
            RecordKind::EventApplied
        );
        assert_eq!(
            writer.records[snapshot.records.len() + 1]
                .body
                .get("disposition")
                .and_then(Value::as_str),
            Some("acked")
        );
        assert_ne!(writer.state.instances["inst-run"], instance);
    } else {
        assert_eq!(writer.state.instances["inst-run"], instance);
    }
    if times_out {
        assert_eq!(
            writer.records[snapshot.records.len()]
                .body
                .get("outcome")
                .unwrap()
                .get("status")
                .and_then(Value::as_str),
            Some("timeout")
        );
        assert_eq!(
            writer.records[snapshot.records.len() + 1]
                .body
                .get("disposition")
                .and_then(Value::as_str),
            Some("attempted")
        );
    }
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    assert_eq!(
        fs::read_to_string(directory.resource().join("calls")).unwrap(),
        "check_prerequisite\n"
    );
    if completes {
        assert!(!marker.try_exists().unwrap());
    } else {
        let matched = fs::symlink_metadata(&marker).unwrap();
        assert_eq!(
            (matched.dev(), matched.ino()),
            (marker_identity.dev(), marker_identity.ino())
        );
        fs::remove_file(marker).unwrap();
    }
    fs::write(directory.resource().join("tree-release"), b"successor only").unwrap();
    *client = Client::start_mode(directory, ExecutionMode::Standalone);
    drop(writer);
    start(directory, "after-active-stop")
}

fn scenario(failures: &str, mode: ExecutionMode) {
    run_scenario_mode(
        failures,
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
        mode,
    );
}

#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn standalone_abort_stops_a_live_tree_and_recovers() {
    scenario("active-stop-abort", ExecutionMode::Standalone);
}
#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn embedded_abort_stops_a_live_tree_and_recovers() {
    scenario("active-stop-abort-embedded", ExecutionMode::Embedded);
}
#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn standalone_drain_escalates_to_abort_on_a_live_tree() {
    scenario("active-stop-drain", ExecutionMode::Standalone);
}
#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn embedded_drain_escalates_to_abort_on_a_live_tree() {
    scenario("active-stop-drain-embedded", ExecutionMode::Embedded);
}

#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn standalone_drain_allows_original_completion() {
    completing_scenario(ExecutionMode::Standalone);
}
#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn embedded_drain_allows_original_completion() {
    completing_scenario(ExecutionMode::Embedded);
}
fn completing_scenario(mode: ExecutionMode) {
    let failures = if matches!(mode, ExecutionMode::Embedded) {
        "active-stop-complete-drain-embedded"
    } else {
        "active-stop-complete-drain"
    };
    run_scenario_mode(
        failures,
        "succeeded",
        &[
            "check_prerequisite",
            "check_identity",
            "check_access",
            "check_target",
            "suspend",
            "perform_work",
            "restore",
        ],
        "active",
        mode,
    );
}

#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn standalone_quiet_drain_enforces_original_handler_timeout() {
    scenario("active-stop-timeout-drain", ExecutionMode::Standalone);
}
#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn embedded_quiet_drain_enforces_original_handler_timeout() {
    scenario(
        "active-stop-timeout-drain-embedded",
        ExecutionMode::Embedded,
    );
}
