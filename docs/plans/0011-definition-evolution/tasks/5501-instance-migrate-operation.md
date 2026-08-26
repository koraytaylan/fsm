---
id: instance-migrate-operation
title: "Instance Migrate Operation"
workstream: "0055"
kind: task
depends_on:
  - migration-preview
gated: false
touches:
  - crates/fsm-store/src/store/instance/migrate.rs
  - crates/fsm-store/src/store/idempotency.rs
  - crates/fsm-store/src/store/instance/mod.rs
  - crates/fsm-core/src/record.rs
  - crates/fsm-store/tests/instance_migrate.rs
  - crates/fsm-core/src/replay/apply/migrate.rs
  - crates/fsm-core/src/replay/apply/mod.rs
  - crates/fsm-core/src/migrate/apply.rs
  - crates/fsm-store/src/store/instance/mod.rs
  - crates/fsm-store/src/store/idempotency.rs
  - crates/fsm-store/tests/instance_migrate.rs
  - crates/fsm-cli/tests/naive_caller/*.rs
  - docs/SPEC.md
status: done
merged_as: ""
---
# Instance Migrate Operation

One record moves one instance, carrying both machine ids and the full report — because a migration that cannot be re-verified from the journal is a hole in the audit posture, not a feature.

**Steps:**

1. Create `crates/fsm-store/src/store/instance/migrate.rs`, declared in `instance/mod.rs`, implementing `migrate_instance` and `migrate_instance_on(clock, instance_id, to_machine, request_id)` in the established mutator style behind `ensure_writable()`.
2. Add the `instance_migrated` record kind to `crates/fsm-core/src/record.rs` with body `{instance_id, from_machine_id, to_machine_id, configuration_before, configuration_after, dropped_history, rescheduled_deadlines, request_id, state_hash, state_format}` plus the optional `microsteps` array plan 0009 defined — the migration runs a reaction phase, so it records one exactly as `event_applied` does, absent when there were no reaction microsteps. `configuration_after` is the configuration at **quiescence**.
3. Enforce the supersede link: the target machine's `supersedes.machine` **must** equal the instance's current `machine_id`, or refuse `req/migrate_not_superseded`. There is no path that migrates an instance onto a machine which did not declare it was superseding this one — not an operator override, not a force flag.
4. Journal the report fields as claims rather than decoration: `dropped_history` and `rescheduled_deadlines` are recomputed and checked by replay in `5502`, so their canonical ordering from `5402` is load-bearing.
5. Key idempotency on `(request_id, fingerprint over instance_id + to_machine_id)`. A retry replays; the same instance under the same key to a **different** machine is refused, not replayed.
6. Journal a refusal from the pure `migrate` as a `request_rejected` claiming the key, exactly as a rejected send is, so an attempted-and-refused migration is visible in the audit trail rather than invisible.
7. Use the record `ts` as the `now_ms` handed to the pure function, so the deadline rescheduling in a record is reproducible by replay without a clock — the same rule every deadline record already follows.
8. **Extend `record::instances_touched`** with the new kind, returning its `instance_id`. The match plan 0010's `4901` added is exhaustive, so the build fails until this is done — that is the mechanism working. Without it, a migrated instance's history would omit the record that migrated it, and a subscriber would never be told.
9. **Teach duplicate replay about this record kind.** `crates/fsm-store/src/store/idempotency.rs::replay_duplicate` reconstructs a retry's response from the journal with a chain of **kind-specific** branches — and it is `if`/`matches!`, not an exhaustive `match`, so a new kind falls through every arm **silently** rather than failing to compile. Add the `instance_migrated` arm that rebuilds this operation's response. Note the trap before you test it: `replay_duplicate` first consults an in-memory `last_responses` cache, so a same-process retry appears to work with no arm at all; the reconstruction path only runs after a restart, which is exactly the case the executor's resumption and every second client depend on.

**Tests:**

- `crates/fsm-store/tests/instance_migrate.rs`: migrating a clean instance writes one `instance_migrated`, lands the instance on the mapped configuration with the projected context, and commits the post-migration `state_hash`.
- Migrating onto a machine that does not supersede this instance's machine reports `req/migrate_not_superseded` and writes no state change.
- Migrating a settled instance reports `req/migrate_settled` and journals a `request_rejected` claiming the key; the retry replays the refusal.
- An unmapped leaf reports `req/migrate_unmapped` and journals the refusal the same way.
- Idempotency: the same key replays with `duplicate: true`; the same key to a different target machine is refused.
- **Cold-path replay:** drop the `Store`, reopen it, and re-issue the same `request_id` — the reconstruction must produce the same `duplicate: true` response from the journal alone. The warm path is served by an in-memory response cache, so a test that only retries in the same process proves nothing about the case that actually matters.
- The record's `rescheduled_deadlines` matches what the pure preview predicted at the record's `ts`.
- The migrated instance's `instance_history` contains the `instance_migrated` record.
- Migrating onto a machine whose mapped leaf has an eventless exit writes a record carrying `microsteps`, and `configuration_after` is the post-reaction configuration; a non-reacting migration writes no `microsteps` key at all.
- Pending effects survive the migration and an ack against one still resolves afterwards.
- Read-only: `migrate_instance` on a read-only store refuses with `io/write`.
- A subsequent `instance_send` against the migrated instance is validated against the **new** machine's declarations.

- **Done when:** `cargo test -p fsm-store --test instance_migrate` passes every case above, the supersede link is unconditional with no override, refusals are journaled, cold-path replay reconstructs from the journal, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `migrate_instance_on` in the established mutator style, the `instance_migrated` kind with its body shape and its `instances_touched` arm, the journaled refusal path, `fp_migrate` over `(instance, target)`, the cold-path replay arm, and a suite covering the record's contents, the supersede-link refusal, both journaled refusals with their replays, idempotency including the different-target conflict, cold-path replay after a restart, the record agreeing with the preview at the same `ts`, a carried effect that still acks, the reacting and quiet cases, the fold, and the read-only refusal.

**Corrections.** (1) The plan leaves the fold arm to `5502`, but this task's own cold-path replay test reopens a store that holds an `instance_migrated` record — which cannot be folded without one. The arm lands here, including the per-instance machine tracking that makes it correct, and `5502` verifies the journaled claims and adds the properties. (2) `MigrationReport` gained `settled` (the configuration at quiescence) and a shared `rescheduled_value` encoder, so the record writer and the replay checker encode the claim the same way: a claim written one way and checked another is a claim nobody is really checking.
