//! Naive event and deadline interpreter: recursive spec walks, no `Tree`
//! tables, compiled expression slots, transition lookup, or deadline selector.
//! Every entry point runs the macrostep loop in `macrostep.rs`, written the
//! dumbest possible way.

use std::collections::BTreeMap;

use fsm_core::expr::eval::{Bindings, Budget, Val, eval};
use fsm_core::expr::parser;
use fsm_core::json::Value;
use fsm_core::machine::{ActiveConfiguration, CompiledMachine, EnforceMode, InstanceState, Status};
use fsm_core::spec::{Block, DeadlineSpec, HistoryKind, MachineSpec, StateNode, Topology};
use fsm_core::step::{
    Applied, DeadlineApplied, DeadlineOutcome, DeadlineRejected, EffectOut, Outcome,
    PendingDeadline, Rejection,
};

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

fn sequential_topology(spec: &MachineSpec) -> (&[StateNode], &str) {
    match &spec.topology {
        Topology::Sequential { states, initial } => (states, initial),
        Topology::Parallel { .. } => panic!("this oracle operation requires one region"),
    }
}

struct ActiveLeaf<'a> {
    region: Option<&'a str>,
    states: &'a [StateNode],
    leaf: String,
}

fn is_real_leaf(states: &[StateNode], name: &str) -> bool {
    find(states, name).is_some_and(|node| node.states.is_empty() && node.history.is_none())
}

/// Reconstruct active leaves directly from the definition and tagged public
/// configuration. This deliberately does not consult `Tree::active_leaves`.
fn active_leaves<'a>(
    spec: &'a MachineSpec,
    configuration: &ActiveConfiguration,
) -> Option<Vec<ActiveLeaf<'a>>> {
    match (&spec.topology, configuration) {
        (Topology::Sequential { states, .. }, ActiveConfiguration::Sequential { leaf })
            if is_real_leaf(states, leaf) =>
        {
            Some(vec![ActiveLeaf {
                region: None,
                states,
                leaf: leaf.clone(),
            }])
        }
        (Topology::Parallel { regions }, ActiveConfiguration::Parallel { leaves })
            if leaves.len() == regions.len() =>
        {
            let mut active = Vec::with_capacity(regions.len());
            for region in regions {
                let leaf = leaves.get(&region.name)?;
                if !is_real_leaf(&region.states, leaf) {
                    return None;
                }
                active.push(ActiveLeaf {
                    region: Some(&region.name),
                    states: &region.states,
                    leaf: leaf.clone(),
                });
            }
            Some(active)
        }
        _ => None,
    }
}

/// Every state name on the active configuration path, unioned across
/// regions. Independent of `Tree::active_state_names`: walks the spec
/// directly via this module's own `active_leaves`/`chain`.
fn active_state_names(
    spec: &MachineSpec,
    configuration: &ActiveConfiguration,
) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    if let Some(active) = active_leaves(spec, configuration) {
        for leaf in active {
            out.extend(chain(leaf.states, &leaf.leaf));
        }
    }
    out
}

fn configuration_is_terminal(spec: &MachineSpec, configuration: &ActiveConfiguration) -> bool {
    active_leaves(spec, configuration).is_some_and(|active| {
        !active.is_empty()
            && active
                .into_iter()
                .all(|leaf| find(leaf.states, &leaf.leaf).is_some_and(|node| node.terminal))
    })
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

#[path = "oracle/create.rs"]
mod create;
#[path = "oracle/deadline.rs"]
mod deadline;
#[path = "oracle/eval.rs"]
mod eval;
#[path = "oracle/macrostep.rs"]
mod macrostep;
#[path = "oracle/reach.rs"]
mod reach;
#[path = "oracle/step.rs"]
mod step;

pub use create::{naive_create, naive_create_at};
pub use deadline::naive_poll_deadline;
pub use macrostep::naive_certain_cycle;
pub use reach::brute_enterable;
pub use step::{naive_step, naive_step_at};

#[cfg(test)]
#[path = "oracle/independence.rs"]
mod independence;
