use fsm_core::json::Value;

use crate::clock::Clock;
use crate::store::{ErrorObj, Store};

use super::validate::{type_name, validate_args};
use super::{MUTATING_TOOLS, registry};

/// What a tool call knows about the request that made it.
///
/// `dispatch` cannot see the request's `params`, so it cannot reach
/// `_meta.progressToken` and cannot consult a cancellation flag. This is what
/// carries both — built and threaded now, consumed by `6002` and `6003`, so
/// neither has to reshape a signature to do its own job.
#[derive(Default)]
pub struct ToolCtx<'a> {
    /// The one writer, for a tool that reports progress.
    pub notifier: Option<&'a crate::mcp::notify::Notifier>,
    /// The request's own id, which a cancellation names.
    pub request_id: Option<Value>,
    /// The request's `_meta`, where a progress token lives.
    pub meta: Option<Value>,
    /// Whether the client has withdrawn this request.
    pub cancel: crate::mcp::cancel::CancelFlag,
    /// Both halves of the session, for a tool that asks the client a
    /// question and waits for the answer. `None` outside a protocol session —
    /// the CLI, and every test that is not one.
    pub io: Option<&'a std::cell::RefCell<crate::mcp::notify::SessionIo<'a>>>,
    /// Whether the client advertised `elicitation` at `initialize`.
    pub client_elicitation: bool,
}

/// Dispatch with no request context: the CLI and every test that is not a
/// protocol session.
pub fn dispatch(
    store: &mut Store,
    clock: &mut dyn Clock,
    name: &str,
    args: &Value,
) -> Result<Value, ErrorObj> {
    dispatch_with(store, clock, name, args, &ToolCtx::default())
}

/// Dispatch a call that came from a protocol request.
pub fn dispatch_with(
    store: &mut Store,
    clock: &mut dyn Clock,
    name: &str,
    args: &Value,
    ctx: &ToolCtx<'_>,
) -> Result<Value, ErrorObj> {
    // A call that carried a `progressToken` gets a live reporter; one that
    // did not gets a discarding one, so a handler never asks whether anyone
    // is listening — and a discarding reporter emits nothing at all, which
    // is what keeps every existing golden byte-identical.
    let progress =
        crate::mcp::progress::ProgressReporter::from_meta(ctx.meta.as_ref(), ctx.notifier);
    // The two tools with coarse loops take the flag as well as the reporter:
    // they are the same two loops, and a call that can report where it is can
    // also notice that nobody is waiting for it any more.
    if (progress.is_live() || ctx.cancel.cancelled()) && super::PROGRESS_TOOLS.contains(&name) {
        if let Some(refusal) = read_only_refusal(store, name, args) {
            return Err(attach_request_id(refusal, args));
        }
        let spec = registry()
            .into_iter()
            .find(|t| t.name == name)
            .ok_or_else(|| ErrorObj::new("req/args_invalid", format!("unknown tool {name}")))?;
        validate_args(&(spec.input_schema)(), args).map_err(|e| attach_request_id(e, args))?;
        return match name {
            "simulate" => {
                super::handlers::run_simulate_with(store, clock, args, &progress, &ctx.cancel)
            }
            _ => super::handlers::run_instance_history_with(
                store,
                clock,
                args,
                &progress,
                &ctx.cancel,
            ),
        }
        .map_err(|e| attach_request_id(e, args));
    }
    let spec = registry()
        .into_iter()
        .find(|t| t.name == name)
        .ok_or_else(|| ErrorObj::new("req/args_invalid", format!("unknown tool {name}")))?;
    if let Some(refusal) = read_only_refusal(store, name, args) {
        return Err(attach_request_id(refusal, args));
    }
    if name == "instance_create" {
        if let Some(ctx) = args.get("context") {
            if !ctx.is_obj() {
                return Err(attach_request_id(
                    crate::store::context_not_object(type_name(ctx)),
                    args,
                ));
            }
        }
    }
    if let Err(mut e) = validate_args(&(spec.input_schema)(), args) {
        if matches!(name, "instance_send" | "deadline_poll") {
            if let Some(iid) = args.get("instance_id").and_then(Value::as_str) {
                if let Ok(view) = store.instance_view(iid, None, None) {
                    if let Value::Obj(d) = &mut e.details {
                        if let Some(en) = view.get("enabled_events") {
                            d.insert("enabled_events".into(), en.clone());
                        }
                        d.insert("instance_id".into(), Value::Str(iid.into()));
                    }
                }
            }
        }
        return Err(attach_request_id(e, args));
    }
    (spec.run)(store, clock, args).map_err(|e| attach_request_id(e, args))
}

/// Refuse a mutator on a server that holds no writer.
///
/// The refusal happens here, before the handler runs, so the model gets one
/// sentence naming the mode rather than an `io/write` from deep inside the
/// store. A `dry_run` create is the documented exception: it validates a
/// definition without writing anything, and that is exactly what an author
/// wants from a monitoring session.
fn read_only_refusal(store: &Store, name: &str, args: &Value) -> Option<ErrorObj> {
    if !store.journal.is_read_only() || !MUTATING_TOOLS.contains(&name) {
        return None;
    }
    // Two tools have a legal read-only path, and both are the same shape: a
    // dry run answers a question without writing an answer down. A migration
    // preview is exactly what a monitoring session should be able to ask
    // before anybody decides to write.
    if matches!(name, "machine_create" | "instance_migrate")
        && args.get("dry_run").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    Some(
        ErrorObj::new(
            "io/write",
            format!("{name} needs a writable store; this server is read-only"),
        )
        .hint(
            "this fsm serve runs read-only so the executor owns writes: author and trigger from the command line, or restart serve without --read-only",
        ),
    )
}

fn attach_request_id(e: ErrorObj, args: &Value) -> ErrorObj {
    match args.get("request_id").and_then(Value::as_str) {
        Some(rid) if !rid.is_empty() => e.request_id(rid),
        _ => e,
    }
}

pub(super) fn str_arg<'a>(args: &'a Value, k: &str) -> Option<&'a str> {
    args.get(k).and_then(Value::as_str)
}

pub(super) fn expect_seq_arg(args: &Value) -> Option<u64> {
    args.get("expect_seq").and_then(|value| match value {
        Value::Num(number) | Value::Str(number) => number.parse().ok(),
        _ => None,
    })
}
