use fsm_core::expr::eval::Budget;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::InstanceState;
use fsm_core::spec::{compile, load_machine_json, parse_machine};
use fsm_core::step::{Outcome, step, validate_event};
use fsm_core::tree::Tree;
use std::collections::BTreeMap;

fn case() -> (fsm_core::machine::CompiledMachine, Tree) {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    (m, t)
}

fn obj(pairs: &[(&str, &str)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    Value::Obj(m)
}

#[test]
fn validate_event_codes() {
    let (m, _) = case();
    assert_eq!(
        validate_event(&m, "nope", &Value::Obj(BTreeMap::new()))
            .unwrap_err()
            .code,
        "req/event_unknown"
    );
    assert_eq!(
        validate_event(&m, "scored", &Value::Obj(BTreeMap::new()))
            .unwrap_err()
            .code,
        "req/field_missing"
    );
    let mut extra = BTreeMap::new();
    extra.insert("score".into(), Value::Str("1".into()));
    extra.insert("x".into(), Value::Str("1".into()));
    assert_eq!(
        validate_event(&m, "scored", &Value::Obj(extra))
            .unwrap_err()
            .code,
        "req/field_unknown"
    );
    let mut num = BTreeMap::new();
    num.insert("score".into(), Value::Num("1".into()));
    assert_eq!(
        validate_event(&m, "scored", &Value::Obj(num))
            .unwrap_err()
            .code,
        "req/number_token"
    );
    let mut bad = BTreeMap::new();
    bad.insert("score".into(), Value::Str("no".into()));
    assert_eq!(
        validate_event(&m, "scored", &Value::Obj(bad))
            .unwrap_err()
            .code,
        "req/field_type"
    );
}

#[test]
fn unhandled_vs_not_enabled() {
    let (m, t) = case();
    let created = fsm_core::step::create(&m, &t, &BTreeMap::new()).unwrap();
    // go to docs_review
    let mut st = InstanceState {
        status: created.status_after,
        leaf: created.leaf_after,
        ctx: created.ctx_after,
        history: created.history_after,
        pending: vec![],
    };
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "docs_ok", &obj(&[]), &mut b) {
        Outcome::Applied(a) => {
            st.leaf = a.leaf_after;
            st.ctx = a.ctx_after;
            st.history = a.history_after;
        }
        o => panic!("{o:?}"),
    }
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "scored", &obj(&[("score", "1")]), &mut b) {
        Outcome::Rejected(r) => assert_eq!(r.code, "run/unhandled"),
        o => panic!("{o:?}"),
    }
}

#[test]
fn ignore_unhandled() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","on_unhandled":"ignore","context":[],"events":[{"name":"e","fields":[]},{"name":"z","fields":[]}],"transitions":[{"from":"a","on":"e"}]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let c = fsm_core::step::create(&m, &t, &BTreeMap::new()).unwrap();
    let st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut b = Budget::new(64);
    assert!(matches!(
        step(&m, &t, &st, "z", &obj(&[]), &mut b),
        Outcome::Ignored
    ));
}

#[test]
fn not_enabled_false_guard() {
    let src = r#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","if":"false","to":"a"}]}"#;
    let spec = parse_machine(&parse(src.as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    let c = fsm_core::step::create(&m, &t, &BTreeMap::new()).unwrap();
    let st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut b = Budget::new(64);
    match step(&m, &t, &st, "e", &obj(&[]), &mut b) {
        Outcome::Rejected(r) => assert_eq!(r.code, "run/not_enabled"),
        o => panic!("{o:?}"),
    }
}
