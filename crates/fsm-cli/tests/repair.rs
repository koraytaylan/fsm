use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fsm_cli::journal_io::{JournalHealth, RepairError, classify, init, repair_truncate_torn_tail};
use fsm_core::json::Value;
use fsm_core::record::RecordKind;

fn tmp() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("fsm-rp-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

fn with_torn() -> PathBuf {
    let dir = tmp();
    let mut j = init(&dir).unwrap();
    let mut b = std::collections::BTreeMap::new();
    b.insert("instance_id".into(), Value::Str("i".into()));
    j.append(RecordKind::Annotated, Value::Obj(b)).unwrap();
    drop(j);
    let seg = dir.join("journal/seg-00000000000000000000.jsonl");
    let mut bytes = fs::read(&seg).unwrap();
    bytes.truncate(bytes.len() - 5);
    fs::write(&seg, bytes).unwrap();
    dir
}

#[test]
fn repair_torn_and_refuse_healthy() {
    let dir = with_torn();
    let orig = fs::read(dir.join("journal/seg-00000000000000000000.jsonl")).unwrap();
    match classify(&dir) {
        JournalHealth::TornTail { .. } => {}
        h => panic!("{h:?}"),
    }
    let rep = repair_truncate_torn_tail(&dir).unwrap();
    assert!(rep.quarantined.exists());
    let q = fs::read(&rep.quarantined).unwrap();
    let now = fs::read(dir.join("journal/seg-00000000000000000000.jsonl")).unwrap();
    let mut reassembled = now.clone();
    reassembled.extend_from_slice(&q);
    assert_eq!(reassembled, orig);
    assert!(matches!(classify(&dir), JournalHealth::Ok));
    assert!(matches!(
        repair_truncate_torn_tail(&dir),
        Err(RepairError::NothingToRepair)
    ));

    let healthy = tmp();
    let _j = init(&healthy).unwrap();
    drop(_j);
    assert!(matches!(
        repair_truncate_torn_tail(&healthy),
        Err(RepairError::NothingToRepair)
    ));
}
