//! Core execution contract tests for parallel regions and deadlines.
//!
//! These tests pin parsing limits, public configuration semantics, ordered
//! region selection, and deadline scheduling, cancellation, and tie-breaking.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::{MAX_DEADLINES, MAX_REGIONS};
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::spec::{Topology, compile, parse_machine};
use fsm_core::step::{DeadlineOutcome, Outcome, create, poll_deadline, step};
use fsm_core::tree::Tree;

fn parse_spec(source: &str) -> fsm_core::spec::MachineSpec {
    let value = parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap();
    parse_machine(&value).unwrap()
}

fn compile_json(source: &str) -> (CompiledMachine, Tree) {
    let machine = compile(parse_spec(source)).unwrap();
    let tree = Tree::for_machine(&machine.spec);
    (machine, tree)
}

fn instance(applied: fsm_core::step::Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after,
        ctx: applied.ctx_after,
        history: applied.history_after,
        deadlines: applied.deadlines_after,
        pending: Vec::new(),
    }
}

fn empty_payload() -> Value {
    Value::Obj(BTreeMap::new())
}

#[test]
fn public_sequential_tree_constructor_supports_create_and_step() {
    let machine = compile(parse_spec(
        r#"{
            "format":"fsm.machine/1","name":"public_tree",
            "states":[{"name":"ready"},{"name":"done","terminal":true}],
            "initial":"ready","context":[],
            "events":[{"name":"finish","fields":[]}],
            "transitions":[{"from":"ready","on":"finish","to":"done"}]
        }"#,
    ))
    .unwrap();
    let (states, initial) = match &machine.spec.topology {
        Topology::Sequential { states, initial } => (states.as_slice(), initial.as_str()),
        Topology::Parallel { .. } => panic!("fixture is sequential"),
    };
    let tree = Tree::build(states, initial);
    let state = instance(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());
    let mut budget = Budget::new(4096);
    let outcome = step(
        &machine,
        &tree,
        &state,
        "finish",
        &empty_payload(),
        1,
        &mut budget,
    );
    assert!(matches!(
        outcome,
        Outcome::Applied(ref applied)
            if applied.configuration_after.sequential_leaf() == Some("done")
                && applied.status_after == Status::Completed
    ));
}

#[test]
fn region_and_deadline_limits_accept_the_exact_ceiling() {
    let regions = (0..MAX_REGIONS)
        .map(|index| {
            format!(
                r#"{{"name":"r{index}","states":[{{"name":"s{index}"}}],"initial":"s{index}"}}"#
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let (machine, tree) = compile_json(&format!(
        r#"{{"format":"fsm.machine/1","name":"max_regions","regions":[{regions}],"context":[],"events":[],"transitions":[]}}"#
    ));
    let created = create(&machine, &tree, &BTreeMap::new(), 0).unwrap();
    assert!(matches!(
        created.configuration_after,
        ActiveConfiguration::Parallel { ref leaves } if leaves.len() == MAX_REGIONS
    ));

    let deadlines = (0..MAX_DEADLINES)
        .map(|index| {
            format!(r#"{{"name":"d{index}","from":"waiting","after":"dur(1, ms)","to":"waiting"}}"#)
        })
        .collect::<Vec<_>>()
        .join(",");
    let (machine, tree) = compile_json(&format!(
        r#"{{"format":"fsm.machine/1","name":"max_deadlines","states":[{{"name":"waiting"}}],"initial":"waiting","context":[],"events":[],"transitions":[],"deadlines":[{deadlines}]}}"#
    ));
    let created = create(&machine, &tree, &BTreeMap::new(), 100).unwrap();
    assert_eq!(created.deadlines_after.len(), MAX_DEADLINES);
    assert!(
        created
            .deadlines_after
            .values()
            .all(|due_ms| *due_ms == 101)
    );
}

#[test]
fn parses_parallel_regions_and_validates_deadline_boundaries() {
    let valid = r#"{
        "format":"fsm.machine/1","name":"parallel",
        "regions":[
            {"name":"left","states":[{"name":"l0"},{"name":"l1"}],"initial":"l0"},
            {"name":"right","states":[{"name":"r0"},{"name":"r1"}],"initial":"r0"}
        ],
        "context":[],"events":[],"transitions":[],
        "deadlines":[{"name":"later","from":"l0","after":"dur(1, s)","to":"l1"}]
    }"#;
    let spec = parse_spec(valid);
    match &spec.topology {
        Topology::Parallel { regions } => {
            assert_eq!(regions.len(), 2);
            assert_eq!(regions[0].name, "left");
            assert_eq!(regions[1].initial, "r0");
        }
        Topology::Sequential { .. } => panic!("expected parallel topology"),
    }
    assert_eq!(spec.deadlines[0].name, "later");
    assert!(compile(spec).is_ok());

    let one_region = parse_spec(
        r#"{
            "format":"fsm.machine/1","name":"not_parallel",
            "regions":[{"name":"only","states":[{"name":"idle"}],"initial":"idle"}],
            "context":[],"events":[],"transitions":[]
        }"#,
    );
    let findings = compile(one_region).unwrap_err();
    let finding = findings
        .iter()
        .find(|finding| finding.path == "/regions")
        .expect("one-region lower-bound finding");
    assert_eq!(finding.code, "def/shape");
    assert_eq!(
        finding.message,
        "parallel machines require at least two regions"
    );
    assert_eq!(
        finding.hint,
        "declare two or more regions, or use states with initial"
    );

    let mixed = parse(
        br#"{"format":"fsm.machine/1","name":"m","states":[],"initial":"a","regions":[],"context":[],"events":[],"transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    let findings = parse_machine(&mixed).unwrap_err();
    assert!(findings.iter().any(|finding| finding.path == "/regions"));

    let cross_region = valid.replace("\"to\":\"l1\"", "\"to\":\"r1\"");
    let findings = compile(parse_spec(&cross_region)).unwrap_err();
    assert!(findings.iter().any(|finding| {
        finding.code == "def/cross_region" && finding.path == "/deadlines/0/to"
    }));
}

#[test]
fn cross_region_initial_cycle_is_rejected_without_following_invalid_links() {
    let spec = parse_spec(
        r#"{
            "format":"fsm.machine/1","name":"initial_cycle",
            "regions":[
                {
                    "name":"left","initial":"left_parent",
                    "states":[{
                        "name":"left_parent","initial":"right_parent",
                        "states":[{"name":"left_leaf"}]
                    }]
                },
                {
                    "name":"right","initial":"right_parent",
                    "states":[{
                        "name":"right_parent","initial":"left_parent",
                        "states":[{"name":"right_leaf"}]
                    }]
                }
            ],
            "context":[],"events":[],"transitions":[]
        }"#,
    );
    let findings = fsm_core::spec::validate(&spec).unwrap_err();
    let invalid_initial_paths: Vec<_> = findings
        .iter()
        .filter(|finding| finding.code == "def/initial_not_child")
        .map(|finding| finding.path.as_str())
        .collect();
    assert_eq!(
        invalid_initial_paths,
        [
            "/states/left_parent/initial",
            "/states/right_parent/initial"
        ]
    );

    // The public tree constructor is safe even when a caller inspects a
    // rejected definition instead of discarding it immediately.
    let tree = Tree::for_machine(&spec);
    for (_, root) in &tree.root_initials {
        assert!(tree.initial_descent(*root).is_empty());
    }
}

#[test]
fn parallel_event_chooses_one_region_in_document_order() {
    let (machine, tree) = compile_json(
        r#"{
            "format":"fsm.machine/1","name":"parallel",
            "regions":[
                {"name":"work","states":[{"name":"w0"},{"name":"w1","terminal":true}],"initial":"w0"},
                {"name":"audit","states":[{"name":"a0"},{"name":"a1","terminal":true}],"initial":"a0"}
            ],
            "context":[],"events":[{"name":"go","fields":[]}],
            "transitions":[
                {"from":"w0","on":"go","to":"w1"},
                {"from":"a0","on":"go","to":"a1"}
            ]
        }"#,
    );
    let mut state = instance(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());
    assert!(
        tree.active_leaves(&ActiveConfiguration::Sequential { leaf: "w0".into() })
            .is_none(),
        "a parallel tree must reject a sequential configuration"
    );
    let mut budget = Budget::new(4096);
    let first = match step(
        &machine,
        &tree,
        &state,
        "go",
        &empty_payload(),
        0,
        &mut budget,
    ) {
        Outcome::Applied(applied) => applied,
        outcome => panic!("{outcome:?}"),
    };
    assert_eq!(first.region.as_deref(), Some("work"));
    assert_eq!(first.source_state, "w0");
    assert_eq!(first.status_after, Status::Running);
    assert!(matches!(
        &first.configuration_after,
        ActiveConfiguration::Parallel { leaves }
            if leaves.get("work").map(String::as_str) == Some("w1")
                && leaves.get("audit").map(String::as_str) == Some("a0")
    ));

    state = instance(first);
    let mut budget = Budget::new(4096);
    let second = match step(
        &machine,
        &tree,
        &state,
        "go",
        &empty_payload(),
        1,
        &mut budget,
    ) {
        Outcome::Applied(applied) => applied,
        outcome => panic!("{outcome:?}"),
    };
    assert_eq!(second.region.as_deref(), Some("audit"));
    assert_eq!(second.status_after, Status::Completed);
}

#[test]
fn mismatched_public_configuration_has_its_own_stable_error() {
    let (machine, tree) = compile_json(
        r#"{
            "format":"fsm.machine/1","name":"parallel_invalid_config",
            "regions":[
                {"name":"left","states":[{"name":"l0"}],"initial":"l0"},
                {"name":"right","states":[{"name":"r0"}],"initial":"r0"}
            ],
            "context":[],"events":[{"name":"go","fields":[]}],"transitions":[]
        }"#,
    );
    let mut state = instance(create(&machine, &tree, &BTreeMap::new(), 0).unwrap());
    state.configuration = ActiveConfiguration::Sequential { leaf: "l0".into() };

    let mut budget = Budget::new(4096);
    let rejection = match step(
        &machine,
        &tree,
        &state,
        "go",
        &empty_payload(),
        1,
        &mut budget,
    ) {
        Outcome::Rejected(rejection) => rejection,
        outcome => panic!("{outcome:?}"),
    };
    assert_eq!(rejection.code, "run/configuration_invalid");

    let mut budget = Budget::new(4096);
    let rejection = match poll_deadline(&machine, &tree, &state, 1, &mut budget) {
        DeadlineOutcome::Rejected(rejected) => rejected.rejection,
        outcome => panic!("{outcome:?}"),
    };
    assert_eq!(rejection.code, "run/configuration_invalid");
}

#[test]
fn deadlines_schedule_report_not_due_break_ties_and_complete() {
    let (machine, tree) = compile_json(
        r#"{
            "format":"fsm.machine/1","name":"timed",
            "states":[
                {"name":"waiting"},
                {"name":"first_done","terminal":true},
                {"name":"second_done","terminal":true},
                {"name":"escaped"}
            ],
            "initial":"waiting","context":[],
            "events":[{"name":"escape","fields":[]}],
            "transitions":[{"from":"waiting","on":"escape","to":"escaped"}],
            "deadlines":[
                {"name":"first","from":"waiting","after":"dur(5, ms)","to":"first_done"},
                {"name":"second","from":"waiting","after":"dur(5, ms)","to":"second_done"}
            ]
        }"#,
    );
    let created = create(&machine, &tree, &BTreeMap::new(), 100).unwrap();
    assert_eq!(
        created.deadlines_after,
        BTreeMap::from([("first".into(), 105), ("second".into(), 105)])
    );
    let state = instance(created);

    let mut budget = Budget::new(4096);
    match poll_deadline(&machine, &tree, &state, 104, &mut budget) {
        DeadlineOutcome::NotDue { next: Some(next) } => {
            assert_eq!(next.name, "first");
            assert_eq!(next.deadline_idx, 0);
            assert_eq!(next.due_ms, 105);
        }
        outcome => panic!("{outcome:?}"),
    }

    let mut budget = Budget::new(4096);
    let fired = match poll_deadline(&machine, &tree, &state, 105, &mut budget) {
        DeadlineOutcome::Applied(applied) => applied,
        outcome => panic!("{outcome:?}"),
    };
    assert_eq!(fired.deadline.name, "first");
    assert_eq!(fired.transition.transition_idx, 0);
    assert_eq!(fired.transition.status_after, Status::Completed);
    assert!(fired.transition.deadlines_after.is_empty());
    assert!(matches!(
        fired.transition.configuration_after,
        ActiveConfiguration::Sequential { ref leaf } if leaf == "first_done"
    ));
}

#[test]
fn leaving_a_deadline_source_cancels_its_schedule() {
    let (machine, tree) = compile_json(
        r#"{
            "format":"fsm.machine/1","name":"cancel",
            "states":[{"name":"waiting"},{"name":"escaped"},{"name":"expired"}],
            "initial":"waiting","context":[],
            "events":[{"name":"escape","fields":[]}],
            "transitions":[{"from":"waiting","on":"escape","to":"escaped"}],
            "deadlines":[{"name":"expire","from":"waiting","after":"dur(5, ms)","to":"expired"}]
        }"#,
    );
    let state = instance(create(&machine, &tree, &BTreeMap::new(), 10).unwrap());
    assert_eq!(state.deadlines.get("expire"), Some(&15));

    let mut budget = Budget::new(4096);
    let escaped = match step(
        &machine,
        &tree,
        &state,
        "escape",
        &empty_payload(),
        12,
        &mut budget,
    ) {
        Outcome::Applied(applied) => applied,
        outcome => panic!("{outcome:?}"),
    };
    assert!(escaped.deadlines_after.is_empty());
    let escaped = instance(escaped);
    let mut budget = Budget::new(4096);
    assert_eq!(
        poll_deadline(&machine, &tree, &escaped, 20, &mut budget),
        DeadlineOutcome::NotDue { next: None }
    );
}

#[test]
fn a_terminal_parallel_region_cancels_ancestor_deadlines() {
    let (machine, tree) = compile_json(
        r#"{
            "format":"fsm.machine/1","name":"terminal_region_timer",
            "regions":[
                {"name":"work","states":[
                    {"name":"flow","initial":"working","states":[
                        {"name":"working"},{"name":"work_done","terminal":true}
                    ]}
                ],"initial":"flow"},
                {"name":"audit","states":[{"name":"auditing"},{"name":"audit_done","terminal":true}],"initial":"auditing"}
            ],
            "context":[],"events":[{"name":"finish_work","fields":[]}],
            "transitions":[{"from":"working","on":"finish_work","to":"work_done"}],
            "deadlines":[{"name":"restart","from":"flow","after":"dur(5, ms)","to":"working"}]
        }"#,
    );
    let state = instance(create(&machine, &tree, &BTreeMap::new(), 10).unwrap());
    assert_eq!(state.deadlines.get("restart"), Some(&15));

    let mut budget = Budget::new(4096);
    let finished = match step(
        &machine,
        &tree,
        &state,
        "finish_work",
        &empty_payload(),
        11,
        &mut budget,
    ) {
        Outcome::Applied(applied) => applied,
        outcome => panic!("{outcome:?}"),
    };
    assert_eq!(finished.status_after, Status::Running);
    assert!(finished.deadlines_after.is_empty());

    let finished = instance(finished);
    let mut budget = Budget::new(4096);
    assert_eq!(
        poll_deadline(&machine, &tree, &finished, 20, &mut budget),
        DeadlineOutcome::NotDue { next: None }
    );
}
