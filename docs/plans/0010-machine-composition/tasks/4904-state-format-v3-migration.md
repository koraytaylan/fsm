---
id: state-format-v3-migration
title: "State Format V3 Migration"
workstream: "0049"
kind: task
depends_on:
  - invocation-return-operation
gated: false
touches:
  - crates/fsm-store/src/store/reconstruct.rs
  - crates/fsm-store/src/journal_io/init.rs
  - crates/fsm-store/src/snapshot.rs
  - crates/fsm-core/src/replay/apply.rs
  - crates/fsm-core/src/replay/verify.rs
  - crates/fsm-store/tests/state_v3_migration.rs
status: planned
merged_as: ""
---
# State Format V3 Migration

`invocations` joins the persisted state, so the state hash changes for every instance — including instances that will never invoke anything — and the only thing standing between that and an unreadable store is the per-record discriminator the format already carries.

**Steps:**

1. Bump the on-disk store `VERSION` to `9` in `crates/fsm-store/src/journal_io/init.rs`, and add `fsm.state/3` to the format table. Follow the existing rules exactly: opening a `VERSION` 1–8 directory folds the complete journal using each record's own `state_format` discriminator and stamps `9` on success; a failed fold refuses and leaves `VERSION` unchanged; interior records are **never** rewritten.
2. In `crates/fsm-core/src/replay/verify.rs`, verify a record carrying `state_format: "fsm.state/2"` under the v2 hash function and one carrying `"fsm.state/3"` under v3. An absent field still denotes the historical `fsm.state/1` per the existing rule. This is the whole migration: no rewriting, no guessing, no heuristics on record age.
3. In `crates/fsm-core/src/replay/apply.rs`, reconstruct `invocations` from `instance_invoked` and `invocation_returned` records and `signals` from the emitting records and `signal_delivered`, and confirm a journal containing none of them folds to two empty maps. Both fields are in the v3 payload from `4802`, so both must be reconstructed here even though `5002` is what first writes a `signal_delivered`.
4. In `crates/fsm-store/src/store/reconstruct.rs`, carry `invocations` **and** `signals` through instance reconstruction and through `state_root` computation. `fsm.state-root/3` gains no new version — the root hashes instance state hashes, and those already carry their own discriminator.
5. In `crates/fsm-store/src/snapshot.rs`, bump the snapshot cache to `fsm.snapshot/5` carrying both new fields. Snapshots are **disposable caches**: a v4 snapshot found under a v9 store is ignored and re-derived, never migrated, which is what the existing migration rule already says to do with them.
6. **Teach the snapshot's known-instance derivation about invoked children.** The helper near the bottom of `snapshot.rs` that returns `(machines, instances)` collects instance ids from `RecordKind::InstanceCreated` **only**, behind a `_ => {}` catch-all — so it compiles fine and silently omits every child, because `4901` derives a child from `instance_invoked` rather than writing a creation record. Add the `instance_invoked` arm returning its `child_instance_id`. A child missing from that set is a child a snapshot does not know exists, and `snapshot_dedup_binding.rs` is where that surfaces.
7. **Verify this plan's three records as claims, not decoration.** In `crates/fsm-core/src/replay/verify.rs`, re-derive and check the extra values each one journals beyond the parent's own `state_hash`: `instance_invoked`'s `child_state_hash` and `overrides`; `invocation_returned`'s `payload` and `outcome`; and `signal_delivered`'s `outcome` and `target_state_hash`. A journaled value nothing recomputes is a value a tamper can change freely, and the whole point of committing them was that a reader can check them. A mismatch takes the existing `StateHashMismatch` posture — refuse, no repair — naming the record `seq` and the first differing field.
8. State the consequence plainly in the module doc: an instance written before this plan keeps its v2 records and its v2 hashes forever, and only records written after it carry v3. There is no moment at which an old hash is recomputed under a new format.

**Tests:**

- `crates/fsm-store/tests/state_v3_migration.rs`: a committed `VERSION` 8 fixture store opens, folds clean, and is stamped `9`; its pre-existing record hashes are byte-identical afterwards.
- A journal mixing v2 and v3 `state_format` records verifies, each under its own hash function.
- A `VERSION` 8 store whose fold fails is refused and its `VERSION` file is unchanged.
- A v4 snapshot beside a v9 journal is ignored and the state is re-derived; the resulting `state_root` matches a full fold.
- A store containing an invoked child snapshots and reloads with the **child** present: its id appears in the snapshot's known-instance set, and `snapshot_dedup_binding.rs` passes against a composed store.
- A store with no invocations and no signals produces v3 records whose `invocations` and `signals` maps are both empty, and those hashes differ from the v2 hashes of the same logical state — assert the difference, because it is the reason this task exists.
- The v3 canonical bytes for an empty state match the golden `4802` committed, proving migration and identity agree on one payload.
- Tamper: altering `child_state_hash` in an `instance_invoked` record fails verification naming the seq and field; altering `overrides` likewise.
- Tamper: altering `invocation_returned`'s `payload` or `outcome` fails verification; altering `signal_delivered`'s `outcome` or `target_state_hash` fails verification.
- `legacy_snapshot_migration.rs` and `snapshot_v4_golden.rs` pass unchanged for the versions they cover.
- `crates/fsm-cli/tests/replay_determinism.rs` and `recovery_classification.rs` pass unchanged.
- An unknown `VERSION` value is still `store/version_mismatch` and is never reinterpreted.

- **Done when:** `cargo test -p fsm-store --test state_v3_migration` passes every case above, a committed v8 fixture migrates without rewriting a byte of history, mixed-format journals verify, every value this plan's three records journal is re-derived and checked, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
