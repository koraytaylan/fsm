---
id: bounded-executor-shutdown
title: "Bounded Executor Shutdown"
workstream: "0094"
kind: task
depends_on:
  - executor-ownership-integration
gated: false
touches:
  - crates/fsm-cli/tests/workflow_race/classification.rs
  - crates/fsm-cli/tests/workflow_race/failed_stop.rs
  - crates/fsm-execute/src/containment/failed_stop_native_tests.rs
  - crates/fsm-cli/tests/workflow_race/fault_control.rs
  - crates/fsm-cli/tests/mcp_execute_workflow.rs
  - crates/fsm-cli/tests/workflow_race/full_disk.rs
  - crates/fsm-execute/src/containment/allocator_native_tests.rs
  - crates/fsm-execute/src/containment/full_disk_native_tests.rs
  - crates/fsm-cli/tests/workflow_race/crash.rs
  - crates/fsm-cli/tests/workflow_race/mod.rs
  - crates/fsm-execute/src/containment/workflow_native_tests.rs
  - crates/fsm-execute/tests/lifecycle_platform/workflow_probe.py
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/service/
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/shutdown.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/lifecycle.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/main.rs
  - crates/fsm-cli/tests/executor_shutdown.rs
  - docs/EXECUTOR-LIFECYCLE.md
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---

# Bounded Executor Shutdown

An explicit stop closes admission immediately and reports whether cleanup
completed, timed out or remains uncertain.

**Steps:**

1. Implement running/draining/stopping/stopped/uncertain lifecycle states and
   a public control handle/report with validated finite timeout bounds; add
   the minimal independent lifecycle pump to the existing production stdio
   entry so blocked input/output cannot prevent stop or already-admitted
   timeout processing, without launching pending work or scheduling retries
   and machine deadlines before plan 0020.
2. Drain current handlers until the deadline, then close their native domains;
   abort closes immediately, and both preserve pending effects on interrupted
   execution instead of inventing instance cancellation or failure events.
3. Add `fsm execute stop --data-dir <dir> --mode drain|abort --timeout-ms <n>`
   using the proved local control mechanism, with owner-only access, bounded
   request size and exact incarnation binding; control must work while the
   journal writer is unavailable.
4. Connect the approved native ordinary-termination/console notification paths
   and embedded host shutdown to the same lifecycle state machine, documenting
   how repeated signals escalate and how uncatchable kill differs.
5. Return an uncertain report within the deadline when native cleanup or
   journaling cannot finish, retain every unresolved durable claim and stop
   admitting work; keep `Drop` outside the guarantee and update public docs.

**Tests:**

- Standalone and embedded production tests cover explicit drain/abort, empty
  executor, completing and hung trees, zero/maximum/invalid timeout, repeated
  controls and control concurrent with spawn; admission closes before drain.
- Launch the current production embedded stdio binary, admit a long-running
  handler, leave stdin open without more frames, and issue external `execute
  stop`: the handler closes and the writer releases within the lifecycle bound
  without plan 0020 code, polling requests or EOF; a separate quiet-client case
  enforces the admitted handler's timeout but starts no new pending effect.
- Native Ctrl-C and ordinary termination cases use their actual OS mechanisms,
  demonstrate bounded reports or documented signal exits and retain claims
  whenever termination cannot be proved; hard kill is tested as recovery.
- Writer contention, full disk and failed native stop cannot hold the control
  response forever or turn interrupted shutdown into domain cancellation.
- Unauthorized/stale control requests fail, endpoint cleanup is bounded, and
  handlers cannot consume the MCP server's protocol stdin.

- **Done when:** the public API and native standalone/embedded shutdown tests prove bounded drain and abort with immediate admission closure, correct signal integration and durable uncertainty preservation, without inventing machine events or relying on `Drop`.

Acceptance is frozen at the steps, tests and Done when criterion above.
Historical checkpoint and review evidence is archived outside the repository in
the task-cache plan-status-archives directory, addressed by SHA-256:
`0d6fe5607ad2863c64eb8564df2a8dd74f7105ca194f8aac41b65b754f2e477c`.
