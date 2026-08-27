//! Which resources one session is watching.
//!
//! Subscriptions are **per session**. A second client connecting to a second
//! `fsm serve` process shares no state with the first, which is exactly right
//! for stdio — one process, one client, one watch set — and is the thing a
//! shared transport will have to revisit, because there the sessions are
//! several and the store is one.
//!
//! The set is behind an `Arc<Mutex<..>>` because the change feed reads it
//! from its own thread, and it is capped because an unbounded set is an
//! unbounded per-poll cost: this cap is the only backpressure the design has.
//!
//! Plan 0012 tasks 5702 (registry) and 5901 (rules and the feed's copy).

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// The most URIs one session may watch.
pub const MAX_SUBSCRIPTIONS: usize = 64;

/// The resource URIs one session has subscribed to.
#[derive(Debug, Clone, Default)]
pub struct Subscriptions {
    uris: Arc<Mutex<BTreeSet<String>>>,
}

impl Subscriptions {
    /// Start watching one URI. Returns whether it was newly added.
    ///
    /// Idempotent: subscribing twice succeeds and leaves one entry. The
    /// client's intent is satisfied either way, and an error would only
    /// invite a retry loop.
    pub fn subscribe(&mut self, uri: &str) -> bool {
        self.lock().insert(uri.to_string())
    }

    /// Stop watching one URI. Returns whether it had been watched.
    ///
    /// Idempotent for the same reason.
    pub fn unsubscribe(&mut self, uri: &str) -> bool {
        self.lock().remove(uri)
    }

    /// Whether this session watches the URI.
    pub fn watches(&self, uri: &str) -> bool {
        self.lock().contains(uri)
    }

    /// How many URIs are watched.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is watched.
    ///
    /// The change feed is spawned when this first turns false, so a session
    /// that never subscribes pays nothing for this plan.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// A copy of the watched set, for the feed to work from without holding
    /// the lock across a poll.
    pub fn snapshot(&self) -> BTreeSet<String> {
        self.lock().clone()
    }

    /// Another handle onto the same set, for the feed thread.
    pub fn clone_handle(&self) -> Self {
        Self {
            uris: Arc::clone(&self.uris),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.uris
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Whether a record changes what `resources/list` would return.
///
/// Membership, not movement: a machine being defined, an instance being
/// created, and an instance being **invoked** — because a child joins the
/// listing without a creation record of its own, and a child that appears in
/// `resources/list` without a `list_changed` is a listing a client never
/// re-reads.
///
/// Deliberately not every advancing record. An `event_applied` leaves the
/// listing's membership exactly as it was, and a client that re-listed on
/// every transition would be worse off than one that polled.
pub fn changes_the_listing(kind: fsm_core::record::RecordKind) -> bool {
    use fsm_core::record::RecordKind;
    matches!(
        kind,
        RecordKind::MachineDefined | RecordKind::InstanceCreated | RecordKind::InstanceInvoked
    )
}
