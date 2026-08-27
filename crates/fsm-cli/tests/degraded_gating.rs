//! What a degraded server will and will not do, and what it says instead.
//!
//! A caller that stumbles into a refusal should learn exactly what it would
//! have learned by asking `store_doctor`. An error that only says
//! "unavailable" makes a model retry; one that carries the health, the blast
//! radius and the remedy makes it diagnose.
//!
//! Plan 0014 task 6702.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::notify::SharedSink;
use fsm_cli::mcp::serve::{ServeMode, serve_dir_with};
use fsm_cli::mcp::tools::{DEGRADED_TOOLS, ToolCtx, dispatch_degraded, names};
use fsm_cli::store::{ErrorObj, Store};
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
        "fsm-gating-{tag}-{}-{}",
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

const CASE: &str = r#"{"format":"fsm.machine/1","name":"gating_case","states":[{"name":"open"},{"name":"held"}],"initial":"open","context":[],"events":[{"name":"push","fields":[]}],"transitions":[{"from":"open","on":"push","to":"held"},{"from":"held","on":"push","to":"open"}]}"#;

fn segment(dir: &Scratch) -> std::path::PathBuf {
    dir.join("journal/seg-00000000000000000000.jsonl")
}

/// A store that classifies as torn and will not open.
fn torn(tag: &str) -> Scratch {
    let dir = scratch(tag);
    let mut store = Store::open(&dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "gating_case",
            "inst-g",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    // One event after the creation, so the torn record is that event and
    // the instance itself survives a repair.
    store
        .send_event(
            "inst-g",
            "push",
            Value::Obj(BTreeMap::new()),
            "seed-push",
            None,
        )
        .unwrap();
    drop(store);
    let mut bytes = fs::read(segment(&dir)).unwrap();
    bytes.truncate(bytes.len() - 3);
    fs::write(segment(&dir), &bytes).unwrap();
    assert!(Store::open(&dir).is_err());
    dir
}

/// A store nothing can open, read-only included: the bytes of a record are
/// no longer canonical, which is interior damage rather than a torn tail.
fn unopenable(tag: &str) -> Scratch {
    let dir = torn(tag);
    let mut bytes = fs::read(segment(&dir)).unwrap();
    let position = bytes.iter().position(|b| *b == b'{').unwrap();
    bytes.insert(position + 1, b' ');
    fs::write(segment(&dir), &bytes).unwrap();
    assert!(Store::open_read_only(&dir).is_err(), "{tag} still opens");
    dir
}

fn call(dir: &Scratch, name: &str, args: &str) -> Result<Value, ErrorObj> {
    dispatch_degraded(
        dir,
        &mut FixedClock::new(2_000, 1),
        name,
        &value(args),
        &ToolCtx::default(),
    )
}

/// Plausible arguments for every tool, so a refusal is about the gate rather
/// than about a missing field.
fn arguments(name: &str) -> &'static str {
    match name {
        "machine_create" => {
            r#"{"spec":{"format":"fsm.machine/1","name":"gating_probe","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"}]}}"#
        }
        "machine_list" | "instance_list" | "store_doctor" | "journal_verify" | "journal_replay" => {
            "{}"
        }
        "machine_get" | "machine_analyze" => r#"{"machine":"gating_case"}"#,
        "machine_diagram" => r#"{"machine":"gating_case","format":"mermaid"}"#,
        "instance_create" => r#"{"machine":"gating_case","request_id":"g1"}"#,
        "instance_send" => r#"{"instance_id":"inst-g","event":{"name":"push"},"request_id":"g2"}"#,
        "deadline_poll" => r#"{"instance_id":"inst-g","request_id":"g3"}"#,
        "effect_ack" => {
            r#"{"instance_id":"inst-g","effect_id":"e1","outcome":"ok","request_id":"g4"}"#
        }
        "instance_cancel" => r#"{"instance_id":"inst-g","reason":"stop","request_id":"g5"}"#,
        "instance_migrate" => {
            r#"{"instance_id":"inst-g","to_machine":"gating_case","request_id":"g6"}"#
        }
        "invocation_start" | "invocation_return" => {
            r#"{"instance_id":"inst-g","slot":"child","request_id":"g7"}"#
        }
        "signal_deliver" => {
            r#"{"instance_id":"inst-g","signal_id":"inst-g/1/0","request_id":"g8"}"#
        }
        "instance_get" | "instance_history" => r#"{"instance_id":"inst-g"}"#,
        "instance_elicit" => r#"{"instance_id":"inst-g","event":"push","request_id":"g9"}"#,
        "explain_step" => r#"{"instance_id":"inst-g","seq":1}"#,
        "simulate" => r#"{"machine":"gating_case","events":[{"name":"push"}]}"#,
        other => panic!("no arguments authored for {other}"),
    }
}

#[test]
fn the_three_diagnostic_tools_answer_from_the_directory() {
    let dir = torn("diagnostics");
    for name in DEGRADED_TOOLS {
        let answered = call(&dir, name, "{}").unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let text = format!("{answered:?}");
        assert!(
            text.contains("TornTail") || text.contains("matches") || text.contains("health"),
            "{name} answered without saying anything about the store: {text:.200}"
        );
    }
    // And they report the store's real health, not a placeholder.
    let doctor = call(&dir, "store_doctor", "{}").unwrap();
    assert_eq!(
        doctor.get("health").and_then(Value::as_str),
        Some("TornTail")
    );
}

#[test]
fn every_other_tool_is_refused_with_the_diagnosis() {
    let dir = torn("refusals");
    let doctor = call(&dir, "store_doctor", "{}").unwrap();
    for name in names() {
        if DEGRADED_TOOLS.contains(&name) {
            continue;
        }
        let error = call(&dir, name, arguments(name))
            .expect_err("a store-backed tool cannot answer without a store");
        assert_eq!(error.code, "store/degraded", "{name}");
        // The same three facts, from the same source, so the refusal and
        // `store_doctor` can never disagree.
        assert_eq!(
            error.details.get("health"),
            doctor.get("health"),
            "{name} reported a different health from store_doctor"
        );
        assert_eq!(
            error.details.get("message"),
            doctor.get("message"),
            "{name}"
        );
        assert_eq!(error.details.get("remedy"), doctor.get("remedy"), "{name}");
        assert!(
            error.hint.contains("store_doctor"),
            "{name}: a refusal must point somewhere: {}",
            error.hint
        );
    }
}

#[test]
fn the_allowed_set_is_the_constant_rather_than_a_list_in_a_test() {
    let dir = torn("constant");
    for name in names() {
        let allowed = DEGRADED_TOOLS.contains(&name);
        let answered = call(&dir, name, arguments(name)).is_ok();
        // `machine_create` is the documented exception below.
        if name == "machine_create" {
            continue;
        }
        assert_eq!(
            answered, allowed,
            "{name}: answered={answered} but DEGRADED_TOOLS says {allowed}"
        );
    }
}

#[test]
fn authoring_still_works_because_it_needs_no_store() {
    let dir = torn("dryrun");
    let spec = arguments("machine_create");
    let dry = spec.replace("{\"spec\"", "{\"dry_run\":true,\"spec\"");
    let checked = call(&dir, "machine_create", &dry).expect("a definition is checked, not stored");
    assert_eq!(
        checked.get("dry_run").and_then(Value::as_bool),
        Some(true),
        "a dry run says it was one: {checked:?}"
    );
    assert!(
        Store::open(&dir).is_err(),
        "and the store is still the store that would not open"
    );

    // Without `dry_run` it is a write, and there is nothing to write to.
    let error = call(&dir, "machine_create", spec).expect_err("a real create needs a store");
    assert_eq!(error.code, "store/degraded");

    // And a bad definition still fails as a bad definition, not as a
    // degraded store: the check is real.
    let bad = r#"{"dry_run":true,"spec":{"format":"fsm.machine/1","name":"bad","states":[{"name":"a"}],"initial":"nope","context":[],"events":[],"transitions":[]}}"#;
    let error = call(&dir, "machine_create", bad).expect_err("an invalid definition");
    assert_ne!(error.code, "store/degraded", "{error:?}");
}

#[test]
fn a_degraded_read_only_server_reports_the_reason_that_applies() {
    // Both constraints hold; the stronger one is the true one, and a caller
    // told "read-only" would try the same call against a writer and fail
    // again for a reason nobody named.
    let dir = unopenable("both");
    let sink = SharedSink::new();
    let input = format!(
        "{}\n{}\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"instance_send","arguments":{"instance_id":"inst-g","event":{"name":"push"},"request_id":"g1"}}}"#
    );
    serve_dir_with(
        &dir,
        ServeMode::ReadOnly,
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();
    let refusal = sink
        .text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .find(|m| m.get("id").and_then(Value::as_num) == Some("2"))
        .expect("the call is answered");
    let text = format!("{refusal:?}");
    assert!(text.contains("store/degraded"), "{text:.300}");
    assert!(
        !text.contains("read-only"),
        "the read-only reason would send a caller to a writer that fails too: {text:.300}"
    );
}

#[test]
fn the_tool_list_is_the_same_list() {
    // A shrinking list would make a client cache a surface that reappears
    // when the store is repaired. The refusals are self-describing instead.
    let dir = torn("list");
    let sink = SharedSink::new();
    let input = format!(
        "{}\n{}\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#
    );
    serve_dir_with(
        &dir,
        ServeMode::Writer,
        Cursor::new(input.into_bytes()),
        sink.writer(),
    )
    .unwrap();
    let degraded_list = sink
        .text()
        .lines()
        .filter_map(|line| parse(line.as_bytes(), &JsonLimits::DEFAULT).ok())
        .find(|m| m.get("id").and_then(Value::as_num) == Some("2"))
        .and_then(|m| m.get("result").cloned())
        .expect("answered");
    assert_eq!(
        fsm_core::canon::canon_bytes(&degraded_list),
        fsm_core::canon::canon_bytes(&fsm_cli::mcp::tools::tools_list_result()),
        "the tool list does not depend on whether the store opened"
    );
}

#[test]
fn a_refused_tool_writes_nothing_and_claims_no_key() {
    let dir = torn("nowrite");
    let before = fs::read(segment(&dir)).unwrap();
    let _ = call(
        &dir,
        "instance_send",
        r#"{"instance_id":"inst-g","event":{"name":"push"},"request_id":"reused"}"#,
    );
    assert_eq!(
        fs::read(segment(&dir)).unwrap(),
        before,
        "a refusal wrote to the journal"
    );
    // The key is unclaimed, so the same one still works once the store is
    // repaired — proved by repairing it and using the key.
    fsm_cli::journal_io::repair_truncate_torn_tail(&dir).expect("the torn tail is repairable");
    let mut store = Store::open(&dir).expect("and then the store opens");
    store
        .send_event(
            "inst-g",
            "push",
            Value::Obj(BTreeMap::new()),
            "reused",
            None,
        )
        .expect("the request_id was never claimed");
}
