use std::collections::BTreeMap;

use fsm_core::analyze::{EventStatus, enabled_events};
use fsm_core::expr::eval::Budget;
use fsm_core::json::Value;
use fsm_core::machine::InstanceState;
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

fn case() -> (fsm_core::machine::CompiledMachine, Tree) {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    (m, t)
}

#[test]
fn docs_review_and_risk_review() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "docs_ok", &Value::Obj(BTreeMap::new()), &mut b) {
        Outcome::Applied(a) => {
            st.leaf = a.leaf_after;
            st.ctx = a.ctx_after;
            st.history = a.history_after;
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.leaf, "docs_review");
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
    match step(&m, &t, &st, "docs_ok", &Value::Obj(BTreeMap::new()), &mut b) {
        Outcome::Applied(a) => {
            st.leaf = a.leaf_after;
            st.ctx = a.ctx_after;
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
