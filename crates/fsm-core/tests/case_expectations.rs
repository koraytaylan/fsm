//! Comparing an expectation against a run.
//!
//! Plan 0018 task 8403. Two tests here carry the weight and they are a pair:
//! a `configuration` written in a different order must **not** diverge and
//! `effects` written in a different order **must**. The asymmetry is
//! deliberate — a configuration is a set and an emission order is a sequence —
//! and a single test could not pin it, because either rule alone looks like a
//! reasonable choice.

use std::collections::BTreeMap;

use fsm_core::cases::expect::{Rule, checked, diverge, passes};
use fsm_core::cases::format::{Case, Expect, Step};
use fsm_core::cases::run::{CaseRun, run_case};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::CompiledMachine;
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

const CASE_REVIEW: &str = include_str!("fixtures/machines/case_review.json");

/// Two regions so a configuration has more than one leaf, and two effects
/// emitted in a fixed order so the order rule has something to bite on.
const TWO_OF_EACH: &str = r#"{
  "format":"fsm.machine/1","name":"twin",
  "regions":[
    {"name":"left","initial":"l0","states":[{"name":"l0","entry":{"emit":[{"effect":"first","args":{}}]}},{"name":"l1"}]},
    {"name":"right","initial":"r0","states":[{"name":"r0","entry":{"emit":[{"effect":"second","args":{}}]}},{"name":"r1"}]}
  ],
  "context":[],
  "effects":[{"name":"first","fields":[]},{"name":"second","fields":[]}],
  "events":[{"name":"a","fields":[]},{"name":"b","fields":[]}],
  "transitions":[{"from":"l0","on":"a","to":"l1"},{"from":"r0","on":"b","to":"r1"}]
}"#;

/// A decimal context slot, so scale can differ without the value differing.
const PRICED: &str = r#"{
  "format":"fsm.machine/1","name":"priced","initial":"open",
  "context":[{"name":"total","ty":{"decimal":"2"},"init":"10.00"},{"name":"count","ty":"int","init":"3"}],
  "events":[{"name":"close","fields":[]}],
  "states":[{"name":"open"},{"name":"shut","terminal":true}],
  "transitions":[{"from":"open","on":"close","to":"shut"}]
}"#;

fn compiled(source: &str) -> (CompiledMachine, Tree) {
    let value = parse(source.as_bytes(), &JsonLimits::DEFAULT).expect("the machine parses");
    let machine = compile_accepted(&value).expect("the machine compiles");
    let tree = Tree::for_machine(&machine.spec);
    (machine, tree)
}

fn send(event: &str) -> Step {
    Step::Send {
        event: event.into(),
        payload: Value::Obj(BTreeMap::new()),
    }
}

fn run(source: &str, script: Vec<Step>) -> CaseRun {
    let (machine, tree) = compiled(source);
    run_case(
        &machine,
        &tree,
        &Case {
            name: "c".into(),
            context: BTreeMap::new(),
            script,
            expect: Expect::default(),
        },
    )
    .expect("the case runs")
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn a_fully_matching_expectation_produces_no_divergences() {
    let observed = run(CASE_REVIEW, vec![send("docs_ok")]);
    let expect = Expect {
        configuration: Some(strings(&["docs_review"])),
        context: Some(BTreeMap::from([
            ("visits".into(), "1".into()),
            ("notes".into(), "0".into()),
            ("score".into(), "0".into()),
        ])),
        enabled: Some(strings(&["docs_ok", "note_added", "suspend", "withdraw"])),
        effects: Some(strings(&["notify"])),
        terminal: Some(false),
    };
    assert_eq!(diverge(&expect, &observed), Vec::new());
    assert!(passes(&expect, &observed));
    assert_eq!(checked(&expect).len(), 5);
}

#[test]
fn one_wrong_configuration_leaf_is_one_divergence_carrying_both_values() {
    let observed = run(CASE_REVIEW, vec![send("docs_ok")]);
    let expect = Expect {
        configuration: Some(strings(&["risk_review"])),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].field, "configuration");
    assert_eq!(found[0].expected, "risk_review");
    assert_eq!(found[0].found, "docs_review");
    assert_eq!(found[0].rule, Rule::Set);
}

#[test]
fn a_configuration_in_a_different_order_does_not_diverge() {
    // Half of the pair. A configuration is a set of active leaves, and the
    // order it was written in is an artefact of writing it down.
    let observed = run(TWO_OF_EACH, vec![send("a"), send("b")]);
    for order in [["l1", "r1"], ["r1", "l1"]] {
        let expect = Expect {
            configuration: Some(strings(&order)),
            ..Expect::default()
        };
        assert_eq!(
            diverge(&expect, &observed),
            Vec::new(),
            "configuration compared as a sequence: {order:?}"
        );
    }
}

#[test]
fn effects_in_a_different_order_do_diverge() {
    // The other half. Emission order is deterministic and an executor runs
    // them in it, so a case that pins the order is pinning something real.
    let observed = run(TWO_OF_EACH, vec![]);
    assert_eq!(observed.final_pending, ["first", "second"]);

    let right = Expect {
        effects: Some(strings(&["first", "second"])),
        ..Expect::default()
    };
    assert_eq!(diverge(&right, &observed), Vec::new());

    let swapped = Expect {
        effects: Some(strings(&["second", "first"])),
        ..Expect::default()
    };
    let found = diverge(&swapped, &observed);
    assert_eq!(found.len(), 1, "effects compared as a set");
    assert_eq!(found[0].field, "effects");
    assert_eq!(found[0].rule, Rule::Ordered);
    assert_eq!(found[0].expected, "second, first");
    assert_eq!(found[0].found, "first, second");
}

#[test]
fn a_set_compared_field_ignores_a_repeated_entry() {
    // "Compares as a set" has to mean it on **both** sides. Reporting
    // `[x, x]` against an observed `[x]` compares a multiset while claiming to
    // compare a set — and a `supersedes` mapping that merges two old leaves
    // onto one new state produces exactly that shape.
    let observed = run(TWO_OF_EACH, vec![send("a"), send("b")]);
    let repeated = Expect {
        configuration: Some(strings(&["l1", "r1", "l1"])),
        enabled: Some(strings(&[])),
        ..Expect::default()
    };
    assert_eq!(
        diverge(&repeated, &observed),
        Vec::new(),
        "a repeated leaf in a set-compared field was reported as a difference"
    );
}

#[test]
fn an_enabled_set_is_order_insensitive() {
    let observed = run(TWO_OF_EACH, vec![]);
    for order in [["a", "b"], ["b", "a"]] {
        let expect = Expect {
            enabled: Some(strings(&order)),
            ..Expect::default()
        };
        assert_eq!(
            diverge(&expect, &observed),
            Vec::new(),
            "enabled compared as a sequence: {order:?}"
        );
    }
}

#[test]
fn one_differing_context_key_is_one_divergence_naming_that_key() {
    let observed = run(CASE_REVIEW, vec![send("docs_ok")]);
    let expect = Expect {
        context: Some(BTreeMap::from([
            ("visits".into(), "9".into()),
            ("score".into(), "0".into()),
        ])),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(found.len(), 1, "the whole map was reported: {found:?}");
    assert_eq!(found[0].field, "context");
    assert_eq!(found[0].key.as_deref(), Some("visits"));
    assert_eq!(found[0].expected, "9");
    assert_eq!(found[0].found, "1");
    assert_eq!(found[0].rule, Rule::Keyed);
}

#[test]
fn a_decimal_of_a_different_scale_diverges_and_the_message_shows_both() {
    // A scale is part of a value in this engine — exact arithmetic is the
    // reason — so a comparison that coerced `10.0` into `10.00` would swallow
    // exactly the change a case exists to catch.
    let observed = run(PRICED, vec![]);
    let expect = Expect {
        context: Some(BTreeMap::from([("total".into(), "10.0".into())])),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(found.len(), 1, "a scale difference compared equal");
    assert_eq!(found[0].expected, "10.0");
    assert_eq!(found[0].found, "10.00");

    // And the same value at the same scale does not diverge.
    let exact = Expect {
        context: Some(BTreeMap::from([("total".into(), "10.00".into())])),
        ..Expect::default()
    };
    assert_eq!(diverge(&exact, &observed), Vec::new());
}

#[test]
fn an_absent_field_asserts_nothing_whatever_the_run_holds() {
    let observed = run(CASE_REVIEW, vec![send("docs_ok")]);
    assert_eq!(diverge(&Expect::default(), &observed), Vec::new());
    assert!(passes(&Expect::default(), &observed));
    assert!(checked(&Expect::default()).is_empty());

    // Naming one field asserts one field and nothing beside it: the run has a
    // pending effect and a non-empty context, and neither is checked here.
    let one = Expect {
        configuration: Some(strings(&["docs_review"])),
        ..Expect::default()
    };
    assert_eq!(diverge(&one, &observed), Vec::new());
    assert!(!observed.final_pending.is_empty());
}

#[test]
fn three_divergences_are_all_reported_each_with_its_step_index() {
    let observed = run(CASE_REVIEW, vec![send("docs_ok"), send("docs_ok")]);
    let expect = Expect {
        configuration: Some(strings(&["approved"])),
        context: Some(BTreeMap::from([("visits".into(), "7".into())])),
        terminal: Some(true),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(found.len(), 3, "{found:?}");
    let fields: Vec<&str> = found.iter().map(|d| d.field).collect();
    assert_eq!(fields, ["configuration", "context", "terminal"]);
    for divergence in &found {
        assert_eq!(
            divergence.step,
            Some(observed.steps.len() - 1),
            "a final-state divergence does not name where the script ended"
        );
    }
}

#[test]
fn terminal_expected_true_against_a_running_instance_names_the_field() {
    let observed = run(CASE_REVIEW, vec![send("docs_ok")]);
    let expect = Expect {
        terminal: Some(true),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].field, "terminal");
    assert_eq!(found[0].expected, "true");
    assert_eq!(found[0].found, "false");
    assert_eq!(found[0].rule, Rule::Scalar);
}

#[test]
fn a_step_that_could_not_run_is_reported_before_any_expectation() {
    // The case failed for a reason that has nothing to do with what it
    // expected, and leading with "configuration differs" sends the author to
    // the wrong half of the file.
    let (machine, tree) = compiled(CASE_REVIEW);
    let observed = run_case(
        &machine,
        &tree,
        &Case {
            name: "c".into(),
            context: BTreeMap::new(),
            script: vec![
                send("docs_ok"),
                Step::Ack {
                    effect: "nosuch".into(),
                    outcome: fsm_core::cases::format::AckOutcome::Ok,
                    result: None,
                },
            ],
            expect: Expect::default(),
        },
    )
    .expect("the case runs");
    let expect = Expect {
        configuration: Some(strings(&["approved"])),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(found[0].field, "script");
    assert_eq!(found[0].step, Some(1), "the refusal does not name its step");
    assert_eq!(found[0].rule, Rule::Script);
    // `expected` is always "the step runs" and `found` is always the reason.
    // Two shapes here made every consumer pick a half, and the delta report
    // picked the one that dropped the effect's name.
    assert_eq!(found[0].expected, "the step runs");
    assert!(
        found[0].found.contains("nosuch") && found[0].found.contains("notify"),
        "the reason does not name the effect or list what was pending: {:?}",
        found[0]
    );
    assert_eq!(found[1].field, "configuration");
}

/// A deadline transition whose entry action breaks an `enforce` invariant, so
/// the engine rejects the poll atomically.
const POLL_IS_REJECTED: &str = r#"{
  "format":"fsm.machine/1","name":"guarded","initial":"a",
  "context":[{"name":"n","ty":"int","init":"0"}],
  "events":[{"name":"noop","fields":[]}],
  "states":[{"name":"a"},{"name":"b","entry":{"do":[{"target":"n","value":"-1"}]}}],
  "deadlines":[{"name":"tick","from":"a","after":"dur(30, s)","to":"b"}],
  "transitions":[],
  "invariants":[{"name":"nonneg","expr":"ctx.n >= 0","mode":"enforce"}]
}"#;

#[test]
fn a_poll_the_engine_rejected_is_a_divergence_rather_than_a_silent_pass() {
    // The case this format exists to prevent, reached from the other side: the
    // poll was refused, the configuration therefore matched what the author
    // wrote, and the case reported `ok` while asserting nothing about a script
    // the machine would not run.
    let observed = run(POLL_IS_REJECTED, vec![Step::Poll { now_ms: 30_000 }]);
    let expect = Expect {
        configuration: Some(strings(&["a"])),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(
        found.len(),
        1,
        "a rejected poll was dropped and the case passed: {found:?}"
    );
    assert_eq!(found[0].field, "script");
    assert_eq!(found[0].rule, Rule::Script);
    assert_eq!(found[0].step, Some(0));
    assert!(!passes(&expect, &observed));
}

#[test]
fn a_divergence_is_data_rather_than_prose() {
    // Constructed directly, with no formatting step anywhere: this is what
    // lets the human output and `--json` agree by construction instead of by
    // two formatters staying in step.
    let observed = run(CASE_REVIEW, vec![send("docs_ok")]);
    let expect = Expect {
        terminal: Some(true),
        ..Expect::default()
    };
    let divergence = diverge(&expect, &observed).remove(0);
    assert_eq!(divergence.field, "terminal");
    assert_eq!(divergence.key, None);
    // Every field is a value a renderer chooses how to show; none of them is a
    // sentence the core already wrote.
    assert!(!divergence.expected.contains(' '));
    assert!(!divergence.found.contains(' '));
}

#[test]
fn a_case_with_no_script_names_no_step() {
    // `steps.len() - 1` saturated to zero, so a scriptless case reported a
    // divergence "at step 0" — a step that does not exist.
    let observed = run(CASE_REVIEW, vec![]);
    let expect = Expect {
        configuration: Some(strings(&["approved"])),
        ..Expect::default()
    };
    let found = diverge(&expect, &observed);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].step, None, "a scriptless case pointed at a step");
}
