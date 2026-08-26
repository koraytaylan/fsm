use std::collections::BTreeMap;

use fsm_core::analyze::{EventStatus, enabled_events};
use fsm_core::expr::eval::Budget;
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, InstanceState};
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

fn case() -> (fsm_core::machine::CompiledMachine, Tree) {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

#[test]
fn docs_review_and_risk_review() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };
    let mut b = Budget::new(4096);
    match step(
        &m,
        &t,
        &st,
        "docs_ok",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut b,
    ) {
        Outcome::Applied(a) => {
            st.configuration = a.configuration_after;
            st.ctx = a.ctx_after;
            st.history = a.history_after;
            st.deadlines = a.deadlines_after;
        }
        o => panic!("{o:?}"),
    }
    assert!(matches!(
        st.configuration,
        ActiveConfiguration::Sequential { ref leaf } if leaf == "docs_review"
    ));
    let mut b = Budget::new(4096);
    let rep = enabled_events(&m, &t, &st, &mut b);
    assert_eq!(rep.len(), m.spec.events.len());
    let get = |n: &str| rep.iter().find(|r| r.event == n).unwrap();
    assert_eq!(get("docs_ok").status, EventStatus::Enabled);
    assert_eq!(get("scored").status, EventStatus::Disabled);
    assert!(get("scored").candidates.is_empty());
    assert_eq!(get("suspend").status, EventStatus::Enabled);
    assert_eq!(get("withdraw").status, EventStatus::Enabled);
    assert_eq!(get("note_added").status, EventStatus::Enabled);
    assert_eq!(get("resume").status, EventStatus::Disabled);

    let mut b = Budget::new(4096);
    match step(
        &m,
        &t,
        &st,
        "docs_ok",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut b,
    ) {
        Outcome::Applied(a) => {
            st.configuration = a.configuration_after;
            st.ctx = a.ctx_after;
            st.deadlines = a.deadlines_after;
        }
        o => panic!("{o:?}"),
    }
    let mut b = Budget::new(4096);
    let rep = enabled_events(&m, &t, &st, &mut b);
    let scored = rep.iter().find(|r| r.event == "scored").unwrap();
    assert_eq!(scored.status, EventStatus::DependsOnPayload);
    assert_eq!(scored.payload_fields, ["score"]);
    assert!(
        scored
            .candidates
            .iter()
            .any(|c| c.truth == EventStatus::PreemptedMaybe)
    );
}

// Plan 0009 task 4703: `enabled_events` keeps its exact meaning and never
// lists what a caller cannot send.

fn reactive_case() -> (fsm_core::machine::CompiledMachine, Tree) {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"b"},{"name":"c"},{"name":"end","terminal":true}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true},{"name":"stop","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","on":"tick","to":"c"},{"from":"c","if":"ctx.n > 0","to":"end"},{"from":"c","on":"stop","to":"end"}]}"#;
    let spec = fsm_core::spec::parse_machine(
        &fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap(),
    )
    .unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn state_at(m: &fsm_core::machine::CompiledMachine, t: &Tree, leaf: &str) -> InstanceState {
    let created = create(m, t, &BTreeMap::new(), 0).unwrap();
    InstanceState {
        status: created.status_after,
        configuration: fsm_core::machine::ActiveConfiguration::Sequential { leaf: leaf.into() },
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    }
}

#[test]
fn internal_and_generated_events_never_appear() {
    let (m, t) = reactive_case();
    for leaf in ["a", "b", "c"] {
        let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
        let reports = enabled_events(&m, &t, &state_at(&m, &t, leaf), &mut budget);
        let names: Vec<&str> = reports.iter().map(|r| r.event.as_str()).collect();
        assert!(!names.contains(&"tick"), "{leaf}: {names:?}");
        assert!(
            names.iter().all(|n| !n.starts_with('$')),
            "{leaf}: {names:?}"
        );
        assert_eq!(
            names,
            ["go", "stop"],
            "the sendable events, in declaration order"
        );
    }
}

#[test]
fn a_state_whose_only_exit_is_eventless_reports_no_enabled_event() {
    // In `b`, only the internal `tick` selects anything: nothing a caller
    // can send is enabled, and the eventless analysis is where the exit shows.
    let (m, t) = reactive_case();
    let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    let reports = enabled_events(&m, &t, &state_at(&m, &t, "b"), &mut budget);
    assert!(
        reports.iter().all(|r| r.status == EventStatus::Disabled),
        "{reports:?}"
    );
    let summary = fsm_core::analyze::reactive_summary(&m, &t);
    assert_eq!(summary.eventless_transitions, 1);
    assert_eq!(summary.internal_events, ["tick"]);
}

#[test]
fn the_scan_keeps_the_standard_budget() {
    let (m, t) = reactive_case();
    let mut budget = Budget::new(fsm_core::limits::MAX_EVAL_TICKS);
    let _ = enabled_events(&m, &t, &state_at(&m, &t, "c"), &mut budget);
    assert!(budget.remaining() > 0);
    assert!(
        fsm_core::limits::MAX_EVAL_TICKS - budget.remaining() <= 8,
        "a handful of ticks for two events"
    );
}
