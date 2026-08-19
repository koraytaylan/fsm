use super::deadline::{clear_terminal_region_deadlines, update_deadline_schedules};
use super::eval::{apply_block, eval_bool, eval_invariants, is_compound, naive_validate, reject};
use super::*;

pub(super) struct SelectedCandidate {
    region: Option<String>,
    leaf: String,
    source: String,
    transition_index: usize,
}

/// Apply SPEC's global scan literally: regions in document order, then each
/// recursive leaf-to-root chain, then transitions in document order.
fn select_event_candidate(
    spec: &MachineSpec,
    active: &[ActiveLeaf<'_>],
    state: &InstanceState,
    event: &str,
    fields: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<(Option<SelectedCandidate>, bool), Rejection> {
    let mut any_candidate = false;
    for active_leaf in active {
        if find(active_leaf.states, &active_leaf.leaf).is_some_and(|node| node.terminal) {
            continue;
        }
        for source in chain(active_leaf.states, &active_leaf.leaf) {
            for (transition_index, transition) in spec.transitions.iter().enumerate() {
                if transition.from != source || transition.on != event {
                    continue;
                }
                any_candidate = true;
                match eval_bool(transition.guard.as_deref(), &state.ctx, fields, budget) {
                    Ok(true) => {
                        return Ok((
                            Some(SelectedCandidate {
                                region: active_leaf.region.map(str::to_string),
                                leaf: active_leaf.leaf.clone(),
                                source,
                                transition_index,
                            }),
                            true,
                        ));
                    }
                    Ok(false) => {}
                    Err(rejection) => return Err(rejection),
                }
            }
        }
    }
    Ok((None, any_candidate))
}

pub(super) fn states_for_region<'a>(
    spec: &'a MachineSpec,
    region_name: Option<&str>,
) -> Option<&'a [StateNode]> {
    match (&spec.topology, region_name) {
        (Topology::Sequential { states, .. }, None) => Some(states),
        (Topology::Parallel { regions }, Some(region_name)) => regions
            .iter()
            .find(|region| region.name == region_name)
            .map(|region| region.states.as_slice()),
        _ => None,
    }
}

pub(super) fn configuration_with_leaf(
    configuration: &ActiveConfiguration,
    region_name: Option<&str>,
    leaf: String,
) -> Option<ActiveConfiguration> {
    match (configuration, region_name) {
        (ActiveConfiguration::Sequential { .. }, None) => {
            Some(ActiveConfiguration::Sequential { leaf })
        }
        (ActiveConfiguration::Parallel { leaves }, Some(region_name))
            if leaves.contains_key(region_name) =>
        {
            let mut leaves = leaves.clone();
            leaves.insert(region_name.to_string(), leaf);
            Some(ActiveConfiguration::Parallel { leaves })
        }
        _ => None,
    }
}

pub fn naive_step(
    m: &CompiledMachine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    budget: &mut Budget,
) -> Outcome {
    naive_step_at(m, st, event, payload, 0, budget)
}

pub fn naive_step_at(
    m: &CompiledMachine,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    if st.status != Status::Running {
        return Outcome::Rejected(Rejection {
            code: if st.status == Status::Completed {
                "run/instance_completed"
            } else {
                "run/instance_cancelled"
            },
            message: "not running".into(),
            hint: "create a new instance".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace: Default::default(),
            cause: None,
        });
    }
    let fields = match naive_validate(&m.spec, event, payload) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let active = match active_leaves(&m.spec, &st.configuration) {
        Some(active) => active,
        None => return Outcome::Rejected(reject("run/unhandled", "invalid configuration")),
    };
    let (winner, any_candidate) =
        match select_event_candidate(&m.spec, &active, st, event, &fields, budget) {
            Ok(selection) => selection,
            Err(rejection) => return Outcome::Rejected(rejection),
        };
    let Some(winner) = winner else {
        if !any_candidate {
            return match m.spec.on_unhandled {
                fsm_core::spec::Unhandled::Ignore => Outcome::Ignored,
                fsm_core::spec::Unhandled::Reject => Outcome::Rejected(Rejection {
                    code: "run/unhandled",
                    message: "unhandled".into(),
                    hint: "n".into(),
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: None,
                    trace: Default::default(),
                    cause: None,
                }),
            };
        }
        return Outcome::Rejected(Rejection {
            code: "run/not_enabled",
            message: "not enabled".into(),
            hint: "n".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace: Default::default(),
            cause: None,
        });
    };
    let Some(states) = states_for_region(&m.spec, winner.region.as_deref()) else {
        return Outcome::Rejected(reject("run/unhandled", "invalid transition region"));
    };
    let tr = &m.spec.transitions[winner.transition_index];
    let internal = tr.to.is_none();
    let (exited, entered, new_leaf) = if internal {
        (Vec::new(), Vec::new(), winner.leaf.clone())
    } else {
        let mut target = tr.to.clone().unwrap();
        let mut extra = Vec::new();
        if let Some(tn) = find(states, &target) {
            if tn.history.is_some() {
                let owner = parent_of(states, &target).unwrap();
                extra = hist_descent(states, &target, st.history.get(&owner).map(String::as_str));
                target = owner;
            }
        }
        let external_self = tr.to.as_deref() == Some(winner.source.as_str());
        let dom = if external_self {
            parent_of(states, &winner.source)
        } else {
            lca(states, &winner.source, &target)
        };
        let exited = exit_set(states, &winner.leaf, &dom);
        let mut entered = entry_path(states, &dom, &target);
        if find(states, &target).is_some_and(is_compound) && extra.is_empty() {
            entered.extend(initial_descent(states, &target));
        }
        entered.extend(extra);
        let leaf = entered.last().cloned().unwrap_or(target);
        (exited, entered, leaf)
    };
    let Some(configuration_after) =
        configuration_with_leaf(&st.configuration, winner.region.as_deref(), new_leaf)
    else {
        return Outcome::Rejected(reject("run/unhandled", "invalid transition region"));
    };
    let mut ctx = st.ctx.clone();
    let mut effects = Vec::new();
    for name in &exited {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.exit {
                if let Err(r) = apply_block(b, &mut ctx, &fields, false, budget, &mut effects) {
                    return Outcome::Rejected(r);
                }
            }
        }
    }
    let tblock = fsm_core::spec::Block {
        sets: tr.sets.clone(),
        emits: tr.emits.clone(),
    };
    if let Err(r) = apply_block(&tblock, &mut ctx, &fields, true, budget, &mut effects) {
        return Outcome::Rejected(r);
    }
    for name in &entered {
        if let Some(node) = find(states, name) {
            if let Some(b) = &node.entry {
                if let Err(r) = apply_block(b, &mut ctx, &fields, false, budget, &mut effects) {
                    return Outcome::Rejected(r);
                }
            }
        }
    }
    let mut history_after = st.history.clone();
    for name in &exited {
        if let Some(node) = find(states, name) {
            if is_compound(node) {
                for ch in &node.states {
                    if let Some(hk) = ch.history {
                        let bound = match hk {
                            HistoryKind::Deep => winner.leaf.clone(),
                            HistoryKind::Shallow => chain(states, &winner.leaf)
                                .into_iter()
                                .find(|n| parent_of(states, n).as_deref() == Some(name.as_str()))
                                .unwrap_or_else(|| winner.leaf.clone()),
                        };
                        history_after.insert(name.clone(), bound);
                    }
                }
            }
        }
    }
    let flags = match eval_invariants(&m.spec, &ctx, budget) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let mut deadlines_after = match update_deadline_schedules(
        &m.spec,
        &st.deadlines,
        &exited,
        &entered,
        &ctx,
        now_ms,
        budget,
    ) {
        Ok(schedules) => schedules,
        Err(rejection) => return Outcome::Rejected(rejection),
    };
    clear_terminal_region_deadlines(&m.spec, &configuration_after, &mut deadlines_after);
    let status_after = if configuration_is_terminal(&m.spec, &configuration_after) {
        deadlines_after.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Outcome::Applied(Applied {
        configuration_after,
        ctx_after: ctx,
        history_after,
        deadlines_after,
        effects,
        monitor_flags: flags,
        status_after,
        internal,
        region: winner.region,
        source_state: winner.source,
        transition_idx: winner.transition_index as u32,
        exited,
        entered,
        trace: Default::default(),
    })
}
