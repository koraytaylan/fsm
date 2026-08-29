//! Verifying a sealed store: three verdicts, three exit codes, and the rule
//! that the middle one can never be mistaken for the first.
//!
//! Plan 0017 task 8102. The claim this project makes about a journal is the
//! strongest thing it says, so a verification that did **not** read the sealed
//! bytes must never report what one that did reports — in prose, in the
//! structured result, and in the exit status, because a shell script reads
//! only the last of those.

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
            "fsm-verify-sealed-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        for sub in ["store", "archive", "elsewhere", "empty"] {
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

/// Run the real binary and return `(exit code, parsed --json result)`.
fn verify(store: &Path, extra: &[String]) -> (i32, Value) {
    let mut argv = vec![
        "journal".to_string(),
        "verify".to_string(),
        "--json".to_string(),
        format!("--data-dir={}", store.display()),
    ];
    argv.extend(extra.iter().cloned());
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(&argv)
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    let text = String::from_utf8(output.stdout).expect("stdout is utf-8");
    let parsed = parse(text.as_bytes(), &JsonLimits::DEFAULT)
        .unwrap_or_else(|error| panic!("{error:?}: {text}"));
    (output.status.code().unwrap_or(-1), parsed)
}

fn seal_a_store(directory: &TestDirectory) -> u64 {
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
    let report = store
        .seal_and_archive(&directory.archive(), None)
        .expect("the store seals");
    drop(store);
    report.sealed_through_seq
}

fn verdict(result: &Value) -> Option<&str> {
    result
        .get("seal")
        .and_then(|seal| seal.get("verdict"))
        .and_then(Value::as_str)
}

#[test]
fn an_unsealed_store_reports_no_seal_and_exits_zero() {
    // Byte-for-byte the answer it always gave: no seal object, exit zero.
    let directory = TestDirectory::create("unsealed");
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    drop(store);
    let (code, result) = verify(&directory.store(), &[]);
    assert_eq!(code, 0);
    assert!(
        result.get("seal").is_none(),
        "an unsealed store reported a seal: {result:?}"
    );
}

#[test]
fn a_sealed_store_with_no_archive_presented_reports_the_middle_verdict() {
    let directory = TestDirectory::create("not-presented");
    let cut = seal_a_store(&directory);
    let (code, result) = verify(&directory.store(), &[]);
    assert_eq!(
        code, 7,
        "the middle verdict must not share an exit code with success or failure"
    );
    assert_eq!(verdict(&result), Some("prefix_not_presented"));
    let message = result
        .get("message")
        .and_then(Value::as_str)
        .expect("the middle verdict says so in prose");
    assert!(
        message.contains("not presented") && message.contains(&cut.to_string()),
        "the prose does not say what was not read: {message}"
    );
    assert!(
        result
            .get("seal")
            .and_then(|seal| seal.get("archive_dir"))
            .is_none(),
        "a verification that read no archive named one"
    );
}

#[test]
fn the_same_store_with_the_archive_presented_reports_a_complete_walk() {
    let directory = TestDirectory::create("presented");
    seal_a_store(&directory);
    let (code, result) = verify(
        &directory.store(),
        &[format!("--with-archive={}", directory.archive().display())],
    );
    assert_eq!(
        code, 0,
        "a complete walk is the only success on a sealed store"
    );
    assert_eq!(verdict(&result), Some("prefix_walked"));
    assert_eq!(
        result
            .get("seal")
            .and_then(|seal| seal.get("archive_dir"))
            .and_then(Value::as_str),
        Some(directory.archive().display().to_string().as_str()),
        "the result does not record which bytes were walked"
    );
}

#[test]
fn the_three_verdicts_are_distinguishable_without_reading_prose() {
    // A caller that has to parse a sentence to learn what was verified is a
    // caller that will get it wrong.
    let directory = TestDirectory::create("distinct");
    let mut store = Store::open(&directory.store()).expect("a fresh store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    drop(store);
    let (unsealed_code, unsealed) = verify(&directory.store(), &[]);
    seal_a_store(&directory);
    let (middle_code, middle) = verify(&directory.store(), &[]);
    let (walked_code, walked) = verify(
        &directory.store(),
        &[format!("--with-archive={}", directory.archive().display())],
    );

    let verdicts = [verdict(&unsealed), verdict(&middle), verdict(&walked)];
    assert_eq!(
        verdicts,
        [None, Some("prefix_not_presented"), Some("prefix_walked")]
    );
    let codes = [unsealed_code, middle_code, walked_code];
    assert_eq!(codes[0], codes[2], "both complete walks report success");
    assert_ne!(
        codes[1], codes[0],
        "the middle verdict shares an exit code with success"
    );
}

#[test]
fn an_archive_with_one_flipped_byte_fails_and_names_the_segment() {
    let directory = TestDirectory::create("flipped");
    seal_a_store(&directory);
    let manifest =
        fsm_store::archive::read_manifest(&directory.archive()).expect("the manifest is readable");
    let target = &manifest.segments[0];
    let path = directory.archive().join(&target.name);
    let mut bytes = fs::read(&path).expect("the segment is readable");
    let position = bytes.len() / 2;
    bytes[position] ^= 0x20;
    fs::write(&path, &bytes).expect("the segment is writable");

    let (code, result) = verify(
        &directory.store(),
        &[format!("--with-archive={}", directory.archive().display())],
    );
    assert_ne!(code, 0, "a tampered archive verified");
    let rendered = format!("{result:?}");
    assert!(
        rendered.contains(&target.name),
        "the failure does not name the segment: {rendered}"
    );
}

#[test]
fn an_archive_belonging_to_another_store_is_refused() {
    let directory = TestDirectory::create("foreign");
    seal_a_store(&directory);
    // A second store with its own archive, presented against the first.
    let elsewhere = directory.0.join("elsewhere");
    let other_archive = directory.0.join("empty");
    let mut store = Store::open(&elsewhere).expect("a second store opens");
    store
        .define_machine(
            parse(CASE_REVIEW, &JsonLimits::DEFAULT).expect("the committed machine parses"),
            false,
            false,
        )
        .expect("the machine is definable");
    store
        .seal_and_archive(&other_archive, None)
        .expect("the second store seals");
    drop(store);

    let (code, result) = verify(
        &directory.store(),
        &[format!("--with-archive={}", other_archive.display())],
    );
    assert_ne!(code, 0, "a foreign archive was accepted");
    let rendered = format!("{result:?}");
    assert!(
        rendered.contains("seal") || rendered.contains("archive"),
        "the failure does not say what disagreed: {rendered}"
    );
}

#[test]
fn a_directory_with_no_manifest_says_so_rather_than_calling_the_store_corrupt() {
    let directory = TestDirectory::create("no-manifest");
    seal_a_store(&directory);
    let (code, result) = verify(
        &directory.store(),
        &[format!(
            "--with-archive={}",
            directory.0.join("empty").display()
        )],
    );
    assert_ne!(code, 0);
    let rendered = format!("{result:?}");
    assert!(
        rendered.to_lowercase().contains("manifest") || rendered.to_lowercase().contains("archive"),
        "the failure blames the store rather than the missing manifest: {rendered}"
    );
}

#[test]
fn a_tampered_base_fails_before_any_archive_is_consulted() {
    let directory = TestDirectory::create("tampered-base");
    seal_a_store(&directory);
    let base = directory.store().join("journal").join("BASE");
    let bytes = fs::read(&base).expect("the base is readable");
    let value = parse(&bytes, &JsonLimits::DEFAULT).expect("the base parses");
    let mut object = value.as_obj().expect("the base is an object").clone();
    object.insert(
        "base_dedup_fp_root".into(),
        Value::Str(format!("sha256:{}", "ab".repeat(32))),
    );
    let mut rewritten = fsm_core::canon::canon_bytes(&Value::Obj(object));
    rewritten.push(b'\n');
    fs::write(&base, rewritten).expect("the base is writable");

    // With the archive presented, so the failure cannot be blamed on its absence.
    let (code, result) = verify(
        &directory.store(),
        &[format!("--with-archive={}", directory.archive().display())],
    );
    assert_ne!(code, 0, "a tampered base verified");
    assert_ne!(
        verdict(&result),
        Some("prefix_walked"),
        "a tampered base reported a complete walk"
    );
}
