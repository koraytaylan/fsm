use fsm_core::json::Value;
use std::collections::BTreeMap;

use crate::store::ErrorObj;

pub const INSTRUCTIONS: &str = "fsm runs deterministic, auditable state machines. Author a JSON spec (state tree, typed context, typed events, guarded transitions), then create instances and send events. Workflow: read fsm://docs/spec if unsure → machine_create (dry_run: true first) → instance_create → instance_send. Every response includes enabled_events — consult it instead of guessing. All decimal values are JSON strings (\"125.50\"), never numbers. When a response lists pending effects, execute them, acknowledge each with effect_ack, and advance with a domain event. Every error includes a hint: retry the SAME request_id after a timeout, a NEW one after a correction. Use simulate to test sequences without recording.";

pub const AUTHOR_MACHINE: &str =
    "Guided flow to author, validate, and prove a new machine from a goal.";

pub fn list() -> Value {
    let mut arg = BTreeMap::new();
    arg.insert("name".into(), Value::Str("goal".into()));
    arg.insert(
        "description".into(),
        Value::Str("What the workflow must accomplish.".into()),
    );
    arg.insert("required".into(), Value::Bool(true));
    let mut p = BTreeMap::new();
    p.insert("name".into(), Value::Str("author_machine".into()));
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
         2. Draft the spec JSON (state tree, typed context, typed events, guarded transitions, invariants).\n\
         3. Call machine_create with dry_run until clean.\n\
         4. Call machine_create to persist the definition.\n\
         5. simulate a happy path and a rejection path, checking traces.\n\
         6. instance_create and drive with instance_send, consulting enabled_events."
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
