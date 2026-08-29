//! Bounded small-machine differential against the deliberately naive oracle.

// The generated/executed/sequence counts are this suite's report to whoever is
// reading a CI log: an enumeration that silently covered nothing looks exactly
// like one that covered everything. `enumerate_reactive` prints for the same
// reason and carries the same allow.
#![allow(clippy::print_stderr)]

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::spec::{Topology, compile, parse_machine};
use fsm_core::step::{Applied, DeadlineOutcome, Outcome, create, poll_deadline, step};
use fsm_core::tree::Tree;

mod oracle;

fn payload() -> Value {
    Value::Obj(BTreeMap::new())
}

fn compile_src(src: &str) -> (fsm_core::machine::CompiledMachine, Tree) {
    let value = fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT)
        .unwrap_or_else(|err| panic!("generated JSON did not parse: {err:?}\n{src}"));
    let spec = parse_machine(&value)
        .unwrap_or_else(|findings| panic!("generated machine did not parse: {findings:?}\n{src}"));
    let machine = compile(spec).unwrap_or_else(|findings| {
        panic!("generated machine did not compile: {findings:?}\n{src}")
    });
    let tree = Tree::for_machine(&machine.spec);
    (machine, tree)
}

// The enumeration toolkit is shared with `enumerate_reactive`; each root uses
// the part it needs.
#[path = "enumerate_small/compare.rs"]
#[allow(dead_code)]
mod compare;
#[path = "enumerate_small/machine_json.rs"]
#[allow(dead_code)]
mod machine_json;
#[path = "enumerate_small/trees.rs"]
mod trees;

use compare::*;
use machine_json::*;
use trees::*;

#[test]
fn enumerate_deadline_schedule_poll_cancel_and_reentry_differential() {
    let mut generated = 0u64;
    let mut not_due = 0u64;
    let mut applied = 0u64;
    let mut ties = 0u64;
    let mut cancellations = 0u64;
    let mut reentries = 0u64;

    for first_after in 0..=2 {
        for second_after in 0..=2 {
            generated += 1;
            let source = generated_deadline_machine(first_after, second_after, 0, "1", "2");
            let (machine, tree) = compile_src(&source);
            let engine_created = create(&machine, &tree, &BTreeMap::new(), 10).unwrap();
            let oracle_created = oracle::naive_create_at(&machine, &BTreeMap::new(), 10).unwrap();
            assert_applied_parity(&engine_created, &oracle_created, &source);
            assert_eq!(
                engine_created.deadlines_after,
                BTreeMap::from([
                    ("first".to_string(), 10 + first_after),
                    ("second".to_string(), 10 + second_after),
                ])
            );
            let engine_state = state_from_applied(engine_created);
            let oracle_state = state_from_applied(oracle_created);
            let first_due = 10 + first_after;
            let second_due = 10 + second_after;
            let due_ms = first_due.min(second_due);
            let expected_index = usize::from(second_due < first_due);
            let engine_before_poll = engine_state.clone();
            let oracle_before_poll = oracle_state.clone();

            let mut engine_budget = Budget::new(4096);
            let mut oracle_budget = Budget::new(4096);
            let engine_early = poll_deadline(
                &machine,
                &tree,
                &engine_state,
                due_ms - 1,
                &mut engine_budget,
            );
            let oracle_early = oracle::naive_poll_deadline(
                &machine,
                &oracle_state,
                due_ms - 1,
                &mut oracle_budget,
            );
            match (engine_early, oracle_early) {
                (
                    DeadlineOutcome::NotDue { next: engine },
                    DeadlineOutcome::NotDue { next: oracle },
                ) => {
                    not_due += 1;
                    assert_eq!(engine, oracle, "{source}");
                    let next = engine.expect("a generated waiting state has schedules");
                    assert_eq!(next.deadline_idx, expected_index as u32, "{source}");
                    assert_eq!(next.due_ms, due_ms, "{source}");
                }
                outcomes => panic!("NotDue mismatch: {outcomes:?}\n{source}"),
            }
            assert_eq!(
                engine_state, engine_before_poll,
                "engine NotDue mutated state"
            );
            assert_eq!(
                oracle_state, oracle_before_poll,
                "oracle NotDue mutated state"
            );

            let mut engine_budget = Budget::new(4096);
            let mut oracle_budget = Budget::new(4096);
            let engine_due =
                poll_deadline(&machine, &tree, &engine_state, due_ms, &mut engine_budget);
            let oracle_due =
                oracle::naive_poll_deadline(&machine, &oracle_state, due_ms, &mut oracle_budget);
            let (engine_fired, oracle_fired) = match (engine_due, oracle_due) {
                (DeadlineOutcome::Applied(engine), DeadlineOutcome::Applied(oracle)) => {
                    applied += 1;
                    assert_eq!(engine.deadline, oracle.deadline, "{source}");
                    assert_eq!(
                        engine.deadline.deadline_idx, expected_index as u32,
                        "{source}"
                    );
                    assert_applied_parity(&engine.transition, &oracle.transition, &source);
                    (engine.transition, oracle.transition)
                }
                outcomes => panic!("due poll mismatch: {outcomes:?}\n{source}"),
            };
            if first_due == second_due {
                ties += 1;
                assert_eq!(expected_index, 0);
            }
            assert_eq!(
                engine_fired.ctx_after.get("n"),
                Some(&fsm_core::expr::eval::Val::Int(if expected_index == 0 {
                    1
                } else {
                    2
                })),
                "one poll must apply exactly one selected deadline"
            );

            let engine_reentered = state_from_applied(engine_fired);
            let oracle_reentered = state_from_applied(oracle_fired);
            assert_eq!(
                engine_reentered.deadlines,
                BTreeMap::from([
                    ("first".to_string(), due_ms + first_after),
                    ("second".to_string(), due_ms + second_after),
                ]),
                "external self-target must reschedule from poll time"
            );

            let mut engine_budget = Budget::new(4096);
            let mut oracle_budget = Budget::new(4096);
            let engine_left = step(
                &machine,
                &tree,
                &engine_reentered,
                "leave",
                &payload(),
                due_ms + 3,
                &mut engine_budget,
            );
            let oracle_left = oracle::naive_step_at(
                &machine,
                &oracle_reentered,
                "leave",
                &payload(),
                due_ms + 3,
                &mut oracle_budget,
            );
            let (engine_left, oracle_left) = match (engine_left, oracle_left) {
                (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
                    assert_applied_parity(&engine, &oracle, &source);
                    (engine, oracle)
                }
                outcomes => panic!("deadline cancellation mismatch: {outcomes:?}\n{source}"),
            };
            cancellations += 1;
            assert!(engine_left.deadlines_after.is_empty());

            let engine_away = state_from_applied(engine_left);
            let oracle_away = state_from_applied(oracle_left);
            let return_ms = due_ms + 20;
            let mut engine_budget = Budget::new(4096);
            let mut oracle_budget = Budget::new(4096);
            let engine_returned = step(
                &machine,
                &tree,
                &engine_away,
                "return",
                &payload(),
                return_ms,
                &mut engine_budget,
            );
            let oracle_returned = oracle::naive_step_at(
                &machine,
                &oracle_away,
                "return",
                &payload(),
                return_ms,
                &mut oracle_budget,
            );
            match (engine_returned, oracle_returned) {
                (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
                    reentries += 1;
                    assert_applied_parity(&engine, &oracle, &source);
                    assert_eq!(
                        engine.deadlines_after,
                        BTreeMap::from([
                            ("first".to_string(), return_ms + first_after),
                            ("second".to_string(), return_ms + second_after),
                        ])
                    );
                }
                outcomes => panic!("deadline re-entry mismatch: {outcomes:?}\n{source}"),
            }
        }
    }

    assert_eq!(generated, 9);
    assert_eq!(not_due, generated);
    assert_eq!(applied, generated);
    assert_eq!(ties, 3);
    assert_eq!(cancellations, generated);
    assert_eq!(reentries, generated);
}

#[test]
fn enumerate_deadline_rejection_is_selected_once_and_atomic() {
    let cases = [
        (0, 0, "ctx.n + 1", "0", 0u32),
        (1, 0, "0", "ctx.n + 1", 1u32),
    ];
    let mut rejected = 0u64;
    for (first_after, second_after, first_value, second_value, expected_index) in cases {
        let source = generated_deadline_machine(
            first_after,
            second_after,
            i64::MAX,
            first_value,
            second_value,
        );
        let (machine, tree) = compile_src(&source);
        let engine_created = create(&machine, &tree, &BTreeMap::new(), 10).unwrap();
        let oracle_created = oracle::naive_create_at(&machine, &BTreeMap::new(), 10).unwrap();
        assert_applied_parity(&engine_created, &oracle_created, &source);
        let engine_state = state_from_applied(engine_created);
        let oracle_state = state_from_applied(oracle_created);
        let engine_before = engine_state.clone();
        let oracle_before = oracle_state.clone();

        let mut engine_budget = Budget::new(4096);
        let mut oracle_budget = Budget::new(4096);
        let engine = poll_deadline(&machine, &tree, &engine_state, 10, &mut engine_budget);
        let oracle = oracle::naive_poll_deadline(&machine, &oracle_state, 10, &mut oracle_budget);
        match (engine, oracle) {
            (DeadlineOutcome::Rejected(engine), DeadlineOutcome::Rejected(oracle)) => {
                rejected += 1;
                assert_eq!(engine.deadline, oracle.deadline, "{source}");
                let selected = engine.deadline.expect("a due schedule was selected");
                assert_eq!(selected.deadline_idx, expected_index, "{source}");
                assert_eq!(engine.rejection.code, oracle.rejection.code, "{source}");
                assert_eq!(engine.rejection.cause, oracle.rejection.cause, "{source}");
                assert_eq!(engine.rejection.code, "run/action_error");
                assert_eq!(engine.rejection.cause, Some("run/overflow"));
            }
            outcomes => panic!("deadline rejection mismatch: {outcomes:?}\n{source}"),
        }
        assert_eq!(
            engine_state, engine_before,
            "engine rejection mutated state"
        );
        assert_eq!(
            oracle_state, oracle_before,
            "oracle rejection mutated state"
        );
        assert_eq!(
            engine_state.ctx.get("n"),
            Some(&fsm_core::expr::eval::Val::Int(i64::MAX)),
            "a rejected selected deadline must not fall through to the other due deadline"
        );
    }
    assert_eq!(rejected, cases.len() as u64);
}

#[test]
fn enumerate_parallel_global_winner_differential() {
    let mut counts = SuiteCounts::default();

    for case in PARALLEL_WINNER_CASES {
        let selection: Vec<String> = case
            .rows
            .iter()
            .map(|row| {
                transition_json(
                    row.source,
                    "choose",
                    row.target,
                    Some(row.guard),
                    BlockCase::IncrementAndEmit,
                )
            })
            .collect();
        let src = parallel_machine_json(&selection);
        let (machine, tree) = compile_src(&src);
        let engine_create = create(&machine, &tree, &BTreeMap::new(), 0).unwrap();
        let oracle_create = oracle::naive_create(&machine, &BTreeMap::new()).unwrap();
        let expected_leaves = BTreeMap::from([
            ("alpha".to_string(), "a0".to_string()),
            ("beta".to_string(), "b0".to_string()),
        ]);
        assert_eq!(
            engine_create.configuration_after,
            ActiveConfiguration::Parallel {
                leaves: expected_leaves.clone()
            }
        );
        assert_eq!(
            oracle_create.configuration_after,
            ActiveConfiguration::Parallel {
                leaves: expected_leaves
            }
        );
        assert_eq!(engine_create.entered, ["a", "a0", "b", "b0"]);
        assert_eq!(oracle_create.entered, engine_create.entered);
        assert_eq!(
            engine_create.ctx_after.get("n"),
            Some(&fsm_core::expr::eval::Val::Int(4))
        );
        assert_eq!(oracle_create.ctx_after, engine_create.ctx_after);

        let state = InstanceState {
            status: Status::Running,
            configuration: engine_create.configuration_after,
            ctx: engine_create.ctx_after,
            history: engine_create.history_after,
            deadlines: BTreeMap::new(),
            pending: Vec::new(),
            invocations: BTreeMap::new(),
            signals: BTreeMap::new(),
        };
        let mut engine_budget = Budget::new(4096);
        let mut oracle_budget = Budget::new(4096);
        let engine = step(
            &machine,
            &tree,
            &state,
            "choose",
            &payload(),
            0,
            &mut engine_budget,
        );
        let oracle = oracle::naive_step(&machine, &state, "choose", &payload(), &mut oracle_budget);
        match (engine, oracle) {
            (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
                assert_eq!(
                    engine.region.as_deref(),
                    Some(case.expected_region),
                    "{}",
                    case.name
                );
                assert_eq!(oracle.region, engine.region);
                assert_eq!(engine.source_state, case.expected_source, "{}", case.name);
                assert_eq!(oracle.source_state, engine.source_state);
                assert_eq!(oracle.transition_idx, engine.transition_idx);
                assert_eq!(oracle.configuration_after, engine.configuration_after);
                let ActiveConfiguration::Parallel { leaves } = engine.configuration_after else {
                    panic!("parallel event produced a sequential configuration");
                };
                assert_eq!(
                    leaves.get("alpha").map(String::as_str),
                    Some(case.expected_alpha),
                    "{}",
                    case.name
                );
                assert_eq!(
                    leaves.get("beta").map(String::as_str),
                    Some(case.expected_beta),
                    "{}",
                    case.name
                );
            }
            other => panic!("parallel winner mismatch: {other:?}\n{src}"),
        }

        execute_case(src, &mut counts);
    }

    assert_eq!(counts.generated, 6, "parallel case grammar changed");
    assert_eq!(counts.generated, counts.executed);
    assert_eq!(counts.runs.sequences, counts.executed * 341);
    assert_eq!(counts.runs.steps, counts.executed * 1_252);
    assert!(counts.runs.internal_applied > 0);
    assert!(counts.runs.external_applied > 0);
    assert!(counts.runs.leaf_changes > 0);
    assert!(counts.runs.effects > 0);
    assert!(counts.runs.rejected > 0);
    assert!(counts.runs.ignored > 0);
}

#[test]
fn enumerate_small_differential() {
    let guards = [
        None,
        Some("true"),
        Some("false"),
        Some("ctx.b"),
        Some("not ctx.b"),
    ];
    let mut counts = SuiteCounts::default();
    let mut forest_count = 0u64;
    let mut topology_internal = 0u64;
    let mut topology_external = 0u64;
    let mut initial_cases = 0u64;
    let mut guard_block_cases = 0u64;
    let mut entry_block_cases = 0u64;
    let mut exit_block_cases = 0u64;
    let mut pipeline_cases = 0u64;

    for n in 1..=5 {
        for forest in forests(n, 3) {
            if forest.is_empty() {
                continue;
            }
            forest_count += 1;
            let named = name_forest(&forest);
            let choices = initial_choices(&named);
            let mut names = Vec::new();
            collect_names(&named, &mut names);

            // Every state is made active in turn, then crossed with internal and
            // every external target. Thus every generated source/target topology
            // executes at least once rather than merely compiling unreachable rows.
            for source in &names {
                let choice = choice_activating(&named, &choices, source);
                let states = emit_states(&named, &choice, &Decorations::default(), None);
                let internal = transition_json(source, "e", None, None, BlockCase::None);
                execute_case(
                    machine_json(&states, &choice.root, &["e", "f"], &[internal]),
                    &mut counts,
                );
                topology_internal += 1;
                for target in &names {
                    let external =
                        transition_json(source, "e", Some(target), None, BlockCase::None);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[external]),
                        &mut counts,
                    );
                    topology_external += 1;
                }
            }

            // Cross every legal selection of the root and every compound's
            // initial child. The active leaf owns an executing internal row.
            for choice in &choices {
                let chain = active_chain(&named, choice);
                let source = chain.last().expect("initial chain has a leaf");
                let states = emit_states(&named, choice, &Decorations::default(), None);
                let transition =
                    transition_json(source, "e", None, Some("ctx.b"), BlockCase::Increment);
                execute_case(
                    machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                    &mut counts,
                );
                initial_cases += 1;
            }

            // The finite guard pool is crossed with the complete transition-block
            // pool (none, both assignments, emit, and assignment+emit).
            let choice = choices.first().expect("nonempty initial choices");
            let chain = active_chain(&named, choice);
            let source = chain.last().expect("initial chain has a leaf");
            let states = emit_states(&named, choice, &Decorations::default(), None);
            for guard in guards {
                for block in TRANSITION_BLOCKS {
                    let transition = transition_json(source, "e", None, guard, block);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                        &mut counts,
                    );
                    guard_block_cases += 1;
                }
            }

            // Put every nonempty block variant at entry and exit on every state.
            // An external self-transition on that active state traverses the block.
            for state in &names {
                let choice = choice_activating(&named, &choices, state);
                for block in STATE_BLOCKS {
                    let entry = Decorations {
                        entry: Some((state.clone(), block)),
                        exit: None,
                    };
                    let states = emit_states(&named, &choice, &entry, None);
                    let transition =
                        transition_json(state, "e", Some(state), None, BlockCase::None);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                        &mut counts,
                    );
                    entry_block_cases += 1;

                    let exit = Decorations {
                        entry: None,
                        exit: Some((state.clone(), block)),
                    };
                    let states = emit_states(&named, &choice, &exit, None);
                    let transition =
                        transition_json(state, "e", Some(state), None, BlockCase::None);
                    execute_case(
                        machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                        &mut counts,
                    );
                    exit_block_cases += 1;
                }
            }

            // One row per topology crosses exit, transition, and entry blocks in
            // one event, making effect order and pre-block snapshots observable.
            let choice = choices.first().expect("nonempty initial choices");
            let source = active_chain(&named, choice)
                .last()
                .expect("initial chain has a leaf")
                .clone();
            let decorations = Decorations {
                entry: Some((source.clone(), BlockCase::IncrementAndEmit)),
                exit: Some((source.clone(), BlockCase::IncrementAndEmit)),
            };
            let states = emit_states(&named, choice, &decorations, None);
            let transition = transition_json(
                &source,
                "e",
                Some(&source),
                None,
                BlockCase::IncrementAndEmit,
            );
            execute_case(
                machine_json(&states, &choice.root, &["e", "f"], &[transition]),
                &mut counts,
            );
            pipeline_cases += 1;
        }
    }

    let mut history_placements = 0u64;
    let mut history_targets = 0u64;
    // A history pseudostate counts toward the five-state bound, so its owner
    // topologies use at most four ordinary states. Every compound owner, kind,
    // and owner-initial child is emitted. Whenever an outside leaf exists, the
    // generated go/back pair first binds and then targets history legally.
    for n in 2..=4 {
        for forest in forests(n, 3) {
            if forest.is_empty() {
                continue;
            }
            let named = name_forest(&forest);
            let choices = initial_choices(&named);
            let mut compounds = Vec::new();
            let mut leaves = Vec::new();
            collect_compounds(&named, &mut compounds);
            collect_leaves(&named, &mut leaves);
            for owner in compounds {
                let owner_node = find_named(&named, &owner).expect("compound exists");
                for owner_initial in &owner_node.kids {
                    let choice = choices
                        .iter()
                        .find(|choice| {
                            choice.children.get(&owner) == Some(&owner_initial.name)
                                && active_chain(&named, choice)
                                    .iter()
                                    .any(|name| name == &owner)
                        })
                        .unwrap_or_else(|| panic!("no choice activates history owner {owner}"));
                    let inside = active_chain(&named, choice)
                        .last()
                        .expect("owner initial descends to a leaf")
                        .clone();
                    let outside = leaves
                        .iter()
                        .find(|leaf| !is_descendant_or_self(&named, &owner, leaf.as_str()));
                    for kind in ["shallow", "deep"] {
                        let history = HistoryPlacement {
                            owner: owner.clone(),
                            name: format!("h_{kind}"),
                            kind,
                        };
                        let states =
                            emit_states(&named, choice, &Decorations::default(), Some(&history));
                        let transitions = if let Some(outside) = outside {
                            history_targets += 1;
                            vec![
                                transition_json(
                                    &inside,
                                    "go",
                                    Some(outside),
                                    None,
                                    BlockCase::IncrementAndEmit,
                                ),
                                transition_json(
                                    outside,
                                    "back",
                                    Some(&history.name),
                                    None,
                                    BlockCase::Emit,
                                ),
                            ]
                        } else {
                            vec![transition_json(
                                &inside,
                                "go",
                                None,
                                None,
                                BlockCase::IncrementAndEmit,
                            )]
                        };
                        execute_case(
                            machine_json(&states, &choice.root, &["go", "back"], &transitions),
                            &mut counts,
                        );
                        history_placements += 1;
                    }
                }
            }
        }
    }

    // Outcome::Ignored is part of the differential contract as well.
    let ignore_src = r#"{"format":"fsm.machine/1","name":"ignore","states":[{"name":"a"}],"initial":"a","on_unhandled":"ignore","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]},{"name":"f","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
    execute_case(ignore_src.to_string(), &mut counts);

    // Exhaustion in a transition block has one exact public/private error pair.
    let budget_src = r#"{"format":"fsm.machine/1","name":"budget","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"v","ty":"int"}]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}],"emit":[{"effect":"fx","args":{"v":"ctx.n"}}]}]}"#;
    let (machine, tree) = compile_src(budget_src);
    let state = state_from_create(&machine, &tree);
    let before = state.clone();
    let mut engine_budget = Budget::new(1);
    let mut oracle_budget = Budget::new(1);
    let engine = step(
        &machine,
        &tree,
        &state,
        "e",
        &payload(),
        0,
        &mut engine_budget,
    );
    let oracle = oracle::naive_step(&machine, &state, "e", &payload(), &mut oracle_budget);
    match (&engine, &oracle) {
        (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
            assert_eq!(engine.code, "run/action_error");
            assert_eq!(engine.cause, Some("internal/budget"));
            assert_eq!(oracle.code, "run/action_error");
            assert_eq!(oracle.cause, Some("internal/budget"));
        }
        other => panic!("budget exhaustion mismatch: {other:?}"),
    }
    assert_eq!(state, before, "budget rejection mutated input state");

    let enum_src = r#"{"format":"fsm.machine/1","name":"en","enums":{"Color":["red","blue"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"paint","fields":[{"name":"c","ty":{"enum":"Color"}}]}],"transitions":[{"from":"a","on":"paint"}]}"#;
    let (machine, tree) = compile_src(enum_src);
    let state = state_from_create(&machine, &tree);
    let mut bad = BTreeMap::new();
    bad.insert("c".into(), Value::Str("green".into()));
    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    match (
        step(
            &machine,
            &tree,
            &state,
            "paint",
            &Value::Obj(bad.clone()),
            0,
            &mut engine_budget,
        ),
        oracle::naive_step(
            &machine,
            &state,
            "paint",
            &Value::Obj(bad),
            &mut oracle_budget,
        ),
    ) {
        (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
            assert_eq!(engine.code, oracle.code);
            assert_eq!(engine.code, "req/field_type");
        }
        other => panic!("enum mismatch: {other:?}"),
    }
    let mut good = BTreeMap::new();
    good.insert("c".into(), Value::Str("red".into()));
    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    match (
        step(
            &machine,
            &tree,
            &state,
            "paint",
            &Value::Obj(good.clone()),
            0,
            &mut engine_budget,
        ),
        oracle::naive_step(
            &machine,
            &state,
            "paint",
            &Value::Obj(good),
            &mut oracle_budget,
        ),
    ) {
        (Outcome::Applied(engine), Outcome::Applied(oracle)) => {
            assert_eq!(engine.effects, oracle.effects);
        }
        other => panic!("valid enum mismatch: {other:?}"),
    }

    let decimal_src = r#"{"format":"fsm.machine/1","name":"dc","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"pay","fields":[{"name":"amt","ty":{"decimal":"2"}}]}],"transitions":[{"from":"a","on":"pay"}]}"#;
    let (machine, tree) = compile_src(decimal_src);
    let state = state_from_create(&machine, &tree);
    let mut wide = BTreeMap::new();
    wide.insert("amt".into(), Value::Str("1.000".into()));
    let mut engine_budget = Budget::new(4096);
    let mut oracle_budget = Budget::new(4096);
    match (
        step(
            &machine,
            &tree,
            &state,
            "pay",
            &Value::Obj(wide.clone()),
            0,
            &mut engine_budget,
        ),
        oracle::naive_step(
            &machine,
            &state,
            "pay",
            &Value::Obj(wide),
            &mut oracle_budget,
        ),
    ) {
        (Outcome::Rejected(engine), Outcome::Rejected(oracle)) => {
            assert_eq!(engine.code, oracle.code);
            assert_eq!(engine.code, "req/field_scale");
        }
        other => panic!("decimal mismatch: {other:?}"),
    }

    assert_eq!(forest_count, 55, "bounded forest grammar changed");
    assert_eq!(topology_internal, 242, "internal topology rows changed");
    assert_eq!(topology_external, 1_112, "external topology rows changed");
    assert_eq!(initial_cases, 181, "initial-choice cross-product changed");
    assert_eq!(
        guard_block_cases, 1_375,
        "guard/block cross-product changed"
    );
    assert_eq!(entry_block_cases, 968, "entry-block placement rows changed");
    assert_eq!(exit_block_cases, 968, "exit-block placement rows changed");
    assert_eq!(pipeline_cases, 55, "pipeline rows changed");
    assert!(history_placements > 0, "history placement grammar is empty");
    assert!(
        history_targets > 0,
        "no legal outside-to-history target executed"
    );
    assert_eq!(
        counts.generated, counts.executed,
        "a generated machine was skipped"
    );
    assert_eq!(
        counts.runs.sequences,
        counts.executed * 31,
        "not every event sequence of length at most four executed"
    );
    assert_eq!(
        counts.runs.steps,
        counts.executed * 98,
        "executed event count changed"
    );
    assert!(counts.runs.internal_applied > 0);
    assert!(counts.runs.external_applied > 0);
    assert!(counts.runs.leaf_changes > 0);
    assert!(counts.runs.effects > 0);
    assert!(counts.runs.history_changes > 0);
    assert!(counts.runs.rejected > 0);
    assert!(counts.runs.ignored > 0);

    eprintln!(
        "enumerate_small generated={} executed={} sequences={} steps={} applied={} rejected={} ignored={} history_placements={} history_targets={}",
        counts.generated,
        counts.executed,
        counts.runs.sequences,
        counts.runs.steps,
        counts.runs.applied,
        counts.runs.rejected,
        counts.runs.ignored,
        history_placements,
        history_targets
    );
}
