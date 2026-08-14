---
id: simulate
title: "Simulate"
workstream: "0015"
kind: task
depends_on:
  - apply-pipeline
  - creation-entry-chain
gated: false
touches:
  - crates/fsm-core/src/simulate.rs
  - crates/fsm-core/tests/simulate_runs.rs
status: planned
merged_as: ""
---
# Simulate

Pure what-if execution: create an instance in memory, drive an event sequence through `step()` with a fresh budget per event, and report per-step outcomes and traces — no persistence, no identifiers, no side effects, so authors can test machines before anything is recorded.

**Steps:**

1. Implement `OnReject { Stop, Continue }`, `SimStep`, `SimReport`, and `simulate(m, t, overrides, events, on_reject)` in `crates/fsm-core/src/simulate.rs` per architecture.
2. Add `crates/fsm-core/tests/simulate_runs.rs`: drive `case_review` through a full accept path (create → `docs_ok` → `docs_ok` → `scored` high → terminal), a suspend/resume path exercising deep history, and a rejection path under both `Stop` (later events unprocessed, `stopped_at` set) and `Continue` (rejection recorded, subsequent events still applied); assert simulation of a sequence equals the fold of individual `step()` calls.

- **Done when:** the three scenario runs and the simulate-equals-stepping assertion pass under `cargo test -p fsm-core --test simulate_runs`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
