//! Pure exclusive ownership and durable retry admission for claim-era stores.
//!
//! The caller supplies journal-derived pending state and authenticated closure;
//! these transitions never consult a clock, filesystem, process, or supervisor.

use std::collections::BTreeMap;

use crate::json::Value;

use super::bounded::{MAX_ENTRIES, block_size};
use super::ownership_values::digest;
use super::{
    Claim, FailureClass, RetryPolicy, ShapeError, Stopped, closed, number, object, signed, text,
    unsigned,
};

/// Whether this store may allocate new execution ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// New stores or independently verified legacy execution environments.
    Enabled,
    /// Legacy migration alone cannot establish execution quiescence.
    Quarantined,
}

/// Pending-effect observation checked by the production caller under the writer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingEffect {
    /// The effect is still pending in its instance state.
    Present,
    /// External acknowledgement or cancellation has removed the effect.
    Absent,
}

/// Single-consumption disposition of a proved-stopped execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Settlement {
    /// Consume the result with the existing acknowledgement semantics.
    Acked,
    /// Record exactly one failed attempt and durable retry deadline.
    Attempted,
    /// Consume closure without applying a result or increasing failed count.
    Interrupted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Owned {
    claim: Claim,
    stopped: Option<Stopped>,
    // Replay context, excluded from logical roots to avoid claim self-reference.
    claim_record_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RetryEntry {
    handler_fingerprint: String,
    retry: RetryPolicy,
    failed_count: u32,
    last_timestamp: i64,
    failure_class: FailureClass,
    eligible_at: i64,
}

/// Bounded claim-era state; constructing it does not authorize native execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionState {
    admission: Admission,
    run_high_water: u64,
    claims: BTreeMap<(String, String), Owned>,
    retry: BTreeMap<(String, String), RetryEntry>,
}

impl ExecutionState {
    /// Start an empty new-store or quarantined legacy execution state.
    pub fn new(admission: Admission) -> Self {
        Self {
            admission,
            run_high_water: 0,
            claims: BTreeMap::new(),
            retry: BTreeMap::new(),
        }
    }

    /// Return the admission state without treating it as a native capability.
    pub fn admission(&self) -> Admission {
        self.admission
    }

    pub(crate) fn enable(&mut self) -> Result<(), ShapeError> {
        if self.admission != Admission::Quarantined {
            return Err(ShapeError("already_enabled"));
        }
        self.admission = Admission::Enabled;
        Ok(())
    }

    pub(crate) fn retain_pending(&mut self, is_pending: impl Fn(&str, &str) -> bool) {
        let claims = &self.claims;
        self.retry
            .retain(|key, _| claims.contains_key(key) || is_pending(&key.0, &key.1));
    }

    /// Observe every unresolved run and its optional stopped result without consumption.
    pub fn unresolved(&self) -> impl Iterator<Item = (&Claim, Option<&Stopped>)> {
        self.claims
            .values()
            .map(|owned| (&owned.claim, owned.stopped.as_ref()))
    }

    /// Return the allocation high-water mark, including already settled runs.
    pub fn run_high_water(&self) -> u64 {
        self.run_high_water
    }

    /// Compute the next allocation without advancing or wrapping the counter.
    pub fn next_run_id(&self) -> Result<u64, ShapeError> {
        self.run_high_water
            .checked_add(1)
            .ok_or(ShapeError("run_exhausted"))
    }

    /// Borrow unresolved ownership; stopped executions remain visible here.
    pub fn claim_for(&self, instance_id: &str, effect_id: &str) -> Option<&Claim> {
        self.claims
            .iter()
            .find(|((instance, effect), _)| instance == instance_id && effect == effect_id)
            .map(|(_, owned)| &owned.claim)
    }

    /// Borrow a stopped result without consuming its exclusive ownership.
    pub fn stopped_for(&self, instance_id: &str, effect_id: &str) -> Option<&Stopped> {
        self.claims
            .iter()
            .find(|((instance, effect), _)| instance == instance_id && effect == effect_id)
            .and_then(|(_, owned)| owned.stopped.as_ref())
    }

    /// Borrow the original journal hash for this exact unresolved claim.
    /// Decoded logical execution values alone carry no verified hash context.
    pub fn claim_record_hash(&self, claim: &Claim) -> Option<&str> {
        self.match_owned(claim).ok()?.claim_record_hash.as_deref()
    }

    /// Attach separately verified journal or sealed-base context to an exact claim.
    ///
    /// The caller MUST verify the original record or authenticated base index;
    /// this shape check does not authenticate caller-supplied hashes or authorize
    /// execution. Publication projections must replace provisional record hashes
    /// with final hashes after checkpoint root fields have been added.
    pub fn attach_claim_record_hash(
        &mut self,
        claim: &Claim,
        hash: &str,
    ) -> Result<(), ShapeError> {
        self.match_owned(claim)?;
        if !digest(hash) {
            return Err(ShapeError("claim_hash"));
        }
        self.claims
            .get_mut(&claim.key())
            .ok_or(ShapeError("claim_binding"))?
            .claim_record_hash = Some(hash.into());
        Ok(())
    }

    /// Return the durable failed count, independent of native run allocation.
    pub fn failed_count(&self, instance_id: &str, effect_id: &str) -> u32 {
        self.retry
            .iter()
            .find(|((instance, effect), _)| instance == instance_id && effect == effect_id)
            .map_or(0, |(_, entry)| entry.failed_count)
    }

    /// Allocate the exact next run after pending, ownership and retry validation.
    pub fn claim(
        &mut self,
        claim: Claim,
        pending: PendingEffect,
        now: i64,
    ) -> Result<(), ShapeError> {
        if self.admission != Admission::Enabled {
            return Err(ShapeError("quarantined"));
        }
        if pending != PendingEffect::Present {
            return Err(ShapeError("effect_not_pending"));
        }
        let key = claim.key();
        if self.claims.contains_key(&key) {
            return Err(ShapeError("owned"));
        }
        if claim.run_id != self.next_run_id()? {
            return Err(ShapeError("run_id"));
        }
        match self.retry.get(&key) {
            Some(entry) => {
                if claim.handler_fingerprint != entry.handler_fingerprint
                    || claim.retry != entry.retry
                {
                    return Err(ShapeError("contract"));
                }
                if claim.attempt != entry.failed_count + 1 {
                    return Err(ShapeError("attempt"));
                }
                if !entry.retry.admits_retry(
                    entry.failed_count,
                    entry.failure_class,
                    entry.last_timestamp,
                    now,
                ) {
                    return Err(ShapeError("retry_ineligible"));
                }
            }
            None if claim.attempt != 1 => return Err(ShapeError("attempt")),
            None => {}
        }
        if !self.retry.contains_key(&key) && self.entry_count() >= MAX_ENTRIES {
            return Err(ShapeError("entries"));
        }
        let mut next = self.clone();
        next.run_high_water = claim.run_id;
        next.claims.insert(
            key,
            Owned {
                claim,
                stopped: None,
                claim_record_hash: None,
            },
        );
        self.install(next)
    }

    /// Fold authenticated native closure; ownership is retained until settlement.
    pub fn stop(&mut self, claim: &Claim, stopped: Stopped) -> Result<(), ShapeError> {
        let owned = self.match_owned(claim)?;
        if owned.stopped.is_some() {
            return Err(ShapeError("already_stopped"));
        }
        if !stopped.closure.matches(claim) {
            return Err(ShapeError("closure_binding"));
        }
        let mut next = self.clone();
        next.claims
            .get_mut(&claim.key())
            .ok_or(ShapeError("owned"))?
            .stopped = Some(stopped);
        self.install(next)
    }

    /// Select from the original stopped claim policy without mutating ownership.
    pub fn settlement_for(
        &self,
        claim: &Claim,
        pending: PendingEffect,
    ) -> Result<Settlement, ShapeError> {
        let stopped = self
            .match_owned(claim)?
            .stopped
            .as_ref()
            .ok_or(ShapeError("not_stopped"))?;
        if pending == PendingEffect::Absent || stopped.outcome.status() == "interrupted" {
            return Ok(Settlement::Interrupted);
        }
        if let Some(class) = stopped.outcome.failure()
            && claim.attempt < claim.retry.attempts
            && claim.retry.on.contains(&class)
        {
            return Ok(Settlement::Attempted);
        }
        Ok(Settlement::Acked)
    }

    /// Consume a stopped run with its disposition, retaining the allocation counter.
    ///
    /// The store must combine this transition and its instance mutation in one
    /// durable record; this method alone does not acknowledge an engine effect.
    pub fn settle(
        &mut self,
        claim: &Claim,
        settlement: Settlement,
        pending: PendingEffect,
        timestamp: i64,
    ) -> Result<Stopped, ShapeError> {
        let stopped = self
            .match_owned(claim)?
            .stopped
            .as_ref()
            .ok_or(ShapeError("not_stopped"))?;
        let failure = stopped.outcome.failure();
        let allowed = match settlement {
            Settlement::Acked => {
                pending == PendingEffect::Present
                    && (matches!(stopped.outcome.status(), "ok" | "failed") || failure.is_some())
            }
            Settlement::Attempted => pending == PendingEffect::Present && failure.is_some(),
            Settlement::Interrupted => {
                pending == PendingEffect::Absent || stopped.outcome.status() == "interrupted"
            }
        };
        if !allowed {
            return Err(ShapeError("disposition"));
        }
        let stopped = stopped.clone();
        let key = claim.key();
        let mut next = self.clone();
        match settlement {
            Settlement::Attempted => {
                next.retry.insert(
                    key.clone(),
                    RetryEntry {
                        handler_fingerprint: claim.handler_fingerprint.clone(),
                        retry: claim.retry.clone(),
                        failed_count: claim.attempt,
                        last_timestamp: timestamp,
                        failure_class: failure.ok_or(ShapeError("failure_class"))?,
                        eligible_at: claim.retry.ready_at(claim.attempt, timestamp)?,
                    },
                );
            }
            Settlement::Acked => {
                next.retry.remove(&key);
            }
            Settlement::Interrupted if pending == PendingEffect::Absent => {
                next.retry.remove(&key);
            }
            Settlement::Interrupted => {}
        }
        next.claims.remove(&key);
        self.install(next)?;
        Ok(stopped)
    }

    /// Forget a removed effect's ledger once its ownership is resolved.
    ///
    /// An unresolved claim retains its previous ledger for authenticated replay;
    /// settlement with an absent effect consumes both without applying a result.
    pub fn effect_removed(&mut self, instance_id: &str, effect_id: &str) {
        let key = self
            .retry
            .keys()
            .find(|(instance, effect)| instance == instance_id && effect == effect_id);
        if let Some(key) = key.filter(|key| !self.claims.contains_key(*key)).cloned() {
            self.retry.remove(&key);
        }
    }

    fn match_owned(&self, claim: &Claim) -> Result<&Owned, ShapeError> {
        let owned = self
            .claims
            .get(&claim.key())
            .ok_or(ShapeError("claim_binding"))?;
        if owned.claim != *claim {
            return Err(ShapeError("claim_binding"));
        }
        Ok(owned)
    }

    fn entry_count(&self) -> usize {
        self.claims.len()
            + self
                .retry
                .keys()
                .filter(|key| !self.claims.contains_key(*key))
                .count()
    }

    fn install(&mut self, next: Self) -> Result<(), ShapeError> {
        // Each input is individually bounded and the old state is bounded;
        // copying the candidate cannot allocate in proportion to untrusted input.
        block_size(&next.to_value())?;
        *self = next;
        Ok(())
    }

    /// Encode canonical arrays, retaining stopped results and settled run history.
    pub fn to_value(&self) -> Value {
        let mut claims: Vec<_> = self.claims.values().collect();
        claims.sort_unstable_by_key(|owned| owned.claim.run_id);
        object([
            (
                "admission",
                Value::Str(
                    match self.admission {
                        Admission::Enabled => "enabled",
                        Admission::Quarantined => "quarantined",
                    }
                    .into(),
                ),
            ),
            ("run_high_water", number(self.run_high_water)),
            (
                "claims",
                Value::Arr(
                    claims
                        .into_iter()
                        .map(|owned| {
                            object([
                                ("claim", owned.claim.to_value()),
                                (
                                    "stopped",
                                    owned
                                        .stopped
                                        .as_ref()
                                        .map_or(Value::Null, Stopped::to_value),
                                ),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "retry",
                Value::Arr(
                    self.retry
                        .iter()
                        .map(|((instance_id, effect_id), entry)| {
                            entry.to_value(instance_id, effect_id)
                        })
                        .collect(),
                ),
            ),
        ])
    }

    /// Decode an authenticated block without normalizing contradictory evidence.
    pub fn from_value(value: &Value) -> Result<Self, ShapeError> {
        block_size(value)?;
        closed(value, &["admission", "run_high_water", "claims", "retry"])?;
        let admission = match text(value, "admission")? {
            "enabled" => Admission::Enabled,
            "quarantined" => Admission::Quarantined,
            _ => return Err(ShapeError("admission")),
        };
        let mut state = Self::new(admission);
        state.run_high_water = unsigned(value, "run_high_water")?;
        let claims = array(value, "claims")?;
        let retry = array(value, "retry")?;
        if claims.len() > MAX_ENTRIES || retry.len() > MAX_ENTRIES {
            return Err(ShapeError("entries"));
        }
        let mut previous_run = 0;
        for entry in claims {
            closed(entry, &["claim", "stopped"])?;
            let claim = Claim::from_value(entry.get("claim").ok_or(ShapeError("claim"))?)?;
            if claim.run_id <= previous_run || claim.run_id > state.run_high_water {
                return Err(ShapeError("run_order"));
            }
            previous_run = claim.run_id;
            let stopped = match entry.get("stopped").ok_or(ShapeError("stopped"))? {
                Value::Null => None,
                value => {
                    let stopped = Stopped::from_value(value)?;
                    if !stopped.closure.matches(&claim) {
                        return Err(ShapeError("closure_binding"));
                    }
                    Some(stopped)
                }
            };
            if state
                .claims
                .insert(
                    claim.key(),
                    Owned {
                        claim,
                        stopped,
                        claim_record_hash: None,
                    },
                )
                .is_some()
            {
                return Err(ShapeError("duplicate_effect"));
            }
        }
        let mut previous_key = None;
        for value in retry {
            let (key, entry) = RetryEntry::from_value(value)?;
            if previous_key
                .as_ref()
                .is_some_and(|previous| previous >= &key)
            {
                return Err(ShapeError("retry_order"));
            }
            previous_key = Some(key.clone());
            if let Some(owned) = state.claims.get(&key)
                && (owned.claim.handler_fingerprint != entry.handler_fingerprint
                    || owned.claim.retry != entry.retry
                    || owned.claim.attempt != entry.failed_count + 1)
            {
                return Err(ShapeError("contract"));
            }
            state.retry.insert(key, entry);
        }
        if state.entry_count() > MAX_ENTRIES {
            return Err(ShapeError("entries"));
        }
        // An active second attempt must retain its previous failed ledger.
        if state
            .claims
            .iter()
            .any(|(key, owned)| owned.claim.attempt > 1 && !state.retry.contains_key(key))
        {
            return Err(ShapeError("retry_missing"));
        }
        Ok(state)
    }
}

fn array<'a>(value: &'a Value, field: &'static str) -> Result<&'a [Value], ShapeError> {
    value
        .get(field)
        .and_then(Value::as_arr)
        .ok_or(ShapeError(field))
}

impl RetryEntry {
    fn to_value(&self, instance_id: &str, effect_id: &str) -> Value {
        object([
            ("instance_id", Value::Str(instance_id.into())),
            ("effect_id", Value::Str(effect_id.into())),
            (
                "handler_fingerprint",
                Value::Str(self.handler_fingerprint.clone()),
            ),
            ("retry", self.retry.to_value()),
            ("failed_count", number(u64::from(self.failed_count))),
            (
                "last_timestamp",
                Value::Num(self.last_timestamp.to_string()),
            ),
            (
                "failure_class",
                Value::Str(self.failure_class.as_str().into()),
            ),
            ("eligible_at", Value::Num(self.eligible_at.to_string())),
        ])
    }

    fn from_value(value: &Value) -> Result<((String, String), Self), ShapeError> {
        closed(
            value,
            &[
                "instance_id",
                "effect_id",
                "handler_fingerprint",
                "retry",
                "failed_count",
                "last_timestamp",
                "failure_class",
                "eligible_at",
            ],
        )?;
        let instance_id = text(value, "instance_id")?;
        let effect_id = text(value, "effect_id")?;
        let fingerprint = text(value, "handler_fingerprint")?;
        if instance_id.is_empty() || effect_id.is_empty() || !digest(fingerprint) {
            return Err(ShapeError("retry_identity"));
        }
        let retry = RetryPolicy::from_value(value.get("retry").ok_or(ShapeError("retry"))?)?;
        let failed_count = u32::try_from(unsigned(value, "failed_count")?)
            .map_err(|_| ShapeError("failed_count"))?;
        let last_timestamp = signed(value, "last_timestamp")?;
        let eligible_at = signed(value, "eligible_at")?;
        if failed_count > retry.attempts
            || retry.ready_at(failed_count, last_timestamp)? != eligible_at
        {
            return Err(ShapeError("eligible_at"));
        }
        Ok((
            (instance_id.into(), effect_id.into()),
            Self {
                handler_fingerprint: fingerprint.into(),
                retry,
                failed_count,
                last_timestamp,
                failure_class: FailureClass::parse(text(value, "failure_class")?)?,
                eligible_at,
            },
        ))
    }
}
