---
id: determinism-suite
title: "Determinism Suite"
workstream: "0033"
kind: task
depends_on:
  - machine-generators
gated: false
touches:
  - crates/fsm-cli/tests/determinism.rs
status: planned
merged_as: ""
---
# Determinism Suite

Determinism at scale is the headline guarantee, so generated machines must refold bit-identically three ways — snapshot-plus-tail, full replay, fresh reopen — and the worst legal request must stay inside the latency budget.

**Steps:**

1. Implement `crates/fsm-cli/tests/determinism.rs`, importing the generator via `#[path = "../../fsm-core/tests/proputil.rs"] mod proputil;` (a test-only path include; no shipped coupling — stated in the file).
2. For 50 seeds: generate a machine and event sequence, drive them through the real `Store` append path with a `FixedClock`, then refold snapshot-plus-tail, full-replay-ignoring-snapshots, and fresh-reopen, asserting all per-instance `state_hash` values are bit-identical and verification is green.
3. Add the perf smoke: build the largest legal definition and the deepest-pipeline worst-case event per the plan-0001 limits, and assert the mean of 10 `instance_send` round-trips stays under 250 ms — timing via `std::time::Instant` in test code, with a comment noting the `Instant` ban covers `fsm-core/src` only.

- **Done when:** `cargo test -p fsm-cli --test determinism` passes the three-way bit-identity assertion for all 50 seeds and the sub-250 ms perf smoke, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
