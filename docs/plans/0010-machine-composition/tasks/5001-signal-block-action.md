---
id: signal-block-action
title: "Signal Block Action"
workstream: "0050"
kind: task
depends_on:
  - invocation-outbox-and-state-format
gated: false
touches:
  - crates/fsm-core/src/spec/parse/states.rs
  - crates/fsm-core/src/spec/parse/transitions.rs
  - crates/fsm-core/src/step/block.rs
  - crates/fsm-core/src/machine.rs
  - crates/fsm-core/src/limits.rs
  - crates/fsm-core/tests/signal_actions.rs
status: planned
merged_as: ""
---
# Signal Block Action

A signal reaches exactly one instance, named by an expression evaluated at emit time — because a query-targeted delivery would match a different set on replay, and a store that is not a function of its journal is not this store.

**Steps:**

1. Add `pub signals: Vec<SignalSpec>` to `Block` and `pub struct SignalSpec { pub to: String, pub event: String, pub with: Vec<(String, String)> }` in the spec types, where `to` is `expr/1` source of type `str`.
2. In `crates/fsm-core/src/spec/parse/states.rs`, add `"signal"` to `parse_block`'s allow-list; in `crates/fsm-core/src/spec/parse/transitions.rs`, forward the key into the synthetic block object at both `check_keys` sites, for transition and deadline blocks alike.
3. Type-check `to` as `str` at compile time (`def/assign_type` on anything else). **Do not** type-check `event` or `with` at compile time: the target machine is a run-time value, so its declarations are unknown here. Write that in a comment naming the alternative that was rejected — declaring the target machine statically — and why: a signal exists to reach an instance the sender learned about at run time.
4. Add `pub const MAX_SIGNALS_PER_BLOCK: usize = 4;` to `crates/fsm-core/src/limits.rs`, enforced as `def/limit_signals`, and documented as absent from the genesis limits block like its siblings.
5. **Populate** the `signals` field `4802` already added to `InstanceState` and to the `fsm.state/3` payload — do **not** change the format. Signal ids are derived exactly as effect ids are, `{instance_id}/{seq}/{k}`; reusing that shape is deliberate, because an operator already knows how to read one. If this step finds itself editing the hash payload or the format string, `4802` was landed incomplete and the fix belongs there.
6. In `crates/fsm-core/src/step/block.rs`, evaluate a block's signals under the same snapshot semantics as `do`, `emit`, and `raise`: RHS against the ctx the previous block left, document order, and **a discarded block's signals do not enqueue**.
7. Number signals under their own `k` sequence, independent of the effect `k`, so an operator reading a trace never has to wonder which outbox a `k` belongs to.

**Tests:**

- `crates/fsm-core/tests/signal_actions.rs`: a signal in an entry block adds one entry to `signals` with the evaluated target, event name, and payload.
- `to` of a non-`str` type reports `def/assign_type`; a `to` reading `ctx.counterparty` of type `str` is accepted.
- An `event` name unknown to *this* machine is accepted at compile time — the check belongs to delivery — and the test asserts no finding is reported, so the two halves do not double-report.
- Five signals in one block report `def/limit_signals`; four are accepted.
- Snapshot semantics: a transition block sets `ctx.target` and the signal's `to` reads it, yielding the previous block's value.
- A discarded block's signals do not enqueue while its computed values still appear in the trace.
- Signal `k` numbering is independent of effect `k` in a block that does both.
- `signals` participates in the `fsm.state/3` hash: two states differing only in a pending signal hash differently.
- **The format did not move:** the canonical v3 bytes for a state with no invocations and no signals are byte-identical to the golden `4802` committed, and the `fsm.state/3` format string is unchanged.
- Identity: a machine with no `signal` serializes without the key and keeps its committed `machine_id`.

- **Done when:** `cargo test -p fsm-core --test signal_actions` passes every case above including the deliberate absence of compile-time payload checking, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
