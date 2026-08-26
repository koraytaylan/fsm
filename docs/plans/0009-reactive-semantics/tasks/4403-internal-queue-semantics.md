---
id: internal-queue-semantics
title: "Internal Queue Semantics"
workstream: "0044"
kind: task
depends_on:
  - raise-block-action
gated: false
touches:
  - crates/fsm-core/src/step/micro.rs
  - crates/fsm-core/tests/internal_queue.rs
status: planned
merged_as: ""
---
# Internal Queue Semantics

The queue is where determinism is won or lost: FIFO, breadth-first, drained from the front, refilled at the back, with an unhandled event discarded rather than rejected — and every one of those is a ruling somebody will otherwise implement the other way.

**Steps:**

1. In `crates/fsm-core/src/step/micro.rs`, replace `4201`'s queue seam with the real drain: when eventless selection yields nothing and `queue` is non-empty, pop the **front** and attempt selection for that event name over the current working configuration, with the raised payload bound as `evt`.
2. Enqueue a committed block's raises at the **back**, in the order `4402` produces them. An event raised while handling another internal event therefore lands behind every event already waiting — breadth-first, which is the only order under which "raised together, delivered together" is true.
3. Implement the discard rule: a popped event that selects no transition is recorded in the macrostep trace as `internal_unhandled` and the loop continues with the next queue entry. It is **not** `run/unhandled`, and `on_unhandled: reject` does not apply to it — that setting governs the trigger microstep only. Write the reason in a comment: rejecting here would have to unwind an already-applied trigger transition, and an engine-generated done event nobody listens for is not a caller error.
4. Bind `evt` for an internal microstep exactly as for an external one — the raised payload object, with the declared field types — so guards and blocks on a handling transition read `evt.field` the way they always have.
5. Add no queue-length limit. `MAX_MICROSTEPS` already bounds the total work; a second constant would be one more thing to explain and could only fire in cases the ceiling already catches. Say so in a comment so it reads as a decision rather than an oversight.
6. Confirm the queue is still a stack-frame local: after `run_to_quiescence` returns, nothing about it survives into `InstanceState`.

**Tests:**

- `crates/fsm-core/tests/internal_queue.rs`: one raise in an entry block is delivered as one reaction microstep with `trigger: Internal(name)` and the payload bound as `evt`.
- FIFO across blocks: a transition whose exit, transition, and entry blocks each raise a distinct event delivers them in exit → transition → entry order.
- Breadth-first: handling event `a` raises `c` while `b` is already queued; delivery order is `a`, `b`, `c`.
- Eventless-before-queue: with both an enabled eventless transition and a non-empty queue, the eventless transition is taken first; the queued event is delivered on a later iteration.
- An internal event with no handling transition is discarded, the macrostep still returns `Applied`, and the trace records `internal_unhandled` for it.
- The same machine with `on_unhandled: "reject"` still discards it — pin this directly, it is the ruling most likely to be "corrected" by a later reader.
- A raise chain of 64 events rejects with `run/microstep_limit` and leaves the caller's state untouched; a chain of 63 completes.
- Payload typing: a guard on the handling transition reading `evt.amount` sees the decimal the raise computed, at the declared scale.
- After a macrostep with a non-empty queue during its run, the sealed `InstanceState` has exactly its six fields and no queue residue; the `fsm.state/2` hash matches a hand-computed value for the final configuration.

- **Done when:** `cargo test -p fsm-core --test internal_queue` passes every case above including the `on_unhandled` ruling and the ceiling, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
