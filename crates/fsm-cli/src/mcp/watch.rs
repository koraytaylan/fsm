//! The change feed: what the journal did while nobody was asking.
//!
//! A skeleton: plan 0012 tasks 5902 and 5903 fill it.

use fsm_core::json::Value;

/// One pass over the journal, mapping new records to the URIs they touch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    /// The seq this pass observed.
    pub to_seq: u64,
    /// Subscribed URIs whose resource changed.
    pub updated: Vec<String>,
    /// Whether the machine or instance listings changed.
    pub machines_changed: bool,
    pub instances_changed: bool,
}

/// Poll a data directory for what changed since `from_seq`.
pub fn poll(_data_dir: &std::path::Path, _from_seq: u64) -> std::io::Result<Changes> {
    unimplemented!("plan 0012 task 5902")
}

/// The parameters of a `notifications/resources/updated`.
pub fn updated_params(_uri: &str) -> Value {
    unimplemented!("plan 0012 task 5902")
}
