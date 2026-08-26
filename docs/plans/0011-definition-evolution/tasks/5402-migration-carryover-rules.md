---
id: migration-carryover-rules
title: "Migration Carryover Rules"
workstream: "0054"
kind: task
depends_on:
  - pure-migrate-function
gated: false
touches:
  - crates/fsm-core/src/migrate/carryover.rs
  - crates/fsm-core/tests/migrate_carryover.rs
status: planned
merged_as: ""
---
# Migration Carryover Rules

An instance holds five collections besides its status, configuration, and context, and each needs a decision rather than a default: history is dropped when unmapped, deadlines are recomputed, pending effects and pending signals are kept verbatim, and a running invocation refuses.

**Steps:**

1. In `crates/fsm-core/src/migrate/carryover.rs`, implement `pub(crate) fn carry_over(...)` filling `5401`'s seam, returning the carried collections plus the report entries that make every loss visible.
2. **History — remap, drop on miss.** A binding `owner → child` becomes `states[owner] → states[child]` when **both** are mapped; a binding whose owner or child is unmapped is **dropped**, not refused, and every drop is listed in the report. The reason belongs in a comment: a history binding concerns a state the instance is not currently in, so losing it degrades a future re-entry rather than corrupting the present, and refusing a whole migration over one is disproportionate.
3. **Deadlines — recompute, never carry.** Drop every existing schedule, then schedule the **new** machine's deadlines for the mapped configuration from the migration's `now_ms`, by exactly the rule state entry uses. Carrying old due times would keep a promise the new definition never made. Record each old and new due time in the report, because this is the one carry-over rule whose consequence an operator must see: **migration restarts the clock on every timer.**
4. **Pending effects — retain verbatim.** An effect id is `{instance}/{seq}/{k}` and its name re-derives by replaying the emitting record against the machine that emitted it, which is still in the catalogue. Dropping one would strand a handler that is already running against the outside world. Carry the vector unchanged and list it in the report.
5. **Invocation slots — carry or refuse.** A slot carries when the new machine declares the same slot id with the **same** `child_machine_id`; otherwise refuse with `req/migrate_slot`. Unlike a history binding, a `Running` child is a live instance doing work and cannot be dropped. A `Returned` slot whose id is gone **is** dropped with a report entry, since its result was already delivered.
6. **Pending signals — retain verbatim.** A signal names a target instance id and an event the *target's* machine declares, so the migrating instance's own definition cannot invalidate one: neither `states` nor `context` has any bearing on deliverability. Dropping one would silently lose a message the sender's journal says it produced; refusing over one would block an upgrade for an unrelated reason. Carry the map unchanged and list it in the report.
7. **Enumerate the collections from the struct, not from this list.** Before implementing, read `crates/fsm-core/src/machine.rs` and confirm `InstanceState` has exactly the five collections this task rules on. A field added by a later plan with no ruling here has undefined migration behaviour, and a compile-time exhaustiveness check — destructuring `InstanceState` rather than accessing fields — is what makes that impossible to miss. Use one.
8. Populate `MigrationReport` with `dropped_history`, `rescheduled_deadlines` (name, old due, new due), `retained_effects`, `retained_signals`, and `dropped_slots`, each canonically ordered so the report is byte-stable and can be journaled and re-verified.

**Tests:**

- `crates/fsm-core/tests/migrate_carryover.rs`: a history binding whose owner and child are both mapped is remapped; one whose child is unmapped is dropped and appears in the report; one whose owner is unmapped likewise.
- Deadlines: an instance with two active schedules migrates to schedules computed from the new machine's `after` expressions at the migration `now_ms`, and the report lists both old and new due times.
- A state whose new machine declares **no** deadline ends with no schedule, and the report records the old one as dropped.
- A deadline whose new `after` expression overflows refuses through `5401`'s `run/action_error` path rather than producing a negative due time.
- Pending effects survive byte-for-byte, and an ack against a retained effect id still resolves after migration — assert by re-deriving the effect's name against the old machine.
- Invocations: a slot present in both machines with the same child machine id carries with its status; a `Running` slot absent from the new machine refuses `req/migrate_slot`; a `Running` slot whose child machine id differs refuses the same; a `Returned` slot absent from the new machine is dropped with a report entry.
- Pending signals survive byte-for-byte, including one whose target instance no longer exists — deliverability is the target's problem and is decided at delivery, not at migration.
- A signal naming an event the **new** machine does not declare still survives, since the event belongs to the target's machine — assert this directly, because it is the case a reader will assume is a bug.
- `InstanceState` is destructured exhaustively in the carry-over implementation, so adding a field to the struct fails to compile until it gets a ruling — verify by adding a field locally during development and observing the error, then revert.
- The report is byte-stable across two runs with identical inputs.
- An instance with empty history, no deadlines, no effects, no signals, and no slots migrates with an empty report.

- **Done when:** `cargo test -p fsm-core --test migrate_carryover` passes every case above, each of the five rulings behaves exactly as the architecture states, `InstanceState` is destructured exhaustively so a new field cannot ship without a ruling, the report is byte-stable, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
