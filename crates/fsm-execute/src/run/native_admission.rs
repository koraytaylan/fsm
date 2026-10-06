//! Retained pre-claim admission; helper retirement never proves domain absence.

use super::native_client::{NativePreparation, NativePreparedCleanup};
use crate::{config::HandlerSpec, effect::PendingEffect, error::ExecError, sched::Scheduler};
use fsm_core::record::execution::{Admission, Claim, NativeDomain};
use fsm_store::store::Store;
use std::{collections::BTreeMap, time::Duration};

const TRANSPORT_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_PREPARATIONS: usize = 4096;

enum Phase {
    Queued,
    Preparing(NativePreparation),
    Prepared(NativeDomain),
    Cleaning(NativeDomain, NativePreparedCleanup),
    UnknownAllocation,
    UncertainPreparation(NativePreparation),
    UncertainCleanup(NativeDomain, NativePreparedCleanup),
    UncertainDomain(NativeDomain),
    ClaimUncertain(NativeDomain),
    Closed,
}

struct Pending {
    effect: PendingEffect,
    handler: HandlerSpec,
    namespace: String,
    generation: u64,
    cancelled: bool,
    phase: Phase,
}

#[derive(Default)]
pub(super) struct NativeAdmissions {
    pending: BTreeMap<String, Pending>,
}

pub(super) struct AdmissionRequest {
    pub effect: PendingEffect,
    pub handler: HandlerSpec,
    pub domain: NativeDomain,
    pub request_id: String,
}

impl NativeAdmissions {
    pub(super) fn queue(
        &mut self,
        snapshot: &Store,
        effect: &PendingEffect,
        handler: &HandlerSpec,
        scheduler: &mut Scheduler,
    ) -> Result<(), ExecError> {
        if snapshot.journal.is_memory()
            || snapshot.journal.poisoned
            || !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
        {
            return Err(ExecError::new(
                "exec/mode",
                "native admission requires a healthy supported durable store",
            ));
        }
        if self.pending.len() >= MAX_PREPARATIONS
            || self.pending.contains_key(&effect.effect_id)
            || !eligible(snapshot, effect)
        {
            return Err(deferred());
        }
        let (fingerprint, contract) = handler.checked_contract()?;
        let original = HandlerSpec::from_contract(&contract, &fingerprint)?;
        if original.effect != effect.effect_name {
            return Err(deferred());
        }
        let (namespace, generation) = super::native_client::discover_store(&snapshot.data_dir)
            .map_err(|_| {
                ExecError::new(
                    "exec/mode",
                    "native physical-store authority is unavailable",
                )
            })?;
        if !scheduler.retain_native_preparation(effect) {
            return Err(deferred());
        }
        self.pending.insert(
            effect.effect_id.clone(),
            Pending {
                effect: effect.clone(),
                handler: original,
                namespace,
                generation,
                cancelled: false,
                phase: Phase::Queued,
            },
        );
        Ok(())
    }

    pub(super) fn cancel(&mut self, effect: &str) -> Option<Result<(), ExecError>> {
        let pending = self.pending.get_mut(effect)?;
        pending.cancelled = true;
        match &mut pending.phase {
            Phase::Queued => pending.phase = Phase::Closed,
            Phase::Preparing(preparation) => {
                let result = preparation.cancel().map_err(|_| deferred());
                return Some(result);
            }
            // Delivered domains are cleaned using the complete original route;
            // unknown allocation or claim writes cannot be discarded here.
            _ => {}
        }
        Some(Ok(()))
    }

    pub(super) fn start_queued(&mut self) {
        if self
            .pending
            .values()
            .any(|pending| pending.phase.helper_busy())
        {
            return;
        }
        if let Some(pending) = self
            .pending
            .values_mut()
            .find(|pending| !pending.cancelled && matches!(pending.phase, Phase::Queued))
        {
            pending.phase = match NativePreparation::start(
                &pending.namespace,
                pending.generation,
                TRANSPORT_TIMEOUT,
            ) {
                Ok(preparation) => Phase::Preparing(preparation),
                Err(_) => Phase::UnknownAllocation,
            };
        }
    }

    pub(super) fn observe(&mut self) {
        for pending in self.pending.values_mut() {
            let phase = std::mem::replace(&mut pending.phase, Phase::UnknownAllocation);
            pending.phase = match phase {
                Phase::Preparing(mut preparation) if pending.cancelled => {
                    let _ = preparation.cancel();
                    let _ = preparation.reap();
                    Phase::UncertainPreparation(preparation)
                }
                Phase::Preparing(mut preparation) => match preparation.poll() {
                    Ok(Some(domain)) => Phase::Prepared(domain),
                    Ok(None) => Phase::Preparing(preparation),
                    Err(_) => Phase::UncertainPreparation(preparation),
                },
                Phase::Prepared(domain) if pending.cancelled => {
                    match NativePreparedCleanup::start(&domain, TRANSPORT_TIMEOUT) {
                        Ok(cleanup) => Phase::Cleaning(domain, cleanup),
                        Err(_) => Phase::UncertainDomain(domain),
                    }
                }
                Phase::Cleaning(domain, mut cleanup) => match cleanup.poll() {
                    Ok(true) => Phase::Closed,
                    Ok(false) => Phase::Cleaning(domain, cleanup),
                    Err(_) => Phase::UncertainCleanup(domain, cleanup),
                },
                Phase::UncertainPreparation(mut preparation) => {
                    let _ = preparation.reap();
                    Phase::UncertainPreparation(preparation)
                }
                Phase::UncertainCleanup(domain, mut cleanup) => {
                    let _ = cleanup.reap();
                    Phase::UncertainCleanup(domain, cleanup)
                }
                other => other,
            };
        }
    }

    pub(super) fn refresh(&mut self, snapshot: &Store, scheduler: &mut Scheduler) {
        let mut transferred = Vec::new();
        for (effect, pending) in &mut self.pending {
            // An append may have become durable before reporting failure;
            // adopt the genuine observed claim, never discard its domain.
            if matches!(pending.phase, Phase::ClaimUncertain(_)) {
                let domain = pending.phase.original_domain();
                if let Some(claim) = snapshot
                    .state
                    .execution
                    .claim_for(&pending.effect.instance_id, effect)
                {
                    if Some(claim.domain()) == domain && scheduler.retain_claim(claim) {
                        transferred.push(effect.clone());
                    }
                }
            } else if !eligible(snapshot, &pending.effect) {
                pending.cancelled = true;
                if matches!(pending.phase, Phase::Queued) {
                    pending.phase = Phase::Closed;
                } else if let Phase::Preparing(preparation) = &mut pending.phase {
                    let _ = preparation.cancel();
                }
            }
        }
        for effect in transferred {
            self.pending.remove(&effect);
        }
        self.release_closed(scheduler);
    }

    pub(super) fn release_closed(&mut self, scheduler: &mut Scheduler) {
        self.pending.retain(|_, pending| {
            if matches!(pending.phase, Phase::Closed) {
                scheduler.complete_unclaimed(&pending.effect);
                false
            } else {
                true
            }
        });
    }

    pub(super) fn ready(&self) -> bool {
        self.pending
            .values()
            .any(|pending| !pending.cancelled && matches!(pending.phase, Phase::Prepared(_)))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn len(&self) -> usize {
        self.pending.len()
    }

    pub(super) fn uncertain(&self) -> bool {
        self.pending.values().any(|pending| match &pending.phase {
            Phase::UnknownAllocation
            | Phase::UncertainDomain(_)
            | Phase::ClaimUncertain(_)
            | Phase::UncertainPreparation(_)
            | Phase::UncertainCleanup(_, _) => true,
            Phase::Preparing(preparation) => {
                preparation.progress().phase
                    == super::native_client::NativePreparationPhase::Uncertain
            }
            _ => false,
        })
    }

    pub(super) fn take_ready(
        &mut self,
        store: &Store,
    ) -> Option<Result<AdmissionRequest, ExecError>> {
        let pending = self
            .pending
            .values_mut()
            .find(|pending| !pending.cancelled && matches!(pending.phase, Phase::Prepared(_)))?;
        if store.journal.is_memory() || store.journal.is_read_only() || store.journal.poisoned {
            return Some(Err(ExecError::new(
                "exec/mode",
                "native admission requires a healthy durable writer",
            )));
        }
        if !eligible(store, &pending.effect) {
            pending.cancelled = true;
            return Some(Err(deferred()));
        }
        let Phase::Prepared(domain) = &pending.phase else {
            return Some(Err(deferred()));
        };
        let domain = domain.clone();
        let material = fsm_core::json::Value::Arr(vec![
            fsm_core::json::Value::Str(pending.effect.effect_id.clone()),
            domain.to_value(),
        ]);
        let request_id = format!(
            "exec-claim-{}",
            fsm_core::sha256::to_hex(&fsm_core::sha256::sha256(&fsm_core::canon::canon_bytes(
                &material
            )))
        );
        // Retain before the fallible append: any error may conceal a durable claim.
        pending.phase = Phase::ClaimUncertain(domain.clone());
        Some(Ok(AdmissionRequest {
            effect: pending.effect.clone(),
            handler: pending.handler.clone(),
            domain,
            request_id,
        }))
    }

    pub(super) fn transferred(&mut self, claim: &Claim) {
        self.pending.remove(claim.effect().1);
    }
}

impl Phase {
    fn helper_busy(&self) -> bool {
        let progress = match self {
            Self::Preparing(preparation) | Self::UncertainPreparation(preparation) => {
                preparation.progress().helper
            }
            Self::Cleaning(_, cleanup) | Self::UncertainCleanup(_, cleanup) => cleanup.progress(),
            _ => return false,
        };
        !progress.reaped || !progress.stdout_eof || !progress.stderr_eof
    }

    fn original_domain(&self) -> Option<&NativeDomain> {
        match self {
            Self::Prepared(domain)
            | Self::Cleaning(domain, _)
            | Self::UncertainCleanup(domain, _)
            | Self::UncertainDomain(domain)
            | Self::ClaimUncertain(domain) => Some(domain),
            _ => None,
        }
    }
}

fn eligible(store: &Store, effect: &PendingEffect) -> bool {
    store.state.execution.admission() == Admission::Enabled
        && store
            .state
            .execution
            .claim_for(&effect.instance_id, &effect.effect_id)
            .is_none()
        && store
            .state
            .execution
            .stopped_for(&effect.instance_id, &effect.effect_id)
            .is_none()
        && store
            .state
            .instances
            .get(&effect.instance_id)
            .is_some_and(|instance| {
                instance.status == fsm_core::machine::Status::Running
                    && instance.pending.contains(&effect.effect_id)
            })
        && crate::effect::resolve(store, &effect.effect_id)
            .is_ok_and(|resolved| resolved == *effect)
}

fn deferred() -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        "native preparation retains original ownership until authenticated reconciliation",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{HandlerKind, HandlerTable, Retry},
        sched::Directive,
        watch::Observation,
    };

    // These are local reservation transitions, not native authority fixtures.
    fn reservation(phase: Phase) -> (NativeAdmissions, Scheduler, PendingEffect) {
        let effect = PendingEffect {
            instance_id: "local".into(),
            effect_id: "local/3/0".into(),
            effect_name: "notify".into(),
            args: BTreeMap::new(),
            emitted_seq: 3,
            k: 0,
        };
        let handler = HandlerSpec {
            effect: "notify".into(),
            kind: HandlerKind::Process,
            argv: vec!["/bin/false".into()],
            timeout_ms: 1,
            on_ok: None,
            on_failed: None,
            retry: Retry::default(),
        };
        let mut table = HandlerTable::default();
        table.handlers.insert("notify".into(), handler.clone());
        let mut scheduler = Scheduler::new(table);
        assert!(matches!(
            scheduler
                .on_observation(
                    &Observation {
                        pending: vec![effect.clone()],
                        ..Observation::default()
                    },
                    0
                )
                .as_slice(),
            [Directive::Start { .. }]
        ));
        assert!(scheduler.retain_native_preparation(&effect));
        let pending = Pending {
            effect: effect.clone(),
            handler,
            namespace: "0123456789abcdef0123456789abcdef".into(),
            generation: 1,
            cancelled: false,
            phase,
        };
        let admissions = NativeAdmissions {
            pending: BTreeMap::from([(effect.effect_id.clone(), pending)]),
        };
        (admissions, scheduler, effect)
    }

    #[test]
    fn queued_cancellation_releases_but_unknown_allocation_stays_charged() {
        let (mut queued, mut scheduler, effect) = reservation(Phase::Queued);
        queued.cancel(&effect.effect_id).unwrap().unwrap();
        queued.release_closed(&mut scheduler);
        assert!(queued.is_empty());
        assert!(scheduler.inflight_effect(&effect.effect_id).is_none());

        let (mut unknown, mut scheduler, effect) = reservation(Phase::UnknownAllocation);
        for _ in 0..3 {
            unknown.cancel(&effect.effect_id).unwrap().unwrap();
            unknown.observe();
            unknown.release_closed(&mut scheduler);
            assert!(unknown.uncertain());
            assert_eq!(scheduler.inflight_effect(&effect.effect_id), Some(&effect));
        }
        // Logical handler time has elapsed, but preparation was never entry.
        assert!(
            scheduler
                .on_observation(
                    &Observation {
                        pending: vec![effect],
                        ..Observation::default()
                    },
                    100
                )
                .is_empty()
        );
    }
}
