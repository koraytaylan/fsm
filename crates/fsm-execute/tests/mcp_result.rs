//! What a tool call becomes in the journal, and what happens next.
//!
//! The store fingerprints an ack over its `result`, so a mapping that is not
//! **deterministic** turns a retry into a `req/request_id_conflict` instead of
//! a replay — the exact failure `rid.rs` warns about. And a journal record is
//! permanent, so a mapping that is not **bounded** lets one chatty tool push an
//! ack past `MAX_PAYLOAD_BYTES` and fail to journal at all.
//!
//! The second half of the file is about there being no second code path: an
//! `mcp` handler's ack, advance, retry, exhaustion, and dead-lettering are the
//! `process` handler's, reached with a different outcome in hand.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::{JsonLimits, Value, parse, write_canonical};
use fsm_core::machine::Status;
use fsm_core::record::RecordKind;
use fsm_core::sha256::{sha256, to_hex};
use fsm_execute::config::HandlerTable;
use fsm_execute::dead;
use fsm_execute::mcp_client::McpOutcome;
use fsm_execute::run::{ACK_OUTPUT_CAP, McpCall, Pipeline, RunOutcome, Runner};
use fsm_execute::sched::Scheduler;
use fsm_execute::service::tick;
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
                "fsm-execute-mcpres-{test_name}-{}-{sequence}",
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

fn stub() -> String {
    env!("CARGO_BIN_EXE_fsm-mcp-stub").to_string()
}

fn json(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("the fixture is valid JSON")
}

fn canonical(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    write_canonical(value, &mut out);
    out
}

/// Run one stub script to completion and return what the conversation did.
fn converse(script: &str, effect_id: &str) -> RunOutcome {
    let mut runner = Runner::new().expect("a scratch directory");
    let call = McpCall {
        tool: "summarize".to_string(),
        arguments: Value::Obj(BTreeMap::new()),
    };
    runner
        .spawn(
            effect_id.to_string(),
            &[stub(), script.to_string()],
            Some(&call),
        )
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

// ---------------------------------------------------------------- the mapping

#[test]
fn typed_output_reaches_the_journal_typed() {
    let run = converse("echo", "case-1/3/0");
    let acked = run.ack_result();
    // Not flattened to rendered text: a tool that returns structured data has
    // it journaled as structure.
    assert!(matches!(acked.get("structured"), Some(Value::Obj(_))));
    assert!(run.succeeded());
    // A successful call has no `error`, and no digest because nothing was cut.
    assert!(acked.get("error").is_none());
    assert!(acked.get("structured_sha256").is_none());
}

#[test]
fn content_is_the_fallback_when_a_tool_returns_no_structured_output() {
    let run = converse("content-only", "case-2/3/0");
    assert_eq!(
        run.ack_result().get("structured"),
        Some(&json(r#"[{"type":"text","text":"summary"}]"#))
    );
}

#[test]
fn a_tool_error_acks_failed_and_keeps_what_the_tool_returned() {
    let run = converse("tool-error", "case-3/3/0");
    assert!(!run.succeeded());
    let acked = run.ack_result();
    assert_eq!(
        acked.get("error").and_then(Value::as_str),
        Some("mcp/tool_error")
    );
    // The content survives: an operator reading this ack needs what the tool
    // said, not only that it said no.
    assert_eq!(
        acked.get("structured"),
        Some(&json(r#"[{"type":"text","text":"no reviewer"}]"#))
    );
}

#[test]
fn a_json_rpc_error_acks_failed_with_its_code_and_message() {
    let run = converse("rpc-error", "case-4/3/0");
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

#[test]
fn a_protocol_violation_acks_failed_under_its_own_code() {
    let run = converse("malformed", "case-5/3/0");
    assert_eq!(
        run.ack_result().get("error").and_then(Value::as_str),
        Some("exec/mcp_protocol")
    );
}

#[test]
fn a_spawn_failure_and_a_timeout_keep_their_existing_codes() {
    // Both are failures of the *run*, not of the call, so they are journaled
    // exactly as a process handler's are — one vocabulary across both kinds.
    let mut runner = Runner::new().expect("a scratch directory");
    let call = McpCall {
        tool: "summarize".to_string(),
        arguments: Value::Obj(BTreeMap::new()),
    };
    let spawn = runner
        .spawn(
            "case-6/3/0".to_string(),
            &["/nonexistent/mcp-server".to_string()],
            Some(&call),
        )
        .expect_err("there is no such command");
    assert_eq!(spawn.code, "exec/spawn");

    runner
        .spawn(
            "case-6/3/1".to_string(),
            &[stub(), "silent".to_string()],
            Some(&call),
        )
        .expect("the stub server spawns");
    let killed = runner.kill("case-6/3/1", fsm_execute::run::KillReason::Timeout);
    assert_eq!(
        killed.ack_result().get("error").and_then(Value::as_str),
        Some("exec/timeout")
    );
}

#[test]
fn the_same_response_produces_a_byte_identical_ack_across_runs() {
    // Nothing in the mapping varies between two runs of the same call: no
    // timestamp, no pid, no duration, no elapsed value. This is the property
    // that makes a re-issued ack a replay instead of a conflict.
    let first = converse("echo", "case-7/3/0");
    let second = converse("echo", "case-7/3/1");
    assert_eq!(
        canonical(&first.ack_result()),
        canonical(&second.ack_result())
    );

    // And said as a scan too, since a new field is where this would break.
    let rendered = String::from_utf8(canonical(&first.ack_result())).expect("canonical UTF-8");
    for varying in ["elapsed", "duration", "pid", "started", "finished", "ms"] {
        assert!(!rendered.contains(varying), "{varying} in {rendered}");
    }
}

#[test]
fn a_failure_ack_is_deterministic_too() {
    let first = converse("rpc-error", "case-8/3/0");
    let second = converse("rpc-error", "case-8/3/1");
    assert_eq!(
        canonical(&first.ack_result()),
        canonical(&second.ack_result())
    );
}

#[test]
fn an_oversized_result_is_truncated_at_the_cap_and_digested() {
    let run = converse("huge-result", "case-9/3/0");
    assert!(run.succeeded(), "a large answer is still an answer");
    let acked = run.ack_result();
    let structured = acked
        .get("structured")
        .and_then(Value::as_str)
        .expect("a truncated result is journaled as a prefix string");
    assert_eq!(structured.len(), ACK_OUTPUT_CAP);
    // The digest is over the *whole* value, which is what keeps a permanent
    // record tamper-evident about output it does not store.
    let McpOutcome::Answered {
        structured: whole, ..
    } = outcome(&run)
    else {
        panic!("expected an answer");
    };
    assert_eq!(
        acked.get("structured_sha256").and_then(Value::as_str),
        Some(to_hex(&sha256(&canonical(whole))).as_str())
    );
    // And the ack still fits the journal, which is the whole point.
    assert!(
        canonical(&acked).len() <= fsm_core::limits::MAX_PAYLOAD_BYTES,
        "{} bytes",
        canonical(&acked).len()
    );
}

#[test]
fn truncation_lands_on_a_character_boundary() {
    // The canonical bytes of a parsed value are always valid UTF-8, so the
    // hazard is not an invalid byte the tool wrote — it is the cap falling
    // *inside* a multi-byte character. A naive cut would render that half
    // character as a replacement character, putting something the tool never
    // sent into a permanent record.
    let run = converse("wide-result", "case-10/3/0");
    let acked = run.ack_result();
    let structured = acked
        .get("structured")
        .and_then(Value::as_str)
        .expect("a truncated result is a prefix string");
    assert!(!structured.contains('\u{FFFD}'), "no half characters");
    assert!(structured.len() <= ACK_OUTPUT_CAP);
    assert!(acked.get("structured_sha256").is_some());
    assert!(canonical(&acked).len() <= fsm_core::limits::MAX_PAYLOAD_BYTES);
}

fn outcome(run: &RunOutcome) -> &McpOutcome {
    match run {
        RunOutcome::Mcp { outcome, .. } => outcome,
        other => panic!("expected an mcp outcome, got {other:?}"),
    }
}

// ------------------------------------------------- one ack-and-advance path

/// A review workflow whose notification is the one effect.
fn review_machine() -> Value {
    parse(
        br#"{
            "format":"fsm.machine/1",
            "name":"review_dispatch_mcp",
            "context":[{"name":"case_ref","ty":"str","init":"case-7"}],
            "events":[
                {"name":"open","fields":[]},
                {"name":"notified","fields":[]},
                {"name":"notify_failed","fields":[]}
            ],
            "effects":[{"name":"notify","fields":[{"name":"case","ty":"str"}]}],
            "states":[
                {"name":"intake"},
                {"name":"notifying","entry":{"emit":[
                    {"effect":"notify","args":{"case":"ctx.case_ref"}}
                ]}},
                {"name":"reviewer_notified","terminal":true},
                {"name":"reviewer_unreachable","terminal":true}
            ],
            "initial":"intake",
            "transitions":[
                {"from":"intake","on":"open","to":"notifying"},
                {"from":"notifying","on":"notified","to":"reviewer_notified"},
                {"from":"notifying","on":"notify_failed","to":"reviewer_unreachable"}
            ]
        }"#,
        &JsonLimits::DEFAULT,
    )
    .expect("the review machine parses")
}

/// A one-handler table of the given kind, with the given retry block.
fn table(kind: &str, retry: &str) -> HandlerTable {
    let stub = stub().replace('\\', "\\\\");
    let (kind_keys, argv) = match kind {
        "mcp" => (
            r#""kind":"mcp","tool":"summarize","arguments":{"case":"{case}"},"#.to_string(),
            format!(r#"["{stub}","tool-error"]"#),
        ),
        script => (String::new(), format!(r#"["{stub}","{script}"]"#)),
    };
    HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",
            "handlers":[{{
                "effect":"notify",
                {kind_keys}"argv":{argv},
                "timeout_ms":30000,
                {retry}
                "on_ok":{{"event":"notified"}},
                "on_failed":{{"event":"notify_failed"}}
            }}]
        }}"#
    ))
    .expect("the table validates")
}

fn pending_notification(test_name: &str) -> (TestDirectory, String) {
    let directory = TestDirectory::create(test_name);
    let mut store = Store::open(directory.path()).expect("a fresh directory opens");
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, review_machine(), false, false)
        .expect("the machine defines");
    store
        .create_instance_ctx_on(
            &mut clock,
            "review_dispatch_mcp",
            "case-1",
            "req-create",
            None,
            &BTreeMap::new(),
            &[],
        )
        .expect("the instance is created");
    store
        .send_event_stamp_on(
            &mut clock,
            "case-1",
            "open",
            &mut Value::Obj(BTreeMap::new()),
            "req-open",
            None,
            &[],
        )
        .expect("opening the case emits the notification");
    let effect_id = store.state.instances["case-1"].pending[0].clone();
    drop(store);
    (directory, effect_id)
}

fn run_to_settled(directory: &TestDirectory, handlers: HandlerTable) -> Vec<String> {
    let mut watcher = Watcher::new(
        directory.path().to_path_buf(),
        fsm_execute::service::advancing_effects(&handlers),
    );
    let mut scheduler = Scheduler::new(handlers);
    let mut runner = Runner::new().expect("the runner makes its scratch directory");
    let mut pipeline = Pipeline;
    let mut clock = FixedClock::new(5_000, 1);
    let mut now_ms = 5_000_i64;
    let mut lines = Vec::new();
    for _ in 0..400 {
        lines.extend(tick(
            &mut watcher,
            &mut scheduler,
            &mut runner,
            &mut pipeline,
            directory.path(),
            &mut clock,
            now_ms,
        ));
        now_ms += 100;
        if lines.iter().any(|line| line.starts_with("acked ")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    lines
}

fn acks_of(store: &Store, effect_id: &str) -> Vec<Value> {
    store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::EffectAcked)
        .filter(|record| record.body.get("effect_id").and_then(Value::as_str) == Some(effect_id))
        .map(|record| record.body.clone())
        .collect()
}

#[test]
fn a_failure_path_fires_identically_for_both_handler_kinds() {
    // The same machine, the same advance configuration, and the same terminal
    // state — reached once through an exit status and once through a tool's
    // own error flag. There is no second ack-and-advance path.
    for (test_name, handlers) in [
        ("mcp-on-failed", table("mcp", "")),
        ("process-on-failed", table("exit-early", "")),
    ] {
        let (directory, _) = pending_notification(test_name);
        run_to_settled(&directory, handlers);
        let store = Store::open_read_only(directory.path()).expect("the store opens");
        let instance = &store.state.instances["case-1"];
        assert_eq!(
            instance.configuration.sequential_leaf(),
            Some("reviewer_unreachable"),
            "{test_name}"
        );
        assert_eq!(instance.status, Status::Completed, "{test_name}");
    }
}

#[test]
fn a_successful_tool_call_fires_the_success_path() {
    let (directory, effect_id) = pending_notification("mcp-on-ok");
    let stub = stub().replace('\\', "\\\\");
    let handlers = HandlerTable::parse(&format!(
        r#"{{
            "format":"fsm.handlers/1",
            "handlers":[{{
                "effect":"notify",
                "kind":"mcp","tool":"summarize","arguments":{{"case":"{{case}}"}},
                "argv":["{stub}","echo"],
                "timeout_ms":30000,
                "on_ok":{{"event":"notified"}},
                "on_failed":{{"event":"notify_failed"}}
            }}]
        }}"#
    ))
    .expect("the table validates");
    run_to_settled(&directory, handlers);
    let store = Store::open_read_only(directory.path()).expect("the store opens");
    assert_eq!(
        store.state.instances["case-1"]
            .configuration
            .sequential_leaf(),
        Some("reviewer_notified")
    );
    let acks = acks_of(&store, &effect_id);
    assert_eq!(acks.len(), 1);
    assert_eq!(acks[0].get("outcome").and_then(Value::as_str), Some("ok"));
    // The template reached the tool substituted, which is what the stub echoes.
    assert_eq!(
        acks[0]
            .get("result")
            .and_then(|result| result.get("structured"))
            .and_then(|structured| structured.get("echo"))
            .and_then(|echo| echo.get("case"))
            .and_then(Value::as_str),
        Some("case-7")
    );
}

#[test]
fn a_tool_error_retries_only_when_the_policy_says_so() {
    // With `mcp_error` in `on`, a failing tool is tried again and exhausts.
    let (retrying, effect_id) = pending_notification("mcp-retry");
    run_to_settled(
        &retrying,
        table(
            "mcp",
            r#""retry":{"attempts":3,"backoff_ms":1,"max_backoff_ms":10,"on":["mcp_error"]},"#,
        ),
    );
    let store = Store::open_read_only(retrying.path()).expect("the store opens");
    let attempts = store
        .records
        .iter()
        .filter(|record| record.kind == RecordKind::EffectAttempted)
        .count();
    assert_eq!(attempts, 2, "two records, and the third failure is the ack");
    let acks = acks_of(&store, &effect_id);
    assert_eq!(acks.len(), 1);
    let result = acks[0].get("result").expect("an ack carries a result");
    assert_eq!(
        result.get("error").and_then(Value::as_str),
        Some("exec/retries_exhausted")
    );
    assert_eq!(result.get("attempts").and_then(Value::as_num), Some("3"));
    assert_eq!(
        result.get("class").and_then(Value::as_str),
        Some("mcp_error")
    );
    // Exhaustion and dead-lettering work for an mcp handler exactly as for a
    // process one: same derivation, same report, no second source of truth.
    let letters = dead::dead_letters(&store, 0);
    assert_eq!(letters.len(), 1);
    assert_eq!(letters[0].effect_name.as_deref(), Some("notify"));
    assert_eq!(letters[0].attempts, 3);
    assert_eq!(letters[0].class, "mcp_error");

    // Without it, the same failure acks on the first try and is no dead letter.
    let (immediate, effect_id) = pending_notification("mcp-no-retry");
    run_to_settled(
        &immediate,
        table("mcp", r#""retry":{"attempts":3,"on":["timeout"]},"#),
    );
    let store = Store::open_read_only(immediate.path()).expect("the store opens");
    assert_eq!(
        store
            .records
            .iter()
            .filter(|record| record.kind == RecordKind::EffectAttempted)
            .count(),
        0
    );
    let acks = acks_of(&store, &effect_id);
    assert_eq!(acks.len(), 1);
    assert_eq!(
        acks[0]
            .get("result")
            .and_then(|result| result.get("error"))
            .and_then(Value::as_str),
        Some("mcp/tool_error"),
        "a failure that never exhausted keeps its own cause"
    );
    assert!(dead::dead_letters(&store, 0).is_empty());
}
