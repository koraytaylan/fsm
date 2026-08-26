use fsm_cli::mcp::descriptions::{
    DEADLINE_POLL, INSTANCE_CREATE, INSTANCE_GET, INSTANCE_LIST, INSTANCE_SEND, MACHINE_CREATE,
    MACHINE_GET, MACHINE_LIST,
};
use fsm_cli::mcp::tools::tools_list_result;
use fsm_core::canon::canon_bytes;

#[test]
fn tools_list_budget() {
    // 20 000 until plan 0009 added the reactive surface to `machine_analyze`,
    // `simulate`, and the instance view — four additive schema fields, each
    // kept to one line. The per-description word caps below are what bound
    // the reading cost; this ceiling only stops the listing growing unnoticed.
    let bytes = canon_bytes(&tools_list_result());
    assert!(bytes.len() <= 21_000, "tools/list is {} bytes", bytes.len());
}

#[test]
fn per_description_caps() {
    assert!(!MACHINE_CREATE.is_empty());
    assert!(MACHINE_CREATE.split_whitespace().count() <= 190);
    assert!(INSTANCE_SEND.split_whitespace().count() <= 190);
    assert!(DEADLINE_POLL.split_whitespace().count() <= 180);
    for (name, t) in [
        ("machine_list", MACHINE_LIST),
        ("machine_get", MACHINE_GET),
        ("instance_get", INSTANCE_GET),
        ("instance_list", INSTANCE_LIST),
    ] {
        assert!(!t.is_empty(), "{name}");
        assert!(t.split_whitespace().count() <= 40, "{name}");
    }
}

#[test]
fn flow_and_invariants() {
    assert!(MACHINE_CREATE.contains("instance_create"));
    assert!(INSTANCE_CREATE.contains("instance_send"));
    assert!(INSTANCE_SEND.contains("effect_ack"));
    assert!(INSTANCE_SEND.contains("enabled_events"));
    assert!(INSTANCE_SEND.contains("request_id"));
    assert!(INSTANCE_SEND.contains("deadline_poll"));
    assert!(DEADLINE_POLL.contains("request_id"));
    assert!(MACHINE_CREATE.contains("JSON strings"));
    assert!(MACHINE_CREATE.contains("dry_run"));
}

#[test]
fn guideline_header() {
    let src = include_str!("../src/mcp/descriptions.rs");
    assert!(src.contains("Writing guidelines"));
    assert!(src.contains("when-to-use") || src.contains("Open with when-to-use"));
}
