//! The golden two-process session: a writer triggers a workflow, then stops
//! talking, and the executor carries it to terminal on its own.
//!
//! The point of the byte comparison is that the *executor half* is emergent.
//! Only the first three store calls are scripted; everything after them —
//! observing the pending effect, running the handler, acking, sending the
//! advance the table declares — comes out of the loop, and the expected stream
//! was hand-derived from the design rather than captured from a run.
//!
//! The stub handler is this test binary re-executed, as `crash_harness.rs`
//! does it: CI runs the whole suite on Windows as a full test leg, so a `.sh`
//! fixture would be a red job rather than a fixture. The committed handler
//! table therefore carries a `%STUB%` placeholder — `{…}` would mean *effect
//! argument* to the substituter — which the test replaces with the resolved
//! path when it materializes the table into its temp dir. No absolute path is
//! ever committed, and none enters the golden.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::journal_io::{JournalHealth, verify};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;
use fsm_execute::config::HandlerTable;
use fsm_execute::run::{Pipeline, Runner};
use fsm_execute::sched::Scheduler;
use fsm_execute::service::tick;
use fsm_execute::watch::Watcher;
use fsm_store::clock::FixedClock;
use fsm_store::store::Store;

static TEMPORARY_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const STUB_STDOUT: &str = "supplier notified\n";

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-executor-session-{test_name}-{}-{sequence}",
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

/// The stub handler: prints one line and exits zero.
#[test]
fn stub_handler() {
    if std::env::args().any(|argument| argument == "stub:ok") {
        use std::io::Write;
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(STUB_STDOUT.as_bytes());
        let _ = stdout.flush();
        std::process::exit(0);
    }
}

/// Open a writer, tolerating a lock this process itself just released.
///
/// Spawning a handler forks, and between `fork` and `exec` the child holds a
/// copy of every open descriptor — so an advisory lock dropped a moment ago
/// can still be held for the length of that window. The property under test is
/// that the executor does not *keep* the lock, not that a fork never happened.
fn open_writer(path: &Path) -> Store {
    for _ in 0..50 {
        match Store::open(path) {
            Ok(store) => return store,
            Err(error) if error.code == "store/lock" => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Err(error) => panic!("open writer {}: {error:?}", path.display()),
        }
    }
    panic!("the writer lock on {} never became free", path.display())
}

fn machine() -> Value {
    parse(
        include_bytes!("fixtures/executor/machine.json"),
        &JsonLimits::DEFAULT,
    )
    .expect("the committed machine parses")
}

/// Materialize the committed template into this run's temp dir.
fn materialize_table(directory: &TestDirectory) -> HandlerTable {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        .into_owned();
    let template = include_str!("fixtures/executor/handlers.template.json");
    let text = template.replace("%STUB%", &stub.replace('\\', "\\\\"));
    let path = directory.path().join("handlers.json");
    fs::write(&path, &text).expect("write the materialized table");
    HandlerTable::parse(&text).expect("the materialized table validates")
}

/// The scripted half: define, create, trigger. Exactly what a chat session
/// would do before going away.
fn writer_half(directory: &TestDirectory) {
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(1_700_000_000_000, 1);
    store
        .define_machine_on(&mut clock, machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation",
            "order-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event_stamp_on(
            &mut clock,
            "order-1",
            "submit",
            &mut Value::Obj(BTreeMap::new()),
            "req-submit",
            None,
            &[],
        )
        .unwrap();
    assert_eq!(
        store.state.instances["order-1"].pending,
        ["order-1/3/0"],
        "the trigger left exactly the effect the golden names"
    );
    // `tick` opens its own writer, and the advisory lock is per data dir
    // rather than per process: a live handle here would lock the executor out
    // of every tick it takes.
    drop(store);
}

/// The emergent half: ticks, and nothing else.
fn executor_half(directory: &TestDirectory, table: HandlerTable) -> Vec<String> {
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&table),
    );
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(1_700_000_100_000, 1);
    let mut lines = Vec::new();
    for _ in 0..60 {
        let now_ms = clock.now;
        lines.extend(tick(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            directory.path(),
            &mut clock,
            now_ms,
        ));
        if lines.iter().any(|line| line.starts_with("instance ")) {
            break;
        }
        // Waiting for a child to exit, not for wall-clock time to pass: every
        // decision in the loop takes its time from the injected clock.
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    lines
}

#[test]
fn the_executor_carries_a_triggered_workflow_to_terminal() {
    let directory = TestDirectory::create("golden");
    let table = materialize_table(&directory);
    writer_half(&directory);

    let lines = executor_half(&directory, table);
    let expected = include_str!("fixtures/executor/session.expected.txt");
    assert_eq!(format!("{}\n", lines.join("\n")), expected);
}

#[test]
fn the_journal_verifies_and_shows_the_ack_before_the_advance() {
    let directory = TestDirectory::create("journal");
    let table = materialize_table(&directory);
    writer_half(&directory);
    executor_half(&directory, table);

    let report = verify(directory.path());
    assert!(
        matches!(report.health, JournalHealth::Ok),
        "{:?}",
        report.health
    );

    let store = Store::open_read_only(directory.path()).unwrap();
    let acked = store
        .records
        .iter()
        .find(|record| record.kind == RecordKind::EffectAcked)
        .expect("the executor acked the effect");
    let advance = store
        .records
        .iter()
        .find(|record| record.body.get("event").and_then(Value::as_str) == Some("confirmed"))
        .expect("the executor sent the advance");
    assert!(acked.seq < advance.seq, "ack first, then advance");

    let captured = acked
        .body
        .get("result")
        .and_then(|result| result.get("stdout"))
        .and_then(Value::as_str)
        .expect("the ack carries the handler's output");
    assert!(
        captured.contains(STUB_STDOUT),
        "the ack carries what the handler printed: {captured:?}"
    );

    let stamped = advance
        .body
        .get("payload")
        .and_then(|payload| payload.get("at"))
        .and_then(Value::as_str)
        .expect("the advance carries the stamped field the table declared");
    assert!(stamped.parse::<i64>().is_ok(), "stamped {stamped}");

    let history = store
        .history_page("order-1", 0, 100, false, true)
        .expect("history renders");
    let kinds: Vec<String> = history
        .get("entries")
        .and_then(Value::as_arr)
        .expect("history entries")
        .iter()
        .filter_map(|entry| entry.get("kind").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let ack_position = kinds
        .iter()
        .position(|kind| kind == "EffectAcked")
        .expect("history shows the ack");
    let advance_position = kinds
        .iter()
        .rposition(|kind| kind == "EventApplied")
        .expect("history shows the advance");
    assert!(ack_position < advance_position);
    assert_eq!(
        store.state.instances["order-1"].status.as_str(),
        "completed"
    );
}

#[test]
fn a_fresh_directory_reproduces_the_identical_stream() {
    // The writer half is the only scripted part; if the executor half were
    // order-dependent or clock-dependent, this second run would drift.
    let first = TestDirectory::create("repeat-a");
    let table = materialize_table(&first);
    writer_half(&first);
    let first_lines = executor_half(&first, table);

    let second = TestDirectory::create("repeat-b");
    let table = materialize_table(&second);
    writer_half(&second);
    let second_lines = executor_half(&second, table);

    assert_eq!(first_lines, second_lines);
}

#[test]
fn no_line_of_the_golden_carries_a_path_a_pid_or_a_duration() {
    // An effect id contains slashes by construction — `{instance}/{seq}/{k}` —
    // so what is being excluded is a *path*: a rooted token, a temp
    // directory, this process's pid, or a measured duration. Each of those
    // differs per machine or per run and would make the golden uncomparable.
    let expected = include_str!("fixtures/executor/session.expected.txt");
    let temporary = std::env::temp_dir().to_string_lossy().into_owned();
    for line in expected.lines() {
        for token in line.split_whitespace() {
            assert!(
                !Path::new(token).has_root(),
                "{line} carries the rooted token {token}"
            );
        }
        assert!(!line.contains(&temporary), "{line}");
        assert!(!line.contains(".exe"), "{line}");
        assert!(
            !line.contains(&format!("pid={}", std::process::id())),
            "{line} names a pid"
        );
        assert!(!line.contains("ms="), "{line} carries a duration");
    }
}
