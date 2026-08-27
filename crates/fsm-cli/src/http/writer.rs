//! One process, many clients, one writer.
//!
//! Single-writer stops being the limitation clients trip over and becomes
//! the serialization point they share: this process holds the lock, and
//! every session's call goes through one mutex.
//!
//! **Why a mutex and not a work queue.** Engine operations are bounded by
//! the evaluation budget and short by construction — a macrostep is
//! sixty-four microsteps at most, and every tool call is one macrostep or a
//! bounded read. A queue would add latency to every call and a second
//! failure mode (a queue that grows, a worker that dies) in exchange for
//! nothing that a short critical section does not already give.
//!
//! **Why reads take it too.** A read that observed a half-applied macrostep
//! would be a worse bug than a slow read, and the reason there is no such
//! state to observe is precisely that the lock is held across the whole
//! call. Anyone tempted to move reads out of the lock is removing the thing
//! that makes them correct.
//!
//! **Why that is affordable.** The two calls whose cost grows with the
//! journal — plan 0014's `journal_verify` and `journal_replay` — read
//! through `Store::open_read_only`, which takes no lock at all. They do not
//! pass through here, and a future change that routed them through this
//! handle would break the argument that makes the mutex fine.
//!
//! Per-session state is not here. Subscriptions, logging levels,
//! cancellations and the outstanding ask belong to a session; this module
//! owns exactly one thing.
//!
//! Plan 0015 task 7201.

use std::sync::Mutex;

use crate::store::Store;

/// The store every session's calls are serialized through.
pub struct SerializedWriter {
    store: Mutex<Option<Store>>,
}

impl SerializedWriter {
    pub fn new(store: Option<Store>) -> Self {
        Self {
            store: Mutex::new(store),
        }
    }

    /// Run one call against the store, with every other caller waiting.
    ///
    /// A poisoned lock hands back the store rather than propagating the
    /// panic: a call that panicked must not make the store unreachable for
    /// every other session. The panic itself was already fatal, or was
    /// isolated at the connection boundary where a stranger's input belongs.
    pub fn with_store<T>(&self, body: impl FnOnce(Option<&mut Store>) -> T) -> T {
        let mut store = self
            .store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        body(store.as_mut())
    }

    /// Whether this server holds a store at all.
    pub fn is_open(&self) -> bool {
        self.store
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }
}
