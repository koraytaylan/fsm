//! Startup ownership retained independently of pending effects and writer access.

use super::{
    Pipeline, SettleOutcome,
    native_client::{NativeExecution, NativeRunPhase},
};
use crate::{error::ExecError, sched::Scheduler, watch::Observation};
use fsm_core::record::execution::{Claim, Stopped};
use fsm_store::clock::Clock;
use fsm_store::store::Store;
use std::{collections::BTreeMap, os::unix::fs::MetadataExt, time::Duration};

const MAX_OWNERS: usize = 4096;
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(3);

struct Owner {
    claim: Claim,
    stopped: Option<Stopped>,
    execution: NativeExecution,
    requested: bool,
    parked_at: Option<u64>,
}

#[derive(Default)]
pub(super) struct NativeOwners {
    physical_store: Option<(u64, u64)>,
    owners: BTreeMap<u64, Owner>,
    observed_seq: u64,
    cursor: u64,
}

impl NativeOwners {
    pub(super) fn adopt(
        &mut self,
        snapshot: &Store,
        observation: &mut Observation,
    ) -> Result<(), ExecError> {
        self.observed_seq = snapshot.journal.last_seq;
        // Pin the durable route even before the first claim; otherwise a
        // queued preparation could adopt a replacement physical store.
        if snapshot.journal.is_memory() {
            return if self.owners.is_empty() && observation.execution_owners.is_empty() {
                Ok(())
            } else {
                Err(deferred())
            };
        }
        if snapshot.journal.poisoned {
            return Err(deferred());
        }
        let metadata = std::fs::metadata(&snapshot.data_dir).map_err(|_| deferred())?;
        let physical = (metadata.dev(), metadata.ino());
        if self
            .physical_store
            .is_some_and(|original| original != physical)
        {
            return Err(deferred());
        }
        self.physical_store = Some(physical);
        for (claim, stopped) in &observation.execution_owners {
            self.retain(claim, stopped.as_ref())?;
        }
        // Missing journal observation cannot retire a locally held identity.
        // Exact settlement reconciliation will explicitly consume this state.
        for owner in self
            .owners
            .values()
            .filter(|owner| owner.execution.progress().retained)
        {
            if !observation
                .execution_owners
                .iter()
                .any(|(claim, _)| claim == &owner.claim)
            {
                observation
                    .execution_owners
                    .push((owner.claim.clone(), owner.stopped.clone()));
            }
        }
        // One owned recovery transport at a time, independent of owner count.
        // A failed request is never replaced by a bind/execute request.
        let busy =
            self.owners.values().any(|owner| {
                owner.execution.progress().helper.is_some_and(|helper| {
                    !helper.reaped || !helper.stdout_eof || !helper.stderr_eof
                })
            });
        if !busy {
            for owner in self.owners.values_mut().filter(|owner| !owner.requested) {
                let (instance, effect) = owner.claim.effect();
                if snapshot.state.execution.claim_for(instance, effect) != Some(&owner.claim) {
                    continue;
                }
                owner.requested = true;
                if let Ok(execution) =
                    NativeExecution::recover(snapshot, &owner.claim, RECOVERY_TIMEOUT)
                {
                    owner.execution = execution;
                }
                break;
            }
        }
        Ok(())
    }

    fn retain(&mut self, claim: &Claim, stopped: Option<&Stopped>) -> Result<(), ExecError> {
        if let Some(owner) = self.owners.get_mut(&claim.run_id()) {
            if owner.claim != *claim {
                return Err(deferred());
            }
            if let Some(stopped) = stopped {
                if owner
                    .stopped
                    .as_ref()
                    .is_some_and(|original| original != stopped)
                {
                    return Err(deferred());
                }
                owner.stopped = Some(stopped.clone());
            }
            return Ok(());
        }
        if self.owners.len() >= MAX_OWNERS {
            return Err(deferred());
        }
        self.owners.insert(
            claim.run_id(),
            Owner {
                claim: claim.clone(),
                stopped: stopped.cloned(),
                execution: NativeExecution::retain_uncertain(claim),
                requested: false,
                parked_at: None,
            },
        );
        Ok(())
    }

    pub(super) fn ready(&self) -> bool {
        self.owners
            .values()
            .any(|owner| owner.ready(self.observed_seq))
    }

    pub(super) fn apply(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
        scheduler: &mut Scheduler,
    ) -> Option<Result<String, ExecError>> {
        let ready: Vec<u64> = self
            .owners
            .iter()
            .filter(|(_, owner)| owner.ready(store.journal.last_seq))
            .map(|(run, _)| *run)
            .collect();
        let selected = ready
            .iter()
            .find(|run| **run > self.cursor)
            .or_else(|| ready.first())?;
        self.cursor = *selected;
        let owner = self.owners.get_mut(selected)?;
        let result = owner.apply(store, clock, pipeline);
        // Settlement releases durable ownership before optional event delivery;
        // a later event error must not keep the consumed local slot occupied.
        if !owner.execution.progress().retained {
            scheduler.complete_claim(&owner.claim);
        }
        match result {
            Ok((line, finished)) => {
                if finished {
                    self.owners.remove(selected);
                }
                Some(Ok(line))
            }
            Err(error) => Some(Err(error)),
        }
    }

    pub(super) fn observe(&mut self) {
        for owner in self.owners.values_mut() {
            if owner.execution.progress().phase == NativeRunPhase::Uncertain {
                let _ = owner.execution.reap();
            } else {
                let _ = owner.execution.observe();
            }
        }
    }
}

impl Owner {
    fn ready(&self, seq: u64) -> bool {
        self.execution.completion().is_some() && self.parked_at != Some(seq)
    }

    fn apply(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
    ) -> Result<(String, bool), ExecError> {
        let response = self.execution.settle(store, clock)?;
        let disposition = response
            .get("execution")
            .and_then(|body| body.get("disposition"))
            .and_then(fsm_core::json::Value::as_str)
            .ok_or_else(deferred)?;
        let (_, effect) = self.claim.effect();
        let (advance, finished) = match disposition {
            "acked" => {
                let completion = self.execution.completion().ok_or_else(deferred)?;
                let outcome = completion.stopped_outcome().status();
                let handler = completion.handler();
                let declared = if outcome == "ok" {
                    handler.on_ok.is_some()
                } else {
                    handler.on_failed.is_some()
                };
                match pipeline.advance_native_settled(
                    store,
                    clock,
                    &self.claim,
                    completion,
                    &crate::rid::ack_rid(effect),
                )? {
                    SettleOutcome::Advanced => ("advanced", true),
                    SettleOutcome::AlreadySettled => ("already-settled", true),
                    SettleOutcome::AckedNoAdvance if declared => {
                        self.parked_at = Some(store.journal.last_seq);
                        ("deferred", false)
                    }
                    SettleOutcome::AckedNoAdvance => ("none", true),
                }
            }
            "attempted" | "interrupted" => ("none", true),
            _ => return Err(deferred()),
        };
        Ok((
            format!(
                "native-settled {effect} run_id={} disposition={disposition} advance={advance}",
                self.claim.run_id()
            ),
            finished,
        ))
    }
}

fn deferred() -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        "original native ownership requires reconciliation",
    )
}
