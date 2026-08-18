//! Exhaustive small-machine differential against the naive oracle.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Budget;
use fsm_core::json::Value;
use fsm_core::machine::InstanceState;
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

#[derive(Clone)]
struct Node {
    kids: Vec<Node>,
}

fn trees(n: usize, max_depth: u32) -> Vec<Node> {
    if n == 0 || max_depth == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![Node { kids: vec![] }];
    }
    forests(n - 1, max_depth - 1)
        .into_iter()
        .map(|kids| Node { kids })
        .collect()
}

fn forests(n: usize, max_depth: u32) -> Vec<Vec<Node>> {
    if n == 0 {
        return vec![vec![]];
    }
    if max_depth == 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for k in 1..=n {
        for t in trees(k, max_depth) {
            for rest in forests(n - k, max_depth) {
                let mut f = vec![t.clone()];
                f.extend(rest);
                out.push(f);
            }
        }
    }
    out
}

fn name_forest(forest: &[Node]) -> Vec<Named> {
    let mut i = 0u32;
    fn walk(n: &Node, i: &mut u32) -> Named {
        let name = format!("s{i}");
        *i += 1;
        Named {
            name,
            kids: n.kids.iter().map(|c| walk(c, i)).collect(),
        }
    }
    forest.iter().map(|n| walk(n, &mut i)).collect()
}

#[derive(Clone)]
struct Named {
    name: String,
    kids: Vec<Named>,
}

fn emit_states(nodes: &[Named]) -> String {
    let parts: Vec<String> = nodes
        .iter()
        .map(|n| {
            if n.kids.is_empty() {
                format!(r#"{{"name":"{}"}}"#, n.name)
            } else {
                format!(
                    r#"{{"name":"{}","initial":"{}","states":{}}}"#,
                    n.name,
                    n.kids[0].name,
                    emit_states(&n.kids)
                )
            }
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn emit_states_with_history(nodes: &[Named], owner: &str, kind: &str) -> String {
    let parts: Vec<String> = nodes
        .iter()
        .map(|n| {
            if n.kids.is_empty() {
                format!(r#"{{"name":"{}"}}"#, n.name)
            } else {
                let kids = if n.name == owner {
                    let hist = format!(r#"{{"name":"h_{kind}","history":"{kind}"}}"#);
                    let rest = emit_states(&n.kids);
                    format!("[{},{}", hist, &rest[1..])
                } else {
                    emit_states_with_history(&n.kids, owner, kind)
                };
                format!(
                    r#"{{"name":"{}","initial":"{}","states":{}}}"#,
                    n.name, n.kids[0].name, kids
                )
            }
        })
        .collect();
    format!("[{}]", parts.join(","))
}

fn first_compound(nodes: &[Named]) -> Option<&Named> {
    for n in nodes {
        if !n.kids.is_empty() {
            return Some(n);
        }
        if let Some(c) = first_compound(&n.kids) {
            return Some(c);
        }
    }
    None
}

fn first_leaf(nodes: &[Named]) -> Option<&str> {
    for n in nodes {
        if n.kids.is_empty() {
            return Some(&n.name);
        }
        if let Some(l) = first_leaf(&n.kids) {
            return Some(l);
        }
    }
    None
}

fn sequences<'a>(events: &'a [&'a str]) -> Vec<Vec<&'a str>> {
    let mut out = vec![vec![]];
    for _ in 0..4 {
        let mut next = Vec::new();
        for s in &out {
            for e in events {
                let mut n = s.clone();
                n.push(*e);
                next.push(n);
            }
        }
        out.extend(next);
    }
    out
}

fn machine_json(states: &str, initial: &str, ev_names: &[&str], transitions: &str) -> String {
    let evs: Vec<String> = ev_names
        .iter()
        .map(|n| format!(r#"{{"name":"{n}","fields":[]}}"#))
        .collect();
    format!(
        r#"{{"format":"fsm.machine/1","name":"g","states":{states},"initial":"{initial}","context":[{{"name":"b","ty":"bool","init":"true"}},{{"name":"n","ty":"int","init":"0"}}],"events":[{}],"transitions":[{transitions}],"invariants":[{{"name":"nneg","expr":"ctx.n >= 0","mode":"enforce"}}]}}"#,
        evs.join(",")
    )
}

fn compare_run(src: &str) {
    let Some((m, t)) = compile_src(src) else {
        return;
    };
    let ev_names: Vec<String> = m.spec.events.iter().map(|e| e.name.clone()).collect();
    let ev_refs: Vec<&str> = ev_names.iter().map(String::as_str).collect();
    let Ok(c_engine) = create(&m, &t, &BTreeMap::new()) else {
        return;
    };
    let Ok(c_naive) = oracle::naive_create(&m, &BTreeMap::new()) else {
        panic!("oracle create failed for compiled machine {src}");
    };
    assert_eq!(c_engine.leaf_after, c_naive.leaf_after, "create leaf {src}");
    assert_eq!(c_engine.ctx_after, c_naive.ctx_after, "create ctx {src}");
    assert_eq!(
        c_engine.history_after, c_naive.history_after,
        "create hist {src}"
    );
    let enter_e = fsm_core::analyze::enterable(&m, &t);
    let enter_n = oracle::brute_enterable(&m);
    assert_eq!(enter_e, enter_n, "enterable {src}");
    let st = InstanceState {
        status: c_engine.status_after,
        leaf: c_engine.leaf_after,
        ctx: c_engine.ctx_after,
        history: c_engine.history_after,
        pending: vec![],
    };
    let st2 = InstanceState {
        status: c_naive.status_after,
        leaf: c_naive.leaf_after,
        ctx: c_naive.ctx_after,
        history: c_naive.history_after,
        pending: vec![],
    };
    for seq in sequences(&ev_refs) {
        let mut a = st.clone();
        let mut b = st2.clone();
        for ev in &seq {
            let pre_a = a.clone();
            let pre_b = b.clone();
            let mut b1 = Budget::new(4096);
            let mut b2 = Budget::new(4096);
            let o1 = step(&m, &t, &a, ev, &payload(), &mut b1);
            let o2 = oracle::naive_step(&m, &b, ev, &payload(), &mut b2);
            match (&o1, &o2) {
                (Outcome::Applied(x), Outcome::Applied(y)) => {
                    assert_eq!(x.leaf_after, y.leaf_after, "{src} {seq:?}");
                    assert_eq!(x.ctx_after, y.ctx_after, "{src} {seq:?}");
                    assert_eq!(x.history_after, y.history_after, "{src} {seq:?}");
                    assert_eq!(x.status_after, y.status_after, "{src} {seq:?}");
                    assert_eq!(x.effects, y.effects, "{src} {seq:?}");
                    a.leaf = x.leaf_after.clone();
                    a.ctx = x.ctx_after.clone();
                    a.history = x.history_after.clone();
                    a.status = x.status_after;
                    b.leaf = y.leaf_after.clone();
                    b.ctx = y.ctx_after.clone();
                    b.history = y.history_after.clone();
                    b.status = y.status_after;
                }
                (Outcome::Rejected(r1), Outcome::Rejected(r2)) => {
                    assert_eq!(r1.code, r2.code, "{src} {seq:?}");
                    assert_eq!(a, pre_a, "engine mutated on reject {src}");
                    assert_eq!(b, pre_b, "oracle mutated on reject {src}");
                    assert_eq!(a.ctx, pre_a.ctx);
                    assert_eq!(a.leaf, pre_a.leaf);
                    assert_eq!(a.history, pre_a.history);
                    assert_ne!(r1.code, "internal/budget");
                }
                (Outcome::Ignored, Outcome::Ignored) => {
                    assert_eq!(a, pre_a);
                    assert_eq!(b, pre_b);
                }
                _ => panic!("kind mismatch {src} {seq:?} {o1:?} {o2:?}"),
            }
        }
    }
}

#[test]
fn enumerate_small_differential() {
    let guards = ["", "true", "false", "ctx.b", "not ctx.b"];
    let sets = [
        "",
        r#","do":[{"target":"n","value":"1"}]"#,
        r#","do":[{"target":"n","value":"ctx.n + 1"}]"#,
    ];
    let mut machines = Vec::new();
    for n in 1..=5 {
        for forest in forests(n, 3) {
            if forest.is_empty() {
                continue;
            }
            let named = name_forest(&forest);
            let states = emit_states(&named);
            let initial = named[0].name.clone();
            let leaf = first_leaf(&named).unwrap_or(&initial).to_string();
            for g in guards {
                let ifg = if g.is_empty() {
                    String::new()
                } else {
                    format!(r#","if":"{g}""#)
                };
                for set in sets {
                    let trans = format!(
                        r#"{{"from":"{leaf}","on":"e"{ifg}{set}}},{{"from":"{initial}","on":"f"}}"#
                    );
                    machines.push(machine_json(&states, &initial, &["e", "f"], &trans));
                }
            }
            if let Some(c) = first_compound(&named) {
                if let Some(child) = c.kids.first() {
                    let dest = c
                        .kids
                        .get(1)
                        .map(|k| k.name.as_str())
                        .unwrap_or(child.name.as_str());
                    for kind in ["deep", "shallow"] {
                        let states_h = emit_states_with_history(&named, &c.name, kind);
                        let trans = format!(
                            r#"{{"from":"{}","on":"go","to":"{dest}"}},{{"from":"{}","on":"back","to":"h_{kind}"}}"#,
                            child.name, c.name
                        );
                        machines.push(machine_json(&states_h, &initial, &["go", "back"], &trans));
                    }
                }
            }
        }
    }
    let mut count = 0u32;
    for src in &machines {
        count += 1;
        compare_run(src);
    }

    let emit_src = r#"{"format":"fsm.machine/1","name":"g","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"v","ty":"int"}]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}],"emit":[{"effect":"fx","args":{"v":"ctx.n"}}]}]}"#;
    compare_run(emit_src);
    let (m, t) = compile_src(emit_src).unwrap();
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut tiny = Budget::new(1);
    let mut tiny2 = Budget::new(1);
    let e1 = step(&m, &t, &st, "e", &payload(), &mut tiny);
    let e2 = oracle::naive_step(&m, &st, "e", &payload(), &mut tiny2);
    match (&e1, &e2) {
        (Outcome::Rejected(r1), Outcome::Rejected(r2)) => {
            assert_eq!(r1.code, r2.code);
            assert_eq!(r1.cause.or(Some(r1.code)), r2.cause.or(Some(r2.code)));
            assert!(
                r1.code == "internal/budget"
                    || r1.cause == Some("internal/budget")
                    || r1.code == "run/action_error"
            );
        }
        other => panic!("budget exhaustion {other:?}"),
    }

    let enum_src = r#"{"format":"fsm.machine/1","name":"en","enums":{"Color":["red","blue"]},"states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"paint","fields":[{"name":"c","ty":{"enum":"Color"}}]}],"transitions":[{"from":"a","on":"paint"}]}"#;
    let (m, t) = compile_src(enum_src).unwrap();
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut bad = BTreeMap::new();
    bad.insert("c".into(), Value::Str("green".into()));
    let mut b1 = Budget::new(4096);
    let mut b2 = Budget::new(4096);
    match (
        step(&m, &t, &st, "paint", &Value::Obj(bad.clone()), &mut b1),
        oracle::naive_step(&m, &st, "paint", &Value::Obj(bad), &mut b2),
    ) {
        (Outcome::Rejected(r1), Outcome::Rejected(r2)) => {
            assert_eq!(r1.code, r2.code);
            assert_eq!(r1.code, "req/field_type");
        }
        other => panic!("enum {other:?}"),
    }
    let mut good = BTreeMap::new();
    good.insert("c".into(), Value::Str("red".into()));
    let mut b1 = Budget::new(4096);
    let mut b2 = Budget::new(4096);
    match (
        step(&m, &t, &st, "paint", &Value::Obj(good.clone()), &mut b1),
        oracle::naive_step(&m, &st, "paint", &Value::Obj(good), &mut b2),
    ) {
        (Outcome::Applied(_), Outcome::Applied(_)) => {}
        other => panic!("enum ok {other:?}"),
    }

    let dec_src = r#"{"format":"fsm.machine/1","name":"dc","states":[{"name":"a"}],"initial":"a","context":[],"events":[{"name":"pay","fields":[{"name":"amt","ty":{"decimal":"2"}}]}],"transitions":[{"from":"a","on":"pay"}]}"#;
    let (m, t) = compile_src(dec_src).unwrap();
    let c = create(&m, &t, &BTreeMap::new()).unwrap();
    let st = InstanceState {
        status: c.status_after,
        leaf: c.leaf_after,
        ctx: c.ctx_after,
        history: c.history_after,
        pending: vec![],
    };
    let mut wide = BTreeMap::new();
    wide.insert("amt".into(), Value::Str("1.000".into()));
    let mut b1 = Budget::new(4096);
    let mut b2 = Budget::new(4096);
    match (
        step(&m, &t, &st, "pay", &Value::Obj(wide.clone()), &mut b1),
        oracle::naive_step(&m, &st, "pay", &Value::Obj(wide), &mut b2),
    ) {
        (Outcome::Rejected(r1), Outcome::Rejected(r2)) => {
            assert_eq!(r1.code, r2.code);
            assert_eq!(r1.code, "req/field_scale");
        }
        other => panic!("decimal {other:?}"),
    }

    eprintln!("enumerate_small machines={count}");
    assert!(count > 100, "generator shrank: {count}");
}
