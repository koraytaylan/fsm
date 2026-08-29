//! `journal replay` and `doctor` on a sealed store: both tell the truth about
//! a prefix that is no longer here, and both answer from a **path**.
//!
//! Plan 0017 task 8103.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::store::Store;

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
            "fsm-sealed-diag-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        for sub in ["store", "archive"] {
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

fn run(store: &Path, argv: &[&str]) -> (i32, Value) {
    let mut arguments: Vec<String> = argv.iter().map(|part| (*part).to_string()).collect();
    arguments.push("--json".to_string());
    arguments.push(format!("--data-dir={}", store.display()));
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(&arguments)
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    // A refusal renders on stderr; both streams are the command's answer.
    let text = String::from_utf8(output.stdout).expect("stdout is utf-8");
    let errors = String::from_utf8(output.stderr).expect("stderr is utf-8");
    let chosen = if text.trim().is_empty() {
        &errors
    } else {
        &text
    };
    let parsed = parse(chosen.as_bytes(), &JsonLimits::DEFAULT)
        .unwrap_or_else(|error| panic!("{error:?}: stdout={text} stderr={errors}"));
    (output.status.code().unwrap_or(-1), parsed)
}

/// Seal a store and **keep** the writer that did it.
fn seal_holding_the_writer(directory: &TestDirectory) -> (Store, u64) {
    let mut store = build_store(directory);
    let report = store
        .seal_and_archive(&directory.archive(), None)
        .expect("the store seals");
    store
        .annotate("live", "after-seal", "a note above the cut")
        .expect("the annotation succeeds");
    let cut = report.sealed_through_seq;
    (store, cut)
}

/// A sealed store, returning the cut it sealed through.
fn seal_a_store(directory: &TestDirectory) -> u64 {
    let (store, cut) = seal_holding_the_writer(directory);
    drop(store);
    cut
}

/// A store with one live instance whose effects are acked, so nothing pins.
fn build_store(directory: &TestDirectory) -> Store {
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    store
        .create_instance("case_review", "live", "create-live", None)
        .expect("create succeeds");
    store
        .send_event(
            "live",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send-live",
            None,
        )
        .expect("send succeeds");
    let pending: Vec<String> = store.state.instances["live"].pending.clone();
    for (index, effect_id) in pending.iter().enumerate() {
        store
            .ack_effect("live", effect_id, &format!("ack-{index}"))
            .expect("ack succeeds");
    }
    store
}

#[test]
fn a_test_directory_is_unique_per_invocation() {
    // Not decoration: a process id alone collided under a full `--workspace`
    // run, and the symptom was a `store/lock` naming this very process — an
    // earlier run's directory, reached through a reused id. `crash_harness.rs`
    // pins the same property for the same reason.
    let first = TestDirectory::create("uniqueness");
    let second = TestDirectory::create("uniqueness");
    assert_ne!(first.0, second.0);
    assert!(invocation_tag() != format!("{}", std::process::id()));
}

#[test]
fn replay_of_a_sealed_store_agrees_and_reports_where_it_started() {
    let directory = TestDirectory::create("replay-ok");
    let cut = seal_a_store(&directory);
    let (code, result) = run(&directory.store(), &["journal", "replay"]);
    assert_eq!(code, 0, "replay of a sealed store disagreed: {result:?}");
    assert_eq!(result.get("agreement").and_then(Value::as_bool), Some(true));
    let from = result
        .get("replayed_from_seal")
        .expect("replay reports the seal it started from");
    assert_eq!(
        from.get("sealed_through_seq").and_then(Value::as_num),
        Some(cut.to_string().as_str()),
        "replay does not name the sequence it started after"
    );
    assert!(
        from.get("archive_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("sha256:")),
        "replay does not name the archive its prefix went to"
    );
}

#[test]
fn replay_to_a_sequence_below_the_seal_is_refused_and_says_where_the_records_are() {
    // The records are not absent, they are elsewhere. Telling an operator
    // where is the difference between a refusal and a dead end.
    let directory = TestDirectory::create("replay-below");
    let cut = seal_a_store(&directory);
    let (code, result) = run(
        &directory.store(),
        &["journal", "replay", &format!("--to-seq={}", cut - 1)],
    );
    assert_ne!(code, 0, "a to-seq below the seal was replayed");
    let rendered = format!("{result:?}");
    assert!(
        rendered.contains("archive_refused"),
        "the refusal is not the archive one: {rendered}"
    );
    assert!(
        rendered.contains(&cut.to_string()) && rendered.contains("sha256:"),
        "the refusal names neither the cut nor the archive: {rendered}"
    );
}

#[test]
fn replay_to_a_sequence_above_the_seal_still_works() {
    let directory = TestDirectory::create("replay-above");
    let cut = seal_a_store(&directory);
    let (code, result) = run(
        &directory.store(),
        &["journal", "replay", &format!("--to-seq={}", cut + 1)],
    );
    assert_eq!(code, 0, "replay above the cut failed: {result:?}");
    assert_eq!(result.get("agreement").and_then(Value::as_bool), Some(true));
}

/// A store whose cut is an **existing segment boundary**, so the seal record
/// lands far above it rather than at `cut + 1`.
///
/// Every other sealed fixture here cuts at the head, where the seal record is
/// the very next sequence — which leaves the window `cut < n < seal_record_seq`
/// empty, and that is why nothing caught a replay into it.
fn seal_at_a_boundary(directory: &TestDirectory) -> (u64, u64) {
    let mut store = build_store(directory);
    store
        .journal
        .force_rotate()
        .expect("the journal rotates on demand");
    let boundary = store.state.last_seq;
    store
        .create_instance("case_review", "pending", "create-pending", None)
        .expect("create succeeds");
    store
        .send_event(
            "pending",
            "docs_ok",
            Value::Obj(BTreeMap::new()),
            "send-pending",
            None,
        )
        .expect("send succeeds");
    assert!(
        !store.state.instances["pending"].pending.is_empty(),
        "the case needs an unacked effect to pin the cut"
    );
    let report = store
        .seal_and_archive(&directory.archive(), None)
        .expect("a pinned store seals the segments below the pin");
    assert_eq!(report.sealed_through_seq, boundary);
    let seal_seq = report
        .seal_record_seq
        .expect("a real seal appends a record");
    assert!(
        seal_seq > boundary + 1,
        "this fixture needs a gap between the cut and the seal record"
    );
    drop(store);
    (boundary, seal_seq)
}

#[test]
fn replay_between_the_cut_and_the_seal_record_reports_a_healthy_store() {
    // The seal record authenticates the base, and filtering the window by
    // `--to-seq` used to filter it away — so a perfectly healthy store
    // reported `agreement: false` with a bogus divergent sequence and exited
    // non-zero, for every sequence between its cut and its seal record.
    let directory = TestDirectory::create("replay-window");
    let (cut, seal_seq) = seal_at_a_boundary(&directory);
    for to in (cut + 1)..seal_seq {
        let (code, result) = run(
            &directory.store(),
            &["journal", "replay", &format!("--to-seq={to}")],
        );
        assert_eq!(
            code, 0,
            "a healthy store failed replay at to-seq {to}: {result:?}"
        );
        assert_eq!(
            result.get("agreement").and_then(Value::as_bool),
            Some(true),
            "at to-seq {to}: {result:?}"
        );
    }
}

#[test]
fn doctor_reports_the_seal_on_a_healthy_sealed_store() {
    // "How much of this store is live" is the first question a sealed store is
    // asked, so it is answered rather than left to be inferred.
    let directory = TestDirectory::create("doctor-ok");
    let cut = seal_a_store(&directory);
    let (code, result) = run(&directory.store(), &["doctor"]);
    assert_eq!(code, 0);
    let seal = result.get("seal").expect("doctor reports the seal");
    assert_eq!(
        seal.get("sealed_through_seq").and_then(Value::as_num),
        Some(cut.to_string().as_str())
    );
    assert!(
        seal.get("archive_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("sha256:"))
    );
    assert!(
        seal.get("live_records").is_some(),
        "doctor does not say how much is live"
    );
    assert_eq!(
        seal.get("verdict").and_then(Value::as_str),
        Some("prefix_not_presented"),
        "doctor reads no archive, and must say so"
    );
}

#[test]
fn doctor_reports_no_seal_for_an_unsealed_store() {
    let directory = TestDirectory::create("doctor-unsealed");
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    drop(store);
    let (code, result) = run(&directory.store(), &["doctor"]);
    assert_eq!(code, 0);
    assert!(
        result.get("seal").is_none(),
        "an unsealed store reported a seal"
    );
}

#[test]
fn a_sealed_store_with_no_base_is_classified_and_carries_a_remedy() {
    let directory = TestDirectory::create("doctor-base-missing");
    seal_a_store(&directory);
    fs::remove_file(directory.store().join("journal").join("BASE")).expect("the base is removable");
    let (code, result) = run(&directory.store(), &["doctor"]);
    assert_ne!(code, 0, "a store with no base was reported healthy");
    let rendered = format!("{result:?}");
    assert!(
        rendered.contains("base_missing"),
        "the classification is not base_missing: {rendered}"
    );
}

#[test]
fn a_base_that_is_present_and_unreadable_is_not_reported_as_absent() {
    // `base_missing` says "records were removed from this directory without a
    // seal saying so" and tells the operator to restore the segments from
    // backup. For a base file sitting right there with a truncated last line
    // that is the wrong diagnosis *and* the wrong remedy: the archive is fine
    // and the bytes to restore are one file, not a journal.
    for (tag, bytes) in [
        ("truncated", b"{\"format\": \"fsm.base/1\", \"se".to_vec()),
        ("not-json", b"this is not a base file\n".to_vec()),
        ("not-an-object", b"[1, 2, 3]\n".to_vec()),
    ] {
        let directory = TestDirectory::create(&format!("doctor-base-unreadable-{tag}"));
        seal_a_store(&directory);
        let base = directory.store().join("journal").join("BASE");
        fs::write(&base, &bytes).expect("the base is writable");
        let (code, result) = run(&directory.store(), &["doctor"]);
        assert_ne!(code, 0, "an unreadable base was reported healthy");
        let rendered = format!("{result:?}");
        assert!(
            rendered.contains("base_mismatch"),
            "an unreadable base was classified as absent ({tag}): {rendered}"
        );
        assert!(
            !rendered.contains("base_missing"),
            "an unreadable base was classified as absent ({tag}): {rendered}"
        );
    }
}

#[test]
fn a_sealed_store_with_a_tampered_base_is_classified_and_offers_no_repair() {
    let directory = TestDirectory::create("doctor-base-mismatch");
    seal_a_store(&directory);
    let base = directory.store().join("journal").join("BASE");
    let value = parse(
        &fs::read(&base).expect("the base is readable"),
        &JsonLimits::DEFAULT,
    )
    .expect("the base parses");
    let mut object = value.as_obj().expect("the base is an object").clone();
    object.insert(
        "base_state_root".into(),
        Value::Str(format!("sha256:{}", "ab".repeat(32))),
    );
    let mut rewritten = fsm_core::canon::canon_bytes(&Value::Obj(object));
    rewritten.push(b'\n');
    fs::write(&base, rewritten).expect("the base is writable");

    let (code, result) = run(&directory.store(), &["doctor"]);
    assert_ne!(code, 0, "a tampered base was reported healthy");
    let rendered = format!("{result:?}");
    assert!(
        rendered.contains("base_mismatch"),
        "the classification is not base_mismatch: {rendered}"
    );
    assert!(
        rendered.contains("no repair") || rendered.contains("archive"),
        "the remedy does not say the base cannot be reconstructed here: {rendered}"
    );
}

#[test]
fn both_diagnostics_answer_while_a_writer_holds_the_store() {
    // Plan 0014's property: a diagnosis is a function of a path, never of an
    // open store. This plan must not narrow it.
    let directory = TestDirectory::create("path-only");
    // The writer this test needs is the one that sealed: holding it rather
    // than dropping and reopening keeps the property exact and removes a
    // reopen the test never needed.
    let (writer, _cut) = seal_holding_the_writer(&directory);
    let (doctor_code, doctor) = run(&directory.store(), &["doctor"]);
    let (replay_code, replay) = run(&directory.store(), &["journal", "replay"]);
    let (verify_code, verify) = run(&directory.store(), &["journal", "verify"]);
    drop(writer);
    assert_eq!(
        doctor_code, 0,
        "doctor refused while a writer held: {doctor:?}"
    );
    assert_eq!(
        replay_code, 0,
        "replay refused while a writer held: {replay:?}"
    );
    assert_eq!(
        verify_code, 7,
        "verify reported something other than the middle verdict: {verify:?}"
    );
    assert!(doctor.get("seal").is_some());
    assert!(replay.get("replayed_from_seal").is_some());
}

#[test]
fn an_unsealed_stores_diagnostics_are_unchanged() {
    let directory = TestDirectory::create("unsealed");
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    store
        .create_instance("case_review", "live", "create-live", None)
        .expect("create succeeds");
    drop(store);
    let (replay_code, replay) = run(&directory.store(), &["journal", "replay"]);
    assert_eq!(replay_code, 0);
    assert_eq!(replay.get("agreement").and_then(Value::as_bool), Some(true));
    assert!(
        replay.get("replayed_from_seal").is_none(),
        "an unsealed replay reported a seal"
    );
    let (verify_code, verify) = run(&directory.store(), &["journal", "verify"]);
    assert_eq!(verify_code, 0);
    assert!(verify.get("seal").is_none());
}
