//! Fourteen-tool MCP registry, schemas, validation, and dispatch.

use std::collections::BTreeMap;

use fsm_core::canon::canon_bytes;
use fsm_core::json::Value;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use super::descriptions;

mod dispatch;
mod handlers;
mod schema_common;
mod schema_in;
mod schema_out;
mod validate;

#[cfg(test)]
mod tests;

pub use dispatch::{ToolCtx, dispatch, dispatch_with};
pub use handlers::machine_summary;
pub use validate::validate_args;

use handlers::{
    run_deadline_poll, run_effect_ack, run_instance_cancel, run_instance_create,
    run_instance_elicit, run_instance_get, run_instance_history, run_instance_list,
    run_instance_migrate, run_instance_send, run_invocation_return, run_invocation_start,
    run_machine_analyze, run_machine_create, run_machine_diagram, run_machine_get,
    run_machine_list, run_signal_deliver, run_simulate,
};
use schema_in::{
    schema_deadline_poll_in, schema_diagram_in, schema_effect_ack_in, schema_instance_cancel_in,
    schema_instance_create_in, schema_instance_elicit_in, schema_instance_history_in,
    schema_instance_id_in, schema_instance_list_in, schema_instance_migrate_in,
    schema_instance_send_in, schema_invocation_slot_in, schema_machine_create_in,
    schema_machine_list_in, schema_machine_ref_in, schema_signal_deliver_in, schema_simulate_in,
};
use schema_out::{
    schema_deadline_poll_out, schema_effect_ack_out, schema_instance_cancel_out,
    schema_instance_create_out, schema_instance_elicit_out, schema_instance_get_out,
    schema_instance_history_out, schema_instance_list_out, schema_instance_migrate_out,
    schema_instance_send_out, schema_invocation_return_out, schema_invocation_start_out,
    schema_machine_analyze_out, schema_machine_create_out, schema_machine_diagram_out,
    schema_machine_get_out, schema_machine_list_out, schema_signal_deliver_out,
    schema_simulate_out,
};

/// Every tool that reaches a store mutator, and therefore every tool a
/// read-only server must refuse.
///
/// Counted from the store code — `store/lifecycle.rs` and
/// `store/instance/*.rs` — rather than from memory. `machine_create` is the
/// easy one to forget, and it is the *authoring* path, so forgetting it means
/// the model gets an unexplained failure at the moment it is being most
/// useful. `dispatch` consults this table instead of six match arms, and the
/// documentation test imports the same constant, so the gate and the docs
/// cannot drift apart.
pub const MUTATING_TOOLS: &[&str] = &[
    "machine_create",
    "instance_elicit",
    "instance_create",
    "instance_send",
    "deadline_poll",
    "effect_ack",
    "instance_cancel",
    "instance_migrate",
    "invocation_start",
    "invocation_return",
    "signal_deliver",
];

/// The tools whose result names exactly one instance, and therefore carries
/// a link to it.
///
/// One list rather than six match arms, beside `MUTATING_TOOLS` for the same
/// reason: membership is a fact about the tool, and a fact kept in one place
/// cannot disagree with itself. `instance_list` is deliberately absent — a
/// list result would carry N links and bury the text — and so is every
/// machine and simulate tool, which name no instance.
pub const LINKED_TOOLS: &[&str] = &[
    "instance_create",
    "instance_send",
    "deadline_poll",
    "effect_ack",
    "instance_cancel",
    "instance_get",
];

/// The tools that take long enough to report progress.
///
/// Two, deliberately: a report on a call that returns in a microsecond is
/// noise, and this plan does not add reports it cannot justify. Both know
/// their size up front, so both send a `total`.
pub const PROGRESS_TOOLS: &[&str] = &["simulate", "instance_history"];

pub struct ToolSpec {
    pub name: &'static str,
    /// The display name a host shows. Derived from nothing — a title is a
    /// human fact, unlike every hint below it.
    pub title: &'static str,
    pub description: &'static str,
    pub input_schema: fn() -> Value,
    pub output_schema: fn() -> Value,
    pub run: fn(&mut Store, &mut dyn Clock, &Value) -> Result<Value, ErrorObj>,
}

pub fn registry() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "machine_create",
            title: descriptions::MACHINE_CREATE_TITLE,
            description: descriptions::MACHINE_CREATE,
            input_schema: schema_machine_create_in,
            output_schema: schema_machine_create_out,
            run: run_machine_create,
        },
        ToolSpec {
            name: "machine_list",
            title: descriptions::MACHINE_LIST_TITLE,
            description: descriptions::MACHINE_LIST,
            input_schema: schema_machine_list_in,
            output_schema: schema_machine_list_out,
            run: run_machine_list,
        },
        ToolSpec {
            name: "machine_get",
            title: descriptions::MACHINE_GET_TITLE,
            description: descriptions::MACHINE_GET,
            input_schema: schema_machine_ref_in,
            output_schema: schema_machine_get_out,
            run: run_machine_get,
        },
        ToolSpec {
            name: "machine_analyze",
            title: descriptions::MACHINE_ANALYZE_TITLE,
            description: descriptions::MACHINE_ANALYZE,
            input_schema: schema_machine_ref_in,
            output_schema: schema_machine_analyze_out,
            run: run_machine_analyze,
        },
        ToolSpec {
            name: "machine_diagram",
            title: descriptions::MACHINE_DIAGRAM_TITLE,
            description: descriptions::MACHINE_DIAGRAM,
            input_schema: schema_diagram_in,
            output_schema: schema_machine_diagram_out,
            run: run_machine_diagram,
        },
        ToolSpec {
            name: "instance_create",
            title: descriptions::INSTANCE_CREATE_TITLE,
            description: descriptions::INSTANCE_CREATE,
            input_schema: schema_instance_create_in,
            output_schema: schema_instance_create_out,
            run: run_instance_create,
        },
        ToolSpec {
            name: "instance_send",
            title: descriptions::INSTANCE_SEND_TITLE,
            description: descriptions::INSTANCE_SEND,
            input_schema: schema_instance_send_in,
            output_schema: schema_instance_send_out,
            run: run_instance_send,
        },
        ToolSpec {
            name: "deadline_poll",
            title: descriptions::DEADLINE_POLL_TITLE,
            description: descriptions::DEADLINE_POLL,
            input_schema: schema_deadline_poll_in,
            output_schema: schema_deadline_poll_out,
            run: run_deadline_poll,
        },
        ToolSpec {
            name: "effect_ack",
            title: descriptions::EFFECT_ACK_TITLE,
            description: descriptions::EFFECT_ACK,
            input_schema: schema_effect_ack_in,
            output_schema: schema_effect_ack_out,
            run: run_effect_ack,
        },
        ToolSpec {
            name: "instance_cancel",
            title: descriptions::INSTANCE_CANCEL_TITLE,
            description: descriptions::INSTANCE_CANCEL,
            input_schema: schema_instance_cancel_in,
            output_schema: schema_instance_cancel_out,
            run: run_instance_cancel,
        },
        ToolSpec {
            name: "instance_migrate",
            title: descriptions::INSTANCE_MIGRATE_TITLE,
            description: descriptions::INSTANCE_MIGRATE,
            input_schema: schema_instance_migrate_in,
            output_schema: schema_instance_migrate_out,
            run: run_instance_migrate,
        },
        ToolSpec {
            name: "invocation_start",
            title: descriptions::INVOCATION_START_TITLE,
            description: descriptions::INVOCATION_START,
            input_schema: schema_invocation_slot_in,
            output_schema: schema_invocation_start_out,
            run: run_invocation_start,
        },
        ToolSpec {
            name: "invocation_return",
            title: descriptions::INVOCATION_RETURN_TITLE,
            description: descriptions::INVOCATION_RETURN,
            input_schema: schema_invocation_slot_in,
            output_schema: schema_invocation_return_out,
            run: run_invocation_return,
        },
        ToolSpec {
            name: "signal_deliver",
            title: descriptions::SIGNAL_DELIVER_TITLE,
            description: descriptions::SIGNAL_DELIVER,
            input_schema: schema_signal_deliver_in,
            output_schema: schema_signal_deliver_out,
            run: run_signal_deliver,
        },
        ToolSpec {
            name: "instance_get",
            title: descriptions::INSTANCE_GET_TITLE,
            description: descriptions::INSTANCE_GET,
            input_schema: schema_instance_id_in,
            output_schema: schema_instance_get_out,
            run: run_instance_get,
        },
        ToolSpec {
            name: "instance_list",
            title: descriptions::INSTANCE_LIST_TITLE,
            description: descriptions::INSTANCE_LIST,
            input_schema: schema_instance_list_in,
            output_schema: schema_instance_list_out,
            run: run_instance_list,
        },
        ToolSpec {
            name: "instance_history",
            title: descriptions::INSTANCE_HISTORY_TITLE,
            description: descriptions::INSTANCE_HISTORY,
            input_schema: schema_instance_history_in,
            output_schema: schema_instance_history_out,
            run: run_instance_history,
        },
        ToolSpec {
            name: "instance_elicit",
            title: descriptions::INSTANCE_ELICIT_TITLE,
            description: descriptions::INSTANCE_ELICIT,
            input_schema: schema_instance_elicit_in,
            output_schema: schema_instance_elicit_out,
            run: run_instance_elicit,
        },
        ToolSpec {
            name: "simulate",
            title: descriptions::SIMULATE_TITLE,
            description: descriptions::SIMULATE,
            input_schema: schema_simulate_in,
            output_schema: schema_simulate_out,
            run: run_simulate,
        },
    ]
}

pub fn names() -> Vec<&'static str> {
    registry().into_iter().map(|t| t.name).collect()
}

/// The one tool this server has that destroys something a caller cares
/// about: cancelling an instance ends it, and no later event revives it.
/// Everything else either reads, or advances a workflow the caller asked to
/// advance.
const DESTRUCTIVE_TOOLS: &[&str] = &["instance_cancel"];

/// The four hints the protocol defines, derived rather than declared.
///
/// A second table would eventually disagree with the first, and a
/// `readOnlyHint` that contradicts `MUTATING_TOOLS` is worse than no hint at
/// all: a host would auto-approve a writer on the strength of it. So every
/// hint below is an expression over a constant this code already keeps
/// honest for its own reasons.
///
/// `idempotentHint` is `true` for exactly the mutating tools, and the claim
/// is unusually strong here: each one takes a `request_id`, the store
/// refuses a reused key carrying different content rather than replaying it,
/// and repeating the same call returns the first outcome with
/// `duplicate: true`. `machine_create` qualifies through content addressing
/// — an identical spec is the same machine, and `if_exists: "return_existing"`
/// is its idempotent form.
///
/// `openWorldHint` is `false` everywhere: this server reads and writes one
/// data directory. Effects do reach the world, but the executor runs them —
/// no call in this surface does.
pub fn annotations(name: &str) -> Value {
    let mutating = MUTATING_TOOLS.contains(&name);
    Value::Obj(BTreeMap::from([
        ("readOnlyHint".into(), Value::Bool(!mutating)),
        // Emitted for every tool, including the read-only ones where it means
        // nothing, because a hint that appears only sometimes reads as a
        // claim about the tools it appears on.
        (
            "destructiveHint".into(),
            Value::Bool(DESTRUCTIVE_TOOLS.contains(&name)),
        ),
        ("idempotentHint".into(), Value::Bool(mutating)),
        ("openWorldHint".into(), Value::Bool(false)),
    ]))
}

pub fn tools_list_result() -> Value {
    let tools: Vec<Value> = registry()
        .into_iter()
        .map(|t| {
            let mut tool = BTreeMap::new();
            tool.insert("name".into(), Value::Str(t.name.into()));
            tool.insert("title".into(), Value::Str(t.title.into()));
            tool.insert("description".into(), Value::Str(t.description.into()));
            tool.insert("annotations".into(), annotations(t.name));
            tool.insert("inputSchema".into(), (t.input_schema)());
            tool.insert("outputSchema".into(), (t.output_schema)());
            Value::Obj(tool)
        })
        .collect();
    Value::Obj(BTreeMap::from([("tools".into(), Value::Arr(tools))]))
}

#[allow(dead_code)]
pub fn canon_tool_list_len() -> usize {
    canon_bytes(&tools_list_result()).len()
}
