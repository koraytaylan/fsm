//! Pure instance creation, event stepping, and explicit deadline polling.
//!
//! All time enters as a caller-supplied millisecond timestamp. The engine does
//! not read a clock, run background timers, or drain more than one deadline in
//! a poll.

#![allow(
    clippy::collapsible_if,
    clippy::too_many_arguments,
    clippy::result_large_err,
    clippy::if_same_then_else
)]

mod block;
mod create;
mod deadline;
mod guard;
mod transition;
mod validate;

pub use create::{create, payload_from_pairs};
pub use deadline::poll_deadline;
pub use validate::validate_event;

use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::json::Value;
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use crate::spec::Block;

#[derive(Clone)]
pub(super) enum ExprSlotOwner {
    Transition(usize),
    Deadline(usize),
    Entry(String),
    Exit(String),
}
use crate::trace::{BlockKind, CandidateTrace, DecisionTrace, GuardTrace, LevelTrace};
use crate::tree::Tree;

use transition::{SelectedTransition, apply_selected_transition};
use validate::{invalid_state_rejection, reject};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectOut {
    pub name: String,
    pub args: BTreeMap<String, Val>,
    pub k: u32,
}

/// State and diagnostics produced by one successfully applied transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// Complete active configuration after the transition.
    pub configuration_after: ActiveConfiguration,
    /// Typed context after all exit, transition, and entry actions.
    pub ctx_after: BTreeMap<String, Val>,
    /// History bindings after exited compound states have been recorded.
    pub history_after: BTreeMap<String, String>,
    /// Active deadline name to absolute due timestamp after rescheduling.
    pub deadlines_after: BTreeMap<String, i64>,
    /// Effects emitted by the accepted pipeline, in execution order.
    pub effects: Vec<EffectOut>,
    /// Names of monitor-mode invariants that failed.
    pub monitor_flags: Vec<String>,
    /// Lifecycle status after the transition.
    ///
    /// A parallel instance is completed only when every region is terminal.
    pub status_after: Status,
    /// Whether the selected transition kept the same active state hierarchy.
    pub internal: bool,
    /// Winning region for a parallel transition, or `None` for sequential flow
    /// and instance creation.
    pub region: Option<String>,
    /// State that owned the selected transition or deadline.
    pub source_state: String,
    /// Document index of the selected transition or deadline definition.
    pub transition_idx: u32,
    /// States exited in leaf-to-root execution order.
    pub exited: Vec<String>,
    /// States entered in root-to-leaf execution order.
    pub entered: Vec<String>,
    /// Complete deterministic decision and action trace.
    pub trace: DecisionTrace,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    pub code: &'static str,
    pub message: String,
    pub hint: String,
    pub source_state: Option<String>,
    pub transition_idx: Option<u32>,
    pub block: Option<String>,
    pub span: Option<(u32, u32)>,
    pub trace: DecisionTrace,
    /// Underlying stable cause when `code` is the public `run/action_error`
    /// wrapper: normally an evaluator code (`run/overflow`, `run/div_zero`,
    /// …), or `def/shape` for a grandfathered malformed history target. Never
    /// used as the public code.
    pub cause: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Applied(Applied),
    Rejected(Rejection),
    Ignored,
}

/// A scheduled deadline visible to deterministic callers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDeadline {
    /// Definition name.
    pub name: String,
    /// Zero-based definition document index.
    pub deadline_idx: u32,
    /// Absolute caller-supplied millisecond timestamp at which it becomes due.
    pub due_ms: i64,
}

/// The result of applying one due deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlineApplied {
    /// The selected schedule before it was applied.
    pub deadline: PendingDeadline,
    /// The ordinary transition pipeline result.
    pub transition: Applied,
}

/// A rejected poll, optionally after selecting a due deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadlineRejected {
    /// The selected schedule, or `None` when the instance/configuration gate failed.
    pub deadline: Option<PendingDeadline>,
    /// Structured deterministic rejection.
    pub rejection: Rejection,
}

/// Pure result of polling an instance at a caller-supplied timestamp.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeadlineOutcome {
    /// One due deadline was applied.
    Applied(DeadlineApplied),
    /// Polling or application was rejected atomically.
    Rejected(DeadlineRejected),
    /// Nothing was due.
    NotDue {
        /// Earliest active schedule, if the configuration has one.
        next: Option<PendingDeadline>,
    },
}

/// Deliver one event and apply at most one globally selected transition.
///
/// Parallel regions are scanned in semantic region order; the input state is
/// never mutated. `now_ms` is used only to schedule deadlines on state entry.
pub fn step(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    if let Err(error) = t.validate_instance_state(m, st) {
        return Outcome::Rejected(invalid_state_rejection(error.detail()));
    }
    match st.status {
        Status::Completed => {
            return Outcome::Rejected(reject("run/instance_completed", "instance is completed"));
        }
        Status::Cancelled => {
            return Outcome::Rejected(reject("run/instance_cancelled", "instance is cancelled"));
        }
        Status::Running => {}
    }
    let fields = match validate_event(m, event, payload) {
        Ok(f) => f,
        Err(r) => return Outcome::Rejected(r),
    };
    let active_leaves = match t.active_leaves(&st.configuration) {
        Some(active_leaves) => active_leaves,
        None => {
            return Outcome::Rejected(reject(
                "run/configuration_invalid",
                "supply a configuration matching the machine topology and real leaf states",
            ));
        }
    };
    let mut trace = DecisionTrace::default();
    let mut winner: Option<(Option<String>, u16, u16, usize)> = None;
    for (region, leaf) in active_leaves {
        let leaf_name = &t.names[leaf as usize];
        if block::find_node(&m.spec, leaf_name).is_some_and(|node| node.terminal) {
            continue;
        }
        for sid in t.chain(leaf) {
            let state_name = t.names[sid as usize].clone();
            let indices = m
                .transitions_by
                .get(&(state_name.clone(), event.to_string()))
                .cloned()
                .unwrap_or_default();
            if indices.is_empty() {
                continue;
            }
            let mut level = LevelTrace {
                source_state: state_name.clone(),
                transitions: Vec::new(),
            };
            for index in indices {
                if winner.is_some() {
                    level.transitions.push(CandidateTrace {
                        transition_idx: index as u32,
                        guard: GuardTrace::NotConsidered,
                    });
                    continue;
                }
                let transition = &m.spec.transitions[index];
                match guard::eval_guard(
                    transition,
                    &st.ctx,
                    &fields,
                    budget,
                    &m.spec,
                    event,
                    index,
                    &m.compiled_exprs,
                ) {
                    Ok((true, guard_trace)) => {
                        level.transitions.push(CandidateTrace {
                            transition_idx: index as u32,
                            guard: GuardTrace::Evaluated(guard_trace),
                        });
                        winner = Some((region.map(str::to_string), leaf, sid, index));
                    }
                    Ok((false, guard_trace)) => {
                        level.transitions.push(CandidateTrace {
                            transition_idx: index as u32,
                            guard: GuardTrace::Evaluated(guard_trace),
                        });
                    }
                    Err(mut rejection) => {
                        rejection.source_state = Some(state_name.clone());
                        rejection.transition_idx = Some(index as u32);
                        if let Some(failing_level) = rejection.trace.candidates.first_mut() {
                            failing_level.source_state = state_name.clone();
                        }
                        if !level.transitions.is_empty() {
                            if let Some(failing_level) = rejection.trace.candidates.first_mut() {
                                let mut evaluated = level.transitions;
                                evaluated.append(&mut failing_level.transitions);
                                failing_level.transitions = evaluated;
                            } else {
                                rejection.trace.candidates.insert(0, level);
                            }
                        }
                        let mut prior_trace = trace;
                        prior_trace
                            .candidates
                            .append(&mut rejection.trace.candidates);
                        rejection.trace.candidates = prior_trace.candidates;
                        return Outcome::Rejected(rejection);
                    }
                }
            }
            trace.candidates.push(level);
        }
    }
    let Some((region, leaf, src, tidx)) = winner else {
        let any = trace.candidates.iter().any(|l| !l.transitions.is_empty());
        if !any {
            return match m.spec.on_unhandled {
                crate::spec::Unhandled::Ignore => Outcome::Ignored,
                crate::spec::Unhandled::Reject => Outcome::Rejected(Rejection {
                    code: "run/unhandled",
                    message: format!("no handler for {event} in the active configuration"),
                    hint: "add a transition or send a handled event".into(),
                    source_state: None,
                    transition_idx: None,
                    block: None,
                    span: None,
                    trace,
                    cause: None,
                }),
            };
        }
        return Outcome::Rejected(Rejection {
            code: "run/not_enabled",
            message: format!("all guards false for {event}"),
            hint: "adjust the payload or add a child override".into(),
            source_state: None,
            transition_idx: None,
            block: None,
            span: None,
            trace,
            cause: None,
        });
    };
    let transition = &m.spec.transitions[tidx];
    apply_selected_transition(
        m,
        t,
        st,
        SelectedTransition {
            region,
            leaf,
            source: src,
            target: transition.to.as_deref(),
            action: Block {
                sets: transition.sets.clone(),
                emits: transition.emits.clone(),
            },
            action_kind: BlockKind::Transition,
            owner: ExprSlotOwner::Transition(tidx),
            event_name: event,
            event_fields: &fields,
            sees_event: true,
            public_index: tidx as u32,
            trace,
        },
        now_ms,
        budget,
    )
}
