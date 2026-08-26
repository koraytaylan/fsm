---
id: executor-composition-directives
title: "Executor Composition Directives"
workstream: "0051"
kind: task
depends_on:
  - signal-delivery-operation
gated: false
touches:
  - crates/fsm-execute/src/effect.rs
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/rid.rs
  - crates/fsm-execute/src/error.rs
  - crates/fsm-execute/tests/composition.rs
status: planned
merged_as: ""
---
# Executor Composition Directives

Composition must run unattended or it is a feature only a live session can use — and unlike an effect, none of these three directives needs a subprocess, so they bypass the runner entirely and go straight to the pipeline.

**Steps:**

1. **Teach effect resolution that a child's creation record is `instance_invoked`.** This is the task's highest-value fix and the least obvious. `crates/fsm-execute/src/effect.rs` resolves a pending effect whose id is `{instance}/0/{k}` — a **creation-emitted** effect — by scanning backwards for an `InstanceCreated` record for that instance, and its `instance_of` helper reads `body.get("instance_id")`. A child created by `4901` has **neither**: its creation is an `instance_invoked` record and its id lives in `child_instance_id`. Left alone, every effect a child emits on entry resolves as `exec/effect_unresolved` and never runs — which silently breaks the headline case, because emitting work on entry is exactly what a child machine is for.

   Extend the creation-record search to accept `instance_invoked` (matching on `child_instance_id`) alongside `InstanceCreated`, and keep the existing newest-first rule: the comment there explains that a journal may hold more than one creation for an id and the pending effect belongs to the current one, and that reasoning applies unchanged to a re-invoked slot. Re-derive the child's emitted effects by running `create` against the child machine with the record's `overrides` at the record's `ts`, which is the same replay technique the function already uses.
2. In `crates/fsm-execute/src/watch.rs`, extend `Observation` with `pending_invocations`, `returnable_invocations` (a `Running` slot whose child has settled), and `pending_signals`, all read from `InstanceState`'s public fields through the existing `Store::open_read_only` scan. No new store call and no `instance_view` — the watcher's cost discipline from plan 0008 applies unchanged.
3. In `crates/fsm-execute/src/rid.rs`, add the derived keys: `invoke_rid(parent, slot) -> "exec-inv-{parent}/{slot}"`, `return_rid(parent, slot) -> "exec-ret-{parent}/{slot}"`, `signal_rid(sender, signal_id) -> "exec-sig-{sender}/{signal_id}"`. Every key derives from journaled content, so a restarted executor recomputes it identically.
4. In `crates/fsm-execute/src/sched.rs`, add the three directives to the decision table, each gated on its key being **absent** from `claimed_request_ids` exactly as the existing rules are. A returnable invocation is only directed when the child is genuinely settled — the watcher decides that from the child's status, never from elapsed time.
5. In `crates/fsm-execute/src/service.rs`, route the three directives straight to the pipeline: they take the writer for the tick like any other write and never touch `Runner`. Add a sentence to the module doc saying why — a subprocess is for reaching the world's computers, and these three reach only the journal.
6. Add `exec/invoke` and `exec/signal` to the crate's `ALL_CODES`, raised when a store call for one of these directives fails, with the underlying `ErrorObj` preserved in `details` like `exec/store`.
7. Emit one identifier-only action line per directive — parent, slot, child id, signal id, request id — honouring plan 0008's rule that a tick trace carries no path, pid, or duration, so the golden stays byte-comparable.
8. Enforce ordering within a tick: invoke before return before signal, so a slot created and settled across two ticks never races itself, and the trace reads in causal order.

**Tests:**

- **A child's creation-emitted effect resolves and runs.** Invoke a child whose initial state emits an effect, and assert the executor resolves the effect's name and args from the `instance_invoked` record and runs its handler — this is the case that fails silently without step 1, and it is the plan's headline capability end to end.
- An effect emitted by a **root** instance's creation still resolves through `InstanceCreated` exactly as before, with the existing executor tests unchanged.
- A slot invoked, returned, and invoked again resolves a creation-emitted effect against the **newest** `instance_invoked` record, preserving the existing newest-first rule.
- `crates/fsm-execute/tests/composition.rs`: a store with one pending invocation produces exactly one `InvokeChild` directive with the derived key; re-presenting the same observation after the key is claimed produces none.
- A `Running` slot whose child is `Completed` produces one `InvocationReturn`; while the child is still running it produces none.
- A pending signal produces one `SignalDeliver` with the derived key.
- Restart equivalence: a fresh scheduler fed the post-invoke observation emits the outstanding return and no second invoke, proving decisions are journal-derived.
- Within-tick ordering is invoke, then return, then signal.
- A depth-2 tree runs from root creation to every child settled across a sequence of ticks with no manual step.
- A store failure on any directive surfaces as `exec/invoke` or `exec/signal` with the inner error in `details`, and the next tick retries.
- Action lines contain no absolute path, pid, temp dir, or duration.
- The runner is never invoked for these directives — assert the child-process count is zero across a composition-only tick.

- **Done when:** `cargo test -p fsm-execute --test composition` passes every case above including restart equivalence and within-tick ordering, a depth-2 tree completes unattended, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
