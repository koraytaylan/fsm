use std::collections::BTreeMap;

use crate::expr::eval::{Bindings, Budget, Val, eval};
use crate::expr::typeck::{ScopeKind, Ty, annotate_if_widening};
use crate::machine::{EnforceMode, ExprSlot};
use crate::spec::{Block, MachineSpec};
use crate::trace::{
    BlockKind, BlockTrace, DecisionTrace, EmitTrace, InvariantTrace, LevelTrace, RaiseTrace,
    SetTrace,
};

use super::guard::{
    coerce_to_ty, compiled_or_annotate, owner_emit_slot, owner_raise_slot, owner_set_slot,
    spec_scope,
};
use super::micro::{InternalEvent, InternalOrigin};
use super::{EffectOut, ExprSlotOwner, Rejection};

/// What a pipeline's blocks produce besides context: the effects for the
/// outbox under one continuing `k`, and the internal events for the
/// macrostep's queue, in block order.
pub(super) struct PipelineOutputs {
    pub(super) effects: Vec<EffectOut>,
    pub(super) next_effect_index: u32,
    pub(super) raised: Vec<InternalEvent>,
}

impl PipelineOutputs {
    pub(super) fn starting_at(first_effect_index: u32) -> Self {
        Self {
            effects: Vec::new(),
            next_effect_index: first_effect_index,
            raised: Vec::new(),
        }
    }
}

pub(super) fn apply_block(
    block: &Block,
    kind: BlockKind,
    ctx: &mut BTreeMap<String, Val>,
    outputs: &mut PipelineOutputs,
    see_evt: bool,
    evt: &BTreeMap<String, Val>,
    budget: &mut Budget,
    spec: &crate::spec::MachineSpec,
    event_name: &str,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
    owner: ExprSlotOwner,
) -> Result<BlockTrace, Rejection> {
    let snapshot = ctx.clone();
    let mut sets = Vec::new();
    let mut emits = Vec::new();
    let mut raises = Vec::new();
    let evt_ref = if see_evt { Some(evt) } else { None };
    let b = Bindings {
        ctx: &snapshot,
        evt: evt_ref,
        active: None,
    };
    let ctx_tys: BTreeMap<String, Ty> = spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let evt_tys: Option<BTreeMap<String, Ty>> =
        spec.events.iter().find(|e| e.name == event_name).map(|e| {
            e.fields
                .iter()
                .map(|f| (f.name.clone(), f.ty.to_ty()))
                .collect()
        });
    let state_names = spec.state_names();
    let scope_kind = if see_evt {
        ScopeKind::TransitionAction
    } else {
        ScopeKind::Block
    };
    for (i, set) in block.sets.iter().enumerate() {
        let e = compiled_or_annotate(
            &set.value,
            &owner_set_slot(&owner, i),
            compiled,
            spec,
            scope_kind,
            &ctx_tys,
            evt_tys.as_ref(),
            &state_names,
            &kind,
        )?;
        match eval(&e, &b, budget, true) {
            (Ok(v), Some(tn)) => {
                let v = if let Some(decl) = spec.context.iter().find(|c| c.name == set.target) {
                    coerce_to_ty(v, &decl.ty).map_err(|c| action_err(&kind, c.into(), c.into()))?
                } else {
                    v
                };
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
            (Err(err), tn) => {
                if let Some(tn) = tn {
                    sets.push(SetTrace {
                        target: set.target.clone(),
                        before: ctx
                            .get(&set.target)
                            .map(Val::canonical_string)
                            .unwrap_or_default(),
                        after: String::new(),
                        expr: tn,
                    });
                }
                return Err(action_err_at(
                    &kind,
                    "run/action_error",
                    err.message,
                    err.hint,
                    Some((err.span.start, err.span.end)),
                    sets,
                    emits,
                    Some(err.code),
                ));
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
    let k = &mut outputs.next_effect_index;
    for (ei, em) in block.emits.iter().enumerate() {
        let mut args = BTreeMap::new();
        let fx = spec.effects.iter().find(|e| e.name == em.effect);
        for (name, src) in &em.args {
            let e = compiled_or_annotate(
                src,
                &owner_emit_slot(&owner, ei, name),
                compiled,
                spec,
                scope_kind,
                &ctx_tys,
                evt_tys.as_ref(),
                &state_names,
                &kind,
            )?;
            match eval(&e, &b, budget, true) {
                (Ok(v), _) => {
                    let v = if let Some(f) =
                        fx.and_then(|e| e.fields.iter().find(|f| f.name == *name))
                    {
                        coerce_to_ty(v, &f.ty).map_err(|c| action_err(&kind, c.into(), c.into()))?
                    } else {
                        v
                    };
                    args.insert(name.clone(), v);
                }
                (Err(err), tn) => {
                    emits.push(EmitTrace {
                        effect: em.effect.clone(),
                        k: *k,
                        expr: tn,
                    });
                    return Err(action_err_at(
                        &kind,
                        "run/action_error",
                        err.message,
                        err.hint,
                        Some((err.span.start, err.span.end)),
                        sets,
                        emits,
                        Some(err.code),
                    ));
                }
            }
        }
        outputs.effects.push(EffectOut {
            name: em.effect.clone(),
            args,
            k: *k,
        });
        emits.push(EmitTrace {
            effect: em.effect.clone(),
            k: *k,
            expr: None,
        });
        *k += 1;
    }
    // A raise is an emit turned inward: the same snapshot semantics, the same
    // typed argument evaluation, but the payload lands in the macrostep's
    // queue instead of the outbox — and only from a block that commits,
    // which is every block that returns `Ok` here.
    for (ri, raise) in block.raises.iter().enumerate() {
        let declared = spec.events.iter().find(|e| e.name == raise.event);
        let mut payload = BTreeMap::new();
        for (name, src) in &raise.with {
            let e = compiled_or_annotate(
                src,
                &owner_raise_slot(&owner, ri, name),
                compiled,
                spec,
                scope_kind,
                &ctx_tys,
                evt_tys.as_ref(),
                &state_names,
                &kind,
            )?;
            match eval(&e, &b, budget, true) {
                (Ok(v), _) => {
                    let v = if let Some(f) =
                        declared.and_then(|e| e.fields.iter().find(|f| f.name == *name))
                    {
                        coerce_to_ty(v, &f.ty).map_err(|c| action_err(&kind, c.into(), c.into()))?
                    } else {
                        v
                    };
                    payload.insert(name.clone(), v);
                }
                (Err(err), _) => {
                    return Err(action_err_at(
                        &kind,
                        "run/action_error",
                        err.message,
                        err.hint,
                        Some((err.span.start, err.span.end)),
                        sets,
                        emits,
                        Some(err.code),
                    ));
                }
            }
        }
        raises.push(RaiseTrace {
            event: raise.event.clone(),
            with: payload
                .iter()
                .map(|(field, value)| (field.clone(), value.canonical_string()))
                .collect(),
        });
        outputs.raised.push(InternalEvent {
            name: raise.event.clone(),
            payload,
            origin: InternalOrigin::Raise {
                block: kind.clone(),
            },
        });
    }
    Ok(BlockTrace {
        block: kind,
        sets,
        emits,
        raises,
        discarded: false,
    })
}

pub(super) fn reject_pipeline(
    mut r: Rejection,
    mut done: Vec<BlockTrace>,
    candidates: &[LevelTrace],
) -> Rejection {
    for p in &mut done {
        p.discarded = true;
    }
    for p in &mut r.trace.pipeline {
        p.discarded = true;
    }
    done.append(&mut r.trace.pipeline);
    r.trace.pipeline = done;
    r.trace.candidates = candidates.to_vec();
    r
}

pub(super) fn action_err(kind: &BlockKind, message: String, hint: String) -> Rejection {
    action_err_at(
        kind,
        "run/action_error",
        message,
        hint,
        None,
        Vec::new(),
        Vec::new(),
        None,
    )
}

fn action_err_at(
    kind: &BlockKind,
    code: &'static str,
    message: String,
    hint: String,
    span: Option<(u32, u32)>,
    sets: Vec<SetTrace>,
    emits: Vec<EmitTrace>,
    cause: Option<&'static str>,
) -> Rejection {
    Rejection {
        code,
        message,
        hint,
        source_state: None,
        transition_idx: None,
        block: Some(kind.as_label()),
        span,
        cause,
        trace: DecisionTrace {
            pipeline: vec![BlockTrace {
                block: kind.clone(),
                sets,
                emits,
                raises: Vec::new(),
                discarded: true,
            }],
            ..DecisionTrace::default()
        },
    }
}

pub(super) fn eval_invariants(
    spec: &MachineSpec,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
    ctx: &BTreeMap<String, Val>,
    active: &std::collections::BTreeSet<String>,
    budget: &mut Budget,
) -> (bool, Vec<String>, Vec<InvariantTrace>) {
    let mut ok = true;
    let mut flags = Vec::new();
    let mut traces = Vec::new();
    let b = Bindings {
        ctx,
        evt: None,
        active: Some(active),
    };
    let ctx_tys: BTreeMap<String, Ty> = spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let state_names = spec.state_names();
    for (i, inv) in spec.invariants.iter().enumerate() {
        let e = if let Some(c) = compiled.get(&ExprSlot::Invariant(i)) {
            c.expr.clone()
        } else {
            match crate::expr::parser::parse(&inv.expr) {
                Ok(mut e) => {
                    annotate_if_widening(
                        &mut e,
                        &spec_scope(spec, ScopeKind::Invariant, &ctx_tys, None, &state_names),
                    );
                    e
                }
                Err(_) => {
                    ok = false;
                    traces.push(InvariantTrace {
                        name: inv.name.clone(),
                        passed: false,
                        expr: None,
                        error: None,
                    });
                    continue;
                }
            }
        };
        match eval(&e, &b, budget, true) {
            (Ok(Val::Bool(true)), tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: true,
                    expr: tn,
                    error: None,
                });
            }
            (Ok(Val::Bool(false)), tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                    expr: tn,
                    error: None,
                });
                match inv.mode {
                    EnforceMode::Enforce => ok = false,
                    EnforceMode::Monitor => flags.push(inv.name.clone()),
                }
            }
            (Err(err), tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                    expr: tn.clone(),
                    error: Some(crate::trace::InvariantEvalError {
                        code: err.code,
                        message: err.message,
                        span: Some((err.span.start, err.span.end)),
                        expr: tn,
                    }),
                });
                ok = false;
            }
            (_, tn) => {
                traces.push(InvariantTrace {
                    name: inv.name.clone(),
                    passed: false,
                    expr: tn,
                    error: None,
                });
                ok = false;
            }
        }
    }
    (ok, flags, traces)
}

pub(super) fn find_node<'a>(
    spec: &'a MachineSpec,
    name: &str,
) -> Option<&'a crate::spec::StateNode> {
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
    for (_, states, _) in spec.state_groups() {
        if let Some(node) = rec(states, name) {
            return Some(node);
        }
    }
    None
}
