//! Sealing a prefix: what the operation writes, what it refuses, and what a
//! store looks like when it is interrupted at each step.
//!
//! Plan 0017 task 8002. The interruption cases here cover the store-level
//! shape of each step; the kill-and-recover harness that drives a real process
//! is task 8301's.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_store::archive::{MANIFEST_FILE, manifest_path, read_manifest, verify};
use fsm_store::snapshot::store_states_eq;
use fsm_store::store::{BASE_FILE, Store};

const CASE_REVIEW: &[u8] =
    include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

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
        let path =
            std::env::temp_dir().join(format!("fsm-archive-op-{tag}-{}-{index}", invocation_tag()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
        Self(path)
    }

    fn store(&self) -> PathBuf {
        let path = self.0.join("store");
        fs::create_dir_all(&path).expect("the store directory is creatable");
        path
    }

    fn archive(&self, tag: &str) -> PathBuf {
        let path = self.0.join(format!("archive-{tag}"));
        fs::create_dir_all(&path).expect("the archive directory is creatable");
        path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn definition() -> Value {
    parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses")
}

/// A store with `live` running instances and `settled` cancelled ones.
fn populated(path: &Path, live: usize, settled: usize) -> Store {
    let mut store = Store::open(path).expect("a fresh store opens");
    store
        .define_machine(definition(), false, false)
        .expect("the machine is definable");
    for index in 0..live {
        let id = format!("live-{index}");
        store
            .create_instance("case_review", &id, &format!("create-{id}"), None)
            .expect("create succeeds");
        store
            .send_event(
                &id,
                "docs_ok",
                Value::Obj(BTreeMap::new()),
                &format!("send-{id}"),
                None,
            )
            .expect("send succeeds");
        // The committed example machine emits an effect on this transition, and
        // an unacked effect pins the cut. Acking it is what an executor would
        // have done, and it is what leaves the head sealable.
        let pending: Vec<String> = store.state.instances[&id].pending.clone();
        for (k, effect_id) in pending.iter().enumerate() {
            store
                .ack_effect(&id, effect_id, &format!("ack-{id}-{k}"))
                .expect("ack succeeds");
        }
    }
    for index in 0..settled {
        let id = format!("settled-{index}");
        store
            .create_instance("case_review", &id, &format!("create-{id}"), None)
            .expect("create succeeds");
        store
            .cancel_instance(&id, &format!("cancel-{id}"))
            .expect("cancel succeeds");
    }
    store
}

fn journal_dir(store_path: &Path) -> PathBuf {
    store_path.join("journal")
}

fn segment_names(store_path: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(journal_dir(store_path))
        .expect("the journal directory is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("seg-"))
        .collect();
    names.sort();
    names
}

#[test]
fn a_successful_seal_writes_a_manifest_a_base_and_a_seal_record() {
    let directory = TestDirectory::create("success");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 2, 3);
    let before = store.state.clone();

    let report = store
        .seal_and_archive(&archive, None)
        .expect("a store with nothing pending seals");

    assert_eq!(report.sealed_through_seq, report.records_sealed - 1);
    assert!(report.archive_id.starts_with("sha256:"));
    assert_eq!(report.seal_record_seq, Some(report.sealed_through_seq + 1));

    // The archive verifies as a chain ending in the hash the seal committed.
    let manifest = verify(&archive).expect("the written archive verifies");
    assert_eq!(manifest.sealed_through_seq, report.sealed_through_seq);
    assert_eq!(manifest.sealed_last_hash, report.sealed_last_hash);
    assert_eq!(manifest.archive_id(), report.archive_id);

    // The base file is there and its roots are the ones the seal committed.
    let base_path = journal_dir(&store_path).join(BASE_FILE);
    assert!(base_path.is_file(), "the seal wrote no base state file");

    // The live journal begins with the seal record and holds no sealed segment.
    let seal_record = store
        .records
        .iter()
        .find(|record| record.kind == RecordKind::JournalSealed)
        .expect("the seal record was appended");
    assert_eq!(
        seal_record
            .body
            .get("sealed_through_seq")
            .and_then(Value::as_num),
        Some(report.sealed_through_seq.to_string().as_str())
    );
    assert_eq!(
        seal_record
            .body
            .get("sealed_last_hash")
            .and_then(Value::as_str),
        Some(format!("sha256:{}", report.sealed_last_hash).as_str())
    );
    for name in segment_names(&store_path) {
        let first: u64 = name
            .strip_prefix("seg-")
            .and_then(|rest| rest.strip_suffix(".jsonl"))
            .and_then(|digits| digits.parse().ok())
            .expect("a segment name carries its first sequence");
        assert!(
            first > report.sealed_through_seq,
            "sealed segment {name} is still in the live journal"
        );
    }

    // The fold is unchanged by sealing: the same machines, instances, and
    // instance-machine bindings, whatever moved on disk.
    assert_eq!(store.state.machines.len(), before.machines.len());
    assert_eq!(store.state.instances, before.instances);
    assert_eq!(store.state.instance_machines, before.instance_machines);
}

#[test]
fn the_operation_creates_its_own_segment_final_checkpoint() {
    // Nothing in the store produces a sequence that is both a checkpoint and
    // the last record of a segment by chance, so the operation makes one.
    let directory = TestDirectory::create("checkpoint");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 1, 0);
    // A sealed store's handle holds only the live suffix, exactly as a
    // reopened one does, so the archived cut record is looked up in the record
    // set as it stood before the seal.
    let before_seal = store.records.clone();
    let report = store
        .seal_and_archive(&archive, None)
        .expect("the seal runs");

    let manifest = read_manifest(&archive).expect("the manifest is readable");
    let last_segment = manifest
        .segments
        .last()
        .expect("the archive holds at least one segment");
    assert_eq!(
        last_segment.last_seq, report.sealed_through_seq,
        "the cut is not the last record of its segment"
    );
    // The checkpoint is created *by* the seal and archived by it, so it is in
    // neither the pre-seal record set nor the live suffix. It is in the
    // archive, which is where this asserts it — a stronger claim than reading
    // it out of memory, because it proves the bytes moved.
    assert!(
        !before_seal
            .iter()
            .any(|record| record.seq == report.sealed_through_seq),
        "the cut already existed, so the operation did not create it"
    );
    let archived = fs::read_to_string(archive.join(&last_segment.name))
        .expect("the archived segment is readable");
    let last_line = archived
        .lines()
        .next_back()
        .expect("the archived segment has a line");
    let cut_record =
        parse(last_line.as_bytes(), &JsonLimits::DEFAULT).expect("the archived record parses");
    assert_eq!(
        cut_record.get("kind").and_then(Value::as_str),
        Some("state_checkpoint"),
        "the cut is not a state_checkpoint"
    );
    assert_eq!(
        cut_record.get("seq").and_then(Value::as_num),
        Some(report.sealed_through_seq.to_string().as_str()),
        "the archive's last record is not the cut"
    );
    assert!(
        !store
            .records
            .iter()
            .any(|record| record.seq <= report.sealed_through_seq),
        "the handle still holds records the seal moved into the archive"
    );
    // The seal landed in the fresh segment the rotation opened.
    assert_eq!(segment_names(&store_path).len(), 1);
}

#[test]
fn an_expect_cut_that_names_another_sequence_is_refused_and_writes_nothing() {
    let directory = TestDirectory::create("expect");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 1, 0);
    let before = segment_names(&store_path);
    let error = store
        .seal_and_archive(&archive, Some(1))
        .expect_err("a stale assertion is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert!(
        error.hint.contains("--dry-run") || error.hint.contains("preview"),
        "the hint does not point at the preview: {}",
        error.hint
    );
    assert_eq!(segment_names(&store_path), before, "a refusal rotated");
    assert!(
        !manifest_path(&archive).exists(),
        "a refusal wrote a manifest"
    );

    // The sequence the preview names is accepted.
    let preview = store.preview_seal(None).expect("the preview runs");
    store
        .seal_and_archive(&archive, Some(preview.sealed_through_seq))
        .expect("the sequence the preview named is accepted");
}

#[test]
fn a_second_seal_into_the_same_archive_directory_is_refused() {
    let directory = TestDirectory::create("twice");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 1, 0);
    store
        .seal_and_archive(&archive, None)
        .expect("the first seal runs");
    let error = store
        .seal_and_archive(&archive, None)
        .expect_err("a second seal into one archive is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert!(error.message.contains(MANIFEST_FILE));
}

#[test]
fn two_seals_to_two_archives_leave_a_store_sealed_at_the_later_cut() {
    let directory = TestDirectory::create("sequential");
    let store_path = directory.store();
    let mut store = populated(&store_path, 1, 0);
    let first = store
        .seal_and_archive(&directory.archive("one"), None)
        .expect("the first seal runs");
    store
        .annotate("live-0", "after-first-seal", "a note after the first seal")
        .expect("the store still writes after a seal");
    let second = store
        .seal_and_archive(&directory.archive("two"), None)
        .expect("the second seal runs");
    assert!(second.sealed_through_seq > first.sealed_through_seq);
    // The second archive's chain starts where the first one's ended.
    let second_manifest = verify(&directory.archive("two")).expect("the second archive verifies");
    assert_eq!(second_manifest.first_seq, first.sealed_through_seq + 1);
}

#[test]
fn a_second_seal_on_a_reopened_store_keeps_every_machine_and_instance() {
    // The bug this pins: after one seal the live journal is a *suffix*, so a
    // second seal that folded it from an empty state would write a base with
    // no machines and no instances — and because a `journal_sealed` record
    // applies as a no-op and no fold checks for genesis, that fold *succeeds*.
    // The roots would then be committed over the empty file, the next open
    // would recompute them and agree, and a year of history would be gone with
    // nothing reporting an error. Reopening between the seals is the whole
    // point: one handle keeps the full record set in memory and hides it.
    let directory = TestDirectory::create("reopen-seal");
    let store_path = directory.store();
    let first = {
        let mut store = populated(&store_path, 2, 1);
        store
            .seal_and_archive(&directory.archive("one"), None)
            .expect("the first seal runs")
    };

    let mut reopened = Store::open(&store_path).expect("a sealed store reopens");
    let machines_before = reopened.state.machines.len();
    let instances_before = reopened.state.instances.len();
    assert!(machines_before > 0 && instances_before > 0);
    reopened
        .annotate("live-0", "between-seals", "a note between the two seals")
        .expect("the store still writes after a seal");
    let second = reopened
        .seal_and_archive(&directory.archive("two"), None)
        .expect("a second seal on a reopened store runs");
    assert!(second.sealed_through_seq > first.sealed_through_seq);
    drop(reopened);

    let twice = Store::open(&store_path).expect("a twice-sealed store reopens");
    assert_eq!(
        twice.state.machines.len(),
        machines_before,
        "the second seal dropped machines from the base"
    );
    assert_eq!(
        twice.state.instances.len(),
        instances_before,
        "the second seal dropped instances from the base"
    );
    assert!(twice.state.instances.contains_key("live-0"));
}

#[test]
fn a_second_seal_carries_the_historical_definition_ceiling_forward() {
    // The genesis record holds the discriminator and genesis is below every
    // cut, so after one seal only the base can answer. A second seal that
    // asked the journal instead would write `current` and the next open would
    // recompile machines admitted under the old ceiling.
    let directory = TestDirectory::create("limits-forward");
    let store_path = directory.store();
    {
        let mut store = populated(&store_path, 1, 0);
        store
            .seal_and_archive(&directory.archive("one"), None)
            .expect("the first seal runs");
    }
    let before = fsm_store::base::read_header(&store_path)
        .expect("the base header reads")
        .expect("a sealed store has a base");
    let mut reopened = Store::open(&store_path).expect("a sealed store reopens");
    reopened
        .annotate("live-0", "between-seals", "a note between the two seals")
        .expect("the store still writes after a seal");
    reopened
        .seal_and_archive(&directory.archive("two"), None)
        .expect("a second seal runs");
    drop(reopened);
    let after = fsm_store::base::read_header(&store_path)
        .expect("the base header reads")
        .expect("a twice-sealed store has a base");
    assert_eq!(
        after.definition_limits, before.definition_limits,
        "the second seal changed the ceiling its machines were admitted under"
    );
}

#[test]
fn a_preview_on_a_sealed_store_counts_only_the_records_it_would_archive() {
    // `cut + 1` is the record count only while the archive starts at the
    // origin. On a store sealed once it over-reports by everything the first
    // archive already took.
    let directory = TestDirectory::create("preview-count");
    let store_path = directory.store();
    {
        let mut store = populated(&store_path, 1, 0);
        store
            .seal_and_archive(&directory.archive("one"), None)
            .expect("the first seal runs");
    }
    let mut reopened = Store::open(&store_path).expect("a sealed store reopens");
    for index in 0..3 {
        reopened
            .annotate("live-0", &format!("note-{index}"), "a note")
            .expect("the store still writes after a seal");
    }
    let preview = reopened.preview_seal(None).expect("the preview runs");
    let run = reopened
        .seal_and_archive(&directory.archive("two"), Some(preview.sealed_through_seq))
        .expect("the second seal runs");
    assert_eq!(
        preview.records_sealed, run.records_sealed,
        "the preview and the run disagree about how many records move"
    );
    assert!(
        preview.records_sealed <= preview.sealed_through_seq,
        "the preview counted records an earlier archive already holds"
    );
}

#[test]
fn a_read_only_store_is_refused_and_the_archive_directory_stays_empty() {
    let directory = TestDirectory::create("readonly");
    let store_path = directory.store();
    let archive = directory.archive("one");
    drop(populated(&store_path, 1, 0));
    let mut store = Store::open_read_only(&store_path).expect("a read-only store opens");
    let error = store
        .seal_and_archive(&archive, None)
        .expect_err("a read-only store cannot seal");
    assert_eq!(error.code, "io/write");
    assert_eq!(
        fs::read_dir(&archive)
            .expect("the archive directory is listable")
            .count(),
        0,
        "a refused seal wrote into the archive"
    );
}

#[test]
fn a_missing_archive_directory_is_refused_rather_than_created() {
    // An operator who mistypes a path should not discover a new directory
    // holding their history.
    let directory = TestDirectory::create("absent");
    let store_path = directory.store();
    let missing = directory.0.join("not-created");
    let mut store = populated(&store_path, 1, 0);
    let error = store
        .seal_and_archive(&missing, None)
        .expect_err("a missing archive directory is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert!(!missing.exists(), "the refusal created the directory");
}

#[test]
fn a_pending_effect_seals_the_whole_segments_below_it_rather_than_refusing() {
    // A live store almost always has an effect in flight — the executor settles
    // each within a tick — so a rule that only ever cut at the head would
    // refuse every seal a running store asked for. It seals as many whole
    // segments as the pin allows instead, which is the useful answer.
    let directory = TestDirectory::create("boundary");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 1, 1);
    // Close a segment, so a boundary exists below whatever comes next.
    store
        .journal
        .force_rotate()
        .expect("the journal rotates on demand");
    let boundary = store.state.last_seq;
    store
        .create_instance("case_review", "pending-holder", "create-pending", None)
        .expect("create succeeds");
    store
        .send_event(
            "pending-holder",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send-pending",
            None,
        )
        .expect("send succeeds");
    assert!(
        !store.state.instances["pending-holder"].pending.is_empty(),
        "the case needs an unacked effect to pin the cut"
    );

    let before_seal = store.records.clone();
    let report = store
        .seal_and_archive(&archive, None)
        .expect("a pinned store seals the segments below the pin");
    assert_eq!(
        report.sealed_through_seq, boundary,
        "the seal did not stop at the segment boundary the pin allowed"
    );
    // No checkpoint was created: the cut already existed.
    let cut_record = before_seal
        .iter()
        .find(|record| record.seq == report.sealed_through_seq)
        .expect("the cut record was in the journal the seal read");
    assert_ne!(cut_record.kind, RecordKind::StateCheckpoint);
    // The seal is not adjacent to the prefix it sealed, and that is legal.
    let seal_record = store
        .records
        .iter()
        .find(|record| record.kind == RecordKind::JournalSealed)
        .expect("the seal record was appended");
    assert!(
        seal_record.seq > report.sealed_through_seq + 1,
        "the cut was at the head after all, so this case proved nothing"
    );
    verify(&archive).expect("the archive of a partial prefix verifies");
}

#[test]
fn a_pending_effect_with_no_boundary_below_it_refuses_and_names_what_would_clear_it() {
    // The pin: the executor recovers a pending effect's arguments and attempt
    // count by reading records, so the records it needs cannot be archived.
    // One segment, so there is no boundary to fall back to and the honest
    // answer is a refusal that names the effect standing in the way.
    let directory = TestDirectory::create("pinned");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 1, 0);
    store
        .create_instance("case_review", "pending-holder", "create-pending", None)
        .expect("create succeeds");
    store
        .send_event(
            "pending-holder",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send-pending",
            None,
        )
        .expect("send succeeds");
    let error = store
        .seal_and_archive(&archive, None)
        .expect_err("a pending effect pins the cut");
    assert_eq!(error.code, "store/archive_refused");
    assert!(
        error.details.get("effect_id").is_some(),
        "the refusal does not name the effect: {error:?}"
    );
    assert!(
        !manifest_path(&archive).exists(),
        "a pinned refusal wrote a manifest"
    );
}

#[test]
fn a_preview_writes_nothing_and_appends_no_checkpoint() {
    let directory = TestDirectory::create("preview");
    let store_path = directory.store();
    let store = populated(&store_path, 2, 1);
    let before_records = store.records.len();
    let before_segments = segment_names(&store_path);
    let report = store.preview_seal(None).expect("the preview runs");
    assert_eq!(report.seal_record_seq, None);
    assert_eq!(report.sealed_through_seq, store.state.last_seq + 1);
    assert!(report.keys_carried + report.keys_dropped > 0);
    assert_eq!(store.records.len(), before_records, "a preview appended");
    assert_eq!(
        segment_names(&store_path),
        before_segments,
        "a preview rotated"
    );
}

#[test]
fn a_snapshot_cache_at_or_below_the_seal_is_dropped() {
    let directory = TestDirectory::create("snapshots");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 1, 0);
    store
        .shutdown_snapshot()
        .expect("a shutdown snapshot is writable");
    assert!(
        !fsm_store::snapshot::listed_snaps(&store_path).is_empty(),
        "the test needs a snapshot to exist"
    );
    let report = store
        .seal_and_archive(&archive, None)
        .expect("the seal runs");
    for (seq, path) in fsm_store::snapshot::listed_snaps(&store_path) {
        assert!(
            seq > report.sealed_through_seq,
            "a cache at seq {seq} survived a seal through {} at {path:?}",
            report.sealed_through_seq
        );
    }
}

// ---------------------------------------------------------------------------
// Interruption: every prefix of the ordering leaves a store that opens
// ---------------------------------------------------------------------------

/// Re-open the store, fold it, and return the folded state.
fn reopen(store_path: &Path) -> Store {
    Store::open(store_path).expect("the store opens after an interruption")
}

#[test]
fn an_interruption_before_the_commit_point_leaves_an_unsealed_store() {
    // Steps 4 through 6 write a manifest, copies, and a base. None of them is
    // referenced by the chain, so the store is exactly what it was and a re-run
    // overwrites them.
    for (name, leave) in [("manifest", 1usize), ("copies", 2), ("base", 3)] {
        let directory = TestDirectory::create(&format!("pre-commit-{name}"));
        let store_path = directory.store();
        let archive = directory.archive("one");
        let expected = {
            let store = populated(&store_path, 1, 1);
            store.state.clone()
        };
        // Simulate the interruption by writing the artifacts the step would
        // have left and then not continuing.
        fs::write(archive.join("MANIFEST.partial"), b"{}\n").expect("writable");
        if leave >= 2 {
            fs::write(archive.join("seg-00000000000000000000.jsonl"), b"").expect("writable");
        }
        if leave >= 3 {
            fs::write(journal_dir(&store_path).join("BASE.tmp"), b"{}\n").expect("writable");
        }
        let store = reopen(&store_path);
        assert!(
            store_states_eq(&expected, &store.state),
            "an interrupted seal changed the folded state at step {name}"
        );
        assert!(
            !journal_dir(&store_path).join(BASE_FILE).is_file(),
            "an interrupted seal left a BASE the chain does not reference"
        );
        // A re-run completes, whatever the interrupted attempt left behind.
        let mut store = store;
        store
            .seal_and_archive(&directory.archive("clean"), None)
            .expect("a re-run after an interruption completes");
    }
}

#[test]
fn a_store_interrupted_after_the_commit_point_opens_sealed_and_finishes_on_a_re_run() {
    // Between step 7 and step 8 the seal record exists and the copied segments
    // are still in the live journal. The store is sealed; the removal is what
    // is outstanding, and the records are readable from the archive either way.
    let directory = TestDirectory::create("post-commit");
    let store_path = directory.store();
    let archive = directory.archive("one");
    let mut store = populated(&store_path, 1, 1);
    let report = store
        .seal_and_archive(&archive, None)
        .expect("the seal runs");
    drop(store);

    // Put a sealed segment back, as an interruption between 7 and 8 would.
    let manifest = read_manifest(&archive).expect("the manifest is readable");
    let restored = &manifest.segments[0];
    fs::copy(
        archive.join(&restored.name),
        journal_dir(&store_path).join(&restored.name),
    )
    .expect("the segment is restorable");

    // The sealed records are still readable from the archive, which is the
    // assertion a live-store-only check would miss: an implementation that
    // removed a segment it had not successfully copied passes everything else.
    verify(&archive).expect("the archive still verifies");

    let store = reopen(&store_path);
    assert!(
        store
            .records
            .iter()
            .any(|record| record.kind == RecordKind::JournalSealed),
        "a store interrupted after the commit point does not read as sealed"
    );
    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::JournalSealed)
            .count(),
        1,
        "more than one seal record survived"
    );
    let _ = report;
}
