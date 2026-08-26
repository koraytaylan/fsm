//! Names, trees, initials, history, terminals, regions, declarations, and
//! the definition ceilings — the rules that hold before any transition is
//! examined.

use std::collections::{BTreeMap, BTreeSet};

use crate::limits;

use super::super::{Finding, MachineSpec, StateNode, Topology, TySpec};

/// Name-indexed views of every state node, built once by [`collect_states`]
/// and read by every later phase.
pub(super) struct StateTables<'a> {
    pub(super) names: BTreeSet<String>,
    pub(super) by_name: BTreeMap<String, &'a StateNode>,
    pub(super) parent: BTreeMap<String, Option<String>>,
    pub(super) region_by_state: BTreeMap<String, Option<String>>,
}

pub(super) fn check_regions(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    if let Topology::Parallel { regions } = &spec.topology {
        if regions.len() < 2 {
            errs.push(Finding::err(
                "def/shape",
                "/regions",
                "parallel machines require at least two regions",
                "declare two or more regions, or use states with initial",
            ));
        }
        if regions.len() > limits::MAX_REGIONS {
            errs.push(Finding::err(
                "def/limit_regions",
                "/regions",
                "more than 8 parallel regions",
                "reduce the region count to 8",
            ));
        }
        let mut region_names = BTreeSet::new();
        for (index, region) in regions.iter().enumerate() {
            if !region_names.insert(region.name.as_str()) {
                errs.push(Finding::err(
                    "def/dup_name",
                    format!("/regions/{index}/name"),
                    format!("duplicate region {}", region.name),
                    "rename one of the regions",
                ));
            }
            if region.name.starts_with('$') {
                errs.push(Finding::err(
                    "def/reserved_ident",
                    format!("/regions/{index}/name"),
                    "$-prefixed region names are reserved",
                    "remove the $ prefix",
                ));
            }
        }
    }
}

pub(super) fn collect_states<'a>(
    spec: &'a MachineSpec,
    errs: &mut Vec<Finding>,
) -> StateTables<'a> {
    let mut names = BTreeSet::new();
    let mut by_name: BTreeMap<String, &StateNode> = BTreeMap::new();
    let mut parent: BTreeMap<String, Option<String>> = BTreeMap::new();
    let mut region_by_state: BTreeMap<String, Option<String>> = BTreeMap::new();
    fn collect<'a>(
        nodes: &'a [StateNode],
        par: Option<String>,
        region: Option<&str>,
        names: &mut BTreeSet<String>,
        by_name: &mut BTreeMap<String, &'a StateNode>,
        parent: &mut BTreeMap<String, Option<String>>,
        region_by_state: &mut BTreeMap<String, Option<String>>,
        errs: &mut Vec<Finding>,
        depth: u32,
    ) {
        for n in nodes {
            if depth > limits::MAX_NESTING {
                errs.push(Finding::err(
                    "def/limit_depth",
                    "/states",
                    "nesting exceeds 12",
                    "flatten the tree",
                ));
            }
            if !names.insert(n.name.clone()) {
                errs.push(Finding::err(
                    "def/dup_name",
                    format!("/states/{}", n.name),
                    format!("duplicate name {}", n.name),
                    "rename one of the nodes",
                ));
            }
            if n.name.starts_with('$') {
                errs.push(Finding::err(
                    "def/reserved_ident",
                    format!("/states/{}", n.name),
                    "$-prefixed names are reserved",
                    "remove the $ prefix",
                ));
            }
            by_name.insert(n.name.clone(), n);
            parent.insert(n.name.clone(), par.clone());
            region_by_state.insert(n.name.clone(), region.map(str::to_string));
            collect(
                &n.states,
                Some(n.name.clone()),
                region,
                names,
                by_name,
                parent,
                region_by_state,
                errs,
                depth + 1,
            );
        }
    }
    for (region, states, _) in spec.state_groups() {
        collect(
            states,
            None,
            region,
            &mut names,
            &mut by_name,
            &mut parent,
            &mut region_by_state,
            errs,
            1,
        );
    }
    StateTables {
        names,
        by_name,
        parent,
        region_by_state,
    }
}

pub(super) fn check_state_count(tables: &StateTables<'_>, errs: &mut Vec<Finding>) {
    let names = &tables.names;
    if names.len() > limits::MAX_STATES {
        errs.push(Finding::err(
            "def/limit_states",
            "/states",
            "more than 256 state nodes",
            "reduce the machine",
        ));
    }
}

pub(super) fn check_nodes(
    tables: &StateTables<'_>,
    permits_legacy_history_shapes: bool,
    errs: &mut Vec<Finding>,
) {
    let by_name = &tables.by_name;
    let parent = &tables.parent;
    let mut hist_count = 0usize;
    for (n, node) in by_name {
        if node.history.is_some() {
            hist_count += 1;
            let has_compound_owner = parent
                .get(n)
                .and_then(Option::as_ref)
                .and_then(|owner| by_name.get(owner))
                .is_some_and(|owner| owner.history.is_none() && !owner.states.is_empty());
            if !has_compound_owner && !permits_legacy_history_shapes {
                errs.push(Finding::err(
                    "def/shape",
                    format!("/states/{n}/history"),
                    "history pseudostate must have a compound owner",
                    "move history under the compound whose configuration it remembers",
                ));
            }
            if (!node.states.is_empty() || node.terminal || node.initial.is_some())
                && !permits_legacy_history_shapes
            {
                errs.push(Finding::err(
                    "def/shape",
                    format!("/states/{n}"),
                    "history pseudostate must be childless, non-terminal, and have no initial",
                    "remove states, terminal, and initial from the history node",
                ));
            }
        }
        if node.history.is_none() && !node.states.is_empty() {
            match &node.initial {
                None => errs.push(Finding::err(
                    "def/one_initial",
                    format!("/states/{n}"),
                    "compound needs initial",
                    "set initial to a direct child",
                )),
                Some(init) => {
                    let child = node.states.iter().find(|c| c.name == *init);
                    match child {
                        None => {
                            if by_name.contains_key(init) {
                                errs.push(Finding::err(
                                    "def/initial_not_child",
                                    format!("/states/{n}/initial"),
                                    "initial is not a direct child",
                                    "name a direct real child",
                                ));
                            } else {
                                errs.push(Finding::err(
                                    "def/unknown_state",
                                    format!("/states/{n}/initial"),
                                    format!("unknown initial {init}"),
                                    "name a declared state",
                                ));
                            }
                        }
                        Some(c) if c.history.is_some() => {
                            errs.push(Finding::err(
                                "def/initial_is_history",
                                format!("/states/{n}/initial"),
                                "initial cannot be a history pseudostate",
                                "name a real child",
                            ));
                        }
                        Some(_) => {}
                    }
                }
            }
            let hists: Vec<_> = node.states.iter().filter(|c| c.history.is_some()).collect();
            if hists.len() > 1 {
                errs.push(Finding::err(
                    "def/multiple_history",
                    format!("/states/{n}"),
                    "at most one history per compound",
                    "remove extra history nodes",
                ));
            }
        }
        if node.terminal && !node.states.is_empty() {
            errs.push(Finding::err(
                "def/terminal_not_leaf",
                format!("/states/{n}"),
                "terminal must be a leaf",
                "remove children or terminal",
            ));
        }
    }
    if hist_count > limits::MAX_HISTORY {
        errs.push(Finding::err(
            "def/limit_history",
            "/states",
            "more than 32 history nodes",
            "reduce history",
        ));
    }
}

pub(super) fn check_initial_chains(
    spec: &MachineSpec,
    tables: &StateTables<'_>,
    errs: &mut Vec<Finding>,
) {
    let by_name = &tables.by_name;
    for (region_index, (_, states, initial)) in spec.state_groups().into_iter().enumerate() {
        let initial_path = match &spec.topology {
            Topology::Sequential { .. } => "/initial".to_string(),
            Topology::Parallel { .. } => format!("/regions/{region_index}/initial"),
        };
        if !by_name.contains_key(initial) {
            errs.push(Finding::err(
                "def/unknown_state",
                &initial_path,
                format!("unknown initial {initial}"),
                "name a top-level state in this region",
            ));
        } else if !states.iter().any(|state| state.name == initial) {
            errs.push(Finding::err(
                "def/initial_not_child",
                &initial_path,
                "initial is not a top-level state in this region",
                "name a direct top-level child",
            ));
        } else if states
            .iter()
            .find(|state| state.name == initial)
            .and_then(|state| state.history)
            .is_some()
        {
            errs.push(Finding::err(
                "def/initial_is_history",
                &initial_path,
                "initial cannot be a history pseudostate",
                "name a real top-level state",
            ));
        } else {
            // Walk the actual child objects rather than resolving each name
            // through the global index. Rejected definitions may contain
            // duplicate names, and following that lossy index could otherwise
            // jump between unrelated subtrees or cycle on hostile input.
            let mut node = states
                .iter()
                .find(|state| state.name == initial)
                .expect("the direct-child check above established the initial");
            loop {
                if node.states.is_empty() {
                    if node.terminal {
                        errs.push(Finding::err(
                            "def/initial_terminal",
                            &initial_path,
                            "creation chain lands on a terminal",
                            "start in a non-terminal leaf",
                        ));
                    }
                    break;
                }
                match node.initial.as_deref() {
                    Some(next) => match node.states.iter().find(|child| child.name == next) {
                        Some(child) => node = child,
                        None => break,
                    },
                    _ => break,
                }
            }
        }
    }
}

pub(super) fn check_declarations(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    let mut seen_fx = BTreeSet::new();
    for ev in &spec.effects {
        if !seen_fx.insert(ev.name.as_str()) {
            errs.push(Finding::err(
                "def/dup_name",
                format!("/effects/{}", ev.name),
                format!("duplicate effect {}", ev.name),
                "rename one of the effects",
            ));
        }
        let mut seen_f = BTreeSet::new();
        for f in &ev.fields {
            if !seen_f.insert(f.name.as_str()) {
                errs.push(Finding::err(
                    "def/dup_name",
                    format!("/effects/{}/{}", ev.name, f.name),
                    format!("duplicate field {}", f.name),
                    "rename one of the fields",
                ));
            }
        }
    }
    let mut seen_ctx = BTreeSet::new();
    for c in &spec.context {
        if !seen_ctx.insert(c.name.as_str()) {
            errs.push(Finding::err(
                "def/dup_name",
                format!("/context/{}", c.name),
                format!("duplicate context {}", c.name),
                "rename one of the variables",
            ));
        }
    }
    let mut seen_ev = BTreeSet::new();
    for e in &spec.events {
        if !seen_ev.insert(e.name.as_str()) {
            errs.push(Finding::err(
                "def/dup_name",
                format!("/events/{}", e.name),
                format!("duplicate event {}", e.name),
                "rename one of the events",
            ));
        }
        let mut seen_f = BTreeSet::new();
        for f in &e.fields {
            if !seen_f.insert(f.name.as_str()) {
                errs.push(Finding::err(
                    "def/dup_name",
                    format!("/events/{}/{}", e.name, f.name),
                    format!("duplicate field {}", f.name),
                    "rename one of the fields",
                ));
            }
        }
        if e.name.starts_with('$') {
            errs.push(Finding::err(
                "def/reserved_ident",
                format!("/events/{}", e.name),
                "$-prefixed identifiers are reserved",
                "remove the $ prefix",
            ));
        }
    }
    if spec.events.len() > limits::MAX_EVENTS {
        errs.push(Finding::err(
            "def/limit_events",
            "/events",
            "more than 128 events",
            "reduce events",
        ));
    }
    if spec.enums.len() > limits::MAX_ENUMS {
        errs.push(Finding::err(
            "def/limit_enums",
            "/enums",
            "more than 32 enums",
            "reduce enums",
        ));
    }
    for (en, vars) in &spec.enums {
        if vars.len() > limits::MAX_VARIANTS {
            errs.push(Finding::err(
                "def/limit_variants",
                format!("/enums/{en}"),
                "more than 64 variants",
                "reduce variants",
            ));
        }
    }
    if spec.context.len() > limits::MAX_CTX_VARS {
        errs.push(Finding::err(
            "def/limit_ctx",
            "/context",
            "more than 64 context variables",
            "reduce context",
        ));
    }
    if spec.transitions.len() > limits::MAX_TRANSITIONS {
        errs.push(Finding::err(
            "def/limit_transitions",
            "/transitions",
            "more than 2048 transitions",
            "reduce transitions",
        ));
    }
    if spec.invariants.len() > limits::MAX_INVARIANTS {
        errs.push(Finding::err(
            "def/limit_invariants",
            "/invariants",
            "more than 64 invariants",
            "reduce invariants",
        ));
    }
}

pub(super) fn check_field_counts(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    for ev in &spec.events {
        if ev.fields.len() > limits::MAX_FIELDS {
            errs.push(Finding::err(
                "def/limit_fields",
                format!("/events/{}", ev.name),
                "more than 32 fields",
                "reduce fields",
            ));
        }
    }
    for ev in &spec.effects {
        if ev.fields.len() > limits::MAX_FIELDS {
            errs.push(Finding::err(
                "def/limit_fields",
                format!("/effects/{}", ev.name),
                "more than 32 fields",
                "reduce fields",
            ));
        }
    }
}

pub(super) fn check_enum_references(spec: &MachineSpec, errs: &mut Vec<Finding>) {
    // enum refs in context
    for c in &spec.context {
        if let TySpec::Enum { of } = &c.ty {
            if !spec.enums.contains_key(of) {
                errs.push(Finding::err(
                    "def/unknown_enum",
                    format!("/context/{}", c.name),
                    format!("unknown enum {of}"),
                    "declare the enum",
                ));
            } else if !spec.enums[of].iter().any(|v| v == &c.init) {
                errs.push(Finding::err(
                    "def/shape",
                    format!("/context/{}/init", c.name),
                    format!("unknown variant {}", c.init),
                    "use a declared variant",
                ));
            }
        }
    }
    for ev in &spec.events {
        for f in &ev.fields {
            if let TySpec::Enum { of } = &f.ty {
                if !spec.enums.contains_key(of) {
                    errs.push(Finding::err(
                        "def/unknown_enum",
                        format!("/events/{}/fields/{}", ev.name, f.name),
                        format!("unknown enum {of}"),
                        "declare the enum",
                    ));
                }
            }
        }
    }
    for ev in &spec.effects {
        for f in &ev.fields {
            if let TySpec::Enum { of } = &f.ty {
                if !spec.enums.contains_key(of) {
                    errs.push(Finding::err(
                        "def/unknown_enum",
                        format!("/effects/{}/fields/{}", ev.name, f.name),
                        format!("unknown enum {of}"),
                        "declare the enum",
                    ));
                }
            }
        }
    }
}
