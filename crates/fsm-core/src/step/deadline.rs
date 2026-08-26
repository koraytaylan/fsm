use std::collections::BTreeMap;

use crate::expr::eval::Budget;
use crate::machine::{CompiledMachine, InstanceState, Status};
use crate::spec::Block;
use crate::trace::BlockKind;
use crate::tree::Tree;

use super::micro::{EngineSelector, ReactionSelector, run_to_quiescence};
use super::transition::{SelectedTransition, apply_selected_transition};
use super::validate::{invalid_state_rejection, reject};
use super::{DeadlineApplied, DeadlineOutcome, DeadlineRejected, ExprSlotOwner, PendingDeadline};

/// Poll the active configuration and apply at most one due deadline, then
/// run the machine's reactions to quiescence as one atomic macrostep.
///
/// Selection is stable by `(due_ms, deadline document index)`. Time is explicit
/// caller input; this function never consults a clock and never loops to drain
/// multiple schedules.
pub fn poll_deadline(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
) -> DeadlineOutcome {
    poll_deadline_with(machine, tree, state, now_ms, budget, &mut EngineSelector)
}

/// [`poll_deadline`] with an explicit reaction selector; tests script the
/// reactions.
pub fn poll_deadline_with(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    now_ms: i64,
    budget: &mut Budget,
    selector: &mut dyn ReactionSelector,
) -> DeadlineOutcome {
    if let Err(error) = tree.validate_instance_state(machine, state) {
        return DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: None,
            rejection: invalid_state_rejection(error.detail()),
        });
    }
    match state.status {
        Status::Completed => {
            return DeadlineOutcome::Rejected(DeadlineRejected {
                deadline: None,
                rejection: reject("run/instance_completed", "instance is completed"),
            });
        }
        Status::Cancelled => {
            return DeadlineOutcome::Rejected(DeadlineRejected {
                deadline: None,
                rejection: reject("run/instance_cancelled", "instance is cancelled"),
            });
        }
        Status::Running => {}
    }
    let active_leaves = match tree.active_leaves(&state.configuration) {
        Some(active_leaves) => active_leaves,
        None => {
            return DeadlineOutcome::Rejected(DeadlineRejected {
                deadline: None,
                rejection: reject(
                    "run/configuration_invalid",
                    "supply a configuration matching the machine topology and real leaf states",
                ),
            });
        }
    };

    let mut selected: Option<(i64, usize, Option<String>, u16, u16)> = None;
    for (deadline_index, deadline) in machine.spec.deadlines.iter().enumerate() {
        let Some(&due_ms) = state.deadlines.get(&deadline.name) else {
            continue;
        };
        let source_location = active_leaves.iter().find_map(|(region, leaf)| {
            if super::block::find_node(&machine.spec, &tree.names[*leaf as usize])
                .is_some_and(|node| node.terminal)
            {
                return None;
            }
            tree.chain(*leaf)
                .into_iter()
                .find(|source| tree.names[*source as usize] == deadline.from)
                .map(|source| (region.map(str::to_string), *leaf, source))
        });
        let Some((region, leaf, source)) = source_location else {
            continue;
        };
        let candidate = (due_ms, deadline_index, region, leaf, source);
        if selected
            .as_ref()
            .is_none_or(|current| (candidate.0, candidate.1) < (current.0, current.1))
        {
            selected = Some(candidate);
        }
    }
    let Some((due_ms, deadline_index, region, leaf, source)) = selected else {
        return DeadlineOutcome::NotDue { next: None };
    };
    let deadline = &machine.spec.deadlines[deadline_index];
    let pending = PendingDeadline {
        name: deadline.name.clone(),
        deadline_idx: deadline_index as u32,
        due_ms,
    };
    if due_ms > now_ms {
        return DeadlineOutcome::NotDue {
            next: Some(pending),
        };
    }

    let empty_event = BTreeMap::new();
    let trigger = apply_selected_transition(
        machine,
        tree,
        state,
        SelectedTransition {
            region,
            leaf,
            source,
            target: Some(&deadline.to),
            action: Block {
                sets: deadline.sets.clone(),
                emits: deadline.emits.clone(),
                raises: deadline.raises.clone(),
            },
            action_kind: BlockKind::Deadline(deadline.name.clone()),
            owner: ExprSlotOwner::Deadline(deadline_index),
            event_name: "",
            event_fields: &empty_event,
            sees_event: false,
            public_index: deadline_index as u32,
            candidates: Vec::new(),
            first_effect_index: 0,
        },
        budget,
    );
    match trigger.and_then(|trigger| {
        run_to_quiescence(machine, tree, state, trigger, now_ms, budget, selector)
    }) {
        Ok(transition) => DeadlineOutcome::Applied(DeadlineApplied {
            deadline: pending,
            transition,
        }),
        Err(rejection) => DeadlineOutcome::Rejected(DeadlineRejected {
            deadline: Some(pending),
            rejection,
        }),
    }
}
