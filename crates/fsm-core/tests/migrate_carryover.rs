//! The five carry-over rulings: what an instance keeps, what it loses, and
//! what refuses the migration outright.
//!
//! Plan 0011 task 5402.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::{digest_of, machine_id};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{
    ActiveConfiguration, CompiledMachine, InstanceState, Invocation, InvokeStatus, PendingSignal,
    Status,
};
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
    digest_of(&machine_id(&value(source))).unwrap().to_string()
}

/// A child machine an invoke slot can name.
const CHILD: &str = r#"{"format":"fsm.machine/1","name":"child","states":[{"name":"w"},{"name":"d","terminal":true}],"initial":"w","context":[],"events":[{"name":"f","fields":[]}],"transitions":[{"from":"w","on":"f","to":"d"}]}"#;

/// The old definition: a compound with history, a deadline, and a slot.
fn old_source() -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"v1","states":[{{"name":"box","initial":"one","states":[{{"name":"one","invoke":[{{"id":"check","machine":"{child}"}}]}},{{"name":"two"}},{{"name":"h","history":"shallow"}}]}},{{"name":"elsewhere"}}],"initial":"box","context":[{{"name":"wait","ty":"int","init":"60"}}],"events":[{{"name":"go","fields":[]}}],"deadlines":[{{"name":"old_timer","from":"one","after":"dur(ctx.wait, s)","to":"two"}}],"transitions":[{{"from":"one","on":"go","to":"two"}}]}}"#,
        child = digest(CHILD)
    )
}

/// The new definition, with whatever this case needs spliced in.
fn new_source(states: &str, slot: &str, deadline: &str, extra_ctx: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"v2","states":[{{"name":"box","initial":"uno","states":[{{"name":"uno"{slot}}},{{"name":"dos"}},{{"name":"h","history":"shallow"}}]}},{{"name":"elsewhere"}}],"initial":"box","context":[{{"name":"wait","ty":"int","init":"60"}}{extra_ctx}],"events":[{{"name":"go","fields":[]}}],{deadline}"transitions":[{{"from":"uno","on":"go","to":"dos"}}],"supersedes":{{"machine":"{old}","states":{states},"context":{{"wait":"ctx.wait"}}}}}}"#,
        old = digest(&old_source())
    )
}

const MAPPED: &str = r#"{"one":"uno","two":"dos","box":"box","h":"h","elsewhere":"elsewhere"}"#;
const NEW_TIMER: &str =
    r#""deadlines":[{"name":"new_timer","from":"uno","after":"dur(ctx.wait * 2, s)","to":"dos"}],"#;

fn instance() -> InstanceState {
    InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential { leaf: "one".into() },
        ctx: BTreeMap::from([("wait".to_string(), Val::Int(60))]),
        history: BTreeMap::from([("box".to_string(), "two".to_string())]),
        deadlines: BTreeMap::from([("old_timer".to_string(), 9_000)]),
        pending: vec!["inst-1/3/0".to_string()],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn run(new: &str, state: &InstanceState) -> Result<Migrated, fsm_core::step::Rejection> {
    let (old_machine, _) = compiled(&old_source());
    let (new_machine, tree) = compiled(new);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    migrate(&old_machine, &new_machine, &tree, state, 5_000, &mut budget)
}

#[test]
fn history_is_remapped_when_both_ends_are_mapped_and_dropped_otherwise() {
    let migrated = run(&new_source(MAPPED, "", NEW_TIMER, ""), &instance()).unwrap();
    assert_eq!(
        migrated.state.history,
        BTreeMap::from([("box".to_string(), "dos".to_string())])
    );
    assert!(migrated.report.dropped_history.is_empty());

    // The child is unmapped: the binding is dropped, not refused.
    let child_unmapped = r#"{"one":"uno","box":"box","h":"h","elsewhere":"elsewhere"}"#;
    let migrated = run(&new_source(child_unmapped, "", NEW_TIMER, ""), &instance()).unwrap();
    assert!(migrated.state.history.is_empty());
    assert_eq!(migrated.report.dropped_history, ["box/two"]);

    // And the owner unmapped, likewise.
    let owner_unmapped = r#"{"one":"uno","two":"dos","h":"h","elsewhere":"elsewhere"}"#;
    let migrated = run(&new_source(owner_unmapped, "", NEW_TIMER, ""), &instance()).unwrap();
    assert!(migrated.state.history.is_empty());
    assert_eq!(migrated.report.dropped_history, ["box/two"]);
}

#[test]
fn migration_restarts_the_clock_on_every_timer() {
    let migrated = run(&new_source(MAPPED, "", NEW_TIMER, ""), &instance()).unwrap();
    // The old schedule is gone; the new machine's is computed from now_ms.
    assert!(!migrated.state.deadlines.contains_key("old_timer"));
    assert_eq!(migrated.state.deadlines["new_timer"], 5_000 + 120_000);
    let mut reported = migrated.report.rescheduled_deadlines.clone();
    reported.sort();
    assert_eq!(
        reported,
        [
            ("new_timer".to_string(), None, Some(125_000)),
            ("old_timer".to_string(), Some(9_000), None),
        ],
        "both halves of the clock restart are visible"
    );
}

#[test]
fn a_state_with_no_deadline_in_the_new_machine_ends_with_none() {
    let migrated = run(&new_source(MAPPED, "", "", ""), &instance()).unwrap();
    assert!(migrated.state.deadlines.is_empty());
    assert_eq!(
        migrated.report.rescheduled_deadlines,
        [("old_timer".to_string(), Some(9_000), None)]
    );
}

#[test]
fn a_deadline_expression_that_overflows_refuses() {
    let overflowing = r#""deadlines":[{"name":"new_timer","from":"uno","after":"dur(ctx.wait * 9223372036854775807, s)","to":"dos"}],"#;
    let rejection = run(&new_source(MAPPED, "", overflowing, ""), &instance())
        .expect_err("the new after expression overflows");
    assert_eq!(rejection.code, "run/action_error");
    assert_eq!(rejection.cause, Some("run/overflow"));
}

#[test]
fn pending_effects_survive_byte_for_byte() {
    let migrated = run(&new_source(MAPPED, "", NEW_TIMER, ""), &instance()).unwrap();
    assert_eq!(migrated.state.pending, ["inst-1/3/0"]);
    assert_eq!(migrated.report.retained_effects, ["inst-1/3/0"]);
    // The id still names the record that emitted it, so an ack against it
    // still resolves: the emitting machine is the old one, which the
    // catalogue still holds.
    let (old_machine, _) = compiled(&old_source());
    assert_eq!(old_machine.spec.name, "v1");
}

#[test]
fn a_running_slot_carries_only_when_the_new_machine_declares_it_the_same_way() {
    let slot = format!(
        r#","invoke":[{{"id":"check","machine":"{}"}}]"#,
        digest(CHILD)
    );
    let mut state = instance();
    state.invocations.insert(
        "check".to_string(),
        Invocation {
            child_machine_id: format!("child@sha256:{}", digest(CHILD)),
            status: InvokeStatus::Running,
            overrides: BTreeMap::new(),
        },
    );
    let migrated = run(&new_source(MAPPED, &slot, NEW_TIMER, ""), &state).unwrap();
    assert_eq!(
        migrated.state.invocations["check"].status,
        InvokeStatus::Running
    );

    // The new machine does not declare it: a live child cannot be dropped.
    let rejection = run(&new_source(MAPPED, "", NEW_TIMER, ""), &state)
        .expect_err("a running child is doing work");
    assert_eq!(rejection.code, "req/migrate_slot");
    assert!(rejection.message.contains("check"), "{}", rejection.message);

    // A different child machine is the same refusal.
    let other = r#"{"format":"fsm.machine/1","name":"other","states":[{"name":"w"}],"initial":"w","context":[],"events":[],"transitions":[]}"#;
    let different = format!(
        r#","invoke":[{{"id":"check","machine":"{}"}}]"#,
        digest(other)
    );
    let rejection = run(&new_source(MAPPED, &different, NEW_TIMER, ""), &state)
        .expect_err("a different machine is a different invocation");
    assert_eq!(rejection.code, "req/migrate_slot");
}

#[test]
fn a_returned_slot_is_dropped_because_its_result_was_delivered() {
    let mut state = instance();
    state.invocations.insert(
        "check".to_string(),
        Invocation {
            child_machine_id: format!("child@sha256:{}", digest(CHILD)),
            status: InvokeStatus::Returned,
            overrides: BTreeMap::new(),
        },
    );
    let migrated = run(&new_source(MAPPED, "", NEW_TIMER, ""), &state).unwrap();
    assert!(migrated.state.invocations.is_empty());
    assert_eq!(migrated.report.dropped_slots, ["check"]);
}

#[test]
fn pending_signals_survive_whatever_the_new_definition_says() {
    let mut state = instance();
    state.signals.insert(
        "inst-1/4/0".to_string(),
        PendingSignal {
            // A target that no longer exists, carrying an event this machine
            // never declares: both are the *target's* business, decided at
            // delivery. This is the case a reader will assume is a bug.
            target_instance_id: "inst-gone".to_string(),
            event: "an_event_v2_never_heard_of".to_string(),
            payload: BTreeMap::from([("note".to_string(), Val::Str("kept".into()))]),
        },
    );
    let migrated = run(&new_source(MAPPED, "", NEW_TIMER, ""), &state).unwrap();
    assert_eq!(migrated.state.signals, state.signals);
    assert_eq!(migrated.report.retained_signals, ["inst-1/4/0"]);
}

#[test]
fn an_empty_instance_migrates_with_an_empty_report() {
    let bare = InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential { leaf: "one".into() },
        ctx: BTreeMap::from([("wait".to_string(), Val::Int(60))]),
        history: BTreeMap::new(),
        deadlines: BTreeMap::new(),
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let migrated = run(&new_source(MAPPED, "", "", ""), &bare).unwrap();
    let report = &migrated.report;
    assert!(report.dropped_history.is_empty());
    assert!(report.rescheduled_deadlines.is_empty());
    assert!(report.retained_effects.is_empty());
    assert!(report.retained_signals.is_empty());
    assert!(report.dropped_slots.is_empty());
}

#[test]
fn the_report_is_stable_across_two_identical_runs() {
    let source = new_source(MAPPED, "", NEW_TIMER, "");
    let first = run(&source, &instance()).unwrap();
    let second = run(&source, &instance()).unwrap();
    assert_eq!(first.report, second.report);
    assert_eq!(first.state, second.state);
}
