---
id: raise-block-action
title: "Raise Block Action"
workstream: "0044"
kind: task
depends_on:
  - internal-event-declaration
  - macrostep-driver
gated: false
touches:
  - crates/fsm-core/src/spec/parse/states.rs
  - crates/fsm-core/src/spec/parse/transitions.rs
  - crates/fsm-core/src/spec/mod.rs
  - crates/fsm-core/src/step/block.rs
  - crates/fsm-core/src/limits.rs
  - crates/fsm-core/tests/raise_actions.rs
status: planned
merged_as: ""
---
# Raise Block Action

`raise` is `emit`'s inward-facing twin: the same typed, snapshot-evaluated, document-ordered block action, except the payload lands in the macrostep's own queue instead of the outbox.

**Steps:**

1. Add `pub raises: Vec<RaiseSpec>` to `Block` and `pub struct RaiseSpec { pub event: String, pub with: Vec<(String, String)> }` to `crates/fsm-core/src/spec/mod.rs`. `with` is an ordered vector rather than a map so document order is preserved for the trace and for canonical serialization.
2. In `crates/fsm-core/src/spec/parse/states.rs`, add `"raise"` to `parse_block`'s `check_keys` allow-list and parse the array. Each entry is `{event, with?}`; `with` defaults to an empty map. Validate the field set against the declared event exactly as `emit` validates effect args: every declared field present (`def/shape`), no extras (`def/shape`), and RHS type equal to the declared field type including decimal scale (`def/assign_type`).
3. In `crates/fsm-core/src/spec/parse/transitions.rs`, forward `raise` into the synthetic block object that function already builds for `do` and `emit`, for transition blocks and deadline blocks alike. Add `"raise"` to both `check_keys` call sites.
4. Add `pub const MAX_RAISES_PER_BLOCK: usize = 8;` to `crates/fsm-core/src/limits.rs`, mirroring `MAX_EMITS_PER_BLOCK`, enforced as `def/limit_raises`. Its doc comment must state that it is **not** added to the genesis `limits` block, for the reason `MAX_PAYLOAD_BYTES` gives: that block is hash-verified on fold, so a new key makes every store written by an earlier build unreadable rather than migratable.
5. In `crates/fsm-core/src/step/block.rs`, evaluate a block's raises inside the existing snapshot semantics: all RHS evaluate against the ctx the previous block left, in document order, and the results are appended to the block's output alongside its sets and emits.
6. Enforce the rule that makes discarded blocks honest: a raise in a block the pipeline **discards** does not enqueue. The existing `BlockTrace.discarded` flag marks those blocks and their computed-but-discarded values stay in the trace; the queue must not see them. Only a committed block's raises reach the macrostep.
7. Charge each raise's `with` expressions to `def/limit_eval` admission exactly as emit args are charged. No new budget concept.

**Tests:**

- `crates/fsm-core/tests/raise_actions.rs`: a `raise` in an entry block enqueues one internal event whose payload fields carry the evaluated values, in `with` document order.
- Snapshot semantics: a transition block sets `ctx.x` and raises `with: {v: "ctx.x"}`; the raised payload carries the value the **previous** block left, matching how `emit` and `do` already behave in the same block.
- A raise naming an undeclared event is `def/unknown_event`; a raise missing a declared field is `def/shape`; a raise with an extra field is `def/shape`; a raise whose RHS type or decimal scale differs is `def/assign_type`.
- Nine raises in one block report `def/limit_raises`; eight are accepted.
- A discarded block's raises do **not** enqueue, while its computed values still appear in the trace with `discarded: true`.
- Ordering within a block is document order; ordering across blocks follows the pipeline's exit inner→outer, transition, entry outer→inner sequence — assert the full enqueue sequence for a transition that raises in three blocks.
- Raising an event declared `internal: false` is legal — `internal` restricts the *external send* path, not the raise path — and the test pins that so nobody adds a rule the architecture did not ask for.
- Identity: a machine with no `raise` serializes without the key and keeps its committed `machine_id`.

- **Done when:** `cargo test -p fsm-core --test raise_actions` passes every case above, `MAX_RAISES_PER_BLOCK` is absent from the genesis limits block, every `examples/` machine keeps its `machine_id`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
