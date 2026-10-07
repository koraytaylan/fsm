//! The one component that writes.
//!
//! Split from the runner when `run.rs` passed the workspace's thousand-line
//! ceiling, along the seam its own module doc already named: the runner owns
//! no policy and spawns processes, and this owns no processes and maps an
//! outcome onto journaled reality through the store's own idempotent mutators.
//! Native startup also checks the writer-held claim before handing an owned
//! transport run to the execution host; the pipeline retains no process.
//!
//! Plan 0016 task 7702.

use fsm_core::json::Value;
use fsm_store::clock::Clock;
use fsm_store::store::Store;

use crate::config::{Advance, HandlerSpec};
use crate::effect::PendingEffect;
use crate::error::ExecError;
use crate::rid::{ack_rid, attempt_rid, event_rid, poll_rid};

use super::{Exhaustion, RunOutcome};

/// What settling one outcome did to the journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettleOutcome {
    /// Acked, and the declared advance event was sent.
    Advanced,
    /// Acked, and no advance was sent — none declared, or not enabled.
    AckedNoAdvance,
    /// Another path had already settled the effect.
    AlreadySettled,
}

/// The one component that writes.
///
/// It holds no state: everything it needs to decide is either journaled or
/// handed to it, which is why a fresh `Pipeline` after a restart behaves
/// exactly like the one that died.
pub struct Pipeline;

impl Pipeline {
    /// Durably claim a prepared native domain before binding or handler launch.
    #[cfg(target_os = "linux")]
    pub fn claim_native(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        request: fsm_store::store::ExecutionClaimRequest<'_>,
    ) -> Result<Value, ExecError> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native claim requires a supported healthy durable writer",
            ));
        }
        store
            .claim_execution_on(clock, request)
            .map_err(|error| ExecError::store(&error))
    }

    /// Derive and durably claim the original handler contract for a journal effect.
    ///
    /// This performs no helper startup; a host prepares its domain first, then
    /// claims under the writer before calling NativeExecution::start. Refusal
    /// never authorizes a direct-child fallback or clears existing ownership.
    #[cfg(target_os = "linux")]
    pub fn claim_native_handler(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect_id: &str,
        handler: &HandlerSpec,
        domain: &fsm_core::record::execution::NativeDomain,
        request_id: &str,
    ) -> Result<fsm_core::record::execution::Claim, ExecError> {
        use fsm_core::record::execution::RetryPolicy;
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native claim requires a supported healthy durable writer",
            ));
        }
        let effect = crate::effect::resolve(store, effect_id)?;
        if handler.effect != effect.effect_name {
            return Err(ExecError::new(
                "exec/config",
                "native handler does not match the journal-derived effect",
            ));
        }
        let (fingerprint, contract) = handler.checked_contract()?;
        HandlerSpec::from_contract(&contract, &fingerprint)?;
        let retry = RetryPolicy::from_value(contract.get("retry").ok_or_else(|| {
            ExecError::new("exec/config", "native original retry policy is missing")
        })?)
        .map_err(|error| ExecError::new("exec/config", error.to_string()))?;
        self.claim_native(
            store,
            clock,
            fsm_store::store::ExecutionClaimRequest {
                instance_id: &effect.instance_id,
                effect_id,
                handler_fingerprint: &fingerprint,
                domain,
                retry: &retry,
                request_id,
                expected_seq: None,
            },
        )?;
        let claim = store
            .state
            .execution
            .claim_for(&effect.instance_id, effect_id)
            .cloned()
            .ok_or_else(|| {
                ExecError::new(
                    "exec/inflight_deferred",
                    "original native claim is no longer owned",
                )
            })?;
        let material = claim.to_value();
        if claim.domain() != domain
            || material.get("retry") != Some(&retry.to_value())
            || material.get("handler_fingerprint") != Some(&Value::Str(fingerprint))
        {
            return Err(ExecError::new(
                "exec/inflight_deferred",
                "owned native claim differs from the original contract",
            ));
        }
        store
            .current_execution_claim_hash(&claim)
            .map_err(|error| ExecError::store(&error))?;
        Ok(claim)
    }

    /// Recheck current durable ownership under a writer before starting binding.
    ///
    /// The returned run owns helper I/O independently of the writer; the host
    /// must retain uncertain ownership and persist authenticated completion.
    /// Root authority repeats admission checks before actual handler entry.
    #[cfg(target_os = "linux")]
    pub fn start_native(
        &mut self,
        store: &mut Store,
        claim: &fsm_core::record::execution::Claim,
        timeout: std::time::Duration,
    ) -> Result<super::native_client::NativeRun, ExecError> {
        let hash = Self::native_launch_hash(store, claim)?;
        super::native_client::NativeRun::start(claim, &hash, timeout).map_err(|error| {
            ExecError::new("exec/spawn", error)
                .hint("retain the durable claim and reconcile native closure before retrying")
        })
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn native_launch_hash(
        store: &Store,
        claim: &fsm_core::record::execution::Claim,
    ) -> Result<String, ExecError> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native launch requires a supported healthy durable writer",
            ));
        }
        let hash = store
            .current_execution_claim_hash(claim)
            .map_err(|error| ExecError::store(&error))?;
        let (instance_id, effect_id) = claim.effect();
        if store.state.execution.admission() != fsm_core::record::execution::Admission::Enabled
            || store
                .state
                .execution
                .stopped_for(instance_id, effect_id)
                .is_some()
            || !store
                .state
                .instances
                .get(instance_id)
                .is_some_and(|instance| {
                    instance.status == fsm_core::machine::Status::Running
                        && instance.pending.iter().any(|pending| pending == effect_id)
                })
        {
            return Err(ExecError::new(
                "exec/inflight_deferred",
                "native claim is not eligible for launch",
            ));
        }
        Ok(hash)
    }

    /// Recover original completion from a durable snapshot, independently of a writer.
    ///
    /// The retained current claim/hash must match; recovery does not require
    /// pending launch eligibility or consult a current handler table, and does
    /// not stop/settle the journal or authorize a replacement handler.
    #[cfg(target_os = "linux")]
    pub fn recover_native(
        &mut self,
        store: &Store,
        claim: &fsm_core::record::execution::Claim,
        timeout: std::time::Duration,
    ) -> Result<super::native_client::NativeRun, ExecError> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native recovery requires a supported durable snapshot",
            ));
        }
        let hash = store
            .current_execution_claim_hash(claim)
            .map_err(|error| ExecError::store(&error))?;
        super::native_client::NativeRun::recover(claim, &hash, timeout).map_err(|error| {
            ExecError::new("exec/spawn", error)
                .hint("retain the durable claim when original completion cannot be recovered")
        })
    }

    /// Atomically consume a durable stopped result under the store writer.
    pub fn settle_stopped(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        claim: &fsm_core::record::execution::Claim,
        disposition: fsm_core::record::execution::Settlement,
        request_id: &str,
    ) -> Result<Value, ExecError> {
        store
            .settle_execution_on(
                clock,
                fsm_store::store::ExecutionSettleRequest {
                    claim,
                    disposition,
                    request_id,
                    expected_seq: None,
                },
            )
            .map_err(|error| ExecError::store(&error))
    }

    /// Atomically settle a matching stopped result using its original claim policy.
    ///
    /// Requires a healthy durable writer and the exactly retained stopped owner;
    /// it consults no current table and sends no outcome event. A host recovering
    /// a committed transaction uses exact settlement replay instead of reselecting.
    #[cfg(target_os = "linux")]
    pub fn settle_native_stopped(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        claim: &fsm_core::record::execution::Claim,
        completion: &super::native_client::NativeCompletion,
    ) -> Result<Value, ExecError> {
        use fsm_core::record::execution::{PendingEffect, Settlement};
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native settlement requires a supported healthy durable writer",
            ));
        }
        let unproven = || {
            ExecError::new(
                "exec/inflight_deferred",
                "matching stopped native ownership is not proven",
            )
        };
        if !completion.matches_original(claim) {
            return Err(unproven());
        }
        completion
            .proof()
            .check_store(&store.data_dir)
            .map_err(|error| ExecError::store(&error))?;
        let hash = store
            .current_execution_claim_hash(claim)
            .map_err(|error| ExecError::store(&error))?;
        let (instance_id, effect_id) = claim.effect();
        if !completion.proof().matches_claim(claim, &hash)
            || store
                .state
                .execution
                .stopped_for(instance_id, effect_id)
                .is_none_or(|stopped| stopped.outcome() != completion.stopped_outcome())
        {
            return Err(unproven());
        }
        let instance = store
            .state
            .instances
            .get(instance_id)
            .ok_or_else(unproven)?;
        let pending = if instance.status == fsm_core::machine::Status::Running
            && instance.pending.iter().any(|effect| effect == effect_id)
        {
            PendingEffect::Present
        } else {
            PendingEffect::Absent
        };
        let disposition = store
            .state
            .execution
            .settlement_for(claim, pending)
            .map_err(|_| unproven())?;
        let request_id = match disposition {
            Settlement::Acked => ack_rid(effect_id),
            Settlement::Attempted => {
                let material = claim.to_value();
                let attempt = material
                    .get("attempt")
                    .and_then(Value::as_num)
                    .and_then(|raw| raw.parse::<u32>().ok())
                    .ok_or_else(unproven)?;
                attempt_rid(effect_id, attempt)
            }
            Settlement::Interrupted => format!("exec-interrupted-{effect_id}-{}", claim.run_id()),
        };
        let advance = if completion.stopped_outcome().status() == "ok" {
            completion.handler().on_ok.as_ref()
        } else {
            completion.handler().on_failed.as_ref()
        };
        if disposition == Settlement::Acked && advance.is_some() {
            let sequence = store.journal.last_seq.checked_add(1).ok_or_else(unproven)?;
            let handoff = fsm_core::record::execution::AcknowledgedHandoff::new(
                claim,
                &hash,
                &completion.handler().contract_value(),
                completion.stopped_outcome(),
                &request_id,
                sequence,
            )
            .map_err(|_| unproven())?;
            store
                .settle_execution_with_handoff_on(
                    clock,
                    fsm_store::store::ExecutionSettleRequest {
                        claim,
                        disposition,
                        request_id: &request_id,
                        expected_seq: None,
                    },
                    &handoff,
                )
                .map_err(|error| ExecError::store(&error))
        } else {
            self.settle_stopped(store, clock, claim, disposition, &request_id)
        }
    }

    /// Persist verified native completion while retaining claim ownership.
    #[cfg(target_os = "linux")]
    pub fn stop_native(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        claim: &fsm_core::record::execution::Claim,
        completion: &super::native_client::NativeCompletion,
        request_id: &str,
    ) -> Result<Value, ExecError> {
        store
            .stop_execution_on(
                clock,
                fsm_store::store::ExecutionStopRequest {
                    claim,
                    proof: completion.proof(),
                    outcome: completion.stopped_outcome(),
                    request_id,
                    expected_seq: None,
                },
            )
            .map_err(|error| ExecError::store(&error))
    }

    /// Ack one outcome, then send the declared advance event when the engine
    /// says that event is enabled.
    ///
    /// Ack first, always. The ack is what clears the effect from the outbox,
    /// so a kill between the two writes leaves a journal that says "this ran,
    /// its advance did not" — which the next executor can read and finish.
    pub fn settle(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect: &PendingEffect,
        outcome: RunOutcome,
        handler: &HandlerSpec,
        exhausted: Option<Exhaustion>,
    ) -> Result<SettleOutcome, ExecError> {
        let acked = if outcome.succeeded() { "ok" } else { "failed" };
        // Exhaustion is a failure like any other from here on: same ack, same
        // outcome word, same declared advance. Only the `result` says how it
        // got here, which is what leaves an existing `on_failed` path working
        // unchanged and makes the dead-letter report derivable.
        let result = match exhausted {
            Some(exhaustion) => outcome.exhausted_ack_result(exhaustion.attempts, exhaustion.class),
            None => outcome.ack_result(),
        };
        let ack_seq = match store.ack_effect_outcome_on(
            clock,
            &effect.instance_id,
            &effect.effect_id,
            &ack_rid(&effect.effect_id),
            acked,
            Some(result),
        ) {
            Ok(response) => response
                .get("seq")
                .and_then(Value::as_num)
                .and_then(|seq| seq.parse::<u64>().ok()),
            // The store journals a `request_rejected` record for an ack of an
            // effect that is not pending and returns this exact code. Another
            // path already settled it; that is benign, and the rejection also
            // claims the derived key so a later re-issue replays it.
            Err(error) if error.code == "req/field_unknown" => {
                return Ok(SettleOutcome::AlreadySettled);
            }
            Err(error) => return Err(ExecError::store(&error)),
        };
        let declared = if outcome.succeeded() {
            handler.on_ok.as_ref()
        } else {
            handler.on_failed.as_ref()
        };
        // No declared advance is a deliberate stall, not an omission: the
        // instance waits for a deadline or an external event.
        let Some(advance) = declared else {
            return Ok(SettleOutcome::AckedNoAdvance);
        };
        self.advance(
            store,
            clock,
            &effect.effect_id,
            &effect.instance_id,
            advance,
            ack_seq,
        )
    }

    /// Journal one failed attempt, leaving the effect pending.
    ///
    /// The counterpart to [`Pipeline::settle`] for a failure the policy will
    /// try again: nothing is acked, nothing is advanced, and the effect stays
    /// in the outbox where the next scan finds it. The record is the whole
    /// point — it is what makes the count and the backoff deadline survive a
    /// restart, since a process that dies between the failure and the retry
    /// remembers nothing.
    ///
    /// The run's capture goes into the record so an operator reading a
    /// dead letter can see why each earlier attempt failed, not only the last.
    ///
    /// `Ok(false)` means another writer had already settled the effect — the
    /// same benign race [`Pipeline::settle`] reports as
    /// [`SettleOutcome::AlreadySettled`].
    pub fn attempt(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect: &PendingEffect,
        outcome: &RunOutcome,
        attempt: u32,
    ) -> Result<bool, ExecError> {
        match store.attempt_effect_on(
            clock,
            &effect.instance_id,
            &effect.effect_id,
            &attempt_rid(&effect.effect_id, attempt),
            u64::from(attempt),
            Some(outcome.ack_result()),
        ) {
            Ok(_) => Ok(true),
            // The store journals a `request_rejected` for an attempt against
            // an effect that is not pending and returns this exact code,
            // exactly as it does for an ack of one.
            Err(error) if error.code == "req/field_unknown" => Ok(false),
            Err(error) => Err(ExecError::store(&error)),
        }
    }

    /// Send an advance for an effect already acknowledged in a previous life.
    ///
    /// The ack is already in the journal, so there is no `expect_seq` to hold
    /// anything still; the derived key makes a send that did land replay as
    /// `duplicate: true` rather than transition a second time.
    pub fn advance_only(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect_id: &str,
        instance_id: &str,
        advance: &Advance,
    ) -> Result<SettleOutcome, ExecError> {
        self.advance(store, clock, effect_id, instance_id, advance, None)
    }

    /// Resume the original outcome event only after exact terminal settlement replay.
    ///
    /// Uses the recovered checked contract, never a current handler table;
    /// missing/conflicting/archived settlement evidence cannot authorize an event.
    #[cfg(target_os = "linux")]
    pub fn advance_native_settled(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        claim: &fsm_core::record::execution::Claim,
        completion: &super::native_client::NativeCompletion,
        settlement_request_id: &str,
    ) -> Result<SettleOutcome, ExecError> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native advance requires a supported healthy durable writer",
            ));
        }
        let unproven = || {
            ExecError::new(
                "exec/inflight_deferred",
                "original native terminal settlement is not proven",
            )
        };
        if !completion.matches_original(claim)
            || completion.stopped_outcome().status() == "interrupted"
        {
            return Err(unproven());
        }
        completion
            .proof()
            .check_store(&store.data_dir)
            .map_err(|error| ExecError::store(&error))?;
        let response = store
            .replay_execution_settlement(
                claim,
                fsm_core::record::execution::Settlement::Acked,
                settlement_request_id,
            )
            .map_err(|error| ExecError::store(&error))?
            .ok_or_else(unproven)?;
        let body = response.get("execution").ok_or_else(unproven)?;
        let outcome = if completion.stopped_outcome().status() == "ok" {
            "ok"
        } else {
            "failed"
        };
        let (instance_id, effect_id) = claim.effect();
        if body.get("disposition").and_then(Value::as_str) != Some("acked")
            || body.get("instance_id").and_then(Value::as_str) != Some(instance_id)
            || body.get("effect_id").and_then(Value::as_str) != Some(effect_id)
            || body.get("run_id") != Some(&Value::Num(claim.run_id().to_string()))
            || body.get("outcome").and_then(Value::as_str) != Some(outcome)
            || body.get("result") != completion.stopped_outcome().result()
        {
            return Err(unproven());
        }
        let handler = completion.handler();
        let advance = if outcome == "ok" {
            handler.on_ok.as_ref()
        } else {
            handler.on_failed.as_ref()
        };
        let Some(advance) = advance else {
            return Ok(SettleOutcome::AckedNoAdvance);
        };
        let seq = response
            .get("seq")
            .and_then(Value::as_num)
            .and_then(|raw| raw.parse().ok());
        if let Some(material) = body.get("handoff") {
            let original = fsm_core::record::execution::AcknowledgedHandoff::from_value(material)
                .map_err(|_| unproven())?;
            if original.claim() != claim
                || original.handler_contract() != &handler.contract_value()
                || original.outcome() != completion.stopped_outcome()
                || original.acknowledgement_request_id() != settlement_request_id
                || Some(original.acknowledgement_seq()) != seq
                || !completion
                    .proof()
                    .matches_claim(claim, original.original_claim_hash())
            {
                return Err(unproven());
            }
            match store
                .state
                .execution_handoffs
                .outstanding()
                .find(|handoff| handoff.claim().run_id() == claim.run_id())
            {
                Some(current) if current != &original => return Err(unproven()),
                // The exact replayed acknowledgement installed this obligation;
                // only a matching accepted-event fold can have removed it.
                None => return Ok(SettleOutcome::AlreadySettled),
                Some(_) => {}
            }
            return self.deliver_native_handoff(store, clock, &original, advance);
        }
        self.advance(store, clock, effect_id, instance_id, advance, seq)
    }

    /// Deliver only an exact durable event obligation; never execute or acknowledge.
    #[cfg(target_os = "linux")]
    pub(crate) fn advance_native_handoff(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        handoff: &fsm_core::record::execution::AcknowledgedHandoff,
    ) -> Result<SettleOutcome, ExecError> {
        if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
            || store.journal.is_memory()
            || store.journal.is_read_only()
            || store.journal.poisoned
        {
            return Err(ExecError::new(
                "exec/mode",
                "native handoff requires a supported healthy durable writer",
            ));
        }
        let unproven = || {
            ExecError::new(
                "exec/inflight_deferred",
                "original native event handoff is not proven",
            )
        };
        if !store
            .state
            .execution_handoffs
            .outstanding()
            .any(|original| original == handoff)
        {
            return Err(unproven());
        }
        super::native_client::check_claim_store(&store.data_dir, handoff.claim())
            .map_err(|_| unproven())?;
        let original = handoff.claim().to_value();
        let fingerprint = original
            .get("handler_fingerprint")
            .and_then(Value::as_str)
            .ok_or_else(unproven)?;
        let handler = HandlerSpec::from_contract(handoff.handler_contract(), fingerprint)?;
        let advance = if handoff.outcome().status() == "ok" {
            handler.on_ok.as_ref()
        } else {
            handler.on_failed.as_ref()
        }
        .ok_or_else(unproven)?;
        let (_, effect) = handoff.claim().effect();
        if event_rid(effect, &advance.event) != handoff.event_request_id() {
            return Err(unproven());
        }
        self.deliver_native_handoff(store, clock, handoff, advance)
    }

    #[cfg(target_os = "linux")]
    fn deliver_native_handoff(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        handoff: &fsm_core::record::execution::AcknowledgedHandoff,
        advance: &Advance,
    ) -> Result<SettleOutcome, ExecError> {
        let (instance, effect) = handoff.claim().effect();
        let head = store.journal.last_seq;
        self.advance(store, clock, effect, instance, advance, Some(head))?;
        // A duplicate rejected/ignored response is not accepted-event proof.
        // Only the store's verified fold can consume the durable obligation.
        if store
            .state
            .execution_handoffs
            .outstanding()
            .any(|original| original == handoff)
        {
            Ok(SettleOutcome::AckedNoAdvance)
        } else {
            Ok(SettleOutcome::Advanced)
        }
    }

    /// Poll one due deadline under a derived key.
    ///
    /// A `NotDue` observation is journaled and claims its key, exactly as SPEC
    /// describes, so a repeat of the same observation replays rather than
    /// polling again.
    pub fn poll(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        instance_id: &str,
        deadline: &str,
        due_ms: i64,
    ) -> Result<Value, ExecError> {
        store
            .poll_instance_deadline_on(
                clock,
                instance_id,
                &poll_rid(instance_id, deadline, due_ms),
                None,
            )
            .map_err(|error| ExecError::store(&error))
    }

    fn advance(
        &mut self,
        store: &mut Store,
        clock: &mut dyn Clock,
        effect_id: &str,
        instance_id: &str,
        advance: &Advance,
        expect_seq: Option<u64>,
    ) -> Result<SettleOutcome, ExecError> {
        if !advance_is_enabled(store, instance_id, advance)? {
            return Ok(SettleOutcome::AckedNoAdvance);
        }
        let request_id = event_rid(effect_id, &advance.event);
        match send(store, clock, instance_id, advance, &request_id, expect_seq) {
            Ok(()) => Ok(SettleOutcome::Advanced),
            // Something else advanced the instance between the ack and the
            // send. SPEC excludes `expect_seq` from the fingerprint and leaves
            // the key unconsumed on a mismatch, so the same request_id is
            // retried against the current seq.
            Err(error) if error.code == "req/seq_mismatch" => {
                let current = store.journal.last_seq;
                if !advance_is_enabled(store, instance_id, advance)? {
                    return Ok(SettleOutcome::AckedNoAdvance);
                }
                send(
                    store,
                    clock,
                    instance_id,
                    advance,
                    &request_id,
                    Some(current),
                )
                .map(|()| SettleOutcome::Advanced)
                .map_err(|error| ExecError::store(&error))
            }
            Err(error) => Err(ExecError::store(&error)),
        }
    }
}

fn send(
    store: &mut Store,
    clock: &mut dyn Clock,
    instance_id: &str,
    advance: &Advance,
    request_id: &str,
    expect_seq: Option<u64>,
) -> Result<(), fsm_store::store::ErrorObj> {
    let stamps: Vec<&str> = advance.stamps.iter().map(String::as_str).collect();
    // The store stamps into the payload it is given, so each attempt starts
    // from the table's own value. The request fingerprint is taken before
    // stamping, which is what lets a re-issue after a restart match even
    // though the stamped timestamp differs.
    let mut payload = advance.payload.clone();
    store
        .send_event_stamp_on(
            clock,
            instance_id,
            &advance.event,
            &mut payload,
            request_id,
            expect_seq,
            &stamps,
        )
        .map(|_| ())
}

/// Whether the engine would accept this advance right now.
///
/// Two conditions, and neither is redundant. Presence in `enabled_events` is
/// not a gate at all — every declared event appears there with a status. And
/// the status alone is not enough either, because `enabled_events` reasons
/// from the configuration rather than the lifecycle: cancelling an instance
/// leaves its configuration in place, so a cancelled instance still reports
/// its events as enabled and only `step` refuses — by journaling an
/// `event_rejected` that burns the derived key for good.
fn advance_is_enabled(
    store: &Store,
    instance_id: &str,
    advance: &Advance,
) -> Result<bool, ExecError> {
    let view = store
        .instance_view(instance_id, None, None)
        .map_err(|error| ExecError::store(&error))?;
    if view.get("status").and_then(Value::as_str) != Some("running") {
        return Ok(false);
    }
    let Some(events) = view.get("enabled_events").and_then(Value::as_arr) else {
        return Ok(false);
    };
    let Some(entry) = events
        .iter()
        .find(|event| event.get("event").and_then(Value::as_str) == Some(advance.event.as_str()))
    else {
        return Ok(false);
    };
    Ok(match entry.get("status").and_then(Value::as_str) {
        Some("enabled") => true,
        // A guard that reads the payload cannot be decided without one, so an
        // advance that carries fields is worth attempting and one that carries
        // nothing is not.
        Some("depends_on_payload") => {
            !advance.stamps.is_empty()
                || advance
                    .payload
                    .as_obj()
                    .is_some_and(|fields| !fields.is_empty())
        }
        _ => false,
    })
}
