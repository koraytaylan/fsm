---
id: latency-regression-guard
title: "Latency Regression Guard"
workstream: "0087"
kind: task
depends_on:
  - widen-committed-gate
gated: false
touches:
  - crates/fsm-store/tests/append_guard.rs
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Latency Regression Guard

The workspace has exactly one performance measurement, it is `#[ignore]`d and run by hand at release time, and plan 0017 is about to change how a store opens and appends.

**Steps:**

1. Create `crates/fsm-store/tests/append_guard.rs` as a **guard** beside the existing `append_latency.rs` **measurement**. Keep both, and say at the top of each which it is: a measurement reports a number for a human, a guard asserts a bound and fails. Conflating them produces either a flaky gate or a number nobody reads.
2. Leave `append_latency.rs` exactly as it is, `#[ignore]`d, and leave `docs/RELEASE.md`'s manual step in place. The guard catches a collapse; the manual harness produces the table for the release notes. They answer different questions and neither replaces the other.
3. Assert a **ceiling with a wide tolerance**, not a regression against a stored number. CI is a shared, noisy, variable machine; a tight bound produces flakes, and a flaky performance test is deleted within a month.
4. Commit the baseline and the tolerance as named constants, with the measured numbers, the host, and the filesystem in a comment. The next person to widen the bound then has to state why, rather than nudging a literal.
5. Size the iteration count by measurement on this host and record the debug and release timings in the commit message, exactly as `executor_policy_chaos.rs` did for its committed count. The per-job ceiling is 45 minutes across three operating systems and two toolchains and is already dominated by `crash_harness.rs` and `executor_chaos.rs`.
6. Provide an environment variable to raise the iteration count for a real measurement run, so the guard and the harness are one code path with two budgets rather than two implementations that can disagree.
7. Guard the append path and the **cold open** path, since plan 0017 changes both and open cost is the thing sealing exists to reduce. A guard that only covers append would miss the regression this plan most expects.
8. Fail with a message stating the measured value, the ceiling, and the iteration count. A performance failure with no numbers cannot be triaged from a CI log.
9. Skip cleanly, with a message, where the environment cannot support a meaningful measurement — and make skipping visible rather than silent, since a guard that quietly skips everywhere is worse than none.

**Tests:**

- `crates/fsm-store/tests/append_guard.rs` passes on this host inside its committed budget.
- The guard fails when the ceiling is set below the measured value, asserted by driving the bound directly rather than by slowing the store — a guard that cannot be made to fail is not a guard.
- The failure message names the measured value, the ceiling, and the iteration count.
- The environment variable raises the iteration count, and the default count is used without it.
- The cold-open path is measured as well as append, with its own named ceiling.
- The guard leaves no temporary directory behind, per the standing rule after a temporary-directory leak once exhausted the host inode table.
- `append_latency.rs` is unchanged and still `#[ignore]`d, asserted by diff.
- `docs/RELEASE.md`'s manual step is intact and still names the same command.
- The committed budget fits the per-job ceiling with the existing heavy suites, with the measured timings recorded.

- **Done when:** `cargo test -p fsm-store --test append_guard` passes inside its measured budget, the guard is provably falsifiable by lowering its own bound, both append and cold-open have named ceilings with recorded host measurements, the existing measurement harness and its release step are untouched, no temporary directory survives, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
