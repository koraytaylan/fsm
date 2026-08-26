//! A preview answers "what would this do" before anything is written, and
//! agrees with the apply because both are the same attempt read two ways.
//!
//! Plan 0011 task 5403.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::migrate::apply::migrate;
use fsm_core::migrate::preview::{MigrationPreview, preview, preview_all};
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

const OLD: &str = r#"{"format":"fsm.machine/1","name":"v1","states":[{"name":"intake"},{"name":"triage"},{"name":"awaiting_countersign"}],"initial":"intake","context":[{"name":"score","ty":"int","init":"3"},{"name":"legacy","ty":"str","init":"kept"}],"events":[{"name":"go","fields":[]}],"deadlines":[{"name":"old_timer","from":"intake","after":"dur(60, s)","to":"triage"}],"transitions":[{"from":"intake","on":"go","to":"triage"}]}"#;

fn new_with(states: &str, extra_states: &str, extra_transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"v2","states":[{{"name":"intake"}},{{"name":"triage"}}{extra_states}],"initial":"intake","context":[{{"name":"score","ty":"int","init":"0"}},{{"name":"fresh","ty":"str","init":"default"}}],"events":[{{"name":"go","fields":[]}}],"deadlines":[{{"name":"new_timer","from":"triage","after":"dur(30, s)","to":"intake"}}],"transitions":[{{"from":"intake","on":"go","to":"triage"}}{extra_transitions}],"supersedes":{{"machine":"{}","states":{states},"context":{{"score":"ctx.score + 1"}}}}}}"#,
        digest(OLD)
    )
}

const MAPPED: &str = r#"{"intake":"intake","triage":"triage"}"#;

fn instance(leaf: &str) -> InstanceState {
    InstanceState {
        status: Status::Running,
        configuration: ActiveConfiguration::Sequential { leaf: leaf.into() },
        ctx: BTreeMap::from([
            ("score".to_string(), Val::Int(3)),
            ("legacy".to_string(), Val::Str("kept".into())),
        ]),
        history: BTreeMap::new(),
        deadlines: BTreeMap::from([("old_timer".to_string(), 9_000)]),
        pending: vec!["inst-1/3/0".to_string()],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

fn look(source: &str, state: &InstanceState) -> MigrationPreview {
    let (old, _) = compiled(OLD);
    let (new, tree) = compiled(source);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    preview(&old, &new, &tree, state, 7_000, &mut budget)
}

#[test]
fn a_clean_instance_previews_its_whole_migration() {
    let outcome = look(&new_with(MAPPED, "", ""), &instance("intake"));
    assert!(outcome.clean());
    assert_eq!(
        outcome.mapped_configuration,
        Some(ActiveConfiguration::Sequential {
            leaf: "intake".into()
        })
    );
    assert_eq!(outcome.settled_configuration, outcome.mapped_configuration);
    // Before and after, per variable, including the one being dropped.
    let seen: BTreeMap<&str, (Option<&str>, Option<&str>)> = outcome
        .context
        .iter()
        .map(|(name, before, after)| (name.as_str(), (before.as_deref(), after.as_deref())))
        .collect();
    assert_eq!(seen["score"], (Some("3"), Some("4")));
    assert_eq!(seen["fresh"], (None, Some("default")));
    assert_eq!(seen["legacy"], (Some("kept"), None), "a drop is visible");
    assert_eq!(outcome.report.retained_effects, ["inst-1/3/0"]);
}

#[test]
fn a_refusal_is_returned_rather_than_raised() {
    let unmapped = look(
        &new_with(r#"{"triage":"triage"}"#, "", ""),
        &instance("intake"),
    );
    let refusal = unmapped.refusal.as_ref().expect("this one cannot migrate");
    assert_eq!(refusal.code, "req/migrate_unmapped");
    assert!(!unmapped.clean());
    assert!(
        unmapped.settled_configuration.is_none(),
        "it never lands anywhere"
    );

    let mut settled = instance("intake");
    settled.status = Status::Completed;
    let done = look(&new_with(MAPPED, "", ""), &settled);
    assert_eq!(
        done.refusal.as_ref().map(|rejection| rejection.code),
        Some("req/migrate_settled")
    );
}

#[test]
fn the_preview_reports_what_the_apply_would_do_to_the_clock() {
    let outcome = look(&new_with(MAPPED, "", ""), &instance("triage"));
    let mut reported = outcome.report.rescheduled_deadlines.clone();
    reported.sort();
    assert_eq!(
        reported,
        [
            ("new_timer".to_string(), None, Some(37_000)),
            ("old_timer".to_string(), Some(9_000), None),
        ]
    );
    // And the apply at the same now_ms produces the same numbers.
    let (old, _) = compiled(OLD);
    let source = new_with(MAPPED, "", "");
    let (new, tree) = compiled(&source);
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let applied = migrate(&old, &new, &tree, &instance("triage"), 7_000, &mut budget).unwrap();
    assert_eq!(
        applied.report.rescheduled_deadlines,
        outcome.report.rescheduled_deadlines
    );
}

#[test]
fn a_reaction_is_predicted_not_skipped() {
    // The mapped leaf has a guardless eventless exit: a preview that stopped
    // before the reaction would report a leaf the migration never lands on.
    let source = new_with(
        MAPPED,
        r#",{"name":"settled"}"#,
        r#",{"from":"triage","to":"settled"}"#,
    );
    let outcome = look(&source, &instance("triage"));
    assert_eq!(
        outcome.mapped_configuration,
        Some(ActiveConfiguration::Sequential {
            leaf: "triage".into()
        })
    );
    assert_eq!(
        outcome.settled_configuration,
        Some(ActiveConfiguration::Sequential {
            leaf: "settled".into()
        }),
        "the preview reports where it actually lands"
    );
    assert_eq!(outcome.report.microsteps.len(), 1);
}

#[test]
fn preview_and_apply_agree_on_every_instance() {
    let source = new_with(MAPPED, "", "");
    let (old, _) = compiled(OLD);
    let (new, tree) = compiled(&source);
    // A spread of instances: mapped, unmapped, settled, and one whose
    // projection is fine but whose leaf is not.
    let mut cases = Vec::new();
    for leaf in ["intake", "triage", "awaiting_countersign"] {
        for status in [Status::Running, Status::Completed, Status::Cancelled] {
            let mut state = instance(leaf);
            state.status = status;
            cases.push(state);
        }
    }
    for state in &cases {
        let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
        let looked = preview(&old, &new, &tree, state, 7_000, &mut budget);
        let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
        let applied = migrate(&old, &new, &tree, state, 7_000, &mut budget);
        match (looked.refusal, applied) {
            (None, Ok(migrated)) => {
                assert_eq!(migrated.report, looked.report, "the same report");
                assert_eq!(
                    Some(migrated.state.configuration),
                    looked.settled_configuration,
                    "the same landing"
                );
            }
            (Some(expected), Err(actual)) => assert_eq!(expected.code, actual.code),
            (looked, applied) => panic!(
                "preview and apply disagree: {:?} vs {:?}",
                looked.map(|r| r.code),
                applied.map(|m| m.state.status)
            ),
        }
    }
}

#[test]
fn a_cohort_is_grouped_by_outcome_biggest_first() {
    let source = new_with(MAPPED, "", "");
    let (old, _) = compiled(OLD);
    let (new, tree) = compiled(&source);
    let mut cohort = Vec::new();
    for index in 0..5 {
        cohort.push((format!("clean-{index}"), instance("intake")));
    }
    for index in 0..2 {
        cohort.push((format!("stuck-{index}"), instance("awaiting_countersign")));
    }
    let mut settled = instance("intake");
    settled.status = Status::Cancelled;
    cohort.push(("settled-0".to_string(), settled));

    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let groups = preview_all(&old, &new, &tree, &cohort, 7_000, &mut budget);
    assert_eq!(groups.len(), 3);
    assert_eq!(groups[0].code, None);
    assert_eq!(groups[0].count, 5, "the biggest cohort reads first");
    assert_eq!(groups[1].code, Some("req/migrate_unmapped"));
    assert_eq!(groups[1].count, 2);
    assert!(
        groups[1].detail.contains("awaiting_countersign"),
        "an operator sees which state blocks them: {}",
        groups[1].detail
    );
    assert_eq!(groups[2].code, Some("req/migrate_settled"));

    // Stable across two runs.
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    let again = preview_all(&old, &new, &tree, &cohort, 7_000, &mut budget);
    assert_eq!(groups, again);
}

#[test]
fn a_preview_changes_nothing_it_was_given() {
    let source = new_with(MAPPED, "", "");
    let before = instance("intake");
    let after = before.clone();
    let outcome = look(&source, &before);
    assert!(outcome.clean());
    assert_eq!(before, after, "a preview is a pure function of its inputs");
}
