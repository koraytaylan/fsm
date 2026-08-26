//! Reachability, completeness, shadowing, enabled-events, and the eventless
//! graph: every analysis a definition is subjected to after it compiles.
//!
//! [`analyze_all`] runs the findings-producing passes in one fixed order;
//! callers surface the result as warnings beside the compiler's own. Each
//! pass lives in the submodule named for it. The eventless cycle pass is
//! also called by the compiler, where its `Error`-severity findings refuse
//! the definition.

#![allow(clippy::collapsible_if)]

use crate::machine::CompiledMachine;
use crate::spec::{Finding, MachineSpec};
use crate::tree::Tree;

mod completeness;
mod creation;
mod enabled_events;
mod eventless;
mod reachability;
mod shadowing;

pub use crate::spec::Finding as AnalyzeFinding;
pub use completeness::completeness_matrix;
pub use creation::create_always_fails;
pub(crate) use enabled_events::enabled_events_historical;
pub use enabled_events::{CandidateReport, EventReport, EventStatus, enabled_events};
pub use eventless::{ReactiveSummary, reactive_summary};
pub use eventless::{eventless_cycle_findings, eventless_noop_findings};
pub use reachability::{enterable, reachability_findings};
pub use shadowing::{ancestor_shadowed, shadowing_findings};

pub(super) fn find_node<'a>(
    nodes: &'a [crate::spec::StateNode],
    name: &str,
) -> Option<&'a crate::spec::StateNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(hit) = find_node(&n.states, name) {
            return Some(hit);
        }
    }
    None
}

pub(super) fn find_machine_node<'a>(
    spec: &'a MachineSpec,
    name: &str,
) -> Option<&'a crate::spec::StateNode> {
    spec.state_groups()
        .into_iter()
        .find_map(|(_, states, _)| find_node(states, name))
}

pub fn analyze_all(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let mut out = reachability_findings(m, t);
    out.extend(shadowing_findings(m));
    out.extend(ancestor_shadowed(m, t));
    out.extend(create_always_fails(m, t));
    out.extend(eventless_noop_findings(m));
    out.extend(eventless_cycle_findings(m, t));
    out
}
