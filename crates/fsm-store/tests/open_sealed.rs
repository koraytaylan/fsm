//! Opening a sealed store: the loader starts from a pair the base declares and
//! the chain confirms, and every way that pair can be wrong is a refusal.
//!
//! Plan 0017 task 8101.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_store::store::{BASE_FILE, SealReport, Store};

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
        let path = std::env::temp_dir().join(format!(
            "fsm-open-sealed-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        for sub in ["store", "archive", "other-store", "other-archive"] {
            fs::create_dir_all(path.join(sub)).expect("the directory is creatable");
        }
        Self(path)
    }

    fn store(&self) -> PathBuf {
        self.0.join("store")
    }

    fn archive(&self) -> PathBuf {
        self.0.join("archive")
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

/// A store with instances in both states and every effect acked, so nothing
/// pins the cut.
fn populated(path: &Path, tag: &str) -> Store {
    let mut store = Store::open(path).expect("a fresh store opens");
    store
        .define_machine(definition(), false, false)
        .expect("the machine is definable");
    for index in 0..2 {
        let id = format!("{tag}-live-{index}");
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
    let settled = format!("{tag}-settled");
    store
        .create_instance("case_review", &settled, &format!("create-{settled}"), None)
        .expect("create succeeds");
    store
        .cancel_instance(&settled, &format!("cancel-{settled}"))
        .expect("cancel succeeds");
    store
}

/// Seal a populated store and return the state it folded to beforehand.
fn sealed(directory: &TestDirectory, tag: &str) -> (SealReport, fsm_core::replay::StoreState) {
    let mut store = populated(&directory.store(), tag);
    let before = store.state.clone();
    let report = store
        .seal_and_archive(&directory.archive(), None)
        .expect("a store with nothing pending seals");
    drop(store);
    (report, before)
}

fn base_path(directory: &TestDirectory) -> PathBuf {
    directory.store().join("journal").join(BASE_FILE)
}

fn rewrite_base(directory: &TestDirectory, edit: impl FnOnce(&mut BTreeMap<String, Value>)) {
    let path = base_path(directory);
    let bytes = fs::read(&path).expect("the base is readable");
    let value = parse(&bytes, &JsonLimits::DEFAULT).expect("the base parses");
    let mut object = value.as_obj().expect("the base is an object").clone();
    edit(&mut object);
    let mut rewritten = canon_bytes(&Value::Obj(object));
    rewritten.push(b'\n');
    fs::write(&path, rewritten).expect("the base is writable");
}

#[test]
fn a_sealed_store_folds_to_the_state_it_folded_to_before_sealing() {
    // The plan's headline property: sealing moves bytes and changes nothing a
    // reader can observe about the store's logical state.
    let directory = TestDirectory::create("headline");
    let (report, before) = sealed(&directory, "a");
    let reopened = Store::open(&directory.store()).expect("a sealed store opens");
    // Machines, instances, and their bindings are preserved exactly. The three
    // fields that legitimately move are the journal position — the seal
    // appended two records — and the ledger, whose dropped keys are the point.
    assert_eq!(before.machines.len(), reopened.state.machines.len());
    assert_eq!(before.instances, reopened.state.instances);
    assert_eq!(before.instance_machines, reopened.state.instance_machines);
    for (request_id, slot) in &reopened.state.dedup {
        assert_eq!(
            before.dedup.get(request_id),
            Some(slot),
            "the base carried {request_id} with different content"
        );
    }
    assert!(reopened.state.last_seq > report.sealed_through_seq);
    // The live suffix is what was loaded — the archived prefix is not in it.
    assert!(
        reopened
            .records
            .iter()
            .all(|record| record.seq > report.sealed_through_seq),
        "an archived record was loaded from the live journal"
    );
}

#[test]
fn a_sealed_store_still_writes_and_reopens() {
    let directory = TestDirectory::create("writes");
    let (_report, _before) = sealed(&directory, "a");
    let mut store = Store::open(&directory.store()).expect("a sealed store opens");
    store
        .annotate("a-live-0", "after-seal", "a note after the seal")
        .expect("a sealed store still writes");
    let seq = store.state.last_seq;
    drop(store);
    let reopened = Store::open(&directory.store()).expect("a sealed store reopens");
    assert_eq!(reopened.state.last_seq, seq);
}

#[test]
fn an_unsealed_store_is_unchanged() {
    // Every path that was not sealed keeps its exact behaviour: the default
    // chain start is the origin, and nothing consults a base that is not there.
    let directory = TestDirectory::create("unsealed");
    let store = populated(&directory.store(), "a");
    let before = store.state.clone();
    drop(store);
    let reopened = Store::open(&directory.store()).expect("an unsealed store opens");
    assert_eq!(before.instances, reopened.state.instances);
    assert_eq!(before.dedup, reopened.state.dedup);
    assert!(!base_path(&directory).exists());
    assert_eq!(reopened.records.first().map(|record| record.seq), Some(0));
}

#[test]
fn a_deleted_base_is_refused_rather_than_folded_from_the_seal() {
    let directory = TestDirectory::create("base-deleted");
    sealed(&directory, "a");
    fs::remove_file(base_path(&directory)).expect("the base is removable");
    let error = match Store::open(&directory.store()) {
        Ok(_) => panic!("a sealed store with no base was opened"),
        Err(error) => error,
    };
    assert_eq!(error.code, "store/base_missing");
}

#[test]
fn a_journal_starting_above_zero_with_no_base_is_base_missing_not_a_seal() {
    // The deleted-segments case. It must never be mistaken for a seal, which is
    // the whole reason this condition has a code of its own.
    let directory = TestDirectory::create("deleted-segments");
    let store = populated(&directory.store(), "a");
    drop(store);
    let journal = directory.store().join("journal");
    // Rotate by hand: move the only segment to a name that starts above zero.
    let entries: Vec<PathBuf> = fs::read_dir(&journal)
        .expect("the journal is listable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("seg-"))
        })
        .collect();
    assert_eq!(entries.len(), 1);
    fs::rename(&entries[0], journal.join("seg-00000000000000000042.jsonl"))
        .expect("the segment is renamable");
    let error = match Store::open(&directory.store()) {
        Ok(_) => panic!("a journal with records deleted out from under it was opened"),
        Err(error) => error,
    };
    assert_eq!(error.code, "store/base_missing");
}

#[test]
fn a_base_from_another_store_is_refused() {
    let directory = TestDirectory::create("foreign");
    sealed(&directory, "a");
    // Build a second store, seal it, and swap its base in.
    let other = directory.0.join("other-store");
    let other_archive = directory.0.join("other-archive");
    let mut store = populated(&other, "b");
    store
        .seal_and_archive(&other_archive, None)
        .expect("the second store seals");
    drop(store);
    fs::copy(other.join("journal").join(BASE_FILE), base_path(&directory))
        .expect("the foreign base is copyable");
    let error = match Store::open(&directory.store()) {
        Ok(_) => panic!("a foreign base was accepted"),
        Err(error) => error,
    };
    // The chain catches it first when the position differs, and the seal check
    // catches it when it does not. Either is a refusal, and neither serves.
    assert!(
        error.code == "store/base_mismatch" || error.code == "store/chain_broken",
        "unexpected code {}",
        error.code
    );
}

#[test]
fn a_base_with_one_altered_context_byte_is_refused_on_its_state_root() {
    let directory = TestDirectory::create("state-root");
    sealed(&directory, "a");
    rewrite_base(&directory, |object| {
        let mut instances = object
            .get("instances")
            .and_then(Value::as_obj)
            .expect("the base carries instances")
            .clone();
        let (id, instance) = instances
            .iter()
            .next()
            .map(|(id, instance)| (id.clone(), instance.clone()))
            .expect("the base carries at least one instance");
        let mut instance = instance.as_obj().expect("an instance is an object").clone();
        let mut context = instance
            .get("context")
            .and_then(Value::as_obj)
            .expect("the instance carries a context")
            .clone();
        let key = context.keys().next().cloned().expect("a context field");
        context.insert(key, Value::Str("999".into()));
        instance.insert("context".into(), Value::Obj(context));
        instances.insert(id, Value::Obj(instance));
        object.insert("instances".into(), Value::Obj(instances));
    });
    let error = match Store::open(&directory.store()) {
        Ok(_) => panic!("an altered base was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.code, "store/base_mismatch");
}

#[test]
fn a_base_with_one_altered_fingerprint_is_refused_on_its_dedup_root() {
    // The case the second root exists for: `state_root_at` covers the claiming
    // sequence and not the fingerprint, so nothing else would catch this.
    let directory = TestDirectory::create("dedup-root");
    sealed(&directory, "a");
    rewrite_base(&directory, |object| {
        let mut dedup = object
            .get("dedup")
            .and_then(Value::as_obj)
            .expect("the base carries a dedup table")
            .clone();
        let key = dedup
            .iter()
            .find(|(_, entry)| entry.get("fp").is_some())
            .map(|(key, _)| key.clone())
            .expect("the base carries a fingerprinted key");
        let mut entry = dedup[&key].as_obj().expect("an entry is an object").clone();
        entry.insert(
            "fp".into(),
            Value::Str(format!("sha256:{}", "c".repeat(64))),
        );
        dedup.insert(key, Value::Obj(entry));
        object.insert("dedup".into(), Value::Obj(dedup));
    });
    let error = match Store::open(&directory.store()) {
        Ok(_) => panic!("a base with an altered fingerprint was accepted"),
        Err(error) => error,
    };
    assert_eq!(error.code, "store/base_mismatch");
}

#[test]
fn a_base_declaring_the_wrong_position_fails_at_the_first_live_record() {
    // The chain is what checks the base's claim about where the journal picks
    // up, which is why the loader may trust the header to start and then check
    // what it loaded.
    let directory = TestDirectory::create("position");
    sealed(&directory, "a");
    rewrite_base(&directory, |object| {
        object.insert("last_hash".into(), Value::Str("ab".repeat(32)));
    });
    let error = match Store::open(&directory.store()) {
        Ok(_) => panic!("a base declaring the wrong predecessor was accepted"),
        Err(error) => error,
    };
    assert!(
        error.code == "store/chain_broken" || error.code == "io/read",
        "unexpected code {}",
        error.code
    );
}

#[test]
fn a_read_only_open_of_a_sealed_store_takes_no_lock_and_writes_nothing() {
    let directory = TestDirectory::create("readonly");
    let (_report, before) = sealed(&directory, "a");
    // A writer holds the store while the read-only open runs.
    let writer = Store::open(&directory.store()).expect("the writer opens");
    let listing = |path: &Path| -> Vec<std::ffi::OsString> {
        let mut names: Vec<_> = fs::read_dir(path)
            .expect("the directory is listable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name())
            .collect();
        names.sort();
        names
    };
    let journal = directory.store().join("journal");
    let listed = listing(&journal);
    let reader = Store::open_read_only(&directory.store()).expect("a sealed store opens read-only");
    assert_eq!(before.instances, reader.state.instances);
    assert_eq!(before.instance_machines, reader.state.instance_machines);
    assert_eq!(listing(&journal), listed, "a read-only open wrote");
    drop(reader);
    drop(writer);
}

#[test]
fn the_history_index_is_seeded_from_the_base_and_both_open_paths_agree() {
    // The index is fed from the live records *and* from what the base carries.
    // The seeded half deliberately names sequences below the cut: an
    // instance's age is its creation sequence, and reporting zero for a live
    // instance created before the cut is a wrong answer rather than a short
    // one. What must hold is that nothing between the two is invented, and
    // that both open paths build the same thing.
    let directory = TestDirectory::create("history");
    let (report, _before) = sealed(&directory, "a");
    let mut store = Store::open(&directory.store()).expect("a sealed store opens");
    store
        .annotate("a-live-0", "after-seal", "a note after the seal")
        .expect("a sealed store still writes");
    drop(store);

    let writer = Store::open(&directory.store()).expect("a sealed store reopens");
    let reader = Store::open_read_only(&directory.store()).expect("and opens read-only");
    assert_eq!(
        writer.history, reader.history,
        "the two open paths built different history indexes"
    );
    let base = fsm_store::base::open_from_base(&directory.store(), &writer.records)
        .expect("the base opens");
    for (id, sequences) in &writer.history {
        let seeded = base.index.instances.get(id);
        for seq in sequences {
            if *seq > report.sealed_through_seq {
                continue;
            }
            let seeded = seeded.unwrap_or_else(|| {
                panic!("the history index names archived seq {seq} for {id}, which the base does not carry")
            });
            assert!(
                *seq == seeded.created_seq || *seq == seeded.last_seq,
                "the history index invented archived seq {seq} for {id}"
            );
        }
    }
    // And the age of a live instance sealed below the cut is its real one.
    for (id, entry) in &base.index.instances {
        assert_eq!(
            writer.created_seq(id),
            entry.created_seq,
            "a sealed instance's age was not restored from the base"
        );
    }
}

#[test]
fn a_key_carried_in_the_base_is_still_conflict_checked_after_a_reopen() {
    // The carried fingerprints are what make this possible: the record that
    // claimed the key is in the archive, so only the base can answer.
    let directory = TestDirectory::create("conflict");
    sealed(&directory, "a");
    let mut store = Store::open(&directory.store()).expect("a sealed store opens");
    let carried = store
        .state
        .dedup
        .keys()
        .find(|key| key.starts_with("create-a-live-"))
        .cloned()
        .expect("a live instance's creation key was carried");
    // Re-issuing a carried key with different content is a conflict, not a
    // replay, and that is exactly what the fingerprint decides.
    let error = store
        .create_instance("case_review", "something-else", &carried, None)
        .expect_err("a carried key with different content is refused");
    assert_eq!(error.code, "req/request_id_conflict");
}

#[test]
fn a_key_claimed_above_the_cut_still_replays_after_a_reopen() {
    let directory = TestDirectory::create("replay");
    sealed(&directory, "a");
    let mut store = Store::open(&directory.store()).expect("a sealed store opens");
    store
        .annotate("a-live-0", "above-the-cut", "a note above the cut")
        .expect("the annotation succeeds");
    let seq = store.state.last_seq;
    drop(store);

    // After a restart the in-memory response cache is gone, so the answer has
    // to come from the records the live suffix still holds.
    let mut store = Store::open(&directory.store()).expect("a sealed store reopens");
    let replayed = store
        .annotate("a-live-0", "above-the-cut", "a note above the cut")
        .expect("a key claimed above the cut replays");
    assert_eq!(
        replayed.get("duplicate").and_then(Value::as_bool),
        Some(true),
        "the retry applied instead of replaying"
    );
    assert_eq!(store.state.last_seq, seq, "a replay appended a record");
}

#[test]
fn a_snapshot_cache_above_the_seal_is_still_used() {
    // The fast path survives sealing: only caches at or below the cut are
    // skipped, because only those need records that are no longer present.
    let directory = TestDirectory::create("snapshot");
    let (report, _before) = sealed(&directory, "a");
    let mut store = Store::open(&directory.store()).expect("a sealed store opens");
    store
        .annotate("a-live-0", "after-seal", "a note after the seal")
        .expect("the annotation succeeds");
    store
        .shutdown_snapshot()
        .expect("a shutdown snapshot is writable");
    drop(store);
    let caches = fsm_store::snapshot::listed_snaps(&directory.store());
    assert!(
        caches
            .iter()
            .any(|(seq, _)| *seq > report.sealed_through_seq),
        "the test needs a cache above the seal"
    );
    let reopened = Store::open(&directory.store()).expect("a sealed store with a cache opens");
    assert!(
        reopened.opened_from_snapshot,
        "a cache above the seal was skipped"
    );
}

#[test]
fn an_instance_history_on_a_sealed_store_says_what_it_could_not_see() {
    // A short history that does not say it is short reads as a complete one.
    // `next_from_seq` already points forwards; this is the same idea pointing
    // backwards.
    let directory = TestDirectory::create("history-horizon");
    let (report, _before) = sealed(&directory, "a");
    let store = Store::open(&directory.store()).expect("a sealed store opens");
    let page = store
        .history_page("a-live-0", 0, 100, false, true)
        .expect("the history page renders");
    let sealed_before = page
        .get("sealed_before")
        .expect("a sealed store's history names its horizon");
    assert_eq!(
        sealed_before
            .get("sealed_through_seq")
            .and_then(Value::as_num),
        Some(report.sealed_through_seq.to_string().as_str())
    );
    assert!(
        sealed_before
            .get("archive_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("sha256:"))
    );
    // Every entry it does return is above the cut, which is what makes the
    // horizon the whole of what is missing.
    for entry in page.get("entries").and_then(Value::as_arr).unwrap_or(&[]) {
        let seq: u64 = entry
            .get("seq")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse().ok())
            .expect("an entry carries its seq");
        assert!(seq > report.sealed_through_seq);
    }
}

#[test]
fn an_instance_history_on_an_unsealed_store_names_no_horizon() {
    let directory = TestDirectory::create("history-unsealed");
    let store = populated(&directory.store(), "a");
    let page = store
        .history_page("a-live-0", 0, 100, false, true)
        .expect("the history page renders");
    assert!(
        page.get("sealed_before").is_none(),
        "an unsealed history claimed a horizon"
    );
}

#[test]
fn explain_below_the_seal_says_where_the_record_is_rather_than_that_it_is_missing() {
    // `req/field_missing` for a sequence below a seal is actively wrong: the
    // record exists, in the archive.
    let directory = TestDirectory::create("explain");
    let (report, _before) = sealed(&directory, "a");
    let store = Store::open(&directory.store()).expect("a sealed store opens");
    let error = store
        .explain_seq("a-live-0", report.sealed_through_seq - 1)
        .expect_err("a sequence below the seal is refused");
    assert_eq!(error.code, "store/archive_refused");
    assert!(
        error.message.contains("archive") && error.message.contains("sha256:"),
        "the refusal does not say where the record is: {}",
        error.message
    );

    // A sequence that never existed at all is still the other refusal, so the
    // two absences stay distinguishable.
    let never = store
        .explain_seq("a-live-0", store.state.last_seq + 500)
        .expect_err("a sequence above the head is refused");
    assert_eq!(never.code, "req/field_missing");

    // And a record of this instance above the cut still explains. The seal
    // record itself sits at `cut + 1` and touches no instance, which is its
    // own correct refusal.
    let mut store = store;
    store
        .annotate("a-live-0", "above-the-cut", "a note above the cut")
        .expect("the annotation succeeds");
    let explained = store
        .explain_seq("a-live-0", store.state.last_seq)
        .expect("a record above the cut explains");
    assert_eq!(
        explained.get("kind").and_then(Value::as_str),
        Some("Annotated")
    );
}

#[test]
fn a_snapshot_written_from_a_sealed_store_holds_every_machine_and_instance() {
    // The reader that would have written a *wrong file*: a cache claiming a
    // smaller store than exists, with no error anywhere. The snapshot writer
    // materializes the folded state, which on a sealed store is the base plus
    // the live suffix — so it is complete by construction, and this is the
    // assertion that keeps it so.
    let directory = TestDirectory::create("snapshot-complete");
    sealed(&directory, "a");
    let mut store = Store::open(&directory.store()).expect("a sealed store opens");
    store
        .annotate("a-live-0", "after-seal", "a note above the cut")
        .expect("the annotation succeeds");
    let expected_machines: Vec<String> = store.state.machines.keys().cloned().collect();
    let expected_instances: Vec<String> = store.state.instances.keys().cloned().collect();
    assert!(!expected_instances.is_empty());
    store
        .shutdown_snapshot()
        .expect("a shutdown snapshot is writable");
    drop(store);

    let (_seq, path) = fsm_store::snapshot::listed_snaps(&directory.store())
        .into_iter()
        .next_back()
        .expect("a cache was written");
    let bytes = fs::read(&path).expect("the cache is readable");
    let cache = parse(&bytes, &JsonLimits::DEFAULT).expect("the cache parses");
    let machines: Vec<String> = cache
        .get("machines")
        .and_then(Value::as_obj)
        .expect("the cache carries machines")
        .keys()
        .cloned()
        .collect();
    let instances: Vec<String> = cache
        .get("instances")
        .and_then(Value::as_obj)
        .expect("the cache carries instances")
        .keys()
        .cloned()
        .collect();
    assert_eq!(machines, expected_machines, "the cache lost a machine");
    assert_eq!(instances, expected_instances, "the cache lost an instance");

    // And it is accepted on the next open, so the cache and the base agree.
    let reopened = Store::open(&directory.store()).expect("a sealed store with a cache opens");
    assert!(reopened.opened_from_snapshot);
}

#[test]
fn the_seal_record_is_the_first_live_record_after_a_head_cut() {
    let directory = TestDirectory::create("first-record");
    let (report, _before) = sealed(&directory, "a");
    let store = Store::open(&directory.store()).expect("a sealed store opens");
    let first = store.records.first().expect("the live suffix is not empty");
    assert_eq!(first.kind, RecordKind::JournalSealed);
    assert_eq!(first.seq, report.sealed_through_seq + 1);
}
