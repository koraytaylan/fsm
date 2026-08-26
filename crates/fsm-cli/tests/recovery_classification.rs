use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use fsm_cli::journal_io::{JournalHealth, classify, init, verify};

/// Per-process counter. Tests in one binary run concurrently, and a timestamp
/// alone can collide between two threads building a path together.
static TMP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A scratch directory that removes itself.
///
/// Every temp directory a test makes has to be given back: a suite that
/// leaks one per run exhausts a long-lived machine's tmpfs inodes long
/// before it exhausts its bytes, and the failure looks like a broken
/// toolchain rather than a leaky test.
struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::ffi::OsStr> for Scratch {
    fn as_ref(&self) -> &std::ffi::OsStr {
        self.0.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tmp() -> Scratch {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let i = TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("fsm-rec-{pid}-{n}-{i}"));
    fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

fn clean_journal() -> Scratch {
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
