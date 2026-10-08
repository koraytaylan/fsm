---
id: uncertain-run-reconciliation
title: "Uncertain Run Reconciliation"
workstream: "0094"
kind: task
depends_on:
  - bounded-executor-shutdown
gated: false
touches:
  - crates/fsm-execute/src/service/
  - crates/fsm-execute/src/error.rs
  - crates/fsm-execute/tests/reconcile_runs.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/tests/executor_reconcile.rs
  - docs/EXECUTOR-LIFECYCLE.md
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Uncertain Run Reconciliation

An operator needs an inspectable interrupted run and a recovery action whose
evidence is stronger than a guessed PID or elapsed timeout.

**Steps:**

1. Add strictly read-only `fsm execute runs --data-dir <dir>` with bounded,
   stable summaries of run/effect identity, lifecycle phase, native evidence
   and the next required recovery action; expose matching MCP health counts.
2. Add `fsm execute reconcile --data-dir <dir> --run-id <id>` using recorded
   backend/domain identity to close admission, terminate remaining members,
   prove closure and commit a matching stopped record before retry admission.
3. Share reconciliation with startup recovery so operator and automatic paths
   cannot disagree about sufficient evidence; revalidate the run under the
   writer and refuse races with another live owner or a changed identity.
4. Keep unknown domains unresolved, including after missing native facilities,
   identity reuse, handler-table changes or copied store data; document any
   proved environment-reset recovery mechanism selected by task 9301.
5. Provide concrete upgrade, interruption and recovery transcripts and precise
   diagnostic hints; do not add a force-clear, age threshold or kill-by-PID
   bypass, and keep local termination separate from remote reconciliation.

**Tests:**

- Listing through CLI/MCP is byte-for-byte non-mutating with a live writer,
  unknown backend and read-only/degraded store, and omits handler secrets.
- Real orphaned process and MCP trees are reconciled and then permit a
  sequential restart, while PID reuse, wrong environment identity, surviving
  descendants and unavailable facilities refuse clearance without a spawn.
- Repeated/concurrent reconciliation is idempotent, stale run ids cannot stop
  another tree, and a removed/changed handler cannot reinterpret old results.
- Copied stores and verified backend reset fixtures distinguish proven death
  from missing metadata; malformed and unknown commands return actionable
  bounded diagnostics without weakening the claim invariant.

- **Done when:** production inspection and reconciliation commands pass native orphan/identity/race tests, permit retry only after durable verified closure, and give an actionable non-destructive recovery path for every documented uncertain state.
