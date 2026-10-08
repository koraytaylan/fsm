//! SPEC Recovery: a torn final record refuses writes until explicit repair.

use fsm_core::record::execution::Claim;
use fsm_store::{journal_io::repair_truncate_torn_tail, store::Store};
use std::{fs, io::Write, path::Path};

pub(super) fn repair_original_tail(
    directory: &Path,
    writer: Store,
    claim: &Claim,
    hash: &str,
) -> Store {
    let records = writer.records.clone();
    let sequence = writer.journal.last_seq;
    let segment =
        fsm_store::journal_io::journal_dir(&writer.journal.dir).join(&writer.journal.seg_name);
    let original = fs::read(&segment).unwrap();
    assert!(original.ends_with(b"\n"));
    let last = original
        .split(|byte| *byte == b'\n')
        .rev()
        .find(|line| !line.is_empty())
        .unwrap();
    assert!(last.len() > 1);
    let partial = last[..last.len() / 2].to_vec();
    // Fault injection holds the actual writer lease: no concurrent production
    // writer can create or complete this deliberately interrupted final append.
    let mut file = fs::OpenOptions::new().append(true).open(&segment).unwrap();
    file.write_all(&partial).unwrap();
    file.sync_all().unwrap();
    drop(file);
    drop(writer);
    let refused = Store::open(directory)
        .err()
        .expect("torn tail admitted a writer");
    assert_eq!(refused.code, "store/torn_tail");
    let snapshot = Store::open_read_only(directory).unwrap();
    assert_eq!(snapshot.records, records);
    assert_eq!(snapshot.current_execution_claim_hash(claim).unwrap(), hash);
    assert_eq!(
        snapshot
            .state
            .execution
            .claim_for(claim.effect().0, claim.effect().1),
        Some(claim)
    );
    assert!(
        snapshot
            .state
            .execution
            .stopped_for(claim.effect().0, claim.effect().1)
            .is_none()
    );
    drop(snapshot);
    let damaged = fs::read(&segment).unwrap();
    assert_eq!(&damaged[..original.len()], original);
    assert_eq!(&damaged[original.len()..], partial);
    // Repair is explicit, quarantines exactly the torn bytes, and cannot grant
    // native closure or settle the still-original durable execution claim.
    let repair = repair_truncate_torn_tail(directory).unwrap();
    assert_eq!(repair.bytes, partial.len() as u64);
    assert_eq!(repair.truncated_to_seq, sequence);
    assert_eq!(fs::read(repair.quarantined).unwrap(), partial);
    assert_eq!(fs::read(&segment).unwrap(), original);
    let restored = Store::open(directory).unwrap();
    assert_eq!(restored.records, records);
    assert_eq!(restored.current_execution_claim_hash(claim).unwrap(), hash);
    restored
}
