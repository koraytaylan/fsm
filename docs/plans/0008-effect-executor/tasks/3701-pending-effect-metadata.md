---
id: pending-effect-metadata
title: "Pending Effect Metadata"
workstream: "0037"
kind: task
depends_on:
  - crate-scaffold-and-skeleton
gated: false
touches:
  - crates/fsm-execute/src/effect.rs
  - crates/fsm-execute/tests/effect.rs
status: done
merged_as: ""
---
# Pending Effect Metadata

The store surfaces a pending effect as an opaque `{instance_id}/{seq}/{k}` id and nothing else — no name, no args, in no view and no record body — so before anything can be matched to a handler or substituted into an argv it must be re-derived by replaying the one record that emitted it. Every later workstream depends on this function.

**Steps:**

1. Implement `pub struct PendingEffect { instance_id: String, effect_id: String, effect_name: String, args: BTreeMap<String, Val>, emitted_seq: u64, k: u32 }` and `pub fn resolve(store: &Store, effect_id: &str) -> Result<PendingEffect, ExecError>` in `effect.rs`.
2. Parse the id: split from the right into `(instance_id, seq, k)`, then find the record with that `seq` in `store.records` (a public field) and require its body's `instance_id` to match. A malformed id, an out-of-range `seq`, a missing record, or a mismatched instance → `exec/effect_unresolved` naming which of those it was.
3. Fold the prefix: `fsm_core::replay::fold_with(records_up_to(seq - 1), &mut NopSink)` to obtain the `StoreState` the record was applied against, and take the machine from `state.machines[state.instance_machines[instance_id]]`.
4. Re-run the pure entry point that wrote the record, always with the record's own `ts` (never a clock — SPEC pins each record's timestamp to the exact `now_ms` the pure call received, which is what makes this replay exact): `EventApplied` → `step(compiled, tree, pre_instance, event, payload, rec.ts, budget)` → `Outcome::Applied(a).effects`; `InstanceCreated` → `create(compiled, tree, &overrides, rec.ts)` with `overrides` read back from the record body via `fsm_core::replay::parse_ctx_json` against the machine's declared context types; `DeadlineApplied` → `poll_deadline(compiled, tree, pre_instance, rec.ts, budget)` → `transition.effects`. Any other record kind, or a non-`Applied` outcome → `exec/effect_unresolved`.
5. Select the emit whose **`k` equals the id's `k`** — never by vector position — and return its `name` and `args`. Budget every replay with `Budget::new(fsm_core::limits::MAX_EVAL_TICKS)`, exactly as `fsm-store` does when it replays a record for history.

**Tests:**

- Round-trip from a real `Store`: define a machine that emits on entering a state, create an instance, send the event, then `resolve` each id in `effects_pending` — the returned `effect_name` matches the machine's declared effect and `args` match the values the emit's expressions evaluate to (including an int, a decimal, and a string arg, asserted through `ctx_val_string`).
- Creation-time emit: a machine that emits from an entry block on the initial state resolves from its `instance_created` record, with context overrides honoured.
- Deadline-time emit: an effect emitted by a deadline transition resolves from its `deadline_applied` record.
- Multiple emits in one transition: ids `.../k=0` and `.../k=1` resolve to their own names and args, and a `k` no emit produced → `exec/effect_unresolved`.
- Rejected ids: `""`, `"no-slashes"`, `"inst/notanumber/0"`, `"inst/0/0"` (genesis emits nothing, and `seq - 1` must not underflow), an id whose `seq` exceeds `last_seq`, and an id whose instance does not match the record → `exec/effect_unresolved`, never a panic.
- Read-only source: the whole suite runs against a `Store::open_read_only` handle, proving resolution needs no writer and no lock.
- Determinism: resolving the same id twice, and from a fresh read-only open, returns byte-identical name and args.

- **Done when:** `cargo test -p fsm-execute --test effect` passes every row across all three emitting record kinds, resolution works from a read-only store with no lock, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
