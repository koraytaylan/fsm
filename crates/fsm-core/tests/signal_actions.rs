//! `signal`: one event to exactly one other instance, named by an expression
//! evaluated when the block runs.
//!
//! Plan 0010 task 5001. Delivery — and every check that needs the target's
//! declarations — is task 5002's; this pins the shape, the target's type, the
//! outbox, and the deliberate absence of compile-time payload checking.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::state_hash;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MACROSTEP_EVAL_TICKS, MAX_SIGNALS_PER_BLOCK};
use fsm_core::machine::{CompiledMachine, InstanceState, PendingSignal};
use fsm_core::spec::{Finding, compile, parse_machine, validate};
use fsm_core::step::{Applied, Outcome, create, step};
use fsm_core::tree::Tree;

fn findings(src: &str) -> Vec<Finding> {
    match parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()) {
        Ok(spec) => match validate(&spec) {
            Ok(()) => compile(spec).err().unwrap_or_default(),
            Err(found) => found,
        },
        Err(found) => found,
    }
}

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap_or_else(|e| panic!("{e:?}"));
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn instance(applied: &Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after.clone(),
        ctx: applied.ctx_after.clone(),
        history: applied.history_after.clone(),
        deadlines: applied.deadlines_after.clone(),
        pending: Vec::new(),
        invocations: applied.invocations_after.clone(),
        signals: BTreeMap::new(),
    }
}

fn send(m: &CompiledMachine, t: &Tree, state: &InstanceState, event: &str) -> Outcome {
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    step(
        m,
        t,
        state,
        event,
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    )
}

fn applied(outcome: Outcome) -> Applied {
    match outcome {
        Outcome::Applied(applied) => applied,
        other => panic!("{other:?}"),
    }
}

/// A machine whose `working` entry block signals the counterparty it learned
/// about at run time.
fn sender(entry: &str, context: &str, transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"sender","states":[{{"name":"idle"}},{{"name":"working"{entry}}}],"initial":"idle","context":[{context}],"events":[{{"name":"go","fields":[]}}],"effects":[{{"name":"fx","fields":[]}}],"transitions":[{{"from":"idle","on":"go","to":"working"}}{transitions}]}}"#
    )
}

const COUNTERPARTY: &str = r#"{"name":"counterparty","ty":"str","init":"inst-other"},{"name":"batch","ty":"str","init":"b7"}"#;

#[test]
fn a_signal_lands_in_the_senders_outbox_with_its_evaluated_target() {
    let src = sender(
        r#","entry":{"signal":[{"to":"ctx.counterparty","event":"batch_ready","with":{"batch":"ctx.batch"}}]}"#,
        COUNTERPARTY,
        "",
    );
    assert!(findings(&src).is_empty(), "{:?}", findings(&src));
    let (m, t) = machine(&src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "go"));
    assert_eq!(out.signals.len(), 1);
    let (k, signal) = &out.signals[0];
    assert_eq!(*k, 0);
    assert_eq!(signal.target_instance_id, "inst-other");
    assert_eq!(signal.event, "batch_ready");
    assert_eq!(signal.payload["batch"], Val::Str("b7".into()));
    // And the trace shows it beside the block that emitted it.
    let rendered = String::from_utf8(canon_bytes(&out.trace.to_value())).unwrap();
    assert!(
        rendered.contains("\"signals\"") && rendered.contains("batch_ready"),
        "{rendered}"
    );
}

#[test]
fn the_target_must_be_a_str() {
    let bad = sender(
        r#","entry":{"signal":[{"to":"ctx.count","event":"batch_ready"}]}"#,
        r#"{"name":"count","ty":"int","init":"1"}"#,
        "",
    );
    let found = findings(&bad);
    let finding = found
        .iter()
        .find(|f| f.code == "def/assign_type")
        .unwrap_or_else(|| panic!("{found:?}"));
    assert!(finding.hint.contains("str"), "{}", finding.hint);
    let good = sender(
        r#","entry":{"signal":[{"to":"ctx.counterparty","event":"batch_ready"}]}"#,
        COUNTERPARTY,
        "",
    );
    assert!(findings(&good).is_empty(), "{:?}", findings(&good));
}

#[test]
fn the_payload_is_not_typed_at_admission() {
    // The target machine is a run-time value, so its declarations are not
    // here to check against. Declaring the target statically was the
    // alternative and it defeats the purpose of a signal; the check belongs
    // to delivery, and this task must not double-report it.
    let src = sender(
        r#","entry":{"signal":[{"to":"ctx.counterparty","event":"an_event_this_machine_never_declares","with":{"anything":"ctx.batch"}}]}"#,
        COUNTERPARTY,
        "",
    );
    assert!(findings(&src).is_empty(), "{:?}", findings(&src));
}

#[test]
fn at_most_four_signals_in_one_block() {
    let one = r#"{"to":"ctx.counterparty","event":"e"}"#;
    for (count, refused) in [(4, false), (5, true)] {
        let signals: Vec<&str> = std::iter::repeat_n(one, count).collect();
        let src = sender(
            &format!(r#","entry":{{"signal":[{}]}}"#, signals.join(",")),
            COUNTERPARTY,
            "",
        );
        let found = findings(&src);
        assert_eq!(
            found.iter().any(|f| f.code == "def/limit_signals"),
            refused,
            "{count}: {found:?}"
        );
    }
    assert_eq!(MAX_SIGNALS_PER_BLOCK, 4);
}

#[test]
fn a_signal_reads_the_pre_block_snapshot() {
    // The transition block sets `counterparty`; the entry block's signal
    // reads the value that block left, and the transition's own signal reads
    // what was there before it ran.
    let src = sender(
        r#","entry":{"signal":[{"to":"ctx.counterparty","event":"after"}]}"#,
        COUNTERPARTY,
        r#",{"from":"working","on":"go","do":[{"target":"counterparty","value":"\"inst-late\""}],"signal":[{"to":"ctx.counterparty","event":"during"}]}"#,
    );
    let (m, t) = machine(&src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let entered = applied(send(&m, &t, &instance(&created), "go"));
    assert_eq!(entered.signals[0].1.target_instance_id, "inst-other");
    let again = applied(send(&m, &t, &instance(&entered), "go"));
    assert_eq!(
        again.signals[0].1.target_instance_id, "inst-other",
        "the transition's own signal read the context the previous block left"
    );
    assert_eq!(
        again.ctx_after["counterparty"],
        Val::Str("inst-late".into())
    );
}

#[test]
fn a_rejected_macrostep_enqueues_nothing() {
    // The block computes its signal and then the invariant refuses at
    // quiescence: nothing is enqueued, and the computed values are still in
    // the rejection's trace.
    let src = format!(
        r#"{{"format":"fsm.machine/1","name":"sender","states":[{{"name":"idle"}},{{"name":"working","entry":{{"do":[{{"target":"n","value":"9"}}],"signal":[{{"to":"ctx.counterparty","event":"batch_ready"}}]}}}}],"initial":"idle","context":[{COUNTERPARTY},{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"invariants":[{{"name":"small","expr":"ctx.n < 5","mode":"enforce"}}],"transitions":[{{"from":"idle","on":"go","to":"working"}}]}}"#
    );
    let (m, t) = machine(&src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let before = instance(&created);
    let Outcome::Rejected(rejection) = send(&m, &t, &before, "go") else {
        panic!("the invariant refuses");
    };
    assert_eq!(rejection.code, "run/invariant");
    let rendered = String::from_utf8(canon_bytes(&rejection.trace.to_value())).unwrap();
    assert!(
        rendered.contains("batch_ready"),
        "the computed signal is in the trace"
    );
    assert!(before.signals.is_empty(), "and nothing was enqueued");
}

#[test]
fn signal_numbering_is_independent_of_effect_numbering() {
    let src = sender(
        r#","entry":{"emit":[{"effect":"fx"}],"signal":[{"to":"ctx.counterparty","event":"one"},{"to":"ctx.counterparty","event":"two"}]}"#,
        COUNTERPARTY,
        "",
    );
    let (m, t) = machine(&src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "go"));
    assert_eq!(out.effects.len(), 1);
    assert_eq!(out.effects[0].k, 0);
    let indices: Vec<u32> = out.signals.iter().map(|(k, _)| *k).collect();
    assert_eq!(indices, [0, 1], "signals run in their own sequence");
}

#[test]
fn a_pending_signal_is_in_the_state_hash_and_the_format_did_not_move() {
    let src = sender("", COUNTERPARTY, "");
    let (m, t) = machine(&src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let empty = instance(&created);
    let mut with_signal = empty.clone();
    with_signal.signals.insert(
        "inst-1/3/0".into(),
        PendingSignal {
            target_instance_id: "inst-other".into(),
            event: "batch_ready".into(),
            payload: BTreeMap::new(),
        },
    );
    assert_ne!(
        state_hash(&m.machine_id, "inst-1", 3, &empty),
        state_hash(&m.machine_id, "inst-1", 3, &with_signal)
    );
    // The format string and the empty-state payload are exactly what task
    // 4802 committed: this task populates a field, it does not move a format.
    assert_eq!(fsm_core::hashes::STATE_FORMAT, "fsm.state/3");
    let machine_id = format!("m@sha256:{}", "9f2c".repeat(16));
    let mut bare = empty.clone();
    bare.ctx = BTreeMap::from([("total".to_string(), Val::Int(6))]);
    bare.configuration = fsm_core::machine::ActiveConfiguration::Sequential {
        leaf: "await_review".into(),
    };
    assert_eq!(
        fsm_core::hashes::state_hash_v3(&machine_id, "inst-1", 4, &bare),
        "sha256:b3cffbbcc3c2f39ecae18016bbbc6162a3cd8c88e9a809ddace40f95b9947d43",
        "the v3 payload is the one 4802 committed"
    );
}

#[test]
fn a_machine_without_signal_keeps_its_bytes() {
    let src = sender("", COUNTERPARTY, "");
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let bytes = String::from_utf8(canon_bytes(&spec.to_value())).unwrap();
    assert!(!bytes.contains("signal"));
}
