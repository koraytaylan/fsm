//! Bounded original acknowledgement/event material, not proof of publication.

use crate::hashes::domain_hash;
use crate::json::Value;
use crate::sha256::to_hex;

use super::bounded::{MAX_OUTCOME, size};
use super::ownership_values::digest;
use super::{
    Claim, RetryPolicy, ShapeError, Stopped, StoppedOutcome, closed, number, object, text, unsigned,
};

const MAX_HANDOFF: usize = 128 * 1024;

/// Original acknowledged result and event contract for prospective cold recovery.
///
/// Decoding checks bounded shape, contract identity and derived request keys;
/// it authenticates neither a journal acknowledgement nor native closure.
/// A store must match this material against actual ownership and stopped
/// evidence in the atomic acknowledgement transition before retaining it.
/// Current store formats do not yet publish or carry these values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcknowledgedHandoff {
    claim: Claim,
    original_claim_hash: String,
    handler_contract: Value,
    outcome: StoppedOutcome,
    acknowledgement_request_id: String,
    acknowledgement_seq: u64,
    event_request_id: String,
}

impl AcknowledgedHandoff {
    /// Compare every binding against the actual acknowledgement inputs.
    ///
    /// The store must supply its original claim, authenticated stopped result,
    /// verified claim-record hash and actual append identity; this comparison
    /// cannot authenticate arbitrary caller-owned values or a key alone.
    pub fn matches_acknowledgement(
        &self,
        claim: &Claim,
        stopped: &Stopped,
        original_claim_hash: &str,
        request_id: &str,
        sequence: u64,
    ) -> bool {
        &self.claim == claim
            && stopped.closure.matches(claim)
            && &self.outcome == stopped.outcome()
            && self.original_claim_hash == original_claim_hash
            && self.acknowledgement_request_id == request_id
            && self.acknowledgement_seq == sequence
    }

    /// Decode an original hash-bound candidate without consulting current handlers.
    ///
    /// The full handler parser remains the execution layer's responsibility;
    /// this pure value checks its fingerprint, retry identity and selected
    /// event envelope rather than authorizing a handler or an event.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        size(value, MAX_HANDOFF)?;
        closed(
            value,
            &[
                "format",
                "claim",
                "original_claim_hash",
                "handler_contract",
                "outcome",
                "acknowledgement_request_id",
                "acknowledgement_seq",
                "event_request_id",
            ],
        )?;
        if text(value, "format")? != "fsm.execution-handoff/1" {
            return Err(ShapeError("handoff_format"));
        }
        let claim = Claim::from_value(value.get("claim").ok_or(ShapeError("claim"))?)?;
        let original_claim_hash = text(value, "original_claim_hash")?;
        if !digest(original_claim_hash) {
            return Err(ShapeError("handoff_claim_hash"));
        }
        let handler_contract = value
            .get("handler_contract")
            .ok_or(ShapeError("handoff_contract"))?;
        size(handler_contract, MAX_OUTCOME)?;
        let fingerprint = format!(
            "sha256:{}",
            to_hex(&domain_hash("fsm:handler-contract:1", handler_contract))
        );
        if handler_contract.get("format").and_then(Value::as_str) != Some("fsm.handler-contract/1")
            || fingerprint != claim.handler_fingerprint
            || RetryPolicy::from_value(handler_contract.get("retry").ok_or(ShapeError("retry"))?)?
                != claim.retry
        {
            return Err(ShapeError("handoff_contract"));
        }
        let outcome =
            StoppedOutcome::from_value(value.get("outcome").ok_or(ShapeError("handoff_outcome"))?)?;
        if outcome.status() == "interrupted" {
            return Err(ShapeError("handoff_outcome"));
        }
        let advance = handler_contract
            .get(if outcome.status() == "ok" {
                "on_ok"
            } else {
                "on_failed"
            })
            .ok_or(ShapeError("handoff_event"))?;
        closed(advance, &["event", "payload", "stamps"])?;
        let event = text(advance, "event")?;
        if event.is_empty()
            || advance.get("payload").and_then(Value::as_obj).is_none()
            || !matches!(advance.get("stamps"), Some(Value::Arr(stamps)) if stamps.iter().all(|stamp| stamp.as_str().is_some()))
        {
            return Err(ShapeError("handoff_event"));
        }
        let acknowledgement_seq = unsigned(value, "acknowledgement_seq")?;
        let acknowledgement_request_id = text(value, "acknowledgement_request_id")?;
        let event_request_id = text(value, "event_request_id")?;
        if acknowledgement_seq == 0
            || acknowledgement_request_id != format!("exec-ack-{}", claim.effect_id)
            || event_request_id != format!("exec-ev-{}-{event}", claim.effect_id)
        {
            return Err(ShapeError("handoff_request"));
        }
        Ok(Self {
            claim,
            original_claim_hash: original_claim_hash.into(),
            handler_contract: handler_contract.clone(),
            outcome,
            acknowledgement_request_id: acknowledgement_request_id.into(),
            acknowledgement_seq,
            event_request_id: event_request_id.into(),
        })
    }

    /// Encode the complete original material without inferring publication.
    pub fn to_value(&self) -> Value {
        object([
            ("format", Value::Str("fsm.execution-handoff/1".into())),
            ("claim", self.claim.to_value()),
            (
                "original_claim_hash",
                Value::Str(self.original_claim_hash.clone()),
            ),
            ("handler_contract", self.handler_contract.clone()),
            ("outcome", self.outcome.to_value()),
            (
                "acknowledgement_request_id",
                Value::Str(self.acknowledgement_request_id.clone()),
            ),
            ("acknowledgement_seq", number(self.acknowledgement_seq)),
            (
                "event_request_id",
                Value::Str(self.event_request_id.clone()),
            ),
        ])
    }

    /// Borrow the original execution identity, never current ownership proof.
    pub fn claim(&self) -> &Claim {
        &self.claim
    }

    /// Borrow the original claim-record hash, not a hash of Claim metadata.
    pub fn original_claim_hash(&self) -> &str {
        &self.original_claim_hash
    }

    /// Borrow potentially sensitive original contract material for checked recovery.
    pub fn handler_contract(&self) -> &Value {
        &self.handler_contract
    }

    /// Borrow the actual terminal outcome, preserving omitted versus null result.
    pub fn outcome(&self) -> &StoppedOutcome {
        &self.outcome
    }

    /// Return the original acknowledgement sequence candidate, not journal proof.
    pub fn acknowledgement_seq(&self) -> u64 {
        self.acknowledgement_seq
    }

    /// Return the original derived acknowledgement key.
    pub fn acknowledgement_request_id(&self) -> &str {
        &self.acknowledgement_request_id
    }

    /// Return the original derived outcome-event key; rejection cannot consume it.
    pub fn event_request_id(&self) -> &str {
        &self.event_request_id
    }
}
