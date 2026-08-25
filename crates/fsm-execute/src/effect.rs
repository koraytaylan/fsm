//! Re-derivation of a pending effect's name and args from the journal.
//!
//! The store surfaces a pending effect as an opaque `{instance}/{seq}/{k}` id
//! and nothing else — no record body and no view carries the emitted name or
//! the evaluated args. A human host reads the machine to know what
//! `order-1/3/0` means; a mechanical executor replays the one record that
//! emitted it.

use std::collections::BTreeMap;

use fsm_core::expr::eval::Val;
use fsm_store::store::Store;

use crate::error::ExecError;

/// One pending effect, resolved back to what the machine actually emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingEffect {
    /// The instance that emitted it.
    pub instance_id: String,
    /// The opaque `{instance}/{seq}/{k}` id the store hands out.
    pub effect_id: String,
    /// The declared effect name, re-derived by replay.
    pub effect_name: String,
    /// The evaluated arguments, re-derived by the same replay.
    pub args: BTreeMap<String, Val>,
    /// The `seq` component of the id, which is `0` for a creation-time emit.
    pub emitted_seq: u64,
    /// The emit's own index within its transition.
    pub k: u32,
}

/// Resolve one pending effect id against a store opened for reading.
pub fn resolve(store: &Store, effect_id: &str) -> Result<PendingEffect, ExecError> {
    let _ = (store, effect_id);
    unimplemented!("task 3701")
}
