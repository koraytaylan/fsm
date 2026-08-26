---
id: invocation-outbox-and-state-format
title: "Invocation Outbox And State Format"
workstream: "0048"
kind: task
depends_on:
  - invoke-declaration-and-validation
gated: false
touches:
  - crates/fsm-core/src/machine.rs
  - crates/fsm-core/src/hashes.rs
  - crates/fsm-core/src/step/create.rs
  - crates/fsm-core/src/step/transition.rs
  - crates/fsm-core/src/step/invoke.rs
  - crates/fsm-core/src/step/mod.rs
  - crates/fsm-core/src/step/micro.rs
  - crates/fsm-core/src/spec/compile.rs
  - crates/fsm-core/src/replay/apply.rs
  - crates/fsm-store/src/store/instance/{create,send,poll}.rs
  - crates/fsm-core/tests/invocation_outbox.rs
  - crates/fsm-core/tests/oracle/macrostep.rs
status: done
merged_as: ""
---
# Invocation Outbox And State Format

The pure core cannot create an instance, so it does what it already does for effects: it records that one should exist and lets the shell make it so — and the child's id is derived here, which is what makes enactment idempotent without consulting anything.

**Steps:**

1. Add **both** new fields to `InstanceState` in `crates/fsm-core/src/machine.rs`: `pub invocations: BTreeMap<String, Invocation>` with `pub struct Invocation { pub child_machine_id: String, pub child_instance_id: String, pub status: InvokeStatus }` and `pub enum InvokeStatus { Pending, Running, Returned }`, **and** `pub signals: BTreeMap<String, PendingSignal>` with its struct. `signals` stays empty until `5001` populates it, and it lands here because a format version must be defined exactly once — see step 3.
2. Implement the derived child id in `crates/fsm-core/src/hashes.rs`: `child_instance_id = "inst-" || hex(sha256("fsm:child:1" || parent_instance_id || 0x00 || slot))[..24]`, following the domain-separation convention every other hash in this workspace uses. Add it beside the existing hash helpers with a golden vector, because an id scheme that drifts silently orphans every child in every store.
3. Bump the state identity hash to `fsm.state/3` over `{format, machine_id, instance_id, seq, status, configuration, ctx, history, deadlines, pending, invocations, signals}`, `invocations` sorted by slot and `signals` by signal id, **both keys always present including when empty**. Defining v3 completely here is the point of the task: adding `signals` to v3 later, in `5001`, would give one version string two payloads, and every v3 record written in between — including anything `4904`'s migration stamped — would carry a hash no later build could reproduce. Keep the v2 computation reachable for records that declare `state_format: "fsm.state/2"` — `4904` owns migration, but the v2 function must survive this task, not be replaced by it.
4. In `crates/fsm-core/src/step/create.rs` and `crates/fsm-core/src/step/transition.rs`, insert a `Pending` invocation for each slot on every **entered** state, evaluating the slot's `with` projection against the context the entry pipeline produced and carrying the evaluated overrides in the state. Evaluating once, at entry, is what makes the enacted child see the values the pipeline computed rather than whatever the context holds when enactment happens.
5. Remove a slot's entry entirely when its state is **exited**, at whatever status it held. A slot that was `Running` at exit additionally sets a flag the store reads for the cascade in `4903`; the core records the fact and takes no action, because cancelling is I/O.
6. Confirm a state that is entered and exited within one macrostep leaves no invocation behind, falling out of steps 4 and 5 rather than needing a special case — and pin it, because "falls out" is a claim.

**Tests:**

- `crates/fsm-core/tests/invocation_outbox.rs`: entering an invoking state inserts one `Pending` slot carrying the derived child id and the evaluated overrides.
- The derived child id matches a committed golden vector for a fixed `(parent_id, slot)` pair, and two different slots on one parent derive different ids.
- Overrides are evaluated against the post-entry-block context: an entry block sets `ctx.total` and the slot's `with` reads it, yielding the block's value.
- Exiting the state removes the slot; a `Running` slot at exit sets the cascade flag.
- A state entered and exited inside one macrostep leaves `invocations` empty.
- `fsm.state/3` hashing: two states differing only in an invocation slot hash differently; the v2 hash function still produces its committed golden values for a v2 payload.
- **The v3 payload is complete:** the canonical v3 bytes for a state with no invocations and no signals contain **both** the `invocations` and `signals` keys as empty maps. Commit that byte string as a golden, so `5001` cannot change the format while merely populating a field.
- A state whose `signals` map is non-empty hashes differently from one whose is empty, proving the field is in the payload before anything writes to it.
- A machine with no invokes produces an empty `invocations` map, and its **v3** state hash differs from its v2 hash — assert this explicitly, because it is the fact that makes `4904` necessary and a reader will otherwise assume empty means unchanged.

- **Done when:** `cargo test -p fsm-core --test invocation_outbox` passes every case above, the child-id golden vector is committed, the v2 hash function still returns its committed values, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `InstanceState` gains `invocations` and `signals` (74 construction sites across the workspace updated with them), `machine.rs` gains `Invocation`, `InvokeStatus`, `PendingSignal`, and `CancelledChild`, and `step/invoke.rs::settle_invocations` is the one place slots are inserted and removed — called by `apply_selected_transition` and by `create` immediately, not deferred like schedules, so a failing projection is attributed to the microstep that entered the state. `Applied` carries `invocations_after` and `cancelled_children`; the driver threads the map through `working` and accumulates cancellations across microsteps; the store's three writers and replay's three sites now carry the outbox forward, without which a slot would vanish on the next step.

**Corrections.** (1) Step 1 and the architecture put `child_instance_id` in the state; it cannot live there. The pure core never learns an instance id — `InstanceState` has none, by design, because the id is the caller's handle on the state — so `Invocation` carries `{child_machine_id, status, overrides}` and the id is `hashes::child_instance_id(parent, slot)` computed wherever the parent's id is known. Nothing is lost: the v3 payload commits `instance_id` and the slot key, which are the id's only two inputs, so hashing the id as well would commit nothing new. (2) The plan's id formula concatenates `"fsm:child:1"` directly onto the parent id; the implementation puts the `0x0A` after the tag that every other hash in this workspace uses, keeping the domain-separation convention the same sentence claims to follow, and a golden vector — verified against an independent computation of those bytes — freezes it. (3) `compile.rs` was needed as well: an invoke's `with` must be compiled like any other expression, so `ExprSlot::InvokeWith(state, slot index, field)` binds against the entry-block scope. (4) The naive oracle grew its own outbox (a linear spec walk, expressions re-parsed from source) rather than being left to diverge, so the differential covers invocations the moment an enumerated machine declares one. (5) Plan 0009's inertness suite asserted `fsm.state/2` was the only format named anywhere — the proxy it needed for "the reactive plan bumped no format". This plan's bump is deliberate, so the guard is restated around the set this build declares (v1 legacy, v2 writer, v3 defined): an undeclared format literal in any source file still fails it, which is the tooth it always had.
