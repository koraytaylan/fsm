//! A call that takes a while says so — at most ten times a second, and
//! always at the end.
//!
//! Plan 0012 task 6002.

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
        let path = std::env::temp_dir().join(format!("fsm-prog-{}-{n}", std::process::id()));
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"prog_case","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]},{"name":"back","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"back","to":"a"}]}"#;

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

/// Drive a session with an injected clock stepping `step_ms` per read.
fn session(directory: &TestDirectory, lines: &[String], step_ms: i64) -> SharedSink {
    let mut store = Store::open(directory.path()).unwrap();
    store
        .define_machine_on(&mut FixedClock::new(1_000, 1), value(CASE), false, false)
        .unwrap();
    let sink = SharedSink::new();
    let input = format!("{HELLO}\n{}\n", lines.join("\n"));
    serve_session(
        Some(&mut store),
        &mut FixedClock::new(10_000, step_ms),
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();
    sink
}

fn progress_notifications(sink: &SharedSink) -> Vec<Value> {
    sink.text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .filter(|message| {
            message.get("method").and_then(Value::as_str) == Some("notifications/progress")
        })
        .filter_map(|message| message.get("params").cloned())
        .collect()
}

/// A `simulate` call over `count` events, with an optional progress token.
fn simulate_call(count: usize, token: Option<&str>) -> String {
    let events: Vec<String> = (0..count)
        .map(|index| {
            let name = if index % 2 == 0 { "go" } else { "back" };
            format!(r#"{{"name":"{name}"}}"#)
        })
        .collect();
    let meta = token
        .map(|token| format!(r#","_meta":{{"progressToken":{token}}}"#))
        .unwrap_or_default();
    format!(
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"simulate","arguments":{{"machine":"prog_case","events":[{}]}}{meta}}}}}"#,
        events.join(",")
    )
}

#[test]
fn a_token_gets_progress_ending_at_the_total() {
    let directory = TestDirectory::create();
    let sink = session(&directory, &[simulate_call(3, Some(r#""tok-1""#))], 1_000);
    let reports = progress_notifications(&sink);
    assert!(!reports.is_empty(), "{}", sink.text());
    for report in &reports {
        assert_eq!(
            report.get("progressToken").and_then(Value::as_str),
            Some("tok-1"),
            "the token is echoed verbatim"
        );
        assert!(report.get("total").is_some(), "a denominator, always");
    }
    let last = reports.last().unwrap();
    assert_eq!(
        last.get("progress").and_then(Value::as_num),
        last.get("total").and_then(Value::as_num),
        "the final report is complete"
    );
}

#[test]
fn a_numeric_token_is_echoed_as_a_number() {
    let directory = TestDirectory::create();
    let sink = session(&directory, &[simulate_call(2, Some("7"))], 1_000);
    let reports = progress_notifications(&sink);
    assert!(!reports.is_empty());
    assert_eq!(
        reports[0].get("progressToken").and_then(Value::as_num),
        Some("7"),
        "the specification permits both, so both round-trip"
    );
}

#[test]
fn no_token_means_no_notifications() {
    let directory = TestDirectory::create();
    let sink = session(&directory, &[simulate_call(5, None)], 1_000);
    assert!(progress_notifications(&sink).is_empty());
    for line in sink.text().lines() {
        let message = parse(line.as_bytes(), &JsonLimits::DEFAULT).unwrap();
        assert!(
            message.get("method").is_none(),
            "a call without a token says nothing extra: {line}"
        );
    }
}

#[test]
fn a_fast_call_reports_far_fewer_times_than_it_steps() {
    let directory = TestDirectory::create();
    // One millisecond per clock read, fifty events: the rate limit collapses
    // them, and the final report still arrives.
    let sink = session(&directory, &[simulate_call(50, Some(r#""tok""#))], 1);
    let reports = progress_notifications(&sink);
    assert!(
        reports.len() < 50,
        "a thousand notifications is worse than none: {}",
        reports.len()
    );
    let last = reports.last().expect("the last one always arrives");
    assert_eq!(last.get("progress").and_then(Value::as_num), Some("50"));
    assert_eq!(last.get("total").and_then(Value::as_num), Some("50"));
}

#[test]
fn a_single_step_reports_exactly_once() {
    let directory = TestDirectory::create();
    let sink = session(&directory, &[simulate_call(1, Some(r#""tok""#))], 1_000);
    assert_eq!(progress_notifications(&sink).len(), 1);
}

#[test]
fn history_reports_its_page_and_finishes_complete() {
    let directory = TestDirectory::create();
    let mut store = Store::open(directory.path()).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "prog_case",
            "inst-1",
            "c1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    for index in 0..25 {
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
    let sink = SharedSink::new();
    let call = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_history","arguments":{"instance_id":"inst-1"},"_meta":{"progressToken":"h"}}}"#;
    serve_session(
        Some(&mut store),
        &mut FixedClock::new(10_000, 1_000),
        Cursor::new(format!("{HELLO}\n{call}\n").into_bytes()),
        sink.writer(),
    )
    .unwrap();
    let reports = progress_notifications(&sink);
    assert!(!reports.is_empty(), "{}", sink.text());
    let last = reports.last().unwrap();
    assert_eq!(
        last.get("progress").and_then(Value::as_num),
        last.get("total").and_then(Value::as_num)
    );
}

#[test]
fn no_other_tool_reports_progress() {
    let directory = TestDirectory::create();
    let with_token = |id: u64, name: &str, arguments: &str| {
        format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"tools/call","params":{{"name":"{name}","arguments":{arguments},"_meta":{{"progressToken":"t"}}}}}}"#
        )
    };
    let sink = session(
        &directory,
        &[
            with_token(2, "machine_list", "{}"),
            with_token(3, "machine_get", r#"{"machine":"prog_case"}"#),
            with_token(4, "machine_analyze", r#"{"machine":"prog_case"}"#),
            with_token(
                5,
                "machine_diagram",
                r#"{"machine":"prog_case","format":"mermaid"}"#,
            ),
            with_token(
                6,
                "instance_create",
                r#"{"machine":"prog_case","request_id":"p1"}"#,
            ),
            with_token(7, "instance_list", "{}"),
            with_token(8, "instance_get", r#"{"instance_id":"inst-p1"}"#),
        ],
        1_000,
    );
    assert!(
        progress_notifications(&sink).is_empty(),
        "a report on a call that returns in a microsecond is noise: {}",
        sink.text()
    );
}
