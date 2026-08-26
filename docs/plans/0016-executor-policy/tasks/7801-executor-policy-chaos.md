---
id: executor-policy-chaos
title: "Executor Policy Chaos"
workstream: "0078"
kind: task
depends_on:
  - mcp-result-mapping
  - per-instance-fairness
gated: false
touches:
  - crates/fsm-cli/tests/executor_policy_chaos.rs
status: planned
merged_as: ""
---
# Executor Policy Chaos

Retry exists to survive a restart, so the suite that proves it has to restart the executor in the middle of one — and the invariant that matters is that a killed process never costs or gains an attempt.

**Steps:**

1. Create `crates/fsm-cli/tests/executor_policy_chaos.rs` following `executor_chaos.rs`'s precedent exactly: a self-contained seeded xorshift64\* generator with the duplication documented in the header, 200 seeded iterations with a `FSM_POLICY_CHAOS_ITERS` override, and `POLICY_CHAOS_SEED` to replay one.
2. Restart the executor at each named point inside a retry sequence: (a) after an attempt runs, before its `effect_attempted` record; (b) after the record, before the backoff elapses; (c) during backoff; (d) after exhaustion, before the ack; (e) mid-MCP-conversation.
3. Assert the attempt invariants at every point: attempts are **gapless and strictly increasing** per effect; there are **at most `attempts`** attempt records per effect; there is exactly one `effect_acked` per effect; and the ack carries the exhaustion cause **if and only if** the count reached the limit.
4. Assert the at-least-once boundary honestly, as plan 0008 did: a restart at point (a) may re-run the handler, because the attempt record is what makes an attempt remembered and a killed process loses what it had not journaled. The claim is that the **journal** stays exact, not that the world does.
5. Assert the cancellation rule under chaos: a cancelled effect is never retried at any restart point, whatever the table says.
6. Assert the caps hold with a fresh scheduler: across every restart, no tick exceeds `max_inflight` or `max_inflight_per_instance`, and no instance is starved across the run.
7. Assert MCP handlers behave identically: a restart mid-conversation leaves the journal exact, the child is re-parented or reaped, and the next executor re-runs the effect.
8. Print the seed and the restart point on failure so a red run is a reproduction.
9. **CI budget is shared and this plan is not the only claimant.** `ci.yml` sets `timeout-minutes: 45` per job across a three-OS, two-toolchain matrix, and `crash_harness.rs` (1,000 spawns per profile) plus `executor_chaos.rs` (200 iterations) already dominate it. Measure this suite's wall time on the slowest CI platform, and if it adds more than a few minutes, **lower the committed default iteration count and keep the depth behind the env override** — the pattern `FSM_CRASH_ITERS` and `FSM_EXECUTOR_CHAOS_ITERS` already establish. Record the measured time and the chosen default in the commit message. Four new heavy suites each quietly assuming they have room is how a 45-minute ceiling becomes a red build nobody can attribute.

**Tests:**

- The harness is the test; every invariant above is asserted at every restart point across every seed.
- Fixtures include a handler with `attempts: 3`, one with no `retry`, one `mcp` handler, and a table exercising both caps.
- A deliberately introduced bug — for example, keeping the attempt counter in `inflight` instead of deriving it — is caught; verify during development, then revert.
- Journals verify clean after every iteration.
- No tick panics at any restart point, including mid-MCP-conversation.
- The suite runs on all three CI operating systems with no path or line-ending dependency.
- Iteration count is configurable via the env override, and the committed default is the one the wall-time measurement justified — stated in the commit message alongside the measurement.

- **Done when:** `cargo test -p fsm-cli --test executor_policy_chaos` passes 200 seeded iterations across all five restart points with gapless attempts, at most `attempts` records per effect, exactly one ack, no retried cancellation, and caps respected by a fresh scheduler; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
