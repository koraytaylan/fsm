//! Startup ownership retained independently of pending effects and writer access.

use super::{
    Pipeline, SettleOutcome,
    native_client::{NativeExecution, NativeRunPhase},
};
use crate::{error::ExecError, sched::Scheduler, watch::Observation};
use fsm_core::record::execution::{Claim, Stopped};
use fsm_store::clock::Clock;
use fsm_store::store::Store;
use std::{
    collections::BTreeMap,
    os::unix::fs::MetadataExt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const MAX_OWNERS: usize = 4096;
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(3);

struct Owner {
    claim: Claim,
    locally_admitted: bool,
    stopped: Option<Stopped>,
    execution: NativeExecution,
    requested: bool,
    entry_requested: bool,
    parked_at: Option<u64>,
}

#[derive(Default)]
pub(super) struct NativeOwners {
    admission_closed: Arc<AtomicBool>,
    admissions: super::native_admission::NativeAdmissions,
    handoffs: super::native_handoffs::NativeHandoffs,
    prefer_handoff: bool,
    physical_store: Option<(u64, u64)>,
    owners: BTreeMap<u64, Owner>,
    observed_seq: u64,
    cursor: u64,
    execution_diagnostic: Option<String>,
}

impl NativeOwners {
    pub(super) fn take_cleanup_diagnostic(&mut self) -> Option<String> {
        self.admissions
            .take_cleanup_diagnostic()
            .or_else(|| self.execution_diagnostic.take())
    }
    pub(super) fn preparation_inventory(&self) -> [usize; 10] {
        self.admissions.phase_counts()
    }

    pub(super) fn shutdown_inventory(&self) -> (Vec<u64>, usize, bool) {
        let helpers_retired = self.admissions.helpers_retired()
            && self.owners.values().all(|owner| {
                owner
                    .execution
                    .progress()
                    .helper
                    .is_none_or(|helper| helper.is_retired())
            });
        (
            self.local_claims().map(Claim::run_id).collect(),
            self.admissions.len(),
            helpers_retired,
        )
    }

    pub(super) fn has_completion(&self, claim: &Claim) -> bool {
        self.owners
            .get(&claim.run_id())
            .is_some_and(|owner| &owner.claim == claim && owner.execution.completion().is_some())
    }

    pub(super) fn helper_retired(&self, claim: &Claim) -> bool {
        self.owners.get(&claim.run_id()).is_some_and(|owner| {
            &owner.claim == claim
                && owner
                    .execution
                    .progress()
                    .helper
                    .is_none_or(|helper| helper.is_retired())
        })
    }

    pub(super) fn cancel_foreign_helpers(&mut self) {
        for owner in self
            .owners
            .values_mut()
            .filter(|owner| !owner.locally_admitted)
        {
            owner.requested = true;
            let _ = owner.execution.cancel();
        }
    }

    pub(super) fn admission_control(&self) -> super::NativeAdmissionControl {
        super::NativeAdmissionControl::from_state(self.admission_closed.clone())
    }

    pub(super) fn admission_is_closed(&self) -> bool {
        self.admission_closed.load(Ordering::Acquire)
    }

    pub(super) fn local_claims(&self) -> impl Iterator<Item = &Claim> {
        self.owners
            .values()
            .filter(|owner| owner.locally_admitted && owner.execution.progress().retained)
            .map(|owner| &owner.claim)
    }

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
                && self.handoffs.is_empty()
                && snapshot
                    .state
                    .execution_handoffs
                    .outstanding()
                    .next()
                    .is_none()
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
        if let Err(error) = self
            .handoffs
            .adopt(snapshot, &self.owners.keys().copied().collect())
        {
            // Keep existing work actionable under the original writer: a full
            // retained set may need exact retirement before adopting more.
            observation.unresolved.push(error);
        }
        for (claim, stopped) in &observation.execution_owners {
            self.retain(claim, stopped.as_ref())?;
        }
        // Transfer genuine local publication from the verified snapshot even
        // when an observation omitted it, before refresh removes its reservation.
        for claim in self.admissions.local_publications(snapshot) {
            self.retain(
                &claim,
                snapshot
                    .state
                    .execution
                    .stopped_for(claim.effect().0, claim.effect().1),
            )?;
            self.owners
                .get_mut(&claim.run_id())
                .ok_or_else(deferred)?
                .locally_admitted = true;
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
        let busy = self.owners.values().any(|owner| {
            owner
                .execution
                .progress()
                .helper
                .is_some_and(|helper| !helper.is_retired())
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
                locally_admitted: false,
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
        // A publication authorized before closure must still retain and bind
        // its original claim; the closed fence separately forbids handler entry.
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
        owner.locally_admitted = true;
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
        if !owner.locally_admitted {
            return Some(Err(deferred()));
        }
        owner.entry_requested = true;
        Some(owner.execution.cancel().map_err(|error| {
            ExecError::new("exec/inflight_deferred", error)
                .hint("retain original native ownership until authenticated reconciliation")
        }))
    }

    pub(super) fn ready(&self) -> bool {
        if self.admission_is_closed() {
            return self.handoffs.ready()
                || self
                    .owners
                    .values()
                    .any(|owner| owner.settlement_ready(self.observed_seq));
        }
        self.admissions.ready()
            || self.handoffs.ready()
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
        if self.admission_is_closed() {
            return self.apply_completed(store, clock, pipeline, scheduler);
        }
        let ownership_ready = self.admissions.ready()
            || self
                .owners
                .values()
                .any(|owner| owner.ready(store.journal.last_seq));
        if self.handoffs.ready() && (self.prefer_handoff || !ownership_ready) {
            if !self.matches_store(store) {
                return Some(Err(deferred()));
            }
            self.prefer_handoff = false;
            return self.handoffs.apply(store, clock, pipeline);
        }
        self.prefer_handoff = true;
        if self.admissions.ready()
            && !self
                .owners
                .values()
                .any(|owner| owner.ready(store.journal.last_seq))
        {
            if !self.matches_store(store) || self.owners.len() >= MAX_OWNERS {
                return Some(Err(deferred()));
            }
            if self.admission_is_closed() {
                return self.apply_completed(store, clock, pipeline, scheduler);
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
            let owner = self.owners.get_mut(&claim.run_id()).ok_or_else(deferred);
            match owner {
                Ok(owner) if owner.claim == claim => owner.locally_admitted = true,
                _ => return Some(Err(deferred())),
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
        let result = owner.apply(store, clock, pipeline, &self.admission_closed);
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

    pub(super) fn apply_completed(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
        scheduler: &mut Scheduler,
    ) -> Option<Result<String, ExecError>> {
        let completed = self
            .owners
            .values()
            .any(|owner| owner.settlement_ready(store.journal.last_seq));
        if self.handoffs.ready() && (self.prefer_handoff || !completed) {
            if !self.matches_store(store) {
                return Some(Err(deferred()));
            }
            self.prefer_handoff = false;
            return self.handoffs.apply(store, clock, pipeline);
        }
        let ready: Vec<u64> = self
            .owners
            .iter()
            .filter(|(_, owner)| owner.settlement_ready(store.journal.last_seq))
            .map(|(run, _)| *run)
            .collect();
        let selected = ready
            .iter()
            .find(|run| **run > self.cursor)
            .or_else(|| ready.first())?;
        if !self.matches_store(store) {
            return Some(Err(deferred()));
        }
        self.prefer_handoff = true;
        self.cursor = *selected;
        let owner = self.owners.get_mut(selected)?;
        let result = owner.apply_completion(store, clock, pipeline);
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

    pub(super) fn retire_interrupted(
        &mut self,
        store: &mut Store,
        claim: &Claim,
        shutdown: &mut super::native_client::NativeShutdown,
        scheduler: &mut Scheduler,
    ) -> Result<bool, ExecError> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "interruption retirement requires the original healthy durable writer",
            ));
        }
        if !self.matches_store(store) || !shutdown.matches_original(claim) {
            return Err(deferred());
        }
        let owner = self.owners.get_mut(&claim.run_id()).ok_or_else(deferred)?;
        if &owner.claim != claim || !owner.locally_admitted {
            return Err(deferred());
        }
        let proof = shutdown
            .poll()
            .map_err(|_| deferred())?
            .ok_or_else(deferred)?;
        proof.check_store(&store.data_dir).map_err(|_| deferred())?;
        if !shutdown.reap().map_err(|_| deferred())? {
            return Ok(false);
        }
        let helper = shutdown.progress();
        if !helper.is_retired() {
            return Ok(false);
        }
        if !owner.execution.retire_interrupted(store)? {
            return Ok(false);
        }
        scheduler.complete_claim(&owner.claim);
        self.owners.remove(&claim.run_id());
        Ok(true)
    }

    pub(super) fn observe(&mut self) {
        if self.admission_is_closed() {
            self.admissions.close_admission();
        }
        self.admissions.observe();
        for owner in self.owners.values_mut() {
            if owner.execution.progress().phase == NativeRunPhase::Uncertain {
                let _ = owner.execution.reap();
            } else if let Err(error) = owner.execution.observe() {
                self.execution_diagnostic.get_or_insert_with(|| {
                    super::native_admission::bounded_diagnostic(
                        "native-execution-uncertain ",
                        &error.message,
                    )
                });
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
        if self.admission_is_closed()
            || !self.matches_store(snapshot)
            || self.owners.len() + self.admissions.len() >= MAX_OWNERS
            || self
                .owners
                .values()
                .any(|owner| owner.claim.effect().1 == effect.effect_id)
        {
            return Err(deferred());
        }
        self.admissions
            .queue(snapshot, effect, handler, scheduler, &self.admission_closed)
    }

    pub(super) fn release_preparations(&mut self, scheduler: &mut Scheduler) {
        self.admissions.release_closed(scheduler);
    }

    pub(super) fn start_preparations(&mut self) {
        if !self.admission_is_closed() {
            self.admissions.start_queued(&self.admission_closed);
        }
    }
}

impl Owner {
    fn ready(&self, seq: u64) -> bool {
        self.entry_ready() || self.settlement_ready(seq)
    }

    fn entry_ready(&self) -> bool {
        !self.entry_requested && self.execution.progress().phase == NativeRunPhase::Bound
    }

    fn settlement_ready(&self, seq: u64) -> bool {
        self.execution.completion().is_some() && self.parked_at != Some(seq)
    }

    fn apply(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        pipeline: &mut Pipeline,
        admission_closed: &AtomicBool,
    ) -> Result<(String, bool), ExecError> {
        if self.entry_ready() {
            // A read-only or stale writer refusal leaves the bound owner intact;
            // only a validated entry attempt consumes its one-shot permission.
            let entry_requested = &mut self.entry_requested;
            if !self.execution.launch_bound_checked(store, || {
                if admission_closed.load(Ordering::Acquire) {
                    return false;
                }
                *entry_requested = true;
                true
            })? {
                return Err(deferred());
            }
            return Ok((
                format!(
                    "native-launched {} run_id={}",
                    self.claim.effect().1,
                    self.claim.run_id()
                ),
                false,
            ));
        }
        self.apply_completion(store, clock, pipeline)
    }

    // Completion application cannot launch a bound owner or an admission.
    fn apply_completion(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_target_iteration_filters_foreign_metadata_and_orders_original_runs() {
        // Classifier fixtures supply no allocation, launch or closure authority.
        let fixture = fsm_core::json::parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        let mut owners = NativeOwners::default();
        for (run, local) in [(9, true), (5, false), (2, true)] {
            let fsm_core::json::Value::Obj(mut value) = fixture.get("claim").unwrap().clone()
            else {
                unreachable!()
            };
            value.insert("run_id".into(), fsm_core::json::Value::Num(run.to_string()));
            let claim = Claim::from_value(&fsm_core::json::Value::Obj(value)).unwrap();
            owners.retain(&claim, None).unwrap();
            owners.owners.get_mut(&run).unwrap().locally_admitted = local;
        }
        assert_eq!(
            owners.local_claims().map(Claim::run_id).collect::<Vec<_>>(),
            vec![2, 9]
        );
        assert_eq!(owners.owners.len(), 3);
        assert!(!owners.owners[&5].locally_admitted);
        assert!(
            owners
                .owners
                .values()
                .all(|owner| owner.execution.progress().retained)
        );
    }

    #[test]
    fn runner_cancellation_refuses_observed_foreign_ownership() {
        // An observed metadata fixture supplies no native closure authority.
        let fixture = fsm_core::json::parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        let claim = Claim::from_value(fixture.get("claim").unwrap()).unwrap();
        let mut runner = super::super::Runner::new().unwrap();
        runner.native.retain(&claim, None).unwrap();
        let error = runner.cancel_native(claim.effect().1).unwrap().unwrap_err();
        assert_eq!(error.code, "exec/inflight_deferred");
        assert_eq!(
            error.message,
            "original native ownership requires reconciliation"
        );
        let owner = runner.native.owners.get(&claim.run_id()).unwrap();
        assert_eq!(owner.claim, claim);
        assert!(!owner.locally_admitted);
        assert!(!owner.requested);
        assert!(owner.execution.progress().retained);
        // Explicitly local retention still permits helper cancellation without
        // claiming protected domain closure or releasing the durable identity.
        runner
            .native
            .owners
            .get_mut(&claim.run_id())
            .unwrap()
            .locally_admitted = true;
        runner.cancel_native(claim.effect().1).unwrap().unwrap();
        assert!(
            runner.native.owners[&claim.run_id()]
                .execution
                .progress()
                .retained
        );
    }
}
