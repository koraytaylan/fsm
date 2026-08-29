---
id: archive-crash-harness
title: "Archive Crash Harness"
workstream: "0083"
kind: task
depends_on:
  - cli-journal-archive
gated: false
touches:
  - crates/fsm-cli/tests/archive_crash.rs
status: planned
merged_as: ""
---
# Archive Crash Harness

The ordering is the whole safety argument, so the harness interrupts the seal at each of its nine steps rather than at random instants, and asserts the archive survives every one.

**Steps:**

1. Create `crates/fsm-cli/tests/archive_crash.rs` driving a real store through `fsm journal archive` with an injected interruption point, one case per numbered step of the operation.
2. For **every** interruption point assert four things: the store opens, it folds to a state consistent with the records that survive, `journal verify` passes for whichever of the three verdicts applies, and re-running the identical archive command completes the operation.
3. Assert separately, at every interruption point, that **the sealed records are readable from the archive**. This is the assertion that catches the failure mode the ordering exists to prevent: an implementation that removed a segment it had not successfully copied passes every live-store assertion and loses the history.
4. Assert the pre-commit and post-commit halves have different shapes, and pin both: interrupted at steps 4 through 6 the store opens **unsealed** with the partial archive inert; interrupted between 7 and 8 it opens **sealed** with segments still to remove. A harness that accepts either at every point proves nothing about where the commit point is.
5. Assert the folded state after an interrupted-then-completed seal equals the folded state of a store sealed without interruption, from the same starting journal. Restart equivalence is the property, and comparing to the uninterrupted run is how it is stated.
6. Follow plan 0016's chaos lesson explicitly: ask what an **extra** run would look like in the records, and assert against it. A re-run after a completed seal must be refused by the existing manifest, and a re-run mid-way must not produce a second seal record. Count seal records and assert exactly one.
7. Size the harness by measurement, not by ambition. `crash_harness.rs` and `executor_chaos.rs` already dominate the 45-minute per-job ceiling across three operating systems and two toolchains. Commit an iteration count measured on the host, behind an environment variable with a seed variable to replay one case, exactly as `executor_policy_chaos.rs` does with `FSM_POLICY_CHAOS_ITERS` and `POLICY_CHAOS_SEED`. Record the measured debug and release timings in the commit message.
8. Do not lower the `crash_harness.rs` 1 000-iteration floor to make room. If the suite is too slow, make this harness cheaper.

**Tests:**

- One named case per interruption point, so a failure line names the step that broke.
- Every case asserts the store opens, folds, verifies, and completes on re-run.
- Every case asserts the sealed records are readable from the archive.
- Pre-commit interruptions leave an unsealed store; post-commit interruptions leave a sealed one; both are pinned rather than accepted interchangeably.
- An interrupted-then-completed seal folds identically to an uninterrupted seal from the same journal.
- Exactly one seal record exists after any sequence of interruptions and re-runs.
- A torn tail written into the live journal after a seal is classified and repaired exactly as it is on an unsealed store, proving the seal did not change the tail contract.
- The committed iteration count runs inside the measured budget on this host, and the environment variables override it.

- **Done when:** `cargo test -p fsm-cli --test archive_crash` passes with one named case per interruption point, every case asserts archive readability as well as store health, the commit point is pinned by asserting different shapes on either side of it, exactly one seal record survives any interruption sequence, the committed iteration count and its measured timings are recorded, and `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo fmt --check` succeed.
