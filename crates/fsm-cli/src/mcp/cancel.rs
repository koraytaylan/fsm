//! Cancellation, and an honest account of what it can interrupt.
//!
//! The registry lands with the routing (task 5702), so a
//! `notifications/cancelled` is recorded rather than written to stderr and
//! discarded; `6003` is what consults it and says honestly what it can and
//! cannot interrupt.

use std::collections::BTreeSet;

use fsm_core::json::Value;

/// The request ids a client has asked this server to cancel.
#[derive(Debug, Clone, Default)]
pub struct Cancellations {
    ids: BTreeSet<String>,
}

impl Cancellations {
    /// Record a `notifications/cancelled` for one request id.
    pub fn cancel(&mut self, id: &Value) {
        self.ids.insert(key(id));
    }

    /// Whether this request was cancelled.
    pub fn cancelled(&self, id: &Value) -> bool {
        self.ids.contains(&key(id))
    }

    /// Forget a request that has finished.
    pub fn finish(&mut self, id: &Value) {
        self.ids.remove(&key(id));
    }
}

/// A request id is a string or a number on the wire; both are compared by
/// their canonical text, so `1` and `"1"` are the same request to nobody.
fn key(id: &Value) -> String {
    String::from_utf8(fsm_core::canon::canon_bytes(id)).unwrap_or_default()
}
