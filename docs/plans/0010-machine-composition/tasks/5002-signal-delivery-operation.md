---
id: signal-delivery-operation
title: "Signal Delivery Operation"
workstream: "0050"
kind: task
depends_on:
  - signal-block-action
  - state-format-v3-migration
gated: false
touches:
  - crates/fsm-store/src/store/instance/signal.rs
  - crates/fsm-store/src/store/idempotency.rs
  - crates/fsm-store/src/store/instance/mod.rs
  - crates/fsm-core/src/record.rs
  - crates/fsm-store/tests/signal_delivery.rs
status: planned
merged_as: ""
---
# Signal Delivery Operation

Delivery is the one place two instances touch, so one record names both of them — and a delivery that fails is journaled as attempted rather than dropped, because a sender's audit trail must show what it tried.

**Steps:**

1. Create `crates/fsm-store/src/store/instance/signal.rs`, declared in `instance/mod.rs`, implementing `signal_deliver_on(clock, sender_id, signal_id, request_id)` in the established mutator style behind `ensure_writable()`.
2. Add the `signal_delivered` record kind to `crates/fsm-core/src/record.rs` with body `{sender_instance_id, signal_id, target_instance_id, event, payload, outcome, request_id, sender_state_hash, target_state_hash, state_format}`. Both hashes are present when the target advanced and `target_state_hash` is absent when it did not.
3. Apply the event to the target as an ordinary macrostep, with the target's own machine validating the event name and payload. This is the run-time half of the typing `5001` deliberately deferred: an unknown event is `req/event_unknown` **against the target**, a bad field is `req/field_type`, and each is recorded as the delivery's `outcome`.
4. Journal every terminal outcome rather than losing it: `"applied"`, `"rejected"` with the target's code, `"target_missing"`, `"target_settled"`, and `"ignored"` when the target's `on_unhandled` is `ignore`. The sender's `signals_pending` entry is cleared in **every** case — a signal is fire-and-forget by design, and a sender that needs an answer models the target signalling back.
5. Leave the sender's logical state otherwise untouched: delivery is not a transition of the sender, and nothing about it may advance the sender's configuration.
6. Refuse self-delivery — a `to` naming the sender — with `req/signal_target`, because it is always a modelling mistake and `raise` is the construct the author wanted.
7. Key idempotency on `(request_id, fingerprint over sender_id + signal_id)` so a retry after a lost response replays the original outcome exactly, including a rejection.
8. **Teach duplicate replay about this record kind.** `crates/fsm-store/src/store/idempotency.rs::replay_duplicate` reconstructs a retry's response from the journal with a chain of **kind-specific** branches — and it is `if`/`matches!`, not an exhaustive `match`, so a new kind falls through every arm **silently** rather than failing to compile. Add the `signal_delivered` arm that rebuilds this operation's response. Note the trap before you test it: `replay_duplicate` first consults an in-memory `last_responses` cache, so a same-process retry appears to work with no arm at all; the reconstruction path only runs after a restart, which is exactly the case the executor's resumption and every second client depend on.

**Tests:**

- `crates/fsm-store/tests/signal_delivery.rs`: delivering a pending signal writes one `signal_delivered` naming both instances, advances the target, and clears the sender's pending entry.
- An event the target does not declare records `outcome: "rejected"` with `req/event_unknown` and still clears the sender's entry.
- A payload field of the wrong type records `outcome: "rejected"` with `req/field_type`.
- A target that does not exist records `"target_missing"`; a completed or cancelled target records `"target_settled"`.
- A target whose `on_unhandled` is `ignore` and which has no matching transition records `"ignored"`.
- A target whose handling transition cascades produces one record carrying the target's reaction microsteps.
- Self-delivery reports `req/signal_target` and journals nothing.
- The sender's configuration and context are unchanged by every outcome above.
- Idempotency: the same key replays the same outcome, including a rejection; different content under the same key is refused.
- **Cold-path replay:** drop the `Store`, reopen it, and re-issue the same `request_id` — the reconstruction must produce the same `duplicate: true` response from the journal alone. The warm path is served by an in-memory response cache, so a test that only retries in the same process proves nothing about the case that actually matters.
- Read-only: `signal_deliver` on a read-only store refuses with `io/write`.

- **Done when:** `cargo test -p fsm-store --test signal_delivery` passes every outcome above with the sender's entry always cleared, its state never advanced, and cold-path replay reconstructing from the journal, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
