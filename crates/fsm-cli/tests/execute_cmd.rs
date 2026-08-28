//! `fsm execute` from the outside: what the operator's pre-flight prints, what
//! a bad table costs, and that the loop leaves the writer lock alone between
//! ticks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

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

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_fsm")
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create(test_name: &str) -> Self {
        loop {
            let sequence = TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "fsm-execute-cmd-{test_name}-{}-{sequence}",
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

/// The stub handler: this test binary re-executed with a marker argument the
/// harness ignores.
#[test]
fn stub_handler() {
    if std::env::args().any(|argument| argument == "stub:ok") {
        std::process::exit(0);
    }
    if std::env::args().any(|argument| argument == "stub:fail") {
        std::process::exit(3);
    }
}

fn stub_table_json() -> String {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        // A Windows path is backslash-separated, and a backslash begins an
        // escape in a JSON string: interpolating one raw makes the table
        // unparseable on that platform and nowhere else.
        .replace('\\', "\\\\");
    format!(
        r#"{{
  "format": "fsm.handlers/1",
  "handlers": [
    {{
      "effect": "request_confirmation",
      "argv": ["{stub}", "stub_handler", "--exact", "--nocapture", "stub:ok"],
      "timeout_ms": 30000,
      "on_ok": {{"event": "confirmed", "payload": {{}}, "stamps": ["at"]}},
      "on_failed": {{"event": "confirmation_failed"}}
    }}
  ]
}}"#
    )
}

fn machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"order_confirmation_cmd",
            "context":[{"name":"order_id","ty":"str","init":"order-7"}],
            "events":[
                {"name":"submit","fields":[]},
                {"name":"confirmed","fields":[{"name":"at","ty":"timestamp"}]},
                {"name":"confirmation_failed","fields":[]}
            ],
            "effects":[{"name":"request_confirmation","fields":[{"name":"order","ty":"str"}]}],
            "states":[
                {"name":"placed"},
                {"name":"awaiting_confirmation","entry":{"emit":[
                    {"effect":"request_confirmation","args":{"order":"ctx.order_id"}}
                ]}},
                {"name":"confirmed_order","terminal":true},
                {"name":"unconfirmed","terminal":true}
            ],
            "initial":"placed",
            "transitions":[
                {"from":"placed","on":"submit","to":"awaiting_confirmation"},
                {"from":"awaiting_confirmation","on":"confirmed","to":"confirmed_order"},
                {"from":"awaiting_confirmation","on":"confirmation_failed","to":"unconfirmed"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

#[test]
fn check_mode_prints_the_resolved_table_and_touches_no_data_dir() {
    let table = TestDirectory::create("check");
    let handlers = table.path().join("handlers.json");
    fs::write(&handlers, stub_table_json()).unwrap();
    let data_dir = std::env::temp_dir().join(format!(
        "fsm-execute-cmd-never-created-{}-{}",
        std::process::id(),
        TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));

    let output = Command::new(binary())
        .args([
            "--data-dir",
            &data_dir.to_string_lossy(),
            "execute",
            "--check",
            "--handlers",
            &handlers.to_string_lossy(),
        ])
        .output()
        .expect("run fsm execute --check");
    assert!(output.status.success(), "{output:?}");
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.contains("request_confirmation"), "{printed}");
    assert!(printed.contains("confirmed"), "{printed}");
    assert!(
        !data_dir.exists(),
        "a pre-flight must not create the data dir"
    );
}

#[test]
fn a_malformed_table_fails_before_any_store_is_opened() {
    let table = TestDirectory::create("bad-table");
    let handlers = table.path().join("handlers.json");
    fs::write(
        &handlers,
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"x","argv":[],"timeout_ms":1}]}"#,
    )
    .unwrap();
    let data_dir = std::env::temp_dir().join(format!(
        "fsm-execute-cmd-never-opened-{}-{}",
        std::process::id(),
        TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));

    let output = Command::new(binary())
        .args([
            "--data-dir",
            &data_dir.to_string_lossy(),
            "execute",
            "--handlers",
            &handlers.to_string_lossy(),
        ])
        .output()
        .expect("run fsm execute");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let rendered = String::from_utf8_lossy(&output.stderr);
    assert!(rendered.contains("exec/config"), "{rendered}");
    assert!(!data_dir.exists(), "the table is validated first");
}

#[test]
fn the_json_frame_carries_the_resolved_table() {
    let table = TestDirectory::create("check-json");
    let handlers = table.path().join("handlers.json");
    fs::write(&handlers, stub_table_json()).unwrap();

    let output = Command::new(binary())
        .args([
            "--json",
            "execute",
            "--check",
            "--handlers",
            &handlers.to_string_lossy(),
        ])
        .output()
        .expect("run fsm execute --check --json");
    assert!(output.status.success(), "{output:?}");
    let framed = parse(&output.stdout, &JsonLimits::DEFAULT).expect("one canonical JSON frame");
    assert_eq!(
        framed.get("format").and_then(Value::as_str),
        Some("fsm.handlers/1")
    );
    assert_eq!(
        framed
            .get("handlers")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        Some(1)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).is_empty(),
        "the mode line belongs to a run, not a pre-flight"
    );
}

#[test]
fn a_missing_handlers_flag_is_a_usage_error() {
    let output = Command::new(binary())
        .args(["execute"])
        .output()
        .expect("run fsm execute");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("--handlers"));
}

#[test]
fn driven_ticks_journal_the_ack_and_the_advance_without_holding_the_lock() {
    // The sleeping loop is not what is under test — the tick is. Driving it
    // directly keeps the test wall-clock-free.
    let directory = TestDirectory::create("driven");
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation_cmd",
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
    // The advisory lock is per data dir, not per process: a still-live handle
    // here would lock the executor out of every tick.
    drop(store);

    let table = HandlerTable::parse(&stub_table_json()).unwrap();
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&table),
    );
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut executor_clock = FixedClock::new(5_000, 1);

    for _ in 0..40 {
        let now_ms = executor_clock.now;
        tick(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            directory.path(),
            &mut executor_clock,
            now_ms,
        );
        // Between ticks the lock is free — this open would fail with
        // `store/lock` if the loop held it across the interval.
        let opened = open_writer(directory.path());
        let done = opened.state.instances["order-1"].status.as_str() == "completed";
        drop(opened);
        if done {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let store = Store::open_read_only(directory.path()).unwrap();
    let acked = store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::EffectAcked)
        .count();
    assert_eq!(acked, 1, "exactly one ack per effect");
    let advanced = store
        .records
        .iter()
        .any(|record| record.body.get("event").and_then(Value::as_str) == Some("confirmed"));
    assert!(advanced, "the declared advance event landed");
    assert_eq!(
        store.state.instances["order-1"].status.as_str(),
        "completed"
    );
}

/// A table whose handler always fails and is retried twice, so one run of the
/// tick loop leaves exactly one dead letter behind.
fn exhausting_table_json() -> String {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        // A Windows path is backslash-separated, and a backslash begins an
        // escape in a JSON string: interpolating one raw makes the table
        // unparseable on that platform and nowhere else.
        .replace('\\', "\\\\");
    format!(
        r#"{{
  "format": "fsm.handlers/1",
  "handlers": [
    {{
      "effect": "request_confirmation",
      "argv": ["{stub}", "stub_handler", "--exact", "--nocapture", "stub:fail"],
      "timeout_ms": 30000,
      "retry": {{"attempts": 2, "backoff_ms": 1, "max_backoff_ms": 10, "on": ["nonzero_exit"]}},
      "on_ok": {{"event": "confirmed", "payload": {{}}, "stamps": ["at"]}}
    }}
  ]
}}"#
    )
}

/// Drive a store to one exhausted effect and hand back its data directory.
///
/// No `on_failed` on purpose: this is the shape the report exists for — the
/// instance is left running with nothing in its outbox to say why.
fn store_with_one_dead_letter(test_name: &str) -> TestDirectory {
    let directory = TestDirectory::create(test_name);
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation_cmd",
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
    drop(store);

    let table = HandlerTable::parse(&exhausting_table_json()).unwrap();
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&table),
    );
    let mut scheduler = Scheduler::new(table);
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut executor_clock = FixedClock::new(5_000, 1);
    let mut now_ms = 5_000_i64;
    for _ in 0..120 {
        tick(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            directory.path(),
            &mut executor_clock,
            now_ms,
        );
        now_ms += 100;
        let opened = Store::open_read_only(directory.path()).unwrap();
        let acked = opened
            .records
            .iter()
            .any(|record| record.kind == RecordKind::EffectAcked);
        drop(opened);
        if acked {
            return directory;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the handler never exhausted its retry budget");
}

fn list_dead(data_dir: &Path, extra: &[&str]) -> Value {
    let mut args = vec![
        "--json".to_string(),
        "--data-dir".to_string(),
        data_dir.to_string_lossy().into_owned(),
        "execute".to_string(),
        "--list-dead".to_string(),
    ];
    args.extend(extra.iter().map(|argument| (*argument).to_string()));
    let output = Command::new(binary())
        .args(&args)
        .output()
        .expect("run fsm execute --list-dead");
    assert!(output.status.success(), "{output:?}");
    parse(&output.stdout, &JsonLimits::DEFAULT).expect("one canonical JSON frame")
}

#[test]
fn list_dead_names_the_instance_the_effect_and_the_last_capture() {
    let directory = store_with_one_dead_letter("list-dead");
    let framed = list_dead(directory.path(), &[]);
    let letters = framed
        .get("dead_letters")
        .and_then(Value::as_arr)
        .expect("the report is an array");
    assert_eq!(letters.len(), 1, "{framed:?}");
    let letter = &letters[0];
    assert_eq!(
        letter.get("instance_id").and_then(Value::as_str),
        Some("order-1")
    );
    assert_eq!(
        letter.get("effect").and_then(Value::as_str),
        Some("request_confirmation")
    );
    assert_eq!(letter.get("attempts").and_then(Value::as_num), Some("2"));
    assert_eq!(
        letter.get("class").and_then(Value::as_str),
        Some("nonzero_exit")
    );
    // The last attempt's capture, whole: an operator reading a dead letter
    // wants the output of the run that finally gave up.
    assert_eq!(
        letter
            .get("result")
            .and_then(|result| result.get("status"))
            .and_then(Value::as_num),
        Some("3")
    );
    assert_eq!(
        letter
            .get("result")
            .and_then(|result| result.get("error"))
            .and_then(Value::as_str),
        Some("exec/retries_exhausted")
    );
}

#[test]
fn list_dead_since_bounds_the_report() {
    let directory = store_with_one_dead_letter("list-dead-since");
    let framed = list_dead(directory.path(), &[]);
    let seq = framed
        .get("dead_letters")
        .and_then(Value::as_arr)
        .and_then(|letters| letters.first())
        .and_then(|letter| letter.get("seq"))
        .and_then(Value::as_num)
        .and_then(|seq| seq.parse::<u64>().ok())
        .expect("the entry carries its record seq");

    let after = list_dead(directory.path(), &["--since", &seq.to_string()]);
    assert_eq!(after.get("count").and_then(Value::as_num), Some("0"));
    let before = list_dead(directory.path(), &["--since", &(seq - 1).to_string()]);
    assert_eq!(before.get("count").and_then(Value::as_num), Some("1"));
}

#[test]
fn list_dead_reports_none_for_a_store_that_never_gave_up() {
    let directory = TestDirectory::create("list-dead-clean");
    drop(open_writer(directory.path()));
    let framed = list_dead(directory.path(), &[]);
    assert_eq!(framed.get("count").and_then(Value::as_num), Some("0"));
    assert_eq!(
        framed.get("dead_letters").and_then(Value::as_arr),
        Some(&[][..])
    );
}

#[test]
fn list_dead_answers_while_the_executor_holds_the_writer() {
    let directory = store_with_one_dead_letter("list-dead-live");
    // Held across the whole call: the report opens read-only, which takes no
    // lock, so an operator can ask this of a running system.
    let writer = open_writer(directory.path());
    let framed = list_dead(directory.path(), &[]);
    assert_eq!(framed.get("count").and_then(Value::as_num), Some("1"));
    drop(writer);
}

#[test]
fn a_non_numeric_since_is_a_usage_error() {
    let directory = TestDirectory::create("list-dead-bad-since");
    let output = Command::new(binary())
        .args([
            "--data-dir",
            &directory.path().to_string_lossy(),
            "execute",
            "--list-dead",
            "--since",
            "yesterday",
        ])
        .output()
        .expect("run fsm execute --list-dead --since yesterday");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("--since"));
}

#[test]
fn the_pre_flight_reports_what_has_already_given_up() {
    let directory = store_with_one_dead_letter("check-dead");
    let handlers = directory.path().join("handlers.json");
    fs::write(&handlers, exhausting_table_json()).unwrap();

    let output = Command::new(binary())
        .args([
            "--json",
            "--data-dir",
            &directory.path().to_string_lossy(),
            "execute",
            "--check",
            "--handlers",
            &handlers.to_string_lossy(),
        ])
        .output()
        .expect("run fsm execute --check");
    assert!(output.status.success(), "{output:?}");
    let framed = parse(&output.stdout, &JsonLimits::DEFAULT).expect("one canonical JSON frame");
    // "Your table is valid" is only half of what a pre-flight owes an
    // operator: an effect that exhausted under the previous run is still
    // sitting there with an instance stalled behind it.
    assert_eq!(
        framed
            .get("dead_letters")
            .and_then(Value::as_arr)
            .map(<[Value]>::len),
        Some(1),
        "{framed:?}"
    );
}

/// An `fsm.handlers/1` table with the given keys spliced in, as a file.
fn write_table(directory: &TestDirectory, extra: &str) -> PathBuf {
    let handlers = directory.path().join("handlers.json");
    fs::write(
        &handlers,
        format!(
            r#"{{
  "format": "fsm.handlers/1",
  "handlers": [
    {{
      "effect": "summarize_case",
      "argv": ["/usr/local/bin/case-tools", "--stdio"],
      "timeout_ms": 60000{extra}
    }}
  ]
}}"#
        ),
    )
    .unwrap();
    handlers
}

/// Run `--check` against a data dir that does not exist, so a store read would
/// be visible as a created directory.
fn check(directory: &TestDirectory, extra: &str) -> std::process::Output {
    let handlers = write_table(directory, extra);
    let data_dir = std::env::temp_dir().join(format!(
        "fsm-execute-cmd-never-created-{}-{}",
        std::process::id(),
        TEMPORARY_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let output = Command::new(binary())
        .args([
            "--json",
            "--data-dir",
            &data_dir.to_string_lossy(),
            "execute",
            "--check",
            "--handlers",
            &handlers.to_string_lossy(),
        ])
        .output()
        .expect("run fsm execute --check");
    assert!(
        !data_dir.exists(),
        "a pre-flight must not create the data dir"
    );
    output
}

#[test]
fn the_pre_flight_resolves_an_mcp_handler_before_opening_any_store() {
    let directory = TestDirectory::create("check-mcp");
    let output = check(
        &directory,
        r#","kind":"mcp","tool":"summarize","arguments":{"case_id":"{case_id}"}"#,
    );
    assert!(output.status.success(), "{output:?}");
    let framed = parse(&output.stdout, &JsonLimits::DEFAULT).expect("one canonical JSON frame");
    let handler = framed
        .get("handlers")
        .and_then(Value::as_arr)
        .and_then(<[Value]>::first)
        .expect("one resolved handler");
    assert_eq!(handler.get("kind").and_then(Value::as_str), Some("mcp"));
    assert_eq!(
        handler.get("tool").and_then(Value::as_str),
        Some("summarize")
    );
    assert_eq!(
        handler
            .get("arguments")
            .and_then(|arguments| arguments.get("case_id"))
            .and_then(Value::as_str),
        Some("{case_id}")
    );
}

#[test]
fn the_pre_flight_reports_a_process_handler_as_one() {
    let directory = TestDirectory::create("check-process");
    let output = check(&directory, "");
    assert!(output.status.success(), "{output:?}");
    let framed = parse(&output.stdout, &JsonLimits::DEFAULT).expect("one canonical JSON frame");
    let handler = framed
        .get("handlers")
        .and_then(Value::as_arr)
        .and_then(<[Value]>::first)
        .expect("one resolved handler");
    assert_eq!(handler.get("kind").and_then(Value::as_str), Some("process"));
    assert!(
        handler.get("tool").is_none(),
        "a process handler has no tool"
    );
}

#[test]
fn every_mcp_config_fault_is_reported_by_the_pre_flight() {
    // Each of these is refused before any store is opened, which is the whole
    // point of a pre-flight: a malformed table costs an error rather than a
    // half-executed workflow.
    let faults = [
        (r#","kind":"mcp""#, "tool"),
        (r#","kind":"mcp","tool":"""#, "tool"),
        (r#","tool":"summarize""#, "tool"),
        (r#","arguments":{}"#, "arguments"),
        (r#","kind":"grpc""#, "grpc"),
        (
            r#","kind":"mcp","tool":"summarize","arguments":{"a":"{Bad}"}"#,
            "arguments.a",
        ),
        (r#","retry":{"attempts":2,"on":["mcp_error"]}"#, "mcp_error"),
    ];
    for (extra, expected) in faults {
        let directory = TestDirectory::create("check-mcp-fault");
        let output = check(&directory, extra);
        assert_eq!(output.status.code(), Some(2), "{extra}: {output:?}");
        let rendered = String::from_utf8_lossy(&output.stderr);
        assert!(rendered.contains("exec/config"), "{extra}: {rendered}");
        assert!(rendered.contains(expected), "{extra}: {rendered}");
    }
}
