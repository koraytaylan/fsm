//! The macrostep driver: run a triggered transition's reactions to quiescence.
//!
//! A macrostep is one trigger microstep — an event's selected transition, a
//! due deadline's transition, or creation's initial entry — followed by zero
//! or more reaction microsteps run until nothing more is enabled. Each
//! iteration of [`run_to_quiescence`] tries, in this order: an eventless
//! transition over the working configuration; then the front of the internal
//! event queue, whose selected handler is applied or, when no transition
//! handles it, discarded; and quiescence when neither yields anything. Every
//! reaction runs the same pipeline as the trigger. Invariants are evaluated
//! once, at quiescence, on the final context and configuration.
//!
//! The queue lives in a stack frame for the duration of one macrostep and is
//! never part of [`InstanceState`]: the loop drains it before returning, so
//! it is empty at every sealed state, and putting it in `fsm.state/2` would
//! move every state hash and force every existing store through a migration.
//!
//! Atomicity: the driver works on a copy of the caller's state. Any rejection
//! from any microstep — a guard or action error, an invariant failure at
//! quiescence, the [`MAX_MICROSTEPS`] ceiling — rejects the whole macrostep,
//! and the rejection's trace keeps the microsteps that ran before it, every
//! block marked discarded.

use std::collections::{BTreeMap, VecDeque};

use crate::expr::eval::{Budget, Val};
use crate::limits::MAX_MICROSTEPS;
use crate::machine::{CompiledMachine, EnforceMode, InstanceState};
use crate::spec::Block;
use crate::spec::MachineSpec;
use crate::trace::{
    BlockKind, BlockTrace, DecisionTrace, InvariantTrace, LevelTrace, MicrostepTrace,
    MicrostepTrigger, UnhandledInternalTrace,
};
use crate::tree::Tree;

use super::block::eval_invariants;
use super::transition::{
    SelectedTransition, Transitioned, apply_selected_transition, settle_schedules,
};
use super::{Applied, EffectOut, ExprSlotOwner, Rejection};

/// An event raised inside a macrostep and delivered to this instance only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InternalEvent {
    /// Declared internal event name, or a generated `$done.*` name.
    pub name: String,
    /// Typed payload the raise computed; empty for generated events.
    pub payload: BTreeMap<String, Val>,
    /// What put the event on the queue.
    pub origin: InternalOrigin,
}

/// What put an internal event on the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InternalOrigin {
    /// A `raise` in the named block.
    Raise { block: BlockKind },
    /// A `final` child of the named compound was entered.
    DoneState { compound: String },
    /// The named region's active leaf became terminal.
    DoneRegion { region: String },
}

/// The transition a selector chose for one reaction microstep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionSelection {
    /// Winning region for a parallel machine.
    pub region: Option<String>,
    /// Active leaf whose chain owned the selected transition.
    pub leaf: u16,
    /// State on that chain that owns the transition.
    pub source: u16,
    /// Document index of the selected transition.
    pub transition_idx: usize,
    /// The candidate scan that selected it, for the trace.
    pub candidates: Vec<LevelTrace>,
}

/// Selects the reactions a macrostep runs after its trigger.
///
/// The driver owns the loop order; a selector only answers "which transition,
/// if any" for each scan. [`EngineSelector`] is the engine's own scan. A test
/// substitutes a scripted selector to drive the loop — its ceiling, ordering,
/// and atomicity — without needing a reactive definition.
pub trait ReactionSelector {
    /// Select an eventless transition over the working configuration.
    ///
    /// `Ok(None)` is quiescence with respect to eventless transitions —
    /// whether because no candidate exists or because every guard is false.
    /// Neither is an error: only a guard that fails to evaluate rejects.
    fn select_eventless(
        &mut self,
        machine: &CompiledMachine,
        tree: &Tree,
        working: &InstanceState,
        budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection>;

    /// Select the handler for an internal event popped from the queue.
    ///
    /// `Ok(None)` discards the event; the driver records it as unhandled and
    /// continues, because an engine-generated event nobody listens for is
    /// not a caller error.
    fn select_internal(
        &mut self,
        machine: &CompiledMachine,
        tree: &Tree,
        working: &InstanceState,
        event: &InternalEvent,
        budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection>;
}

/// The engine's own reaction scan.
///
/// Both scans are seams at this stage of plan 0009: workstream 0043 fills the
/// eventless scan and 0044 the internal-event scan. Until then a macrostep is
/// exactly its trigger microstep, which is what every existing golden pins.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EngineSelector;

impl ReactionSelector for EngineSelector {
    fn select_eventless(
        &mut self,
        _machine: &CompiledMachine,
        _tree: &Tree,
        _working: &InstanceState,
        _budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection> {
        Ok(None)
    }

    fn select_internal(
        &mut self,
        _machine: &CompiledMachine,
        _tree: &Tree,
        _working: &InstanceState,
        _event: &InternalEvent,
        _budget: &mut Budget,
    ) -> Result<Option<ReactionSelection>, Rejection> {
        Ok(None)
    }
}

/// The driver's working aggregate for one macrostep.
struct Macrostep {
    queue: VecDeque<InternalEvent>,
    microsteps: Vec<MicrostepTrace>,
    internal_unhandled: Vec<UnhandledInternalTrace>,
    effects: Vec<EffectOut>,
    /// Reactions performed so far, applied and discarded alike.
    reactions: u32,
}

impl Macrostep {
    fn after_trigger(trigger: &mut Transitioned) -> Self {
        Self {
            queue: VecDeque::new(),
            microsteps: Vec::new(),
            internal_unhandled: Vec::new(),
            effects: std::mem::take(&mut trigger.effects),
            reactions: 0,
        }
    }

    /// Charge one reaction against the ceiling before performing it.
    fn count_reaction(&mut self) -> Result<(), Rejection> {
        if self.reactions >= MAX_MICROSTEPS {
            return Err(self.limit_rejection());
        }
        self.reactions += 1;
        Ok(())
    }

    fn limit_rejection(&self) -> Rejection {
        let applied = self.microsteps.len();
        let hint = match (self.microsteps.last(), self.internal_unhandled.last()) {
            (Some(last), _) => format!(
                "microstep {} fired transition {} from state {} and the machine still had more to do; make a guard on the cycle become false, or point it at a state that quiesces",
                last.index, last.transition_idx, last.source_state
            ),
            (None, Some(discarded)) => format!(
                "every reaction discarded an internal event nothing handles, the last being {}; raise fewer events or add a transition that handles them",
                discarded.event
            ),
            (None, None) => "reduce the machine's reactions".into(),
        };
        Rejection {
            code: "run/microstep_limit",
            message: format!(
                "macrostep did not quiesce within {MAX_MICROSTEPS} reactions; {applied} microsteps applied after the trigger"
            ),
            hint,
            source_state: self.microsteps.last().map(|last| last.source_state.clone()),
            transition_idx: self.microsteps.last().map(|last| last.transition_idx),
            block: None,
            span: None,
            trace: DecisionTrace::default(),
            cause: None,
        }
    }

    /// Reject the whole macrostep: the trigger's trace at the top level, every
    /// completed microstep (and the one that failed, if any) with its blocks
    /// discarded.
    fn into_rejection(
        self,
        mut rejection: Rejection,
        trigger: TriggerSummary,
        failed: Option<MicrostepTrace>,
    ) -> Rejection {
        rejection.trace.candidates = trigger.candidates;
        rejection.trace.pipeline = discard(trigger.pipeline);
        let mut microsteps = self.microsteps;
        for microstep in &mut microsteps {
            for block in &mut microstep.pipeline {
                block.discarded = true;
            }
        }
        microsteps.extend(failed);
        rejection.trace.microsteps = microsteps;
        rejection.trace.internal_unhandled = self.internal_unhandled;
        rejection
    }
}

fn discard(mut pipeline: Vec<BlockTrace>) -> Vec<BlockTrace> {
    for block in &mut pipeline {
        block.discarded = true;
    }
    pipeline
}

/// What the trigger microstep contributes to the sealed result: the record
/// identity fields describe the trigger, never the union of microsteps.
struct TriggerSummary {
    internal: bool,
    region: Option<String>,
    source_state: String,
    transition_idx: u32,
    exited: Vec<String>,
    entered: Vec<String>,
    candidates: Vec<LevelTrace>,
    pipeline: Vec<BlockTrace>,
}

impl TriggerSummary {
    fn take_from(trigger: &mut Transitioned, tree: &Tree) -> Self {
        Self {
            internal: trigger.internal,
            region: trigger.region.clone(),
            source_state: trigger.source_state.clone(),
            transition_idx: trigger.public_index,
            exited: trigger.exited_names(tree),
            entered: trigger.entered_names(tree),
            candidates: std::mem::take(&mut trigger.candidates),
            pipeline: std::mem::take(&mut trigger.pipeline),
        }
    }
}

/// Run the reactions of a macrostep whose trigger microstep already applied.
///
/// `state` is the caller's pre-macrostep state, which supplies the prior
/// deadline schedules and is never mutated. `trigger` is the unsettled trigger
/// microstep; its deadline schedules settle here, after the invariants when
/// no reaction follows it and before the next microstep otherwise, so a
/// non-reactive machine keeps SPEC §Semantics' 8-then-9 order to the byte.
pub(super) fn run_to_quiescence(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    mut trigger: Transitioned,
    now_ms: i64,
    budget: &mut Budget,
    selector: &mut dyn ReactionSelector,
) -> Result<Applied, Rejection> {
    let summary = TriggerSummary::take_from(&mut trigger, tree);
    let mut macrostep = Macrostep::after_trigger(&mut trigger);
    let mut working = InstanceState {
        status: state.status,
        configuration: trigger.configuration_after.clone(),
        ctx: trigger.context.clone(),
        history: trigger.history_after.clone(),
        deadlines: state.deadlines.clone(),
        pending: state.pending.clone(),
    };
    let mut unsettled = trigger;
    loop {
        let index = macrostep.microsteps.len() as u32 + 1;
        let NextReaction {
            trigger: kind,
            selection,
            event,
        } = match select_next(machine, tree, &working, &mut macrostep, budget, selector) {
            Ok(Some(next)) => next,
            Ok(None) => break,
            Err(ScanFailure::Ceiling(rejection)) => {
                return Err(macrostep.into_rejection(rejection, summary, None));
            }
            Err(ScanFailure::Guard {
                trigger: kind,
                mut rejection,
            }) => {
                // A guard that failed to evaluate names its transition; keep
                // the scan's candidates as the microstep that never applied.
                let failed = MicrostepTrace {
                    index,
                    trigger: kind,
                    source_state: rejection.source_state.clone().unwrap_or_default(),
                    transition_idx: rejection.transition_idx.unwrap_or_default(),
                    region: None,
                    exited: Vec::new(),
                    entered: Vec::new(),
                    candidates: std::mem::take(&mut rejection.trace.candidates),
                    pipeline: Vec::new(),
                };
                return Err(macrostep.into_rejection(rejection, summary, Some(failed)));
            }
        };
        // Another reaction follows, so the previous microstep's schedules can
        // settle now; the last microstep's settle after the invariants below.
        match settle_schedules(
            machine,
            tree,
            &working.deadlines,
            &unsettled,
            now_ms,
            budget,
        ) {
            Ok((deadlines, status)) => {
                working.deadlines = deadlines;
                working.status = status;
            }
            Err(rejection) => return Err(macrostep.into_rejection(rejection, summary, None)),
        }
        let identity = (
            tree.names[selection.source as usize].clone(),
            selection.transition_idx as u32,
            selection.region.clone(),
        );
        let mut next = match apply_reaction(
            machine,
            tree,
            &working,
            selection,
            event.as_ref(),
            macrostep.effects.len() as u32,
            budget,
        ) {
            Ok(next) => next,
            Err(mut rejection) => {
                let (source_state, transition_idx, region) = identity;
                let failed = MicrostepTrace {
                    index,
                    trigger: kind,
                    source_state,
                    transition_idx,
                    region,
                    exited: Vec::new(),
                    entered: Vec::new(),
                    candidates: std::mem::take(&mut rejection.trace.candidates),
                    pipeline: std::mem::take(&mut rejection.trace.pipeline),
                };
                return Err(macrostep.into_rejection(rejection, summary, Some(failed)));
            }
        };
        working.configuration = next.configuration_after.clone();
        working.ctx = next.context.clone();
        working.history = next.history_after.clone();
        macrostep.effects.append(&mut next.effects);
        macrostep.microsteps.push(MicrostepTrace {
            index,
            trigger: kind,
            source_state: next.source_state.clone(),
            transition_idx: next.public_index,
            region: next.region.clone(),
            exited: next.exited_names(tree),
            entered: next.entered_names(tree),
            candidates: std::mem::take(&mut next.candidates),
            pipeline: std::mem::take(&mut next.pipeline),
        });
        unsettled = next;
    }

    // SPEC §Semantics 8, once per macrostep: an intermediate configuration is
    // mid-reaction by definition, and tripping an enforce invariant on a state
    // the machine was about to leave would make a correct machine unrunnable.
    let active = tree.active_state_names(&working.configuration);
    let (invariants_hold, monitor_flags, invariants) = eval_invariants(
        &machine.spec,
        &machine.compiled_exprs,
        &working.ctx,
        &active,
        budget,
    );
    if !invariants_hold {
        let rejection = invariant_rejection(machine, &summary, invariants);
        return Err(macrostep.into_rejection(rejection, summary, None));
    }
    let (deadlines_after, status_after) = match settle_schedules(
        machine,
        tree,
        &working.deadlines,
        &unsettled,
        now_ms,
        budget,
    ) {
        Ok(settled) => settled,
        Err(rejection) => {
            let mut rejection = macrostep.into_rejection(rejection, summary, None);
            rejection.trace.invariants = invariants;
            return Err(rejection);
        }
    };
    Ok(Applied {
        configuration_after: unsettled.configuration_after,
        ctx_after: unsettled.context,
        history_after: unsettled.history_after,
        deadlines_after,
        effects: macrostep.effects,
        monitor_flags,
        status_after,
        internal: summary.internal,
        region: summary.region,
        source_state: summary.source_state,
        transition_idx: summary.transition_idx,
        exited: summary.exited,
        entered: summary.entered,
        trace: DecisionTrace {
            candidates: summary.candidates,
            pipeline: summary.pipeline,
            invariants,
            microsteps: macrostep.microsteps,
            internal_unhandled: macrostep.internal_unhandled,
        },
    })
}

/// The loop order, fixed by SPEC: eventless first, then the queue front.
///
/// Draining eventless transitions before delivering an internal event means
/// an event is always delivered to a settled configuration, and it makes the
/// fixpoint independent of how many events a block raised. A popped event
/// that selects nothing is recorded and skipped, never rejected: rejecting
/// would have to unwind an already-applied trigger, and `on_unhandled`
/// governs callers sending events the machine does not model — it never
/// applies to an event the machine raised itself.
/// The reaction the loop selected next, before it is applied.
struct NextReaction {
    trigger: MicrostepTrigger,
    selection: ReactionSelection,
    /// The popped internal event, whose payload binds as `evt`.
    event: Option<InternalEvent>,
}

/// Why a scan ended the macrostep instead of selecting or quiescing.
enum ScanFailure {
    /// The ceiling was reached before the reaction could be performed.
    Ceiling(Rejection),
    /// A guard failed to evaluate during the named kind of scan.
    Guard {
        trigger: MicrostepTrigger,
        rejection: Rejection,
    },
}

fn select_next(
    machine: &CompiledMachine,
    tree: &Tree,
    working: &InstanceState,
    macrostep: &mut Macrostep,
    budget: &mut Budget,
    selector: &mut dyn ReactionSelector,
) -> Result<Option<NextReaction>, ScanFailure> {
    loop {
        let eventless = selector
            .select_eventless(machine, tree, working, budget)
            .map_err(|rejection| ScanFailure::Guard {
                trigger: MicrostepTrigger::Eventless,
                rejection,
            })?;
        if let Some(selection) = eventless {
            macrostep.count_reaction().map_err(ScanFailure::Ceiling)?;
            return Ok(Some(NextReaction {
                trigger: MicrostepTrigger::Eventless,
                selection,
                event: None,
            }));
        }
        let Some(event) = macrostep.queue.pop_front() else {
            return Ok(None);
        };
        macrostep.count_reaction().map_err(ScanFailure::Ceiling)?;
        let handler = selector
            .select_internal(machine, tree, working, &event, budget)
            .map_err(|rejection| ScanFailure::Guard {
                trigger: MicrostepTrigger::Internal(event.name.clone()),
                rejection,
            })?;
        match handler {
            Some(selection) => {
                return Ok(Some(NextReaction {
                    trigger: MicrostepTrigger::Internal(event.name.clone()),
                    selection,
                    event: Some(event),
                }));
            }
            None => macrostep.internal_unhandled.push(UnhandledInternalTrace {
                event: event.name,
                after_microstep: macrostep.microsteps.len() as u32,
            }),
        }
    }
}

fn apply_reaction(
    machine: &CompiledMachine,
    tree: &Tree,
    working: &InstanceState,
    selection: ReactionSelection,
    event: Option<&InternalEvent>,
    first_effect_index: u32,
    budget: &mut Budget,
) -> Result<Transitioned, Rejection> {
    let transition = &machine.spec.transitions[selection.transition_idx];
    let no_payload = BTreeMap::new();
    // `evt` binds only in the microstep whose trigger supplied it: an
    // eventless transition's block sees none, an internal event's block sees
    // the raised payload.
    let (event_name, event_fields, sees_event) = match event {
        Some(event) => (event.name.as_str(), &event.payload, true),
        None => ("", &no_payload, false),
    };
    apply_selected_transition(
        machine,
        tree,
        working,
        SelectedTransition {
            region: selection.region,
            leaf: selection.leaf,
            source: selection.source,
            target: transition.to.as_deref(),
            action: Block {
                sets: transition.sets.clone(),
                emits: transition.emits.clone(),
            },
            action_kind: BlockKind::Transition,
            owner: ExprSlotOwner::Transition(selection.transition_idx),
            event_name,
            event_fields,
            sees_event,
            public_index: selection.transition_idx as u32,
            candidates: selection.candidates,
            first_effect_index,
        },
        budget,
    )
}

/// Which invariant to blame, read out of the evaluated traces.
pub(super) struct InvariantFailure {
    /// The first invariant whose evaluation errored.
    pub(super) evaluation_error: Option<InvariantEvaluationError>,
    /// The invariant to name in the hint: the erroring one, else the first
    /// failing enforce invariant.
    pub(super) failed_invariant: Option<String>,
}

/// An invariant that could not be evaluated, with the error's identity.
pub(super) struct InvariantEvaluationError {
    pub(super) name: String,
    pub(super) code: &'static str,
    pub(super) message: String,
    pub(super) span: Option<(u32, u32)>,
}

pub(super) fn invariant_failure(
    spec: &MachineSpec,
    invariants: &[InvariantTrace],
) -> InvariantFailure {
    let evaluation_error = invariants.iter().find_map(|invariant| {
        invariant
            .error
            .as_ref()
            .map(|error| InvariantEvaluationError {
                name: invariant.name.clone(),
                code: error.code,
                message: error.message.clone(),
                span: error.span,
            })
    });
    let failed_invariant = evaluation_error
        .as_ref()
        .map(|error| error.name.clone())
        .or_else(|| {
            invariants
                .iter()
                .zip(&spec.invariants)
                .find(|(result, invariant)| {
                    !result.passed && invariant.mode == EnforceMode::Enforce
                })
                .map(|(result, _)| result.name.clone())
        });
    InvariantFailure {
        evaluation_error,
        failed_invariant,
    }
}

fn invariant_rejection(
    machine: &CompiledMachine,
    trigger: &TriggerSummary,
    invariants: Vec<InvariantTrace>,
) -> Rejection {
    let failure = invariant_failure(&machine.spec, &invariants);
    Rejection {
        code: "run/invariant",
        message: failure
            .evaluation_error
            .as_ref()
            .map(|error| format!("invariant {}: {}", error.name, error.message))
            .unwrap_or_else(|| "enforce invariant failed".into()),
        hint: failure
            .failed_invariant
            .as_ref()
            .map(|name| format!("adjust the action or invariant {name}"))
            .unwrap_or_else(|| "adjust the action or the invariant".into()),
        source_state: Some(trigger.source_state.clone()),
        transition_idx: Some(trigger.transition_idx),
        block: failure
            .evaluation_error
            .as_ref()
            .map(|error| format!("invariant({})", error.name)),
        span: failure
            .evaluation_error
            .as_ref()
            .and_then(|error| error.span),
        cause: failure.evaluation_error.as_ref().map(|error| error.code),
        trace: DecisionTrace {
            invariants,
            ..DecisionTrace::default()
        },
    }
}
