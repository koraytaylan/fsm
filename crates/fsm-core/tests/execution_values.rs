//! Independent values from SPEC's reserved claim-era persistence contract.
use fsm_core::json::{JsonLimits, Value, parse};
use fsm_core::record::execution::{FailureClass, FileIdentity, NativeDomain, RetryPolicy};

fn json(text: &str) -> Value {
    parse(text.as_bytes(), &JsonLimits::DEFAULT).unwrap()
}

#[test]
fn retry_policy_has_canonical_closed_persistence() {
    let policy = RetryPolicy::new(
        3,
        10,
        40,
        vec![
            FailureClass::Timeout,
            FailureClass::Spawn,
            FailureClass::Timeout,
        ],
    )
    .unwrap();
    let expected =
        json(r#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["spawn","timeout"]}"#);
    assert_eq!(policy.to_value(), expected);
    assert_eq!(RetryPolicy::from_value(&expected).unwrap(), policy);
    for bad in [
        r#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["timeout","spawn"]}"#,
        r#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["spawn","spawn"]}"#,
        r#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":["cancelled"]}"#,
        r#"{"attempts":3,"backoff_ms":10,"max_backoff_ms":40,"on":[],"unknown":0}"#,
    ] {
        assert!(RetryPolicy::from_value(&json(bad)).is_err());
    }
}

#[test]
fn retry_boundary_and_exhaustion_use_the_durable_failed_count() {
    let policy = RetryPolicy::new(3, 10, 40, vec![FailureClass::Timeout]).unwrap();
    assert_eq!(policy.ready_at(1, 100).unwrap(), 110);
    assert!(!policy.admits_retry(1, FailureClass::Timeout, 100, 109));
    assert!(policy.admits_retry(1, FailureClass::Timeout, 100, 110));
    assert_eq!(policy.ready_at(2, 100).unwrap(), 120);
    assert!(!policy.admits_retry(3, FailureClass::Timeout, 100, i64::MAX));
    assert!(!policy.admits_retry(1, FailureClass::Spawn, 100, i64::MAX));
    assert!(policy.ready_at(0, 100).is_err());
    assert!(policy.ready_at(17, 100).is_err());
    assert!(RetryPolicy::new(16, 1, 1, vec![]).is_ok());
    assert!(RetryPolicy::new(17, 1, 1, vec![]).is_err());
    assert!(RetryPolicy::new(1, 0, 1, vec![]).is_err());
    assert!(RetryPolicy::new(1, 2, 1, vec![]).is_err());
}

#[test]
fn retry_arithmetic_saturates_instead_of_becoming_eligible_early() {
    let policy = RetryPolicy::new(16, i64::MAX / 2, i64::MAX, vec![FailureClass::Timeout]).unwrap();
    assert_eq!(policy.ready_at(16, 100).unwrap(), i64::MAX);
    assert!(!policy.admits_retry(15, FailureClass::Timeout, 100, i64::MAX - 1));
    assert!(policy.admits_retry(15, FailureClass::Timeout, 100, i64::MAX));
}

#[test]
fn retry_class_count_accepts_four_and_refuses_five_before_normalization() {
    let four = vec![
        FailureClass::Timeout,
        FailureClass::Spawn,
        FailureClass::NonzeroExit,
        FailureClass::McpError,
    ];
    assert!(RetryPolicy::new(16, 1, 1, four.clone()).is_ok());
    let mut five = four;
    five.push(FailureClass::Timeout);
    assert!(RetryPolicy::new(16, 1, 1, five).is_err());
    let too_many = json(
        r#"{"attempts":16,"backoff_ms":1,"max_backoff_ms":1,"on":["mcp_error","nonzero_exit","spawn","timeout","timeout"]}"#,
    );
    assert!(RetryPolicy::from_value(&too_many).is_err());
}

#[test]
fn native_domain_round_trips_full_authority_without_unknown_fields() {
    let value = json(
        r#"{
        "backend":"linux-systemd/1","namespace":"0123456789abcdef0123456789abcdef",
        "allocation":7,"boot":"01234567-89ab-cdef-0123-456789abcdef",
        "cgroup":{"device":0,"inode":42},"authority":{"device":8,"inode":43},
        "generation":9
    }"#,
    );
    let identity = NativeDomain::from_value(&value).unwrap();
    assert_eq!(identity.to_value(), value);
    for (field, replacement) in [
        ("backend", Value::Str("unknown/1".into())),
        ("namespace", Value::Str("0".repeat(33))),
        ("namespace", Value::Str("A".repeat(32))),
        ("boot", Value::Str("0".repeat(36))),
        ("allocation", Value::Num("0".into())),
        ("generation", Value::Num("0".into())),
        ("cgroup", json(r#"{"device":0,"inode":0}"#)),
        ("authority", json(r#"{"device":8,"inode":43,"pid":1}"#)),
    ] {
        let mut mutated = value.as_obj().unwrap().clone();
        mutated.insert(field.into(), replacement);
        assert!(
            NativeDomain::from_value(&Value::Obj(mutated)).is_err(),
            "{field}"
        );
    }
    assert!(FileIdentity::new(0, 0).is_err());
    let file = FileIdentity::new(0, 42).unwrap();
    assert_eq!((file.device(), file.inode()), (0, 42));
}
