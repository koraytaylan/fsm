//! The pure migration: seven steps in a fixed order, every refusal atomic.
//!
//! Plan 0011 task 5401.

// A helper hands back the engine's own `Rejection`, which is what a refused
// migration *is*. Boxing it would only make every assertion dereference to
// read a code.
#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::migrate::apply::{Migrated, migrate};
use fsm_core::spec::compile_accepted;
use fsm_core::tree::Tree;

fn value(source: &str) -> Value {
    parse(source.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn compiled(source: &str) -> (CompiledMachine, Tree) {
    let machine = compile_accepted(&value(source)).unwrap_or_else(|e| panic!("{e:?}"));
    let tree = Tree::for_machine(&machine.spec);
    (machine, tree)
}

fn digest(source: &str) -> String {
    fsm_core::hashes::digest_of(&fsm_core::hashes::machine_id(&value(source)))
        .unwrap()
        .to_string()
}

/// The definition an instance is on: two leaves, an int and a decimal.
const OLD: &str = r#"{"format":"fsm.machine/1","name":"v1","states":[{"name":"intake"},{"name":"triage"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"3"},{"name":"legacy","ty":"str","init":"kept"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"intake","on":"go","to":"triage"}]}"#;

/// A corrected definition with whatever mapping and shape the case needs.
fn new_with(states: &str, context: &str, extra_states: &str, extra: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"v2","states":[{{"name":"intake"}},{{"name":"triage"}}{extra_states}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}},{{"name":"fresh","ty":"str","init":"default"}}],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}}]{extra},"supersedes":{{"machine":"{}","states":{states},"context":{context}}}}}"#,
        digest(OLD)
    )
}

fn running(leaf: &str) -> InstanceState {
    InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential { leaf: leaf.into() },
        ctx: BTreeMap::from([
            ("score".to_string(), Val::Int(3)),
            ("legacy".to_string(), Val::Str("kept".into())),
        ]),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn run(new_source: &str, state: &InstanceState) -> Result<Migrated, fsm_core::step::Rejection> {
    let (old, _) = compiled(OLD);
    let (new, tree) = compiled(new_source);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    migrate(&old, &new, &tree, state, 5_000, &mut budget)
}

#[test]
fn a_mapped_leaf_migrates_with_its_projected_context() {
    let source = new_with(
        r#"{"intake":"triage"}"#,
        r#"{"score":"ctx.score + 10"}"#,
        "",
        "",
    );
    let migrated = run(&source, &running("intake")).expect("a mapped leaf migrates");
    assert_eq!(
        migrated.state.configuration,
        ActiveConfiguration::Sequential {
            leaf: "triage".into()
        }
    );
    assert_eq!(migrated.state.ctx["score"], Val::Int(13));
    // An unmapped new variable takes its declared init.
    assert_eq!(migrated.state.ctx["fresh"], Val::Str("default".into()));
    // An old variable the new definition does not declare is dropped.
    assert!(!migrated.state.ctx.contains_key("legacy"));
    assert_eq!(migrated.report.defaulted, ["fresh"]);
    assert_eq!(migrated.report.dropped, ["legacy"]);
    assert_eq!(
        migrated.report.leaves,
        [(String::new(), "intake".to_string(), "triage".to_string())]
    );
    assert_eq!(migrated.state.status, Status::Running);
}

#[test]
fn an_unmapped_leaf_refuses_the_whole_migration() {
    let source = new_with(r#"{"triage":"triage"}"#, "{}", "", "");
    let before = running("intake");
    let rejection = run(&source, &before).expect_err("no entry for intake");
    assert_eq!(rejection.code, "req/migrate_unmapped");
    assert!(
        rejection.message.contains("intake"),
        "{}",
        rejection.message
    );
    assert_eq!(before, running("intake"), "the input state is untouched");
}

#[test]
fn a_parallel_instance_needs_every_region_mapped() {
    let old_parallel = r#"{"format":"fsm.machine/1","name":"p1","regions":[{"name":"left","states":[{"name":"a"},{"name":"a2"}],"initial":"a"},{"name":"right","states":[{"name":"b"},{"name":"b2"}],"initial":"b"}],"context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"a2"}]}"#;
    let mapped = |states: &str| {
        format!(
            r#"{{"format":"fsm.machine/1","name":"p2","regions":[{{"name":"left","states":[{{"name":"a"}},{{"name":"a2"}}],"initial":"a"}},{{"name":"right","states":[{{"name":"b"}},{{"name":"b2"}}],"initial":"b"}}],"context":[],"events":[{{"name":"go","fields":[]}}],"transitions":[{{"from":"a","on":"go","to":"a2"}}],"supersedes":{{"machine":"{}","states":{states},"context":{{}}}}}}"#,
            digest(old_parallel)
        )
    };
    let state = InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Parallel {
            leaves: BTreeMap::from([
                ("left".to_string(), "a".to_string()),
                ("right".to_string(), "b".to_string()),
            ]),
        },
        ctx: BTreeMap::new(),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let (old, _) = compiled(old_parallel);
    let both = mapped(r#"{"a":"a2","b":"b2"}"#);
    let (new, tree) = compiled(&both);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let migrated = migrate(&old, &new, &tree, &state, 5_000, &mut budget).expect("both mapped");
    assert_eq!(
        migrated.state.configuration,
        ActiveConfiguration::Parallel {
            leaves: BTreeMap::from([
                ("left".to_string(), "a2".to_string()),
                ("right".to_string(), "b2".to_string()),
            ])
        }
    );

    let one = mapped(r#"{"a":"a2"}"#);
    let (new, tree) = compiled(&one);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection = migrate(&old, &new, &tree, &state, 5_000, &mut budget)
        .expect_err("the right region is unmapped");
    assert_eq!(rejection.code, "req/migrate_unmapped");
    assert!(rejection.message.contains("right"), "{}", rejection.message);
}

#[test]
fn a_settled_instance_has_nothing_to_migrate() {
    let source = new_with(r#"{"intake":"triage"}"#, "{}", "", "");
    for status in [Status::Completed, Status::Cancelled] {
        let mut state = running("intake");
        state.status = status;
        let rejection = run(&source, &state).expect_err("settled");
        assert_eq!(rejection.code, "req/migrate_settled");
        assert!(
            rejection.message.contains(status.as_str()),
            "{}",
            rejection.message
        );
    }
}

#[test]
fn a_projection_that_overflows_names_the_migration_block() {
    let source = new_with(
        r#"{"intake":"triage"}"#,
        r#"{"score":"ctx.score * 9223372036854775807"}"#,
        "",
        "",
    );
    let rejection = run(&source, &running("intake")).expect_err("overflow");
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.block.as_deref(), Some("migration"));
    assert_eq!(rejection.cause, Some("run/overflow"));
}

#[test]
fn the_new_definitions_invariants_gate_the_migration() {
    let enforce = r#","invariants":[{"name":"small","expr":"ctx.score < 5","mode":"enforce"}]"#;
    let source = new_with(
        r#"{"intake":"triage"}"#,
        r#"{"score":"ctx.score + 10"}"#,
        "",
        enforce,
    );
    let rejection = run(&source, &running("intake")).expect_err("13 is not small");
    assert_eq!(rejection.code, "run/invariant");
    assert!(rejection.message.contains("small"), "{}", rejection.message);

    // A monitor failure is reported and does not block.
    let monitor = r#","invariants":[{"name":"small","expr":"ctx.score < 5","mode":"monitor"}]"#;
    let source = new_with(
        r#"{"intake":"triage"}"#,
        r#"{"score":"ctx.score + 10"}"#,
        "",
        monitor,
    );
    let migrated = run(&source, &running("intake")).expect("a monitor does not block");
    assert_eq!(migrated.report.monitor_flags, ["small"]);
}

#[test]
fn a_migrated_instance_runs_its_reaction_phase() {
    // The mapped leaf has a guardless eventless exit: a migrated instance
    // parked there would be sitting in a state its own machine says it
    // should have left.
    let source = new_with(
        r#"{"intake":"triage"}"#,
        "{}",
        r#",{"name":"settled"}"#,
        r#""#,
    )
    .replace(
        r#"{"from":"intake","on":"go","to":"triage"}"#,
        r#"{"from":"intake","on":"go","to":"triage"},{"from":"triage","to":"settled"}"#,
    );
    let migrated = run(&source, &running("intake")).expect("the reaction runs");
    assert_eq!(
        migrated.state.configuration,
        ActiveConfiguration::Sequential {
            leaf: "settled".into()
        },
        "the eventless exit ran"
    );
    assert_eq!(migrated.report.microsteps.len(), 1);

    // And a reaction that reaches a terminal leaf completes the instance.
    let terminal = source.replace(
        r#"{"name":"settled"}"#,
        r#"{"name":"settled","terminal":true}"#,
    );
    let migrated = run(&terminal, &running("intake")).expect("the reaction runs");
    assert_eq!(migrated.state.status, Status::Completed);
}

#[test]
fn a_reaction_that_cannot_settle_rejects_the_whole_migration() {
    // Two guarded eventless transitions whose guards never turn false: a
    // definition admission accepts (an unguarded cycle is refused outright),
    // and the macrostep ceiling is what stops it.
    let source = new_with(r#"{"intake":"triage"}"#, "{}", r#",{"name":"other"}"#, "")
        .replace(
            r#"{"from":"intake","on":"go","to":"triage"}"#,
            r#"{"from":"intake","on":"go","to":"triage"},{"from":"triage","if":"ctx.score >= 0","to":"other"},{"from":"other","if":"ctx.score >= 0","to":"triage"}"#,
        );
    let before = running("intake");
    let rejection = run(&source, &before).expect_err("it never settles");
    assert_eq!(rejection.code, "run/microstep_limit");
    assert_eq!(before, running("intake"));
}

#[test]
fn a_definition_that_supersedes_nothing_cannot_be_migrated_onto() {
    let (old, _) = compiled(OLD);
    let (new, tree) = compiled(OLD);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let rejection =
        migrate(&old, &new, &tree, &running("intake"), 5_000, &mut budget).expect_err("no mapping");
    assert_eq!(rejection.code, "req/migrate_not_superseded");
}

#[test]
fn the_same_inputs_produce_the_same_migration() {
    let source = new_with(
        r#"{"intake":"triage"}"#,
        r#"{"score":"ctx.score + 1"}"#,
        "",
        "",
    );
    let first = run(&source, &running("intake")).unwrap();
    let second = run(&source, &running("intake")).unwrap();
    assert_eq!(first, second, "a pure function of its inputs");
}

#[test]
fn a_projection_refuses_rather_than_looping_when_the_budget_is_gone() {
    let source = new_with(
        r#"{"intake":"triage"}"#,
        r#"{"score":"ctx.score + 1"}"#,
        "",
        "",
    );
    let (old, _) = compiled(OLD);
    let (new, tree) = compiled(&source);
    let mut budget = Budget::new(1);
    let rejection = migrate(&old, &new, &tree, &running("intake"), 5_000, &mut budget)
        .expect_err("the budget is spent");
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.cause, Some("internal/budget"));
}
