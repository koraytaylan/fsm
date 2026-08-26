//! Moving a cohort: N idempotent operations that resume, not a transaction
//! that cannot exist.
//!
//! Plan 0011 task 5504.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::store::Store;
use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-bulkmig-{}-{n}", std::process::id()));
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

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn digest(source: &str) -> String {
    digest_of(&machine_id(&value(source))).unwrap().to_string()
}

const OLD: &str = r#"{"format":"fsm.machine/1","name":"cohort_v1","states":[{"name":"intake"},{"name":"stuck"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"1"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"stuck"}]}"#;

fn new_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"cohort_v2","states":[{{"name":"intake"}},{{"name":"stuck"}}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"stuck"}}],"supersedes":{{"machine":"{}","states":{{"intake":"intake"}},"context":{{"score":"ctx.score + 1"}}}}}}"#,
        digest(OLD)
    )
}

/// A store with `clean` instances in `intake` and `stuck` instances in the
/// leaf the mapping does not cover.
fn cohort(directory: &TestDirectory, clean: usize, stuck: usize) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    for source in [OLD.to_string(), new_source()] {
        store
            .define_machine_on(&mut clock, value(&source), false, false)
            .unwrap();
    }
    for index in 0..(clean + stuck) {
        let id = format!("inst-{index:02}");
        store
            .create_instance_ctx_on(
                &mut clock,
                "cohort_v1",
                &id,
                &format!("create-{index}"),
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        if index >= clean {
            store
                .send_event(
                    &id,
                    "go",
                    Value::Obj(BTreeMap::new()),
                    &format!("go-{index}"),
                    None,
                )
                .unwrap();
        }
    }
    store
}

/// Run `fsm migrate` against a directory and return its parsed summary.
fn run(directory: &TestDirectory, extra: &[&str]) -> (Value, i32) {
    let mut argv = vec![
        "migrate".to_string(),
        "--from=cohort_v1".to_string(),
        "--to=cohort_v2".to_string(),
        "--json".to_string(),
    ];
    argv.extend(extra.iter().map(|flag| (*flag).to_string()));
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(&argv)
        .arg(format!("--data-dir={}", directory.path().display()))
        .env("FSM_CLOCK_MS", "9000")
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let text = String::from_utf8(output.stdout).unwrap();
    let parsed = parse(text.as_bytes(), &JsonLimits::DEFAULT)
        .unwrap_or_else(|error| panic!("{error:?}: {text}"));
    (parsed, output.status.code().unwrap_or(-1))
}

fn migrations(store: &Store) -> usize {
    store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::InstanceMigrated)
        .count()
}

#[test]
fn a_clean_cohort_migrates_with_one_record_each() {
    let directory = TestDirectory::create();
    drop(cohort(&directory, 10, 0));
    let (summary, code) = run(&directory, &[]);
    assert_eq!(code, 0);
    assert_eq!(summary.get("migrated").and_then(Value::as_num), Some("10"));
    assert_eq!(summary.get("skipped").and_then(Value::as_num), Some("0"));
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(migrations(&store), 10);
    for index in 0..10 {
        let id = format!("inst-{index:02}");
        assert_eq!(
            store.state.instances[&id].ctx["score"].canonical_string(),
            "2"
        );
    }
}

#[test]
fn a_mixed_cohort_migrates_what_it_can_and_names_what_it_cannot() {
    let directory = TestDirectory::create();
    drop(cohort(&directory, 6, 4));
    let (summary, code) = run(&directory, &[]);
    assert_eq!(code, 0, "a predicted refusal is not a surprise");
    assert_eq!(summary.get("migrated").and_then(Value::as_num), Some("6"));
    assert_eq!(summary.get("skipped").and_then(Value::as_num), Some("4"));
    let groups = summary.get("groups").and_then(Value::as_arr).unwrap();
    let refusal = groups
        .iter()
        .find(|group| group.get("outcome").and_then(Value::as_str) == Some("req/migrate_unmapped"))
        .expect("the refusal is grouped");
    assert_eq!(refusal.get("count").and_then(Value::as_num), Some("4"));
    assert!(
        refusal
            .get("detail")
            .and_then(Value::as_str)
            .is_some_and(|detail| detail.contains("stuck")),
        "an operator sees which state blocks them: {refusal:?}"
    );
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(migrations(&store), 6);
}

#[test]
fn a_dry_run_writes_nothing_and_says_the_same_thing() {
    let directory = TestDirectory::create();
    let store = cohort(&directory, 6, 4);
    let before = store.records.len();
    drop(store);
    let (summary, code) = run(&directory, &["--dry-run"]);
    assert_eq!(code, 0);
    assert_eq!(summary.get("dry_run").and_then(Value::as_bool), Some(true));
    assert_eq!(summary.get("migrated").and_then(Value::as_num), Some("0"));
    assert_eq!(summary.get("skipped").and_then(Value::as_num), Some("4"));
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(store.records.len(), before, "a question writes nothing");
}

#[test]
fn an_interrupted_run_resumes_instead_of_migrating_twice() {
    let directory = TestDirectory::create();
    drop(cohort(&directory, 10, 0));
    // Half the cohort, as an interrupted run would leave it.
    let (first, _) = run(&directory, &["--limit=5"]);
    assert_eq!(first.get("migrated").and_then(Value::as_num), Some("5"));
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(migrations(&store), 5);
    drop(store);

    // Re-running finishes it. The cohort is "every instance still on the
    // from machine", so the five already moved are no longer in it — the
    // property that matters is that ten instances leave ten records, never
    // fifteen.
    let (second, code) = run(&directory, &[]);
    assert_eq!(code, 0);
    assert_eq!(second.get("migrated").and_then(Value::as_num), Some("5"));
    assert_eq!(second.get("cohort").and_then(Value::as_num), Some("5"));
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(migrations(&store), 10, "ten instances, ten records");
    drop(store);

    // And a third run has nothing to do.
    let (third, code) = run(&directory, &[]);
    assert_eq!(code, 0);
    assert_eq!(third.get("cohort").and_then(Value::as_num), Some("0"));
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(migrations(&store), 10);
}

#[test]
fn the_derived_key_is_what_makes_resumption_free() {
    // The key comes from content the journal already holds, so re-issuing it
    // replays rather than migrating a second time — which is what a resumed
    // run relies on when its cohort selection happens to include an instance
    // it already moved.
    let directory = TestDirectory::create();
    let mut store = cohort(&directory, 1, 0);
    let target = store
        .resolve_machine("cohort_v2")
        .unwrap()
        .compiled
        .machine_id
        .clone();
    let request_id = format!("migrate-inst-00-{target}");
    let first = store
        .migrate_instance("inst-00", "cohort_v2", &request_id)
        .unwrap();
    let records = store.records.len();
    let again = store
        .migrate_instance("inst-00", "cohort_v2", &request_id)
        .unwrap();
    assert_eq!(again.get("duplicate").and_then(Value::as_bool), Some(true));
    assert_eq!(again.get("seq"), first.get("seq"));
    assert_eq!(store.records.len(), records, "nothing was written twice");
}

#[test]
fn a_limit_moves_exactly_that_many() {
    let directory = TestDirectory::create();
    drop(cohort(&directory, 10, 0));
    let (summary, _) = run(&directory, &["--limit=3"]);
    assert_eq!(summary.get("migrated").and_then(Value::as_num), Some("3"));
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(migrations(&store), 3);
    let moved = store
        .state
        .instances
        .values()
        .filter(|state| state.ctx["score"].canonical_string() == "2")
        .count();
    assert_eq!(moved, 3, "the rest are untouched");
}

#[test]
fn the_output_carries_identifiers_only() {
    let directory = TestDirectory::create();
    drop(cohort(&directory, 3, 1));
    let (summary, _) = run(&directory, &[]);
    let rendered = String::from_utf8(fsm_core::canon::canon_bytes(&summary)).unwrap();
    let path = directory.path().display().to_string();
    assert!(!rendered.contains(&path), "no paths: {rendered}");
    assert!(!rendered.contains("/tmp"), "no temp dirs: {rendered}");
    assert!(
        !rendered.contains("ms\"") && !rendered.contains("elapsed"),
        "no durations: {rendered}"
    );
}

#[test]
fn a_read_only_store_refuses_before_it_previews() {
    let directory = TestDirectory::create();
    let store = cohort(&directory, 2, 0);
    drop(store);
    // Hold the writer, so the command's own open is refused.
    let _holder = Store::open(directory.path()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["migrate", "--from=cohort_v1", "--to=cohort_v2", "--json"])
        .arg(format!("--data-dir={}", directory.path().display()))
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    assert!(!output.status.success(), "another writer holds the store");
    let rendered = String::from_utf8_lossy(&output.stderr);
    assert!(rendered.contains("store/lock"), "{rendered}");
}

#[test]
fn the_help_text_says_it_is_not_atomic() {
    let output = Command::new(env!("CARGO_BIN_EXE_fsm"))
        .args(["--help"])
        .env("NO_COLOR", "1")
        .output()
        .unwrap();
    let rendered = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        rendered.contains("Not atomic"),
        "an operator reads the help before the docs: {rendered}"
    );
}
