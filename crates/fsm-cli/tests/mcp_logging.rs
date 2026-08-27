//! A client sets a level and gets what it asked for — and the terminal keeps
//! everything, because the two audiences are different.
//!
//! Plan 0012 task 6001.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::logging::{DEFAULT_LEVEL, Level};
use fsm_cli::mcp::notify::{Notifier, SharedSink};
use fsm_cli::mcp::serve::{ExecutorLoop, serve_session, serve_session_with};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-log-{}-{n}", std::process::id()));
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

const HELLO: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

/// A machine whose entry emits an effect, so an embedded tick has work.
const CASE: &str = r#"{"format":"fsm.machine/1","name":"log_case","states":[{"name":"intake"},{"name":"working","entry":{"emit":[{"effect":"notify"}]}}],"initial":"intake","context":[],"events":[{"name":"go","fields":[]}],"effects":[{"name":"notify","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"working"}]}"#;

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn session(store: &mut Store, lines: &[String]) -> SharedSink {
    let sink = SharedSink::new();
    let input = format!("{HELLO}\n{}\n", lines.join("\n"));
    serve_session(
        store.into(),
        &mut FixedClock::new(2_000, 1),
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();
    sink
}

fn replies(sink: &SharedSink) -> Vec<Value> {
    sink.text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .collect()
}

fn set_level(id: u64, level: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"logging/setLevel","params":{{"level":"{level}"}}}}"#
    )
}

#[test]
fn every_named_level_is_accepted_and_an_unknown_one_lists_them_all() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let names = [
        "debug",
        "info",
        "notice",
        "warning",
        "error",
        "critical",
        "alert",
        "emergency",
    ];
    let lines: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(index, level)| set_level(index as u64 + 2, level))
        .collect();
    let sink = session(&mut store, &lines);
    for reply in replies(&sink).iter().filter(|r| r.get("id").is_some()) {
        assert!(reply.get("error").is_none(), "{reply:?}");
    }
    // And an unknown one names all eight.
    let sink = session(&mut store, &[set_level(2, "chatty")]);
    let error = replies(&sink)
        .into_iter()
        .find_map(|reply| reply.get("error").cloned())
        .expect("refused");
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    for name in names {
        assert!(message.contains(name), "{name} missing from {message}");
    }
}

#[test]
fn the_default_is_info_and_a_level_takes_effect_immediately() {
    assert_eq!(DEFAULT_LEVEL, Level::Info);
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let data = || Value::Obj(BTreeMap::from([("k".to_string(), Value::Num("1".into()))]));

    // Below the default: nothing.
    fsm_cli::mcp::logging::message(&notifier, None, true, Level::Debug, "fsm.serve", data);
    assert!(sink.text().is_empty(), "a debug message on an info session");

    // At the default: sent.
    fsm_cli::mcp::logging::message(&notifier, None, true, Level::Warning, "fsm.serve", data);
    assert_eq!(sink.text().lines().count(), 1);

    // Raise the threshold and the same debug message arrives, with no
    // restart of anything.
    fsm_cli::mcp::logging::message(
        &notifier,
        Some(Level::Debug),
        true,
        Level::Debug,
        "fsm.serve",
        data,
    );
    assert_eq!(sink.text().lines().count(), 2);
}

#[test]
fn nothing_is_sent_before_initialize_completes() {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    fsm_cli::mcp::logging::message(
        &notifier,
        Some(Level::Debug),
        false,
        Level::Error,
        "fsm.serve",
        || Value::Obj(BTreeMap::new()),
    );
    assert!(
        sink.text().is_empty(),
        "a notification to a client that has not negotiated the capability is a protocol error"
    );
}

#[test]
fn an_embedded_tick_reaches_the_client_as_well_as_the_terminal() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "log_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();

    // A handler table with no handler for `notify`: the tick reports the
    // unhandled effect, which is a line either way.
    let table = fsm_execute::config::HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"other","argv":["/bin/true"],"timeout_ms":1000}]}"#,
    )
    .unwrap();
    let mut executor = ExecutorLoop::new(directory.path(), table).unwrap();
    let sink = SharedSink::new();
    let lines = format!(
        "{HELLO}\n{}\n{}\n",
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#
    );
    serve_session_with(
        Some(&mut store),
        &mut FixedClock::new(5_000, 1),
        Some(&mut executor),
        None,
        Cursor::new(lines.into_bytes()),
        sink.writer(),
    )
    .unwrap();

    let logged: Vec<Value> = replies(&sink)
        .into_iter()
        .filter(|message| {
            message.get("method").and_then(Value::as_str) == Some("notifications/message")
        })
        .collect();
    assert!(
        !logged.is_empty(),
        "the tick said something: {}",
        sink.text()
    );
    for message in &logged {
        let params = message.get("params").expect("params");
        assert_eq!(
            params.get("logger").and_then(Value::as_str),
            Some("fsm.execute")
        );
        assert_eq!(params.get("level").and_then(Value::as_str), Some("info"));
        assert!(
            params.get("data").and_then(Value::as_obj).is_some(),
            "structured, not a rendered sentence"
        );
    }
}

#[test]
fn a_logged_line_carries_identifiers_only() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "log_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
        .send_event("inst-1", "go", Value::Obj(BTreeMap::new()), "go-1", None)
        .unwrap();
    let table = fsm_execute::config::HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"other","argv":["/bin/true"],"timeout_ms":1000}]}"#,
    )
    .unwrap();
    let mut executor = ExecutorLoop::new(directory.path(), table).unwrap();
    let sink = SharedSink::new();
    serve_session_with(
        Some(&mut store),
        &mut FixedClock::new(5_000, 1),
        Some(&mut executor),
        None,
        Cursor::new(
            format!(
                "{HELLO}\n{}\n",
                r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#
            )
            .into_bytes(),
        ),
        sink.writer(),
    )
    .unwrap();
    let rendered = sink.text();
    let path = directory.path().display().to_string();
    assert!(!rendered.contains(&path), "no paths");
    assert!(!rendered.contains("/tmp/"), "no temp dirs");
    assert!(
        !rendered.contains("elapsed") && !rendered.contains("duration"),
        "no durations"
    );
}

#[test]
fn a_quiet_session_says_nothing() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let sink = session(
        &mut store,
        &[r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string()],
    );
    for message in replies(&sink) {
        assert!(
            message.get("method").is_none(),
            "a session with no producers emits nothing: {message:?}"
        );
    }
}
