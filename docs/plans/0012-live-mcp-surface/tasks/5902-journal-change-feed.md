---
id: journal-change-feed
title: "Journal Change Feed"
workstream: "0059"
kind: task
depends_on:
  - subscription-registry
gated: false
touches:
  - crates/fsm-cli/src/mcp/watch.rs
  - crates/fsm-cli/tests/mcp_change_feed.rs
status: planned
merged_as: ""
---
# Journal Change Feed

The feed runs on every interval whether or not anything happened, so its common path must be one open and one integer comparison — the same discipline plan 0008 imposed on the executor's watcher, for the same reason.

**Steps:**

1. Create `crates/fsm-cli/src/mcp/watch.rs` with the poll loop: `loop { if stop { break } ; poll() ; sleep_in_slices(interval) }`, `interval` defaulting to **250 ms** to match the executor's default so the two processes have one cadence to explain.
2. `poll()` opens `Store::open_read_only(data_dir)`, reads `journal.last_seq`, and **returns immediately if it is unchanged**. No view rendering, no `enabled_events` scan, no record walk. This is the case that runs four times a second forever.
3. Use `fsm_core::record::instances_touched` — the exhaustive per-kind mapping plan 0010's `4901` added and `history_page` already consumes — rather than probing for a field named `instance_id`. Plan 0010's records carry `parent_instance_id`/`child_instance_id` and `sender_instance_id`/`target_instance_id`, so a field-name probe would silently never notify a subscriber that its child returned. Do **not** write a second rule here: one helper, exhaustively matched where record kinds are defined, is what keeps this feed and `instance_history` from ever disagreeing about which instances a record concerns.
4. When `last_seq` advanced, walk **only the records after the watermark** and map each through `instances_touched`: every id it returns affects `fsm://instance/{id}` and `fsm://instance/{id}/history`, and a `machine_defined` affects `fsm://machine/{id}`.
5. Emit one `notifications/resources/updated` per **subscribed** URI present in the batch, de-duplicated within the batch: ten records touching one instance produce one notification, not ten. Order notifications by URI so a batch is deterministic and can be golden-compared.
6. Advance the watermark to the new `last_seq` only after the notifications are written, so a broken stream mid-batch does not silently skip records — the next poll re-derives the same batch, and a duplicate notification is harmless while a missed one is not.
7. Read the subscription set through `Subscriptions::snapshot` rather than holding the lock across the record walk and the writes, so a `resources/subscribe` arriving mid-poll is never blocked behind I/O.
8. Add a test seam: `pub fn poll_once(&mut self) -> usize` returning the number of notifications emitted, so `6101`'s golden can drive the feed deterministically instead of sleeping.
9. Take no lock and write nothing: the feed uses `open_read_only` exclusively and is safe beside any writer, including this same process in writer mode. A notification for a change this session just made is correct — a client that subscribed asked to be told, regardless of who caused it.

**Tests:**

- `crates/fsm-cli/tests/mcp_change_feed.rs`: with a subscription to an instance, a write that advances it produces exactly one `notifications/resources/updated` for that URI on the next `poll_once`.
- An unchanged journal makes `poll_once` return 0 and open no view — assert no notification and, via a counter, no record walk.
- A batch of ten records touching one instance produces one notification for `fsm://instance/{id}` and one for its history URI, not twenty.
- A record touching an **unsubscribed** instance produces no notification.
- Notifications within a batch are ordered by URI and are byte-deterministic across two runs.
- The watermark prevents re-notification: a second `poll_once` with no new records emits nothing.
- A write error mid-batch leaves the watermark unadvanced, and the next poll re-derives the same batch.
- The feed coexists with a writer: a test holding a writable `Store` while the feed polls produces correct notifications and no lock error.
- `machine_defined` notifies a subscribed `fsm://machine/{id}`.
- **Composition records notify both sides:** a subscriber on a parent is notified by `instance_invoked` and `invocation_returned`; a subscriber on a child is notified by the same records; a subscriber on a signal's target is notified by `signal_delivered`. Assert each, because these are the records a field-name probe would silently miss.
- The feed and `instance_history` agree: for every record kind, the set of instances the feed notifies equals the set whose history contains that record. Assert this over a store exercising composition, since two independent rules drifting apart is exactly what sharing the helper prevents.

- **Done when:** `cargo test -p fsm-cli --test mcp_change_feed` passes every case above, the unchanged-journal path does no work beyond one open and one comparison, the feed never takes a lock or writes, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
