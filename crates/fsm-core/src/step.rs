//! Pure `step()` and `create()`.

#![allow(
    clippy::collapsible_if,
    clippy::too_many_arguments,
    clippy::result_large_err,
    clippy::if_same_then_else
)]

use std::collections::BTreeMap;

use crate::expr::eval::{Bindings, Budget, Val, eval};
use crate::expr::parser;
use crate::json::Value;
use crate::machine::{CompiledMachine, EnforceMode, InstanceState, Status};
use crate::spec::{Block, HistoryKind, MachineSpec, TransitionSpec, TySpec};
use crate::trace::{
    BlockKind, BlockTrace, CandidateTrace, DecisionTrace, EmitTrace, GuardTrace, InvariantTrace,
    LevelTrace, SetTrace,
};
use crate::tree::{NodeKind, Tree};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectOut {
    pub name: String,
    pub args: BTreeMap<String, Val>,
    pub k: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub leaf_after: String,
    pub ctx_after: BTreeMap<String, Val>,
    pub history_after: BTreeMap<String, String>,
    pub effects: Vec<EffectOut>,
    pub monitor_flags: Vec<String>,
    pub status_after: Status,
    pub internal: bool,
    pub source_state: String,
    pub transition_idx: u32,
    pub exited: Vec<String>,
    pub entered: Vec<String>,
    pub trace: DecisionTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
    pub source_state: Option<String>,
    pub transition_idx: Option<u32>,
    pub block: Option<String>,
    pub trace: DecisionTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Applied(Applied),
    Rejected(Rejection),
    Ignored,
}

pub fn validate_event(
    m: &CompiledMachine,
    name: &str,
    payload: &Value,
) -> Result<BTreeMap<String, Val>, Rejection> {
    let ev = m
        .spec
        .events
        .iter()
        .find(|e| e.name == name)
        .ok_or_else(|| reject("req/event_unknown", name))?;
    let obj = match payload {
        Value::Obj(o) => o.clone(),
        _ => {
            return Err(reject("req/field_type", "payload must be an object"));
        }
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
        if let Val::Enum { ty, variant } = &v {
            let allowed = m.spec.enums.get(ty).cloned().unwrap_or_default();
            if !allowed.iter().any(|x| x == variant) {
                return Err(reject("req/field_type", &f.name));
            }
        }
        if let (Val::Dec(d), TySpec::Dec { scale }) = (&v, &f.ty) {
            if d.scale != *scale {
                return Err(reject("req/field_scale", &f.name));
            }
        }
        out.insert(f.name.clone(), v);
    }
    for k in obj.keys() {
        if !ev.fields.iter().any(|f| f.name == *k) {
            return Err(reject("req/field_unknown", k));
        }
    }
    Ok(out)
}

fn reject(code: &'static str, what: &str) -> Rejection {
    Rejection {
        code,
        message: format!("{code}: {what}"),
        hint: what.into(),
        source_state: None,
        transition_idx: None,
        block: None,
        trace: DecisionTrace::default(),
    }
}

fn parse_typed(raw: &Value, ty: &TySpec) -> Result<Val, &'static str> {
    match ty {
        TySpec::Bool => raw.as_bool().map(Val::Bool).ok_or("req/field_type"),
        TySpec::Str => raw
            .as_str()
            .map(|s| Val::Str(s.into()))
            .ok_or("req/field_type"),
        TySpec::Int => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Int).map_err(|_| "req/field_type")
        }
        TySpec::Ts => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Ts).map_err(|_| "req/field_type")
        }
        TySpec::Dur => {
            let s = raw.as_str().ok_or("req/field_type")?;
            s.parse::<i64>().map(Val::Dur).map_err(|_| "req/field_type")
        }
        TySpec::Dec { scale } => {
            let s = raw.as_str().ok_or("req/field_type")?;
            match crate::decimal::Dec::parse(s, *scale) {
                Ok(d) => Ok(Val::Dec(d)),
                Err(crate::decimal::DecError::Parse) => {
                    // too many fraction digits
                    if s.contains('.')
                        && s.split('.').nth(1).map(|f| f.len()).unwrap_or(0) > *scale as usize
                    {
                        Err("req/field_scale")
                    } else {
                        Err("req/field_type")
                    }
                }
                Err(_) => Err("req/field_type"),
            }
        }
        TySpec::Enum { of } => {
            let s = raw.as_str().ok_or("req/field_type")?;
            Ok(Val::Enum {
                ty: of.clone(),
                variant: s.into(),
            })
        }
    }
}

pub fn step(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    budget: &mut Budget,
) -> Outcome {
    match st.status {
        Status::Completed => {
            return Outcome::Rejected(reject("run/instance_completed", "instance is completed"));
        }
        Status::Cancelled => {
            return Outcome::Rejected(reject("run/instance_cancelled", "instance is cancelled"));
        }
        Status::Running => {}
    }
    let fields = match validate_event(m, event, payload) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let leaf = match t.id(&st.leaf) {
        Some(i) => i,
        None => return Outcome::Rejected(reject("run/unhandled", "unknown leaf")),
    };
    let chain = t.chain(leaf);
    let mut trace = DecisionTrace::default();
    let mut winner: Option<(u16, usize)> = None;
    for &sid in &chain {
        let sname = t.names[sid as usize].clone();
        let idxs = m
            .transitions_by
            .get(&(sname.clone(), event.to_string()))
            .cloned()
            .unwrap_or_default();
        if idxs.is_empty() {
            continue;
        }
        let mut level = LevelTrace {
            source_state: sname.clone(),
            transitions: Vec::new(),
        };
        for idx in idxs {
            if winner.is_some() {
                level.transitions.push(CandidateTrace {
                    transition_idx: idx as u32,
                    guard: GuardTrace::NotConsidered,
                });
                continue;
            }
            let tr = &m.spec.transitions[idx];
            match eval_guard(tr, &st.ctx, &fields, budget) {
                Ok((true, gtrace)) => {
                    level.transitions.push(CandidateTrace {
                        transition_idx: idx as u32,
                        guard: GuardTrace::Evaluated(gtrace),
                    });
                    winner = Some((sid, idx));
                }
                Ok((false, gtrace)) => {
                    level.transitions.push(CandidateTrace {
                        transition_idx: idx as u32,
                        guard: GuardTrace::Evaluated(gtrace),
                    });
                }
                Err(mut r) => {
                    r.source_state = Some(sname);
                    r.transition_idx = Some(idx as u32);
                    r.trace = trace;
                    return Outcome::Rejected(r);
                }
            }
        }
        trace.candidates.push(level);
    }
    let Some((src, tidx)) = winner else {
        let any = trace.candidates.iter().any(|l| !l.transitions.is_empty());
        if !any {
            return match m.spec.on_unhandled {
                crate::spec::Unhandled::Ignore => Outcome::Ignored,
                crate::spec::Unhandled::Reject => Outcome::Rejected(Rejection {
                    code: "run/unhandled",
                    message: format!("no handler for {event} at {}", st.leaf),
                    hint: "add a transition or send a handled event".into(),
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    trace,
                }),
            };
        }
        return Outcome::Rejected(Rejection {
            code: "run/not_enabled",
            message: format!("all guards false for {event}"),
            hint: "adjust the payload or add a child override".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            trace,
        });
    };
    let tr = &m.spec.transitions[tidx];
    let internal = tr.to.is_none();
    let (exited_ids, entered_ids, new_leaf) = if internal {
        (Vec::new(), Vec::new(), st.leaf.clone())
    } else {
        let mut target_name = tr.to.clone().unwrap();
        let mut extra_descent = Vec::new();
        if let Some(tid) = t.id(&target_name) {
            if matches!(t.kind[tid as usize], NodeKind::History(_)) {
                let owner_name = &t.names[t.history_owner(tid).unwrap() as usize];
                extra_descent =
                    t.history_descent(tid, st.history.get(owner_name).map(String::as_str));
                // owner for dom
                target_name = t.names[t.history_owner(tid).unwrap() as usize].clone();
            }
        }
        let tid = t.id(&target_name).unwrap();
        let src_for_dom = src;
        let external_self = tr.to.as_deref() == Some(&t.names[src as usize]);
        let dom = if external_self {
            t.parent[src as usize]
        } else {
            t.proper_lca(src_for_dom, tid)
        };
        let exited = t.exit_set(leaf, dom);
        let mut entered = t.entry_path(dom, tid);
        // if target is compound, add initial descent
        if matches!(t.kind[tid as usize], NodeKind::Compound) && extra_descent.is_empty() {
            entered.extend(t.initial_descent(tid));
        }
        entered.extend(extra_descent);
        let leaf_after = entered.last().copied().unwrap_or(tid);
        (exited, entered, t.names[leaf_after as usize].clone())
    };

    let mut ctx = st.ctx.clone();
    let mut effects = Vec::new();
    let mut k = 0u32;
    let mut pipeline = Vec::new();

    let apply = |block: &Block,
                 kind: BlockKind,
                 ctx: &mut BTreeMap<String, Val>,
                 effects: &mut Vec<EffectOut>,
                 k: &mut u32,
                 see_evt: bool,
                 budget: &mut Budget|
     -> Result<BlockTrace, Rejection> {
        apply_block(block, kind, ctx, effects, k, see_evt, &fields, budget)
    };

    // exit inner → outer
    for &id in &exited_ids {
        let name = &t.names[id as usize];
        if let Some(node) = find_node(&m.spec, name) {
            if let Some(b) = &node.exit {
                match apply(
                    b,
                    BlockKind::Exit(name.clone()),
                    &mut ctx,
                    &mut effects,
                    &mut k,
                    false,
                    budget,
                ) {
                    Ok(bt) => pipeline.push(bt),
                    Err(mut r) => {
                        for p in &mut pipeline {
                            p.discarded = true;
                        }
                        r.trace.pipeline = pipeline;
                        r.trace.candidates = trace.candidates.clone();
                        return Outcome::Rejected(r);
                    }
                }
            }
        }
    }
    // transition
    let tblock = Block {
        sets: tr.sets.clone(),
        emits: tr.emits.clone(),
    };
    match apply(
        &tblock,
        BlockKind::Transition,
        &mut ctx,
        &mut effects,
        &mut k,
        true,
        budget,
    ) {
        Ok(bt) => pipeline.push(bt),
        Err(mut r) => {
            for p in &mut pipeline {
                p.discarded = true;
            }
            r.trace.pipeline = pipeline;
            r.trace.candidates = trace.candidates.clone();
            return Outcome::Rejected(r);
        }
    }
    // entry outer → inner
    for &id in &entered_ids {
        let name = &t.names[id as usize];
        if let Some(node) = find_node(&m.spec, name) {
            if let Some(b) = &node.entry {
                match apply(
                    b,
                    BlockKind::Entry(name.clone()),
                    &mut ctx,
                    &mut effects,
                    &mut k,
                    false,
                    budget,
                ) {
                    Ok(bt) => pipeline.push(bt),
                    Err(mut r) => {
                        for p in &mut pipeline {
                            p.discarded = true;
                        }
                        r.trace.pipeline = pipeline;
                        r.trace.candidates = trace.candidates.clone();
                        return Outcome::Rejected(r);
                    }
                }
            }
        }
    }

    // history capture from pre-transition
    let mut history_after = st.history.clone();
    for &id in &exited_ids {
        if matches!(t.kind[id as usize], NodeKind::Compound) {
            // owns history?
            for &ch in &t.children[id as usize] {
                if let NodeKind::History(hk) = t.kind[ch as usize] {
                    let bound = match hk {
                        HistoryKind::Deep => st.leaf.clone(),
                        HistoryKind::Shallow => {
                            // owner's direct child on pre chain
                            t.chain(leaf)
                                .into_iter()
                                .find(|&n| t.parent[n as usize] == Some(id))
                                .map(|n| t.names[n as usize].clone())
                                .unwrap_or_else(|| st.leaf.clone())
                        }
                    };
                    history_after.insert(t.names[id as usize].clone(), bound);
                }
            }
        }
    }

    // invariants
    let (ok_inv, flags, inv_trace) = eval_invariants(&m.spec, &ctx, budget);
    if !ok_inv {
        for p in &mut pipeline {
            p.discarded = true;
        }
        let mut trc = trace;
        trc.pipeline = pipeline;
        trc.invariants = inv_trace;
        return Outcome::Rejected(Rejection {
            code: "run/invariant",
            message: "enforce invariant failed".into(),
            hint: "adjust the action or the invariant".into(),
            source_state: Some(t.names[src as usize].clone()),
            transition_idx: Some(tidx as u32),
            block: None,
            trace: trc,
        });
    }

    let status_after = {
        let leaf_node = find_node(&m.spec, &new_leaf);
        if leaf_node.map(|n| n.terminal).unwrap_or(false) {
            Status::Completed
        } else {
            Status::Running
        }
    };
    trace.pipeline = pipeline;
    trace.invariants = inv_trace;
    Outcome::Applied(Applied {
        leaf_after: new_leaf,
        ctx_after: ctx,
        history_after,
        effects,
        monitor_flags: flags,
        status_after,
        internal,
        source_state: t.names[src as usize].clone(),
        transition_idx: tidx as u32,
        exited: exited_ids
            .iter()
            .map(|&i| t.names[i as usize].clone())
            .collect(),
        entered: entered_ids
            .iter()
            .map(|&i| t.names[i as usize].clone())
            .collect(),
        trace,
    })
}

fn eval_guard(
    tr: &TransitionSpec,
    ctx: &BTreeMap<String, Val>,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<(bool, crate::expr::eval::TraceNode), Rejection> {
    match &tr.guard {
        None => {
            let dummy = parser::parse("true").unwrap();
            let b = Bindings {
                ctx,
                evt: Some(evt),
            };
            let (_v, t) = eval(&dummy, &b, budget, true);
            Ok((
                true,
                t.unwrap_or(crate::expr::eval::TraceNode {
                    span: crate::expr::lexer::Span::new(0, 0),
                    outcome: crate::expr::eval::TraceOutcome::Value("true".into()),
                    children: vec![],
                }),
            ))
        }
        Some(src) => {
            let e = parser::parse(src).map_err(|err| Rejection {
                code: "run/guard_error",
                message: err.message,
                hint: err.hint,
                source_state: None,
                transition_idx: None,
                block: None,
                trace: DecisionTrace::default(),
            })?;
            let b = Bindings {
                ctx,
                evt: Some(evt),
            };
            match eval(&e, &b, budget, true) {
                (Ok(Val::Bool(v)), t) => Ok((v, t.unwrap())),
                (Err(err), _t) => Err(Rejection {
                    code: "run/guard_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    trace: DecisionTrace::default(),
                }),
                _ => Err(reject("run/guard_error", "guard not bool")),
            }
        }
    }
}

fn apply_block(
    block: &Block,
    kind: BlockKind,
    ctx: &mut BTreeMap<String, Val>,
    effects: &mut Vec<EffectOut>,
    k: &mut u32,
    see_evt: bool,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<BlockTrace, Rejection> {
    let snapshot = ctx.clone();
    let mut sets = Vec::new();
    let evt_ref = if see_evt { Some(evt) } else { None };
    let b = Bindings {
        ctx: &snapshot,
        evt: evt_ref,
    };
    for set in &block.sets {
        let e =
            parser::parse(&set.value).map_err(|err| action_err(&kind, err.message, err.hint))?;
        match eval(&e, &b, budget, true) {
            (Ok(v), Some(tn)) => {
                let before = ctx
                    .get(&set.target)
                    .map(Val::canonical_string)
                    .unwrap_or_default();
                sets.push(SetTrace {
                    target: set.target.clone(),
                    before,
                    after: v.canonical_string(),
                    expr: tn,
                });
                ctx.insert(set.target.clone(), v);
            }
            (Err(err), _) => {
                return Err(action_err(&kind, err.message, err.hint));
            }
            _ => {
                return Err(action_err(
                    &kind,
                    "eval failed".into(),
                    "check the expression".into(),
                ));
            }
        }
    }
    let mut emits = Vec::new();
    for em in &block.emits {
        let mut args = BTreeMap::new();
        for (name, src) in &em.args {
            let e = parser::parse(src).map_err(|err| action_err(&kind, err.message, err.hint))?;
            match eval(&e, &b, budget, false) {
                (Ok(v), _) => {
                    args.insert(name.clone(), v);
                }
                (Err(err), _) => return Err(action_err(&kind, err.message, err.hint)),
            }
        }
        effects.push(EffectOut {
            name: em.effect.clone(),
            args,
            k: *k,
        });
        emits.push(EmitTrace {
            effect: em.effect.clone(),
            k: *k,
        });
        *k += 1;
    }
    Ok(BlockTrace {
        block: kind,
        sets,
        emits,
        discarded: false,
    })
}

fn action_err(kind: &BlockKind, message: String, hint: String) -> Rejection {
    Rejection {
        code: "run/action_error",
        message,
        hint,
        source_state: None,
        transition_idx: None,
        block: Some(kind.as_label()),
        trace: DecisionTrace::default(),
    }
}

fn eval_invariants(
    spec: &MachineSpec,
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> (bool, Vec<String>, Vec<InvariantTrace>) {
    let mut ok = true;
    let mut flags = Vec::new();
    let mut traces = Vec::new();
    let b = Bindings { ctx, evt: None };
    for inv in &spec.invariants {
        let e = match parser::parse(&inv.expr) {
            Ok(e) => e,
            Err(_) => {
                ok = false;
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                });
                continue;
            }
        };
        match eval(&e, &b, budget, false).0 {
            Ok(Val::Bool(true)) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: true,
                });
            }
            Ok(Val::Bool(false)) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                });
                match inv.mode {
                    EnforceMode::Enforce => ok = false,
                    EnforceMode::Monitor => flags.push(inv.name.clone()),
                }
            }
            Ok(_) | Err(_) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                });
                ok = false;
            }
        }
    }
    (ok, flags, traces)
}

fn find_node<'a>(spec: &'a MachineSpec, name: &str) -> Option<&'a crate::spec::StateNode> {
    fn rec<'a>(
        nodes: &'a [crate::spec::StateNode],
        name: &str,
    ) -> Option<&'a crate::spec::StateNode> {
        for n in nodes {
            if n.name == name {
                return Some(n);
            }
            if let Some(f) = rec(&n.states, name) {
                return Some(f);
            }
        }
        None
    }
    rec(&spec.states, name)
}

/// Creation is a pure function of (definition, overrides). The shell NEVER
/// journals a failed create and consumes no id or seq.
pub fn create(
    m: &CompiledMachine,
    t: &Tree,
    overrides: &BTreeMap<String, Val>,
) -> Result<Applied, Rejection> {
    // validate overrides
    let ctx_map: BTreeMap<_, _> = m
        .spec
        .context
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    for (k, v) in overrides {
        let Some(decl) = ctx_map.get(k.as_str()) else {
            return Err(reject("req/field_unknown", k));
        };
        if !val_matches(v, &decl.ty) {
            return Err(reject("req/field_type", k));
        }
        if let (Val::Dec(d), TySpec::Dec { scale }) = (v, &decl.ty) {
            if d.scale != *scale {
                return Err(reject("req/field_scale", k));
            }
        }
        if let (Val::Enum { ty, variant }, TySpec::Enum { of }) = (v, &decl.ty) {
            if ty != of {
                return Err(reject("req/field_type", k));
            }
            let allowed = m.spec.enums.get(of).cloned().unwrap_or_default();
            if !allowed.iter().any(|x| x == variant) {
                return Err(reject("req/field_type", k));
            }
        }
    }
    let mut ctx = BTreeMap::new();
    for c in &m.spec.context {
        let v = if let Some(ov) = overrides.get(&c.name) {
            ov.clone()
        } else {
            parse_init(&c.init, &c.ty).map_err(|code| reject(code, &c.name))?
        };
        if let TySpec::Enum { of } = &c.ty {
            if let Val::Enum { variant, .. } = &v {
                let allowed = m.spec.enums.get(of).cloned().unwrap_or_default();
                if !allowed.iter().any(|x| x == variant) {
                    return Err(reject("req/field_type", &c.name));
                }
            }
        }
        ctx.insert(c.name.clone(), v);
    }
    let root_init = t
        .id(&m.spec.initial)
        .ok_or_else(|| reject("run/create_failed", "bad initial"))?;
    let mut entered = vec![root_init];
    entered.extend(t.initial_descent(root_init));
    let mut effects = Vec::new();
    let mut k = 0u32;
    let mut pipeline = Vec::new();
    let empty_evt = BTreeMap::new();
    let mut budget = Budget::new(4096);
    for &id in &entered {
        let name = &t.names[id as usize];
        if let Some(node) = find_node(&m.spec, name) {
            if let Some(b) = &node.entry {
                match apply_block(
                    b,
                    BlockKind::Entry(name.clone()),
                    &mut ctx,
                    &mut effects,
                    &mut k,
                    false,
                    &empty_evt,
                    &mut budget,
                ) {
                    Ok(bt) => pipeline.push(bt),
                    Err(inner) => {
                        return Err(Rejection {
                            code: "run/create_failed",
                            message: inner.message,
                            hint: inner.hint,
                            source_state: None,
                            transition_idx: None,
                            block: inner.block,
                            trace: DecisionTrace {
                                pipeline,
                                ..DecisionTrace::default()
                            },
                        });
                    }
                }
            }
        }
    }
    let (ok_inv, flags, inv_trace) = eval_invariants(&m.spec, &ctx, &mut budget);
    if !ok_inv {
        return Err(Rejection {
            code: "run/create_failed",
            message: "invariant failed at create".into(),
            hint: "fix inits or the invariant".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            trace: DecisionTrace {
                pipeline,
                invariants: inv_trace,
                ..DecisionTrace::default()
            },
        });
    }
    let leaf = t.names[*entered.last().unwrap() as usize].clone();
    Ok(Applied {
        leaf_after: leaf,
        ctx_after: ctx,
        history_after: BTreeMap::new(),
        effects,
        monitor_flags: flags,
        status_after: Status::Running,
        internal: false,
        source_state: String::new(),
        transition_idx: 0,
        exited: Vec::new(),
        entered: entered
            .iter()
            .map(|&i| t.names[i as usize].clone())
            .collect(),
        trace: DecisionTrace {
            pipeline,
            invariants: inv_trace,
            ..DecisionTrace::default()
        },
    })
}

fn parse_init(s: &str, ty: &TySpec) -> Result<Val, &'static str> {
    match ty {
        TySpec::Int => s.parse::<i64>().map(Val::Int).map_err(|_| "req/field_type"),
        TySpec::Bool => match s {
            "true" => Ok(Val::Bool(true)),
            "false" => Ok(Val::Bool(false)),
            _ => Err("req/field_type"),
        },
        TySpec::Str => Ok(Val::Str(s.into())),
        TySpec::Ts => s.parse::<i64>().map(Val::Ts).map_err(|_| "req/field_type"),
        TySpec::Dur => s.parse::<i64>().map(Val::Dur).map_err(|_| "req/field_type"),
        TySpec::Dec { scale } => crate::decimal::Dec::parse(s, *scale)
            .map(Val::Dec)
            .map_err(|_| "req/field_type"),
        TySpec::Enum { of } => Ok(Val::Enum {
            ty: of.clone(),
            variant: s.into(),
        }),
    }
}

fn val_matches(v: &Val, ty: &TySpec) -> bool {
    match (v, ty) {
        (Val::Int(_), TySpec::Int)
        | (Val::Bool(_), TySpec::Bool)
        | (Val::Str(_), TySpec::Str)
        | (Val::Ts(_), TySpec::Ts)
        | (Val::Dur(_), TySpec::Dur) => true,
        (Val::Dec(_), TySpec::Dec { .. }) => true,
        (Val::Enum { ty: got, .. }, TySpec::Enum { of }) => got == of,
        _ => false,
    }
}

/// Helper used by tests to parse a payload object of string fields.
pub fn payload_from_pairs(pairs: &[(&str, &str)]) -> Value {
    let mut m = BTreeMap::new();
    for (k, v) in pairs {
        m.insert((*k).into(), Value::Str((*v).into()));
    }
    Value::Obj(m)
}
