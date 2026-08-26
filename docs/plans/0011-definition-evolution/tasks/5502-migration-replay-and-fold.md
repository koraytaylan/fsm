---
id: migration-replay-and-fold
title: "Migration Replay And Fold"
workstream: "0055"
kind: task
depends_on:
  - instance-migrate-operation
gated: false
touches:
  - crates/fsm-core/src/replay/apply.rs
  - crates/fsm-core/src/replay/verify.rs
  - crates/fsm-store/src/store/reconstruct.rs
  - crates/fsm-core/tests/migrate_replay.rs
status: planned
merged_as: ""
---
# Migration Replay And Fold

One instance's records now legitimately span two definitions, and the only structural change replay needs is to stop resolving an instance's machine once — because the migration record carries both hashes, everything else falls out.

**Steps:**

1. In `crates/fsm-core/src/replay/apply.rs`, track the **current machine per instance** through the fold instead of resolving it once from `instance_created`. On an `instance_migrated` record, switch that instance's machine to `to_machine_id`; every subsequent record for it replays against the new definition.
2. Assert the link during fold: the record's `from_machine_id` must equal the machine the fold currently holds for that instance, or the fold fails. A record claiming to migrate from a machine the instance was not on is corruption, not a reinterpretation.
3. In `crates/fsm-core/src/replay/verify.rs`, verify the migration as a claim: re-run the pure `migrate` with the record's `ts` and check the resulting `state_hash`, `configuration_after`, `dropped_history`, `rescheduled_deadlines`, and the `microsteps` array against the journaled values — in both directions, exactly as plan 0009's `4602` verifies them for an `event_applied`: present means it must match, absent means replay must derive none. A mismatch takes the existing `StateHashMismatch` posture — refuse, no repair — and names the record `seq` and the first differing field.
4. In `crates/fsm-store/src/store/reconstruct.rs`, carry the per-instance machine through instance reconstruction, and make `instance_view` resolve declarations against the instance's **current** machine so `enabled_events`, payload validation, and the rendered configuration all reflect the definition the instance is actually on.
5. Confirm the old machine is never removed from the catalogue. Pre-migration records replay against it and pending effect names re-derive from it; a store that garbage-collected superseded definitions would become unreplayable. Say so in a comment where a future cleanup would otherwise start.
6. Confirm `state_root` computation is unaffected: it hashes instance state hashes, each of which already carries its own format discriminator.

**Tests:**

- `crates/fsm-core/tests/migrate_replay.rs`: a journal whose instance is created, stepped, migrated, and stepped again folds clean and reproduces every `state_hash`.
- Post-migration records are validated against the new machine: a payload legal under the new declarations and illegal under the old one replays successfully.
- Pre-migration records still replay against the old machine, including one whose event no longer exists in the new definition.
- Tamper: altering `configuration_after` in a migration record fails verification naming the seq and field; altering `rescheduled_deadlines` likewise; altering or deleting `microsteps` likewise; altering `from_machine_id` fails the link assertion.
- A migration record whose `from_machine_id` does not match the instance's current machine fails the fold.
- The superseded machine remains resolvable after migration, and a pending effect emitted before the migration still re-derives its name.
- `instance_view` on a migrated instance reports `enabled_events` from the new machine.
- `state_root` after a migration matches a full independent fold.
- `replay_determinism.rs` and `crates/fsm-cli/tests/replay_determinism.rs` pass unchanged for journals without migrations.

- **Done when:** `cargo test -p fsm-core --test migrate_replay` passes every case above including all four tamper cases, an instance's records span two definitions correctly, superseded machines stay resolvable, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
