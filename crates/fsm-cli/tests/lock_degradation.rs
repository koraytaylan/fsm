//! A writer somebody else holds is a reason to start read-only, not a
//! reason to exit before the client connects.
//!
//! Plan 0015 task 7202.

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
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-contend-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"lock_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "lock_case",
            "inst-l",
            "create-1",
            None,
            &std::collections::BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

const HELLO: &str =
    r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#;

fn session(dir: &Scratch, lines: &[&str]) -> Vec<Value> {
    let sink = SharedSink::new();
    let input: String = lines.iter().map(|line| format!("{line}\n")).collect();
    serve_dir_with(
        dir,
        ServeMode::Writer,
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .expect("a contended server runs");
    sink.text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .collect()
}

fn reply(messages: &[Value], id: &str) -> Option<Value> {
    messages
        .iter()
        .find(|m| m.get("id").and_then(|i| i.as_num().or_else(|| i.as_str())) == Some(id))
        .cloned()
}

#[test]
fn a_held_writer_starts_a_server_instead_of_killing_one() {
    let dir = scratch("held");
    // Somebody else has the writer for the whole session.
    let holder = seeded(&dir);
    let messages = session(&dir, &[HELLO]);
    let initialize = reply(&messages, "1").expect("initialize is answered");
    assert!(initialize.get("result").is_some(), "{initialize:?}");
    drop(holder);
}

#[test]
fn the_instructions_say_busy_rather_than_broken() {
    let dir = scratch("note");
    let holder = seeded(&dir);
    let messages = session(&dir, &[HELLO]);
    let instructions = reply(&messages, "1")
        .and_then(|m| m.get("result").and_then(|r| r.get("instructions")).cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .expect("instructions");
    assert!(
        instructions.contains("contended"),
        "a model must be told which state it is in: {instructions}"
    );
    assert!(
        instructions.contains("healthy and busy"),
        "and that this is not a fault: {instructions}"
    );
    assert!(
        instructions.contains("paired deployment"),
        "and what to do about it: {instructions}"
    );
    // The two states are told apart by their words, because their remedies
    // are completely different.
    assert!(
        !instructions.contains("store_doctor"),
        "a busy store is not a store to diagnose: {instructions}"
    );
    drop(holder);
}

#[test]
fn the_client_hears_the_reason_at_error_level() {
    let dir = scratch("logged");
    let holder = seeded(&dir);
    let messages = session(&dir, &[HELLO]);
    let logged = messages
        .iter()
        .find(|m| m.get("method").and_then(Value::as_str) == Some("notifications/message"))
        .expect("a client reading only stdout still learns why");
    let detail = format!("{:?}", logged.get("params"));
    assert!(detail.contains("error"), "{detail}");
    assert!(detail.contains("holds the writer"), "{detail}");
    drop(holder);
}

#[test]
fn a_contended_session_reads_and_refuses_to_write() {
    let dir = scratch("refuses");
    let holder = seeded(&dir);
    let messages = session(
        &dir,
        &[
            HELLO,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_get","arguments":{"instance_id":"inst-l"}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-l","event":{"name":"push"},"request_id":"contended-1"}}}"#,
        ],
    );
    let read = reply(&messages, "2").expect("a read is answered");
    assert!(
        format!("{read:?}").contains("configuration"),
        "reads work normally on a busy store: {read:?}"
    );
    let write = reply(&messages, "3").expect("a write is answered");
    let text = format!("{write:?}");
    assert!(
        text.contains("read-only") || text.contains("io/write"),
        "a write must be refused with the mode named: {text:.300}"
    );
    drop(holder);
}

#[test]
fn a_writer_released_inside_the_window_yields_a_full_session() {
    // The executor takes and releases the writer once a tick, so a brief
    // collision at startup is expected rather than fatal — the retry window
    // exists for exactly that.
    let dir = scratch("released");
    let holder = seeded(&dir);
    let path = dir.to_path_buf();
    let releasing = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        drop(holder);
    });
    let messages = session(
        &dir,
        &[
            HELLO,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-l","event":{"name":"push"},"request_id":"after-release"}}}"#,
        ],
    );
    releasing.join().unwrap();
    let write = reply(&messages, "2").expect("answered");
    assert!(
        format!("{write:?}").contains("\"applied\": Bool(true)")
            || format!("{write:?}").contains("applied"),
        "a writer released inside the window yields a writer: {write:?}"
    );
    // And the event really landed.
    let store = Store::open_read_only(&path).unwrap();
    assert!(
        store
            .records
            .iter()
            .any(|record| record.kind == fsm_core::record::RecordKind::EventApplied),
        "the write was refused even though the lock came free"
    );
}

#[test]
fn the_two_unavailable_states_do_not_share_their_words() {
    // Plan 0014's degraded note and this one must not be confusable: one
    // says diagnose and repair, the other says stop the other writer.
    let degraded = scratch("degraded");
    {
        let store = seeded(&degraded);
        drop(store);
    }
    let segment = degraded.join("journal/seg-00000000000000000000.jsonl");
    let mut bytes = std::fs::read(&segment).unwrap();
    bytes.truncate(bytes.len() - 3);
    std::fs::write(&segment, &bytes).unwrap();

    let unhealthy = session(&degraded, &[HELLO]);
    let unhealthy_note = reply(&unhealthy, "1")
        .and_then(|m| m.get("result").and_then(|r| r.get("instructions")).cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap();

    let busy_dir = scratch("busy");
    let holder = seeded(&busy_dir);
    let busy = session(&busy_dir, &[HELLO]);
    let busy_note = reply(&busy, "1")
        .and_then(|m| m.get("result").and_then(|r| r.get("instructions")).cloned())
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap();
    drop(holder);

    assert_ne!(unhealthy_note, busy_note);
    assert!(unhealthy_note.contains("store_doctor"), "{unhealthy_note}");
    assert!(busy_note.contains("paired deployment"), "{busy_note}");
    assert!(
        !busy_note.contains("could not open its store"),
        "a busy store opened fine: {busy_note}"
    );
}
