//! The read-only observer.
//!
//! Every fact a decision needs is read here, from the journal, through
//! `Store::open_read_only` — no lock, no writes, one hash-verified consistent
//! prefix per open — so monitoring never perturbs a concurrent writer and a
//! restarted executor observes the same world its predecessor did.
//!
//! The scan reads `store.state.instances` directly rather than going through
//! `instance_view`. The view builds a whole response per instance and
//! evaluates `enabled_events` under a step budget for each; nothing here needs
//! either, and paying a full analysis per instance several times a second buys
//! nothing. `enabled_events` matters exactly once, in the pipeline, on the one
//! instance it just acked.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use fsm_core::json::Value;
use fsm_core::machine::{InvokeStatus, Status};
use fsm_core::record::RecordKind;
use fsm_store::store::Store;

use crate::effect::{PendingEffect, resolve};
use crate::error::ExecError;

/// One acknowledged effect whose advance event may still be outstanding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettledEffect {
    /// The instance that emitted it.
    pub instance_id: String,
    /// The acknowledged effect id.
    pub effect_id: String,
    /// Its re-derived effect name, so a handler can be looked up.
    pub effect_name: String,
    /// `ok` or `failed`, as journaled.
    pub outcome: String,
    /// The `seq` of the `effect_acked` record.
    pub seq: u64,
}

/// One deadline that is at or past the observed time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueDeadline {
    /// The instance holding the schedule.
    pub instance_id: String,
    /// The deadline's declared name.
    pub deadline_name: String,
    /// Its absolute due timestamp in milliseconds.
    pub due_ms: i64,
}

/// Counts and a status per observed instance — nothing hashed or rendered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceSnap {
    /// `running`, `completed`, or `cancelled`.
    pub status: String,
    /// How many effects are pending.
    pub pending: usize,
    /// How many deadlines are scheduled.
    pub deadlines: usize,
}

/// One consistent journal prefix, reduced to the facts a decision needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observation {
    /// The last seq the previous scan saw.
    pub from_seq: u64,
    /// The last seq this scan saw.
    pub to_seq: u64,
    /// Every effect currently in an instance's outbox, resolved.
    pub pending: Vec<PendingEffect>,
    /// Acks whose advance event may still be outstanding.
    pub settled: Vec<SettledEffect>,
    /// Deadlines at or past the observed time, on running instances.
    pub due_deadlines: Vec<DueDeadline>,
    /// Instances newly observed as cancelled.
    pub cancellations: Vec<String>,
    /// The executor's own claimed idempotency keys, from the journal.
    pub claimed_request_ids: BTreeSet<String>,
    /// What has already been tried, per effect, read from the journal.
    ///
    /// The whole point of journaling attempts is that a fresh process
    /// reaches the same conclusion its killed predecessor did, so this is
    /// derived from records rather than remembered between ticks.
    pub attempts: BTreeMap<String, AttemptState>,
    /// Status and counts per observed instance.
    pub instance_states: BTreeMap<String, InstanceSnap>,
    /// Slots waiting for their child to exist, parent first.
    pub pending_invocations: Vec<Slot>,
    /// Slots whose child has settled and whose result the parent has not
    /// taken yet.
    pub returnable_invocations: Vec<Slot>,
    /// Signals emitted and not yet delivered.
    pub pending_signals: Vec<PendingSignalRef>,
    /// Effect ids this scan could not re-derive, one error each.
    ///
    /// A pending effect nobody can name is not a reason to abandon the scan —
    /// the other instances still need running — so it is reported here and the
    /// service loop logs it.
    pub unresolved: Vec<ExecError>,
}

/// One invocation slot the executor can act on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Slot {
    /// The invoking instance.
    pub parent_instance_id: String,
    /// The slot id, unique machine-wide.
    pub slot: String,
    /// The child's derived id, which the executor never invents.
    pub child_instance_id: String,
}

/// One undelivered signal the executor can deliver.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PendingSignalRef {
    /// The instance holding it.
    pub sender_instance_id: String,
    /// Its derived id, `{instance}/{seq}/{k}`.
    pub signal_id: String,
    /// Where it is addressed.
    pub target_instance_id: String,
    /// The event it carries.
    pub event: String,
}

/// The read side of the executor.
pub struct Watcher {
    data_dir: PathBuf,
    last_seq: u64,
    resolved: BTreeMap<String, PendingEffect>,
    /// What each instance's status was at the previous scan, so a cancellation
    /// is reported on the scan that observes it and not on every scan after.
    previous_statuses: BTreeMap<String, String>,
    /// Effect names whose handler declares an advance for some outcome.
    ///
    /// An ack for anything else can never retire — no advance means no key is
    /// ever claimed — so listing it as outstanding would fill the bounded
    /// window with acks nobody will ever act on and push a genuinely
    /// interrupted advance out of it.
    advancing_effects: BTreeSet<String>,
    /// Effect ids already reported as unresolvable, so a broken id is one line
    /// rather than one line per tick forever.
    reported_unresolved: BTreeSet<String>,
}

impl Watcher {
    /// Watch the store in `data_dir` without opening it yet.
    ///
    /// `advancing_effects` names the effects whose handler declares an advance
    /// event; the driver takes it from the handler table. An empty set is
    /// honest for a watcher that only observes.
    pub fn new(data_dir: PathBuf, advancing_effects: BTreeSet<String>) -> Self {
        Self {
            data_dir,
            last_seq: 0,
            resolved: BTreeMap::new(),
            previous_statuses: BTreeMap::new(),
            advancing_effects,
            reported_unresolved: BTreeSet::new(),
        }
    }

    /// Open a fresh read-only store and reduce it to one [`Observation`].
    ///
    /// A fresh open per scan is deliberate: `open_read_only` returns one
    /// consistent prefix, and records a writer appends afterwards are visible
    /// on the next open. Holding a handle across scans would mean deciding
    /// against a snapshot that is quietly going stale.
    pub fn scan(&mut self, now_ms: i64) -> Result<Observation, ExecError> {
        let store = Store::open_read_only(&self.data_dir).map_err(|error| {
            ExecError::store(&error).hint(format!(
                "point the executor at a readable fsm data directory ({} could not be opened)",
                self.data_dir.display()
            ))
        })?;
        let mut observation = Observation {
            from_seq: self.last_seq,
            to_seq: store.journal.last_seq,
            claimed_request_ids: claimed_executor_keys(&store),
            attempts: attempt_state(&store),
            ..Observation::default()
        };
        let mut memo = BTreeMap::new();
        let mut unresolved: Vec<(String, ExecError)> = Vec::new();

        for (instance_id, instance) in &store.state.instances {
            observation.instance_states.insert(
                instance_id.clone(),
                InstanceSnap {
                    status: instance.status.as_str().to_string(),
                    pending: instance.pending.len(),
                    deadlines: instance.deadlines.len(),
                },
            );
            // Composition, read from the same public fields the scan
            // already has open: no second store call and no `instance_view`.
            if instance.status == Status::Running {
                for (slot, invocation) in &instance.invocations {
                    let child_instance_id = fsm_core::hashes::child_instance_id(instance_id, slot);
                    let entry = Slot {
                        parent_instance_id: instance_id.clone(),
                        slot: slot.clone(),
                        child_instance_id: child_instance_id.clone(),
                    };
                    match invocation.status {
                        InvokeStatus::Pending => observation.pending_invocations.push(entry),
                        // Settled means the child's own status says so; the
                        // executor never decides that from elapsed time.
                        InvokeStatus::Running => {
                            if store
                                .state
                                .instances
                                .get(&child_instance_id)
                                .is_some_and(|child| child.status != Status::Running)
                            {
                                observation.returnable_invocations.push(entry);
                            }
                        }
                        InvokeStatus::Returned => {}
                    }
                }
                for (signal_id, signal) in &instance.signals {
                    observation.pending_signals.push(PendingSignalRef {
                        sender_instance_id: instance_id.clone(),
                        signal_id: signal_id.clone(),
                        target_instance_id: signal.target_instance_id.clone(),
                        event: signal.event.clone(),
                    });
                }
            }
            if instance.status == Status::Cancelled
                && self.previous_statuses.get(instance_id).map(String::as_str)
                    == Some(Status::Running.as_str())
            {
                observation.cancellations.push(instance_id.clone());
            }
            // Completed instances included, cancelled ones not. A transition
            // into a terminal state emits its entry-block effects like any
            // other — a final notification is the obvious modelling of it — so
            // filtering on `running` would silently never run them. Cancel is
            // the opposite case: it means stop, the scheduler kills whatever
            // is in flight for that instance, and starting a *new* handler for
            // work the operator just cancelled would undo that. Those effects
            // stay pending in the journal, unacknowledged and visible, which
            // is the honest record of a run that was stopped.
            if instance.status != Status::Cancelled {
                for effect_id in &instance.pending {
                    match self.resolve_once(&store, effect_id, &mut memo) {
                        Ok(effect) => observation.pending.push(effect),
                        Err(error) => unresolved.push((effect_id.clone(), error)),
                    }
                }
            }
            // The engine rejects a poll against a completed or cancelled
            // instance, so a due deadline on one is not work, it is noise.
            if instance.status == Status::Running {
                for (deadline_name, due_ms) in &instance.deadlines {
                    if *due_ms <= now_ms {
                        observation.due_deadlines.push(DueDeadline {
                            instance_id: instance_id.clone(),
                            deadline_name: deadline_name.clone(),
                            due_ms: *due_ms,
                        });
                    }
                }
            }
        }

        for ack in outstanding_acks(&store, &observation.claimed_request_ids) {
            match self.resolve_once(&store, &ack.effect_id, &mut memo) {
                Ok(effect) if self.advancing_effects.contains(&effect.effect_name) => {
                    observation.settled.push(SettledEffect {
                        instance_id: ack.instance_id,
                        effect_id: ack.effect_id,
                        effect_name: effect.effect_name,
                        outcome: ack.outcome,
                        seq: ack.seq,
                    });
                }
                // An ack whose handler declares no advance needs nothing from
                // anyone; carrying it would only crowd the window.
                Ok(_) => {}
                Err(error) => unresolved.push((ack.effect_id.clone(), error)),
            }
        }

        // One line per broken id, not one per tick: an id that cannot be
        // resolved this scan cannot be resolved by the next one either.
        for (effect_id, error) in unresolved {
            if self.reported_unresolved.insert(effect_id) {
                observation.unresolved.push(error);
            }
        }

        // Carrying only what this scan referenced keeps the memo bounded: an
        // effect that has been settled and advanced past is never asked for
        // again.
        self.resolved = memo;
        self.previous_statuses = observation
            .instance_states
            .iter()
            .map(|(instance_id, snapshot)| (instance_id.clone(), snapshot.status.clone()))
            .collect();
        self.last_seq = observation.to_seq;
        Ok(observation)
    }

    /// How many effect ids the memo currently holds.
    pub fn resolved_count(&self) -> usize {
        self.resolved.len()
    }

    /// Resolve through the memo: one prefix fold per effect id, ever, rather
    /// than one per scan for as long as the effect stays pending.
    ///
    /// A creation-time id is never memoized across scans. `{instance}/0/{k}`
    /// carries a literal zero rather than the record's seq, so re-using an
    /// instance id produces the same id for a different emit; the resolver
    /// deliberately reads the *newest* `instance_created` record, and a memo
    /// keyed on the id alone would hand back the previous life's arguments and
    /// run the handler against values the instance no longer holds.
    fn resolve_once(
        &self,
        store: &Store,
        effect_id: &str,
        memo: &mut BTreeMap<String, PendingEffect>,
    ) -> Result<PendingEffect, ExecError> {
        if let Some(known) = memo.get(effect_id) {
            return Ok(known.clone());
        }
        let effect = match self.resolved.get(effect_id) {
            Some(known) if !is_creation_time(effect_id) => known.clone(),
            _ => resolve(store, effect_id)?,
        };
        if !is_creation_time(effect_id) {
            memo.insert(effect_id.to_string(), effect.clone());
        }
        Ok(effect)
    }
}

/// One journaled ack, before its name has been re-derived.
struct AckedEffect {
    instance_id: String,
    effect_id: String,
    outcome: String,
    seq: u64,
}

/// How many outstanding acks one instance contributes to a scan.
///
/// One is the number that matters: sending an advance transitions the
/// instance, so a second advance for the same instance in the same moment
/// could not be enabled anyway. The rest of the allowance is slack for a
/// transition that emitted several effects at once, all of them acked before
/// the executor died.
const MAX_SETTLED_PER_INSTANCE: usize = 8;

/// Acks of running instances whose advance event has not been journaled.
///
/// "Has not been journaled" is read from the dedup map rather than inferred
/// from record order: every advance the executor sends claims a key beginning
/// `exec-ev-{effect_id}-`, so its absence is exact. The obvious alternative —
/// dropping any ack older than the instance's newest transition — silently
/// abandons the advance whenever some other writer moves the instance between
/// the ack and the restart, which is precisely the interruption this list
/// exists to repair.
///
/// Acks with no advance to send (a handler that declares none, so no key is
/// ever claimed) would otherwise accumulate for the life of a running
/// instance, so each instance contributes at most its newest
/// [`MAX_SETTLED_PER_INSTANCE`].
fn outstanding_acks(store: &Store, claimed: &BTreeSet<String>) -> Vec<AckedEffect> {
    let mut per_instance: BTreeMap<&str, Vec<AckedEffect>> = BTreeMap::new();
    for record in store.records.iter().rev() {
        if record.kind != RecordKind::EffectAcked {
            continue;
        }
        let (Some(instance_id), Some(effect_id)) = (
            instance_of(record),
            record.body.get("effect_id").and_then(Value::as_str),
        ) else {
            continue;
        };
        let running = store
            .state
            .instances
            .get(instance_id)
            .is_some_and(|instance| instance.status == Status::Running);
        if !running {
            continue;
        }
        let collected = per_instance.entry(instance_id).or_default();
        if collected.len() >= MAX_SETTLED_PER_INSTANCE {
            continue;
        }
        if advance_already_sent(claimed, effect_id) {
            continue;
        }
        collected.push(AckedEffect {
            instance_id: instance_id.to_string(),
            effect_id: effect_id.to_string(),
            outcome: record
                .body
                .get("outcome")
                .and_then(Value::as_str)
                .unwrap_or("ok")
                .to_string(),
            seq: record.seq,
        });
    }
    let mut acks: Vec<AckedEffect> = per_instance.into_values().flatten().collect();
    acks.sort_by_key(|ack| ack.seq);
    acks
}

/// Whether any advance key for this effect is already claimed.
///
/// One effect can declare a different event per outcome, so the check is over
/// the `exec-ev-{effect_id}-` prefix rather than one exact key — the watcher
/// does not know which event the table names, and does not need to.
fn advance_already_sent(claimed: &BTreeSet<String>, effect_id: &str) -> bool {
    let prefix = format!("exec-ev-{effect_id}-");
    claimed
        .range(prefix.clone()..)
        .next()
        .is_some_and(|key| key.starts_with(&prefix))
}

/// The executor's own claimed keys.
///
/// The full dedup map holds one entry per request the store has *ever* served;
/// copying that several times a second would cost real memory for no purpose,
/// since the executor only ever asks about keys it derived itself.
/// What one effect has already been through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttemptState {
    /// The highest attempt number journaled for it.
    pub attempt: u32,
    /// When that attempt was recorded, by the record's own `ts`.
    ///
    /// The record's timestamp rather than a clock read: a backoff deadline
    /// computed from the journal is the same deadline in every process that
    /// reads it.
    pub last_ts: i64,
}

/// The attempt count for every effect, derived from the records.
///
/// This scans the live journal and nothing else, and on a sealed store that is
/// **complete** rather than merely convenient. Plan 0017's pin refuses to
/// archive any record a pending effect's execution is derived from — its
/// emitting record, its instance's creation record, and every one of its
/// attempt records — so an archived attempt record for a pending effect cannot
/// exist. Without that guarantee the count would fall silently, an exhausted
/// effect would retry again, and `exec/retries_exhausted` would never fire.
/// `crates/fsm-execute/tests/sealed_store.rs` proves it rather than assuming
/// it; this comment is why the scan is safe, not a promise that it is.
fn attempt_state(store: &Store) -> BTreeMap<String, AttemptState> {
    let mut out: BTreeMap<String, AttemptState> = BTreeMap::new();
    for record in &store.records {
        if record.kind != RecordKind::EffectAttempted {
            continue;
        }
        let Some(effect_id) = record.body.get("effect_id").and_then(Value::as_str) else {
            continue;
        };
        let attempt = record
            .body
            .get("attempt")
            .and_then(Value::as_num)
            .and_then(|attempt| attempt.parse::<u32>().ok())
            .unwrap_or(0);
        let state = out.entry(effect_id.to_string()).or_default();
        if attempt >= state.attempt {
            state.attempt = attempt;
            state.last_ts = record.ts;
        }
    }
    out
}

fn claimed_executor_keys(store: &Store) -> BTreeSet<String> {
    store
        .state
        .dedup
        .keys()
        .filter(|key| key.starts_with("exec-"))
        .cloned()
        .collect()
}

fn instance_of(record: &fsm_core::record::Record) -> Option<&str> {
    record.body.get("instance_id").and_then(Value::as_str)
}

/// Whether an effect id names a creation-time emit, whose `seq` component is a
/// literal zero and therefore repeats if an instance id is re-used.
fn is_creation_time(effect_id: &str) -> bool {
    effect_id
        .rsplit_once('/')
        .and_then(|(head, _)| head.rsplit_once('/'))
        .is_some_and(|(_, seq)| seq == "0")
}
