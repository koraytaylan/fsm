//! Store preparation and settlement for a client interaction.
//!
//! The session owns the question and answer between these calls; settlement
//! uses the current Store state through the ordinary event-send operation.

use std::collections::BTreeMap;

use fsm_core::json::Value;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use super::dispatch::str_arg;
use super::handlers::run_instance_send;

/// Owned values crossing the store boundary while the client answers.
pub(crate) struct PreparedElicitation {
    instance_id: String,
    event: String,
    request_id: String,
    machine: fsm_core::machine::CompiledMachine,
    params: Value,
}

impl PreparedElicitation {
    /// Move the question to the session without retaining a duplicate wire value.
    pub(crate) fn take_params(&mut self) -> Value {
        std::mem::replace(&mut self.params, Value::Null)
    }
}

/// Prepare owned interaction state without retaining a Store borrow.
pub(crate) fn prepare_elicitation(
    store: &Store,
    args: &Value,
) -> Result<PreparedElicitation, ErrorObj> {
    let instance_id = str_arg(args, "instance_id").unwrap_or("").to_string();
    let event = str_arg(args, "event").unwrap_or("").to_string();
    let request_id = str_arg(args, "request_id").unwrap_or("").to_string();
    // Enabled on *this* instance, right now, in the ordinary vocabulary.
    let view = store.instance_view(&instance_id, None, None)?;
    let sendable = view
        .get("enabled_events")
        .and_then(Value::as_arr)
        .map(|events| {
            events
                .iter()
                .filter(|e| {
                    matches!(
                        e.get("status").and_then(Value::as_str),
                        Some("enabled" | "depends_on_payload")
                    )
                })
                .filter_map(|e| e.get("event").and_then(Value::as_str))
                .map(str::to_string)
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();
    if !sendable.contains(&event) {
        return Err(ErrorObj::new(
            "run/not_enabled",
            format!("{event} cannot fire on {instance_id} right now"),
        )
        .hint(format!("currently enabled: {}", sendable.join(", ")))
        .details(Value::Obj(BTreeMap::from([
            ("instance_id".into(), Value::Str(instance_id.clone())),
            (
                "enabled_events".into(),
                view.get("enabled_events").cloned().unwrap_or(Value::Null),
            ),
        ]))));
    }

    let machine_id = store
        .state
        .instance_machines
        .get(&instance_id)
        .cloned()
        .unwrap_or_default();
    let machine = store
        .state
        .machines
        .get(&machine_id)
        .ok_or_else(|| ErrorObj::new("req/machine_not_found", machine_id.clone()))?
        .compiled
        .clone();
    let schema = crate::mcp::elicit::schema_for_event(&machine, &event)?;
    let message = str_arg(args, "message")
        .map(str::to_string)
        .unwrap_or_else(|| format!("{event} on {instance_id}"));
    let params = Value::Obj(BTreeMap::from([
        ("message".to_string(), Value::Str(message)),
        ("requestedSchema".to_string(), schema),
    ]));

    Ok(PreparedElicitation {
        instance_id,
        event,
        request_id,
        machine,
        params,
    })
}

/// Resume through the ordinary send path using the original idempotency key.
pub(crate) fn settle_elicitation(
    store: &mut Store,
    clock: &mut dyn Clock,
    prepared: PreparedElicitation,
    answer: Value,
) -> Result<Value, ErrorObj> {
    let PreparedElicitation {
        instance_id,
        event,
        request_id,
        machine,
        params: _,
    } = prepared;
    let action = answer
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("cancel")
        .to_string();
    if action != "accept" {
        // Nothing was sent, so nothing is journaled and the key stays
        // unclaimed — the caller may reuse it for whatever it does instead.
        return Ok(Value::Obj(BTreeMap::from([
            ("ok".to_string(), Value::Str("true".into())),
            ("action".to_string(), Value::Str(action)),
            ("applied".to_string(), Value::Bool(false)),
            ("event".to_string(), Value::Str(event)),
            ("instance_id".to_string(), Value::Str(instance_id)),
            ("request_id".to_string(), Value::Str(request_id)),
        ])));
    }

    let content = answer
        .get("content")
        .cloned()
        .unwrap_or(Value::Obj(BTreeMap::new()));
    let payload = crate::mcp::elicit::payload_from_content(&machine, &event, &content)?;
    // The ordinary send path with the caller's key. There is no elicitation
    // record and no new record kind: what happened to the workflow is that
    // an event arrived.
    let mut sent = run_instance_send(
        store,
        clock,
        &Value::Obj(BTreeMap::from([
            ("instance_id".to_string(), Value::Str(instance_id.clone())),
            (
                "event".to_string(),
                Value::Obj(BTreeMap::from([
                    ("name".to_string(), Value::Str(event.clone())),
                    ("payload".to_string(), payload),
                ])),
            ),
            ("request_id".to_string(), Value::Str(request_id)),
        ])),
    )?;
    if let Value::Obj(fields) = &mut sent {
        fields.insert("action".to_string(), Value::Str("accept".into()));
        fields.insert("event".to_string(), Value::Str(event));
        fields.insert("instance_id".to_string(), Value::Str(instance_id));
    }
    Ok(sent)
}
