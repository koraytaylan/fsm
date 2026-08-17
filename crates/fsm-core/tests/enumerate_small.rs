//! Exhaustive small-machine differential against the naive oracle.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Budget, Val};
use fsm_core::json::Value;
use fsm_core::spec::{compile, parse_machine};
use fsm_core::step::{Outcome, create, step};
use fsm_core::tree::Tree;

mod oracle;

fn payload() -> Value {
    Value::Obj(BTreeMap::new())
}

fn compile_src(src: &str) -> Option<(fsm_core::machine::CompiledMachine, Tree)> {
    let v = fsm_core::json::parse(src.as_bytes(), &fsm_core::json::JsonLimits::DEFAULT).ok()?;
    let spec = parse_machine(&v).ok()?;
    let m = compile(spec).ok()?;
    let t = Tree::build(&m.spec.states);
    Some((m, t))
}

#[test]
fn enumerate_small_differential() {
    let mut count = 0u32;
    let mut machines = Vec::new();
    // topologies
    let tops = [
        r#"[{"name":"a"}]"#,
        r#"[{"name":"a"},{"name":"b"}]"#,
        r#"[{"name":"c","initial":"l","states":[{"name":"l"}]}]"#,
        r#"[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"},{"name":"r"}]}]"#,
        r#"[{"name":"c","initial":"x","states":[{"name":"x","initial":"y","states":[{"name":"y"}]}]}]"#,
        r#"[{"name":"root","initial":"left","states":[{"name":"hist","history":"shallow"},{"name":"left"},{"name":"right"}]}]"#,
    ];
    let inits = ["a", "a", "c", "c", "c", "root"];
    let guards = ["", "true", "false", "ctx.b", "not ctx.b"];
    for (top, init) in tops.iter().zip(inits) {
        for g in guards {
            let ifg = if g.is_empty() {
                String::new()
            } else {
                format!(r#","if":"{g}""#)
            };
            let src = format!(
                r#"{{"format":"fsm.machine/1","name":"g","states":{top},"initial":"{init}","context":[{{"name":"b","ty":"bool","init":"true"}},{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}},{{"name":"f","fields":[]}}],"transitions":[{{"from":"{init}","on":"e"{ifg},"do":[{{"target":"n","value":"ctx.n + 1"}}]}}],"invariants":[{{"name":"nneg","expr":"ctx.n >= 0","mode":"enforce"}}]}}"#
            );
            machines.push(src);
        }
    }
    machines.push(
        r#"{"format":"fsm.machine/1","name":"hist","states":[{"name":"c","initial":"l","states":[{"name":"h","history":"deep"},{"name":"l"},{"name":"r"}]}],"initial":"c","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]},{"name":"back","fields":[]}],"transitions":[{"from":"l","on":"go","to":"r"},{"from":"c","on":"back","to":"h"}]}"#.into(),
    );
    for src in &machines {
        let Some((m, t)) = compile_src(src) else {
            continue;
        };
        let events = ["e", "f"];
        let seqs: Vec<Vec<&str>> = {
            let mut out = vec![vec![]];
            for _ in 0..4 {
                let mut next = Vec::new();
                for s in &out {
                    for e in events {
                        let mut n = s.clone();
                        n.push(e);
                        next.push(n);
                    }
                }
                out.extend(next);
            }
            out
        };
        for seq in seqs {
            count += 1;
            let Ok(c_engine) = create(&m, &t, &BTreeMap::new()) else {
                continue;
            };
            let Ok(c_naive) = oracle::naive_create(&m, &BTreeMap::new()) else {
                continue;
            };
            assert_eq!(c_engine.leaf_after, c_naive.leaf_after, "create leaf {src}");
            assert_eq!(c_engine.ctx_after, c_naive.ctx_after, "create ctx {src}");
            let mut st = fsm_core::machine::InstanceState {
                status: c_engine.status_after,
                leaf: c_engine.leaf_after,
                ctx: c_engine.ctx_after,
                history: c_engine.history_after,
                pending: vec![],
            };
            let mut st2 = fsm_core::machine::InstanceState {
                status: c_naive.status_after,
                leaf: c_naive.leaf_after,
                ctx: c_naive.ctx_after,
                history: c_naive.history_after,
                pending: vec![],
            };
            for ev in &seq {
                let mut b1 = Budget::new(4096);
                let mut b2 = Budget::new(4096);
                let o1 = step(&m, &t, &st, ev, &payload(), &mut b1);
                let o2 = oracle::naive_step(&m, &st2, ev, &payload(), &mut b2);
                match (&o1, &o2) {
                    (Outcome::Applied(a), Outcome::Applied(b)) => {
                        assert_eq!(a.leaf_after, b.leaf_after, "{src} {seq:?}");
                        assert_eq!(a.ctx_after, b.ctx_after);
                        assert_eq!(a.history_after, b.history_after);
                        assert_eq!(a.status_after, b.status_after);
                        assert_eq!(a.effects, b.effects);
                        assert_eq!(a.monitor_flags, b.monitor_flags);
                        assert!(b1.remaining() < 4096 && b2.remaining() < 4096);
                        st.leaf = a.leaf_after.clone();
                        st.ctx = a.ctx_after.clone();
                        st.history = a.history_after.clone();
                        st.status = a.status_after;
                        st2.leaf = b.leaf_after.clone();
                        st2.ctx = b.ctx_after.clone();
                        st2.history = b.history_after.clone();
                        st2.status = b.status_after;
                    }
                    (Outcome::Rejected(r1), Outcome::Rejected(r2)) => {
                        assert_eq!(r1.code, r2.code, "{src} {seq:?}");
                        assert_eq!(r1.source_state, r2.source_state);
                    }
                    (Outcome::Ignored, Outcome::Ignored) => {}
                    _ => panic!("kind mismatch {src} {seq:?} {o1:?} {o2:?}"),
                }
            }
        }
    }
    let emit_src = r#"{"format":"fsm.machine/1","name":"g","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"v","ty":"int"}]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}],"emit":[{"effect":"fx","args":{"v":"ctx.n"}}]}]}"#;
    let (m, t) = compile_src(emit_src).unwrap();
    let enter_e = fsm_core::analyze::enterable(&m, &t);
    let enter_n = oracle::brute_enterable(&m);
    assert!(!enter_e.is_empty() && !enter_n.is_empty());
    let c = fsm_core::step::create(&m, &t, &BTreeMap::new()).unwrap();
    let st = fsm_core::machine::InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut b1 = Budget::new(4096);
    let mut b2 = Budget::new(4096);
    match (
        fsm_core::step::step(&m, &t, &st, "e", &payload(), &mut b1),
        oracle::naive_step(&m, &st, "e", &payload(), &mut b2),
    ) {
        (Outcome::Applied(a), Outcome::Applied(b)) => {
            assert_eq!(a.effects, b.effects);
            assert_eq!(a.effects[0].args.get("v"), Some(&Val::Int(0)));
        }
        other => panic!("{other:?}"),
    }
    let mut tiny = Budget::new(1);
    let exhausted = fsm_core::step::step(&m, &t, &st, "e", &payload(), &mut tiny);
    assert!(matches!(exhausted, Outcome::Rejected(_)));
    eprintln!("enumerate_small count={count}");
    assert!(count > 100, "generator shrank: {count}");
}
