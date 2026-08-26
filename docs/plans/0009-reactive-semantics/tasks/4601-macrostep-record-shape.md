---
id: macrostep-record-shape
title: "Macrostep Record Shape"
workstream: "0046"
kind: task
depends_on:
  - internal-queue-semantics
gated: false
touches:
  - crates/fsm-core/src/record.rs
  - crates/fsm-store/src/store/instance/send.rs
  - crates/fsm-store/src/store/instance/poll.rs
  - crates/fsm-store/src/store/instance/create.rs
  - crates/fsm-store/src/store/reconstruct.rs
  - crates/fsm-store/tests/macrostep_records.rs
status: planned
merged_as: ""
---
# Macrostep Record Shape

One optional key carries the whole reaction, and its **absence** on a non-reactive machine is the compatibility anchor of the plan: emitting `"microsteps": []` instead would change every record hash in every store that exists.

**Steps:**

1. In `crates/fsm-core/src/record.rs`, add the optional `microsteps` body key to `event_applied`, `deadline_applied`, and `instance_created`, shaped as architecture §0046 specifies: an array of `{index, trigger, event?, source_state, transition_idx, exited, entered}` with `index` starting at **1**.
2. Keep the existing `exited`, `entered`, `source_state`, `transition_idx`, and `deadline_idx` fields describing the **trigger microstep only**. They are checked by fold; redefining them as unions would silently change what every existing record asserts.
3. Emit the key **only when the array is non-empty**. Write this as a single guarded insert with a comment naming the consequence, because it is one line and it is the whole compatibility story.
4. `trigger` is `"eventless"` or `"internal"`; an `"internal"` entry additionally carries `event`. There is no `"event"` trigger value — index 0 is not in the array.
5. In `crates/fsm-store/src/store/instance/{send,poll,create}.rs`, construct macrostep budgets as `Budget::new(MACROSTEP_EVAL_TICKS)` instead of `Budget::new(MAX_EVAL_TICKS)`, and write the reaction microsteps from the `Applied` into the record body. Leave the enabled-event scan on the standard `MAX_EVAL_TICKS` budget — a scan selects, it never applies a pipeline.
6. In `crates/fsm-store/src/store/reconstruct.rs`, carry the microsteps through history reconstruction so `instance_history` can render them, without changing the shape of any existing rendered field.
7. Confirm `state_hash` still commits the state after the **whole** macrostep and that `fsm.state/2` is untouched — no new field, no new format version, no snapshot format change.

**Tests:**

- `crates/fsm-store/tests/macrostep_records.rs`: a non-reactive machine's `event_applied` body has **no** `microsteps` key, and its canonical bytes and record hash equal the values the pre-change build produced for the same inputs (committed as a fixture).
- A reactive machine's `event_applied` body carries `microsteps` with `index` 1..=N, correct triggers, and per-microstep `exited`/`entered`.
- The record's top-level `exited`/`entered`/`source_state` describe the trigger transition only, not the union across microsteps.
- An `instance_created` for a machine whose initial state has an eventless exit carries microsteps — creation runs a macrostep like everything else.
- A `deadline_applied` whose transition cascades carries microsteps, and `deadline_idx` still names the selected deadline.
- Payload bounds: a macrostep with 64 microsteps produces a record under `MAX_PAYLOAD_BYTES`; if it cannot, the microstep entry shape is too fat and must be trimmed rather than the ceiling raised.
- `state_hash` for a reactive macrostep equals a hand-computed `fsm.state/2` hash of the final configuration, proving no queue residue reached the state.
- The genesis `limits` block is byte-identical to before this plan — no `max_microsteps`, no `max_raises`.

- **Done when:** `cargo test -p fsm-store --test macrostep_records` proves both the absent-key and present-key shapes, a non-reactive machine's record bytes are provably unchanged, the genesis limits block has not moved, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
