//! Check the protected original binding before any closure side effect.

use super::super::{
    authority_lock, closed, closing, closure, number, protected_directory, read_value, stop, text,
};
use fsm_core::{json::Value, record::execution::Claim};
use std::path::Path;

pub(super) fn close_claimed(directory: &Path, payload: &Value) -> Result<Value, String> {
    protected_directory(directory)?;
    let claim = claimed(payload)?;
    let allocation = number(&claim.domain().to_value(), "allocation")?;
    {
        let _lock = authority_lock(directory)?;
        let original = read_value(&directory.join(format!("binding-{allocation}.json")), true)?;
        let domain = closing::recorded_domain(directory, allocation)?;
        matches_original(payload, &original, &domain)?;
    }
    // Binding and allocation publications are immutable and never reused.
    // Release the lock before fencing/completion, which each acquire it.
    let _ = stop::fence(directory, allocation);
    closure::complete(directory, allocation).map(|_| Value::Null)
}

fn claimed(binding: &Value) -> Result<Claim, String> {
    closed(binding, &["format", "claim", "journal_claim"])?;
    if text(binding, "format")? != "fsm.native-claim-binding/1" {
        return Err("claimed closure binding format differs".into());
    }
    let hash = text(binding, "journal_claim")?;
    if !hash.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Err("claimed closure original hash invalid".into());
    }
    Claim::from_value(
        binding
            .get("claim")
            .ok_or("claimed closure claim missing")?,
    )
    .map_err(|error| error.to_string())
}

fn matches_original(request: &Value, original: &Value, domain: &Value) -> Result<(), String> {
    let claim = claimed(request)?;
    claimed(original)?;
    if request != original || claim.domain().to_value() != *domain {
        return Err("claimed closure original protected binding differs".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fsm_core::json::{JsonLimits, parse};

    #[test]
    fn matching_original_binding_requires_full_claim_hash_and_domain() {
        let fixture = parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        let original = Value::Obj(std::collections::BTreeMap::from([
            (
                "format".into(),
                Value::Str("fsm.native-claim-binding/1".into()),
            ),
            ("claim".into(), fixture.get("claim").unwrap().clone()),
            (
                "journal_claim".into(),
                fixture.get("original_claim_hash").unwrap().clone(),
            ),
        ]));
        let domain = claimed(&original).unwrap().domain().to_value();
        assert!(matches_original(&original, &original, &domain).is_ok());
        let Value::Obj(mut wrong_hash) = original.clone() else {
            unreachable!()
        };
        wrong_hash.insert(
            "journal_claim".into(),
            Value::Str(format!("sha256:{}", "a".repeat(64))),
        );
        assert!(matches_original(&Value::Obj(wrong_hash), &original, &domain).is_err());
        let Value::Obj(mut wrong_run) = original.clone() else {
            unreachable!()
        };
        let Value::Obj(claim) = wrong_run.get_mut("claim").unwrap() else {
            unreachable!()
        };
        claim.insert("run_id".into(), Value::Num("2".into()));
        assert!(matches_original(&Value::Obj(wrong_run), &original, &domain).is_err());
        let Value::Obj(mut wrong_domain) = domain else {
            unreachable!()
        };
        wrong_domain.insert("allocation".into(), Value::Num("8".into()));
        assert!(matches_original(&original, &original, &Value::Obj(wrong_domain)).is_err());
        assert!(claimed(&Value::Num("7".into())).is_err());
    }
}
