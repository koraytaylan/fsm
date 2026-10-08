//! Actual drain expiry keeps pending work behind authenticated native closure.
use super::fault_control::{drain_with_uncertainty, live, reconcile_original_closure};
use super::*;
use std::os::unix::fs::MetadataExt;

pub(super) fn restart_after_expiry(
    directory: &Directory,
    client: &mut Client,
    original: Option<&mut Competitor>,
) -> Competitor {
    assert!(directory.1.is_some(), "genuine native fixture required");
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
        assert!(Instant::now() < until, "original drain tree never entered");
        std::thread::sleep(Duration::from_millis(5));
    };
    let marker_identity = fs::symlink_metadata(&marker).unwrap();
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
    let instance = snapshot.state.instances["inst-run"].clone();
    let entry = directory.1.as_ref().unwrap();
    let root = original.as_ref().map_or_else(
        || PathBuf::from(text(entry, "home")).join(".cache/fsm/control"),
        |owner| owner.root.clone(),
    );
    assert!(identities.iter().all(live));
    assert!(client.input.is_some());
    // The original handler timeout is thirty seconds, so closure within this
    // separate eight-second bound must follow drain expiry or owner retirement.
    let until = Instant::now() + Duration::from_secs(8);
    drain_with_uncertainty(
        directory,
        &root,
        &directory.resource().join("expired-drain.json"),
    );
    let process = original.map_or(&mut client.process, |owner| &mut owner.child);
    loop {
        if let Some(status) = process.try_wait().unwrap() {
            assert_eq!(
                status.code(),
                Some(1),
                "expired owner must report uncertainty"
            );
            break;
        }
        assert!(Instant::now() < until, "expired owner did not retire");
        std::thread::sleep(Duration::from_millis(5));
    }
    while identities.iter().any(live) {
        assert!(
            Instant::now() < until,
            "expired drain left its original tree live"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let observed = Store::open_read_only(&directory.store()).unwrap();
    assert_eq!(
        &observed.records[..snapshot.records.len()],
        snapshot.records
    );
    assert_eq!(observed.state.instances["inst-run"], instance);
    let writer = if observed.state.execution.unresolved().count() == 1 {
        assert_eq!(observed.records, snapshot.records);
        reconcile_original_closure(directory, &claim, &snapshot.records)
    } else {
        Store::open(&directory.store()).unwrap()
    };
    assert_eq!(writer.records.len(), snapshot.records.len() + 2);
    assert_eq!(
        writer.records[snapshot.records.len()].kind,
        RecordKind::ExecutionStopped
    );
    assert_eq!(
        writer.records[snapshot.records.len() + 1].kind,
        RecordKind::ExecutionSettled
    );
    assert_eq!(
        writer.records[snapshot.records.len() + 1]
            .body
            .get("disposition")
            .and_then(Value::as_str),
        Some("interrupted")
    );
    assert_eq!(writer.state.instances["inst-run"], instance);
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    assert_eq!(
        fs::read_to_string(directory.resource().join("calls")).unwrap(),
        "check_prerequisite\n"
    );
    let matched = fs::symlink_metadata(&marker).unwrap();
    assert_eq!(
        (matched.dev(), matched.ino()),
        (marker_identity.dev(), marker_identity.ino())
    );
    fs::remove_file(marker).unwrap();
    fs::write(directory.resource().join("tree-release"), b"successor only").unwrap();
    *client = Client::start_mode(directory, ExecutionMode::Standalone);
    drop(writer);
    start(directory, "after-expired-drain")
}

fn scenario(mode: ExecutionMode) {
    let failures = if matches!(mode, ExecutionMode::Embedded) {
        "expired-drain-embedded"
    } else {
        "expired-drain"
    };
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
fn standalone_expired_drain_preserves_pending_and_recovers() {
    scenario(ExecutionMode::Standalone);
}
#[test]
#[ignore = "requires genuine root-provisioned native shutdown"]
fn embedded_expired_drain_preserves_pending_and_recovers() {
    scenario(ExecutionMode::Embedded);
}
