//! Byte-stream questions stay client-owned while the native owner progresses.

use std::{
    collections::BTreeMap,
    io::{BufReader, Write},
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use fsm_core::{canon::canon_bytes, json::Value};
use fsm_execute::{
    config::HandlerTable,
    service::{ExecutorControl, ExecutorPhase, OwnedNativeExecutor, ShutdownMode},
};

use crate::{
    mcp::{
        notify::SharedSink,
        serve::hosted::{self, HostedReport},
    },
    store::Store,
};

use super::{Scratch, interaction::CASE, value};

struct LogicalClock(Arc<AtomicI64>);
impl crate::clock::Clock for LogicalClock {
    fn now_ms(&mut self) -> i64 {
        self.0.load(Ordering::Acquire)
    }
}

struct Conversation {
    scratch: Scratch,
    client: UnixStream,
    output: SharedSink,
    clock: Arc<AtomicI64>,
    control: ExecutorControl,
    report: mpsc::Receiver<std::io::Result<HostedReport>>,
    worker: Option<JoinHandle<()>>,
}

impl Conversation {
    fn start() -> Self {
        let scratch = Scratch::new();
        let driver =
            OwnedNativeExecutor::new(Store::open(&scratch.0).unwrap(), HandlerTable::default())
                .unwrap();
        let control = driver.control();
        let (client, server) = UnixStream::pair().unwrap();
        let output = SharedSink::new();
        let writer = output.writer();
        let clock = Arc::new(AtomicI64::new(1000));
        let owner_clock = LogicalClock(Arc::clone(&clock));
        let (report, observed) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            report
                .send(hosted::serve(
                    driver,
                    owner_clock,
                    move || BufReader::new(server),
                    writer,
                    std::io::sink(),
                ))
                .unwrap();
        });
        let mut conversation = Self {
            scratch,
            client,
            output,
            clock,
            control,
            report: observed,
            worker: Some(worker),
        };
        conversation.frame(value(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{"elicitation":{}}}}"#));
        conversation.frame(value(
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        ));
        conversation.tool(
            2,
            "machine_create",
            Value::Obj(BTreeMap::from([("spec".into(), value(CASE))])),
        );
        conversation.tool(
            3,
            "instance_create",
            value(r#"{"machine":"question_case","request_id":"question-create"}"#),
        );
        let created = conversation.wait_rpc(3);
        assert_eq!(
            created
                .get("result")
                .and_then(|result| result.get("structuredContent"))
                .and_then(|result| result.get("instance_id"))
                .and_then(Value::as_str),
            Some("inst-question-create")
        );
        conversation
    }

    fn frame(&mut self, frame: Value) {
        self.client.write_all(&canon_bytes(&frame)).unwrap();
        self.client.write_all(b"\n").unwrap();
    }

    fn tool(&mut self, id: u64, name: &str, arguments: Value) {
        self.frame(Value::Obj(BTreeMap::from([
            ("jsonrpc".into(), Value::Str("2.0".into())),
            ("id".into(), Value::Num(id.to_string())),
            ("method".into(), Value::Str("tools/call".into())),
            (
                "params".into(),
                Value::Obj(BTreeMap::from([
                    ("name".into(), Value::Str(name.into())),
                    ("arguments".into(), arguments),
                ])),
            ),
        ])));
    }

    fn ask(&mut self, id: u64) {
        self.tool(id, "instance_elicit", value(r#"{"instance_id":"inst-question-create","event":"decide","request_id":"question-answer"}"#));
    }

    fn wait_rpc(&self, id: u64) -> Value {
        let expected = Value::Num(id.to_string());
        self.wait_frame(|frame| frame.get("id") == Some(&expected))
    }

    fn wait_question(&self, ordinal: usize) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(frame) = self
                .output
                .text()
                .lines()
                .map(value)
                .filter(|frame| {
                    frame.get("method").and_then(Value::as_str) == Some("elicitation/create")
                })
                .nth(ordinal)
            {
                return frame;
            }
            assert!(
                Instant::now() < deadline,
                "actual question must reach the byte stream"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_frame(&self, predicate: impl Fn(&Value) -> bool) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(frame) = self.output.text().lines().map(value).find(&predicate) {
                return frame;
            }
            assert!(
                Instant::now() < deadline,
                "actual response must reach the byte stream"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn answer(&mut self, question: &Value) {
        self.answer_body(
            question,
            value(r#"{"action":"accept","content":{"score":7}}"#),
        );
    }

    fn answer_body(&mut self, question: &Value, result: Value) {
        self.frame(Value::Obj(BTreeMap::from([
            ("jsonrpc".into(), Value::Str("2.0".into())),
            ("id".into(), question.get("id").unwrap().clone()),
            ("result".into(), result),
        ])));
    }

    fn take_report(&mut self) -> HostedReport {
        let result = self.report.recv_timeout(Duration::from_secs(5)).unwrap();
        self.worker.take().unwrap().join().unwrap();
        result.unwrap()
    }

    fn finish(&mut self) -> HostedReport {
        self.client.shutdown(Shutdown::Write).unwrap();
        self.take_report()
    }
}

impl Drop for Conversation {
    fn drop(&mut self) {
        if self.worker.is_some() {
            let _ = self.control.stop(ShutdownMode::Abort, 1000);
            let _ = self.client.shutdown(Shutdown::Write);
            // Failure cleanup does not join a live worker or claim its release.
            if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                let _ = self.worker.take().unwrap().join();
            }
        }
    }
}

fn error_code(frame: &Value) -> Option<&str> {
    frame
        .get("result")?
        .get("structuredContent")?
        .get("error")?
        .get("code")?
        .as_str()
}

#[test]
fn execution_host_session_channels_stdio_question_allows_quiet_deadline_and_revalidates_answer() {
    let mut conversation = Conversation::start();
    conversation.ask(4);
    let original = conversation.wait_question(0);
    assert_eq!(
        original
            .get("params")
            .and_then(|params| params.get("requestedSchema"))
            .and_then(|schema| schema.get("properties"))
            .and_then(|properties| properties.get("score"))
            .and_then(|score| score.get("type"))
            .and_then(Value::as_str),
        Some("integer")
    );
    conversation.clock.store(1001, Ordering::Release);
    // No subsequent client frame drives the native deadline.
    let watchdog = Instant::now() + Duration::from_secs(5);
    loop {
        let observed = Store::open_read_only(&conversation.scratch.0).unwrap();
        if observed.journal.last_seq == 3 {
            assert!(!observed.state.dedup.contains_key("question-answer"));
            break;
        }
        assert!(
            Instant::now() < watchdog,
            "a client-owned question must not hold the writer turn"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    conversation.answer(&original);
    let stale = conversation.wait_rpc(4);
    if error_code(&stale) != Some("req/seq_mismatch") {
        conversation.finish();
    }
    assert_eq!(error_code(&stale), Some("req/seq_mismatch"));
    let observed = Store::open_read_only(&conversation.scratch.0).unwrap();
    assert_eq!(observed.journal.last_seq, 3);
    assert!(!observed.state.dedup.contains_key("question-answer"));
    drop(observed);
    conversation.ask(5);
    let current = conversation.wait_question(1);
    assert_ne!(current.get("id"), original.get("id"));
    conversation.answer(&current);
    let settled = conversation.wait_rpc(5);
    assert!(error_code(&settled).is_none());
    let report = conversation.finish();
    assert!(report.failure.is_none());
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(report.shutdown.writer_released && report.output_drained);
    let reopened = Store::open(&conversation.scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 4);
    assert!(reopened.state.dedup.contains_key("question-answer"));
    assert_eq!(
        reopened
            .instance_view("inst-question-create", None, None)
            .unwrap()
            .get("status")
            .and_then(Value::as_str),
        Some("completed")
    );
}

#[test]
fn execution_host_session_channels_stdio_original_rpc_cancel_releases_question_and_reuses_key() {
    let mut conversation = Conversation::start();
    conversation.ask(4);
    conversation.wait_question(0);
    conversation.frame(value(
        r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":4}}"#,
    ));
    let cancelled = conversation.wait_rpc(4);
    assert_eq!(error_code(&cancelled), Some("req/cancelled"));
    let observed = Store::open_read_only(&conversation.scratch.0).unwrap();
    assert_eq!(observed.journal.last_seq, 2);
    assert!(!observed.state.dedup.contains_key("question-answer"));
    drop(observed);
    conversation.ask(4);
    let replacement = conversation.wait_question(1);
    conversation.answer(&replacement);
    // Both responses use the reused RPC id; select the successful one explicitly.
    conversation.wait_frame(|frame| {
        frame.get("id") == Some(&Value::Num("4".into()))
            && frame.get("result").is_some()
            && error_code(frame).is_none()
    });
    let report = conversation.finish();
    assert!(report.failure.is_none());
    assert!(report.shutdown.writer_released);
    let reopened = Store::open(&conversation.scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 3);
    assert!(reopened.state.dedup.contains_key("question-answer"));
}

#[test]
fn execution_host_session_channels_stdio_original_stop_retires_an_unanswered_question() {
    let mut conversation = Conversation::start();
    conversation.ask(4);
    conversation.wait_question(0);
    let request = conversation
        .control
        .stop(ShutdownMode::Drain, 5000)
        .unwrap();
    let report = conversation.take_report();
    assert_eq!(report.shutdown_deadline, request.deadline());
    assert_eq!(report.shutdown.phase, ExecutorPhase::Stopped);
    assert!(report.failure.is_none());
    assert!(report.shutdown.writer_released && report.shutdown.helpers_retired);
    assert!(report.output_drained && report.operator_output_drained);
    assert!(report.worker.is_none());
    assert_eq!(
        conversation
            .output
            .text()
            .lines()
            .map(value)
            .filter(|frame| frame.get("id") == Some(&Value::Num("4".into())))
            .count(),
        0
    );
    // Release the underlying fixture reader; no generic Read interruption is inferred.
    conversation.client.shutdown(Shutdown::Write).unwrap();
    let reopened = Store::open(&conversation.scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 2);
    assert!(!reopened.state.dedup.contains_key("question-answer"));
}

#[test]
fn execution_host_session_channels_stdio_invalid_answer_keeps_original_key_and_allows_correction() {
    let mut conversation = Conversation::start();
    conversation.ask(4);
    let question = conversation.wait_question(0);
    conversation.answer_body(
        &question,
        value(r#"{"action":"accept","content":{"score":true}}"#),
    );
    let invalid = conversation.wait_rpc(4);
    let request_key = invalid
        .get("result")
        .and_then(|result| result.get("structuredContent"))
        .and_then(|result| result.get("error"))
        .and_then(|error| error.get("details"))
        .and_then(|error| error.get("request_id"))
        .and_then(Value::as_str);
    if error_code(&invalid) != Some("req/field_type") || request_key != Some("question-answer") {
        conversation.finish();
    }
    assert_eq!(error_code(&invalid), Some("req/field_type"));
    assert_eq!(request_key, Some("question-answer"));
    let observed = Store::open_read_only(&conversation.scratch.0).unwrap();
    assert_eq!(observed.journal.last_seq, 2);
    assert!(!observed.state.dedup.contains_key("question-answer"));
    drop(observed);
    conversation.ask(5);
    let question = conversation.wait_question(1);
    conversation.answer(&question);
    conversation.wait_rpc(5);
    let report = conversation.finish();
    assert!(report.failure.is_none());
    assert!(report.shutdown.writer_released);
    let reopened = Store::open(&conversation.scratch.0).unwrap();
    assert_eq!(reopened.journal.last_seq, 3);
    assert!(reopened.state.dedup.contains_key("question-answer"));
}
