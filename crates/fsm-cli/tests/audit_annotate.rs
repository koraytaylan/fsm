//! A note in the audit trail, and the state it does not change.
//!
//! Plan 0014 task 6801.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{MUTATING_TOOLS, annotations, dispatch};
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

fn scratch(tag: &str) -> Scratch {
    let path = std::env::temp_dir().join(format!(
        "fsm-annotate-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"note_case","states":[{"name":"open"},{"name":"done","terminal":true}],"initial":"open","context":[{"name":"seen","ty":"int","init":"0"}],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"open","on":"finish","to":"done","do":[{"target":"seen","value":"ctx.seen + 1"}]}]}"#;

fn seeded(dir: &Scratch) -> Store {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "note_case",
            "inst-n",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    store
}

fn annotate(store: &mut Store, note: &str, request_id: &str) -> Result<Value, ErrorObj> {
    dispatch(
        store,
        &mut FixedClock::new(2_000, 1),
        "instance_annotate",
        &Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str("inst-n".into())),
            ("note".into(), Value::Str(note.into())),
            ("request_id".into(), Value::Str(request_id.into())),
        ])),
    )
}

fn field(report: &Value, name: &str) -> Option<String> {
    report.get(name).and_then(|v| match v {
        Value::Str(s) => Some(s.clone()),
        Value::Num(n) => Some(n.clone()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

#[test]
fn a_note_is_written_and_nothing_moves() {
    let dir = scratch("write");
    let mut store = seeded(&dir);
    let before_leaf = store.state.instances["inst-n"]
        .configuration
        .sequential_leaf()
        .map(str::to_string);
    let before_ctx = store.state.instances["inst-n"].ctx.clone();

    let noted = annotate(&mut store, "cancelled after the customer called", "note-1")
        .expect("a note is always legal");
    assert_eq!(
        field(&noted, "note").as_deref(),
        Some("cancelled after the customer called")
    );
    assert_eq!(field(&noted, "duplicate").as_deref(), Some("false"));

    // The view comes back with it, unchanged.
    assert_eq!(
        noted
            .get("configuration")
            .and_then(|c| c.get("leaf"))
            .and_then(Value::as_str)
            .map(str::to_string),
        before_leaf,
        "an annotation moved the instance"
    );
    assert_eq!(
        store.state.instances["inst-n"].ctx, before_ctx,
        "an annotation changed the context"
    );

    // Exactly one record, of the kind SPEC names.
    let annotated: Vec<_> = store
        .records
        .iter()
        .filter(|r| r.kind == RecordKind::Annotated)
        .collect();
    assert_eq!(annotated.len(), 1);
    assert_eq!(
        annotated[0].body.get("note").and_then(Value::as_str),
        Some("cancelled after the customer called")
    );
    assert_eq!(
        field(&noted, "seq").as_deref(),
        Some(annotated[0].seq.to_string().as_str()),
        "and the reported seq is where a reader will find it"
    );
}

#[test]
fn the_note_is_in_the_history_at_the_seq_it_reported() {
    let dir = scratch("history");
    let mut store = seeded(&dir);
    let noted = annotate(&mut store, "ticket AB-1", "note-1").unwrap();
    let seq: u64 = field(&noted, "seq").unwrap().parse().unwrap();
    let history = dispatch(
        &mut store,
        &mut FixedClock::new(2_000, 1),
        "instance_history",
        &value(r#"{"instance_id":"inst-n"}"#),
    )
    .unwrap();
    let text = format!("{history:?}");
    assert!(text.contains("ticket AB-1"), "the note is in the trail");
    assert!(
        text.contains(&seq.to_string()),
        "at the seq the tool reported"
    );
}

#[test]
fn the_same_key_replays_and_a_different_note_conflicts() {
    let dir = scratch("idempotent");
    let mut store = seeded(&dir);
    let first = annotate(&mut store, "same words", "note-1").unwrap();
    let records = store.records.len();

    let replayed = annotate(&mut store, "same words", "note-1").expect("a replay is not a failure");
    assert_eq!(field(&replayed, "duplicate").as_deref(), Some("true"));
    assert_eq!(field(&replayed, "seq"), field(&first, "seq"));
    assert_eq!(
        store.records.len(),
        records,
        "a replay wrote a second record"
    );

    let conflict = annotate(&mut store, "different words", "note-1")
        .expect_err("a key means the content it was claimed for");
    assert_eq!(conflict.code, "req/request_id_conflict");
    assert_eq!(store.records.len(), records);
}

#[test]
fn an_oversized_note_claims_no_key_and_a_shorter_one_lands_under_it() {
    let dir = scratch("toolong");
    let mut store = seeded(&dir);
    let huge = "x".repeat(fsm_core::limits::MAX_PAYLOAD_BYTES + 1);
    let records = store.records.len();
    let error = annotate(&mut store, &huge, "note-big").expect_err("too long");
    assert_eq!(error.code, "req/payload_too_large");
    assert_eq!(store.records.len(), records, "nothing was journaled");

    // The same key still works, because it was never consumed: correct and
    // resend is the whole recovery.
    annotate(&mut store, "the short version", "note-big")
        .expect("an oversized note consumes no request_id");
}

#[test]
fn a_finished_instance_can_still_be_annotated() {
    // A note about why something ended is exactly the note somebody wants to
    // leave, so the lifecycle gate that stops `instance_send` does not apply.
    let dir = scratch("finished");
    let mut store = seeded(&dir);
    store
        .send_event(
            "inst-n",
            "finish",
            Value::Obj(BTreeMap::new()),
            "finish-1",
            None,
        )
        .unwrap();
    annotate(&mut store, "closed by the review board", "note-done")
        .expect("a completed instance takes notes");

    let dir = scratch("cancelled");
    let mut store = seeded(&dir);
    store.cancel_instance("inst-n", "cancel-1").unwrap();
    annotate(
        &mut store,
        "cancelled because the customer withdrew",
        "note-cancel",
    )
    .expect("a cancelled instance takes notes");
}

#[test]
fn an_unknown_instance_is_a_structured_error() {
    let dir = scratch("unknown");
    let mut store = seeded(&dir);
    let error = dispatch(
        &mut store,
        &mut FixedClock::new(2_000, 1),
        "instance_annotate",
        &value(r#"{"instance_id":"inst-nope","note":"hello","request_id":"note-x"}"#),
    )
    .expect_err("no such instance");
    assert_eq!(error.code, "req/instance_not_found");
}

#[test]
fn it_writes_so_a_read_only_server_refuses_it() {
    assert!(MUTATING_TOOLS.contains(&"instance_annotate"));
    let derived = annotations("instance_annotate");
    assert_eq!(derived.get("readOnlyHint"), Some(&Value::Bool(false)));
    assert_eq!(derived.get("destructiveHint"), Some(&Value::Bool(false)));
    assert_eq!(
        derived.get("idempotentHint"),
        Some(&Value::Bool(true)),
        "it takes a request_id like every other writer"
    );

    let dir = scratch("readonly");
    let store = seeded(&dir);
    drop(store);
    let mut store = Store::open_read_only(&dir).unwrap();
    let error = annotate(&mut store, "no writer here", "note-ro").expect_err("read-only");
    assert_eq!(error.code, "io/write");
    assert!(
        error.message.contains("instance_annotate"),
        "{}",
        error.message
    );
}

#[test]
fn the_tool_and_the_command_line_agree() {
    // CLI/MCP parity: the tool wraps `Store::annotate`, so the record and the
    // note are the same whichever surface wrote them.
    let dir = scratch("parity");
    let mut store = seeded(&dir);
    let via_tool = annotate(&mut store, "one way", "note-tool").unwrap();
    let via_store = store
        .annotate("inst-n", "note-store", "another way")
        .unwrap();
    assert_eq!(
        via_tool.get("note").and_then(Value::as_str),
        Some("one way")
    );
    assert_eq!(
        via_store.get("note").and_then(Value::as_str),
        Some("another way")
    );
    let notes: Vec<&str> = store
        .records
        .iter()
        .filter(|r| r.kind == RecordKind::Annotated)
        .filter_map(|r| r.body.get("note").and_then(Value::as_str))
        .collect();
    assert_eq!(notes, ["one way", "another way"]);
}
