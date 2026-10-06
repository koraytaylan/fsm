//! Protected closure requests retained independently of execution transport.

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
    error: Option<String>,
}

impl NativeShutdown {
    /// Request native closure without requiring or acquiring the writer lease.
    ///
    /// The snapshot must contain the exact original unresolved claim and its
    /// verified record hash; copied stores and replaced authorities refuse.
    /// This operation never binds or launches work and never settles ownership.
    pub fn start(store: &Store, claim: &Claim, timeout: Duration) -> Result<Self, String> {
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
            ("action".into(), Value::Str("close-claimed".into())),
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
                let _ = self.request.cancel();
                Err(error)
            }
        }
    }

    fn poll_once(&mut self) -> Result<Option<VerifiedClosure>, String> {
        if Instant::now() >= self.deadline {
            return Err("native shutdown deadline; original claim remains unresolved".into());
        }
        if !self.response_checked {
            let Some(response) = self.request.poll()? else {
                return Ok(None);
            };
            validate_response(&response)?;
            self.response_checked = true;
        }
        check_claim_store(&self.store_directory, &self.claim)?;
        let proof = VerifiedClosure::read(&self.receipt).map_err(|error| error.message)?;
        proof
            .check_store(&self.store_directory)
            .map_err(|error| error.message)?;
        if !proof.matches_claim(&self.claim, &self.journal_claim) {
            return Err("native shutdown receipt differs from original claim".into());
        }
        if Instant::now() >= self.deadline {
            return Err("native shutdown deadline; original claim remains unresolved".into());
        }
        Ok(Some(proof))
    }

    /// Observe helper retirement without asserting closure or releasing ownership.
    pub fn reap(&mut self) -> Result<bool, String> {
        self.request.reap()
    }

    /// Inspect identifier-free helper facts; these never prove domain closure.
    pub fn progress(&self) -> NativeHelperProgress {
        self.request.progress()
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
