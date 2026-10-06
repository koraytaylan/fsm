//! Event-only recovery retained separately from execution ownership and capacity.

use super::{Pipeline, SettleOutcome};
use crate::{config::HandlerSpec, error::ExecError};
use fsm_core::{json::Value, record::execution::AcknowledgedHandoff};
use fsm_store::{clock::Clock, store::Store};
use std::collections::{BTreeMap, BTreeSet};

const MAX_HANDOFFS: usize = 4096;
const MAX_HANDOFF_BYTES: usize = 8 * 1024 * 1024;

struct Entry {
    original: AcknowledgedHandoff,
    parked_at: Option<u64>,
    canonical_bytes: usize,
}

#[derive(Default)]
pub(super) struct NativeHandoffs {
    entries: BTreeMap<u64, Entry>,
    observed_seq: u64,
    cursor: u64,
    retained_bytes: usize,
}

impl NativeHandoffs {
    pub(super) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(super) fn adopt(
        &mut self,
        snapshot: &Store,
        warm_runs: &BTreeSet<u64>,
    ) -> Result<(), ExecError> {
        self.observed_seq = snapshot.journal.last_seq;
        for original in snapshot.state.execution_handoffs.outstanding() {
            let run = original.claim().run_id();
            // A live host already owns its original completion retry path.
            if warm_runs.contains(&run) {
                continue;
            }
            if let Some(entry) = self.entries.get(&run) {
                if entry.original != *original {
                    return Err(deferred());
                }
                continue;
            }
            if self.entries.len() >= MAX_HANDOFFS {
                return Err(deferred());
            }
            let canonical_bytes = fsm_core::canon::canon_bytes(&original.to_value()).len();
            let offered = self
                .retained_bytes
                .checked_add(canonical_bytes + 1)
                .filter(|bytes| *bytes < MAX_HANDOFF_BYTES)
                .ok_or_else(deferred)?;
            let claim = original.claim().to_value();
            let fingerprint = claim
                .get("handler_fingerprint")
                .and_then(Value::as_str)
                .ok_or_else(deferred)?;
            HandlerSpec::from_contract(original.handler_contract(), fingerprint)?;
            self.entries.insert(
                run,
                Entry {
                    original: original.clone(),
                    parked_at: None,
                    canonical_bytes,
                },
            );
            self.retained_bytes = offered;
        }
        // Observation never erases retained obligations; an original healthy
        // writer must reconcile exact membership before local retirement.
        Ok(())
    }

    pub(super) fn ready(&self) -> bool {
        self.entries
            .values()
            .any(|entry| entry.parked_at != Some(self.observed_seq))
    }

    pub(super) fn apply(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
    ) -> Option<Result<String, ExecError>> {
        let selected = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.parked_at != Some(store.journal.last_seq))
            .map(|(run, _)| *run)
            .find(|run| *run > self.cursor)
            .or_else(|| {
                self.entries
                    .iter()
                    .find(|(_, entry)| entry.parked_at != Some(store.journal.last_seq))
                    .map(|(run, _)| *run)
            })?;
        if store.journal.is_memory() || store.journal.is_read_only() || store.journal.poisoned {
            return Some(Err(deferred()));
        }
        self.cursor = selected;
        let entry = self.entries.get_mut(&selected)?;
        let current = store
            .state
            .execution_handoffs
            .outstanding()
            .find(|handoff| handoff.claim().run_id() == selected);
        match current {
            None => {
                if let Some(retired) = self.entries.remove(&selected) {
                    self.retained_bytes -= retired.canonical_bytes + 1;
                }
                return Some(Ok(format!("native-handoff reconciled run_id={selected}")));
            }
            Some(original) if original != &entry.original => {
                entry.parked_at = Some(store.journal.last_seq);
                return Some(Err(deferred()));
            }
            Some(_) => {}
        }
        let result = pipeline.advance_native_handoff(store, clock, &entry.original);
        // Park after the attempt, including any rejected-event append, so the
        // attempt's own journal progress does not immediately trigger itself.
        entry.parked_at = Some(store.journal.last_seq);
        match result {
            Ok(SettleOutcome::Advanced) => {
                if let Some(retired) = self.entries.remove(&selected) {
                    self.retained_bytes -= retired.canonical_bytes + 1;
                }
                Some(Ok(format!("native-handoff advanced run_id={selected}")))
            }
            Ok(_) => Some(Ok(format!("native-handoff parked run_id={selected}"))),
            Err(error) => Some(Err(error)),
        }
    }
}

fn deferred() -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        "original native event handoff remains unresolved",
    )
}
