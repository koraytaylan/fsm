//! The lowest sequence a live derivation still depends on.
//!
//! The carry rule covers idempotency keys, which live in the folded state.
//! It does not cover the facts that live **only in records**, and `fsm-execute`
//! is built entirely out of those: plan 0016's rule is that the journal is the
//! executor's only memory, so several things about a *pending* effect are
//! recovered by scanning rather than read from `StoreState`. Archiving those
//! records does not corrupt the store. It changes what the executor concludes,
//! silently, which is worse.
//!
//! # The three scans that pin
//!
//! Each is real code in `crates/fsm-execute/`, and each fails differently:
//!
//! | Scan | Where | What archiving it does |
//! |---|---|---|
//! | the emitting record, by sequence | `effect.rs::emitting_record` | the effect is `exec/effect_unresolved` forever — it never runs and never fails |
//! | the creation record, scanned backwards | `effect.rs::emitting_record` | every effect a child emits on entry becomes unresolvable |
//! | every `effect_attempted` | `watch.rs::attempt_state` | the attempt count falls, so an exhausted effect retries again and `exec/retries_exhausted` never fires |
//!
//! This module reimplements which records those scans need rather than calling
//! into `fsm-execute`, because the dependency points the other way. The two
//! must agree, and agreement is proved by task `8104`'s suite rather than
//! assumed here.
//!
//! # Only a pending effect pins anything
//!
//! A live instance sitting idle at a gate contributes nothing, whatever its
//! age — its whole history is derivable from the base. That is what keeps the
//! feature useful on exactly the long-running workflows it exists for: a
//! workflow that has been running for a year but is waiting on an event does
//! not hold a year of records hostage. A settled instance pins nothing either:
//! a cancelled or completed instance's outstanding effects are never retried,
//! so their records are not load-bearing.

use fsm_core::json::Value;
use fsm_core::machine::Status;
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::StoreState;

use crate::store::ErrorObj;

/// Which of the three scans set the pin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinSource {
    /// The record whose transition emitted the effect.
    EmittingRecord,
    /// The `instance_created`, or `instance_invoked` for a child, that a
    /// creation-time emit resolves against.
    CreationRecord,
    /// The earliest `effect_attempted` for the effect: the count needs all of
    /// them, so the earliest is the one that bounds the cut.
    AttemptRecord,
}

impl PinSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EmittingRecord => "emitting_record",
            Self::CreationRecord => "creation_record",
            Self::AttemptRecord => "attempt_record",
        }
    }
}

/// The lowest sequence a live derivation needs, and why.
///
/// "Cannot seal above 38 240" is actionable; "cut refused" is not, so the
/// reason travels with the number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pin {
    pub seq: u64,
    pub instance_id: String,
    pub effect_id: String,
    pub source: PinSource,
}

impl Pin {
    /// The highest cut this pin admits. A cut must be strictly below the
    /// pinned record, because the cut itself is sealed.
    pub fn highest_admissible_cut(&self) -> u64 {
        self.seq.saturating_sub(1)
    }
}

/// The `seq` component of a `{instance}/{seq}/{k}` effect id.
fn emitted_seq(effect_id: &str) -> Option<u64> {
    let (head, _k) = effect_id.rsplit_once('/')?;
    let (_instance, seq) = head.rsplit_once('/')?;
    seq.parse().ok()
}

/// Whether a record is the one that brought `instance_id` into existence.
///
/// A child has no `instance_created`: its whole existence is derived from the
/// `instance_invoked` record whose `child_instance_id` names it, and a reader
/// that needs the former has already lost.
fn creates_instance(record: &Record, instance_id: &str) -> bool {
    let field = |name: &str| record.body.get(name).and_then(Value::as_str);
    match record.kind {
        RecordKind::InstanceCreated => field("instance_id") == Some(instance_id),
        RecordKind::InstanceInvoked => field("child_instance_id") == Some(instance_id),
        _ => false,
    }
}

fn creation_seq(records: &[Record], instance_id: &str) -> Option<u64> {
    records
        .iter()
        .find(|record| creates_instance(record, instance_id))
        .map(|record| record.seq)
}

/// The earliest `effect_attempted` for an effect id, which is what bounds the
/// cut: `attempt_state` derives the count from **all** of them, so losing the
/// earliest lowers the count and an exhausted effect retries again.
fn earliest_attempt_seq(records: &[Record], effect_id: &str) -> Option<u64> {
    records
        .iter()
        .filter(|record| record.kind == RecordKind::EffectAttempted)
        .filter(|record| record.body.get("effect_id").and_then(Value::as_str) == Some(effect_id))
        .map(|record| record.seq)
        .min()
}

/// The pin over a whole store, or `None` when nothing is pending.
///
/// Takes no lock, opens no store, and writes nothing — the preview asks this
/// question read-only.
pub fn pin(state: &StoreState, records: &[Record]) -> Option<Pin> {
    let mut lowest: Option<Pin> = None;
    let mut consider = |candidate: Pin| {
        if lowest.as_ref().is_none_or(|held| candidate.seq < held.seq) {
            lowest = Some(candidate);
        }
    };
    for (instance_id, instance) in &state.instances {
        if instance.status != Status::Running {
            continue;
        }
        for effect_id in &instance.pending {
            let Some(seq) = emitted_seq(effect_id) else {
                continue;
            };
            if seq == 0 {
                // A creation-time emit: the id carries a literal zero because
                // it is composed before the record's own sequence is known, so
                // what it needs is the creation record wherever it landed.
                if let Some(creation) = creation_seq(records, instance_id) {
                    consider(Pin {
                        seq: creation,
                        instance_id: instance_id.clone(),
                        effect_id: effect_id.clone(),
                        source: PinSource::CreationRecord,
                    });
                }
            } else {
                consider(Pin {
                    seq,
                    instance_id: instance_id.clone(),
                    effect_id: effect_id.clone(),
                    source: PinSource::EmittingRecord,
                });
            }
            if let Some(attempt) = earliest_attempt_seq(records, effect_id) {
                consider(Pin {
                    seq: attempt,
                    instance_id: instance_id.clone(),
                    effect_id: effect_id.clone(),
                    source: PinSource::AttemptRecord,
                });
            }
        }
    }
    lowest
}

/// The highest cut a store admits, or `None` when nothing pins it.
pub fn highest_admissible_cut(state: &StoreState, records: &[Record]) -> Option<u64> {
    pin(state, records).map(|pin| pin.highest_admissible_cut())
}

/// Refuse a cut at or above the pin.
///
/// The same `store/archive_refused` the carry rule uses, distinguished by its
/// hint rather than by a second code: one code for "this cut cannot be taken",
/// and the hint says which of the two reasons applies and what clears it.
pub fn admissible(cut: u64, state: &StoreState, records: &[Record]) -> Result<(), ErrorObj> {
    let Some(pin) = pin(state, records) else {
        return Ok(());
    };
    if cut < pin.seq {
        return Ok(());
    }
    let highest = pin.highest_admissible_cut();
    Err(ErrorObj::new(
        "store/archive_refused",
        format!(
            "sealing at seq {cut} would archive the record at seq {} that instance {} still needs \
             to resolve pending effect {} ({})",
            pin.seq,
            pin.instance_id,
            pin.effect_id,
            pin.source.as_str()
        ),
    )
    .hint(format!(
        "seal at `--before-seq {highest}` or lower, or acknowledge the pending effect and seal \
         again. The executor recovers a pending effect's arguments and attempt count by reading \
         records, so the records it still needs cannot be archived"
    ))
    .details(Value::Obj(std::collections::BTreeMap::from([
        (
            "highest_admissible_cut".into(),
            Value::Num(highest.to_string()),
        ),
        ("pinned_seq".into(), Value::Num(pin.seq.to_string())),
        ("instance_id".into(), Value::Str(pin.instance_id.clone())),
        ("effect_id".into(), Value::Str(pin.effect_id.clone())),
        ("source".into(), Value::Str(pin.source.as_str().into())),
    ]))))
}
