use std::collections::BTreeMap;

use crate::expr::eval::{Bindings, Budget, Val, eval};
use crate::expr::parser;
use crate::expr::typeck::{Scope, ScopeKind, Ty, annotate_if_widening};
use crate::machine::ExprSlot;
use crate::spec::{MachineSpec, TransitionSpec, TySpec};
use crate::trace::{BlockKind, CandidateTrace, DecisionTrace, GuardTrace, LevelTrace};

use super::block::action_err;
use super::validate::reject;
use super::{ExprSlotOwner, Rejection};

pub(super) fn eval_guard(
    tr: &TransitionSpec,
    ctx: &BTreeMap<String, Val>,
    evt: Option<&BTreeMap<String, Val>>,
    budget: &mut Budget,
    spec: &MachineSpec,
    event_name: &str,
    tidx: usize,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
) -> Result<(bool, crate::expr::eval::TraceNode), Rejection> {
    match &tr.guard {
        None => {
            // Omitted guards have historically evaluated an implicit `true`.
            // Keep that one-tick accounting for replay compatibility; the
            // compiler includes the worst-case tick in `def/limit_eval`.
            let dummy = parser::parse("true").expect("static guard expression");
            let bindings = Bindings {
                ctx,
                evt,
                active: None,
            };
            match eval(&dummy, &bindings, budget, true) {
                (Ok(Val::Bool(value)), Some(trace)) => Ok((value, trace)),
                (Err(error), trace) => Err(Rejection {
                    code: "run/guard_error",
                    message: error.message,
                    hint: error.hint,
                    source_state: None,
                    transition_idx: Some(tidx as u32),
                    block: None,
                    span: Some((error.span.start, error.span.end)),
                    cause: Some(error.code),
                    trace: DecisionTrace {
                        candidates: vec![LevelTrace {
                            source_state: String::new(),
                            transitions: vec![CandidateTrace {
                                transition_idx: tidx as u32,
                                guard: GuardTrace::Evaluated(trace.unwrap_or(
                                    crate::expr::eval::TraceNode {
                                        span: error.span,
                                        outcome: crate::expr::eval::TraceOutcome::Error {
                                            code: error.code,
                                            inputs: Vec::new(),
                                        },
                                        children: vec![],
                                    },
                                )),
                            }],
                        }],
                        ..DecisionTrace::default()
                    },
                }),
                _ => Err(reject("run/guard_error", "guard not bool")),
            }
        }
        Some(src) => {
            let e = if let Some(c) = compiled.get(&ExprSlot::TransitionGuard(tidx)) {
                c.expr.clone()
            } else {
                let ctx_tys: BTreeMap<String, Ty> = spec
                    .context
                    .iter()
                    .map(|c| (c.name.clone(), c.ty.to_ty()))
                    .collect();
                let evt_tys: BTreeMap<String, Ty> = spec
                    .events
                    .iter()
                    .find(|e| e.name == event_name)
                    .map(|e| {
                        e.fields
                            .iter()
                            .map(|f| (f.name.clone(), f.ty.to_ty()))
                            .collect()
                    })
                    .unwrap_or_default();
                let state_names = spec.state_names();
                let mut e = parser::parse(src).map_err(|err| Rejection {
                    code: "run/guard_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: Some((err.span.start, err.span.end)),
                    trace: DecisionTrace::default(),
                    cause: Some(err.code),
                })?;
                annotate_if_widening(
                    &mut e,
                    &spec_scope(
                        spec,
                        ScopeKind::Guard,
                        &ctx_tys,
                        Some(&evt_tys),
                        &state_names,
                    ),
                );
                e
            };
            let b = Bindings {
                ctx,
                evt,
                active: None,
            };
            match eval(&e, &b, budget, true) {
                (Ok(Val::Bool(v)), t) => Ok((v, t.unwrap())),
                (Err(err), t) => Err(Rejection {
                    code: "run/guard_error",
                    message: err.message,
                    hint: err.hint,
                    source_state: None,
                    transition_idx: Some(tidx as u32),
                    block: None,
                    span: Some((err.span.start, err.span.end)),
                    cause: Some(err.code),
                    trace: DecisionTrace {
                        candidates: vec![LevelTrace {
                            source_state: String::new(),
                            transitions: vec![CandidateTrace {
                                transition_idx: tidx as u32,
                                guard: GuardTrace::Evaluated(t.unwrap_or(
                                    crate::expr::eval::TraceNode {
                                        span: err.span,
                                        outcome: crate::expr::eval::TraceOutcome::Error {
                                            code: err.code,
                                            inputs: Vec::new(),
                                        },
                                        children: vec![],
                                    },
                                )),
                            }],
                        }],
                        ..DecisionTrace::default()
                    },
                }),
                _ => Err(reject("run/guard_error", "guard not bool")),
            }
        }
    }
}

pub(super) fn coerce_to_ty(v: Val, ty: &TySpec) -> Result<Val, &'static str> {
    match (v, ty) {
        (Val::Dec(d), TySpec::Dec { scale }) if d.scale == *scale => Ok(Val::Dec(d)),
        (Val::Dec(d), TySpec::Dec { scale }) if d.scale < *scale => d
            .rescale_up(*scale)
            .map(Val::Dec)
            .map_err(|_| "run/overflow"),
        (Val::Dec(_), TySpec::Dec { .. }) => Err("req/field_scale"),
        (v, ty) if val_matches(&v, ty) => Ok(v),
        _ => Err("req/field_type"),
    }
}

pub(super) fn spec_scope<'a>(
    spec: &'a MachineSpec,
    kind: ScopeKind,
    ctx_tys: &'a BTreeMap<String, Ty>,
    evt_tys: Option<&'a BTreeMap<String, Ty>>,
    states: &'a std::collections::BTreeSet<String>,
) -> Scope<'a> {
    Scope {
        kind,
        ctx: ctx_tys,
        evt: evt_tys,
        enums: &spec.enums,
        states,
    }
}

pub(super) fn compiled_or_annotate(
    src: &str,
    slot: &ExprSlot,
    compiled: &BTreeMap<ExprSlot, crate::machine::CompiledExpr>,
    spec: &MachineSpec,
    kind: ScopeKind,
    ctx_tys: &BTreeMap<String, Ty>,
    evt_tys: Option<&BTreeMap<String, Ty>>,
    states: &std::collections::BTreeSet<String>,
    block: &BlockKind,
) -> Result<crate::expr::ast::Expr, Rejection> {
    if let Some(c) = compiled.get(slot) {
        return Ok(c.expr.clone());
    }
    let mut e = parser::parse(src).map_err(|err| action_err(block, err.message, err.hint))?;
    annotate_if_widening(&mut e, &spec_scope(spec, kind, ctx_tys, evt_tys, states));
    Ok(e)
}

pub(super) fn owner_set_slot(owner: &ExprSlotOwner, i: usize) -> ExprSlot {
    match owner {
        ExprSlotOwner::Transition(t) => ExprSlot::TransitionSet(*t, i),
        ExprSlotOwner::Deadline(deadline) => ExprSlot::DeadlineSet(*deadline, i),
        ExprSlotOwner::Entry(n) => ExprSlot::StateEntrySet(n.clone(), i),
        ExprSlotOwner::Exit(n) => ExprSlot::StateExitSet(n.clone(), i),
    }
}

pub(super) fn owner_emit_slot(owner: &ExprSlotOwner, i: usize, arg: &str) -> ExprSlot {
    match owner {
        ExprSlotOwner::Transition(t) => ExprSlot::TransitionEmitArg(*t, i, arg.into()),
        ExprSlotOwner::Deadline(deadline) => ExprSlot::DeadlineEmitArg(*deadline, i, arg.into()),
        ExprSlotOwner::Entry(n) => ExprSlot::StateEntryEmitArg(n.clone(), i, arg.into()),
        ExprSlotOwner::Exit(n) => ExprSlot::StateExitEmitArg(n.clone(), i, arg.into()),
    }
}

pub(super) fn owner_raise_slot(owner: &ExprSlotOwner, i: usize, field: &str) -> ExprSlot {
    match owner {
        ExprSlotOwner::Transition(t) => ExprSlot::TransitionRaiseArg(*t, i, field.into()),
        ExprSlotOwner::Deadline(deadline) => ExprSlot::DeadlineRaiseArg(*deadline, i, field.into()),
        ExprSlotOwner::Entry(n) => ExprSlot::StateEntryRaiseArg(n.clone(), i, field.into()),
        ExprSlotOwner::Exit(n) => ExprSlot::StateExitRaiseArg(n.clone(), i, field.into()),
    }
}

pub(super) fn owner_signal_to_slot(owner: &ExprSlotOwner, i: usize) -> ExprSlot {
    match owner {
        ExprSlotOwner::Transition(t) => ExprSlot::TransitionSignalTo(*t, i),
        ExprSlotOwner::Deadline(deadline) => ExprSlot::DeadlineSignalTo(*deadline, i),
        ExprSlotOwner::Entry(n) => ExprSlot::StateEntrySignalTo(n.clone(), i),
        ExprSlotOwner::Exit(n) => ExprSlot::StateExitSignalTo(n.clone(), i),
    }
}

pub(super) fn owner_signal_arg_slot(owner: &ExprSlotOwner, i: usize, field: &str) -> ExprSlot {
    match owner {
        ExprSlotOwner::Transition(t) => ExprSlot::TransitionSignalArg(*t, i, field.into()),
        ExprSlotOwner::Deadline(deadline) => {
            ExprSlot::DeadlineSignalArg(*deadline, i, field.into())
        }
        ExprSlotOwner::Entry(n) => ExprSlot::StateEntrySignalArg(n.clone(), i, field.into()),
        ExprSlotOwner::Exit(n) => ExprSlot::StateExitSignalArg(n.clone(), i, field.into()),
    }
}

pub(super) fn val_matches(v: &Val, ty: &TySpec) -> bool {
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
