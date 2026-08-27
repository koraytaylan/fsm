//! What cancellation can do, and the one thing it cannot.
//!
//! Plan 0012 task 6003.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::serve_session;
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn create() -> Self {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("fsm-cancel-{}-{n}", std::process::id()));
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"cancel_case","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]},{"name":"back","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"back","to":"a"}]}"#;

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn cancelled(id: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","method":"notifications/cancelled","params":{{"requestId":{id},"reason":"user"}}}}"#
    )
}

/// A store with the machine defined and one instance.
fn ready(directory: &TestDirectory) -> Store {
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "cancel_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn run(store: &mut Store, lines: &[String]) -> (Vec<Value>, usize) {
    let sink = SharedSink::new();
    let input = format!("{HELLO}\n{}\n", lines.join("\n"));
    serve_session(
        Some(store),
        &mut FixedClock::new(5_000, 1),
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();
    let messages: Vec<Value> = sink
        .text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .collect();
    let count = sink.text().lines().count();
    (messages, count)
}

fn reply(messages: &[Value], id: &str) -> Option<Value> {
    messages
        .iter()
        .find(|message| {
            message
                .get("id")
                .and_then(Value::as_num)
                .or_else(|| message.get("id").and_then(Value::as_str))
                == Some(id)
        })
        .cloned()
}

fn simulate_call(id: u64, count: usize) -> String {
    let events: Vec<String> = (0..count)
        .map(|index| {
            let name = if index % 2 == 0 { "go" } else { "back" };
            format!(r#"{{"name":"{name}"}}"#)
        })
        .collect();
    format!(
        r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"simulate","arguments":{{"machine":"cancel_case","events":[{}]}}}}}}"#,
        events.join(",")
    )
}

#[test]
fn a_request_cancelled_before_dispatch_is_never_answered() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let (messages, _) = run(
        &mut store,
        &[
            cancelled("2"),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#.to_string(),
        ],
    );
    assert!(
        reply(&messages, "2").is_none(),
        "a request that was never executed gets nothing — not an error, not a courtesy reply"
    );
    assert!(
        reply(&messages, "3").is_some(),
        "and the next one is answered"
    );
}

#[test]
fn the_cancellation_itself_is_never_answered() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let (messages, count) = run(&mut store, &[cancelled("99")]);
    // The initialize reply and nothing else.
    assert_eq!(count, 1, "{messages:?}");
    // And a cancellation for an unknown id changes nothing.
    let (messages, _) = run(
        &mut store,
        &[
            cancelled("99"),
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#.to_string(),
        ],
    );
    assert!(reply(&messages, "2").is_some());
}

#[test]
fn a_cancelled_id_is_cleared_after_it_is_used() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let (messages, _) = run(
        &mut store,
        &[
            cancelled("5"),
            r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":5,"method":"tools/list"}"#.to_string(),
        ],
    );
    // The first id 5 was skipped; the second executes, so exactly one reply
    // carries that id.
    let answered = messages
        .iter()
        .filter(|message| message.get("id").and_then(Value::as_num) == Some("5"))
        .count();
    assert_eq!(
        answered, 1,
        "a client reusing an id must not be silently cancelled by a stale entry"
    );
}

#[test]
fn a_cancelled_call_returns_a_tool_error_and_stops_early() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    // Cancelled *before* the call, so the flag is set when the loop checks
    // it: the observable outcome — a tool error and no completed work — is
    // the same one a mid-call cancellation produces at the same boundary.
    let (messages, _) = run(&mut store, &[cancelled("2"), simulate_call(2, 20)]);
    // Cancelled before dispatch, so it is skipped entirely.
    assert!(reply(&messages, "2").is_none());

    // And through the dispatcher directly, with the flag already set: the
    // call is dispatched and stops at its first boundary.
    let mut cancellations = fsm_cli::mcp::cancel::Cancellations::default();
    let id = Value::Num("9".into());
    cancellations.cancel(&id);
    let ctx = fsm_cli::mcp::tools::ToolCtx {
        notifier: None,
        request_id: Some(id.clone()),
        meta: None,
        cancel: cancellations.flag(&id),
        ..Default::default()
    };
    let error = fsm_cli::mcp::tools::dispatch_with(
        &mut store,
        &mut FixedClock::new(5_000, 1),
        "simulate",
        &value(r#"{"machine":"cancel_case","events":[{"name":"go"},{"name":"back"}]}"#),
        &ctx,
    )
    .expect_err("the client withdrew it");
    assert_eq!(error.code, "req/cancelled");
    assert!(
        error.hint.contains("not interruptible"),
        "the documented limit travels with the refusal: {}",
        error.hint
    );
}

#[test]
fn a_cancelled_history_call_stops_between_chunks() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    for index in 0..30 {
        let event = if index % 2 == 0 { "go" } else { "back" };
        store
            .send_event(
                "inst-1",
                event,
                Value::Obj(BTreeMap::new()),
                &format!("e{index}"),
                None,
            )
            .unwrap();
    }
    let mut cancellations = fsm_cli::mcp::cancel::Cancellations::default();
    let id = Value::Num("4".into());
    cancellations.cancel(&id);
    let ctx = fsm_cli::mcp::tools::ToolCtx {
        notifier: None,
        request_id: Some(id.clone()),
        meta: None,
        cancel: cancellations.flag(&id),
        ..Default::default()
    };
    let error = fsm_cli::mcp::tools::dispatch_with(
        &mut store,
        &mut FixedClock::new(5_000, 1),
        "instance_history",
        &value(r#"{"instance_id":"inst-1"}"#),
        &ctx,
    )
    .expect_err("withdrawn");
    assert_eq!(error.code, "req/cancelled");
}

#[test]
fn a_cancelled_call_writes_nothing() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let before = store.records.len();
    let (_, _) = run(
        &mut store,
        &[
            cancelled("2"),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-1","event":{"name":"go"},"request_id":"cancel-me"}}}"#.to_string(),
        ],
    );
    drop(store);
    let reopened = Store::open(directory.path()).unwrap();
    assert_eq!(
        reopened.records.len(),
        before,
        "a call that never ran journaled nothing"
    );
}

#[test]
fn a_single_step_is_not_interrupted_mid_call() {
    // The documented limit, pinned rather than left implicit: an engine step
    // is bounded by the evaluation budget and short by construction, and
    // threading a token through the pure core would cost the core its purity.
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let mut cancellations = fsm_cli::mcp::cancel::Cancellations::default();
    let id = Value::Num("3".into());
    cancellations.cancel(&id);
    let ctx = fsm_cli::mcp::tools::ToolCtx {
        notifier: None,
        request_id: Some(id.clone()),
        meta: None,
        cancel: cancellations.flag(&id),
        ..Default::default()
    };
    let result = fsm_cli::mcp::tools::dispatch_with(
        &mut store,
        &mut FixedClock::new(5_000, 1),
        "instance_send",
        &value(r#"{"instance_id":"inst-1","event":{"name":"go"},"request_id":"step-1"}"#),
        &ctx,
    );
    assert!(
        result.is_ok(),
        "a dispatched step runs to completion: {result:?}"
    );
    assert_eq!(
        store.state.instances["inst-1"]
            .configuration
            .sequential_leaf(),
        Some("b")
    );
}

#[test]
fn an_arriving_cancellation_is_visible_at_debug() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let (messages, _) = run(
        &mut store,
        &[
            r#"{"jsonrpc":"2.0","id":2,"method":"logging/setLevel","params":{"level":"debug"}}"#
                .to_string(),
            cancelled("7"),
            r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#.to_string(),
        ],
    );
    let logs: Vec<&Value> = messages
        .iter()
        .filter(|m| m.get("method").and_then(Value::as_str) == Some("notifications/message"))
        .collect();
    let text = format!("{logs:?}");
    assert!(
        text.contains("cancel_requested"),
        "an operator asking whether their cancel arrived can see that it did: {text}"
    );
    assert!(
        text.contains("cancelled"),
        "and that the request it named was skipped: {text}"
    );
    assert!(
        reply(&messages, "7").is_none(),
        "the skipped request still gets no response"
    );
}

#[test]
fn a_session_that_cancels_nothing_says_nothing_new() {
    let directory = TestDirectory::create();
    let mut store = ready(&directory);
    let (messages, _) = run(
        &mut store,
        &[r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#.to_string()],
    );
    for message in messages {
        assert!(
            message.get("method").is_none(),
            "no notifications on a session that cancelled nothing: {message:?}"
        );
    }
}
