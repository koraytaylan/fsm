//! Running a case script: the three things a workflow does, on the production
//! stepper, with every engine bound inherited.
//!
//! Plan 0018 task 8402. Two properties carry most of the weight here and both
//! are engine rules that a case author will guess wrong: an **ack drives
//! nothing**, and **one poll applies at most one due deadline**. Each gets a
//! test that would fail loudly if the runner grew a convenience.

use std::collections::BTreeMap;

use fsm_core::cases::format::{AckOutcome, Case, Expect, Step, parse_cases};
use fsm_core::cases::run::{CaseError, StepOutcome, run_case};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, CompiledMachine};
use fsm_core::spec::compile_accepted;
use fsm_core::step::{DeadlineOutcome, Outcome};
use fsm_core::tree::Tree;

const CASE_REVIEW: &str = include_str!("fixtures/machines/case_review.json");
const GOLDEN_CASES: &str = include_str!("fixtures/cases_v1.json");

/// A machine with a deadline and an effect, since the committed example has no
/// deadline and `poll` is a third of this format.
const TIMED: &str = r#"{
  "format":"fsm.machine/1","name":"timed","initial":"waiting",
  "context":[{"name":"ticks","ty":"int","init":"0"}],
  "events":[{"name":"go","fields":[]},{"name":"stop","fields":[]}],
  "effects":[{"name":"ping","fields":[]},{"name":"pong","fields":[]}],
  "states":[
    {"name":"waiting","entry":{"emit":[{"effect":"ping","args":{}},{"effect":"pong","args":{}}]}},
    {"name":"first"},
    {"name":"second"},
    {"name":"done","terminal":true}
  ],
  "deadlines":[
    {"name":"early","from":"waiting","after":"dur(10, ms)","to":"first"},
    {"name":"late","from":"first","after":"dur(10, ms)","to":"second"}
  ],
  "transitions":[
    {"from":"second","on":"go","to":"done"},
    {"from":"waiting","on":"stop","to":"done"}
  ]
}"#;

/// Two orthogonal regions, so the configuration a run records is a set rather
/// than a leaf.
const PARALLEL: &str = r#"{
  "format":"fsm.machine/1","name":"twin",
  "regions":[
    {"name":"left","initial":"l0","states":[{"name":"l0"},{"name":"l1"}]},
    {"name":"right","initial":"r0","states":[{"name":"r0"},{"name":"r1"}]}
  ],
  "context":[],
  "events":[{"name":"a","fields":[]},{"name":"b","fields":[]}],
  "transitions":[{"from":"l0","on":"a","to":"l1"},{"from":"r0","on":"b","to":"r1"}]
}"#;

/// A machine whose reaction never settles. Admission accepts it — an unguarded
/// eventless cycle is refused outright, guarded ones are not — and the
/// macrostep ceiling is what stops it.
const NEVER_SETTLES: &str = r#"{
  "format":"fsm.machine/1","name":"spin","initial":"idle",
  "context":[{"name":"n","ty":"int","init":"0"}],
  "events":[{"name":"go","fields":[]}],
  "states":[{"name":"idle"},{"name":"here"},{"name":"there"}],
  "transitions":[
    {"from":"idle","on":"go","to":"here"},
    {"from":"here","if":"ctx.n >= 0","to":"there"},
    {"from":"there","if":"ctx.n >= 0","to":"here"}
  ]
}"#;

fn compiled(source: &str) -> (CompiledMachine, Tree) {
    let value = parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("the machine parses");
    let machine = compile_accepted(&value).expect("the machine compiles");
    let tree = Tree::for_machine(&machine.spec);
    (machine, tree)
}

fn case(name: &str, script: Vec<Step>) -> Case {
    Case {
        name: name.into(),
        context: BTreeMap::new(),
        script,
        expect: Expect::default(),
    }
}

fn send(event: &str) -> Step {
    Step::Send {
        event: event.into(),
        payload: Value::Obj(BTreeMap::new()),
    }
}

fn send_with(event: &str, field: &str, value: &str) -> Step {
    Step::Send {
        event: event.into(),
        payload: Value::Obj(BTreeMap::from([(field.into(), Value::Str(value.into()))])),
    }
}

fn ack(effect: &str) -> Step {
    Step::Ack {
        effect: effect.into(),
        outcome: AckOutcome::Ok,
        result: None,
    }
}

fn leaves(configuration: &ActiveConfiguration) -> Vec<String> {
    match configuration {
        ActiveConfiguration::Sequential { leaf } => vec![leaf.clone()],
        ActiveConfiguration::Parallel { leaves } => leaves.values().cloned().collect(),
    }
}

fn enabled_names(reports: &[fsm_core::analyze::EventReport]) -> Vec<String> {
    reports
        .iter()
        .filter(|report| report.status == fsm_core::analyze::EventStatus::Enabled)
        .map(|report| report.event.clone())
        .collect()
}

#[test]
fn a_send_poll_ack_script_drives_the_machine_to_the_expected_configuration() {
    let (machine, tree) = compiled(TIMED);
    let run = run_case(
        &machine,
        &tree,
        &case(
            "three",
            vec![ack("ping"), Step::Poll { now_ms: 10 }, send("stop")],
        ),
    )
    .expect("the case runs");
    assert_eq!(run.steps.len(), 3);
    assert!(matches!(run.steps[0].outcome, StepOutcome::Acked { .. }));
    assert!(matches!(
        run.steps[1].outcome,
        StepOutcome::Polled(DeadlineOutcome::Applied(_))
    ));
    // `stop` fires from `waiting`, and the poll has already left it, so this
    // last send is ignored rather than applied — which the run records rather
    // than hides.
    assert_eq!(leaves(&run.final_configuration), ["first"]);
    assert!(!run.terminal);
}

#[test]
fn one_poll_applies_at_most_one_due_deadline() {
    // The engine's rule, and the one a case author is most likely to guess
    // wrong: two deadlines that are both due need two polls. A runner that
    // drained the schedule would make every deadline case pass for the wrong
    // reason.
    let (machine, tree) = compiled(TIMED);
    let early = run_case(
        &machine,
        &tree,
        &case("early", vec![Step::Poll { now_ms: 9 }]),
    )
    .expect("the case runs");
    assert!(
        matches!(
            early.steps[0].outcome,
            StepOutcome::Polled(DeadlineOutcome::NotDue { .. })
        ),
        "a poll before the deadline applied something"
    );
    assert_eq!(leaves(&early.final_configuration), ["waiting"]);

    let once = run_case(
        &machine,
        &tree,
        &case("once", vec![Step::Poll { now_ms: 1_000 }]),
    )
    .expect("the case runs");
    assert_eq!(
        leaves(&once.final_configuration),
        ["first"],
        "one poll drained more than one schedule"
    );

    let twice = run_case(
        &machine,
        &tree,
        &case(
            "twice",
            vec![Step::Poll { now_ms: 1_000 }, Step::Poll { now_ms: 2_000 }],
        ),
    )
    .expect("the case runs");
    assert_eq!(leaves(&twice.final_configuration), ["second"]);
}

#[test]
fn an_ack_clears_the_effect_and_changes_nothing_else() {
    // An ack is exactly a removal from `pending`. This is the engine rule made
    // checkable: if the runner ever grows an `on_ok` follow-up, one of these
    // three equalities breaks.
    let (machine, tree) = compiled(TIMED);
    let run = run_case(
        &machine,
        &tree,
        &case("ack", vec![send("nothing_declared"), ack("ping")]),
    )
    .expect("the case runs");
    let before = &run.steps[0];
    let after = &run.steps[1];
    assert_eq!(before.pending, ["ping", "pong"]);
    assert_eq!(after.pending, ["pong"], "the ack did not clear its effect");
    assert_eq!(before.configuration, after.configuration);
    assert_eq!(before.ctx, after.ctx);
    assert_eq!(
        enabled_names(&before.enabled),
        enabled_names(&after.enabled),
        "an ack changed which events are enabled"
    );
    assert_eq!(before.terminal, after.terminal);
    assert!(after.emitted.is_empty(), "an ack emitted something");
}

#[test]
fn a_send_after_an_ack_proceeds() {
    let (machine, tree) = compiled(TIMED);
    let run = run_case(
        &machine,
        &tree,
        &case("after", vec![ack("ping"), ack("pong"), send("stop")]),
    )
    .expect("the case runs");
    assert!(run.final_pending.is_empty());
    assert_eq!(leaves(&run.final_configuration), ["done"]);
    assert!(run.terminal);
}

#[test]
fn acking_something_that_is_not_pending_fails_the_case_and_lists_what_was() {
    // The mistake an author actually makes — a misspelling, or acking twice —
    // and the list is the fix. A bare "unknown effect" costs a round trip.
    let (machine, tree) = compiled(TIMED);
    let run = run_case(
        &machine,
        &tree,
        &case("missing", vec![ack("ping"), ack("ping")]),
    )
    .expect("the case runs");
    let StepOutcome::Refused(refusal) = &run.steps[1].outcome else {
        panic!("acking a settled effect was accepted: {:?}", run.steps[1]);
    };
    assert!(refusal.message.contains("ping"), "{refusal:?}");
    assert_eq!(
        refusal.pending,
        ["pong"],
        "the refusal does not list what was pending"
    );
}

#[test]
fn a_rejected_send_is_recorded_and_the_script_continues() {
    // Stopping at the first failure would hide the other two, which is exactly
    // what an author correcting one expectation needs to see.
    let (machine, tree) = compiled(CASE_REVIEW);
    let run = run_case(
        &machine,
        &tree,
        &case(
            "continues",
            vec![send("scored"), send("docs_ok"), send("docs_ok")],
        ),
    )
    .expect("the case runs");
    assert_eq!(run.steps.len(), 3, "the script stopped early");
    let StepOutcome::Sent(first) = &run.steps[0].outcome else {
        panic!("not a send");
    };
    assert!(
        matches!(first, Outcome::Rejected(_)),
        "an unhandled event on a reject machine was not rejected: {first:?}"
    );
    assert_eq!(leaves(&run.final_configuration), ["risk_review"]);
}

#[test]
fn pending_and_enabled_are_recorded_after_every_step_not_only_at_the_end() {
    let (machine, tree) = compiled(CASE_REVIEW);
    let run = run_case(
        &machine,
        &tree,
        &case("per_step", vec![send("docs_ok"), send("docs_ok")]),
    )
    .expect("the case runs");
    // The example machine emits `notify` on entering `in_review`, and that
    // effect is pending from the first step onward.
    assert_eq!(run.steps[0].pending, ["notify"]);
    assert_eq!(run.steps[0].emitted.len(), 1);
    assert!(
        run.steps[1].emitted.is_empty(),
        "re-entering a child re-ran the parent's entry"
    );
    for step in &run.steps {
        assert!(
            !step.enabled.is_empty(),
            "step {} recorded no enabled-event scan",
            step.index
        );
    }
}

#[test]
fn a_run_is_deterministic() {
    let (machine, tree) = compiled(CASE_REVIEW);
    let scripted = case(
        "twice",
        vec![
            send("docs_ok"),
            send("docs_ok"),
            send_with("scored", "score", "700"),
        ],
    );
    let first = run_case(&machine, &tree, &scripted).expect("the case runs");
    let second = run_case(&machine, &tree, &scripted).expect("the case runs again");
    assert_eq!(first, second, "two runs of one case disagreed");
    assert!(first.terminal);
    assert_eq!(leaves(&first.final_configuration), ["approved"]);
}

#[test]
fn a_reaction_that_never_settles_reports_the_engines_own_error() {
    // Not a case-runner error: a case that trips an engine bound must report
    // what any caller would see, because a runner that relaxed a bound would
    // be testing a machine the engine will not run.
    let (machine, tree) = compiled(NEVER_SETTLES);
    let run = run_case(&machine, &tree, &case("spin", vec![send("go")])).expect("the case runs");
    let StepOutcome::Sent(Outcome::Rejected(rejection)) = &run.steps[0].outcome else {
        panic!("the ceiling did not stop it: {:?}", run.steps[0].outcome);
    };
    assert_eq!(rejection.code, "run/microstep_limit");
    // And the state is untouched: the whole macrostep was rejected.
    assert_eq!(leaves(&run.final_configuration), ["idle"]);
}

#[test]
fn a_parallel_machine_records_its_whole_configuration() {
    let (machine, tree) = compiled(PARALLEL);
    let run = run_case(&machine, &tree, &case("both", vec![send("a"), send("b")]))
        .expect("the case runs");
    let mut final_leaves = leaves(&run.final_configuration);
    final_leaves.sort();
    assert_eq!(final_leaves, ["l1", "r1"]);
    assert!(matches!(
        run.final_configuration,
        ActiveConfiguration::Parallel { .. }
    ));
}

#[test]
fn a_context_override_is_coerced_the_way_every_other_caller_coerces_one() {
    let (machine, tree) = compiled(CASE_REVIEW);
    let mut scripted = case("ctx", vec![]);
    scripted.context = BTreeMap::from([("score".into(), "42".into())]);
    let run = run_case(&machine, &tree, &scripted).expect("the case runs");
    assert_eq!(
        run.final_ctx.get("score"),
        Some(&fsm_core::expr::eval::Val::Int(42))
    );

    // An undeclared slot names itself and lists what the machine declares.
    let mut unknown = case("ctx", vec![]);
    unknown.context = BTreeMap::from([("nope".into(), "1".into())]);
    let CaseError::Context { key, message } =
        run_case(&machine, &tree, &unknown).expect_err("an undeclared slot is refused")
    else {
        panic!("wrong error");
    };
    assert_eq!(key, "nope");
    assert!(message.contains("score"), "{message}");

    // And a value of the wrong type is refused rather than coerced.
    let mut mistyped = case("ctx", vec![]);
    mistyped.context = BTreeMap::from([("score".into(), "not-an-int".into())]);
    assert!(matches!(
        run_case(&machine, &tree, &mistyped),
        Err(CaseError::Context { .. })
    ));
}

#[test]
fn the_committed_golden_case_file_runs_against_the_committed_machine() {
    // The format and the runner meet: every case in the golden parses and
    // runs, so neither can drift from the other unnoticed.
    let (machine, tree) = compiled(CASE_REVIEW);
    let file = parse_cases(GOLDEN_CASES.as_bytes()).expect("the golden parses");
    for scripted in &file.cases {
        let run = run_case(&machine, &tree, scripted)
            .unwrap_or_else(|error| panic!("{} did not run: {error:?}", scripted.name));
        assert_eq!(run.steps.len(), scripted.script.len());
    }
}
