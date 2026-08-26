use std::collections::BTreeMap;

use fsm_core::json::Value;

use super::schema_common::{
    completeness_obj, instance_core_props, instance_row, machine_row, reachability_obj, schema_obj,
    simulate_final_obj, simulate_initial_obj, simulate_step_obj, summary_obj, transition_obj, ty,
    ty_array_of, ty_nullable,
};

pub(super) fn schema_machine_create_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("name".into(), ty("string"));
    p.insert("created".into(), ty("boolean"));
    p.insert("dry_run".into(), ty("boolean"));
    p.insert("warnings".into(), ty("array"));
    p.insert("summary".into(), summary_obj());
    schema_obj(
        p,
        &[
            "machine_id",
            "name",
            "created",
            "dry_run",
            "warnings",
            "summary",
        ],
        true,
    )
}

pub(super) fn schema_machine_list_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machines".into(), ty_array_of(machine_row()));
    p.insert("next_cursor".into(), ty("string"));
    schema_obj(p, &["machines"], true)
}

pub(super) fn schema_machine_get_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("name".into(), ty("string"));
    p.insert("spec".into(), ty("object"));
    p.insert("summary".into(), summary_obj());
    schema_obj(p, &["machine_id", "name", "spec", "summary"], true)
}

pub(super) fn schema_machine_analyze_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("machine_id".into(), ty("string"));
    p.insert("findings".into(), ty("array"));
    p.insert("completeness".into(), completeness_obj());
    p.insert("reachability".into(), reachability_obj());
    p.insert("shadowing".into(), ty("array"));
    // Additive and optional: an existing caller's parse is unaffected.
    p.insert("eventless_transitions".into(), ty("integer"));
    p.insert("done_events".into(), ty("array"));
    p.insert("unhandled_done_events".into(), ty("array"));
    p.insert("internal_events".into(), ty("array"));
    schema_obj(
        p,
        &[
            "machine_id",
            "findings",
            "completeness",
            "reachability",
            "shadowing",
        ],
        true,
    )
}

pub(super) fn schema_machine_diagram_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("format".into(), ty("string"));
    p.insert("diagram".into(), ty("string"));
    schema_obj(p, &["format", "diagram"], true)
}

pub(super) fn schema_instance_create_out() -> Value {
    let mut p = instance_core_props();
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "instance_id",
            "machine",
            "status",
            "configuration",
            "seq",
            "context",
            "effects_pending",
            "deadlines_pending",
            "enabled_events",
            "state_hash",
            "state_format",
        ],
        true,
    )
}

pub(super) fn schema_instance_send_out() -> Value {
    let mut p = instance_core_props();
    p.insert("applied".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("ignored".into(), ty("boolean"));
    p.insert("request_id".into(), ty("string"));
    p.insert("transition".into(), transition_obj());
    p.insert("monitor_flags".into(), ty("array"));
    p.insert("trace".into(), ty("object"));
    schema_obj(
        p,
        &[
            "applied",
            "duplicate",
            "seq",
            "configuration",
            "status",
            "context",
            "effects_pending",
            "deadlines_pending",
            "enabled_events",
            "state_hash",
            "state_format",
            "transition",
            "trace",
            "monitor_flags",
        ],
        true,
    )
}

pub(super) fn schema_deadline_poll_out() -> Value {
    let mut p = instance_core_props();
    p.insert("deadline_applied".into(), ty("boolean"));
    p.insert("deadline_not_due".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("request_id".into(), ty("string"));
    p.insert("deadline".into(), ty("string"));
    p.insert("deadline_idx".into(), ty("number"));
    p.insert("due_ms".into(), ty("string"));
    p.insert("next_deadline".into(), ty("string"));
    p.insert("next_deadline_idx".into(), ty("number"));
    p.insert("next_due_ms".into(), ty("string"));
    p.insert("transition".into(), transition_obj());
    p.insert("monitor_flags".into(), ty("array"));
    p.insert("trace".into(), ty("object"));
    schema_obj(
        p,
        &[
            "deadline_applied",
            "deadline_not_due",
            "instance_id",
            "machine",
            "status",
            "configuration",
            "seq",
            "context",
            "effects_pending",
            "deadlines_pending",
            "enabled_events",
            "state_hash",
            "state_format",
        ],
        true,
    )
}

pub(super) fn schema_instance_get_out() -> Value {
    let mut p = instance_core_props();
    p.insert("history".into(), ty("object"));
    // Additive: the tree an existing caller never asked for and whose parse
    // is unaffected by its arrival.
    p.insert("parent".into(), ty_nullable("object"));
    p.insert("children".into(), ty("array"));
    p.insert("created_seq".into(), ty("number"));
    schema_obj(
        p,
        &[
            "instance_id",
            "machine",
            "status",
            "configuration",
            "seq",
            "context",
            "history",
            "effects_pending",
            "deadlines_pending",
            "enabled_events",
            "state_hash",
            "state_format",
        ],
        true,
    )
}

pub(super) fn schema_instance_cancel_out() -> Value {
    let mut p = instance_core_props();
    p.insert("request_id".into(), ty("string"));
    p.insert("duplicate".into(), ty("boolean"));
    schema_obj(
        p,
        &[
            "instance_id",
            "status",
            "seq",
            "configuration",
            "context",
            "deadlines_pending",
            "state_hash",
            "state_format",
        ],
        true,
    )
}

pub(super) fn schema_invocation_start_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("parent_instance_id".into(), ty("string"));
    p.insert("slot".into(), ty("string"));
    p.insert("child_instance_id".into(), ty("string"));
    p.insert("child_machine_id".into(), ty("string"));
    p.insert("status".into(), ty("string"));
    p.insert("invoked".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("seq".into(), ty("number"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "parent_instance_id",
            "slot",
            "child_instance_id",
            "child_machine_id",
            "request_id",
        ],
        true,
    )
}

pub(super) fn schema_invocation_return_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("parent_instance_id".into(), ty("string"));
    p.insert("slot".into(), ty("string"));
    p.insert("child_instance_id".into(), ty("string"));
    p.insert("outcome".into(), ty("string"));
    p.insert("status".into(), ty("string"));
    p.insert("returned".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("seq".into(), ty("number"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "parent_instance_id",
            "slot",
            "child_instance_id",
            "outcome",
            "request_id",
        ],
        true,
    )
}

pub(super) fn schema_signal_deliver_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("sender_instance_id".into(), ty("string"));
    p.insert("signal_id".into(), ty("string"));
    p.insert("target_instance_id".into(), ty("string"));
    p.insert("event".into(), ty("string"));
    p.insert("outcome".into(), ty("string"));
    p.insert("delivered".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("seq".into(), ty("number"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "sender_instance_id",
            "signal_id",
            "target_instance_id",
            "event",
            "outcome",
            "request_id",
        ],
        true,
    )
}

pub(super) fn schema_effect_ack_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("effect_id".into(), ty("string"));
    p.insert("acked".into(), ty("boolean"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("seq".into(), ty("number"));
    p.insert("effects_pending".into(), ty("array"));
    p.insert("request_id".into(), ty("string"));
    schema_obj(
        p,
        &[
            "instance_id",
            "effect_id",
            "acked",
            "duplicate",
            "seq",
            "effects_pending",
        ],
        true,
    )
}

pub(super) fn schema_instance_list_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instances".into(), ty_array_of(instance_row()));
    p.insert("next_cursor".into(), ty("string"));
    schema_obj(p, &["instances"], true)
}

pub(super) fn schema_instance_history_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("instance_id".into(), ty("string"));
    p.insert("entries".into(), ty_array_of(ty("object")));
    p.insert("chain_verified".into(), ty("boolean"));
    p.insert("next_from_seq".into(), ty("number"));
    schema_obj(p, &["instance_id", "entries", "chain_verified"], true)
}

pub(super) fn schema_simulate_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("steps".into(), ty_array_of(simulate_step_obj()));
    p.insert("final".into(), simulate_final_obj());
    p.insert("initial".into(), simulate_initial_obj());
    p.insert("stopped_at".into(), ty("number"));
    schema_obj(p, &["steps", "final", "initial"], true)
}
