//! Exclusive execution mutators: validate, durably append, then publish once.

use std::collections::BTreeMap;

use fsm_core::hashes::{STATE_FORMAT, request_fp, state_hash};
use fsm_core::json::Value;
use fsm_core::record::execution::{
    Claim, NativeDomain, PendingEffect, RetryPolicy, Settlement, ShapeError, Stopped,
    StoppedOutcome,
};
use fsm_core::record::{Record, RecordKind, seal};
use fsm_core::replay::{NopSink, fold_from};

use super::{ErrorObj, Store, VerifiedClosure, VerifiedQuiescence};
use crate::clock::Clock;

#[cfg(test)]
#[path = "execution_tests.rs"]
mod tests;

/// Caller content for an allocation; run ID and attempt are derived durably.
pub struct ExecutionClaimRequest<'a> {
    pub instance_id: &'a str,
    pub effect_id: &'a str,
    pub handler_fingerprint: &'a str,
    pub retry: &'a RetryPolicy,
    pub domain: &'a NativeDomain,
    pub request_id: &'a str,
    pub expected_seq: Option<u64>,
}

/// A stopped result needs an opaque native proof for the original claim hash.
pub struct ExecutionStopRequest<'a> {
    pub claim: &'a Claim,
    pub proof: &'a VerifiedClosure,
    pub outcome: &'a StoppedOutcome,
    pub request_id: &'a str,
    pub expected_seq: Option<u64>,
}

/// Single consumption of an immutable stopped result and its instance effect.
pub struct ExecutionSettleRequest<'a> {
    pub claim: &'a Claim,
    pub disposition: Settlement,
    pub request_id: &'a str,
    pub expected_seq: Option<u64>,
}

pub(super) fn response(record: &Record, duplicate: bool) -> Value {
    Value::Obj(BTreeMap::from([
        ("seq".into(), Value::Num(record.seq.to_string())),
        ("duplicate".into(), Value::Bool(duplicate)),
        ("execution".into(), record.body.clone()),
    ]))
}

fn refusal(error: ShapeError) -> ErrorObj {
    let code = match error.0 {
        "quarantined" => "store/execution_quarantined",
        "owned" | "already_stopped" | "not_stopped" => "store/execution_owned",
        "run_exhausted" => "store/execution_exhausted",
        "contract" | "retry_contract" | "claim_contract" => "store/execution_contract",
        "retry_ineligible" | "retry_class" | "retry_exhausted" | "retry_due" | "attempt" => {
            "store/execution_retry"
        }
        "disposition" => "store/execution_disposition",
        "bytes" | "depth" | "entries" => "store/execution_limit",
        _ => "store/execution_stale",
    };
    ErrorObj::new(code, format!("execution transition refused: {}", error.0))
}

impl Store {
    fn execution_precondition(&self, expected: Option<u64>) -> Result<(), ErrorObj> {
        if expected.is_some_and(|seq| seq != self.journal.last_seq) {
            return Err(ErrorObj::new(
                "store/execution_stale",
                "execution observation is not the current journal head",
            ));
        }
        Ok(())
    }

    fn execution_request(
        &mut self,
        request_id: &str,
        operation: &str,
        material: &BTreeMap<String, Value>,
    ) -> Result<Option<Result<Value, ErrorObj>>, ErrorObj> {
        self.ensure_writable()?;
        if request_id.len() > 4096 {
            return Err(ErrorObj::new(
                "store/execution_limit",
                "execution request ID exceeds 4 KiB",
            ));
        }
        self.claim_request(request_id, request_fp(operation, material))
    }

    fn execution_pending(&self, claim: &Claim) -> Result<PendingEffect, ErrorObj> {
        let (instance_id, effect_id) = claim.effect();
        let instance = self.state.instances.get(instance_id).ok_or_else(|| {
            ErrorObj::new("store/execution_stale", "execution instance does not exist")
        })?;
        Ok(
            if instance.status == fsm_core::machine::Status::Running
                && instance.pending.iter().any(|pending| pending == effect_id)
            {
                PendingEffect::Present
            } else {
                PendingEffect::Absent
            },
        )
    }

    fn append_execution(
        &mut self,
        clock: &mut dyn Clock,
        kind: RecordKind,
        mut body: BTreeMap<String, Value>,
        request_id: &str,
        timestamp: i64,
    ) -> Result<Value, ErrorObj> {
        body.insert("request_id".into(), Value::Str(request_id.into()));
        let fingerprint = self.pending_fp.clone().ok_or_else(|| {
            ErrorObj::new(
                "store/execution_contract",
                "execution request fingerprint is missing",
            )
        })?;
        body.insert("request_fp".into(), Value::Str(fingerprint));
        let sequence = self.journal.last_seq.checked_add(1).ok_or_else(|| {
            ErrorObj::new("store/execution_exhausted", "journal sequence exhausted")
        })?;
        let provisional = seal(
            sequence,
            timestamp,
            kind,
            Value::Obj(body.clone()),
            &self.journal.last_hash,
        );
        let mut projected =
            fold_from(self.state.clone(), [provisional], &mut NopSink).map_err(|error| {
                ErrorObj::new(
                    "store/execution_stale",
                    format!("execution projection refused: {error:?}"),
                )
            })?;
        let record =
            self.append_at_with_root(kind, Value::Obj(body), clock.commit_reserved_ms(timestamp))?;
        projected.last_hash = record.hash.clone();
        self.state = projected;
        self.note_record(&record);
        for instance_id in fsm_core::record::instances_touched(&record) {
            self.history
                .entry(instance_id.into())
                .or_default()
                .push(record.seq);
        }
        let result = response(&record, false);
        self.commit_dedup(request_id, result.clone(), record.seq);
        self.finish_commit();
        Ok(result)
    }

    /// Allocate exclusive ownership of a still-pending effect under the writer.
    pub fn claim_execution_on(
        &mut self,
        clock: &mut dyn Clock,
        request: ExecutionClaimRequest<'_>,
    ) -> Result<Value, ErrorObj> {
        // Bound raw caller strings before creating their persisted copies.
        if [
            request.instance_id,
            request.effect_id,
            request.handler_fingerprint,
        ]
        .iter()
        .any(|field| field.len() > 4096)
        {
            return Err(ErrorObj::new(
                "store/execution_limit",
                "claim metadata exceeds 4 KiB",
            ));
        }
        let mut material = BTreeMap::from([
            ("instance_id".into(), Value::Str(request.instance_id.into())),
            ("effect_id".into(), Value::Str(request.effect_id.into())),
            (
                "handler_fingerprint".into(),
                Value::Str(request.handler_fingerprint.into()),
            ),
            ("retry".into(), request.retry.to_value()),
            ("domain".into(), request.domain.to_value()),
        ]);
        if let Some(replay) =
            self.execution_request(request.request_id, "execution_claimed", &material)?
        {
            return replay;
        }
        self.execution_precondition(request.expected_seq)?;
        material.insert(
            "run_id".into(),
            Value::Num(
                self.state
                    .execution
                    .next_run_id()
                    .map_err(refusal)?
                    .to_string(),
            ),
        );
        material.insert(
            "attempt".into(),
            Value::Num(
                (u64::from(
                    self.state
                        .execution
                        .failed_count(request.instance_id, request.effect_id),
                ) + 1)
                    .to_string(),
            ),
        );
        let claim = Claim::from_value(&Value::Obj(material.clone())).map_err(refusal)?;
        let timestamp = clock.reserve_ms();
        if self.attempts_for(request.instance_id, request.effect_id) != 0 {
            return Err(ErrorObj::new("store/execution_contract", "historical failed attempts have no immutable classified retry contract")
                .hint("resolve the legacy effect with the existing acknowledgement or cancellation API before admitting newly emitted work; do not reset its failed count"));
        }
        let mut execution = self.state.execution.clone();
        execution
            .claim(claim.clone(), self.execution_pending(&claim)?, timestamp)
            .map_err(refusal)?;
        self.append_execution(
            clock,
            RecordKind::ExecutionClaimed,
            material,
            request.request_id,
            timestamp,
        )
    }

    /// Persist verified closure while retaining ownership until settlement.
    pub fn stop_execution_on(
        &mut self,
        clock: &mut dyn Clock,
        request: ExecutionStopRequest<'_>,
    ) -> Result<Value, ErrorObj> {
        let (instance_id, effect_id) = request.claim.effect();
        let mut material = BTreeMap::from([
            ("claim".into(), request.claim.to_value()),
            ("closure".into(), request.proof.closure.to_value()),
            ("outcome".into(), request.outcome.to_value()),
        ]);
        if let Some(replay) =
            self.execution_request(request.request_id, "execution_stopped", &material)?
        {
            return replay;
        }
        self.execution_precondition(request.expected_seq)?;
        if self.state.execution.claim_for(instance_id, effect_id) != Some(request.claim) {
            return Err(ErrorObj::new(
                "store/execution_stale",
                "run or immutable claim does not match current ownership",
            ));
        }
        let original = self.execution_claim_hash(request.claim)?;
        if original != request.proof.journal_claim {
            return Err(ErrorObj::new(
                "store/execution_evidence",
                "closure receipt names a different original claim record",
            ));
        }
        let mut execution = self.state.execution.clone();
        execution
            .stop(
                request.claim,
                Stopped::new(request.proof.closure.clone(), request.outcome.clone()),
            )
            .map_err(refusal)?;
        material.remove("claim");
        material.insert("instance_id".into(), Value::Str(instance_id.into()));
        material.insert("effect_id".into(), Value::Str(effect_id.into()));
        material.insert(
            "run_id".into(),
            Value::Num(request.claim.run_id().to_string()),
        );
        material.insert(
            "handler_fingerprint".into(),
            request
                .claim
                .to_value()
                .get("handler_fingerprint")
                .cloned()
                .ok_or_else(|| {
                    ErrorObj::new("store/execution_contract", "claim fingerprint is missing")
                })?,
        );
        let timestamp = clock.reserve_ms();
        self.append_execution(
            clock,
            RecordKind::ExecutionStopped,
            material,
            request.request_id,
            timestamp,
        )
    }

    /// Read the original journal hash for an exactly matching current owner.
    ///
    /// Works on read-only handles and after sealing through the authenticated
    /// base index; performs no mutation, writer acquisition or handler lookup.
    /// Matching identifies original material but does not authorize launch or
    /// prove closure, pending eligibility or execution admission.
    pub fn current_execution_claim_hash(&self, claim: &Claim) -> Result<String, ErrorObj> {
        let (instance_id, effect_id) = claim.effect();
        if self.state.execution.claim_for(instance_id, effect_id) != Some(claim) {
            return Err(ErrorObj::new(
                "store/execution_stale",
                "run or immutable claim does not match current ownership",
            ));
        }
        self.execution_claim_hash(claim)
    }

    fn execution_claim_hash(&self, claim: &Claim) -> Result<String, ErrorObj> {
        if let Some(record) = self.records.iter().find(|record| {
            record.kind == RecordKind::ExecutionClaimed
                && record
                    .body
                    .get("run_id")
                    .and_then(Value::as_num)
                    .and_then(|raw| raw.parse::<u64>().ok())
                    == Some(claim.run_id())
        }) {
            return Ok(format!("sha256:{}", record.hash));
        }
        if self.sealed_open {
            if let Some(hash) = crate::base::open_from_base(&self.data_dir, &self.records)?
                .index
                .execution_claims
                .get(&claim.run_id())
            {
                return Ok(hash.clone());
            }
        }
        Err(ErrorObj::new(
            "store/execution_evidence",
            "original claim hash is unavailable",
        ))
    }

    /// Read a committed settlement under its exact original request fingerprint.
    ///
    /// Does not claim an unused key, acquire a writer, append, consume ownership
    /// or authorize launch; read-only handles are supported. A carried sealed
    /// key whose original response is unavailable retains the existing refusal.
    pub fn replay_execution_settlement(
        &mut self,
        claim: &Claim,
        disposition: Settlement,
        request_id: &str,
    ) -> Result<Option<Value>, ErrorObj> {
        if request_id.len() > 4096 {
            return Err(ErrorObj::new(
                "store/execution_limit",
                "execution request ID exceeds 4 KiB",
            ));
        }
        if self
            .state
            .dedup
            .get(request_id)
            .is_some_and(|slot| slot.fp.is_none())
        {
            return Err(ErrorObj::new(
                "store/execution_evidence",
                "settlement request has no original fingerprint",
            ));
        }
        let disposition = match disposition {
            Settlement::Acked => "acked",
            Settlement::Attempted => "attempted",
            Settlement::Interrupted => "interrupted",
        };
        let material = BTreeMap::from([
            ("claim".into(), claim.to_value()),
            ("disposition".into(), Value::Str(disposition.into())),
        ]);
        self.lookup_request(request_id, &request_fp("execution_settled", &material))?
            .transpose()
    }

    /// Consume a stopped result with its engine disposition in one record.
    pub fn settle_execution_on(
        &mut self,
        clock: &mut dyn Clock,
        request: ExecutionSettleRequest<'_>,
    ) -> Result<Value, ErrorObj> {
        let disposition = match request.disposition {
            Settlement::Acked => "acked",
            Settlement::Attempted => "attempted",
            Settlement::Interrupted => "interrupted",
        };
        let mut material = BTreeMap::from([
            ("claim".into(), request.claim.to_value()),
            ("disposition".into(), Value::Str(disposition.into())),
        ]);
        if let Some(replay) =
            self.execution_request(request.request_id, "execution_settled", &material)?
        {
            return replay;
        }
        self.execution_precondition(request.expected_seq)?;
        let timestamp = clock.reserve_ms();
        let mut execution = self.state.execution.clone();
        let stopped = execution
            .settle(
                request.claim,
                request.disposition,
                self.execution_pending(request.claim)?,
                timestamp,
            )
            .map_err(refusal)?;
        let (instance_id, effect_id) = request.claim.effect();
        material.remove("claim");
        material.insert("instance_id".into(), Value::Str(instance_id.into()));
        material.insert("effect_id".into(), Value::Str(effect_id.into()));
        material.insert(
            "run_id".into(),
            Value::Num(request.claim.run_id().to_string()),
        );
        let mut instance = self
            .state
            .instances
            .get(instance_id)
            .cloned()
            .ok_or_else(|| ErrorObj::new("store/execution_stale", "instance disappeared"))?;
        if request.disposition == Settlement::Acked {
            instance.pending.retain(|pending| pending != effect_id);
        }
        if request.disposition != Settlement::Interrupted {
            material.insert(
                "outcome".into(),
                Value::Str(
                    if stopped.outcome().status() == "ok" {
                        "ok"
                    } else {
                        "failed"
                    }
                    .into(),
                ),
            );
            if let Some(result) = stopped.outcome().result() {
                material.insert("result".into(), result.clone());
            }
        }
        if request.disposition == Settlement::Attempted {
            material.insert(
                "attempt".into(),
                request
                    .claim
                    .to_value()
                    .get("attempt")
                    .cloned()
                    .ok_or_else(|| {
                        ErrorObj::new("store/execution_contract", "claim attempt is missing")
                    })?,
            );
        }
        let machine_id = self
            .state
            .instance_machines
            .get(instance_id)
            .ok_or_else(|| {
                ErrorObj::new("store/execution_stale", "instance machine disappeared")
            })?;
        material.insert(
            "state_hash".into(),
            Value::Str(state_hash(
                machine_id,
                instance_id,
                self.journal.last_seq.checked_add(1).ok_or_else(|| {
                    ErrorObj::new("store/execution_exhausted", "journal sequence exhausted")
                })?,
                &instance,
            )),
        );
        material.insert("state_format".into(), Value::Str(STATE_FORMAT.into()));
        self.append_execution(
            clock,
            RecordKind::ExecutionSettled,
            material,
            request.request_id,
            timestamp,
        )
    }

    /// Enable a migrated legacy prefix only with an opaque native quiescence proof.
    pub fn enable_execution_on(
        &mut self,
        clock: &mut dyn Clock,
        proof: &VerifiedQuiescence,
        request_id: &str,
    ) -> Result<Value, ErrorObj> {
        let material = BTreeMap::from([
            (
                "previous_head".into(),
                Value::Str(proof.previous_head.clone()),
            ),
            ("quiescence".into(), proof.value.clone()),
        ]);
        if let Some(replay) = self.execution_request(request_id, "execution_enabled", &material)? {
            return replay;
        }
        if proof.previous_head != format!("sha256:{}", self.journal.last_hash) {
            return Err(ErrorObj::new(
                "store/execution_evidence",
                "quiescence proof names a different journal prefix",
            ));
        }
        let timestamp = clock.reserve_ms();
        self.append_execution(
            clock,
            RecordKind::ExecutionEnabled,
            material,
            request_id,
            timestamp,
        )
    }
}
