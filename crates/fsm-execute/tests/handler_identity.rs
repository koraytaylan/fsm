//! Contract identity fixtures derive from SPEC's full material and LF domain.

use fsm_execute::config::{HandlerSpec, HandlerTable};

fn handler(source: &str) -> HandlerSpec {
    let table = HandlerTable::parse(&format!(
        "{{\"format\":\"fsm.handlers/1\",\"handlers\":[{source}]}}"
    ))
    .unwrap();
    table.handlers.into_values().next().unwrap()
}

const BASE: &str = r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":100}"#;

#[test]
fn process_contract_matches_independent_spec_digest_and_explicit_defaults() {
    // Independently computed with Python hashlib over SPEC's canonical full
    // material, not regenerated from the Rust implementation under test.
    let expected = "sha256:e3620952b6ee04bccd76cc079b1a2fd30aca0cf1246a96c2dffefbf2d6061a02";
    assert_eq!(handler(BASE).fingerprint(), expected);
    let explicit = handler(
        r#"{"effect":"notify","kind":"process","argv":["/bin/true"],"timeout_ms":100,
        "retry":{"attempts":1,"backoff_ms":1000,"max_backoff_ms":60000,"on":["timeout","spawn","nonzero_exit"]}}"#,
    );
    assert_eq!(explicit.fingerprint(), expected);
    assert_eq!(handler(BASE).fingerprint(), expected);
}

#[test]
fn every_process_contract_dimension_changes_identity() {
    let expected = handler(BASE).fingerprint();
    for source in [
        r#"{"effect":"other","argv":["/bin/true"],"timeout_ms":100}"#,
        r#"{"effect":"notify","argv":["/bin/false"],"timeout_ms":100}"#,
        r#"{"effect":"notify","argv":["/bin/true","{message}"],"timeout_ms":100}"#,
        r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":101}"#,
        r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"on_ok":{"event":"ok"}}"#,
        r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"on_failed":{"event":"failed"}}"#,
        r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"retry":{"attempts":2}}"#,
        r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"retry":{"backoff_ms":1001}}"#,
        r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"retry":{"max_backoff_ms":60001}}"#,
        r#"{"effect":"notify","argv":["/bin/true"],"timeout_ms":100,"retry":{"on":["timeout"]}}"#,
    ] {
        assert_ne!(handler(source).fingerprint(), expected, "{source}");
    }
}

#[test]
fn mcp_tool_templates_and_advance_payload_and_stamp_order_are_bound() {
    let base = handler(
        r#"{"effect":"notify","kind":"mcp","argv":["/bin/true"],"timeout_ms":100,
        "tool":"notify","arguments":{"message":"{message}"},
        "on_ok":{"event":"ok","payload":{"sent":true},"stamps":["first","second"]}}"#,
    );
    for changed in [
        r#"{"effect":"notify","kind":"mcp","argv":["/bin/true"],"timeout_ms":100,"tool":"other","arguments":{"message":"{message}"},"on_ok":{"event":"ok","payload":{"sent":true},"stamps":["first","second"]}}"#,
        r#"{"effect":"notify","kind":"mcp","argv":["/bin/true"],"timeout_ms":100,"tool":"notify","arguments":{"message":"fixed"},"on_ok":{"event":"ok","payload":{"sent":true},"stamps":["first","second"]}}"#,
        r#"{"effect":"notify","kind":"mcp","argv":["/bin/true"],"timeout_ms":100,"tool":"notify","arguments":{"message":"{message}"},"on_ok":{"event":"ok","payload":{"sent":false},"stamps":["first","second"]}}"#,
        r#"{"effect":"notify","kind":"mcp","argv":["/bin/true"],"timeout_ms":100,"tool":"notify","arguments":{"message":"{message}"},"on_ok":{"event":"ok","payload":{"sent":true},"stamps":["second","first"]}}"#,
    ] {
        assert_ne!(handler(changed).fingerprint(), base.fingerprint());
    }
    assert_ne!(handler(BASE).fingerprint(), base.fingerprint());
}

#[test]
fn declaration_object_order_and_host_concurrency_do_not_change_handler_identity() {
    let reordered = handler(r#"{"timeout_ms":100,"argv":["/bin/true"],"effect":"notify"}"#);
    let table = HandlerTable::parse(&format!(
        "{{\"format\":\"fsm.handlers/1\",\"handlers\":[{BASE}],\"manual_effects\":[\"audit\"],\"max_inflight\":16,\"max_inflight_per_instance\":4}}"
    )).unwrap();
    assert_eq!(reordered.fingerprint(), handler(BASE).fingerprint());
    assert_eq!(
        table.handlers["notify"].fingerprint(),
        handler(BASE).fingerprint()
    );
}

#[test]
fn immutable_contract_recovers_original_templates_advances_and_retry() {
    for source in [
        BASE,
        r#"{"effect":"notify","kind":"mcp","argv":["/bin/true","{message}"],"timeout_ms":100,"tool":"notify","arguments":{"secret":"{message}"},"on_ok":{"event":"ok","payload":{"sent":true},"stamps":["second","first"]},"on_failed":{"event":"failed"},"retry":{"attempts":3,"on":["timeout","mcp_error"]}}"#,
    ] {
        let original = handler(source);
        let material = original.contract_value();
        let recovered =
            fsm_execute::config::HandlerSpec::from_contract(&material, &original.fingerprint())
                .unwrap();
        // SPEC normalizes retry classes as a set; argv and outcome stamp
        // order remain significant and must survive full structural equality.
        let mut normalized = original.clone();
        normalized.retry.on.sort();
        normalized.retry.on.dedup();
        assert_eq!(recovered, normalized);
        assert_eq!(recovered.fingerprint(), original.fingerprint());
        assert_eq!(recovered.contract_value(), material);
        for (field, replacement) in [
            (
                "argv",
                fsm_core::json::Value::Arr(vec![fsm_core::json::Value::Str("/bin/false".into())]),
            ),
            ("timeout_ms", fsm_core::json::Value::Num("101".into())),
            ("on_failed", fsm_core::json::Value::Null),
            (
                "format",
                fsm_core::json::Value::Str("fsm.handler-contract/0".into()),
            ),
        ] {
            let mut changed = material.as_obj().unwrap().clone();
            if changed.get(field) == Some(&replacement) {
                continue;
            }
            changed.insert(field.into(), replacement);
            assert!(
                fsm_execute::config::HandlerSpec::from_contract(
                    &fsm_core::json::Value::Obj(changed),
                    &original.fingerprint()
                )
                .is_err()
            );
        }
        let mut missing = material.as_obj().unwrap().clone();
        missing.remove("retry");
        assert!(
            fsm_execute::config::HandlerSpec::from_contract(
                &fsm_core::json::Value::Obj(missing),
                &original.fingerprint()
            )
            .is_err()
        );
        assert!(
            fsm_execute::config::HandlerSpec::from_contract(&material, "sha256:wrong").is_err()
        );
    }
}

#[test]
fn recovery_refuses_semantically_equivalent_noncanonical_contracts() {
    use fsm_core::json::Value;
    let original = handler(BASE);
    let material = original.contract_value();
    for omitted in [false, true] {
        let mut changed = material.as_obj().unwrap().clone();
        let mut retry = changed["retry"].as_obj().unwrap().clone();
        if omitted {
            retry.remove("backoff_ms");
        } else {
            let mut classes = retry["on"].as_arr().unwrap().to_vec();
            classes.reverse();
            retry.insert("on".into(), Value::Arr(classes));
        }
        changed.insert("retry".into(), Value::Obj(retry));
        assert!(HandlerSpec::from_contract(&Value::Obj(changed), &original.fingerprint()).is_err());
    }
    let mut extra = material.as_obj().unwrap().clone();
    extra.insert("future".into(), Value::Null);
    assert!(HandlerSpec::from_contract(&Value::Obj(extra), &original.fingerprint()).is_err());
}

#[test]
fn contract_recovery_bounds_caller_values_before_serializing() {
    use fsm_core::json::{JsonLimits, Value};
    let original = handler(BASE);
    let mut deep = Value::Null;
    for _ in 0..=JsonLimits::DEFAULT.max_depth {
        deep = Value::Arr(vec![deep]);
    }
    let mut material = original.contract_value().as_obj().unwrap().clone();
    material.insert("arguments".into(), deep);
    assert_eq!(
        HandlerSpec::from_contract(&Value::Obj(material), &original.fingerprint())
            .unwrap_err()
            .code,
        "exec/config"
    );
}
