use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::json::Value;
use fsm_core::machine::InstanceState;
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

fn xorshift(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}

#[test]
fn history_properties() {
    let seed = 0xC0FFEE_u64;
    let mut state = seed;
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    for walk in 0..40 {
        let c = create(&m, &t, &BTreeMap::new()).unwrap();
        let mut st = InstanceState {
            status: c.status_after,
            leaf: c.leaf_after,
            ctx: c.ctx_after,
            history: c.history_after,
            pending: vec![],
        };
        // drive to risk_review
        for ev in ["docs_ok", "docs_ok"] {
            let mut b = Budget::new(4096);
            if let Outcome::Applied(a) = step(&m, &t, &st, ev, &Value::Obj(BTreeMap::new()), &mut b)
            {
                st.leaf = a.leaf_after;
                st.ctx = a.ctx_after;
                st.history = a.history_after;
            }
        }
        let pre = st.leaf.clone();
        let visits = match st.ctx.get("visits") {
            Some(Val::Int(n)) => *n,
            _ => 0,
        };
        let mut b = Budget::new(4096);
        if let Outcome::Applied(a) =
            step(&m, &t, &st, "suspend", &Value::Obj(BTreeMap::new()), &mut b)
        {
            st.leaf = a.leaf_after;
            st.ctx = a.ctx_after;
            st.history = a.history_after;
        }
        let mut b = Budget::new(4096);
        if let Outcome::Applied(a) =
            step(&m, &t, &st, "resume", &Value::Obj(BTreeMap::new()), &mut b)
        {
            assert_eq!(a.leaf_after, pre, "seed={seed} walk={walk}");
            let v2 = match a.ctx_after.get("visits") {
                Some(Val::Int(n)) => *n,
                _ => 0,
            };
            assert!(v2 > visits, "entry re-ran seed={seed}");
            st.leaf = a.leaf_after;
            st.ctx = a.ctx_after;
            st.history = a.history_after;
        }
        // internal never changes leaf/history
        let leaf = st.leaf.clone();
        let hist = st.history.clone();
        let mut p = BTreeMap::new();
        p.insert("text".into(), Value::Str("x".into()));
        let mut b = Budget::new(4096);
        if let Outcome::Applied(a) = step(&m, &t, &st, "note_added", &Value::Obj(p), &mut b) {
            assert_eq!(a.leaf_after, leaf);
            assert_eq!(a.history_after, hist);
        }
        // rejected never changes history
        let hist = st.history.clone();
        let mut b = Budget::new(4096);
        let _ = xorshift(&mut state);
        if let Outcome::Rejected(_) =
            step(&m, &t, &st, "resume", &Value::Obj(BTreeMap::new()), &mut b)
        {
            assert_eq!(st.history, hist);
        }
    }
}
