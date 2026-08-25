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
}

fn stub_table_json() -> String {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        .into_owned();
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
