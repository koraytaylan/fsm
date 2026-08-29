//! Running a case script against the production stepper.
//!
//! # This is not a second interpreter, and must never become one
//!
//! `crates/fsm-core/tests/oracle.rs` duplicates the engine's semantics on
//! purpose, to catch engine bugs by disagreeing with it. Cases test
//! **machines**, not the engine, so they run on the same primitives a live
//! send runs on. A second interpreter here would report an author's machine as
//! broken when the interpreter was, which is the one failure a testing tool
//! must not have.
//!
//! # An ack drives nothing, and that is the engine's rule
//!
//! There is no pure acknowledgement primitive in this crate, and this module
//! deliberately does not add one. Acking lives in `fsm-store`: it removes the
//! effect id from `instance.pending` and journals a record. In pure terms an
//! ack **is** that removal — no event, no transition, no configuration change —
//! and doing it here as the removal is the clearest statement of that rule
//! anywhere in the codebase.
//!
//! The consequence is worth stating before an author meets it: a case does
//! **not** inherit the executor's `on_ok` / `on_failed` follow-ups. That
//! mapping lives in the handler table, not in the machine, so a case that
//! wants the follow-up event writes the `send` itself. An author who expects
//! an ack to advance a workflow will otherwise write a case that mystifies
//! them.
//!
//! # Time is the script's, never the runner's
//!
//! A `send` uses its step index in milliseconds, exactly as
//! [`crate::simulate::simulate`] does. A `poll` uses the time the script
//! carries. Nothing here reads a clock — this crate has none, and acquiring
//! one would make a case's result depend on when it ran.
//!
//! # Every engine bound is inherited, none is relaxed
//!
//! The 64-microstep reaction bound, the evaluation budget, and every payload
//! ceiling apply exactly as they do to a live caller, and a case that trips one
//! reports the engine's own error. A runner that quietly raised a bound would
//! be testing a machine the engine will not run.

// A `Rejection` is large and is carried by value, exactly as
// `crate::simulate` carries it: these two entry points sit beside each other
// and an author reading one after the other should not meet a `Box` in one and
// not the other for a lint's sake.
#![allow(clippy::result_large_err)]
#![allow(clippy::large_enum_variant)]

use std::collections::BTreeMap;

use crate::analyze::{EventReport, enabled_events};
use crate::cases::format::{AckOutcome, Case, Step};
use crate::expr::eval::{Budget, Val};
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use crate::replay::parse_ctx_val;
use crate::step::{
    DeadlineOutcome, EffectOut, Outcome, Rejection, create, poll_deadline, step as step_event,
};
use crate::tree::Tree;

/// What one script step did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    /// An event was delivered, with the stepper's own verdict.
    Sent(Outcome),
    /// Deadlines were polled at the script's time, with the engine's verdict.
    Polled(DeadlineOutcome),
    /// A pending effect was acknowledged and removed from `pending`.
    Acked { effect: String, outcome: AckOutcome },
    /// The step could not run at all, and the case fails for this reason.
    ///
    /// Distinct from a *rejection*, which is the engine answering. This is the
    /// script asking for something that does not exist.
    Refused(StepRefusal),
}

/// A script step that could not be attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRefusal {
    pub message: String,
    /// The effects that *were* pending, when the step named one that was not.
    ///
    /// The list is the fix: naming an effect that has already settled, or
    /// misspelling one, is the mistake an author makes here, and a bare
    /// "unknown effect" costs them a round trip to discover which.
    pub pending: Vec<String>,
}

/// The complete observation after one step.
///
/// `pending` and `enabled` are what [`crate::simulate::SimStep`] lacks and what
/// a workflow case needs: a case that waits at a gate is asserting about
/// exactly those two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepObservation {
    /// Zero-based index into the case's script.
    pub index: usize,
    pub outcome: StepOutcome,
    pub configuration: ActiveConfiguration,
    pub ctx: BTreeMap<String, Val>,
    /// Effects emitted **by this step**, in emission order.
    pub emitted: Vec<EffectOut>,
    /// Every effect awaiting an ack after this step, in emission order.
    pub pending: Vec<String>,
    /// Declared events that would select a transition if sent now.
    pub enabled: Vec<EventReport>,
    /// Whether every active regional leaf is terminal after this step.
    pub terminal: bool,
}

/// A whole case, run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaseRun {
    pub name: String,
    /// Every step, always. The runner never stops early: an author correcting
    /// one expectation wants to see the other two in the same run.
    pub steps: Vec<StepObservation>,
    pub final_configuration: ActiveConfiguration,
    pub final_ctx: BTreeMap<String, Val>,
    pub final_pending: Vec<String>,
    pub final_enabled: Vec<EventReport>,
    pub terminal: bool,
}

/// Why a case could not be started at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaseError {
    /// A `context` entry names a slot the machine does not declare, or a value
    /// that is not of the declared type.
    Context { key: String, message: String },
    /// Creation itself was rejected, with the engine's own rejection.
    Create(Rejection),
}

/// Coerce a case's `context` block against the machine's declared slots.
///
/// The same string form and the same coercion every other caller uses, so a
/// value written in a case file means what it means everywhere else.
fn overrides_of(
    machine: &CompiledMachine,
    written: &BTreeMap<String, String>,
) -> Result<BTreeMap<String, Val>, CaseError> {
    let mut out = BTreeMap::new();
    for (key, raw) in written {
        let Some(slot) = machine.spec.context.iter().find(|slot| slot.name == *key) else {
            return Err(CaseError::Context {
                key: key.clone(),
                message: format!(
                    "the machine declares no context slot {key}; it declares {}",
                    machine
                        .spec
                        .context
                        .iter()
                        .map(|slot| slot.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        };
        let Some(value) = parse_ctx_val(&slot.ty, raw) else {
            return Err(CaseError::Context {
                key: key.clone(),
                message: format!("{raw:?} is not a value of {key}'s declared type"),
            });
        };
        out.insert(key.clone(), value);
    }
    Ok(out)
}

/// The names of an instance's pending effects, in emission order.
///
/// `InstanceState::pending` holds effect *ids* in a live store; in a pure run
/// there is no allocator, so the runner tracks names directly — which is also
/// the vocabulary the case file acks in.
fn observe(
    machine: &CompiledMachine,
    tree: &Tree,
    state: &InstanceState,
    index: usize,
    outcome: StepOutcome,
    emitted: Vec<EffectOut>,
) -> StepObservation {
    let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
    StepObservation {
        index,
        outcome,
        configuration: state.configuration.clone(),
        ctx: state.ctx.clone(),
        emitted,
        pending: state.pending.clone(),
        enabled: enabled_events(machine, tree, state, &mut budget),
        terminal: state.status == Status::Completed,
    }
}

/// Run one case against one definition.
///
/// Creation failures are returned as [`CaseError`]; everything after that is
/// recorded per step and the whole script runs.
pub fn run_case(machine: &CompiledMachine, tree: &Tree, case: &Case) -> Result<CaseRun, CaseError> {
    let overrides = overrides_of(machine, &case.context)?;
    let created = create(machine, tree, &overrides, 0).map_err(CaseError::Create)?;
    let mut state = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after.clone(),
        ctx: created.ctx_after.clone(),
        history: created.history_after.clone(),
        deadlines: created.deadlines_after.clone(),
        // Creation can emit, and those effects are pending from the first
        // instant — a case that acks one before its first send is correct.
        pending: created.effects.iter().map(|e| e.name.clone()).collect(),
        invocations: BTreeMap::new(),
        signals: BTreeMap::new(),
    };

    let mut steps = Vec::new();
    for (index, scripted) in case.script.iter().enumerate() {
        let (outcome, emitted) = match scripted {
            Step::Send { event, payload } => {
                let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
                let out = step_event(
                    machine,
                    tree,
                    &state,
                    event,
                    payload,
                    index as i64,
                    &mut budget,
                );
                let emitted = apply_event(&mut state, &out);
                (StepOutcome::Sent(out), emitted)
            }
            Step::Poll { now_ms } => {
                let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
                let out = poll_deadline(machine, tree, &state, *now_ms, &mut budget);
                let emitted = apply_deadline(&mut state, &out);
                (StepOutcome::Polled(out), emitted)
            }
            // `result` is the payload the executor would have carried back.
            // It reaches no machine: an ack drives nothing, so there is
            // nowhere for it to go. It is carried in the file because the
            // regeneration and reporting surfaces show it, and dropping it
            // here would make a case's text and its run disagree.
            Step::Ack {
                effect,
                outcome,
                result: _,
            } => {
                // An ack is *exactly* the removal. Nothing else here touches
                // configuration, context, deadlines, or history, and the test
                // that asserts they are unchanged is the engine rule made
                // checkable.
                match state.pending.iter().position(|name| name == effect) {
                    Some(at) => {
                        state.pending.remove(at);
                        (
                            StepOutcome::Acked {
                                effect: effect.clone(),
                                outcome: *outcome,
                            },
                            Vec::new(),
                        )
                    }
                    None => (
                        StepOutcome::Refused(StepRefusal {
                            message: format!("{effect} is not pending"),
                            pending: state.pending.clone(),
                        }),
                        Vec::new(),
                    ),
                }
            }
        };
        steps.push(observe(machine, tree, &state, index, outcome, emitted));
    }

    let mut budget = Budget::new(crate::limits::MAX_EVAL_TICKS);
    Ok(CaseRun {
        name: case.name.clone(),
        final_configuration: state.configuration.clone(),
        final_ctx: state.ctx.clone(),
        final_pending: state.pending.clone(),
        final_enabled: enabled_events(machine, tree, &state, &mut budget),
        terminal: state.status == Status::Completed,
        steps,
    })
}

/// Advance the state for an applied event, returning what it emitted.
///
/// A rejected or ignored event changes nothing, which is the stepper's own
/// atomicity rule rather than a decision made here.
fn apply_event(state: &mut InstanceState, outcome: &Outcome) -> Vec<EffectOut> {
    let Outcome::Applied(applied) = outcome else {
        return Vec::new();
    };
    state.configuration = applied.configuration_after.clone();
    state.ctx = applied.ctx_after.clone();
    state.history = applied.history_after.clone();
    state.deadlines = applied.deadlines_after.clone();
    state.status = applied.status_after;
    for effect in &applied.effects {
        state.pending.push(effect.name.clone());
    }
    applied.effects.clone()
}

/// The same, for a poll that applied one due deadline.
fn apply_deadline(state: &mut InstanceState, outcome: &DeadlineOutcome) -> Vec<EffectOut> {
    let DeadlineOutcome::Applied(applied) = outcome else {
        return Vec::new();
    };
    apply_event(state, &Outcome::Applied(applied.transition.clone()))
}
