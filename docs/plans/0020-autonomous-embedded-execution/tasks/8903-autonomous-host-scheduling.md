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


Current acceptance evidence:

- Frozen `c3f27b90` passes twelve protected process/MCP scheduling cases on stable
  and MSRV in [CI 37922489917](https://github.com/koraytaylan/fsm/actions/runs/37922489917):
  autonomous success, exact injected retry backoff, failure-driven compensation,
  durable-acknowledgement recovery without another RPC or handler entry, and
  writer-only/read-only/contended/degraded construction without fixture entry
  or authority allocation. Large-outbox acceptance preserves global capacity
  two and per-instance capacity one: the quiet instance completes while the
  earlier instance retains its unreleased original tree and 32 pending effects;
  original abort and strict journal verification then pass. Independent
  retained-report digest:
  `c4015df70de63c2bf9fe94426fbd329be0319bb646154e79b3406107159c8428`.
  Reports bind source, compiler, inventory and invocation; executable bytes
  were not independently compared. Native scope is Linux/systemd.
- Bounded original-driver decisions use one logical sample and yield after
  eight turns; retained guard sensitivity is 0/101/0, task-cache digest
  `412c26375075bb38f96e117f029c868f1aef6f1cd731e881ad2d6a8a76ecb04e`.
  Injected independent wait-clock tests preserve ten idle prefixes and fire
  exactly one deadline record only at logical due time.
- Frozen `fe0f5119` shares the authority's inclusive monotonic timeout predicate
  between polling and manager-query classification. The one-nanosecond boundary
  test passes, fails at exit 101 when equality alone is removed, and passes
  after restoration; this supplements genuine authority timeout observations.
  Native preparation retains `i64::MAX` as the scheduler deadline because the
  authority times actual handler entry independently of journal time.
- Frozen `38174050` services 88 reads across four sessions, including an initial
  32-request backlog covering sixteen progressing decision batches; each
  observed prefix advances by exactly eight turns. Existing host/session
  admission limits are preserved. Focused tests, fixture-enabled all-target
  clippy, formatting, size and diff checks pass.

Remaining acceptance: genuine continuously ready completion/application queues
must satisfy both eight-turn bounds together; the complete frozen
integration gate and review must pass. Pure scheduler fairness fixtures alone
do not prove owner wiring. No landing OID is assigned and the original written
inventory above is unchanged.

Historical milestone prose is archived outside the repository by SHA-256:
`7b97be7468f15d9bf316c331ce5ad18f36ebda9c722c8d501fcf84e23c92dc05`.
