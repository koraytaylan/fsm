use super::eval::{apply_block, is_compound, reject};
use super::step::{NaiveMicro, configuration_with_leaf, states_for_region};
use super::*;

fn deadline_rejection(
    deadline: &DeadlineSpec,
    message: String,
    hint: String,
    span: Option<(u32, u32)>,
    cause: Option<&'static str>,
) -> Rejection {
    Rejection {
        code: "run/action_error",
        message,
        hint,
        source_state: Some(deadline.from.clone()),
        transition_idx: None,
        block: Some(format!("deadline({})", deadline.name)),
        span,
        trace: Default::default(),
        cause,
    }
}

fn evaluate_deadline_after(
    deadline: &DeadlineSpec,
    ctx: &BTreeMap<String, Val>,
    budget: &mut Budget,
) -> Result<i64, Rejection> {
    let expression = parser::parse(&deadline.after).map_err(|error| {
        deadline_rejection(
            deadline,
            error.message,
            error.hint,
            Some((error.span.start, error.span.end)),
            Some(error.code),
        )
    })?;
    let bindings = Bindings {
        ctx,
        evt: None,
        active: None,
    };
    match eval(&expression, &bindings, budget, false).0 {
        Ok(Val::Dur(duration)) if duration >= 0 => Ok(duration),
        Ok(Val::Dur(_)) => Err(deadline_rejection(
            deadline,
            "deadline duration is negative".into(),
            "return a zero or positive duration".into(),
            None,
            Some("run/overflow"),
        )),
        Ok(_) => Err(deadline_rejection(
            deadline,
            "deadline expression did not return a duration".into(),
            "return a duration".into(),
            None,
            None,
        )),
        Err(error) => Err(deadline_rejection(
            deadline,
            error.message,
            error.hint,
            Some((error.span.start, error.span.end)),
            Some(error.code),
        )),
    }
}

/// Rebuild schedules by literally applying SPEC's exit cancellation followed
/// by entry-order/document-order scheduling. This deliberately does not read
/// production expression slots or deadline indices.
pub(super) fn update_deadline_schedules(
    spec: &MachineSpec,
    prior: &BTreeMap<String, i64>,
    exited: &[String],
    entered: &[String],
    ctx: &BTreeMap<String, Val>,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<BTreeMap<String, i64>, Rejection> {
    let mut schedules = prior.clone();
    for state in exited {
        for deadline in spec
            .deadlines
            .iter()
            .filter(|deadline| deadline.from == *state)
        {
            schedules.remove(&deadline.name);
        }
    }
    for state in entered {
        for deadline in spec
            .deadlines
            .iter()
            .filter(|deadline| deadline.from == *state)
        {
            let duration = evaluate_deadline_after(deadline, ctx, budget)?;
            let due_ms = now_ms.checked_add(duration).ok_or_else(|| {
                deadline_rejection(
                    deadline,
                    "deadline due timestamp overflowed".into(),
                    "use a smaller timestamp or duration".into(),
                    None,
                    Some("run/overflow"),
                )
            })?;
            schedules.insert(deadline.name.clone(), due_ms);
        }
    }
    Ok(schedules)
}

pub(super) fn clear_terminal_region_deadlines(
    spec: &MachineSpec,
    configuration: &ActiveConfiguration,
    schedules: &mut BTreeMap<String, i64>,
) {
    let Some(active) = active_leaves(spec, configuration) else {
        return;
    };
    for active_leaf in active {
        if !find(active_leaf.states, &active_leaf.leaf).is_some_and(|node| node.terminal) {
            continue;
        }
        let terminal_chain = chain(active_leaf.states, &active_leaf.leaf);
        schedules.retain(|name, _| {
            spec.deadlines
                .iter()
                .find(|deadline| deadline.name == *name)
                .is_none_or(|deadline| !terminal_chain.contains(&deadline.from))
        });
    }
}

struct SelectedDeadline {
    due_ms: i64,
    document_index: usize,
    region: Option<String>,
    leaf: String,
    source: String,
}

/// Select the minimum active `(due_ms, document index)` by scanning the
/// definition and recursive active chains directly. No production selector or
/// compiled deadline index participates in this oracle.
fn select_deadline(
    spec: &MachineSpec,
    active: &[ActiveLeaf<'_>],
    schedules: &BTreeMap<String, i64>,
) -> Option<SelectedDeadline> {
    let mut selected: Option<SelectedDeadline> = None;
    for (document_index, deadline) in spec.deadlines.iter().enumerate() {
        let Some(&due_ms) = schedules.get(&deadline.name) else {
            continue;
        };
        let source = active.iter().find_map(|active_leaf| {
            if find(active_leaf.states, &active_leaf.leaf).is_some_and(|node| node.terminal) {
                return None;
            }
            chain(active_leaf.states, &active_leaf.leaf)
                .into_iter()
                .find(|state| state == &deadline.from)
                .map(|source| {
                    (
                        active_leaf.region.map(str::to_string),
                        active_leaf.leaf.clone(),
                        source,
                    )
                })
        });
        let Some((region, leaf, source)) = source else {
            continue;
        };
        let candidate = SelectedDeadline {
            due_ms,
            document_index,
            region,
            leaf,
            source,
        };
        if selected.as_ref().is_none_or(|current| {
            (candidate.due_ms, candidate.document_index) < (current.due_ms, current.document_index)
        }) {
            selected = Some(candidate);
        }
    }
    selected
}

fn apply_naive_deadline(
    m: &CompiledMachine,
    st: &InstanceState,
    selected: &SelectedDeadline,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    let Some(states) = states_for_region(&m.spec, selected.region.as_deref()) else {
        return Outcome::Rejected(reject(
            "run/configuration_invalid",
            "invalid deadline region",
        ));
    };
    let deadline = &m.spec.deadlines[selected.document_index];
    let mut target = deadline.to.clone();
    let mut extra = Vec::new();
    if let Some(target_node) = find(states, &target)
        && target_node.history.is_some()
    {
        let owner = parent_of(states, &target).expect("compiled history has an owner");
        extra = hist_descent(states, &target, st.history.get(&owner).map(String::as_str));
        target = owner;
    }
    let domain = if deadline.to == selected.source {
        parent_of(states, &selected.source)
    } else {
        lca(states, &selected.source, &target)
    };
    let exited = exit_set(states, &selected.leaf, &domain);
    let mut entered = entry_path(states, &domain, &target);
    if find(states, &target).is_some_and(is_compound) && extra.is_empty() {
        entered.extend(initial_descent(states, &target));
    }
    entered.extend(extra);
    let new_leaf = entered.last().cloned().unwrap_or(target);
    let Some(configuration_after) =
        configuration_with_leaf(&st.configuration, selected.region.as_deref(), new_leaf)
    else {
        return Outcome::Rejected(reject(
            "run/configuration_invalid",
            "invalid deadline region",
        ));
    };

    let mut ctx = st.ctx.clone();
    let mut effects = Vec::new();
    let mut raised = Vec::new();
    let no_event = BTreeMap::new();
    for name in &exited {
        if let Some(block) = find(states, name).and_then(|node| node.exit.as_ref())
            && let Err(rejection) = apply_block(
                block,
                &mut ctx,
                &no_event,
                false,
                budget,
                &mut effects,
                &mut raised,
            )
        {
            return Outcome::Rejected(rejection);
        }
    }
    let deadline_block = Block {
        sets: deadline.sets.clone(),
        emits: deadline.emits.clone(),
        raises: deadline.raises.clone(),
    };
    if let Err(rejection) = apply_block(
        &deadline_block,
        &mut ctx,
        &no_event,
        false,
        budget,
        &mut effects,
        &mut raised,
    ) {
        return Outcome::Rejected(rejection);
    }
    for name in &entered {
        if let Some(block) = find(states, name).and_then(|node| node.entry.as_ref())
            && let Err(rejection) = apply_block(
                block,
                &mut ctx,
                &no_event,
                false,
                budget,
                &mut effects,
                &mut raised,
            )
        {
            return Outcome::Rejected(rejection);
        }
    }

    let mut history_after = st.history.clone();
    for name in &exited {
        if let Some(node) = find(states, name)
            && is_compound(node)
        {
            for child in &node.states {
                if let Some(kind) = child.history {
                    let bound = match kind {
                        HistoryKind::Deep => selected.leaf.clone(),
                        HistoryKind::Shallow => chain(states, &selected.leaf)
                            .into_iter()
                            .find(|state| {
                                parent_of(states, state).as_deref() == Some(name.as_str())
                            })
                            .unwrap_or_else(|| selected.leaf.clone()),
                    };
                    history_after.insert(name.clone(), bound);
                }
            }
        }
    }
    // The deadline's transition is the trigger of a macrostep like any
    // other: reactions, invariants once at quiescence, then settlement.
    let first = NaiveMicro {
        configuration_after,
        ctx,
        history_after,
        internal: false,
        region: selected.region.clone(),
        source: selected.source.clone(),
        transition_index: selected.document_index,
        exited,
        entered,
        raised,
    };
    match super::macrostep::run_reactions(m, st, first, effects, now_ms, budget) {
        Ok(applied) => Outcome::Applied(applied),
        Err(rejection) => Outcome::Rejected(rejection),
    }
}

pub fn naive_poll_deadline(
    m: &CompiledMachine,
    st: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
) -> DeadlineOutcome {
    if st.status != Status::Running {
        return DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: None,
            rejection: reject(
                if st.status == Status::Completed {
                    "run/instance_completed"
                } else {
                    "run/instance_cancelled"
                },
                "instance is not running",
            ),
        });
    }
    let Some(active) = active_leaves(&m.spec, &st.configuration) else {
        return DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: None,
            rejection: reject("run/configuration_invalid", "invalid configuration"),
        });
    };
    let Some(selected) = select_deadline(&m.spec, &active, &st.deadlines) else {
        return DeadlineOutcome::NotDue { next: None };
    };
    let pending = PendingDeadline {
        name: m.spec.deadlines[selected.document_index].name.clone(),
        deadline_idx: selected.document_index as u32,
        due_ms: selected.due_ms,
    };
    if selected.due_ms > now_ms {
        return DeadlineOutcome::NotDue {
            next: Some(pending),
        };
    }
    match apply_naive_deadline(m, st, &selected, now_ms, budget) {
        Outcome::Applied(transition) => DeadlineOutcome::Applied(DeadlineApplied {
            deadline: pending,
            transition,
        }),
        Outcome::Rejected(rejection) => DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: Some(pending),
            rejection,
        }),
        Outcome::Ignored => panic!("a selected deadline is never ignored"),
    }
}
