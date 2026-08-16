use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::{domain_hash, state_hash};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{InstanceState, Status};
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

#[test]
fn traces_and_hashes() {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut b = Budget::new(4096);
    let a = match step(&m, &t, &st, "docs_ok", &Value::Obj(BTreeMap::new()), &mut b) {
        Outcome::Applied(a) => a,
        o => panic!("{o:?}"),
    };
    let v = a.trace.to_value();
    let bytes = canon_bytes(&v);
    assert!(!bytes.is_empty());
    assert!(std::str::from_utf8(&bytes).unwrap().contains("candidates"));
    // not_considered grouping exists on scored from risk after two docs_ok
    st.leaf = a.leaf_after;
    st.ctx = a.ctx_after;
    st.history = a.history_after;
    let mut b = Budget::new(4096);
    let a = match step(&m, &t, &st, "docs_ok", &Value::Obj(BTreeMap::new()), &mut b) {
        Outcome::Applied(a) => a,
        o => panic!("{o:?}"),
    };
    st.leaf = a.leaf_after;
    st.ctx = a.ctx_after;
    let mut payload = BTreeMap::new();
    payload.insert("score".into(), Value::Str("800".into()));
    let mut b = Budget::new(4096);
    let a = match step(&m, &t, &st, "scored", &Value::Obj(payload), &mut b) {
        Outcome::Applied(a) => a,
        o => panic!("{o:?}"),
    };
    let rendered = std::str::from_utf8(&canon_bytes(&a.trace.to_value()))
        .unwrap()
        .to_string();
    assert!(rendered.contains("not_considered") || rendered.contains("candidates"));

    let mut st1 = InstanceState {
        status: Status::Running,
        leaf: "a".into(),
        ctx: BTreeMap::from([("x".into(), Val::Int(1))]),
        history: BTreeMap::from([("c".into(), "l".into())]),
        pending: vec!["p1".into()],
    };
    let h1 = state_hash("mid", "iid", 1, &st1);
    st1.leaf = "b".into();
    let h2 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h2);
    st1.leaf = "a".into();
    st1.ctx.insert("x".into(), Val::Int(2));
    let h3 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h3);
    st1.ctx.insert("x".into(), Val::Int(1));
    st1.history.insert("c".into(), "z".into());
    let h4 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h4);
    st1.history.insert("c".into(), "l".into());
    st1.pending = vec!["p2".into()];
    let h5 = state_hash("mid", "iid", 1, &st1);
    assert_ne!(h1, h5);
    st1.pending = vec!["p1".into()];
    let h6 = state_hash("mid", "iid", 2, &st1);
    assert_ne!(h1, h6);
    assert_eq!(
        h1,
        state_hash(
            "mid",
            "iid",
            1,
            &InstanceState {
                status: Status::Running,
                leaf: "a".into(),
                ctx: BTreeMap::from([("x".into(), Val::Int(1))]),
                history: BTreeMap::from([("c".into(), "l".into())]),
                pending: vec!["p1".into()],
            }
        )
    );
    let v = parse(b"{}", &JsonLimits::DEFAULT).unwrap();
    assert_ne!(
        h1,
        format!(
            "sha256:{}",
            fsm_core::sha256::to_hex(&domain_hash("fsm:machine:1", &v))
        )
    );
}
