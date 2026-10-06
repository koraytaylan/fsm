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
