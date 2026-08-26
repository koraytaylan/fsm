//! The transition pipeline of one microstep and the schedule settlement that
//! follows it.
//!
//! [`apply_selected_transition`] performs SPEC §Semantics 5–7 for one selected
//! transition — target and domain, exit/transition/entry blocks, history
//! capture — and returns the result unsettled. Invariants (§8) are evaluated
//! once per macrostep by the driver in `micro.rs`, and [`settle_schedules`]
//! performs §9–10 when the driver knows this microstep's context is final.

use std::collections::BTreeMap;

use crate::expr::eval::{Bindings, Budget, Val, eval};
use crate::expr::parser;
use crate::expr::typeck::{ScopeKind, Ty, annotate_if_widening};
use crate::machine::{
    ActiveConfiguration, CancelledChild, CompiledMachine, ExprSlot, InstanceState, Invocation,
    Status,
};
use crate::spec::{Block, DeadlineSpec, HistoryKind};
use crate::trace::{BlockKind, BlockTrace, DecisionTrace, LevelTrace};
use crate::tree::{NodeKind, Tree};

use super::block::{PipelineOutputs, apply_block, find_node, reject_pipeline};
use super::guard::spec_scope;
use super::invoke::settle_invocations;
use super::micro::InternalEvent;
use super::validate::reject;
use super::{EffectOut, ExprSlotOwner, Rejection};

pub(super) struct SelectedTransition<'a> {
    pub(super) region: Option<String>,
    pub(super) leaf: u16,
    pub(super) source: u16,
    pub(super) target: Option<&'a str>,
    pub(super) action: Block,
    pub(super) action_kind: BlockKind,
    pub(super) owner: ExprSlotOwner,
    pub(super) event_name: &'a str,
    pub(super) event_fields: &'a BTreeMap<String, Val>,
    pub(super) sees_event: bool,
    pub(super) public_index: u32,
    /// The candidate scan that selected this transition.
    pub(super) candidates: Vec<LevelTrace>,
    /// Global effect index the first emit of this pipeline takes; effects
    /// number continuously across every microstep of one macrostep.
    pub(super) first_effect_index: u32,
}

/// One microstep's transition result before its deadline schedules settle.
///
/// The driver keeps the last of these unsettled until it knows whether another
/// reaction follows, so that invariants are evaluated before the final
/// microstep's schedules exactly as SPEC §Semantics orders 8 before 9.
pub(super) struct Transitioned {
    pub(super) configuration_after: ActiveConfiguration,
    pub(super) context: BTreeMap<String, Val>,
    pub(super) history_after: BTreeMap<String, String>,
    pub(super) effects: Vec<EffectOut>,
    /// Internal events the committed blocks raised, in pipeline order.
    pub(super) raises: Vec<InternalEvent>,
    pub(super) pipeline: Vec<BlockTrace>,
    pub(super) candidates: Vec<LevelTrace>,
    pub(super) exited: Vec<u16>,
    pub(super) entered: Vec<u16>,
    /// Invocation slots after this microstep's exits and entries.
    pub(super) invocations: BTreeMap<String, Invocation>,
    /// Children this microstep's exits left running, for the store to cancel.
    pub(super) cancelled_children: Vec<CancelledChild>,
    pub(super) internal: bool,
    pub(super) region: Option<String>,
    pub(super) source_state: String,
    pub(super) public_index: u32,
}

impl Transitioned {
    pub(super) fn exited_names(&self, tree: &Tree) -> Vec<String> {
        self.exited
            .iter()
            .map(|&id| tree.names[id as usize].clone())
            .collect()
    }

    pub(super) fn entered_names(&self, tree: &Tree) -> Vec<String> {
        self.entered
            .iter()
            .map(|&id| tree.names[id as usize].clone())
            .collect()
    }
}

pub(super) fn apply_selected_transition(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    selected: SelectedTransition<'_>,
    budget: &mut Budget,
) -> Result<Transitioned, Rejection> {
    let SelectedTransition {
        region,
        leaf,
        source,
        target,
        action,
        action_kind,
        owner,
        event_name,
        event_fields,
        sees_event,
        public_index,
        candidates,
        first_effect_index,
    } = selected;
    let internal = target.is_none();
    let current_leaf = tree.names[leaf as usize].clone();
    let (exited_ids, entered_ids, new_leaf) = if let Some(target) = target {
        let mut target_name = target.to_string();
        let mut extra_descent = Vec::new();
        if let Some(target_id) = tree.id(&target_name) {
            if matches!(tree.kind[target_id as usize], NodeKind::History(_)) {
                let Some(owner_id) = tree.history_owner(target_id) else {
                    let mut rejection = reject(
                        "run/action_error",
                        &format!("history target {target} has no compound owner"),
                    );
                    rejection.hint =
                        "define a replacement machine with history under a compound state".into();
                    rejection.source_state = Some(tree.names[source as usize].clone());
                    rejection.transition_idx = Some(public_index);
                    rejection.cause = Some("def/shape");
                    rejection.trace = DecisionTrace {
                        candidates,
                        ..DecisionTrace::default()
                    };
                    return Err(rejection);
                };
                let owner_name = &tree.names[owner_id as usize];
                extra_descent = tree
                    .history_descent(target_id, state.history.get(owner_name).map(String::as_str));
                target_name = owner_name.clone();
            }
        }
        let target_id = tree.id(&target_name).expect("validated transition target");
        let external_self = target == tree.names[source as usize];
        let domain = if external_self {
            tree.parent[source as usize]
        } else {
            tree.proper_lca(source, target_id)
        };
        let exited = tree.exit_set(leaf, domain);
        let mut entered = tree.entry_path(domain, target_id);
        if matches!(tree.kind[target_id as usize], NodeKind::Compound) && extra_descent.is_empty() {
            entered.extend(tree.initial_descent(target_id));
        }
        entered.extend(extra_descent);
        let leaf_after = entered.last().copied().unwrap_or(target_id);
        (exited, entered, tree.names[leaf_after as usize].clone())
    } else {
        (Vec::new(), Vec::new(), current_leaf.clone())
    };
    let configuration_after = match state.configuration.with_leaf(region.as_deref(), new_leaf) {
        Some(configuration) => configuration,
        None => {
            return Err(reject("run/unhandled", "transition region is not active"));
        }
    };

    let mut context = state.ctx.clone();
    let mut outputs = PipelineOutputs::starting_at(first_effect_index);
    let mut pipeline = Vec::new();

    let apply = |block: &Block,
                 kind: BlockKind,
                 context: &mut BTreeMap<String, Val>,
                 outputs: &mut PipelineOutputs,
                 see_event: bool,
                 block_owner: ExprSlotOwner,
                 budget: &mut Budget|
     -> Result<BlockTrace, Rejection> {
        apply_block(
            block,
            kind,
            context,
            outputs,
            see_event,
            event_fields,
            budget,
            &machine.spec,
            event_name,
            &machine.compiled_exprs,
            block_owner,
        )
    };

    for &id in &exited_ids {
        let name = &tree.names[id as usize];
        if let Some(block) = find_node(&machine.spec, name).and_then(|node| node.exit.as_ref()) {
            match apply(
                block,
                BlockKind::Exit(name.clone()),
                &mut context,
                &mut outputs,
                false,
                ExprSlotOwner::Exit(name.clone()),
                budget,
            ) {
                Ok(block_trace) => pipeline.push(block_trace),
                Err(rejection) => {
                    return Err(reject_pipeline(rejection, pipeline, &candidates));
                }
            }
        }
    }
    match apply(
        &action,
        action_kind,
        &mut context,
        &mut outputs,
        sees_event,
        owner,
        budget,
    ) {
        Ok(block_trace) => pipeline.push(block_trace),
        Err(rejection) => {
            return Err(reject_pipeline(rejection, pipeline, &candidates));
        }
    }
    for &id in &entered_ids {
        let name = &tree.names[id as usize];
        if let Some(block) = find_node(&machine.spec, name).and_then(|node| node.entry.as_ref()) {
            match apply(
                block,
                BlockKind::Entry(name.clone()),
                &mut context,
                &mut outputs,
                false,
                ExprSlotOwner::Entry(name.clone()),
                budget,
            ) {
                Ok(block_trace) => pipeline.push(block_trace),
                Err(rejection) => {
                    return Err(reject_pipeline(rejection, pipeline, &candidates));
                }
            }
        }
    }

    // SPEC §Semantics 7: history binds from the pre-transition configuration,
    // which for a reaction microstep means the configuration it started from.
    let mut history_after = state.history.clone();
    for &id in &exited_ids {
        if matches!(tree.kind[id as usize], NodeKind::Compound) {
            for &child in &tree.children[id as usize] {
                if let NodeKind::History(kind) = tree.kind[child as usize] {
                    let bound = match kind {
                        HistoryKind::Deep => current_leaf.clone(),
                        HistoryKind::Shallow => tree
                            .chain(leaf)
                            .into_iter()
                            .find(|&node| tree.parent[node as usize] == Some(id))
                            .map(|node| tree.names[node as usize].clone())
                            .unwrap_or_else(|| current_leaf.clone()),
                    };
                    history_after.insert(tree.names[id as usize].clone(), bound);
                }
            }
        }
    }

    let (invocations, cancelled_children) = settle_invocations(
        machine,
        tree,
        &state.invocations,
        &exited_ids,
        &entered_ids,
        &context,
        budget,
    )
    .map_err(|rejection| reject_pipeline(rejection, pipeline.clone(), &candidates))?;

    Ok(Transitioned {
        configuration_after,
        context,
        history_after,
        effects: outputs.effects,
        raises: outputs.raised,
        pipeline,
        candidates,
        exited: exited_ids,
        entered: entered_ids,
        invocations,
        cancelled_children,
        internal,
        region,
        source_state: tree.names[source as usize].clone(),
        public_index,
    })
}

/// SPEC §Semantics 9–10 for one microstep: reschedule the deadlines its exit
/// and entry sets own from the context it left, clear the schedules of any
/// region it completed, and settle the lifecycle status.
pub(super) fn settle_schedules(
    machine: &CompiledMachine,
    tree: &Tree,
    prior: &BTreeMap<String, i64>,
    transitioned: &Transitioned,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<(BTreeMap<String, i64>, Status), Rejection> {
    let mut deadlines = update_deadline_schedules(
        machine,
        prior,
        &transitioned.exited,
        &transitioned.entered,
        tree,
        &transitioned.context,
        now_ms,
        budget,
    )?;
    clear_terminal_region_deadlines(
        machine,
        tree,
        &transitioned.configuration_after,
        &mut deadlines,
    );
    let status = if configuration_is_terminal(machine, tree, &transitioned.configuration_after) {
        deadlines.clear();
        Status::Completed
    } else {
        Status::Running
    };
    Ok((deadlines, status))
}

pub(super) fn clear_terminal_region_deadlines(
    machine: &CompiledMachine,
    tree: &Tree,
    configuration: &ActiveConfiguration,
    schedules: &mut BTreeMap<String, i64>,
) {
    let Some(active_leaves) = tree.active_leaves(configuration) else {
        return;
    };
    for (_, leaf) in active_leaves {
        if !find_node(&machine.spec, &tree.names[leaf as usize]).is_some_and(|node| node.terminal) {
            continue;
        }
        let terminal_chain: std::collections::BTreeSet<&str> = tree
            .chain(leaf)
            .into_iter()
            .map(|state| tree.names[state as usize].as_str())
            .collect();
        schedules.retain(|name, _| {
            machine
                .spec
                .deadlines
                .iter()
                .find(|deadline| deadline.name == *name)
                .is_none_or(|deadline| !terminal_chain.contains(deadline.from.as_str()))
        });
    }
}

pub(super) fn update_deadline_schedules(
    machine: &CompiledMachine,
    prior: &BTreeMap<String, i64>,
    exited: &[u16],
    entered: &[u16],
    tree: &Tree,
    context: &BTreeMap<String, Val>,
    now_ms: i64,
    budget: &mut Budget,
) -> Result<BTreeMap<String, i64>, Rejection> {
    let mut schedules = prior.clone();
    for state in exited {
        let state_name = &tree.names[*state as usize];
        for deadline in machine
            .spec
            .deadlines
            .iter()
            .filter(|deadline| deadline.from == *state_name)
        {
            schedules.remove(&deadline.name);
        }
    }
    let context_types: BTreeMap<String, Ty> = machine
        .spec
        .context
        .iter()
        .map(|variable| (variable.name.clone(), variable.ty.to_ty()))
        .collect();
    let bindings = Bindings {
        ctx: context,
        evt: None,
        active: None,
    };
    let state_names = machine.spec.state_names();
    for state in entered {
        let state_name = &tree.names[*state as usize];
        for (index, deadline) in machine.spec.deadlines.iter().enumerate() {
            if deadline.from != *state_name {
                continue;
            }
            let expression = if let Some(compiled) =
                machine.compiled_exprs.get(&ExprSlot::DeadlineAfter(index))
            {
                compiled.expr.clone()
            } else {
                let mut expression = parser::parse(&deadline.after).map_err(|error| {
                    deadline_schedule_rejection(
                        deadline,
                        error.message,
                        error.hint,
                        Some((error.span.start, error.span.end)),
                        Some(error.code),
                    )
                })?;
                annotate_if_widening(
                    &mut expression,
                    &spec_scope(
                        &machine.spec,
                        ScopeKind::Block,
                        &context_types,
                        None,
                        &state_names,
                    ),
                );
                expression
            };
            let duration = match eval(&expression, &bindings, budget, false).0 {
                Ok(Val::Dur(duration)) if duration >= 0 => duration,
                Ok(Val::Dur(_)) => {
                    return Err(deadline_schedule_rejection(
                        deadline,
                        "deadline duration is negative".into(),
                        "return a zero or positive duration".into(),
                        None,
                        Some("run/overflow"),
                    ));
                }
                Ok(_) => {
                    return Err(deadline_schedule_rejection(
                        deadline,
                        "deadline expression did not return a duration".into(),
                        "return a duration, for example dur(5, min)".into(),
                        None,
                        None,
                    ));
                }
                Err(error) => {
                    return Err(deadline_schedule_rejection(
                        deadline,
                        error.message,
                        error.hint,
                        Some((error.span.start, error.span.end)),
                        Some(error.code),
                    ));
                }
            };
            let due_ms = now_ms.checked_add(duration).ok_or_else(|| {
                deadline_schedule_rejection(
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

pub(super) fn deadline_schedule_rejection(
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
        trace: DecisionTrace::default(),
        cause,
    }
}

pub(super) fn configuration_is_terminal(
    machine: &CompiledMachine,
    tree: &Tree,
    configuration: &ActiveConfiguration,
) -> bool {
    tree.active_leaves(configuration).is_some_and(|active| {
        !active.is_empty()
            && active.iter().all(|(_, state)| {
                find_node(&machine.spec, &tree.names[*state as usize])
                    .is_some_and(|node| node.terminal)
            })
    })
}
