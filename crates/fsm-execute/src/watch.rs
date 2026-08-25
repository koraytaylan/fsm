//! The read-only observer.
//!
//! Every fact a decision needs is read here, from the journal, through
//! `Store::open_read_only` — no lock, no writes, one hash-verified consistent
//! prefix per open — so monitoring never perturbs a concurrent writer and a
//! restarted executor observes the same world its predecessor did.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::effect::PendingEffect;
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    /// Status and counts per observed instance.
    pub instance_states: BTreeMap<String, InstanceSnap>,
}

/// The read side of the executor.
pub struct Watcher {
    data_dir: PathBuf,
    last_seq: u64,
    resolved: BTreeMap<String, PendingEffect>,
}

impl Watcher {
    /// Watch the store in `data_dir` without opening it yet.
    pub fn new(data_dir: PathBuf) -> Self {
        let _ = (&data_dir, 0u64);
        unimplemented!("task 3703")
    }

    /// Open a fresh read-only store and reduce it to one [`Observation`].
    pub fn scan(&mut self, now_ms: i64) -> Result<Observation, ExecError> {
        let _ = (&self.data_dir, self.last_seq, &self.resolved, now_ms);
        unimplemented!("task 3703")
    }

    /// How many effect ids the memo currently holds.
    pub fn resolved_count(&self) -> usize {
        unimplemented!("task 3703")
    }
}
