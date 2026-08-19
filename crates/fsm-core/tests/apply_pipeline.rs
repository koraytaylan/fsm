use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::spec::{compile, load_machine_json};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

fn case() -> (fsm_core::machine::CompiledMachine, Tree) {
    let spec = load_machine_json(include_bytes!("fixtures/machines/case_review.json")).unwrap();
    let m = compile(spec).unwrap();
    let t = Tree::for_machine(&m.spec);
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

fn leaf(configuration: &ActiveConfiguration) -> &str {
    match configuration {
        ActiveConfiguration::Sequential { leaf } => leaf,
        ActiveConfiguration::Parallel { .. } => panic!("expected sequential configuration"),
    }
}

fn apply(
    m: &fsm_core::machine::CompiledMachine,
    t: &Tree,
    st: &mut InstanceState,
    ev: &str,
    p: &Value,
) -> fsm_core::step::Applied {
    let mut b = Budget::new(4096);
    match step(m, t, st, ev, p, 0, &mut b) {
        Outcome::Applied(a) => {
            st.configuration = a.configuration_after.clone();
            st.ctx = a.ctx_after.clone();
            st.history = a.history_after.clone();
            st.deadlines = a.deadlines_after.clone();
            st.status = a.status_after;
            a
        }
        o => panic!("{ev} {o:?}"),
    }
}

#[test]
fn walkthrough_suspend_resume() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    };
    apply(&m, &t, &mut st, "docs_ok", &empty());
    apply(&m, &t, &mut st, "docs_ok", &empty());
    assert_eq!(leaf(&st.configuration), "risk_review");
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
    assert_eq!(leaf(&st.configuration), "risk_review");
}

#[test]
fn internal_note_added() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    };
    apply(&m, &t, &mut st, "docs_ok", &empty());
    let visits = st.ctx.get("visits").unwrap().clone();
    let configuration = st.configuration.clone();
    let hist = st.history.clone();
    let a = apply(&m, &t, &mut st, "note_added", &{
        let mut m = BTreeMap::new();
        m.insert("text".into(), Value::Str("hi".into()));
        Value::Obj(m)
    });
    assert!(a.internal);
    assert_eq!(st.configuration, configuration);
    assert_eq!(st.history, hist);
    assert_eq!(st.ctx.get("visits"), Some(&visits));
    assert_eq!(st.ctx.get("notes").unwrap().canonical_string(), "1");
}

#[test]
fn scored_completes() {
    let (m, t) = case();
    let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    };
    apply(&m, &t, &mut st, "docs_ok", &empty());
    apply(&m, &t, &mut st, "docs_ok", &empty());
    let a = apply(&m, &t, &mut st, "scored", &scored("700"));
    assert_eq!(leaf(&a.configuration_after), "approved");
    assert_eq!(a.status_after, Status::Completed);
    let mut b = Budget::new(64);
    match step(&m, &t, &st, "suspend", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => assert_eq!(r.code, "run/instance_completed"),
        o => panic!("{o:?}"),
    }
    st.status = Status::Cancelled;
    let mut b = Budget::new(64);
    match step(&m, &t, &st, "suspend", &empty(), 0, &mut b) {
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
    let t = Tree::for_machine(&m.spec);
    let c = create(&m, &t, &BTreeMap::new(), 0).unwrap();
    let mut st = InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
        pending: vec![],
    };
    let a = apply(&m, &t, &mut st, "e", &empty());
    assert_eq!(a.monitor_flags, ["pos"]);
}

fn compile_src(src: &str) -> (fsm_core::machine::CompiledMachine, Tree) {
    let v = fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).unwrap();
    let m = fsm_core::spec::compile_accepted(&v).unwrap();
    let t = Tree::for_machine(&m.spec);
    (m, t)
}

fn inst(m: &fsm_core::machine::CompiledMachine, t: &Tree) -> InstanceState {
    let c = create(m, t, &BTreeMap::new(), 0).unwrap();
    InstanceState {
        status: c.status_after,
        configuration: c.configuration_after,
        ctx: c.ctx_after,
        history: c.history_after,
        deadlines: c.deadlines_after,
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
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            assert_eq!(r.cause, Some("run/overflow"));
            assert_eq!(r.block.as_deref(), Some("exit(a)"));
            assert!(r.span.is_some());
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.ctx, pre.ctx);
    assert_eq!(st.configuration, pre.configuration);
    assert_eq!(st.history, pre.history);
    assert!(st.pending.is_empty());

    let trans = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"x","value":"ctx.x + 1"}]}]}"#;
    let (m, t) = compile_src(trans);
    let st = inst(&m, &t);
    let pre = st.clone();
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            assert_eq!(r.cause, Some("run/overflow"));
            assert_eq!(r.block.as_deref(), Some("transition"));
            assert!(r.span.is_some());
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.ctx, pre.ctx);
    assert_eq!(st.configuration, pre.configuration);
    assert_eq!(st.history, pre.history);

    let entry = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"0"},{"name":"y","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a","exit":{"do":[{"target":"y","value":"ctx.x + 1"}]}},{"name":"b","terminal":true,"entry":{"do":[{"target":"x","value":"9223372036854775807 + 1"}]}}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b"}]}"#;
    let (m, t) = compile_src(entry);
    let st = inst(&m, &t);
    let pre = st.clone();
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            assert_eq!(r.cause, Some("run/overflow"));
            assert_eq!(r.block.as_deref(), Some("entry(b)"));
            assert!(r.span.is_some());
            let exit = r
                .trace
                .pipeline
                .iter()
                .find(|p| matches!(p.block, fsm_core::trace::BlockKind::Exit(_)))
                .expect("completed exit");
            assert!(exit.discarded);
            assert_eq!(exit.sets[0].target, "y");
            assert_eq!(exit.sets[0].after, "1");
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.ctx, pre.ctx);
    assert_eq!(st.configuration, pre.configuration);
}

#[test]
fn pipeline_ordering_snapshot_and_effects() {
    let src = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"a","ty":"int","init":"1"},{"name":"b","ty":"int","init":"2"},{"name":"x","ty":"int","init":"0"},{"name":"seen","ty":"int","init":"0"},{"name":"seen_exit","ty":"int","init":"9"}],"events":[{"name":"go","fields":[{"name":"y","ty":"int"}]}],"effects":[{"name":"exit_fx","fields":[]},{"name":"trans_fx","fields":[]},{"name":"entry_fx","fields":[]}],"states":[{"name":"p","exit":{"do":[{"target":"seen_exit","value":"ctx.x"}],"emit":[{"effect":"exit_fx","args":{}}]}},{"name":"q","entry":{"do":[{"target":"seen","value":"ctx.x"}],"emit":[{"effect":"entry_fx","args":{}}]}}],"initial":"p","transitions":[{"from":"p","on":"go","to":"q","do":[{"target":"x","value":"evt.y"},{"target":"a","value":"ctx.b"},{"target":"b","value":"ctx.a"}],"emit":[{"effect":"trans_fx","args":{}}]}]}"#;
    let (m, t) = compile_src(src);
    let mut st = inst(&m, &t);
    let a = apply(&m, &t, &mut st, "go", &{
        let mut p = BTreeMap::new();
        p.insert("y".into(), Value::Str("9".into()));
        Value::Obj(p)
    });
    assert_eq!(st.ctx.get("x").unwrap().canonical_string(), "9");
    assert_eq!(st.ctx.get("seen").unwrap().canonical_string(), "9");
    assert_eq!(st.ctx.get("seen_exit").unwrap().canonical_string(), "0");
    assert_eq!(st.ctx.get("a").unwrap().canonical_string(), "2");
    assert_eq!(st.ctx.get("b").unwrap().canonical_string(), "1");
    assert_eq!(
        a.effects
            .iter()
            .map(|e| (e.name.as_str(), e.k))
            .collect::<Vec<_>>(),
        vec![("exit_fx", 0), ("trans_fx", 1), ("entry_fx", 2)]
    );
    assert_eq!(
        a.trace
            .pipeline
            .iter()
            .map(|p| p.block.as_label())
            .collect::<Vec<_>>(),
        vec!["exit(p)", "transition", "entry(q)"]
    );
}

#[test]
fn guard_and_invariant_atomicity() {
    let guard = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","if":"ctx.x + 1 > 0","to":"b"}]}"#;
    let (m, t) = compile_src(guard);
    let st = inst(&m, &t);
    let pre = st.clone();
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/guard_error");
            assert_eq!(r.source_state.as_deref(), Some("a"));
            assert_eq!(r.transition_idx, Some(0));
            assert!(r.span.is_some());
            assert!(!r.trace.candidates.is_empty());
            assert!(matches!(
                r.trace.candidates[0].transitions[0].guard,
                fsm_core::trace::GuardTrace::Evaluated(_)
            ));
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.ctx, pre.ctx);
    assert_eq!(st.configuration, pre.configuration);
    assert_eq!(st.history, pre.history);

    let inv = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"p","initial":"a","states":[{"name":"h","history":"deep"},{"name":"a"},{"name":"b"}]},{"name":"out","terminal":true}],"initial":"p","transitions":[{"from":"a","on":"go","to":"out","do":[{"target":"x","value":"-1"}]}],"invariants":[{"name":"pos","expr":"ctx.x >= 0","mode":"enforce"},{"name":"zero","expr":"ctx.x == 0","mode":"enforce"}]}"#;
    let (m, t) = compile_src(inv);
    let st = inst(&m, &t);
    let pre = st.clone();
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/invariant");
            let failed: Vec<_> = r
                .trace
                .invariants
                .iter()
                .filter(|i| !i.passed)
                .map(|i| i.name.as_str())
                .collect();
            assert!(failed.contains(&"pos"));
            assert!(failed.contains(&"zero"));
            assert!(r.trace.pipeline.iter().all(|p| p.discarded));
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(st.ctx, pre.ctx);
    assert_eq!(st.configuration, pre.configuration);
    assert_eq!(st.history, pre.history);
}

#[test]
fn step_invariant_eval_error_has_operands() {
    let src = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"9223372036854775806"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"}],"initial":"a","transitions":[{"from":"a","on":"go","do":[{"target":"x","value":"ctx.x + 1"}]}],"invariants":[{"name":"pos","expr":"ctx.x + 1 > 0","mode":"enforce"}]}"#;
    let (m, t) = compile_src(src);
    let st = inst(&m, &t);
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/invariant");
            let inv = r
                .trace
                .invariants
                .iter()
                .find(|i| i.name == "pos")
                .expect("pos");
            let err = inv.error.as_ref().expect("eval error");
            assert_eq!(err.code, "run/overflow");
            let node = err.expr.as_ref().expect("expr node");
            fn has_input(n: &fsm_core::expr::eval::TraceNode, want: &str) -> bool {
                let here = match &n.outcome {
                    fsm_core::expr::eval::TraceOutcome::Error { inputs, .. } => {
                        inputs.iter().any(|s| s.contains(want))
                    }
                    fsm_core::expr::eval::TraceOutcome::Value(v) => v.contains(want),
                    _ => false,
                };
                here || n.children.iter().any(|c| has_input(c, want))
            }
            assert!(
                has_input(node, "9223372036854775807") || has_input(node, "1"),
                "{node:?}"
            );
        }
        o => panic!("{o:?}"),
    }
}

fn value_contains(v: &Value, want: &str) -> bool {
    match v {
        Value::Str(s) | Value::Num(s) => s.contains(want),
        Value::Arr(a) => a.iter().any(|x| value_contains(x, want)),
        Value::Obj(m) => m.values().any(|x| value_contains(x, want)),
        Value::Bool(_) | Value::Null => false,
    }
}

#[test]
fn step_ordinary_false_invariant_renders_expr() {
    let src = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"}],"initial":"a","transitions":[{"from":"a","on":"go","do":[{"target":"x","value":"-1"}]}],"invariants":[{"name":"pos","expr":"ctx.x >= 0","mode":"enforce"}]}"#;
    let (m, t) = compile_src(src);
    let st = inst(&m, &t);
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/invariant");
            let rendered = r.trace.to_value();
            let invs = rendered
                .get("invariants")
                .and_then(Value::as_arr)
                .expect("invariants");
            let pos = invs
                .iter()
                .find(|i| i.get("name").and_then(Value::as_str) == Some("pos"))
                .expect("pos");
            assert_eq!(pos.get("passed").and_then(Value::as_bool), Some(false));
            let expr = pos.get("expr").expect("expr node");
            assert!(
                value_contains(expr, "-1") || value_contains(expr, "0"),
                "{expr:?}"
            );
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn later_guard_error_keeps_earlier_false_candidate() {
    let src = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"9223372036854775807"}],"events":[{"name":"go","fields":[]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","if":"false","to":"b"},{"from":"a","on":"go","if":"ctx.x + 1 > 0","to":"b"}]}"#;
    let (m, t) = compile_src(src);
    let st = inst(&m, &t);
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/guard_error");
            let idxs: Vec<u32> = r
                .trace
                .candidates
                .iter()
                .flat_map(|c| c.transitions.iter().map(|t| t.transition_idx))
                .collect();
            assert_eq!(idxs, vec![0, 1]);
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn failing_emit_keeps_pipeline_trace() {
    let src = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"x","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"effects":[{"name":"bill","fields":[{"name":"amt","ty":"int"}]}],"states":[{"name":"a"},{"name":"b","terminal":true}],"initial":"a","transitions":[{"from":"a","on":"go","to":"b","emit":[{"effect":"bill","args":{"amt":"9223372036854775807 + 1"}}]}]}"#;
    let (m, t) = compile_src(src);
    let st = inst(&m, &t);
    let mut b = Budget::new(4096);
    match step(&m, &t, &st, "go", &empty(), 0, &mut b) {
        Outcome::Rejected(r) => {
            assert_eq!(r.code, "run/action_error");
            assert_eq!(r.cause, Some("run/overflow"));
            assert_eq!(r.block.as_deref(), Some("transition"));
            let tr = r
                .trace
                .pipeline
                .iter()
                .find(|p| matches!(p.block, fsm_core::trace::BlockKind::Transition))
                .expect("transition");
            assert!(tr.discarded);
            assert_eq!(tr.emits[0].effect, "bill");
            assert!(tr.emits[0].expr.is_some());
        }
        o => panic!("{o:?}"),
    }
}

#[test]
fn external_self_reenters() {
    let src = r#"{"format":"fsm.machine/1","name":"m","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"tick","fields":[]}],"states":[{"name":"x","entry":{"do":[{"target":"n","value":"ctx.n + 1"}]}}],"initial":"x","transitions":[{"from":"x","on":"tick","to":"x"}]}"#;
    let (m, t) = compile_src(src);
    let mut st = inst(&m, &t);
    assert_eq!(st.ctx.get("n").unwrap().canonical_string(), "1");
    let a = apply(&m, &t, &mut st, "tick", &empty());
    assert_eq!(a.exited, ["x"]);
    assert_eq!(a.entered, ["x"]);
    assert_eq!(st.ctx.get("n").unwrap().canonical_string(), "2");
}
