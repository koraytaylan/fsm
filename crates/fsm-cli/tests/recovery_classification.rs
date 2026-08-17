use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fsm_cli::journal_io::{Journal, JournalHealth, classify, init, verify};
use fsm_core::json::Value;
use fsm_core::record::RecordKind;

fn tmp() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("fsm-rec-{n}"));
    fs::create_dir_all(&p).unwrap();
    p
}

fn clean_journal() -> PathBuf {
    let dir = tmp();
    let _j = init(&dir).unwrap();
    dir
}

#[test]
fn clean_ok() {
    let dir = clean_journal();
    assert!(matches!(classify(&dir), JournalHealth::Ok));
    let v = verify(&dir);
    assert!(matches!(v.health, JournalHealth::Ok));
    assert!(v.records >= 1);
}

#[test]
fn torn_tail() {
    let dir = clean_journal();
    let seg = dir.join("journal/seg-00000000000000000000.jsonl");
    let mut bytes = fs::read(&seg).unwrap();
    bytes.truncate(bytes.len() - 3);
    fs::write(&seg, &bytes).unwrap();
    match classify(&dir) {
        JournalHealth::TornTail { .. } => {}
        h => panic!("{h:?}"),
    }
    assert!(
        classify(&dir)
            .message()
            .contains("fsm repair --truncate-torn-tail")
    );
}

#[test]
fn interior_and_non_canonical() {
    let dir = clean_journal();
    let seg = dir.join("journal/seg-00000000000000000000.jsonl");
    let mut bytes = fs::read(&seg).unwrap();
    // insert space after first `{` of first record — interior if more records follow
    if let Some(pos) = bytes.iter().position(|&b| b == b'{') {
        bytes.insert(pos + 1, b' ');
    }
    fs::write(&seg, &bytes).unwrap();
    let h = classify(&dir);
    match h {
        JournalHealth::NonCanonical { .. }
        | JournalHealth::TornTail { .. }
        | JournalHealth::ChainBroken { .. } => {}
        other => panic!("{other:?}"),
    }
    if matches!(h, JournalHealth::ChainBroken { .. }) {
        assert!(!h.message().contains("truncate-torn-tail"));
        assert!(h.message().contains("unverifiable"));
    }
}

#[test]
fn verify_readonly() {
    let dir = clean_journal();
    let before = fs::read(dir.join("journal/seg-00000000000000000000.jsonl")).unwrap();
    let _ = verify(&dir);
    let after = fs::read(dir.join("journal/seg-00000000000000000000.jsonl")).unwrap();
    assert_eq!(before, after);
}
