use std::collections::{BTreeMap, BTreeSet};

use crate::limits;

use super::{EmitSpec, Finding, MachineSpec, SetSpec, StateNode, Topology, TySpec};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DefinitionCompatibility {
    Current,
    HistoricalPersistence,
}

impl DefinitionCompatibility {
    fn permits_legacy_history_shapes(self, spec: &MachineSpec) -> bool {
        self == Self::HistoricalPersistence
            && matches!(spec.topology, Topology::Sequential { .. })
            && spec.deadlines.is_empty()
    }
}

pub fn validate(spec: &MachineSpec) -> Result<(), Vec<Finding>> {
    validate_with_compatibility(spec, DefinitionCompatibility::Current)
}

pub(super) fn validate_with_compatibility(
    spec: &MachineSpec,
    compatibility: DefinitionCompatibility,
) -> Result<(), Vec<Finding>> {
    let mut errs = Vec::new();
    let permits_legacy_history_shapes = compatibility.permits_legacy_history_shapes(spec);
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
            &mut errs,
            1,
        );
    }
    if names.len() > limits::MAX_STATES {
        errs.push(Finding::err(
            "def/limit_states",
            "/states",
            "more than 256 state nodes",
            "reduce the machine",
        ));
    }
    let mut hist_count = 0usize;
    for (n, node) in &by_name {
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
    let event_names: BTreeSet<_> = spec.events.iter().map(|e| e.name.as_str()).collect();
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
    let effect_names: BTreeSet<_> = spec.effects.iter().map(|e| e.name.as_str()).collect();
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
        check_block_limits(&t.sets, &t.emits, &effect_names, &p, &mut errs);
        *cell.entry((t.from.clone(), t.on.clone())).or_insert(0) += 1;
    }
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
        check_block_limits(
            &deadline.sets,
            &deadline.emits,
            &effect_names,
            &path,
            &mut errs,
        );
    }
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
        walk_state_blocks(states, &path, &effect_names, &mut errs);
    }
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
    if errs.is_empty() { Ok(()) } else { Err(errs) }
}
