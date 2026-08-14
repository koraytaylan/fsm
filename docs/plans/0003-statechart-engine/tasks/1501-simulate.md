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

1. Author `crates/fsm-core/tests/simulate_runs.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `OnReject { Stop, Continue }`, `SimStep`, `SimReport`, and `simulate(m, t, overrides, events, on_reject)` in `crates/fsm-core/src/simulate.rs` per architecture.

**Tests:**

- Full accept path over `case_review`: create → `docs_ok` → `docs_ok` → `scored` (score ≥ 700) — per-step leaves asserted (`intake` → `in_review`/`docs_review` → `risk_review` → `approved`), `final.terminal = true`, and each `SimStep` carries its outcome and trace.
- Suspend/resume path exercising deep history: … `suspend` → `resume` — the report shows the restored leaf `risk_review` and `visits = 2` at the end (entry blocks re-ran during simulation exactly as in live stepping).
- Rejection under `Stop`: a mid-sequence unhandled event → `stopped_at` set to its index, later events absent from the report; under `Continue`: the rejection recorded as its step's outcome and subsequent events still applied (final leaf asserted).
- Simulate-equals-stepping: for the accept path, the report's per-step leaves, contexts, and traces equal a manual fold of `create` + individual `step()` calls over the same inputs (structural equality).
- Fresh budget per event (hand-built): an event sequence whose *cumulative* node visits exceed one budget but whose per-event visits fit — simulation succeeds, proving the budget resets each event.
- Purity: `SimReport` contains no instance ids, no timestamps, and two runs over identical inputs are structurally equal.

- **Done when:** the three scenario runs, the simulate-equals-stepping assertion, and the fresh-budget case pass under `cargo test -p fsm-core --test simulate_runs`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
