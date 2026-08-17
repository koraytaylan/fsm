use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::json::Value;
use fsm_core::machine::{InstanceState, Status};
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

fn case() -> (fsm_core::machine::CompiledMachine, Tree) {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::build(&m.spec.states);
    (m, t)
}

fn empty() -> Value {
    Value::Obj(BTreeMap::new())
}

fn scored(n: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("score".into(), Value::Str(n.into()));
    Value::Obj(m)
}

fn apply(
    m: &fsm_core::machine::CompiledMachine,
    t: &Tree,
    st: &mut InstanceState,
    ev: &str,
    p: &Value,
) -> fsm_core::step::Applied {
    let mut b = Budget::new(4096);
    match step(m, t, st, ev, p, &mut b) {
        Outcome::Applied(a) => {
            st.leaf = a.leaf_after.clone();
            st.ctx = a.ctx_after.clone();
            st.history = a.history_after.clone();
            st.status = a.status_after;
            a
        }
        o => panic!("{ev} {o:?}"),
    }
}

#[test]
fn walkthrough_suspend_resume() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    apply(&m, &t, &mut st, "docs_ok", &empty());
    apply(&m, &t, &mut st, "docs_ok", &empty());
    assert_eq!(st.leaf, "risk_review");
    let sus = apply(&m, &t, &mut st, "suspend", &empty());
    assert_eq!(sus.exited, ["risk_review", "in_review"]);
    assert_eq!(sus.entered, ["suspended"]);
    assert_eq!(st.ctx.get("notes").unwrap().canonical_string(), "0");
    assert_eq!(
        st.history.get("in_review").map(String::as_str),
        Some("risk_review")
    );
    assert!(sus.effects.is_empty());
    let res = apply(&m, &t, &mut st, "resume", &empty());
    assert_eq!(res.entered, ["in_review", "risk_review"]);
    assert_eq!(st.ctx.get("visits").unwrap().canonical_string(), "2");
    assert_eq!(st.ctx.get("score").unwrap().canonical_string(), "0");
    assert_eq!(st.leaf, "risk_review");
}

#[test]
fn internal_note_added() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    apply(&m, &t, &mut st, "docs_ok", &empty());
    let visits = st.ctx.get("visits").unwrap().clone();
    let leaf = st.leaf.clone();
    let hist = st.history.clone();
    let a = apply(&m, &t, &mut st, "note_added", &{
        let mut m = BTreeMap::new();
        m.insert("text".into(), Value::Str("hi".into()));
        Value::Obj(m)
    });
    assert!(a.internal);
    assert_eq!(st.leaf, leaf);
    assert_eq!(st.history, hist);
    assert_eq!(st.ctx.get("visits"), Some(&visits));
    assert_eq!(st.ctx.get("notes").unwrap().canonical_string(), "1");
}

#[test]
fn scored_completes() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    apply(&m, &t, &mut st, "docs_ok", &empty());
    apply(&m, &t, &mut st, "docs_ok", &empty());
    let a = apply(&m, &t, &mut st, "scored", &scored("700"));
    assert_eq!(a.leaf_after, "approved");
    assert_eq!(a.status_after, Status::Completed);
    let mut b = Budget::new(64);
    match step(&m, &t, &st, "suspend", &empty(), &mut b) {
        Outcome::Rejected(r) => assert_eq!(r.code, "run/instance_completed"),
        o => panic!("{o:?}"),
    }
    st.status = Status::Cancelled;
    let mut b = Budget::new(64);
    match step(&m, &t, &st, "suspend", &empty(), &mut b) {
        Outcome::Rejected(r) => assert_eq!(r.code, "run/instance_cancelled"),
        o => panic!("{o:?}"),
    }
}

#[test]
fn monitor_does_not_block() {
    let src = br#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"x","value":"-1"}]}],"invariants":[{"name":"pos","expr":"ctx.x >= 0","mode":"monitor"}]}"#;
    let spec = fsm_core::spec::parse_machine(
        &fsm_core::json::parse(src, &fsm_core::json::JsonLimits::DEFAULT).unwrap(),
    )
    .unwrap();
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
    let a = apply(&m, &t, &mut st, "e", &empty());
    assert_eq!(a.monitor_flags, ["pos"]);
}

fn compile_src(src: &str) -> (fsm_core::machine::CompiledMachine, Tree) {
    let v = fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap();
    let m = fsm_core::spec::compile_accepted(&v).unwrap();
    let t = Tree::build(&m.spec.states);
    (m, t)
}

fn inst(m: &fsm_core::machine::CompiledMachine, t: &Tree) -> InstanceState {
    let c = create(m, t, &BTreeMap::new()).unwrap();
    InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    }
}

#[test]
fn block_overflow_is_action_error() {
    let exit = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a","exit":{"do":[{"target":"x","value":"ctx.x + 1"}]}},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b"}]}"#;
    let (m, t) = compile_src(exit);
    let st = inst(&m, &t);
    let pre = st.clone();
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            assert_eq!(r.block.as_deref(), Some("exit(a)"));
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.ctx, pre.ctx);
    assert_eq!(st.leaf, pre.leaf);

    let trans = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"x","value":"ctx.x + 1"}]}]}"#;
    let (m, t) = compile_src(trans);
    let st = inst(&m, &t);
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            assert_eq!(r.block.as_deref(), Some("transition"));
        }
        o => panic!("{o:?}"),
    }

    let entry = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"},{"name":"b","terminal":true,"entry":{"do":[{"target":"x","value":"ctx.x + 1"}]}}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b"}]}"#;
    let (m, t) = compile_src(entry);
    let st = inst(&m, &t);
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            assert_eq!(r.block.as_deref(), Some("entry(b)"));
            assert!(r.trace.pipeline.iter().any(|p| p.discarded));
        }
        o => panic!("{o:?}"),
    }
}
