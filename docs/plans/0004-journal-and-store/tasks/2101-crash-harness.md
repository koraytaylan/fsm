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
status: planned
merged_as: ""
---
# Crash Harness

Crash safety is proven, not asserted: a child process is killed mid-append at seeded random points 1,000 times, and after every kill the recovered store must equal the replay of a prefix of issued requests that contains every acknowledged request.

**Steps:**

1. Implement `crates/fsm-cli/tests/crash_harness.rs` with the self-re-execution mechanism from architecture: the parent test spawns `std::env::current_exe()` targeting the child entry test with `FSM_CRASH_CHILD=<data_dir>;<seed>`; the child appends a scripted request stream through the real `Store`, printing each request_id as its success response returns.
2. Kill the child after a seeded random delay, record the acknowledged request_ids, recover the store (invoking `repair_truncate_torn_tail` when classified as a torn tail), and assert the recovery invariant: recovered state equals the replay of a prefix of issued requests, and every acknowledged request is inside that prefix.
3. Loop for 1,000 seeded iterations (env `FSM_CRASH_ITERS` may raise, never lower, the count in CI), printing the seed on any failure.

- **Done when:** `cargo test -p fsm-cli --test crash_harness` completes 1,000 kill-and-recover iterations with the invariant holding in every one, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
