//! Naive second interpreter: recursive spec walks, no Tree tables.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Bindings, Budget, Val, eval};
use fsm_core::expr::parser;
use fsm_core::json::Value;
use fsm_core::machine::{CompiledMachine, EnforceMode, InstanceState, Status};
use fsm_core::spec::{HistoryKind, MachineSpec, StateNode};
use fsm_core::step::{Applied, EffectOut, Outcome, Rejection};

fn find<'a>(nodes: &'a [StateNode], name: &str) -> Option<&'a StateNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(f) = find(&n.states, name) {
            return Some(f);
        }
    }
    None
}

fn parent_of(nodes: &[StateNode], name: &str) -> Option<String> {
    fn rec(nodes: &[StateNode], name: &str, parent: Option<&str>) -> Option<String> {
        for n in nodes {
            if n.name == name {
                return parent.map(str::to_string);
            }
            if let Some(p) = rec(&n.states, name, Some(&n.name)) {
                return Some(p);
            }
        }
        None
    }
    rec(nodes, name, None)
}

fn chain(states: &[StateNode], leaf: &str) -> Vec<String> {
    let mut out = vec![leaf.to_string()];
    let mut cur = leaf.to_string();
    while let Some(p) = parent_of(states, &cur) {
        out.push(p.clone());
        cur = p;
    }
    out
}

fn depth(states: &[StateNode], name: &str) -> u32 {
    chain(states, name).len() as u32
}

fn lca(states: &[StateNode], a: &str, b: &str) -> Option<String> {
    let mut x = parent_of(states, a);
    let mut y = parent_of(states, b);
    while depth_opt(states, &x) > depth_opt(states, &y) {
        x = x.and_then(|n| parent_of(states, &n));
    }
    while depth_opt(states, &y) > depth_opt(states, &x) {
        y = y.and_then(|n| parent_of(states, &n));
    }
    while x != y {
        x = x.and_then(|n| parent_of(states, &n));
        y = y.and_then(|n| parent_of(states, &n));
    }
    x
}

fn depth_opt(states: &[StateNode], n: &Option<String>) -> u32 {
    n.as_ref().map(|s| depth(states, s)).unwrap_or(0)
}

fn exit_set(states: &[StateNode], leaf: &str, dom: &Option<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = Some(leaf.to_string());
    while let Some(n) = cur {
        if Some(&n) == dom.as_ref() {
            break;
        }
        out.push(n.clone());
        cur = parent_of(states, &n);
    }
    out
}

fn entry_path(states: &[StateNode], dom: &Option<String>, target: &str) -> Vec<String> {
    let mut walk = Vec::new();
    let mut cur = Some(target.to_string());
    while let Some(n) = cur {
        if Some(&n) == dom.as_ref() {
            break;
        }
        walk.push(n.clone());
        cur = parent_of(states, &n);
    }
    walk.reverse();
    walk
}

fn initial_of(node: &StateNode) -> Option<&str> {
    node.initial.as_deref()
}

fn initial_descent(states: &[StateNode], from: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = find(states, from).and_then(initial_of).map(str::to_string);
    while let Some(n) = cur {
        out.push(n.clone());
        cur = find(states, &n).and_then(initial_of).map(str::to_string);
    }
    out
}

fn hist_descent(states: &[StateNode], hist: &str, binding: Option<&str>) -> Vec<String> {
    let owner = parent_of(states, hist).unwrap();
    let kind = find(states, hist).and_then(|n| n.history);
    match (kind, binding) {
        (_, None) => initial_descent(states, &owner),
        (Some(HistoryKind::Deep), Some(b)) => entry_path(states, &Some(owner), b),
        (Some(HistoryKind::Shallow), Some(b)) => {
            let mut v = vec![b.to_string()];
            v.extend(initial_descent(states, b));
            v
        }
        _ => initial_descent(states, &owner),
    }
}

fn eval_bool(
    src: Option<&str>,
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<bool, Rejection> {
    match src {
        None => Ok(true),
        Some(s) => {
            let e = parser::parse(s).map_err(|err| Rejection {
                code: "run/guard_error",
                message: err.message,
                hint: err.hint,
                source_state: None,
                transition_idx: None,
                block: None,
                span: Some((err.span.start, err.span.end)),
                trace: Default::default(),
            })?;
            let b = Bindings {
                ctx,
                evt: Some(evt),
            };
            match eval(&e, &b, budget, false).0 {
                Ok(Val::Bool(v)) => Ok(v),
                Err(err) => Err(Rejection {
                    code: "run/guard_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: Some((err.span.start, err.span.end)),
                    trace: Default::default(),
                }),
                _ => Ok(false),
            }
        }
    }
}

fn parse_init(s: &str, ty: &fsm_core::spec::TySpec) -> Result<Val, Rejection> {
    use fsm_core::spec::TySpec;
    let reject = |code: &'static str| Rejection {
        code,
        message: s.into(),
        hint: "bad init".into(),
        source_state: None,
        transition_idx: None,
        block: None,
        span: None,
        trace: Default::default(),
    };
    match ty {
        TySpec::Int => s
            .parse()
            .map(Val::Int)
            .map_err(|_| reject("req/field_type")),
        TySpec::Bool => match s {
            "true" => Ok(Val::Bool(true)),
            "false" => Ok(Val::Bool(false)),
            _ => Err(reject("req/field_type")),
        },
        TySpec::Str => Ok(Val::Str(s.into())),
        TySpec::Ts => s.parse().map(Val::Ts).map_err(|_| reject("req/field_type")),
        TySpec::Dur => s
            .parse()
            .map(Val::Dur)
            .map_err(|_| reject("req/field_type")),
        TySpec::Dec { scale } => fsm_core::decimal::Dec::parse(s, *scale)
            .map(Val::Dec)
            .map_err(|_| reject("req/field_type")),
        TySpec::Enum { of } => Ok(Val::Enum {
            ty: of.clone(),
            variant: s.into(),
        }),
    }
}

fn apply_sets(
    sets: &[fsm_core::spec::SetSpec],
    ctx: &mut BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<(), Rejection> {
    let b = Bindings {
        ctx: &*ctx,
        evt: Some(evt),
    };
    let mut next = ctx.clone();
    for s in sets {
        let e = parser::parse(&s.value).map_err(|err| Rejection {
            code: err.code,
            message: err.message,
            hint: err.hint,
            source_state: None,
            transition_idx: None,
            block: None,
            span: Some((err.span.start, err.span.end)),
            trace: Default::default(),
        })?;
        let (v, _) = eval(&e, &b, budget, false);
        let v = v.map_err(|err| Rejection {
            code: err.code,
            message: err.message,
            hint: err.hint,
            source_state: None,
            transition_idx: None,
            block: None,
            span: Some((err.span.start, err.span.end)),
            trace: Default::default(),
        })?;
        next.insert(s.target.clone(), v);
    }
    *ctx = next;
    Ok(())
}

fn apply_entry_chain(
    m: &CompiledMachine,
    start: &str,
    ctx: &mut BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<Vec<String>, Rejection> {
    let mut entered = vec![start.to_string()];
    let mut cur = start.to_string();
    loop {
        let Some(node) = find(&m.spec.states, &cur) else {
            break;
        };
        if let Some(b) = &node.entry {
            apply_sets(&b.sets, ctx, &BTreeMap::new(), budget)?;
        }
        match &node.initial {
            Some(n) => {
                entered.push(n.clone());
                cur = n.clone();
            }
            None => break,
        }
    }
    Ok(entered)
}

pub fn naive_create(
    m: &CompiledMachine,
    overrides: &BTreeMap<String, Val>,
) -> Result<Applied, Rejection> {
    let mut ctx = BTreeMap::new();
    let mut budget = Budget::new(4096);
    for c in &m.spec.context {
        let v = if let Some(ov) = overrides.get(&c.name) {
            ov.clone()
        } else {
            parse_init(&c.init, &c.ty)?
        };
        ctx.insert(c.name.clone(), v);
    }
    let entered = apply_entry_chain(m, &m.spec.initial, &mut ctx, &mut budget)?;
    let leaf = entered
        .last()
        .cloned()
        .unwrap_or_else(|| m.spec.initial.clone());
    Ok(Applied {
        leaf_after: leaf,
        ctx_after: ctx,
        history_after: BTreeMap::new(),
        effects: Vec::new(),
        monitor_flags: Vec::new(),
        status_after: Status::Running,
        internal: false,
        source_state: String::new(),
        transition_idx: 0,
        exited: Vec::new(),
        entered,
        trace: Default::default(),
    })
}

pub fn naive_step(
    m: &CompiledMachine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    budget: &mut Budget,
) -> Outcome {
    if st.status != Status::Running {
        return Outcome::Rejected(Rejection {
            code: if st.status == Status::Completed {
                "run/instance_completed"
            } else {
                "run/instance_cancelled"
            },
            message: "not running".into(),
            hint: "create a new instance".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace: Default::default(),
        });
    }
    let fields = match fsm_core::step::validate_event(m, event, payload) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let states = &m.spec.states;
    let ch = chain(states, &st.leaf);
    let mut winner = None;
    for sname in &ch {
        if let Some(idxs) = m.transitions_by.get(&(sname.clone(), event.to_string())) {
            for &idx in idxs {
                match eval_bool(
                    m.spec.transitions[idx].guard.as_deref(),
                    &st.ctx,
                    &fields,
                    budget,
                ) {
                    Ok(true) => {
                        winner = Some(idx);
                        break;
                    }
                    Ok(false) => {}
                    Err(r) => return Outcome::Rejected(r),
                }
            }
        }
        if winner.is_some() {
            break;
        }
    }
    let Some(tidx) = winner else {
        let any = ch.iter().any(|s| {
            m.transitions_by
                .contains_key(&(s.clone(), event.to_string()))
        });
        if !any {
            return match m.spec.on_unhandled {
                fsm_core::spec::Unhandled::Ignore => Outcome::Ignored,
                fsm_core::spec::Unhandled::Reject => Outcome::Rejected(Rejection {
                    code: "run/unhandled",
                    message: "unhandled".into(),
                    hint: "n".into(),
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: None,
                    trace: Default::default(),
                }),
            };
        }
        return Outcome::Rejected(Rejection {
            code: "run/not_enabled",
            message: "not enabled".into(),
            hint: "n".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace: Default::default(),
        });
    };
    let tr = &m.spec.transitions[tidx];
    let mut ctx = st.ctx.clone();
    if let Err(r) = apply_sets(&tr.sets, &mut ctx, &fields, budget) {
        return Outcome::Rejected(r);
    }
    let leaf = tr.to.clone().unwrap_or_else(|| st.leaf.clone());
    for inv in &m.spec.invariants {
        match eval_bool(Some(inv.expr.as_str()), &ctx, &BTreeMap::new(), budget) {
            Ok(true) => {}
            Ok(false) => {
                return Outcome::Rejected(Rejection {
                    code: "run/invariant",
                    message: inv.name.clone(),
                    hint: "fix context".into(),
                    source_state: None,
                    transition_idx: Some(tidx as u32),
                    block: None,
                    span: None,
                    trace: Default::default(),
                });
            }
            Err(r) => return Outcome::Rejected(r),
        }
    }
    Outcome::Applied(Applied {
        leaf_after: leaf,
        ctx_after: ctx,
        history_after: st.history.clone(),
        effects: Vec::new(),
        monitor_flags: Vec::new(),
        status_after: Status::Running,
        internal: tr.to.is_none(),
        source_state: tr.from.clone(),
        transition_idx: tidx as u32,
        exited: Vec::new(),
        entered: Vec::new(),
        trace: Default::default(),
    })
}

pub fn brute_enterable(m: &CompiledMachine) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    fn walk(nodes: &[StateNode], name: &str, out: &mut std::collections::BTreeSet<String>) {
        if !out.insert(name.into()) {
            return;
        }
        if let Some(n) = find(nodes, name) {
            if let Some(init) = &n.initial {
                walk(nodes, init, out);
            }
        }
    }
    walk(&m.spec.states, &m.spec.initial, &mut out);
    for tr in &m.spec.transitions {
        if let Some(to) = &tr.to {
            walk(&m.spec.states, to, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod independence {
    use super::*;
    use fsm_core::json::{JsonLimits, parse};
    use fsm_core::spec::{compile, parse_machine};
    use fsm_core::step::{create, step};
    use fsm_core::tree::Tree;

    fn tiny() -> CompiledMachine {
        let src = br#"{"format":"fsm.machine/1","name":"m","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
        let v = parse(src, &JsonLimits::DEFAULT).unwrap();
        compile(parse_machine(&v).unwrap()).unwrap()
    }

    #[test]
    fn naive_step_matches_engine_and_not_wrong_apply() {
        let m = tiny();
        let t = Tree::build(&m.spec.states);
        let a = naive_create(&m, &BTreeMap::new()).unwrap();
        let via_engine = create(&m, &t, &BTreeMap::new()).unwrap();
        assert_eq!(a.ctx_after.get("n"), via_engine.ctx_after.get("n"));
        let st = InstanceState {
            status: a.status_after,
            leaf: a.leaf_after,
            ctx: a.ctx_after,
            history: a.history_after,
            pending: vec![],
        };
        let mut b1 = Budget::new(4096);
        let mut b2 = Budget::new(4096);
        let engine = step(&m, &t, &st, "e", &Value::Obj(BTreeMap::new()), &mut b1);
        let naive = naive_step(&m, &st, "e", &Value::Obj(BTreeMap::new()), &mut b2);
        match (&engine, &naive) {
            (Outcome::Applied(x), Outcome::Applied(y)) => {
                assert_eq!(x.ctx_after.get("n"), y.ctx_after.get("n"));
                assert_eq!(x.ctx_after.get("n"), Some(&Val::Int(1)));
            }
            other => panic!("{other:?}"),
        }
        let mut wrong = st.ctx.clone();
        wrong.insert("n".into(), Val::Int(2));
        assert_ne!(
            match &engine {
                Outcome::Applied(x) => x.ctx_after.get("n").cloned(),
                _ => None,
            },
            wrong.get("n").cloned()
        );
    }
}
