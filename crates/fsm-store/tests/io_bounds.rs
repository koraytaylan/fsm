//! Production-entry regressions for bounded, regular persistence inputs.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, parse};
use fsm_store::journal_io::{JournalHealth, classify, load_records, verify};
#[cfg(unix)]
use fsm_store::journal_io::{RepairError, repair_truncate_torn_tail};
#[cfg(unix)]
use fsm_store::snapshot::write_snapshot;
use fsm_store::store::{ErrorObj, Store};

const PERSISTENCE_READ_CAP: usize = JsonLimits::DEFAULT.max_bytes;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-store-io-bounds-{}-{sequence}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
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

fn first_segment(directory: &Path) -> PathBuf {
    fs::read_dir(directory.join("journal"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("seg-") && name.ends_with(".jsonl"))
        })
        .unwrap()
}

fn first_snapshot(directory: &Path) -> PathBuf {
    fs::read_dir(directory.join("snapshots"))
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("snap-") && name.ends_with(".json"))
        })
        .unwrap()
}

fn define_case_review(directory: &Path) {
    let mut store = Store::open(directory).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(definition, false, false).unwrap();
}

fn open_error(directory: &Path) -> ErrorObj {
    match Store::open(directory) {
        Ok(_) => panic!("store unexpectedly opened"),
        Err(error) => error,
    }
}

fn open_read_only_error(directory: &Path) -> ErrorObj {
    match Store::open_read_only(directory) {
        Ok(_) => panic!("store unexpectedly opened read-only"),
        Err(error) => error,
    }
}

fn assert_read_only_write_error<T>(result: Result<T, ErrorObj>) {
    let error = result
        .err()
        .expect("read-only mutation unexpectedly succeeded");
    assert_eq!(error.code, "io/write");
    assert_eq!(error.docs, "fsm://docs/spec#io/write");
    assert_eq!(error.message, "store was opened read-only");
}

fn assert_non_directory_journal_error(error: &ErrorObj, directory: &Path) {
    assert_eq!(error.code, "io/read");
    assert_eq!(error.docs, "fsm://docs/spec#io/read");
    assert!(
        error
            .message
            .contains("cannot inspect store format: read journal directory"),
        "{error:?}"
    );
    assert!(
        error
            .message
            .contains(&directory.join("journal").display().to_string()),
        "{error:?}"
    );
}

#[test]
fn read_only_open_rejects_a_non_directory_journal_without_mutating() {
    let directory = TestDirectory::create();
    fs::write(directory.path().join("journal"), b"not a directory").unwrap();

    let error = open_read_only_error(directory.path());
    assert_non_directory_journal_error(&error, directory.path());
    assert!(!directory.path().join("VERSION").exists());
    assert_eq!(
        fs::read(directory.path().join("journal")).unwrap(),
        b"not a directory"
    );
}

#[test]
fn writer_open_rejects_a_non_directory_journal_without_mutating() {
    let directory = TestDirectory::create();
    fs::write(directory.path().join("journal"), b"not a directory").unwrap();

    let error = open_error(directory.path());
    assert_eq!(error.code, "io/write");
    assert_eq!(error.docs, "fsm://docs/spec#io/write");
    assert!(error.message.contains("persistence directory"), "{error:?}");
    assert!(
        error
            .message
            .contains(&directory.path().join("journal").display().to_string()),
        "{error:?}"
    );
    assert!(!directory.path().join("VERSION").exists());
    assert_eq!(
        fs::read(directory.path().join("journal")).unwrap(),
        b"not a directory"
    );
}

#[test]
fn every_read_only_store_mutator_refuses_before_changing_persistence() {
    let directory = TestDirectory::create();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let mut writer = Store::open(directory.path()).unwrap();
    writer
        .define_machine(definition.clone(), false, false)
        .unwrap();
    writer
        .create_instance("case_review", "read-only-instance", "create", None)
        .unwrap();
    drop(writer);
    let segment = first_segment(directory.path());
    let segment_before = fs::read(&segment).unwrap();
    let snapshots_before = fs::read_dir(directory.path().join("snapshots"))
        .unwrap()
        .count();

    let mut reader = Store::open_read_only(directory.path()).unwrap();
    assert_read_only_write_error(reader.define_machine(definition, false, false));
    assert_read_only_write_error(reader.allocate_request_id());
    assert_read_only_write_error(reader.create_instance(
        "case_review",
        "another-instance",
        "read-only-create",
        None,
    ));
    assert_read_only_write_error(reader.send_event(
        "read-only-instance",
        "docs_ok",
        fsm_core::json::Value::Obj(Default::default()),
        "read-only-send",
        None,
    ));
    assert_read_only_write_error(reader.poll_instance_deadline(
        "read-only-instance",
        "read-only-poll",
        None,
    ));
    assert_read_only_write_error(reader.ack_effect(
        "read-only-instance",
        "effect",
        "read-only-ack",
    ));
    assert_read_only_write_error(reader.cancel_instance("read-only-instance", "read-only-cancel"));
    assert_read_only_write_error(reader.annotate(
        "read-only-instance",
        "read-only-annotate",
        "note",
    ));
    assert_read_only_write_error(reader.maybe_snapshot());
    assert_read_only_write_error(reader.shutdown_snapshot());
    drop(reader);

    assert_eq!(fs::read(segment).unwrap(), segment_before);
    assert!(!directory.path().join("alloc").exists());
    assert!(!directory.path().join("alloc.tmp").exists());
    assert_eq!(
        fs::read_dir(directory.path().join("snapshots"))
            .unwrap()
            .count(),
        snapshots_before
    );
}

#[test]
fn read_only_open_ignores_an_in_progress_final_tail_as_one_complete_prefix() {
    let directory = TestDirectory::create();
    let mut writer = Store::open(directory.path()).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    writer.define_machine(definition, false, false).unwrap();
    let segment = first_segment(directory.path());
    let complete_bytes = fs::metadata(&segment).unwrap().len();
    let complete_records = writer.records.len();
    let complete_sequence = writer.state.last_seq;
    let complete_hash = writer.state.last_hash.clone();
    let complete_segment_records = writer.journal.seg_records;
    OpenOptions::new()
        .append(true)
        .open(&segment)
        .unwrap()
        .write_all(b"{\"seq\":")
        .unwrap();

    assert!(matches!(
        classify(directory.path()),
        JournalHealth::TornTail { .. }
    ));
    assert!(matches!(
        verify(directory.path()).health,
        JournalHealth::TornTail { .. }
    ));
    assert!(
        load_records(directory.path())
            .unwrap_err()
            .contains("unterminated journal record")
    );

    let reader = Store::open_read_only(directory.path()).unwrap();
    assert_eq!(reader.records.len(), complete_records);
    assert_eq!(reader.state.last_seq, complete_sequence);
    assert_eq!(reader.state.last_hash, complete_hash);
    assert_eq!(reader.journal.last_seq, complete_sequence);
    assert_eq!(reader.journal.seg_bytes, complete_bytes);
    assert_eq!(reader.journal.seg_records, complete_segment_records);
    assert_eq!(fs::metadata(&segment).unwrap().len(), complete_bytes + 7);
    drop(reader);
    drop(writer);

    let error = open_error(directory.path());
    assert_eq!(error.code, "store/torn_tail");
}

#[test]
fn complete_json_without_lf_is_torn_for_strict_readers_and_omitted_read_only() {
    let directory = TestDirectory::create();
    define_case_review(directory.path());
    let segment = first_segment(directory.path());
    let bytes = fs::read(&segment).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    let complete_prefix_bytes = bytes.iter().position(|byte| *byte == b'\n').unwrap() as u64 + 1;
    OpenOptions::new()
        .write(true)
        .open(&segment)
        .unwrap()
        .set_len(bytes.len() as u64 - 1)
        .unwrap();

    assert!(matches!(
        classify(directory.path()),
        JournalHealth::TornTail { .. }
    ));
    assert!(matches!(
        verify(directory.path()).health,
        JournalHealth::TornTail { .. }
    ));
    assert!(
        load_records(directory.path())
            .unwrap_err()
            .contains("unterminated journal record")
    );

    let reader = Store::open_read_only(directory.path()).unwrap();
    assert_eq!(reader.records.len(), 1);
    assert_eq!(reader.state.last_seq, 0);
    assert!(reader.state.machines.is_empty());
    assert_eq!(reader.journal.last_seq, 0);
    assert_eq!(reader.journal.seg_bytes, complete_prefix_bytes);
    assert_eq!(reader.journal.seg_records, 1);
    drop(reader);

    let error = open_error(directory.path());
    assert_eq!(error.code, "store/torn_tail");
}

#[test]
fn exact_cap_version_reaches_format_detection() {
    let directory = TestDirectory::create();
    drop(Store::open(directory.path()).unwrap());
    let version = directory.path().join("VERSION");
    let mut bytes = vec![b' '; PERSISTENCE_READ_CAP];
    bytes[0] = b'8';
    fs::write(&version, bytes).unwrap();
    assert_eq!(
        fs::metadata(version).unwrap().len(),
        PERSISTENCE_READ_CAP as u64
    );

    let store = Store::open(directory.path()).unwrap();
    assert_eq!(store.state.last_seq, 0);
}

#[test]
fn oversized_version_is_rejected_before_reading() {
    let directory = TestDirectory::create();
    File::create(directory.path().join("VERSION"))
        .unwrap()
        .set_len(PERSISTENCE_READ_CAP as u64 + 1)
        .unwrap();

    let error = open_error(directory.path());
    assert_eq!(error.code, "io/read");
    assert!(
        error.message.contains("exceeds 16777216 bytes"),
        "{error:?}"
    );
}

#[test]
fn exact_cap_journal_record_reaches_record_verification() {
    let directory = TestDirectory::create();
    drop(Store::open(directory.path()).unwrap());
    let segment = first_segment(directory.path());
    let original = fs::metadata(&segment).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(&segment)
        .unwrap()
        .set_len(original + PERSISTENCE_READ_CAP as u64)
        .unwrap();

    let error = open_error(directory.path());
    assert_eq!(error.code, "store/torn_tail");
    assert!(
        !error.message.contains("journal record exceeds"),
        "{error:?}"
    );
}

#[test]
fn oversized_journal_record_is_rejected_with_a_bounded_stream() {
    let directory = TestDirectory::create();
    drop(Store::open(directory.path()).unwrap());
    let segment = first_segment(directory.path());
    let original = fs::metadata(&segment).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(&segment)
        .unwrap()
        .set_len(original + PERSISTENCE_READ_CAP as u64 + 1)
        .unwrap();

    let error = open_error(directory.path());
    assert_eq!(error.code, "io/read");
    assert_eq!(error.docs, "fsm://docs/spec#io/read");
    assert!(
        error.message.contains("journal record exceeds"),
        "{error:?}"
    );
    assert!(error.hint.contains("restore the named persistence path"));
    assert!(matches!(
        verify(directory.path()).health,
        JournalHealth::StoreIo(_)
    ));
}

#[test]
fn exact_cap_snapshot_reaches_parsing_verification_and_selection() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(definition, false, false).unwrap();
    store.shutdown_snapshot().unwrap();

    let original_snapshot = first_snapshot(directory.path());
    let snapshot_name = original_snapshot.file_name().unwrap().to_owned();
    let mut bytes = fs::read(original_snapshot).unwrap();
    assert!(bytes.len() < PERSISTENCE_READ_CAP);
    bytes.resize(PERSISTENCE_READ_CAP, b' ');
    drop(store);

    let snapshots = directory.path().join("snapshots");
    fs::remove_dir_all(&snapshots).unwrap();
    fs::create_dir_all(&snapshots).unwrap();
    let exact_snapshot = snapshots.join(snapshot_name);
    fs::write(&exact_snapshot, bytes).unwrap();
    assert_eq!(
        fs::metadata(exact_snapshot).unwrap().len(),
        PERSISTENCE_READ_CAP as u64
    );

    let store = Store::open(directory.path()).unwrap();
    assert!(store.opened_from_snapshot);
    assert_eq!(store.replayed_records, 0);
    assert_eq!(store.state.machines.len(), 1);
}

#[test]
fn oversized_snapshot_is_ignored_as_an_untrusted_cache() {
    let directory = TestDirectory::create();
    define_case_review(directory.path());
    let snapshots = directory.path().join("snapshots");
    let _ = fs::remove_dir_all(&snapshots);
    fs::create_dir_all(&snapshots).unwrap();
    File::create(snapshots.join("snap-999.json"))
        .unwrap()
        .set_len(PERSISTENCE_READ_CAP as u64 + 1)
        .unwrap();

    let store = Store::open(directory.path()).unwrap();
    assert!(!store.opened_from_snapshot);
    assert_eq!(store.state.machines.len(), 1);
}

#[cfg(unix)]
#[test]
fn symlinked_version_and_endless_journal_are_rejected_without_following() {
    use std::os::unix::fs::symlink;

    let version_directory = TestDirectory::create();
    fs::write(version_directory.path().join("actual-version"), "8\n").unwrap();
    symlink("actual-version", version_directory.path().join("VERSION")).unwrap();
    let version_error = open_error(version_directory.path());
    assert_eq!(version_error.code, "io/read");
    assert!(version_error.message.contains("non-symlink"));

    let journal_directory = TestDirectory::create();
    drop(Store::open(journal_directory.path()).unwrap());
    let segment = first_segment(journal_directory.path());
    fs::remove_file(&segment).unwrap();
    symlink("/dev/zero", &segment).unwrap();
    let journal_error = open_error(journal_directory.path());
    assert_eq!(journal_error.code, "io/read");
    assert_eq!(journal_error.docs, "fsm://docs/spec#io/read");
    assert!(journal_error.message.contains("non-symlink"));
    assert!(matches!(
        verify(journal_directory.path()).health,
        JournalHealth::StoreIo(_)
    ));
}

#[cfg(unix)]
#[test]
fn symlinked_endless_snapshot_is_ignored_without_opening() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    define_case_review(directory.path());
    let snapshots = directory.path().join("snapshots");
    let _ = fs::remove_dir_all(&snapshots);
    fs::create_dir_all(&snapshots).unwrap();
    symlink("/dev/zero", snapshots.join("snap-999.json")).unwrap();

    let store = Store::open(directory.path()).unwrap();
    assert_eq!(store.state.machines.len(), 1);
}

#[cfg(unix)]
#[test]
fn symlinked_snapshot_directory_is_ignored_for_reads_and_refused_for_writes() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    define_case_review(directory.path());
    let snapshots = directory.path().join("snapshots");
    fs::remove_dir_all(&snapshots).unwrap();

    let external = TestDirectory::create();
    for sequence in 1..=4 {
        fs::write(
            external.path().join(format!("snap-{sequence}.json")),
            format!("external sentinel {sequence}"),
        )
        .unwrap();
    }
    symlink(external.path(), &snapshots).unwrap();

    let reader = Store::open_read_only(directory.path()).unwrap();
    assert_eq!(reader.state.machines.len(), 1);
    drop(reader);
    let error = open_error(directory.path());
    assert_eq!(error.code, "io/write");
    assert!(error.message.contains("persistence directory"), "{error:?}");
    for sequence in 1..=4 {
        assert_eq!(
            fs::read_to_string(external.path().join(format!("snap-{sequence}.json"))).unwrap(),
            format!("external sentinel {sequence}")
        );
    }
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 4);
    fs::remove_file(snapshots).unwrap();
}

#[test]
fn non_directory_snapshot_cache_is_ignored_for_reads_and_refused_for_writes() {
    let directory = TestDirectory::create();
    define_case_review(directory.path());
    let snapshots = directory.path().join("snapshots");
    fs::remove_dir_all(&snapshots).unwrap();
    fs::write(&snapshots, b"not a directory").unwrap();

    let reader = Store::open_read_only(directory.path()).unwrap();
    assert_eq!(reader.state.machines.len(), 1);
    drop(reader);
    let error = open_error(directory.path());
    assert_eq!(error.code, "io/write");
    assert_eq!(fs::read(snapshots).unwrap(), b"not a directory");
}

#[cfg(unix)]
#[test]
fn symlinked_journal_directory_is_rejected_without_external_mutation() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    let external = TestDirectory::create();
    let sentinel = external.path().join("sentinel");
    fs::write(&sentinel, b"external journal sentinel").unwrap();
    let journal = directory.path().join("journal");
    symlink(external.path(), &journal).unwrap();

    let read_error = open_read_only_error(directory.path());
    assert_eq!(read_error.code, "io/read");
    assert!(read_error.message.contains("persistence directory"));
    let write_error = open_error(directory.path());
    assert_eq!(write_error.code, "io/write");
    assert_eq!(write_error.docs, "fsm://docs/spec#io/write");
    assert!(write_error.message.contains("persistence directory"));
    assert!(matches!(
        verify(directory.path()).health,
        JournalHealth::StoreIo(_)
    ));
    assert_eq!(fs::read(&sentinel).unwrap(), b"external journal sentinel");
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 1);
    assert!(!directory.path().join("VERSION").exists());
    fs::remove_file(journal).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinked_lock_file_never_overwrites_its_target() {
    use std::os::unix::fs::symlink;

    let lock_directory = TestDirectory::create();
    let external = TestDirectory::create();
    fs::create_dir(lock_directory.path().join("journal")).unwrap();
    let external_lock = external.path().join("external-lock");
    fs::write(&external_lock, b"lock sentinel").unwrap();
    symlink(&external_lock, lock_directory.path().join("journal/LOCK")).unwrap();
    let lock_error = open_error(lock_directory.path());
    assert_eq!(lock_error.code, "io/write");
    assert_eq!(lock_error.docs, "fsm://docs/spec#io/write");
    assert_eq!(fs::read(&external_lock).unwrap(), b"lock sentinel");
}

#[cfg(unix)]
#[test]
fn symlinked_allocation_temp_file_never_overwrites_its_target() {
    use std::os::unix::fs::symlink;

    let allocation_directory = TestDirectory::create();
    let external = TestDirectory::create();
    let mut store = Store::open(allocation_directory.path()).unwrap();
    let external_allocation = external.path().join("external-allocation");
    fs::write(&external_allocation, b"allocation sentinel").unwrap();
    symlink(
        &external_allocation,
        allocation_directory.path().join("alloc.tmp"),
    )
    .unwrap();
    let allocation_error = store.allocate_request_id().unwrap_err();
    assert_eq!(allocation_error.code, "io/write");
    assert_eq!(
        fs::read(&external_allocation).unwrap(),
        b"allocation sentinel"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_allocation_destination_is_replaced_without_overwriting_its_target() {
    use std::os::unix::fs::symlink;

    let allocation_directory = TestDirectory::create();
    let external = TestDirectory::create();
    let mut store = Store::open(allocation_directory.path()).unwrap();
    let external_allocation = external.path().join("external-allocation");
    fs::write(&external_allocation, b"allocation destination sentinel").unwrap();
    symlink(
        &external_allocation,
        allocation_directory.path().join("alloc"),
    )
    .unwrap();

    let request_id = store.allocate_request_id().unwrap();
    assert!(request_id.starts_with("req-"));
    assert_eq!(
        fs::read(&external_allocation).unwrap(),
        b"allocation destination sentinel"
    );
    assert!(
        !fs::symlink_metadata(allocation_directory.path().join("alloc"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn symlinked_rotation_segment_never_overwrites_its_target() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    let external = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let next_sequence = store.journal.last_seq + 1;
    let next_segment = directory
        .path()
        .join(format!("journal/seg-{next_sequence:020}.jsonl"));
    let sentinel = external.path().join("segment-target");
    fs::write(&sentinel, b"segment sentinel").unwrap();
    symlink(&sentinel, &next_segment).unwrap();

    let error = store.journal.force_rotate().unwrap_err();
    assert!(error.to_string().contains("non-symlink"), "{error:?}");
    assert_eq!(fs::read(&sentinel).unwrap(), b"segment sentinel");
    assert!(
        fs::symlink_metadata(next_segment)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[test]
fn symlinked_snapshot_destination_never_overwrites_its_target() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    let external = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let definition = parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    store.define_machine(definition, false, false).unwrap();
    let destination = directory
        .path()
        .join(format!("snapshots/snap-{}.json", store.state.last_seq));
    let sentinel = external.path().join("snapshot-target");
    fs::write(&sentinel, b"snapshot destination sentinel").unwrap();
    symlink(&sentinel, &destination).unwrap();

    let installed = write_snapshot(directory.path(), &store.state).unwrap();
    assert_ne!(installed, destination);
    drop(store);
    assert_eq!(
        fs::read(&sentinel).unwrap(),
        b"snapshot destination sentinel"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_quarantine_directory_is_refused_before_tail_truncation() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    drop(Store::open(directory.path()).unwrap());
    let segment = first_segment(directory.path());
    let mut torn_bytes = fs::read(&segment).unwrap();
    torn_bytes.extend_from_slice(b"{\"partial");
    fs::write(&segment, &torn_bytes).unwrap();

    let external = TestDirectory::create();
    let sentinel = external.path().join("sentinel");
    fs::write(&sentinel, b"quarantine sentinel").unwrap();
    symlink(external.path(), directory.path().join("journal/quarantine")).unwrap();

    let error = repair_truncate_torn_tail(directory.path()).unwrap_err();
    assert!(matches!(error, RepairError::WriteIo(_)));
    assert_eq!(fs::read(&segment).unwrap(), torn_bytes);
    assert_eq!(fs::read(&sentinel).unwrap(), b"quarantine sentinel");
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn symlinked_quarantine_leaf_is_skipped_without_overwriting_its_target() {
    use std::os::unix::fs::symlink;

    let directory = TestDirectory::create();
    drop(Store::open(directory.path()).unwrap());
    let segment = first_segment(directory.path());
    let segment_name = segment.file_name().unwrap().to_string_lossy();
    let mut torn_bytes = fs::read(&segment).unwrap();
    torn_bytes.extend_from_slice(b"{\"partial");
    fs::write(&segment, torn_bytes).unwrap();
    let quarantine = directory.path().join("journal/quarantine");
    fs::create_dir(&quarantine).unwrap();

    let external = TestDirectory::create();
    let sentinel = external.path().join("sentinel");
    fs::write(&sentinel, b"quarantine leaf sentinel").unwrap();
    symlink(
        &sentinel,
        quarantine.join(format!("{segment_name}-tail-1.bin")),
    )
    .unwrap();

    let repaired = repair_truncate_torn_tail(directory.path()).unwrap();
    assert_ne!(
        repaired.quarantined,
        quarantine.join(format!("{segment_name}-tail-1.bin"))
    );
    assert!(
        repaired
            .quarantined
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(&format!("{segment_name}-tail-1-"))
    );
    assert_eq!(fs::read(&sentinel).unwrap(), b"quarantine leaf sentinel");
}
