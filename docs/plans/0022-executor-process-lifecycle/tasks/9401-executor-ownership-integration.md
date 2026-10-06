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

Task 9303 has now completed its frozen local and nine-job CI acceptance at
`9f1f175`; ownership integration is opened for implementation using the
projection/capacity design above, with all integrated-host cases still pending.

### Ownership projection implementation review

Scoped source review of `9b32cd9..ea9a17c` confirms that watcher ownership is
cloned from the same read-only Store before instance filtering and that the
scheduler checks the full instance/effect pair before current handler lookup.
The 21 existing watcher tests and public inventory gate pass, the independent
cancelled-owner regression passes with a held writer and cold reopen, and all
21 scheduler cases pass including remote ownership and changed handler lookup.
These results prove projection and start exclusion only; stopped-result scan
fixtures and capacity cases remain pending, and full frozen host/CI acceptance
has not yet been rerun for these integration changes.

Review identifies the remaining capacity defect in `Scheduler::has_room_for`
and `Capped.inflight`: both still count only local entries, and the old comment
assumes orphan children disappear after restart. The next implementation must
count observed owners plus retained local handles, deduplicating only a matched
original run identity; a different incarnation or missing current observation
cannot erase occupied capacity. Production launch still uses the legacy path,
so this slice does not accept writer-protected native routing or settlement.
All-targets executor clippy is running serially with retained log
`~/.cache/fsm-plan-native-matrix-20261005/ownership-projection-clippy.log`.

### Capacity implementation review at `311498f`

Capacity now unions observed `(instance, effect, run_id)` owners with retained
local reservations; only `retain_claim` matching the local effect and immutable
original claim supplies a deduplicating run identity. Unbound reservations
remain distinct, and losing observation retains the local reservation. Global
and per-instance admission and capped diagnostics use that same union.
Scheduler and public inventory tests pass, including a remote owner filling a
one-slot host, matched local/observed overlap allowing the remaining slot, and
missing observation preserving both local reservations. The original projection
all-targets clippy completed successfully; capacity clippy is running serially.
Separate stopped-owner, mismatched-incarnation and per-instance fixtures remain
required before integrated acceptance, along with full frozen host and CI gates.

Current production service inspection confirms the next composition boundary:
`tick_reporting` calls `prepare`, including `runner.spawn`, before `Store::open`,
and clears settled scheduler entries on writer refusal. Native composition must
split stop/observe from starts, retain preparation and execution in Runner,
claim/recheck/start binding under the writer, and preserve original completion
and capacity when settlement cannot obtain the writer. Existing public tick
entries must share that sequence; the current legacy service remains unaccepted.

### Automatic route discovery design

The provisioned authority already publishes immutable Root-owned mode-0444
`store-identity.json` containing physical device/inode, while private
`store.json` retains its path; discovery must use the public identity only.
Add a private Linux client discovery module used by Runner for automatic ticks:
scan only fixed `/var/lib/fsm-containment`, canonical 32-hex namespace names
and canonical positive `authority-<generation>` names, charging at most 4096
total entries across both directory levels before retaining another entry.
Require protected non-symlink Root-owned directory identities before and after
scanning, bounded no-follow regular public identity reads and canonical closed
JSON. Match the actual store directory device/inode, never journal bytes or a
caller-supplied authority path. Zero or multiple matching registrations refuse
new execution; offline or damaged matching registration cannot be silently
ignored to select another generation. For the unique match, validate the current
immutable broker route, boot, operator UID, authority inode and socket identity
using the existing client access contract, and repeat identity checks before
preparation. Discovery grants neither claim ownership nor handler entry.

Keep discovery private: public tick signatures remain shared through Runner,
and the authority continues rechecking physical store and original claim at
binding. Unsupported/unprovisioned hosts refuse automatic native execution;
there is no direct-child fallback. Recovery uses each original claim's route,
not discovery or the current handler table. Required discovery fixtures cover
exact/plus-one inventory and file bounds, missing/duplicate/torn/symlink/writable
registrations, wrong physical store, wrong operator/boot/socket and replacement
between observations; native provisioning cases must exercise the production
entry before accepting automatic routing. This design is not implementation
or execution evidence, and task 9401 remains In progress.

Discovery contract review confirms that current transport validates the helper but has no protected discovery reader; the existing store proof reader provides the no-follow/nonblocking, opened/path identity, bounded canonical-read pattern to follow. SPEC now fixes the shared inventory and document budgets before implementation; discovery remains unimplemented. Capacity all-targets clippy passed with exit code zero.
