use std::collections::BTreeMap;

use fsm_core::json::Value;

use super::schema_common::{
    enum_str, event_obj, schema_obj, ty, ty_array_of, ty_num, ty_str_array,
};

pub(super) fn schema_machine_create_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("spec".into(), ty("object"));
    p.insert("dry_run".into(), ty("boolean"));
    p.insert("if_exists".into(), enum_str(&["return_existing", "error"]));
    schema_obj(p, &["spec"], false)
}

pub(super) fn schema_machine_list_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("name_contains".into(), ty("string"));
    p.insert("limit".into(), ty_num(1, 200));
    p.insert("cursor".into(), ty("string"));
    schema_obj(p, &[], false)
}

pub(super) fn schema_machine_ref_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    schema_obj(p, &["machine"], false)
}

pub(super) fn schema_diagram_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("format".into(), enum_str(&["mermaid", "dot"]));
    p.insert("instance".into(), ty("string"));
    schema_obj(p, &["machine", "format"], false)
}

pub(super) fn schema_instance_create_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("context".into(), ty("object"));
    p.insert("request_id".into(), ty("string"));
    p.insert("tags".into(), ty_str_array(32));
    schema_obj(p, &["machine", "request_id"], false)
}

pub(super) fn schema_instance_send_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("event".into(), event_obj());
    p.insert("request_id".into(), ty("string"));
    p.insert("stamp".into(), ty_str_array(32));
    p.insert("expect_seq".into(), ty_num(0, i64::MAX));
    schema_obj(p, &["instance_id", "event", "request_id"], false)
}

/// An ask: which instance, which declared event, and the key its answer
/// will be sent under.
pub(super) fn schema_instance_elicit_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("event".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    p.insert("message".into(), ty("string"));
    schema_obj(p, &["instance_id", "event", "request_id"], false)
}

/// One journaled step: the instance whose story it is, and its seq.
pub(super) fn schema_explain_step_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("seq".into(), ty_num(0, i64::MAX));
    schema_obj(p, &["instance_id", "seq"], false)
}

/// An optional window. Both absent means the whole journal.
pub(super) fn schema_journal_verify_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("from_seq".into(), ty_num(0, i64::MAX));
    p.insert("to_seq".into(), ty_num(0, i64::MAX));
    schema_obj(p, &[], false)
}

pub(super) fn schema_deadline_poll_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    p.insert("expect_seq".into(), ty_num(0, i64::MAX));
    schema_obj(p, &["instance_id", "request_id"], false)
}

/// An invocation slot: the parent and the slot name, which are the whole of
/// the request — the child id, the machine, and the overrides all derive
/// from them and the parent's state.
/// A migration: the instance, the target, and whether to only ask.
///
/// `request_id` is required for the writing form and must be **absent** for
/// a dry run: a preview claims no idempotency key because it changes
/// nothing, and requiring one would teach a caller to burn keys on
/// questions.
pub(super) fn schema_instance_migrate_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("to_machine".into(), ty("string"));
    p.insert("dry_run".into(), ty("boolean"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(p, &["instance_id", "to_machine"], false)
}

pub(super) fn schema_invocation_slot_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("slot".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(p, &["instance_id", "slot", "request_id"], false)
}

pub(super) fn schema_signal_deliver_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("signal_id".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(p, &["instance_id", "signal_id", "request_id"], false)
}

pub(super) fn schema_effect_ack_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("effect_id".into(), ty("string"));
    p.insert("outcome".into(), enum_str(&["ok", "failed"]));
    p.insert("result".into(), ty("object"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &["instance_id", "effect_id", "outcome", "request_id"],
        false,
    )
}

pub(super) fn schema_instance_cancel_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("reason".into(), ty("string"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(p, &["instance_id", "reason", "request_id"], false)
}

pub(super) fn schema_instance_id_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    schema_obj(p, &["instance_id"], false)
}

pub(super) fn schema_instance_list_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("state".into(), ty("string"));
    p.insert(
        "status".into(),
        enum_str(&["running", "completed", "cancelled", "all"]),
    );
    p.insert("tag".into(), ty("string"));
    // The tree filters: one instance's children, or every root. Both compose
    // with the cursor rather than replacing it.
    p.insert("parent".into(), ty("string"));
    p.insert("roots_only".into(), ty("boolean"));
    p.insert("limit".into(), ty_num(1, 200));
    p.insert("cursor".into(), ty("string"));
    schema_obj(p, &[], false)
}

pub(super) fn schema_instance_history_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("from_seq".into(), ty_num(0, i64::MAX));
    p.insert("limit".into(), ty_num(1, 500));
    p.insert("include_trace".into(), ty("boolean"));
    p.insert("include_rejected".into(), ty("boolean"));
    schema_obj(p, &["instance_id"], false)
}

pub(super) fn schema_simulate_in() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine".into(), ty("string"));
    p.insert("spec".into(), ty("object"));
    p.insert("context".into(), ty("object"));
    p.insert("events".into(), ty_array_of(event_obj()));
    p.insert("on_reject".into(), enum_str(&["stop", "continue"]));
    schema_obj(p, &["events"], false)
}
