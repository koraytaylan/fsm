//! `fsm journal archive`: the one operation in plan 0017 an operator performs,
//! and the preview that can be asked what it would do before it is allowed to
//! do it.
//!
//! Plan 0017 task 8201.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_store::store::Store;

const CASE_REVIEW: &[u8] =
    include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json");

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// A directory name no other run of this binary can produce.
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
            "fsm-journal-archive-{tag}-{}-{index}",
            invocation_tag()
        ));
        let _ = fs::remove_dir_all(&path);
        for sub in ["store", "archive", "second"] {
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

fn run(store: &Path, argv: &[String]) -> (i32, Value, String) {
    let mut arguments = vec!["journal".to_string(), "archive".to_string()];
    arguments.extend(argv.iter().cloned());
    arguments.push("--json".to_string());
    arguments.push(format!("--data-dir={}", store.display()));
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(&arguments)
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    let stdout = String::from_utf8(output.stdout).expect("stdout is utf-8");
    let stderr = String::from_utf8(output.stderr).expect("stderr is utf-8");
    let chosen = if stdout.trim().is_empty() {
        stderr.clone()
    } else {
        stdout.clone()
    };
    let parsed = parse(chosen.as_bytes(), &JsonLimits::DEFAULT).unwrap_or(Value::Obj(
        BTreeMap::from([("raw".into(), Value::Str(chosen.clone()))]),
    ));
    (output.status.code().unwrap_or(-1), parsed, chosen)
}

/// A store with one live instance whose effects are acked, so nothing pins.
///
/// Returns the open writer. A test that needs one holds *this* rather than
/// dropping it and opening again — the reopen is a step none of them needs,
/// and under concurrent test threads it can transiently see `store/lock`.
fn populated(store_path: &Path) -> Store {
    let mut store = Store::open(store_path).expect("a fresh store opens");
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

/// The same store, closed — for the tests that need no writer held.
fn populate(store_path: &Path) {
    drop(populated(store_path));
}

/// Every file in a directory tree, as `(relative path, bytes)`.
fn snapshot_tree(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("the path is under the root")
                    .display()
                    .to_string();
                out.push((relative, fs::read(&path).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn a_dry_run_reports_what_would_be_sealed_and_writes_nothing() {
    let directory = TestDirectory::create("dry-run");
    populate(&directory.store());
    let before = snapshot_tree(&directory.store());
    let (code, result, rendered) = run(
        &directory.store(),
        &[
            format!("--to={}", directory.archive().display()),
            "--dry-run".to_string(),
        ],
    );
    assert_eq!(code, 0, "{rendered}");
    assert_eq!(result.get("dry_run").and_then(Value::as_bool), Some(true));
    assert!(result.get("sealed_through_seq").is_some());
    assert!(result.get("segments").and_then(Value::as_arr).is_some());
    assert!(result.get("keys_carried").is_some());
    assert!(result.get("keys_dropped").is_some());
    // A preview reports the prefix and the partition, never a hash of a
    // checkpoint that does not exist yet.
    assert!(result.get("archive_id").is_none());
    assert!(result.get("seal_record_seq").is_none());

    assert_eq!(
        snapshot_tree(&directory.store()),
        before,
        "a preview changed the data directory"
    );
    assert_eq!(
        fs::read_dir(directory.archive())
            .expect("the archive is listable")
            .count(),
        0,
        "a preview wrote into the archive"
    );
}

#[test]
fn a_dry_run_takes_no_lock() {
    let directory = TestDirectory::create("dry-run-lock");
    let writer = populated(&directory.store());
    let (code, _result, rendered) = run(
        &directory.store(),
        &[
            format!("--to={}", directory.archive().display()),
            "--dry-run".to_string(),
        ],
    );
    drop(writer);
    assert_eq!(code, 0, "a preview refused while a writer held: {rendered}");
}

#[test]
fn the_sequence_a_dry_run_names_is_the_one_the_run_seals_through() {
    let directory = TestDirectory::create("agree");
    populate(&directory.store());
    let (_code, preview, _) = run(
        &directory.store(),
        &[
            format!("--to={}", directory.archive().display()),
            "--dry-run".to_string(),
        ],
    );
    let named = preview
        .get("sealed_through_seq")
        .and_then(Value::as_str)
        .or_else(|| preview.get("sealed_through_seq").and_then(Value::as_num))
        .expect("the preview names a sequence")
        .to_string();
    let (code, run_result, rendered) = run(
        &directory.store(),
        &[
            format!("--to={}", directory.archive().display()),
            format!("--before-seq={named}"),
        ],
    );
    assert_eq!(code, 0, "{rendered}");
    assert_eq!(
        run_result.get("sealed_through_seq").and_then(Value::as_num),
        Some(named.as_str())
    );
    assert!(
        run_result
            .get("archive_id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("sha256:")),
        "a real run does not report the archive it wrote"
    );
    assert_eq!(
        run_result.get("dry_run").and_then(Value::as_bool),
        Some(false)
    );
}

#[test]
fn a_stale_before_seq_is_refused_in_the_preview_and_in_the_run_alike() {
    let directory = TestDirectory::create("stale");
    populate(&directory.store());
    for extra in [vec!["--dry-run".to_string()], Vec::new()] {
        let mut argv = vec![
            format!("--to={}", directory.archive().display()),
            "--before-seq=1".to_string(),
        ];
        argv.extend(extra.clone());
        let (code, _result, rendered) = run(&directory.store(), &argv);
        assert_ne!(code, 0, "a stale assertion was accepted: {rendered}");
        assert!(
            rendered.contains("archive_refused"),
            "the refusal is not the archive one: {rendered}"
        );
    }
    // And nothing was written by either refusal.
    assert_eq!(
        fs::read_dir(directory.archive())
            .expect("the archive is listable")
            .count(),
        0
    );
}

#[test]
fn omitting_to_is_a_usage_error_that_names_the_flag() {
    let directory = TestDirectory::create("no-to");
    populate(&directory.store());
    let (code, _result, rendered) = run(&directory.store(), &[]);
    assert_ne!(code, 0);
    assert!(
        rendered.contains("--to"),
        "the usage error does not name the flag: {rendered}"
    );
}

#[test]
fn a_second_run_into_the_same_archive_is_refused() {
    let directory = TestDirectory::create("twice");
    populate(&directory.store());
    let argv = vec![format!("--to={}", directory.archive().display())];
    let (first, _result, rendered) = run(&directory.store(), &argv);
    assert_eq!(first, 0, "{rendered}");
    let (second, _result, rendered) = run(&directory.store(), &argv);
    assert_ne!(second, 0, "a second seal into one archive was accepted");
    assert!(rendered.contains("MANIFEST"), "{rendered}");
}

#[test]
fn a_run_against_a_store_another_writer_holds_reports_the_contended_message() {
    let directory = TestDirectory::create("contended");
    let writer = populated(&directory.store());
    let (code, _result, rendered) = run(
        &directory.store(),
        &[format!("--to={}", directory.archive().display())],
    );
    drop(writer);
    assert_ne!(code, 0, "a contended store was sealed");
    assert!(
        rendered.contains("store/lock") || rendered.contains("locked"),
        "the refusal is not the existing contended-writer one: {rendered}"
    );
}

#[test]
fn a_real_run_seals_and_the_store_reopens() {
    let directory = TestDirectory::create("real");
    populate(&directory.store());
    let (code, result, rendered) = run(
        &directory.store(),
        &[format!("--to={}", directory.archive().display())],
    );
    assert_eq!(code, 0, "{rendered}");
    let cut: u64 = result
        .get("sealed_through_seq")
        .and_then(Value::as_num)
        .and_then(|raw| raw.parse().ok())
        .expect("the run names the cut");

    // The archive verifies, the store reopens, and its live suffix is above
    // the cut.
    fsm_store::archive::verify(&directory.archive()).expect("the written archive verifies");
    let reopened = Store::open(&directory.store()).expect("a sealed store reopens");
    assert!(
        reopened.records.iter().all(|record| record.seq > cut),
        "an archived record was still in the live journal"
    );
    assert!(reopened.state.instances.contains_key("live"));
}

#[test]
fn the_help_output_lists_the_command_and_its_flags() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fsm"))
        .arg("--help")
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs");
    let text = String::from_utf8(output.stdout).expect("stdout is utf-8");
    assert!(
        text.contains("journal archive"),
        "the command is not in --help: {text}"
    );
    assert!(
        text.contains("to") && text.contains("before-seq") && text.contains("dry-run"),
        "the flags are not in --help: {text}"
    );
}
