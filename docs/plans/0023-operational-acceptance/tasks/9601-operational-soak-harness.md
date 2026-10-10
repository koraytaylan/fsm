---
id: operational-soak-harness
title: "Operational Soak Harness"
workstream: "0096"
kind: task
depends_on:
  - installed-executor-acceptance
gated: false
touches:
  - acceptance/suite/soak.py
  - acceptance/suite/metrics.py
  - acceptance/suite/soak_workload.py
  - acceptance/suite/soak_resources.py
  - acceptance/suite/native_fixture.py
  - acceptance/suite/evidence.py
  - acceptance/tests/test_soak.py
  - acceptance/tests/test_soak_workload.py
  - acceptance/tests/test_tree_observer.py
  - acceptance/profiles/operational.json
  - acceptance/fixtures/executor_handler.py
  - docs/OPERATIONS.md
status: in_progress
merged_as: ""
---
# Operational Soak Harness

Correct isolated scenarios need a duration test that detects stalled work,
accumulating resources and restart failures under a mixed workload.

**Steps:**

1. Implement seeded smoke and sustained profiles with the architecture's
   duration/count floors, finite fault schedule, no-progress watchdog and
   maximum run time; expose the profile, seed and report directory explicitly.
2. Mix retries, deadlines, cancellation, compensation, manual pauses, client
   churn, output floods, contention, archive/reopen and executor replacement
   through the installed executable, retaining an independent expected ledger.
3. Sample latency, scheduler lag and resources per phase; separate retained
   journal growth from leaks and compare equivalent warmed/quiescent states.
4. Calibrate named native hosts with numeric ceilings/tolerances before the
   candidate run and commit the profile; an unavailable required metric or an
   uncalibrated host cannot pass its resource claim.
5. Document invocation, sample provenance, fixture constraints, disk/CPU
   budgets and cleanup; serialize costly runs and use task-owned temporary
   stores and deterministic seed replay for failures.

**Tests:**

- `python3 -m unittest discover -s acceptance/tests -p test_soak.py` injects a
  clock and sample stream to prove both floors, upper deadline, watchdog,
  resource thresholds, missing metrics and incomplete-run classification.
- The smoke profile completes at least 100 cycles and two minutes against a
  real installed binary with valid journal and no correctness violation.
- Injected lost work, overlapping handler ownership, a stuck queue and leaked
  capture/child resources each fail the appropriate independent observation.
- Repeat a failing seed to reproduce the same injected fault order, without
  requiring identical OS scheduling or wall-clock latency measurements.

- **Done when:** the harness passes a real smoke run and its fault/threshold
  tests, with committed calibrated budgets and a sustained profile that cannot
  pass before both eight hours and 10,000 completed cycles.
