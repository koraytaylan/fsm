use fsm_core::json::Value;
use std::collections::BTreeMap;

use crate::store::ErrorObj;

pub const INSTRUCTIONS: &str = "Read fsm://docs/spec and fsm://executor for mode, handlers, arguments and outcome events. Workflow: machine_create (dry_run first) → instance_create → instance_send. Consult enabled_events and deadlines_pending. Writer mode: run effects, effect_ack, send outcome events and deadline_poll when due. Executor progress: autonomous runs effects, deadlines and recovery with quiet clients (keep stdio stdin open; HTTP disconnects do not stop the host). Legacy client_requests needs requests to drive ticks. Subscribe to fsm://instance/{id} for updates; polling and subscriptions only observe autonomous progress. Decimal values are JSON strings. Retry the SAME request_id after timeout; corrected content needs a NEW id. simulate runs without effects or deadline polls.";

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
         1. Read fsm://executor to learn the current execution mode and configured handlers. Then instance_get({instance_id}) — read `configuration`, `enabled_events`, `effects_pending`, and `deadlines_pending`.\n\
         2. Send only an event listed as enabled: instance_send with a NEW request_id, and the SAME id on a retry after a timeout.\n\
         3. Effects in `effects_pending` are descriptors — the engine emits them but never runs them.\n\
            - In embedded mode, inspect the executor progress field. Autonomous mode progresses while the host runs, even when clients are quiet; polling and subscriptions observe it. Legacy client_requests mode drives a tick after each request. The executor acks handled effects and sends configured outcome events.\n\
            - In writer mode, run each effect yourself, effect_ack with the outcome, then instance_send a domain outcome event; an ack never advances a state. In read-only mode, confirm the external executor with the operator and subscribe to observe it.\n\
         4. In writer mode, call deadline_poll when due, once per due schedule. The executor polls deadlines automatically on its ticks.\n\
         5. Subscribe to fsm://instance/{instance_id} for updates; in embedded mode continue requests until the workflow settles, including any failure recovery."
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
         3. Before declaring side effects, read fsm://executor. Match configured effect names and required_args, and declare the on_ok/on_failed events with fields matching their static payloads and clock stamps. Missing handlers remain pending; missing outcome events leave an acked workflow stalled. Handler stdout is not mapped into event payloads: operations over dynamic data need an operator-owned adapter. If no executor is configured, arrange manual execution or restart with serve --execute --handlers <file>. Read fsm://docs/embedding for setup.\n\
         4. Call machine_create with dry_run until clean.\n\
         5. Call machine_create to persist the definition.\n\
         6. simulate a happy path and a rejection path, checking traces.\n\
         7. instance_create and drive with instance_send; use the execution mode from fsm://executor. Autonomous embedded mode progresses through completion and recovery while the host runs; legacy client_requests mode requires repeated requests. Writer mode requires manual effects, outcome events, and due deadline polls."
    );
    Ok(message(text))
}
