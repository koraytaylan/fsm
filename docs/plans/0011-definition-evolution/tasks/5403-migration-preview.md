---
id: migration-preview
title: "Migration Preview"
workstream: "0054"
kind: task
depends_on:
  - migration-carryover-rules
gated: false
touches:
  - crates/fsm-core/src/migrate/preview.rs
  - crates/fsm-core/tests/migrate_preview.rs
status: planned
merged_as: ""
---
# Migration Preview

Nobody should move four hundred live workflows to find out that eight of them are in a state the mapping does not cover; the preview is the honest answer to "what will this do", and it writes nothing.

**Steps:**

1. In `crates/fsm-core/src/migrate/preview.rs`, implement `pub fn preview(from: &CompiledMachine, to: &CompiledMachine, t_to: &Tree, st: &InstanceState, now_ms: i64, budget: &mut Budget) -> MigrationPreview`. It runs **every** one of `5401`'s steps, reaction phase included, and returns what would happen instead of a state. Predicting the reaction is not optional: `migrate` runs it, so a preview that stopped before it would report a configuration the migration never lands on, and the preview/apply agreement this task promises would be false.
2. `MigrationPreview` carries: the configuration **after quiescence**, plus the mapped configuration before the reaction so a reader can see both; the projected context as before/after pairs so a reader sees what changes rather than only the result; every history binding that would be dropped; every deadline that would be rescheduled with its old and new due times; every effect that would be retained; every slot that would be dropped; monitor-invariant warnings; and `refusal: Option<Rejection>` when there would be one.
3. A refusal is **returned, not raised**: the preview always produces a value, because "this one cannot migrate, and here is the code" is exactly the information the caller asked for.
4. Implement `pub fn preview_all(...)` over a set of instance states, grouping by outcome — clean, or refused with a given code and a given state — so an operator sees "412 migrate cleanly, 8 are in `awaiting_countersign` which your map does not cover" instead of discovering the eight one at a time. Group ordering is by descending count, then by code, so the summary is stable and the biggest cohort reads first.
5. Keep both functions pure and free of any `request_id`, so they are safe against a read-only store and can be exposed on a read-only server.
6. Guarantee preview/apply agreement in the code's structure, not by parallel implementations: `preview` and `migrate` call the same mapping, projection, and carry-over functions, and `5601`'s property suite asserts that a preview reporting no refusal is always followed by a successful `migrate` with the identical report.

**Tests:**

- `crates/fsm-core/tests/migrate_preview.rs`: a clean instance previews with the mapped configuration, before/after context pairs, and no refusal.
- An unmapped-leaf instance previews with `refusal` carrying `req/migrate_unmapped` and still reports everything it could determine.
- A settled instance previews with `req/migrate_settled`.
- Deadline rescheduling appears in the preview with both old and new due times, matching what `migrate` produces at the same `now_ms`.
- Dropped history bindings and dropped slots appear in the preview exactly as in the report.
- `preview_all` over a mixed cohort groups correctly, orders by descending count then code, and is byte-stable across two runs.
- **Agreement:** for a randomly generated set of instance states, every preview without a refusal is followed by a `migrate` that succeeds with a byte-identical report **and the same final configuration**, and every preview with a refusal is followed by a `migrate` that fails with the same code.
- A machine whose mapped leaf has a guardless eventless exit previews the configuration **after** the reaction, not the mapped leaf, and the preview's microsteps match what `migrate` produces.
- Purity: neither function takes a `request_id` nor mutates its inputs.

- **Done when:** `cargo test -p fsm-core --test migrate_preview` passes every case above including preview/apply agreement, both functions are pure and safe on a read-only store, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
