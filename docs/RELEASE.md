# Releasing

CLI `journal replay` now detects snapshot disagreement in execution state,
acknowledgement handoffs and complete instance hashes using the store's full
logical comparator, with no persisted format or version changes.

Paired native cancellation now acts on the exact current local claim's cancelled
instance after polling refreshes, preserving original completion, closure and
interrupted settlement instead of waiting for its handler timeout.
No persisted format or version changes.

Sealing now removes dropped request keys from the existing writer and retires
cached outcomes whose records moved to the archive, matching a reopened handle;
subsequent shutdown snapshots agree with the sealed base and live suffix.
No persisted format or version changes.

Local executor stop and observation can reach the unique connected replacement
after a signal-killed owner leaves a refused original socket; stale files are
preserved, multiple connected owners still refuse, and socket refusal grants
no native cleanup or writer-release fact, with no format or version changes.

Degraded HTTP initialization now returns its original JSON-RPC reply instead
of replacing it with the trailing diagnostic; the diagnostic remains available
in the same session's bounded SSE history, with no format or version changes.

Contract-check discovery now fits the existing 38,000-byte tools/list budget
through shorter guidance; schemas retain their constraints, older tools retain
their relative order, and degraded-tool discovery includes independent draft
checking with the normal selector rules. No persisted format or version changes.

MCP `executor_check` now preserves independent definition findings while
unavailable host tables remain explicitly unknown, including effect-free drafts
and degraded/read-only authority; immutable host-session configuration supplies
embedded analysis, with no private literals or approval tokens. Complete task
9202 native/mode/authority acceptance remains pending; report formats are unchanged.

Executor machine checks now reject `--control-dir` with usage exit 2, preserving
the documented separation from execution options; private nested MCP literals
remain absent from compatible/invalid reports and malformed-table diagnostics.

Shared native ticks now admit preparation queues under the original healthy
writer, release unclaimed reservations on contention and keep stop/cleanup
observation serviceable before writer acquisition. Helper startup uses bounded
workers with the existing host budget or a runner-owned fallback; retained queues
remain eligible for writer retry. No persisted format or public signature changes,
and full plan 0021 service/native acceptance remains pending.

Executor outcome delivery now validates its selected original payload/stamps
against the current receiver before sending a derived event request, including
acknowledged recovery after migration; refusal preserves the request key and
does not rerun a handler. Lifecycle suppression, runtime guards and original
acknowledgement ordering remain in place, with no persisted-format change.

Shared service bound-native entry now repeats current contract validation and
requires the original claimed handler fingerprint before requesting execution;
refusal preserves the original owner and entry permission without journal
mutation. Transport-only guard tests do not establish genuine native acceptance,
and plan 0021 admission remains incomplete.

Prepared native work now rechecks its pending contract and original handler
against the current writer and loaded table before claim publication; refusal
retains the original domain for cleanup without consuming an attempt or
changing persisted formats. Final entry, caching and recovery acceptance for
plan 0021 admission remain incomplete.

Prepared native admission now compares the existing full immutable contracts
rather than raw handler structs, so canonical recovery of an unordered retry
set no longer refuses an unchanged loaded table; private command and policy
changes still refuse before claim publication, with unchanged fingerprints and
persisted formats.

Hosted HTTP ordinary tool calls no longer wait behind an unanswered question
in the same session; they reach the existing count/byte admission directly,
preserving cancellation and the question waiter's exclusive reverse mailbox.

HTTP buffered output now bounds aggregate retained response allocation to 8 MiB,
discards overflow, refuses suffix publication and transfers completed bytes
without cloning the entire retained buffer; queue limits remain unchanged.

Hosted HTTP now supplies the bounded notifier required by read-only diagnostic
workers, fixing empty socket responses for journal_verify and preserving owned
frame publication; JSON response buffers are read only after observed drainage.

Production HTTP POST streams now deliver questions before dispatch returns,
with bounded socket-worker output and session-local failure retirement; borrowed
endpoint helpers remain synchronous. Focused native and exact command-saturation
acceptance pass; full integration gates remain required before plan completion.

HTTP SSE now enforces the replay byte ceiling for individual events, refuses
oversized partial line frames without emitting suffix events, and closes live
delivery on replay gaps; production POST uses bounded asynchronous delivery.

HTTP idle-expiry sweeps now retire original protocol/host state, reverse waits
and replay streams together, including sweeps through public session counts.
DELETE uses that same retirement; neither action stops the shared execution
owner or waits for a protocol-state lock. Retired lookups cannot allocate orphan
incarnations, late stream writes refuse, and feed stop does not join blocked I/O.
SSE resume now follows producer/retirement lock order, removing the inverse-lock
wait between replay and publication. Public signatures and durable/wire formats
are unchanged; focused task 9002 acceptance is recorded in its frozen verdict.

Writer-only HTTP routes store calls through one private bounded owner; session
protocol state and cancellation controls are separate, and admission refusal
returns HTTP 503. Real-binary cross-session idempotency verifies one committed
result without duplicate journal mutation. Focused acceptance covers live
ordered output, cancellation, exact command saturation and byte-pressure refusal.

Supported Linux embedded HTTP now retains the existing native owner for the
server lifetime, independently of sessions, and exposes autonomous
`fsm.executor/2` with transport-correct guidance. A real-binary deadline case
deletes the only session, observes completion without HTTP calls, reconnects to
the same writer and retires through authenticated original local control.
Retirement retains the first stop deadline and requires actual native closure,
worker return, endpoint removal and diagnostic drainage; uncertainty reports
observed facts via `exec/inflight_deferred`. Genuine HTTP success, retry,
compensation and active-handler DELETE acceptance pass on the frozen task
landing; full integration gates remain due. Journal formats, error codes and
version numbers are unchanged.
Unsupported production embedded HTTP explicitly refuses before opening or
binding, matching production stdio; those platform axes remain unexecuted.

HTTP fallback now refreshes healthy contended stores without upgrading to a
writer and serves unhealthy-store diagnosis from the original directory in
every session. Real-binary configured-handler refusal cases cover both paths;
these fallback cases do not establish genuine HTTP handler acceptance.

Plan 0022 proves local process ownership and closure, not exactly-once external
effects: remote work submitted before termination can survive local closure,
and recovery can repeat the operation. External effects remain at-least-once;
handlers require domain-specific idempotency keys, reconciliation or
compensation. Release acceptance must not describe a local closure receipt as
proof of remote cancellation or rollback.

Task 9404 provides the portable `fsm-lifecycle-fixture` and dedicated
`executor_lifecycle_crash` target, with genuine Linux/systemd production-host
acceptance in disposable CI. Frozen `66c785ba` has independently verified
60 crash cases, 48 resource observations, 101 containment cases, 34 production
workflow scenarios and two historical upgrade scenarios per compiler in
[CI 37857295898](https://github.com/koraytaylan/fsm/actions/runs/37857295898).
The crash cases exercise standalone and embedded hosts with process and MCP
handlers at claim, enrolled authorization, pre-publication, collected candidate,
supervisor death, domain closure, stopped-record, acknowledgement and event
boundaries, including actual SIGINT, SIGTERM and SIGKILL and torn durable writes.
Independent observers require matching original ownership and verified closure
before replacement; successful original results recover without another entry.
Every supported recovery also verifies the complete journal after the observer
has exited, rather than accepting only a valid prefix with a torn suffix.

Repeated noisy trees test twelve sequential runs for each host/handler pair,
with bounded output and finite descriptor, thread and RSS growth checks;
this is a finite resource test, not an unlimited-run guarantee. Native runtime
scope is the approved Linux/systemd backend; macOS and Windows retain portable
acceptance and unsupported-capability refusal. Default shipped binaries contain
no crash barrier. The lifecycle guide describes the protected test protocol and
retained-artifact verification; plan 0022 STATUS references the scoped verdicts.
All six portable gates and both native jobs now pass at the frozen checkpoint;
the final written-inventory review completes task 9404, as recorded in plan 0022
STATUS. This does not release another production gate or independently verify
executable bytes retained only as digests.

Public native preparation MUST use owned allocations: `start` and `for_store`
request `prepare-owned`, and `poll` returns `NativePreparedOwner`; callers retain
the guard and read metadata through `domain()`. The unprivileged broker and
client MUST refuse legacy `prepare` allocation requests. Existing protected
legacy allocations retain their original recovery rules and gain no ownership
witness. This corrects the unreleased Rust preparation API before native release;
its `poll` return type changes, with no journal format, hash or error-code change,
and task 9403 native acceptance passes for provisioned Linux/systemd;
the complete 9404 crash matrix passes at `66c785ba`.


The additive `NativePreparation::start_owned` / `poll_owned` path now returns
a non-cloneable `NativePreparedOwner` only after acquiring the original operator
lease and checking the protected authority, boot, route and lease inode; `poll` and `poll_owned` both retain the original lease in their returned guard. Production admission now requests owned
preparation, retains its guard across
prepared and uncertain claim states, and moves it into the original execution
owner before dropping the admission reservation; native acceptance passes under task 9403.


Allocator-side `prepare-owned` leases and binding/closure checks are implemented;
absent-binding reconciliation now reuses exact claim validation while holding both
original leases, and production client guard transfer is wired; task 9403
acceptance passes; the complete 9404 matrix passes at `66c785ba`.
Task 9403 acceptance proves continuous guard transfer, public-entry enforcement
and refusal of missing or legacy ownership material on provisioned Linux/systemd.

Repeated `execute reconcile --run-id <id>` and shared service reconciliation now
replay the exact original settlement when verified claim/history and its request
ledger remain available, without targeting a successor or issuing native work.

Native shutdown and reconciliation now retain bounded, sanitized broker refusal
reasons, including an active original runner, instead of reporting every valid
refusal as a response-shape mismatch; malformed envelopes still refuse.

Startup now attempts guarded original-run closure once per observed orphan
without completion, using operator reconciliation's authenticated primitive;
active/missing leases and partial results remain unresolved, and later original
completion recovery stays available after refusal; native startup acceptance
passes under task 9403, with no format or hash change; owned pre-run recovery uses both original leases.

Published original completions in `service::reconcile_run` now use the same
authenticated recovery and original settlement as startup, without current
handlers or outcome-event dispatch; partial publications remain unresolved,
and shared startup orphan closure is implemented; task 9403 acceptance passes.
The complete 9404 matrix passes at `66c785ba`.

Added `execute reconcile --run-id <id> [--timeout-ms <ms>]` for shared Linux
original-run recovery; startup shares its original identity and closure checks.
The client request policy now admits the exact claimed reconciliation action,
fixing a refusal before broker dispatch found by genuine native acceptance.

Task 9403 adds read-only `execute runs` and `service::inspect_runs`, exposing
sanitized unresolved ownership with explicit unverified native state; task 9403
acceptance passes secret/nonmutation coverage, live-owner-safe shared recovery
and genuine orphan/identity/race tests on provisioned Linux/systemd, with no journal format, hash
domain or error-code change.
CLI inspection and MCP health now share journal-derived ownership counts;
native liveness remains unverified and missing observations remain unavailable.
Run inspection also refuses existing uninitialized directories without creating
files, instead of reporting their synthetic empty read-only view as verified.
Added a lease-serialized claimed closure broker primitive that refuses active
or missing runner leases, with additive `NativeShutdown::start_reconciliation`
transport; operator and startup recovery use it, with task 9403 acceptance passing.
Closure-only reconciliation now refuses existing or partial original result
publications, preserving them for authenticated recovery without interruption.
Added Linux `service::reconcile_run` as the shared closure-only writer path;
production startup uses the same guarded recovery, with task 9403 acceptance passing.

The native runner holds an allocation-specific protected execution lease through
cleanup and result publication; reconciliation retains its original lease
through closure, with complete task acceptance pending under task 9403.

OwnedNativeExecutor adds explicit enable_worker_polling; Linux owned stdio
selects it while standalone defaults remain synchronous. Original raw helper
requests move to reserved worker polling for socket exchange, reap and final
transport drop; immutable new startup is also dispatched to that worker,
including protected-helper checks, sockets and spawn, under the original
absolute deadline. Started responses require actual worker join plus helper
reap and both EOFs. Each of 128 pool slots reserves 16 MiB before dispatch and retains
that charge until owner handle and worker retire, with a 2 MiB parsed-response
storage preflight. Joined startup refusal reports not_started separately from
actual child reap/EOF; is_retired recognizes that empty transport without
granting native closure or claim release. Owned startup failures now surface
through polling, while standalone construction remains synchronous. The new
NativeHelperProgress field changes downstream struct literals on this
provisional API; the field and method are inventoried. Receipt verification
for original completion and shutdown now runs on proof workers after actual
transport response collection, reusing the original reservation and deadline.
Response delivery and retirement inventory require actual proof-worker join;
cancellation and expiry suppress returned proof without releasing the original
claim. Those readers capture immutable material and take no Store or journal
allocator. Store-route discovery and current writer-held physical-store checks
remain on the owner; full retained completion storage and worker panic acceptance
remain pending. No wire or journal format,
core semantics, dependency or MSRV changes are introduced.

OwnedNativeExecutor adds has_ready_native_work, a read-only retained-local
readiness query performing no I/O, logical clock sampling or ownership change;
it grants no completion or closure authority. Linux owned stdio uses readiness
after admitted observation to schedule the next writer decision before a long
scheduler timer expires, servicing application commands between decisions.
The provisional executor public-surface inventory records this additive method;
this has no journal, wire-format, core, dependency or MSRV consequence. Real
preparation, bound-entry and completion wake acceptance remains pending.

Linux owned stdio now observes already-admitted native work every 50 ms while
idle, independently of the configured scheduler interval, using the original
driver and one injected logical-time sample with publication deferred through
its return; these observations admit no effects, retries or machine deadlines
and do not reset the eight-command allowance. This fixes long-interval
transport observation without changing public signatures, wire formats,
journal bytes, core semantics, dependencies or MSRV; completion-triggered
wakes, worker isolation and real-handler acceptance remain pending.

Clarify the existing membership-based hosted change feed and correct the elicitation publication fixture to require its single instance invalidation after response admission, retaining durable completed-state and verified-journal checks.

Owned stdio fixes initialization warning and notification handling during owner-response waits and avoids waiting for impossible broken-stderr drainage after original-owner retirement; production stderr lifecycle regressions provide the focused acceptance cases, with broader changed-source acceptance pending.

The provisional native original-outcome advance checks physical store identity before replay or event application; a protected native control replaces the original directory inode after durable Acked settlement and asserts refusal with unchanged journal/state, restoring the original directory before subsequent fixture work, while changed-source compilation and native acceptance remain pending.

Executor tick scan failures no longer stall owned output draining; a Linux regression uses a 4 MiB writer and an independent completion marker, with both public tick regressions passing and both rejecting the previous behavior in a sensitivity run; broader changed-source acceptance remains pending.

Native exec-status association checks its shared two-second deadline before
every accept and hello-read retry, including interrupted I/O; expiry refuses
association without granting entry or releasing the original claim.
This corrects the provisional retry bound without changing formats, hash domains
or public APIs; corrected-source native/portable/frozen acceptance is required.

Native exec-status association now uses a 32-byte ready-kernel-random challenge
sent only through the original manager stdin, and an exact PID/nonce hello;
PID and group permissions alone never authenticate a result. The installed
ordinary Root helper must be 0711, unreadable to the isolated gate, whose private
proc fd directory must be Root-owned and whose status must show no tracer before
and after association; pre-launch inspection requires fs.suid_dumpable 0 or 2
to protect the interval before enrollment. This protects the challenge from unprivileged same-UID
access, including stale dynamic-identity actors; Root tracing actors are outside
the guarantee. MCP startup follows the authenticated hello, and process exec
restores null stdin after consuming the challenge, preserving handler input.
The nonce is neither persisted nor passed to handler argv/environment; the
status descriptor closes before handler exec can restore dumpability. An invalid
nonce retains uncertainty with no grant. This replaces the provisional sender
exclusivity assumption for exec-status authentication; existing private metadata
and journal/receipt/attestation/hash/public response formats remain unchanged.
The unreleased installed-helper permission profile requires reprovisioning and
fresh native/portable/frozen acceptance before production routing.


The provisional native runner adds a private one-shot exec-status stream bound
to its original protected claim and enrolled gate before entry grant. A Root-only
initial directory prevents socket-publication races; only the verified reserved
dynamic group receives temporary traversal/connect access, and the original
socket/listener/directory retire before grant. The gate verifies close-on-exec,
reports only actual exec syscall failure with a bounded private frame, and never
exposes this descriptor to handler code. Existing spawn classification requires
this report plus full closure; stdout, stderr and reserved exit codes cannot
select it. Separate private `fsm.native-exec-status/1` metadata binds original
socket/directory identities for conservative crash cleanup; journal, receipt,
attestation, public response and hash formats remain unchanged. This unreleased
boundary requires fresh native faults, portable gates and frozen review; it does
not release the production gate or finish task 9303. The inherited-input nonce
and nondumpable-gate controls above replace the initial sender-exclusivity
assumption; native execution and frozen acceptance remain required.


Authenticated broker close now shares the runner's original-domain fencing:
when matched manager stop refuses, it attempts verified original kernel
freeze/kill while preserving that refusal and requiring independent complete
closure before returning success. Kernel submission never proves manager
retirement, closure or reusable capacity, and damaged protected handoff remains
unresolved until exact original proof is available. The native fault control
requires durable revocation, original-domain depopulation or actual absence,
unchanged damaged handoff and no fabricated manager completion or closure.
No journal, receipt, attestation, hash or request-format bytes change.


Broker lifetime leadership and startup now validate protected configuration,
authority/boot and lock identity independently of the current handler catalogue;
provisioning still requires approved catalogue validation. A missing or malformed
catalogue therefore permits authenticated original-result recovery, including
broker restart, while new allocation/binding/execution retain their existing
current-catalogue checks and refuse unavailable approval. Native fixtures require
original recovery through missing and malformed catalogue faults, unchanged
allocation counters on refused preparation, live broker retention and restarted
recovery without the catalogue. This repairs the broker access failure in CI run
37339071702; current full native/portable/frozen acceptance remains pending.
No journal, closure, broker configuration, epoch or attestation bytes change.


A present verified original cgroup retains an unconditional admission-revocation
path even when protected handoff corruption prevents matched manager stop;
absent-domain revocation still requires the verified handoff and original manager
policy. On stop refusal, the native runner additionally attempts original-identity
kernel freeze/kill before transport retirement, without writer access and without
claiming closure, manager retirement or reusable capacity. Replacement identities
still refuse; later closure still requires exact original handoff and independent
full retirement proof. This repairs the corrupted-handoff control failure in
CI run 37336060083; full native/portable/frozen acceptance remains pending.


After durable entry revocation and exact manager unit/job retirement, submitted
native closure may remove an original protected residual empty cgroup using a
bounded exact no-follow population sample and repeat identity/manager checks;
kernel removal must succeed and actual absence is still required before receipt
publication. Replacement, population, unknown child cgroups or failed removal
retain uncertainty; cleanup never recursively removes domains. This addresses
CI run 37334952656's independent systemd exit probe, which observed an unloaded
unit with the original empty cgroup still present after three seconds. The probe
now performs the same matched residual cleanup, retaining actual absence and
unrelated-process controls. Current native/portable/frozen acceptance remains
pending; historical journal and closure bytes are unchanged.


Native launch retains invocation-matched root status with `RemainAfterExit=yes`
and `CollectMode=inactive`, without `systemd-run --collect`, until Root performs
matched stop after durable entry revocation; fast success, failure and signal
exits cannot use launcher status as a substitute. Stop accepts an absent original
cgroup only after verifying the protected completed handoff and unchanged manager
invocation/security policy; any present replacement cgroup refuses. Root clears
retained failed status only after stop and exact original invocation/ExecMainPID
matching. Stop completion remains separate from closure, which still requires
actual cgroup absence, unloaded manager unit, no queued job and helper retirement.
Native fixtures exercise fast true/false/signal handlers and reject closure while
a successful exited unit remains retained; current compiled, native, portable and
frozen acceptance remain pending under the unreleased provisional boundary.


NativeCompletion now additionally authenticates the full bounded successful
response using separate immutable Root-issued result attestation; a matching
closure receipt alone cannot authenticate candidate output or its failure class.
Automatic native host recovery now waits for the protected completed response
instead of spending its one request while the predecessor is still running;
Root stages and syncs the existing response before atomic final-name publication.
All original completion and writer-held settlement checks remain required,
without launch fallback, ownership clearance, request replacement or deadline
renewal when requested recovery fails.
After proving closure, Root publishes `result-<allocation>-<run_id>.json` as a
bounded canonical Root-owned 0444 object with exactly `format` (the private
`fsm.native-result-attestation/1` tag), `domain`, `run_id`, `journal_claim` and
`response_hash`; the last is SHA-256 over the entire response in the new private
`fsm:native-response:1` hash domain. No argv, contract or captured output is
published in this attestation, and historical closure/journal/hash bytes remain
unchanged under the unreleased provisional native boundary.
Publication creates once, syncs the file and its protected parent, then makes
and syncs its immutable public mode before checking NativeCompletion and writing
the existing private completed response; crashes before full publication refuse
recovery rather than inferring a candidate. Verification checks protected location,
authority identity, original run/claim and exact response digest without repairing
missing/torn/writable evidence. Older unattested native responses refuse.
Native fixtures require changed captured output to fail attestation despite a
matching original closure, plus missing/torn/symlink/writable attestation refusal and
unchanged fixture-owned bytes after restoration; current native/portable/frozen
acceptance and automatic production routing remain pending.


The provisional `Pipeline::claim_native_handler` derives instance/effect identity
from journal replay, validates the original HandlerSpec and takes its fingerprint
and retry snapshot from that same contract under a healthy durable writer.
It rejects a mismatched effect name before claiming any request key and returns
only a matching currently owned Claim with its original record hash available;
an already consumed claim is not recreated from an old duplicate response.
It starts no helpers: prepare the native domain, acquire the writer, claim, then
use NativeExecution::start while the writer still protects launch eligibility.
Root binding still rechecks its approved catalogue; refusal retains unresolved
ownership and permits no direct-child fallback. Existing formats, claim/hash
bytes and retry/ack/event semantics are unchanged, and automatic service routing
plus compiled/native acceptance remain pending.


The provisional `run::native_client::NativeExecution` owns the original native
run and retains checked completion across writer contention; `observe` polls
bounded helper work without a Store and repeatedly reports cached readiness.
Its bounded progress exposes only phase, optional helper retirement facts and
whether the host retains capacity, never identifiers, contracts or captures.
`start` uses the writer-held pipeline check; `recover` uses a durable snapshot
and never launches, while `from_completion` adopts independently checked original
material without spawning helpers and grants no current ownership.
`settle` requires a healthy durable writer and matching physical store, persists
stopped evidence when absent, consumes it with the original retry policy, and
releases its retained slot only after durable success or exact original request
ledger replay; any refusal preserves completion and unresolved capacity.
An existing stopped result must match; a different/newer owner never authorizes
new stopped writes for this original run, and replay does not select policy from
a changed table or create unused request keys.
Outcome events remain a separate `Pipeline::advance_native_settled` step after
acknowledgement; journal/receipt bytes and existing ack/attempt/event keys remain
unchanged, with the provisional stopped key derived from effect and run identity.
This component is not yet automatic routing for service, scheduler or public
ticks, and full native/portable/frozen acceptance remains pending.


Native closure and legacy-quiescence file proofs now retain the physical store
identity from the protected authority's immutable `store-identity.json`;
new stop/admission writes reject a different directory device/inode with
`store/execution_evidence`, including a byte-identical copied journal.
Registration publishes this separate root-owned 0444 bounded canonical
`fsm.native-store-identity/1` record containing only `format` and the registered
`identity` (`device`, `inode`); the private registration path remains private.
Missing, torn or mismatched identity metadata refuses without repairing it.
Existing journal/receipt formats, hashes and historical bytes are unchanged;
older provisioned authorities lacking this metadata require operator-reviewed
reprovisioning and cannot silently infer it from the caller's store.
This provisional native boundary fix retains the unreleased API/version scope,
and full compiled/native acceptance and production host routing remain pending.


The provisional `run::native_client::NativeCompletion` now checks a successful
broker response against the original full claim/hash, closed result envelope,
canonical derived receipt path and existing failure classes, preserving the
acknowledgement candidate unchanged. It reads protected opaque evidence and
requires exact run/domain/hash matching before exposing candidate/class/proof;
this still writes no stopped result and permits no settlement or capacity release
without the store's current-ownership recheck. Pure mismatch controls and the
actual native broker receipt case exercise this validator; compiled/native
acceptance and production service wiring remain pending.

Opaque native closure evidence now offers read-only `matches_claim`, comparing
run ID, complete native domain and original journal-claim hash before the
executor accepts a candidate. This does not establish current ownership or
change journal bytes; the store's stopped transition still verifies current
ownership under its writer lease, and matching alone permits no settlement,
retry or capacity release. Pure preauthenticated fixture controls cover matching,
wrong hash/run/allocation and replaced authority/cgroup identity without journal
mutation; compiled acceptance and executor result/service wiring remain pending.

Process-root observation now distinguishes a manager-query deadline from
identity and other inspection failures: if the approved handler deadline has
actually elapsed, that specific query deadline selects the existing timeout
candidate and still requires full matched closure and owned-handle retirement.
An earlier query timeout, mismatched identity or other inspection error remains
uncertain. This repairs a native short-timeout race without inferring root exit
or relaxing proof, and changes no public error code or journal format.

The provisional Linux `run::native_client::NativeRequest` supervisor now owns
the fixed protected transport helper and nonblocking standard-stream sockets.
It bounds request/response/diagnostic retention and work per poll, enforces a
checked caller deadline, and collects a canonical response only after successful
actual child reap and both stream EOFs. Cancellation or transport/deadline failure
requests helper death and retains explicit reap progress; none of these proves
handler closure or releases a journal claim. Drop is bounded best-effort only.
The public inventory includes its start/poll/cancel/reap methods; pure framing,
policy and retained-writer controls are authored, while native supervisor and
claim/closure-matched production service integration remain pending.

Native broker access and client-death controls now exec the installed Rust
`client` helper after dropping root UID/group privilege, replacing Python
transport for valid requests. Access checks decode its framed responses and
require helper policy refusal without counter mutation; independent denied
socket access remains a direct kernel EACCES control. Before killing a helper
with enrolled process/MCP descendants, the observer additionally verifies its
actual operator UID and installed executable inode. These Rust-helper controls
await compiled native acceptance; the public parent supervisor/service remains
incomplete.

The provisioned authority binary now has a private unprivileged `client`
transport helper: it validates its actual operator UID, current boot, protected
read-only route, authority inode and exact socket identity/access before sending
one bounded canonical request and after receiving one bounded canonical response.
The execution host must own and supervise this helper process and its streams,
imposing startup/request/shutdown deadlines; potentially blocking Unix connect
stays in the killable helper process rather than a detached connection thread.
Helper death closes its broker connection and requests verified cancellation,
without releasing a journal claim. Public supervisor/client/service wiring and
compiled native helper acceptance remain required.

Native broker disconnect controls now kill an independent unprivileged client
after checking actual process/MCP root, child and grandchild membership and
identities while inherited streams remain held. The installed broker must stay
alive, revoke admission and publish matching closure plus manager-stop evidence,
remove the native group and return to a single thread within eight seconds,
before the ten-second handler timeout. Journal ownership remains unresolved and
unstopped and duplicate execution refuses; these controls await compiled native
acceptance and do not complete public client/service integration.

Native broker review now includes the installed production authority binary
and independent Python clients that drop supplementary groups and both root
UIDs before reading the protected public route and connecting. Controls require
operator preparation/binding/execution with matching closure evidence and
retained journal ownership, denied access for another UID and a handler-range
UID, fixed-policy refusal, exclusive leadership, monotonic restart with stale
socket identity preserved, and refusal of missing/rolled-back counter or missing
epoch history. This is authored native coverage awaiting stable/MSRV execution,
not completed broker/service acceptance.

Broker provisioning now applies 0755 explicitly to its newly created directory
and refuses existing ancestors without operator traversal; registration does
the same for its own newly created namespace/authority directories, preserving
existing namespace permissions. This avoids umask=0077 filtering away required
traversal. The chown sentinel UID is refused and socket type, final operator
owner and exact 0600 access are verified before route publication; native
acceptance remains pending.

Private broker transport is implemented for review: root `provision-broker`
binds one operator UID outside the handler identity range, and `serve` holds
exclusive lifetime leadership, burns durable socket epochs, preserves stale
sockets and publishes a read-only route after applying operator-only access.
Connections use bounded canonical length-prefixed JSON with a fixed action
policy and at most eight owned sessions; execution delegates to the claimed
runner and treats client EOF as cancellation, retaining worker ownership until
cleanup returns. Missing/changed authority refuses operation, and no broker
response settles journal ownership. Protocol controls are authored; compiled
native authentication, restart, disconnect, client/service wiring and host
shutdown/recovery acceptance remain required.

Native runner fault controls now corrupt only fixture-owned protected handoff
material after independent process/MCP tree enrollment and request cancellation.
The production runner must return cleanup uncertainty, preserve durable closing
and unresolved journal ownership, and publish no closure receipt or domain
closure record; duplicate execution remains refused. Restoring the exact fixture
fault then completing independent native closure cleans the test domain without
turning the failed execution into a successful result; compiled acceptance is
still pending.

Private claimed execution now accepts an explicit shared cancellation control:
an observed pre-launch request refuses launch and retains the claim, while
an in-flight request selects existing `exec/cancelled` semantics through the
same descendant termination, matching closure proof and owned-handle cleanup
path; this adds no stable API/error or journal version, and authenticated
broker/service wiring remains incomplete. Native controls cover process and
MCP cancellation after independent child/grandchild enrollment, plus
pre-cancelled launch refusal; compiled acceptance remains pending.

Unreleased private root `execute` adds one claimed enrolled execution/cleanup
path for process and MCP handlers, with approved journal-derived invocation,
shared bounded live capture, independently cancellable/joined protocol I/O,
matching closure proof and owned transport retirement before returning a
candidate result. Provisional Linux native I/O adapters expose the shared
implementation; native process boundary/timeout controls are authored, while
native MCP/tree, broker and production service acceptance remain pending.
Native MCP review controls now exercise the production authority path against
a lingering server and inherited child/grandchild protocol streams, with
independent enrollment barriers, error-answer capture/digest checks, timeout
cleanup, matching closure evidence, retained journal ownership and duplicate
launch refusal; this is test coverage awaiting native execution acceptance.
Process-root observation now matches manager invocation and original gate PID
to canonical exit information even while descendants keep the transport alive,
then performs the same full cleanup/proof path. Native review adds an early
root-exit tree case and preserves strict argv-template syntax by expressing
fixture Python dictionaries without reserved brace characters.

Unreleased private `complete-close` adds protected matched-stop retirement,
domain tombstone and immutable closure receipt publication with exclusive
fsynced pending/hard-link ordering and identity-preserving replay; missing,
mismatched or partial material retains ownership rather than fabricating
closure, and full contained runner/native acceptance remains pending.
Natural handler exit can now complete this retirement protocol without a
live cgroup stop request, with distinct retirement acknowledgement, preserved
claim ownership and startup verification of lifetime/kill/no-restart policy.

Unreleased `request-stop` verifies protected handoff, actual prepared domain
and current manager invocation/isolation policy under the authority lock,
revokes admission before bounded replacement stop, and retains durable claims.
Its new native control stops an approved running handler without producing
closure evidence; full runner and permanent closure acceptance remain pending.
Matched successful stops now publish an exclusive bounded protected completion
record after the manager operation, preserving its identity across authority
death without claiming that permanent closure has been established.

Unreleased launch now refuses existing entry or pending authorization of any
file type before reserving intent, preserving verified gate handoff ordering.

Unreleased startup now retains the authority lock through bounded verified
manager/gate handoff and fsyncs its protected binding/diagnostic record before
returning transport ownership. Failed startup revokes admission under that
lock and retires the owned transport; claims remain unresolved. Derived-group
authorization requires a matching handoff and syncs exact replay before
publication. Native missing/changed-handoff controls and a shared-deadline
refusal check remain pending compiled CI, without full runner acceptance.

Unreleased private root `launch` now fsyncs single-submission intent before
starting the installed gate through a fixed protected manager launcher.
Isolation, direct owned pipe streams, environment clearing, approved runtime
ceiling and bounded command monitoring are enforced; duplicate/partial intent
refuses another submission. Failures retain intent and durable ownership.
The positive native gate case now calls this production startup path, while
complete broker/runner I/O and permanent closure remain unaccepted.

Unreleased root `authorize-enrolled` checks the installed gate's actual
manager/proc identity and derives immutable grant access without a caller
group override. A new native case requires the real production gate under
DynamicUser, unchanged durable ownership after approved handler exit and no
closed receipt. CI installs only its freshly built binary exclusively at the
fixed protected path and removes only the matched owned inode/digest; existing
installations refuse the fixture. This remains partial backend evidence,
pending compiled native acceptance and complete runner/closure integration.

Unreleased native catalogue approval precedes allocation and binds grants to
the replayed effect's approved full handler contract, retry and substituted
argv. Root-protected source/provenance/freshness checks prevent silent hot
reload or reconstruction of lost authority; record byte/depth limits remain
enforced. Broker authorization, actual enrollment, I/O and closure integration
are still required before shipping contained execution.

Unreleased pure `HandlerSpec::fingerprint` supplies full canonical contract
identity for future claim/privileged-catalogue integration; it includes command,
MCP tool/templates, timeout, outcomes and normalized retry policy. The new
domain changes no historical hash, and hashing alone grants no execution.
Public inventory, independent digest/default/field-binding checks and the
full applicable release gates must pass before publication.

Unreleased root-only `observe` adds bounded read-only cleanup progress with
domain/phase revalidation and no lock/record creation. Empty population is
not closure; manager fencing, verified permanent closure and shared runner
integration remain required before contained execution can ship.

Unreleased production preparation requires a bounded read-only query proving
access to the active system slice before burning allocation intent/counter.
Manager failure, malformed/excessive output or incomplete I/O refuses;
no journal format or release version changes, and complete native acceptance
is still required before shipping contained execution.

Unreleased root `request-kill` retains the authority lock through durable
revocation and matched-cgroup freeze/kill submission. Successful submission
does not establish native closure; manager fencing, completed inspection and
closure receipts remain required before contained execution can ship.

Unreleased root-only `begin-close` publishes a durable matched-domain closing
marker before revoking grants, with exact replay and ownership/type refusal.
It neither terminates the domain nor issues closure evidence, and preserves
journal claims; manager fencing and complete native shutdown remain required.
Closing replay syncs the existing marker and parent before removing grants;
unexpected grant types leave closing admission fenced and require repair.

Unreleased protected entry verification requires an unprivileged handler
identity, immutable root-owned claim grant and actual boot/authority/cgroup
enrollment before exec, with no caller-provided command authorization.
Shape/refusal checks do not establish real launch or tree closure; privileged
native launch/I/O and closure integration remain required. Root-only grant
publication now revalidates the protected binding and current claim, refuses
closing/closed allocations and group zero, and exclusively publishes a synced
root-owned 0440 grant. Trusted broker group selection remains outstanding.
An enrolled gate now waits up to five seconds for publication and refuses
closing/closed markers during the wait and before exec; real DynamicUser
launch and closure fencing remain required for complete acceptance.

Unreleased native authority work adds a separately provisioned root-only
registration/binding binary with bounded canonical protected records and
independent read-only verification of original durable claim identity and
current pending ownership. Binding never starts a handler or publishes
closure evidence. Production broker, protected entry and runner
closure integration remain incomplete; the added binary does not enable or
advertise contained execution, and no package version or release tag changes.

Native preparation now burns an allocation intent/counter durably before
creating an empty cgroup and recording its actual identity. Counter authority,
boot, history and unknown-domain checks refuse instead of recycling an
incomplete allocation. Unit names include generation; the initial limits are
4096 lifetime allocations per generation and 32768 inventory entries. The
provisioned native CI inventory adds five production allocator/binding cases;
their execution is required and does not establish complete runner closure.

Unreleased Linux runner capture now drains nonblocking sockets on each poll,
without output spool files or capture-reader threads, keeping at most 4 KiB
per stream and hashing at most 1 MiB. EOF is required for a whole-stream
digest; retained peers yield an incomplete prefix within bounded final drain
work. Other portable hosts retain file capture. Embedders must continue
polling while handlers run. This changes capture transport, not execution
ownership: direct-child completion and the existing MCP worker still do not
provide the planned contained-runner closure guarantee.

Unreleased Linux MCP workers now use independently cancellable stdin/stdout
sockets and tracked join handles. Results wait for an observed worker join;
cancelled workers still closing remain retained and prevent another launch
until joined. Drop remains nonblocking best-effort cleanup. This prevents
retained I/O peers from forcing reliance on root death for worker cancellation,
but does not prove native domain closure or ship contained execution.

The current format documentation consistently identifies VERSION 11,
state-root/4 and snapshot/6; VERSION 10 sealing rules are explicitly historical.
This clarification changes no persisted bytes or public behavior and does not
complete the pending claim-era acceptance or contained runtime integration.

Unreleased writer-lock release now explicitly unlocks through one internal
guard shared by open, initialization and repair, including error returns.
A deterministic Unix duplicate-descriptor regression reproduces the previous
retained-lease defect and checks that a replacement writer remains exclusive.
This changes no public API, error, persisted byte or format; the complete
portable matrix must validate the fix before release.

Unreleased execution ownership preparation now rejects stopped results that
would exceed the persistence parser's depth after adding its outer envelope;
the exact 63-container block limit remains accepted without partial mutation.

Unreleased preparation adds pure exclusive execution ownership, immutable
stopped results and durable retry eligibility with bounded canonical codecs;
the production store now writes VERSION 11 with claim/stop/settle/enable
records, root/4, snapshot/6 and base/2. Historical roots and authoritative
base/1 decode under their original formats; legacy stores migrate unchanged
and remain execution-quarantined. A separate claim-hash root authenticates
original unresolved claim records after sealing. Task 9302 persistence
acceptance passes its frozen host and complete portable/native CI gates;
native authority publication and runner integration remain downstream work,
so this does not complete the lifecycle plan or ship contained execution.

Unreleased plan 0022 preparation adds pure, constructor-validated native
identity and retry-policy values under `fsm_core::record::execution`, with
external embedding coverage. The claim-era format and stable execution errors
require their own migration/crash/seal and native-proof evidence and a breaking
minor release before shipping; native feasibility does not waive these
requirements, and this work does not change the package version or create a tag.

Releases are cut from `develop` and driven entirely by pushing a tag.
[`.github/workflows/release.yml`](../.github/workflows/release.yml) is
authoritative for what happens next; this document covers the decisions and the
manual checks it cannot make for you.

The tag is the only irreversible step, and it is irreversible for a specific
reason: [`API-POLICY.md`](API-POLICY.md) promises that a published tag is never
moved or deleted, because library consumers pin it. There is no crates.io
publish to undo — a git tag *is* the distribution artifact — so a mistake found
late is superseded by a new patch version, never by rewriting the tag.

## Pending executor and MCP fixes

- MCP clients can read `fsm://executor` for execution mode and the loaded
  handler contracts, and `fsm://docs/embedding` for setup. Prompts explain
  how to drive embedded execution through completion and failure recovery.
- Embedded startup that falls back to read-only no longer launches handlers;
  fallback readers refresh their journal view on requests.
- Acknowledgements without outcome events no longer consume the executor's
  recovery window and hide interrupted advances.
- Integration coverage exercises capability discovery, sequential command
  execution, prerequisite failures, and compensating actions through the
  actual MCP server and executor.

These changes add no Rust dependencies and change no journal or state hash
format.

## Upgrading from v0.1.0

`v0.2.0` is a breaking release under the pre-`1.0` rule in
[`API-POLICY.md`](API-POLICY.md): with a nonzero minor, the minor is the Cargo
compatibility boundary, and this one moves. Nothing was removed from the
`fsm-core` or `fsm-store` public API, but types a downstream matches
exhaustively gained fields and variants, `TransitionSpec.on` became optional,
and the persisted formats moved. The complete list is the migration list in
[`API-POLICY.md`](API-POLICY.md). Migration paths from the untagged builds that
preceded `v0.1.0` are in that tag's own copy of these two files, which is where
they stay.

**What is new.** Machine composition — a state invokes another machine and
reads its result through `$done.invoke.<slot>`, and one instance signals
another. A bounded run-to-completion macrostep: eventless transitions,
internal events from `raise`, and generated done events, sealed in one record.
Journaled, idempotent definition migration under a `supersedes` block, with a
preview and a cohort command. The standalone `fsm execute` effect executor,
with journaled retries, deterministic backoff, bounded concurrency, and a
handler kind that calls another MCP server's tool. A Streamable HTTP transport
for `fsm serve`, with sessions and server-sent events. And ten more MCP tools —
24 in total, up from 14 — including the five audit capabilities that were CLI
only.

**Upgrading a store.** The instance state format moves to `fsm.state/3`, which
adds the composition fields, and the on-disk store to `VERSION` 9. A `v0.1.0`
store is at `VERSION` 8 and is migrated on **first open**: the complete journal
is folded using each record's own `state_format` discriminator, and the marker
is stamped forward on success. Interior records are never rewritten and old
hashes are never recomputed under the new format, so a record written by
`v0.1.0` keeps its `fsm.state/2` identity forever, and `journal verify` still
checks it under that format. A fold that fails refuses the open and leaves
`VERSION` untouched. Stores at `VERSION` 1 through 8 are all accepted this way.
Snapshot caches move to `fsm.snapshot/5`; a `fsm.snapshot/4` file beside a
current journal is skipped and the state re-derived, because a snapshot is a
disposable cache and bumping its format is never a data-loss event.

**Definitions written for `v0.1.0` still compile**, and every `machine_id` they
hash to is unchanged: the new spec fields are optional, and a definition
without them canonicalizes exactly as it did. Adding a `supersedes` block
produces a new machine rather than changing an existing one, because the block
is inside the canonical bytes.

**New error codes, no changed ones.** The `def/`, `req/`, and `run/` families
gained codes for composition, reactive semantics, migration, and the executor.
Adding a code is compatible; no situation that returned a code in `v0.1.0`
returns a different one now.

## Before tagging

- The CI matrix is green **on the exact commit you intend to tag**, not merely
  on an earlier commit in the branch. It covers the Linux, macOS and Windows OS
  families at stable and the minimum supported Rust version; target-specific
  release binaries are additionally built and smoke-tested on their matching
  runner. A local run on one host cannot stand in for that matrix.
  `rust-toolchain.toml` pins 1.89.0 locally, so a plain `cargo test` never
  exercises stable at all.
- `acceptance/acceptance.sh` is green against the candidate build. It builds
  its own image from this tree, so run it after the version bump, not before.
- `manual:` live-model acceptance has been run against the candidate build.
- `manual:` if the `fsm-core`, `fsm-store`, or `fsm-execute` public API
  changed, the version bump matches the semver rules in
  [`API-POLICY.md`](API-POLICY.md). Before `1.0` the minor is the breaking bump
  and the patch is the compatible one, so a release that adds a field to a
  public struct, a variant to a public enum, or moves a persisted format
  advances the minor.
- Release notes are generated from the conventional-commit history by
  `cliff.toml`, so the commit messages *are* the changelog. Write them for a
  reader of the release, not for the diff.

## Checks the pipeline runs for you

Every line here is executed by `release.yml` on the tagged commit. Run them
locally first if you want the answer sooner.

### Gate

- `cargo fmt --all -- --check`
- `cargo test --workspace --no-fail-fast`
- `cargo test --workspace --release --no-fail-fast`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo doc --workspace --no-deps`
- `cargo build --manifest-path fuzz/Cargo.toml --bins` — the fuzz crate is a
  separate workspace, so nothing else compiles it. Linux legs only: the
  targets are `#![no_main]` and libFuzzer provides the entry point, which the
  MSVC linker will not (`LNK1561`)

`clippy --all-targets` since plan 0019: the test targets' lint debt is paid
and test code is now held to the same lints as production code. The command
appears in `CONTRIBUTING.md`, `ci.yml`, and `release.yml`; change all three
together or the gate a contributor runs stops being the gate that ships.

### Supported consumers

- `cargo test -p fsm-embed-acceptance` — the library loop from a crate that
  depends on `fsm-core` alone.
- `cargo tree -p fsm-embed-acceptance` shows `fsm-core` and nothing else.
- `cargo test -p fsm-cli --test zero_deps` — the resolved graph contains only
  this workspace's own crates.
- The `git-dep` job builds a scratch crate against `git = <repo>, tag = <tag>`.
  This is what `cargo publish` would be for a registry project: proof that the
  instruction in the README resolves and compiles against the tag being cut.

### Version stamping

- Workspace tests pin `fsm version` and MCP `serverInfo.version` to the workspace
  package version.
- After changing the root manifest version, regenerate every byte-exact
  fixture that carries `serverInfo.version`, then rerun each test without its
  environment variable:

  ```console
  $ REGEN_SKELETON=1  cargo +stable test -p fsm-cli --test mcp_skeleton
  $ REGEN_MCP_FULL=1  cargo +stable test -p fsm-cli --test mcp_full
  $ REGEN_MCP_LIVE=1  cargo +stable test -p fsm-cli --test mcp_live_golden
  $ REGEN_AFFORDANCE=1 cargo +stable test -p fsm-cli --test mcp_affordance_golden
  $ REGEN_AUDIT=1     cargo +stable test -p fsm-cli --test audit_golden
  ```

  `grep -rl "$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
  crates/*/tests/fixtures` is the check that no stamped fixture was missed: it
  should list exactly the files those five tests own.
- The `version` job refuses lightweight tags, dereferences the required
  annotated tag, and refuses a tagged commit that is not contained in
  `develop`, or a tag version that does not match the manifest. Containment
  permits a release candidate to lag later `develop` pushes without permitting
  a tag cut from another branch.

## Acceptance

```console
$ acceptance/acceptance.sh              # every scenario
$ acceptance/acceptance.sh seal         # only scenarios whose name matches
```

Native executor scenarios require `FSM_ACCEPTANCE_DISPOSABLE_NATIVE=1` on an
explicitly opted-in disposable Linux GitHub runner; `installed-consumer-check`
dispatches `acceptance/acceptance.sh executor` or the unfiltered recipe for an
immutable candidate. The consumer image installs the CLI through `cargo install`
and its protected authority from the same controlled source snapshot, then runs
the independent client as a nonroot operator under the container's own systemd
manager and private cgroup namespace, with finite memory and zero swap.
The wrapper retains original reports, binaries, native observations and container
retirement before removing its owned container; filtered evidence stays
ineligible for the complete acceptance inventory, and native proof requires an
actual passing run. Offline filters such as `seal` retain the ordinary Podman path;
the focused `seal` dispatch checks its evidence-volume writes with the original
root user inside the rootless container, without opting into native containment.

The installed claim-cut scenario uses GDB hardware breakpoints outside the
candidate to stop the ordinary standalone owner after its claim is durable and
before binding; mapped executable instructions must match the original file.
Its disposable systemd unit enforces 1 GiB memory, zero swap, a 45-second runtime
and control-group termination, including on observer failure; original claim,
forced termination, closure-before-entry and sequential recovery observations
remain distinct, and a missing symbol or hardware facility fails the scenario.
The stopped-result and acknowledged-result cuts use the same external observer
at exact pre-settlement and pre-event entry points; quiet recovery must retain
the original successful result, acknowledge and advance exactly once, and
never repeat validation or allocate a replacement for its completed domain.
The post-event cut observes the same original call's hardware entry and its
stack-derived hardware return on x86-64; all threads stop, code stays unchanged,
and the accepted event is durable before another domain can be claimed.
Quiet recovery must preserve the original advance without sending it again.
The stdio cut inventory initializes a real MCP client before its one trigger,
then uses separate private FIFO stdin/stdout/stderr under the external debugger.
Original descriptor identities must match those endpoints; debugger diagnostics
cannot stand in for protocol responses, and fresh stdio recovery must preserve
the exact original claim, successful result or accepted event at each cut.
The HTTP cut inventory completes real session initialization and sends its
one trigger without waiting for a potentially hardware-stopped reply; the
original process must own the exact loopback listening socket, with retained
descriptor, socket-table, executable and request-endpoint identities.
Fresh HTTP recovery preserves the same four durable phases and explicitly
drains its execution owner after journal verification and resource restoration.
The helper-closure cut hardware-stops the unchanged installed Root helper after
its original domain and claim-bound closure receipt are durable, before result
attestation or completed-response publication; the original host and helper
are independently killed and reaped, and a same-configuration helper at the
next irreversible epoch permits receipt-only recovery and quiet successor work.
Protected debugger records remain separate from authority records and numeric
host wait receipts; the fresh Root observer unit enforces 1 GiB, zero swap,
a two-minute runtime and control-group retirement on observer failure.
The helper-boundary matrix additionally observes exact pre-submission,
pre-authorization and collected-result entries across standalone, stdio and
HTTP owners, including HTTP domain-closed recovery; approximate file timing
never replaces these hardware stops. Original launch intent, isolated gate,
entry grant, fixture history and closure presence must match the declared
phase, and every fresh validation waits until the original claim-bound
closure receipt and absence of original user-code processes are observed.
Fixture entry identities append to an existing shared observation slot, so
DynamicUser cleanup of an invocation-owned readiness marker cannot erase
the original PID and birth identity; this fixture log grants no closure authority.
The installed descendant matrix observes process/MCP handler exit while a real
descendant remains alive with both inherited capture/protocol endpoints and
the same native cgroup, after a bounded stderr flood; the exact original helper
candidate entry precedes fencing, and receipt-only restart must close that
original descendant before releasing fresh validation on all three hosts.
Original/final descendant and output-write logs retain their bytes separately
from native closure records, and every observed descendant must be dead before
the successor owner confirms its final drain.
Installed supervisor and execution-owner recovery observe original and replacement invocation
identities through the persistent shared entry log, so DynamicUser cleanup of
an old ready marker cannot abort observation while native recovery is running;
live process identities and original claim-bound closure remain required.
The stdio hardware observer redirects the launcher's explicit original argument
list, retaining it with the debugger's reported arguments instead of relying on
older debugger parameter access to preserve the command during redirection.

The installed timeout matrix observes the original deadline predicate's
hardware entry, then its stack-derived same-thread return with a true boolean
and the runner's exact pre-fence call; original ELF call bytes must target those
unchanged functions, excluding error/Drop cleanup as timeout evidence.
An original still-live parent and inherited-pipe descendant must both die under
claim-bound native closure before fresh validation is released; the raw
interrupted start remains in the fixture trace without an invented end/result.
The private deadline predicate retains its function boundary in optimized
helpers as well, so installed consumer acceptance can observe the same original
expiry/fence sequence; deadline comparison, public APIs and persisted formats
are unchanged.

`acceptance/` builds an image with `cargo install --path crates/fsm-cli
--locked`, the same command a consumer runs, and drives that binary from a
client that shares no code with it: a standard-library MCP implementation
speaking newline-delimited JSON-RPC over stdio and Streamable HTTP over a
socket. Nothing in `crates/` is imported. A suite assembled out of the engine's
own helpers would agree with the engine by construction, which is precisely
what the host checks existed to catch.

This replaces a list that was ticked by hand. The list was honest about why —
each item wanted a live host, a human reader, or a real filesystem — but an
honour-system list is run differently by different people, differently by the
same person twice, and not at all under time pressure. What it was really
asking is recorded in the original fifteen-scenario baseline below; the complete
installed suite adds the independently observed native executor scenarios and
requires its entire current inventory to pass:

| scenario | the item it replaces |
|---|---|
| `tools_list_is_complete_and_within_its_budget` | connect and list all 25 tools, on every host |
| `the_golden_loop_runs_end_to_end` | run the golden loop end-to-end |
| `a_rejected_event_is_refused_rather_than_silently_ignored` | — (a refusal reported as success is the failure a host check would have shown) |
| `the_http_transport_serves_a_session_and_pushes_a_notification` | a real client over HTTP, initialize through teardown, one notification on the SSE stream |
| `the_http_transport_refuses_a_request_without_its_session` | — (teardown is only real if the session stops working) |
| `a_parent_and_child_workflow_runs_and_reads_back_as_a_tree` | drive a parent and child through a live host |
| `a_reactive_cascade_reads_as_one_macrostep` | drive a reactive machine — the fork/join in `examples/parallel_fork_join.json` — and confirm one macrostep |
| `a_cohort_preview_groups_its_refusals_legibly` | preview a cohort and confirm the grouped refusals read correctly |
| `the_executor_validates_a_shipped_handler_table` | validate a table with `--check` |
| `the_executor_settles_a_pending_effect_and_advances_the_instance` | the executor runs a real workflow unattended |
| `the_executor_exhausts_retries_onto_the_failure_path` | retries, exhaustion onto the failure path, `--list-dead` |
| `the_installed_binary_reports_its_version_and_prints_the_spec` | `cargo install --locked && fsm version && fsm docs spec` |
| `the_decimal_vectors_regenerate_byte_identically` | regenerate the decimal vectors |
| `a_sealed_store_archives_verifies_and_reopens` | **new** — sealing is the operation that removes data |
| `machine_test_runs_cases_reports_a_delta_and_regenerates` | **new** — `fsm machine test` is what an author runs most |

The last two were never on the manual list because the list predates plans
0017 and 0018. Sealing removes data and a review found a path where it did so
silently, so it is driven end to end here: preview, seal, verify without the
archive and with it, and confirm a mistyped `--with-archive` leaves the store
reported healthy rather than condemned.

**The golden loop is written down.** It was named three times and defined
nowhere, so each release it meant whatever the person running it remembered.
It is: create a machine, create an instance, advance it, acknowledge the effect
that advance emitted, drive it to a terminal state, read the history back, and
confirm the chain verifies.

### What is still genuinely manual

Two items are not in the suite, and neither is a scheduling problem:

- `manual:` **an LLM authors and drives the case-review machine from a
  natural-language brief, unaided, in a bounded number of tool calls.** This is
  the project's premise, it needs a live model, and a pass you coached is not a
  pass. Automating it against a pinned model would test the model.
- `manual:` **Claude Desktop specifically.** Its transport is stdio MCP, which
  the suite covers; what it does not cover is Desktop's own configuration
  parsing and UI. Worth one connect-and-list before a release, and worth being
  clear that is all it proves.

Everything else on the old list is above, and running it is one command.

### Latency

Still separate, because it produces numbers a human reads rather than a verdict:

```console
$ FSM_BENCH_ROOT=/path/on/filesystem-under-test \
    cargo +stable test --release -p fsm-store --test append_latency -- --ignored --nocapture
```

Update the measured table in [`EMBEDDING.md`](EMBEDDING.md) if the numbers have
moved materially. `crates/fsm-store/tests/append_guard.rs` answers the other
question — it asserts a wide ceiling and fails on a collapse — and runs in the
ordinary suite. A guard tight enough to notice a drift would be flaky on a
shared runner, and a flaky performance test is deleted within a month.

### Fuzz corpora

Gated by the release workflow's `fuzz-smoke` job on every tag push. Locally:

```console
$ rustup toolchain install nightly && cargo install cargo-fuzz
$ cargo +nightly fuzz run --fuzz-dir fuzz \
    --target "$(rustc -vV | sed -n 's/^host: //p')" json_parse -- -runs=2048
```

The `--target` is not optional in the job: cargo-fuzz defaults it to the
platform its own binary was built for, and a prebuilt one is musl-linked.

## Tagging and pushing

Push the branch first, wait for its CI matrix, and only then push the tag.
Splitting the two is what makes a platform-specific failure stoppable: the
release workflow runs the same matrix, but a tag that has already been pushed
is one a consumer may already have pinned.

Tags are annotated and named `vX.Y.Z`.

```console
$ git push origin develop
# wait for the branch matrix to pass
$ version="$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)"
$ test -n "$version"
$ tag="v${version}"
$ git tag -a "$tag" -m "fsm ${version}"
$ git push origin "$tag"
```

## What the tag runs

The version check runs first. The six-leg gate matrix, git-dependency proof,
changelog generation, and target CLI builds then run in parallel. The GitHub
release with checksums waits for all four, and the fast-forward of `main` runs
last. Every step converges on re-runs, so a release that fails partway is
finished with GitHub Actions' **Re-run failed jobs** action. Do not move, delete,
or attempt to re-push the immutable tag.

## Afterwards

The workflow fast-forwards `main` to the released commit, so `main` always names
the latest released state rather than relying on someone remembering. It refuses
rather than merges if `main` has diverged: a branch advertised as the latest
release is worse than a historical one when it silently lags, because it looks
authoritative. If that job fails, reconcile `main` deliberately.

Confirm the outcome from outside the workflow that produced it:

- a fresh crate can `git = "<repo>", tag = "vX.Y.Z"` and build;
- the GitHub release is not a draft and carries every platform archive plus
  `SHA256SUMS`;
- `main` points at the tagged commit.

## Definition of done

A release is done when the gate, the supported-consumer checks, version
stamping, `acceptance/acceptance.sh`, the two remaining manual items, and the
tag pipeline are all complete and green.

There are three supported consumers, and all three are in that list: the CLI,
the MCP hosts, and a Rust program embedding `fsm-core` (optionally `fsm-store`).
A release that satisfies only the first two is not done.

Unreleased native completion retention adds a Root-owned mode-0600, create-once
completed response record with a separate 64 KiB bound and read-only broker
recovery by allocation; normal control-record, journal, hash and receipt formats
are unchanged. Publication syncs the record and parent before success; missing
or torn completion records retain ownership uncertainty. Recovery revalidates
original contract/claim and native closure without current catalogue lookup,
and does not release service integration or native acceptance gates.

Never-launched native closure now uses the existing bounded exact observation
and repeated original-domain, closing and manager-retirement checks before
nonrecursive removal of a present empty cgroup. An isolated `populated 0` line
is insufficient. This preserves existing journal, receipt and response formats;
the production native gate remains unreleased.

Native empty-domain preparation checks the installed ordinary Root 0711 gate,
the nondumpable profile and bounded kernel entropy readiness before creating
its lock or burning an allocation. Failure leaves allocation history unchanged;
launch repeats validation and generates a separate nonce. This early refusal
preserves private/public record and journal formats and grants no handler entry.

Native allocation now uses the same protected bounded exact cgroup-events
reader as observation, requires an empty unfrozen domain and repeats original
directory checks before prepared publication. Failure retains the burned
allocation without authorizing claims, launch or reuse; record formats stay
unchanged and the production native gate remains unreleased.

Native typed handler admission bounds borrowed caller-owned strings and nested
argument/outcome values before cloning or hashing the full contract, refusing
excess with the existing `exec/config` error before claim mutation or helpers.
This preserves public signatures and journal/hash formats; fresh native,
portable and frozen host acceptance remains required before production routing.

Plan 0022 ownership integration: watcher observations expose unresolved original claims and stopped results from the current read-only prefix, including cancelled or removed effects; production tick routing and capacity integration remain pending.

Plan 0022: pure scheduler start selection now respects unresolved original execution ownership before looking up the current handler; integrated host capacity and routing remain pending.

Plan 0022 scheduler capacity now accounts for durable owners and retained local reservations, deduplicating only bound original run identities; integrated host launch and shutdown remain pending.

Plan 0022 adds bounded protected physical-store route discovery to native preparation; automatic service routing and provisioned discovery fault acceptance remain pending.

Native discovery accommodates operator-store namespace siblings while retaining the shared inventory bound; the native preparation fixture now invokes physical-store discovery instead of supplying its route.

The provisional `NativeExecution::retain_uncertain` retains an original durable
claim after helper startup failure without starting transport or accepting a
completion. Its progress remains `Uncertain`, capacity remains retained, and
observation/application refuse without verified original reconciliation; this
constructor alone authenticates no journal ownership. This additive host
primitive changes no journal, receipt, attestation or hash format and does not
complete production Runner routing or release its gate.

The provisional owned native client adds one-shot `start_retained`, allowing
hosts to install the actual durable claim before requesting helper startup and
keep that owner on failure. The provisioned positive owner probe now exercises
read-only startup refusal, refusal of a later retry with a healthy writer, and
successful installed-owner startup followed by duplicate-start refusal; these
expanded controls require changed-source native execution before acceptance.
No persistent format or hash changes occur, and fresh shared-tick admission is
still unfinished.

Installed-owner binding now waits for an explicit writer-protected
`launch_bound` call rather than dispatching execution during ordinary polling;
the actual provisioned owner control requires repeated bound observation and
read-only dispatch refusal with absent launch/entry/handoff records, then a
healthy-writer request and duplicate-request refusal before resuming independent
writer contention. These changed-source controls and compilation remain pending
memory headroom and native CI; original constructor coverage remains on the
timeout axis, and no format/hash or production-gate change is claimed.

Prepared-domain cleanup: Root `close` now handles allocations that were prepared but never bound, refusing partial binding/submission records and checking original native identity and manager retirement before tombstone publication, without issuing an execution receipt; the expanded privileged `empty_domain_preparation` control requires provisioned native execution before acceptance.

Original prepared-domain cleanup adds private broker action `discard-prepared`, matching the full retained domain under the authority lock before retirement and echoing it afterward; the expanded privileged preparation control checks wrong-domain refusal before revocation and successful original-domain cold replay, with provisioned execution still required for acceptance.

The provisional native client exposes NativePreparedCleanup with bounded start/poll/cancel/reap/progress methods and original-domain response matching; automatic Runner integration and provisioned validation remain unfinished.

Plan 0022 initial shared Runner startup recovery adopts original unresolved owners from the watcher snapshot, bounds active recovery transport to one, and retains ownership through failures, missing observations and helper retirement; a changed physical directory refuses scheduling. Added a public borrowed-tick regression with a cancelled durable owner and substituted physical store, asserting refusal and unchanged state/records/head. Public signatures and persistent formats are unchanged; complete native launch/settlement and post-ack restart integration remain unfinished.

Plan 0022 recovered native completions now enter shared tick writer-held settlement and original-contract event application, with one fairly selected application per tick; disabled advances retain their original completion but release consumed capacity and park until journal progress. Added a provisioned unprivileged shared-tick control covering a real recovered completion under independent writer contention, readonly refusal, temporarily disabled advance, later resume, original ack-before-event keys, duplicate-free repeated ticks and cold store reopening without current handlers or catalogue. Actual execution of this new native control remains pending; native admission and post-ack cold discovery are not complete.

### Native local reservation after authenticated consumption

Shared native settlement releases a local scheduler reservation only when its
full original claim matches and `NativeExecution::settle` has authenticated
durable consumption. Optional event deferral or an event error after consumption
does not retain that execution slot; the original completion remains available
for event reconciliation. Read-only refusal, uncertain closure, and a different
claim leave the local reservation intact. This internal composition changes no
public API, persisted bytes, hash domain, or published error code; fresh native
admission and cold recovery between Ack and event use the guarded owner and
authenticated handoff paths specified below.

Shared Linux tick ownership pins the physical durable store from its first
verified watcher snapshot, including an empty owner set before the first
claim. A different directory identity at the same path is refused before
scheduler selection even when the copied journal prefix is identical; no
claim, handler entry, or settlement is authorized by that refusal. This
internal route check changes no public signature, persisted bytes, hash
domain, or published error code, and does not complete fresh native admission.

### Runner handoff for an already published native claim

Additive provisional `Runner::start_native` accepts a genuine eligible original
claim under a healthy durable writer and a scheduler that already holds its
local effect reservation. It pins the physical store, installs retained original
ownership, binds the local reservation, and only then requests one-shot helper
startup; post-installation failure retains the claim and capacity without an
automatic replacement request. Shared tick observation pauses at Bound, and
writer-held application rechecks current eligibility before requesting entry
once. Read-only entry refusal retains Bound for a later healthy writer; an
uncertain entry attempt cannot be repeated. Shared cancellation requests native
transport cancellation and retains ownership rather than creating a legacy
settlement. Unsupported platforms refuse startup with `exec/mode` and no
automatic direct-child fallback. Persisted bytes, hash domains, and error codes
are unchanged; supported production host selection, bounded shutdown and cold post-ack recovery
use the original owner and authenticated handoff paths. Full integration gates
remain separately required.

Fresh Runner installation additionally MUST authenticate the claim namespace and
authority generation against the protected registration for the physical store
before retaining ownership or requesting binding; an identical copied journal
does not authorize the original domain. Shared native writer application MUST
match the host physical-store pin before changing entry permission or applying
a completion; refusal preserves original ownership and local capacity. These
checks change no public signature or persisted format, and provisioned native
process and MCP controls remain required before production acceptance.

Provisional `Runner::new_native()` explicitly selects fresh native admission
through both shared tick entry points, with no automatic direct-child fallback.
Public `Runner::spawn` MUST return `exec/mode` on a native-selected runner
before child startup, capture creation or MCP worker creation, including an
explicit direct invocation; direct-child primitives remain available through
`Runner::new`. This closes a bypass of the selected native admission boundary,
without changing journal formats, claim identities or request keys.
The host MUST retain the original journal-derived effect and checked handler
contract before requesting preparation, serialize allocator transport, collect
one original domain only after helper retirement/EOF, and publish its claim
under a healthy writer on the pinned physical store before one-shot binding and
entry. Its claim request key is `exec-claim-` followed by lowercase SHA-256 hex
of canonical JSON `[effect_id, domain]`; existing ack/event keys are unchanged.
Preparation does not consume handler runtime: the authority enforces the
original actual-entry timeout. A read-only borrowed tick MUST NOT enqueue or
start a new allocator request; owned observation and cleanup remain independent
of writer access. A cancelled queued request that never started may release
only its matching unclaimed local reservation; a delivered unclaimed domain
requires exact original-domain cleanup success plus helper retirement/EOF.
Unknown allocation, cleanup, or claim publication MUST retain capacity, never
produce legacy synthetic acknowledgement and never allocate a replacement.
An observed genuine claim after an uncertain append transfers to retained
original ownership. Existing ready native owners take precedence over further
claim publication, and readonly snapshots are dropped before the standalone
writer opens. Production CLI/MCP/service hosts select this native path; owned
shutdown, original-run reconciliation and cold acknowledgement-handoff recovery
retain their respective identity, authority and writer checks. Plan 0022 records
its frozen native acceptance; plan 0020 requires its own transport integration
gate.


The additive pure `fsm_core::record::execution::AcknowledgedHandoff` value
reserves `fsm.execution-handoff/1` candidate material for cold post-ack
recovery: complete original Claim, lowercase original claim-record digest,
original handler contract, actual terminal StoppedOutcome, nonzero
acknowledgement sequence and the unchanged derived acknowledgement/event keys.
Decoding MUST charge the complete candidate to 128 KiB before copying, bound
contract and outcome individually to 64 KiB, reject unknown envelope fields,
match the handler fingerprint under `fsm:handler-contract:1` and the original
retry policy, and select the original on_ok/on_failed event from the actual
outcome; interrupted outcomes and absent/malformed event envelopes MUST refuse.
Omitted and explicit-null result fields MUST remain distinct.

This pure value authenticates neither acknowledgement, native closure nor
accepted event delivery, and does not replace the execution layer's checked
full handler parser. Store integration MUST compare its material against the
actual claim hash/stopped outcome and publish it atomically with acknowledgement,
then retire it only through a matching actually accepted event. The initial
value-only change left ExecutionState, journal VERSION 11, state-root/4,
snapshots and sealed bases unchanged; subsequent persisted handoff integration
and cold recovery use the separately specified original-contract checks.
Decoding a candidate grants no execution or event permission.


`AcknowledgedHandoff::matches_acknowledgement` MUST compare the complete
original Claim, actual stopped result including omitted/null semantics,
stopped closure run/domain binding, verified original claim-record hash,
actual acknowledgement key and append sequence; a matching key or fingerprint
alone MUST NOT pass. The caller must supply verified journal/evidence inputs:
this pure comparison cannot authenticate arbitrary caller-owned values and
does not itself publish a handoff or consume an event.


Fixed automatic snapshotting of in-memory stores accidentally writing to their placeholder data directory at 10,000 records; a boundary regression asserts no filesystem entries while checking the final published claim hash and complete replay equality, with no format or hash change.


Plan 0022 replay now carries original unresolved claim hashes separately from logical execution bytes, corrects checkpoint projections to the final published record hash, and restores anchors from verified journal prefixes or authenticated base indices on cache/sealed reopen. Settlement removes the context with its owner; independently decoded execution blocks have no anchor. Atomic acknowledgement handoff persistence and cold event delivery remain unimplemented.


Unreleased post-ack persistence work introduces VERSION 12, root/5, snapshot/7 and base/3 for atomic acknowledgement-to-event obligations, with explicit historical root/base verification and no journal rewriting; cold host delivery and installed production acceptance remain separate required gates.


Corrected matched-stop inspection racing natural cgroup retirement: a confirmed absent group can reach the existing original-handoff revocation and manager checks, with no weakened closure proof, relaxed deadline or format change; new installed runtime acceptance remains pending.


Added bounded cold native post-ack event recovery through shared tick paths, retaining original contract, result and physical authority identity across cache and sealed restarts; original warm completion retry semantics remain intact, and production native defaults await installed acceptance.

### Durable executor ownership discovery

The additive `execution_ownership` field on `fsm://executor` reports enabled
claim-era state and counts of unresolved runs, stopped runs and outstanding
acknowledged event handoffs from the observed verified prefix. Unavailable
stores report null; read-only stores can expose prefix counts without writer
access. These are durable obligations, not live process health or closure
proof, and disclose no identifiers, commands, native paths or handler results.
This adds resource metadata without changing journal formats, hashes or
execution admission; the existing `fsm.executor/1` format remains applicable.

### Original acknowledgement required for local handoff retirement

Cold event-only recovery retains its local obligation when the observed
writer no longer carries the handoff but lacks the exact original
acknowledgement sequence and settlement fingerprint. A replacement prefix
cannot establish reconciliation merely by omitting an obligation. The
refusal parks without journal mutation and changes no persisted format,
hash domain, public API or acknowledgement/event request-key derivation.


### Provisional native closure request

Added a Linux NativeShutdown primitive that requests original-claim native
closure from a verified durable snapshot without acquiring the writer, keeps
execution transport separate, and authenticates an immutable original receipt
before returning opaque proof. It preserves ownership on uncertainty and
changes no persisted format or production default. Full installed acceptance,
all admission phases and the bounded production shutdown controls remain
unfinished; the primitive alone does not complete task 9402.


### Original binding before claimed closure

Corrected the provisional closure client to send the full original binding
through close-claimed, requiring the authority to match it before fencing or
closure rather than relying solely on receipt validation afterward. Older
helpers refuse without fallback; no Rust signature or persisted format changes.
Installed runtime acceptance remains pending and production defaults unchanged.

### Receipt-only interruption application

Added provisional explicit interrupted settlement from retained authenticated
closure proof under the original healthy writer, preserving pending effects,
recorded outcomes and original request keys without a synthetic completion or
machine event. No persisted format changes; native runtime acceptance and the
production lifecycle controls remain incomplete.


### Local native admission provenance

Track local native admission provenance through uncertain publication and failed
binding, and refuse local helper cancellation of foreign observed claims without
releasing their durable identity; no persisted formats change. This foundation
does not establish native closure or a bounded production shutdown guarantee.


### Bounded queued protocol output

Added opt-in bounded complete-frame notifier output with in-flight allocation accounting, explicit close/drain observation and write-failure preservation; production lifecycle integration remains incomplete.


### Shared bounded protocol input

Bound reverse-protocol reads and remove oversized stdio-frame tail accumulation
using shared framing; preserve ordinary parse errors, exact-cap acceptance and
next-frame synchronization. Native lifecycle deadlines remain a separate gate.


### Admission-free native completion observation

Added a supported-Linux native completion-only service pass that excludes pending admission, preparation starts, bound entry, retries and machine deadline polling while retaining original completion/event-handoff delivery; production shutdown wiring and installed native acceptance remain pending.


### Original interrupted native retirement

Added exact original interrupted-run retirement after authenticated
settlement and retirement of both native transports, preserving pending work
and refusing foreign ownership or missing/pruned ledger evidence; production
shutdown wiring and installed native acceptance remain incomplete.


### Shared native admission closure and local targets

Current operator guidance now describes the implemented shared native admission
and unclaimed-preparation cleanup, including original reservation retention on
refusal; the low-level primitives' individual limits do not imply that their
production integration is absent, and signal integration remains separate.

Corrected original runner candidate selection during external closure: matching
protected revocation is checked before selecting process exits or MCP answers,
including after root-status observation, so shutdown-induced failures cannot
advance the workflow; already selected outcomes and complete-closure requirements
remain intact, with no persistent format or public signature change.

Added shared native admission closure and original local shutdown-target iteration, including final authorization checks after route/writer validation and retention of pre-closure publications. Closed fences preserve completion/handoff processing and uncertain preparations; native Drop closes admission only. The shared fence alone does not establish lifecycle reporting or production stdio shutdown; later opt-in lifecycle and endpoint APIs compose it, with installed production acceptance still required. Persisted formats and production runner selection are unchanged.

- Added an opt-in Linux owned native lifecycle driver and independently
  waitable control: immediate shared admission closure, first-deadline
  preservation, bounded fair original-claim closure, original completion
  precedence and writer release before confirmed `Stopped`; a stalled worker
  reports `Uncertain` without claiming cleanup. Production stdio progress,
  installed native driver acceptance and production endpoint publication
  remain pending; no persisted format or production default changed.

- Corrected the owned native driver so a preclosed admission fence alone cannot
  release its writer; an explicit lifecycle request is required, with a
  downstream actual-writer regression and separate provisioned process/MCP
  bound-owner driver controls, whose native execution remains pending here.

- Added an opt-in Linux owned native MCP session composition with one writer
  owner/clock, bounded single-reader input shared with elicitation, quiet
  admitted-only observation, queued output, and bounded feed-stop admission;
  native cleanup and output delivery are reported separately, with explicit
  timeout facts and first-deadline reuse. Existing borrowed session bounds and
  production CLI selection remain unchanged; native tree/production endpoint
  and signal acceptance are pending, and detached I/O is not claimed retired.


- Added an opt-in Linux owner-only local control endpoint and bounded client
  for an actual owned native driver, with exact incarnation/physical-store
  discovery, bounded nonblocking connections and independent metadata responses;
  waiting drains cannot occupy abort parser workers, and cleanup preserves
  replaced files. Transport uncertainty never confirms native closure or writer
  release; default selection and installed production acceptance remain
  pending, with no journal format or hash changes.


- Added `execute stop` with explicit drain/abort, finite timeout and private
  control-root discovery, using the actual owned endpoint without taking the
  journal writer; confirmed stopped reports exit zero, uncertainty exits one
  with preserved native report or null unknown transport facts. Current executor
  defaults and production endpoint publication remain unchanged and unproved.


- Added an opt-in paired native lifecycle actor with verified read-only
  snapshots, temporary writer leases and independent cloned control, reusing
  authenticated closure before writer attempts and preserving writer-dependent
  settlement; paired endpoint publication binds its pinned physical directory.
  Current production selectors and installed two-live-native-actor acceptance
  remain pending, and no persisted format or hash changes.

The opt-in paired lifecycle driver now retains structured writer availability
through `tick_reporting`, and native control adds finite explicit-request wakeup
through `wait_for_request`; existing line-only ticks remain available, and
production routing and installed acceptance remain pending.

The Linux opt-in `fsm_cli::standalone::run_paired` drives an actual paired
native owner with independently queued diagnostic stdout supplied by the host;
ordinary ticks follow the configured interval while admitted polling and explicit
request notification remain independent of it. Its `StandaloneReport` separates
shutdown facts, successful output drainage, dropped diagnostic line count and
initiating failure. Logs share the existing 256-frame/8 MiB allocation budget;
oversized (over 64 KiB), multiline or saturated lines are counted as dropped,
and broken output requests abort without blocking ownership. Output close/drain
uses the original shutdown deadline and never joins a blocked writer. The host
must publish and close the exact actor endpoint explicitly; production command
routing and installed nonempty native acceptance remain pending.

The Linux standalone shutdown report exposes `shutdown_deadline`, the original
monotonic request deadline, for transport retirement.
`LocalControlEndpoint::close_until(deadline)` stops transport admission even
when that deadline has expired, without starting another waiting budget;
its boolean confirms actual endpoint removal separately from native shutdown,
writer release and response delivery. Deadlines beyond the finite executor
bound are refused before transport admission changes.

Ordinary `fsm execute` now retains a paired native owner and publishes its
private incarnation endpoint at `--control-dir` or `$HOME/.cache/fsm/control`.
It uses bounded diagnostic delivery, independent admitted observation and the
original shutdown deadline for endpoint retirement; unavailable or unsupported
native execution refuses without a legacy fallback. Final success requires
authenticated empty shutdown, endpoint removal and complete diagnostic delivery.
Production embedded MCP and the low-level service host also select native
execution; their distinct owned and borrowed lifecycle contracts apply.

Production standalone final errors preserve the initiating code, message and
hint, nesting its original details under `initiating_details` and attaching
separate observed shutdown, endpoint and output facts. Unavailable output facts
are null; run IDs and counts are decimal strings, avoiding JSON number precision
loss. Endpoint removal does not imply native cleanup or successful delivery.

The Linux owned native session reporting entry
`serve_owned_native_session_reporting` retains the initiating protocol I/O
failure in `OwnedSessionReport.failure`, alongside actual shutdown/output
facts and `shutdown_deadline`, the original monotonic control deadline.
The existing `serve_owned_native_session` entry preserves its error-return
behavior. Invalid timeout options refuse before worker startup or admission
changes; this additive entry does not select production embedded execution.

Production embedded stdio through `run_with_mode` now retains a native writer
owner and publishes its private endpoint under `$HOME/.cache/fsm/control`.
Quiet input permits admitted observation and explicit control; sole reader
construction occurs in its worker. Healthy native startup has no legacy
fallback; contended/unhealthy startup preserves the exact observed diagnostic
prefix without publication or reopening into execution. Borrowed session APIs
retain their bounds and explicit backend selection. Unsupported production
embedded stdio refuses. Supported embedded HTTP retains the same complete owner
with server-scoped lifetime; full plan integration acceptance remains required.

Production native stdio failures expose the initiating I/O message and kind
with separate actual shutdown, endpoint removal/error and output drainage
facts in the existing exec/inflight_deferred error frame. Typed native startup
errors retain their executor code. Cleanup refusal does not replace an
initiating protocol failure or imply confirmed native retirement.

Native stdio endpoint publication refusal is reported through the executor
error frame; a startup transport refusal does not confirm native cleanup.

Owned native MCP action diagnostics now use the shared bounded operator output worker instead of writing stderr on the journal owner; `OwnedSessionReport` separates operator drainage and rejected diagnostic lines from protocol delivery and native cleanup, and production success requires confirmed delivery without diagnostic loss under the original deadline.

Native owned MCP protocol warnings about requests preceding `notifications/initialized` use the same bounded operator diagnostic queue as executor action lines, with the same separate drainage and loss facts; borrowed sessions retain their existing warning behavior, and no journal or wire format changes.

Supported Linux native standalone/owned stdio final error delivery now obeys the original shutdown deadline without joining blocked stderr workers; small pipe reports use separate atomic nonblocking writes, and undelivered reports do not prevent failure exit or imply successful cleanup, with no durable or wire format change.

Native preparation cancellation retains its original finite transport deadline:
when eligibility is lost or admission closes, an already requested preparation
continues bounded observation so an authenticated delivered domain can be
cleaned through the original prepared-domain protocol; no bind or execute is
permitted after cancellation, and transport failure or unknown allocation
retains uncertainty and capacity rather than treating helper death as closure.
This changes cancellation cleanup only, with no journal format, hash domain,
request key, dependency or MSRV change.

The Linux private owner control additionally accepts the closed four-field
fsm.executor-observe/1 request (format, incarnation, store_device, store_inode)
through local_control::observe(root, data_dir, timeout_ms). It MUST authenticate
the same original private endpoint and physical identity and use the existing
1024-byte request, 128-KiB response, 64-connection and eight-client-worker bounds.
It MUST return only the actual bounded control metadata snapshot, without
opening or writing Store, requesting stop, changing admission, extending any
existing shutdown deadline or authorizing closure/settlement. Observation may
report running/draining/stopping as well as terminal phases; stop clients still
MUST accept only terminal identity-matched reports. An observation snapshot
MUST NOT be interpreted as a native closure receipt or current durable prefix.
This is additive local metadata observation, with no journal/format/hash-domain,
dependency or MSRV change; production race and full lifecycle acceptance remain
separate requirements.

Private observation version 2 keeps the closed four-field identity request
fsm.executor-observe/2 and returns fsm.executor-observation-report/1 with the
original twelve metadata fields plus preparation_phases. This is either null
(unpublished/poisoned, never inferred zero) or exactly ten bounded nonnegative
counts: queued, preparing, prepared, cleaning, unknown_allocation,
uncertain_preparation, uncertain_cleanup, uncertain_domain, claim_uncertain,
and closed; their sum MUST equal unclaimed_reservations and be at most 4096.
The original owner's coherent ExecutorControl::observation snapshot supplies
these counts, with no journal/native I/O or control mutation. The fixed array
uses the same phase order and is optional when unpublished/poisoned. Counts do
not grant cleanup, settlement or closure permission. Legacy observation /1
and terminal stop reports keep their original twelve-field schema unchanged.
local_control::observe now requests /2; this additive health API/protocol version
changes no persisted format, hash domain, dependency or MSRV.


Native prepared-cleanup failure diagnostics are best-effort lifecycle log lines
beginning `native-prepared-cleanup-uncertain`, bounded to 1024 UTF-8 bytes with
control characters replaced by spaces. A runner retains at most one pending
line and drains it once through owned or paired lifecycle polling; additional
simultaneous failures may be omitted. It reports the cleanup transport/refusal
message, not handler output or native closure evidence, and never releases an
uncertain reservation, changes shutdown eligibility or renews a deadline.
This additive diagnostic leaves the provisional public Rust surface, private
control versions, persistent journal/hash formats, dependencies and MSRV intact.

For a closed authenticated prepared-cleanup refusal response, the diagnostic
preserves its broker reason prefixed `prepared cleanup refused`, capped to
1024 Unicode characters before the lifecycle line's stricter 1024-byte limit;
success still requires the complete matching original domain, and malformed
or extended responses never confirm cleanup.

Native owner observation failures also produce best-effort
`native-execution-uncertain` lifecycle lines under the same 1024-byte UTF-8 and
single-line rules. A runner retains at most one cleanup line and one execution
line (2048 bytes total), drains one per lifecycle poll with cleanup first, and
may omit simultaneous additional failures. These messages do not authorize
retry, launch, closure or settlement and leave original claims, reservations,
helpers, deadlines and public/wire/persistent formats unchanged.

Native preparation MUST tolerate contention before allocation publication:
only a protected authority lock's `WouldBlock` acquisition may be retried,
within one two-second monotonic budget established at preparation entry.
Preparation MUST revalidate the original authority identity after acquisition
and before publishing an allocation intent or advancing its counter. Other
lock errors and acquisition exhaustion MUST refuse without those mutations.
No already-published preparation or execute action may be retried, no host or
handler deadline is renewed, and no refusal proves native closure or releases
uncertain ownership; public APIs and persistent formats are unchanged.

A native transport worker MUST NOT finish solely because a later reap observes
helper exit and both EOFs after its preceding response poll returned pending.
It MUST retain that original transport and decode its response, or publish an
explicit refusal, before finishing; delivery still requires actual worker join.
A joined transport worker without a published response MUST refuse as uncertain
rather than remain pending. Neither case grants native closure or claim release,
retries an execute action, or renews the original absolute deadline; public APIs
and persistent formats are unchanged.

Binding validation MUST tolerate contention of its protected authority lock
before any binding, entry authorization or manager submission mutation: only
`authority busy` during acquisition may be retried, within one two-second
monotonic acquisition budget established at validation entry. Validation MUST
check the original authority identity after acquiring the lock. Exhaustion
MUST refuse without publishing binding or launch state; subsequent validation
still checks the current original claim and all existing entry guards. This is
not permission to retry an execute action, renew a host/handler/shutdown deadline,
release ownership or infer closure, and changes no public or persistent format.

Exec-status association MUST tolerate protected authority-lock contention only
before changing socket or directory access: only `authority busy` acquisition may
be retried, within the existing two-second monotonic association budget; lock
acquisition, original handoff/domain revalidation and peer authentication share
that budget. Other lock failures refuse immediately, exhaustion retains the
claim without entry authorization, and no execution or shutdown deadline is
renewed; public APIs and persistent formats are unchanged.

Prepared-domain discard MUST retain the complete original domain while the
protected authority lock is contended: only the `authority busy` refusal before
revocation/native mutation may be retried, and acquisition plus retirement share
a single two-second cleanup budget established at request entry. Other refusals
remain uncertain immediately; exhaustion never releases capacity or proves
closure. Original prepared identity, no-submission, durable revocation, manager
retirement and empty-cgroup checks still govern success. The host transport and
shutdown deadlines are unchanged, with no new public or persistent format.

Native preparation failures now share the single pending lifecycle diagnostic
slot with prepared cleanup, using `native-preparation-uncertain`; startup and
poll failures retain their original uncertainty and reservation semantics.
Closed native-response/1 refusal envelopes preserve preparation and completion
broker reasons with their operation prefix, sanitized and capped to 1024 UTF-8
bytes; malformed envelopes retain generic errors. These additive diagnostics
change no error code, journal/hash format, control version, dependency or MSRV
and grant no closure, ownership release or renewed deadline.

The public low-level `service::run` loop now selects `Runner::new_native()`
and uses the shared preparation, writer-held durable claim and one-shot entry
sequence; unavailable native authority or unsupported platforms refuse fresh
effects with `exec/mode`, without a direct-child fallback or synthetic ack.
Explicit legacy Runner primitives remain available. The existing borrowed
clock/emitter signature, polling and contention policy are unchanged, as are
journal/hash formats and error codes; this is a provisional native
host selection change, not completion of bounded shutdown or recovery.
Its borrowed emitter still executes inline and this loop has no stop handle;
callers requiring independent control should use the owned/paired lifecycle
drivers, and full service-loop shutdown integration remains unfinished.

Exclusive public service loops now probe and release the writer when native
admission leaves a tick without a write attempt, so a held writer still
produces its store diagnosis and the existing three-blocked-tick exec/mode
refusal; paired loops keep their existing writer-on-demand behavior. The probe
claims no execution ownership and changes no journal/hash format, error code,
public signature or shutdown deadline; complete bounded shutdown remains open.

Borrowed MCP ExecutorLoop construction now selects native shared-tick
admission: the public borrowed session helper requires protected native
authority for fresh effects and refuses unavailable capability with exec/mode,
without direct-child fallback or synthetic acknowledgement. Read-only borrowed
ticks retain their prohibition on fresh allocation or entry. This provisional
host selection changes no public signature, error code, journal/hash format,
dependency or MSRV; protocol input still drives these borrowed ticks and its
inline diagnostics/EOF shutdown remain outside the bounded owned-host claim.

### HTTP reverse-response idle correction

Quiet mailbox polls remain pending rather than masquerading as disconnected
clients; session DELETE closes and wakes the original mailbox before acquiring
execution state. Existing protocol errors and journal formats are unchanged.
This isolated correction does not itself establish reverse-request streaming,
bounded mailbox admission or HTTP native ownership; subsequent production HTTP
integration supplies those contracts and their focused acceptance.

HTTP reverse-response queues now have count and charged-payload budgets;
overload returns 503 without store dispatch, and saturated queues still close.
Original protocol errors and journal formats remain unchanged. This mailbox
bound does not complete autonomous HTTP ownership or streaming acceptance.

Private-host preparation now includes bounded reserved cancellation controls,
with response suppression and reusable journal keys before dispatch, original
session-generation isolation and charged control/RPC allocations. Coarse-loop
tool cancellation also remains active without progress metadata. This is a
correction/preparation step, not autonomous stdio/HTTP execution or a new public
API, error code, discovery version, journal format or hash domain.

The private Linux native command owner retains the existing
OwnedNativeExecutor and drives its decision passes independently of client
input, sharing the writer-only owner's complete command boundary. Stop fences
original native admission before queue rejection and returns the original
driver/report after supervised polling, retaining uncertain ownership for its
caller. Operator diagnostics use the existing bounded output worker. This
private integration changes no supported public signature, error code, wire
discriminator, journal/hash format, dependency or MSRV. Supported production
stdio and HTTP construct this owner; their focused real-process acceptance is
recorded in plan 0020, with full integration gates due at plan completion.

Private host protocol-read commands now cover resource listing/resolution and
argument completion through the same bounded mailbox as tools, capturing one
committed prefix and charging URI/Value allocation capacities. They reuse the
existing resolvers and the original native driver's sanitized handler table.
Production stdio and HTTP use this command boundary; public wire/error/journal
formats and hashes are unchanged.

The shared MCP method handler now has a private hosted entry that submits tools
and protocol reads to the owner, leaving response formatting and client waits
in the session adapter. Borrowed public helper signatures and wire responses
remain unchanged. Admission busy, cancelled-request retirement and transport
output failures have distinct internal types, so a committed request cannot
be mislabeled server-busy when output is saturated. Supported production
transports select this entry with interactive continuations, progress forwarding
and bounded egress admission; public errors, journal/hash formats, versions,
dependencies and MSRV remain unchanged.

The private owned stdio composition now connects the existing capped framing
and shared hosted method handler to the native owner with independent bounded
input/output workers. Quiet byte input permits deadline progress; EOF and
explicit control preserve the original shutdown deadline. A blocked output
worker cannot keep the writer, and failed/unfinished delivery remains explicit;
a live uncertain owner is retained instead of joined after its deadline. This
composition is selected by supported production stdio, with interactive
continuations, progress forwarding, bounded egress and autonomous discovery
described by the production contract; this composition changes no journal/hash
format, dependency or MSRV.

The private hosted adapter's admitted-response wait also observes original
session close, failed protocol output and native lifecycle stop at finite
intervals, without requiring
another owner turn. Retirement suppresses the response and cancels the original
session; output failure retains its BrokenPipe classification instead of
becoming a silent retirement or admission-busy reply. This does not prove
completion, undo committed work, or release the
writer. The composition still retains and reports the original uncertain owner.
This internal control integration adds no public signature, error, journal,
hash, dependency, wire version or production-backend selection.

The private hosted elicitation path now prepares and settles on the store owner
while the session adapter waits for the client outside that owner. The original
session generation, RPC cancellation control, request key and admission slot
remain reserved through the question; continuation and answer allocations grow
that same host/session byte reservation before settlement. A replacement session
cannot resume it. Accepted answers use the ordinary idempotent send path, with
an instance-history guard that refuses a changed target while allowing unrelated
commits; replay still checks deduplication before the sequence precondition.
Decline, cancellation, retirement and rejected answer growth do not claim a
journal key. Borrowed public helpers preserve their existing behavior.
This advances the private stdio path without selecting a production backend or
changing public signatures, error codes, wire versions, journal formats, hash
domains, dependencies or MSRV; progress forwarding, complete egress accounting,
versioned discovery and installed-handler acceptance remain pending.

The private native owner now samples its injected logical clock once per
decision or shutdown-observation pass and uses a fixed clock for all journal
operations in that pass. Monotonic waits remain independent of journal time.
This aligns the staged autonomous scheduling contract without changing the
public explicit-tick APIs, persistent format, hash domain or production routing;
changed-source verification and the broader activation milestone remain pending.

Private hosted stdio output now preflights canonical frame size on borrowed
values before allocating encoded bytes, caps the complete frame at 16 MiB,
and retains at most 64 frames / 32 MiB including blocked in-flight allocations.
Refusal closes admission and exposes failure to idle input/lifecycle controls;
enqueue still does not prove delivery. Legacy direct and public queued helpers
retain their behavior. Response Value construction, long diagnostics, progress
forwarding, response/notification ordering and production activation remain
separate integration obligations; no public wire/API/format version changes.

The private owned stdio adapter now preserves tool progress metadata through
a charged hosted command context and dispatches with the original cancellation
flag. Its context constructor accepts only the bounded hosted output mode, so
owner-side progress cannot call a direct transport writer. Existing progress
rate/final-report behavior is reused. Admission conservatively charges metadata
copies and adapter/RPC retention; output refusal remains observable by the
independent hosted lifetime controls. Legacy borrowed helpers are unchanged.
This does not move long diagnostics off the owner or establish response/change
notification ordering, production activation, HTTP or installed-handler
acceptance, and introduces no public signature, error code or wire version.

Private hosted stdio response waits now read owned input at finite intervals,
route current cancellation and EOF without an owner turn, and answer ping
while waiting. Other requests stay in wire order in an independently bounded
eight-frame / 16 MiB raw input backlog, then enter ordinary dispatch; parsed
copies are dropped. Known deferred cancellation suppresses that frame without
creating future cancellation metadata for unknown IDs. Application admission
and the raw input backlog remain separate accounting units. Borrowed helpers
retain their blocking behavior; retirement does not undo committed work or
prove writer release. Production transports use the separately bounded
diagnostic and publication paths; focused transport/native acceptance is
recorded in plan 0020, with full integration gates still due.

Private hosted stdio now catches protocol-adapter unwinds after owner startup,
retires the original session and requests original-control shutdown while
retaining the initiating panic as a report failure; native retirement and
output delivery still require their independent evidence within the original
deadline, with no new public API or wire/journal format.

Production Linux embedded stdio now constructs the original owned native host,
with independently scheduled decision passes and bounded owned protocol input
and output. The active host MUST expose `fsm.executor/2` with
`progress: "autonomous"` at the unchanged `fsm://executor` URI, retaining
sanitized handler fields and `external_executor: "unknown"`; the closed v1
progress enum MUST NOT be extended. Polling observes this host rather than
causing progress, and an open quiet stdin remains a live session. EOF and
output refusal request original-control supervised shutdown; uncertainty
remains explicit until native, endpoint and output retirement are proven.
HTTP, contention/degraded fallback and borrowed helpers keep their existing
contracts. This stdio capability and wire discriminator change has a pre-1.0
minor-version consequence, with no core semantics, journal bytes, hash domains,
public Rust signatures, dependency or MSRV changes. Focused production stdio
and bounded egress acceptance is recorded in plan 0020; full plan integration
gates remain due before completion.

Linux embedded stdio accepts `serve --execute --poll-interval-ms N`, default
250 ms, within 1..=86400000 milliseconds; malformed values and incompatible
modes refuse with the existing args error before table loading or store open.
The original native owner receives this interval, and reserved stop observation
stays independent at 50 ms. This adds a CLI option without altering supported
public Rust signatures, persistence, core time semantics, dependencies or MSRV.

Hosted stdio retires its wait once the original native owner has returned
and operator output has drained when protocol output is permanently broken;
it preserves failed-delivery facts and the initiating error, while merely
blocked output still shares the original deadline. This prevents a known
failed output from consuming the final diagnostic delivery window and does
not change native ownership proof, public Rust APIs or persistence.
Repair hosted stdio error retention: an actual queued write/flush failure now
survives output-triggered input interruption with its original kind and message,
while preserving any earlier failure and independent output-drainage facts;
public signatures, error codes and formats are unchanged.

The installed panic hook now permits unwind only inside the hosted adapter
thread cleanup scope, recording the failure through bounded operator output
and original-control supervised shutdown. Legacy serve and other threads
retain fatal behavior; this does not establish native owner/handler panic
containment acceptance or change public APIs, protocol bytes or persistence.

Owned hosted stdio now scopes application publication through final response
admission, starts elicitation publication only after the answer, and guards
native decision passes through their journal returns. The feed defers without
advancing its watermark while any scope is active, using shared atomic control
without a Store or transport lock; scoped drop releases it on every exit.
Static guard handles add fixed per-session/host metadata, with one held guard
per request scope and no growing record/ID registry. This changes notification
ordering without journal/core/public API or dependency/MSRV changes; full
commit-barrier and guard-neutralization acceptance remains pending.

The provisional `filter_native_worker_panics` API wraps an embedding hook without
installing it, allowing unwind only inside internally marked opt-in transport
and proof worker bodies. Linux owned stdio uses that wrapper around its existing
fatal/adapter hook. Unmarked panics still reach the original hook, including a
thread with a forged worker name. Worker join failure replaces any published
response/proof with bounded uncertainty; it creates no new reap/EOF, closure,
outcome or claim release. Published actual observations may still establish
transport retirement after join. The additive function is inventoried and has
no journal, wire, hash-domain, dependency, core or MSRV change; ordinary unwind
is covered, while native handler/owner faults and installed Root lifecycle
acceptance remain distinct obligations.

Native completion verification now rejects caller-built responses exceeding
the existing 2 MiB conservative retained-response charge before cloning or
receipt access, including responses with small encoded content but large
allocation capacities; this changes no persistent format or public signature.
That preflight also enforces the default JSON depth ceiling before traversal,
preventing caller-built nesting from reaching unbounded recursive accounting.

Fixed owned native attempts requesting a second worker slot between binding
and execution, which could refuse already-bound work at pool saturation;
execution now reuses the original reservation after actual helper retirement.
Worker mode selected after standalone binding applies to successor startup too.
Binding, recovery and execution startup now preserve the original run's exact
absolute deadline rather than extending it while preparing the next request.

Pending-effect reconstruction now retains the historical emitting machine identity for subsequent contract admission across migration; the provisional Rust PendingEffect struct gains emitting_machine_id, requiring callers constructing literals to adapt, without changing journal bytes, hashes or runtime spawn policy.

The provisional read-only `contract::check_pending` API checks incompatible
later steps, current receiving outcomes and concrete pending arguments without
changing journal bytes or granting spawn permission; shared native services
now repeat these checks at queue, pre-claim and bound entry, with bounded
private structural caching as described below.

Pending-contract checks preserve explicitly manual effects as compatible evidence without changing their pending state or allowing automatic execution.

The shared native preparation path refuses incompatible or unknown pending
contracts before helper preparation, retaining pending work without new journal
entries or preparation reservations; pre-claim and bound-entry rechecks are
implemented, while final integration acceptance remains pending.

Fixed warm native owners remaining retained after another host delivered their original acknowledged event: exact acknowledgement and closure-bound handoff reconciliation now recognizes accepted-event fold retirement without another event send, preserving outstanding obligations and historical acknowledgement behavior.

Native execution refusal diagnostics now distinguish original binding validation, manager launch and enrolled entry authorization without exposing handler arguments or changing refusal, recovery or deadline policy.

Corrected warm completion delivery to preserve its existing verified physical-writer authorization while sharing accepted-event fold retirement checks with cold recovery; cold operator-route validation remains mandatory for cold adoption.

Fixed native retry scheduling after recovered timeout settlement: watcher observations now count native attempted settlements and preserve their original timestamps instead of reusing a consumed attempt key and leaving the pending effect stalled.

The portable lifecycle fixture also provides `noisy-exit`: it emits 16 KiB
of stderr, publishes its process/MCP result after the explicit root barrier,
and exits while both descendants retain stdout and stderr until their own
release. Independent portable observers require the complete noise stream,
successful root exit and delayed pipe EOF; this prepares repeated-host native
resource acceptance but does not establish that acceptance or native closure.

The forty-eight-scenario crash/resource coordinator adds twelve sequential
`noisy-exit` trees in each standalone/embedded process/MCP host. Each run uses
a separate protected observation directory and effect-argument substitution;
the observer pins one host PID/birth token, checks prior tree death and matched
store/claim closure before successor entry, and compares host descriptor/thread
counts and RSS at the same held-candidate phase after warm-up. The finite
fixture permits two transient descriptors/tasks and 16 MiB RSS growth, and
requires twelve claim/stop/settlement/event records. This extension remains
unverified natively and does not complete 9404 or establish an unlimited-run
resource bound.

Native preparation refused by worker capacity before helper dispatch stays queued for a later owner turn; it does not create an unknown allocation or publish an attempt outcome.

Private native owner decisions now follow durable progress or retained native
readiness for up to eight driver ticks using one logical timestamp, then offer
an admitted application command before continuing immediately if work remains.
An unchanged idle prefix or refusal ends the batch; the configured poll interval
still bounds idle rechecks. This removes timer delays between eligible
composition/deadline steps while preserving scheduler eligibility, pure core
explicit polls, journal formats and public API. Full scheduling fairness,
retry/compensation and transport acceptance remain plan 0020 obligations.
The private owner now injects its wait clock independently of logical time;
production uses the monotonic clock, while deterministic acceptance advances
wait deadlines without client commands or real sleeps. An exact wait deadline
is ready immediately. Original lifecycle shutdown deadlines remain unchanged;
no public signature, record format, hash domain or error code changes.


Owned native ordinary decision ticks also return retained bounded transport
uncertainty diagnostics through the existing operator output path, as
admission-free observation already does; a short poll interval cannot hide
those diagnostics by continually scheduling decisions. Diagnostic publication
changes no claim, outcome, closure proof, journal bytes or error code.

Completed native closure MUST tolerate only pre-mutation `authority busy`
contention within one two-second budget sampled at request entry; acquisition
and original-domain retirement share that budget. After acquiring the lock,
it MUST revalidate the protected authority's original device/inode identity
before reading closure material or mutating native state. Other refusals,
identity changes and exhausted budgets remain uncertain and cannot settle a
journal claim; public APIs, persistent formats and error codes are unchanged.

Matched native manager stop MUST retry only pre-mutation authority-lock
contention within its original two-second request budget and revalidate the
original protected authority identity after acquisition; acquisition, manager
inspection and stop share that budget. Closure cannot substitute waiting for
an unissued stop. Refusal or timeout remains uncertain, and persistent formats,
public APIs and error codes are unchanged.

Native preparation MUST wait before burning an allocation when a known domain
has a missing cgroup and an exact original-domain durable closing marker but
no closed tombstone. It MUST release the authority lock while waiting, retain
the original two-second pre-allocation budget, and revalidate authority,
registration, counter and full inventory on reacquisition. Missing or mismatched
closing evidence remains an immediate refusal; waiting never proves closure
or authorizes capacity release. Formats, public APIs and codes are unchanged.

Private native preparation refusal diagnostics include bounded static phase
names for inventory, facility readiness, cgroup creation and original leases;
phase context changes no allocation, journal record, public error code or format.

For a known missing native cgroup without a closing marker, preparation MAY
wait within the same original pre-allocation budget only after validating a
protected original claim binding whose complete domain matches the recorded
domain and a protected launch handoff whose binding matches exactly. This
covers natural exit before stop publishes closing. Missing, malformed or
mismatched binding/handoff remains an immediate refusal. Waiting MUST release
the authority lock and MUST NOT burn an allocation until the full inventory
proves original closure; handoff and absence never establish closure. No public
API, journal format or error code changes.


Hosted long read-only tools (`simulate`, `instance_history`, `journal_verify`
and `journal_replay`) run in session-owned workers against a fresh read-only
store, never on the writer owner. Each worker retains its original host/session
count and allocation reservation and original cancellation control until it
returns; worker concurrency is bounded by those existing admission counts.
Clock samples are requested through a one-message rendezvous with the adapter,
so injected clock behavior is preserved without introducing wall-clock reads.
Owned-input adapters continue lifetime and cancellation routing during these
waits. Session retirement cancels the original work and releases clock waits;
coarse-loop cancellation remains cooperative, and read-only store opening or
an operating-system read is not promised to be interruptible. Worker retirement
retains its charge until actual return and never holds the writer lock.
Adapter failure, including unwinding from an injected clock, cancels the
original session/request controls before releasing the clock rendezvous;
successful completion does not cancel other controls sharing that identity.

Hosted change feeds MUST cap their record interval at the original writer's
completed append prefix, shared by every session and retained after writer
retirement; complete bytes visible to a read-only opener do not establish
durability. The writer advances this observation only after successful append
and fsync, while each originating session retains its response-order guard.
The additive `Journal::committed_prefix` and read-only `CommittedPrefix::sequence`
API observes the original writer; a read-only journal returns no such observer.
This observation changes no journal bytes, hashes, error codes or versions.
Ordinary hosted tool responses retain their originating output guard through
response enqueueing even when no borrowed input adapter exists; read-only
diagnostics and unanswered elicitation continue allowing committed feeds.

A hosted output failure MUST invoke its one registered transport-retirement
action exactly once, outside the egress mutex and without waiting for its
writer. Late registration after failure invokes the action immediately.
The fixed callback metadata is separate from retained frame allocation;
internal registration captures only session/control handles and timeout.
Hosted session adapters close and cancel their original generation, while
stdio also requests original host shutdown without escalating a prior drain
or extending its deadline. Graceful output close continues draining normally.

Hosted elicitation settlement MUST retain the originating response guard on
its separate feed stream as well as its response stream, until response enqueue
finishes; unanswered questions continue allowing already committed updates.

HTTP server stop wakes admitted sockets before joining connection workers,
preventing silent and partial requests from spending the normal 30-second I/O
timeout during transport retirement; bounded read waits observe the original
stop flag on Windows without renewing idle deadlines or discarding partial
input, and finished workers explicitly close their
connections despite retained shutdown descriptors. This correction does not
establish native executor closure or interrupt application work outside socket
I/O, and changes no journal or wire format.

Shared native queue, pre-claim and bound-entry admission cache only successful
structural analyses in a private scheduler-owned set of at most 128 digests,
keyed by analyzer format, emitter/receiver, the complete available definition
catalogue and full private handler settings; cache hits still reconstruct
pending effects and recheck membership, concrete arguments and receiving
outcomes, and restart clears evidence. Concrete reconstruction still incurs the
journal-prefix replay cost at each decision. No public signature, error code,
persisted format or version changes.

Shared native ticks filter structural refusals before concurrency selection, then
repeat concrete checks under the writer; incompatible work cannot repeatedly
take the only slot, while retained owners remain observable and repaired work
becomes eligible on the next observation. This corrects admission starvation
without changing public signatures, errors, persisted formats or versions.

Shared service cancellation before native entry now requests original-claim
closure after its local binding helper retires, independently of writer access;
authenticated interrupted settlement still requires the original healthy writer.
This corrects cleanup of bound, unlaunched local claims without changing public
signatures, errors, persisted formats, hash domains or versions.

Operational cancellation acceptance now covers an entered native handler whose
instance is independently cancelled: the original helper retires, the owner
collects its protected completed result once, and interrupted settlement
consumes ownership without acknowledgement, outcome advance or recreated
pending work. Original claim/physical-store/closure checks and bounded refusal
remain required; authored focused controls are auxiliary until the installed
operational run passes.
