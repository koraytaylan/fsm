//! Composition smells: the two ways an invoke slot is a modelling choice
//! somebody probably did not mean to make.
//!
//! Both are warnings, not errors. A slot whose result nothing reads may be
//! deliberate fire-and-forget, and a state with no other exit may be exactly
//! the wait an author intended — but neither is usually what somebody meant,
//! and neither is visible without reading the whole definition.
//!
//! Plan 0010 task 5103.

use crate::machine::CompiledMachine;
use crate::spec::{Finding, StateNode};
use crate::step::DONE_INVOKE_PREFIX;

/// Every state with a slot, with the path a finding should name.
fn invoking_states(m: &CompiledMachine) -> Vec<(&StateNode, String)> {
    fn walk<'a>(nodes: &'a [StateNode], path: &str, out: &mut Vec<(&'a StateNode, String)>) {
        for (index, node) in nodes.iter().enumerate() {
            let here = format!("{path}/{index}");
            if !node.invokes.is_empty() {
                out.push((node, here.clone()));
            }
            walk(&node.states, &format!("{here}/states"), out);
        }
    }
    let mut out = Vec::new();
    match &m.spec.topology {
        crate::spec::Topology::Sequential { states, .. } => walk(states, "/states", &mut out),
        crate::spec::Topology::Parallel { regions } => {
            for (index, region) in regions.iter().enumerate() {
                walk(
                    &region.states,
                    &format!("/regions/{index}/states"),
                    &mut out,
                );
            }
        }
    }
    out
}

/// The two composition warnings, in state then slot order.
pub fn invoke_findings(m: &CompiledMachine) -> Vec<Finding> {
    let mut out = Vec::new();
    for (node, path) in invoking_states(m) {
        // Any exit that is not the slot's own result: an event, an eventless
        // transition, or a deadline. A state whose only way out is the child
        // returning waits forever if the child never settles.
        let done_names: Vec<String> = node
            .invokes
            .iter()
            .map(|invoke| format!("{DONE_INVOKE_PREFIX}{}", invoke.id))
            .collect();
        let other_exit = m.spec.transitions.iter().any(|transition| {
            transition.from == node.name
                && transition.to.is_some()
                && !transition
                    .on
                    .as_deref()
                    .is_some_and(|event| done_names.iter().any(|name| name == event))
        }) || m
            .spec
            .deadlines
            .iter()
            .any(|deadline| deadline.from == node.name);
        for (index, invoke) in node.invokes.iter().enumerate() {
            let event = format!("{DONE_INVOKE_PREFIX}{}", invoke.id);
            let handled = m
                .spec
                .transitions
                .iter()
                .any(|transition| transition.on.as_deref() == Some(event.as_str()));
            if !handled {
                out.push(Finding::warn(
                    "def/invoke_result_unhandled",
                    format!("{path}/invoke/{index}"),
                    format!("nothing handles {event}"),
                    "add a transition on that event, or say in a comment that the result is deliberately ignored",
                ));
            }
        }
        if !other_exit {
            out.push(Finding::warn(
                "def/invoke_only_exit",
                path,
                format!(
                    "{} leaves only when an invoked child returns",
                    node.name
                ),
                "add a deadline or another transition, so a child that never settles cannot hold this state forever",
            ));
        }
    }
    out
}
