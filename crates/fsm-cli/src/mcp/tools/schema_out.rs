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

/// The two shapes one ask can produce: the sent event's view, or the action
/// the person took instead. `action` is always there, so a caller reacts
/// rather than guesses.
pub(super) fn schema_instance_elicit_out() -> Value {
    let mut p = instance_core_props();
    p.insert("action".into(), ty("string"));
    p.insert("applied".into(), ty("boolean"));
    p.insert("event".into(), ty("string"));
    p.insert("instance_id".into(), ty("string"));
    p.insert("duplicate".into(), ty("boolean"));
    p.insert("request_id".into(), ty("string"));
    p.insert("transition".into(), transition_obj());
    p.insert("monitor_flags".into(), ty("array"));
    p.insert("trace".into(), ty("object"));
    schema_obj(p, &["action", "applied", "event", "instance_id"], true)
}

/// The trace `explain_seq` reconstructs, passed through as it stands.
///
/// The properties named here are the ones a caller can rely on; the rest of
/// a history entry's fields come through as they are, because reshaping
/// them is how this tool and `fsm explain --json` would diverge.
pub(super) fn schema_explain_step_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("seq".into(), ty("number"));
    p.insert("kind".into(), ty("string"));
    p.insert("trace".into(), ty("object"));
    schema_obj(p, &["seq", "kind"], true)
}

/// A verdict in the recovery table's vocabulary, with the count actually
/// walked and — where the table prescribes one — the remedy to run.
pub(super) fn schema_journal_verify_out() -> Value {
    let mut p = BTreeMap::new();
    p.insert("health".into(), ty("string"));
    p.insert("verified_records".into(), ty("number"));
    p.insert("message".into(), ty("string"));
    p.insert("first_bad_seq".into(), ty("number"));
    p.insert("blast_radius".into(), ty("string"));
    p.insert("remedy".into(), ty("string"));
    p.insert("segments".into(), ty("array"));
    schema_obj(p, &["health", "verified_records", "message"], true)
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
    p.insert("machine_history".into(), ty("array"));
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

/// Either the preview or the post-migration instance view: a dry run and a
/// migration answer the same question at two different moments.
pub(super) fn schema_instance_migrate_out() -> Value {
    let mut p = instance_core_props();
    p.insert("dry_run".into(), ty("boolean"));
    p.insert("instance_id".into(), ty("string"));
    p.insert("from_machine_id".into(), ty("string"));
    p.insert("to_machine_id".into(), ty("string"));
    p.insert("migrated".into(), ty("boolean"));
    p.insert("would_migrate".into(), ty("boolean"));
    p.insert("configuration_mapped".into(), ty("object"));
    p.insert("configuration_after".into(), ty("object"));
    p.insert("context_changes".into(), ty("array"));
    p.insert("dropped_history".into(), ty("array"));
    p.insert("rescheduled_deadlines".into(), ty("array"));
    p.insert("dropped_slots".into(), ty("array"));
    p.insert("retained_effects".into(), ty("array"));
    p.insert("refusal".into(), ty("object"));
    schema_obj(
        p,
        &["instance_id", "from_machine_id", "to_machine_id", "dry_run"],
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
