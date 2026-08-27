//! Asking the client a typed question, and reading the typed answer.
//!
//! The shell lands with `6301` because a module cannot be declared without
//! its file, and the task that owns `mod.rs` is the one that declares it.
//! `6401` fills the request path, `6402` the schema derivation, and `6403`
//! the tool.
//!
//! Nothing here is routed inbound: the server *sends* an elicitation request
//! and waits for the response on the same stream.
//!
//! Plan 0013 task 6301.

use std::sync::atomic::{AtomicU64, Ordering};

use fsm_core::json::Value;

use crate::clock::Clock;
use crate::mcp::jsonrpc::{Incoming, WireError, error_response, parse_line, result_response};
use crate::mcp::notify::SessionIo;
use crate::store::ErrorObj;

/// How long the server waits for an answer before giving up.
///
/// A person is being asked a question, so this is generous — but a server
/// that waits forever for a client that will never answer is a hung server.
pub const DEFAULT_TIMEOUT_MS: i64 = 300_000;

/// The JSON-RPC code for a request this server cannot serve *right now*.
///
/// In the implementation-defined range, because the protocol has no code for
/// "ask me again in a moment".
pub const SERVER_BUSY: i64 = -32004;

/// Server-side request ids, monotonic for the life of the process.
///
/// The prefix is what makes a collision impossible: whatever ids a client
/// chooses, none of them is `fsm-elicit-*` unless it is deliberately
/// impersonating this server, and a client that does that is answering its
/// own questions.
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// The next server-side request id.
pub fn next_request_id() -> String {
    format!("fsm-elicit-{}", NEXT_ID.fetch_add(1, Ordering::Relaxed))
}

/// Whether the client said it can be asked.
///
/// Captured at `initialize`, because that is the only message that carries
/// it, and consulted by the tool that would otherwise ask a client that
/// cannot answer.
pub fn client_supports(params: Option<&Value>) -> bool {
    params
        .and_then(|p| p.get("capabilities"))
        .and_then(|c| c.get("elicitation"))
        .is_some()
}

/// The elicitation request's `requestedSchema`, derived from an event's
/// declared fields. `6402`.
pub fn schema_for_event(_machine: &Value, _event: &str) -> Value {
    unimplemented!("plan 0013 task 6402")
}

/// Ask the client something and wait for its answer, with the nesting cap.
///
/// The cap is structural: the session's two halves are borrowed for the
/// whole exchange, so a second ask while one is outstanding cannot take
/// them. It returns an error rather than panicking on the failed borrow,
/// because a recursive ask is a design mistake and a diagnosable error is
/// worth more than a stack trace.
pub fn ask(
    io: &std::cell::RefCell<SessionIo<'_>>,
    method: &str,
    params: Value,
    clock: &mut dyn Clock,
) -> Result<Value, ErrorObj> {
    let Ok(mut borrowed) = io.try_borrow_mut() else {
        return Err(ErrorObj::new(
            "req/elicit_nested",
            "this session is already waiting for an answer",
        )
        .hint("finish the outstanding elicitation before starting another"));
    };
    request_and_await(&mut borrowed, method, params, clock)
}

/// Write one server-to-client request and read until its answer arrives.
///
/// While waiting, the client keeps working: its notifications are handled,
/// and its requests are answered — from the static surface where that is
/// possible, and with a busy error otherwise, because during a tool call the
/// store is already borrowed by the tool that is asking. A client left
/// waiting for a response it will never get would deadlock against a server
/// waiting for an answer it will never receive.
///
/// **The timeout bounds a talking client, not a silent one.** It is checked
/// before each read, so a client that sends nothing at all leaves this
/// blocked in `read_line` until the transport itself closes. Making it
/// bound silence too would need a reader that can be woken, which stdio does
/// not portably provide — so it is stated rather than implied.
pub fn request_and_await(
    io: &mut SessionIo<'_>,
    method: &str,
    params: Value,
    clock: &mut dyn Clock,
) -> Result<Value, ErrorObj> {
    let id = next_request_id();
    let request = Value::Obj(std::collections::BTreeMap::from([
        ("jsonrpc".to_string(), Value::Str("2.0".into())),
        ("id".to_string(), Value::Str(id.clone())),
        ("method".to_string(), Value::Str(method.into())),
        ("params".to_string(), params),
    ]));
    io.notifier()
        .send(&request)
        .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;

    let deadline = clock.now_ms().saturating_add(DEFAULT_TIMEOUT_MS);
    loop {
        if clock.now_ms() > deadline {
            return Err(ErrorObj::new(
                "req/elicit_timeout",
                format!("no answer within {DEFAULT_TIMEOUT_MS} ms"),
            )
            .hint("ask again, or send the event directly with instance_send"));
        }
        let Some(line) = io
            .read_line()
            .map_err(|e| ErrorObj::new("io/read", e.to_string()))?
        else {
            // The client is gone. There is nothing to answer and nothing to
            // report to: this is a session ending, not a failure.
            return Err(ErrorObj::new(
                "req/elicit_timeout",
                "the client closed the connection while the question was outstanding",
            )
            .hint("the session ended before an answer arrived"));
        };
        if line.trim().is_empty() {
            continue;
        }
        match parse_line(&line) {
            Ok(Incoming::Response {
                id: answered,
                result,
                error,
            }) => {
                if answered.as_str() != Some(id.as_str()) {
                    // Not ours. A client bug, and dropping it is strictly
                    // better than failing the session over it.
                    continue;
                }
                if let Some(error) = error {
                    let message = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("the client refused the request");
                    return Err(ErrorObj::new("req/elicit_failed", message)
                        .hint("the client answered with an error rather than a result"));
                }
                return Ok(result.unwrap_or(Value::Obj(Default::default())));
            }
            Ok(Incoming::Notification { method, params }) => {
                if method == "notifications/cancelled"
                    && params
                        .as_ref()
                        .and_then(|p| p.get("requestId"))
                        .and_then(Value::as_str)
                        == Some(id.as_str())
                {
                    return Err(ErrorObj::new(
                        "req/elicit_failed",
                        "the client cancelled the question",
                    )
                    .hint("nothing was journaled; the request_id is unclaimed"));
                }
            }
            Ok(Incoming::Request {
                id: their_id,
                method,
                params,
            }) => {
                let _ = params;
                let answer = match method.as_str() {
                    "ping" => result_response(their_id, Value::Obj(Default::default())),
                    "tools/list" => {
                        result_response(their_id, crate::mcp::tools::tools_list_result())
                    }
                    "prompts/list" => result_response(their_id, crate::mcp::prompts::list()),
                    "resources/templates/list" => {
                        result_response(their_id, crate::mcp::resources::templates())
                    }
                    // Everything else needs the store, which the tool that
                    // asked this question is already holding. Saying so beats
                    // both silence and a wrong answer.
                    _ => error_response(
                        their_id,
                        SERVER_BUSY,
                        "the server is waiting for an answer to its elicitation request; \
                         retry after answering it",
                    ),
                };
                io.notifier()
                    .send(&answer)
                    .map_err(|e| ErrorObj::new("io/write", e.to_string()))?;
            }
            Err(WireError::Parse(_)) => {
                let _ = io.notifier().send(&error_response(
                    Value::Null,
                    crate::mcp::jsonrpc::PARSE_ERROR,
                    "parse error",
                ));
            }
            Err(_) => {
                let _ = io.notifier().send(&error_response(
                    Value::Null,
                    crate::mcp::jsonrpc::INVALID_REQUEST,
                    "invalid request while an elicitation is outstanding",
                ));
            }
        }
    }
}
