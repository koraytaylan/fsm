---
id: installed-executor-acceptance
title: "Installed Executor Acceptance"
workstream: "0095"
kind: task
depends_on:
  - acceptance-evidence-reports
gated: false
touches:
  - acceptance/suite/scenarios.py
  - acceptance/suite/executor_scenarios.py
  - acceptance/suite/mcp.py
  - acceptance/suite/fsm.py
  - acceptance/suite/run.py
  - acceptance/suite/evidence.py
  - acceptance/tests/test_executor_observer.py
  - acceptance/fixtures/executor_handler.py
  - acceptance/fixtures/executor_workflow.json
  - acceptance/Containerfile
status: in_progress
merged_as: ""
---
# Installed Executor Acceptance

An independently written client must observe the three implementation plans
working together through the consumer-installed executable.

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
