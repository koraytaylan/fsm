---
id: executor-chaos-harness
title: "Executor Chaos Harness"
workstream: "0040"
kind: task
depends_on:
  - golden-two-process-session
gated: false
touches:
  - crates/fsm-cli/tests/executor_chaos.rs
status: done
merged_as: ""
---
# Executor Chaos Harness

A seeded harness (200 iterations, self-contained xorshift64* generator — the deliberate ~30-line duplication with the plan-0007 suites is documented in the file header per precedent) makes the resumability claim checkable: restart the executor at every named point and assert the journal stays coherent and each effect is acked exactly once.

**Steps:**

1. The recording stub is this test binary re-executed (`std::env::current_exe()` plus a marker argument, as in `crash_harness.rs`) — no `.sh` fixture, since CI runs the suite on Windows too. It appends one line to the path in an env var each time it runs, then exits 0: that side file is the double-run detector.
2. Implement `crates/fsm-cli/tests/executor_chaos.rs`: 200 seeded iterations (floor 200, raised by `FSM_EXECUTOR_CHAOS_ITERS`, mirroring `FSM_CRASH_ITERS`) over a fresh temp data dir; each builds a machine with one effect, drives a writer to leave one effect pending, drops that writer handle (the per-data-dir lock would otherwise shut the executor out), then runs `fsm_execute` ticks while interleaving a simulated executor death at one of the named points: (a) after spawn before reap, (b) after reap before ack, (c) after ack before advance-send, (d) mid-poll.
3. Simulate death by dropping the runner/pipeline/scheduler and constructing a *fresh* executor against the same data dir, then continuing ticks to completion. Say **restart**, not `kill -9`, in the file header and the assertions: real signal-kill coverage of the journal lives in `crash_harness.rs`, and what this harness proves is that a fresh executor's journal-derived decisions resume correctly with nothing carried in memory.
4. After each iteration assert the invariant in the shape the design actually guarantees — **at-least-once execution, exactly-once journaling**:
   - the journal verifies clean and no tick panicked;
   - the instance holds **exactly one** `effect_acked` record per effect id, and at most one advance `event_applied` per `(effect_id, event)`;
   - for death points (c) and (d) — where the ack is already journaled — the side file lists the effect **at most once**;
   - for death points (a) and (b) — both *before* the ack — the side file may list it **twice**, and the assertion is only that the journal still shows one ack. Reaping a child puts its outcome in memory and a restart loses memory, so a successor finding a pending effect with an unclaimed key cannot know the handler already ran; that second run is the documented at-least-once boundary, not a defect to assert away;
   - the instance ends coherent: terminal, or still pending with a resumable effect.
5. On failure print the seed; honour `EXECUTOR_CHAOS_SEED` to replay exactly one seed.

**Tests:**

- All iterations satisfy the four invariant rows above at every death point.
- Deterministic: a fixed base seed reproduces identical pass/fail across two invocations; `EXECUTOR_CHAOS_SEED` replays one seed exactly.
- Death-after-ack-before-advance: the fresh executor finds the ack in `settled`, sees its `exec-ev-…` key unclaimed, and sends the advance — the resume rule, exercised end to end. If the send had already landed, the re-issue replays as `duplicate: true`.
- Death-before-ack, whether the child had been reaped or not: the effect is re-started and the resulting ack is the only `effect_acked` record for that effect in the journal.
- Budget note in the file header: the 45-minute CI ceiling is already dominated by `crash_harness.rs`'s 1,000 spawns per profile across two profiles and three operating systems, and Windows process creation is several times costlier — raise the iteration floor only with that in view.

- **Done when:** `cargo test -p fsm-cli --test executor_chaos` passes all seeded iterations with the exactly-once-journaling and coherence invariants at every death point, seed replay works, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
