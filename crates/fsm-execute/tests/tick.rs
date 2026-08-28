//! The tick driver, end to end: a triggered instance, a real handler, and the
//! exact ordered action lines the loop produces.
//!
//! Separate from `pipeline.rs` because it is a different subject — that file
//! is about what one settle writes to the journal, this one is about the
//! driver that decides when to settle and who owns the writer while it does.
//!
//! The stub handler is this test binary re-executed, as `crash_harness.rs`
//! does it: CI runs the suite on Windows as a full test leg, so a `.sh`
//! fixture would be a red job rather than a fixture.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::config::HandlerTable;
use fsm_execute::rid::{ack_rid, event_rid};
use fsm_execute::run::{Pipeline, Runner};
use fsm_execute::sched::Scheduler;
use fsm_execute::service::{tick, tick_with};
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
                "fsm-execute-tick-{test_name}-{}-{sequence}",
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

/// The stub handler, as in `run.rs`: this test binary re-executed with a
/// marker argument the harness ignores.
#[test]
fn stub_handler() {
    if std::env::args().any(|argument| argument == "stub:ok") {
        std::process::exit(0);
    }
}

fn stub_table() -> HandlerTable {
    let stub = std::env::current_exe()
        .expect("the test binary knows its own path")
        .to_string_lossy()
        // A Windows path is backslash-separated, and a backslash begins an
        // escape in a JSON string: interpolating one raw makes the table
        // unparseable on that platform and nowhere else.
        .replace('\\', "\\\\");
    HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",
            "handlers":[{{
                "effect":"request_confirmation",
                "argv":["{stub}","stub_handler","--exact","--nocapture","stub:ok"],
                "timeout_ms":30000,
                "on_ok":{{"event":"confirmed","payload":{{}},"stamps":["at"]}},
                "on_failed":{{"event":"confirmation_failed"}}
            }}]
        }}"#
    ))
    .expect("the stub table validates")
}

/// A machine whose confirmation state is terminal, so one effect drives one
/// instance from trigger to finish.
fn tick_machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"order_confirmation_tick",
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

/// Drive a writer into "one effect pending", then hand the directory back with
/// no handle held: `tick` opens its own writer, and the advisory lock is per
/// data dir rather than per process.
fn triggered_instance(test_name: &str) -> (TestDirectory, String) {
    let directory = TestDirectory::create(test_name);
    let mut store = open_writer(directory.path());
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, tick_machine(), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "order_confirmation_tick",
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
    let effect_id = store.state.instances["order-1"].pending[0].clone();
    drop(store);
    (directory, effect_id)
}

fn expected_trace(effect_id: &str) -> Vec<String> {
    vec![
        format!("observed pending request_confirmation {effect_id}"),
        format!("spawned handler request_confirmation {effect_id}"),
        format!("acked ok {effect_id} request_id={}", ack_rid(effect_id)),
        format!(
            "sent confirmed order-1 request_id={}",
            event_rid(effect_id, "confirmed")
        ),
        "instance order-1 completed".to_string(),
    ]
}

#[test]
fn ticking_drives_a_triggered_instance_to_terminal_and_frees_the_lock() {
    let (directory, effect_id) = triggered_instance("pipe-tick");
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&stub_table()),
    );
    let mut scheduler = Scheduler::new(stub_table());
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(5_000, 1);

    let mut lines = Vec::new();
    for _ in 0..40 {
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
        // Immediately after a tick returns, the writer lock is free: nothing
        // is held across the interval, which is what lets the CLI or an MCP
        // writer act between ticks.
        drop(open_writer(directory.path()));
        if lines.iter().any(|line| line.starts_with("instance ")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(lines, expected_trace(&effect_id));
}

#[test]
fn a_lent_writer_produces_the_identical_trace_without_opening_anything() {
    let (directory, effect_id) = triggered_instance("pipe-tick-embedded");
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&stub_table()),
    );
    let mut scheduler = Scheduler::new(stub_table());
    let mut runner = Runner::new().unwrap();
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(5_000, 1);
    // Embedded mode: the caller already holds the one writer, exactly as
    // `fsm serve` does, and a second `Store::open` would collide with it.
    let mut store = Store::open(directory.path()).unwrap();

    let mut lines = Vec::new();
    for _ in 0..40 {
        let now_ms = clock.now;
        lines.extend(tick_with(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            &mut store,
            &mut clock,
            now_ms,
        ));
        if lines.iter().any(|line| line.starts_with("instance ")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(lines, expected_trace(&effect_id));
}
