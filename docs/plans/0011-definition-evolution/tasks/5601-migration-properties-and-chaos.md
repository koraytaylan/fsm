---
id: migration-properties-and-chaos
title: "Migration Properties And Chaos"
workstream: "0056"
kind: task
depends_on:
  - migration-replay-and-fold
  - bulk-migration-command
gated: false
touches:
  - crates/fsm-core/tests/migrate_props.rs
  - crates/fsm-cli/tests/migration_chaos.rs
status: planned
merged_as: ""
---
# Migration Properties And Chaos

Migration is judged by one property above all others: an instance that migrates must behave exactly like an instance that started life in the state it landed in — and that is a property, not an example.

**Steps:**

1. Create `crates/fsm-core/tests/migrate_props.rs` using the existing `proputil.rs` and `enumerate_small/` machinery. Generate pairs by taking an enumerated small machine and applying one random structural edit — rename a state, add a state, tighten a guard, add or remove a deadline, add a context variable — then synthesise the mapping that edit implies.
2. Assert the **equivalence property**: for every generated pair and every reachable instance state, migrating and then applying an event produces the same configuration, context, effects, and status as creating a fresh instance directly in the mapped state with the projected context and applying that same event. Where the two legitimately differ — deadline due times, which restart at migration — compare everything except the schedule and assert the schedule separately against the rescheduling rule.
3. Assert **preview/apply agreement**: every preview with no refusal is followed by a `migrate` that succeeds with a byte-identical report; every preview with a refusal is followed by a `migrate` that fails with the same code.
4. Assert **fold reproduction**: a journal containing the migration folds clean and reproduces every `state_hash`, for every generated pair.
5. Assert **status preservation**: a `Running` instance stays `Running`, and no generated pair ever produces a `Completed` or `Cancelled` instance by migration alone.
6. Create `crates/fsm-cli/tests/migration_chaos.rs` for the cohort leg, following `executor_chaos.rs`'s seeded-restart precedent: interrupt a bulk migration at a random instance boundary, restart, and assert exactly one `instance_migrated` per instance, no instance left half-migrated, a clean journal, and a resumable cohort that finishes on the second run.
7. Print the seed and the generated definition pair on failure so a red run is a reproduction rather than a hunt.
8. **CI budget is shared and this plan is not the only claimant.** `ci.yml` sets `timeout-minutes: 45` per job across a three-OS, two-toolchain matrix, and `crash_harness.rs` (1,000 spawns per profile) plus `executor_chaos.rs` (200 iterations) already dominate it. Measure this suite's wall time on the slowest CI platform, and if it adds more than a few minutes, **lower the committed default iteration count and keep the depth behind the env override** — the pattern `FSM_CRASH_ITERS` and `FSM_EXECUTOR_CHAOS_ITERS` already establish. Record the measured time and the chosen default in the commit message. Four new heavy suites each quietly assuming they have room is how a 45-minute ceiling becomes a red build nobody can attribute.

**Tests:**

- The property suite is the test: equivalence, preview/apply agreement, fold reproduction, and status preservation over every generated pair.
- A deliberately introduced bug — for example, carrying old deadline due times instead of recomputing — is caught by the equivalence property; verify during development, then revert.
- The chaos leg asserts exactly one migration record per instance across every interruption point and a cohort that completes on re-run.
- Generated pairs include sequential and parallel machines, machines with history, machines with deadlines, and machines with invoke slots.
- Iteration count is configurable via the env override, and the committed default is the one the wall-time measurement justified — stated in the commit message alongside the measurement.

- **Done when:** `cargo test -p fsm-core --test migrate_props` and `cargo test -p fsm-cli --test migration_chaos` both pass, the equivalence property holds over every generated pair with deadlines compared separately, interrupted cohorts resume to exactly one record per instance, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
