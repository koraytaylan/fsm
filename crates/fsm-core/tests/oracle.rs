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
    see_evt: bool,
    budget: &mut Budget,
) -> Result<(), Rejection> {
    let snapshot = ctx.clone();
    let b = Bindings {
        ctx: &snapshot,
        evt: if see_evt { Some(evt) } else { None },
    };
    let mut next = ctx.clone();
    for s in sets {
        let e = parser::parse(&s.value).map_err(|err| Rejection {
            code: "run/action_error",
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
            code: "run/action_error",
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

fn apply_emits(
    emits: &[fsm_core::spec::EmitSpec],
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    see_evt: bool,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
) -> Result<(), Rejection> {
    let b = Bindings {
        ctx,
        evt: if see_evt { Some(evt) } else { None },
    };
    for em in emits {
        let mut args = BTreeMap::new();
        for (k, src) in &em.args {
            let e = parser::parse(src).map_err(|err| Rejection {
                code: "run/action_error",
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
                code: "run/action_error",
                message: err.message,
                hint: err.hint,
                source_state: None,
                transition_idx: None,
                block: None,
                span: Some((err.span.start, err.span.end)),
                trace: Default::default(),
            })?;
            args.insert(k.clone(), v);
        }
        let k = effects.len() as u32;
        effects.push(EffectOut {
            name: em.effect.clone(),
            args,
            k,
        });
    }
    Ok(())
}

fn apply_block(
    block: &fsm_core::spec::Block,
    ctx: &mut BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    see_evt: bool,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
) -> Result<(), Rejection> {
    let snapshot = ctx.clone();
    apply_sets(&block.sets, ctx, evt, see_evt, budget)?;
    apply_emits(&block.emits, &snapshot, evt, see_evt, budget, effects)?;
    Ok(())
}

fn naive_validate(
    spec: &MachineSpec,
    name: &str,
    payload: &Value,
) -> Result<BTreeMap<String, Val>, Rejection> {
    let ev = spec
        .events
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| reject("req/event_unknown", name))?;
    let obj = match payload {
        Value::Obj(o) => o,
        _ => return Err(reject("req/field_type", "payload must be an object")),
    };
    let mut out = BTreeMap::new();
    for f in &ev.fields {
        let Some(raw) = obj.get(&f.name) else {
            return Err(reject("req/field_missing", &f.name));
        };
        if raw.as_num().is_some() {
            return Err(reject("req/number_token", &f.name));
        }
        let v = parse_typed(raw, &f.ty).map_err(|c| reject(c, &f.name))?;
        out.insert(f.name.clone(), v);
    }
    for k in obj.keys() {
        if !ev.fields.iter().any(|f| f.name == *k) {
            return Err(reject("req/field_unknown", k));
        }
    }
    Ok(out)
}

fn parse_typed(raw: &Value, ty: &fsm_core::spec::TySpec) -> Result<Val, &'static str> {
    use fsm_core::spec::TySpec;
    match ty {
        TySpec::Bool => raw.as_bool().map(Val::Bool).ok_or("req/field_type"),
        TySpec::Str => raw
            .as_str()
            .map(|s| Val::Str(s.into()))
            .ok_or("req/field_type"),
        TySpec::Int => raw
            .as_str()
            .and_then(|s| s.parse().ok())
            .map(Val::Int)
            .ok_or("req/field_type"),
        TySpec::Ts => raw
            .as_str()
            .and_then(|s| s.parse().ok())
            .map(Val::Ts)
            .ok_or("req/field_type"),
        TySpec::Dur => raw
            .as_str()
            .and_then(|s| s.parse().ok())
            .map(Val::Dur)
            .ok_or("req/field_type"),
        TySpec::Dec { scale } => raw
            .as_str()
            .and_then(|s| fsm_core::decimal::Dec::parse(s, *scale).ok())
            .map(Val::Dec)
            .ok_or("req/field_type"),
        TySpec::Enum { of } => raw
            .as_str()
            .map(|s| Val::Enum {
                ty: of.clone(),
                variant: s.into(),
            })
            .ok_or("req/field_type"),
    }
}

fn reject(code: &'static str, what: &str) -> Rejection {
    Rejection {
        code,
        message: format!("{code}: {what}"),
        hint: what.into(),
        source_state: None,
        transition_idx: None,
        block: None,
        span: None,
        trace: Default::default(),
    }
}

fn eval_invariants(
    spec: &MachineSpec,
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<Vec<String>, Rejection> {
    let mut flags = Vec::new();
    for inv in &spec.invariants {
        match eval_bool(Some(inv.expr.as_str()), ctx, &BTreeMap::new(), budget) {
            Ok(true) => {}
            Ok(false) => match inv.mode {
                EnforceMode::Monitor => flags.push(inv.name.clone()),
                EnforceMode::Enforce => {
                    return Err(Rejection {
                        code: "run/invariant",
                        message: inv.name.clone(),
                        hint: "fix context".into(),
                        source_state: None,
                        transition_idx: None,
                        block: None,
                        span: None,
                        trace: Default::default(),
                    });
                }
            },
            Err(r) => return Err(r),
        }
    }
    Ok(flags)
}

fn is_compound(n: &StateNode) -> bool {
    !n.states.is_empty() && n.history.is_none()
}

fn apply_entry_chain(
    spec: &MachineSpec,
    start: &str,
    ctx: &mut BTreeMap<String, Val>,
    budget: &mut Budget,
    effects: &mut Vec<EffectOut>,
) -> Result<Vec<String>, Rejection> {
    let mut entered = vec![start.to_string()];
    if let Some(node) = find(&spec.states, start) {
        if let Some(b) = &node.entry {
            apply_block(b, ctx, &BTreeMap::new(), false, budget, effects)?;
        }
    }
    entered.extend(initial_descent(&spec.states, start));
    for name in entered.iter().skip(1) {
        if let Some(node) = find(&spec.states, name) {
            if let Some(b) = &node.entry {
                apply_block(b, ctx, &BTreeMap::new(), false, budget, effects)?;
            }
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
    let mut effects = Vec::new();
    let entered = apply_entry_chain(
        &m.spec,
        &m.spec.initial,
        &mut ctx,
        &mut budget,
        &mut effects,
    )?;
    let leaf = entered
        .last()
        .cloned()
        .unwrap_or_else(|| m.spec.initial.clone());
    let flags = eval_invariants(&m.spec, &ctx, &mut budget)?;
    let status_after = find(&m.spec.states, &leaf)
        .map(|n| {
            if n.terminal {
                Status::Completed
            } else {
                Status::Running
            }
        })
        .unwrap_or(Status::Running);
    Ok(Applied {
        leaf_after: leaf,
        ctx_after: ctx,
        history_after: BTreeMap::new(),
        effects,
        monitor_flags: flags,
        status_after,
        internal: false,
        source_state: String::new(),
        transition_idx: 0,
        exited: Vec::new(),
        entered,
        trace: Default::default(),
    })
}

fn own_transition_index(spec: &MachineSpec) -> BTreeMap<(String, String), Vec<usize>> {
    let mut idx = BTreeMap::new();
    for (i, tr) in spec.transitions.iter().enumerate() {
        idx.entry((tr.from.clone(), tr.on.clone()))
            .or_insert_with(Vec::new)
            .push(i);
    }
    idx
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
    let fields = match naive_validate(&m.spec, event, payload) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let states = &m.spec.states;
    let index = own_transition_index(&m.spec);
    let ch = chain(states, &st.leaf);
    let mut winner = None;
    for sname in &ch {
        if let Some(idxs) = index.get(&(sname.clone(), event.to_string())) {
            for &idx in idxs {
                match eval_bool(
                    m.spec.transitions[idx].guard.as_deref(),
                    &st.ctx,
                    &fields,
                    budget,
                ) {
                    Ok(true) => {
                        winner = Some((sname.clone(), idx));
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
    let Some((src, tidx)) = winner else {
        let any = ch
            .iter()
            .any(|s| index.contains_key(&(s.clone(), event.to_string())));
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
    let internal = tr.to.is_none();
    let (exited, entered, new_leaf) = if internal {
        (Vec::new(), Vec::new(), st.leaf.clone())
    } else {
        let mut target = tr.to.clone().unwrap();
        let mut extra = Vec::new();
        if let Some(tn) = find(states, &target) {
            if tn.history.is_some() {
                let owner = parent_of(states, &target).unwrap();
                extra = hist_descent(states, &target, st.history.get(&owner).map(String::as_str));
                target = owner;
            }
        }
        let external_self = tr.to.as_deref() == Some(src.as_str());
        let dom = if external_self {
            parent_of(states, &src)
        } else {
            lca(states, &src, &target)
        };
        let exited = exit_set(states, &st.leaf, &dom);
        let mut entered = entry_path(states, &dom, &target);
        if find(states, &target).is_some_and(is_compound) && extra.is_empty() {
            entered.extend(initial_descent(states, &target));
        }
        entered.extend(extra);
        let leaf = entered.last().cloned().unwrap_or(target);
        (exited, entered, leaf)
    };
    let mut ctx = st.ctx.clone();
    let mut effects = Vec::new();
    for name in &exited {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.exit {
                if let Err(r) = apply_block(b, &mut ctx, &fields, false, budget, &mut effects) {
                    return Outcome::Rejected(r);
                }
            }
        }
    }
    let tblock = fsm_core::spec::Block {
        sets: tr.sets.clone(),
        emits: tr.emits.clone(),
    };
    if let Err(r) = apply_block(&tblock, &mut ctx, &fields, true, budget, &mut effects) {
        return Outcome::Rejected(r);
    }
    for name in &entered {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.entry {
                if let Err(r) = apply_block(b, &mut ctx, &fields, false, budget, &mut effects) {
                    return Outcome::Rejected(r);
                }
            }
        }
    }
    let mut history_after = st.history.clone();
    for name in &exited {
        if let Some(node) = find(states, name) {
            if is_compound(node) {
                for ch in &node.states {
                    if let Some(hk) = ch.history {
                        let bound = match hk {
                            HistoryKind::Deep => st.leaf.clone(),
                            HistoryKind::Shallow => chain(states, &st.leaf)
                                .into_iter()
                                .find(|n| parent_of(states, n).as_deref() == Some(name.as_str()))
                                .unwrap_or_else(|| st.leaf.clone()),
                        };
                        history_after.insert(name.clone(), bound);
                    }
                }
            }
        }
    }
    let flags = match eval_invariants(&m.spec, &ctx, budget) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let status_after = find(states, &new_leaf)
        .map(|n| {
            if n.terminal {
                Status::Completed
            } else {
                Status::Running
            }
        })
        .unwrap_or(Status::Running);
    Outcome::Applied(Applied {
        leaf_after: new_leaf,
        ctx_after: ctx,
        history_after,
        effects,
        monitor_flags: flags,
        status_after,
        internal,
        source_state: src,
        transition_idx: tidx as u32,
        exited,
        entered,
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
    fn hierarchical_entry_pipeline_leaf_b_n_11() {
        let src = br#"{"format":"fsm.machine/1","name":"h","states":[{"name":"q","initial":"a","states":[{"name":"a"},{"name":"b","entry":{"do":[{"target":"n","value":"ctx.n + 10"}]}}]}],"initial":"q","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"go","fields":[]}],"transitions":[{"from":"a","on":"go","to":"b","do":[{"target":"n","value":"ctx.n + 1"}]}]}"#;
        let v = parse(src, &JsonLimits::DEFAULT).unwrap();
        let m = compile(parse_machine(&v).unwrap()).unwrap();
        let t = Tree::build(&m.spec.states);
        let a = naive_create(&m, &BTreeMap::new()).unwrap();
        let e = create(&m, &t, &BTreeMap::new()).unwrap();
        assert_eq!(a.leaf_after, e.leaf_after);
        let st = InstanceState {
            status: a.status_after,
            leaf: a.leaf_after,
            ctx: a.ctx_after,
            history: a.history_after,
            pending: vec![],
        };
        let mut b1 = Budget::new(4096);
        let mut b2 = Budget::new(4096);
        let engine = step(&m, &t, &st, "go", &Value::Obj(BTreeMap::new()), &mut b1);
        let naive = naive_step(&m, &st, "go", &Value::Obj(BTreeMap::new()), &mut b2);
        match (&engine, &naive) {
            (Outcome::Applied(x), Outcome::Applied(y)) => {
                assert_eq!(x.leaf_after, "b");
                assert_eq!(y.leaf_after, "b");
                assert_eq!(x.ctx_after.get("n"), Some(&Val::Int(11)));
                assert_eq!(y.ctx_after.get("n"), Some(&Val::Int(11)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn oracle_emit_uses_pre_block_context() {
        let src = br#"{"format":"fsm.machine/1","name":"em","states":[{"name":"a"}],"initial":"a","context":[{"name":"n","ty":"int","init":"0"}],"events":[{"name":"e","fields":[]}],"effects":[{"name":"fx","fields":[{"name":"v","ty":"int"}]}],"transitions":[{"from":"a","on":"e","do":[{"target":"n","value":"1"}],"emit":[{"effect":"fx","args":{"v":"ctx.n"}}]}]}"#;
        let v = parse(src, &JsonLimits::DEFAULT).unwrap();
        let m = compile(parse_machine(&v).unwrap()).unwrap();
        let t = Tree::build(&m.spec.states);
        let created = create(&m, &t, &BTreeMap::new()).unwrap();
        let st = InstanceState {
            status: created.status_after,
            leaf: created.leaf_after,
            ctx: created.ctx_after,
            history: created.history_after,
            pending: vec![],
        };
        let mut b1 = Budget::new(4096);
        let mut b2 = Budget::new(4096);
        let engine = step(&m, &t, &st, "e", &Value::Obj(BTreeMap::new()), &mut b1);
        let naive = naive_step(&m, &st, "e", &Value::Obj(BTreeMap::new()), &mut b2);
        match (&engine, &naive) {
            (Outcome::Applied(x), Outcome::Applied(y)) => {
                assert_eq!(x.effects, y.effects);
                assert_eq!(x.effects[0].args.get("v"), Some(&Val::Int(0)));
            }
            other => panic!("{other:?}"),
        }
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
