---
id: executor-ownership-integration
title: "Executor Ownership Integration"
workstream: "0094"
kind: task
depends_on:
  - contained-handler-runner
gated: false
touches:
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/sched.rs
  - crates/fsm-execute/src/watch.rs
  - crates/fsm-execute/src/run/pipeline.rs
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/execution_ownership.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/tests/executor_ownership.rs
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: planned
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

## Integration review before implementation

Reviewed against product source `31e0c6318cb45bcc5f76468d6586e172356e466a`;
this task remains planned while task 9303's current native stable and full
portable gates are pending.

The existing primitives provide the following handoff, but the service does
not yet compose it:

| Host phase | Existing entry | Integration obligation |
| --- | --- | --- |
| Observe ownership | `store.state.execution.unresolved()` | Project claims and stopped results from the same read-only prefix as pending effects, including claims for removed effects; do not filter them through the current handler table. |
| Prepare empty domain | `NativePreparation::start/poll` | Obtain the provisioned original namespace and generation, retain the helper between ticks, and distinguish preparation from permission to execute. |
| Claim and admit | `Pipeline::claim_native_handler`, `NativeExecution::start` | Acquire a healthy writer, revalidate eligibility, durably claim the prepared domain, and start binding before releasing that writer; contention must leave user code unstarted. |
| Observe active run | `NativeExecution::observe/cancel/reap` | Drive owned handles before requesting a writer, retaining original run identity and capacity on uncertainty or unavailable settlement. |
| Apply stopped result | `NativeExecution::settle` | Persist authenticated stopped evidence, then atomically consume it under the original retry policy; preserve completion on refusal. |
| Apply outcome event | `Pipeline::advance_native_settled` | Use the retained original completion only after durable acknowledgement, with the existing derived event key. |
| Recover completion | `NativeExecution::recover` | Read original completion without binding or launching; missing completion retains ownership and requires the reconciliation work in task 9403. |

`Observation` currently carries attempts and executor request keys but no
execution claims, and `Scheduler::on_observation` excludes only local
in-flight runs and those keys before producing starts; both need durable
ownership facts before the service can authorize starts safely.
`service::tick_reporting` currently calls `prepare` before opening the writer,
and its contention path clears scheduler entries for immediate settles;
that path must be replaced with retained claimed execution rather than used
for native completions.

The public tick signatures accept a `Runner`, so changing only `service::run`
would leave embedded and downstream callers on the old sequence; the runner
must own the same preparation/execution state used by all tick entries.
Provisioned route discovery and startup admission still need a production
design: neither a caller-supplied unchecked route nor a direct-child fallback
satisfies this task, and native helper retirement alone never permits claim
release.

This review establishes implementation boundaries, not acceptance evidence;
the race, contention, crash, changed-contract and downstream API cases above
remain unexecuted for the integrated host.

### Ownership projection and capacity design

Reinspection at corrected product source `9f1f175` confirms that `Watcher::scan`
initializes attempts and request keys from one read-only Store, while scheduler
capacity counts only its local `inflight` map. Integrate ownership into that same
scan before any pending-effect filtering: retain original `Claim` and optional
`Stopped` for every `execution.unresolved()` entry, including cancelled instances
and removed effects. Current handler lookup must not decide which owners exist.

The scheduler must exclude owned `(instance_id, effect_id)` pairs from starts,
and compute global/per-instance occupied capacity from the union of observed
owners and retained local handles, deduplicating a local claimed handle by its
original run identity. An uncertain local handle remains occupied even when a
later scan cannot establish its matching claim; missing observation is not
permission to discard the handle. Prepared helpers also reserve local capacity
until delivered or actually retired, but preparation grants no durable ownership.
Stopped owners remain occupied until durable consumption, consistent with SPEC's
owned native host clause; a stopped result is recovery work rather than a new
candidate. This avoids both double-counting a local run and making remote or
removed-effect ownership invisible after restart.

Implement the projection and pure scheduler exclusion first with independent
fixtures for remote ownership, local/remote overlap, removed effects, cancelled
instances, stopped ownership and uncertain retained handles; then compose the
writer-held launch path and original-contract recovery through every public tick.
These are implementation decisions, not executed integration evidence, and the
task remains planned behind task 9303's outstanding acceptance.
