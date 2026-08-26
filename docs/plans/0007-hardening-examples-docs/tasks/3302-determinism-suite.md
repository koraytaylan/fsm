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
status: done
merged_as: ""
---
# Determinism Suite

Determinism at scale is the headline guarantee, so generated machines must refold bit-identically three ways — snapshot-plus-tail, full replay, fresh reopen — and the worst legal request must stay inside the latency budget.

**Steps:**

1. Implement `crates/fsm-cli/tests/determinism.rs`, importing the generator via `#[path = "../../fsm-core/tests/proputil.rs"] mod proputil;` (a test-only path include; no shipped coupling — stated in the file).
2. Implement the corpus run and the perf smoke exactly as inventoried under **Tests**.

**Tests:**

- Corpus run over 50 fixed seeds: generate a machine and event sequence (including the generator's tagged wrong-payload share, so rejections are in the journal), drive them through the real `Store` append path with a `FixedClock`, then refold three ways — snapshot-plus-tail, full-replay-ignoring-snapshots, fresh-reopen — asserting every per-instance `state_hash` is bit-identical across all three and journal verification is green each way.
- Snapshot-path honesty: a snapshot is forced mid-sequence for at least half the seeds (threshold crossed or explicit flush), asserted by counting snapshot files — the snapshot-plus-tail leg cannot silently degenerate into full replay.
- Rejection refold: at least one seed's journal contains `event_rejected` records, and their recorded unchanged `state_hash` re-verifies on every refold leg.
- Perf smoke: build the largest legal definition (the plan-0001 limits maxima) and the deepest-pipeline worst-case event (12-deep exit and entry chains with full blocks); the mean of 10 `instance_send` round-trips stays under 250 ms — timed with `std::time::Instant` in test code (a comment notes the `Instant` ban covers `fsm-core/src` only), printing the measured mean on failure.
- Reproducibility: every assertion failure prints the offending seed.

- **Done when:** `cargo test -p fsm-cli --test determinism` passes the three-way bit-identity assertion for all 50 seeds (snapshot leg proven exercised) and the sub-250 ms perf smoke, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
