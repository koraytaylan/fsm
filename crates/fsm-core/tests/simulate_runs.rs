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
