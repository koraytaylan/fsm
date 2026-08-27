//! Eventless selection: the trigger's candidate scan with the `$always` key.
//!
//! Plan 0009 task 4303. The one asymmetry these tests exist to pin: for an
//! event, "candidates but every guard false" is `run/not_enabled`; for the
//! eventless scan it is quiescence, and so is "no candidates at all".

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MACROSTEP_EVAL_TICKS, MAX_MICROSTEPS};
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{Applied, DeadlineOutcome, Outcome, Rejection, create, poll_deadline, step};
use fsm_core::trace::MicrostepTrigger;
use fsm_core::tree::Tree;

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
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn empty() -> Value {
    Value::Obj(BTreeMap::new())
}

fn leaf_of(configuration: &ActiveConfiguration) -> &str {
    configuration.sequential_leaf().expect("sequential machine")
}

fn applied(outcome: Outcome) -> Applied {
    match outcome {
        Outcome::Applied(applied) => applied,
        other => panic!("expected Applied, got {other:?}"),
    }
}

fn rejected(outcome: Outcome) -> Rejection {
    match outcome {
        Outcome::Rejected(rejection) => rejection,
        other => panic!("expected Rejected, got {other:?}"),
    }
}

fn sequential(states: &str, transitions: &str, extra: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"m","states":{states},"initial":"a","context":[{{"name":"x","ty":"int","init":"0"}}],"events":[{{"name":"go","fields":[]}}],"transitions":{transitions}{extra}}}"#
    )
}

#[test]
fn a_guardless_eventless_exit_from_the_initial_state_runs_at_creation() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","to":"b"}]"#,
        "",
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert_eq!(leaf_of(&created.configuration_after), "b");
    assert_eq!(
        created.entered,
        ["a"],
        "the trigger is creation's own entry"
    );
    assert_eq!(created.trace.microsteps.len(), 1);
    let reaction = &created.trace.microsteps[0];
    assert_eq!(reaction.index, 1);
    assert_eq!(reaction.trigger, MicrostepTrigger::Eventless);
    assert_eq!(reaction.source_state, "a");
    assert_eq!(reaction.transition_idx, 0);
    assert_eq!(reaction.exited, ["a"]);
    assert_eq!(reaction.entered, ["b"]);
    assert_eq!(
        reaction.candidates.len(),
        1,
        "the scan that selected it is in the trace"
    );
}

#[test]
fn all_guards_false_is_quiescence_not_a_rejection() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","if":"ctx.x > 0","to":"b"},{"from":"a","if":"ctx.x > 1","to":"b"},{"from":"a","on":"go","do":[{"target":"x","value":"ctx.x + 1"}]}]"#,
        "",
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    assert_eq!(leaf_of(&created.configuration_after), "a");
    assert!(created.trace.microsteps.is_empty());
    // Once the guard holds, the next macrostep reacts.
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
    ));
    assert_eq!(leaf_of(&out.configuration_after), "b");
    assert_eq!(out.trace.microsteps.len(), 1);
    assert_eq!(out.trace.microsteps[0].transition_idx, 0);
}

#[test]
fn no_eventless_candidates_is_quiescence_not_unhandled() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"}]"#,
        r#","on_unhandled":"reject""#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
    ));
    assert_eq!(leaf_of(&out.configuration_after), "b");
    assert!(out.trace.microsteps.is_empty());
    assert!(out.trace.internal_unhandled.is_empty());
}

#[test]
fn an_eventless_guard_that_errors_rejects_the_whole_macrostep() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"b"},{"name":"c"}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.x + 9223372036854775807 + 1 > 0","to":"c"}]"#,
        "",
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = instance(&created);
    let before = state.clone();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = rejected(step(&m, &t, &state, "go", &empty(), 0, &mut budget));
    assert_eq!(rejection.code, "run/guard_error");
    assert_eq!(rejection.cause, Some("run/overflow"));
    assert_eq!(rejection.source_state.as_deref(), Some("b"));
    assert_eq!(rejection.transition_idx, Some(1));
    assert_eq!(
        rejection.trace.microsteps.len(),
        1,
        "the failing scan is kept"
    );
    assert_eq!(
        rejection.trace.microsteps[0].trigger,
        MicrostepTrigger::Eventless
    );
    assert!(
        rejection.trace.pipeline.iter().all(|b| b.discarded),
        "the trigger is discarded"
    );
    assert_eq!(state, before);
}

#[test]
fn innermost_eventless_transition_wins() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"p","initial":"child","states":[{"name":"child"},{"name":"inner_target"}]},{"name":"outer_target"}]"#,
        r#"[{"from":"a","on":"go","to":"p"},{"from":"p","to":"outer_target"},{"from":"child","to":"inner_target"}]"#,
        "",
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
    ));
    assert_eq!(out.trace.microsteps[0].source_state, "child");
    assert_eq!(out.trace.microsteps[0].transition_idx, 2);
    // From inner_target nothing is eventless, so the ancestor's transition
    // now wins the next scan: p → outer_target.
    assert_eq!(out.trace.microsteps[1].source_state, "p");
    assert_eq!(out.trace.microsteps[1].transition_idx, 1);
    assert_eq!(leaf_of(&out.configuration_after), "outer_target");
    let considered: Vec<u32> = out.trace.microsteps[0]
        .candidates
        .iter()
        .flat_map(|level| level.transitions.iter().map(|c| c.transition_idx))
        .collect();
    assert_eq!(
        considered,
        [2, 1],
        "child first, then the ancestor as not_considered"
    );
}

const FORK: &str = r#"{"format":"fsm.machine/1","name":"fork",
"regions":[
 {"name":"left","states":[{"name":"lp","initial":"l0","states":[{"name":"l0"},{"name":"l1"},{"name":"l_done","terminal":true}]},{"name":"l_reset"}],"initial":"lp"},
 {"name":"right","states":[{"name":"r0"},{"name":"r1"}],"initial":"r0"}
],
"context":[{"name":"armed","ty":"bool","init":"false"},{"name":"finished","ty":"bool","init":"false"}],
"events":[{"name":"arm","fields":[]},{"name":"finish","fields":[]}],
"transitions":[
 {"from":"l0","on":"arm","do":[{"target":"armed","value":"true"}]},
 {"from":"l0","if":"ctx.armed","to":"l1"},
 {"from":"r0","if":"ctx.armed","to":"r1"},
 {"from":"l1","on":"finish","to":"l_done","do":[{"target":"finished","value":"true"}]},
 {"from":"lp","if":"ctx.finished","to":"l_reset"}
]}"#;

#[test]
fn parallel_regions_take_one_eventless_transition_per_microstep_in_document_order() {
    let (m, t) = machine(FORK);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "arm",
        &empty(),
        0,
        &mut budget,
    ));
    let order: Vec<(&str, Option<&str>)> = out
        .trace
        .microsteps
        .iter()
        .map(|m| (m.source_state.as_str(), m.region.as_deref()))
        .collect();
    assert_eq!(order, [("l0", Some("left")), ("r0", Some("right"))]);
    assert_eq!(
        out.configuration_after,
        ActiveConfiguration::Parallel {
            leaves: BTreeMap::from([("left".into(), "l1".into()), ("right".into(), "r1".into())])
        }
    );
}

#[test]
fn a_region_on_a_terminal_leaf_is_skipped_by_the_eventless_scan() {
    let (m, t) = machine(FORK);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let armed = applied(step(
        &m,
        &t,
        &instance(&created),
        "arm",
        &empty(),
        0,
        &mut budget,
    ));
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let finished = applied(step(
        &m,
        &t,
        &instance(&armed),
        "finish",
        &empty(),
        0,
        &mut budget,
    ));
    // `lp`'s eventless transition is enabled (`ctx.finished` is true) and sits
    // on the finished region's chain, but that region's leaf is terminal, so
    // the scan never visits it: the join stays where it finished.
    assert_eq!(finished.status_after, Status::Running);
    assert!(finished.trace.microsteps.is_empty());
    assert_eq!(
        finished.configuration_after,
        ActiveConfiguration::Parallel {
            leaves: BTreeMap::from([
                ("left".into(), "l_done".into()),
                ("right".into(), "r1".into())
            ])
        }
    );
}

#[test]
fn a_chain_of_three_eventless_transitions_settles_in_one_macrostep() {
    let (m, t) = machine(
        r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"b"},{"name":"c"},{"name":"d"},{"name":"e"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"deadlines":[{"name":"in_b","from":"b","after":"dur(1, s)","to":"a"},{"name":"in_c","from":"c","after":"dur(2, s)","to":"a"},{"name":"in_e","from":"e","after":"dur(3, s)","to":"a"}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","to":"c"},{"from":"c","to":"d"},{"from":"d","to":"e"}]}"#,
    );
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        100,
        &mut budget,
    ));
    let indices: Vec<u32> = out.trace.microsteps.iter().map(|m| m.index).collect();
    assert_eq!(indices, [1, 2, 3]);
    assert_eq!(leaf_of(&out.configuration_after), "e");
    assert_eq!(
        out.deadlines_after,
        BTreeMap::from([("in_e".to_string(), 3100)]),
        "schedules of states entered and exited mid-macrostep net to nothing"
    );
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match poll_deadline(&m, &t, &instance(&out), 3100, &mut budget) {
        DeadlineOutcome::Applied(applied) => assert_eq!(applied.deadline.name, "in_e"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_internal_eventless_transition_keeps_the_configuration() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.x == 0","do":[{"target":"x","value":"1"}]}]"#,
        "",
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
    ));
    assert_eq!(
        out.trace.microsteps.len(),
        1,
        "the guard became false after one reaction"
    );
    assert!(out.trace.microsteps[0].exited.is_empty());
    assert!(out.trace.microsteps[0].entered.is_empty());
    assert_eq!(out.ctx_after["x"].canonical_string(), "1");
    assert_eq!(leaf_of(&out.configuration_after), "b");
}

#[test]
fn an_external_eventless_self_transition_re_enters_its_state() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"b","entry":{"do":[{"target":"x","value":"ctx.x + 1"}]}}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.x < 3","to":"b"}]"#,
        "",
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
    ));
    // Entry ran once for the trigger (x=1) and once per re-entry until the
    // guard failed (x=2, x=3): two reactions.
    assert_eq!(out.ctx_after["x"].canonical_string(), "3");
    assert_eq!(out.trace.microsteps.len(), 2);
    assert_eq!(out.trace.microsteps[0].exited, ["b"]);
    assert_eq!(out.trace.microsteps[0].entered, ["b"]);
}

#[test]
fn a_guarded_eventless_cycle_that_never_settles_is_run_microstep_limit() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.x >= 0","to":"b"}]"#,
        "",
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = instance(&created);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = rejected(step(&m, &t, &state, "go", &empty(), 0, &mut budget));
    assert_eq!(rejection.code, "run/microstep_limit");
    assert_eq!(rejection.trace.microsteps.len(), MAX_MICROSTEPS as usize);
    assert_eq!(rejection.source_state.as_deref(), Some("b"));
    assert_eq!(rejection.transition_idx, Some(1));
    assert!(
        rejection.hint.contains("transition 1"),
        "{}",
        rejection.hint
    );
    assert!(
        budget.remaining() > 0,
        "an accepted definition never exhausts the macrostep budget, even when it loops"
    );
}

#[test]
fn a_creation_that_cannot_settle_is_create_failed_with_the_limit_as_cause() {
    let (m, t) = machine(&sequential(
        r#"[{"name":"a"}]"#,
        r#"[{"from":"a","if":"ctx.x >= 0","to":"a"}]"#,
        "",
    ));
    let rejection = create(&m, &t, &BTreeMap::new(), 0).unwrap_err();
    assert_eq!(rejection.code, "run/create_failed");
    assert_eq!(rejection.cause, Some("run/microstep_limit"));
}
