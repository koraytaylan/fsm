//! Production-entry regressions for the complete canonical journal-record cap.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::expr::eval::Val;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::{Record, RecordKind, seal};
use fsm_core::replay::{STATE_ROOT_FORMAT, state_root_at};
use fsm_core::sha256::sha256;
use fsm_store::clock::FixedClock;
use fsm_store::journal_io::{Journal, JournalIoError, should_rotate};
use fsm_store::snapshot::{load_newest_valid, write_snapshot};
use fsm_store::store::{ErrorObj, Store};

const PERSISTENCE_CAP: usize = JsonLimits::DEFAULT.max_bytes;
const STRING_CONTEXT_MACHINE: &[u8] = br#"{
    "format":"fsm.machine/1",
    "name":"journal_record_bounds",
    "states":[{"name":"waiting"}],
    "initial":"waiting",
    "context":[{"name":"label","ty":"str","init":"base"}],
    "events":[],
    "transitions":[]
}"#;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let sequence = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "fsm-store-journal-record-bounds-{}-{sequence}",
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

fn record_bytes(record: &Record) -> usize {
    let line = record.to_line();
    assert_eq!(line.last(), Some(&b'\n'));
    line.len() - 1
}

fn filler_for_record(base_record_bytes: usize, target_record_bytes: usize) -> String {
    assert!(base_record_bytes < target_record_bytes);
    "x".repeat(target_record_bytes - base_record_bytes)
}

fn segment_fingerprints(directory: &Path) -> Vec<(String, u64, [u8; 32])> {
    let mut segments = fs::read_dir(directory.join("journal"))
        .unwrap()
        .map(|entry| entry.unwrap())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !name.starts_with("seg-") || !name.ends_with(".jsonl") {
                return None;
            }
            let bytes = fs::read(entry.path()).unwrap();
            Some((name, bytes.len() as u64, sha256(&bytes)))
        })
        .collect::<Vec<_>>();
    segments.sort_by(|left, right| left.0.cmp(&right.0));
    segments
}

fn snapshot_fingerprints(directory: &Path) -> Vec<(String, u64, [u8; 32])> {
    let snapshots = directory.join("snapshots");
    let entries = match fs::read_dir(snapshots) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(error) => panic!("cannot list snapshot cache: {error}"),
    };
    let mut files = entries
        .map(|entry| entry.unwrap())
        .map(|entry| {
            let name = entry.file_name().into_string().unwrap();
            let bytes = fs::read(entry.path()).unwrap();
            (name, bytes.len() as u64, sha256(&bytes))
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.0.cmp(&right.0));
    files
}

fn define_string_context_machine(store: &mut Store) {
    let definition = parse(STRING_CONTEXT_MACHINE, &JsonLimits::DEFAULT).unwrap();
    store
        .define_machine_on(&mut FixedClock::new(1, 0), definition, false, false)
        .unwrap();
}

fn string_context_store(directory: &Path) -> Store {
    let mut store = Store::open(directory).unwrap();
    define_string_context_machine(&mut store);
    store
}

fn create_with_override(
    store: &mut Store,
    timestamp: i64,
    instance_id: &str,
    request_id: &str,
    value: String,
) -> Result<Value, ErrorObj> {
    store.create_instance_ctx_on(
        &mut FixedClock::new(timestamp, 0),
        "journal_record_bounds",
        instance_id,
        request_id,
        None,
        &BTreeMap::from([("label".into(), Val::Str(value))]),
        &[],
    )
}

fn create_with_tag(
    store: &mut Store,
    timestamp: i64,
    instance_id: &str,
    request_id: &str,
    tag: String,
) -> Result<Value, ErrorObj> {
    store.create_instance_ctx_on(
        &mut FixedClock::new(timestamp, 0),
        "journal_record_bounds",
        instance_id,
        request_id,
        None,
        &BTreeMap::new(),
        &[tag],
    )
}

struct StoreMarker {
    sequence: u64,
    hash: String,
    state_root: String,
    record_count: usize,
    journal_bytes: u64,
    journal_records: u32,
    segment_name: String,
    segments: Vec<(String, u64, [u8; 32])>,
}

fn mark_store(store: &Store, directory: &Path) -> StoreMarker {
    StoreMarker {
        sequence: store.state.last_seq,
        hash: store.state.last_hash.clone(),
        state_root: state_root_at(&store.state, store.state.last_seq),
        record_count: store.records.len(),
        journal_bytes: store.journal.seg_bytes,
        journal_records: store.journal.seg_records,
        segment_name: store.journal.seg_name.clone(),
        segments: segment_fingerprints(directory),
    }
}

fn assert_atomic_store_refusal(
    store: &Store,
    directory: &Path,
    marker: &StoreMarker,
    error: &ErrorObj,
    request_id: &str,
) {
    assert_eq!(error.code, "io/write");
    assert_eq!(error.docs, "fsm://docs/spec#io/write");
    assert_eq!(
        error.details.get("bytes").and_then(Value::as_num),
        Some((PERSISTENCE_CAP + 1).to_string().as_str())
    );
    assert_eq!(
        error.details.get("max_bytes").and_then(Value::as_num),
        Some(PERSISTENCE_CAP.to_string().as_str())
    );
    assert_eq!(
        error.details.get("request_id").and_then(Value::as_str),
        Some(request_id)
    );
    assert!(error.hint.contains("shorten identifiers"), "{error:?}");
    assert_eq!(store.state.last_seq, marker.sequence);
    assert_eq!(store.state.last_hash, marker.hash);
    assert_eq!(
        state_root_at(&store.state, store.state.last_seq),
        marker.state_root
    );
    assert!(!store.state.dedup.contains_key(request_id));
    assert_eq!(store.records.len(), marker.record_count);
    assert_eq!(store.journal.last_seq, marker.sequence);
    assert_eq!(store.journal.last_hash, marker.hash);
    assert_eq!(store.journal.seg_bytes, marker.journal_bytes);
    assert_eq!(store.journal.seg_records, marker.journal_records);
    assert_eq!(store.journal.seg_name, marker.segment_name);
    assert_eq!(segment_fingerprints(directory), marker.segments);
}

fn direct_padding_body(
    journal: &Journal,
    timestamp: i64,
    target: usize,
    state_root: &str,
) -> Value {
    let empty = Value::Obj(BTreeMap::from([
        ("padding".into(), Value::Str(String::new())),
        ("state_root".into(), Value::Str(state_root.into())),
        (
            "state_root_format".into(),
            Value::Str(STATE_ROOT_FORMAT.into()),
        ),
    ]));
    let base = seal(
        journal.last_seq + 1,
        timestamp,
        RecordKind::StateCheckpoint,
        empty,
        &journal.last_hash,
    );
    Value::Obj(BTreeMap::from([
        (
            "padding".into(),
            Value::Str(filler_for_record(record_bytes(&base), target)),
        ),
        ("state_root".into(), Value::Str(state_root.into())),
        (
            "state_root_format".into(),
            Value::Str(STATE_ROOT_FORMAT.into()),
        ),
    ]))
}

#[test]
fn direct_append_accepts_exact_cap_and_refuses_plus_one_before_rotation() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();

    let exact_root = state_root_at(&store.state, store.journal.last_seq + 1);
    let exact_body = direct_padding_body(&store.journal, 1, PERSISTENCE_CAP, &exact_root);
    let exact = store
        .journal
        .append_at(RecordKind::StateCheckpoint, exact_body, 1)
        .unwrap();
    assert_eq!(record_bytes(&exact), PERSISTENCE_CAP);

    let oversized_root = state_root_at(&store.state, store.journal.last_seq + 1);
    let oversized_body =
        direct_padding_body(&store.journal, 2, PERSISTENCE_CAP + 1, &oversized_root);
    store.journal.seg_bytes = u64::MAX;
    assert!(should_rotate(
        store.journal.seg_bytes,
        store.journal.seg_records
    ));
    let sequence = store.journal.last_seq;
    let hash = store.journal.last_hash.clone();
    let segment = store.journal.seg_name.clone();
    let segment_records = store.journal.seg_records;
    let segments = segment_fingerprints(directory.path());

    let error = store
        .journal
        .append_at(RecordKind::StateCheckpoint, oversized_body, 2)
        .unwrap_err();
    assert!(matches!(
        error,
        JournalIoError::RecordTooLarge {
            bytes,
            max_bytes
        } if bytes == PERSISTENCE_CAP + 1 && max_bytes == PERSISTENCE_CAP
    ));
    assert_eq!(store.journal.last_seq, sequence);
    assert_eq!(store.journal.last_hash, hash);
    assert_eq!(store.journal.seg_name, segment);
    assert_eq!(store.journal.seg_bytes, u64::MAX);
    assert_eq!(store.journal.seg_records, segment_records);
    assert!(!store.journal.poisoned);
    assert_eq!(segment_fingerprints(directory.path()), segments);
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(reopened.state.last_seq, sequence);
    assert_eq!(reopened.state.last_hash, hash);
}

#[test]
fn create_override_boundary_is_atomic_and_request_id_is_reusable() {
    let calibration_directory = TestDirectory::create();
    let mut calibration = string_context_store(calibration_directory.path());
    create_with_override(
        &mut calibration,
        2,
        "exact",
        "override-exact",
        String::new(),
    )
    .unwrap();
    let exact_base = record_bytes(calibration.records.last().unwrap());
    create_with_override(&mut calibration, 3, "over", "override-over", String::new()).unwrap();
    let oversized_base = record_bytes(calibration.records.last().unwrap());
    drop(calibration);

    let directory = TestDirectory::create();
    let mut store = string_context_store(directory.path());
    create_with_override(
        &mut store,
        2,
        "exact",
        "override-exact",
        filler_for_record(exact_base, PERSISTENCE_CAP),
    )
    .unwrap();
    assert_eq!(record_bytes(store.records.last().unwrap()), PERSISTENCE_CAP);

    let marker = mark_store(&store, directory.path());
    let error = create_with_override(
        &mut store,
        3,
        "over",
        "override-over",
        filler_for_record(oversized_base, PERSISTENCE_CAP + 1),
    )
    .unwrap_err();
    assert_atomic_store_refusal(&store, directory.path(), &marker, &error, "override-over");

    create_with_override(&mut store, 4, "over", "override-over", "corrected".into()).unwrap();
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert!(reopened.state.dedup.contains_key("override-exact"));
    assert!(reopened.state.dedup.contains_key("override-over"));
    assert_eq!(
        reopened.state.instances["over"].ctx.get("label"),
        Some(&Val::Str("corrected".into()))
    );
}

#[test]
fn create_tag_boundary_is_atomic_and_request_id_is_reusable() {
    let calibration_directory = TestDirectory::create();
    let mut calibration = string_context_store(calibration_directory.path());
    create_with_tag(&mut calibration, 2, "exact", "tag-exact", String::new()).unwrap();
    let exact_base = record_bytes(calibration.records.last().unwrap());
    create_with_tag(&mut calibration, 3, "over", "tag-over", String::new()).unwrap();
    let oversized_base = record_bytes(calibration.records.last().unwrap());
    drop(calibration);

    let directory = TestDirectory::create();
    let mut store = string_context_store(directory.path());
    create_with_tag(
        &mut store,
        2,
        "exact",
        "tag-exact",
        filler_for_record(exact_base, PERSISTENCE_CAP),
    )
    .unwrap();
    assert_eq!(record_bytes(store.records.last().unwrap()), PERSISTENCE_CAP);

    let marker = mark_store(&store, directory.path());
    let error = create_with_tag(
        &mut store,
        3,
        "over",
        "tag-over",
        filler_for_record(oversized_base, PERSISTENCE_CAP + 1),
    )
    .unwrap_err();
    assert_atomic_store_refusal(&store, directory.path(), &marker, &error, "tag-over");

    create_with_tag(&mut store, 4, "over", "tag-over", "corrected".into()).unwrap();
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert!(reopened.state.dedup.contains_key("tag-exact"));
    assert!(reopened.state.dedup.contains_key("tag-over"));
    assert_eq!(reopened.tags["over"], ["corrected"]);
}

fn cancellation_store(directory: &Path) -> Store {
    let mut store = string_context_store(directory);
    create_with_override(&mut store, 2, "exact", "create-exact", String::new()).unwrap();
    create_with_override(&mut store, 3, "over", "create-over", String::new()).unwrap();
    store
}

#[test]
fn cancellation_reason_boundary_is_atomic_and_request_id_is_reusable() {
    let calibration_directory = TestDirectory::create();
    let mut calibration = cancellation_store(calibration_directory.path());
    calibration
        .cancel_instance_reason_on(&mut FixedClock::new(4, 0), "exact", "cancel-exact", "")
        .unwrap();
    let exact_base = record_bytes(calibration.records.last().unwrap());
    calibration
        .cancel_instance_reason_on(&mut FixedClock::new(5, 0), "over", "cancel-over", "")
        .unwrap();
    let oversized_base = record_bytes(calibration.records.last().unwrap());
    drop(calibration);

    let directory = TestDirectory::create();
    let mut store = cancellation_store(directory.path());
    let exact_reason = filler_for_record(exact_base, PERSISTENCE_CAP);
    store
        .cancel_instance_reason_on(
            &mut FixedClock::new(4, 0),
            "exact",
            "cancel-exact",
            &exact_reason,
        )
        .unwrap();
    assert_eq!(record_bytes(store.records.last().unwrap()), PERSISTENCE_CAP);

    let marker = mark_store(&store, directory.path());
    let oversized_reason = filler_for_record(oversized_base, PERSISTENCE_CAP + 1);
    let error = store
        .cancel_instance_reason_on(
            &mut FixedClock::new(5, 0),
            "over",
            "cancel-over",
            &oversized_reason,
        )
        .unwrap_err();
    assert_atomic_store_refusal(&store, directory.path(), &marker, &error, "cancel-over");

    store
        .cancel_instance_reason_on(
            &mut FixedClock::new(6, 0),
            "over",
            "cancel-over",
            "corrected",
        )
        .unwrap();
    drop(store);

    let reopened = Store::open(directory.path()).unwrap();
    assert!(reopened.state.dedup.contains_key("cancel-exact"));
    assert!(reopened.state.dedup.contains_key("cancel-over"));
}

fn snapshot_state_store(label: String) -> Store {
    let mut store = Store::open_memory().unwrap();
    define_string_context_machine(&mut store);
    create_with_override(&mut store, 2, "snapshot", "snapshot-create", label).unwrap();
    store
}

fn snapshot_filler_for(target_bytes: usize) -> String {
    let calibration_directory = TestDirectory::create();
    let calibration = snapshot_state_store(String::new());
    let installed = write_snapshot(calibration_directory.path(), &calibration.state).unwrap();
    let base_bytes = fs::metadata(installed).unwrap().len() as usize;
    assert!(base_bytes < target_bytes);
    drop(calibration);
    "x".repeat(target_bytes - base_bytes)
}

#[test]
fn exact_cap_snapshot_is_written_and_round_trips_through_production() {
    let label = snapshot_filler_for(PERSISTENCE_CAP);
    let label_bytes = label.len();
    let directory = TestDirectory::create();
    let store = snapshot_state_store(label);
    assert!(record_bytes(store.records.last().unwrap()) < PERSISTENCE_CAP);

    let installed = write_snapshot(directory.path(), &store.state).unwrap();
    assert_eq!(
        fs::metadata(installed).unwrap().len(),
        PERSISTENCE_CAP as u64
    );
    let (sequence, reloaded) = load_newest_valid(directory.path()).unwrap();
    assert_eq!(sequence, store.state.last_seq);
    assert_eq!(reloaded.last_hash, store.state.last_hash);
    let Some(Val::Str(reloaded_label)) = reloaded.instances["snapshot"].ctx.get("label") else {
        panic!("snapshot label did not round-trip as a string");
    };
    assert_eq!(reloaded_label.len(), label_bytes);
    assert!(reloaded_label.bytes().all(|byte| byte == b'x'));
}

#[test]
fn cap_plus_one_snapshot_is_refused_before_any_cache_file_is_installed() {
    let label = snapshot_filler_for(PERSISTENCE_CAP + 1);
    let directory = TestDirectory::create();
    let store = snapshot_state_store(label);
    assert!(record_bytes(store.records.last().unwrap()) < PERSISTENCE_CAP);
    let before = snapshot_fingerprints(directory.path());

    let error = write_snapshot(directory.path(), &store.state).unwrap_err();
    assert_eq!(error.code, "io/write");
    assert_eq!(
        error.details.get("bytes").and_then(Value::as_num),
        Some((PERSISTENCE_CAP + 1).to_string().as_str())
    );
    assert_eq!(
        error.details.get("max_bytes").and_then(Value::as_num),
        Some(PERSISTENCE_CAP.to_string().as_str())
    );
    assert!(error.hint.contains("was not installed"), "{error:?}");
    assert_eq!(snapshot_fingerprints(directory.path()), before);
    drop(store);
    assert_eq!(snapshot_fingerprints(directory.path()), before);
}
