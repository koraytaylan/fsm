//! The eventless transition graph: transitions that can only burn a
//! microstep, cycles the machine can never leave, and cascade depth.

use crate::machine::CompiledMachine;
use crate::spec::{ALWAYS_KEY, Finding};
use crate::tree::{NodeKind, Tree};

use super::find_machine_node;
use super::shadowing::is_true_guard;

/// `def/eventless_internal_noop`: an eventless transition with no target and
/// no actions can only spend a microstep.
///
/// A warning, not an error: a definition may be mid-authoring, but in a
/// shipped machine such a transition is always a mistake.
pub fn eventless_noop_findings(m: &CompiledMachine) -> Vec<Finding> {
    m.spec
        .transitions
        .iter()
        .enumerate()
        .filter(|(_, transition)| {
            transition.is_eventless()
                && transition.to.is_none()
                && transition.sets.is_empty()
                && transition.emits.is_empty()
        })
        .map(|(index, transition)| {
            Finding::warn(
                "def/eventless_internal_noop",
                format!("/transitions/{index}"),
                format!(
                    "eventless transition {index} from {} has no to, do, or emit",
                    transition.from
                ),
                "give it a target or an action; as written it can only burn a microstep",
            )
        })
        .collect()
}

/// One eventless reaction the scan could select from an active leaf.
struct EventlessEdge {
    from_leaf: u16,
    transition_idx: usize,
    to_leaf: u16,
    /// Guardless or literally `true`: the scan selects it whenever it is
    /// reached, so nothing after it on the chain can fire.
    certain: bool,
}

/// The eventless transition graph over the leaves a scan can start from.
///
/// From each non-terminal leaf, the candidates in scan order — innermost
/// state first, document order within a cell — up to and including the
/// first certain one, because a certain winner ends the scan. Targets
/// resolve through the same tree rules `step` uses: an internal transition
/// keeps the leaf, a history target descends its owner's initial chain (the
/// binding is unknown at admission), an external self-transition re-enters
/// `from`, and a compound target descends to its initial leaf.
///
/// Guard truth is decided syntactically and never by partial evaluation:
/// admission must be a pure function of the definition, and evaluating a
/// guard over an unknown context would make whether a machine is accepted
/// depend on which context a caller might later supply.
fn eventless_edges(m: &CompiledMachine, t: &Tree) -> Vec<EventlessEdge> {
    let mut edges = Vec::new();
    for leaf in 0..t.names.len() as u16 {
        if !matches!(t.kind[leaf as usize], NodeKind::Leaf)
            || find_machine_node(&m.spec, &t.names[leaf as usize]).is_some_and(|node| node.terminal)
        {
            continue;
        }
        'scan: for source in t.chain(leaf) {
            let cell = (t.names[source as usize].clone(), ALWAYS_KEY.to_string());
            for &transition_idx in m.transitions_by.get(&cell).into_iter().flatten() {
                let transition = &m.spec.transitions[transition_idx];
                let to_leaf = match &transition.to {
                    None => leaf,
                    Some(target) => landing_leaf(t, target),
                };
                let certain = is_true_guard(&transition.guard);
                edges.push(EventlessEdge {
                    from_leaf: leaf,
                    transition_idx,
                    to_leaf,
                    certain,
                });
                if certain {
                    break 'scan;
                }
            }
        }
    }
    edges
}

/// The leaf a transition to `target` lands in, before any history binding.
fn landing_leaf(t: &Tree, target: &str) -> u16 {
    let Some(target_id) = t.id(target) else {
        return 0;
    };
    let (root, descent) = match t.kind[target_id as usize] {
        NodeKind::History(_) => match t.history_owner(target_id) {
            Some(owner) => (owner, t.history_descent(target_id, None)),
            None => (target_id, Vec::new()),
        },
        NodeKind::Compound => (target_id, t.initial_descent(target_id)),
        NodeKind::Leaf => (target_id, Vec::new()),
    };
    descent.last().copied().unwrap_or(root)
}

/// Strongly connected components of the eventless graph, iteratively.
///
/// Iterative rather than recursive: `MAX_STATES` is 256 and depth 12, but a
/// hostile definition must not be able to blow the stack.
fn strongly_connected_components(node_count: usize, edges: &[EventlessEdge]) -> Vec<Vec<u16>> {
    let mut adjacency: Vec<Vec<u16>> = vec![Vec::new(); node_count];
    for edge in edges {
        adjacency[edge.from_leaf as usize].push(edge.to_leaf);
    }
    let mut index_of = vec![usize::MAX; node_count];
    let mut low_link = vec![0usize; node_count];
    let mut on_stack = vec![false; node_count];
    let mut stack: Vec<u16> = Vec::new();
    let mut components = Vec::new();
    let mut next_index = 0usize;
    for start in 0..node_count as u16 {
        if index_of[start as usize] != usize::MAX {
            continue;
        }
        let mut work: Vec<(u16, usize)> = vec![(start, 0)];
        index_of[start as usize] = next_index;
        low_link[start as usize] = next_index;
        next_index += 1;
        stack.push(start);
        on_stack[start as usize] = true;
        while let Some(&mut (node, ref mut next_edge)) = work.last_mut() {
            if *next_edge < adjacency[node as usize].len() {
                let successor = adjacency[node as usize][*next_edge];
                *next_edge += 1;
                if index_of[successor as usize] == usize::MAX {
                    index_of[successor as usize] = next_index;
                    low_link[successor as usize] = next_index;
                    next_index += 1;
                    stack.push(successor);
                    on_stack[successor as usize] = true;
                    work.push((successor, 0));
                } else if on_stack[successor as usize] {
                    low_link[node as usize] =
                        low_link[node as usize].min(index_of[successor as usize]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                low_link[parent as usize] = low_link[parent as usize].min(low_link[node as usize]);
            }
            if low_link[node as usize] == index_of[node as usize] {
                let mut component = Vec::new();
                loop {
                    let member = stack.pop().expect("tarjan stack holds the component");
                    on_stack[member as usize] = false;
                    component.push(member);
                    if member == node {
                        break;
                    }
                }
                component.sort_unstable();
                components.push(component);
            }
        }
    }
    components
}

/// Cycles in the eventless graph, and how deep an acyclic cascade can run.
///
/// A component that the machine provably cannot leave — every node in it has
/// a certain edge, and every edge that could be selected from any node stays
/// inside it — is `def/eventless_cycle`, an error: whatever the guards say,
/// a macrostep that enters it never quiesces. Any other cycle is
/// `def/eventless_cycle_guarded`, a warning, because the engine cannot
/// decide the guard at admission and `MAX_MICROSTEPS` is what stops it at
/// run time. `def/eventless_depth` warns when the longest acyclic cascade,
/// multiplied by the region count that shares the ceiling, reaches half of
/// `MAX_MICROSTEPS`.
pub fn eventless_cycle_findings(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let edges = eventless_edges(m, t);
    if edges.is_empty() {
        return Vec::new();
    }
    let node_count = t.names.len();
    let components = strongly_connected_components(node_count, &edges);
    let mut component_of = vec![usize::MAX; node_count];
    for (component_index, component) in components.iter().enumerate() {
        for &member in component {
            component_of[member as usize] = component_index;
        }
    }
    let mut out = Vec::new();
    for (component_index, component) in components.iter().enumerate() {
        let inside = |leaf: u16| component_of[leaf as usize] == component_index;
        let component_edges: Vec<&EventlessEdge> =
            edges.iter().filter(|edge| inside(edge.from_leaf)).collect();
        let cyclic = component.len() > 1
            || component_edges
                .iter()
                .any(|edge| edge.to_leaf == edge.from_leaf);
        if !cyclic {
            continue;
        }
        let inescapable = component.iter().all(|&member| {
            let from_member = component_edges
                .iter()
                .filter(|edge| edge.from_leaf == member);
            let mut has_certain = false;
            let mut all_inside = true;
            for edge in from_member {
                has_certain |= edge.certain;
                all_inside &= inside(edge.to_leaf);
            }
            has_certain && all_inside
        });
        let mut transition_indices: Vec<usize> = component_edges
            .iter()
            .filter(|edge| inside(edge.to_leaf))
            .map(|edge| edge.transition_idx)
            .collect();
        transition_indices.sort_unstable();
        transition_indices.dedup();
        let states: Vec<&str> = component
            .iter()
            .map(|&member| t.names[member as usize].as_str())
            .collect();
        let indices = transition_indices
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ");
        let path = format!("/transitions/{}", transition_indices[0]);
        if inescapable {
            out.push(Finding::err(
                "def/eventless_cycle",
                path,
                format!(
                    "eventless transitions {indices} cycle through {} and no guard can stop them",
                    states.join(", ")
                ),
                "the machine can never quiesce; guard one transition on the cycle, or point it at a state outside the cycle",
            ));
        } else {
            out.push(Finding::warn(
                "def/eventless_cycle_guarded",
                path,
                format!(
                    "eventless transitions {indices} form a cycle through {} that only a guard can break",
                    states.join(", ")
                ),
                format!(
                    "the engine cannot decide the guard at admission; a macrostep that never settles is refused after {} reactions as run/microstep_limit",
                    crate::limits::MAX_MICROSTEPS
                ),
            ));
        }
    }
    let region_count = match &m.spec.topology {
        crate::spec::Topology::Sequential { .. } => 1,
        crate::spec::Topology::Parallel { regions } => regions.len(),
    };
    let longest = longest_cascade(&components, &component_of, &edges);
    let shared = longest * region_count;
    if shared >= crate::limits::MAX_MICROSTEPS as usize / 2 {
        out.push(Finding::warn(
            "def/eventless_depth",
            "/transitions",
            format!(
                "the longest eventless cascade is {longest} microsteps and {region_count} region(s) share one ceiling: {shared} of the {} reactions a macrostep allows",
                crate::limits::MAX_MICROSTEPS
            ),
            "shorten the cascade or merge decisions so one macrostep stays well under the ceiling",
        ));
    }
    out
}

/// The longest path, in reactions, through the condensation of the eventless
/// graph. Tarjan emits components in reverse topological order, so every
/// successor component is finished before the component that reaches it.
fn longest_cascade(
    components: &[Vec<u16>],
    component_of: &[usize],
    edges: &[EventlessEdge],
) -> usize {
    let mut longest = vec![0usize; components.len()];
    for (component_index, component) in components.iter().enumerate() {
        for &member in component {
            for edge in edges.iter().filter(|edge| edge.from_leaf == member) {
                let successor = component_of[edge.to_leaf as usize];
                if successor != component_index {
                    longest[component_index] = longest[component_index].max(longest[successor] + 1);
                }
            }
        }
    }
    longest.into_iter().max().unwrap_or(0)
}
