//! The server as a requester: parsing answers, and waiting for one without
//! deadlocking the client that is meant to give it.
//!
//! Plan 0013 task 6401.

use std::cell::RefCell;
use std::io::Cursor;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::elicit::{DEFAULT_TIMEOUT_MS, ask, next_request_id, request_and_await};
use fsm_cli::mcp::jsonrpc::{Incoming, WireError, parse_line};
use fsm_cli::mcp::notify::{Notifier, SessionIo, SharedSink};
use fsm_core::json::{JsonLimits, Value, parse};

static IDS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lines(sink: &SharedSink) -> Vec<Value> {
    sink.text()
        .lines()
        .map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("a whole message"))
        .collect()
}

/// Drive one exchange over a scripted client.
///
/// The script is written with `{id}` where the server's own request id goes,
/// which the caller cannot know in advance — the counter is monotonic for
/// the life of the process, so pinning a literal would make this suite
/// order-dependent.
fn exchange(script: &[&str], clock: &mut FixedClock) -> (SharedSink, Result<Value, String>) {
    // Ids are monotonic for the life of the process, so predicting the next
    // one means nobody else may take one in between.
    let _turn = IDS.lock().unwrap_or_else(|e| e.into_inner());
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let expected = format!("fsm-elicit-{}", peek_next());
    let input: String = script
        .iter()
        .map(|line| format!("{}\n", line.replace("{id}", &expected)))
        .collect();
    let mut reader = Cursor::new(input.into_bytes());
    let mut io = SessionIo::new(&notifier, &mut reader);
    let result = request_and_await(
        &mut io,
        "elicitation/create",
        Value::Obj(Default::default()),
        clock,
    )
    .map_err(|e| e.code);
    (sink, result)
}

/// The id the next call will use, without consuming it: ids are monotonic,
/// so this reads the counter by taking one and adding one.
fn peek_next() -> u64 {
    let taken = next_request_id();
    let n: u64 = taken.trim_start_matches("fsm-elicit-").parse().unwrap();
    n + 1
}

#[test]
fn a_response_is_recognised_and_a_hybrid_is_not() {
    match parse_line(r#"{"jsonrpc":"2.0","id":"fsm-elicit-1","result":{"action":"accept"}}"#) {
        Ok(Incoming::Response { id, result, error }) => {
            assert_eq!(id.as_str(), Some("fsm-elicit-1"));
            assert!(result.is_some() && error.is_none());
        }
        other => panic!("expected a response, got {other:?}"),
    }
    match parse_line(r#"{"jsonrpc":"2.0","id":7,"error":{"code":-32601,"message":"no"}}"#) {
        Ok(Incoming::Response { result, error, .. }) => {
            assert!(result.is_none() && error.is_some());
        }
        other => panic!("expected an error response, got {other:?}"),
    }
    // Both a method and a result is neither, and guessing which the sender
    // meant is how a protocol loop starts inventing semantics.
    assert!(matches!(
        parse_line(r#"{"jsonrpc":"2.0","id":1,"method":"ping","result":{}}"#),
        Err(WireError::Invalid)
    ));
    // A result with no id is not addressable and so not a response.
    assert!(matches!(
        parse_line(r#"{"jsonrpc":"2.0","result":{}}"#),
        Err(WireError::Invalid)
    ));
}

#[test]
fn the_question_is_written_and_the_answer_returned() {
    let mut clock = FixedClock::new(1_000, 0);
    let (sink, result) = exchange(
        &[r#"{"jsonrpc":"2.0","id":"{id}","result":{"action":"accept","content":{"note":"ok"}}}"#],
        &mut clock,
    );
    let answer = result.expect("the client answered");
    assert_eq!(answer.get("action").and_then(Value::as_str), Some("accept"));
    let written = lines(&sink);
    assert_eq!(written.len(), 1, "one question, nothing else");
    assert_eq!(
        written[0].get("method").and_then(Value::as_str),
        Some("elicitation/create")
    );
    assert!(
        written[0]
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("fsm-elicit-")),
        "a server id a client cannot collide with"
    );
}

#[test]
fn a_client_request_arriving_first_is_answered_and_the_wait_continues() {
    // A client is not obliged to stop working because the server asked a
    // question, and a client waiting for a response it never gets would
    // deadlock against a server waiting for an answer it never gets.
    let mut clock = FixedClock::new(1_000, 0);
    let (sink, result) = exchange(
        &[
            r#"{"jsonrpc":"2.0","id":41,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":"fsm-elicit-999999","result":{"action":"decline"}}"#,
            r#"{"jsonrpc":"2.0","id":42,"method":"tools/list"}"#,
            r#"{"jsonrpc":"2.0","id":"{id}","result":{"action":"accept"}}"#,
        ],
        &mut clock,
    );
    assert_eq!(
        result
            .expect("answered")
            .get("action")
            .and_then(Value::as_str),
        Some("accept"),
        "a stray response, a notification and two requests did not disturb the wait"
    );
    let written = lines(&sink);
    let ids: Vec<Option<&str>> = written
        .iter()
        .map(|m| {
            m.get("id")
                .and_then(|id| id.as_num().or_else(|| id.as_str()))
        })
        .collect();
    assert_eq!(
        ids.len(),
        3,
        "the question, then two answers, in the order they were asked: {written:?}"
    );
    assert!(ids[0].is_some_and(|id| id.starts_with("fsm-elicit-")));
    assert_eq!(ids[1], Some("41"));
    assert_eq!(ids[2], Some("42"));
    assert!(
        written[2]
            .get("result")
            .and_then(|r| r.get("tools"))
            .is_some(),
        "a static listing is answerable while waiting"
    );
}

#[test]
fn a_request_that_needs_the_store_is_told_to_come_back() {
    // The tool that asked this question is holding the store; a wrong answer
    // and silence are both worse than saying so.
    let mut clock = FixedClock::new(1_000, 0);
    let (sink, result) = exchange(
        &[
            r#"{"jsonrpc":"2.0","id":5,"method":"resources/list"}"#,
            r#"{"jsonrpc":"2.0","id":"{id}","result":{"action":"accept"}}"#,
        ],
        &mut clock,
    );
    assert!(result.is_ok());
    let written = lines(&sink);
    let error = written[1].get("error").expect("an error response");
    assert_eq!(error.get("code").and_then(Value::as_num), Some("-32004"));
    assert!(
        error
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|m| m.contains("retry")),
        "{error:?}"
    );
}

#[test]
fn an_error_answer_comes_back_as_an_error() {
    let mut clock = FixedClock::new(1_000, 0);
    let (_, result) = exchange(
        &[r#"{"jsonrpc":"2.0","id":"{id}","error":{"code":-32601,"message":"cannot ask"}}"#],
        &mut clock,
    );
    assert_eq!(result.unwrap_err(), "req/elicit_failed");
}

#[test]
fn a_cancellation_naming_the_question_ends_the_wait() {
    let mut clock = FixedClock::new(1_000, 0);
    let (sink, result) = exchange(
        &[
            r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"{id}","reason":"changed their mind"}}"#,
        ],
        &mut clock,
    );
    assert_eq!(result.unwrap_err(), "req/elicit_failed");
    assert_eq!(lines(&sink).len(), 1, "the question, and nothing after it");
}

#[test]
fn a_client_that_leaves_ends_the_wait() {
    let mut clock = FixedClock::new(1_000, 0);
    let (_, result) = exchange(&[], &mut clock);
    assert_eq!(
        result.unwrap_err(),
        "req/elicit_timeout",
        "end of input while a question is outstanding is a session ending, not a panic"
    );
}

#[test]
fn a_client_that_talks_without_answering_runs_out_of_time() {
    // The clock steps past the limit on its second reading, so the wait ends
    // before the client's next line is read.
    let mut clock = FixedClock::new(1_000, DEFAULT_TIMEOUT_MS + 1);
    let (sink, result) = exchange(
        &[r#"{"jsonrpc":"2.0","id":"{id}","result":{"action":"accept"}}"#],
        &mut clock,
    );
    assert_eq!(result.unwrap_err(), "req/elicit_timeout");
    assert_eq!(
        lines(&sink).len(),
        1,
        "the question was written and nothing else was"
    );
}

#[test]
fn a_second_question_while_one_is_outstanding_is_refused() {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut reader = Cursor::new(Vec::new());
    let io = RefCell::new(SessionIo::new(&notifier, &mut reader));
    let outstanding = io.borrow_mut();
    let mut clock = FixedClock::new(1_000, 0);
    let error = ask(
        &io,
        "elicitation/create",
        Value::Obj(Default::default()),
        &mut clock,
    )
    .expect_err("a recursive ask is a design mistake");
    assert_eq!(error.code, "req/elicit_nested");
    assert!(
        sink.text().is_empty(),
        "and no second question was written: {}",
        sink.text()
    );
    drop(outstanding);
}

#[test]
fn server_ids_are_monotonic_and_prefixed() {
    let _turn = IDS.lock().unwrap_or_else(|e| e.into_inner());
    let first = next_request_id();
    let second = next_request_id();
    assert!(first.starts_with("fsm-elicit-"));
    let n = |id: &str| -> u64 { id.trim_start_matches("fsm-elicit-").parse().unwrap() };
    assert!(n(&second) > n(&first), "{first} then {second}");
}
