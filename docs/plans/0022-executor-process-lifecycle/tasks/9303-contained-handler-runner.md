---
id: contained-handler-runner
title: "Contained Handler Runner"
workstream: "0093"
kind: task
depends_on:
  - durable-execution-claims
gated: false
touches:
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/run/
  - crates/fsm-execute/src/mcp_client.rs
  - crates/fsm-execute/src/error.rs
  - crates/fsm-execute/tests/lifecycle_runner.rs
  - crates/fsm-execute/tests/lifecycle_runner/
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - docs/EXECUTOR-LIFECYCLE.md
  - docs/EMBEDDING.md
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Contained Handler Runner

The runner reports a settleable result only after the entire owned domain is
closed; a root exit or MCP response cannot release surviving descendants.

**Steps:**

1. Implement the proved backend behind one runner path for process and MCP
   handlers, binding launch authorization and every result to the claimed
   run identity and preventing user code before domain enrollment.
2. Separate candidate outcome, domain closing, verified termination and
   uncertain cleanup; retain ownership when kill, wait or native inspection
   fails, and never convert those failures into a successful termination.
3. Make natural root exit, MCP result, timeout, cancellation and explicit stop
   close remaining descendants and future admission before returning a
   settleable result or releasing a concurrency slot.
4. Bound captured bytes during execution, including disk usage, while draining
   excess output; preserve truthful prefix/digest semantics and bound MCP
   worker, reader and handle cleanup even when descendants retain pipes.
5. Expose explicit cleanup progress for the service; keep `Drop` best-effort,
   update the provisional API inventory and publish the actual native limits.

**Tests:**

- `cargo test -p fsm-execute --test lifecycle_runner` uses real roots and
  grandchildren for both handler kinds, including early root exit, lingering
  MCP server, retained pipes, repeated spawning and uncooperative shutdown.
- Limit/limit-plus-one stream cases prove bounded memory and spool usage;
  noisy children keep draining, exact truncation/digest semantics hold and
  many repeated runs do not leak threads, handles or capture files.
- Injected native termination/inspection errors leave an uncertain result;
  no success, reaped-tree claim or freed capacity is reported prematurely.
- Root enrollment and cleanup tests are load-bearing against the production
  runner, and native stable/MSRV coverage plus public-surface/zero-dependency
  gates pass without falling back to weaker platform implementations.

- **Done when:** the same production runner passes native process and MCP descendant/pipe/capture tests and returns a settleable result only with matching run identity and proved closed containment, while every uncertain cleanup remains explicit and bounded.
