//! Rules for the reactive definition shapes plan 0009 introduces.
//!
//! Workstream 0043 owns the eventless-transition rules here, 0044 the `raise`
//! and internal-event rules, and 0045 the `final` state rules (`def/final_*`).
//! Plan 0010's workstream 0048 adds the `invoke` rules decidable from this
//! definition alone (`def/invoke_machine_ref`, `def/invoke_dup_slot`,
//! `def/invoke_on_terminal`, `def/invoke_evt`, `def/limit_invokes`); the
//! catalogue-dependent ones — `def/invoke_unknown_ctx`, `def/invoke_type`,
//! `def/invoke_cycle`, `def/invoke_depth`, `def/invoke_unknown_machine` —
//! need the child definitions in hand and run in the store's
//! `define_machine_on` (task 4901), not here. Plan 0011's workstream 0053
//! adds the two `supersedes` rules decidable from this definition alone
//! (`def/supersedes_machine_ref`, `def/supersedes_self`); every other
//! `def/supersedes_*` rule needs the superseded definition in hand and runs
//! at admission.
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

use super::super::{Finding, MachineSpec, Topology, TransitionSpec};

/// The `supersedes` rules that need nothing but this definition.
///
/// `def/supersedes_self` is unsatisfiable rather than merely wrong: the block
/// is part of the canonical bytes the hash covers, so a definition naming its
/// own hash would have to contain a hash of itself.
fn validate_supersedes(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    let Some(supersedes) = &spec.supersedes else {
        return;
    };
    let machine = &supersedes.machine;
    if machine.len() != 64
        || !machine.bytes().all(|b| b.is_ascii_hexdigit())
        || machine.bytes().any(|b| b.is_ascii_uppercase())
    {
        errs.push(Finding::err(
            "def/supersedes_machine_ref",
            "/supersedes/machine",
            "supersedes names a machine other than by 64-lowercase-hex digest",
            "use the superseded definition's machine_id digest, without the name@sha256: prefix",
        ));
        return;
    }
    if crate::hashes::digest_of(&crate::hashes::machine_id(&spec.to_value()))
        == Some(machine.as_str())
    {
        errs.push(Finding::err(
            "def/supersedes_self",
            "/supersedes/machine",
            "a definition supersedes itself",
            "name the definition this one replaces; a machine cannot contain its own hash",
        ));
    }
}

pub(super) fn validate_reactive(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    validate_supersedes(spec, errs);
    check_final_states(spec, errs);
    check_invokes(spec, errs);
    check_generated_event_names(spec, errs);
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

/// Every generated event name this machine can produce, in document order:
/// `$done.state.<compound>` for each compound owning a `final` child, then
/// `$done.region.<region>` for each region, then `$done.invoke.<slot>` for
/// each declared invoke slot.
pub fn generated_event_names(spec: &MachineSpec) -> Vec<String> {
    let mut names = Vec::new();
    for (node, _) in spec.walk_states() {
        if node.history.is_none()
            && node
                .states
                .iter()
                .any(|child| child.final_state && child.history.is_none())
        {
            names.push(format!("$done.state.{}", node.name));
        }
    }
    if let Topology::Parallel { regions } = &spec.topology {
        for region in regions {
            names.push(format!("$done.region.{}", region.name));
        }
    }
    for (node, _) in spec.walk_states() {
        for invoke in &node.invokes {
            names.push(format!("$done.invoke.{}", invoke.id));
        }
    }
    names
}

/// `on: "$done…"` resolves only to a name this machine generates; anything
/// else `$`-shaped is `def/unknown_event` whose hint lists the real names —
/// that list is the feature's discoverability.
fn check_generated_event_names(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    let generated = generated_event_names(spec);
    for (index, transition) in spec.transitions.iter().enumerate() {
        let Some(on) = transition.on.as_deref().filter(|on| on.starts_with('$')) else {
            continue;
        };
        if generated.iter().any(|name| name == on) {
            continue;
        }
        let hint = if generated.is_empty() {
            "this machine generates no done events: mark a compound's leaf final, or declare regions".to_string()
        } else {
            format!("this machine generates only: {}", generated.join(", "))
        };
        errs.push(Finding::err(
            "def/unknown_event",
            format!("/transitions/{index}/on"),
            format!("{on} is not an event this machine generates"),
            hint,
        ));
    }
}

/// The `invoke` rules a definition decides alone. `machine` is a 64-hex
/// digest so the parent's identity pins the child's; slot ids are unique
/// machine-wide because the child id, the generated event, and the audit
/// trail all read by them; an invoke on a terminal or final state could
/// never have its result consumed; and `with` sees `ctx` only, because an
/// invocation is triggered by state entry, not by an event.
fn check_invokes(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    let mut slots: BTreeSet<&str> = BTreeSet::new();
    for (node, _) in spec.walk_states() {
        if node.invokes.is_empty() {
            continue;
        }
        let base = format!("/states/{}/invoke", node.name);
        if node.terminal || node.final_state {
            errs.push(Finding::err(
                "def/invoke_on_terminal",
                base.clone(),
                format!(
                    "state {} is {} and invokes a child machine",
                    node.name,
                    if node.terminal { "terminal" } else { "final" }
                ),
                "nothing runs after this state, so nothing could consume the child's result; invoke from a state that has work left to do",
            ));
        }
        if node.invokes.len() > crate::limits::MAX_INVOKES_PER_STATE {
            errs.push(Finding::err(
                "def/limit_invokes",
                base.clone(),
                format!(
                    "state {} declares {} invoke slots",
                    node.name,
                    node.invokes.len()
                ),
                format!(
                    "at most {} invoke slots on one state; split the work across states",
                    crate::limits::MAX_INVOKES_PER_STATE
                ),
            ));
        }
        for (index, invoke) in node.invokes.iter().enumerate() {
            let path = format!("{base}/{index}");
            if invoke.id.starts_with('$') {
                errs.push(Finding::err(
                    "def/reserved_ident",
                    format!("{path}/id"),
                    format!("invoke slot {} uses the reserved $ prefix", invoke.id),
                    "the $ prefix belongs to generated names; choose another slot id",
                ));
            }
            if !slots.insert(invoke.id.as_str()) {
                errs.push(Finding::err(
                    "def/invoke_dup_slot",
                    format!("{path}/id"),
                    format!("invoke slot {} is declared twice", invoke.id),
                    "slot ids are unique across the whole machine: the child id, the done event, and the audit trail all read by them; rename one",
                ));
            }
            if !is_machine_digest(&invoke.machine) {
                errs.push(Finding::err(
                    "def/invoke_machine_ref",
                    format!("{path}/machine"),
                    format!("invoke slot {} names its machine as {:?}", invoke.id, invoke.machine),
                    "name the child by its 64-lowercase-hex machine_id digest, never by name, so this definition pins the exact child forever",
                ));
            }
            for (key, source) in &invoke.with {
                let Ok(expression) = parser::parse(source) else {
                    continue;
                };
                let mut references = Vec::new();
                collect_evt_references(&expression, &mut references);
                if let Some((name, span)) = references.first() {
                    let mut finding = Finding::err(
                        "def/invoke_evt",
                        format!("{path}/with/{key}"),
                        format!("invoke slot {} reads evt.{name}", invoke.id),
                        "an invocation starts when its state is entered, not when an event arrives, so `with` sees ctx only; stage the value into ctx first",
                    );
                    finding.span = Some(*span);
                    errs.push(finding);
                }
            }
        }
    }
}

/// A `machine_id` digest: exactly 64 lowercase hex characters.
fn is_machine_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The five `final` rules, each with a hint that says which of `final` and
/// `terminal` the author probably wanted. A `final` state is otherwise an
/// ordinary leaf: it may have blocks, be a target, and bind history.
fn check_final_states(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    let mut final_states: BTreeSet<&str> = BTreeSet::new();
    for (node, parent) in spec.walk_states() {
        if node.final_state && node.history.is_none() {
            final_states.insert(node.name.as_str());
            let path = format!("/states/{}", node.name);
            if !node.states.is_empty() {
                errs.push(Finding::err(
                    "def/final_not_leaf",
                    path.clone(),
                    format!("final state {} has children", node.name),
                    "a final state is a leaf that ends its parent's inner workflow; mark one of its leaves final instead",
                ));
            }
            if parent.is_none() {
                errs.push(Finding::err(
                    "def/final_at_root",
                    path.clone(),
                    format!("final state {} has no parent compound to finish", node.name),
                    "at the machine or region root, terminal is the spelling that ends the machine or region",
                ));
            }
            if node.terminal {
                errs.push(Finding::err(
                    "def/final_and_terminal",
                    path,
                    format!("state {} is both final and terminal", node.name),
                    "final ends the parent compound; terminal ends the machine or region; keep one",
                ));
            }
        }
        if node.history.is_none() && !node.states.is_empty() {
            if let Some(initial) = node.initial.as_deref() {
                if node.states.iter().any(|child| {
                    child.name == initial && child.final_state && child.history.is_none()
                }) {
                    errs.push(Finding::err(
                        "def/final_is_initial",
                        format!("/states/{}/initial", node.name),
                        format!("compound {} starts in its final child {initial}", node.name),
                        "start in a child that has work to do; a compound that begins finished never runs",
                    ));
                }
            }
        }
    }
    for (index, transition) in spec.transitions.iter().enumerate() {
        if final_states.contains(transition.from.as_str()) {
            errs.push(Finding::err(
                "def/final_has_transitions",
                format!("/transitions/{index}/from"),
                format!("transition leaves final state {}", transition.from),
                "a final state ends its compound; handle $done.state.<compound> on the compound or outside it instead",
            ));
        }
    }
    for (index, deadline) in spec.deadlines.iter().enumerate() {
        if final_states.contains(deadline.from.as_str()) {
            errs.push(Finding::err(
                "def/final_has_transitions",
                format!("/deadlines/{index}/from"),
                format!("deadline leaves final state {}", deadline.from),
                "a final state ends its compound; schedule the deadline on the compound or a state that is not final",
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
    for (index, raise) in transition.raises.iter().enumerate() {
        for (name, source) in &raise.with {
            sources.push((format!("{path}/raise/{index}/with/{name}"), source));
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
