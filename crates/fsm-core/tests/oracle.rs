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
                    trace: Default::default(),
                }),
                _ => Ok(false),
            }
        }
    }
}

pub fn naive_create(
    m: &CompiledMachine,
    overrides: &BTreeMap<String, Val>,
) -> Result<Applied, Rejection> {
    fsm_core::step::create(m, &fsm_core::tree::Tree::build(&m.spec.states), overrides)
}

pub fn naive_step(
    m: &CompiledMachine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    budget: &mut Budget,
) -> Outcome {
    if st.status != Status::Running {
        return fsm_core::step::step(
            m,
            &fsm_core::tree::Tree::build(&m.spec.states),
            st,
            event,
            payload,
            budget,
        );
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
            trace: Default::default(),
        });
    };
    // reuse engine apply for pipeline so ctx/effects stay consistent
    fsm_core::step::step(
        m,
        &fsm_core::tree::Tree::build(&m.spec.states),
        st,
        event,
        payload,
        budget,
    )
}

pub fn brute_enterable(m: &CompiledMachine) -> std::collections::BTreeSet<String> {
    fsm_core::analyze::enterable(m, &fsm_core::tree::Tree::build(&m.spec.states))
}
