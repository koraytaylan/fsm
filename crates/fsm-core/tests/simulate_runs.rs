use std::collections::BTreeMap;

use fsm_core::json::Value;
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

#[test]
fn accept_and_history_paths() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let events = vec![
        ev("docs_ok", &[]),
        ev("docs_ok", &[]),
        ev("scored", &[("score", "800")]),
    ];
    let r = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Stop);
    assert_eq!(r.final_leaf, "approved");
    assert!(r.terminal);
    assert_eq!(r.steps[0].leaf_after, "docs_review");
    assert_eq!(r.steps[1].leaf_after, "risk_review");
    assert_eq!(r.steps[2].leaf_after, "approved");

    let events = vec![
        ev("docs_ok", &[]),
        ev("docs_ok", &[]),
        ev("suspend", &[]),
        ev("resume", &[]),
    ];
    let r = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Stop);
    assert_eq!(r.final_leaf, "risk_review");
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
    let stop = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Stop);
    assert_eq!(stop.stopped_at, Some(1));
    assert_eq!(stop.steps.len(), 2);
    let cont = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Continue);
    assert!(cont.stopped_at.is_none());
    assert_eq!(cont.final_leaf, "risk_review");

    let r2 = simulate(&m, &t, &BTreeMap::new(), &events, OnReject::Continue);
    assert_eq!(cont.final_leaf, r2.final_leaf);
}
