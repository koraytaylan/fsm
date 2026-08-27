//! Cancellation, and an honest account of what it can interrupt.
//!
//! The registry lands with the routing (task 5702); this is what consults it
//! and says honestly what it can and cannot interrupt.
//!
//! Two things it can do. It can decline to start a call whose id was already
//! cancelled — genuinely reachable, since a client can cancel request 7
//! while the server is still working on request 6 — and it can stop a call
//! at a coarse loop boundary, between events in a simulation or between
//! chunks of a history page.
//!
//! One thing it cannot: **a single `step` is not interruptible**. Engine
//! operations are bounded by the evaluation budget and are short by
//! construction, and threading a cancellation token through the pure core
//! would cost the core its purity and buy nothing. A capability that
//! overpromises is worse than one that is absent, so this is stated here and
//! in the documentation rather than left for somebody to discover.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use fsm_core::json::Value;

/// The request ids a client has asked this server to cancel.
#[derive(Debug, Clone, Default)]
pub struct Cancellations {
    ids: Arc<Mutex<BTreeSet<String>>>,
}

impl Cancellations {
    /// Record a `notifications/cancelled` for one request id.
    pub fn cancel(&mut self, id: &Value) {
        self.lock().insert(key(id));
    }

    /// Whether this request was cancelled.
    pub fn cancelled(&self, id: &Value) -> bool {
        self.lock().contains(&key(id))
    }

    /// Forget a request that has been dealt with.
    ///
    /// An id is cleared once consumed, so a client reusing it later is not
    /// silently cancelled by a stale entry.
    pub fn finish(&mut self, id: &Value) {
        self.lock().remove(&key(id));
    }

    /// A flag one call can consult from inside its own loops.
    pub fn flag(&self, id: &Value) -> CancelFlag {
        CancelFlag {
            ids: Arc::clone(&self.ids),
            id: key(id),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.ids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// One call's view of whether it has been cancelled.
///
/// Checked at coarse boundaries only. A call that finds itself cancelled
/// returns a **tool error** carrying `req/cancelled`, not a JSON-RPC error:
/// the call was dispatched, so the outcome is a tool outcome.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag {
    ids: Arc<Mutex<BTreeSet<String>>>,
    id: String,
}

impl CancelFlag {
    /// Whether this call has been cancelled.
    pub fn cancelled(&self) -> bool {
        if self.id.is_empty() {
            return false;
        }
        self.ids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(&self.id)
    }

    /// The tool error a cancelled call returns.
    pub fn refusal() -> crate::store::ErrorObj {
        crate::store::ErrorObj::new("req/cancelled", "the client cancelled this request").hint(
            "the work stopped at its next boundary; a single engine step is not interruptible",
        )
    }
}

/// A request id is a string or a number on the wire; both are compared by
/// their canonical text, so `1` and `"1"` are the same request to nobody.
fn key(id: &Value) -> String {
    String::from_utf8(fsm_core::canon::canon_bytes(id)).unwrap_or_default()
}
