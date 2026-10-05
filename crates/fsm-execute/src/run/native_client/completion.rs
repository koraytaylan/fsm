//! Claim-bound candidates with opaque native proof, without journal mutation.

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{Claim, FailureClass, StoppedOutcome};
use fsm_store::store::VerifiedClosure;
use std::path::PathBuf;

/// Checked candidate and matching closure; current ownership remains the store's job.
pub struct NativeCompletion {
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
        Ok(Self {
            candidate: material.candidate,
            failure_class: material.failure_class,
            proof,
            stopped: material.stopped,
        })
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

    /// Borrow the unchanged candidate mapped to its immutable stopped semantics.
    pub fn stopped_outcome(&self) -> &StoppedOutcome {
        &self.stopped
    }
}

struct Material {
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
    let bytes = canon_bytes(response);
    if bytes.len() > 65536 {
        return Err("native completion exceeds response bound".into());
    }
    parse(&bytes, &JsonLimits::DEFAULT).map_err(|_| "native completion exceeds JSON limits")?;
    closed(response, &["format", "ok", "result"])?;
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
            "claim",
            "journal_claim",
            "receipt",
            "candidate",
            "failure_class",
        ],
    )?;
    if result.get("format").and_then(Value::as_str) != Some("fsm.native-run-result/2")
        || result.get("claim") != Some(&claim.to_value())
        || result.get("journal_claim").and_then(Value::as_str) != Some(journal_claim)
    {
        return Err("native completion original identity differs".into());
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
        stopped: stopped(candidate, failure_class)?,
        candidate: candidate.clone(),
        failure_class,
        receipt: PathBuf::from(receipt),
    })
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

    fn claim() -> Claim {
        Claim::from_value(&parse(format!(r#"{{"run_id":1,"instance_id":"instance","effect_id":"effect","attempt":1,"handler_fingerprint":"sha256:{}","retry":{{"attempts":1,"backoff_ms":10,"max_backoff_ms":10,"on":[]}},"domain":{{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef","allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef","cgroup":{{"device":0,"inode":42}},"authority":{{"device":8,"inode":43}},"generation":9}}}}"#, "a".repeat(64)).as_bytes(), &JsonLimits::DEFAULT).unwrap()).unwrap()
    }

    fn response(claim: &Claim, hash: &str) -> Value {
        Value::Obj(BTreeMap::from([
            ("format".into(), Value::Str("fsm.native-response/1".into())),
            ("ok".into(), Value::Bool(true)),
            ("result".into(), Value::Obj(BTreeMap::from([
                ("format".into(), Value::Str("fsm.native-run-result/2".into())),
                ("handler_kind".into(), Value::Str("process".into())),
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
