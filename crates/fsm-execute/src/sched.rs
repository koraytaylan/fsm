//! The pure decision table.
//!
//! Given an [`Observation`] and a `now_ms` it emits [`Directive`]s: it spawns
//! nothing, touches no store, and reads no clock, so a fresh process with an
//! empty in-flight map reaches the same conclusions its killed predecessor
//! did.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::json::Value;

use crate::config::HandlerTable;
use crate::effect::PendingEffect;
use crate::run::KillReason;
use crate::watch::Observation;

/// One thing the executor should do this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Directive {
    /// Run a handler for a pending effect.
    Start {
        /// The resolved effect the handler answers.
        effect: PendingEffect,
        /// The substituted argv, `argv[0]` first.
        argv: Vec<String>,
        /// Milliseconds before the run is killed.
        timeout_ms: i64,
    },
    /// Stop an in-flight handler.
    Kill {
        /// The effect whose run is being stopped.
        effect_id: String,
        /// Why it is being stopped, which the ack records.
        reason: KillReason,
    },
    /// Poll one due deadline.
    PollDeadline {
        /// The instance holding the schedule.
        instance_id: String,
        /// The deadline's declared name — a key ingredient, not a selector.
        deadline: String,
        /// Its absolute due timestamp in milliseconds.
        due_ms: i64,
        /// The derived idempotency key.
        request_id: String,
    },
    /// Send an advance event for an already-acknowledged effect.
    SendEvent {
        /// The instance to advance.
        instance_id: String,
        /// The effect whose outcome triggers it.
        effect_id: String,
        /// The machine-declared event the handler table names.
        event: String,
        /// The payload the table declares.
        payload: Value,
        /// The stamp fields the table declares.
        stamps: Vec<String>,
        /// The derived idempotency key.
        request_id: String,
    },
}

/// One handler running in *this* process right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inflight {
    /// The effect being run.
    pub effect: PendingEffect,
    /// The `now_ms` past which the run is killed.
    pub deadline_ms: i64,
}

/// The executor's brain.
pub struct Scheduler {
    table: HandlerTable,
    inflight: BTreeMap<String, Inflight>,
    issued_polls: BTreeSet<(String, String, i64)>,
}

impl Scheduler {
    /// Decide against this handler table.
    pub fn new(table: HandlerTable) -> Self {
        let _ = &table;
        unimplemented!("task 3704")
    }

    /// Apply the decision table to one observation at one time.
    pub fn on_observation(&mut self, obs: &Observation, now_ms: i64) -> Vec<Directive> {
        let _ = (&self.table, &self.inflight, &self.issued_polls, obs, now_ms);
        unimplemented!("task 3704")
    }

    /// Pending effects seen with no handler, for the loop to log once each.
    pub fn unhandled(&self) -> &[String] {
        unimplemented!("task 3704")
    }

    /// Clear an in-flight entry once its run reached a terminal path.
    pub fn complete(&mut self, effect_id: &str) {
        let _ = effect_id;
        unimplemented!("task 3704")
    }
}
