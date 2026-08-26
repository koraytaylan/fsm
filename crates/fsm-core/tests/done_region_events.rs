//! `$done.region.<region>`: the join. A parallel definition has been able to
//! fork since regions shipped and never able to notice that a branch
//! finished; this closes that with no new state concept, because a region's
//! `terminal` leaf already means exactly "this branch is over".
//!
//! Plan 0009 task 4503.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::limits::MACROSTEP_EVAL_TICKS;
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use fsm_core::spec::{Finding, compile, generated_event_names, parse_machine, validate};
use fsm_core::step::{Applied, Outcome, create, step};
use fsm_core::trace::MicrostepTrigger;
use fsm_core::tree::Tree;

fn parsed(src: &str) -> fsm_core::spec::MachineSpec {
    parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap())
        .unwrap_or_else(|e| panic!("{e:?}"))
}

fn machine(src: &str) -> (CompiledMachine, Tree) {
    let m = compile(parsed(src)).unwrap_or_else(|e| panic!("{e:?}"));
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn instance(applied: &Applied) -> InstanceState {
    InstanceState {
        status: applied.status_after,
        configuration: applied.configuration_after.clone(),
        ctx: applied.ctx_after.clone(),
        history: applied.history_after.clone(),
        deadlines: applied.deadlines_after.clone(),
        pending: Vec::new(),
    }
}

fn applied(outcome: Outcome) -> Applied {
    match outcome {
        Outcome::Applied(applied) => applied,
        other => panic!("expected Applied, got {other:?}"),
    }
}

fn triggers(out: &Applied) -> Vec<String> {
    out.trace
        .microsteps
        .iter()
        .map(|m| match &m.trigger {
            MicrostepTrigger::Eventless => "eventless".into(),
            MicrostepTrigger::Internal(event) => event.clone(),
        })
        .collect()
}

fn leaves(out: &Applied) -> BTreeMap<String, String> {
    match &out.configuration_after {
        ActiveConfiguration::Parallel { leaves } => leaves.clone(),
        other => panic!("{other:?}"),
    }
}

/// Region `a` finishes on `finish_a`; region `b` waits for it.
fn fork_join(extra_transitions: &str) -> String {
    format!(
        r#"{{"format":"fsm.machine/1","name":"fj","regions":[{{"name":"a","states":[{{"name":"a_work"}},{{"name":"a_done","terminal":true}}],"initial":"a_work"}},{{"name":"b","states":[{{"name":"waiting"}},{{"name":"proceed"}},{{"name":"b_done","terminal":true}}],"initial":"waiting"}}],"context":[{{"name":"joined","ty":"bool","init":"false"}}],"events":[{{"name":"finish_a","fields":[]}},{{"name":"finish_b","fields":[]}}],"transitions":[{{"from":"a_work","on":"finish_a","to":"a_done"}},{{"from":"waiting","on":"$done.region.a","to":"proceed","do":[{{"target":"joined","value":"true"}}]}},{{"from":"proceed","on":"finish_b","to":"b_done"}}{extra_transitions}]}}"#
    )
}

fn send(m: &CompiledMachine, t: &Tree, state: &InstanceState, event: &str) -> Outcome {
    let mut budget = Budget::new(MACROSTEP_EVAL_TICKS);
    step(
        m,
        t,
        state,
        event,
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    )
}

#[test]
fn a_finished_region_advances_the_waiting_region_in_the_same_macrostep() {
    let (m, t) = machine(&fork_join(""));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "finish_a"));
    assert_eq!(triggers(&out), ["$done.region.a"]);
    assert_eq!(out.trace.microsteps[0].region.as_deref(), Some("b"));
    assert_eq!(
        leaves(&out),
        BTreeMap::from([
            ("a".into(), "a_done".into()),
            ("b".into(), "proceed".into())
        ])
    );
    assert_eq!(out.ctx_after["joined"].canonical_string(), "true");
    assert_eq!(out.status_after, Status::Running, "b is still running");
    let done = applied(send(&m, &t, &instance(&out), "finish_b"));
    assert_eq!(done.status_after, Status::Completed);
}

#[test]
fn a_region_never_handles_its_own_done_event() {
    // `a_work` cannot be the handler (the region is terminal by then), and a
    // transition sourced in region a on its own done event is admissible
    // but inert: the scan skips the finished region, so it is discarded.
    let (m, t) = machine(&fork_join(
        r#",{"from":"a_work","on":"$done.region.a","to":"a_work"}"#,
    ));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "finish_a"));
    assert_eq!(
        triggers(&out),
        ["$done.region.a"],
        "handled by region b only"
    );
    assert_eq!(out.trace.microsteps[0].region.as_deref(), Some("b"));
    assert!(out.trace.internal_unhandled.is_empty());
}

#[test]
fn a_join_cannot_target_another_region() {
    let src = fork_join(r#",{"from":"waiting","on":"$done.region.b","to":"a_work"}"#);
    let errs = validate(&parsed(&src)).unwrap_err();
    assert!(
        errs.iter().any(|f| f.code == "def/cross_region"),
        "{errs:?}"
    );
}

#[test]
fn two_regions_finishing_in_one_macrostep_enqueue_as_they_finish() {
    // Region `y` finishes in the trigger and region `x` in the reaction to
    // y's event, so y's event precedes x's: the queue is the order the regions
    // finished in, and document order cannot reorder an event that was
    // already consumed when the next one was raised. Region `z` handles y's
    // event too, but an internal event has one winner across regions and `x`
    // precedes `z` in document order, so z only ever sees x's event.
    let src = r#"{"format":"fsm.machine/1","name":"m","regions":[{"name":"x","states":[{"name":"x0"},{"name":"x_done","terminal":true}],"initial":"x0"},{"name":"y","states":[{"name":"y0"},{"name":"y_done","terminal":true}],"initial":"y0"},{"name":"z","states":[{"name":"z0"},{"name":"z_lost"},{"name":"z1"}],"initial":"z0"}],"context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"y0","on":"go","to":"y_done"},{"from":"x0","on":"$done.region.y","to":"x_done"},{"from":"z0","on":"$done.region.y","to":"z_lost"},{"from":"z0","on":"$done.region.x","to":"z1"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "go"));
    assert_eq!(triggers(&out), ["$done.region.y", "$done.region.x"]);
    let regions: Vec<Option<&str>> = out
        .trace
        .microsteps
        .iter()
        .map(|m| m.region.as_deref())
        .collect();
    assert_eq!(
        regions,
        [Some("x"), Some("z")],
        "x wins y's event over z by document order; z then handles x's"
    );
    assert_eq!(leaves(&out)["z"], "z1");
    assert!(out.trace.internal_unhandled.is_empty());
}

#[test]
fn an_already_terminal_region_does_not_re_raise() {
    let (m, t) = machine(&fork_join(""));
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let joined = applied(send(&m, &t, &instance(&created), "finish_a"));
    let done = applied(send(&m, &t, &instance(&joined), "finish_b"));
    assert_eq!(triggers(&done), Vec::<String>::new());
    let events: Vec<&str> = done
        .trace
        .internal_unhandled
        .iter()
        .map(|u| u.event.as_str())
        .collect();
    assert_eq!(
        events,
        ["$done.region.b"],
        "only the region that just finished raised; a did not re-raise, and nothing named $done.machine exists"
    );
}

#[test]
fn every_region_finishing_at_once_generates_exactly_the_region_events() {
    let src = r#"{"format":"fsm.machine/1","name":"m","regions":[{"name":"p","states":[{"name":"p0"},{"name":"p_done","terminal":true}],"initial":"p0"},{"name":"q","states":[{"name":"q0"},{"name":"q_done","terminal":true}],"initial":"q0"}],"context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"p0","on":"go","to":"p_done"},{"from":"q0","on":"$done.region.p","to":"q_done"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "go"));
    assert_eq!(out.status_after, Status::Completed);
    assert_eq!(triggers(&out), ["$done.region.p"]);
    let discarded: Vec<&str> = out
        .trace
        .internal_unhandled
        .iter()
        .map(|u| u.event.as_str())
        .collect();
    assert_eq!(discarded, ["$done.region.q"]);
}

#[test]
fn an_unknown_region_name_lists_the_generated_names() {
    let src = fork_join(r#",{"from":"waiting","on":"$done.region.nosuch","to":"proceed"}"#);
    let errs = validate(&parsed(&src)).unwrap_err();
    let finding: &Finding = errs
        .iter()
        .find(|f| f.code == "def/unknown_event")
        .expect("unknown");
    assert!(
        finding.hint.contains("$done.region.a") && finding.hint.contains("$done.region.b"),
        "{}",
        finding.hint
    );
    assert_eq!(
        generated_event_names(&parsed(&fork_join(""))),
        ["$done.region.a", "$done.region.b"]
    );
}

#[test]
fn a_sequential_terminal_leaf_raises_nothing() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"end","terminal":true}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"end"}]}"#;
    let (m, t) = machine(src);
    let created = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let out = applied(send(&m, &t, &instance(&created), "go"));
    assert_eq!(out.status_after, Status::Completed);
    assert!(
        out.trace.internal_unhandled.is_empty(),
        "completion carries the sequential case"
    );
    assert!(generated_event_names(&m.spec).is_empty());
}
