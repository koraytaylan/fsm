//! The MCP client, against a real server on the other end of a real pipe.
//!
//! Every row runs `tests/support/mcp_stub.rs` as a subprocess, because the
//! properties under test are about pipes, exit, and kill. It is a declared
//! `[[bin]]` rather than this test binary re-executed — the trick the rest of
//! the suite uses — because libtest writes `running 1 test` to a child's
//! stdout before the test body runs, and on a protocol stream that banner is a
//! malformed message: exactly what the client is supposed to refuse.
//!
//! The client's contract is that a subprocess can never make the executor
//! panic. A malformed line, an oversized one, an id nobody asked for, a server
//! that exits mid-handshake, and one that never answers at all are each a
//! bounded failure of one effect.

use std::collections::BTreeMap;

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_execute::mcp_client::{McpOutcome, ProtocolFault};
use fsm_execute::run::{KillReason, McpCall, RunOutcome, Runner};

fn stub_argv(script: &str) -> Vec<String> {
    vec![
        env!("CARGO_BIN_EXE_fsm-mcp-stub").to_string(),
        script.to_string(),
    ]
}

fn call(tool: &str, arguments: Value) -> McpCall {
    McpCall {
        tool: tool.to_string(),
        arguments,
    }
}

fn json(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("the fixture is valid JSON")
}

/// Run one script to completion and return what the conversation did.
fn converse(runner: &mut Runner, effect_id: &str, script: &str, call: &McpCall) -> RunOutcome {
    runner
        .spawn(effect_id.to_string(), &stub_argv(script), Some(call))
        .expect("the stub server spawns");
    for _ in 0..1_000 {
        if runner.finished_effects().iter().any(|id| id == effect_id) {
            return runner
                .poll(effect_id)
                .expect("a finished run has an outcome");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("{script} never finished its conversation");
}

fn outcome_of(run: &RunOutcome) -> &McpOutcome {
    match run {
        RunOutcome::Mcp { outcome, .. } => outcome,
        other => panic!("expected an mcp outcome, got {other:?}"),
    }
}

fn fault_of(run: &RunOutcome) -> ProtocolFault {
    match outcome_of(run) {
        McpOutcome::Protocol(fault) => *fault,
        other => panic!("expected a protocol fault, got {other:?}"),
    }
}

#[test]
fn a_full_exchange_returns_the_tool_result() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-1/3/0",
        "echo",
        &call("summarize", json(r#"{"case_id":"case-91","mode":"brief"}"#)),
    );
    assert!(run.succeeded());
    let McpOutcome::Answered {
        structured,
        is_error,
    } = outcome_of(&run)
    else {
        panic!("expected an answer, got {:?}", outcome_of(&run));
    };
    assert!(!is_error);
    // The arguments reached the tool exactly as the table declared them, which
    // is the whole reason the stub echoes them back.
    assert_eq!(
        structured.get("echo"),
        Some(&json(r#"{"case_id":"case-91","mode":"brief"}"#))
    );
    // A successful call carries the tool's answer and not the server's logs.
    assert_eq!(run.ack_result().get("stderr"), None);
    assert_eq!(run.failure_class(), None);
}

#[test]
fn structured_content_is_preferred_and_content_is_the_fallback() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-2/3/0",
        "content-only",
        &call("summarize", Value::Obj(BTreeMap::new())),
    );
    assert!(run.succeeded());
    let McpOutcome::Answered { structured, .. } = outcome_of(&run) else {
        panic!("expected an answer");
    };
    // A tool with no typed output still reaches the journal whole rather than
    // being dropped for lacking `structuredContent`.
    assert_eq!(structured, &json(r#"[{"type":"text","text":"summary"}]"#));
}

#[test]
fn a_tool_that_reports_its_own_failure_is_not_a_protocol_failure() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-3/3/0",
        "tool-error",
        &call("assign", Value::Obj(BTreeMap::new())),
    );
    assert!(!run.succeeded());
    assert!(matches!(
        outcome_of(&run),
        McpOutcome::Answered { is_error: true, .. }
    ));
    // A failed tool call is a failure the world might answer differently.
    assert_eq!(run.failure_class(), Some("mcp_error"));
    assert_eq!(
        run.ack_result().get("error").and_then(Value::as_str),
        Some("mcp/tool_error")
    );
}

#[test]
fn a_json_rpc_error_carries_its_code_and_message() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-4/3/0",
        "rpc-error",
        &call("nope", Value::Obj(BTreeMap::new())),
    );
    assert_eq!(
        outcome_of(&run),
        &McpOutcome::RpcError {
            code: -32602,
            message: "unknown tool".into(),
        }
    );
    assert_eq!(run.failure_class(), Some("mcp_error"));
    let acked = run.ack_result();
    assert_eq!(
        acked.get("error").and_then(Value::as_str),
        Some("mcp/rpc_error")
    );
    assert_eq!(acked.get("code").and_then(Value::as_num), Some("-32602"));
    assert_eq!(
        acked.get("message").and_then(Value::as_str),
        Some("unknown tool")
    );
}

/// A server that has gone is reported as gone, whichever side noticed first.
///
/// `Closed` is a read that hit end of stream and `WriteFailed` is a write that
/// hit a closed pipe — the same fact seen from the two directions, and which
/// one arrives is a scheduling detail between two processes. Pinning either
/// would be pinning the scheduler.
fn assert_server_gone(run: &RunOutcome) {
    let fault = fault_of(run);
    assert!(
        matches!(fault, ProtocolFault::Closed | ProtocolFault::WriteFailed),
        "expected the server to be reported gone, got {fault:?}"
    );
    assert_eq!(
        run.ack_result().get("error").and_then(Value::as_str),
        Some("exec/mcp_protocol")
    );
    // A broken server is not a transient failure: running it again produces
    // the same broken exchange, so no retry policy applies.
    assert_eq!(run.failure_class(), None);
}

#[test]
fn a_server_that_exits_during_the_handshake_is_a_protocol_failure() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-5/3/0",
        "exit-early",
        &call("summarize", Value::Obj(BTreeMap::new())),
    );
    assert_server_gone(&run);
}

#[test]
fn a_server_that_exits_after_the_handshake_is_a_protocol_failure() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-15/3/0",
        "close-after-handshake",
        &call("summarize", Value::Obj(BTreeMap::new())),
    );
    assert_server_gone(&run);
}

#[test]
fn every_protocol_violation_is_a_bounded_failure_rather_than_a_panic() {
    let rows = [
        ("no-init-result", ProtocolFault::NoInitializeResult),
        ("malformed", ProtocolFault::MalformedLine),
        ("not-an-object", ProtocolFault::MalformedLine),
        ("oversized", ProtocolFault::OversizedLine),
        ("wrong-id", ProtocolFault::IdMismatch),
        ("call-malformed", ProtocolFault::MalformedLine),
        ("call-wrong-id", ProtocolFault::IdMismatch),
        ("call-no-result", ProtocolFault::NoCallResult),
    ];
    for (index, (script, expected)) in rows.into_iter().enumerate() {
        let mut runner = Runner::new().expect("a scratch directory");
        let run = converse(
            &mut runner,
            &format!("case-6/3/{index}"),
            script,
            &call("summarize", Value::Obj(BTreeMap::new())),
        );
        assert_eq!(fault_of(&run), expected, "{script}");
        // The fault reaches the ack as an identifier from a closed set, never
        // as an OS message: the store fingerprints this object, and a varying
        // string would turn a re-issue into a conflict.
        assert_eq!(
            run.ack_result().get("detail").and_then(Value::as_str),
            Some(expected.as_str()),
            "{script}"
        );
    }
}

#[test]
fn notifications_and_server_requests_are_ignored_and_the_answer_still_arrives() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-7/3/0",
        "chatty",
        &call("summarize", Value::Obj(BTreeMap::new())),
    );
    // A server that logs is not a server that failed — and neither is one that
    // asks a question this client declared no capability to answer.
    assert!(run.succeeded(), "{:?}", outcome_of(&run));
    let McpOutcome::Answered { structured, .. } = outcome_of(&run) else {
        panic!("expected an answer");
    };
    assert_eq!(
        structured.get("seen").and_then(Value::as_str),
        Some("chatty")
    );
}

#[test]
fn a_server_that_never_answers_is_killed_and_reported_as_a_timeout() {
    let mut runner = Runner::new().expect("a scratch directory");
    runner
        .spawn(
            "case-8/3/0".to_string(),
            &stub_argv("silent"),
            Some(&call("summarize", Value::Obj(BTreeMap::new()))),
        )
        .expect("the stub server spawns");
    // Nothing arrives, tick after tick — which is exactly the state the
    // scheduler's deadline exists for.
    for _ in 0..5 {
        assert!(runner.finished_effects().is_empty());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // One timeout for both handler kinds, in one place.
    let run = runner.kill("case-8/3/0", KillReason::Timeout);
    assert_eq!(
        run,
        RunOutcome::Killed {
            reason: KillReason::Timeout
        }
    );
    assert_eq!(
        run.ack_result().get("error").and_then(Value::as_str),
        Some("exec/timeout")
    );
    assert_eq!(run.failure_class(), Some("timeout"));
    // And the child is gone rather than left as a zombie.
    assert!(runner.poll("case-8/3/0").is_none());
}

#[test]
fn a_command_that_does_not_exist_reports_a_spawn_failure() {
    let mut runner = Runner::new().expect("a scratch directory");
    let error = runner
        .spawn(
            "case-9/3/0".to_string(),
            &["/nonexistent/mcp-server".to_string()],
            Some(&call("summarize", Value::Obj(BTreeMap::new()))),
        )
        .expect_err("there is no such command");
    assert_eq!(error.code, "exec/spawn");
    assert!(
        error.message.contains("/nonexistent/mcp-server"),
        "{}",
        error.message
    );
    assert!(runner.finished_effects().is_empty(), "nothing is in flight");
}

#[test]
fn the_servers_standard_error_is_captured_bounded_and_digested() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-10/3/0",
        "noisy-stderr",
        &call("summarize", Value::Obj(BTreeMap::new())),
    );
    let RunOutcome::Mcp { stderr, .. } = &run else {
        panic!("expected an mcp outcome");
    };
    // The same bounded, digest-backed capture a process handler gets: a
    // crashing server leaves evidence rather than silence, and a chatty one
    // cannot put a megabyte in a permanent record.
    assert!(stderr.truncated, "9000 bytes is past the 4096-byte cap");
    assert_eq!(stderr.bytes.len(), fsm_execute::run::ACK_OUTPUT_CAP);
    assert!(stderr.sha256.is_some(), "the whole stream is digested");
}

#[test]
fn a_failed_call_carries_the_servers_standard_error_as_evidence() {
    let mut runner = Runner::new().expect("a scratch directory");
    let run = converse(
        &mut runner,
        "case-11/3/0",
        "exit-early",
        &call("summarize", Value::Obj(BTreeMap::new())),
    );
    // Nothing was written here, but the field is present: a failure's ack is
    // where an operator looks for what the server said on its way down.
    assert!(run.ack_result().get("stderr").is_some());
}

#[test]
fn two_effects_get_two_servers_and_neither_is_reused() {
    let mut runner = Runner::new().expect("a scratch directory");
    let arguments = json(r#"{"case_id":"case-1"}"#);
    let other = json(r#"{"case_id":"case-2"}"#);
    runner
        .spawn(
            "case-12/3/0".to_string(),
            &stub_argv("echo"),
            Some(&call("summarize", arguments.clone())),
        )
        .expect("the first server spawns");
    runner
        .spawn(
            "case-12/3/1".to_string(),
            &stub_argv("echo"),
            Some(&call("summarize", other.clone())),
        )
        .expect("the second server spawns");
    // No pooling: each effect gets its own process, its own timeout, and its
    // own kill, so one effect's failure can never be another's problem.
    let mut answers = BTreeMap::new();
    for _ in 0..600 {
        for effect_id in runner.finished_effects() {
            if let Some(run) = runner.poll(&effect_id) {
                answers.insert(effect_id, run);
            }
        }
        if answers.len() == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(answers.len(), 2, "both conversations finished");
    for (effect_id, expected) in [("case-12/3/0", &arguments), ("case-12/3/1", &other)] {
        let McpOutcome::Answered { structured, .. } = outcome_of(&answers[effect_id]) else {
            panic!("{effect_id} did not answer");
        };
        assert_eq!(
            structured.get("echo"),
            Some(expected),
            "{effect_id} got the other effect's arguments"
        );
    }
}

#[test]
fn a_second_run_for_one_effect_is_refused() {
    // Two servers for one effect could produce two acks over the same derived
    // key with different content — the one collision the whole design refuses.
    let mut runner = Runner::new().expect("a scratch directory");
    let asked = call("summarize", Value::Obj(BTreeMap::new()));
    runner
        .spawn(
            "case-13/3/0".to_string(),
            &stub_argv("silent"),
            Some(&asked),
        )
        .expect("the first server spawns");
    let error = runner
        .spawn(
            "case-13/3/0".to_string(),
            &stub_argv("silent"),
            Some(&asked),
        )
        .expect_err("a run is already in flight");
    assert_eq!(error.code, "exec/spawn");
    runner.kill("case-13/3/0", KillReason::Cancelled);
}

#[test]
fn a_cancelled_conversation_is_reaped_and_never_retried() {
    let mut runner = Runner::new().expect("a scratch directory");
    runner
        .spawn(
            "case-14/3/0".to_string(),
            &stub_argv("silent"),
            Some(&call("summarize", Value::Obj(BTreeMap::new()))),
        )
        .expect("the stub server spawns");
    let run = runner.kill("case-14/3/0", KillReason::Cancelled);
    assert_eq!(
        run,
        RunOutcome::Killed {
            reason: KillReason::Cancelled
        }
    );
    // Somebody decided this instance was over; a retry would spend the
    // operator's budget undoing that decision.
    assert_eq!(run.failure_class(), None);
    assert!(
        runner.finished_effects().is_empty(),
        "nothing is left in flight"
    );
}

#[test]
fn dropping_the_runner_stops_a_conversation_and_removes_its_captures() {
    // The MCP analogue of the process runner's own row. A worker blocked on a
    // read cannot be signalled; it ends because the pipes close, and the pipes
    // close because `Drop` kills the child. There is no second stop path.
    let kept;
    {
        let mut runner = Runner::new().expect("a scratch directory");
        kept = runner.scratch_dir().to_path_buf();
        runner
            .spawn(
                "case-16/3/0".to_string(),
                &stub_argv("call-silent"),
                Some(&call("summarize", Value::Obj(BTreeMap::new()))),
            )
            .expect("the stub server spawns");
        // In flight: the handshake finished and the call never will.
        for _ in 0..50 {
            if !runner.finished_effects().is_empty() {
                panic!("a silent call must not finish");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(kept.is_dir());
    }
    assert!(!kept.exists(), "the capture directory goes with the runner");
}

#[test]
fn talking_a_protocol_added_no_dependency() {
    // The whole client is written against this workspace's own JSON parser and
    // writer. `zero_deps.rs` proves the resolved graph; this proves the
    // manifest, which is where a dependency would be added first.
    let manifest = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"),
    )
    .expect("this crate's manifest is readable");
    let dependencies = manifest
        .split_once("[dependencies]")
        .map(|(_, rest)| rest.split("\n[").next().unwrap_or("").to_string())
        .expect("the manifest declares dependencies");
    let named: Vec<&str> = dependencies
        .lines()
        .filter_map(|line| line.split_once(" = ").map(|(name, _)| name.trim()))
        .collect();
    assert_eq!(named, ["fsm-core", "fsm-store"], "{dependencies}");
}
