//! Hierarchy tables: parent, depth, LCA, exit/entry, descents.

#![allow(clippy::collapsible_if)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use crate::spec::{HistoryKind, MachineSpec, StateNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeKind {
    Leaf,
    Compound,
    History(HistoryKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub names: Vec<String>,
    pub parent: Vec<Option<u16>>,
    pub depth: Vec<u8>,
    pub children: Vec<Vec<u16>>,
    pub initial_child: Vec<Option<u16>>,
    pub kind: Vec<NodeKind>,
    pub index: BTreeMap<String, u16>,
    /// Region membership parallel to [`Tree::names`]; `None` for a sequential tree.
    pub region: Vec<Option<String>>,
    /// Region name and top-level initial state in semantic scan order.
    ///
    /// A sequential tree has one entry whose region is `None`.
    pub root_initials: Vec<(Option<String>, u16)>,
}

/// Why a public [`InstanceState`] does not describe a coherent state of its
/// compiled machine.
///
/// The detail is suitable for persistence diagnostics. Runtime entry points
/// map every such failure to the stable `run/configuration_invalid` code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateValidationError {
    detail: String,
}

impl StateValidationError {
    fn new(detail: impl Into<String>) -> Self {
        Self {
            detail: detail.into(),
        }
    }

    /// Human-readable validation detail.
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for StateValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.detail)
    }
}

impl std::error::Error for StateValidationError {}

impl Tree {
    /// Build a sequential state tree with its top-level initial state.
    ///
    /// Prefer [`Tree::for_machine`] when a complete definition is available;
    /// it also constructs parallel region membership and scan order.
    pub fn build(states: &[StateNode], initial: &str) -> Tree {
        Self::build_groups(vec![(None, states, initial)])
    }

    /// Build all top-level state trees and their region membership from a
    /// validated machine definition.
    pub fn for_machine(machine: &MachineSpec) -> Tree {
        Self::build_groups(machine.state_groups())
    }

    fn build_groups(groups: Vec<(Option<&str>, &[StateNode], &str)>) -> Tree {
        let mut names = Vec::new();
        let mut parent = Vec::new();
        let mut depth = Vec::new();
        let mut kind = Vec::new();
        let mut region = Vec::new();
        let mut children: Vec<Vec<u16>> = Vec::new();
        let mut index = BTreeMap::new();
        let mut stack: Vec<(&StateNode, Option<u16>, Option<String>)> = Vec::new();
        for (region_name, states, _) in groups.iter().rev() {
            for child in states.iter().rev() {
                stack.push((child, None, region_name.map(str::to_string)));
            }
        }
        while let Some((node, par, region_name)) = stack.pop() {
            let idx = names.len() as u16;
            names.push(node.name.clone());
            parent.push(par);
            region.push(region_name.clone());
            let d = match par {
                None => 1,
                Some(p) => depth[p as usize] + 1,
            };
            depth.push(d);
            let k = if let Some(h) = node.history {
                NodeKind::History(h)
            } else if node.states.is_empty() {
                NodeKind::Leaf
            } else {
                NodeKind::Compound
            };
            kind.push(k);
            children.push(Vec::new());
            index.insert(node.name.clone(), idx);
            if let Some(p) = par {
                children[p as usize].push(idx);
            }
            for child in node.states.iter().rev() {
                stack.push((child, Some(idx), region_name.clone()));
            }
        }
        let mut initial_child = vec![None; names.len()];
        fn fill_initial(
            nodes: &[StateNode],
            index: &BTreeMap<String, u16>,
            initial_child: &mut [Option<u16>],
        ) {
            for n in nodes {
                let initial = if n.history.is_some() {
                    // The legacy builder resolved a malformed history
                    // pseudostate's `initial` globally. Current definition
                    // admission forbids any history initial, but complete
                    // historical folds must reproduce already-sealed outcomes.
                    n.initial
                        .as_deref()
                        .and_then(|initial| index.get(initial).copied())
                } else {
                    n.initial
                        .as_deref()
                        .and_then(|initial| n.states.iter().find(|child| child.name == initial))
                        .and_then(|initial| index.get(&initial.name).copied())
                };
                if let (Some(&me), Some(initial)) = (index.get(&n.name), initial) {
                    initial_child[me as usize] = Some(initial);
                }
                fill_initial(&n.states, index, initial_child);
            }
        }
        for (_, states, _) in &groups {
            fill_initial(states, &index, &mut initial_child);
        }
        let root_initials = groups
            .iter()
            .filter_map(|(region, states, initial)| {
                states
                    .iter()
                    .any(|state| state.name == *initial)
                    .then_some(index.get(*initial).copied())
                    .flatten()
                    .map(|state| (region.map(str::to_string), state))
            })
            .collect();
        Tree {
            names,
            parent,
            depth,
            children,
            initial_child,
            kind,
            index,
            region,
            root_initials,
        }
    }

    pub fn id(&self, name: &str) -> Option<u16> {
        self.index.get(name).copied()
    }

    pub fn chain(&self, leaf: u16) -> Vec<u16> {
        let mut out = Vec::new();
        let mut cur = Some(leaf);
        while let Some(i) = cur {
            out.push(i);
            cur = self.parent[i as usize];
        }
        out
    }

    /// Return the orthogonal region containing `state`, if this is a parallel tree.
    pub fn region_of(&self, state: u16) -> Option<&str> {
        self.region[state as usize].as_deref()
    }

    /// Resolve active leaves in deterministic region scan order.
    ///
    /// Returns `None` when the configuration shape, regions, or leaf names do
    /// not match this tree. A tree containing a legacy malformed history
    /// shape also recognizes the history pseudostates that old execution could
    /// seal as active; current-valid definitions cannot trigger that exception.
    pub fn active_leaves(
        &self,
        configuration: &ActiveConfiguration,
    ) -> Option<Vec<(Option<&str>, u16)>> {
        match configuration {
            ActiveConfiguration::Sequential { leaf } => {
                if self.root_initials.len() != 1 || self.root_initials[0].0.is_some() {
                    return None;
                }
                let state = self.id(leaf)?;
                (self.region_of(state).is_none()
                    && (matches!(self.kind[state as usize], NodeKind::Leaf)
                        || self.is_legacy_active_history(state)))
                .then_some(vec![(None, state)])
            }
            ActiveConfiguration::Parallel { leaves } => {
                if self.root_initials.len() < 2
                    || self
                        .root_initials
                        .iter()
                        .any(|(region, _)| region.is_none())
                {
                    return None;
                }
                let mut active = Vec::with_capacity(self.root_initials.len());
                for (region, _) in &self.root_initials {
                    let region = region.as_deref()?;
                    let leaf = leaves.get(region)?;
                    let state = self.id(leaf)?;
                    if self.region_of(state) != Some(region)
                        || !matches!(self.kind[state as usize], NodeKind::Leaf)
                    {
                        return None;
                    }
                    active.push((Some(region), state));
                }
                if active.len() == leaves.len() {
                    Some(active)
                } else {
                    None
                }
            }
        }
    }

    /// Validate the topology-dependent parts of a durable instance state.
    ///
    /// This checks the complete active configuration, lifecycle/terminal
    /// coherence, history owner and binding shape, and the exact set of
    /// deadline schedules required by the active nonterminal state chains.
    /// Deadline timestamps themselves are caller time and may be any `i64`.
    /// Hash-authenticated legacy machines may contain child-bearing history
    /// nodes that current definition admission rejects; bindings emitted by
    /// that historical shape and global-name descents from their malformed
    /// `initial` fields remain valid for replay compatibility.
    pub fn validate_instance_state(
        &self,
        machine: &CompiledMachine,
        state: &InstanceState,
    ) -> Result<(), StateValidationError> {
        let active_leaves = self.active_leaves(&state.configuration).ok_or_else(|| {
            StateValidationError::new(
                "configuration does not match the machine topology and admissible active states",
            )
        })?;

        self.validate_history(&state.history)?;

        let terminal_states: BTreeSet<&str> = machine
            .spec
            .walk_states()
            .into_iter()
            .filter_map(|(node, _)| node.terminal.then_some(node.name.as_str()))
            .collect();
        let configuration_is_terminal = !active_leaves.is_empty()
            && active_leaves
                .iter()
                .all(|(_, leaf)| terminal_states.contains(self.names[*leaf as usize].as_str()));
        match state.status {
            Status::Running if configuration_is_terminal => {
                return Err(StateValidationError::new(
                    "running status does not match a terminal configuration",
                ));
            }
            Status::Completed if !configuration_is_terminal => {
                return Err(StateValidationError::new(
                    "completed status does not match a non-terminal configuration",
                ));
            }
            Status::Running | Status::Completed | Status::Cancelled => {}
        }

        let mut active_sources = BTreeSet::new();
        if state.status == Status::Running {
            for (_, leaf) in &active_leaves {
                let leaf_name = self.names[*leaf as usize].as_str();
                if terminal_states.contains(leaf_name) {
                    // A terminal parallel region has finished independently;
                    // schedules owned by its complete chain are cleared.
                    continue;
                }
                active_sources.extend(
                    self.chain(*leaf)
                        .into_iter()
                        .map(|node| self.names[node as usize].as_str()),
                );
            }
        }
        let expected_deadlines: BTreeSet<&str> = machine
            .spec
            .deadlines
            .iter()
            .filter(|deadline| active_sources.contains(deadline.from.as_str()))
            .map(|deadline| deadline.name.as_str())
            .collect();
        let actual_deadlines: BTreeSet<&str> = state.deadlines.keys().map(String::as_str).collect();
        if expected_deadlines != actual_deadlines {
            let missing = expected_deadlines
                .difference(&actual_deadlines)
                .copied()
                .collect::<Vec<_>>();
            let unexpected = actual_deadlines
                .difference(&expected_deadlines)
                .copied()
                .collect::<Vec<_>>();
            return Err(StateValidationError::new(format!(
                "deadline schedule set mismatch (missing: {missing:?}; unexpected: {unexpected:?})"
            )));
        }

        Ok(())
    }

    fn validate_history(
        &self,
        history: &BTreeMap<String, String>,
    ) -> Result<(), StateValidationError> {
        for (owner_name, binding_name) in history {
            let owner = self.id(owner_name).ok_or_else(|| {
                StateValidationError::new(format!("history owner {owner_name} is unknown"))
            })?;
            if !matches!(self.kind[owner as usize], NodeKind::Compound) {
                return Err(StateValidationError::new(format!(
                    "history owner {owner_name} is not a compound state"
                )));
            }
            let mut history_children = self.children[owner as usize]
                .iter()
                .copied()
                .filter(|child| matches!(self.kind[*child as usize], NodeKind::History(_)));
            let history_node = history_children.next().ok_or_else(|| {
                StateValidationError::new(format!(
                    "history owner {owner_name} has no history pseudostate"
                ))
            })?;
            if history_children.next().is_some() {
                return Err(StateValidationError::new(format!(
                    "history owner {owner_name} has ambiguous history pseudostates"
                )));
            }
            let binding = self.id(binding_name).ok_or_else(|| {
                StateValidationError::new(format!(
                    "history binding {owner_name} -> {binding_name} names an unknown state"
                ))
            })?;
            let kind = match self.kind[history_node as usize] {
                NodeKind::History(kind) => kind,
                NodeKind::Leaf | NodeKind::Compound => unreachable!("filtered history child"),
            };
            match kind {
                HistoryKind::Deep => {
                    if !self.is_descendant(binding, owner) {
                        return Err(StateValidationError::new(format!(
                            "deep history binding {owner_name} -> {binding_name} is not a descendant of its owner"
                        )));
                    }
                    if !matches!(self.kind[binding as usize], NodeKind::Leaf)
                        && !self.is_legacy_active_history(binding)
                    {
                        return Err(StateValidationError::new(format!(
                            "deep history binding {owner_name} -> {binding_name} must name a leaf"
                        )));
                    }
                }
                HistoryKind::Shallow => {
                    if self.parent[binding as usize] != Some(owner) {
                        return Err(StateValidationError::new(format!(
                            "shallow history binding {owner_name} -> {binding_name} must name a direct child"
                        )));
                    }
                    if matches!(self.kind[binding as usize], NodeKind::History(_))
                        && !self.is_legacy_history_binding(binding)
                    {
                        return Err(StateValidationError::new(format!(
                            "shallow history binding {owner_name} -> {binding_name} must name a real child"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    fn is_descendant(&self, node: u16, ancestor: u16) -> bool {
        let mut current = Some(node);
        while let Some(state) = current {
            if state == ancestor {
                return state != node;
            }
            current = self.parent[state as usize];
        }
        false
    }

    fn is_legacy_history_binding(&self, state: u16) -> bool {
        matches!(self.kind[state as usize], NodeKind::History(_))
            && (!self.children[state as usize].is_empty() || self.is_legacy_active_history(state))
    }

    fn is_legacy_active_history(&self, state: u16) -> bool {
        if !matches!(self.kind[state as usize], NodeKind::History(_)) {
            return false;
        }
        for seed in 0..self.kind.len() {
            if !self.is_legacy_history_seed(seed as u16) {
                continue;
            }
            let mut seen = BTreeSet::new();
            let mut current = seed as u16;
            loop {
                if !seen.insert(current) {
                    // `initial_descent(seed)` safely falls back to the seed
                    // when this malformed chain cycles.
                    if seed as u16 == state {
                        return true;
                    }
                    break;
                }
                match self.initial_child[current as usize] {
                    Some(initial) => current = initial,
                    None => {
                        if current == state {
                            return true;
                        }
                        break;
                    }
                }
            }
        }
        false
    }

    fn is_legacy_history_seed(&self, state: u16) -> bool {
        let NodeKind::History(kind) = self.kind[state as usize] else {
            return false;
        };
        if self.children[state as usize].is_empty() {
            return false;
        }
        let can_be_shallow_binding = kind == HistoryKind::Shallow
            && self.parent[state as usize]
                .is_some_and(|owner| matches!(self.kind[owner as usize], NodeKind::Compound));
        let can_own_targeted_history = self.children[state as usize]
            .iter()
            .any(|child| matches!(self.kind[*child as usize], NodeKind::History(_)));
        can_be_shallow_binding || can_own_targeted_history
    }

    pub fn proper_lca(&self, a: u16, b: u16) -> Option<u16> {
        let mut x = self.parent[a as usize];
        let mut y = self.parent[b as usize];
        while depth_of(x, &self.depth) > depth_of(y, &self.depth) {
            x = x.and_then(|i| self.parent[i as usize]);
        }
        while depth_of(y, &self.depth) > depth_of(x, &self.depth) {
            y = y.and_then(|i| self.parent[i as usize]);
        }
        while x != y {
            x = x.and_then(|i| self.parent[i as usize]);
            y = y.and_then(|i| self.parent[i as usize]);
        }
        x
    }

    pub fn exit_set(&self, leaf: u16, dom: Option<u16>) -> Vec<u16> {
        let mut out = Vec::new();
        let mut cur = Some(leaf);
        while let Some(i) = cur {
            if Some(i) == dom {
                break;
            }
            out.push(i);
            cur = self.parent[i as usize];
        }
        out
    }

    pub fn entry_path(&self, dom: Option<u16>, target: u16) -> Vec<u16> {
        let mut walk = Vec::new();
        let mut cur = Some(target);
        while let Some(i) = cur {
            if Some(i) == dom {
                break;
            }
            walk.push(i);
            cur = self.parent[i as usize];
        }
        walk.reverse();
        walk
    }

    pub fn initial_descent(&self, from: u16) -> Vec<u16> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cur = self.initial_child[from as usize];
        while let Some(i) = cur {
            // Valid definitions descend strictly through the tree and cannot
            // cycle. Historical malformed history initials could point
            // globally (including at one another); old execution would hang,
            // so no sealed Applied outcome exists to reproduce. Returning no
            // descent keeps history restoration at the last structurally
            // entered, compatibility-valid history node. A current-valid
            // definition cannot trigger this branch because history nodes
            // cannot carry `initial`.
            if !seen.insert(i) {
                return Vec::new();
            }
            out.push(i);
            cur = self.initial_child[i as usize];
        }
        out
    }

    /// Dotted display path from the nearest compound ancestor, e.g. `in_review.docs_review`.
    pub fn dotted_path(&self, leaf: &str) -> String {
        let Some(id) = self.id(leaf) else {
            return leaf.to_string();
        };
        let mut names = Vec::new();
        let mut cur = Some(id);
        while let Some(i) = cur {
            names.push(self.names[i as usize].clone());
            cur = self.parent[i as usize];
        }
        names.reverse();
        names.join(".")
    }

    /// Every state name on the active configuration path: each active leaf
    /// plus its compound ancestors, unioned across all regions.
    ///
    /// This is the membership set the `in(state)` invariant predicate tests.
    pub fn active_state_names(&self, configuration: &ActiveConfiguration) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        match configuration {
            ActiveConfiguration::Sequential { leaf } => {
                out.extend(self.configuration(leaf));
            }
            ActiveConfiguration::Parallel { leaves } => {
                for leaf in leaves.values() {
                    out.extend(self.configuration(leaf));
                }
            }
        }
        out
    }

    /// Active configuration: ancestors then leaf, root-first, excluding history nodes.
    pub fn configuration(&self, leaf: &str) -> Vec<String> {
        let Some(id) = self.id(leaf) else {
            return vec![leaf.to_string()];
        };
        let mut names = Vec::new();
        let mut cur = Some(id);
        while let Some(i) = cur {
            if !matches!(self.kind[i as usize], NodeKind::History(_)) {
                names.push(self.names[i as usize].clone());
            }
            cur = self.parent[i as usize];
        }
        names.reverse();
        names
    }

    pub fn history_owner(&self, hist: u16) -> Option<u16> {
        self.parent[hist as usize]
    }

    pub fn history_descent(&self, hist: u16, binding: Option<&str>) -> Vec<u16> {
        let owner = match self.history_owner(hist) {
            Some(o) => o,
            None => return Vec::new(),
        };
        let kind = match &self.kind[hist as usize] {
            NodeKind::History(k) => *k,
            _ => return self.initial_descent(owner),
        };
        match (kind, binding) {
            (_, None) => self.initial_descent(owner),
            (crate::spec::HistoryKind::Deep, Some(name)) => {
                if let Some(leaf) = self.id(name) {
                    // path from just below owner down to leaf
                    self.entry_path(Some(owner), leaf)
                } else {
                    self.initial_descent(owner)
                }
            }
            (crate::spec::HistoryKind::Shallow, Some(name)) => {
                if let Some(child) = self.id(name) {
                    let mut out = vec![child];
                    out.extend(self.initial_descent(child));
                    out
                } else {
                    self.initial_descent(owner)
                }
            }
        }
    }
}

fn depth_of(x: Option<u16>, depth: &[u8]) -> u8 {
    x.map(|i| depth[i as usize]).unwrap_or(0)
}
