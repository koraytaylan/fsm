use super::*;
use std::fs::OpenOptions;
use std::time::{SystemTime, UNIX_EPOCH};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{RecordKind, seal, verify_line, zeros};
use fsm_core::replay::NopSink;

use super::paths::acquire_lock;

/// Per-process counter. Tests in one binary run concurrently and a
/// timestamp alone collides: two threads landing in the same nanosecond
/// bucket share a directory, and one wipes the other's store mid-run. It
/// showed up first on a fast macOS release build.
static TMP_N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn tmp() -> PathBuf {
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let i = TMP_N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("fsm-j-{pid}-{n}-{i}"));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn init_genesis_and_durability() {
    crate::clock::force_ms(5000);
    let dir = tmp();
    let mut j = init(&dir).unwrap();
    let seg = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
    let bytes = fs::read(&seg).unwrap();
    assert!(bytes.ends_with(b"\n"));
    let rec = verify_line(&bytes, 0, &zeros()).unwrap();
    assert_eq!(rec.kind, RecordKind::Genesis);
    assert_eq!(rec.prev, zeros());
    let a = j
        .append(RecordKind::Annotated, {
            let mut b = std::collections::BTreeMap::new();
            b.insert("instance_id".into(), Value::Str("i".into()));
            Value::Obj(b)
        })
        .unwrap();
    let fresh = fs::read(&seg).unwrap();
    assert!(std::str::from_utf8(&fresh).unwrap().contains(&a.hash));
    assert!(a.ts >= 5000);
    crate::clock::reset_injected();
    crate::clock::reset_injected();
}

#[test]
fn rotate_decision() {
    assert!(!should_rotate(0, 65_535));
    assert!(should_rotate(0, 65_536));
    assert!(!should_rotate(64 * 1024 * 1024 - 1, 1));
    assert!(should_rotate(64 * 1024 * 1024, 1));
}

#[test]
fn poison_fast() {
    let dir = tmp();
    let mut j = init(&dir).unwrap();
    j.poisoned = true;
    let before = fs::metadata(journal_dir(&dir).join(&j.seg_name))
        .unwrap()
        .len();
    assert!(
        j.append(RecordKind::Annotated, Value::Obj(Default::default()))
            .is_err()
    );
    let after = fs::metadata(journal_dir(&dir).join(&j.seg_name))
        .unwrap()
        .len();
    assert_eq!(before, after);
}

#[test]
fn lock_exclusion_and_reacquire() {
    let dir = tmp();
    let j = init(&dir).unwrap();
    let lock_path = journal_dir(&dir).join("LOCK");
    // Readable while held only where locks are advisory. Windows refuses
    // this read outright, which is why the pid is diagnostic and the
    // contention path never depends on it.
    #[cfg(unix)]
    {
        let meta = fs::read_to_string(&lock_path).unwrap();
        assert!(meta.contains(&format!("\"pid\":{}", std::process::id())));
    }
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    assert!(f.try_lock().is_err());
    drop(j);
    let j2 = {
        // init would rewrite genesis; just acquire lock via open path
        let jdir = journal_dir(&dir);
        acquire_lock(&jdir).unwrap()
    };
    drop(j2);
}

#[test]
fn read_only_open_returns_the_exact_folded_record_prefix() {
    let dir = tmp();
    let mut writer = crate::store::Store::open(&dir).unwrap();
    let definition = parse(
        include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    writer.define_machine(definition, false, false).unwrap();

    let mut sink = NopSink;
    let (reader, state, open_path, records) = open_read_only(&dir, &mut sink).unwrap();
    let returned_last_seq = records.last().map(|record| record.seq).unwrap_or(0);
    assert_eq!(returned_last_seq, reader.last_seq);
    assert_eq!(returned_last_seq, state.last_seq);
    assert_eq!(open_path.replayed_records, records.len());
    assert_eq!(reader.seg_records as usize, records.len());
    let returned_segment = dir.join("journal").join(&reader.seg_name);
    assert_eq!(
        fs::metadata(&returned_segment).unwrap().len(),
        reader.seg_bytes
    );
    let returned_segment_bytes = reader.seg_bytes;

    // A writer may advance immediately after the bounded read. The
    // returned vector remains the authoritative prefix for both the
    // journal metadata and folded state assembled above.
    writer
        .create_instance("case_review", "later-instance", "create-later", None)
        .unwrap();
    assert!(load_records(&dir).unwrap().last().unwrap().seq > returned_last_seq);
    assert_eq!(records.last().unwrap().seq, state.last_seq);
    assert!(
        fs::metadata(returned_segment).unwrap().len() > returned_segment_bytes,
        "the live writer should advance the same segment after the read-only prefix"
    );
    assert_eq!(reader.seg_bytes, returned_segment_bytes);
}

#[test]
fn version_marker_preflight() {
    let dir = tmp();
    let j = init(&dir).unwrap();
    drop(j);
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
    fs::write(dir.join("VERSION"), "3\n").unwrap();
    assert_eq!(
        detect_store_format(&dir),
        DetectedStoreFormat::Migratable { found: "3".into() }
    );
    assert!(refuse_incompatible_store_format(&dir).is_ok());
    let j = init(&dir).unwrap();
    drop(j);
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
    fs::remove_file(dir.join("VERSION")).unwrap();
    assert_eq!(
        detect_store_format(&dir),
        DetectedStoreFormat::Migratable { found: "1".into() }
    );
    let j = init(&dir).unwrap();
    drop(j);
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
    // One past the current format: written by a newer build, so it is
    // refused rather than migrated. Derived so a version bump keeps this a
    // future version instead of silently testing the current one.
    let future = (STORE_VERSION.parse::<u32>().unwrap() + 1).to_string();
    fs::write(dir.join("VERSION"), format!("{future}\n")).unwrap();
    assert!(matches!(
        refuse_incompatible_store_format(&dir),
        Err(JournalHealth::VersionMismatch { found }) if found == future
    ));
    assert!(matches!(
        init(&dir),
        Err(JournalIoError::VersionMismatch { found }) if found == future
    ));
}

#[test]
fn migratable_marker_stamps_after_successful_open() {
    let dir = tmp();
    let j = init(&dir).unwrap();
    drop(j);
    fs::write(dir.join("VERSION"), "5\n").unwrap();
    let mut sink = NopSink;
    drop(open(&dir, &mut sink).unwrap());
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
    fs::remove_file(dir.join("VERSION")).unwrap();
    drop(open(&dir, &mut sink).unwrap());
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
}

#[test]
fn every_prior_version_migrates_after_successful_full_fold() {
    for prior in 1..STORE_VERSION.parse::<u32>().unwrap() {
        let dir = tmp();
        let journal = init(&dir).unwrap();
        drop(journal);
        fs::write(dir.join("VERSION"), format!("{prior}\n")).unwrap();
        assert_eq!(
            detect_store_format(&dir),
            DetectedStoreFormat::Migratable {
                found: prior.to_string()
            }
        );

        let mut sink = NopSink;
        drop(open(&dir, &mut sink).unwrap());
        assert_eq!(
            fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
            STORE_VERSION
        );
    }
}

#[test]
fn version_seven_migrates_with_the_exact_historical_genesis_limits() {
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    let cleanup = Cleanup(tmp());
    let dir = &cleanup.0;
    let journal = init(dir).unwrap();
    drop(journal);

    let current = load_records(dir).unwrap().remove(0);
    let Value::Obj(mut body) = current.body else {
        panic!("genesis body must be an object")
    };
    let Value::Obj(mut limits) = body.remove("limits").unwrap() else {
        panic!("genesis limits must be an object")
    };
    limits.remove("max_regions");
    limits.remove("max_deadlines");
    limits.remove("max_eval_ticks");
    body.insert("limits".into(), Value::Obj(limits));
    let legacy = seal(
        0,
        current.ts,
        RecordKind::Genesis,
        Value::Obj(body),
        &zeros(),
    );
    fs::write(
        journal_dir(dir).join("seg-00000000000000000000.jsonl"),
        legacy.to_line(),
    )
    .unwrap();
    fs::write(dir.join("VERSION"), "7\n").unwrap();

    let mut sink = NopSink;
    let (reopened, state, path) = open(dir, &mut sink).unwrap();
    assert!(state.instances.is_empty());
    assert_eq!(path.replayed_records, 1);
    assert!(!path.used_snapshot);
    drop(reopened);
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
}

#[test]
fn migratable_torn_tail_does_not_stamp() {
    let dir = tmp();
    let j = init(&dir).unwrap();
    drop(j);
    let seg = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
    let mut bytes = fs::read(&seg).unwrap();
    bytes.extend_from_slice(b"{\"partial");
    fs::write(&seg, bytes).unwrap();
    fs::write(dir.join("VERSION"), "5\n").unwrap();
    let mut sink = NopSink;
    assert!(matches!(
        open(&dir, &mut sink),
        Err(OpenError::Health(JournalHealth::TornTail { .. }))
    ));
    assert_eq!(fs::read_to_string(dir.join("VERSION")).unwrap().trim(), "5");
}

#[test]
fn migratable_marker_without_journal_refuses() {
    let dir = tmp();
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("VERSION"), "3\n").unwrap();
    let mut sink = NopSink;
    assert!(matches!(
        open(&dir, &mut sink),
        Err(OpenError::Health(JournalHealth::MissingGenesis))
    ));
    assert!(matches!(init(&dir), Err(JournalIoError::Io(_))));
    assert_eq!(fs::read_to_string(dir.join("VERSION")).unwrap().trim(), "3");
}

#[test]
fn stray_tmp_segment_is_not_a_store() {
    let dir = tmp();
    let jdir = journal_dir(&dir);
    fs::create_dir_all(&jdir).unwrap();
    fs::write(jdir.join("seg-00000000000000000000.jsonl.tmp"), b"junk").unwrap();
    assert_eq!(detect_store_format(&dir), DetectedStoreFormat::Empty);
    let j = init(&dir).unwrap();
    drop(j);
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
}

#[test]
fn unreadable_version_is_store_io() {
    let dir = tmp();
    fs::create_dir_all(dir.join("VERSION")).unwrap();
    assert!(matches!(
        detect_store_format(&dir),
        DetectedStoreFormat::Unreadable { .. }
    ));
    assert!(matches!(
        refuse_incompatible_store_format(&dir),
        Err(JournalHealth::StoreIo(_))
    ));
    let mut sink = NopSink;
    assert!(matches!(
        open(&dir, &mut sink),
        Err(OpenError::Health(JournalHealth::StoreIo(_)))
    ));
}

#[test]
fn repair_stamps_migratable_store() {
    let dir = tmp();
    let j = init(&dir).unwrap();
    drop(j);
    let seg = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
    let mut bytes = fs::read(&seg).unwrap();
    bytes.extend_from_slice(b"{\"partial");
    fs::write(&seg, bytes).unwrap();
    fs::write(dir.join("VERSION"), "5\n").unwrap();
    repair_truncate_torn_tail(&dir).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("VERSION")).unwrap().trim(),
        STORE_VERSION
    );
    let mut sink = NopSink;
    drop(open(&dir, &mut sink).unwrap());
}

#[test]
fn clock_injected() {
    crate::clock::force_ms(100);
    assert_eq!(crate::clock::now_ms(), 100);
    assert_eq!(crate::clock::now_ms(), 101);
    crate::clock::reset_injected();
}

#[test]
fn repair_rotated_torn_tail() {
    let dir = tmp();
    let def = parse(
        include_bytes!("../../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let mut s = crate::store::Store::open(&dir).unwrap();
    s.define_machine(def, false, false).unwrap();
    s.create_instance("case_review", "i1", "c1", None).unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(Default::default()), "s1", None)
        .unwrap();
    s.send_event("i1", "docs_ok", Value::Obj(Default::default()), "s2", None)
        .unwrap();
    drop(s);
    let first = journal_dir(&dir).join("seg-00000000000000000000.jsonl");
    let bytes = fs::read(&first).unwrap();
    let lines: Vec<&[u8]> = bytes.split_inclusive(|&b| b == b'\n').collect();
    assert!(lines.len() >= 5);
    let mut keep = Vec::new();
    for line in &lines[..3] {
        keep.extend_from_slice(line);
    }
    let mut rest = Vec::new();
    for line in &lines[3..] {
        rest.extend_from_slice(line);
    }
    rest.extend(b"{\"seq\":\"5\"");
    fs::write(&first, keep).unwrap();
    let seg = journal_dir(&dir).join("seg-00000000000000000003.jsonl");
    fs::write(&seg, &rest).unwrap();
    match classify(&dir) {
        JournalHealth::TornTail { segment, .. } => {
            assert!(segment.contains("00000000000000000003"));
        }
        h => panic!("{h:?}"),
    }
    let r = repair_truncate_torn_tail(&dir).unwrap();
    assert_eq!(r.truncated_to_seq, 4);
    assert!(
        r.quarantined
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("tail-5")
    );
    let v = verify(&dir);
    assert!(matches!(v.health, JournalHealth::Ok), "{:?}", v.health);
    crate::store::Store::open(&dir).unwrap();
    let mut again = fs::read(&seg).unwrap();
    again.extend(b"{\"seq\":\"5\"");
    fs::write(&seg, again).unwrap();
    let r2 = repair_truncate_torn_tail(&dir).unwrap();
    assert_eq!(r2.truncated_to_seq, 4);
    assert!(r2.quarantined.exists());
    assert_ne!(r.quarantined, r2.quarantined);
    assert!(matches!(verify(&dir).health, JournalHealth::Ok));
}
