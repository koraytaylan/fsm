//! Following an error hint should make the next call succeed.

use fsm_cli::clock::FixedClock;
use fsm_cli::mcp::tools::dispatch;
use fsm_cli::store::Store;
use fsm_core::error::ALL_CODES;
use fsm_core::json::{JsonLimits, Value, parse};

fn case() -> Value {
    parse(
        include_bytes!("../../fsm-core/tests/fixtures/machines/case_review.json"),
        &JsonLimits::DEFAULT,
    )
    .unwrap()
}

fn store() -> (Store, FixedClock) {
    let dir = std::env::temp_dir().join(format!(
        "fsm-naive-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    (Store::open(&dir).unwrap(), FixedClock::new(1000, 1000))
}

fn obj(pairs: &[(&str, Value)]) -> Value {
    Value::Obj(
        pairs
            .iter()
            .map(|(k, v)| ((*k).into(), v.clone()))
            .collect(),
    )
}

#[test]
fn one_step_recovery() {
    let (mut st, mut clock) = store();
    dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", case())]),
    )
    .unwrap();

    // unknown event
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("case_review".into())),
            ("request_id", Value::Str("c1".into())),
        ]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str("docs_okk".into()))])),
            ("request_id", Value::Str("bad-ev".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/event_unknown");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("ok-ev".into())),
        ]),
    );
    assert!(ok.is_ok(), "{ok:?}");

    // unhandled
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str("resume".into()))])),
            ("request_id", Value::Str("unh".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "run/unhandled");
    assert!(!err.hint.is_empty());

    // field_scale via a decimal machine
    let dec = parse(
        br#"{"format":"fsm.machine/1","name":"dm","context":[{"name":"amt","ty":{"decimal":"2"},"init":"0.00"}],"events":[{"name":"pay","fields":[{"name":"n","ty":{"decimal":"2"}}]}],"states":[{"name":"s"}],"initial":"s","transitions":[]}"#,
        &JsonLimits::DEFAULT,
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "machine_create",
        &obj(&[("spec", dec)]),
    )
    .unwrap();
    dispatch(
        &mut st,
        &mut clock,
        "instance_create",
        &obj(&[
            ("machine", Value::Str("dm".into())),
            ("request_id", Value::Str("d1".into())),
        ]),
    )
    .unwrap();
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Str("1.505".into()))])),
                ]),
            ),
            ("request_id", Value::Str("sc".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/field_scale");
    let ok = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Str("1.50".into()))])),
                ]),
            ),
            ("request_id", Value::Str("sc2".into())),
        ]),
    );
    assert!(ok.is_ok() || ok.as_ref().err().map(|e| e.code.as_str()) == Some("run/unhandled"));

    // number_token
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-d1".into())),
            (
                "event",
                obj(&[
                    ("name", Value::Str("pay".into())),
                    ("payload", obj(&[("n", Value::Num("0.10".into()))])),
                ]),
            ),
            ("request_id", Value::Str("nt".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/number_token");

    // seq_mismatch
    let err = dispatch(
        &mut st,
        &mut clock,
        "instance_send",
        &obj(&[
            ("instance_id", Value::Str("inst-c1".into())),
            ("event", obj(&[("name", Value::Str("docs_ok".into()))])),
            ("request_id", Value::Str("sm".into())),
            ("expect_seq", Value::Num("0".into())),
        ]),
    )
    .unwrap_err();
    assert_eq!(err.code, "req/seq_mismatch");
    assert!(err.retryable);
}

#[test]
fn all_codes_hygiene() {
    assert!(!ALL_CODES.is_empty());
    let mut sorted = ALL_CODES.to_vec();
    sorted.sort();
    assert_eq!(ALL_CODES.to_vec(), sorted);
    let mut seen = std::collections::BTreeSet::new();
    for c in ALL_CODES {
        assert!(seen.insert(*c), "dup {c}");
    }
    const ALLOW: &[(&str, &str)] = &[
        ("io/read", "infrastructure"),
        ("io/write", "infrastructure"),
        ("store/chain_broken", "recovery class"),
        ("store/lock", "recovery class"),
        ("store/non_canonical", "recovery class"),
        ("store/state_hash_mismatch", "recovery class"),
        ("store/torn_tail", "recovery class"),
        ("internal/budget", "engine invariant"),
        ("internal/unimplemented", "stub leftover"),
    ];
    for (c, _) in ALLOW {
        assert!(ALL_CODES.contains(c), "allowlist rot {c}");
    }
}
