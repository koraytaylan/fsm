//! Opening a store: a snapshot is used only after its journal
//! prefix binds or reproduces it.
use crate::store::ErrorObj;
use fsm_core::hashes::state_hash;
use fsm_core::json::{JsonLimits, parse};
use fsm_core::replay::{StoreState, fold_from};
use std::path::Path;

use super::decode::*;

pub fn store_states_eq(a: &StoreState, b: &StoreState) -> bool {
    if a.last_seq != b.last_seq || a.last_hash != b.last_hash {
        return false;
    }
    if a.dedup != b.dedup {
        return false;
    }
    if a.instance_machines != b.instance_machines {
        return false;
    }
    if a.machines.len() != b.machines.len() {
        return false;
    }
    for (id, ma) in &a.machines {
        let Some(mb) = b.machines.get(id) else {
            return false;
        };
        if ma.compiled.machine_id != mb.compiled.machine_id || ma.def != mb.def {
            return false;
        }
    }
    if a.instances.len() != b.instances.len() {
        return false;
    }
    for (id, ia) in &a.instances {
        let Some(ib) = b.instances.get(id) else {
            return false;
        };
        if ia.configuration != ib.configuration
            || ia.status != ib.status
            || ia.ctx != ib.ctx
            || ia.history != ib.history
            || ia.deadlines != ib.deadlines
            || ia.pending != ib.pending
        {
            return false;
        }
        let mid = a.instance_machines.get(id).cloned().unwrap_or_default();
        if state_hash(&mid, id, a.last_seq, ia) != state_hash(&mid, id, b.last_seq, ib) {
            return false;
        }
    }
    true
}

pub(super) fn snapshot_matches_prefix(
    base: &StoreState,
    recs: &[fsm_core::record::Record],
) -> bool {
    let prefix = recs
        .iter()
        .filter(|record| record.seq <= base.last_seq)
        .cloned();
    let Ok(folded) = fsm_core::replay::fold_with(prefix, &mut fsm_core::replay::NopSink) else {
        return false;
    };
    store_states_eq(base, &folded)
}

#[derive(Debug, Clone, Default)]
pub struct OpenPath {
    pub replayed_records: usize,
    pub used_snapshot: bool,
    pub snapshot_seq: Option<u64>,
}

/// Reconstruct an untrusted snapshot-cache view plus its journal tail.
///
/// This exists only for diagnostics that immediately compare the result with
/// a complete journal fold. It deliberately preserves a self-consistent but
/// divergent cache so `journal replay` can report the first disagreement.
/// Callers MUST NOT use the returned state operationally; [`open::open_state`](super::open::open_state) is
/// the authenticated store-open path.
pub fn reconstruct_snapshot_plus_tail(
    data_dir: &Path,
    recs: &[fsm_core::record::Record],
    to_seq: u64,
) -> Result<StoreState, ErrorObj> {
    let journal_last = recs.last().map(|r| r.seq).unwrap_or(0);
    let want = to_seq.min(journal_last);
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok((base, _definition_limits)) = snapshot_to_state_for_journal(&v, recs) else {
            continue;
        };
        if base.last_seq > want {
            continue;
        }
        let Some(rec) = recs.iter().find(|r| r.seq == base.last_seq) else {
            continue;
        };
        if rec.hash != base.last_hash {
            continue;
        }
        let tail: Vec<_> = recs
            .iter()
            .filter(|r| r.seq > base.last_seq && r.seq <= want)
            .cloned()
            .collect();
        return fold_from(base, tail, &mut fsm_core::replay::NopSink)
            .map_err(|e| ErrorObj::new("io/read", format!("{e:?}")));
    }
    let prefix: Vec<_> = recs.iter().filter(|r| r.seq <= want).cloned().collect();
    fsm_core::replay::fold_with(prefix, &mut fsm_core::replay::NopSink)
        .map_err(|e| ErrorObj::new("io/read", format!("{e:?}")))
}

pub(super) fn open_state_impl(
    data_dir: &Path,
    recs: Vec<fsm_core::record::Record>,
    sink: &mut impl fsm_core::replay::RecordSink,
    may_prune: bool,
) -> Result<(StoreState, OpenPath), fsm_core::replay::ReplayError> {
    // Earlier builds emitted one mutable root file per commit. They are never
    // trust anchors and can be removed opportunistically.
    if may_prune {
        let _ = prune_legacy_root_sidecars(data_dir);
    }
    let journal_last = recs.last().map(|r| r.seq).unwrap_or(0);
    // First pass: prefer a hash-chain-bound snapshot even when a newer
    // clean-shutdown cache exists without a committed root.
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok((base, definition_limits)) = snapshot_to_state_for_journal(&v, &recs) else {
            continue;
        };
        if base.last_seq > journal_last {
            continue;
        }
        let Some(rec) = recs.iter().find(|r| r.seq == base.last_seq) else {
            continue;
        };
        if rec.hash != base.last_hash {
            continue;
        }
        let bound = snapshot_bound(&base, rec, &recs, definition_limits);
        if !bound {
            continue;
        }
        let snap_seq = base.last_seq;
        let tail: Vec<_> = recs
            .iter()
            .filter(|r| r.seq > base.last_seq)
            .cloned()
            .collect();
        let replayed = tail.len();
        let state = fold_from(base, tail, sink)?;
        return Ok((
            state,
            OpenPath {
                replayed_records: replayed,
                used_snapshot: true,
                snapshot_seq: Some(snap_seq),
            },
        ));
    }
    // An unbound snapshot is still a useful disposable cache representation,
    // but it cannot be trusted. Re-fold and compare its complete prefix before
    // using it; this is a correctness fallback, not the fast path.
    for (_seq, path) in listed_snaps(data_dir) {
        let Ok(bytes) = crate::read_regular_file_capped(&path, crate::PERSISTENCE_READ_CAP) else {
            continue;
        };
        let Ok(v) = parse(&bytes, &JsonLimits::DEFAULT) else {
            continue;
        };
        let Ok((base, _definition_limits)) = snapshot_to_state_for_journal(&v, &recs) else {
            continue;
        };
        if base.last_seq > journal_last {
            continue;
        }
        let Some(rec) = recs.iter().find(|r| r.seq == base.last_seq) else {
            continue;
        };
        if rec.hash != base.last_hash || !snapshot_matches_prefix(&base, &recs) {
            continue;
        }
        let snap_seq = base.last_seq;
        let tail: Vec<_> = recs
            .iter()
            .filter(|r| r.seq > base.last_seq)
            .cloned()
            .collect();
        let state = fold_from(base, tail, sink)?;
        return Ok((
            state,
            OpenPath {
                replayed_records: recs.len(),
                used_snapshot: true,
                snapshot_seq: Some(snap_seq),
            },
        ));
    }
    let n = recs.len();
    let state = fsm_core::replay::fold_with(recs, sink)?;
    Ok((
        state,
        OpenPath {
            replayed_records: n,
            used_snapshot: false,
            snapshot_seq: None,
        },
    ))
}

/// Fold a verified journal, using a snapshot only after binding or reproducing
/// its complete journal prefix.
///
/// This writer-side path may prune obsolete cache metadata. Inspection code
/// uses the crate-private non-mutating counterpart.
pub fn open_state(
    data_dir: &Path,
    recs: Vec<fsm_core::record::Record>,
    sink: &mut impl fsm_core::replay::RecordSink,
) -> Result<(StoreState, OpenPath), fsm_core::replay::ReplayError> {
    open_state_impl(data_dir, recs, sink, true)
}

pub(crate) fn open_state_read_only(
    data_dir: &Path,
    recs: Vec<fsm_core::record::Record>,
    sink: &mut impl fsm_core::replay::RecordSink,
) -> Result<(StoreState, OpenPath), fsm_core::replay::ReplayError> {
    open_state_impl(data_dir, recs, sink, false)
}
