//! Derived idempotency keys.
//!
//! Every `request_id` the executor issues is a pure function of journaled
//! content, so a restarted executor re-issues the identical key and the store
//! replays it rather than applying it twice.

/// The key for acknowledging one effect.
pub fn ack_rid(effect_id: &str) -> String {
    let _ = effect_id;
    unimplemented!("task 3702")
}

/// The key for the advance event one effect's outcome triggers.
pub fn event_rid(effect_id: &str, event: &str) -> String {
    let _ = (effect_id, event);
    unimplemented!("task 3702")
}

/// The key for one observation of one due deadline.
pub fn poll_rid(instance_id: &str, deadline: &str, due_ms: i64) -> String {
    let _ = (instance_id, deadline, due_ms);
    unimplemented!("task 3702")
}
