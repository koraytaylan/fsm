//! Closed claim and stopped-result values; none authenticate native evidence.

use crate::json::Value;

use super::bounded::{MAX_METADATA, MAX_OUTCOME, size};
use super::{
    FailureClass, NativeDomain, RetryPolicy, ShapeError, closed, hex, number, object, text,
    unsigned,
};

/// Immutable identity and handler contract allocated before native launch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Claim {
    pub(super) run_id: u64,
    pub(super) instance_id: String,
    pub(super) effect_id: String,
    pub(super) attempt: u32,
    pub(super) handler_fingerprint: String,
    pub(super) retry: RetryPolicy,
    pub(super) domain: NativeDomain,
}

impl Claim {
    /// Decode the seven closed metadata fields, charging their canonical bytes.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        size(value, MAX_METADATA)?;
        closed(
            value,
            &[
                "run_id",
                "instance_id",
                "effect_id",
                "attempt",
                "handler_fingerprint",
                "retry",
                "domain",
            ],
        )?;
        let run_id = unsigned(value, "run_id")?;
        let instance_id = text(value, "instance_id")?;
        let effect_id = text(value, "effect_id")?;
        let fingerprint = text(value, "handler_fingerprint")?;
        let retry = RetryPolicy::from_value(value.get("retry").ok_or(ShapeError("retry"))?)?;
        let attempt =
            u32::try_from(unsigned(value, "attempt")?).map_err(|_| ShapeError("attempt"))?;
        if run_id == 0 || instance_id.is_empty() || effect_id.is_empty() {
            return Err(ShapeError("claim_identity"));
        }
        if attempt == 0 || attempt > retry.attempts || !digest(fingerprint) {
            return Err(ShapeError("claim_contract"));
        }
        Ok(Self {
            run_id,
            instance_id: instance_id.into(),
            effect_id: effect_id.into(),
            attempt,
            handler_fingerprint: fingerprint.into(),
            retry,
            domain: NativeDomain::from_value(value.get("domain").ok_or(ShapeError("domain"))?)?,
        })
    }

    /// Encode the immutable claim metadata without request-ledger fields.
    pub fn to_value(&self) -> Value {
        object([
            ("run_id", number(self.run_id)),
            ("instance_id", Value::Str(self.instance_id.clone())),
            ("effect_id", Value::Str(self.effect_id.clone())),
            ("attempt", number(u64::from(self.attempt))),
            (
                "handler_fingerprint",
                Value::Str(self.handler_fingerprint.clone()),
            ),
            ("retry", self.retry.to_value()),
            ("domain", self.domain.to_value()),
        ])
    }

    /// Return the store-local non-reusable execution identity.
    pub fn run_id(&self) -> u64 {
        self.run_id
    }

    /// Return the instance and effect whose launch this claim excludes.
    pub fn effect(&self) -> (&str, &str) {
        (&self.instance_id, &self.effect_id)
    }

    /// Return the exact native domain that must close before settlement.
    pub fn domain(&self) -> &NativeDomain {
        &self.domain
    }

    pub(super) fn key(&self) -> (String, String) {
        (self.instance_id.clone(), self.effect_id.clone())
    }
}

pub(super) fn digest(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .is_some_and(|suffix| hex(suffix, 64))
}

/// A claimed run's native closure identity and protected receipt digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Closure {
    run_id: u64,
    domain: NativeDomain,
    receipt: String,
}

impl Closure {
    /// Construct a value naming evidence; this does not authenticate that evidence.
    pub fn new(run_id: u64, domain: NativeDomain, receipt: String) -> Result<Self, ShapeError> {
        if run_id == 0 || !digest(&receipt) {
            return Err(ShapeError("closure"));
        }
        Ok(Self {
            run_id,
            domain,
            receipt,
        })
    }

    /// Decode a closed receipt reference without native I/O.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        size(value, MAX_METADATA)?;
        closed(value, &["run_id", "domain", "receipt"])?;
        let receipt = text(value, "receipt")?;
        if !digest(receipt) {
            return Err(ShapeError("receipt"));
        }
        Self::new(
            unsigned(value, "run_id")?,
            NativeDomain::from_value(value.get("domain").ok_or(ShapeError("domain"))?)?,
            receipt.into(),
        )
    }

    /// Encode the exact run/domain binding and evidence digest.
    pub fn to_value(&self) -> Value {
        object([
            ("run_id", number(self.run_id)),
            ("domain", self.domain.to_value()),
            ("receipt", Value::Str(self.receipt.clone())),
        ])
    }

    pub(super) fn matches(&self, claim: &Claim) -> bool {
        self.run_id == claim.run_id && self.domain == claim.domain
    }
}

/// A bounded immutable result retained until single-consumption settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoppedOutcome {
    status: String,
    result: Option<Value>,
}

impl StoppedOutcome {
    /// Decode a closed outcome, preserving omitted versus explicit-null results.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        size(value, MAX_OUTCOME)?;
        let fields = if value.get("result").is_some() {
            &["status", "result"][..]
        } else {
            &["status"][..]
        };
        closed(value, fields)?;
        let status = text(value, "status")?;
        if status != "ok" && status != "interrupted" && FailureClass::parse(status).is_err() {
            return Err(ShapeError("status"));
        }
        Ok(Self {
            status: status.into(),
            result: value.get("result").cloned(),
        })
    }

    /// Encode this result with its original presence semantics.
    pub fn to_value(&self) -> Value {
        let mut value = object([("status", Value::Str(self.status.clone()))]);
        if let (Value::Obj(fields), Some(result)) = (&mut value, &self.result) {
            fields.insert("result".into(), result.clone());
        }
        value
    }

    /// Return the canonical status used to select settlement semantics.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Borrow the optional result without losing explicit null.
    pub fn result(&self) -> Option<&Value> {
        self.result.as_ref()
    }

    pub(super) fn failure(&self) -> Option<FailureClass> {
        FailureClass::parse(&self.status).ok()
    }
}

/// Closure and result kept together while exclusive ownership remains unresolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stopped {
    pub(super) closure: Closure,
    pub(super) outcome: StoppedOutcome,
}

impl Stopped {
    /// Pair typed values; binding to a claim is checked by the ownership fold.
    pub fn new(closure: Closure, outcome: StoppedOutcome) -> Self {
        Self { closure, outcome }
    }

    /// Decode a closed stopped result; authentication remains the store's job.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        closed(value, &["closure", "outcome"])?;
        Ok(Self::new(
            Closure::from_value(value.get("closure").ok_or(ShapeError("closure"))?)?,
            StoppedOutcome::from_value(value.get("outcome").ok_or(ShapeError("outcome"))?)?,
        ))
    }

    /// Encode both the native binding and immutable outcome.
    pub fn to_value(&self) -> Value {
        object([
            ("closure", self.closure.to_value()),
            ("outcome", self.outcome.to_value()),
        ])
    }

    /// Borrow the result needed by the atomic settlement record.
    pub fn outcome(&self) -> &StoppedOutcome {
        &self.outcome
    }
}
