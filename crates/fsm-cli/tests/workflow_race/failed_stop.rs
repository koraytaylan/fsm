//! Production stop refusal retains the original claim until authenticated repair.
use super::fault_control::{abort_with_uncertainty, live, reconcile_original_closure};
use super::*;
use std::os::unix::fs::MetadataExt;

pub(super) fn restart_after_failed_stop(
    directory: &Directory,
    client: &mut Client,
    original: Option<&mut Competitor>,
) -> Competitor {
    assert!(
        directory.1.is_some(),
        "genuine protected native fault fixture required"
    );
    let marker = directory.resource().join("tree-live");
    let until = Instant::now() + Duration::from_secs(8);
    let identifiers = loop {
        if let Ok(bytes) = fs::read_to_string(&marker) {
            let identifiers: Vec<u32> = bytes
                .split_whitespace()
                .filter_map(|value| value.parse().ok())
                .collect();
            if identifiers.len() == 2 {
                break identifiers;
            }
        }
        assert!(Instant::now() < until, "failed-stop tree did not enter");
        std::thread::sleep(Duration::from_millis(5));
    };
    let identities: Vec<_> = identifiers.into_iter().map(process_identity).collect();
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
    let records = snapshot.records.clone();
    request_fault(directory, b"hide", b"hidden");
    assert!(
        identities.iter().all(live),
        "fault must precede native stop"
    );
    let entry = directory.1.as_ref().unwrap();
    let root = original.as_ref().map_or_else(
        || PathBuf::from(text(entry, "home")).join(".cache/fsm/control"),
        |owner| owner.root.clone(),
    );
    abort_with_uncertainty(
        directory,
        &root,
        &directory.resource().join("failed-native-stop.json"),
    );
    // Independently reach the actual native closure entry while its original
    // protected handoff is unavailable: no mock supplies this broker refusal.
    let mut failed = fsm_execute::run::native_client::NativeShutdown::start(
        &snapshot,
        &claim,
        Duration::from_secs(3),
    )
    .unwrap();
    let until = Instant::now() + Duration::from_secs(4);
    let refusal = loop {
        match failed.poll() {
            Err(message) => break message,
            Ok(None) => {}
            Ok(Some(_)) => panic!("failed native stop fabricated authenticated closure"),
        }
        assert!(
            Instant::now() < until,
            "failed native stop exceeded its bound"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        refusal.contains("broker response"),
        "actual broker refusal required: {refusal}"
    );
    while !failed.reap().unwrap() {
        assert!(
            Instant::now() < until,
            "failed native helper did not retire"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let observed = Store::open_read_only(&directory.store()).unwrap();
    assert_eq!(observed.records, records);
    assert_eq!(observed.state.execution.unresolved().count(), 1);
    assert!(
        observed
            .state
            .execution
            .stopped_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    assert_eq!(
        fs::read_to_string(directory.resource().join("calls")).unwrap(),
        "check_prerequisite\n"
    );
    request_fault(directory, b"restore", b"restored");
    if let Some(owner) = original {
        owner.child.kill().unwrap();
        owner.child.wait().unwrap();
    } else {
        client.process.kill().unwrap();
        client.process.wait().unwrap();
    }
    let writer = reconcile_original_closure(directory, &claim, &records);
    assert!(identities.iter().all(|identity| !live(identity)));
    let matched = fs::symlink_metadata(&marker).unwrap();
    assert_eq!(
        (matched.dev(), matched.ino()),
        (marker_identity.dev(), marker_identity.ino())
    );
    fs::remove_file(marker).unwrap();
    fs::write(directory.resource().join("tree-release"), b"successor only").unwrap();
    *client = Client::start_mode(directory, ExecutionMode::Standalone);
    drop(writer);
    start(directory, "after-failed-stop")
}

fn request_fault(directory: &Directory, command: &[u8], acknowledgement: &[u8]) {
    fs::write(directory.resource().join("stop-fault-command"), command).unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if fs::read(directory.resource().join("stop-fault-acknowledgement")).unwrap()
            == acknowledgement
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "root fault coordinator did not acknowledge original handoff"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
#[ignore = "requires genuine root-provisioned native handoff refusal and repair"]
fn standalone_failed_native_stop_preserves_claim_and_recovers() {
    run_scenario_mode(
        "failed-stop",
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
#[ignore = "requires genuine root-provisioned native handoff refusal and repair"]
fn embedded_failed_native_stop_preserves_claim_and_recovers() {
    run_scenario_mode(
        "failed-stop-embedded",
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
