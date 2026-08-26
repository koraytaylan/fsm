//! Cancellation, and an honest account of what it can interrupt.
//!
//! A skeleton: plan 0012 task 6003 fills it.

use std::collections::BTreeSet;

use fsm_core::json::Value;

/// The request ids a client has asked this server to cancel.
#[derive(Debug, Clone, Default)]
pub struct Cancellations {
    /// Read once `6003` fills the methods below.
    #[allow(dead_code)]
    ids: BTreeSet<String>,
}

impl Cancellations {
    /// Record a `notifications/cancelled` for one request id.
    pub fn cancel(&mut self, _id: &Value) {
        unimplemented!("plan 0012 task 6003")
    }

    /// Whether this request was cancelled.
    pub fn cancelled(&self, _id: &Value) -> bool {
        unimplemented!("plan 0012 task 6003")
    }

    /// Forget a request that has finished.
    pub fn finish(&mut self, _id: &Value) {
        unimplemented!("plan 0012 task 6003")
    }
}
