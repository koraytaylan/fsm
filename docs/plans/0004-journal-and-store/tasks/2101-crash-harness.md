---
id: crash-harness
title: "Crash Harness"
workstream: "0021"
kind: task
depends_on:
  - instance-store
gated: false
touches:
  - crates/fsm-cli/tests/crash_harness.rs
status: done
merged_as: ""
---
# Crash Harness

Crash safety is proven, not asserted: a child process is killed mid-append at seeded random points 1,000 times, and after every kill the recovered store must equal the replay of a prefix of issued requests that contains every acknowledged request.

**Steps:**

1. Implement `crates/fsm-cli/tests/crash_harness.rs` with the self-re-execution mechanism from architecture: the parent test spawns `std::env::current_exe()` with args `["crash_child", "--exact", "--nocapture"]` and env `FSM_CRASH_CHILD=<data_dir>;<seed>`; the child appends a scripted request stream through the real `Store`, printing each request_id to stdout as its success response returns.
2. Kill the child after a seeded random delay, record the acknowledged request_ids, recover the store (invoking `repair_truncate_torn_tail` when classified as a torn tail), and assert the recovery invariant per **Tests**.
3. Loop for 1,000 seeded iterations (env `FSM_CRASH_ITERS` may raise, never lower, the count in CI), printing the seed on any failure.

**Tests:**

- `crash_harness.rs` — the recovery invariant, asserted after every one of the 1,000 kills (fixed master seed; the failing iteration's seed printed on failure): the recovered `StoreState` is bit-identical to the pure fold of some *prefix* of the scripted request stream, and every request_id the child printed before dying lies inside that prefix — acknowledged means durable, unacknowledged means at most the single in-flight record.
- Classification handling: when post-kill open classifies `TornTail`, the harness runs `repair_truncate_torn_tail` and the invariant must still hold on the repaired store; when it classifies `Ok`, recovery proceeds directly. Any other classification (`ChainBroken`, `NonCanonical`, `StateHashMismatch`) is an immediate failure — a crash may tear the tail, never the interior.
- Coverage histogram: under the fixed master seed the harness records each iteration's classification and asserts both `Ok` and `TornTail` occurred at least once across the run — proving the kill points actually straddle the commit boundary (deterministic under the fixed seeds).
- Child mechanics: the `crash_child` `#[test]` returns immediately (passing) when `FSM_CRASH_CHILD` is absent, so a plain `cargo test` run stays green; with the env set it appends until killed, printing each request_id only after its `Store` call returns.
- Iteration control: `FSM_CRASH_ITERS=2000` runs 2,000 iterations; `FSM_CRASH_ITERS=10` still runs 1,000 (the count can be raised, never lowered).

- **Done when:** `cargo test -p fsm-cli --test crash_harness` completes 1,000 kill-and-recover iterations with the invariant holding in every one, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
