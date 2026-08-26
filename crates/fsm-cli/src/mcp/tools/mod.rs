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

pub use dispatch::dispatch;
pub use handlers::machine_summary;
pub use validate::validate_args;

use handlers::{
    run_deadline_poll, run_effect_ack, run_instance_cancel, run_instance_create, run_instance_get,
    run_instance_history, run_instance_list, run_instance_migrate, run_instance_send,
    run_invocation_return, run_invocation_start, run_machine_analyze, run_machine_create,
    run_machine_diagram, run_machine_get, run_machine_list, run_signal_deliver, run_simulate,
};
use schema_in::{
    schema_deadline_poll_in, schema_diagram_in, schema_effect_ack_in, schema_instance_cancel_in,
    schema_instance_create_in, schema_instance_history_in, schema_instance_id_in,
    schema_instance_list_in, schema_instance_migrate_in, schema_instance_send_in,
    schema_invocation_slot_in, schema_machine_create_in, schema_machine_list_in,
    schema_machine_ref_in, schema_signal_deliver_in, schema_simulate_in,
};
use schema_out::{
    schema_deadline_poll_out, schema_effect_ack_out, schema_instance_cancel_out,
    schema_instance_create_out, schema_instance_get_out, schema_instance_history_out,
    schema_instance_list_out, schema_instance_migrate_out, schema_instance_send_out,
    schema_invocation_return_out, schema_invocation_start_out, schema_machine_analyze_out,
    schema_machine_create_out, schema_machine_diagram_out, schema_machine_get_out,
    schema_machine_list_out, schema_signal_deliver_out, schema_simulate_out,
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

pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: fn() -> Value,
    pub output_schema: fn() -> Value,
    pub run: fn(&mut Store, &mut dyn Clock, &Value) -> Result<Value, ErrorObj>,
}

pub fn registry() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "machine_create",
            description: descriptions::MACHINE_CREATE,
            input_schema: schema_machine_create_in,
            output_schema: schema_machine_create_out,
            run: run_machine_create,
        },
        ToolSpec {
            name: "machine_list",
            description: descriptions::MACHINE_LIST,
            input_schema: schema_machine_list_in,
            output_schema: schema_machine_list_out,
            run: run_machine_list,
        },
        ToolSpec {
            name: "machine_get",
            description: descriptions::MACHINE_GET,
            input_schema: schema_machine_ref_in,
            output_schema: schema_machine_get_out,
            run: run_machine_get,
        },
        ToolSpec {
            name: "machine_analyze",
            description: descriptions::MACHINE_ANALYZE,
            input_schema: schema_machine_ref_in,
            output_schema: schema_machine_analyze_out,
            run: run_machine_analyze,
        },
        ToolSpec {
            name: "machine_diagram",
            description: descriptions::MACHINE_DIAGRAM,
            input_schema: schema_diagram_in,
            output_schema: schema_machine_diagram_out,
            run: run_machine_diagram,
        },
        ToolSpec {
            name: "instance_create",
            description: descriptions::INSTANCE_CREATE,
            input_schema: schema_instance_create_in,
            output_schema: schema_instance_create_out,
            run: run_instance_create,
        },
        ToolSpec {
            name: "instance_send",
            description: descriptions::INSTANCE_SEND,
            input_schema: schema_instance_send_in,
            output_schema: schema_instance_send_out,
            run: run_instance_send,
        },
        ToolSpec {
            name: "deadline_poll",
            description: descriptions::DEADLINE_POLL,
            input_schema: schema_deadline_poll_in,
            output_schema: schema_deadline_poll_out,
            run: run_deadline_poll,
        },
        ToolSpec {
            name: "effect_ack",
            description: descriptions::EFFECT_ACK,
            input_schema: schema_effect_ack_in,
            output_schema: schema_effect_ack_out,
            run: run_effect_ack,
        },
        ToolSpec {
            name: "instance_cancel",
            description: descriptions::INSTANCE_CANCEL,
            input_schema: schema_instance_cancel_in,
            output_schema: schema_instance_cancel_out,
            run: run_instance_cancel,
        },
        ToolSpec {
            name: "instance_migrate",
            description: descriptions::INSTANCE_MIGRATE,
            input_schema: schema_instance_migrate_in,
            output_schema: schema_instance_migrate_out,
            run: run_instance_migrate,
        },
        ToolSpec {
            name: "invocation_start",
            description: descriptions::INVOCATION_START,
            input_schema: schema_invocation_slot_in,
            output_schema: schema_invocation_start_out,
            run: run_invocation_start,
        },
        ToolSpec {
            name: "invocation_return",
            description: descriptions::INVOCATION_RETURN,
            input_schema: schema_invocation_slot_in,
            output_schema: schema_invocation_return_out,
            run: run_invocation_return,
        },
        ToolSpec {
            name: "signal_deliver",
            description: descriptions::SIGNAL_DELIVER,
            input_schema: schema_signal_deliver_in,
            output_schema: schema_signal_deliver_out,
            run: run_signal_deliver,
        },
        ToolSpec {
            name: "instance_get",
            description: descriptions::INSTANCE_GET,
            input_schema: schema_instance_id_in,
            output_schema: schema_instance_get_out,
            run: run_instance_get,
        },
        ToolSpec {
            name: "instance_list",
            description: descriptions::INSTANCE_LIST,
            input_schema: schema_instance_list_in,
            output_schema: schema_instance_list_out,
            run: run_instance_list,
        },
        ToolSpec {
            name: "instance_history",
            description: descriptions::INSTANCE_HISTORY,
            input_schema: schema_instance_history_in,
            output_schema: schema_instance_history_out,
            run: run_instance_history,
        },
        ToolSpec {
            name: "simulate",
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

pub fn tools_list_result() -> Value {
    let tools: Vec<Value> = registry()
        .into_iter()
        .map(|t| {
            let mut tool = BTreeMap::new();
            tool.insert("name".into(), Value::Str(t.name.into()));
            tool.insert("description".into(), Value::Str(t.description.into()));
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
