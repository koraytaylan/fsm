//! One claim-bound bind/execute sequence; journal ownership stays with the host.

use super::{NativeCompletion, NativeHelperProgress, NativeRequest};
use fsm_core::json::Value;
use fsm_core::record::execution::Claim;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

enum Phase {
    Binding,
    Executing,
    Finished,
}

/// Claim-bound execution progress, separate from transport-helper retirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeRunPhase {
    /// Original-claim binding is pending; execution has not been requested.
    Binding,
    /// Execution has been requested; authenticated closure is still pending.
    Executing,
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
    phase: Phase,
    error: Option<String>,
}

impl NativeRun {
    /// Bind an already durable claim; callers must recheck admission under the writer.
    pub fn start(claim: &Claim, journal_claim: &str, timeout: Duration) -> Result<Self, String> {
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
        let request =
            NativeRequest::start(&namespace, generation, &request("bind", binding), timeout)?;
        Ok(Self {
            claim: claim.clone(),
            journal_claim: journal_claim.into(),
            namespace,
            generation,
            allocation,
            deadline,
            request,
            phase: Phase::Binding,
            error: None,
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
            let _ = self.request.cancel();
        }
        result
    }

    fn advance(&mut self) -> Result<Option<NativeCompletion>, String> {
        if Instant::now() >= self.deadline {
            return Err("native run deadline; claim remains uncertain".into());
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
                let remaining = self
                    .deadline
                    .checked_duration_since(Instant::now())
                    .filter(|remaining| !remaining.is_zero())
                    .ok_or("native run deadline; claim remains uncertain")?;
                let next = NativeRequest::start(
                    &self.namespace,
                    self.generation,
                    &request("execute", self.allocation.clone()),
                    remaining,
                )?;
                self.request = next;
                self.phase = Phase::Executing;
                Ok(None)
            }
            Phase::Executing => {
                let completion =
                    NativeCompletion::verify(&response, &self.claim, &self.journal_claim)?;
                if Instant::now() >= self.deadline {
                    return Err("native run deadline; claim remains uncertain".into());
                }
                self.phase = Phase::Finished;
                Ok(Some(completion))
            }
            Phase::Finished => Err("native run completion already collected".into()),
        }
    }

    /// Request helper cancellation while retaining uncertain claim ownership.
    pub fn cancel(&mut self) -> Result<(), String> {
        self.error
            .get_or_insert_with(|| "native run cancelled; claim remains uncertain".into());
        self.request.cancel()
    }

    /// Read progress without polling or releasing durable execution ownership.
    pub fn progress(&self) -> NativeRunProgress {
        let phase = match self.phase {
            Phase::Finished => NativeRunPhase::Closed,
            _ if self.error.is_some() => NativeRunPhase::Uncertain,
            Phase::Binding => NativeRunPhase::Binding,
            Phase::Executing => NativeRunPhase::Executing,
        };
        NativeRunProgress {
            phase,
            helper: self.request.progress(),
        }
    }

    /// Observe helper retirement; this does not prove native handler closure.
    pub fn reap(&mut self) -> Result<bool, String> {
        self.request.reap()
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
