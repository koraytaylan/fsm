use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use fsm_cli::journal_io::{JournalHealth, RepairError, classify, init, repair_truncate_torn_tail};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;

// A CLI child can briefly inherit a sibling test's advisory lock across fork.
// Keep this file's lock-owning tests out of that transient pre-exec window.
static GATE: Mutex<()> = Mutex::new(());
static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn gate() -> MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|error| error.into_inner())
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("fsm-cli-repair-{}-{sequence}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory {path:?}: {error}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn with_torn() -> TestDirectory {
    let dir = TestDirectory::create();
    let mut j = init(dir.path()).unwrap();
    let mut b = std::collections::BTreeMap::new();
    b.insert("instance_id".into(), Value::Str("i".into()));
    j.append(RecordKind::Annotated, Value::Obj(b)).unwrap();
    drop(j);
    let seg = dir.path().join("journal/seg-00000000000000000000.jsonl");
    let mut bytes = fs::read(&seg).unwrap();
    bytes.truncate(bytes.len() - 5);
    fs::write(&seg, bytes).unwrap();
    dir
}

#[test]
fn repair_torn_and_refuse_healthy() {
    let _guard = gate();
    let dir = with_torn();
    let orig = fs::read(dir.path().join("journal/seg-00000000000000000000.jsonl")).unwrap();
    match classify(dir.path()) {
        JournalHealth::TornTail { .. } => {}
        h => panic!("{h:?}"),
    }
    let rep = repair_truncate_torn_tail(dir.path()).unwrap();
    assert!(rep.quarantined.exists());
    let q = fs::read(&rep.quarantined).unwrap();
    let now = fs::read(dir.path().join("journal/seg-00000000000000000000.jsonl")).unwrap();
    let mut reassembled = now.clone();
    reassembled.extend_from_slice(&q);
    assert_eq!(reassembled, orig);
    assert!(matches!(classify(dir.path()), JournalHealth::Ok));
    assert!(matches!(
        repair_truncate_torn_tail(dir.path()),
        Err(RepairError::NothingToRepair)
    ));

    let healthy = TestDirectory::create();
    let _j = init(healthy.path()).unwrap();
    drop(_j);
    assert!(matches!(
        repair_truncate_torn_tail(healthy.path()),
        Err(RepairError::NothingToRepair)
    ));
}

#[cfg(unix)]
#[test]
fn cli_repair_refuses_a_symlinked_quarantine_as_io_write_before_truncating() {
    use std::os::unix::fs::symlink;

    let _guard = gate();
    let dir = with_torn();
    let segment = dir.path().join("journal/seg-00000000000000000000.jsonl");
    let before = fs::read(&segment).unwrap();
    let external = TestDirectory::create();
    let sentinel = external.path().join("sentinel");
    fs::write(&sentinel, b"external quarantine sentinel").unwrap();
    symlink(external.path(), dir.path().join("journal/quarantine")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["repair", "--truncate-torn-tail", "--json"])
        .arg(format!("--data-dir={}", dir.path().display()))
        .env("FSM_CLOCK_MS", "1000")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let diagnostic = parse(&output.stderr, &JsonLimits::DEFAULT).unwrap();
    assert_eq!(
        diagnostic.get("code").and_then(Value::as_str),
        Some("io/write")
    );
    assert_eq!(
        diagnostic.get("docs").and_then(Value::as_str),
        Some("fsm://docs/spec#io/write")
    );
    assert!(
        diagnostic
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.contains("persistence directory"))
    );
    assert_eq!(fs::read(&segment).unwrap(), before);
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"external quarantine sentinel"
    );
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 1);
}
