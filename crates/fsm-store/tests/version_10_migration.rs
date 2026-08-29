//! Store `VERSION` 10: one named case per prior version still supported, and
//! the plain statement that the 9-to-10 step converts nothing.
//!
//! Plan 0017 task 8003. A store that can hold a seal is a store an older build
//! must not open, so the version moves once. It moves on **first write**,
//! sealed or not, which is the actual compatibility consequence: a 0.3.0 store
//! is not readable by 0.2.x whether or not anything was ever archived.
//!
//! The cases are written out rather than looped over a range, so a failure
//! line names the version that broke.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::replay::{NopSink, fold_with};
use fsm_store::journal_io::{
    DetectedStoreFormat, STORE_VERSION, detect_store_format, load_records,
};
use fsm_store::snapshot::store_states_eq;
use fsm_store::store::Store;

/// A journal an earlier build wrote, replayed here under every prior version
/// marker. Its records are never rewritten, whatever the marker said.
const LEGACY_JOURNAL: &[u8] = include_bytes!("fixtures/non_reactive_session.journal");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A directory name no other run of this binary can produce.
///
/// A process id alone is not unique enough: a full `--workspace` run spawns
/// thousands of short-lived processes, ids get reused, and a reused id names a
/// directory a previous run may still be finishing with — which surfaces as a
/// `store/lock` naming *this* process. `crash_harness.rs` learned the same
/// thing and pins it with a test; this is that idiom.
fn invocation_tag() -> String {
    format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or(0)
    )
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(tag: &str) -> Self {
        let index = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-v10-{tag}-{}-{index}", invocation_tag()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
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

fn lay_out(directory: &TestDirectory, version: &str) {
    let journal = directory.path().join("journal");
    fs::create_dir_all(&journal).expect("the journal directory is creatable");
    fs::write(
        journal.join("seg-00000000000000000000.jsonl"),
        LEGACY_JOURNAL,
    )
    .expect("the segment is writable");
    fs::write(directory.path().join("VERSION"), format!("{version}\n"))
        .expect("the VERSION marker is writable");
}

fn stamped_version(directory: &TestDirectory) -> String {
    fs::read_to_string(directory.path().join("VERSION"))
        .expect("the VERSION marker is readable")
        .trim()
        .to_string()
}

/// Open a store laid out at `version`, and assert it migrated to 10 with the
/// same folded state and byte-identical records.
fn migrates(version: &str) {
    let directory = TestDirectory::create(&format!("from-{version}"));
    lay_out(&directory, version);
    assert_eq!(
        detect_store_format(directory.path()),
        DetectedStoreFormat::Migratable {
            found: version.to_string()
        },
        "VERSION {version} is not classified as migratable"
    );

    let expected = fold_with(
        load_records(directory.path()).expect("the legacy journal loads"),
        &mut NopSink,
    )
    .expect("the legacy journal folds");
    let before = fs::read(
        directory
            .path()
            .join("journal/seg-00000000000000000000.jsonl"),
    )
    .expect("the segment is readable");

    let store = Store::open(directory.path()).expect("a migratable store opens");
    assert!(
        store_states_eq(&expected, &store.state),
        "VERSION {version} folded to a different state after migration"
    );
    assert_eq!(
        stamped_version(&directory),
        STORE_VERSION,
        "VERSION {version} did not stamp the current version"
    );
    drop(store);

    let after = fs::read(
        directory
            .path()
            .join("journal/seg-00000000000000000000.jsonl"),
    )
    .expect("the segment is readable");
    assert_eq!(
        before, after,
        "VERSION {version} rewrote journal bytes during migration"
    );
}

#[test]
fn a_version_1_store_migrates() {
    migrates("1");
}

#[test]
fn a_version_2_store_migrates() {
    migrates("2");
}

#[test]
fn a_version_3_store_migrates() {
    migrates("3");
}

#[test]
fn a_version_4_store_migrates() {
    migrates("4");
}

#[test]
fn a_version_5_store_migrates() {
    migrates("5");
}

#[test]
fn a_version_6_store_migrates() {
    migrates("6");
}

#[test]
fn a_version_7_store_migrates() {
    migrates("7");
}

#[test]
fn a_version_8_store_migrates() {
    migrates("8");
}

#[test]
fn a_version_9_store_migrates_by_stamping_and_nothing_else() {
    // The arm that does no work. A pre-10 store has no seal record and no base
    // state file, so there is nothing to convert — and saying so with its own
    // named case is what stops a later reader from assuming the arm was left
    // unfinished.
    migrates("9");
}

#[test]
fn a_version_10_store_with_no_seal_and_no_base_opens_normally() {
    // The common case after this plan, and the one a reader will assume needs
    // a base file: the version moved on first write, and nothing was archived.
    let directory = TestDirectory::create("current");
    lay_out(&directory, STORE_VERSION);
    assert_eq!(
        detect_store_format(directory.path()),
        DetectedStoreFormat::Current
    );
    let store = Store::open(directory.path()).expect("an unsealed VERSION 10 store opens");
    assert!(!store.state.instances.is_empty());
    assert!(
        !directory.path().join("journal/BASE").exists(),
        "an unsealed store must have no base state file"
    );
}

#[test]
fn a_version_11_store_is_refused_and_nothing_is_written() {
    let directory = TestDirectory::create("future");
    lay_out(&directory, "11");
    assert_eq!(
        detect_store_format(directory.path()),
        DetectedStoreFormat::Incompatible {
            found: "11".to_string()
        }
    );
    let before: Vec<_> = fs::read_dir(directory.path().join("journal"))
        .expect("the journal directory is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    let error = match Store::open(directory.path()) {
        Ok(_) => panic!("a future version was opened"),
        Err(error) => error,
    };
    assert_eq!(error.code, "store/version_mismatch");
    assert_eq!(stamped_version(&directory), "11", "a refusal restamped");
    let after: Vec<_> = fs::read_dir(directory.path().join("journal"))
        .expect("the journal directory is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name())
        .collect();
    assert_eq!(before, after, "a refusal wrote into the journal");
}

#[test]
fn a_store_migrated_from_9_can_then_be_sealed() {
    // The two paths compose: migration stamps, and the stamped store seals.
    let directory = TestDirectory::create("migrate-then-seal");
    lay_out(&directory, "9");
    let archive = directory.path().join("archive");
    fs::create_dir_all(&archive).expect("the archive directory is creatable");

    let mut store = Store::open(directory.path()).expect("the migrated store opens");
    assert_eq!(stamped_version(&directory), STORE_VERSION);
    // Settle everything the legacy journal left pending, so nothing pins the
    // cut; the property under test is that migration and sealing compose.
    let pending: Vec<(String, Vec<String>)> = store
        .state
        .instances
        .iter()
        .map(|(id, instance)| (id.clone(), instance.pending.clone()))
        .collect();
    for (instance_id, effects) in pending {
        for (index, effect_id) in effects.iter().enumerate() {
            store
                .ack_effect(
                    &instance_id,
                    effect_id,
                    &format!("ack-{instance_id}-{index}"),
                )
                .expect("ack succeeds");
        }
    }
    let report = store
        .seal_and_archive(&archive, None)
        .expect("a migrated store seals");
    assert!(report.records_sealed > 0);
    assert!(
        fsm_store::archive::manifest_path(&archive).is_file(),
        "the seal wrote no manifest"
    );
}

#[test]
fn the_snapshot_cache_is_ignored_during_migration() {
    // A stale cache never becomes authoritative: migration folds the complete
    // journal with caches ignored, and this asserts the fold won.
    let directory = TestDirectory::create("stale-cache");
    lay_out(&directory, "8");
    let snapshots = directory.path().join("snapshots");
    fs::create_dir_all(&snapshots).expect("the snapshot directory is creatable");
    fs::write(
        snapshots.join("snap-00000000000000000001.json"),
        br#"{"format":"fsm.snapshot/5"}"#,
    )
    .expect("the stale cache is writable");

    let expected = fold_with(
        load_records(directory.path()).expect("the legacy journal loads"),
        &mut NopSink,
    )
    .expect("the legacy journal folds");
    let store = Store::open(directory.path()).expect("a store beside a stale cache opens");
    assert!(!store.opened_from_snapshot, "a stale cache was trusted");
    assert!(store_states_eq(&expected, &store.state));
}

#[test]
fn the_spec_names_the_current_store_version() {
    // The version is API: a reader who trusts SPEC.md and finds another number
    // in the code has been told the wrong thing about which builds interoperate.
    let spec = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/SPEC.md"))
        .expect("SPEC.md is readable");
    assert!(
        spec.contains(&format!("store `VERSION` is `{STORE_VERSION}`")),
        "SPEC.md does not name store VERSION {STORE_VERSION}"
    );
    // And the legacy journal is still parseable, so the fixture this suite
    // depends on has not silently drifted.
    assert!(
        parse(
            LEGACY_JOURNAL.split(|byte| *byte == b'\n').next().unwrap(),
            &JsonLimits::DEFAULT
        )
        .is_ok_and(|record| record.get("kind").and_then(Value::as_str) == Some("genesis"))
    );
}
