//! Which sessions are watching which resources.
//!
//! A skeleton: plan 0012 task 5901 fills it. Declared here so each later
//! task stays inside its own file, the way plan 0008's scaffold task
//! established.

use std::collections::BTreeSet;

/// The resource URIs one session has subscribed to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subscriptions {
    uris: BTreeSet<String>,
}

impl Subscriptions {
    /// Start watching one URI. Returns whether it was newly added.
    pub fn subscribe(&mut self, _uri: &str) -> bool {
        unimplemented!("plan 0012 task 5901")
    }

    /// Stop watching one URI. Returns whether it had been watched.
    pub fn unsubscribe(&mut self, _uri: &str) -> bool {
        unimplemented!("plan 0012 task 5901")
    }

    /// Whether this session watches the URI.
    pub fn watches(&self, _uri: &str) -> bool {
        unimplemented!("plan 0012 task 5901")
    }

    /// Every watched URI, in canonical order.
    pub fn watched(&self) -> impl Iterator<Item = &str> {
        self.uris.iter().map(String::as_str)
    }
}
