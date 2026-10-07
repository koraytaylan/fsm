//! Owned original run and retained completion, independent of writer access.

use super::{NativeCompletion, NativeHelperProgress, NativeRun, NativeRunPhase};
use crate::error::ExecError;
use crate::rid::{ack_rid, attempt_rid};
use crate::run::Pipeline;
use fsm_core::json::Value;
use fsm_core::record::execution::{Claim, Settlement};
use fsm_store::clock::Clock;
use fsm_store::store::Store;
use std::time::Duration;

/// Identifier-free native host observations; transport retirement is not closure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeExecutionProgress {
    /// Transport phase, Closed for verified completion, or Uncertain without transport.
    pub phase: NativeRunPhase,
    /// Actual observations for an owned helper, when present.
    pub helper: Option<NativeHelperProgress>,
    /// The host retains capacity until matching durable settlement is proven.
    pub retained: bool,
}

/// An owned run retaining its original identity and checked candidate for application.
///
/// Observation never takes a writer; application requires one and retains the
/// completion on any refusal, so another writer cannot make closure disappear.
pub struct NativeExecution {
    claim: Claim,
    run: Option<NativeRun>,
    completion: Option<NativeCompletion>,
    retained: bool,
    start_requested: bool,
}

impl NativeExecution {
    /// Retain an original durable claim without requesting helper startup.
    ///
    /// The caller must retain the actual published claim; this constructor
    /// authenticates no ownership, starts no helper and supplies no completion.
    /// Observation and settlement remain deferred until original reconciliation.
    pub fn retain_uncertain(claim: &Claim) -> Self {
        Self {
            claim: claim.clone(),
            run: None,
            completion: None,
            retained: true,
            start_requested: false,
        }
    }

    /// Adopt current durable ownership and start binding under a healthy writer.
    pub fn start(store: &mut Store, claim: &Claim, timeout: Duration) -> Result<Self, ExecError> {
        let run = Pipeline.start_native(store, claim, timeout)?;
        Ok(Self::with_run(claim, run))
    }

    /// Start once after the host has installed this retained original owner.
    ///
    /// Every refusal preserves the claim and capacity, including a refusal
    /// before transport startup; this object cannot then retry binding or entry.
    /// Reconciliation must use the original durable identity instead.
    pub fn start_retained(
        &mut self,
        store: &mut Store,
        timeout: Duration,
    ) -> Result<(), ExecError> {
        if self.start_requested || self.run.is_some() || self.completion.is_some() || !self.retained
        {
            return Err(unproven());
        }
        self.start_requested = true;
        let mut run = Pipeline.start_native(store, &self.claim, timeout)?;
        run.require_writer_entry();
        self.run = Some(run);
        Ok(())
    }

    /// Request execution of a bound installed owner after a fresh writer recheck.
    /// Observation of this startup path never dispatches execution on its own.
    pub fn launch_bound(&mut self, store: &mut Store) -> Result<(), ExecError> {
        self.launch_bound_checked(store, || true).map(|_| ())
    }

    // The final host authorization runs after writer validation, before dispatch.
    // A refused fence does not consume the owner's one-shot entry permission.
    pub(crate) fn launch_bound_checked(
        &mut self,
        store: &mut Store,
        authorize: impl FnOnce() -> bool,
    ) -> Result<bool, ExecError> {
        let run = self.run.as_mut().ok_or_else(unproven)?;
        let hash = Pipeline::native_launch_hash(store, &self.claim)?;
        if !authorize() {
            return Ok(false);
        }
        run.launch_bound(&self.claim, &hash).map_err(|error| {
            ExecError::new("exec/inflight_deferred", error)
                .hint("retain the original bound owner and reconcile uncertain entry")
        })?;
        Ok(true)
    }

    /// Recover the current original run from a durable snapshot without launching.
    pub fn recover(store: &Store, claim: &Claim, timeout: Duration) -> Result<Self, ExecError> {
        let run = Pipeline.recover_native(store, claim, timeout)?;
        Ok(Self::with_run(claim, run))
    }

    /// Retain an independently verified original completion without starting helpers.
    ///
    /// This permits exact settlement replay after ownership has been consumed;
    /// it does not establish current journal ownership or physical store identity.
    pub fn from_completion(
        claim: &Claim,
        journal_claim: &str,
        completion: NativeCompletion,
    ) -> Result<Self, ExecError> {
        if !completion.matches_original(claim)
            || !completion.proof().matches_claim(claim, journal_claim)
        {
            return Err(unproven());
        }
        Ok(Self {
            claim: claim.clone(),
            run: None,
            completion: Some(completion),
            retained: true,
            start_requested: true,
        })
    }

    fn with_run(claim: &Claim, run: NativeRun) -> Self {
        Self {
            claim: claim.clone(),
            run: Some(run),
            completion: None,
            retained: true,
            start_requested: true,
        }
    }

    /// Poll bounded owned helper work and retain verified completion independently of a writer.
    ///
    /// Once completion is retained, repeated observations report readiness
    /// without polling the already-collected transport or delivering it again.
    pub fn observe(&mut self) -> Result<bool, ExecError> {
        if self.completion.is_some() {
            return Ok(true);
        }
        let run = self.run.as_mut().ok_or_else(unproven)?;
        self.completion = run.poll().map_err(|error| {
            ExecError::new("exec/inflight_deferred", error)
                .hint("retain original ownership and reconcile native closure")
        })?;
        Ok(self.completion.is_some())
    }

    /// Borrow retained original material; it may contain secrets and is not health output.
    pub fn completion(&self) -> Option<&NativeCompletion> {
        self.completion.as_ref()
    }

    /// Persist stopped evidence and consume it with the original retry policy.
    ///
    /// No outcome event is sent here; acknowledgement must be durable before
    /// the caller invokes Pipeline::advance_native_settled with this completion.
    /// A previous transaction is accepted only through exact original ledger replay.
    pub fn settle(&mut self, store: &mut Store, clock: &mut dyn Clock) -> Result<Value, ExecError> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native application requires a supported healthy durable writer",
            ));
        }
        let completion = self.completion.as_ref().ok_or_else(unproven)?;
        completion
            .proof()
            .check_store(&store.data_dir)
            .map_err(|error| ExecError::store(&error))?;
        let (instance, effect) = self.claim.effect();
        let response = if store.state.execution.claim_for(instance, effect) == Some(&self.claim) {
            let hash = store
                .current_execution_claim_hash(&self.claim)
                .map_err(|error| ExecError::store(&error))?;
            if !completion.proof().matches_claim(&self.claim, &hash) {
                return Err(unproven());
            }
            if let Some(stopped) = store.state.execution.stopped_for(instance, effect) {
                if stopped.outcome() != completion.stopped_outcome() {
                    return Err(unproven());
                }
            } else {
                Pipeline.stop_native(
                    store,
                    clock,
                    &self.claim,
                    completion,
                    &format!("exec-stop-{effect}-{}", self.claim.run_id()),
                )?;
            }
            Pipeline.settle_native_stopped(store, clock, &self.claim, completion)?
        } else {
            self.replay(store)?.ok_or_else(unproven)?
        };
        self.retained = false;
        Ok(response)
    }

    pub(crate) fn retire_interrupted(&mut self, store: &mut Store) -> Result<bool, ExecError> {
        use fsm_core::record::execution::Settlement;
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
        super::check_claim_store(&store.data_dir, &self.claim).map_err(|_| unproven())?;
        if self.completion.is_some() {
            // Actual handler completion keeps its original disposition and event path.
            return Err(unproven());
        }
        let (instance, effect) = self.claim.effect();
        if store.state.execution.claim_for(instance, effect) == Some(&self.claim) {
            return Err(unproven());
        }
        let request_id = format!("exec-interrupted-{effect}-{}", self.claim.run_id());
        if store
            .replay_execution_settlement(&self.claim, Settlement::Interrupted, &request_id)
            .map_err(|error| ExecError::store(&error))?
            .is_none()
        {
            return Err(unproven());
        }
        // Reap is bounded observation, never an assertion about native closure.
        if !self.reap().map_err(|_| unproven())? {
            return Ok(false);
        }
        if self
            .progress()
            .helper
            .is_some_and(|helper| !helper.is_retired())
        {
            return Ok(false);
        }
        self.retained = false;
        Ok(true)
    }

    fn replay(&self, store: &mut Store) -> Result<Option<Value>, ExecError> {
        let (_, effect) = self.claim.effect();
        let claim = self.claim.to_value();
        let attempt = claim
            .get("attempt")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse::<u32>().ok())
            .ok_or_else(unproven)?;
        // A later attempt can claim the same ack key; first prove an original
        // interruption/attempt transaction before asking about that shared key.
        for (disposition, request) in [
            (
                Settlement::Interrupted,
                format!("exec-interrupted-{effect}-{}", self.claim.run_id()),
            ),
            (Settlement::Attempted, attempt_rid(effect, attempt)),
            (Settlement::Acked, ack_rid(effect)),
        ] {
            if let Some(response) = store
                .replay_execution_settlement(&self.claim, disposition, &request)
                .map_err(|error| ExecError::store(&error))?
            {
                return Ok(Some(response));
            }
        }
        Ok(None)
    }

    /// Request helper cancellation; verified completion, if present, stays retained.
    pub fn cancel(&mut self) -> Result<(), String> {
        self.start_requested = true;
        match &mut self.run {
            Some(run) if self.completion.is_none() => run.cancel(),
            _ => Ok(()),
        }
    }

    /// Observe actual helper retirement, without releasing journal ownership or capacity.
    pub fn reap(&mut self) -> Result<bool, String> {
        match &mut self.run {
            Some(run) => run.reap(),
            None => Ok(true),
        }
    }

    /// Read bounded metadata without identifiers, candidate material or secrets.
    pub fn progress(&self) -> NativeExecutionProgress {
        let run = self.run.as_ref().map(NativeRun::progress);
        NativeExecutionProgress {
            phase: run.map_or_else(
                || {
                    if self.completion.is_some() {
                        NativeRunPhase::Closed
                    } else {
                        NativeRunPhase::Uncertain
                    }
                },
                |run| run.phase,
            ),
            helper: run.map(|run| run.helper),
            retained: self.retained,
        }
    }
}

fn unproven() -> ExecError {
    ExecError::new(
        "exec/inflight_deferred",
        "matching original native completion or durable settlement is not proven",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original_owner() -> fsm_core::record::execution::Claim {
        use fsm_core::json::{JsonLimits, parse};
        use fsm_core::record::execution::Claim;
        Claim::from_value(&parse(br#"{
      "run_id":1,"instance_id":"case-1","effect_id":"case-1/3/0","attempt":1,
      "handler_fingerprint":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "retry":{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]},
      "domain":{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}
    }"#, &JsonLimits::DEFAULT).unwrap()).unwrap()
    }

    #[test]
    fn missing_transport_cannot_become_verified_completion() {
        let mut execution = NativeExecution::retain_uncertain(&original_owner());
        assert!(execution.observe().is_err());
        execution.cancel().unwrap();
        assert!(execution.reap().unwrap());
        assert!(execution.observe().is_err());
        assert!(execution.completion().is_none());
        let progress = execution.progress();
        assert_eq!(progress.phase, NativeRunPhase::Uncertain);
        assert!(progress.retained);
        assert!(progress.helper.is_none());
    }

    #[test]
    fn joined_startup_refusal_retains_original_execution_without_a_completion() {
        use crate::run::native_client::{startup::FixtureFactory, worker};
        use std::{sync::Arc, time::Instant};
        let budget = Arc::new(worker::Budget::default());
        let _scope = worker::Scope::enter(Some(&budget));
        let _factory = FixtureFactory::install(|_| Err("fixture helper startup refused".into()));
        // Literal original identity is not protected closure evidence.
        let claim = original_owner();
        let hash = format!("sha256:{}", "a".repeat(64));
        let run = NativeRun::recover(&claim, &hash, Duration::from_secs(1)).unwrap();
        let mut execution = NativeExecution::with_run(&claim, run);
        let until = Instant::now() + Duration::from_secs(5);
        let refusal = loop {
            match execution.observe() {
                Ok(false) => {
                    assert!(Instant::now() < until);
                    std::thread::sleep(Duration::from_millis(1));
                }
                Ok(true) => panic!("startup refusal fabricated a verified completion"),
                Err(error) => break error,
            }
        };
        assert_eq!(refusal.code, "exec/inflight_deferred");
        assert!(execution.reap().unwrap());
        let progress = execution.progress();
        assert_eq!(progress.phase, NativeRunPhase::Uncertain);
        assert!(progress.retained && execution.completion().is_none());
        let helper = progress.helper.unwrap();
        assert!(helper.not_started && helper.is_retired());
        assert!(!helper.reaped && !helper.stdout_eof && !helper.stderr_eof);
        assert_eq!(execution.claim, claim);
    }

    #[test]
    fn interruption_retirement_refuses_memory_before_route_or_helper_access() {
        // Literal claim metadata is not closure evidence; no receipt is created.
        let mut execution = NativeExecution::retain_uncertain(&original_owner());
        let mut store = Store::open_memory().unwrap();
        let state = store.state.clone();
        let records = store.records.clone();
        let head = (store.journal.last_seq, store.journal.last_hash.clone());
        assert_eq!(
            execution.retire_interrupted(&mut store).unwrap_err().code,
            "exec/mode"
        );
        assert!(execution.progress().retained);
        assert!(execution.progress().helper.is_none());
        assert_eq!(store.records, records);
        assert_eq!(
            (store.journal.last_seq, store.journal.last_hash.clone()),
            head
        );
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    }

    #[test]
    fn refused_installed_owner_cannot_request_startup_again() {
        let mut execution = NativeExecution::retain_uncertain(&original_owner());
        let mut store = Store::open_memory().unwrap();
        let state = store.state.clone();
        let records = store.records.clone();
        let head = store.journal.last_hash.clone();
        assert_eq!(
            execution
                .start_retained(&mut store, Duration::from_secs(1))
                .unwrap_err()
                .code,
            "exec/mode"
        );
        assert_eq!(
            execution
                .start_retained(&mut store, Duration::from_secs(1))
                .unwrap_err()
                .code,
            "exec/inflight_deferred"
        );
        assert!(execution.progress().retained);
        assert!(execution.progress().helper.is_none());
        assert_eq!(store.records, records);
        assert_eq!(store.journal.last_hash, head);
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    }

    #[test]
    fn cancellation_before_startup_preserves_owner_without_permitting_entry() {
        let mut execution = NativeExecution::retain_uncertain(&original_owner());
        let mut store = Store::open_memory().unwrap();
        let records = store.records.clone();
        let state = store.state.clone();
        let head = store.journal.last_hash.clone();
        execution.cancel().unwrap();
        assert_eq!(
            execution
                .start_retained(&mut store, Duration::from_secs(1))
                .unwrap_err()
                .code,
            "exec/inflight_deferred"
        );
        assert!(execution.progress().retained);
        assert!(execution.progress().helper.is_none());
        assert_eq!(store.records, records);
        assert_eq!(store.journal.last_hash, head);
        assert!(fsm_store::snapshot::store_states_eq(&store.state, &state));
    }
}
