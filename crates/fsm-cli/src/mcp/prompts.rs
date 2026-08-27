use fsm_core::json::Value;
use std::collections::BTreeMap;

use crate::store::ErrorObj;

pub const INSTRUCTIONS: &str = "fsm runs deterministic, auditable state machines with either one state tree or multiple orthogonal regions. Specs may include explicit deadlines. Workflow: read fsm://docs/spec → machine_create (dry_run first) → instance_create → instance_send. Consult tagged configuration, enabled_events, and deadlines_pending instead of guessing. Time never advances implicitly: call deadline_poll when due. Decimal values are JSON strings (\"125.50\"), never numbers. Execute pending effects, then acknowledge each with effect_ack. Retry the SAME request_id after a timeout and a NEW one after correcting content. Use simulate for event sequences without recording; it does not poll deadlines. Subscribe to fsm://instance/{id} to be told when an instance advances instead of polling instance_get.";

pub const AUTHOR_MACHINE: &str =
    "Guided flow to author, validate, and prove a new machine from a goal.";

pub fn list() -> Value {
    let mut arg = BTreeMap::new();
    arg.insert("name".into(), Value::Str("goal".into()));
    // A client rendering this as a form shows the title on the field and the
    // description under it; with only a name it shows `goal`.
    arg.insert("title".into(), Value::Str("Goal".into()));
    arg.insert(
        "description".into(),
        Value::Str("What the workflow must accomplish.".into()),
    );
    arg.insert("required".into(), Value::Bool(true));
    let mut p = BTreeMap::new();
    p.insert("name".into(), Value::Str("author_machine".into()));
    p.insert("title".into(), Value::Str("Author a machine".into()));
    p.insert("description".into(), Value::Str(AUTHOR_MACHINE.into()));
    p.insert("arguments".into(), Value::Arr(vec![Value::Obj(arg)]));
    Value::Obj(BTreeMap::from([(
        "prompts".into(),
        Value::Arr(vec![Value::Obj(p)]),
    )]))
}

pub fn get(name: &str, args: Option<&Value>) -> Result<Value, ErrorObj> {
    if name != "author_machine" {
        return Err(ErrorObj::new("req/args_invalid", "unknown prompt")
            .hint("valid name: author_machine")
            .details(Value::Obj(BTreeMap::from([(
                "valid".into(),
                Value::Arr(vec![Value::Str("author_machine".into())]),
            )]))));
    }
    let goal = args
        .and_then(|a| a.get("goal"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ErrorObj::new("req/args_invalid", "missing goal").details(Value::Obj(BTreeMap::from([
                ("field".into(), Value::Str("goal".into())),
            ])))
        })?;
    let text = format!(
        "Goal: {goal}\n\
         1. Read fsm://docs/spec for the spec format and expression grammar.\n\
         2. Draft the spec JSON (one state tree or orthogonal regions, typed context/events, transitions, optional deadlines, invariants).\n\
         3. Call machine_create with dry_run until clean.\n\
         4. Call machine_create to persist the definition.\n\
         5. simulate a happy path and a rejection path, checking traces.\n\
         6. instance_create and drive with instance_send; consult enabled_events and deadlines_pending, and call deadline_poll only when due."
    );
    let mut msg = BTreeMap::new();
    msg.insert("role".into(), Value::Str("user".into()));
    msg.insert(
        "content".into(),
        Value::Obj(BTreeMap::from([
            ("type".into(), Value::Str("text".into())),
            ("text".into(), Value::Str(text)),
        ])),
    );
    Ok(Value::Obj(BTreeMap::from([(
        "messages".into(),
        Value::Arr(vec![Value::Obj(msg)]),
    )])))
}
