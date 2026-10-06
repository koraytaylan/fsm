//! Startup ownership retained independently of pending effects and writer access.

use super::native_client::{NativeExecution, NativeRunPhase};
use crate::{error::ExecError, watch::Observation};
use fsm_core::record::execution::{Claim, Stopped};
use fsm_store::store::Store;
use std::{collections::BTreeMap, os::unix::fs::MetadataExt, time::Duration};

const MAX_OWNERS: usize = 4096;
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(3);

struct Owner {
    claim: Claim,
    stopped: Option<Stopped>,
    execution: NativeExecution,
    requested: bool,
}

#[derive(Default)]
pub(super) struct NativeOwners {
    physical_store: Option<(u64, u64)>,
    owners: BTreeMap<u64, Owner>,
}

impl NativeOwners {
    pub(super) fn adopt(
        &mut self,
        snapshot: &Store,
        observation: &mut Observation,
    ) -> Result<(), ExecError> {
        if self.owners.is_empty() && observation.execution_owners.is_empty() {
            return Ok(());
        }
        if snapshot.journal.is_memory() || snapshot.journal.poisoned {
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
        for owner in self.owners.values() {
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
            },
        );
        Ok(())
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

fn deferred() -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        "original native ownership requires reconciliation",
    )
}
