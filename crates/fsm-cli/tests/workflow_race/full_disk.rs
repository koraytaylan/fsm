//! Actual ENOSPC through production stop, with durable uncertainty and recovery.
use super::fault_control::{abort_with_uncertainty, live, reconcile_original_closure};
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
    abort_with_uncertainty(
        directory,
        &root,
        &directory.resource().join("full-disk-stop.json"),
    );
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
