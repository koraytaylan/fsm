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
    entry_requested: bool,
    parked_at: Option<u64>,
}

#[derive(Default)]
pub(super) struct NativeOwners {
    admissions: super::native_admission::NativeAdmissions,
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
        scheduler: &mut Scheduler,
    ) -> Result<(), ExecError> {
        self.observed_seq = snapshot.journal.last_seq;
        // Pin the durable route even before the first claim; otherwise a
        // queued preparation could adopt a replacement physical store.
        if snapshot.journal.is_memory() {
            return if self.owners.is_empty()
                && self.admissions.is_empty()
                && observation.execution_owners.is_empty()
            {
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
        self.admissions.refresh(snapshot, scheduler);
        if self.admissions.uncertain() {
            observation.unresolved.push(deferred());
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
                entry_requested: true,
                parked_at: None,
            },
        );
        Ok(())
    }

    pub(super) fn install(
        &mut self,
        store: &mut Store,
        claim: &Claim,
        scheduler: &mut Scheduler,
        timeout: Duration,
    ) -> Result<(), ExecError> {
        // Only a genuine eligible current durable claim may enter this map.
        Pipeline::native_launch_hash(store, claim)?;
        super::native_client::check_claim_store(&store.data_dir, claim).map_err(|_| deferred())?;
        let metadata = std::fs::metadata(&store.data_dir).map_err(|_| deferred())?;
        let physical = (metadata.dev(), metadata.ino());
        if self
            .physical_store
            .is_some_and(|original| original != physical)
            || self.owners.contains_key(&claim.run_id())
        {
            return Err(deferred());
        }
        self.physical_store = Some(physical);
        self.retain(claim, None)?;
        let owner = self.owners.get_mut(&claim.run_id()).ok_or_else(deferred)?;
        // Install retention and disable automatic replacement recovery before
        // any fallible binding transport startup or scheduler binding check.
        owner.requested = true;
        owner.entry_requested = false;
        if !scheduler.retain_claim(claim) {
            return Err(deferred());
        }
        owner.execution.start_retained(store, timeout)
    }

    pub(super) fn cancel(&mut self, effect: &str) -> Option<Result<(), ExecError>> {
        if let Some(result) = self.admissions.cancel(effect) {
            return Some(result);
        }
        let owner = self.owners.values_mut().find(|owner| {
            owner.claim.effect().1 == effect && owner.execution.progress().retained
        })?;
        owner.entry_requested = true;
        Some(owner.execution.cancel().map_err(|error| {
            ExecError::new("exec/inflight_deferred", error)
                .hint("retain original native ownership until authenticated reconciliation")
        }))
    }

    pub(super) fn ready(&self) -> bool {
        self.admissions.ready()
            || self
                .owners
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
        if self.admissions.ready()
            && !self
                .owners
                .values()
                .any(|owner| owner.ready(store.journal.last_seq))
        {
            if !self.matches_store(store) || self.owners.len() >= MAX_OWNERS {
                return Some(Err(deferred()));
            }
            let request = match self.admissions.take_ready(store)? {
                Ok(request) => request,
                Err(error) => return Some(Err(error)),
            };
            let claim = match pipeline.claim_native_handler(
                store,
                clock,
                &request.effect.effect_id,
                &request.handler,
                &request.domain,
                &request.request_id,
            ) {
                Ok(claim) => claim,
                Err(error) => return Some(Err(error)),
            };
            let timeout = Duration::from_millis(request.handler.timeout_ms.max(0) as u64)
                .checked_add(Duration::from_secs(30))
                .unwrap_or(Duration::MAX);
            let result = self.install(store, &claim, scheduler, timeout);
            // Publication has succeeded: retain the exact claim even if a
            // subsequent physical route or helper startup check refuses it.
            if !self.owners.contains_key(&claim.run_id()) {
                if let Err(error) = self.retain(&claim, None) {
                    return Some(Err(error));
                }
                if let Some(owner) = self.owners.get_mut(&claim.run_id()) {
                    owner.requested = true;
                }
                scheduler.retain_claim(&claim);
            }
            self.admissions.transferred(&claim);
            return Some(result.map(|()| {
                format!(
                    "native-claimed {} run_id={}",
                    request.effect.effect_id,
                    claim.run_id()
                )
            }));
        }
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
        // A copied journal has the same logical claim, but its writer cannot
        // authorize entry or settlement for this host's original physical store.
        if !self.matches_store(store) {
            return Some(Err(deferred()));
        }
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
        self.admissions.observe();
        for owner in self.owners.values_mut() {
            if owner.execution.progress().phase == NativeRunPhase::Uncertain {
                let _ = owner.execution.reap();
            } else {
                let _ = owner.execution.observe();
            }
        }
    }

    pub(super) fn matches_store(&self, store: &Store) -> bool {
        self.physical_store.is_some_and(|original| {
            std::fs::metadata(&store.data_dir)
                .is_ok_and(|metadata| original == (metadata.dev(), metadata.ino()))
        })
    }

    pub(super) fn queue(
        &mut self,
        snapshot: &Store,
        effect: &crate::effect::PendingEffect,
        handler: &crate::config::HandlerSpec,
        scheduler: &mut Scheduler,
    ) -> Result<(), ExecError> {
        if !self.matches_store(snapshot)
            || self.owners.len() + self.admissions.len() >= MAX_OWNERS
            || self
                .owners
                .values()
                .any(|owner| owner.claim.effect().1 == effect.effect_id)
        {
            return Err(deferred());
        }
        self.admissions.queue(snapshot, effect, handler, scheduler)
    }

    pub(super) fn release_preparations(&mut self, scheduler: &mut Scheduler) {
        self.admissions.release_closed(scheduler);
    }

    pub(super) fn start_preparations(&mut self) {
        self.admissions.start_queued();
    }
}

impl Owner {
    fn ready(&self, seq: u64) -> bool {
        (!self.entry_requested && self.execution.progress().phase == NativeRunPhase::Bound)
            || (self.execution.completion().is_some() && self.parked_at != Some(seq))
    }

    fn apply(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
    ) -> Result<(String, bool), ExecError> {
        if !self.entry_requested && self.execution.progress().phase == NativeRunPhase::Bound {
            // A read-only or stale writer refusal leaves the bound owner intact;
            // only a validated entry attempt consumes its one-shot permission.
            Pipeline::native_launch_hash(store, &self.claim)?;
            self.entry_requested = true;
            self.execution.launch_bound(store)?;
            return Ok((
                format!(
                    "native-launched {} run_id={}",
                    self.claim.effect().1,
                    self.claim.run_id()
                ),
                false,
            ));
        }
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
