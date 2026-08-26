//! The pure decision table.
//!
//! Given an [`Observation`] and a `now_ms` it emits [`Directive`]s: it spawns
//! nothing, touches no store, and reads no clock, so a fresh process with an
//! empty in-flight map reaches the same conclusions its killed predecessor
//! did. Time arrives as a parameter because the driver owns the one clock in
//! the process; that is what keeps every decision a function of its inputs.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::json::Value;

use crate::config::{Advance, HandlerSpec, HandlerTable, substitute};
use crate::effect::PendingEffect;
use crate::error::ExecError;
use crate::rid::{ack_rid, event_rid, invoke_rid, poll_rid, return_rid, signal_rid};
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
    /// Create the child of a pending invocation slot.
    InvokeChild {
        /// The invoking instance.
        parent_instance_id: String,
        /// The slot to enact.
        slot: String,
        /// The child's derived id, carried for the trace, never invented.
        child_instance_id: String,
        /// The derived idempotency key.
        request_id: String,
    },
    /// Hand a settled child's result to its parent.
    InvocationReturn {
        /// The invoking instance.
        parent_instance_id: String,
        /// The slot to return.
        slot: String,
        /// The child whose result it carries.
        child_instance_id: String,
        /// The derived idempotency key.
        request_id: String,
    },
    /// Deliver one pending signal.
    SignalDeliver {
        /// The instance holding it.
        sender_instance_id: String,
        /// Its derived id.
        signal_id: String,
        /// Where it is addressed, for the trace.
        target_instance_id: String,
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

impl Directive {
    /// The effect this directive acts on, when it acts on one.
    fn effect_id(&self) -> Option<&str> {
        match self {
            Directive::Start { effect, .. } => Some(&effect.effect_id),
            Directive::Kill { effect_id, .. } => Some(effect_id),
            Directive::SendEvent { effect_id, .. } => Some(effect_id),
            Directive::PollDeadline { .. }
            | Directive::InvokeChild { .. }
            | Directive::InvocationReturn { .. }
            | Directive::SignalDeliver { .. } => None,
        }
    }
}

/// One handler running in *this* process right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inflight {
    /// The effect being run.
    pub effect: PendingEffect,
    /// The `now_ms` past which the run is killed.
    pub deadline_ms: i64,
    /// Whether a kill has already been directed for this run, so a tick that
    /// could not settle it does not direct a second one.
    pub killed: bool,
}

/// A pending effect whose handler cannot be started at all, so the run has
/// failed before it began.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unstartable {
    /// The effect that cannot be run.
    pub effect: PendingEffect,
    /// Why — an `exec/config` fault in the argv substitution.
    pub error: ExecError,
}

/// The executor's brain.
pub struct Scheduler {
    table: HandlerTable,
    inflight: BTreeMap<String, Inflight>,
    issued_polls: BTreeSet<(String, String, i64)>,
    unhandled: Vec<String>,
    reported_unhandled: BTreeSet<String>,
    unstartable: Vec<Unstartable>,
    stalled: Vec<String>,
    reported_stalls: BTreeSet<String>,
    parked_advances: BTreeMap<(String, String), u64>,
}

impl Scheduler {
    /// Decide against this handler table.
    pub fn new(table: HandlerTable) -> Self {
        Self {
            table,
            inflight: BTreeMap::new(),
            issued_polls: BTreeSet::new(),
            unhandled: Vec::new(),
            reported_unhandled: BTreeSet::new(),
            unstartable: Vec::new(),
            stalled: Vec::new(),
            reported_stalls: BTreeSet::new(),
            parked_advances: BTreeMap::new(),
        }
    }

    /// The handler for an effect name, for the driver that has to settle a run.
    pub fn handler(&self, effect_name: &str) -> Option<&HandlerSpec> {
        self.table.handlers.get(effect_name)
    }

    /// The effect behind an in-flight run.
    ///
    /// Settling a run needs the effect's name and args, and an observation is
    /// not always the place to find them: a cancelled instance stops offering
    /// its pending effects the moment the cancel is journaled, which is
    /// exactly when the run that must be killed still needs settling.
    pub fn inflight_effect(&self, effect_id: &str) -> Option<&PendingEffect> {
        self.inflight
            .get(effect_id)
            .map(|inflight| &inflight.effect)
    }

    /// Apply the decision table to one observation at one time.
    ///
    /// Two invariants hold over the returned sequence: no two directives name
    /// the same effect, and no directive carries a `request_id` the journal
    /// has already claimed. The second is what makes a restart safe — the
    /// journal, not this process's memory, is what prevents a repeat.
    pub fn on_observation(&mut self, obs: &Observation, now_ms: i64) -> Vec<Directive> {
        self.unhandled.clear();
        self.unstartable.clear();
        self.stalled.clear();
        let mut directives: Vec<Directive> = Vec::new();

        // 1. Start a handler for pending work nobody has claimed or started.
        for effect in &obs.pending {
            let Some(handler) = self.table.handlers.get(&effect.effect_name) else {
                // 2. Default-deny: an effect with no handler is a deliberate
                // stall. The executor refuses to guess what to run.
                if self.reported_unhandled.insert(effect.effect_id.clone()) {
                    self.unhandled.push(effect.effect_id.clone());
                }
                continue;
            };
            if self.inflight.contains_key(&effect.effect_id) {
                continue;
            }
            // A claimed ack key means some writer already settled this effect,
            // or burned the key on a rejection — or, for a creation-time id
            // after an instance id was re-used, that a *previous* life's
            // identical id claimed it. Running the handler again could never
            // be journaled, so the effect is stalled, and it says so out loud
            // rather than being skipped in silence.
            if obs
                .claimed_request_ids
                .contains(&ack_rid(&effect.effect_id))
            {
                if self.reported_stalls.insert(effect.effect_id.clone()) {
                    self.stalled.push(effect.effect_id.clone());
                }
                continue;
            }
            let argv = match substitute(&handler.argv, &effect.args) {
                Ok(argv) => argv,
                Err(error) => {
                    // A placeholder naming an argument this emit did not
                    // produce is a run-time failure of *this* effect, not a
                    // table the loop can repair. The driver acks it `failed`
                    // so the machine's own failure path can fire; skipping it
                    // silently would leave the effect pending forever with no
                    // diagnostic anywhere.
                    self.unstartable.push(Unstartable {
                        effect: effect.clone(),
                        error,
                    });
                    continue;
                }
            };
            self.inflight.insert(
                effect.effect_id.clone(),
                Inflight {
                    effect: effect.clone(),
                    deadline_ms: now_ms.saturating_add(handler.timeout_ms),
                    killed: false,
                },
            );
            directives.push(Directive::Start {
                effect: effect.clone(),
                argv,
                timeout_ms: handler.timeout_ms,
            });
        }

        // 3. Resume an advance whose ack is journaled but whose event is not.
        // This is what survives a kill between the two writes, and it equally
        // honours an effect a human acked from the CLI.
        for settled in &obs.settled {
            let Some(handler) = self.table.handlers.get(&settled.effect_name) else {
                continue;
            };
            let Some(advance) = declared_advance(handler, &settled.outcome) else {
                continue;
            };
            let request_id = event_rid(&settled.effect_id, &advance.event);
            if obs.claimed_request_ids.contains(&request_id) {
                continue;
            }
            // An advance the engine will not accept claims no key, so the ack
            // stays outstanding and this rule would re-derive the same
            // directive on every tick — and every one of those ticks would
            // open the writer, fold the journal, and write a snapshot on drop.
            // Park it until something new is journaled: a guard that is false
            // now can only become true through a record, so nothing to
            // re-evaluate means nothing to retry.
            let parked = (settled.effect_id.clone(), advance.event.clone());
            if self.parked_advances.get(&parked) == Some(&obs.to_seq) {
                continue;
            }
            directives.push(Directive::SendEvent {
                instance_id: settled.instance_id.clone(),
                effect_id: settled.effect_id.clone(),
                event: advance.event.clone(),
                payload: advance.payload.clone(),
                stamps: advance.stamps.clone(),
                request_id,
            });
        }

        // 4. Poll each due deadline once. `inflight` is keyed by effect and
        // holds no deadlines, so this needs its own set — and that set is
        // marked by the driver *after* a poll actually lands, never here.
        // Marking at decision time would silence the deadline forever if the
        // tick could not open the writer, and a deadline that never fires
        // again is a workflow that never times out.
        for due in &obs.due_deadlines {
            let key = (
                due.instance_id.clone(),
                due.deadline_name.clone(),
                due.due_ms,
            );
            let request_id = poll_rid(&due.instance_id, &due.deadline_name, due.due_ms);
            if obs.claimed_request_ids.contains(&request_id) || self.issued_polls.contains(&key) {
                continue;
            }
            directives.push(Directive::PollDeadline {
                instance_id: due.instance_id.clone(),
                deadline: due.deadline_name.clone(),
                due_ms: due.due_ms,
                request_id,
            });
        }

        // 4b. Composition, in causal order within the tick: invoke, then
        // return, then signal. A slot created and settled across two ticks
        // therefore never races itself, and the trace reads in the order the
        // work actually happened. Each rule is gated on its derived key being
        // absent from the journal, exactly as the rules above are.
        for slot in &obs.pending_invocations {
            let request_id = invoke_rid(&slot.parent_instance_id, &slot.slot);
            if obs.claimed_request_ids.contains(&request_id) {
                continue;
            }
            directives.push(Directive::InvokeChild {
                parent_instance_id: slot.parent_instance_id.clone(),
                slot: slot.slot.clone(),
                child_instance_id: slot.child_instance_id.clone(),
                request_id,
            });
        }
        for slot in &obs.returnable_invocations {
            let request_id = return_rid(&slot.parent_instance_id, &slot.slot);
            if obs.claimed_request_ids.contains(&request_id) {
                continue;
            }
            directives.push(Directive::InvocationReturn {
                parent_instance_id: slot.parent_instance_id.clone(),
                slot: slot.slot.clone(),
                child_instance_id: slot.child_instance_id.clone(),
                request_id,
            });
        }
        for signal in &obs.pending_signals {
            let request_id = signal_rid(&signal.sender_instance_id, &signal.signal_id);
            if obs.claimed_request_ids.contains(&request_id) {
                continue;
            }
            directives.push(Directive::SignalDeliver {
                sender_instance_id: signal.sender_instance_id.clone(),
                signal_id: signal.signal_id.clone(),
                target_instance_id: signal.target_instance_id.clone(),
                request_id,
            });
        }

        // 5. and 6. Stop runs the world has moved past. Cancellation is
        // checked first: a run killed because its instance was cancelled says
        // so in its ack, and collapsing it into a timeout would journal a lie.
        let cancelled: BTreeSet<&str> = obs.cancellations.iter().map(String::as_str).collect();
        let already_directed: BTreeSet<String> = directives
            .iter()
            .filter_map(|directive| directive.effect_id().map(str::to_string))
            .collect();
        for (effect_id, inflight) in &mut self.inflight {
            if inflight.killed {
                continue;
            }
            // An effect can be acked by another writer while this process's
            // handler is still running, which puts it in `settled` and in
            // `inflight` at once. One directive per effect per tick: the kill
            // waits for the next one, by which time the advance is journaled.
            if already_directed.contains(effect_id) {
                continue;
            }
            let reason = if cancelled.contains(inflight.effect.instance_id.as_str()) {
                KillReason::Cancelled
            } else if now_ms > inflight.deadline_ms {
                KillReason::Timeout
            } else {
                continue;
            };
            inflight.killed = true;
            directives.push(Directive::Kill {
                effect_id: effect_id.clone(),
                reason,
            });
        }

        debug_assert!(
            no_effect_directed_twice(&directives),
            "two directives for one effect in a single tick"
        );
        directives
    }

    /// Pending effects seen with no handler, for the loop to log once each.
    pub fn unhandled(&self) -> &[String] {
        &self.unhandled
    }

    /// Effects whose argv could not be built from what the emit produced.
    ///
    /// The driver acks these `failed` — the run failed before it began — which
    /// is what lets the machine's own failure path fire instead of the
    /// instance waiting on a handler that can never start.
    pub fn unstartable(&self) -> &[Unstartable] {
        &self.unstartable
    }

    /// Pending effects that can never be acked because their derived key is
    /// already claimed, reported once each.
    pub fn stalled(&self) -> &[String] {
        &self.stalled
    }

    /// Record that the engine declined an advance at this journal position.
    ///
    /// The pair is retried once the journal moves, and not before: the
    /// executor never fires an event it expects to be rejected, and never
    /// spins asking.
    pub fn park_advance(&mut self, effect_id: &str, event: &str, at_seq: u64) {
        self.parked_advances
            .insert((effect_id.to_string(), event.to_string()), at_seq);
    }

    /// Record that a deadline poll actually landed.
    ///
    /// Called by the driver rather than by the decision, so a poll that was
    /// decided but never executed is decided again on the next tick.
    pub fn poll_issued(&mut self, instance_id: &str, deadline: &str, due_ms: i64) {
        self.issued_polls
            .insert((instance_id.to_string(), deadline.to_string(), due_ms));
    }

    /// Clear an in-flight entry once its run reached a terminal path.
    ///
    /// Correctness after a restart never depends on this having been called —
    /// a fresh process starts with an empty map — but within one process an
    /// entry that is never cleared is invisible to the start rule forever,
    /// which is the one way this loop can wedge itself.
    pub fn complete(&mut self, effect_id: &str) {
        self.inflight.remove(effect_id);
    }
}

fn declared_advance<'a>(handler: &'a HandlerSpec, outcome: &str) -> Option<&'a Advance> {
    match outcome {
        "ok" => handler.on_ok.as_ref(),
        _ => handler.on_failed.as_ref(),
    }
}

fn no_effect_directed_twice(directives: &[Directive]) -> bool {
    let mut seen = BTreeSet::new();
    directives
        .iter()
        .filter_map(Directive::effect_id)
        .all(|effect_id| seen.insert(effect_id))
}
