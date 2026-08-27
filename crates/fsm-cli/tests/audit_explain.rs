//! Why one step did what it did, reachable at last.
//!
//! Plan 0014 task 6601.

// One helper hands back the store's own `ErrorObj`, which is what the code
// under test reports a failure with. Boxing it here would only make every
// assertion dereference to read a code.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::{MUTATING_TOOLS, annotations, dispatch};
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
        "fsm-explain-{tag}-{}-{}",
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

/// A guard that decides, an action that computes, and an invariant that
/// holds — so an explanation has all three to report. `settle` is eventless
/// and raises the reaction plan 0009 counts as a microstep.
const CASE: &str = r#"{"format":"fsm.machine/1","name":"explain_case","context":[{"name":"score","ty":"int","init":"0"},{"name":"seen","ty":"int","init":"0"}],"invariants":[{"name":"score_never_negative","expr":"ctx.score >= 0","mode":"enforce"}],"states":[{"name":"open"},{"name":"scored"},{"name":"closed","terminal":true}],"initial":"open","events":[{"name":"score","fields":[{"name":"points","ty":"int"}]},{"name":"shut","fields":[]}],"transitions":[{"from":"open","on":"score","if":"evt.points > 0","to":"scored","do":[{"target":"score","value":"ctx.score + evt.points"},{"target":"seen","value":"ctx.seen + 1"}]},{"from":"open","on":"score","to":"open"},{"from":"scored","to":"closed","if":"ctx.score > 100"},{"from":"scored","on":"shut","to":"closed"}]}"#;

/// A store with one instance and one applied, one rejected step.
fn seeded(dir: &Scratch) -> (Store, u64, u64) {
    let mut store = Store::open(dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "explain_case",
            "inst-x",
            "create-1",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let applied = store
        .send_event(
            "inst-x",
            "score",
            value(r#"{"points":"5"}"#),
            "score-1",
            None,
        )
        .unwrap()
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap();
    // An event the current configuration has no handler for: journaled as a
    // rejection, with its own trace.
    let _ = store.send_event(
        "inst-x",
        "score",
        value(r#"{"points":"1"}"#),
        "score-2",
        None,
    );
    let rejected = store
        .records
        .iter()
        .rev()
        .find(|r| r.kind == fsm_core::record::RecordKind::EventRejected)
        .map(|r| r.seq)
        .expect("the refusal is journaled like everything else");
    (store, applied, rejected)
}

fn explain(store: &mut Store, instance: &str, seq: u64) -> Result<Value, fsm_cli::store::ErrorObj> {
    dispatch(
        store,
        &mut FixedClock::new(2_000, 1),
        "explain_step",
        &value(&format!(r#"{{"instance_id":"{instance}","seq":{seq}}}"#)),
    )
}

#[test]
fn an_applied_step_explains_itself_in_full() {
    let dir = scratch("applied");
    let (mut store, applied, _) = seeded(&dir);
    let explained = explain(&mut store, "inst-x", applied).expect("the step exists");
    let text = format!("{explained:?}");

    // Every candidate, with the verdict that decided between them.
    assert!(
        text.contains("candidates"),
        "an explanation without candidates is a summary: {text:.400}"
    );
    assert!(text.contains("score"), "{text:.400}");
    // What the actions computed, before and after.
    assert!(
        text.contains("before") && text.contains("after"),
        "the block pipeline's values are the point of explaining: {text:.400}"
    );
    // And what the invariants said about the result.
    assert!(
        text.contains("score_never_negative"),
        "invariant results are part of the decision: {text:.400}"
    );
    assert_eq!(
        explained.get("seq").and_then(Value::as_num),
        Some(applied.to_string().as_str())
    );
}

#[test]
fn the_tool_and_the_command_line_say_exactly_the_same_thing() {
    // Divergence between the two surfaces is the failure this tool's
    // implementation exists to avoid, so it is asserted directly rather than
    // trusted to a shared call site.
    let dir = scratch("parity");
    let (mut store, applied, _) = seeded(&dir);
    let from_tool = explain(&mut store, "inst-x", applied).unwrap();
    drop(store);
    let store = Store::open_read_only(&dir).unwrap();
    let from_cli = store.explain_seq("inst-x", applied).unwrap();
    assert_eq!(
        fsm_core::canon::canon_bytes(&from_tool),
        fsm_core::canon::canon_bytes(&from_cli),
        "explain_step must return explain_seq's value unchanged"
    );
}

#[test]
fn a_rejected_step_explains_with_its_own_trace() {
    let dir = scratch("rejected");
    let (mut store, applied, rejected) = seeded(&dir);
    assert_ne!(applied, rejected, "the rejection is a record of its own");
    let explained = explain(&mut store, "inst-x", rejected).expect("the rejection explains");
    assert_eq!(
        explained.get("kind").and_then(Value::as_str),
        Some("EventRejected"),
        "the record's own kind, so a reader knows this step did not apply"
    );
    assert_eq!(
        explained.get("event").and_then(Value::as_str),
        Some("score")
    );
    assert_eq!(
        explained
            .get("payload")
            .and_then(|p| p.get("points"))
            .and_then(Value::as_str),
        Some("1"),
        "what was sent is part of why it was refused"
    );
    // No candidate matched, which is the explanation: the configuration this
    // instance was in has no handler for that event.
    assert_eq!(
        explained
            .get("trace")
            .and_then(|t| t.get("candidates"))
            .and_then(Value::as_arr)
            .map(|c| c.len()),
        Some(0)
    );
    assert_eq!(
        explained.get("from_leaf").and_then(Value::as_str),
        Some("scored")
    );
    assert_eq!(
        explained.get("chain_verified").and_then(Value::as_bool),
        Some(true),
        "and the record it was read from is the one the chain covers"
    );
}

#[test]
fn a_seq_that_is_not_this_instances_is_an_error_not_an_empty_trace() {
    let dir = scratch("other");
    let (mut store, applied, _) = seeded(&dir);
    let mut clock = FixedClock::new(3_000, 1);
    store
        .create_instance_ctx_on(
            &mut clock,
            "explain_case",
            "inst-y",
            "create-2",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let error = explain(&mut store, "inst-y", applied)
        .expect_err("that seq belongs to somebody else's story");
    assert_eq!(error.code, "req/instance_not_found");
}

#[test]
fn a_seq_past_the_end_of_the_journal_is_an_error() {
    let dir = scratch("past");
    let (mut store, _, _) = seeded(&dir);
    let beyond = store.journal.last_seq + 100;
    let error = explain(&mut store, "inst-x", beyond).expect_err("no such record");
    assert_eq!(error.code, "req/field_missing");
    // And an argument that is not a sequence number at all.
    let error = dispatch(
        &mut store,
        &mut FixedClock::new(2_000, 1),
        "explain_step",
        &value(r#"{"instance_id":"inst-x","seq":"first"}"#),
    )
    .expect_err("a seq is a number");
    assert_eq!(error.code, "req/args_invalid");
}

#[test]
fn it_reads_and_does_not_write() {
    assert!(!MUTATING_TOOLS.contains(&"explain_step"));
    let derived = annotations("explain_step");
    assert_eq!(derived.get("readOnlyHint"), Some(&Value::Bool(true)));
    assert_eq!(derived.get("openWorldHint"), Some(&Value::Bool(false)));
    assert_eq!(derived.get("destructiveHint"), Some(&Value::Bool(false)));

    // And it answers on a read-only server, which is where a diagnosis is
    // most often needed.
    let dir = scratch("readonly");
    let (store, applied, _) = seeded(&dir);
    drop(store);
    let mut store = Store::open_read_only(&dir).unwrap();
    explain(&mut store, "inst-x", applied).expect("diagnosis needs no writer");
}

#[test]
fn a_step_with_microsteps_explains_with_them_present() {
    // The eventless transition out of `scored` fires on its own once the
    // guard holds, and plan 0009 records that reaction as a microstep.
    let dir = scratch("microsteps");
    let mut store = Store::open(&dir).unwrap();
    let mut clock = FixedClock::new(1_000, 1);
    store
        .define_machine_on(&mut clock, value(CASE), false, false)
        .unwrap();
    store
        .create_instance_ctx_on(
            &mut clock,
            "explain_case",
            "inst-m",
            "create-m",
            None,
            &BTreeMap::new(),
            &[],
        )
        .unwrap();
    let seq = store
        .send_event(
            "inst-m",
            "score",
            value(r#"{"points":"500"}"#),
            "score-m",
            None,
        )
        .unwrap()
        .get("seq")
        .and_then(Value::as_num)
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap();
    assert_eq!(
        store.state.instances["inst-m"]
            .configuration
            .sequential_leaf(),
        Some("closed"),
        "the machine reacted to itself, which is what leaves a microstep"
    );
    let explained = explain(&mut store, "inst-m", seq).unwrap();
    assert!(
        format!("{explained:?}").contains("microsteps"),
        "the record carries microsteps and the explanation must not drop them"
    );
}
