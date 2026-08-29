//! Machine and instance store over the journal.

#![allow(clippy::collapsible_if, unused_imports)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use fsm_core::analyze::{EventStatus, enabled_events};
use fsm_core::canon::canon_bytes;
use fsm_core::error::{FsmError, retryable};
use fsm_core::expr::eval::{Budget, Val};
use fsm_core::hashes::{
    ResolveError, STATE_FORMAT, configuration_value, machine_id, resolve_machine_ref, state_hash,
};
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::machine::{ActiveConfiguration, InstanceState, Status};
use fsm_core::record::{Record, RecordKind};
use fsm_core::replay::{
    NopSink, RecordSink, STATE_ROOT_FORMAT, StoreState, StoredMachine, ctx_val_json, fold_with,
};
use fsm_core::spec::{Finding, MachineSpec, TySpec};
use fsm_core::step::{
    DeadlineOutcome, Outcome, Rejection, create, poll_deadline, step, validate_event,
};
use fsm_core::tree::Tree;

use crate::journal_io::{self, Journal, JournalHealth, JournalIoError, OpenError};

mod commit;
mod error;
mod idempotency;
mod instance;
mod json_helpers;
mod lifecycle;
mod reconstruct;
mod seal;
mod snapshot_lifecycle;
#[cfg(test)]
mod tests;
mod view;

pub use error::ErrorObj;
pub use json_helpers::{
    apply_context_overrides, coerce_ctx_override, context_not_object, enabled_json,
    number_token_error,
};
pub use lifecycle::DefineOutcome;
pub use seal::{BASE_FILE, SealReport};
pub use view::views_rendered;

use reconstruct::{
    health_err, insert_configuration_fields, insert_transition_configuration_fields,
    load_tags_from_records, pending_deadlines_value, reconstruct_applied,
    reconstruct_deadline_applied, reconstruct_ignored, view_at,
};

pub struct Store {
    pub journal: Journal,
    pub state: StoreState,
    pub history: BTreeMap<String, Vec<u64>>,
    /// Child instance id to the parent and slot that invoked it.
    ///
    /// A store-side index like `history`, rebuilt from the journal on open
    /// and extended on write. It is not part of the hashed state and not in
    /// the snapshot: a child id derives from its parent and slot, but the
    /// derivation is a hash and does not invert, so the edge has to be
    /// remembered somewhere — and remembering it here costs no format.
    pub parents: BTreeMap<String, (String, String)>,
    /// Machine id to the seq of the record that first defined it.
    ///
    /// Another derived index, for the same reason as `parents`: without it,
    /// ordering the catalogue by age costs a journal scan per machine, which
    /// a resource listing pays on every call and a completion would pay on
    /// every keystroke.
    pub machine_seqs: BTreeMap<String, u64>,
    pub records: Vec<Record>,
    pub data_dir: PathBuf,
    pub last_responses: BTreeMap<String, Value>,
    pub last_errors: BTreeMap<String, ErrorObj>,
    pub tags: BTreeMap<String, Vec<String>>,
    pub replayed_records: usize,
    pub opened_from_snapshot: bool,
    pub opened_snapshot_seq: Option<u64>,
    /// Fingerprint of the request currently being committed. Set by
    /// `claim_request`, stamped into every record that request appends, and
    /// cleared on commit. Keeping it here rather than threading it through
    /// each body-building site means a new operation cannot forget it.
    pending_fp: Option<String>,
}

struct HistSink {
    history: BTreeMap<String, Vec<u64>>,
    records: Vec<Record>,
}

impl RecordSink for HistSink {
    fn on_record(&mut self, record: &Record, _state: &StoreState) {
        self.records.push(record.clone());
        for iid in fsm_core::record::instances_touched(record) {
            self.history.entry(iid.into()).or_default().push(record.seq);
        }
    }
}

impl Drop for Store {
    fn drop(&mut self) {
        if !self.journal.is_memory() && !self.journal.is_read_only() && self.journal.last_seq > 0 {
            // Drop must never append: there is no caller-supplied clock and a
            // read-only open/close must leave the authoritative journal alone.
            let _ = crate::snapshot::write_snapshot(&self.data_dir, &self.state);
        }
    }
}
