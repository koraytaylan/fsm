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
  - crates/fsm-execute/src/containment/crash_matrix_native_tests.rs
  - crates/fsm-execute/tests/lifecycle_platform/
  - .github/workflows/ci.yml
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
genuine handler acceptance remains confined to disposable native CI. Workspace all-target
fixture-enabled clippy, formatting, size and diff checks pass. The production
turn bound has retained 0/101/0 sensitivity evidence, task-cache digest
`412c26375075bb38f96e117f029c868f1aef6f1cd731e881ad2d6a8a76ecb04e`.
This is preliminary evidence for bounded progress, composition, logical deadline
boundaries and application service; the original native success/retry,
timeout/backoff boundaries, compensation, completion-queue fairness, interrupted
acknowledgement, idle wait-clock and construction inventories still require
explicit acceptance, followed by the frozen integration gate. No landing OID
is assigned and the written inventory is unchanged.


Frozen `110e4c21` preserves the original twelve-case completion regression
inventory on stable and MSRV in
[CI 37913138357](https://github.com/koraytaylan/fsm/actions/runs/37913138357),
independent retained-report digest
`f92e22f22985a77c94ce0253f3eac66f54e411642f89b19b10de6fa963693790`.
Frozen `f31cdfe5` passes the separate two-case scheduling inventory on stable
and MSRV in
[CI 37915286721](https://github.com/koraytaylan/fsm/actions/runs/37915286721):
genuine process and MCP handlers complete through the private owner without
a session or further command after creation. Independent retained-report
digest `1c96a0e41f369603f55f50005cf0ff4967df87d7004ff1d9efbb4d7b07c3d726`
binds both reports to the frozen source, compilers, inventory and invocation;
staged executable bytes were not independently compared. The protected
coordinator, producer and scoped verifier also pass 42 mocked artifact/evidence
checks. This scoped success verdict neither changes 8902's inventory nor
completes 8903's remaining acceptance requirements; no landing OID is assigned
and the full integration matrix remains outstanding.


Independent injected wait-clock acceptance drives eleven owner wait boundaries
without a session or a real sleep: ten idle prefixes remain unchanged, logical
time at one tick before the deadline appends nothing, and exact logical due
time appends one deadline record with the original timestamp. The wait clock
and logical clock are separate; idle observations do not repeatedly scan a
complete empty inventory. All 55 executable private-host regressions and
workspace all-target fixture-enabled clippy pass, with genuine native handlers
left to disposable CI. Retry, compensation, completion fairness, recovery and
construction acceptance and the frozen integration gate remain outstanding.


Frozen `25cdc79b` preserves genuine process/MCP autonomous success on both
native toolchains in [CI 37916980391](https://github.com/koraytaylan/fsm/actions/runs/37916980391),
independent retained-report digest
`affbe461cceb900a1783df3ed56d928c812bd172e2af3eff41b4cf1d04b679a8`.
This remains a scoped verdict with no task completion or independent staged-byte
comparison. The separate scheduling inventory now adds process/MCP retry
observers: protected authority timeout must produce an actual attempted
settlement, injected logical time must preserve the prefix one tick before
backoff and admit attempt two exactly at expiry, and the successor must finish
without a session or another command. Frozen `cbce4d03` passes all four scheduling cases on both toolchains in
[CI 37917765237](https://github.com/koraytaylan/fsm/actions/runs/37917765237),
independent retained-report digest
`c479e4812a78e9be68ea4b29a3f735cf519e2a05e2549e03edac249abb3936c1`.
The actual first stopped outcome is timeout, its attempted settlement retains
logical timestamp 2000, time 2009 preserves the complete prefix, and time 2010
claims attempt two; the genuine successor completes without another command.
Protected authority runtime establishes the timeout result; this does not yet
prove the private scheduler's logical timeout boundary. The remaining written
compensation, completion fairness, interrupted acknowledgement, construction
and full integration inventory stays open; no landing OID is assigned and
staged executable bytes remain without independent comparison.


The separate scheduling inventory now includes genuine process/MCP compensation:
a non-retried authority timeout must acknowledge its original outcome, emit a
different restore effect, bind that handler's original fingerprint, and reach
the `restored` terminal leaf without another request. The observer uses bounded
wait rendezvous and the existing protected fixture; both original completion
and crash inventories remain unchanged. Frozen `5779f6e7` passes all six scheduling cases on stable and MSRV in
[CI 37918559352](https://github.com/koraytaylan/fsm/actions/runs/37918559352),
independent retained-report digest
`f6d7a1e4067726679798378b6966676526e7c45575728873a41e8b9ef988693e`.
The genuine timeout is acknowledged, a distinct restore handler completes, and
the final leaf is `restored`; local executable host regressions, producer/verifier
faults and fixture-enabled all-target clippy also pass. This scoped verdict
leaves logical timeout, completion fairness, interrupted acknowledgement,
construction and full integration acceptance open; no landing OID is assigned
and executable bytes were not independently compared.


The scheduling inventory now stages process/MCP acknowledgement recovery: an
original private owner is killed at the protected durable-acknowledgement cut,
strict verification must retain one event handoff with no event applied, and a
reopened owner must apply exactly one event without an RPC or handler relaunch.
Frozen `c7ac31b0` passes all eight scheduling cases on both toolchains in
[CI 37919434760](https://github.com/koraytaylan/fsm/actions/runs/37919434760),
independent retained-report digest
`123ad9e401cd766ddc1db138e29294c22005a1098310fb23cf15d9d802cdd963`.
Both original owners are killed after one durable acknowledgement and before
any event; successors append exactly one event at logical timestamp 2000,
retire the outstanding handoff and do not claim or launch another handler.
Strict journal verification passes before and after recovery. Exact timeout,
completion fairness, construction and full integration acceptance remain open;
native preparation deliberately parks the scheduler timeout at `i64::MAX`,
because the authority times actual handler entry independently of host logical
time. This verified recovery slice assigns no landing OID and makes no
independent staged-byte comparison claim.


The separate scheduling inventory now stages process/MCP construction refusal
through production mode selection: writer-only, explicit read-only, contended
embedded and degraded sessions must disclose no executor, leave the original
ready effect and journal unchanged, and start no genuine fixture. Root separately
requires the original allocation counter to remain zero with no allocation
record. The observer and coordinator compile, and focused host regressions plus
producer/verifier faults pass; runtime verification is pending disposable CI.
The timeout, fairness and full integration inventory remains unchanged.
