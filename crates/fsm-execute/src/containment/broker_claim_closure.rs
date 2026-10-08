//! Check the protected original binding before any closure side effect.

use super::super::{
    authority_lock, closed, closing, closure, number, protected_directory, read_value, stop, text,
};
use fsm_core::{json::Value, record::execution::Claim};
use std::path::Path;

pub(super) fn reconcile_claimed(directory: &Path, payload: &Value) -> Result<Value, String> {
    let claim = claimed(payload)?;
    let allocation = number(&claim.domain().to_value(), "allocation")?;
    let prepared = read_value(&directory.join(format!("prepared-{allocation}.json")), true);
    let _owner = if prepared.as_ref().is_ok_and(|prepared| {
        prepared.get("phase").and_then(Value::as_str) == Some("prepared-owned")
    }) {
        Some(super::super::owner_lease::acquire(
            directory,
            allocation,
            super::super::broker_endpoint::operator(directory)?,
        )?)
    } else {
        None
    };
    // Never create missing ownership material or race original publication.
    let _original_runner = super::super::runner_lease::acquire_existing(directory, allocation)?;
    refuse_completion_material(directory, allocation, claim.run_id())?;
    if _owner.is_some() {
        match std::fs::symlink_metadata(directory.join(format!("binding-{allocation}.json"))) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Both original leases exclude preparation and entry while bind
                // authenticates the current claim against the original store.
                super::super::bind(directory, payload)?;
            }
            Err(error) => return Err(super::super::io(error)),
        }
    }
    close_claimed(directory, payload)
}

fn refuse_completion_material(
    directory: &Path,
    allocation: u64,
    run_id: u64,
) -> Result<(), String> {
    for name in [
        format!("result-{allocation}-{run_id}.json"),
        format!("completed-{allocation}-{run_id}.json"),
        format!("completed-{allocation}-{run_id}.json.pending"),
    ] {
        match std::fs::symlink_metadata(directory.join(name)) {
            Ok(_) => {
                return Err(
                    "original completion material requires authenticated result recovery".into(),
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(super::super::io(error)),
        }
    }
    Ok(())
}

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
    #[ignore = "requires a provisioned root-owned cache directory"]
    fn reconciliation_preserves_complete_and_partial_original_result_material() {
        let root =
            std::path::PathBuf::from(std::env::var_os("FSM_RUNNER_LEASE_NATIVE_ROOT").unwrap());
        assert!(!root.starts_with("/tmp"));
        protected_directory(&root).unwrap();
        let directory = root.join(format!("reconcile-result-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        drop(super::super::super::runner_lease::acquire(&directory, 7).unwrap());
        let binding = original_binding();
        for name in [
            "result-7-1.json",
            "completed-7-1.json",
            "completed-7-1.json.pending",
        ] {
            let path = directory.join(name);
            std::fs::write(&path, b"partial original result").unwrap();
            let refusal = reconcile_claimed(&directory, &binding);
            let preserved = std::fs::read(&path).unwrap();
            let unchanged = std::fs::read_dir(&directory).unwrap().count() == 2;
            std::fs::remove_file(path).unwrap();
            assert_eq!(
                refusal.unwrap_err(),
                "original completion material requires authenticated result recovery"
            );
            assert_eq!(preserved, b"partial original result");
            assert!(unchanged);
        }
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    #[ignore = "requires a provisioned root-owned cache directory"]
    fn reconciliation_refuses_live_or_missing_lease_before_authority_mutation() {
        let root = std::path::PathBuf::from(
            std::env::var_os("FSM_RUNNER_LEASE_NATIVE_ROOT")
                .expect("native reconciliation test requires a protected cache root"),
        );
        assert!(!root.starts_with("/tmp"));
        protected_directory(&root).unwrap();
        let directory = root.join(format!("reconcile-entry-{}", std::process::id()));
        std::fs::create_dir(&directory).unwrap();
        let binding = original_binding();
        let missing = reconcile_claimed(&directory, &binding);
        let missing_unchanged = std::fs::read_dir(&directory).unwrap().count() == 0;
        let lease = super::super::super::runner_lease::acquire(&directory, 7).unwrap();
        let active = reconcile_claimed(&directory, &binding);
        let active_unchanged = std::fs::read_dir(&directory).unwrap().count() == 1;
        drop(lease);
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(missing.is_err());
        assert!(missing_unchanged);
        assert_eq!(
            active.unwrap_err(),
            "original runner remains active or lease locking is unavailable"
        );
        assert!(active_unchanged);
    }

    fn original_binding() -> Value {
        let fixture = parse(
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../fsm-core/tests/fixtures/execution-handoff.json"
            )),
            &JsonLimits::DEFAULT,
        )
        .unwrap();
        Value::Obj(std::collections::BTreeMap::from([
            (
                "format".into(),
                Value::Str("fsm.native-claim-binding/1".into()),
            ),
            ("claim".into(), fixture.get("claim").unwrap().clone()),
            (
                "journal_claim".into(),
                fixture.get("original_claim_hash").unwrap().clone(),
            ),
        ]))
    }

    #[test]
    fn matching_original_binding_requires_full_claim_hash_and_domain() {
        let original = original_binding();
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
