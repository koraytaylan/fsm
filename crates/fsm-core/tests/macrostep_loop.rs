//! The macrostep driver, driven through scripted reaction selectors.
//!
//! Plan 0009 task 4201. No reactive definition shape exists yet, so these
//! tests substitute a `ReactionSelector` that scripts which transition each
//! reaction selects and pin the loop's own laws: order, the ceiling,
//! atomicity, effect numbering, deadline settlement, invariants once at
//! quiescence, and the compatibility anchor that a non-reactive machine has
//! no reaction microsteps at all.

use std::collections::{BTreeMap, VecDeque};

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MACROSTEP_EVAL_TICKS, MAX_EVAL_TICKS, MAX_MICROSTEPS};
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{
    Applied, DeadlineOutcome, InternalEvent, Outcome, ReactionSelection, ReactionSelector,
    Rejection, create, create_with, poll_deadline_with, step, step_with,
};
use fsm_core::trace::{CandidateTrace, DecisionTrace, GuardTrace, LevelTrace, MicrostepTrigger};
use fsm_core::tree::Tree;

const REACTOR: &str = r#"{"format":"fsm.machine/1","name":"reactor",
"context":[{"name":"x","ty":"int","init":"0"}],
"events":[{"name":"go","fields":[]},{"name":"next","fields":[]},{"name":"loop","fields":[]}],
"effects":[{"name":"fx","fields":[]}],
"states":[{"name":"a"},{"name":"b"},{"name":"c"},{"name":"d","terminal":true}],
"initial":"a",
"transitions":[
 {"from":"a","on":"go","to":"b","emit":[{"effect":"fx"}]},
 {"from":"b","on":"next","to":"c","do":[{"target":"x","value":"ctx.x + 1"}],"emit":[{"effect":"fx"}]},
 {"from":"c","on":"next","to":"d"},
 {"from":"b","on":"loop"}
]}"#;

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
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

fn canonical(value: &Value) -> String {
    String::from_utf8(fsm_core::canon::canon_bytes(value)).unwrap()
}

fn leaf_of(configuration: &ActiveConfiguration) -> &str {
    configuration.sequential_leaf().expect("sequential machine")
}

/// One scripted answer of the eventless seam.
#[derive(Clone)]
enum Scripted {
    Select {
        from: &'static str,
        transition_idx: usize,
    },
    Fail,
}

/// A selector that answers the eventless seam from a script, then quiesces,
/// and never handles an internal event.
struct Script {
    eventless: VecDeque<Scripted>,
    eventless_calls: usize,
    internal_calls: usize,
}

impl Script {
    fn new(steps: &[Scripted]) -> Self {
        Self {
            eventless: steps.iter().cloned().collect(),
            eventless_calls: 0,
            internal_calls: 0,
        }
    }

    fn repeat(from: &'static str, transition_idx: usize, times: usize) -> Self {
        Self::new(&vec![
            Scripted::Select {
                from,
                transition_idx
            };
            times
        ])
    }
}

/// A guard-error-shaped rejection: it names the transition whose guard
/// failed and carries the scan's candidates, as the engine's scan will.
fn scripted_rejection() -> Rejection {
    Rejection {
        code: "run/guard_error",
        message: "scripted failure".into(),
        hint: "the script asked for it".into(),
        source_state: Some("b".into()),
        transition_idx: Some(3),
        block: None,
        span: None,
        trace: DecisionTrace {
            candidates: vec![LevelTrace {
                source_state: "b".into(),
                transitions: vec![CandidateTrace {
                    transition_idx: 3,
                    guard: GuardTrace::NotConsidered,
                }],
            }],
            ..DecisionTrace::default()
        },
        cause: None,
    }
}

fn selection(
    tree: &Tree,
    working: &InstanceState,
    from: &str,
    transition_idx: usize,
) -> ReactionSelection {
    ReactionSelection {
        region: None,
        leaf: tree.id(leaf_of(&working.configuration)).unwrap(),
        source: tree.id(from).unwrap(),
        transition_idx,
        candidates: Vec::new(),
    }
}

impl ReactionSelector for Script {
    fn select_eventless(
        &mut self,
        _machine: &CompiledMachine,
        tree: &Tree,
        working: &InstanceState,
        _budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection> {
        self.eventless_calls += 1;
        match self.eventless.pop_front() {
            None => Ok(None),
            Some(Scripted::Fail) => Err(scripted_rejection()),
            Some(Scripted::Select {
                from,
                transition_idx,
            }) => Ok(Some(selection(tree, working, from, transition_idx))),
        }
    }

    fn select_internal(
        &mut self,
        _machine: &CompiledMachine,
        _tree: &Tree,
        _working: &InstanceState,
        _event: &InternalEvent,
        _budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection> {
        self.internal_calls += 1;
        Ok(None)
    }
}

/// A selector that selects the same transition forever.
struct Forever {
    from: &'static str,
    transition_idx: usize,
}

impl ReactionSelector for Forever {
    fn select_eventless(
        &mut self,
        _machine: &CompiledMachine,
        tree: &Tree,
        working: &InstanceState,
        _budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection> {
        Ok(Some(selection(
            tree,
            working,
            self.from,
            self.transition_idx,
        )))
    }

    fn select_internal(
        &mut self,
        _machine: &CompiledMachine,
        _tree: &Tree,
        _working: &InstanceState,
        _event: &InternalEvent,
        _budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection> {
        Ok(None)
    }
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

#[test]
fn a_macrostep_that_quiesces_at_once_has_no_reaction_microsteps() {
    let (m, t) = machine(REACTOR);
    let created = create_with(&m, &t, &BTreeMap::new(), 0, &mut Script::new(&[])).unwrap();
    assert!(created.trace.microsteps.is_empty());
    assert!(created.trace.internal_unhandled.is_empty());
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let stepped = applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::new(&[]),
    ));
    assert!(stepped.trace.microsteps.is_empty());
    assert_eq!(leaf_of(&stepped.configuration_after), "b");
    assert_eq!(stepped.entered, ["b"]);
    let mut plain_budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let plain = applied(step(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut plain_budget,
    ));
    assert_eq!(
        plain, stepped,
        "the engine selector and an empty script agree"
    );
    assert!(
        !canonical(&stepped.trace.to_value()).contains("microsteps"),
        "no key for a non-reactive macrostep"
    );
}

#[test]
fn a_seam_answering_n_times_yields_n_reaction_microsteps_indexed_from_one() {
    let (m, t) = machine(REACTOR);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut script = Script::new(&[
        Scripted::Select {
            from: "b",
            transition_idx: 1,
        },
        Scripted::Select {
            from: "c",
            transition_idx: 2,
        },
    ]);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut script,
    ));
    let indices: Vec<u32> = out.trace.microsteps.iter().map(|m| m.index).collect();
    assert_eq!(indices, [1, 2]);
    assert!(
        out.trace
            .microsteps
            .iter()
            .all(|m| m.trigger == MicrostepTrigger::Eventless)
    );
    assert_eq!(out.trace.microsteps[0].source_state, "b");
    assert_eq!(out.trace.microsteps[0].transition_idx, 1);
    assert_eq!(out.trace.microsteps[0].exited, ["b"]);
    assert_eq!(out.trace.microsteps[0].entered, ["c"]);
    assert_eq!(out.trace.microsteps[1].exited, ["c"]);
    assert_eq!(out.trace.microsteps[1].entered, ["d"]);
    // The record identity fields still describe the trigger, not the union.
    assert_eq!(out.source_state, "a");
    assert_eq!(out.transition_idx, 0);
    assert_eq!(out.exited, ["a"]);
    assert_eq!(out.entered, ["b"]);
    // The sealed state is the state after the whole macrostep.
    assert_eq!(leaf_of(&out.configuration_after), "d");
    assert_eq!(out.ctx_after["x"].canonical_string(), "1");
    assert_eq!(out.status_after, Status::Completed);
    let rendered = canonical(&out.trace.to_value());
    assert!(rendered.contains("\"microsteps\""));
    assert!(rendered.contains("\"trigger\":\"eventless\""));
}

#[test]
fn effects_number_continuously_across_microsteps() {
    let (m, t) = machine(REACTOR);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::repeat("b", 1, 1),
    ));
    let ks: Vec<u32> = out.effects.iter().map(|e| e.k).collect();
    assert_eq!(
        ks,
        [0, 1],
        "the trigger's emit is k=0, the reaction's is k=1"
    );
    assert_eq!(out.trace.microsteps[0].pipeline[0].emits[0].k, 1);
}

#[test]
fn exceeding_the_ceiling_rejects_the_whole_macrostep() {
    let (m, t) = machine(REACTOR);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = instance(&created);
    let before = state.clone();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = rejected(step_with(
        &m,
        &t,
        &state,
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Forever {
            from: "b",
            transition_idx: 3,
        },
    ));
    assert_eq!(rejection.code, "run/microstep_limit");
    assert_eq!(
        rejection.trace.microsteps.len(),
        MAX_MICROSTEPS as usize,
        "exactly the ceiling's worth of reactions ran before the refusal"
    );
    assert_eq!(rejection.source_state.as_deref(), Some("b"));
    assert_eq!(rejection.transition_idx, Some(3));
    assert!(
        rejection.hint.contains("transition 3") && rejection.hint.contains("state b"),
        "{}",
        rejection.hint
    );
    assert!(rejection.message.contains("64"), "{}", rejection.message);
    assert!(
        rejection
            .trace
            .microsteps
            .iter()
            .all(|m| m.pipeline.iter().all(|b| b.discarded))
    );
    assert!(rejection.trace.pipeline.iter().all(|b| b.discarded));
    assert_eq!(state, before);
}

#[test]
fn a_ceiling_of_exactly_max_microsteps_reactions_is_accepted() {
    let (m, t) = machine(REACTOR);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::repeat("b", 3, MAX_MICROSTEPS as usize),
    ));
    assert_eq!(out.trace.microsteps.len(), MAX_MICROSTEPS as usize);
    assert_eq!(out.trace.microsteps.last().unwrap().index, MAX_MICROSTEPS);
}

#[test]
fn a_failing_reaction_rejects_atomically_and_keeps_the_microsteps_that_ran() {
    let (m, t) = machine(REACTOR);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = instance(&created);
    let before = state.clone();
    let mut script = Script::new(&[
        Scripted::Select {
            from: "b",
            transition_idx: 3,
        },
        Scripted::Select {
            from: "b",
            transition_idx: 3,
        },
        Scripted::Fail,
    ]);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = rejected(step_with(
        &m,
        &t,
        &state,
        "go",
        &empty(),
        0,
        &mut budget,
        &mut script,
    ));
    assert_eq!(rejection.code, "run/guard_error");
    assert_eq!(rejection.message, "scripted failure");
    let indices: Vec<u32> = rejection.trace.microsteps.iter().map(|m| m.index).collect();
    assert_eq!(
        indices,
        [1, 2, 3],
        "the two microsteps that ran are kept, then the scan that failed"
    );
    let failed_scan = &rejection.trace.microsteps[2];
    assert_eq!(failed_scan.trigger, MicrostepTrigger::Eventless);
    assert_eq!(failed_scan.source_state, "b");
    assert_eq!(failed_scan.transition_idx, 3);
    assert_eq!(
        failed_scan.candidates.len(),
        1,
        "the failing scan's candidates survive"
    );
    assert!(
        failed_scan.pipeline.is_empty(),
        "nothing applied for a scan that failed"
    );
    assert!(
        rejection
            .trace
            .microsteps
            .iter()
            .all(|m| m.pipeline.iter().all(|b| b.discarded))
    );
    assert!(
        rejection.trace.pipeline.iter().all(|b| b.discarded),
        "the trigger's pipeline is discarded with the rest"
    );
    assert!(!rejection.trace.pipeline.is_empty());
    assert_eq!(state, before);
}

#[test]
fn a_reaction_whose_block_fails_is_kept_as_the_failing_microstep() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[{"name":"x","ty":"int","init":"9223372036854775807"}],
"events":[{"name":"go","fields":[]},{"name":"next","fields":[]}],
"states":[{"name":"a"},{"name":"b"},{"name":"c"}],
"initial":"a",
"transitions":[
 {"from":"a","on":"go","to":"b"},
 {"from":"b","on":"next","to":"c","do":[{"target":"x","value":"ctx.x + 1"}]}
]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = rejected(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::repeat("b", 1, 1),
    ));
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.cause, Some("run/overflow"));
    assert_eq!(rejection.block.as_deref(), Some("transition"));
    assert_eq!(rejection.trace.microsteps.len(), 1);
    let failed = &rejection.trace.microsteps[0];
    assert_eq!(failed.index, 1);
    assert_eq!(failed.source_state, "b");
    assert_eq!(failed.transition_idx, 1);
    assert_eq!(failed.pipeline.len(), 1);
    assert!(failed.pipeline[0].discarded);
    assert!(rejection.trace.pipeline.iter().all(|b| b.discarded));
}

#[test]
fn the_eventless_seam_is_consulted_before_the_queue_on_every_iteration() {
    let (m, t) = machine(REACTOR);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut script = Script::repeat("b", 3, 3);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut script,
    ));
    assert_eq!(
        script.eventless_calls, 4,
        "one scan per reaction plus the closing scan that proves quiescence"
    );
    assert_eq!(
        script.internal_calls, 0,
        "the queue seam is never consulted while the queue is empty"
    );
}

#[test]
fn the_macrostep_budget_is_one_standard_budget_per_iteration() {
    assert_eq!(MACROSTEP_EVAL_TICKS, MAX_EVAL_TICKS * (MAX_MICROSTEPS + 2));
    assert_eq!(MACROSTEP_EVAL_TICKS, 4096 * 66);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let span = fsm_core::expr::lexer::Span::new(0, 0);
    for _ in 0..(MAX_MICROSTEPS + 2) * MAX_EVAL_TICKS {
        budget.tick(span).expect("within the macrostep budget");
    }
    assert_eq!(budget.remaining(), 0);
    assert_eq!(budget.tick(span).unwrap_err().code, "internal/budget");
}

#[test]
fn instance_state_has_exactly_its_six_fields() {
    // A struct literal without `..` fails to compile the moment a field is
    // added, which is the point: the internal event queue must never be
    // persisted, or `fsm.state/2` would move.
    let state = InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential { leaf: "a".into() },
        ctx: BTreeMap::new(),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let InstanceState {
        status: _,
        configuration: _,
        ctx: _,
        history: _,
        deadlines: _,
        pending: _,
        invocations: _,
        signals: _,
    } = state;
}

#[test]
fn create_runs_the_driver() {
    let (m, t) = machine(REACTOR);
    // Creation enters `a`; the script then drives a → b as if `go` had an
    // eventless twin, so the first sealed state is already `b`.
    let out = create_with(&m, &t, &BTreeMap::new(), 0, &mut Script::repeat("a", 0, 1)).unwrap();
    assert_eq!(out.trace.microsteps.len(), 1);
    assert_eq!(leaf_of(&out.configuration_after), "b");
    assert_eq!(
        out.entered,
        ["a"],
        "the trigger's entry chain is creation's"
    );
    assert_eq!(out.effects.len(), 1);
}

#[test]
fn poll_deadline_runs_the_driver() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[],
"events":[{"name":"next","fields":[]}],
"states":[{"name":"a"},{"name":"b"},{"name":"c"}],
"initial":"a",
"deadlines":[{"name":"expire","from":"a","after":"dur(1, s)","to":"b"}],
"transitions":[{"from":"b","on":"next","to":"c"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = instance(&created);
    assert_eq!(state.deadlines.get("expire"), Some(&1000));
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match poll_deadline_with(
        &m,
        &t,
        &state,
        1000,
        &mut budget,
        &mut Script::repeat("b", 0, 1),
    ) {
        DeadlineOutcome::Applied(applied) => {
            assert_eq!(applied.deadline.name, "expire");
            assert_eq!(applied.transition.trace.microsteps.len(), 1);
            assert_eq!(leaf_of(&applied.transition.configuration_after), "c");
            assert_eq!(applied.transition.entered, ["b"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn invariants_are_evaluated_once_at_quiescence() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[{"name":"x","ty":"int","init":"0"}],
"events":[{"name":"go","fields":[]},{"name":"loop","fields":[]}],
"states":[{"name":"a"},{"name":"b"}],
"initial":"a",
"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"loop"}],
"invariants":[{"name":"pos","expr":"ctx.x >= 0","mode":"enforce"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::repeat("b", 1, 3),
    ));
    assert_eq!(out.trace.microsteps.len(), 3);
    assert_eq!(
        out.trace.invariants.len(),
        1,
        "one trace per declared invariant, not one per microstep"
    );
    // Budget accounting is the second witness: one implicit `true` for the
    // omitted `go` guard, then `ctx.x >= 0` (three nodes) exactly once. Four
    // ticks total, however many reactions the script drove.
    assert_eq!(MACROSTEP_EVAL_TICKS - budget.remaining(), 4);
}

#[test]
fn an_intermediate_configuration_may_violate_an_enforce_invariant() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[],
"events":[{"name":"go","fields":[]},{"name":"next","fields":[]}],
"states":[{"name":"a"},{"name":"b"},{"name":"c"}],
"initial":"a",
"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"next","to":"c"}],
"invariants":[{"name":"never_in_b","expr":"not in(b)","mode":"enforce"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let state = instance(&created);
    // Without a reaction the trigger lands in `b` and the invariant rejects.
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = rejected(step(&m, &t, &state, "go", &empty(), 0, &mut budget));
    assert_eq!(rejection.code, "run/invariant");
    // With a reaction that leaves `b` before quiescence, the macrostep applies.
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step_with(
        &m,
        &t,
        &state,
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::repeat("b", 1, 1),
    ));
    assert_eq!(leaf_of(&out.configuration_after), "c");
    assert!(out.trace.invariants[0].passed);
}

#[test]
fn an_invariant_failing_at_quiescence_rejects_with_the_trigger_identity() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[],
"events":[{"name":"go","fields":[]},{"name":"next","fields":[]}],
"states":[{"name":"a"},{"name":"b"},{"name":"c"}],
"initial":"a",
"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"next","to":"c"}],
"invariants":[{"name":"never_in_c","expr":"not in(c)","mode":"enforce"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = rejected(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::repeat("b", 1, 1),
    ));
    assert_eq!(rejection.code, "run/invariant");
    assert_eq!(rejection.source_state.as_deref(), Some("a"));
    assert_eq!(rejection.transition_idx, Some(0));
    assert_eq!(rejection.hint, "adjust the action or invariant never_in_c");
    assert_eq!(rejection.trace.microsteps.len(), 1);
    assert!(
        rejection.trace.microsteps[0]
            .pipeline
            .iter()
            .all(|b| b.discarded)
    );
    assert!(rejection.trace.pipeline.iter().all(|b| b.discarded));
    assert!(!rejection.trace.invariants[0].passed);
}

#[test]
fn a_monitor_flag_raised_by_the_final_configuration_is_reported_once() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[],
"events":[{"name":"go","fields":[]},{"name":"next","fields":[]}],
"states":[{"name":"a"},{"name":"b"},{"name":"c"}],
"initial":"a",
"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"next","to":"c"}],
"invariants":[{"name":"stay_in_a","expr":"in(a)","mode":"monitor"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        0,
        &mut budget,
        &mut Script::repeat("b", 1, 1),
    ));
    // The monitor fails in `b` (intermediate) and in `c` (final); it is
    // evaluated once, on `c`, so it is reported once.
    assert_eq!(out.monitor_flags, ["stay_in_a"]);
}

#[test]
fn a_deadline_entered_and_exited_within_one_macrostep_leaves_no_schedule() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[],
"events":[{"name":"go","fields":[]},{"name":"next","fields":[]}],
"states":[{"name":"a"},{"name":"b"},{"name":"c"}],
"initial":"a",
"deadlines":[
 {"name":"in_b","from":"b","after":"dur(1, s)","to":"a"},
 {"name":"in_c","from":"c","after":"dur(2, s)","to":"a"}
],
"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"next","to":"c"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let out = applied(step_with(
        &m,
        &t,
        &instance(&created),
        "go",
        &empty(),
        500,
        &mut budget,
        &mut Script::repeat("b", 1, 1),
    ));
    assert_eq!(
        out.deadlines_after,
        BTreeMap::from([("in_c".to_string(), 2500)]),
        "in_b was scheduled by the trigger and removed by the reaction; in_c is scheduled from the macrostep's single now_ms"
    );
    // The sealed state is coherent for the engine's own state gate.
    let mut next_budget = Budget::new(MACROSTEP_EVAL_TICKS);
    match poll_deadline_with(
        &m,
        &t,
        &instance(&out),
        2500,
        &mut next_budget,
        &mut Script::new(&[]),
    ) {
        DeadlineOutcome::Applied(applied) => assert_eq!(applied.deadline.name, "in_c"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn create_failure_inside_the_driver_is_still_run_create_failed() {
    let src = r#"{"format":"fsm.machine/1","name":"m",
"context":[{"name":"x","ty":"int","init":"0"}],
"events":[{"name":"next","fields":[]}],
"states":[{"name":"a"},{"name":"b"}],
"initial":"a",
"transitions":[{"from":"a","on":"next","to":"b","do":[{"target":"x","value":"-1"}]}],
"invariants":[{"name":"pos","expr":"ctx.x >= 0","mode":"enforce"}]}"#;
    let (m, t) = machine(src);
    let rejection =
        create_with(&m, &t, &BTreeMap::new(), 0, &mut Script::repeat("a", 0, 1)).unwrap_err();
    assert_eq!(rejection.code, "run/create_failed");
    assert_eq!(rejection.hint, "fix inits or invariant pos");
    assert_eq!(rejection.message, "invariant failed at create");
    assert_eq!(rejection.source_state, None);
    assert_eq!(rejection.transition_idx, None);
    assert_eq!(rejection.trace.microsteps.len(), 1);
}
