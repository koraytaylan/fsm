use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use fsm_cli::journal_io::{JournalHealth, classify, init, load_records, verify};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_core::replay::{NopSink, fold_with};

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
    let p = std::env::temp_dir().join(format!("fsm-rd-{pid}-{n}-{i}"));
    fs::create_dir_all(&p).unwrap();
    Scratch(p)
}

#[test]
fn mixed_session_and_refold() {
    fsm_cli::clock::force_ms(1);
    let dir = tmp();
    let mut s = Store::open(&dir).unwrap();
    let def = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    s.define_machine(def.clone(), false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(Default::default()), "e1", None)
        .unwrap();
    let _ = s.send_event("i1", "scored", Value::Obj(Default::default()), "e2", None);
    s.cancel_instance("i1", "k1").unwrap();
    s.annotate("i1", "a1", "note").unwrap();
    let recs = load_records(&dir).unwrap();
    let kinds: std::collections::BTreeSet<_> = recs.iter().map(|r| r.kind).collect();
    assert!(kinds.contains(&RecordKind::Genesis));
    assert!(kinds.contains(&RecordKind::MachineDefined));
    assert!(kinds.contains(&RecordKind::InstanceCreated));
    let st = fold_with(recs, &mut NopSink).unwrap();
    assert!(!st.machines.is_empty());
    s.shutdown_snapshot().ok();
    let v = verify(&dir);
    assert!(matches!(v.health, JournalHealth::Ok));
    fsm_cli::clock::reset_injected();
}

#[test]
fn verify_classifies_constructed() {
    let dir = tmp();
    let _ = init(&dir).unwrap();
    assert!(matches!(classify(&dir), JournalHealth::Ok));
    assert!(matches!(verify(&dir).health, JournalHealth::Ok));
}
