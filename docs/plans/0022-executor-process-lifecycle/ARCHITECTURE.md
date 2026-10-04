# Architecture — Plan 0022

> Journal authorization precedes launch; verified containment closure precedes reuse.

## Implementer orientation

Read `CONTRIBUTING.md`, the executor sections of `docs/SPEC.md` and
`docs/EMBEDDING.md`, and this plan's scope before implementing a task. Each
task's **Tests:** block is its acceptance inventory; source changes carry the
matching normative, API and guide changes in the same commit. This plan is a
high-risk persistence and locking change, requiring a frozen review range,
prior-version migration evidence, crash and torn-tail cases, embed acceptance,
zero-dependency checks and the complete applicable native CI matrix.

Do not change `fsm-core` purity, historical hashes, existing request-id
derivations or the one-writer journal guarantee. Keep every file below 1,000
lines by splitting at the execution/ownership/serialization boundaries.

## 0000 — Starting state

- `crates/fsm-execute/src/run.rs` owns one direct child per effect, unbounded
  on-disk capture files with bounded subsequent reads, and detached MCP
  conversation workers; direct-child death is presently treated as sufficient
  to end the run.
- `crates/fsm-execute/src/service.rs::prepare` launches without holding a
  writer. Writer access is acquired later for acknowledgements and outcome
  events. The journal has failed-attempt records, but no durable claim before
  launch and no identity for an execution incarnation.
- `crates/fsm-execute/src/run/pipeline.rs` acknowledges before sending the
  configured outcome event and recovers that event by its derived request id.
  Preserve that ordering and exactly-once journal application.
- `crates/fsm-store/src/journal_io/paths.rs` acquires a short-lived OS writer
  lock. Its PID is diagnostic only; neither PID absence nor the writer lock
  being free proves a previous handler tree is gone.
- `crates/fsm-cli/src/mcp/serve.rs::ExecutorLoop` calls the same service tick
  with its writer. Plan 0020 changes hosting; it must still call the same
  ownership and lifecycle paths rather than implementing a second runner.

## 0093 — Containment and durable ownership

### Native prerequisite

The target is a native containment domain whose identity cannot accidentally
refer to a later unrelated process or tree. It must admit the root before
any user code runs, retain descendants that outlive the root, prevent new
members once closure starts, terminate the domain, and establish that it is
empty and permanently closed. Surviving ownership must be inspectable after
the executor and any helper die. A backend must define what happens when its
own service, kernel facility or required privileges disappear.

The user delegated this decision on 2026-10-04. The selected initial
contained runtime is provisioned Linux/systemd, preserving safe Rust, zero
third-party crates and MSRV 1.89. Other OS families must refuse the new
capability until separately proved; existing six-leg portable Rust coverage
stays mandatory. See `docs/EXECUTOR-LIFECYCLE.md` for authority, trust boundary
and pending proof. This changes the planned platform matrix explicitly and
does not release the native-proof gate.

Task 9301 compared four concrete decision paths:

| Path | Claim it could support | Required decision or proof |
| --- | --- | --- |
| Existing safe standard-library mechanisms | Full claim only if native probes cover atomic enrollment, descendants, death and closure | Compile and execute probes at MSRV and stable on every shipped OS; API resemblance is insufficient |
| Explicit OS-managed supervisor or isolation boundary | Full claim within the specified provisioned environment | Document and approve the runtime prerequisite, its trust boundary, detection and installation; kill the supervisor itself in the probes |
| A narrowly approved charter or platform change | Only the guarantee demonstrated by that new boundary | Separate recorded project decision, precise unsafe/dependency/MSRV/platform consequences, and amended native evidence |
| Direct-child kill, cooperative helper, PID/process-tree scan, inherited advisory lock alone | Cleanup assistance or refusal on uncertainty | Insufficient for the complete target; cannot release the gate or advertise containment |

Do not silently introduce `unsafe`, a dependency, shell utilities, a raised
MSRV, privileges or an excluded OS. Unsupported containment must fail before
handler launch and report the missing capability. A gate can remain blocked
without redefining the lifecycle objective.

### Durable run claims

Add versioned run-claim and run-stopped records through `fsm-store`, with
pure validation/folding in the appropriate core/store representations. A
claim contains a monotonic, store-scoped run identity, instance/effect identity,
attempt identity, immutable handler-contract fingerprint, the retry-policy
snapshot needed to validate subsequent eligibility, containment-domain identity
and native backend identity. Wall time and PIDs are diagnostics,
never exclusion or liveness authorities. Failed attempts and run identities
are separate counters: a crash before an attempt result must not reuse a run
identity or silently exhaust a retry policy.

Only one unresolved claim may exist for an effect. A stopped run whose result
has not been durably consumed by settlement is still unresolved: native death
does not authorize another attempt between the stopped record and its durable
disposition. Claim allocation checks that the effect is still
pending, no active or stopped-but-unsettled run exists, and journaled retry
policy/count/backoff plus the supplied logical time authorize the exact next
attempt, while holding the writer; two executors cannot both succeed. A process-wide
or data-directory executor lease can improve diagnostics, but is not the
safety proof and cannot replace the durable claim.

The launch sequence is:

1. Establish an empty native domain that cannot execute a handler yet.
2. Append and fsync its claim under the writer; verify that the record and
   recovery metadata are durable before authorizing execution.
3. Launch the root atomically inside that claimed domain, with authorization
   bound to the exact run identity. A delayed helper must not act on a closed
   or superseded authorization.
4. Observe outcome, stop remaining members if needed, close further admission
   and prove the domain empty; then persist a stopped record with the bounded
   outcome, or an explicit interrupted/unknown result.
5. Apply the existing retry, acknowledgement and outcome-event pipeline from
   that stopped result using unchanged idempotency derivations; consuming a
   stopped result and committing its ack, failed-attempt or explicit
   interruption disposition is one atomic journal operation, and can happen
   only once for that run identity; interruption leaves the effect pending
   and does not increment the failed-attempt count or invent an outcome event.

A crash after step 1 but before step 2 leaves only an empty, unauthorized
domain. A crash from step 2 through step 4 leaves a claim that prevents another
launch until native reconciliation proves closure and its result is durably
consumed under the retry policy. A crash after step 4
replays settlement from durable outcome bytes without rerunning the handler.
If no reliable outcome survives, a proved-stopped run can be retried under the
declared at-least-once policy; uncertainty about *termination* cannot be retried.
The retry path still consumes that interrupted stopped result durably before
admitting its successor; stop-and-restart cannot bypass a backoff deadline or
consume a failed attempt twice.

Old stores have no run claims, so upgrade has an additional boundary: require
an offline, verified quiescent executor upgrade before enabling claimed runs.
Never infer that pre-upgrade effects are safe merely because the old executor
PID disappeared. The migration retains old journal bytes and machine hashes;
the new store version prevents old binaries ignoring claims. Specify repair,
snapshot, seal/base, archive verification and read-only projections together:
neither archive nor cache reconstruction may discard unresolved ownership.

### Runner contract

Refactor `Runner` behind one production containment path for both process and
MCP handlers. A completed root or MCP response is a candidate result, not
proof of a terminated tree. Before releasing a concurrency slot or returning
a settleable result, terminate lingering descendants, prove closure and reap
the handles the backend owns. If closure fails, retain the claim, report
uncertainty and do not acknowledge success or schedule a retry.

Bound capture while the child runs, including disk usage, not only during
the final read. Define byte accounting, limit and limit-plus-one behavior,
truncation and digest semantics consistently with existing acknowledgements.
Readers may drain and discard beyond a cap without blocking the child; they
must not allocate or spool without bound. MCP readers need an interruptible
termination mechanism so a descendant retaining a pipe cannot strand an
unbounded worker. Native probes must prove that capability rather than assume
that killing the direct server produces EOF.

## 0094 — Shutdown and restart

### Ownership in the tick

Split stop/observation work from start authorization. Stop or enforce an
already-started handler's timeout without waiting for the writer, then acquire
the writer briefly to claim any new run before launching it. Persist results
when the writer becomes available; hold the durable claim meanwhile. Never
turn lock contention into permission to start unclaimed work. An old in-memory
observation must be revalidated during the claim transaction, including an
instance cancellation or effect acknowledgement that arrived after the scan.

All standalone, embedded, public tick and recovery entry points use this
sequence. Stale incarnation results cannot settle a successor's run. Existing
read-only and degraded sessions never acquire run claims, spawn, stop, or
reconcile native work merely by being queried.

### Drain and abort

This lifecycle work must function before plan 0020. Give the current production
stdio entry a minimal independently woken lifecycle pump, with input reading
separated from lifecycle control: external stop, ordinary termination and
already-admitted handler timeouts must be processed while stdin remains open
and quiet. The pump may stop, observe and durably settle already-admitted
runs, but does not launch new pending effects, schedule retries, poll machine
deadlines or introduce autonomous workflow scheduling. Retain one store owner
and bound the input/control queues; a blocked protocol reader or output write
must not be the lifecycle owner's only wake or stop path. Plan 0020 later
subsumes this boundary in its autonomous host without being a prerequisite
for task 9402 or this plan's tests.

Introduce an explicit lifecycle state machine: running, draining, stopping,
stopped or uncertain. Drain first closes admission, waits for current runs
within a caller-selected bound, persists their results and returns a report.
At the bound it enters stopping, closes domains and persists interruptions;
abort enters stopping immediately. An executor shutdown is not an instance
cancellation and does not invent a domain failure event. Interrupted,
proved-stopped effects remain pending for a later sequential attempt.

Expose the same explicit controls to embedders and a local CLI command,
`fsm execute stop --data-dir <dir> --mode drain|abort --timeout-ms <n>`, with a
versioned, owner-only local control protocol; receiving the request must work
while the journal writer is held elsewhere. Authenticate by the OS-local
ownership boundary and bind requests to the live incarnation. Default to
drain for ordinary termination signals only if task 9301 proves a safe
notification path; otherwise record a project decision instead of claiming
that Rust `Drop` intercepts Ctrl-C or termination requests.

Uncatchable kill has no graceful code path: native reconciliation and durable
claims are its proof. A drain timeout or failed native kill returns a bounded
uncertain report with unresolved run identifiers; it never clears the claim
to make shutdown appear successful. Durable claims also protect the gap when
the executor must exit before the writer can persist its already-stopped
results. `Drop` remains best-effort cleanup, never the only correctness path.

### Inspection and reconciliation

Add `fsm execute runs --data-dir <dir>` as a strictly read-only projection and
`fsm execute reconcile --data-dir <dir> --run-id <id>` as the explicit
repair path. Reconciliation closes the identified native domain, proves its
termination and commits a stopped result before retry can be admitted. If
native identity cannot be established, leave the claim unresolved and give
the precise required recovery evidence; do not offer `--force`, elapsed-time
clearance or kill-by-PID as a route around the invariant. A verified whole
containment-environment reset may be a supported proof only if the selected
backend defines a non-reusable environment identity and tests that proof.

Recovery handles all claims whose handlers have since been removed or
changed: native cleanup uses the recorded backend/domain identity, while
settlement must not reinterpret an old outcome using a different handler
fingerprint. Contract mismatch remains explicit until the recorded contract
is available or a separately specified resolution is applied. Expose bounded
summaries in MCP executor health without revealing argv, secrets or raw
captured output.

### Evidence boundaries

Fault tests use real handler trees and an independent observer of their
external markers. Synchronization barriers identify each launch/claim/stop
cutpoint; sleeps are only bounded watchdogs. Include a root that exits before
its descendants, spawning during shutdown, helper death, retained pipes,
unrelated processes and reused identity fixtures, simultaneous executors,
writer contention, disk failure, torn records, archive/seal recovery and
restart while old work remains. Native platform names and exact termination
mechanisms must appear in the evidence; Unix signals cannot stand in for
Windows termination, and cross-compilation cannot stand in for execution.

No test can prove that a remote operation stopped when its local caller died.
Keep domain idempotency keys and compensation in examples and operational
documentation, and assert that the public guarantee remains at-least-once.
