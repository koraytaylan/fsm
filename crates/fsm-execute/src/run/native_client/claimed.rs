//! One claim-bound bind/execute sequence; journal ownership stays with the host.

use super::proof_worker::ProofWorker;
use super::{NativeCompletion, NativeHelperProgress, NativeRequest};
use fsm_core::json::Value;
use fsm_core::record::execution::Claim;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

enum Phase {
    Binding,
    Bound,
    Executing,
    Recovering,
    Finished,
}

/// Claim-bound execution progress, separate from transport-helper retirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRunPhase {
    /// Original-claim binding is pending; execution has not been requested.
    Binding,
    /// Binding completed; execution helper startup waits for the next poll.
    Bound,
    /// Execution has been requested; authenticated closure is still pending.
    Executing,
    /// Recorded original completion is requested; no binding/launch is permitted.
    Recovering,
    /// A matching verified completion has been delivered to the host.
    Closed,
    /// Execution failed or was cancelled without delivering verified closure.
    Uncertain,
}

/// Bounded diagnostic snapshot with no argv, output, authority path or secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NativeRunProgress {
    /// Whether original-claim-matched closure has been delivered.
    pub phase: NativeRunPhase,
    /// Retirement observations for the currently owned transport helper.
    pub helper: NativeHelperProgress,
}

/// Owned contained execution for an already durable immutable claim.
pub struct NativeRun {
    claim: Claim,
    journal_claim: String,
    namespace: String,
    generation: u64,
    allocation: Value,
    deadline: Instant,
    request: NativeRequest,
    verification: Option<ProofWorker<NativeCompletion>>,
    phase: Phase,
    error: Option<String>,
    writer_entry: bool,
}

impl NativeRun {
    /// Bind an already durable claim; callers must recheck admission under the writer.
    pub fn start(claim: &Claim, journal_claim: &str, timeout: Duration) -> Result<Self, String> {
        Self::begin(claim, journal_claim, timeout, false)
    }

    /// Read a recorded original completion without binding or launching a handler.
    ///
    /// The caller supplies the retained original identity/hash; matching proof
    /// and current writer-held ownership checks still govern journal application.
    pub fn recover(claim: &Claim, journal_claim: &str, timeout: Duration) -> Result<Self, String> {
        Self::begin(claim, journal_claim, timeout, true)
    }

    fn begin(
        claim: &Claim,
        journal_claim: &str,
        timeout: Duration,
        recovery: bool,
    ) -> Result<Self, String> {
        if !journal_claim.strip_prefix("sha256:").is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        }) {
            return Err("native run original claim hash invalid".into());
        }
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or("native run deadline exceeds clock range")?;
        let domain = claim.domain().to_value();
        let namespace = domain
            .get("namespace")
            .and_then(Value::as_str)
            .ok_or("native run namespace missing")?
            .to_owned();
        let generation = domain
            .get("generation")
            .and_then(Value::as_num)
            .ok_or("native run generation missing")?
            .parse()
            .map_err(|_| "native run generation invalid")?;
        let allocation = domain
            .get("allocation")
            .ok_or("native run allocation missing")?
            .clone();
        let binding = object([
            ("format", Value::Str("fsm.native-claim-binding/1".into())),
            ("claim", claim.to_value()),
            ("journal_claim", Value::Str(journal_claim.into())),
        ]);
        let (action, payload) = if recovery {
            ("recover", allocation.clone())
        } else {
            ("bind", binding)
        };
        let request = NativeRequest::start_until(
            &namespace,
            generation,
            &request(action, payload),
            deadline,
        )?;
        Ok(Self {
            claim: claim.clone(),
            journal_claim: journal_claim.into(),
            namespace,
            generation,
            allocation,
            deadline,
            request,
            verification: None,
            phase: if recovery {
                Phase::Recovering
            } else {
                Phase::Binding
            },
            error: None,
            writer_entry: false,
        })
    }

    /// Advance bounded helper I/O and return only original-claim-matched closure.
    pub fn poll(&mut self) -> Result<Option<NativeCompletion>, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        if matches!(self.phase, Phase::Finished) {
            return Err("native run completion already collected".into());
        }
        let result = self.advance();
        if let Err(error) = &result {
            self.error = Some(error.clone());
            if let Some(verification) = &self.verification {
                verification.cancel();
            }
            let _ = self.request.cancel();
        }
        result
    }

    fn advance(&mut self) -> Result<Option<NativeCompletion>, String> {
        if Instant::now() >= self.deadline {
            return Err("native run deadline; claim remains uncertain".into());
        }
        if let Some(verification) = &mut self.verification {
            let Some(completion) = verification.poll()? else {
                return Ok(None);
            };
            return self.finish(completion);
        }
        if matches!(self.phase, Phase::Bound) {
            if self.writer_entry {
                return Ok(None);
            }
            self.request_execution()?;
        }
        let response = match self.request.poll()? {
            Some(response) => response,
            None => return Ok(None),
        };
        match self.phase {
            Phase::Binding => {
                if response.get("ok") != Some(&Value::Bool(true))
                    || response.get("result") != Some(&Value::Null)
                {
                    return Err("native run binding refused; claim remains uncertain".into());
                }
                self.phase = Phase::Bound;
                Ok(None)
            }
            Phase::Executing | Phase::Recovering => {
                if let Some(ticket) = &self.request.ticket {
                    let claim = self.claim.clone();
                    let hash = self.journal_claim.clone();
                    self.verification = Some(ProofWorker::start(
                        move || NativeCompletion::verify(&response, &claim, &hash),
                        std::sync::Arc::clone(ticket),
                        self.deadline,
                    )?);
                    return Ok(None);
                }
                let completion =
                    NativeCompletion::verify(&response, &self.claim, &self.journal_claim)?;
                self.finish(completion)
            }
            Phase::Bound => Err("native run bound transition invalid".into()),
            Phase::Finished => Err("native run completion already collected".into()),
        }
    }

    fn finish(&mut self, completion: NativeCompletion) -> Result<Option<NativeCompletion>, String> {
        if Instant::now() >= self.deadline {
            return Err("native run deadline; claim remains uncertain".into());
        }
        self.phase = Phase::Finished;
        Ok(Some(completion))
    }

    pub(super) fn require_writer_entry(&mut self) {
        self.writer_entry = true;
    }

    pub(super) fn launch_bound(&mut self, claim: &Claim, hash: &str) -> Result<(), String> {
        if !self.writer_entry
            || self.error.is_some()
            || !matches!(self.phase, Phase::Bound)
            || self.claim != *claim
            || self.journal_claim != hash
        {
            return Err("native execution requires the original writer-checked bound owner".into());
        }
        let result = self.request_execution();
        if let Err(error) = &result {
            self.error = Some(error.clone());
            let _ = self.request.cancel();
        }
        result
    }

    fn request_execution(&mut self) -> Result<(), String> {
        self.phase = Phase::Executing;
        self.request = self.request.successor(
            &self.namespace,
            self.generation,
            &request("execute", self.allocation.clone()),
            self.deadline,
        )?;
        Ok(())
    }

    /// Request helper cancellation while retaining uncertain claim ownership.
    pub fn cancel(&mut self) -> Result<(), String> {
        self.error
            .get_or_insert_with(|| "native run cancelled; claim remains uncertain".into());
        if let Some(verification) = &self.verification {
            verification.cancel();
        }
        self.request.cancel()
    }

    /// Read progress without polling or releasing durable execution ownership.
    pub fn progress(&self) -> NativeRunProgress {
        let phase = match self.phase {
            Phase::Finished => NativeRunPhase::Closed,
            _ if self.error.is_some() => NativeRunPhase::Uncertain,
            Phase::Binding => NativeRunPhase::Binding,
            Phase::Bound => NativeRunPhase::Bound,
            Phase::Executing => NativeRunPhase::Executing,
            Phase::Recovering => NativeRunPhase::Recovering,
        };
        let helper = self.request.progress();
        NativeRunProgress {
            phase,
            helper: self.verification.as_ref().map_or(helper, |verification| {
                verification.withhold_retirement(helper)
            }),
        }
    }

    /// Observe helper retirement; this does not prove native handler closure.
    pub fn reap(&mut self) -> Result<bool, String> {
        let helper = self.request.reap()?;
        let proof = self.verification.as_mut().is_none_or(ProofWorker::reap);
        Ok(helper && proof)
    }
}

fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    Value::Obj(BTreeMap::from(
        fields.map(|(key, value)| (key.into(), value)),
    ))
}

fn request(action: &str, payload: Value) -> Value {
    object([
        ("format", Value::Str("fsm.native-request/1".into())),
        ("action", Value::Str(action.into())),
        ("payload", payload),
    ])
}

#[cfg(test)]
mod tests;
