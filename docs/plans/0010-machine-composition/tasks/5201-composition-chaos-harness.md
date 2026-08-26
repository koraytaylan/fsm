---
id: composition-chaos-harness
title: "Composition Chaos Harness"
workstream: "0052"
kind: task
depends_on:
  - executor-composition-directives
  - instance-tree-legibility
  - cancel-cascade-and-orphans
gated: false
touches:
  - crates/fsm-cli/tests/composition_chaos.rs
status: done
merged_as: ""
---
# Composition Chaos Harness

Composition adds edges between instances, and an edge is exactly the thing a crash can leave half-written — so every enactment point gets the same restart treatment plan 0008 gave the executor.

**Steps:**

1. Create `crates/fsm-cli/tests/composition_chaos.rs` following `executor_chaos.rs`'s precedent exactly: a self-contained seeded xorshift64\* generator with the duplication documented in the file header, 200 seeded iterations with a `FSM_COMPOSITION_CHAOS_ITERS` override, and `COMPOSITION_CHAOS_SEED` to replay one.
2. Build each iteration's fixture from a parent machine invoking one or two slots, child machines of depth 2 and 3, and one machine that signals another — all in the repository's neutral business-process vocabulary.
3. Simulate death by dropping the driver and constructing a fresh one against the same data directory, at each named point: (a) before `invoke_child`; (b) after `invoke_child`, before the child's first event; (c) after the child settles, before `invocation_return`; (d) between a parent-exit transition and its cascade cancel; (e) mid-signal-delivery. Use the honest word — **restart** — since signal-kill coverage of the journal itself lives in `crash_harness.rs`.
4. Assert the invariants at every death point: the journal verifies clean and no tick panics; **exactly one** `instance_invoked` per slot and **exactly one** `invocation_returned` per slot; no child instance without a derivable `instance_invoked` record; no `Running` slot whose child does not exist; and every instance ends coherent — settled, or waiting on something that exists.
5. Assert death point (d)'s documented window specifically: the store is coherent, the child is `Running` and unreferenced, `doctor` reports exactly one orphan, and `repair --cancel-orphans` settles it with one record. This is the plan's one two-record window and it must be proven recoverable rather than argued to be.
6. Assert signal semantics under restart: at most one `signal_delivered` per `(sender, signal_id)`, and a target that advanced did so exactly once.
7. Print the seed and the death point on failure so a red run is a reproduction.
8. **CI budget is shared and this plan is not the only claimant.** `ci.yml` sets `timeout-minutes: 45` per job across a three-OS, two-toolchain matrix, and `crash_harness.rs` (1,000 spawns per profile) plus `executor_chaos.rs` (200 iterations) already dominate it. Measure this suite's wall time on the slowest CI platform, and if it adds more than a few minutes, **lower the committed default iteration count and keep the depth behind the env override** — the pattern `FSM_CRASH_ITERS` and `FSM_EXECUTOR_CHAOS_ITERS` already establish. Record the measured time and the chosen default in the commit message. Four new heavy suites each quietly assuming they have room is how a 45-minute ceiling becomes a red build nobody can attribute.

**Tests:**

- The harness is the test; every invariant above is asserted at every death point across every seed.
- A deliberately introduced bug — for example, deriving the child id from a counter instead of from `(parent, slot)` — is caught; verify during development, then revert.
- Depth-3 trees and two-slot parents are both exercised, not only the depth-1 case.
- The suite runs on all three CI operating systems with no path or line-ending dependency.
- Iteration count is configurable via the env override, and the committed default is the one the wall-time measurement justified — stated in the commit message alongside the measurement.

- **Done when:** `cargo test -p fsm-cli --test composition_chaos` passes 200 seeded iterations across all five death points with every invariant holding, the parent-exit window is proven recoverable through `doctor` and `repair`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `composition_chaos.rs` with its own xorshift64\* (the documented duplication), 200 seeded iterations over five death points, `FSM_COMPOSITION_CHAOS_ITERS` and `COMPOSITION_CHAOS_SEED`, and a `the_generator_is_deterministic` companion that also asserts every death point is reachable so no seed space is dead. Verified by mutation: deriving a child id from a counter instead of `(parent, slot)` fails the reopen with `ReplayMismatch { field: "child_instance_id" }`, and the mutation was reverted. Measured 15.8 s debug / 2.6 s release for the committed 200 iterations — recorded in the commit message with the chosen default, per step 8.

**Corrections.** The fixture's review machine needed a terminal `decided` state and a `returns` projection naming the field its *own* child declares, not a fixed name: a review over an audit reads `finding`, a review over a review reads `seen`. Without the terminal state a middle level can never settle, so a depth-3 tree could not return past its first level — which is exactly the case the plan asks this harness to exercise.
