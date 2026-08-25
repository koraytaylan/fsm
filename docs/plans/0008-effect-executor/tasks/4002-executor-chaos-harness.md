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
  - crates/fsm-cli/tests/fixtures/executor/recording_stub.sh
status: planned
merged_as: ""
---
# Executor Chaos Harness

A seeded harness (200 iterations, self-contained xorshift64* generator — the deliberate ~30-line duplication with the plan-0007 suites is documented in the file header per precedent) makes the resumability claim checkable: kill the executor at every named point and assert the journal stays coherent and no effect's handler runs twice.

**Steps:**

1. Author `recording_stub.sh`: a handler that appends a line to a side file (path from an env var) each time it executes, then exits 0 — this is the double-run detector.
2. Implement `crates/fsm-cli/tests/executor_chaos.rs`: 200 seeded iterations over a fresh temp data dir; each builds a machine with one effect, drives a writer to leave one effect pending, then runs `fsm_execute` ticks while interleaving a simulated executor death at one of the named points: (a) after spawn before reap, (b) after reap before ack, (c) after ack before advance-send, (d) mid-poll.
3. Simulate death by dropping the runner/pipeline and constructing a *fresh* executor (its `request_id` derivation is stateless, so it re-derives the same ids) against the same data dir, then continuing ticks to completion.
4. After each iteration assert: the journal verifies clean; **the recording stub's side file lists the effect at most once** (no double-run); the instance reaches a coherent terminal or a still-pending state (never an incoherent one); and no tick panicked.
5. On failure print the seed; honour `EXECUTOR_CHAOS_SEED` to replay exactly one seed.

**Tests:**

- 200 seeded iterations all satisfy: journal verify clean, handler side-file shows the effect ran exactly once (for the death points where the ack was already journaled) or is safely re-run to a single journaled ack (for death before ack), instance coherent, no panic.
- Deterministic: a fixed base seed reproduces identical pass/fail across two invocations; a deliberately corrupted expectation under a fixed `EXECUTOR_CHAOS_SEED` replays that one seed.
- Death-after-ack-before-advance: the fresh executor re-derives `exec-ev-…` and the advance send replays as `duplicate: true` — asserting at-least-once delivery without double-application.
- Death-before-ack: the effect is re-started and the resulting ack is the only `effect_acked` record for that effect in the journal.

- **Done when:** `cargo test -p fsm-cli --test executor_chaos` passes all 200 seeded iterations with the no-double-run and journal-coherent invariants, seed replay works, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
