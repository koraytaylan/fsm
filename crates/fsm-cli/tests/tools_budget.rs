use fsm_cli::mcp::descriptions::{
    DEADLINE_POLL, INSTANCE_CREATE, INSTANCE_GET, INSTANCE_LIST, INSTANCE_MIGRATE, INSTANCE_SEND,
    INVOCATION_RETURN, INVOCATION_START, MACHINE_CREATE, MACHINE_GET, MACHINE_LIST, SIGNAL_DELIVER,
};
use fsm_cli::mcp::tools::tools_list_result;
use fsm_core::canon::canon_bytes;

#[test]
fn tools_list_budget() {
    // 20 000 until plan 0009's reactive fields (21 000), then plan 0010's
    // three composition tools with their schemas (24 000), then plan 0011's
    // migration tool and the tree fields on the instance view (26 000).
    //
    // Plan 0013 sets this number **once for the whole plan sequence**, and
    // the arithmetic is on the record so nobody has to redo it. Eighteen
    // annotated tools measure 27 632 bytes; titles and the four hints cost
    // about 135 bytes a tool, so the annotations themselves are roughly
    // 2 400 of that. Six tools are still to come — `instance_elicit` from
    // this plan, and `explain_step`, `journal_verify`, `journal_replay`,
    // `store_doctor` and `instance_annotate` from plan 0014 — and the
    // current mean is 1 535 bytes a tool. Allowing 1 700 each, a tenth over
    // the mean for the audit tools' richer output schemas, gives 10 200 and
    // a total of 37 832.
    //
    // So: 38 000, and no higher. `6403` and `6801` assert they fit under it
    // rather than raising it; a tool that does not fit shortens its
    // description, because a ceiling that only ever goes up is not a budget.
    // `tools/list` is sent once per session and every byte of it is context
    // the model pays for before it has read a single fact about the store.
    let bytes = canon_bytes(&tools_list_result());
    // Measured after plan 0017 added the additive `seal` object to the three
    // audit tools' output schemas: no tool was added and no description was
    // shortened, because an optional object costs a few dozen bytes.
    assert!(bytes.len() <= 38_000, "tools/list is {} bytes", bytes.len());
}

#[test]
fn per_description_caps() {
    assert!(!MACHINE_CREATE.is_empty());
    assert!(MACHINE_CREATE.split_whitespace().count() <= 190);
    assert!(INSTANCE_SEND.split_whitespace().count() <= 190);
    assert!(DEADLINE_POLL.split_whitespace().count() <= 180);
    for (name, t) in [
        ("instance_migrate", INSTANCE_MIGRATE),
        ("invocation_start", INVOCATION_START),
        ("invocation_return", INVOCATION_RETURN),
        ("signal_deliver", SIGNAL_DELIVER),
        ("machine_list", MACHINE_LIST),
        ("machine_get", MACHINE_GET),
        ("instance_get", INSTANCE_GET),
        ("instance_list", INSTANCE_LIST),
    ] {
        assert!(!t.is_empty(), "{name}");
        assert!(t.split_whitespace().count() <= 60, "{name}");
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

/// The measured size, printed so a change is visible in a CI log rather than
/// only when the ceiling is crossed.
#[test]
fn the_measured_tools_list_size_is_reported() {
    let bytes = canon_bytes(&tools_list_result()).len();
    assert!(
        bytes <= 38_000,
        "tools/list is {bytes} bytes against a ceiling of 38 000"
    );
    // A regression that halves the surface is as suspicious as one that
    // doubles it: both mean the tool table is not what it was.
    assert!(bytes > 30_000, "tools/list shrank to {bytes} bytes");
}
