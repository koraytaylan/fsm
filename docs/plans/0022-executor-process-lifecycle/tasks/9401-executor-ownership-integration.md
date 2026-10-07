---
id: executor-ownership-integration
title: "Executor Ownership Integration"
workstream: "0094"
kind: task
depends_on:
  - contained-handler-runner
gated: false
touches:
  - crates/fsm-execute/src/containment/runner_native_tests.rs
  - crates/fsm-execute/src/containment/runner_recovery_native_tests.rs
  - crates/fsm-execute/src/containment/runner_handoff_recovery_native_tests.rs
  - crates/fsm-execute/src/containment/allocator.rs
  - crates/fsm-execute/src/containment/allocation_contention_native_tests.rs
  - crates/fsm-execute/src/run/native_client/worker.rs
  - crates/fsm-execute/src/run/native_client/worker/tests.rs
  - crates/fsm-execute/src/containment/authority.rs
  - crates/fsm-execute/src/containment/allocator_native_tests.rs
  - crates/fsm-execute/src/containment/binding_contention_native_tests.rs
  - crates/fsm-execute/src/containment/closure_prepared.rs
  - crates/fsm-execute/src/containment/closure.rs
  - crates/fsm-execute/src/run/native_client/prepared_cleanup.rs
  - crates/fsm-execute/src/service/lifecycle/paired.rs
  - crates/fsm-execute/src/service/lifecycle/mod.rs
  - crates/fsm-execute/src/service/lifecycle/control.rs
  - crates/fsm-execute/src/run/native_owners.rs
  - crates/fsm-execute/src/run/native_host.rs
  - crates/fsm-execute/src/run/native_admission.rs
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/run/pipeline.rs
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/execution_ownership.rs
  - crates/fsm-execute/src/containment/enrollment_native_tests.rs
  - crates/fsm-execute/src/containment/allocator_native_tests.rs
  - crates/fsm-execute/src/containment/broker_native_tests.rs
  - crates/fsm-execute/src/containment/broker_disconnect_native_tests.rs
  - crates/fsm-execute/src/containment/workflow_native_tests.rs
  - crates/fsm-execute/src/containment/workflow_memory_limits.rs
  - crates/fsm-execute/src/containment/broker_claimed_closure_native_tests.rs
  - crates/fsm-execute/src/containment/supervisor_fresh_native_probe.rs
  - crates/fsm-execute/tests/lifecycle_platform/authority_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/test_authority_retirement.py
  - crates/fsm-execute/tests/lifecycle_platform/workflow_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/workflow_failure_export.py
  - crates/fsm-execute/tests/lifecycle_platform/test_workflow_failure_export.py
  - crates/fsm-execute/src/containment/workflow_failure_diagnostics.rs
  - crates/fsm-execute/tests/lifecycle_platform/test_workflow_producer.py
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/native_error.rs
  - crates/fsm-cli/src/native_error_tests.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/tests/executor_ownership.rs
  - crates/fsm-cli/tests/mcp_execute_workflow.rs
  - crates/fsm-cli/tests/mcp_executor.rs
  - crates/fsm-cli/tests/serve_modes.rs
  - crates/fsm-cli/tests/workflow_race/mod.rs
  - crates/fsm-cli/tests/workflow_race/crash.rs
  - crates/fsm-cli/src/local_control/client.rs
  - crates/fsm-cli/src/local_control/protocol.rs
  - crates/fsm-cli/src/local_control/server.rs
  - crates/fsm-cli/src/local_control/mod.rs
  - crates/fsm-cli/tests/local_executor_control.rs
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---

# Executor Ownership Integration

Every execution host must claim before launch and replay a durable stopped
result before considering another run.

**Steps:**

1. Split `service::prepare` so stopping and observing active runs stay
   independent of writer access, while every new launch follows a durable
   claim and a writer-protected recheck of effect eligibility.
2. Thread run identity and immutable handler fingerprint through scheduler,
   watcher, runner and pipeline; reject stale results and never reinterpret
   recovered results with a changed handler table.
3. Persist verified stopped results before attempt/ack/event settlement and
   recover that settlement without rerunning the handler; keep existing
   acknowledgement-before-event ordering and derived request keys unchanged,
   consuming each stopped result atomically with its retry/ack disposition
   while preserving claim exclusion until that transaction is durable.
4. Route standalone execution, embedded service ticks, public tick helpers and
   startup recovery through the same sequence, reconciling claims before
   allowing any conflicting new run; preserve read-only/degraded non-execution.
5. Keep uncertain runs pending and visible, with bounded health summaries and
   no argv/secret disclosure; update API inventory and execution guarantees.

**Tests:**

- Race standalone/standalone and standalone/embedded executors against one
  pending effect with an independent external marker proving one live tree.
- Hold the writer while an active handler times out: it stops promptly,
  remains claimed, and settles once the writer is available; no unclaimed
  start occurs during contention.
- Crash after claim, after launch, after verified stop, after stopped record,
  after attempt/ack and before outcome event; assert exactly the specified
  sequential retry or settlement recovery and a valid journal each time.
- Pause after a durable stopped result and race a second executor before
  settlement: no new handler starts; retry resumes only after single-consumption
  disposition and the journaled backoff deadline, including after restart.
- Change/remove handler configuration before restart and inject late results
  from an old incarnation; neither starts conflicting work nor applies the
  wrong contract, including public helper and read-only entry points.
- Existing retry, cancellation, dead-letter, embedded workflow and
  interrupted-outcome suites pass with unchanged domain event semantics.

- **Done when:** production standalone, embedded and public tick paths pass concurrent-executor and launch/settle crash tests proving that a matching durable claim precedes every start and that unresolved or stale ownership cannot authorize a second run.

Acceptance is frozen at the steps, tests and Done when criterion above.
Historical checkpoint and review evidence is archived outside the repository in
the task-cache plan-status-archives directory, addressed by SHA-256:
`ca1b7b6fb3b274622760673ccbccb5bbacacc8d7f16ae0748e924b8a70b9b551`.
