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
status: done
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

**Landed:** Five restart points crossed with five fixtures, seeded, with `POLICY_CHAOS_SEED` to replay one and the seed, fixture, and point in every failure message. The journal invariants are the plan's, with one tightened: at most `attempts - 1` records per effect rather than `attempts`, because the last failure is acked rather than journaled — a bound the code actually guarantees is worth more than one it merely satisfies.

**The mutation test changed the suite, which is the point of running it.** Swapping the journal-derived count for a process-local counter — the bug this task names — passed **every** journal assertion. It would: the store refuses an out-of-order attempt, the effect stays pending, and the next tick quietly fixes the number, so the records end up identical while the handler runs an extra time per restart. The suite now counts handler runs against a per-point bound derived from what the journal already knew, and that bound is what turns the mutation red. Without it this task would have shipped a suite that certified a policy turning "three tries" into four runs.

Two things the fixtures taught. A cancelled instance cannot carry a fabricated attempt record: the scheduler never starts an effect of one, and a run killed by cancellation is settled rather than retried, so the first version of the harness was asserting against a state the system cannot produce. The seeded loop now lands the restart on the cancellation itself, and a separate row covers the reachable case — attempt one journaled, *then* cancelled — where a successor reads "one failed attempt, budget remaining" and must still not act. And the resume loop drains outboxes rather than stopping at the first terminal instance, because a terminal instance's remaining effects are still acked (plan 0008's rule) and stopping early would hide what the successor did with them.

The MCP fixture drives **this project's own server**: `fsm serve --read-only` on the same data directory, asked for an instance that does not exist, which is a tool error and therefore the `mcp_error` class. A second stub would only prove the stub.

**Budget, measured rather than assumed.** On this Linux host, 40 iterations take 9.1 s in debug and 2.3 s in release; 200 take 45.0 s in debug. `ci.yml` runs both profiles across three operating systems under a 45-minute ceiling that `crash_harness.rs` and `executor_chaos.rs` already dominate, and Windows process creation is far costlier. The committed default is **40** — about twelve seconds per Linux job — with the depth behind `FSM_POLICY_CHAOS_ITERS`, and a row asserts that 40 still reaches every restart point and every fixture, so a default too low to cover what the suite claims would itself be red.
