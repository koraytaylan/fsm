//! Rules for the reactive definition shapes plan 0009 introduces.
//!
//! Workstream 0043 owns the eventless-transition rules here, 0044 the `raise`
//! and internal-event rules, and 0045 the `final` state rules (`def/final_*`).
//! [`validate_reactive`] runs last in [`super::validate_with_compatibility`],
//! so every finding it adds lands after the structural findings and existing
//! golden order is untouched. Only refusals live here: the advisory eventless
//! rules (`def/eventless_shadowed`, `def/eventless_internal_noop`) sit beside
//! their evented mirrors in `analyze.rs`, because validation has no warning
//! channel and a mirror that refused what its original merely reports would
//! not read alike.

use std::collections::BTreeSet;

use crate::expr::ast::{Arg, Expr};
use crate::expr::lexer::Span;
use crate::expr::parser;

use super::super::{Finding, MachineSpec, TransitionSpec};

pub(super) fn validate_reactive(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    let terminal_states: BTreeSet<&str> = spec
        .walk_states()
        .into_iter()
        .filter_map(|(node, _)| node.terminal.then_some(node.name.as_str()))
        .collect();
    for (index, transition) in spec.transitions.iter().enumerate() {
        if !transition.is_eventless() {
            continue;
        }
        let path = format!("/transitions/{index}");
        check_eventless_evt(transition, &path, errs);
        if terminal_states.contains(transition.from.as_str()) {
            errs.push(Finding::err(
                "def/eventless_from_terminal",
                format!("{path}/from"),
                format!(
                    "eventless transition leaves terminal state {}",
                    transition.from
                ),
                "a terminal state ends its machine or region and nothing runs after it; remove the transition or move it to a state that is not terminal",
            ));
        }
    }
}

/// `def/eventless_evt`: an eventless transition's guard or block names `evt`.
///
/// There is no event, so there is nothing for `evt` to bind to. The scope is
/// decided here, at admission, with the reference's own span, rather than
/// left for the evaluator to discover at run time.
fn check_eventless_evt(transition: &TransitionSpec, path: &str, errs: &mut Vec<Finding>) {
    let mut sources: Vec<(String, &str)> = Vec::new();
    if let Some(guard) = &transition.guard {
        sources.push((format!("{path}/if"), guard));
    }
    for (index, set) in transition.sets.iter().enumerate() {
        sources.push((format!("{path}/do/{index}/value"), &set.value));
    }
    for (index, emit) in transition.emits.iter().enumerate() {
        for (name, source) in &emit.args {
            sources.push((format!("{path}/emit/{index}/args/{name}"), source));
        }
    }
    for (expression_path, source) in sources {
        // A source that does not parse is compile's finding, not ours.
        let Ok(expression) = parser::parse(source) else {
            continue;
        };
        let mut references = Vec::new();
        collect_evt_references(&expression, &mut references);
        for (name, span) in references {
            let mut finding = Finding::err(
                "def/eventless_evt",
                expression_path.clone(),
                format!("eventless transition reads evt.{name}, but it has no event"),
                "read ctx instead, or give the transition an on so an event supplies evt",
            );
            finding.span = Some(span);
            errs.push(finding);
        }
    }
}

fn collect_evt_references(expression: &Expr, out: &mut Vec<(String, Span)>) {
    match expression {
        Expr::EvtRef { name, span } => out.push((name.clone(), *span)),
        Expr::Not { inner, .. } | Expr::Neg { inner, .. } => collect_evt_references(inner, out),
        Expr::And { lhs, rhs, .. }
        | Expr::Or { lhs, rhs, .. }
        | Expr::Cmp { lhs, rhs, .. }
        | Expr::Bin { lhs, rhs, .. } => {
            collect_evt_references(lhs, out);
            collect_evt_references(rhs, out);
        }
        Expr::If {
            cond,
            then_branch,
            else_branch,
            ..
        } => {
            collect_evt_references(cond, out);
            collect_evt_references(then_branch, out);
            collect_evt_references(else_branch, out);
        }
        Expr::Call { args, .. } => {
            for argument in args {
                if let Arg::Expr(inner) = argument {
                    collect_evt_references(inner, out);
                }
            }
        }
        Expr::IntLit { .. }
        | Expr::DecLit { .. }
        | Expr::StrLit { .. }
        | Expr::BoolLit { .. }
        | Expr::CtxRef { .. }
        | Expr::EnumLit { .. } => {}
    }
}
