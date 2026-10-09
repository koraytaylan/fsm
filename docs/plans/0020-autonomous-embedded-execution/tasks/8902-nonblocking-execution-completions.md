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


Frozen `ee296852` passes all twelve native completion cases on stable and MSRV
in [CI 37893915527](https://github.com/koraytaylan/fsm/actions/runs/37893915527).
Independent retained-report review digest:
`b382a31313cd4ab581ab5fe63a06be67e42ab6268b4d7a83cfc62745cff7ccde`.
Each toolchain exercises process and MCP handlers through public held polling,
repeated settlement, writer refusal with authentic successor-generation
rejection, private owner read/mutation/control, exhausted reservation capacity,
and inherited output pipes. The fixture waits for protected completion-record
publication before recovery, preserving the original writer's unsettled state.
The verdict binds frozen source, compiler, inventory, commands and retained logs;
it does not independently compare the staged executable bytes.

Review confirms shared reservation ownership across helper phases and retention
until settlement. Focused evidence comprises 27 artifact/evidence checks,
49 portable host, 25 public-surface/retry and 13 worker regressions; pre-dispatch
capacity refusal has a load-bearing 0/101/0 regression at `631e7496`.
All eight stable host gate stages pass at frozen `0bd1e14e`, retained report
`11d09f239210cfd4a4b7f71a7ba738902ed478da3ace92b5182684aa67dc5a4c`.
Recovery continuity review
`06eb4326bbaff259756e6d9e230eb63d29f6e0def3f2c10d0d3bd77bd3518983`
preserves the original stopped/acked/event crash inventory from `66c785ba`.
The current full integration checkpoint is still required before completion;
the written acceptance inventory remains unchanged and no landing OID is assigned.
Historical implementation evidence remains in the task cache under
`0d89e2ea236fa45e3e3b02f31038b82d3b1ba2de6b998fbf0a1cc034e60b18ac`.
