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
    ];
    let inits = ["a", "a", "c", "c", "c"];
    let guards = ["", "true", "false", "ctx.b", "not ctx.b"];
    for (top, init) in tops.iter().zip(inits) {
        for g in guards {
            let ifg = if g.is_empty() {
                String::new()
            } else {
                format!(r#","if":"{g}""#)
            };
            let src = format!(
                r#"{{"format":"fsm.machine/1","name":"g","states":{top},"initial":"{init}","context":[{{"name":"b","ty":"bool","init":"true"}},{{"name":"n","ty":"int","init":"0"}}],"events":[{{"name":"e","fields":[]}},{{"name":"f","fields":[]}}],"transitions":[{{"from":"{init}","on":"e"{ifg},"do":[{{"target":"n","value":"ctx.n + 1"}}]}}]}}"#
            );
            machines.push(src);
        }
    }
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
            let Ok(c) = create(&m, &t, &BTreeMap::new()) else {
                continue;
            };
            let mut st = fsm_core::machine::InstanceState {
                status: c.status_after,
                leaf: c.leaf_after,
                ctx: c.ctx_after,
                history: c.history_after,
                pending: vec![],
            };
            let mut st2 = st.clone();
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
                        assert_eq!(a.effects.len(), b.effects.len());
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
                        // rejected leaves state
                    }
                    (Outcome::Ignored, Outcome::Ignored) => {}
                    _ => panic!("kind mismatch {src} {seq:?} {o1:?} {o2:?}"),
                }
            }
        }
    }
    eprintln!("enumerate_small count={count}");
    assert!(count > 100, "generator shrank: {count}");
}
