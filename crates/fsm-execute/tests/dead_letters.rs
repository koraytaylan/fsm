//! Exhaustion, and the report that finds what it left behind.
//!
//! The subject is one property with two halves. A handler that uses up its
//! retry budget is acked `failed` like any other failure, so a machine that
//! models a failure path keeps working unchanged; and because a machine that
//! models **no** failure path stalls deliberately instead, the exhausted
//! effects have to be findable from outside. The report is a derivation over
//! the journal — nothing here writes a queue, a marker, or a second copy of
//! what the ack already says.
//!
//! The stub handler is this test binary re-executed, as `tick.rs` does it: CI
//! runs the suite on Windows as a full test leg, so a `.sh` fixture would be a
//! red job rather than a fixture.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::Status;
use fsm_core::record::RecordKind;
use fsm_execute::config::HandlerTable;
use fsm_execute::dead;
use fsm_execute::run::{Pipeline, Runner};
use fsm_execute::sched::Scheduler;
use fsm_execute::service::tick;
use fsm_execute::watch::Watcher;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-execute-dead-{test_name}-{}-{sequence}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory {path:?}: {error}"),
            }
        }
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

/// The stub handler: this test binary re-executed with a marker argument.
///
/// `stub:count <path>` is the one that needs a file — a handler that fails
/// once and then succeeds cannot be expressed in argv alone, and the point of
/// the fixture is that the *second* run behaves differently from the first.
#[test]
fn stub_handler() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|argument| argument == "stub:ok") {
        std::process::exit(0);
    }
    if args.iter().any(|argument| argument == "stub:fail") {
        std::process::exit(3);
    }
    if let Some(index) = args.iter().position(|argument| argument == "stub:count") {
        let Some(path) = args.get(index + 1) else {
            std::process::exit(9);
        };
        let previous = fs::read_to_string(path)
            .ok()
            .and_then(|text| text.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let attempt = previous + 1;
        fs::write(path, attempt.to_string()).expect("the counter file is writable");
        std::process::exit(if attempt >= 2 { 0 } else { 3 });
    }
}

fn stub_path() -> String {
    std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        .into_owned()
}

/// A table whose one handler runs the stub with `marker`, retrying `attempts`
/// times over the classes in `on`.
///
/// `backoff_ms` is one millisecond throughout: the schedule itself is
/// `backoff.rs`'s subject, and a real wait here would only make this suite
/// slow.
fn table(marker: &str, attempts: u32, on: &str, on_failed: bool) -> HandlerTable {
    // Escaped, because it is about to be interpolated into JSON; the raw
    // form is what `stub_path()` returns for the assertions that read output.
    let stub = stub_path().replace('\\', "\\\\");
    let failure_path = if on_failed {
        r#","on_failed":{"event":"notify_failed"}"#
    } else {
        ""
    };
    HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",
            "handlers":[{{
                "effect":"notify",
                "argv":["{stub}","stub_handler","--exact","--nocapture",{marker}],
                "timeout_ms":30000,
                "retry":{{"attempts":{attempts},"backoff_ms":1,"max_backoff_ms":10,"on":{on}}},
                "on_ok":{{"event":"notified"}}{failure_path}
            }}]
        }}"#
    ))
    .expect("the stub table validates")
}

/// A review workflow whose notification is the one effect: one failure path
/// declared, one terminal state per outcome.
fn review_machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"review_dispatch",
            "context":[{"name":"case_ref","ty":"str","init":"case-7"}],
            "events":[
                {"name":"open","fields":[]},
                {"name":"notified","fields":[]},
                {"name":"notify_failed","fields":[]}
            ],
            "effects":[{"name":"notify","fields":[{"name":"case","ty":"str"}]}],
            "states":[
                {"name":"intake"},
                {"name":"notifying","entry":{"emit":[
                    {"effect":"notify","args":{"case":"ctx.case_ref"}}
                ]}},
                {"name":"reviewer_notified","terminal":true},
                {"name":"reviewer_unreachable","terminal":true}
            ],
            "initial":"intake",
            "transitions":[
                {"from":"intake","on":"open","to":"notifying"},
                {"from":"notifying","on":"notified","to":"reviewer_notified"},
                {"from":"notifying","on":"notify_failed","to":"reviewer_unreachable"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("the review machine parses")
}

/// Drive a writer into "one notification pending", then let the lock go.
fn pending_notification(test_name: &str) -> (TestDirectory, String) {
    let directory = TestDirectory::create(test_name);
    let mut store = Store::open(directory.path()).expect("a fresh directory opens");
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, review_machine(), false, false)
        .expect("the machine defines");
    store
        .create_instance_ctx_on(
            &mut clock,
            "review_dispatch",
            "case-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .expect("the instance is created");
    store
        .send_event_stamp_on(
            &mut clock,
            "case-1",
            "open",
            &mut Value::Obj(BTreeMap::new()),
            "req-open",
            None,
            &[],
        )
        .expect("opening the case emits the notification");
    let effect_id = store.state.instances["case-1"].pending[0].clone();
    drop(store);
    (directory, effect_id)
}

/// Tick until the effect is out of the outbox, and return every action line.
///
/// `now_ms` runs ahead of the store's own clock on purpose: the backoff
/// deadline is computed from the attempt record's timestamp, so a `now_ms`
/// that advances faster than the clock takes the wait out of the picture
/// without taking the *record* out of it. What is under test here is
/// exhaustion, not the schedule.
fn run_to_settled(directory: &TestDirectory, table: HandlerTable) -> Vec<String> {
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&table),
    );
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new().expect("the runner makes its scratch directory");
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(5_000, 1);
    let mut now_ms = 5_000_i64;
    let mut lines = Vec::new();
    for _ in 0..120 {
        lines.extend(tick(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            directory.path(),
            &mut clock,
            now_ms,
        ));
        now_ms += 100;
        if lines.iter().any(|line| line.starts_with("acked ")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    lines
}

fn read_only(directory: &TestDirectory) -> Store {
    Store::open_read_only(directory.path()).expect("the store opens for reading")
}

fn records_of(store: &Store, kind: RecordKind, effect_id: &str) -> Vec<Value> {
    store
        .records
        .iter()
        .filter(|record| record.kind == kind)
        .filter(|record| record.body.get("effect_id").and_then(Value::as_str) == Some(effect_id))
        .map(|record| record.body.clone())
        .collect()
}

/// The one active state of a sequential machine.
fn leaf(instance: &fsm_core::machine::InstanceState) -> &str {
    instance
        .configuration
        .sequential_leaf()
        .expect("the review machine is sequential")
}

fn result_of(ack: &Value) -> &Value {
    ack.get("result").expect("an executor ack carries a result")
}

fn field(value: &Value, name: &str) -> String {
    match value.get(name) {
        Some(Value::Str(text)) => text.clone(),
        Some(Value::Num(number)) => number.clone(),
        other => panic!("{name} is {other:?}"),
    }
}

#[test]
fn every_attempt_failing_leaves_two_attempt_records_and_one_exhausted_ack() {
    let (directory, effect_id) = pending_notification("exhausts");
    let lines = run_to_settled(
        &directory,
        table("\"stub:fail\"", 3, r#"["nonzero_exit"]"#, true),
    );
    let store = read_only(&directory);

    // Two records, not three: the last attempt is the ack itself. Journaling
    // both would say the effect failed four times.
    let attempts = records_of(&store, RecordKind::EffectAttempted, &effect_id);
    assert_eq!(attempts.len(), 2, "attempt records in {lines:?}");
    assert_eq!(field(&attempts[0], "attempt"), "1");
    assert_eq!(field(&attempts[1], "attempt"), "2");
    for attempt in &attempts {
        assert_eq!(field(attempt, "outcome"), "failed");
    }

    let acks = records_of(&store, RecordKind::EffectAcked, &effect_id);
    assert_eq!(acks.len(), 1, "acks in {lines:?}");
    assert_eq!(field(&acks[0], "outcome"), "failed");
    let result = result_of(&acks[0]);
    assert_eq!(field(result, "error"), "exec/retries_exhausted");
    assert_eq!(field(result, "attempts"), "3");
    // The cause `error` was carrying before exhaustion replaced it.
    assert_eq!(field(result, "class"), "nonzero_exit");
    // And the last run's capture survives whole.
    assert_eq!(field(result, "status"), "3");
}

#[test]
fn the_declared_failure_path_still_fires_after_exhaustion() {
    let (directory, _) = pending_notification("on-failed");
    run_to_settled(
        &directory,
        table("\"stub:fail\"", 3, r#"["nonzero_exit"]"#, true),
    );
    let store = read_only(&directory);
    let instance = &store.state.instances["case-1"];
    // Exhaustion is an ordinary failure from the machine's point of view, so a
    // definition that models one keeps working with no change at all.
    assert_eq!(leaf(instance), "reviewer_unreachable");
    assert_eq!(instance.status, Status::Completed);
}

#[test]
fn a_handler_with_no_failure_path_stalls_and_the_report_is_how_it_is_found() {
    let (directory, effect_id) = pending_notification("no-on-failed");
    run_to_settled(
        &directory,
        table("\"stub:fail\"", 3, r#"["nonzero_exit"]"#, false),
    );
    let store = read_only(&directory);
    let instance = &store.state.instances["case-1"];
    // Plan 0008's rule, unchanged: an undeclared failure path is a deliberate
    // stall, not an omission the executor may repair.
    assert_eq!(leaf(instance), "notifying");
    assert_eq!(instance.status, Status::Running);
    // The ack still cleared the effect, so nothing in the instance says why it
    // is sitting there. This is exactly the case the report exists for.
    assert!(instance.pending.is_empty());

    let letters = dead::dead_letters(&store, 0);
    assert_eq!(letters.len(), 1);
    assert_eq!(letters[0].instance_id, "case-1");
    assert_eq!(letters[0].effect_id, effect_id);
    assert_eq!(letters[0].effect_name.as_deref(), Some("notify"));
    assert_eq!(letters[0].attempts, 3);
    assert_eq!(letters[0].class, "nonzero_exit");
    assert_eq!(field(&letters[0].result, "status"), "3");
}

#[test]
fn a_store_with_nothing_exhausted_reports_nothing() {
    let (directory, _) = pending_notification("clean");
    run_to_settled(
        &directory,
        table("\"stub:ok\"", 3, r#"["nonzero_exit"]"#, true),
    );
    let store = read_only(&directory);
    assert!(dead::dead_letters(&store, 0).is_empty());
}

#[test]
fn since_bounds_the_report() {
    let (directory, _) = pending_notification("since");
    run_to_settled(
        &directory,
        table("\"stub:fail\"", 2, r#"["nonzero_exit"]"#, true),
    );
    let store = read_only(&directory);
    let letters = dead::dead_letters(&store, 0);
    assert_eq!(letters.len(), 1);
    let seq = letters[0].seq;
    // Exclusive: asking again from the newest entry an operator has seen
    // returns what has died *since*, which is nothing.
    assert!(dead::dead_letters(&store, seq).is_empty());
    assert_eq!(dead::dead_letters(&store, seq - 1).len(), 1);
}

#[test]
fn a_failure_of_a_class_the_policy_does_not_retry_is_not_a_dead_letter() {
    let (directory, effect_id) = pending_notification("unretried-class");
    let lines = run_to_settled(
        &directory,
        // A budget of three, spent on nothing: this handler exits non-zero and
        // the policy only retries timeouts.
        table("\"stub:fail\"", 3, r#"["timeout"]"#, true),
    );
    let store = read_only(&directory);
    assert!(
        records_of(&store, RecordKind::EffectAttempted, &effect_id).is_empty(),
        "a class outside `on` is never retried: {lines:?}"
    );
    let acks = records_of(&store, RecordKind::EffectAcked, &effect_id);
    assert_eq!(acks.len(), 1);
    assert_eq!(field(&acks[0], "outcome"), "failed");
    // Acked failed, and its result says only what the run did. The two
    // failures look alike from a distance, which is why this is asserted.
    assert_eq!(field(result_of(&acks[0]), "status"), "3");
    assert!(result_of(&acks[0]).get("error").is_none());
    assert!(dead::dead_letters(&store, 0).is_empty());
}

#[test]
fn succeeding_on_the_second_attempt_leaves_one_record_and_no_dead_letter() {
    let (directory, effect_id) = pending_notification("recovers");
    let counter = directory.path().join("attempts.txt");
    // The counter path is interpolated into the table as JSON too, so it
    // needs the same escaping the stub path gets.
    let marker = format!(
        "\"stub:count\",\"{}\"",
        counter.to_string_lossy().replace('\\', "\\\\")
    );
    run_to_settled(&directory, table(&marker, 3, r#"["nonzero_exit"]"#, true));
    let store = read_only(&directory);

    let attempts = records_of(&store, RecordKind::EffectAttempted, &effect_id);
    assert_eq!(attempts.len(), 1);
    assert_eq!(field(&attempts[0], "attempt"), "1");

    let acks = records_of(&store, RecordKind::EffectAcked, &effect_id);
    assert_eq!(acks.len(), 1);
    assert_eq!(field(&acks[0], "outcome"), "ok");
    assert!(dead::dead_letters(&store, 0).is_empty());
    assert_eq!(leaf(&store.state.instances["case-1"]), "reviewer_notified");
}

#[test]
fn the_report_answers_while_the_executor_holds_the_writer() {
    let (directory, _) = pending_notification("live-writer");
    run_to_settled(
        &directory,
        table("\"stub:fail\"", 2, r#"["nonzero_exit"]"#, true),
    );
    // A writer, held for the whole read: the report takes no lock, which is
    // what makes it safe to ask an operator's question of a running system.
    let writer = Store::open(directory.path()).expect("the writer lock is free");
    let letters = dead::report(directory.path(), 0).expect("the report reads through the lock");
    assert_eq!(letters.len(), 1);
    drop(writer);
}

#[test]
fn the_exhaustion_line_carries_identifiers_only() {
    let (directory, effect_id) = pending_notification("log-line");
    let lines = run_to_settled(
        &directory,
        table("\"stub:fail\"", 3, r#"["nonzero_exit"]"#, true),
    );
    assert!(
        lines.contains(&format!("exhausted notify {effect_id} case-1 attempts=3")),
        "{lines:?}"
    );
    // No path, no pid, no temporary directory, and no duration reaches the
    // trace: this stream is byte-compared by the golden session.
    let scratch = directory.path().to_string_lossy().into_owned();
    for line in &lines {
        assert!(!line.contains(&scratch), "{line}");
        assert!(!line.contains(&stub_path()), "{line}");
    }
}

#[test]
fn each_attempt_before_the_last_is_traced_with_its_own_key() {
    let (directory, effect_id) = pending_notification("attempt-lines");
    let lines = run_to_settled(
        &directory,
        table("\"stub:fail\"", 3, r#"["nonzero_exit"]"#, true),
    );
    for attempt in 1..=2 {
        assert!(
            lines.contains(&format!(
                "attempt {attempt} failed {effect_id} request_id={}",
                fsm_execute::rid::attempt_rid(&effect_id, attempt)
            )),
            "{lines:?}"
        );
    }
}
