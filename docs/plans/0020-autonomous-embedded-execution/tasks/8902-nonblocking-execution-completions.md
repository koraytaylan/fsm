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
status: done
merged_as: "ee296852b5cdaad2d1f88781fe6a390248233240"
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


Frozen `ee296852` closes the written inventory in
[CI 37894146107](https://github.com/koraytaylan/fsm/actions/runs/37894146107):
all six stable/MSRV portable gates, both native jobs and zero-dependency
acceptance pass. Independent frozen acceptance verdict:
`a91f05d4cc8d7b374681f327ffea2163630f7dd87cad9dded6209cf820cc43af`.
Each native toolchain passes twelve completion cases, sixty crash cases with
forty-eight resource observations, 101 containment cases, thirty-four original
workflow scenarios and two original-executor upgrade scenarios.
The completion cases cover held process/MCP dispatch and polling, private-owner
read/mutation/control, inherited output pipes, exhausted count/byte capacity,
repeated settlement, authentic stale-generation refusal and writer backpressure;
crash cuts verify interrupted settlement and outcome advancement on reopen.

Source review confirms reservation sharing only after predecessor retirement,
retention through writer refusal and owner-side durable settlement, with no
worker store handle or journal allocator. Pre-dispatch capacity refusal remains
queued, pinned by the `631e7496` load-bearing 0/101/0 regression.
All eight stable host gate stages pass at frozen `0bd1e14e`, retained report
`11d09f239210cfd4a4b7f71a7ba738902ed478da3ace92b5182684aa67dc5a4c`;
the successor changes only documentation and completion-fixture publication
waiting, and the full successor CI verifies its exact source.
The frozen verdict binds source, compiler, job/step inventory, native invocation,
reports, runtime markers and retained logs; staged executable bytes are not
independently compared. Native capability remains Linux/systemd only.
Scheduling fairness, channels and transport acceptance remain sibling tasks.
Historical implementation evidence remains in the task cache under
`0d89e2ea236fa45e3e3b02f31038b82d3b1ba2de6b998fbf0a1cc034e60b18ac`.
