//! Claim-bound candidates with opaque native proof, without journal mutation.

use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{Claim, FailureClass, StoppedOutcome};
use fsm_store::store::VerifiedClosure;
use std::path::PathBuf;

/// Checked candidate and matching closure; current ownership remains the store's job.
pub struct NativeCompletion {
    claim: Claim,
    handler: crate::config::HandlerSpec,
    candidate: Value,
    failure_class: Option<FailureClass>,
    proof: VerifiedClosure,
    stopped: StoppedOutcome,
}

impl NativeCompletion {
    /// Verify a collected transport response against its original claim and hash.
    pub fn verify(response: &Value, claim: &Claim, journal_claim: &str) -> Result<Self, String> {
        let material = validate(response, claim, journal_claim)?;
        let proof = VerifiedClosure::read(&material.receipt).map_err(|error| error.message)?;
        if !proof.matches_claim(claim, journal_claim) {
            return Err("native completion closure does not match original claim".into());
        }
        let response_hash = format!(
            "sha256:{}",
            fsm_core::sha256::to_hex(&fsm_core::hashes::domain_hash(
                "fsm:native-response:1",
                response
            ),)
        );
        proof
            .check_result_digest(&response_hash)
            .map_err(|error| format!("native completion result attestation: {}", error.message))?;
        Ok(Self {
            claim: claim.clone(),
            handler: material.handler,
            candidate: material.candidate,
            failure_class: material.failure_class,
            proof,
            stopped: material.stopped,
        })
    }

    pub(crate) fn matches_original(&self, claim: &Claim) -> bool {
        &self.claim == claim
    }

    /// Borrow the checked original contract, including possibly secret values.
    ///
    /// This does not consult a current table or grant ownership/settlement;
    /// hosts must not include its full material in health summaries.
    pub fn handler(&self) -> &crate::config::HandlerSpec {
        &self.handler
    }

    /// Borrow the unchanged acknowledgement candidate from the authenticated transport.
    pub fn candidate(&self) -> &Value {
        &self.candidate
    }

    /// Return the original retry class, without consulting a changed handler table.
    pub fn failure_class(&self) -> Option<FailureClass> {
        self.failure_class
    }

    /// Borrow matched opaque proof for the writer-protected stopped transition.
    pub fn proof(&self) -> &VerifiedClosure {
        &self.proof
    }

    /// Borrow stopped semantics derived from the checked candidate and original claim.
    pub fn stopped_outcome(&self) -> &StoppedOutcome {
        &self.stopped
    }
}

struct Material {
    handler: crate::config::HandlerSpec,
    candidate: Value,
    failure_class: Option<FailureClass>,
    receipt: PathBuf,
    stopped: StoppedOutcome,
}

fn closed(value: &Value, fields: &[&str]) -> Result<(), String> {
    let object = value.as_obj().ok_or("native completion is not an object")?;
    if object.len() != fields.len() || fields.iter().any(|field| !object.contains_key(*field)) {
        return Err("native completion fields differ".into());
    }
    Ok(())
}

fn validate(response: &Value, claim: &Claim, journal_claim: &str) -> Result<Material, String> {
    let bytes = crate::value_limits::canonical(response, 65536)
        .map_err(|_| "native completion exceeds response bound")?;
    parse(&bytes, &JsonLimits::DEFAULT).map_err(|_| "native completion exceeds JSON limits")?;
    closed(response, &["format", "ok", "result"])?;
    if let Some(reason) = super::refusal(response, "native completion refused: ") {
        return Err(reason);
    }
    if response.get("format").and_then(Value::as_str) != Some("fsm.native-response/1")
        || response.get("ok") != Some(&Value::Bool(true))
    {
        return Err("native broker did not return a successful completion".into());
    }
    if !journal_claim.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err("native completion original claim hash invalid".into());
    }
    let result = response
        .get("result")
        .ok_or("native completion result missing")?;
    closed(
        result,
        &[
            "format",
            "handler_kind",
            "handler_contract",
            "claim",
            "journal_claim",
            "receipt",
            "candidate",
            "failure_class",
        ],
    )?;
    if result.get("format").and_then(Value::as_str) != Some("fsm.native-run-result/3")
        || result.get("claim") != Some(&claim.to_value())
        || result.get("journal_claim").and_then(Value::as_str) != Some(journal_claim)
    {
        return Err("native completion original identity differs".into());
    }
    let original = claim.to_value();
    let contract = result
        .get("handler_contract")
        .ok_or("native completion original contract missing")?;
    let handler = crate::config::HandlerSpec::from_contract(
        contract,
        original
            .get("handler_fingerprint")
            .and_then(Value::as_str)
            .ok_or("native completion original fingerprint missing")?,
    )
    .map_err(|_| "native completion original contract differs")?;
    if contract.get("retry") != original.get("retry")
        || result.get("handler_kind").and_then(Value::as_str) != Some(handler.kind.as_str())
    {
        return Err("native completion original contract snapshot differs".into());
    }
    let domain = claim.domain().to_value();
    let namespace = domain
        .get("namespace")
        .and_then(Value::as_str)
        .ok_or("native completion namespace missing")?;
    let generation = domain
        .get("generation")
        .and_then(Value::as_num)
        .ok_or("native completion generation missing")?;
    let allocation = domain
        .get("allocation")
        .and_then(Value::as_num)
        .ok_or("native completion allocation missing")?;
    let receipt = format!(
        "/var/lib/fsm-containment/{namespace}/authority-{generation}/closure-{allocation}-{}.json",
        claim.run_id()
    );
    if result.get("receipt").and_then(Value::as_str) != Some(receipt.as_str()) {
        return Err("native completion receipt route differs".into());
    }
    let failure_class = match result.get("failure_class") {
        Some(Value::Null) => None,
        Some(Value::Str(class)) => Some(match class.as_str() {
            "mcp_error" => FailureClass::McpError,
            "nonzero_exit" => FailureClass::NonzeroExit,
            "spawn" => FailureClass::Spawn,
            "timeout" => FailureClass::Timeout,
            _ => return Err("native completion failure class unknown".into()),
        }),
        _ => return Err("native completion failure class invalid".into()),
    };
    let candidate = result
        .get("candidate")
        .ok_or("native completion candidate missing")?;
    if candidate.as_obj().is_none() {
        return Err("native completion candidate is not an acknowledgement object".into());
    }
    let kind = result
        .get("handler_kind")
        .and_then(Value::as_str)
        .ok_or("native completion handler kind missing")?;
    let generic_error = candidate
        .get("error")
        .and_then(Value::as_str)
        .is_some_and(|error| matches!(error, "exec/cancelled" | "exec/timeout" | "exec/spawn"));
    match kind {
        "process"
            if generic_error
                || (candidate.get("error").is_none() && candidate.get("status").is_some()) => {}
        "mcp" if generic_error || candidate.get("status").is_none() => {}
        _ => return Err("native completion candidate and handler kind differ".into()),
    }
    Ok(Material {
        handler,
        stopped: stopped_for_claim(candidate, failure_class, claim)?,
        candidate: candidate.clone(),
        failure_class,
        receipt: PathBuf::from(receipt),
    })
}

fn stopped_for_claim(
    candidate: &Value,
    class: Option<FailureClass>,
    claim: &Claim,
) -> Result<StoppedOutcome, String> {
    use std::collections::BTreeMap;
    let ordinary = stopped(candidate, class)?;
    let Some(class) = class else {
        return Ok(ordinary);
    };
    let material = claim.to_value();
    let retry = material
        .get("retry")
        .ok_or("native completion retry policy missing")?;
    let attempt = material
        .get("attempt")
        .and_then(Value::as_num)
        .ok_or("native completion attempt missing")?
        .parse::<u32>()
        .map_err(|_| "native completion attempt invalid")?;
    let limit = retry
        .get("attempts")
        .and_then(Value::as_num)
        .ok_or("native completion attempt limit missing")?
        .parse::<u32>()
        .map_err(|_| "native completion attempt limit invalid")?;
    let admitted = retry
        .get("on")
        .and_then(Value::as_arr)
        .ok_or("native completion retry classes missing")?
        .iter()
        .any(|value| value.as_str() == Some(class.as_str()));
    if !admitted || attempt < limit {
        return Ok(ordinary);
    }
    let mut result = candidate
        .as_obj()
        .ok_or("native completion candidate missing")?
        .clone();
    result.insert(
        "error".into(),
        Value::Str(crate::error::RETRIES_EXHAUSTED.into()),
    );
    result.insert("class".into(), Value::Str(class.as_str().into()));
    result.insert("attempts".into(), Value::Num(attempt.to_string()));
    StoppedOutcome::from_value(&Value::Obj(BTreeMap::from([
        ("status".into(), Value::Str(ordinary.status().into())),
        ("result".into(), Value::Obj(result)),
    ])))
    .map_err(|error| error.to_string())
}

fn stopped(candidate: &Value, class: Option<FailureClass>) -> Result<StoppedOutcome, String> {
    use std::collections::BTreeMap;
    let (status, expected) = match candidate.get("error") {
        Some(Value::Str(error)) => match error.as_str() {
            "exec/cancelled" => ("interrupted", None),
            "exec/mcp_protocol" => ("failed", None),
            "exec/timeout" => ("timeout", Some(FailureClass::Timeout)),
            "exec/spawn" => ("spawn", Some(FailureClass::Spawn)),
            "mcp/tool_error" | "mcp/rpc_error" => ("mcp_error", Some(FailureClass::McpError)),
            _ => return Err("native completion candidate error unknown".into()),
        },
        Some(_) => return Err("native completion candidate error invalid".into()),
        None => match candidate.get("status") {
            None => ("ok", None),
            Some(Value::Num(raw)) => {
                let code = raw
                    .parse::<i32>()
                    .map_err(|_| "native completion process status invalid")?;
                if !(-1..=255).contains(&code) || raw != &code.to_string() {
                    return Err("native completion process status invalid".into());
                }
                if code == 0 {
                    ("ok", None)
                } else {
                    ("nonzero_exit", Some(FailureClass::NonzeroExit))
                }
            }
            Some(_) => return Err("native completion process status invalid".into()),
        },
    };
    if expected != class {
        return Err("native completion candidate and failure class differ".into());
    }
    if let Some(Value::Str(error)) = candidate.get("error") {
        let process_error = matches!(
            error.as_str(),
            "exec/cancelled" | "exec/timeout" | "exec/spawn"
        );
        if (process_error && candidate.get("status") != Some(&Value::Num("-1".into())))
            || (!process_error && candidate.get("status").is_some())
        {
            return Err("native completion candidate error/status differ".into());
        }
    }
    StoppedOutcome::from_value(&Value::Obj(BTreeMap::from([
        ("status".into(), Value::Str(status.into())),
        ("result".into(), candidate.clone()),
    ])))
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn exhaustion_uses_original_policy_and_preserves_raw_capture() {
        let candidate = parse(
            br#"{"error":"exec/timeout","status":-1,"stderr":"kept"}"#,
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        for (limit, admitted, exhausted) in
            [(3u32, true, false), (1, true, true), (1, false, false)]
        {
            let mut fields = claim().to_value().as_obj().unwrap().clone();
            let mut retry = fields.get("retry").unwrap().as_obj().unwrap().clone();
            retry.insert("attempts".into(), Value::Num(limit.to_string()));
            retry.insert(
                "on".into(),
                Value::Arr(if admitted {
                    vec![Value::Str("timeout".into())]
                } else {
                    vec![]
                }),
            );
            fields.insert("retry".into(), Value::Obj(retry));
            let original = Claim::from_value(&Value::Obj(fields)).unwrap();
            let outcome =
                stopped_for_claim(&candidate, Some(FailureClass::Timeout), &original).unwrap();
            assert_eq!(outcome.status(), "timeout");
            let result = outcome.result().unwrap();
            if exhausted {
                assert_eq!(
                    result.get("error").and_then(Value::as_str),
                    Some(crate::error::RETRIES_EXHAUSTED)
                );
                assert_eq!(result.get("class").and_then(Value::as_str), Some("timeout"));
                assert_eq!(result.get("attempts"), Some(&Value::Num("1".into())));
                assert_eq!(result.get("stderr"), candidate.get("stderr"));
                assert_eq!(result.get("status"), candidate.get("status"));
            } else {
                assert_eq!(result, &candidate);
            }
            let protocol =
                parse(br#"{"error":"exec/mcp_protocol"}"#, &JsonLimits::DEFAULT).unwrap();
            assert_eq!(
                stopped_for_claim(&protocol, None, &original)
                    .unwrap()
                    .result(),
                Some(&protocol)
            );
        }
        assert_eq!(
            candidate.get("error").and_then(Value::as_str),
            Some("exec/timeout")
        );
    }

    #[test]
    fn stopped_mapping_preserves_candidates_and_refuses_class_contradictions() {
        for (encoded, class, status) in [
            ("{}", None, "ok"),
            (r#"{"status":0}"#, None, "ok"),
            (
                r#"{"status":7}"#,
                Some(FailureClass::NonzeroExit),
                "nonzero_exit",
            ),
            (r#"{"error":"exec/mcp_protocol"}"#, None, "failed"),
            (
                r#"{"error":"exec/cancelled","status":-1}"#,
                None,
                "interrupted",
            ),
            (
                r#"{"error":"exec/timeout","status":-1}"#,
                Some(FailureClass::Timeout),
                "timeout",
            ),
            (
                r#"{"error":"mcp/tool_error"}"#,
                Some(FailureClass::McpError),
                "mcp_error",
            ),
        ] {
            let candidate = parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap();
            let outcome = stopped(&candidate, class).unwrap();
            assert_eq!(outcome.status(), status);
            assert_eq!(outcome.result(), Some(&candidate));
        }
        for (encoded, class) in [
            (r#"{"status":7}"#, None),
            (r#"{"status":0}"#, Some(FailureClass::NonzeroExit)),
            (r#"{"status":-0}"#, None),
            (r#"{"error":"unknown"}"#, None),
            (
                r#"{"error":"exec/timeout","status":0}"#,
                Some(FailureClass::Timeout),
            ),
            (r#"{"error":"exec/mcp_protocol","status":-1}"#, None),
        ] {
            let candidate = parse(encoded.as_bytes(), &JsonLimits::DEFAULT).unwrap();
            assert!(stopped(&candidate, class).is_err());
        }
    }

    fn handler() -> crate::config::HandlerSpec {
        crate::config::HandlerTable::parse(r#"{"format":"fsm.handlers/1","handlers":[{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"retry":{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}}]}"#).unwrap().handlers.remove("notify").unwrap()
    }

    fn claim() -> Claim {
        Claim::from_value(&parse(format!(r#"{{"run_id":1,"instance_id":"instance","effect_id":"effect","attempt":1,"handler_fingerprint":"{}","retry":{{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}},"domain":{{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{{"device":0,"inode":42}},"authority":{{"device":8,"inode":43}},"generation":9}}}}"#, handler().fingerprint()).as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap()
    }

    fn response(claim: &Claim, hash: &str) -> Value {
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-response/1".into())),
            ("ok".into(), Value::Bool(true)),
            ("result".into(), Value::Obj(BTreeMap::from([
                ("format".into(), Value::Str("fsm.native-run-result/3".into())),
                ("handler_kind".into(), Value::Str("process".into())),
                ("handler_contract".into(), handler().contract_value()),
                ("claim".into(), claim.to_value()),
                ("journal_claim".into(), Value::Str(hash.into())),
                ("receipt".into(), Value::Str("/var/lib/fsm-containment/0123456789abcdef0123456789abcdef/authority-9/closure-7-1.json".into())),
                ("candidate".into(), Value::Obj(BTreeMap::from([("status".into(), Value::Num("0".into()))]))),
                ("failure_class".into(), Value::Null),
            ]))),
        ]))
    }

    #[test]
    fn result_material_is_bound_before_any_native_receipt_read() {
        let claim = claim();
        let hash = format!("sha256:{}", "b".repeat(64));
        let response = response(&claim, &hash);
        let valid = validate(&response, &claim, &hash).unwrap();
        assert_eq!(valid.candidate.get("status"), Some(&Value::Num("0".into())));
        assert_eq!(valid.failure_class, None);
        assert_eq!(valid.handler, handler());
        let mut changed_contract = handler();
        changed_contract.timeout_ms += 1;
        let mut old_claim = claim.to_value().as_obj().unwrap().clone();
        old_claim.insert("run_id".into(), Value::Num("2".into()));
        let original = response.get("result").unwrap().as_obj().unwrap();
        for (field, replacement) in [
            ("claim", Value::Obj(old_claim)),
            (
                "journal_claim",
                Value::Str(format!("sha256:{}", "c".repeat(64))),
            ),
            (
                "receipt",
                Value::Str("/var/lib/fsm-containment/other.json".into()),
            ),
            ("failure_class", Value::Str("unknown".into())),
            ("handler_kind", Value::Str("mcp".into())),
            ("handler_kind", Value::Str("unknown".into())),
            ("handler_contract", Value::Null),
            ("handler_contract", changed_contract.contract_value()),
            ("format", Value::Str("fsm.native-run-result/2".into())),
            ("candidate", Value::Null),
        ] {
            let mut result = original.clone();
            result.insert(field.into(), replacement);
            let mut changed = response.as_obj().unwrap().clone();
            changed.insert("result".into(), Value::Obj(result));
            assert!(validate(&Value::Obj(changed), &claim, &hash).is_err());
        }
        let mut changed = response.as_obj().unwrap().clone();
        changed.insert("ok".into(), Value::Bool(false));
        assert!(validate(&Value::Obj(changed), &claim, &hash).is_err());
    }
}
