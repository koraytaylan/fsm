//! What each protocol method does.
//!
//! Split from the serve loop when that file passed a thousand lines, along
//! the seam it already had: `serve.rs` is the transport's loop — modes,
//! startup, reading lines, the executor tick — and this is what a request
//! *means*. Both transports call `handle_request`; only one of them has a
//! loop.
//!
//! Plan 0015 task 7002.

use std::collections::BTreeMap;

use fsm_core::json::Value;

use crate::clock::Clock;
use crate::mcp::jsonrpc::{
    INVALID_PARAMS, METHOD_NOT_FOUND, NOT_INITIALIZED, RESOURCE_NOT_FOUND, result_response,
};
use crate::mcp::notify::Notifier;
use crate::mcp::{logging, subscribe, tools};
use crate::store::{ErrorObj, Store};

use super::serve::{
    Live, fsm_ping_result, initialize_result, negotiate, rpc_error, send_line, tool_error, tool_ok,
};

/// Answer one request.
///
/// The one entry point both transports use: the stdio loop calls it per
/// line, and the HTTP endpoint calls it per POST. A second implementation
/// of "what does this method do" is the thing this plan must not create.
#[allow(clippy::too_many_arguments)]
pub fn handle_request<'a>(
    output: &'a Notifier,
    store: Option<&mut Store>,
    clock: &mut dyn Clock,
    initialized: &mut bool,
    live: &mut Live,
    id: Value,
    method: &str,
    params: Option<Value>,
    mode_note: &'static str,
    io: Option<&'a std::cell::RefCell<crate::mcp::notify::SessionIo<'a>>>,
    // Where the change feed writes, when that is not where this request's
    // answer goes. Over stdio the two are the same stream; over HTTP the
    // answer goes into this POST's body and the feed must outlive it.
    feed_out: Option<&Notifier>,
) -> std::io::Result<()> {
    match method {
        "ping" => send_line(output, &result_response(id, Value::Obj(Default::default()))),
        "initialize" => {
            let offered = params
                .as_ref()
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str);
            let version = negotiate(offered);
            *initialized = true;
            // The only message that carries what the client can do. A tool
            // that would ask the client a question needs to know whether
            // there is anybody able to answer, and it cannot see `initialize`
            // from where it runs.
            live.client_elicitation = super::elicit::client_supports(params.as_ref());
            send_line(
                output,
                &result_response(id, initialize_result(version, mode_note)),
            )?;
            // A client that only reads stdout would otherwise never learn
            // why every store-backed call is failing. Said once, at `error`,
            // and only now: nothing may be sent before `initialize`.
            if let Some(detail) = live.degraded.clone() {
                logging::message(
                    output,
                    live.level,
                    *initialized,
                    logging::Level::Error,
                    "fsm.store",
                    || {
                        Value::Obj(BTreeMap::from([
                            ("degraded".to_string(), Value::Bool(true)),
                            ("detail".to_string(), Value::Str(detail.clone())),
                            (
                                "next".to_string(),
                                Value::Str("call store_doctor for the diagnosis".into()),
                            ),
                        ]))
                    },
                );
            }
            Ok(())
        }
        _ if !*initialized => send_line(
            output,
            &rpc_error(id, NOT_INITIALIZED, "Server not initialized"),
        ),
        "tools/list" => send_line(output, &result_response(id, tools::tools_list_result())),
        "completion/complete" => match super::complete::complete(params.as_ref(), store.as_deref())
        {
            Ok(result) => send_line(output, &result_response(id, result)),
            Err(invalid) => send_line(output, &rpc_error(id, INVALID_PARAMS, &invalid.0)),
        },
        "resources/list" => send_line(
            output,
            &result_response(id, super::resources::list(store.as_deref())),
        ),
        "resources/templates/list" => {
            send_line(output, &result_response(id, super::resources::templates()))
        }
        // Every arm this plan adds is routed here and only here: five tasks
        // each adding one would serialise the plan behind one file for no
        // benefit. The registries below are real; what the later tasks add
        // is the notification each one produces.
        "resources/subscribe" => {
            let uri = params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if uri.is_empty() {
                return send_line(output, &rpc_error(id, INVALID_PARAMS, "uri is required"));
            }
            // Validated against the resolver rather than a prefix match, so a
            // subscription can never name something unreadable — and refused
            // with the code a read of the same URI would give.
            if super::resources::read(uri, store.as_deref()).is_err() {
                return send_line(
                    output,
                    &rpc_error(id, RESOURCE_NOT_FOUND, "Resource not found"),
                );
            }
            // An unbounded set is an unbounded per-poll cost, and this cap is
            // the only backpressure the design has.
            if !live.subscriptions.watches(uri)
                && live.subscriptions.len() >= subscribe::MAX_SUBSCRIPTIONS
            {
                return send_line(
                    output,
                    &rpc_error(
                        id,
                        INVALID_PARAMS,
                        &format!(
                            "a session may watch at most {} resources; unsubscribe one first",
                            subscribe::MAX_SUBSCRIPTIONS
                        ),
                    ),
                );
            }
            live.subscriptions.subscribe(uri);
            // The feed starts on the first successful subscription and not
            // before. It is never stopped when the last one goes: a session
            // that unsubscribes and resubscribes is common, and a parked feed
            // costs one integer comparison per interval.
            live.ensure_feed(
                store.as_ref().map(|st| st.data_dir.clone()),
                feed_out.unwrap_or(output),
            );
            send_line(output, &result_response(id, Value::Obj(Default::default())))
        }
        "resources/unsubscribe" => {
            let uri = params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            if uri.is_empty() {
                return send_line(output, &rpc_error(id, INVALID_PARAMS, "uri is required"));
            }
            live.subscriptions.unsubscribe(uri);
            send_line(output, &result_response(id, Value::Obj(Default::default())))
        }
        "logging/setLevel" => {
            let named = params
                .as_ref()
                .and_then(|p| p.get("level"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match logging::Level::parse(named) {
                Some(level) => {
                    live.level = Some(level);
                    send_line(output, &result_response(id, Value::Obj(Default::default())))
                }
                None => send_line(
                    output,
                    &rpc_error(
                        id,
                        INVALID_PARAMS,
                        &format!("level must be one of {}", logging::Level::names()),
                    ),
                ),
            }
        }
        "resources/read" => {
            let uri = params
                .as_ref()
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match super::resources::read(uri, store.as_deref()) {
                Ok(v) => send_line(output, &result_response(id, v)),
                Err(_) => send_line(
                    output,
                    &rpc_error(id, RESOURCE_NOT_FOUND, "Resource not found"),
                ),
            }
        }
        "prompts/list" => send_line(output, &result_response(id, super::prompts::list())),
        "prompts/get" => {
            let name = params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let args = params.as_ref().and_then(|p| p.get("arguments"));
            match super::prompts::get(name, args) {
                Ok(v) => send_line(output, &result_response(id, v)),
                Err(e) => send_line(output, &rpc_error(id, INVALID_PARAMS, &e.message)),
            }
        }
        "tools/call" => {
            let name = params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let raw_args = params.as_ref().and_then(|p| p.get("arguments"));
            if raw_args.is_some() && raw_args.and_then(Value::as_obj).is_none() {
                return send_line(
                    output,
                    &rpc_error(id, INVALID_PARAMS, "arguments must be an object"),
                );
            }
            let args = raw_args.cloned().unwrap_or(Value::Obj(Default::default()));
            // What the call knows about its own request: the writer, the id
            // a cancellation would name, and the `_meta` a progress token
            // lives in. Threaded now; `6002` and `6003` are the consumers.
            let ctx = tools::ToolCtx {
                notifier: Some(output),
                request_id: Some(id.clone()),
                meta: params.as_ref().and_then(|p| p.get("_meta")).cloned(),
                cancel: live.cancellations.flag(&id),
                // Both halves of the session, for the one tool that will ask
                // the client a question and wait for the answer. Unused until
                // `6401`; provided here so that task never touches this loop.
                io,
                client_elicitation: live.client_elicitation,
            };
            if name == "fsm_ping" {
                send_line(output, &result_response(id, fsm_ping_result()))
            } else if !tools::names().contains(&name) {
                send_line(
                    output,
                    &rpc_error(
                        id,
                        INVALID_PARAMS,
                        &format!("unknown tool; valid: {}", tools::names().join(" ")),
                    ),
                )
            } else {
                let called = match (store, live.degraded_dir.clone()) {
                    (Some(st), _) => tools::dispatch_with(st, clock, name, &args, &ctx),
                    // Degraded: the diagnostic tools answer from the
                    // directory itself, and everything else is refused with
                    // the diagnosis rather than with "unavailable".
                    (None, Some(data_dir)) => {
                        tools::dispatch_degraded(&data_dir, clock, name, &args, &ctx)
                    }
                    (None, None) => Err(ErrorObj::new("io/read", "no store")),
                };
                match called {
                    Ok(v) => send_line(output, &result_response(id, tool_ok(name, v))),
                    Err(e) => send_line(output, &result_response(id, tool_error(&e))),
                }
            }
        }
        other => send_line(
            output,
            &rpc_error(id, METHOD_NOT_FOUND, &format!("method not found: {other}")),
        ),
    }
}
