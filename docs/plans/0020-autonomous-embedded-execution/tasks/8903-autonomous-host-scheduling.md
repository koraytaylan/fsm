---
id: autonomous-host-scheduling
title: "Autonomous Host Scheduling"
workstream: "0089"
kind: task
depends_on:
  - nonblocking-execution-completions
gated: false
touches:
  - crates/fsm-cli/src/mcp/host/
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/sched.rs
  - docs/SPEC.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Autonomous Host Scheduling

Make executor eligibility a function of journal state and supplied time,
independent of whether a client happens to submit another request.

**Steps:**

1. Run the host on incoming commands, completions, control events, and a
   timed wake bounded by the configured executor poll interval; make the
   interval available to embedded serve with the existing validated range.
2. Separate the wait clock from the logical clock and take one logical
   `now_ms` sample per scheduler decision pass; keep time injection intact.
3. Drive retries, due deadline polls, composition, and interrupted outcome
   advances with the current scheduler and pipeline until quiescent or the
   bounded turn allowance is consumed.
4. Enforce the architecture's eight-command/eight-completion fairness
   bounds and preserve existing execution caps and per-instance fairness.
5. Sleep when there is no ready work; do not journal speculative
   `deadline_not_due` polls, busy-spin on a fixed clock, or rescan without a
   bounded scheduled reason.
6. Keep writer-only, read-only, and degraded host construction free of an
   executor; update SPEC and EMBEDDING for host-driven explicit polls while
   preserving the pure core deadline contract.

**Tests:**

- `cargo test -p fsm-cli --lib autonomous_schedule`: private host tests under
  `mcp/host/tests/` use the existing `cfg(test)` harness without widening the
  public API, and with no more commands
  after creation, injected timer/completion events drive success, retry
  after backoff, a due deadline, and compensation to their expected states.
- Exact due time and one tick before it pin timeout, retry, and deadline
  eligibility without relying on real sleeping.
- Continuously ready application and completion queues each yield within
  eight owner turns; an instance with a large outbox does not starve another.
- An idle fixed-clock host performs bounded wakes and appends no records;
  advancing its wait clock alone never fires a logical deadline.
- Reopened interrupted acknowledgement advances without an RPC, and the
  resulting journal verifies; read-only and degraded hosts start no fixture.

- **Done when:** every `autonomous_schedule` case passes with no client request required to drive an eligible executor action, and the stable host gate preserves deterministic core and persistence behavior.


Bounded follow-up decisions now drive the original lifecycle driver through
at most eight ticks with one logical sample, continue immediately after durable
progress or retained readiness, and offer admitted commands between batches.
Five `autonomous_schedule` cases and 54 executable private-host regressions pass;
two genuine native handler cases remain ignored locally. Workspace all-target
fixture-enabled clippy, formatting, size and diff checks pass. The production
turn bound has retained 0/101/0 sensitivity evidence, task-cache digest
`412c26375075bb38f96e117f029c868f1aef6f1cd731e881ad2d6a8a76ecb04e`.
This is preliminary evidence for bounded progress, composition, logical deadline
boundaries and application service; the original native success/retry,
timeout/backoff boundaries, compensation, completion-queue fairness, interrupted
acknowledgement, idle wait-clock and construction inventories still require
explicit acceptance, followed by the frozen integration gate. No landing OID
is assigned and the written inventory is unchanged.
