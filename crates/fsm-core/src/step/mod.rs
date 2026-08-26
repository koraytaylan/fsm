//! Pure instance creation, event stepping, and explicit deadline polling.
//!
//! All time enters as a caller-supplied millisecond timestamp. The engine does
//! not read a clock, run background timers, or drain more than one deadline in
//! a poll. Each entry point applies one trigger microstep and then hands the
//! result to the macrostep driver in `micro.rs`, which runs the machine's own
//! reactions to quiescence inside the same atomic result.

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
mod invoke;
mod micro;
mod transition;
mod validate;

pub use create::{create, create_with, payload_from_pairs};
pub use deadline::{poll_deadline, poll_deadline_with};
pub use micro::{
    EngineSelector, InternalEvent, InternalOrigin, ReactionSelection, ReactionSelector,
};
pub use validate::validate_event;

use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::json::Value;
use crate::machine::{
    ActiveConfiguration, CancelledChild, CompiledMachine, InstanceState, Invocation, Status,
};
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

use micro::run_to_quiescence;
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
    /// Invocation slots after the whole macrostep, by slot id. A `Pending`
    /// slot is a child the store has yet to create.
    pub invocations_after: BTreeMap<String, Invocation>,
    /// Children whose invoking state was exited while they were running: the
    /// parent stopped waiting, so the store cancels them.
    pub cancelled_children: Vec<CancelledChild>,
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

/// Deliver one event: apply at most one globally selected transition, then
/// run the machine's reactions to quiescence as one atomic macrostep.
///
/// Parallel regions are scanned in semantic region order; the input state is
/// never mutated. `now_ms` is used only to schedule deadlines on state entry,
/// and is read once for the whole macrostep.
pub fn step(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    step_with(
        m,
        t,
        st,
        event,
        payload,
        now_ms,
        budget,
        &mut EngineSelector,
    )
}

/// [`step`] with an explicit reaction selector; tests script the reactions.
pub fn step_with(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    event: &str,
    payload: &Value,
    now_ms: i64,
    budget: &mut Budget,
    selector: &mut dyn ReactionSelector,
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
    deliver(m, t, st, event, fields, now_ms, budget, selector)
}

/// Deliver a generated `$done.invoke.<slot>` event as a macrostep trigger.
///
/// The store calls this when an invocation returns, exactly as it hands over
/// a due deadline: the core never learns that a child completed — that is
/// I/O — so the event arrives already named and already typed, its payload
/// being the `returns` projection read out of the child's final context. From
/// there it is an ordinary macrostep, reactions and all.
///
/// This is not a way round [`validate_event`]: a caller sending
/// `$done.invoke.review` through [`step`] is still refused
/// `req/event_internal`, because the refusal is about who may send a
/// generated name, not about whether the engine can deliver one.
pub fn deliver_generated(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    event: &str,
    payload: &BTreeMap<String, Val>,
    now_ms: i64,
    budget: &mut Budget,
) -> Outcome {
    if !event.starts_with('$') {
        return Outcome::Rejected(reject(
            "req/event_internal",
            "only a generated $-prefixed event is delivered this way",
        ));
    }
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
    deliver(
        m,
        t,
        st,
        event,
        payload.clone(),
        now_ms,
        budget,
        &mut EngineSelector,
    )
}

/// The scan, selection, and macrostep every trigger shares once its payload
/// is typed.
#[allow(clippy::too_many_arguments)]
fn deliver(
    m: &CompiledMachine,
    t: &Tree,
    st: &InstanceState,
    event: &str,
    fields: BTreeMap<String, Val>,
    now_ms: i64,
    budget: &mut Budget,
    selector: &mut dyn ReactionSelector,
) -> Outcome {
    let active_leaves = match t.active_leaves(&st.configuration) {
        Some(active_leaves) => active_leaves,
        None => {
            return Outcome::Rejected(reject(
                "run/configuration_invalid",
                "supply a configuration matching the machine topology and real leaf states",
            ));
        }
    };
    let scan = match scan_candidates(m, t, &active_leaves, event, &st.ctx, Some(&fields), budget) {
        Ok(scan) => scan,
        Err(rejection) => return Outcome::Rejected(rejection),
    };
    let trace = DecisionTrace {
        candidates: scan.candidates,
        ..DecisionTrace::default()
    };
    let Some(Winner {
        region,
        leaf,
        source: src,
        transition_idx: tidx,
    }) = scan.winner
    else {
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
    let trigger = apply_selected_transition(
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
                raises: transition.raises.clone(),
            },
            action_kind: BlockKind::Transition,
            owner: ExprSlotOwner::Transition(tidx),
            event_name: event,
            event_fields: &fields,
            sees_event: true,
            public_index: tidx as u32,
            candidates: trace.candidates,
            first_effect_index: 0,
        },
        budget,
    );
    match trigger.and_then(|trigger| run_to_quiescence(m, t, st, trigger, now_ms, budget, selector))
    {
        Ok(applied) => Outcome::Applied(applied),
        Err(rejection) => Outcome::Rejected(rejection),
    }
}

/// The transition one candidate scan selected.
pub(super) struct Winner {
    pub(super) region: Option<String>,
    pub(super) leaf: u16,
    pub(super) source: u16,
    pub(super) transition_idx: usize,
}

/// One complete candidate scan: the global winner, if any, and the trace of
/// every candidate the scan considered.
pub(super) struct Scan {
    pub(super) winner: Option<Winner>,
    pub(super) candidates: Vec<LevelTrace>,
}

/// SPEC §Semantics 3–4 for one cell key: walk each active region's leaf-to-root
/// chain in region document order, skipping a region whose active leaf is
/// terminal, and at each state take the document-ordered transitions in the
/// `(state, key)` cell. The first true guard wins globally; every later
/// candidate is `not_considered`. A guard that fails to evaluate rejects —
/// never treat-as-false — with the levels scanned so far in its trace.
///
/// The key is an event name for the trigger scan and [`ALWAYS_KEY`] for the
/// eventless scan; `fields` is `None` when no event supplies `evt`.
pub(super) fn scan_candidates(
    m: &CompiledMachine,
    t: &Tree,
    active_leaves: &[(Option<&str>, u16)],
    key: &str,
    ctx: &BTreeMap<String, Val>,
    fields: Option<&BTreeMap<String, Val>>,
    budget: &mut Budget,
) -> Result<Scan, Rejection> {
    let mut candidates: Vec<LevelTrace> = Vec::new();
    let mut winner: Option<Winner> = None;
    for &(region, leaf) in active_leaves {
        let leaf_name = &t.names[leaf as usize];
        if block::find_node(&m.spec, leaf_name).is_some_and(|node| node.terminal) {
            continue;
        }
        for sid in t.chain(leaf) {
            let state_name = t.names[sid as usize].clone();
            let indices = m
                .transitions_by
                .get(&(state_name.clone(), key.to_string()))
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
                    ctx,
                    fields,
                    budget,
                    &m.spec,
                    key,
                    index,
                    &m.compiled_exprs,
                ) {
                    Ok((true, guard_trace)) => {
                        level.transitions.push(CandidateTrace {
                            transition_idx: index as u32,
                            guard: GuardTrace::Evaluated(guard_trace),
                        });
                        winner = Some(Winner {
                            region: region.map(str::to_string),
                            leaf,
                            source: sid,
                            transition_idx: index,
                        });
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
                        candidates.append(&mut rejection.trace.candidates);
                        rejection.trace.candidates = candidates;
                        return Err(rejection);
                    }
                }
            }
            candidates.push(level);
        }
    }
    Ok(Scan { winner, candidates })
}
