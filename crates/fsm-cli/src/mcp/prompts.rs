use fsm_core::json::Value;
use std::collections::BTreeMap;

use crate::store::ErrorObj;

pub const INSTRUCTIONS: &str = "fsm runs deterministic, auditable state machines with either one state tree or multiple orthogonal regions. Specs may include explicit deadlines. Workflow: read fsm://docs/spec → machine_create (dry_run first) → instance_create → instance_send. Consult tagged configuration, enabled_events, and deadlines_pending instead of guessing. Time never advances implicitly: call deadline_poll when due. Decimal values are JSON strings (\"125.50\"), never numbers. Execute pending effects, then acknowledge each with effect_ack. Retry the SAME request_id after a timeout and a NEW one after correcting content. Use simulate for event sequences without recording; it does not poll deadlines. Subscribe to fsm://instance/{id} to be told when an instance advances instead of polling instance_get.";

pub const AUTHOR_MACHINE: &str =
    "Guided flow to author, validate, and prove a new machine from a goal.";

pub const DRIVE_INSTANCE: &str = "Guided flow to advance a running instance: what can fire now, what is waiting, and what to send.";

pub const DIAGNOSE_INSTANCE: &str =
    "Guided flow to find out why an instance did what it did, from its own record of it.";

/// One prompt argument. A client rendering a form shows the title on the
/// field and the description under it; with only a name it shows `goal`.
fn argument(name: &str, title: &str, description: &str, required: bool) -> Value {
    Value::Obj(BTreeMap::from([
        ("name".to_string(), Value::Str(name.into())),
        ("title".to_string(), Value::Str(title.into())),
        ("description".to_string(), Value::Str(description.into())),
        ("required".to_string(), Value::Bool(required)),
    ]))
}

fn prompt(name: &str, title: &str, description: &str, arguments: Vec<Value>) -> Value {
    Value::Obj(BTreeMap::from([
        ("name".to_string(), Value::Str(name.into())),
        ("title".to_string(), Value::Str(title.into())),
        ("description".to_string(), Value::Str(description.into())),
        ("arguments".to_string(), Value::Arr(arguments)),
    ]))
}

pub fn list() -> Value {
    Value::Obj(BTreeMap::from([(
        "prompts".into(),
        Value::Arr(vec![
            prompt(
                "author_machine",
                "Author a machine",
                AUTHOR_MACHINE,
                vec![argument(
                    "goal",
                    "Goal",
                    "What the workflow must accomplish.",
                    true,
                )],
            ),
            prompt(
                "drive_instance",
                "Drive an instance",
                DRIVE_INSTANCE,
                vec![
                    argument(
                        "instance_id",
                        "Instance",
                        "The running instance to advance.",
                        true,
                    ),
                    // Optional, and completed from this instance's own
                    // enabled events once the id above is resolved.
                    argument(
                        "event",
                        "Event",
                        "An event to send, if you already know which one.",
                        false,
                    ),
                ],
            ),
            prompt(
                "diagnose_instance",
                "Diagnose an instance",
                DIAGNOSE_INSTANCE,
                vec![argument(
                    "instance_id",
                    "Instance",
                    "The instance that did something surprising.",
                    true,
                )],
            ),
        ]),
    )]))
}

/// The prompts this server serves, in listing order.
pub const NAMES: &[&str] = &["author_machine", "drive_instance", "diagnose_instance"];

pub fn get(name: &str, args: Option<&Value>) -> Result<Value, ErrorObj> {
    match name {
        "author_machine" => author_machine(args),
        "drive_instance" => drive_instance(args),
        "diagnose_instance" => diagnose_instance(args),
        _ => Err(ErrorObj::new("req/args_invalid", "unknown prompt")
            .hint(format!("valid names: {}", NAMES.join(", ")))
            .details(Value::Obj(BTreeMap::from([(
                "valid".into(),
                Value::Arr(NAMES.iter().map(|n| Value::Str((*n).into())).collect()),
            )])))),
    }
}

/// One required argument, or the error naming the field that is missing.
fn required<'a>(args: Option<&'a Value>, field: &str) -> Result<&'a str, ErrorObj> {
    args.and_then(|a| a.get(field))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ErrorObj::new("req/args_invalid", format!("missing {field}")).details(Value::Obj(
                BTreeMap::from([("field".into(), Value::Str(field.into()))]),
            ))
        })
}

/// One user message, which is the shape every prompt here returns.
fn message(text: String) -> Value {
    Value::Obj(BTreeMap::from([(
        "messages".to_string(),
        Value::Arr(vec![Value::Obj(BTreeMap::from([
            ("role".to_string(), Value::Str("user".into())),
            (
                "content".to_string(),
                Value::Obj(BTreeMap::from([
                    ("type".to_string(), Value::Str("text".into())),
                    ("text".to_string(), Value::Str(text)),
                ])),
            ),
        ]))]),
    )]))
}

/// Advancing a workflow: what can fire, what is waiting, what to send, and
/// how to be told when it moves rather than asking again.
fn drive_instance(args: Option<&Value>) -> Result<Value, ErrorObj> {
    let instance_id = required(args, "instance_id")?;
    Ok(message(format!(
        "Instance: {instance_id}\n\
         1. instance_get({instance_id}) — read `configuration`, `enabled_events`, `effects_pending`, and `deadlines_pending`.\n\
         2. Send only an event listed as enabled: instance_send with a NEW request_id, and the SAME id on a retry after a timeout.\n\
         3. Run each pending effect and acknowledge it with effect_ack; an ack advances nothing by itself.\n\
         4. A deadline applies only when you poll for it: call deadline_poll when one is due, once per due schedule.\n\
         5. Subscribe to fsm://instance/{instance_id} to be told when it advances, instead of reading it again."
    )))
}

/// Diagnosis: the instance's own record of what it did, then the decision
/// behind the step that surprised you.
fn diagnose_instance(args: Option<&Value>) -> Result<Value, ErrorObj> {
    let instance_id = required(args, "instance_id")?;
    Ok(message(format!(
        "Instance: {instance_id}\n\
         1. instance_history({instance_id}) with trace on — every record this instance wrote, applied and rejected alike.\n\
         2. Find the seq where it went wrong; a rejection carries its code and a repair hint.\n\
         3. Explain that seq — every candidate transition, each guard's verdict, and every context change the step made.\n\
         4. Read fsm://instance/{instance_id}/history for the same page as a resource, and fsm://machine/{{id}} for the definition it ran against.\n\
         5. Compare what the definition allows against what was sent: a refusal is usually the machine being right."
    )))
}

fn author_machine(args: Option<&Value>) -> Result<Value, ErrorObj> {
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
    Ok(message(text))
}
