---
id: ops-commands
title: "Ops Commands"
workstream: "0026"
kind: task
depends_on:
  - instance-commands
gated: false
touches:
  - crates/fsm-cli/src/cli/ops.rs
status: planned
merged_as: ""
---
# Ops Commands

The auditor and operator surface: full-chain verification with granular integrity exit codes, snapshot-free replay comparison, a store health report, and the explicit quarantine-then-truncate repair.

**Steps:**

1. Fill `crates/fsm-cli/src/cli/ops.rs::SPECS` with `journal verify [--report]` mapping `JournalHealth` to exit codes 0 Ok / 2 TornTail / 3 ChainBroken / 4 StateHashMismatch / 5 NonCanonical / 6 LockIo, `--report` printing per-segment progress and the final counts-and-hashes summary.
2. Add `journal replay [--to-seq N]` — refold ignoring snapshots and report hash agreement or the first divergent seq.
3. Add `doctor` (data dir and VERSION, lock holder, snapshot inventory, quick verify summary, effective env) and `repair --truncate-torn-tail` (invokes the plan-0004 repair, printing quarantine path and truncation seq; refuses interior corruption).
4. Add inline unit tests over the committed journal fixtures: each fixture's exit code from `journal verify`, and repair's refusal on interior corruption.

- **Done when:** inline ops tests prove the six-way exit-code mapping against the fixtures and the repair refusal path, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
