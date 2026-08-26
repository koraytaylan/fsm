//! Pure what-if execution: create then step an event sequence.
//!
//! Simulation preserves sequential or parallel active configurations and
//! updates deadline schedules as events enter and exit states. It does not poll
//! deadlines; hosts do that explicitly through [`crate::step::poll_deadline`].
//! Every event runs a macrostep exactly as a live send would, so the cascade
//! one event causes is visible in its step's trace — the main authoring
//! affordance of a reactive definition — and a rejection is the whole
//! macrostep rejected, atomically.

#![allow(clippy::result_large_err)]

use std::collections::BTreeMap;

use crate::expr::eval::{Budget, Val};
use crate::json::Value;
use crate::machine::{ActiveConfiguration, CompiledMachine, InstanceState, Status};
use crate::step::{Outcome, Rejection, create, step};
use crate::tree::Tree;

/// Policy for an event rejection after simulation has been created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnReject {
    /// Stop after recording the rejected event.
    Stop,
    /// Retain the unchanged state and continue with later events.
    Continue,
}

/// One attempted event in a simulation report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimStep {
    /// Zero-based event index.
    pub index: usize,
    /// Declared event name supplied by the caller.
    pub event: String,
    /// Pure step outcome for this event.
    pub outcome: Outcome,
    /// Complete active configuration after this attempted event.
    pub configuration_after: ActiveConfiguration,
    /// Complete context after this attempted event.
    pub ctx_after: BTreeMap<String, Val>,
    /// Effects emitted by an applied event, in deterministic order.
    pub effects: Vec<crate::step::EffectOut>,
}

/// Successful report from a simulation whose initial creation succeeded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimReport {
    /// Attempted events, including a rejection that stopped the sequence.
    pub steps: Vec<SimStep>,
    /// Complete active configuration after the simulated sequence.
    pub final_configuration: ActiveConfiguration,
    /// Whether every active regional leaf is terminal after the sequence.
    pub terminal: bool,
    /// First rejected event index when [`OnReject::Stop`] stopped execution.
    pub stopped_at: Option<usize>,
}

/// Create an in-memory instance and deliver `events` without journaling.
///
/// Creation uses timestamp zero and event `i` uses `i` milliseconds for
/// deterministic deadline scheduling; no deadline is implicitly polled. A
/// creation failure is returned as its typed [`Rejection`]; no report or
/// placeholder active configuration is produced.
pub fn simulate(
    m: &CompiledMachine,
    t: &Tree,
    overrides: &BTreeMap<String, Val>,
    events: &[(String, Value)],
    on_reject: OnReject,
) -> Result<SimReport, Rejection> {
    let created = create(m, t, overrides, 0)?;
    let mut st = InstanceState {
        status: created.status_after,
        configuration: created.configuration_after.clone(),
        ctx: created.ctx_after.clone(),
        history: created.history_after.clone(),
        deadlines: created.deadlines_after.clone(),
        pending: Vec::new(),
    };
    let mut steps = Vec::new();
    let mut stopped_at = None;
    for (i, (ev, payload)) in events.iter().enumerate() {
        let mut budget = Budget::new(crate::limits::MACROSTEP_EVAL_TICKS);
        let out = step(m, t, &st, ev, payload, i as i64, &mut budget);
        match &out {
            Outcome::Applied(a) => {
                st.configuration = a.configuration_after.clone();
                st.ctx = a.ctx_after.clone();
                st.history = a.history_after.clone();
                st.deadlines = a.deadlines_after.clone();
                st.status = a.status_after;
                steps.push(SimStep {
                    index: i,
                    event: ev.clone(),
                    configuration_after: a.configuration_after.clone(),
                    ctx_after: a.ctx_after.clone(),
                    effects: a.effects.clone(),
                    outcome: out,
                });
            }
            Outcome::Rejected(_) => {
                steps.push(SimStep {
                    index: i,
                    event: ev.clone(),
                    configuration_after: st.configuration.clone(),
                    ctx_after: st.ctx.clone(),
                    effects: Vec::new(),
                    outcome: out,
                });
                if matches!(on_reject, OnReject::Stop) {
                    stopped_at = Some(i);
                    break;
                }
            }
            Outcome::Ignored => {
                steps.push(SimStep {
                    index: i,
                    event: ev.clone(),
                    configuration_after: st.configuration.clone(),
                    ctx_after: st.ctx.clone(),
                    effects: Vec::new(),
                    outcome: out,
                });
            }
        }
    }
    Ok(SimReport {
        final_configuration: st.configuration.clone(),
        terminal: st.status == Status::Completed,
        stopped_at,
        steps,
    })
}
