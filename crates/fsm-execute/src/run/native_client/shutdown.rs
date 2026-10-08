//! Protected closure requests retained independently of execution transport.

use super::proof_worker::ProofWorker;
use super::{NativeHelperProgress, NativeRequest, check_claim_store};
use fsm_core::{json::Value, record::execution::Claim};
use fsm_store::store::{Store, VerifiedClosure};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    time::{Duration, Instant},
};

/// A closure request for one original durable claim; no journal mutation.
///
/// A broker response alone never proves closure; successful polling requires
/// an immutable protected receipt matching the original claim record hash.
/// Keep the execution transport separately until its own reap and EOF finish.
pub struct NativeShutdown {
    request: NativeRequest,
    claim: Claim,
    journal_claim: String,
    store_directory: PathBuf,
    receipt: PathBuf,
    deadline: Instant,
    response_checked: bool,
    proof: Option<VerifiedClosure>,
    verification: Option<ProofWorker<VerifiedClosure>>,
    error: Option<String>,
}

impl NativeShutdown {
    pub(crate) fn matches_original(&self, claim: &Claim) -> bool {
        &self.claim == claim
    }

    /// Request native closure without requiring or acquiring the writer lease.
    ///
    /// The snapshot must contain the exact original unresolved claim and its
    /// verified record hash; copied stores and replaced authorities refuse.
    /// This operation never binds or launches work and never settles ownership.
    pub fn start(store: &Store, claim: &Claim, timeout: Duration) -> Result<Self, String> {
        Self::start_action(store, claim, timeout, "close-claimed")
    }

    /// Request orphan closure while refusing an active or missing runner lease.
    ///
    /// Uses the same original claim, physical store and authenticated receipt
    /// checks as shutdown, but never authorizes cancellation of a live runner.
    /// Closure alone does not justify discarding an original completion result;
    /// callers must recover that evidence before choosing any settlement.
    pub fn start_reconciliation(
        store: &Store,
        claim: &Claim,
        timeout: Duration,
    ) -> Result<Self, String> {
        Self::start_action(store, claim, timeout, "reconcile-claimed")
    }

    fn start_action(
        store: &Store,
        claim: &Claim,
        timeout: Duration,
        action: &str,
    ) -> Result<Self, String> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.poisoned
        {
            return Err("native shutdown requires a supported verified durable store".into());
        }
        if timeout.is_zero() {
            return Err("native shutdown deadline invalid".into());
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("native shutdown deadline exceeds clock range")?;
        let journal_claim = store
            .current_execution_claim_hash(claim)
            .map_err(|error| error.message)?;
        check_claim_store(&store.data_dir, claim)?;
        let domain = claim.domain().to_value();
        let namespace = domain
            .get("namespace")
            .and_then(Value::as_str)
            .ok_or("native shutdown namespace missing")?;
        let generation = domain
            .get("generation")
            .and_then(Value::as_num)
            .ok_or("native shutdown generation missing")?
            .parse::<u64>()
            .map_err(|_| "native shutdown generation invalid")?;
        let allocation = domain
            .get("allocation")
            .and_then(Value::as_num)
            .ok_or("native shutdown allocation missing")?;
        let receipt = PathBuf::from("/var/lib/fsm-containment")
            .join(namespace)
            .join(format!("authority-{generation}"))
            .join(format!("closure-{allocation}-{}.json", claim.run_id()));
        let payload = Value::Obj(BTreeMap::from([
            (
                "format".into(),
                Value::Str("fsm.native-claim-binding/1".into()),
            ),
            ("claim".into(), claim.to_value()),
            ("journal_claim".into(), Value::Str(journal_claim.clone())),
        ]));
        let message = Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-request/1".into())),
            ("action".into(), Value::Str(action.into())),
            ("payload".into(), payload),
        ]));
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or("native shutdown deadline; original claim remains unresolved")?;
        let request = NativeRequest::start(namespace, generation, &message, remaining)?;
        Ok(Self {
            request,
            claim: claim.clone(),
            journal_claim,
            store_directory: store.data_dir.clone(),
            receipt,
            deadline,
            response_checked: false,
            proof: None,
            verification: None,
            error: None,
        })
    }

    /// Poll owned helper work and authenticate original claim-matched closure.
    ///
    /// Missing or mismatched evidence leaves the durable claim unresolved;
    /// helper death, EOF and a successful null response are insufficient.
    pub fn poll(&mut self) -> Result<Option<VerifiedClosure>, String> {
        if let Some(proof) = &self.proof {
            return Ok(Some(proof.clone()));
        }
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let result = self.poll_once();
        match result {
            Ok(proof) => {
                self.proof = proof.clone();
                Ok(proof)
            }
            Err(error) => {
                self.error = Some(error.clone());
                if let Some(verification) = &self.verification {
                    verification.cancel();
                }
                let _ = self.request.cancel();
                Err(error)
            }
        }
    }

    fn poll_once(&mut self) -> Result<Option<VerifiedClosure>, String> {
        if Instant::now() >= self.deadline {
            return Err("native shutdown deadline; original claim remains unresolved".into());
        }
        if let Some(verification) = &mut self.verification {
            let proof = verification.poll()?;
            if Instant::now() >= self.deadline {
                return Err("native shutdown deadline; original claim remains unresolved".into());
            }
            return Ok(proof);
        }
        if !self.response_checked {
            let Some(response) = self.request.poll()? else {
                return Ok(None);
            };
            validate_response(&response)?;
            self.response_checked = true;
        }
        let original = OriginalClosure {
            store_directory: self.store_directory.clone(),
            claim: self.claim.clone(),
            journal_claim: self.journal_claim.clone(),
            receipt: self.receipt.clone(),
        };
        if let Some(ticket) = &self.request.ticket {
            self.verification = Some(ProofWorker::start(
                move || original.verify(),
                std::sync::Arc::clone(ticket),
                self.deadline,
            )?);
            return Ok(None);
        }
        let proof = original.verify()?;
        if Instant::now() >= self.deadline {
            return Err("native shutdown deadline; original claim remains unresolved".into());
        }
        Ok(Some(proof))
    }

    /// Observe helper retirement without asserting closure or releasing ownership.
    pub fn reap(&mut self) -> Result<bool, String> {
        let helper = self.request.reap()?;
        let proof = self.verification.as_mut().is_none_or(ProofWorker::reap);
        Ok(helper && proof)
    }

    /// Inspect identifier-free helper facts; these never prove domain closure.
    pub fn progress(&self) -> NativeHelperProgress {
        let helper = self.request.progress();
        self.verification.as_ref().map_or(helper, |verification| {
            verification.withhold_retirement(helper)
        })
    }
}

struct OriginalClosure {
    store_directory: PathBuf,
    claim: Claim,
    journal_claim: String,
    receipt: PathBuf,
}

impl OriginalClosure {
    fn verify(self) -> Result<VerifiedClosure, String> {
        check_claim_store(&self.store_directory, &self.claim)?;
        let proof = VerifiedClosure::read(&self.receipt).map_err(|error| error.message)?;
        proof
            .check_store(&self.store_directory)
            .map_err(|error| error.message)?;
        if !proof.matches_claim(&self.claim, &self.journal_claim) {
            return Err("native shutdown receipt differs from original claim".into());
        }
        Ok(proof)
    }
}

fn validate_response(response: &Value) -> Result<(), String> {
    if response.as_obj().is_some_and(|fields| fields.len() == 3)
        && response.get("format").and_then(Value::as_str) == Some("fsm.native-response/1")
        && response.get("ok") == Some(&Value::Bool(true))
        && response.get("result") == Some(&Value::Null)
    {
        Ok(())
    } else {
        Err("native shutdown broker response differs".into())
    }
}

impl NativeShutdown {
    /// Apply authenticated interruption without an outcome event or acknowledgement.
    ///
    /// A healthy original writer is required. Existing non-interrupted stopped
    /// outcomes refuse; callers retain their original completion path instead.
    /// Exact original replay is the only reconciliation when the claim is absent.
    /// This method does not discard either helper or release host capacity.
    pub fn settle_interrupted(
        &self,
        store: &mut Store,
        clock: &mut dyn fsm_store::clock::Clock,
    ) -> Result<Value, crate::error::ExecError> {
        use crate::error::ExecError;
        use fsm_core::record::execution::{Settlement, StoppedOutcome};
        use fsm_store::store::{ExecutionSettleRequest, ExecutionStopRequest};
        let deferred = || {
            ExecError::new(
                "exec/inflight_deferred",
                "original authenticated shutdown interruption is not proven",
            )
        };
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "shutdown interruption requires a supported healthy durable writer",
            ));
        }
        let proof = self.proof.as_ref().ok_or_else(deferred)?;
        proof
            .check_store(&store.data_dir)
            .map_err(|error| ExecError::store(&error))?;
        if !proof.matches_claim(&self.claim, &self.journal_claim) {
            return Err(deferred());
        }
        let (instance, effect) = self.claim.effect();
        let request_id = format!("exec-interrupted-{effect}-{}", self.claim.run_id());
        if store.state.execution.claim_for(instance, effect) != Some(&self.claim) {
            return store
                .replay_execution_settlement(&self.claim, Settlement::Interrupted, &request_id)
                .map_err(|error| ExecError::store(&error))?
                .ok_or_else(deferred);
        }
        let hash = store
            .current_execution_claim_hash(&self.claim)
            .map_err(|error| ExecError::store(&error))?;
        if hash != self.journal_claim {
            return Err(deferred());
        }
        let outcome = StoppedOutcome::from_value(&Value::Obj(BTreeMap::from([(
            "status".into(),
            Value::Str("interrupted".into()),
        )])))
        .map_err(|_| deferred())?;
        match store.state.execution.stopped_for(instance, effect) {
            Some(stopped) if stopped.outcome().status() != "interrupted" => return Err(deferred()),
            Some(_) => {}
            None => {
                store
                    .stop_execution_on(
                        clock,
                        ExecutionStopRequest {
                            claim: &self.claim,
                            proof,
                            outcome: &outcome,
                            request_id: &format!("exec-stop-{effect}-{}", self.claim.run_id()),
                            expected_seq: None,
                        },
                    )
                    .map_err(|error| ExecError::store(&error))?;
            }
        }
        store
            .settle_execution_on(
                clock,
                ExecutionSettleRequest {
                    claim: &self.claim,
                    disposition: Settlement::Interrupted,
                    request_id: &request_id,
                    expected_seq: None,
                },
            )
            .map_err(|error| ExecError::store(&error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn original_claim() -> Claim {
        let fixture = fsm_core::json::parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &fsm_core::json::JsonLimits::DEFAULT,
        )
        .unwrap();
        Claim::from_value(fixture.get("claim").unwrap()).unwrap()
    }

    #[test]
    fn native_proof_shutdown_poll_and_inventory_wait_for_the_original_reader_join() {
        use crate::run::native_client::{proof_worker::FixtureHook, test_support, worker};
        use std::sync::{Arc, mpsc};
        let budget = Arc::new(worker::Budget::default());
        let _scope = worker::Scope::enter(Some(&budget));
        let _startup = test_support::completed_transport();
        let claim = original_claim();
        let domain = claim.domain().to_value();
        let namespace = domain.get("namespace").and_then(Value::as_str).unwrap();
        let generation = domain
            .get("generation")
            .and_then(Value::as_num)
            .unwrap()
            .parse()
            .unwrap();
        let message = Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-request/1".into())),
            ("action".into(), Value::Str("close-claimed".into())),
            (
                "payload".into(),
                Value::Obj(BTreeMap::from([
                    (
                        "format".into(),
                        Value::Str("fsm.native-claim-binding/1".into()),
                    ),
                    ("claim".into(), claim.to_value()),
                    (
                        "journal_claim".into(),
                        Value::Str(format!("sha256:{}", "a".repeat(64))),
                    ),
                ])),
            ),
        ]));
        let request =
            NativeRequest::start(namespace, generation, &message, Duration::from_secs(10)).unwrap();
        // This directly retained poll fixture bypasses startup routing, writes
        // no native artifact and can obtain no receipt from this absent path.
        let mut shutdown = NativeShutdown {
            request,
            claim,
            journal_claim: format!("sha256:{}", "a".repeat(64)),
            store_directory: PathBuf::from("/fsm-proof-fixture-no-authority"),
            receipt: PathBuf::from("/fsm-proof-fixture-no-receipt"),
            deadline: Instant::now() + Duration::from_secs(10),
            response_checked: false,
            proof: None,
            verification: None,
            error: None,
        };
        let (entered, arrived) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let _hook = FixtureHook::install(move || {
            entered.send(std::thread::current().id()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let until = Instant::now() + Duration::from_secs(5);
        let reader = loop {
            if let Ok(reader) = arrived.try_recv() {
                break reader;
            }
            assert!(shutdown.poll().unwrap().is_none());
            assert!(Instant::now() < until, "original shutdown reader missing");
            std::thread::sleep(Duration::from_millis(1));
        };
        let before = shutdown.progress();
        let pending = shutdown.poll();
        let retired = shutdown.reap().unwrap();
        let reserved = budget.reserved();
        release.send(()).unwrap();
        while !shutdown.reap().unwrap() {
            assert!(
                Instant::now() < until,
                "original shutdown reader not joined"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let refused = shutdown.poll().is_err();
        assert_ne!(reader, std::thread::current().id());
        assert!(before.stdout_eof && before.stderr_eof && !before.reaped);
        assert!(matches!(pending, Ok(None)) && !retired);
        assert!(refused && shutdown.proof.is_none());
        assert!(shutdown.progress().is_retired());
        assert_eq!(reserved, 1);
        assert_eq!(budget.reserved(), 1);
        drop(shutdown);
        assert_eq!(budget.reserved(), 0);
    }

    #[test]
    fn memory_shutdown_refuses_before_dispatch_and_preserves_the_store() {
        let store = Store::open_memory().unwrap();
        let original_state = store.state.clone();
        let original_records = store.records.clone();
        let original_head = (store.journal.last_seq, store.journal.last_hash.clone());
        assert_eq!(
            NativeShutdown::start(&store, &original_claim(), Duration::from_secs(1))
                .err()
                .unwrap(),
            "native shutdown requires a supported verified durable store",
        );
        assert!(fsm_store::snapshot::store_states_eq(
            &store.state,
            &original_state
        ));
        assert_eq!(store.records, original_records);
        assert_eq!(
            (store.journal.last_seq, store.journal.last_hash.clone()),
            original_head
        );
    }

    #[test]
    fn zero_shutdown_deadline_refuses_before_claim_lookup_or_native_dispatch() {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
            return;
        }
        let cache = std::env::var_os("TMPDIR").expect("dedicated test cache required");
        let directory =
            PathBuf::from(cache).join(format!("fsm-shutdown-zero-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let store = Store::open(&directory).unwrap();
        let original_records = store.records.clone();
        let original_head = (store.journal.last_seq, store.journal.last_hash.clone());
        assert_eq!(
            NativeShutdown::start(&store, &original_claim(), Duration::ZERO)
                .err()
                .unwrap(),
            "native shutdown deadline invalid"
        );
        assert_eq!(store.records, original_records);
        assert_eq!(
            (store.journal.last_seq, store.journal.last_hash.clone()),
            original_head
        );
        drop(store);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn only_closed_successful_null_response_can_precede_receipt_verification() {
        let mut fields = BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-response/1".into())),
            ("ok".into(), Value::Bool(true)),
            ("result".into(), Value::Null),
        ]);
        assert!(validate_response(&Value::Obj(fields.clone())).is_ok());
        fields.insert("ok".into(), Value::Bool(false));
        assert!(validate_response(&Value::Obj(fields.clone())).is_err());
        fields.insert("ok".into(), Value::Bool(true));
        fields.insert("result".into(), Value::Bool(true));
        assert!(validate_response(&Value::Obj(fields.clone())).is_err());
        fields.insert("result".into(), Value::Null);
        fields.insert("proof".into(), Value::Bool(true));
        assert!(validate_response(&Value::Obj(fields)).is_err());
    }
}
