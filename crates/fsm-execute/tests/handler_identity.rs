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
