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

use fsm_core::json::Value;

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
