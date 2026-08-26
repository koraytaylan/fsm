---
id: pure-migrate-function
title: "Pure Migrate Function"
workstream: "0054"
kind: task
depends_on:
  - migration-admission-checks
gated: false
touches:
  - crates/fsm-core/src/migrate/apply.rs
  - crates/fsm-core/tests/migrate_apply.rs
  - crates/fsm-core/src/migrate/carryover.rs
  - crates/fsm-core/src/step/mod.rs
  - crates/fsm-core/src/step/create.rs
  - crates/fsm-core/tests/migrate_apply.rs
status: done
merged_as: ""
---
# Pure Migrate Function

Migration is where a workflow engine is most tempted to guess, so the pure function is written as a sequence of six steps in which every uncovered case refuses with a named code and nothing reasonable-looking is ever improvised.

**Steps:**

1. In `crates/fsm-core/src/migrate/apply.rs`, implement `pub fn migrate(from: &CompiledMachine, to: &CompiledMachine, t_to: &Tree, st: &InstanceState, now_ms: i64, budget: &mut Budget) -> Result<Migrated, Rejection>` returning `pub struct Migrated { pub state: InstanceState, pub report: MigrationReport }`.
2. **Step one — gate.** Refuse a `Completed` or `Cancelled` instance with `req/migrate_settled`. There is nothing to save, and migrating a finished workflow would change what it did.
3. **Step two — map the configuration.** Look up every active leaf in `states`: the single leaf for a sequential instance, each region's leaf for a parallel one. A leaf with no mapping entry refuses the **whole** migration with `req/migrate_unmapped`, naming the leaf and, when the machine is parallel, its region. Partial migration is never performed and no leaf is ever guessed.
4. **Step three — project the context.** Evaluate each `context` expression against the **old** context under the supplied budget. New variables absent from the map take their declared `init`; old variables nobody references are dropped. An evaluation error refuses with `run/action_error` whose block name is `migration`, reusing the existing block-naming vocabulary rather than inventing one.
5. **Step four — carry over** history, deadlines, pending effects, pending signals, and invocation slots by calling into the `carryover` module `5302` stubbed. At this task's stage that call is a seam that carries nothing and reschedules nothing; `5402` fills it, and the seven-step order must not be restructured to accommodate it.
6. **Step five — evaluate the new machine's invariants** on the migrated context and configuration. An enforce failure refuses atomically with `run/invariant`; monitor failures land in the report and do not block. Migrating an instance into a state its own definition calls invalid is precisely what this step prevents.
7. **Step six — run the reaction phase to quiescence.** Hand the mapped state to plan 0009's macrostep driver as a trigger and let eventless transitions, raised events, and `$done.*` events run exactly as they do after a `create`. A migrated instance sitting on a leaf whose new definition has an eventless exit would be parked in a state its own machine says it should have left; `create` and `poll_deadline` already run macrosteps for that reason, and migration is the third case. A rejection in any reaction microstep rejects the whole migration atomically.
8. **Step seven — return.** Status stays `Running` unless the reaction reached a terminal leaf, in which case it is `Completed`. Do not touch `seq`: that is the store's business, and a pure function that invented one would be lying about ordering.
9. Make every refusal atomic: on any `Err`, the caller's `InstanceState` is untouched and no partial state escapes.

**Tests:**

- `crates/fsm-core/tests/migrate_apply.rs`: a sequential instance whose leaf is mapped migrates, landing on the mapped leaf with the projected context.
- A parallel instance with every region's leaf mapped migrates; one unmapped region refuses `req/migrate_unmapped` naming that region, and the input state is unchanged.
- A `Completed` and a `Cancelled` instance each refuse `req/migrate_settled`.
- Context projection: a mapped variable takes the expression's value; an unmapped new variable takes its `init`; an old variable nobody references is absent from the result.
- A projection expression that overflows refuses `run/action_error` with block `migration` and cause `run/overflow`.
- An enforce invariant failing on the migrated state refuses `run/invariant` listing the failing invariants; a monitor failure appears in the report and the migration succeeds.
- A machine whose mapped leaf has a guardless eventless exit migrates and **advances**, with the reaction recorded in the `Migrated` report's microsteps.
- A reaction that hits `run/microstep_limit` rejects the whole migration, leaving the input state untouched.
- Status is `Running` after a successful migration into a non-terminal state; a migration whose reaction reaches a terminal leaf reports `Completed`, and `seq` is untouched in both cases.
- Determinism: the same inputs and the same `now_ms` produce a byte-identical `Migrated` across two runs.
- Budget: a projection whose expressions exhaust the supplied budget refuses rather than looping.

- **Done when:** `cargo test -p fsm-core --test migrate_apply` passes every case above with each refusal atomic and named, the seven-step order matches the architecture and a migrated instance runs its reaction phase, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `migrate` with `Migrated`/`MigrationReport`, the seven steps in the architecture's order, and the suite covering each — the mapped sequential leaf, both parallel cases, both settled statuses, the projection's three outcomes, the overflow that names the `migration` block, both invariant modes, the reaction that advances and the one that completes, the ceiling that rejects atomically, determinism across two runs, and a spent budget refusing rather than looping. `step::parse_init_for`, `step::eval_invariants_for`, and `step::react_from` are the three seams migration needs from the engine; each is a thin re-export of the engine's own answer rather than a second implementation.

**Corrections.** (1) A `to` machine with no `supersedes` block is refused with `req/migrate_not_superseded` here rather than only at the store: the pure function is the one place that can say it without a journal, and a caller that skipped the check would otherwise migrate onto an empty mapping and get `req/migrate_unmapped`, which names the wrong problem. (2) The ceiling test needs a *guarded* eventless cycle — an unguarded one is refused at admission by `def/eventless_cycle`, so the run-time ceiling can only be reached by a definition that admission accepts.
