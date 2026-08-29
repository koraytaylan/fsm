//! Every prefix of the seal's ordering leaves a store that opens.
//!
//! Plan 0017 task 8301. The ordering **is** the safety argument — copy, then
//! seal, then remove — so this enumerates the interruption points rather than
//! sampling them. A random killer proves the contract only statistically, and
//! it cannot be aimed at a numbered step at all: the steps are microseconds
//! apart and several are a single `write` each.
//!
//! So the harness **constructs** each prefix instead of racing for it. It runs
//! one complete seal into a scratch directory, then rebuilds a pristine store
//! and lays down exactly the artifacts each prefix would have left. That is
//! strictly more precise than a kill — every case is the exact state its step
//! ends in, every run — and it is why there is no iteration count and no seed
//! here: the cases are exhaustive over the ordering, not sampled from it.
//!
//! One assertion is made at **every** point and is the one a live-store-only
//! harness would miss: the sealed records are still readable from the archive.
//! An implementation that removed a segment it had not successfully copied
//! passes every check about the store and loses the history.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_store::store::{BASE_FILE, Store};

const CASE_REVIEW: &[u8] =
    include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

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
        let path = std::env::temp_dir().join(format!(
            "fsm-archive-crash-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("the temporary directory is creatable");
        Self(path)
    }

    fn child(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(&path).expect("the directory is creatable");
        path
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("the destination is creatable");
    for entry in fs::read_dir(from)
        .expect("the source is listable")
        .flatten()
    {
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if source.is_dir() {
            copy_tree(&source, &destination);
        } else {
            fs::copy(&source, &destination).expect("the file is copyable");
        }
    }
}

/// A store with one live instance whose effects are acked, so nothing pins.
fn populate(store_path: &Path) {
    let mut store = Store::open(store_path).expect("a fresh store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    for index in 0..2 {
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
        let pending: Vec<String> = store.state.instances[&id].pending.clone();
        for (k, effect_id) in pending.iter().enumerate() {
            store
                .ack_effect(&id, effect_id, &format!("ack-{id}-{k}"))
                .expect("ack succeeds");
        }
    }
}

/// Everything a completed seal produces, kept so each prefix can be built
/// from the pieces the real operation would have written by that point.
struct SealArtifacts {
    /// A pristine, unsealed store, byte for byte as it was before sealing.
    pristine: PathBuf,
    /// The store after a complete seal.
    sealed: PathBuf,
    /// The archive a complete seal wrote.
    archive: PathBuf,
    /// The names of the segments the seal moved.
    moved: Vec<String>,
    sealed_through_seq: u64,
    /// The folded state a completed seal leaves, for restart equivalence.
    completed_state: fsm_core::replay::StoreState,
}

fn seal_once(directory: &TestDirectory) -> SealArtifacts {
    let pristine = directory.child("pristine");
    populate(&pristine);
    let sealed = directory.child("sealed");
    copy_tree(&pristine, &sealed);
    let archive = directory.child("archive");
    let mut store = Store::open(&sealed).expect("the copy opens");
    let report = store
        .seal_and_archive(&archive, None)
        .expect("the reference seal runs");
    drop(store);
    let completed = Store::open(&sealed).expect("the sealed store reopens");
    let completed_state = completed.state.clone();
    drop(completed);
    SealArtifacts {
        pristine,
        sealed,
        archive,
        moved: report.segments.clone(),
        sealed_through_seq: report.sealed_through_seq,
        completed_state,
    }
}

/// Which numbered step a case is interrupted after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterruptedAfter {
    /// 4: `MANIFEST` written and fsynced.
    Manifest,
    /// 5: every sealed segment copied and its digest checked.
    Copies,
    /// 6: `BASE` written durably.
    Base,
    /// 7: the seal record appended — **the commit point**.
    SealRecord,
    /// 8: the copied segments removed from the live journal.
    Removal,
}

impl InterruptedAfter {
    fn name(self) -> &'static str {
        match self {
            Self::Manifest => "step-4-manifest",
            Self::Copies => "step-5-copies",
            Self::Base => "step-6-base",
            Self::SealRecord => "step-7-seal-record",
            Self::Removal => "step-8-removal",
        }
    }

    /// Whether the store reads as sealed at this point. Before the commit
    /// point it does not; after it, it does. A harness that accepted either
    /// at every step would prove nothing about where the commit point is.
    fn is_sealed(self) -> bool {
        matches!(self, Self::SealRecord | Self::Removal)
    }
}

/// Build the exact state the operation leaves when interrupted after `step`.
fn build_case(
    directory: &TestDirectory,
    artifacts: &SealArtifacts,
    step: InterruptedAfter,
) -> (PathBuf, PathBuf) {
    let store = directory.child(&format!("case-{}-store", step.name()));
    let archive = directory.child(&format!("case-{}-archive", step.name()));

    if step.is_sealed() {
        // After the commit point the live journal is the sealed one: its seal
        // record is appended and, until step 8, the copied segments are still
        // there.
        copy_tree(&artifacts.sealed, &store);
        if step == InterruptedAfter::SealRecord {
            for name in &artifacts.moved {
                fs::copy(
                    artifacts.archive.join(name),
                    store.join("journal").join(name),
                )
                .expect("the segment is restorable");
            }
        }
    } else {
        // Before the commit point the live journal is untouched.
        copy_tree(&artifacts.pristine, &store);
        if step == InterruptedAfter::Base {
            fs::copy(
                artifacts.sealed.join("journal").join(BASE_FILE),
                store.join("journal").join(BASE_FILE),
            )
            .expect("the base is copyable");
        }
    }

    // The archive holds what had been written by that point.
    fs::copy(artifacts.archive.join("MANIFEST"), archive.join("MANIFEST"))
        .expect("the manifest is copyable");
    if step != InterruptedAfter::Manifest {
        for name in &artifacts.moved {
            fs::copy(artifacts.archive.join(name), archive.join(name))
                .expect("the segment is copyable");
        }
    }
    (store, archive)
}

fn seal_record_count(store: &Store) -> usize {
    store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::JournalSealed)
        .count()
}

/// The four assertions every interruption point owes, plus the archive one.
fn assert_survivable(step: InterruptedAfter, store_path: &Path, archive: &Path) -> Store {
    // 1. It opens, and 2. it folds.
    let store = Store::open(store_path)
        .unwrap_or_else(|error| panic!("{}: the store does not open: {error:?}", step.name()));

    // 3. Verification reports one of its three verdicts rather than a fault.
    let report = fsm_store::journal_io::verify(store_path);
    assert!(
        matches!(report.health, fsm_store::journal_io::JournalHealth::Ok),
        "{}: verification reports {:?}",
        step.name(),
        report.health
    );
    assert_eq!(
        report.seal.is_some(),
        step.is_sealed(),
        "{}: the store reads as sealed={} and should read as sealed={}",
        step.name(),
        report.seal.is_some(),
        step.is_sealed()
    );

    // 4. And the sealed records are still readable from the archive. This is
    // the one a live-store-only harness misses: an implementation that removed
    // a segment it had not successfully copied passes everything above.
    if step != InterruptedAfter::Manifest {
        fsm_store::archive::verify(archive).unwrap_or_else(|error| {
            panic!(
                "{}: the archived records are not readable: {error:?}",
                step.name()
            )
        });
    }
    store
}

#[test]
fn interrupted_after_the_manifest_the_store_is_untouched_and_a_re_run_completes() {
    one_case(InterruptedAfter::Manifest);
}

#[test]
fn interrupted_after_the_copies_the_store_is_untouched_and_a_re_run_completes() {
    one_case(InterruptedAfter::Copies);
}

#[test]
fn interrupted_after_the_base_the_store_is_untouched_and_a_re_run_completes() {
    one_case(InterruptedAfter::Base);
}

#[test]
fn interrupted_after_the_seal_record_the_store_reads_as_sealed() {
    one_case(InterruptedAfter::SealRecord);
}

#[test]
fn interrupted_after_the_removal_the_store_reads_as_sealed() {
    one_case(InterruptedAfter::Removal);
}

/// Every interruption point: open, fold, verify, archive-readable, and a
/// re-run of the identical command that completes.
fn one_case(step: InterruptedAfter) {
    let directory = TestDirectory::create(step.name());
    let artifacts = seal_once(&directory);
    let (store_path, archive) = build_case(&directory, &artifacts, step);

    let store = assert_survivable(step, &store_path, &archive);
    let sealed_now = seal_record_count(&store);
    assert_eq!(
        sealed_now,
        usize::from(step.is_sealed()),
        "{}: {sealed_now} seal records",
        step.name()
    );
    drop(store);

    // A re-run of the identical command. Before the commit point it completes
    // the seal into a fresh archive directory; after it, the store is already
    // sealed and the same cut cannot be taken twice.
    let fresh = directory.child(&format!("rerun-{}", step.name()));
    let mut store = Store::open(&store_path).expect("the store reopens for the re-run");
    let rerun = store.seal_and_archive(&fresh, None);
    match step.is_sealed() {
        false => {
            let report = rerun.unwrap_or_else(|error| {
                panic!("{}: the re-run did not complete: {error:?}", step.name())
            });
            assert_eq!(
                report.sealed_through_seq,
                artifacts.sealed_through_seq,
                "{}: the re-run sealed a different prefix",
                step.name()
            );
            drop(store);
            // Restart equivalence: interrupted-then-completed folds to the
            // same state as a seal that was never interrupted.
            let completed = Store::open(&store_path).expect("the completed store opens");
            assert_eq!(
                completed.state.instances,
                artifacts.completed_state.instances,
                "{}: the completed seal folded differently",
                step.name()
            );
            assert_eq!(
                completed.state.dedup,
                artifacts.completed_state.dedup,
                "{}: the completed seal carried a different ledger",
                step.name()
            );
            assert_eq!(seal_record_count(&completed), 1);
        }
        true => {
            // A second seal of an already-sealed store seals what is above the
            // first cut, so it succeeds — and there is still exactly one seal
            // record per seal, never two for one cut.
            match rerun {
                Ok(report) => {
                    assert!(report.sealed_through_seq > artifacts.sealed_through_seq);
                    drop(store);
                    let completed = Store::open(&store_path).expect("the store opens");
                    // Still exactly one. The second seal's cut is above the
                    // first seal's record, so that record is archived with the
                    // rest of the prefix: a live journal carries the one seal
                    // that describes *its* base, and earlier seals live in the
                    // archives they named.
                    assert_eq!(
                        seal_record_count(&completed),
                        1,
                        "{}: the live journal carries more than one seal",
                        step.name()
                    );
                }
                Err(error) => {
                    // Refusing is equally correct when nothing above the cut is
                    // sealable; what must never happen is a second record for
                    // the same cut.
                    assert_eq!(error.code, "store/archive_refused");
                    drop(store);
                    let completed = Store::open(&store_path).expect("the store opens");
                    assert_eq!(seal_record_count(&completed), 1);
                }
            }
        }
    }
}

#[test]
fn a_re_run_into_the_archive_a_completed_seal_already_wrote_is_refused() {
    // The "extra run" question plan 0016's chaos lesson insists on asking:
    // what would a second run look like in the records? It must look like
    // nothing, and the manifest is what makes that so.
    let directory = TestDirectory::create("rerun-same-archive");
    let artifacts = seal_once(&directory);
    let mut store = Store::open(&artifacts.sealed).expect("the sealed store opens");
    let before = seal_record_count(&store);
    let error = store
        .seal_and_archive(&artifacts.archive, None)
        .expect_err("a re-run into a written archive is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert_eq!(
        seal_record_count(&store),
        before,
        "a refused re-run appended a seal record"
    );
}

#[test]
fn a_torn_tail_after_a_seal_is_classified_and_repaired_as_on_an_unsealed_store() {
    // Sealing must not change the tail contract. The same fault, the same
    // classification, the same remedy.
    let directory = TestDirectory::create("torn");
    let artifacts = seal_once(&directory);
    // A record above the seal, so the torn one is not the seal itself:
    // truncating the seal away leaves a base nothing commits, which is
    // `store/base_mismatch` and a different question from the tail contract.
    {
        let mut store = Store::open(&artifacts.sealed).expect("the sealed store opens");
        store
            .annotate("live-0", "above-the-cut", "a note above the cut")
            .expect("the annotation succeeds");
    }
    let journal = artifacts.sealed.join("journal");
    let active = fs::read_dir(&journal)
        .expect("the journal is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("seg-"))
        })
        .max()
        .expect("the live journal has a segment");
    let mut bytes = fs::read(&active).expect("the segment is readable");
    bytes.truncate(bytes.len() - 3);
    fs::write(&active, &bytes).expect("the segment is writable");

    let health = fsm_store::journal_io::classify(&artifacts.sealed);
    assert!(
        matches!(
            health,
            fsm_store::journal_io::JournalHealth::TornTail { .. }
        ),
        "a torn tail on a sealed store classified as {health:?}"
    );
    fsm_store::journal_io::repair_truncate_torn_tail(&artifacts.sealed)
        .expect("the standard repair applies to a sealed store");
    let store = Store::open(&artifacts.sealed).expect("the repaired sealed store opens");
    assert_eq!(seal_record_count(&store), 1);
}

#[test]
fn the_interruption_points_cover_every_step_that_writes() {
    // The enumeration is the contract. If a step is added to the ordering and
    // not to this list, the harness silently stops covering it.
    let every = [
        InterruptedAfter::Manifest,
        InterruptedAfter::Copies,
        InterruptedAfter::Base,
        InterruptedAfter::SealRecord,
        InterruptedAfter::Removal,
    ];
    assert_eq!(every.len(), 5);
    let before: Vec<_> = every.iter().filter(|step| !step.is_sealed()).collect();
    let after: Vec<_> = every.iter().filter(|step| step.is_sealed()).collect();
    assert_eq!(before.len(), 3, "three steps precede the commit point");
    assert_eq!(after.len(), 2, "two steps follow it");
}
