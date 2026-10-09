---
id: nonblocking-execution-completions
title: "Nonblocking Execution Completions"
workstream: "0089"
kind: task
depends_on:
  - execution-host-ownership
gated: false
touches:
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/run/
  - crates/fsm-execute/src/containment/crash_matrix_native_tests.rs
  - crates/fsm-execute/tests/lifecycle_platform/
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/async_completion.rs
  - crates/fsm-execute/tests/public_surface.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/mcp/host/
  - .github/workflows/ci.yml
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
status: in_progress
merged_as: ""
---
# Nonblocking Execution Completions

A handler may take its configured timeout without preventing the writer
owner from answering another eligible request.

**Steps:**

1. Split driver preparation into owner-side decisions and bounded supervisor
   commands, with immutable completion messages returned to owner-side
   settlement; reuse existing scheduler and pipeline decisions.
2. Put process spawn, child wait, MCP exchange, capture collection, and stop
   waiting outside the owner; make the CLI host consume that boundary.
3. Reserve a slot before dispatch, identify completions by effect and
   attempt generation, and retain one pending completion per slot until its
   settlement is accepted; no store handle or journal allocator enters a worker.
4. Keep the slot occupied until the plan 0022 supervisor reports the attempt
   safely stopped or completed; duplicate and stale messages neither free a
   newer slot nor append an outcome for a different attempt.
5. Preserve standalone driver behavior and existing attempt/ack/advance
   ordering through an adapter over the same boundary, including recovery
   when settlement is interrupted or the writer is unavailable.
6. Update any changed public API inventory and the specification and guide
   for the new nonblocking boundary; integrate the real plan 0022 lifecycle
   adapter before claiming the real-process cases passed, and exercise the
   private host through its `cfg(test)` `execution_host` harness inside
   `mcp/host/tests/` rather than exposing it as a public API.

**Tests:**

- `cargo test -p fsm-execute --test async_completion`: a barrier-held process
  and a barrier-held MCP conversation prove that dispatch, completion polling,
  and stop requests return without waiting for either fixture's release;
  these crate-level tests cover the supervisor boundary, not the CLI host.
- `cargo test -p fsm-cli --lib execution_host`: integrate the private host,
  real store, and real lifecycle adapter with the same barrier-held handler
  kinds, and assert that a read and unrelated mutation finish before either
  handler is released, without a new public test-access API.
- Exhausted supervisor capacity keeps the next effect pending without
  losing a slot, inventing a successful attempt, or exceeding either cap.
- Duplicate completion, stale generation, completion after stop request,
  and backpressured settlement each produce the specified single outcome.
- The `execution_host` harness proves that a worker blocked on reaping or
  output collection does not block an owner control turn and that shutdown
  reaches the lifecycle adapter.
- Existing retry, restart, MCP result, and executor public-surface tests
  remain green; a journal reopened after interrupted settlement verifies.

- **Done when:** real process and MCP fixtures pass the low-level `async_completion` and private CLI `execution_host` inventories without handler waits on the store owner, while existing executor recovery and public-surface gates pass under the stable host gate.


Acceptance remains incomplete: `async_completion` now contains real held-process
and held-MCP public-driver cases, but those cases have only compile/lint evidence.
The private host observer passes both handler kinds on stable/MSRV at frozen
`217e5e84`; scoped runtime verdict
`568c9272b2fafe095aedb1b439b0bd33f41d0fb95b16ffe42e68d25f129250d7`.
This proves read/mutation/control responsiveness before release through the real
lifecycle adapter, not the remaining capacity, generation, backpressure,
interrupted-settlement or full-gate inventory.
The written tests above remain the acceptance inventory; existing worker and
empty-handler checks cannot replace it. Historical implementation checkpoints
are preserved in the task cache under SHA-256
`0d89e2ea236fa45e3e3b02f31038b82d3b1ba2de6b998fbf0a1cc034e60b18ac`.

Frozen `9e8e89b5` retains all twelve stopped/acked/event crash cases across
standalone/embedded and process/MCP, with five recovery source files unchanged
from verified `66c785ba`; continuity review digest
`06eb4326bbaff259756e6d9e230eb63d29f6e0def3f2c10d0d3bd77bd3518983`.
This preserves prior recovery evidence, but does not verify the changed worker
capacity path or the newly wired six-case completion inventory at this checkpoint.
