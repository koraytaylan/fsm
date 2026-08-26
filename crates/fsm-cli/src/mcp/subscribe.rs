//! Which sessions are watching which resources.
//!
//! The registry itself lands with the routing (task 5702), because an arm
//! routed at a function that panics is not routing. What `5901` adds is the
//! background feed that turns a subscription into a notification.

use std::collections::BTreeSet;

/// The resource URIs one session has subscribed to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subscriptions {
    uris: BTreeSet<String>,
}

impl Subscriptions {
    /// Start watching one URI. Returns whether it was newly added.
    pub fn subscribe(&mut self, uri: &str) -> bool {
        self.uris.insert(uri.to_string())
    }

    /// Stop watching one URI. Returns whether it had been watched.
    pub fn unsubscribe(&mut self, uri: &str) -> bool {
        self.uris.remove(uri)
    }

    /// Whether this session watches the URI.
    pub fn watches(&self, uri: &str) -> bool {
        self.uris.contains(uri)
    }

    /// Whether anything is watched at all.
    ///
    /// The change feed is spawned only when this turns true, so a session
    /// that never subscribes pays nothing for this plan.
    pub fn any(&self) -> bool {
        !self.uris.is_empty()
    }

    /// Every watched URI, in canonical order.
    pub fn watched(&self) -> impl Iterator<Item = &str> {
        self.uris.iter().map(String::as_str)
    }
}
