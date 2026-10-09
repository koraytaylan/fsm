//! Original stdio EOF stops a live tree and preserves work for a verified reopen.
use super::fault_control::live;
use super::*;
use std::os::unix::fs::MetadataExt;

pub(super) fn restart_after_eof(directory: &Directory, client: &mut Client) -> Competitor {
    assert!(directory.1.is_some(), "genuine native fixture required");
    let marker = directory.resource().join("tree-live");
    let deadline = Instant::now() + Duration::from_secs(8);
    let identities = loop {
        if let Ok(bytes) = fs::read_to_string(&marker) {
            let identifiers: Vec<u32> = bytes
                .split_whitespace()
                .map(|identifier| identifier.parse().unwrap())
                .collect();
            if identifiers.len() == 2 {
                break identifiers
                    .into_iter()
                    .map(process_identity)
                    .collect::<Vec<_>>();
            }
        }
        assert!(Instant::now() < deadline, "original tree never entered");
        std::thread::sleep(Duration::from_millis(5));
    };
    let marker_identity = fs::symlink_metadata(&marker).unwrap();
    let snapshot = Store::open_read_only(&directory.store()).unwrap();
    assert_eq!(snapshot.state.execution.unresolved().count(), 1);
    let instance = snapshot.state.instances["inst-run"].clone();
    assert!(!instance.pending.is_empty());
    assert!(identities.iter().all(live));
    assert!(client.process.try_wait().unwrap().is_none());
    assert!(matches!(Store::open(&directory.store()), Err(error) if error.code == "store/lock"));
    // No stop command, signal, release marker or further protocol request;
    // the configured 30-second handler timeout cannot satisfy this EOF bound.
    client.finish();
    assert!(identities.iter().all(|identity| !live(identity)));
    let writer = Store::open(&directory.store()).unwrap();
    assert_eq!(&writer.records[..snapshot.records.len()], snapshot.records);
    assert_eq!(writer.records.len(), snapshot.records.len() + 2);
    let stopped = &writer.records[snapshot.records.len()];
    let settled = &writer.records[snapshot.records.len() + 1];
    assert_eq!(stopped.kind, RecordKind::ExecutionStopped);
    assert_eq!(settled.kind, RecordKind::ExecutionSettled);
    assert_eq!(
        stopped
            .body
            .get("outcome")
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("interrupted")
    );
    assert_eq!(
        settled.body.get("disposition").and_then(Value::as_str),
        Some("interrupted")
    );
    assert_eq!(writer.state.execution.unresolved().count(), 0);
    assert_eq!(writer.state.instances["inst-run"], instance);
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
    start(directory, "after-stdio-eof")
}

#[test]
#[ignore = "requires registered native authority and independent tree observer; task 9001"]
fn eof_stops_live_tree_and_recovers() {
    let calls = std::iter::once("check_prerequisite")
        .chain(OPERATIONS)
        .collect::<Vec<_>>();
    run_scenario_mode(
        "active-stop-eof-embedded",
        "succeeded",
        &calls,
        "active",
        ExecutionMode::Embedded,
    );
}
