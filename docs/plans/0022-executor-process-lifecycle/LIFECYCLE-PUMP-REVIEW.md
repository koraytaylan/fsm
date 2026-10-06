# Bounded lifecycle pump source review

Reviewed source f5ba23e25bb47d9e79ce2c99432b1db76b5b4577; implementation remains pending.

Production stdio serve_session_degraded reads a capped line before ticking;
ExecutorLoop exclusively holds watcher/scheduler/runner, and serve_dir_with
holds the writer for the session. Moving only input to a reader thread does
not solve blocked protocol output: Notifier::send holds its output mutex
through write_all and flush. Lifecycle progress must never take that mutex.
Generic borrowed-input session APIs cannot simply acquire a new Send/static
requirement without reviewing existing callers and public compatibility.

The dedicated production owner should serialize protocol store operations
and executor operations while a separate bounded lifecycle wake/control
channel remains independent of input and output. Control publication must
close admission before waiting for the owner. Report deadlines cannot wait
for a writer lock, protocol mutex, native helper completion or join.
Uncertainty is an outcome, not permission to consume ownership.

Calling service::tick_with periodically is invalid before plan 20:
service::plan calls Scheduler::on_observation, which can emit fresh starts,
retry scheduling and machine deadline polls. Filtering returned directives
is insufficient because scheduler mutation has already reserved admissions.
Likewise NativeOwners::apply can turn a prepared admission into a durable
claim before applying owned completions. A separate scheduler operation must
inspect only existing admitted deadlines; a separate native-owner operation
must observe/cancel/apply existing owners without starting preparations,
claiming queued admissions or launching bound owners. Durable post-ack event
handoffs require an explicit shutdown policy and must not be confused with
remaining executable capacity.

Stop admission must cover local pending preparation, allocation requested,
prepared, published claim, bound-before-entry and already executing phases.
Cancellation of unlaunched bound claims still requires original protected
closure evidence; cannot fabricate NativeCompletion or settle by phase.
Healthy original-writer completion can settle only authenticated original
identity, preserving pending effects for interrupted shutdown. An unavailable
writer must not block closure requests or bounded uncertain reports.

Signal policy is already decided in EXECUTOR-LIFECYCLE.md: SIGTERM/SIGKILL
use default termination and supervisor lease EOF closes domains; no signal
callback settlement or claimed graceful drain. Explicit local controls need
owner-only endpoint access, bounded message size and exact incarnation.

Required first proofs: production quiet-stdin stop; blocked-output stop;
already-admitted timeout with a second pending effect left untouched; writer
contention returning uncertainty; concurrent stop/admission; repeated/stale
controls; original claim survives every unproved closure. No code or public
capability is claimed by this review, and tasks 9401/9402 remain incomplete.

Additional timeout/entry review: Scheduler::retain_native_preparation sets
its local deadline to i64::MAX because the native authority times actual
entry; the pump must observe the original authority timeout rather than
invent a fresh deadline from the current table or preparation timestamp.
NativeOwners::Owner::ready also considers Bound-before-entry ready, and
Owner::apply launches that owner before considering settlement. Merely
excluding the admissions queue from NativeOwners::apply is insufficient:
lifecycle-only operation must separately exclude bound entry while still
allowing original verified terminal settlement. Shutdown cancellation must
explicitly close original bound domains rather than mark them completed.

## Native entry/completion separation

Owner readiness now distinguishes entry_ready from settlement_ready, and
apply delegates its unchanged completion transaction to apply_completion
after its existing bound-entry branch. The completion method has no entry
or preparation path. This is a behavior-preserving private refactor needed
by the future lifecycle-only driver; ordinary apply still admits entry and
must not be used by that pump. No lifecycle control or bounded shutdown is
claimed. Session 50800 passed formatting/source-size, stable/MSRV executor
all-target Clippy and all 29 library tests under verified 1 GiB/no-swap
limits, terminal exit 0; local-native-completion-separation.log is retained.
No persisted bytes change; installed and full changed-source acceptance
remain pending, and task 9402 is not completed by this extraction.

## Cancellation is not a shutdown closure request

Review at 4663bd3: NativeExecution::cancel delegates to NativeRun::cancel,
which stores an uncertain error and calls NativeRequest::cancel; that method
closes helper input and kills the request child, without requesting protected
domain closure or producing a receipt. NativeOwners::cancel therefore cannot
by itself implement verified abort or the expiry of drain. Observing helper
reap/EOF afterward still supplies no closure proof. The new shutdown driver
must retain the original claim while separately requesting native closure.

The existing broker close action fences and completes an allocation but
returns Null, and its wire payload names only the allocation. That success
must not be promoted into claim-matched settlement evidence: receipt recovery
must independently match the original complete domain, claim and journal hash.
Bound-before-entry closure also needs the dedicated unlaunched receipt path
identified in the prelaunch reconciliation review; NativeRun::recover currently
expects a recorded completion. Preserve existing execute EOF containment, but
do not describe helper cancellation as a bounded authenticated shutdown API.
Required tests must distinguish actual protected domain closure from helper
death, including binding, bound, executing and unavailable-writer phases.

## Full stable gate for entry/completion separation

Session 10029 terminated successfully with exit 0. The retained
local-4663bd3-completion-stable-gate.log identifies source 4663bd3 and verifies
MemoryMax=1G / MemorySwapMax=0 before Cargo; observed scope swap remained zero.
Formatting/source-size, full debug and release workspace tests, all-target
workspace Clippy, warning-denied documentation, zero-dependency and complete
embedding acceptance all passed. No runtime source changed during the gate;
documentation-only cancellation review a3e018d was committed during execution.
These host checks prove neither installed shutdown nor production native
selection. The cache-only shutdown request draft is outside this tested source
and remains unintegrated, uncompiled and unaccepted; task 9402 stays planned.

## Provisional claim-bound closure request implementation

NativeShutdown now requests the existing protected broker close operation
from a verified durable snapshot without acquiring the writer. It preserves
the execution transport separately, checks the original physical registration
and authority, shares one monotonic deadline, and returns opaque proof only
after original claim/hash/physical-store receipt authentication. A null broker
reply, helper death or EOF alone never becomes proof. No journal mutation,
binding, entry, event delivery or capacity release occurs in this primitive.

Session 55144 terminated exit 0 under verified 1 GiB/no-swap limits:
format/source-size, stable/MSRV executor all-target Clippy, 32 library tests,
the downstream public API test and all 16 public-surface tests passed;
local-native-shutdown-check.log is retained. Negative tests cover memory-store
refusal without dispatch/mutation, zero deadline before claim lookup/dispatch,
and malformed broker replies. These do not prove actual native closure.
Full changed-source stable and installed positive/negative runtime acceptance
remain pending. The primitive is not yet wired to production or any lifecycle
control: published-before-binding claims, all stop/admission phases, bounded
reports and independent stdio progress remain required, so task 9402 stays
planned and all production acceptance flags remain false.
