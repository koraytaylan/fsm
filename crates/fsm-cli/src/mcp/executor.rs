//! The executor contract visible to an MCP client before it authors effects.
//!
//! This is session configuration, never inferred from a writer lock. A
//! read-only connection cannot know which commands another process runs.

use std::collections::BTreeMap;

use fsm_core::json::Value;
use fsm_execute::config::{Advance, HandlerTable};

use crate::store::Store;

/// Describe the loaded table without disclosing command lines or credentials.
pub(crate) fn handlers(table: &HandlerTable) -> Value {
    Value::Arr(
        table
            .handlers
            .values()
            .map(|handler| {
                Value::Obj(BTreeMap::from([
                    ("effect".into(), Value::Str(handler.effect.clone())),
                    ("kind".into(), Value::Str(handler.kind.as_str().into())),
                    (
                        "required_args".into(),
                        Value::Arr(
                            handler
                                .required_args()
                                .into_iter()
                                .map(Value::Str)
                                .collect(),
                        ),
                    ),
                    (
                        "timeout_ms".into(),
                        Value::Num(handler.timeout_ms.to_string()),
                    ),
                    ("on_ok".into(), advance(handler.on_ok.as_ref())),
                    ("on_failed".into(), advance(handler.on_failed.as_ref())),
                    (
                        "retry".into(),
                        Value::Obj(BTreeMap::from([
                            (
                                "attempts".into(),
                                Value::Num(handler.retry.attempts.to_string()),
                            ),
                            (
                                "backoff_ms".into(),
                                Value::Num(handler.retry.backoff_ms.to_string()),
                            ),
                            (
                                "max_backoff_ms".into(),
                                Value::Num(handler.retry.max_backoff_ms.to_string()),
                            ),
                            (
                                "on".into(),
                                Value::Arr(
                                    handler.retry.on.iter().cloned().map(Value::Str).collect(),
                                ),
                            ),
                        ])),
                    ),
                ]))
            })
            .collect(),
    )
}

fn advance(advance: Option<&Advance>) -> Value {
    advance.map_or(Value::Null, |advance| {
        Value::Obj(BTreeMap::from([
            ("event".into(), Value::Str(advance.event.clone())),
            ("payload".into(), advance.payload.clone()),
            (
                "stamps".into(),
                Value::Arr(advance.stamps.iter().cloned().map(Value::Str).collect()),
            ),
        ]))
    })
}

fn execution_ownership(store: Option<&Store>) -> Value {
    store.map_or(Value::Null, fsm_execute::service::inspect_ownership)
}

pub(crate) fn describe(store: Option<&Store>, handlers: Option<&Value>) -> Value {
    describe_mode(store, handlers, false)
}

pub(crate) fn describe_mode(
    store: Option<&Store>,
    handlers: Option<&Value>,
    autonomous: bool,
) -> Value {
    let (mode, progress, external_executor) = match store {
        None => ("degraded", "unavailable", "unknown"),
        Some(store) if store.journal.is_read_only() => ("read-only", "external", "unknown"),
        Some(_) if handlers.is_some() => (
            "embedded",
            if autonomous {
                "autonomous"
            } else {
                "client_requests"
            },
            "unknown",
        ),
        Some(_) => ("writer", "manual", "unknown"),
    };
    let embedded = mode == "embedded";
    Value::Obj(BTreeMap::from([
        ("format".into(), Value::Str(if autonomous { "fsm.executor/2" } else { "fsm.executor/1" }.into())),
        ("execution_ownership".into(), execution_ownership(store)),
        ("mode".into(), Value::Str(mode.into())),
        ("executes_effects".into(), Value::Bool(embedded)),
        ("external_executor".into(), Value::Str(external_executor.into())),
        ("progress".into(), Value::Str(progress.into())),
        ("handlers".into(), if embedded { handlers.cloned().unwrap_or(Value::Null) } else if mode == "writer" { Value::Arr(Vec::new()) } else { Value::Null }),
        ("result_mapping".into(), Value::Str("Handlers acknowledge outcomes and send the configured static on_ok/on_failed event, payload, and clock stamps. Command stdout and MCP results are not copied into event payloads. Without an outcome event, an ack does not advance the machine.".into())),
        ("unhandled_effects".into(), Value::Str("Pending effects without a matching handler remain pending; they are not executed automatically.".into())),
        ("next".into(), Value::Str(match mode {
            "embedded" if autonomous => "Match effect names and required_args to these handlers and declare their outcome events. Effects, recovery and deadlines progress while stdin remains open, including while the client is quiet. Polling and subscriptions observe progress; they do not drive it. Do not manually ack handled effects. Closing stdin stops admission and requests supervised shutdown.",
            "embedded" => "Match effect names and required_args to these handlers and declare their outcome events. Keep sending instance_get or ping requests while work is pending: each request drives one executor tick after its reply. Subscribing alone does not advance execution. Do not manually ack handled effects. The executor also polls deadlines on ticks.",
            "writer" => "This server does not execute effects. Execute and ack them manually, then send outcome events and poll deadlines; or restart with serve --execute --handlers <operator-owned-file> to author and run through one MCP connection. A separate fsm execute cannot journal outcomes while this writer holds the store.",
            "read-only" => "This connection cannot mutate workflows or verify an external executor's presence or handler table. Ask the operator for that table and execution status. If an external executor is running, subscribe to instances to observe it; otherwise nothing advances automatically.",
            _ => "Call store_doctor to diagnose the unavailable store. Execution capabilities cannot be verified until it opens.",
        }.into())),
    ]))
}
