//! Which declared events would select a transition if sent now.

use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::expr::parser;
use crate::expr::partial::{Truth, partial_eval_bool};
use crate::machine::{CompiledMachine, InstanceState};
use crate::tree::Tree;

use super::find_machine_node;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStatus {
    Enabled,
    Disabled,
    DependsOnPayload,
    Preempted,
    PreemptedMaybe,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateReport {
    pub source_state: String,
    pub transition_idx: usize,
    pub truth: EventStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventReport {
    pub event: String,
    pub status: EventStatus,
    pub candidates: Vec<CandidateReport>,
    pub payload_fields: Vec<String>,
}

pub fn enabled_events(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    budget: &mut Budget,
) -> Vec<EventReport> {
    enabled_events_with_guard_accounting(m, t, st, budget, OmittedGuardAccounting::Current)
}

/// Reproduce the legacy diagnostic accounting used in already-sealed
/// rejection details. Runtime selection always charged omitted guards; only
/// the historical enabled-event diagnostic omitted those ticks.
pub(crate) fn enabled_events_historical(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    budget: &mut Budget,
) -> Vec<EventReport> {
    enabled_events_with_guard_accounting(m, t, st, budget, OmittedGuardAccounting::Historical)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OmittedGuardAccounting {
    Current,
    Historical,
}

fn enabled_events_with_guard_accounting(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    budget: &mut Budget,
    omitted_guard_accounting: OmittedGuardAccounting,
) -> Vec<EventReport> {
    let active_leaves = t.active_leaves(&st.configuration).unwrap_or_default();
    let state_names = m.spec.state_names();
    let ctx_tys: BTreeMap<String, crate::expr::typeck::Ty> = m
        .spec
        .context
        .iter()
        .map(|c| (c.name.clone(), c.ty.to_ty()))
        .collect();
    let mut reports = Vec::new();
    for ev in &m.spec.events {
        let evt_tys: BTreeMap<String, crate::expr::typeck::Ty> = ev
            .fields
            .iter()
            .map(|f| (f.name.clone(), f.ty.to_ty()))
            .collect();
        let mut cands = Vec::new();
        let mut summary = EventStatus::Disabled;
        let mut fields = Vec::new();
        let mut preempt = None;
        for (_, leaf) in &active_leaves {
            let leaf_name = &t.names[*leaf as usize];
            if find_machine_node(&m.spec, leaf_name).is_some_and(|node| node.terminal) {
                continue;
            }
            for sid in t.chain(*leaf) {
                let sname = t.names[sid as usize].clone();
                let idxs = m
                    .transitions_by
                    .get(&(sname.clone(), ev.name.clone()))
                    .cloned()
                    .unwrap_or_default();
                for idx in idxs {
                    let status = if let Some(p) = preempt {
                        p
                    } else {
                        match &m.spec.transitions[idx].guard {
                            None if omitted_guard_accounting == OmittedGuardAccounting::Current => {
                                // Runtime evaluates an omitted guard as an
                                // implicit `true`, including its one budget
                                // tick. Analysis follows the same path; as for
                                // any other concrete evaluation error, budget
                                // exhaustion is conservatively Unknown.
                                let implicit =
                                    parser::parse("true").expect("static omitted-guard expression");
                                match partial_eval_bool(
                                    &implicit,
                                    &st.ctx,
                                    &crate::expr::typeck::Scope {
                                        kind: crate::expr::typeck::ScopeKind::Guard,
                                        ctx: &ctx_tys,
                                        evt: Some(&evt_tys),
                                        enums: &m.spec.enums,
                                        states: &state_names,
                                    },
                                    budget,
                                ) {
                                    Truth::True => EventStatus::Enabled,
                                    Truth::False => EventStatus::Disabled,
                                    Truth::Unknown => EventStatus::DependsOnPayload,
                                }
                            }
                            None => EventStatus::Enabled,
                            Some(src) => {
                                let e = m
                                    .compiled_exprs
                                    .get(&crate::machine::ExprSlot::TransitionGuard(idx))
                                    .map(|c| c.expr.clone())
                                    .or_else(|| parser::parse(src).ok());
                                match e {
                                    Some(e) => match partial_eval_bool(
                                        &e,
                                        &st.ctx,
                                        &crate::expr::typeck::Scope {
                                            kind: crate::expr::typeck::ScopeKind::Guard,
                                            ctx: &ctx_tys,
                                            evt: Some(&evt_tys),
                                            enums: &m.spec.enums,
                                            states: &state_names,
                                        },
                                        budget,
                                    ) {
                                        Truth::True => EventStatus::Enabled,
                                        Truth::False => EventStatus::Disabled,
                                        Truth::Unknown => {
                                            fields = field_reads(src);
                                            EventStatus::DependsOnPayload
                                        }
                                    },
                                    None => EventStatus::DependsOnPayload,
                                }
                            }
                        }
                    };
                    if preempt.is_none() {
                        match status {
                            EventStatus::Enabled => {
                                summary = EventStatus::Enabled;
                                preempt = Some(EventStatus::Preempted);
                            }
                            EventStatus::DependsOnPayload => {
                                summary = EventStatus::DependsOnPayload;
                                preempt = Some(EventStatus::PreemptedMaybe);
                            }
                            EventStatus::Disabled => {}
                            _ => {}
                        }
                    }
                    cands.push(CandidateReport {
                        source_state: sname.clone(),
                        transition_idx: idx,
                        truth: status,
                    });
                }
            }
        }
        if summary == EventStatus::Disabled && !cands.is_empty() {
            // all false
            summary = EventStatus::Disabled;
        }
        reports.push(EventReport {
            event: ev.name.clone(),
            status: summary,
            candidates: cands,
            payload_fields: fields,
        });
    }
    reports
}

fn field_reads(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(e) = parser::parse(src) {
        collect_evt_refs(&e, &mut out);
    }
    out.sort();
    out.dedup();
    out
}

fn collect_evt_refs(e: &crate::expr::ast::Expr, out: &mut Vec<String>) {
    use crate::expr::ast::{Arg, Expr};
    match e {
        Expr::EvtRef { name, .. } => out.push(name.clone()),
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => collect_evt_refs(inner, out),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => {
            collect_evt_refs(lhs, out);
            collect_evt_refs(rhs, out);
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            collect_evt_refs(cond, out);
            collect_evt_refs(then_branch, out);
            collect_evt_refs(else_branch, out);
        }
        Expr::Call { args, .. } => {
            for a in args {
                if let Arg::Expr(inner) = a {
                    collect_evt_refs(inner, out);
                }
            }
        }
        _ => {}
    }
}
