//! Which states a definition can ever enter.

use std::collections::BTreeSet;

use crate::machine::CompiledMachine;
use crate::spec::Finding;
use crate::tree::{NodeKind, Tree};

/// Reachability lemma (history never extends the reachable set):
/// History bindings can only name configurations that were previously active,
/// and a shallow child's initial descent requires that child reachable some
/// other way first. Therefore modeling a history target as the owner's
/// initial chain is sound for the enterable-set over-approximation used here
/// (guard-optimistic).
pub fn enterable(m: &CompiledMachine, t: &Tree) -> BTreeSet<String> {
    let mut enterable = BTreeSet::new();
    for (_, root_initial) in &t.root_initials {
        enterable.insert(t.names[*root_initial as usize].clone());
        for state in t.initial_descent(*root_initial) {
            enterable.insert(t.names[state as usize].clone());
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for transition in &m.spec.transitions {
            if !enterable.contains(&transition.from) {
                continue;
            }
            let Some(target) = &transition.to else {
                continue;
            };
            if add_enterable_target(t, &transition.from, target, &mut enterable) {
                changed = true;
            }
        }
        for deadline in &m.spec.deadlines {
            if enterable.contains(&deadline.from)
                && add_enterable_target(t, &deadline.from, &deadline.to, &mut enterable)
            {
                changed = true;
            }
        }
    }
    enterable
}

fn add_enterable_target(
    tree: &Tree,
    source: &str,
    target: &str,
    enterable: &mut BTreeSet<String>,
) -> bool {
    let Some(target_id) = tree.id(target) else {
        return false;
    };
    let mut additions = Vec::new();
    match &tree.kind[target_id as usize] {
        NodeKind::History(_) => {
            if let Some(owner) = tree.history_owner(target_id) {
                additions.push(owner);
                additions.extend(tree.initial_descent(owner));
                if let Some(source_id) = tree.id(source) {
                    let domain = tree.proper_lca(source_id, owner);
                    additions.extend(tree.entry_path(domain, owner));
                }
            }
        }
        NodeKind::Compound => {
            additions.push(target_id);
            additions.extend(tree.initial_descent(target_id));
        }
        NodeKind::Leaf => additions.push(target_id),
    }
    if let Some(source_id) = tree.id(source)
        && !matches!(tree.kind[target_id as usize], NodeKind::History(_))
    {
        let domain = tree.proper_lca(source_id, target_id);
        additions.extend(tree.entry_path(domain, target_id));
    }
    let mut changed = false;
    for state in additions {
        changed |= enterable.insert(tree.names[state as usize].clone());
    }
    changed
}

pub fn reachability_findings(m: &CompiledMachine, t: &Tree) -> Vec<Finding> {
    let ent = enterable(m, t);
    let mut out = Vec::new();
    for name in &t.names {
        if !ent.contains(name)
            && !matches!(t.kind[t.id(name).unwrap() as usize], NodeKind::History(_))
        {
            out.push(Finding::warn(
                "def/unreachable_state",
                format!("/states/{name}"),
                format!("{name} is not enterable"),
                "add a transition or initial path to this state",
            ));
        }
    }
    out
}
