---
id: installed-executor-acceptance
title: "Installed Executor Acceptance"
workstream: "0095"
kind: task
depends_on:
  - acceptance-evidence-reports
gated: false
touches:
  - crates/fsm-cli/src/mcp/notify/output.rs
  - crates/fsm-cli/src/mcp/serve/hosted.rs
  - crates/fsm-cli/src/mcp/host/tests/stdio.rs
  - crates/fsm-cli/src/local_control/client.rs
  - crates/fsm-cli/tests/local_executor_control.rs
  - crates/fsm-cli/src/http/endpoint.rs
  - crates/fsm-cli/src/http/endpoint/retirement_tests.rs
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
  - .github/workflows/installed-executor-check.yml
  - .github/workflows/installed-baseline-check.yml
  - .github/workflows/installed-consumer-check.yml
  - acceptance/acceptance.sh
  - acceptance/run-installed-executor.py
  - acceptance/suite/installed.py
  - acceptance/suite/container.py
  - acceptance/suite/native_fixture.py
  - acceptance/suite/scenarios.py
  - acceptance/suite/executor_scenarios.py
  - acceptance/suite/executor_lifecycle.py
  - acceptance/suite/executor_crash.py
  - acceptance/suite/executor_helper_cut.py
  - acceptance/suite/executor_tree.py
  - acceptance/suite/executor_timeout.py
  - acceptance/suite/native_debugger.py
  - acceptance/suite/executor_debug_stdio.py
  - acceptance/suite/executor_debug_http.py
  - acceptance/suite/executor_settlement.py
  - acceptance/suite/executor_control.py
  - acceptance/suite/executor_drain.py
  - acceptance/suite/executor_stdio.py
  - acceptance/suite/executor_supervisor.py
  - acceptance/suite/mcp.py
  - acceptance/suite/fsm.py
  - acceptance/suite/run.py
  - acceptance/suite/evidence.py
  - acceptance/tests/test_executor_observer.py
  - acceptance/tests/test_http_post_observer.py
  - acceptance/tests/test_executor_lifecycle_observer.py
  - acceptance/tests/test_claim_cut_observer.py
  - acceptance/tests/test_helper_cut_observer.py
  - acceptance/tests/test_tree_observer.py
  - acceptance/tests/test_timeout_observer.py
  - acceptance/tests/test_settlement_cut_observer.py
  - acceptance/tests/test_event_cut_observer.py
  - acceptance/tests/test_debug_stdio_observer.py
  - acceptance/tests/test_debug_http_observer.py
  - acceptance/tests/test_executor_control_observer.py
  - acceptance/tests/test_executor_drain_observer.py
  - acceptance/tests/test_stdio_output_observer.py
  - acceptance/tests/test_supervisor_observer.py
  - acceptance/tests/test_baseline_observer.py
  - acceptance/tests/test_installed_inventory_observer.py
  - acceptance/tests/test_notification_observer.py
  - acceptance/tests/test_native_failure_observer.py
  - acceptance/tests/test_container_observer.py
  - acceptance/fixtures/container_init.sh
  - acceptance/fixtures/installed_debugger.py
  - acceptance/fixtures/installed_broker_owner.py
  - acceptance/fixtures/executor_handler.py
  - acceptance/fixtures/executor_workflow.json
  - acceptance/Containerfile
status: in_progress
merged_as: ""
---
# Installed Executor Acceptance

An independently written client must observe the three implementation plans
working together through the consumer-installed executable.

The disposable native fixture and focused dispatch workflow support this task's
installed execution inventory; task 9602 still owns the complete platform and
sustained matrix, and the acceptance inventory below remains unchanged.

**Steps:**

1. Add a discoverable scenario module and generic fixture handlers with
   independent mutation, ordering and maximum-concurrency logs; support both
   subprocess exit outcomes and a minimal stdio MCP tool fixture.
2. Make the existing scenario inventory portable before extending its native
   claims: replace `/bin/true`, `/bin/false` and assumed `python3` commands with
   `sys.executable` and explicit fixture modes, preserving their assertions;
   each existing scenario must run on Windows as well as Linux and macOS.
3. Drive autonomous success, failed prerequisites, partial work compensation
   and failed restoration through stdio and HTTP, using barriers rather than
   sleeps to establish that a handler is still in flight.
4. Stop sending requests after the trigger and observe fixture progress before
   the final read; exercise slow subscriptions, disconnected clients and an
   unrelated responsive control request while a handler waits.
5. Submit incompatible contracts and deliberate manual effects through the
   actual CLI/MCP entry points; assert both the diagnostic and the absence of
   an external side effect before validation succeeds.
6. Exercise the supported shutdown/restart matrix from 0022, verify that
   independent mutation concurrency never exceeds one, then verify/replay the
   journal and inspect exactly-once ack/advance bookkeeping.
7. Add read-only/degraded refusal cases and HTTP session-versus-host lifetime
   cases; report every required matrix cell through the evidence manifest.
8. Test the observer's assertions separately with deliberately faulty protocol
   stubs or event traces: missing progress, a side effect after refusal, and
   overlapping runs must fail; label these harness self-tests explicitly and
   keep the installed candidate unchanged, with production-guard mutation
   proof supplied by the owning tests in plans 0020–0022.

**Tests:**

- `acceptance/acceptance.sh executor` passes all new scenarios against the
  installed candidate; filtering is explicitly ineligible for full release
  evidence until the complete suite also passes.
- `FSM_BIN=<installed-binary> FSM_REPO=<checkout> python3 -m acceptance.suite.run`
  executes the same scenarios natively; this is a proposed recipe, with paths
  supplied by the task's test harness rather than literal angle brackets.
- `python3 -m unittest discover -s acceptance/tests -p test_executor_observer.py`
  rejects faulty stub/trace observations for missing progress, forbidden side
  effects and overlap; these synthetic checks never count as candidate runs,
  and the observer must not compensate by polling or repairing the workflow.
- The pre-existing acceptance scenarios also pass natively on all supported
  OS families without assuming a Unix command path or a PATH-resolved Python.

- **Done when:** complete independent installed-binary reports cover both
  transports and handler kinds, prove all rows in the architecture's outcome
  table, and the separately labelled observer tests reject each faulty trace.
