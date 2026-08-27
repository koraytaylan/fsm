//! Bounded reactive-machine differential against the naive macrostep oracle:
//! eventless cascades, raises from every block, final states, the ceiling,
//! and admission of certain cycles — every enumerated machine and every event
//! sequence must agree on the configuration, context, effects, microstep
//! sequence, status, and rejection code.
//!
//! Plan 0009 task 4704.

// The generated/executed/refused counts are this suite's report to whoever
// is reading a CI log: an enumeration that silently covered nothing looks
// exactly like one that covered everything.
#![allow(clippy::print_stderr)]

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::Value;
use fsm_core::machine::InstanceState;
use fsm_core::spec::{Topology, compile, parse_machine};
use fsm_core::step::{Applied, Outcome, create, step};
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

// The enumeration toolkit is shared with `enumerate_small`; each root uses
// the part it needs.
#[path = "enumerate_small/compare.rs"]
#[allow(dead_code)]
mod compare;
#[path = "enumerate_small/machine_json.rs"]
#[allow(dead_code)]
mod machine_json;
#[path = "enumerate_small/trees.rs"]
#[allow(dead_code)]
mod trees;

use compare::*;
use machine_json::*;
use trees::*;

/// Every tree of up to four states, three deep, with its first initial
/// choice: the active leaf, the state names, and the rendered states.
fn small_trees() -> Vec<(Vec<Named>, InitialChoice, String, Vec<String>)> {
    let mut out = Vec::new();
    for n in 1..=4 {
        for forest in forests(n, 3) {
            if forest.is_empty() {
                continue;
            }
            let named = name_forest(&forest);
            let choices = initial_choices(&named);
            let choice = choices.first().expect("nonempty initial choices").clone();
            let leaf = active_chain(&named, &choice)
                .last()
                .expect("initial chain has a leaf")
                .clone();
            let mut names = Vec::new();
            collect_names(&named, &mut names);
            out.push((named, choice, leaf, names));
        }
    }
    out
}

#[test]
fn enumerate_eventless_cascades_and_admission() {
    // The trigger enters `source`, whose eventless transition then fires
    // when its guard allows: every source, every target including internal,
    // every guard of the pool. Certain cycles are refused by both, guarded
    // ones run into the ceiling in both.
    let guards = [None, Some("true"), Some("ctx.b"), Some("not ctx.b")];
    let mut counts = SuiteCounts::default();
    let mut refused = 0u64;
    for (named, choice, leaf, names) in small_trees() {
        let states = emit_states(&named, &choice, &Decorations::default(), None);
        for source in &names {
            for target in names.iter().map(|t| Some(t.as_str())).chain([None]) {
                for guard in guards {
                    let transitions = vec![
                        transition_json(&leaf, "e", Some(source), None, BlockCase::Increment),
                        eventless_transition_json(source, target, guard, BlockCase::Increment),
                    ];
                    let src = reactive_machine_json(&states, &choice.root, &transitions);
                    if !execute_macrostep_case(src, &mut counts) {
                        refused += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "eventless: generated={} executed={} refused={refused} {:?}",
        counts.generated, counts.executed, counts.runs
    );
    assert!(refused > 0, "some enumerated shape is a certain cycle");
    assert!(
        counts.runs.microstep_limits > 0,
        "some guarded cycle hits the ceiling"
    );
    assert!(counts.runs.reactions > 0);
    assert!(counts.executed > 0);
}

#[test]
fn enumerate_raises_from_every_block() {
    // A raise in the transition block, the landing state's entry block, or
    // the leaving state's exit block; a handler somewhere or nowhere.
    let mut counts = SuiteCounts::default();
    for (named, choice, leaf, names) in small_trees() {
        let plain = emit_states(&named, &choice, &Decorations::default(), None);
        for landing in &names {
            for handler in &names {
                for target in names.iter().map(|t| Some(t.as_str())).chain([None]) {
                    let transitions = vec![
                        transition_json(&leaf, "e", Some(landing), None, BlockCase::Raise),
                        transition_json(handler, "r", target, None, BlockCase::Increment),
                    ];
                    let src = reactive_machine_json(&plain, &choice.root, &transitions);
                    assert!(execute_macrostep_case(src, &mut counts));
                }
            }
            // Nobody handles it: a discard, never a rejection.
            let transitions = vec![transition_json(
                &leaf,
                "e",
                Some(landing),
                None,
                BlockCase::Raise,
            )];
            assert!(execute_macrostep_case(
                reactive_machine_json(&plain, &choice.root, &transitions),
                &mut counts
            ));
            // The same raise from the landing state's entry and the leaf's exit.
            for (decorations, handler_from) in [
                (
                    Decorations {
                        entry: Some((landing.clone(), BlockCase::Raise)),
                        exit: None,
                    },
                    landing.clone(),
                ),
                (
                    Decorations {
                        entry: None,
                        exit: Some((leaf.clone(), BlockCase::Raise)),
                    },
                    landing.clone(),
                ),
            ] {
                let states = emit_states(&named, &choice, &decorations, None);
                let transitions = vec![
                    transition_json(&leaf, "e", Some(landing), None, BlockCase::Increment),
                    transition_json(&handler_from, "r", None, None, BlockCase::Increment),
                ];
                assert!(execute_macrostep_case(
                    reactive_machine_json(&states, &choice.root, &transitions),
                    &mut counts
                ));
            }
        }
    }
    eprintln!(
        "raises: generated={} executed={} {:?}",
        counts.generated, counts.executed, counts.runs
    );
    assert!(counts.runs.reactions > 0);
    assert!(counts.runs.discards > 0);
}

#[test]
fn enumerate_raise_and_eventless_orderings() {
    // The trigger raises `r` and lands in `landing`, whose eventless
    // transition and whose handler for `r` are both ready: eventless
    // reactions go first, then the queue, and the handler fires wherever the
    // eventless move left the machine — the one shape that tells the two
    // orders apart.
    let mut counts = SuiteCounts::default();
    let mut refused = 0u64;
    for (named, choice, leaf, names) in small_trees() {
        let states = emit_states(&named, &choice, &Decorations::default(), None);
        for landing in &names {
            for target in &names {
                for handler in &names {
                    let transitions = vec![
                        transition_json(&leaf, "e", Some(landing), None, BlockCase::Raise),
                        eventless_transition_json(
                            landing,
                            Some(target),
                            None,
                            BlockCase::Increment,
                        ),
                        transition_json(handler, "r", None, None, BlockCase::Emit),
                    ];
                    let src = reactive_machine_json(&states, &choice.root, &transitions);
                    if !execute_macrostep_case(src, &mut counts) {
                        refused += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "orderings: generated={} executed={} refused={refused} {:?}",
        counts.generated, counts.executed, counts.runs
    );
    assert!(counts.runs.reactions > 0);
    assert!(
        counts.runs.discards > 0,
        "some handler is out of reach after the eventless move"
    );
    assert!(counts.runs.effects > 0, "some handler fires");
}

#[test]
fn enumerate_final_states_and_done_events() {
    // Every compound's non-initial leaf child becomes final in turn; the
    // trigger enters it, and a handler on the compound or an ancestor — or
    // no handler at all — takes the done event.
    let mut counts = SuiteCounts::default();
    let mut generated_finals = 0u64;
    for (named, choice, leaf, names) in small_trees() {
        let mut compounds = Vec::new();
        collect_compounds(&named, &mut compounds);
        for compound in &compounds {
            let node = find_named(&named, compound).expect("compound exists");
            let initial = choice.children.get(compound).expect("compound initial");
            for child in node
                .kids
                .iter()
                .filter(|k| k.kids.is_empty() && &k.name != initial)
            {
                let states = mark_final(
                    &emit_states(&named, &choice, &Decorations::default(), None),
                    &child.name,
                );
                generated_finals += 1;
                let done = format!("$done.state.{compound}");
                let mut handler_sources: Vec<String> = vec![compound.clone()];
                let mut cursor = compound.clone();
                while let Some(parent) = names.iter().find(|n| {
                    find_named(&named, n).is_some_and(|p| p.kids.iter().any(|k| k.name == cursor))
                }) {
                    handler_sources.push(parent.clone());
                    cursor = parent.clone();
                }
                for handler in &handler_sources {
                    for target in names.iter().map(|t| Some(t.as_str())).chain([None]) {
                        if target == Some(child.name.as_str()) {
                            continue;
                        }
                        let transitions = vec![
                            transition_json(
                                &leaf,
                                "e",
                                Some(&child.name),
                                None,
                                BlockCase::Increment,
                            ),
                            transition_json(handler, &done, target, None, BlockCase::Increment),
                        ];
                        assert!(execute_macrostep_case(
                            reactive_machine_json(&states, &choice.root, &transitions),
                            &mut counts
                        ));
                    }
                }
                let transitions = vec![transition_json(
                    &leaf,
                    "e",
                    Some(&child.name),
                    None,
                    BlockCase::Increment,
                )];
                assert!(execute_macrostep_case(
                    reactive_machine_json(&states, &choice.root, &transitions),
                    &mut counts
                ));
            }
        }
    }
    eprintln!(
        "finals: generated={} executed={} finals={generated_finals} {:?}",
        counts.generated, counts.executed, counts.runs
    );
    assert!(generated_finals > 0);
    assert!(counts.runs.reactions > 0);
}

#[test]
fn the_ceiling_is_a_specified_number_in_both_implementations() {
    // A counted self-loop runs exactly `k` reactions: 64 is accepted, 65 is
    // `run/microstep_limit`, in the engine and in the oracle alike.
    for (k, trips) in [(63u32, false), (64, false), (65, true)] {
        let mut counts = SuiteCounts::default();
        let src = format!(
            r#"{{"format":"fsm.machine/1","name":"ceiling","states":[{{"name":"a"}},{{"name":"b"}}],"initial":"a","context":[{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}}],"transitions":[{{"from":"a","on":"e","to":"b"}},{{"from":"b","if":"ctx.n < {k}","do":[{{"target":"n","value":"ctx.n + 1"}}]}}]}}"#
        );
        assert!(execute_macrostep_case(src, &mut counts));
        assert_eq!(counts.runs.microstep_limits > 0, trips, "k = {k}");
        if !trips {
            // Every applied `e` ran exactly k reactions.
            assert_eq!(
                counts.runs.reactions,
                u64::from(k) * counts.runs.applied,
                "k = {k}"
            );
        }
        eprintln!("ceiling k={k}: {:?}", counts.runs);
    }
}

#[test]
fn fork_and_join_agree_across_regions() {
    let mut counts = SuiteCounts::default();
    let sources = [
        // A finished region joins the other in the same macrostep.
        r#"{"format":"fsm.machine/1","name":"fj","regions":[{"name":"a","states":[{"name":"a_work"},{"name":"a_done","terminal":true}],"initial":"a_work"},{"name":"b","states":[{"name":"waiting"},{"name":"proceed"},{"name":"b_done","terminal":true}],"initial":"waiting"}],"context":[{"name":"joined","ty":"bool","init":"false"}],"events":[{"name":"finish_a","fields":[]},{"name":"finish_b","fields":[]}],"transitions":[{"from":"a_work","on":"finish_a","to":"a_done"},{"from":"waiting","on":"$done.region.a","to":"proceed","do":[{"target":"joined","value":"true"}]},{"from":"proceed","on":"finish_b","to":"b_done"}]}"#,
        // One event with a handler in two regions: one winner, in document order.
        r#"{"format":"fsm.machine/1","name":"two","regions":[{"name":"x","states":[{"name":"x0"},{"name":"x_done","terminal":true}],"initial":"x0"},{"name":"y","states":[{"name":"y0"},{"name":"y_done","terminal":true}],"initial":"y0"},{"name":"z","states":[{"name":"z0"},{"name":"z_lost"},{"name":"z1"}],"initial":"z0"}],"context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"y0","on":"go","to":"y_done"},{"from":"x0","on":"$done.region.y","to":"x_done"},{"from":"z0","on":"$done.region.y","to":"z_lost"},{"from":"z0","on":"$done.region.x","to":"z1"}]}"#,
        // An eventless reaction in one region raises for the other.
        r#"{"format":"fsm.machine/1","name":"cross","regions":[{"name":"p","states":[{"name":"p0"},{"name":"p1"},{"name":"p2"}],"initial":"p0"},{"name":"q","states":[{"name":"q0"},{"name":"q1"}],"initial":"q0"}],"context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"ping","fields":[],"internal":true}],"transitions":[{"from":"p0","on":"go","to":"p1"},{"from":"p1","to":"p2","raise":[{"event":"ping"}]},{"from":"q0","on":"ping","to":"q1","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#,
    ];
    for src in sources {
        assert!(execute_macrostep_case(src.to_string(), &mut counts));
    }
    assert!(counts.runs.reactions >= 3);
}
