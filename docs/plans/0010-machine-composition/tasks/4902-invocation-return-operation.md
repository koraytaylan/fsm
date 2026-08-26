---
id: invocation-return-operation
title: "Invocation Return Operation"
workstream: "0049"
kind: task
depends_on:
  - invoke-child-operation
gated: false
touches:
  - crates/fsm-store/src/store/instance/invoke.rs
  - crates/fsm-store/src/store/idempotency.rs
  - crates/fsm-core/src/record.rs
  - crates/fsm-store/tests/invocation_return.rs
status: planned
merged_as: ""
---
# Invocation Return Operation

Returning is where a child's result becomes the parent's event, and it is a separate journaled operation for the same reason an effect ack is: a state change caused by something outside the instance must be a record somebody can point at, not a side effect of reading.

**Steps:**

1. Implement `invocation_return_on(clock, parent_id, slot, request_id)` in `crates/fsm-store/src/store/instance/invoke.rs`. It is legal only when the slot is `Running` **and** the child's status is `Completed` or `Cancelled`; anything else is `req/invoke_slot_state` with a hint naming the child's actual status.
2. Add the `invocation_returned` record kind to `crates/fsm-core/src/record.rs` with body `{parent_instance_id, slot, child_instance_id, outcome, payload, request_id, state_hash, state_format}`, where `outcome` is `"completed"` or `"cancelled"`.
3. Build `payload` from the slot's `returns` projection read out of the child's **final** context. For a `cancelled` child skip the projection and use `{}`; a parent that must distinguish models a declared field for it. Keep `outcome` **out of** the event payload — injecting an engine-chosen field into a typed payload would break the shape the child's declarations promised.
4. Deliver `$done.invoke.<slot>` into the parent as the trigger of an ordinary macrostep, exactly as `poll_instance_deadline_on` delivers a due deadline, so the parent's whole reaction — every microstep — seals in this one record with its `microsteps` array.
5. Move the slot `Running → Returned`. Do **not** remove it: the parent may sit in the invoking state and read the result through its transition, and the slot is removed when the state is exited, by `4802`'s rule.
6. Handle the unhandled case without inventing an error: if the parent has no transition on the event, plan 0009 discards it, the record still commits, and the slot is still `Returned`. `5103` reports the smell.
7. Derive nothing from the clock beyond the record `ts` the shell already stamps, and keep the request fingerprint over `(parent_id, slot)` so a retry after a lost response replays exactly.
8. **Teach duplicate replay about this record kind.** `crates/fsm-store/src/store/idempotency.rs::replay_duplicate` reconstructs a retry's response from the journal with a chain of **kind-specific** branches — and it is `if`/`matches!`, not an exhaustive `match`, so a new kind falls through every arm **silently** rather than failing to compile. Add the `invocation_returned` arm that rebuilds this operation's response. Note the trap before you test it: `replay_duplicate` first consults an in-memory `last_responses` cache, so a same-process retry appears to work with no arm at all; the reconstruction path only runs after a restart, which is exactly the case the executor's resumption and every second client depend on.

**Tests:**

- `crates/fsm-store/tests/invocation_return.rs`: returning a `Running` slot whose child completed writes one `invocation_returned`, advances the parent, and moves the slot to `Returned`.
- The delivered payload matches the `returns` projection of the child's final context, at the child's declared types and scales.
- A cancelled child returns `outcome: "cancelled"` with an empty payload, and the parent's transition on the event still fires.
- Returning against a child that is still `running` reports `req/invoke_slot_state` naming the child's status.
- Returning a `Pending` or already-`Returned` slot reports `req/invoke_slot_state`.
- Idempotency: the same `request_id` replays with `duplicate: true`; different content under the same key is refused.
- **Cold-path replay:** drop the `Store`, reopen it, and re-issue the same `request_id` — the reconstruction must produce the same `duplicate: true` response from the journal alone. The warm path is served by an in-memory response cache, so a test that only retries in the same process proves nothing about the case that actually matters.
- A parent whose handling transition cascades produces one record carrying both the trigger and its reaction microsteps.
- A parent with no handling transition still commits the record, sets `Returned`, and records `internal_unhandled` in the trace.
- The parent's `state_hash` commits the post-macrostep state and folds identically on replay.

- **Done when:** `cargo test -p fsm-store --test invocation_return` passes every case above including both outcomes, the unhandled path, and cold-path replay, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
