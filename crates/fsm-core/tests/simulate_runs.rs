use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_core::machine::ActiveConfiguration;
use fsm_core::simulate::{OnReject, simulate};
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::tree::Tree;

fn ev(name: &str, fields: &[(&str, &str)]) -> (String, Value) {
    let mut m = BTreeMap::new();
    for (k, v) in fields {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    (name.into(), Value::Obj(m))
}

fn leaf(configuration: &ActiveConfiguration) -> &str {
    match configuration {
        ActiveConfiguration::Sequential { leaf } => leaf,
        ActiveConfiguration::Parallel { .. } => panic!("expected sequential configuration"),
    }
}

#[test]
fn accept_and_history_paths() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
    let events = vec![
        ev("docs_ok", &[]),
        ev("docs_ok", &[]),
        ev("scored", &[("score", "800")]),
    ];
    let r = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Stop).unwrap();
    assert_eq!(leaf(&r.final_configuration), "approved");
    assert!(r.terminal);
    assert_eq!(leaf(&r.steps[0].configuration_after), "docs_review");
    assert_eq!(leaf(&r.steps[1].configuration_after), "risk_review");
    assert_eq!(leaf(&r.steps[2].configuration_after), "approved");

    let events = vec![
        ev("docs_ok", &[]),
        ev("docs_ok", &[]),
        ev("suspend", &[]),
        ev("resume", &[]),
    ];
    let r = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Stop).unwrap();
    assert_eq!(leaf(&r.final_configuration), "risk_review");
    assert_eq!(
        r.steps
            .last()
            .unwrap()
            .ctx_after
            .get("visits")
            .unwrap()
            .canonical_string(),
        "2"
    );

    let events = vec![
        ev("docs_ok", &[]),
        ev("scored", &[("score", "1")]),
        ev("docs_ok", &[]),
    ];
    let stop = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Stop).unwrap();
    assert_eq!(stop.stopped_at, Some(1));
    assert_eq!(stop.steps.len(), 2);
    let cont = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Continue).unwrap();
    assert!(cont.stopped_at.is_none());
    assert_eq!(leaf(&cont.final_configuration), "risk_review");

    let r2 = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Continue).unwrap();
    assert_eq!(cont.final_configuration, r2.final_configuration);
}

#[test]
fn parallel_creation_failure_is_a_typed_error_without_a_report() {
    let spec = load_machine_json(
        br#"{
          "format":"fsm.machine/1",
          "name":"parallel_create_failure",
          "regions":[
            {"name":"left","states":[{"name":"left_ready"}],"initial":"left_ready"},
            {"name":"right","states":[{"name":"right_ready"}],"initial":"right_ready"}
          ],
          "context":[],
          "events":[],
          "transitions":[],
          "invariants":[{"name":"never","expr":"false","mode":"enforce"}]
        }"#,
    )
    .unwrap();
    let machine = compile(spec).unwrap();
    let tree = Tree::for_machine(&machine.spec);

    let rejection = simulate(&machine, &tree, &BTreeMap::new(), &[], OnReject::Stop).unwrap_err();

    assert_eq!(rejection.code, "run/create_failed");
}

// Plan 0009 task 4703: simulate runs macrosteps like every other entry point
// and shows the cascade each event caused.

#[test]
fn a_simulated_event_reports_the_cascade_it_caused() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"idle"},{"name":"working","entry":{"raise":[{"event":"tick"}]}},{"name":"ticked"},{"name":"settled"}],"initial":"idle","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"tick","fields":[],"internal":true}],"transitions":[{"from":"idle","on":"go","to":"working"},{"from":"working","on":"tick","to":"ticked","do":[{"target":"n","value":"ctx.n + 1"}]},{"from":"ticked","to":"settled"}]}"#;
    let compiled = fsm_core::spec::compile_accepted(
        &fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap(),
    )
    .unwrap();
    let tree = Tree::for_machine(&compiled.spec);
    let report = simulate(
        &compiled,
        &tree,
        &BTreeMap::new(),
        &[ev("go", &[])],
        OnReject::Stop,
    )
    .unwrap();
    let step = &report.steps[0];
    assert_eq!(leaf(&step.configuration_after), "settled");
    let fsm_core::step::Outcome::Applied(applied) = &step.outcome else {
        panic!("{:?}", step.outcome);
    };
    let triggers: Vec<String> = applied
        .trace
        .microsteps
        .iter()
        .map(|m| format!("{:?}", m.trigger))
        .collect();
    assert_eq!(triggers, ["Internal(\"tick\")", "Eventless"]);
    // The same as a real step would produce.
    let created = fsm_core::step::create(&compiled, &tree, &BTreeMap::new(), 0).unwrap();
    let state = fsm_core::machine::InstanceState {
        status: created.status_after,
        configuration: created.configuration_after,
        ctx: created.ctx_after,
        history: created.history_after,
        deadlines: created.deadlines_after,
        pending: Vec::new(),
    };
    let mut budget = fsm_core::expr::eval::Budget::new(fsm_core::limits::MACROSTEP_EVAL_TICKS);
    match fsm_core::step::step(
        &compiled,
        &tree,
        &state,
        "go",
        &Value::Obj(BTreeMap::new()),
        0,
        &mut budget,
    ) {
        fsm_core::step::Outcome::Applied(real) => {
            assert_eq!(real.configuration_after, step.configuration_after);
            assert_eq!(real.ctx_after, step.ctx_after);
            assert_eq!(real.trace.microsteps, applied.trace.microsteps);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_simulated_macrostep_that_hits_the_ceiling_reports_it_and_honours_on_reject() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"b"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"nudge","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","if":"ctx.n >= 0","to":"b"},{"from":"a","on":"nudge","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
    let compiled = fsm_core::spec::compile_accepted(
        &fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap(),
    )
    .unwrap();
    let tree = Tree::for_machine(&compiled.spec);
    let events = [ev("go", &[]), ev("nudge", &[])];
    let stopped = simulate(&compiled, &tree, &BTreeMap::new(), &events, OnReject::Stop).unwrap();
    assert_eq!(stopped.stopped_at, Some(0));
    let fsm_core::step::Outcome::Rejected(rejection) = &stopped.steps[0].outcome else {
        panic!("{:?}", stopped.steps[0].outcome);
    };
    assert_eq!(rejection.code, "run/microstep_limit");
    assert_eq!(
        leaf(&stopped.steps[0].configuration_after),
        "a",
        "rejected atomically"
    );
    let continued = simulate(
        &compiled,
        &tree,
        &BTreeMap::new(),
        &events,
        OnReject::Continue,
    )
    .unwrap();
    assert_eq!(continued.stopped_at, None);
    assert_eq!(continued.steps.len(), 2);
    assert!(matches!(
        continued.steps[1].outcome,
        fsm_core::step::Outcome::Applied(_)
    ));
    assert_eq!(continued.steps[1].ctx_after["n"].canonical_string(), "1");
}

#[test]
fn simulate_polls_no_deadline_for_a_reactive_machine_either() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"},{"name":"b"},{"name":"c"},{"name":"late"}],"initial":"a","context":[],"events":[{"name":"go","fields":[]}],"deadlines":[{"name":"expire","from":"c","after":"dur(1, ms)","to":"late"}],"transitions":[{"from":"a","on":"go","to":"b"},{"from":"b","to":"c"}]}"#;
    let compiled = fsm_core::spec::compile_accepted(
        &fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap(),
    )
    .unwrap();
    let tree = Tree::for_machine(&compiled.spec);
    let report = simulate(
        &compiled,
        &tree,
        &BTreeMap::new(),
        &[ev("go", &[]), ev("go", &[])],
        OnReject::Continue,
    )
    .unwrap();
    assert_eq!(
        leaf(&report.final_configuration),
        "c",
        "the deadline never fired"
    );
    assert!(
        matches!(
            report.steps[1].outcome,
            fsm_core::step::Outcome::Rejected(_)
        ),
        "c has no go handler"
    );
}
