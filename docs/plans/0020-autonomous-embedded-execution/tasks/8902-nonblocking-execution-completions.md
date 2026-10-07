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
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/async_completion.rs
  - crates/fsm-execute/tests/public_surface.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/mcp/host/
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

Implementation checkpoint: owned stdio now selects original raw transport
worker polling with reserved count/byte capacity and join-gated retirement;
standalone defaults remain synchronous. Real held-child tests cover polling,
cancellation, response storage and original worker retirement; stable/MSRV
executor and stdio checks, all-target Clippy and four guard sensitivity cases
pass at exact `80f1e5a` (see TRANSPORT-WORKER-REVIEW.md). Helper spawn, receipt verification, durable completion storage,
installed-handler responsiveness and lifecycle fault acceptance remain open;
no prerequisite is released and merged_as remains empty.
