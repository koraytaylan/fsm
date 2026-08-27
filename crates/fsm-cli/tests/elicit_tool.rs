//! Asking a person, then sending what they said — and every path where
//! nothing is sent at all.
//!
//! Plan 0013 task 6403.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io::Cursor;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::{Notifier, SessionIo, SharedSink};
use fsm_cli::mcp::tools::{MUTATING_TOOLS, ToolCtx, annotations, dispatch, dispatch_with};
use fsm_cli::store::{ErrorObj, Store};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::RecordKind;

struct Scratch(std::path::PathBuf);

impl std::ops::Deref for Scratch {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}

impl AsRef<std::path::Path> for Scratch {
    fn as_ref(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Server request ids are monotonic for the life of the process, so a test
/// that scripts an answer for the next one holds the turn while it learns
/// the id and uses it.
static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-elicit-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"gate_case","states":[{"name":"waiting"},{"name":"done","terminal":true}],"initial":"waiting","context":[],"events":[{"name":"decide","fields":[{"name":"verdict","ty":"str"},{"name":"score","ty":"int"}]},{"name":"withdraw","fields":[]}],"transitions":[{"from":"waiting","on":"decide","to":"done"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "gate_case",
            "inst-gate",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn args(event: &str, request_id: &str) -> Value {
    value(&format!(
        r#"{{"instance_id":"inst-gate","event":"{event}","request_id":"{request_id}"}}"#
    ))
}

/// Run one ask against a scripted client.
fn ask(
    store: &mut Store,
    script: &str,
    elicitation: bool,
    args: &Value,
    step: i64,
) -> (SharedSink, Result<Value, ErrorObj>) {
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    // Whatever the client is scripted to say, or silence.
    let mut answered = Cursor::new(script.as_bytes().to_vec());
    let io = RefCell::new(SessionIo::new(&notifier, &mut answered));
    let ctx = ToolCtx {
        io: Some(&io),
        client_elicitation: elicitation,
        ..Default::default()
    };
    let mut clock = FixedClock::new(1_000, step);
    let result = dispatch_with(store, &mut clock, "instance_elicit", args, &ctx);
    (sink, result)
}

/// The id the server used, read back out of what it wrote.
fn question_id(sink: &SharedSink) -> String {
    let first = sink.text().lines().next().unwrap_or_default().to_string();
    parse(first.as_bytes(), &JsonLimits::DEFAULT)
        .ok()
        .and_then(|m| m.get("id").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// An answer for whichever id the server is about to choose. Ids are
/// monotonic per process, so the script is written after the question by
/// running the exchange twice: once to learn the id, once to answer it.
fn answer_for(id: &str, body: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":"{id}","result":{body}}}"#)
}

fn records(dir: &Scratch) -> Vec<RecordKind> {
    Store::open_read_only(dir)
        .unwrap()
        .records
        .iter()
        .map(|r| r.kind)
        .collect()
}

/// Learn the next id by asking once with no answer, then ask again with one.
fn ask_answering(
    store: &mut Store,
    body: &str,
    args: &Value,
) -> (SharedSink, Result<Value, ErrorObj>) {
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let (probe, _) = ask(store, "", true, args, 0);
    let previous: u64 = question_id(&probe)
        .trim_start_matches("fsm-elicit-")
        .parse()
        .unwrap_or(0);
    let script = format!(
        "{}\n",
        answer_for(&format!("fsm-elicit-{}", previous + 1), body)
    );
    ask(store, &script, true, args, 0)
}

#[test]
fn an_accepted_ask_sends_the_event_and_returns_the_view() {
    let dir = scratch("accept");
    let mut store = seeded(&dir);
    let (sink, result) = ask_answering(
        &mut store,
        r#"{"action":"accept","content":{"verdict":"approve","score":7}}"#,
        &args("decide", "gate-1"),
    );
    let sent = result.expect("the answer was sent");
    assert_eq!(sent.get("action").and_then(Value::as_str), Some("accept"));
    assert_eq!(sent.get("applied").and_then(Value::as_bool), Some(true));
    assert_eq!(
        sent.get("configuration")
            .and_then(|c| c.get("leaf"))
            .and_then(Value::as_str),
        Some("done"),
        "the event actually advanced the workflow"
    );

    // The question carried a schema built from the machine's own fields.
    let question = parse(
        sink.text().lines().next().unwrap().as_bytes(),
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    assert_eq!(
        question.get("method").and_then(Value::as_str),
        Some("elicitation/create")
    );
    let schema = question
        .get("params")
        .and_then(|p| p.get("requestedSchema"))
        .expect("a requested schema");
    assert_eq!(
        schema
            .get("properties")
            .and_then(|p| p.get("score"))
            .and_then(|s| s.get("type"))
            .and_then(Value::as_str),
        Some("integer")
    );

    // What happened to the workflow is that an event arrived: one applied
    // event, and no new record kind.
    drop(store);
    let kinds = records(&dir);
    assert_eq!(
        kinds
            .iter()
            .filter(|k| **k == RecordKind::EventApplied)
            .count(),
        1
    );
    assert!(
        kinds.iter().all(|k| matches!(
            k,
            RecordKind::Genesis
                | RecordKind::MachineDefined
                | RecordKind::InstanceCreated
                | RecordKind::EventApplied
        )),
        "an elicitation left no record of its own: {kinds:?}"
    );
}

#[test]
fn a_declined_ask_writes_nothing_and_says_which_way() {
    for action in ["decline", "cancel"] {
        let dir = scratch(action);
        let mut store = seeded(&dir);
        let before = store.journal.last_seq;
        let (_, result) = ask_answering(
            &mut store,
            &format!(r#"{{"action":"{action}"}}"#),
            &args("decide", "gate-1"),
        );
        let outcome = result.expect("a decline is an outcome, not a failure");
        assert_eq!(outcome.get("action").and_then(Value::as_str), Some(action));
        assert_eq!(outcome.get("applied").and_then(Value::as_bool), Some(false));
        assert_eq!(store.journal.last_seq, before, "nothing was journaled");

        // And the key is unclaimed: the same id still works for something
        // else, because nothing ever claimed it.
        let sent = dispatch(
            &mut store,
            &mut FixedClock::new(2_000, 1),
            "instance_send",
            &value(
                r#"{"instance_id":"inst-gate","event":{"name":"decide","payload":{"verdict":"approve","score":"1"}},"request_id":"gate-1"}"#,
            ),
        )
        .expect("the request_id was never claimed");
        assert_eq!(sent.get("applied").and_then(Value::as_bool), Some(true));
    }
}

#[test]
fn a_client_that_cannot_be_asked_is_told_to_send_instead() {
    let dir = scratch("nocap");
    let mut store = seeded(&dir);
    let before = store.journal.last_seq;
    let (sink, result) = ask(&mut store, "", false, &args("decide", "gate-1"), 0);
    let error = result.expect_err("nobody to ask");
    assert_eq!(error.code, "req/elicit_unsupported");
    assert!(error.hint.contains("instance_send"), "{}", error.hint);
    assert!(
        sink.text().is_empty(),
        "no question was written: {}",
        sink.text()
    );
    assert_eq!(store.journal.last_seq, before);
}

#[test]
fn an_event_that_cannot_fire_is_refused_before_anybody_is_asked() {
    let dir = scratch("notenabled");
    let mut store = seeded(&dir);
    let (sink, result) = ask(&mut store, "", true, &args("withdraw", "gate-1"), 0);
    let error = result.expect_err("withdraw has no transition from waiting");
    assert_eq!(error.code, "run/not_enabled");
    assert!(
        !sink.text().contains("elicitation/create"),
        "asking a person to fill in a form for an event that cannot fire is \
         worse than refusing: {}",
        sink.text()
    );
}

#[test]
fn an_answer_that_will_not_coerce_names_the_field_and_writes_nothing() {
    let dir = scratch("coerce");
    let mut store = seeded(&dir);
    let before = store.journal.last_seq;
    let (_, result) = ask_answering(
        &mut store,
        r#"{"action":"accept","content":{"verdict":"approve"}}"#,
        &args("decide", "gate-1"),
    );
    let error = result.expect_err("a missing field");
    assert_eq!(error.code, "req/field_missing");
    assert!(error.message.contains("score"), "{}", error.message);
    assert_eq!(store.journal.last_seq, before);
}

#[test]
fn a_client_that_never_answers_times_out_with_the_key_unclaimed() {
    // This one writes a question, so it takes an id — and a test predicting
    // the next id must not have one taken out from under it.
    let _turn = TURN.lock().unwrap_or_else(|e| e.into_inner());
    let dir = scratch("timeout");
    let mut store = seeded(&dir);
    let before = store.journal.last_seq;
    let (_, result) = ask(
        &mut store,
        "",
        true,
        &args("decide", "gate-1"),
        fsm_cli::mcp::elicit::DEFAULT_TIMEOUT_MS + 1,
    );
    assert_eq!(
        result.expect_err("nobody answered").code,
        "req/elicit_timeout"
    );
    assert_eq!(store.journal.last_seq, before);
    // Unclaimed: the same key still lands an ordinary send.
    dispatch(
        &mut store,
        &mut FixedClock::new(2_000, 1),
        "instance_send",
        &value(
            r#"{"instance_id":"inst-gate","event":{"name":"decide","payload":{"verdict":"approve","score":"1"}},"request_id":"gate-1"}"#,
        ),
    )
    .expect("the request_id was never claimed");
}

#[test]
fn a_second_ask_while_one_is_outstanding_is_refused() {
    let dir = scratch("nested");
    let mut store = seeded(&dir);
    let sink = SharedSink::new();
    let notifier = Notifier::new(Box::new(sink.writer()));
    let mut reader = Cursor::new(Vec::new());
    let io = RefCell::new(SessionIo::new(&notifier, &mut reader));
    let outstanding = io.borrow_mut();
    let ctx = ToolCtx {
        io: Some(&io),
        client_elicitation: true,
        ..Default::default()
    };
    let error = dispatch_with(
        &mut store,
        &mut FixedClock::new(1_000, 0),
        "instance_elicit",
        &args("decide", "gate-1"),
        &ctx,
    )
    .expect_err("a recursive ask is a design mistake");
    assert_eq!(error.code, "req/elicit_nested");
    assert!(sink.text().is_empty());
    drop(outstanding);
}

#[test]
fn the_cli_path_says_there_is_nobody_to_ask() {
    let dir = scratch("nosession");
    let mut store = seeded(&dir);
    let error = dispatch(
        &mut store,
        &mut FixedClock::new(1_000, 1),
        "instance_elicit",
        &args("decide", "gate-1"),
    )
    .expect_err("no session");
    assert_eq!(error.code, "req/elicit_unsupported");
    assert!(error.hint.contains("instance_send"));
}

#[test]
fn it_writes_so_a_read_only_server_refuses_it() {
    assert!(MUTATING_TOOLS.contains(&"instance_elicit"));
    let derived = annotations("instance_elicit");
    assert_eq!(derived.get("readOnlyHint"), Some(&Value::Bool(false)));
    assert_eq!(
        derived.get("idempotentHint"),
        Some(&Value::Bool(true)),
        "its answer is sent under the caller's request_id, like every other writer"
    );

    let dir = scratch("readonly");
    let store = seeded(&dir);
    drop(store);
    let mut store = Store::open_read_only(&dir).unwrap();
    let error = dispatch(
        &mut store,
        &mut FixedClock::new(1_000, 1),
        "instance_elicit",
        &args("decide", "gate-1"),
    )
    .expect_err("a read-only server cannot send the answer");
    assert_eq!(
        error.code, "io/write",
        "the mode-naming refusal every writer gets"
    );
    assert!(
        error.message.contains("instance_elicit"),
        "{}",
        error.message
    );
}
