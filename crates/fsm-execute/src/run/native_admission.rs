//! Retained pre-claim admission; helper retirement never proves domain absence.

use super::native_client::{NativePreparation, NativePreparedCleanup, NativePreparedOwner};
use crate::{
    config::{HandlerSpec, HandlerTable},
    effect::PendingEffect,
    error::ExecError,
    sched::Scheduler,
};
use fsm_core::record::execution::{Admission, Claim, NativeDomain};
use fsm_store::store::Store;
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

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
    owner: Option<NativePreparedOwner>,
}

#[derive(Default)]
pub(super) struct NativeAdmissions {
    pending: BTreeMap<String, Pending>,
    cleanup_diagnostic: Option<String>,
}

pub(super) struct AdmissionRequest {
    pub effect: PendingEffect,
    pub handler: HandlerSpec,
    pub domain: NativeDomain,
    pub request_id: String,
}

impl NativeAdmissions {
    pub(super) fn close_admission(&mut self) {
        for pending in self.pending.values_mut() {
            pending.cancelled = true;
            if matches!(pending.phase, Phase::Queued) {
                pending.phase = Phase::Closed;
            }
        }
    }

    pub(super) fn queue(
        &mut self,
        snapshot: &Store,
        effect: &PendingEffect,
        handler: &HandlerSpec,
        scheduler: &mut Scheduler,
        admission_closed: &AtomicBool,
    ) -> Result<(), ExecError> {
        if snapshot.journal.is_memory()
            || snapshot.journal.is_read_only()
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
        if admission_closed.load(Ordering::Acquire) || !scheduler.retain_native_preparation(effect)
        {
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
                owner: None,
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

    pub(super) fn start_queued(&mut self, admission_closed: &AtomicBool) {
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
            if admission_closed.load(Ordering::Acquire) {
                return;
            }
            pending.phase = match NativePreparation::start_owned(
                &pending.namespace,
                pending.generation,
                TRANSPORT_TIMEOUT,
            ) {
                Ok(preparation) => Phase::Preparing(preparation),
                // Reservation refusal precedes helper dispatch (SPEC worker capacity).
                // No allocation occurred, so retain this effect for a later turn.
                Err(error) if error == super::native_client::worker::CAPACITY_EXHAUSTED => {
                    Phase::Queued
                }
                Err(error) => {
                    self.cleanup_diagnostic.get_or_insert_with(|| {
                        bounded_diagnostic("native-preparation-uncertain ", &error)
                    });
                    Phase::UnknownAllocation
                }
            };
        }
    }

    pub(super) fn observe(&mut self) {
        for pending in self.pending.values_mut() {
            let phase = std::mem::replace(&mut pending.phase, Phase::UnknownAllocation);
            pending.phase = match phase {
                // Preparation alone cannot launch a handler: even cancellation
                // must receive any original delivered domain before cleanup;
                // killing its transport first would strand a known allocation.
                Phase::Preparing(mut preparation) => match preparation.poll_owned() {
                    Ok(Some(owner)) => {
                        let domain = owner.domain().clone();
                        pending.owner = Some(owner);
                        Phase::Prepared(domain)
                    }
                    Ok(None) => Phase::Preparing(preparation),
                    Err(error) => {
                        self.cleanup_diagnostic.get_or_insert_with(|| {
                            bounded_diagnostic("native-preparation-uncertain ", &error)
                        });
                        Phase::UncertainPreparation(preparation)
                    }
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
                    Err(error) => {
                        self.cleanup_diagnostic
                            .get_or_insert_with(|| cleanup_diagnostic(&error));
                        Phase::UncertainCleanup(domain, cleanup)
                    }
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

    pub(super) fn take_cleanup_diagnostic(&mut self) -> Option<String> {
        self.cleanup_diagnostic.take()
    }

    pub(super) fn local_publications(&self, snapshot: &Store) -> Vec<Claim> {
        self.pending
            .values()
            .filter_map(|pending| {
                let claim = snapshot
                    .state
                    .execution
                    .claim_for(&pending.effect.instance_id, &pending.effect.effect_id)?;
                self.matches_local_publication(claim).then(|| claim.clone())
            })
            .collect()
    }

    pub(super) fn matches_local_publication(&self, claim: &Claim) -> bool {
        self.pending
            .get(claim.effect().1)
            .is_some_and(|pending| pending.matches_publication(claim))
    }

    pub(super) fn refresh(&mut self, snapshot: &Store, scheduler: &mut Scheduler) {
        let mut transferred = Vec::new();
        for (effect, pending) in &mut self.pending {
            // An append may have become durable before reporting failure;
            // adopt the genuine observed claim, never discard its domain.
            if matches!(pending.phase, Phase::ClaimUncertain(_)) {
                if let Some(claim) = snapshot
                    .state
                    .execution
                    .claim_for(&pending.effect.instance_id, effect)
                {
                    if pending.owner.is_none()
                        && pending.matches_publication(claim)
                        && scheduler.retain_claim(claim)
                    {
                        transferred.push(effect.clone());
                    }
                }
            } else if !eligible(snapshot, &pending.effect) {
                pending.cancelled = true;
                if matches!(pending.phase, Phase::Queued) {
                    pending.phase = Phase::Closed;
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

    pub(super) fn queued(&self) -> bool {
        self.pending
            .values()
            .any(|pending| !pending.cancelled && matches!(pending.phase, Phase::Queued))
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub(super) fn helpers_retired(&self) -> bool {
        self.pending
            .values()
            .all(|pending| !pending.phase.helper_busy())
    }

    pub(super) fn phase_counts(&self) -> [usize; 10] {
        let mut counts = [0; 10];
        for pending in self.pending.values() {
            let index = match pending.phase {
                Phase::Queued => 0,
                Phase::Preparing(_) => 1,
                Phase::Prepared(_) => 2,
                Phase::Cleaning(_, _) => 3,
                Phase::UnknownAllocation => 4,
                Phase::UncertainPreparation(_) => 5,
                Phase::UncertainCleanup(_, _) => 6,
                Phase::UncertainDomain(_) => 7,
                Phase::ClaimUncertain(_) => 8,
                Phase::Closed => 9,
            };
            counts[index] += 1;
        }
        counts
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
        table: &HandlerTable,
        cache: Option<&crate::contract::AdmissionCache>,
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
        // SPEC pending-contract refusal precedes any uncertain claim publication.
        // Keep the original prepared domain for authenticated cleanup on refusal.
        let admission = crate::contract::check_pending_cached(store, &pending.effect, table, cache)
            .and_then(|()| {
                // SPEC prepared admission matches immutable contract material;
                // recovery sorts retry classes, while parsed lists retain order.
                if table
                    .handlers
                    .get(&pending.effect.effect_name)
                    .map(|handler| {
                        handler
                            .checked_contract()
                            .map(|(fingerprint, _)| fingerprint)
                    })
                    .transpose()?
                    == Some(pending.handler.checked_contract()?.0)
                {
                    Ok(())
                } else {
                    Err(ExecError::new(
                        "exec/contract_unknown",
                        "prepared handler contract no longer matches the loaded table",
                    )
                    .hint("retire the original preparation and observe pending work again"))
                }
            });
        if let Err(error) = admission {
            pending.cancelled = true;
            return Some(Err(error));
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

    pub(super) fn transferred(&mut self, claim: &Claim) -> Option<NativePreparedOwner> {
        if !self.matches_local_publication(claim) {
            return None;
        }
        self.pending
            .remove(claim.effect().1)
            .and_then(|pending| pending.owner)
    }
}

impl Pending {
    fn matches_publication(&self, claim: &Claim) -> bool {
        let (instance, effect) = claim.effect();
        self.effect.instance_id == instance
            && self.effect.effect_id == effect
            && matches!(&self.phase, Phase::ClaimUncertain(_))
            && self.phase.original_domain() == Some(claim.domain())
            && self
                .handler
                .checked_contract()
                .is_ok_and(|(fingerprint, contract)| {
                    let original = claim.to_value();
                    original.get("handler_fingerprint")
                        == Some(&fsm_core::json::Value::Str(fingerprint))
                        && original.get("retry") == contract.get("retry")
                })
    }
}

impl Phase {
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

    fn helper_busy(&self) -> bool {
        let progress = match self {
            Self::Preparing(preparation) | Self::UncertainPreparation(preparation) => {
                preparation.progress().helper
            }
            Self::Cleaning(_, cleanup) | Self::UncertainCleanup(_, cleanup) => cleanup.progress(),
            _ => return false,
        };
        !progress.is_retired()
    }
}

fn cleanup_diagnostic(error: &str) -> String {
    bounded_diagnostic("native-prepared-cleanup-uncertain ", error)
}

pub(super) fn bounded_diagnostic(prefix: &str, error: &str) -> String {
    let mut line = String::from(prefix);
    for character in error.chars() {
        let character = if character.is_control() {
            ' '
        } else {
            character
        };
        if line.len() + character.len_utf8() > 1024 {
            break;
        }
        line.push(character);
    }
    line
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
            emitting_machine_id: "scheduler-fixture-definition".into(),
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
            owner: None,
        };
        let admissions = NativeAdmissions {
            pending: BTreeMap::from([(effect.effect_id.clone(), pending)]),
            cleanup_diagnostic: None,
        };
        (admissions, scheduler, effect)
    }

    #[test]
    fn cleanup_diagnostic_is_single_line_byte_bounded_and_consumed_once() {
        let prefix = cleanup_diagnostic("");
        let boundary = "a".repeat(1024 - prefix.len());
        assert_eq!(cleanup_diagnostic(&boundary).len(), 1024);
        assert_eq!(
            cleanup_diagnostic(&(boundary.clone() + "b")),
            prefix.clone() + &boundary
        );
        assert_eq!(cleanup_diagnostic(&(boundary + "é")).len(), 1024);
        assert_eq!(cleanup_diagnostic("a\n\r\0b"), prefix + "a   b");
        let mut admissions = NativeAdmissions {
            cleanup_diagnostic: Some(cleanup_diagnostic("refused")),
            ..NativeAdmissions::default()
        };
        assert!(admissions.take_cleanup_diagnostic().is_some());
        assert!(admissions.take_cleanup_diagnostic().is_none());
    }

    struct AdmissionDirectory(std::path::PathBuf);

    impl Drop for AdmissionDirectory {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    fn prepared_contract() -> (AdmissionDirectory, Store, NativeAdmissions, HandlerTable) {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = AdmissionDirectory(std::env::temp_dir().join(format!(
            "fsm-prepared-contract-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        )));
        std::fs::create_dir(&directory.0).unwrap();
        let mut store = Store::open(&directory.0).unwrap();
        let mut clock = fsm_store::clock::FixedClock::new(1000, 1);
        let machine = fsm_core::json::parse(
            br#"{"format":"fsm.machine/1","name":"prepared","context":[],
            "events":[],"effects":[{"name":"notify","fields":[]}],
            "states":[{"name":"ready","entry":{"emit":[{"effect":"notify"}]}}],
            "initial":"ready","transitions":[]}"#,
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        store
            .define_machine_on(&mut clock, machine, false, false)
            .unwrap();
        store
            .create_instance_ctx_on(
                &mut clock,
                "prepared",
                "prepared-instance",
                "create",
                None,
                &BTreeMap::new(),
                &[],
            )
            .unwrap();
        let effect = crate::effect::resolve(
            &store,
            &store.state.instances["prepared-instance"].pending[0],
        )
        .unwrap();
        // Reservation metadata only: no authority, native allocation or claim is created.
        store.state.execution =
            fsm_core::record::execution::ExecutionState::new(Admission::Enabled);
        let fixture = fsm_core::json::parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        let domain =
            NativeDomain::from_value(fixture.get("claim").unwrap().get("domain").unwrap()).unwrap();
        let (mut admissions, scheduler, original) = reservation(Phase::Prepared(domain));
        let mut pending = admissions.pending.remove(&original.effect_id).unwrap();
        pending.effect = effect.clone();
        admissions.pending.insert(effect.effect_id, pending);
        (
            directory,
            store,
            admissions,
            scheduler.handler_table().clone(),
        )
    }

    fn refuses_prepared_contract(change: impl FnOnce(&mut HandlerTable), code: &str) {
        let (_directory, store, mut admissions, mut table) = prepared_contract();
        change(&mut table);
        let state = store.state.clone();
        let records = store.records.clone();
        let sequence = store.journal.last_seq;
        let error = match admissions.take_ready(&store, &table, None).unwrap() {
            Err(error) => error,
            Ok(_) => panic!("incompatible preparation authorized claim publication"),
        };
        assert_eq!(error.code, code);
        assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
        assert_eq!(store.records, records);
        assert_eq!(store.journal.last_seq, sequence);
        let pending = admissions.pending.values().next().unwrap();
        assert!(pending.cancelled);
        assert!(matches!(pending.phase, Phase::Prepared(_)));
        assert!(!admissions.ready());
    }

    #[test]
    fn prepared_claim_refuses_invalid_current_outcome_without_publication() {
        refuses_prepared_contract(
            |table| {
                table.handlers.get_mut("notify").unwrap().on_ok = Some(crate::config::Advance {
                    event: "undeclared".into(),
                    payload: fsm_core::json::Value::Obj(BTreeMap::new()),
                    stamps: Vec::new(),
                });
            },
            "exec/contract_invalid",
        );
    }

    #[test]
    fn prepared_claim_refuses_changed_private_command_without_publication() {
        refuses_prepared_contract(
            |table| {
                table.handlers.get_mut("notify").unwrap().argv =
                    vec!["/operator/replacement".into()];
            },
            "exec/contract_unknown",
        );
    }

    #[test]
    fn prepared_claim_refuses_new_manual_disposition_without_publication() {
        refuses_prepared_contract(
            |table| {
                table.handlers.remove("notify");
                table.manual_effects.insert("notify".into());
            },
            "exec/contract_unknown",
        );
    }

    #[test]
    fn compatible_preparation_reaches_publication_phase_without_writing() {
        let (_directory, store, mut admissions, table) = prepared_contract();
        let records = store.records.clone();
        assert!(admissions.take_ready(&store, &table, None).unwrap().is_ok());
        assert!(matches!(
            admissions.pending.values().next().unwrap().phase,
            Phase::ClaimUncertain(_)
        ));
        assert_eq!(store.records, records);
    }

    #[test]
    fn prepared_claim_accepts_canonical_default_and_reordered_retry_classes() {
        for kind in [
            r#""kind":"process""#,
            r#""kind":"mcp","tool":"run","arguments":{"fixed":"private literal"}"#,
        ] {
            for retry in ["", r#", "retry":{"on":["timeout","spawn"]}"#] {
                let (_directory, store, mut admissions, mut table) = prepared_contract();
                let source = format!(
                    r#"{{"format":"fsm.handlers/1","handlers":[{{"effect":"notify",{kind},"argv":["/bin/false"],"timeout_ms":100{retry}}}]}}"#
                );
                let loaded = HandlerTable::parse(&source)
                    .unwrap()
                    .handlers
                    .remove("notify")
                    .unwrap();
                let pending = admissions.pending.values_mut().next().unwrap();
                pending.handler =
                    HandlerSpec::from_contract(&loaded.contract_value(), &loaded.fingerprint())
                        .unwrap();
                assert_ne!(pending.handler.retry.on, loaded.retry.on);
                table.handlers.insert("notify".into(), loaded.clone());
                let records = store.records.clone();
                let state = store.state.clone();
                let request = admissions
                    .take_ready(&store, &table, None)
                    .unwrap()
                    .unwrap();
                assert_eq!(request.handler.fingerprint(), loaded.fingerprint());
                assert_eq!(request.handler.contract_value(), loaded.contract_value());
                assert_eq!(store.records, records);
                assert!(fsm_store::snapshot::store_states_eq(&state, &store.state));
                assert!(matches!(
                    admissions.pending.values().next().unwrap().phase,
                    Phase::ClaimUncertain(_)
                ));
            }
        }
    }

    #[test]
    fn exhausted_worker_capacity_keeps_original_preparation_queued() {
        let (_scope, capacity) = super::super::native_client::worker::exhaust_capacity();
        let _startup = super::super::native_client::worker::refuse_fixture_startup();
        let (mut admissions, scheduler, effect) = reservation(Phase::Queued);
        admissions.start_queued(&AtomicBool::new(false));
        assert!(matches!(
            admissions.pending[&effect.effect_id].phase,
            Phase::Queued
        ));
        assert_eq!(scheduler.inflight_effect(&effect.effect_id), Some(&effect));
        assert!(!admissions.uncertain());
        assert!(admissions.take_cleanup_diagnostic().is_none());
        drop(capacity);
        admissions.start_queued(&AtomicBool::new(false));
        assert!(matches!(
            admissions.pending[&effect.effect_id].phase,
            Phase::Preparing(_)
        ));
        assert_eq!(scheduler.inflight_effect(&effect.effect_id), Some(&effect));
    }

    #[test]
    fn queued_preparation_requires_writer_attention_without_claim_permission() {
        let (mut admissions, _, effect) = reservation(Phase::Queued);
        assert!(admissions.queued());
        assert!(!admissions.ready());
        admissions.cancel(&effect.effect_id).unwrap().unwrap();
        assert!(!admissions.queued());
        assert!(!admissions.ready());
    }

    #[test]
    fn closed_fence_refuses_queued_dispatch_and_retains_unknown_allocation() {
        let fence = AtomicBool::new(true);
        let (mut queued, mut scheduler, effect) = reservation(Phase::Queued);
        queued.start_queued(&fence);
        assert!(matches!(
            queued.pending[&effect.effect_id].phase,
            Phase::Queued
        ));
        queued.close_admission();
        assert!(queued.pending[&effect.effect_id].cancelled);
        queued.release_closed(&mut scheduler);
        assert!(queued.is_empty());
        assert!(scheduler.inflight_effect(&effect.effect_id).is_none());
        let (mut unknown, mut scheduler, effect) = reservation(Phase::UnknownAllocation);
        unknown.close_admission();
        unknown.observe();
        unknown.release_closed(&mut scheduler);
        assert!(unknown.uncertain());
        assert!(unknown.pending[&effect.effect_id].cancelled);
        assert_eq!(scheduler.inflight_effect(&effect.effect_id), Some(&effect));
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

    #[test]
    fn phase_inventory_distinguishes_unknown_allocation_from_uncertain_publication() {
        // Classification metadata only; it cannot grant native closure authority.
        let fixture = fsm_core::json::parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        let domain =
            NativeDomain::from_value(fixture.get("claim").unwrap().get("domain").unwrap()).unwrap();
        for (phase, index) in [
            (Phase::Queued, 0),
            (Phase::Prepared(domain.clone()), 2),
            (Phase::UnknownAllocation, 4),
            (Phase::UncertainDomain(domain.clone()), 7),
            (Phase::ClaimUncertain(domain), 8),
            (Phase::Closed, 9),
        ] {
            let (admissions, _, _) = reservation(phase);
            let counts = admissions.phase_counts();
            assert_eq!(counts.iter().sum::<usize>(), admissions.len());
            assert_eq!(counts[index], 1);
        }
    }

    #[test]
    fn local_publication_requires_original_phase_domain_and_contract() {
        // Metadata here exercises provenance classification, never closure authority.
        let fixture = fsm_core::json::parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        let domain =
            NativeDomain::from_value(fixture.get("claim").unwrap().get("domain").unwrap()).unwrap();
        let (mut admissions, _, effect) = reservation(Phase::ClaimUncertain(domain.clone()));
        let pending = admissions.pending.get_mut(&effect.effect_id).unwrap();
        let (fingerprint, contract) = pending.handler.checked_contract().unwrap();
        let mut fields = fixture.get("claim").unwrap().as_obj().unwrap().clone();
        fields.insert(
            "instance_id".into(),
            fsm_core::json::Value::Str(effect.instance_id),
        );
        fields.insert(
            "effect_id".into(),
            fsm_core::json::Value::Str(effect.effect_id.clone()),
        );
        fields.insert(
            "handler_fingerprint".into(),
            fsm_core::json::Value::Str(fingerprint),
        );
        fields.insert("retry".into(), contract.get("retry").unwrap().clone());
        let claim = Claim::from_value(&fsm_core::json::Value::Obj(fields.clone())).unwrap();
        assert!(admissions.matches_local_publication(&claim));
        let mut different_domain = domain.to_value().as_obj().unwrap().clone();
        different_domain.insert("allocation".into(), fsm_core::json::Value::Num("8".into()));
        fields.insert(
            "domain".into(),
            fsm_core::json::Value::Obj(different_domain),
        );
        let other_domain = Claim::from_value(&fsm_core::json::Value::Obj(fields.clone())).unwrap();
        assert!(!admissions.matches_local_publication(&other_domain));
        fields.insert("domain".into(), domain.to_value());
        fields.insert(
            "handler_fingerprint".into(),
            fixture
                .get("claim")
                .unwrap()
                .get("handler_fingerprint")
                .unwrap()
                .clone(),
        );
        let other = Claim::from_value(&fsm_core::json::Value::Obj(fields)).unwrap();
        assert!(!admissions.matches_local_publication(&other));
        admissions.pending.get_mut(&effect.effect_id).unwrap().phase = Phase::Prepared(domain);
        assert!(!admissions.matches_local_publication(&claim));
    }
}
