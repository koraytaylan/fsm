//! Transitions, deadlines, and the per-block ceilings.

use std::collections::{BTreeMap, BTreeSet};

use crate::limits;

use super::super::{EmitSpec, Finding, MachineSpec, SetSpec, StateNode, Topology};
use super::structure::StateTables;

fn check_block_limits(
    sets: &[SetSpec],
    emits: &[EmitSpec],
    effect_names: &BTreeSet<&str>,
    path: &str,
    errs: &mut Vec<Finding>,
) {
    if sets.len() > limits::MAX_SETS_PER_BLOCK {
        errs.push(Finding::err(
            "def/limit_sets",
            path,
            "more than 32 sets in one block",
            "split the block",
        ));
    }
    if emits.len() > limits::MAX_EMITS_PER_BLOCK {
        errs.push(Finding::err(
            "def/limit_emits",
            path,
            "more than 8 emits in one block",
            "split the block",
        ));
    }
    for em in emits {
        if !effect_names.contains(em.effect.as_str()) {
            errs.push(Finding::err(
                "def/unknown_effect",
                format!("{path}/emit"),
                format!("unknown effect {}", em.effect),
                "declare the effect",
            ));
        }
    }
}

/// Check every transition and return the `(from, on)` cell populations for
/// [`check_cell_limits`].
pub(super) fn check_transitions(
    spec: &MachineSpec,
    tables: &StateTables<'_>,
    event_names: &BTreeSet<&str>,
    effect_names: &BTreeSet<&str>,
    permits_legacy_history_shapes: bool,
    errs: &mut Vec<Finding>,
) -> BTreeMap<(String, String), usize> {
    let by_name = &tables.by_name;
    let parent = &tables.parent;
    let region_by_state = &tables.region_by_state;
    let mut cell: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (i, t) in spec.transitions.iter().enumerate() {
        let p = format!("/transitions/{i}");
        if !by_name.contains_key(&t.from) {
            errs.push(Finding::err(
                "def/unknown_state",
                format!("{p}/from"),
                format!("unknown from {}", t.from),
                "name a declared state",
            ));
        } else {
            let src = by_name[&t.from];
            if src.terminal {
                errs.push(Finding::err(
                    "def/terminal_has_transitions",
                    format!("{p}/from"),
                    "terminal cannot be a source",
                    "do not transition from a terminal",
                ));
            }
            if src.history.is_some() {
                errs.push(Finding::err(
                    "def/from_history",
                    format!("{p}/from"),
                    "history cannot be a source",
                    "use the owner compound",
                ));
            }
        }
        if !event_names.contains(t.on.as_str()) {
            errs.push(Finding::err(
                "def/unknown_event",
                format!("{p}/on"),
                format!("unknown event {}", t.on),
                "declare the event",
            ));
        }
        if let Some(to) = &t.to {
            if !by_name.contains_key(to) {
                errs.push(Finding::err(
                    "def/unknown_state",
                    format!("{p}/to"),
                    format!("unknown to {to}"),
                    "name a declared state",
                ));
            } else if by_name.contains_key(&t.from)
                && region_by_state.get(&t.from) != region_by_state.get(to)
            {
                errs.push(Finding::err(
                    "def/cross_region",
                    format!("{p}/to"),
                    "transition source and target are in different regions",
                    "target a state in the source region",
                ));
            } else if let Some(h) = by_name[to].history {
                let _ = h;
                // owner is parent of history node
                let owner = parent.get(to).and_then(|p| p.clone());
                if let Some(own) = owner {
                    // source must be outside owner
                    let mut walk = Some(t.from.clone());
                    let mut inside = false;
                    while let Some(cur) = walk {
                        if cur == own {
                            inside = true;
                            break;
                        }
                        walk = parent.get(&cur).and_then(|p| p.clone());
                    }
                    if inside {
                        errs.push(Finding::err(
                            "def/history_target_from_inside",
                            format!("{p}/to"),
                            "history may only be targeted from outside its owner",
                            "target a real child instead",
                        ));
                    }
                } else if !permits_legacy_history_shapes {
                    errs.push(Finding::err(
                        "def/shape",
                        format!("{p}/to"),
                        "top-level history target has no compound owner",
                        "move history under a compound and target it from outside that owner",
                    ));
                }
            }
        }
        check_block_limits(&t.sets, &t.emits, effect_names, &p, errs);
        *cell.entry((t.from.clone(), t.on.clone())).or_insert(0) += 1;
    }
    cell
}

pub(super) fn check_deadlines(
    spec: &MachineSpec,
    tables: &StateTables<'_>,
    effect_names: &BTreeSet<&str>,
    errs: &mut Vec<Finding>,
) {
    let by_name = &tables.by_name;
    let parent = &tables.parent;
    let region_by_state = &tables.region_by_state;
    if spec.deadlines.len() > limits::MAX_DEADLINES {
        errs.push(Finding::err(
            "def/limit_deadlines",
            "/deadlines",
            "more than 128 deadlines",
            "reduce the deadline count to 128",
        ));
    }
    let mut deadline_names = BTreeSet::new();
    for (index, deadline) in spec.deadlines.iter().enumerate() {
        let path = format!("/deadlines/{index}");
        if !deadline_names.insert(deadline.name.as_str()) {
            errs.push(Finding::err(
                "def/duplicate_deadline",
                format!("{path}/name"),
                format!("duplicate deadline {}", deadline.name),
                "give every deadline a unique name",
            ));
        }
        if deadline.name.starts_with('$') {
            errs.push(Finding::err(
                "def/reserved_ident",
                format!("{path}/name"),
                "$-prefixed deadline names are reserved",
                "remove the $ prefix",
            ));
        }
        match by_name.get(&deadline.from) {
            None => errs.push(Finding::err(
                "def/unknown_state",
                format!("{path}/from"),
                format!("unknown from {}", deadline.from),
                "name a declared state",
            )),
            Some(source) if source.terminal => errs.push(Finding::err(
                "def/terminal_has_transitions",
                format!("{path}/from"),
                "terminal cannot be a deadline source",
                "move the deadline to a non-terminal state",
            )),
            Some(source) if source.history.is_some() => errs.push(Finding::err(
                "def/from_history",
                format!("{path}/from"),
                "history cannot be a deadline source",
                "use the owner compound",
            )),
            Some(_) => {}
        }
        if !by_name.contains_key(&deadline.to) {
            errs.push(Finding::err(
                "def/unknown_state",
                format!("{path}/to"),
                format!("unknown to {}", deadline.to),
                "name a declared state",
            ));
        } else if by_name.contains_key(&deadline.from)
            && region_by_state.get(&deadline.from) != region_by_state.get(&deadline.to)
        {
            errs.push(Finding::err(
                "def/cross_region",
                format!("{path}/to"),
                "deadline source and target are in different regions",
                "target a state in the source region",
            ));
        } else if by_name[&deadline.to].history.is_some() {
            let owner = parent.get(&deadline.to).and_then(|parent| parent.clone());
            if let Some(owner) = owner {
                let mut current = Some(deadline.from.clone());
                let mut inside = false;
                while let Some(state) = current {
                    if state == owner {
                        inside = true;
                        break;
                    }
                    current = parent.get(&state).and_then(|parent| parent.clone());
                }
                if inside {
                    errs.push(Finding::err(
                        "def/history_target_from_inside",
                        format!("{path}/to"),
                        "history may only be targeted from outside its owner",
                        "target a real child instead",
                    ));
                }
            } else {
                errs.push(Finding::err(
                    "def/shape",
                    format!("{path}/to"),
                    "top-level history target has no compound owner",
                    "move history under a compound and target it from outside that owner",
                ));
            }
        }
        check_block_limits(&deadline.sets, &deadline.emits, effect_names, &path, errs);
    }
}

pub(super) fn check_state_block_limits(
    spec: &MachineSpec,
    effect_names: &BTreeSet<&str>,
    errs: &mut Vec<Finding>,
) {
    fn walk_state_blocks(
        nodes: &[StateNode],
        path: &str,
        effect_names: &BTreeSet<&str>,
        errs: &mut Vec<Finding>,
    ) {
        for (i, n) in nodes.iter().enumerate() {
            let p = format!("{path}/{i}");
            if let Some(b) = &n.entry {
                check_block_limits(&b.sets, &b.emits, effect_names, &format!("{p}/entry"), errs);
            }
            if let Some(b) = &n.exit {
                check_block_limits(&b.sets, &b.emits, effect_names, &format!("{p}/exit"), errs);
            }
            walk_state_blocks(&n.states, &format!("{p}/states"), effect_names, errs);
        }
    }
    for (region_index, (_, states, _)) in spec.state_groups().into_iter().enumerate() {
        let path = match &spec.topology {
            Topology::Sequential { .. } => "/states".to_string(),
            Topology::Parallel { .. } => format!("/regions/{region_index}/states"),
        };
        walk_state_blocks(states, &path, effect_names, errs);
    }
}

pub(super) fn check_cell_limits(cell: BTreeMap<(String, String), usize>, errs: &mut Vec<Finding>) {
    for ((from, on), n) in cell {
        if n > limits::MAX_TRANSITIONS_PER_CELL {
            errs.push(Finding::err(
                "def/limit_cell",
                format!("/transitions/{from}/{on}"),
                "more than 32 transitions per (state, event)",
                "collapse handlers",
            ));
        }
    }
}
