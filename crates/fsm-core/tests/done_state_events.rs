//! `$done.state.<compound>`: the signal a compound has never been able to
//! send — my inner workflow finished, including my final state's entry
//! actions, and something outside me may now act on that.
//!
//! Plan 0009 task 4502.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{CompiledMachine, InstanceState, Status};
use fsm_core::spec::{Finding, compile, generated_event_names, parse_machine, validate};
use fsm_core::step::{Applied, Outcome, create, step};
use fsm_core::trace::MicrostepTrigger;
use fsm_core::tree::Tree;

fn parsed(src: &str) -> fsm_core::spec::MachineSpec {
    parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap_or_else(|e| panic!("{e:?}"))
}

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let m = compile(parsed(src)).unwrap_or_else(|e| panic!("{e:?}"));
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn findings(src: &str) -> Vec<Finding> {
    validate(&parsed(src)).err().unwrap_or_default()
}

fn instance(applied: &Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after.clone(),
        ctx: applied.ctx_after.clone(),
        history: applied.history_after.clone(),
        deadlines: applied.deadlines_after.clone(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn applied(outcome: Outcome) -> Applied {
    match outcome {
        Outcome::Applied(applied) => applied,
        other => panic!("expected Applied, got {other:?}"),
    }
}

fn triggers(out: &Applied) -> Vec<String> {
    out.trace
        .microsteps
        .iter()
        .map(|m| match &m.trigger {
            MicrostepTrigger::Eventless => "eventless".into(),
            MicrostepTrigger::Internal(event) => event.clone(),
        })
        .collect()
}

/// `review` has a final child `approved` whose entry records the decision;
/// `settled` is where the join lands.
fn definition(transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":[{{"name":"review","initial":"pending","states":[{{"name":"pending"}},{{"name":"approved","final":true,"entry":{{"do":[{{"target":"decided","value":"true"}}]}}}}]}},{{"name":"settled"}},{{"name":"closed","terminal":true}}],"initial":"review","context":[{{"name":"decided","ty":"bool","init":"false"}}],"events":[{{"name":"approve","fields":[]}},{{"name":"close","fields":[]}}],"transitions":{transitions}}}"#
    )
}

fn approve(m: &CompiledMachine, t: &Tree) -> Outcome {
    let created = create(m, t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    step(
        m,
        t,
        &instance(&created),
        "approve",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    )
}

#[test]
fn entering_a_final_child_raises_the_parent_done_event_in_the_same_macrostep() {
    let (m, t) = machine(&definition(
        r#"[{"from":"pending","on":"approve","to":"approved"},{"from":"review","on":"$done.state.review","to":"settled"}]"#,
    ));
    let out = applied(approve(&m, &t));
    assert_eq!(triggers(&out), ["$done.state.review"]);
    assert_eq!(out.trace.microsteps[0].source_state, "review");
    assert_eq!(out.trace.microsteps[0].exited, ["approved", "review"]);
    assert_eq!(out.trace.microsteps[0].entered, ["settled"]);
    assert_eq!(out.configuration_after.sequential_leaf(), Some("settled"));
    // One record's worth: the trigger's identity plus one reaction.
    assert_eq!(out.entered, ["approved"]);
}

#[test]
fn the_done_event_is_enqueued_after_the_final_state_entry_actions() {
    let (m, t) = machine(&definition(
        r#"[{"from":"pending","on":"approve","to":"approved"},{"from":"review","on":"$done.state.review","if":"ctx.decided","to":"settled"}]"#,
    ));
    let out = applied(approve(&m, &t));
    assert_eq!(
        out.configuration_after.sequential_leaf(),
        Some("settled"),
        "the guard saw the entry block's write"
    );
}

#[test]
fn a_finished_compound_does_not_complete_the_instance() {
    let (m, t) = machine(&definition(
        r#"[{"from":"pending","on":"approve","to":"approved"},{"from":"review","on":"$done.state.review","to":"settled"},{"from":"settled","on":"close","to":"closed"}]"#,
    ));
    let out = applied(approve(&m, &t));
    assert_eq!(out.status_after, Status::Running, "final is not terminal");
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let closed = applied(step(
        &m,
        &t,
        &instance(&out),
        "close",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ));
    assert_eq!(closed.status_after, Status::Completed);
}

#[test]
fn nesting_raises_only_the_immediate_parent_unless_it_too_finishes() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"outer","initial":"inner","states":[{"name":"inner","initial":"work","states":[{"name":"work"},{"name":"inner_done","final":true}]},{"name":"outer_done","final":true}]},{"name":"after_inner"},{"name":"after_outer"}],"initial":"outer","context":[],"events":[{"name":"finish","fields":[]}],"transitions":[{"from":"work","on":"finish","to":"inner_done"},{"from":"inner","on":"$done.state.inner","to":"outer_done"},{"from":"outer","on":"$done.state.outer","to":"after_outer"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "finish",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ));
    assert_eq!(triggers(&out), ["$done.state.inner", "$done.state.outer"]);
    assert_eq!(
        out.configuration_after.sequential_leaf(),
        Some("after_outer")
    );
    // The grandparent's event fires only because its own final child was
    // entered by the handler of the inner one; nothing raised it directly.
    let only_inner = src.replace(
        r#",{"from":"inner","on":"$done.state.inner","to":"outer_done"}"#,
        "",
    );
    let (m, t) = machine(&only_inner);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "finish",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ));
    assert!(out.trace.microsteps.is_empty());
    assert!(
        out.trace.internal_unhandled.is_empty(),
        "inner's event has no handler and is not raised; outer never finished"
    );
    assert_eq!(
        out.configuration_after.sequential_leaf(),
        Some("inner_done")
    );
}

#[test]
fn an_unknown_done_name_lists_the_real_generated_names() {
    let found = findings(&definition(
        r#"[{"from":"review","on":"$done.state.nosuch","to":"settled"}]"#,
    ));
    let finding = found
        .iter()
        .find(|f| f.code == "def/unknown_event")
        .expect("def/unknown_event");
    assert_eq!(finding.path, "/transitions/0/on");
    assert!(
        finding.hint.contains("$done.state.review"),
        "{}",
        finding.hint
    );
    // A compound with no final child generates nothing.
    let no_final = definition(r#"[{"from":"settled","on":"$done.state.review","to":"closed"}]"#)
        .replace(r#""final":true,"#, "");
    let found = findings(&no_final);
    let finding = found
        .iter()
        .find(|f| f.code == "def/unknown_event")
        .expect("def/unknown_event");
    assert!(
        finding.hint.contains("generates no done events"),
        "{}",
        finding.hint
    );
    assert!(generated_event_names(&parsed(&no_final)).is_empty());
    assert_eq!(
        generated_event_names(&parsed(&definition("[]"))),
        ["$done.state.review"]
    );
}

#[test]
fn a_done_event_nobody_handles_is_never_raised() {
    // The compound finishes, but no transition names `$done.state.review`,
    // so nothing is raised: the macrostep applies with no reaction, no
    // discard in the trace, and nothing counted toward the ceiling. A
    // definition that never names a generated event sees nothing of it.
    let (m, t) = machine(&definition(
        r#"[{"from":"pending","on":"approve","to":"approved"}]"#,
    ));
    let out = applied(approve(&m, &t));
    assert!(out.trace.microsteps.is_empty());
    assert!(out.trace.internal_unhandled.is_empty());
    assert_eq!(out.configuration_after.sequential_leaf(), Some("approved"));
    assert!(
        !String::from_utf8(fsm_core::canon::canon_bytes(&out.trace.to_value()))
            .unwrap()
            .contains("internal_unhandled")
    );
}

#[test]
fn a_done_event_cannot_be_sent_from_outside() {
    let (m, t) = machine(&definition(
        r#"[{"from":"pending","on":"approve","to":"approved"},{"from":"review","on":"$done.state.review","to":"settled"}]"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match step(
        &m,
        &t,
        &instance(&created),
        "$done.state.review",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) {
        Outcome::Rejected(r) => assert_eq!(r.code, "req/event_internal"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_done_handler_sees_an_empty_evt() {
    // A guard that names a field of a fieldless generated event is refused at
    // admission, exactly as for a declared fieldless event.
    let errs = compile(parsed(&definition(
        r#"[{"from":"pending","on":"approve","to":"approved"},{"from":"review","on":"$done.state.review","if":"evt.x == 1","to":"settled"}]"#,
    )))
    .unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "expr/unknown_field"),
        "{errs:?}"
    );
}

#[test]
fn creation_that_lands_in_a_final_state_reacts_before_the_first_sealed_state() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"p","initial":"work","states":[{"name":"work"},{"name":"done","final":true}]},{"name":"out"}],"initial":"p","context":[],"events":[],"transitions":[{"from":"work","to":"done"},{"from":"p","on":"$done.state.p","to":"out"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert_eq!(triggers(&created), ["eventless", "$done.state.p"]);
    assert_eq!(created.configuration_after.sequential_leaf(), Some("out"));
}
