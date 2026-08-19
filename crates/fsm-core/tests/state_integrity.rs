//! Public and decoded instance state cannot violate engine invariants.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{CompiledMachine, InstanceState, Status};
use fsm_core::spec::compile_accepted;
use fsm_core::step::{DeadlineOutcome, Outcome, create, poll_deadline, step};
use fsm_core::tree::Tree;

const DEEP_SEQUENTIAL: &[u8] = br#"{
  "format":"fsm.machine/1",
  "name":"deep_history_boundary",
  "states":[
    {"name":"idle"},
    {"name":"work","initial":"phase","states":[
      {"name":"work_history","history":"deep"},
      {"name":"phase","initial":"one","states":[{"name":"one"},{"name":"two"}]}
    ]}
  ],
  "initial":"idle",
  "context":[],
  "events":[{"name":"resume","fields":[]}],
  "transitions":[{"from":"idle","on":"resume","to":"work_history"}]
}"#;

const SHALLOW_SEQUENTIAL: &[u8] = br#"{
  "format":"fsm.machine/1",
  "name":"shallow_history_boundary",
  "states":[
    {"name":"idle"},
    {"name":"work","initial":"phase","states":[
      {"name":"work_history","history":"shallow"},
      {"name":"phase","initial":"one","states":[{"name":"one"},{"name":"two"}]},
      {"name":"other"}
    ]}
  ],
  "initial":"idle",
  "context":[],
  "events":[{"name":"resume","fields":[]}],
  "transitions":[{"from":"idle","on":"resume","to":"work_history"}]
}"#;

const DEEP_PARALLEL: &[u8] = br#"{
  "format":"fsm.machine/1",
  "name":"parallel_history_boundary",
  "regions":[
    {"name":"work","initial":"idle","states":[
      {"name":"idle"},
      {"name":"job","initial":"doing","states":[
        {"name":"job_history","history":"deep"},
        {"name":"doing"}
      ]}
    ]},
    {"name":"audit","initial":"checking","states":[{"name":"checking"}]}
  ],
  "context":[],
  "events":[{"name":"resume","fields":[]}],
  "transitions":[{"from":"idle","on":"resume","to":"job_history"}]
}"#;

const DEADLINE_MACHINE: &[u8] = br#"{
  "format":"fsm.machine/1",
  "name":"deadline_state_boundary",
  "states":[{"name":"a"},{"name":"b"},{"name":"done","terminal":true}],
  "initial":"a",
  "context":[],
  "events":[{"name":"go","fields":[]},{"name":"finish","fields":[]}],
  "transitions":[
    {"from":"a","on":"go","to":"b"},
    {"from":"b","on":"finish","to":"done"}
  ],
  "deadlines":[{"name":"a_timeout","from":"a","after":"dur(5, ms)","to":"b"}]
}"#;

fn machine(source: &[u8]) -> (CompiledMachine, Tree) {
    let value = parse(source, &JsonLimits::DEFAULT).expect("test definition parses");
    let machine = compile_accepted(&value).expect("test definition compiles");
    let tree = Tree::for_machine(&machine.spec);
    (machine, tree)
}

fn state_from(applied: fsm_core::step::Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after,
        ctx: applied.ctx_after,
        history: applied.history_after,
        deadlines: applied.deadlines_after,
        pending: Vec::new(),
    }
}

fn initial(machine: &CompiledMachine, tree: &Tree, now_ms: i64) -> InstanceState {
    state_from(create(machine, tree, &BTreeMap::new(), now_ms).expect("instance creates"))
}

fn empty_payload() -> Value {
    Value::Obj(BTreeMap::new())
}

fn apply(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    event: &str,
) -> InstanceState {
    let mut budget = Budget::new(4096);
    match step(
        machine,
        tree,
        state,
        event,
        &empty_payload(),
        0,
        &mut budget,
    ) {
        Outcome::Applied(applied) => state_from(applied),
        other => panic!("expected applied {event}, got {other:?}"),
    }
}

fn assert_step_state_invalid(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    event: &str,
) {
    let mut budget = Budget::new(4096);
    match step(
        machine,
        tree,
        state,
        event,
        &empty_payload(),
        0,
        &mut budget,
    ) {
        Outcome::Rejected(rejection) => assert_eq!(rejection.code, "run/configuration_invalid"),
        other => panic!("malformed state must be rejected, got {other:?}"),
    }
}

#[test]
fn malformed_deep_and_shallow_history_cannot_produce_an_applied_state() {
    let (deep_machine, deep_tree) = machine(DEEP_SEQUENTIAL);
    let mut deep = initial(&deep_machine, &deep_tree, 0);

    deep.history.insert("work".into(), "phase".into());
    let error = deep_tree
        .validate_instance_state(&deep_machine, &deep)
        .expect_err("deep history must bind a leaf");
    assert!(error.detail().contains("must name a leaf"));
    assert_step_state_invalid(&deep_machine, &deep_tree, &deep, "resume");

    deep.history.insert("work".into(), "idle".into());
    let error = deep_tree
        .validate_instance_state(&deep_machine, &deep)
        .expect_err("deep history must stay under its owner");
    assert!(error.detail().contains("not a descendant"));

    deep.history.clear();
    deep.history.insert("idle".into(), "idle".into());
    let error = deep_tree
        .validate_instance_state(&deep_machine, &deep)
        .expect_err("a leaf cannot own history");
    assert!(error.detail().contains("is not a compound state"));

    deep.history.clear();
    deep.history.insert("phase".into(), "one".into());
    let error = deep_tree
        .validate_instance_state(&deep_machine, &deep)
        .expect_err("a compound without a pseudostate cannot own history");
    assert!(error.detail().contains("has no history pseudostate"));

    deep.history.clear();
    deep.history.insert("missing".into(), "one".into());
    let error = deep_tree
        .validate_instance_state(&deep_machine, &deep)
        .expect_err("history owner must exist");
    assert!(error.detail().contains("owner missing is unknown"));

    deep.history.clear();
    deep.history.insert("work".into(), "missing".into());
    let error = deep_tree
        .validate_instance_state(&deep_machine, &deep)
        .expect_err("history binding must exist");
    assert!(error.detail().contains("names an unknown state"));

    deep.history.clear();
    deep.history.insert("work".into(), "one".into());
    let resumed = apply(&deep_machine, &deep_tree, &deep, "resume");
    assert_eq!(resumed.configuration.sequential_leaf(), Some("one"));
    deep_tree
        .validate_instance_state(&deep_machine, &resumed)
        .expect("valid deep history produces a valid state");

    let (shallow_machine, shallow_tree) = machine(SHALLOW_SEQUENTIAL);
    let mut shallow = initial(&shallow_machine, &shallow_tree, 0);
    shallow.history.insert("work".into(), "one".into());
    let error = shallow_tree
        .validate_instance_state(&shallow_machine, &shallow)
        .expect_err("shallow history must bind a direct child");
    assert!(error.detail().contains("must name a direct child"));
    assert_step_state_invalid(&shallow_machine, &shallow_tree, &shallow, "resume");

    shallow.history.insert("work".into(), "work_history".into());
    let error = shallow_tree
        .validate_instance_state(&shallow_machine, &shallow)
        .expect_err("shallow history cannot bind its pseudostate");
    assert!(error.detail().contains("must name a real child"));

    shallow.history.insert("work".into(), "phase".into());
    let resumed = apply(&shallow_machine, &shallow_tree, &shallow, "resume");
    assert_eq!(resumed.configuration.sequential_leaf(), Some("one"));
    shallow_tree
        .validate_instance_state(&shallow_machine, &resumed)
        .expect("valid shallow history produces a valid state");
}

#[test]
fn parallel_history_cannot_bind_a_leaf_from_another_region() {
    let (machine, tree) = machine(DEEP_PARALLEL);
    let mut state = initial(&machine, &tree, 0);
    state.history.insert("job".into(), "checking".into());

    let error = tree
        .validate_instance_state(&machine, &state)
        .expect_err("cross-region history is not under its owner");
    assert!(error.detail().contains("not a descendant"));
    assert_step_state_invalid(&machine, &tree, &state, "resume");

    state.history.insert("job".into(), "doing".into());
    let resumed = apply(&machine, &tree, &state, "resume");
    tree.validate_instance_state(&machine, &resumed)
        .expect("valid parallel history produces a valid state");
}

#[test]
fn deadline_and_lifecycle_coherence_are_checked_at_both_runtime_boundaries() {
    let (machine, tree) = machine(DEADLINE_MACHINE);
    let created = initial(&machine, &tree, 100);
    tree.validate_instance_state(&machine, &created)
        .expect("created state is coherent");
    assert_eq!(created.deadlines.get("a_timeout"), Some(&105));

    let mut missing = created.clone();
    missing.deadlines.clear();
    let error = tree
        .validate_instance_state(&machine, &missing)
        .expect_err("active deadline is required");
    assert!(error.detail().contains("missing: [\"a_timeout\"]"));
    assert_step_state_invalid(&machine, &tree, &missing, "go");
    let mut budget = Budget::new(4096);
    match poll_deadline(&machine, &tree, &missing, 100, &mut budget) {
        DeadlineOutcome::Rejected(rejected) => {
            assert_eq!(rejected.rejection.code, "run/configuration_invalid");
            assert!(rejected.deadline.is_none());
        }
        other => panic!("missing schedule must reject a poll, got {other:?}"),
    }

    let at_b = apply(&machine, &tree, &created, "go");
    tree.validate_instance_state(&machine, &at_b)
        .expect("ordinary step preserves validity");
    assert!(at_b.deadlines.is_empty());

    let mut inactive = at_b.clone();
    inactive.deadlines.insert("a_timeout".into(), 105);
    let error = tree
        .validate_instance_state(&machine, &inactive)
        .expect_err("inactive deadline must not remain scheduled");
    assert!(error.detail().contains("unexpected: [\"a_timeout\"]"));
    assert_step_state_invalid(&machine, &tree, &inactive, "finish");

    let mut completed_nonterminal = at_b.clone();
    completed_nonterminal.status = Status::Completed;
    let error = tree
        .validate_instance_state(&machine, &completed_nonterminal)
        .expect_err("completed status requires terminal configuration");
    assert!(error.detail().contains("completed status"));

    let completed = apply(&machine, &tree, &at_b, "finish");
    assert_eq!(completed.status, Status::Completed);
    tree.validate_instance_state(&machine, &completed)
        .expect("terminal step preserves validity");

    let mut running_terminal = completed;
    running_terminal.status = Status::Running;
    let error = tree
        .validate_instance_state(&machine, &running_terminal)
        .expect_err("terminal configuration cannot be running");
    assert!(error.detail().contains("running status"));
}
