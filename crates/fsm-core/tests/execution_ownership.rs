//! Independent ownership transitions and canonical fixtures from reserved SPEC.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{
    Admission, Claim, Closure, ExecutionState, PendingEffect, Settlement, ShapeError, Stopped,
    StoppedOutcome,
};

fn json(text: &str) -> Value {
    parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

fn set(value: &mut Value, field: &str, replacement: Value) {
    if let Value::Obj(fields) = value {
        fields.insert(field.into(), replacement);
    } else {
        panic!("expected object");
    }
}

fn claim_value(run_id: u64, effect: &str, attempt: u32) -> Value {
    let mut value = json(
        r#"{
        "run_id":1,"instance_id":"instance","effect_id":"effect","attempt":1,
        "handler_fingerprint":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "retry":{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]},
        "domain":{"backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef",
          "allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef",
          "cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},"generation":9}
    }"#,
    );
    set(&mut value, "run_id", Value::Num(run_id.to_string()));
    set(&mut value, "effect_id", Value::Str(effect.into()));
    set(&mut value, "attempt", Value::Num(attempt.to_string()));
    value
}

fn claim(run_id: u64, effect: &str, attempt: u32) -> Claim {
    Claim::from_value(&claim_value(run_id, effect, attempt)).unwrap()
}

fn stopped(claim: &Claim, status: &str) -> Stopped {
    Stopped::new(
        Closure::new(
            claim.run_id(),
            claim.domain().clone(),
            format!("sha256:{}", "b".repeat(64)),
        )
        .unwrap(),
        StoppedOutcome::from_value(&json(&format!(r#"{{"status":"{status}","result":null}}"#)))
            .unwrap(),
    )
}

fn recovered(state: &ExecutionState) -> ExecutionState {
    let bytes = canon_bytes(&state.to_value());
    let value = parse(&bytes, &JsonLimits::DEFAULT).unwrap();
    let restored = ExecutionState::from_value(&value).unwrap();
    assert_eq!(restored, *state);
    assert_eq!(canon_bytes(&restored.to_value()), bytes);
    restored
}

#[test]
fn stopped_ownership_survives_recovery_and_retry_requires_single_settlement() {
    let mut state = ExecutionState::new(Admission::Enabled);
    let first = claim(1, "effect", 1);
    state
        .claim(first.clone(), PendingEffect::Present, 100)
        .unwrap();
    state = recovered(&state);
    state.stop(&first, stopped(&first, "timeout")).unwrap();
    state = recovered(&state);
    let before = state.clone();
    assert_eq!(
        state.claim(claim(2, "effect", 1), PendingEffect::Present, 200),
        Err(ShapeError("owned"))
    );
    assert_eq!(
        state.stop(&first, stopped(&first, "ok")),
        Err(ShapeError("already_stopped"))
    );
    assert_eq!(state, before);
    state
        .settle(&first, Settlement::Attempted, PendingEffect::Present, 100)
        .unwrap();
    state = recovered(&state);
    assert_eq!(state.failed_count("instance", "effect"), 1);
    assert_eq!(state.run_high_water(), 1);
    let before = state.clone();
    assert!(
        state
            .settle(&first, Settlement::Attempted, PendingEffect::Present, 100)
            .is_err()
    );
    assert_eq!(
        state.claim(claim(2, "effect", 2), PendingEffect::Present, 109),
        Err(ShapeError("retry_ineligible"))
    );
    assert_eq!(state, before);
    let second = claim(2, "effect", 2);
    state
        .claim(second.clone(), PendingEffect::Present, 110)
        .unwrap();
    state = recovered(&state);
    assert!(state.stop(&first, stopped(&first, "ok")).is_err());
    state.stop(&second, stopped(&second, "ok")).unwrap();
    state
        .settle(&second, Settlement::Acked, PendingEffect::Present, 111)
        .unwrap();
    assert_eq!(state.failed_count("instance", "effect"), 0);
    assert!(state.claim_for("instance", "effect").is_none());
    assert_eq!(recovered(&state).next_run_id().unwrap(), 3);
}

#[test]
fn stale_observation_quarantine_and_exhaustion_do_not_burn_allocations() {
    for admission in [Admission::Enabled, Admission::Quarantined] {
        let mut state = ExecutionState::new(admission);
        let before = state.clone();
        assert!(
            state
                .claim(claim(1, "effect", 1), PendingEffect::Absent, 0)
                .is_err()
        );
        assert_eq!(state, before);
    }
    let mut state = ExecutionState::new(Admission::Enabled);
    let before = state.clone();
    assert!(
        state
            .claim(claim(2, "effect", 1), PendingEffect::Present, 0)
            .is_err()
    );
    assert_eq!(state, before);
    let mut value = state.to_value();
    set(
        &mut value,
        "run_high_water",
        Value::Num(u64::MAX.to_string()),
    );
    let mut state = ExecutionState::from_value(&value).unwrap();
    let before = state.clone();
    assert_eq!(state.next_run_id(), Err(ShapeError("run_exhausted")));
    assert!(
        state
            .claim(claim(1, "effect", 1), PendingEffect::Present, 0)
            .is_err()
    );
    assert_eq!(state, before);
}

#[test]
fn native_closure_must_bind_the_full_domain_and_run() {
    let mut state = ExecutionState::new(Admission::Enabled);
    let owned = claim(1, "effect", 1);
    state
        .claim(owned.clone(), PendingEffect::Present, 0)
        .unwrap();
    let before = state.clone();
    let wrong_run = claim(2, "effect", 1);
    assert_eq!(
        state.stop(&owned, stopped(&wrong_run, "ok")),
        Err(ShapeError("closure_binding"))
    );
    let mut changed = owned.to_value();
    let mut domain = changed.get("domain").unwrap().clone();
    set(&mut domain, "generation", Value::Num("10".into()));
    set(&mut changed, "domain", domain);
    let wrong_authority = Claim::from_value(&changed).unwrap();
    assert_eq!(
        state.stop(&owned, stopped(&wrong_authority, "ok")),
        Err(ShapeError("closure_binding"))
    );
    assert_eq!(state, before);
    assert!(
        state
            .settle(&owned, Settlement::Interrupted, PendingEffect::Absent, 0)
            .is_err()
    );
}

#[test]
fn interruption_retains_retry_count_and_external_removal_cannot_erase_ownership() {
    let mut state = ExecutionState::new(Admission::Enabled);
    let first = claim(1, "effect", 1);
    state
        .claim(first.clone(), PendingEffect::Present, 0)
        .unwrap();
    state.stop(&first, stopped(&first, "timeout")).unwrap();
    state
        .settle(&first, Settlement::Attempted, PendingEffect::Present, 100)
        .unwrap();
    let second = claim(2, "effect", 2);
    state
        .claim(second.clone(), PendingEffect::Present, 110)
        .unwrap();
    state
        .stop(&second, stopped(&second, "interrupted"))
        .unwrap();
    state
        .settle(
            &second,
            Settlement::Interrupted,
            PendingEffect::Present,
            110,
        )
        .unwrap();
    assert_eq!(state.failed_count("instance", "effect"), 1);
    let third = claim(3, "effect", 2);
    state
        .claim(third.clone(), PendingEffect::Present, 110)
        .unwrap();
    state.effect_removed("instance", "effect");
    state = recovered(&state);
    assert!(state.claim_for("instance", "effect").is_some());
    state.stop(&third, stopped(&third, "ok")).unwrap();
    let before = state.clone();
    assert!(
        state
            .settle(&third, Settlement::Acked, PendingEffect::Absent, 110)
            .is_err()
    );
    assert_eq!(state, before);
    state
        .settle(&third, Settlement::Interrupted, PendingEffect::Absent, 110)
        .unwrap();
    assert_eq!(state.failed_count("instance", "effect"), 0);
    assert_eq!(recovered(&state).run_high_water(), 3);
}

#[test]
fn changed_handler_contract_cannot_reinterpret_a_retry_ledger() {
    let mut state = ExecutionState::new(Admission::Enabled);
    let first = claim(1, "effect", 1);
    state
        .claim(first.clone(), PendingEffect::Present, 0)
        .unwrap();
    state.stop(&first, stopped(&first, "timeout")).unwrap();
    state
        .settle(&first, Settlement::Attempted, PendingEffect::Present, 100)
        .unwrap();
    let before = state.clone();
    for field in ["handler_fingerprint", "retry"] {
        let mut value = claim_value(2, "effect", 2);
        let replacement = if field == "retry" {
            json(r#"{"attempts":3,"backoff_ms":1,"max_backoff_ms":40,"on":["timeout"]}"#)
        } else {
            Value::Str(format!("sha256:{}", "c".repeat(64)))
        };
        set(&mut value, field, replacement);
        assert_eq!(
            state.claim(
                Claim::from_value(&value).unwrap(),
                PendingEffect::Present,
                110
            ),
            Err(ShapeError("contract"))
        );
        assert_eq!(state, before);
    }
}

#[test]
fn metadata_and_outcome_bounds_count_escapes_and_accept_exact_limits() {
    let mut value = claim_value(1, "effect", 1);
    let overhead = canon_bytes(&value).len() - "effect".len();
    set(
        &mut value,
        "effect_id",
        Value::Str("e".repeat(4096 - overhead)),
    );
    assert_eq!(canon_bytes(&value).len(), 4096);
    assert!(Claim::from_value(&value).is_ok());
    set(
        &mut value,
        "effect_id",
        Value::Str("e".repeat(4097 - overhead)),
    );
    assert!(Claim::from_value(&value).is_err());
    let mut outcome = json(r#"{"status":"ok","result":""}"#);
    let overhead = canon_bytes(&outcome).len();
    // One control character costs six canonical bytes, not one source byte.
    set(
        &mut outcome,
        "result",
        Value::Str(format!(
            "\u{0001}{}",
            "é".repeat((65536 - overhead - 6) / 2)
        )),
    );
    let missing = 65536 - canon_bytes(&outcome).len();
    if let Some(Value::Str(result)) = outcome.as_obj().unwrap().get("result") {
        let result = format!("{result}{}", "x".repeat(missing));
        set(&mut outcome, "result", Value::Str(result));
    }
    assert_eq!(canon_bytes(&outcome).len(), 65536);
    assert!(StoppedOutcome::from_value(&outcome).is_ok());
    let result = format!("{}x", outcome.get("result").unwrap().as_str().unwrap());
    set(&mut outcome, "result", Value::Str(result));
    assert!(StoppedOutcome::from_value(&outcome).is_err());
}

#[test]
fn outcome_preserves_null_presence_and_rejects_unknown_status_or_fields() {
    let absent = StoppedOutcome::from_value(&json(r#"{"status":"ok"}"#)).unwrap();
    let null = StoppedOutcome::from_value(&json(r#"{"status":"ok","result":null}"#)).unwrap();
    assert_eq!(absent.result(), None);
    assert_eq!(null.result(), Some(&Value::Null));
    assert_ne!(absent, null);
    for invalid in [r#"{"status":"unknown"}"#, r#"{"status":"ok","pid":1}"#] {
        assert!(StoppedOutcome::from_value(&json(invalid)).is_err());
    }
    let mut deeply_nested = Value::Null;
    for _ in 0..66 {
        deeply_nested = Value::Arr(vec![deeply_nested]);
    }
    assert!(
        StoppedOutcome::from_value(&Value::Obj(BTreeMap::from([
            ("status".into(), Value::Str("ok".into())),
            ("result".into(), deeply_nested),
        ])))
        .is_err()
    );
}

fn block(claims: Vec<Value>, retry: Vec<Value>, high_water: u64) -> Value {
    Value::Obj(BTreeMap::from([
        ("admission".into(), Value::Str("enabled".into())),
        ("run_high_water".into(), Value::Num(high_water.to_string())),
        ("claims".into(), Value::Arr(claims)),
        ("retry".into(), Value::Arr(retry)),
    ]))
}

fn owned_value(claim: Value, stopped: Value) -> Value {
    Value::Obj(BTreeMap::from([
        ("claim".into(), claim),
        ("stopped".into(), stopped),
    ]))
}

#[test]
fn ownership_count_accepts_4096_and_refuses_4097_without_burning_a_run() {
    let claims: Vec<_> = (1..=4096)
        .map(|run| {
            owned_value(
                claim_value(run, &format!("effect-{run:04}"), 1),
                Value::Null,
            )
        })
        .collect();
    let value = block(claims.clone(), vec![], 4096);
    let mut state = ExecutionState::from_value(&value).unwrap();
    let before = state.clone();
    assert_eq!(
        state.claim(claim(4097, "extra", 1), PendingEffect::Present, 0),
        Err(ShapeError("entries"))
    );
    assert_eq!(state, before);
    let mut too_many = claims;
    too_many.push(owned_value(claim_value(4097, "extra", 1), Value::Null));
    assert!(ExecutionState::from_value(&block(too_many, vec![], 4097)).is_err());
}

#[test]
fn entry_limit_counts_the_union_of_retry_and_ownership() {
    let retry: Vec<_> = (1..=4096)
        .map(|number| {
            let metadata = claim_value(1, &format!("effect-{number:04}"), 1);
            Value::Obj(BTreeMap::from([
                ("instance_id".into(), Value::Str("instance".into())),
                (
                    "effect_id".into(),
                    metadata.get("effect_id").unwrap().clone(),
                ),
                (
                    "handler_fingerprint".into(),
                    metadata.get("handler_fingerprint").unwrap().clone(),
                ),
                ("retry".into(), metadata.get("retry").unwrap().clone()),
                ("failed_count".into(), Value::Num("1".into())),
                ("last_timestamp".into(), Value::Num("100".into())),
                ("failure_class".into(), Value::Str("timeout".into())),
                ("eligible_at".into(), Value::Num("110".into())),
            ]))
        })
        .collect();
    let mut state = ExecutionState::from_value(&block(vec![], retry.clone(), 0)).unwrap();
    state
        .claim(claim(1, "effect-0001", 2), PendingEffect::Present, 110)
        .unwrap();
    let before = recovered(&state);
    assert_eq!(
        state.claim(claim(2, "extra", 1), PendingEffect::Present, 110),
        Err(ShapeError("entries"))
    );
    assert_eq!(state, before);
    assert!(
        ExecutionState::from_value(&block(
            vec![owned_value(claim_value(1, "extra", 1), Value::Null)],
            retry,
            1
        ))
        .is_err()
    );
}

#[test]
fn complete_block_bound_counts_every_delimiter_identity_and_result() {
    const LIMIT: usize = 8 * 1024 * 1024;
    let mut entries: Vec<_> = (1..=128)
        .map(|run| {
            let claim = claim(run, &format!("effect-{run:03}"), 1);
            let mut outcome = json(r#"{"status":"ok","result":""}"#);
            let closure = stopped(&claim, "ok")
                .to_value()
                .get("closure")
                .unwrap()
                .clone();
            set(&mut outcome, "result", Value::Str(String::new()));
            owned_value(
                claim.to_value(),
                Value::Obj(BTreeMap::from([
                    ("closure".into(), closure),
                    ("outcome".into(), outcome),
                ])),
            )
        })
        .collect();
    let overhead = canon_bytes(&block(entries.clone(), vec![], 128)).len();
    let mut remaining = LIMIT - overhead;
    let outcome_overhead = canon_bytes(&json(r#"{"status":"ok","result":""}"#)).len();
    for entry in &mut entries {
        let amount = remaining.min(65536 - outcome_overhead);
        let mut stopped = entry.get("stopped").unwrap().clone();
        let mut outcome = stopped.get("outcome").unwrap().clone();
        set(&mut outcome, "result", Value::Str("x".repeat(amount)));
        set(&mut stopped, "outcome", outcome);
        set(entry, "stopped", stopped);
        remaining -= amount;
    }
    assert_eq!(remaining, 0);
    let exact = block(entries.clone(), vec![], 128);
    assert_eq!(canon_bytes(&exact).len(), LIMIT);
    assert!(ExecutionState::from_value(&exact).is_ok());
    let last_claim = Claim::from_value(entries[127].get("claim").unwrap()).unwrap();
    let last_stopped = Stopped::from_value(entries[127].get("stopped").unwrap()).unwrap();
    let mut unclosed = entries.clone();
    set(&mut unclosed[127], "stopped", Value::Null);
    let initial = ExecutionState::from_value(&block(unclosed, vec![], 128)).unwrap();
    let mut accepted = initial.clone();
    accepted.stop(&last_claim, last_stopped).unwrap();
    assert_eq!(accepted.to_value(), exact);
    let mut too_big_stopped = entries[127].get("stopped").unwrap().clone();
    let mut outcome = too_big_stopped.get("outcome").unwrap().clone();
    let result = format!("{}x", outcome.get("result").unwrap().as_str().unwrap());
    set(&mut outcome, "result", Value::Str(result));
    set(&mut too_big_stopped, "outcome", outcome);
    set(&mut entries[127], "stopped", too_big_stopped.clone());
    let too_big = block(entries, vec![], 128);
    assert_eq!(canon_bytes(&too_big).len(), LIMIT + 1);
    assert!(ExecutionState::from_value(&too_big).is_err());
    let mut refused = initial.clone();
    assert!(
        refused
            .stop(&last_claim, Stopped::from_value(&too_big_stopped).unwrap())
            .is_err()
    );
    assert_eq!(refused, initial);
}

#[test]
fn block_decoder_refuses_reordered_duplicate_or_contradictory_evidence() {
    let first = owned_value(claim_value(1, "first", 1), Value::Null);
    let second = owned_value(claim_value(2, "second", 1), Value::Null);
    assert!(
        ExecutionState::from_value(&block(vec![first.clone(), second.clone()], vec![], 2)).is_ok()
    );
    for entries in [
        vec![second.clone(), first.clone()],
        vec![first.clone(), first.clone()],
        vec![
            first.clone(),
            owned_value(claim_value(2, "first", 1), Value::Null),
        ],
    ] {
        assert!(ExecutionState::from_value(&block(entries, vec![], 2)).is_err());
    }
    assert!(ExecutionState::from_value(&block(vec![first], vec![], 0)).is_err());
    assert!(
        ExecutionState::from_value(&block(
            vec![owned_value(claim_value(1, "effect", 2), Value::Null)],
            vec![],
            1
        ))
        .is_err()
    );
    let mut mismatched = stopped(&claim(2, "second", 1), "ok").to_value();
    let mut closure = mismatched.get("closure").unwrap().clone();
    set(&mut closure, "run_id", Value::Num("1".into()));
    set(&mut mismatched, "closure", closure);
    assert!(
        ExecutionState::from_value(&block(
            vec![owned_value(claim_value(2, "second", 1), mismatched)],
            vec![],
            2
        ))
        .is_err()
    );
}

#[test]
fn settlement_refuses_to_apply_an_incompatible_stopped_result() {
    for (status, disposition) in [
        ("ok", Settlement::Attempted),
        ("ok", Settlement::Interrupted),
        ("interrupted", Settlement::Acked),
        ("interrupted", Settlement::Attempted),
    ] {
        let mut state = ExecutionState::new(Admission::Enabled);
        let claim = claim(1, "effect", 1);
        state
            .claim(claim.clone(), PendingEffect::Present, 0)
            .unwrap();
        state.stop(&claim, stopped(&claim, status)).unwrap();
        let before = state.clone();
        assert_eq!(
            state.settle(&claim, disposition, PendingEffect::Present, 0),
            Err(ShapeError("disposition"))
        );
        assert_eq!(state, before);
    }
}

#[test]
fn disallowed_failure_class_and_final_attempt_cannot_launch_a_retry() {
    for status in ["spawn", "timeout"] {
        let mut value = claim_value(1, "effect", 1);
        if status == "timeout" {
            let mut retry = value.get("retry").unwrap().clone();
            set(&mut retry, "attempts", Value::Num("1".into()));
            set(&mut value, "retry", retry);
        }
        let first = Claim::from_value(&value).unwrap();
        let mut state = ExecutionState::new(Admission::Enabled);
        state
            .claim(first.clone(), PendingEffect::Present, 0)
            .unwrap();
        state.stop(&first, stopped(&first, status)).unwrap();
        state
            .settle(&first, Settlement::Attempted, PendingEffect::Present, 100)
            .unwrap();
        let before = recovered(&state);
        set(&mut value, "run_id", Value::Num("2".into()));
        set(&mut value, "attempt", Value::Num("2".into()));
        match Claim::from_value(&value) {
            Ok(next) => assert_eq!(
                state.claim(next, PendingEffect::Present, i64::MAX),
                Err(ShapeError("retry_ineligible"))
            ),
            Err(error) => assert_eq!(error, ShapeError("claim_contract")),
        }
        assert_eq!(state, before);
    }
}

#[test]
fn claim_and_closure_decoders_refuse_malformed_closed_identities() {
    for (field, replacement) in [
        ("run_id", Value::Num("0".into())),
        ("run_id", Value::Num("01".into())),
        ("attempt", Value::Num("0".into())),
        ("attempt", Value::Num("4".into())),
        ("instance_id", Value::Str(String::new())),
        ("effect_id", Value::Str(String::new())),
        (
            "handler_fingerprint",
            Value::Str(format!("sha256:{}", "A".repeat(64))),
        ),
        ("handler_fingerprint", Value::Str("sha256:abc".into())),
        ("extra", Value::Null),
    ] {
        let mut value = claim_value(1, "effect", 1);
        set(&mut value, field, replacement);
        assert!(Claim::from_value(&value).is_err(), "{field}");
    }
    let claim = claim(1, "effect", 1);
    assert!(
        Closure::new(
            0,
            claim.domain().clone(),
            format!("sha256:{}", "b".repeat(64))
        )
        .is_err()
    );
    let closure = stopped(&claim, "ok")
        .to_value()
        .get("closure")
        .unwrap()
        .clone();
    for (field, replacement) in [
        ("receipt", Value::Str("untrusted".into())),
        ("run_id", Value::Num("0".into())),
        ("pid", Value::Num("1".into())),
    ] {
        let mut value = closure.clone();
        set(&mut value, field, replacement);
        assert!(Closure::from_value(&value).is_err());
    }
}

#[test]
fn retry_decoder_refuses_deadline_count_contract_and_order_tampering() {
    let entry = json(
        r#"{
        "instance_id":"instance","effect_id":"effect",
        "handler_fingerprint":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "retry":{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout"]},
        "failed_count":1,"last_timestamp":100,"failure_class":"timeout","eligible_at":110
    }"#,
    );
    assert!(ExecutionState::from_value(&block(vec![], vec![entry.clone()], 1)).is_ok());
    for (field, replacement) in [
        ("eligible_at", Value::Num("109".into())),
        ("failed_count", Value::Num("0".into())),
        ("failed_count", Value::Num("4".into())),
        ("failure_class", Value::Str("interrupted".into())),
        ("instance_id", Value::Str(String::new())),
        ("extra", Value::Null),
    ] {
        let mut changed = entry.clone();
        set(&mut changed, field, replacement);
        assert!(
            ExecutionState::from_value(&block(vec![], vec![changed], 1)).is_err(),
            "{field}"
        );
    }
    assert!(
        ExecutionState::from_value(&block(vec![], vec![entry.clone(), entry.clone()], 1)).is_err()
    );
    let mut earlier = entry.clone();
    set(&mut earlier, "effect_id", Value::Str("earlier".into()));
    assert!(ExecutionState::from_value(&block(vec![], vec![entry.clone(), earlier], 1)).is_err());
    let mut mismatched = claim_value(2, "effect", 2);
    set(
        &mut mismatched,
        "handler_fingerprint",
        Value::Str(format!("sha256:{}", "b".repeat(64))),
    );
    assert!(
        ExecutionState::from_value(&block(
            vec![owned_value(mismatched, Value::Null)],
            vec![entry.clone()],
            2
        ))
        .is_err()
    );
    assert!(
        ExecutionState::from_value(&block(
            vec![owned_value(claim_value(2, "effect", 1), Value::Null)],
            vec![entry],
            2
        ))
        .is_err()
    );
}

#[test]
fn canonical_accounting_charges_all_short_escapes_and_nested_value_types() {
    let mut outcome =
        json(r#"{"status":"ok","result":{"array":[1,true,false,null,"é"],"padding":""}}"#);
    let prefix = "\"\\\n\r\t\u{0008}\u{000c}";
    let mut result = outcome.get("result").unwrap().clone();
    set(&mut result, "padding", Value::Str(prefix.into()));
    set(&mut outcome, "result", result.clone());
    let overhead = canon_bytes(&outcome).len();
    set(
        &mut result,
        "padding",
        Value::Str(format!("{prefix}{}", "x".repeat(65536 - overhead))),
    );
    set(&mut outcome, "result", result.clone());
    assert_eq!(canon_bytes(&outcome).len(), 65536);
    assert!(StoppedOutcome::from_value(&outcome).is_ok());
    let padding = format!("{}x", result.get("padding").unwrap().as_str().unwrap());
    set(&mut result, "padding", Value::Str(padding));
    set(&mut outcome, "result", result);
    assert_eq!(canon_bytes(&outcome).len(), 65537);
    assert!(StoppedOutcome::from_value(&outcome).is_err());
}
