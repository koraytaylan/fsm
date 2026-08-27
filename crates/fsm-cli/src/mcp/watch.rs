//! The change feed: what the journal did while nobody was asking.
//!
//! This is the loop that runs four times a second forever, so the common
//! case — nothing happened — costs one integer comparison and nothing else:
//! no view rendering, no enabled-event scan, no record walk.
//!
//! It takes no lock and writes nothing. `open_read_only` is safe beside any
//! writer, including this same process in writer mode, and a notification
//! for a change this session just made is correct: a client that subscribed
//! asked to be told, whoever caused it.
//!
//! Plan 0012 task 5902.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use fsm_core::json::Value;

use super::notify::Notifier;
use super::subscribe::Subscriptions;
use crate::store::Store;

/// The feed's cadence, matched to the executor's default so the two
/// processes have one number to explain rather than two.
pub const DEFAULT_INTERVAL_MS: u64 = 250;

/// One session's view of the journal, between polls.
pub struct Feed {
    data_dir: PathBuf,
    watched: Subscriptions,
    out: Notifier,
    /// Everything up to and including this seq has been reported.
    watermark: u64,
    /// How many polls walked records rather than returning on the seq.
    walks: u64,
}

impl Feed {
    /// A feed that reports changes after `from_seq`.
    pub fn new(data_dir: &Path, watched: Subscriptions, out: Notifier, from_seq: u64) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
            watched,
            out,
            watermark: from_seq,
            walks: 0,
        }
    }

    /// How many polls have walked records. The common case must not.
    pub fn walks(&self) -> u64 {
        self.walks
    }

    /// The seq this feed has reported up to.
    pub fn watermark(&self) -> u64 {
        self.watermark
    }

    /// One pass. Returns how many notifications were written.
    ///
    /// Public so a golden can drive the feed deterministically instead of
    /// sleeping.
    pub fn poll_once(&mut self) -> usize {
        let Ok(store) = Store::open_read_only(&self.data_dir) else {
            // A directory that cannot be opened read-only is one a writer is
            // rebuilding; the next poll finds it.
            return 0;
        };
        // The whole common case: one comparison, then out.
        if store.journal.last_seq <= self.watermark {
            return 0;
        }
        self.walks += 1;
        let to_seq = store.journal.last_seq;
        // A copy, so a `resources/subscribe` arriving mid-poll is never
        // blocked behind this walk and these writes.
        let watching = self.watched.snapshot();
        let mut uris: BTreeSet<String> = BTreeSet::new();
        // Decided from the same walk: the feed already has these records in
        // hand, and a second read would be slower and able to disagree with
        // the first.
        let mut listing_changed = false;
        for record in store.records.iter().filter(|r| r.seq > self.watermark) {
            listing_changed |= super::subscribe::changes_the_listing(record.kind);
            // The exhaustive per-kind mapping, not a probe for a field named
            // `instance_id`: composition records name a parent and a child,
            // or a sender and a target, and a probe would silently never tell
            // a subscriber that its child returned.
            for instance_id in fsm_core::record::instances_touched(record) {
                uris.insert(format!("fsm://instance/{instance_id}"));
                uris.insert(format!("fsm://instance/{instance_id}/history"));
            }
            if record.kind == fsm_core::record::RecordKind::MachineDefined
                && let Some(machine_id) = record.body.get("machine_id").and_then(Value::as_str)
            {
                uris.insert(format!("fsm://machine/{machine_id}"));
            }
        }

        // One notification per subscribed URI in the batch, de-duplicated by
        // the set above and ordered by it, so ten records touching one
        // instance produce one notification and a batch is comparable.
        let mut sent = 0;
        for uri in uris.iter().filter(|uri| watching.contains(*uri)) {
            if self
                .out
                .notify("notifications/resources/updated", updated_params(uri))
                .is_err()
            {
                // The watermark stays where it was: the next poll re-derives
                // this same batch. A duplicate notification is harmless; a
                // missed one is not.
                return sent;
            }
            sent += 1;
        }
        // After the batch's updates, so a client that reacts by re-listing
        // sees a listing consistent with what it was just told. Emitted
        // independently of any subscription: this notification is about the
        // *listing*, not about a resource, and a session that negotiated the
        // capability gets it whether or not it watches anything.
        if listing_changed {
            if self
                .out
                .notify(
                    "notifications/resources/list_changed",
                    Value::Obj(std::collections::BTreeMap::new()),
                )
                .is_err()
            {
                return sent;
            }
            sent += 1;
        }
        self.watermark = to_seq;
        sent
    }

    /// Poll until stopped, sleeping between passes.
    pub fn run(&mut self, stop: &std::sync::atomic::AtomicBool, interval_ms: u64) {
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            if self.out.is_broken() {
                return;
            }
            self.poll_once();
            super::notify::sleep_unless_stopped(stop, interval_ms);
        }
    }
}

/// The parameters of a `notifications/resources/updated`.
pub fn updated_params(uri: &str) -> Value {
    Value::Obj(std::collections::BTreeMap::from([(
        "uri".to_string(),
        Value::Str(uri.to_string()),
    )]))
}
