//! A store that will not open starts a server instead of killing one.
//!
//! Diagnosis is precisely the case where the server must not vanish: a model
//! cannot ask what is wrong with a store if pointing a server at it exits
//! before the client connects.
//!
//! Plan 0014 task 6701.

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::{ServeMode, serve_dir_with};
use fsm_cli::store::Store;
use fsm_core::json::{JsonLimits, Value, parse};

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
        let _ = fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-degraded-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    Scratch(path)
}

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

const CASE: &str = r#"{"format":"fsm.machine/1","name":"degraded_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "degraded_case",
            "inst-g",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn segment(dir: &Scratch) -> std::path::PathBuf {
    dir.join("journal/seg-00000000000000000000.jsonl")
}

/// A store with a torn tail: it classifies, and it will not open.
fn torn(tag: &str) -> Scratch {
    let dir = scratch(tag);
    let store = seeded(&dir);
    drop(store);
    let mut bytes = fs::read(segment(&dir)).unwrap();
    bytes.truncate(bytes.len() - 3);
    fs::write(segment(&dir), &bytes).unwrap();
    assert!(Store::open(&dir).is_err(), "the fixture must not open");
    dir
}

fn session(dir: &Scratch, mode: ServeMode, lines: &[&str]) -> Vec<Value> {
    let sink = SharedSink::new();
    let input: String = lines.iter().map(|line| format!("{line}\n")).collect();
    serve_dir_with(dir, mode, Cursor::new(input.into_bytes()), sink.writer())
        .expect("a degraded server runs, and a clean disconnect is not a failure");
    sink.text()
        .lines()
        .map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).expect("a whole message"))
        .collect()
}

const HELLO: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

fn reply(messages: &[Value], id: &str) -> Option<Value> {
    messages
        .iter()
        .find(|m| m.get("id").and_then(|i| i.as_num().or_else(|| i.as_str())) == Some(id))
        .cloned()
}

#[test]
fn a_store_that_will_not_open_starts_a_server_anyway() {
    let dir = torn("starts");
    let messages = session(
        &dir,
        ServeMode::Writer,
        &[HELLO, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#],
    );
    let initialize = reply(&messages, "1").expect("initialize is answered");
    assert!(
        initialize.get("result").is_some(),
        "the session starts: {initialize:?}"
    );

    // The tool list is unchanged, because the tools are unchanged: which of
    // them can answer is a property of this store, not of this build.
    let tools = reply(&messages, "2").expect("tools/list is answered");
    let listed = tools
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_arr)
        .expect("a tool array");
    assert_eq!(listed.len(), fsm_cli::mcp::tools::names().len());
}

#[test]
fn the_instructions_say_what_happened_and_what_to_call() {
    let dir = torn("note");
    let messages = session(&dir, ServeMode::Writer, &[HELLO]);
    let instructions = reply(&messages, "1")
        .and_then(|m| m.get("result").and_then(|r| r.get("instructions")).cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("instructions");
    assert!(
        instructions.contains("mode=degraded"),
        "a model must be told which state it is in"
    );
    assert!(
        instructions.contains("store_doctor"),
        "and the one tool that explains it: {instructions}"
    );
}

#[test]
fn the_client_is_told_at_error_level() {
    let dir = torn("logged");
    let messages = session(&dir, ServeMode::Writer, &[HELLO]);
    let logged = messages
        .iter()
        .find(|m| m.get("method").and_then(Value::as_str) == Some("notifications/message"))
        .expect("a client reading only stdout still learns why");
    let params = logged.get("params").expect("params");
    assert_eq!(params.get("level").and_then(Value::as_str), Some("error"));
    let detail = format!("{:?}", params.get("data"));
    assert!(detail.contains("store_doctor"), "{detail}");
    assert!(
        detail.contains("torn") || detail.contains("store/"),
        "the health travels with it: {detail}"
    );
}

#[test]
fn the_documentation_still_reads_and_the_instances_do_not() {
    let dir = torn("resources");
    let messages = session(
        &dir,
        ServeMode::Writer,
        &[
            HELLO,
            r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"fsm://docs/spec"}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":"fsm://docs/examples"}}"#,
            r#"{"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":"fsm://instance/inst-g"}}"#,
        ],
    );
    for id in ["2", "3"] {
        let read = reply(&messages, id).expect("answered");
        assert!(
            read.get("result").is_some(),
            "the documentation is a constant in the binary: {read:?}"
        );
    }
    let missing = reply(&messages, "4").expect("answered");
    assert_eq!(
        missing
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Value::as_num),
        Some("-32002"),
        "an instance nobody can read is not found"
    );
}

#[test]
fn the_diagnostic_tools_answer_from_the_directory() {
    let dir = torn("tools");
    let messages = session(
        &dir,
        ServeMode::Writer,
        &[
            HELLO,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"store_doctor","arguments":{}}}"#,
        ],
    );
    let called = reply(&messages, "2").expect("answered");
    let text = format!("{called:?}");
    // Whether it answers or refuses, it must not be silence — and in a
    // degraded session the answer a caller needs is the diagnosis. `6702`
    // routes it; here the session must at least survive the call.
    assert!(
        called.get("result").is_some() || called.get("error").is_some(),
        "{text}"
    );
}

#[test]
fn an_embedded_server_starts_without_an_executor() {
    let dir = torn("embedded");
    // `--execute` against a store that will not open: nothing to write to,
    // so nothing runs — and the session still starts.
    let table = fsm_execute::config::HandlerTable::parse(
        r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":1000}]}"#,
    )
    .unwrap();
    let loop_ = fsm_cli::mcp::serve::ExecutorLoop::new(&dir, table).unwrap();
    let messages = session(&dir, ServeMode::Embedded(Box::new(loop_)), &[HELLO]);
    assert!(reply(&messages, "1").is_some());
}

#[test]
fn a_healthy_store_is_untouched_by_any_of_this() {
    // The whole task must be inert for every working deployment.
    let dir = scratch("healthy");
    let store = seeded(&dir);
    drop(store);
    let messages = session(
        &dir,
        ServeMode::Writer,
        &[HELLO, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#],
    );
    let instructions = reply(&messages, "1")
        .and_then(|m| m.get("result").and_then(|r| r.get("instructions")).cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap();
    assert!(
        !instructions.contains("degraded"),
        "a healthy server says nothing about a state it is not in"
    );
    assert!(
        !messages
            .iter()
            .any(|m| m.get("method").and_then(Value::as_str) == Some("notifications/message")),
        "and pushes nothing it would not have pushed before"
    );
}

#[test]
fn every_kind_of_unopenable_store_degrades_rather_than_exits() {
    // A broken chain, and a VERSION nobody can read.
    let dir = scratch("chain");
    let store = seeded(&dir);
    drop(store);
    let mut bytes = fs::read(segment(&dir)).unwrap();
    let position = bytes.iter().position(|b| *b == b'{').unwrap();
    bytes.insert(position + 1, b' ');
    fs::write(segment(&dir), &bytes).unwrap();
    assert!(Store::open(&dir).is_err());
    assert!(reply(&session(&dir, ServeMode::Writer, &[HELLO]), "1").is_some());

    let dir = scratch("version");
    let store = seeded(&dir);
    drop(store);
    fs::write(dir.join("VERSION"), b"not-a-version\n").unwrap();
    assert!(Store::open(&dir).is_err());
    assert!(
        reply(&session(&dir, ServeMode::Writer, &[HELLO]), "1").is_some(),
        "a store this build cannot read is still a store worth diagnosing"
    );
}
