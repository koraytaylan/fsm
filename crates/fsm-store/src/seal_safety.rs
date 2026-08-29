//! Which idempotency keys a seal carries, and the proof that dropping the rest
//! is safe.
//!
//! # Why a dropped key needs a proof rather than a policy
//!
//! A `request_id` is an idempotency key over content. A key the store still
//! holds is replayed; a key it has never seen is applied. A key it **dropped**
//! is indistinguishable from one it never saw — there is no honest way to
//! report "this one expired", because nothing records that it existed. So a
//! seal may only drop a key when every path that would re-apply it is closed
//! for an independent reason, and there are exactly three kinds of claim that
//! can sit at or below a cut and not belong to a live instance:
//!
//! * **An event, poll, ack, or annotation against a settled instance.** A
//!   completed or cancelled instance refuses the operation on its terminal
//!   status, whatever the key says.
//! * **A `create`.** Every surface derives `inst-<request_id>`, so re-issuing
//!   the request produces the same instance id — and `create` **refuses** an
//!   id that exists rather than replacing it. That refusal did not exist
//!   before this task: the store would silently reset the instance it had
//!   already made, which is precisely the double-apply this rule promises is
//!   impossible. A closure has to be true, not plausible.
//! * **A `machine add`.** A definition is content-addressed, so re-issuing it
//!   is idempotent by hash.
//!
//! Each of those is asserted against a real store in
//! `crates/fsm-store/tests/seal_safety.rs`, not argued here. This module doc
//! is the reason; the suite is the evidence.
//!
//! # Why the naive rule does not survive contact with a real store
//!
//! The first shape of this rule dropped everything at or below the cut and
//! refused the seal if any of it belonged to a live instance. A cut sits at or
//! near the head, so nearly every entry is below it — **including every key of
//! every instance still running**. That rule would have refused every seal a
//! live store could ever ask for. So the rule is:
//!
//! > A seal at `N` **carries** every entry whose claiming sequence is above
//! > `N`, **or** whose claiming record names an instance that is live in the
//! > base state. It **drops** the rest, and it is refused only when the
//! > carried set does not fit the base file.
//!
//! The bound that produces is the correct one: **carried dedup tracks live
//! workload, not lifetime.** A store with a thousand finished instances and
//! three running ones carries three instances' keys. The refusal is a size
//! limit, not a liveness veto, and the difference is why the feature is usable.
//!
//! # Carrying an entry is not yet enough to replay it
//!
//! `store/idempotency.rs::replay_request` rebuilds a retry's original response
//! by **scanning records** for the one that claimed the key, so a carried entry
//! whose claiming record is below the cut has an entry and no record. Closing
//! that is task `8101`'s, where the reader lives; this module's obligation is
//! to decide the partition.

use std::collections::{BTreeMap, BTreeSet};

use fsm_core::canon::canon_bytes;
use fsm_core::machine::Status;
use fsm_core::record::{Record, instances_touched};
use fsm_core::replay::{RequestSlot, StoreState};

use crate::base::{self, DefinitionLimits};
use crate::store::ErrorObj;

/// The partition of a store's idempotency ledger at a proposed cut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CarryDecision {
    /// Entries the base file will hold, with their fingerprints.
    pub carried: BTreeMap<String, RequestSlot>,
    /// Keys the seal drops, each closed by one of the three arguments above.
    pub dropped: BTreeSet<String>,
}

impl CarryDecision {
    pub fn carried_count(&self) -> usize {
        self.carried.len()
    }

    pub fn dropped_count(&self) -> usize {
        self.dropped.len()
    }
}

fn refused(reason: &str) -> ErrorObj {
    ErrorObj::new("store/archive_refused", reason.to_string()).hint(
        "two things clear this: seal at an earlier `--before-seq`, so fewer keys are live at the \
         cut, or let running instances settle, so their keys stop needing to be carried. This is \
         a size limit on the base state file, not a rule against sealing a store that has work in \
         flight",
    )
}

/// Whether an instance is still able to accept an operation.
///
/// A settled instance refuses every event, poll, ack, and annotation on its
/// terminal status, which is what makes its keys droppable.
fn is_live(state: &StoreState, instance_id: &str) -> bool {
    state
        .instances
        .get(instance_id)
        .is_some_and(|instance| instance.status == Status::Running)
}

/// The record that claimed a key, by its sequence.
///
/// Records are in sequence order, so this is a binary search rather than a
/// scan; a seal over a long journal asks it once per entry.
fn claiming_record(records: &[Record], seq: u64) -> Option<&Record> {
    records
        .binary_search_by(|record| record.seq.cmp(&seq))
        .ok()
        .map(|index| &records[index])
}

/// Partition the ledger and refuse if the carried set does not fit.
///
/// `state_at_cut` is the folded state at the cut, whose `last_seq` **is** the
/// cut — taking it from the state rather than as a second argument is what
/// stops a caller passing a pair that disagrees. `records_through_cut` holds
/// every record at or below it, in sequence order.
///
/// Takes no lock, opens no store, and writes nothing, which is what lets
/// `--dry-run` ask this question from a monitoring session.
pub fn carry_at_cut(
    state_at_cut: &StoreState,
    records_through_cut: &[Record],
    index: &base::BaseIndex,
    definition_limits: DefinitionLimits,
) -> Result<CarryDecision, ErrorObj> {
    let cut = state_at_cut.last_seq;
    let mut carried = BTreeMap::new();
    let mut dropped = BTreeSet::new();
    for (request_id, slot) in &state_at_cut.dedup {
        if slot.seq > cut {
            carried.insert(request_id.clone(), slot.clone());
            continue;
        }
        let Some(record) = claiming_record(records_through_cut, slot.seq) else {
            // The caller handed an incomplete record set. Carrying is the safe
            // direction — too much carried risks only the size limit, while too
            // much dropped risks a request applied twice — so carry it and let
            // the ceiling decide.
            carried.insert(request_id.clone(), slot.clone());
            continue;
        };
        // `instances_touched` and never a `body.get("instance_id")` probe: the
        // composition records name their instances `parent_instance_id` and
        // `child_instance_id`, and a probe would judge an invoked child's keys
        // unattached and drop them, silently.
        let touches_live = instances_touched(record)
            .into_iter()
            .any(|instance_id| is_live(state_at_cut, instance_id));
        if touches_live {
            carried.insert(request_id.clone(), slot.clone());
        } else {
            dropped.insert(request_id.clone());
        }
    }

    let decision = CarryDecision { carried, dropped };
    let mut trial = state_at_cut.clone();
    trial.dedup = decision.carried.clone();
    // The index is part of the file whose size this bounds, and it grows with
    // the instances the base carries — so it is measured, not assumed small.
    let bytes = canon_bytes(&base::encode(&trial, index, definition_limits)).len();
    if bytes > crate::PERSISTENCE_READ_CAP {
        return Err(refused(&format!(
            "sealing at seq {cut} would carry {} idempotency keys into a base state file of \
             {bytes} bytes, and the ceiling is {} bytes",
            decision.carried_count(),
            crate::PERSISTENCE_READ_CAP
        )));
    }
    Ok(decision)
}
