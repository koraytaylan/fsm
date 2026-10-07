# Provisioned ordinary workflow fixture review — 2026-10-07

The original four mcp_execute_workflow tests and their nine scenarios retain
their ordering, compensation, external-resource, acknowledgement and verified
journal assertions and original per-scenario deadline. A private test manifest
can supply separately registered physical stores, external resource paths,
the exact staged CLI and a compact private operator home; it does not select
a different executor. Without the manifest the existing test environment and
production CLI remain unchanged, including the currently failing Linux cases.

The root fixture uses the existing strict root-only store transfer and genuine
broker daemon, publishes each immutable catalogue before allocation, and runs
the copied original test binary after dropping supplementary groups, GID and
UID to 65534. The helper binary lives outside ProtectHome in an exclusively
created protected staging directory. Artifact reads reject symlinks, nonfiles
and more than 64 MiB, and authenticate bounded bytes before any executable
publication. The producer packages only debug-stripped exact Cargo artifacts;
the frozen full-gate CLI is 84 MiB unstripped, exceeding that unchanged bound.
Debug assertions and optimization defaults are not changed by packaging.

External resource files are initialized while their directory is root-private,
then made writable for distinct contained DynamicUser allocations; the work
result remains readable by its separate operator observer. Store/control paths
stay private and executable/catalogue paths stay protected. Native final socket
paths are checked against the transport limit. Cleanup checks original resource
and home inodes, then uses existing changed/populated/unknown-domain refusal;
only complete matched fixture teardown retires staged bytes. No namespace or
cgroup absence is promoted into a production closure receipt.

After each original scenario group, independent root readback requires the
exact native allocation and Claimed/Stopped/Settled counts, no unresolved run,
and an original protected closure receipt for every claim and physical store.
These checks reject a legacy route even if it produces the expected external
workflow effects. Four verified-case markers bind the producer's nine-scenario
verdict; missing markers cannot pass. The producer retains authority and staged
bytes on unknown teardown, saves timeout diagnostics with unknown exit status,
and preserves the original TimeoutExpired cause.

Seven permanent mocked producer tests pass; neutralizing namespace, staged-file
or missing-case guards individually fails their cached sensitivity controls,
with restored tests passing. No native process executes in these mocked tests.
Stable all-target CLI/executor Clippy, MSRV compilation, size checks and the
existing real env-cleared helper control pass in native-workflow-fixture-final-
check.log under serial asserted 1 GiB/zero-swap scopes. Earlier check logs record
a six-line size excess and then an ancestor-visibility compile refusal; the
setup is now in its own module and the original root-only ownership assertions
are unchanged. Actual registered production workflow execution remains pending;
compilation and mocked evidence do not promote task 9401 or release its gate.

## First native execution and fixture corrections

Frozen 9aa7c63 terminated with native test exit 101 in 4.67 seconds: the first
ordinary scenario reached its succeeded state but its legacy EffectAcked
history counter read zero versus seven expected calls. SPEC settlement is an
atomic ExecutionSettled/acked record, without a separate EffectAcked append;
the manifest-backed test now counts only durable acked settlements through a
read-only Store and cross-checks the MCP settlement-entry count, while ordinary
legacy counting stays unchanged. Independent root readback also requires every
settlement to be acked; attempted/interrupted dispositions never substitute.

The retirement guard retained exact authority and staged bytes. Subsequent
authoritative inspection found no authority namespace, production-profile
cgroup or owned executable process, with all seven external calls and active
phase; work was missing. An owned DynamicUser service with MemoryMax=64 MiB
and MemorySwapMax=0 reproduced deletion of its newly owned shm file while a
root-owned linked file survived, consistent with systemd's implied RemoveIPC
policy ([upstream documentation](https://raw.githubusercontent.com/systemd/systemd/main/man/systemd.exec.xml)).
The fixture now preprovisions a root-owned empty backing inode and the actual
perform_work handler publishes its work link and writes the real full/partial
result; work remains absent for rejected preflights, and containment policy
is unchanged. Protected staging inventories now retain original resource/home
identities for failed-run recovery. Matched cleanup verified staged helper/CLI
digests, original authority device/inode/digest, no live owned executables and
no production domains before removing only these test artifacts; this is owned
fixture teardown, not native closure inferred from absence or a passed verdict.
Failed evidence remains local-native-workflow-9aa7c63-stable. Stable all-target
Clippy, MSRV compilation, source size, seven producer tests and the real helper
argument control pass after correction (native-workflow-settlement-resource-
final-check.log); a fresh frozen native rerun remains required.

## Exact registered ordinary workflow execution

Frozen 9f744da663add6bb86480df27163492b3e6f5e1e passed all four original
workflow groups and their nine scenarios on stable Rust 1.98.1 (session 88622)
and MSRV 1.89.0 (session 55823), both terminal exit zero. Independent readback
verifies exact clean source, all nine scenario counts, each unique verified-case
marker and the complete log SHA-256 in local-native-workflow-9f744da-stable
and local-native-workflow-9f744da-msrv. Root assertions execute after the actual
ordinary CLI checks and require original native claims, matching closure
receipts, acked settlements and physical store identity for every call; legacy
effects cannot satisfy this verdict. Matched teardown completed without retained
artifact fields. Both reports keep gate_released=false. Controllers/builds ran
serially under asserted 1 GiB and zero swap; root-manager service units are
outside those scopes. This accepts these registered embedded workflow scenarios,
not standalone/signal/blockage races or full ownership integration.

## Mandatory privileged workflow classification

The four original workflow scenario tests now declare Linux native provisioning
as their ignore reason and link this review; other platforms retain ordinary
execution. The privileged supervisor explicitly selects --ignored, while its
existing exact one-passed/zero-ignored assertion and independent native receipt
checks prevent a skipped child from passing. Stable and MSRV native CI jobs now
require workflow_probe.py after the containment matrix and retain its report
alongside the other evidence, with no optional success path. Native CI scratch
and Cargo targets use a dedicated HOME cache with inherited TMPDIR.

Seven mocked producer retirement controls, formatting, source-size and diff
checks pass; an initial sandbox attempt could not write the external task cache,
and the authorized unsandboxed rerun passed. These mocked controls do not prove
native execution. Fresh frozen-source native stable/MSRV runs and the full host
gate remain required after this classification; hosted CI remains unexecuted.
Task 9401 remains in progress and plan completion stays 3/7.

Frozen 594425b7c3cad104710095b2f0b2c3f0551dee46 subsequently passed the actual
privileged workflow producer on stable and MSRV in serial session 44303,
terminal exit zero; local-native-workflow-594425b-stable and
local-native-workflow-594425b-msrv independently verify clean exact source,
four executed groups/nine scenarios, every unique original verified-case marker,
zero timeout and matching complete log SHA-256, with no retained authority.
This verifies --ignored executes the original scenarios rather than skipping
them. The controller's cgroup asserted 1073741824-byte memory.max and zero
memory.swap.max; separate root-manager units remain outside that scope.
The full host gate and hosted CI are still required and unexecuted here.

## Complete local stable host gate

Frozen 93a258f0ae08300b51a849394378296a632c13f5 completed session 98900
with exit zero and eight stage exits zero, ending GATE_FAILED_STAGES=0.
workflow-classification-full-stable-gate.log has SHA-256 7de2844a18e5146076cde9d20f73c5343ab561226ae3d12cf41375fae08ebb67.
All prescribed CONTRIBUTING.md host stages ran serially under asserted 1 GiB
and zero swap. The four Linux privileged scenario skips are supplemented by
the independently verified actual stable/MSRV producer runs at 594425b; the
intervening commit changes this review document only. Hosted CI and all remaining
production ownership/crash/shutdown requirements remain incomplete.

## Standalone/embedded live-tree control prepared

A fifth provisioned group retains the seven-operation happy workflow but holds
its first embedded handler and actual sleep child open with an exclusive
root-backed external marker. A real standalone CLI starts against the same
registered physical store and immutable table; the observer requires its
pending observation, unchanged original PID/start-time identities, one durable
claim and one unresolved owner through a bounded competition window, then
releases the original tree and requires seven original calls, normal domain
outcome and matching native receipts. Standalone receives an actual private
control drain after completion and must exit successfully; cleanup owns only
that exact process. Producer counts now derive from all five groups/ten cases.
Stable all-target CLI/executor Clippy, formatting, size and seven mocked
producer controls pass after correcting an initial missing path borrow; failed
and corrected logs are retained as workflow-race-focus*.log. Actual native
execution, MSRV checks, sensitivity and changed-source full gates remain
required; no race acceptance or task promotion is inferred from compilation.

The frozen 5d0e12c stable execution passed all original nine scenarios but
failed the new observer after ten seconds: native recovery correctly retains
the foreign claim without a Start directive, so it emits no observed-pending
launch line. This is an observer failure, not a proved ownership defect.
The control now requires actual paired startup and subsequent kernel read
counter growth across the live-tree competition interval, preserving PID/start
identity, one claimed/unresolved run, exact external call count and all final
receipt assertions. Read-counter growth is process activity evidence, not an
independent proof that a particular journal prefix was consumed; full race
acceptance must be assessed with actual results and call-chain review.

The failed producer retained its exact authority and protected staging. Matched
root inventory verified original authority/store/resource/home identities,
artifact digests, absent original PIDs and no namespace-prefixed native cgroup;
its failed manager still matched original ExecMainPID with no live PID/job.
All regular evidence and socket metadata were archived under
local-native-workflow-5d0e12c-stable/retained-fixture before resetting only that
matched failed unit and retiring the exact fixture and installed authority.
An initial archive attempt refused a Unix socket before any deletion; the
corrected archive preserves its metadata. This is test teardown and creates
no closure receipt or successful native verdict. Corrected formatting/size,
seven mocked producer tests and stable all-target Clippy pass; fresh execution
and stronger observation proof remain required.

Frozen be271fa passed the actual live-tree interval and completed all seven
embedded operations, but failed the standalone private drain transport deadline;
the producer correctly retained failed evidence and exact authority/staging.
The original catalogue counter advanced to eight: seven genuinely claimed runs
plus allocation two prepared by the competing executor, with no binding/closure
for that unclaimed domain. Source review found refresh/observe cancelled its
preparation helper immediately when a competing claim changed eligibility,
losing the opportunity to receive the authenticated allocated domain and clean
it. The corrected cancellation path retains the original finite preparation
transport and polls for delivery; cancellation still forbids binding/entry,
and only known delivery permits the existing prepared cleanup. Transport
failure remains charged uncertainty. New code/gates and actual native cleanup
execution remain pending; the retained failed fixture has not been retired.

Corrected stable CLI/executor all-target Clippy, formatting, file size and
seven mocked producer checks pass in terminal session 40244, retained as
native-preparation-cancel-delivery-check.log; native acceptance remains pending.

Fresh 2976089 stable producer execution (session 20432, exit one) still failed
standalone drain after all original scenarios and embedded workflow completion.
However protected original allocation-two now has exact matching closing-2 and
closed-2 records (fsm.native-domain-closed/1), absent in the earlier run: actual
prepared cleanup now executes, while this does not prove full driver shutdown.
The producer retained exact authority SHA-256
7d9fe96f598f11b3353ea93ce44330d404309bfd58218e6661d369fb8d612ec5 and staging
fsm-workflow-6f5284006093792f310c5948f6b7bb6f. The prior failed be271fa fixture
was archived completely, including socket metadata and original empty
allocation-two cgroup identity, before matched test-only retirement; no execution
closure was manufactured. Current failed fixture remains retained.

The race control now captures the competitor's actual JSON final error after
transport timeout with a one-second observer-only wait; this preserves the
original five-second driver request deadline and continues to fail on transport
uncertainty. It supplies missing shutdown facts before the owned child guard
retires the exact parent, without accepting the failed drain or increasing its
budget. Fresh execution and remaining ownership proof are still required.

The diagnostic run at frozen 03e1aab (session 57769, terminal exit one) again
passed all nine original scenarios and failed the new standalone drain transport;
the one-second observer-only wait found no final JSON diagnostic beyond paired
startup. Retained endpoint inspection found race-control empty after the exact
competitor exited, while the embedded endpoint remained; this is not a stopped
verdict or proof the control response arrived. Final rendering is best effort
under the expired original deadline, so absent stderr cannot establish a phase.
The exact report remains local-native-workflow-03e1aab-stable/workflow.json,
with its authority and staged fixture retained by producer guards.

The earlier 2976089 failed fixture was fully archived before matched test-only
retirement. Cleanup initially refused an original empty allocation-three cgroup;
protected prepared metadata matched its dev/inode and binding was absent, with
an inactive original manager/no PID/job and no staged executable surviving.
Only those matched empty fixture resources were retired, without manufacturing
an execution closure. Archived journal readback independently counts seven
ExecutionClaimed, seven ExecutionStopped and seven ExecutionSettled records;
handler log counts were not used as a substitute for journal evidence.

The next diagnosis needs bounded observation of actual local control inventory
before its deadline, including original unclaimed reservations/helper retirement,
while the mandatory stopped assertion stays intact. Existing stop-only transport
and process activity counters cannot supply those facts. This is aligned with
task 9401's bounded uncertainty health requirement, which remains unfinished;
no native race or complete cancellation/shutdown acceptance is claimed.

## Authenticated bounded local inventory observation

The private control now accepts fsm.executor-observe/1 as a closed original
four-field identity request and replies with actual ExecutorControl metadata
without asking it to stop. It uses the existing bounded transport loop, response
allocation, discovery and charged client-worker cap; stop clients still require
terminal reports and stopped reports require admission closed plus complete
empty/retired/released facts. Observation may report running/draining/stopping
and never supplies native closure, durable prefix or settlement authorization.
The race diagnostic now requires an authenticated running observation before
competition and records bounded actual snapshots alongside its original stop
request, preserving the five-second driver deadline and mandatory stopped
assertion. No production race acceptance is inferred from the new transport.

Terminal session 93284 passes stable and MSRV all-target CLI Clippy and all
19 real control tests, including writer-held observation with unchanged complete
records/state root/head, draining observation preserving the original deadline,
and foreign/extended/invalid-bound refusals without control mutation. Failed
first compilation used StoreState equality, corrected to the complete normative
state-root API; MSRV Clippy then exposed an existing explicit stdout-write lint
in the renderer readiness fixture, corrected separately in 2684165 with exact
bytes preserved. All four actual renderer controls pass on MSRV in session
99115 after correction. Logs are local-control-observation-final-check.log and
renderer-msrv-readiness-check.log.

Sensitivity session 41222 neutralizes only observation identity/schema equality:
the actual foreign/extended transport control fails with exit 101; exact source
bytes are restored in finally, and all 19 controls pass again. Its retained log
local-control-observation-sensitivity.log includes the verified 1 GiB/zero-swap
scope and terminal sensitivity marker. This proves the new refusal is wired
through actual transport, with no weakening of stopped eligibility.

The frozen workflow producer must next execute with these actual inventory
snapshots to diagnose retained competing ownership; the failed 03e1aab fixture
is still retained. Full changed-source host gates, provisioned stable/MSRV race
execution, complete bounded health and remaining lifecycle proof stay required.
Task 9401 remains in progress; plan 0022 remains 3/7.

## Actual competing-owner inventory diagnosis

Frozen a1ba97c stable producer session 45108 ended with exit one after passing
all nine original scenarios and reaching completed embedded workflow in the
race. The authenticated last report, retained verbatim in
local-native-workflow-a1ba97c-stable/workflow.log, proves admission closed,
phase draining, inventory_complete=true, helpers_retired=true,
writer_released=true, no unresolved local run IDs, and exactly one unclaimed
reservation before timeout. Thus writer retention and unretired helpers do not
explain the blocked stop; releasing the reservation without original domain
cleanup would weaken the guarantee. The failed fixture and exact installed
artifact remain retained by producer guards. This is actual control metadata,
not inferred process activity or a native closure proof.

The prior 03e1aab fixture was fully archived with socket and original cgroup
metadata before identity-matched test teardown; only its empty unbound original
allocation-three cgroup and inactive matched units were retired, with no
manufactured successful native verdict. Existing aggregate metadata cannot
distinguish whether the remaining reservation is an uncertain preparation,
known prepared cleanup, or uncertain claim publication. The next bounded health
step must expose that phase distinction from the retained original admission
state before changing reconciliation behavior; aggregate absence or helper reap
alone cannot release it. Task 9401 and plan completion remain unchanged.


## Coherent preparation phase observation

The observation request now uses fsm.executor-observe/2 and returns the closed
fsm.executor-observation-report/1 schema with ten named preparation counts;
legacy observe/1 keeps its original twelve-field report and stop responses are
unchanged. ExecutorControl::observation takes one metadata lock for both the
report and counts, publishing their total together; unpublished or poisoned
counts remain null. Both native owner routes publish the original admissions
phases, and received named counts must sum to unclaimed_reservations within
4096. Counts are diagnostic metadata, never authority to close a domain, drop
an uncertain reservation, or manufacture a stopped report.

Terminal session 59772 passes all twenty actual local control tests on stable
and MSRV, both CLI all-target Clippy checks and seventeen public API inventory
tests; local-control-phase-inventory-check.log retains the asserted one-GiB,
zero-swap scope. Actual socket tests cover unpublished versus published zero
counts and the legacy twelve-field response. Terminal session 39894 passes
stable/MSRV executor all-target Clippy, four admission controls on each
toolchain, formatting and file size checks, with evidence in
local-control-phase-classification-corrected-check.log; the classification
control distinguishes unknown allocation from uncertain domain and claim
publication. Initial executor Clippy failed on the existing workflow marker
stdout lint, fixed separately in 9120474 before the successful checks.

Review confirms no persistent journal bytes, native closure eligibility,
shutdown deadlines, dependencies or MSRV changed; the provisional public API
inventory and SPEC/API-POLICY/EMBEDDING/RELEASE move with this addition.
This does not yet prove the new phase snapshot in the provisioned race, whose
failed a1ba97c fixture remains retained, nor current-source complete host,
native or platform acceptance. Matched archival/test teardown of that fixture
must precede a new producer, and separate root-manager test services need
explicit memory and swap limits before further native execution.
Task 9401 remains in progress and plan 0022 remains 3/7.


## Retained a1ba97c fixture archival and matched teardown

The protected inventory identifies stage
/usr/libexec/fsm-workflow-3fde1be75801f257140544daf35ca758 and namespace
c1c8026bc78f29007df3f25145558da9; the original protected counter records eight
allocations, with prepared-3.json the sole unbound prepared domain. The guarded
retire-failed-workflow-a1ba97c.py completed successfully after verifying original
store/authority/resource/home identities, staged executable digests, absence of
live staged processes, matched inactive/failed units and empty unbound original
cgroups. Namespace, resource, home and staging evidence, socket metadata and
manager/cgroup inventories were archived under
local-native-workflow-a1ba97c-stable/retained-fixture before teardown; removal of
the installed test authority matched its original device, inode and digest.
No domain closure or successful race verdict was manufactured by this cleanup.

The retained fixture no longer prevents a fresh producer, but root-manager
native services still require explicit verified memory/swap limits before any
new native execution; the user-controller scope does not constrain them.
No heavy build or native test ran during this teardown, and all scratch stayed
in the existing dedicated home cache. Task and roll-up completion are unchanged.


## Namespace-scoped native workflow memory controls

The provisioned workflow now creates an exclusive root-owned runtime drop-in
for each original fixture namespace before starting its broker or operator;
MemoryMax=1G and MemorySwapMax=0 apply to that namespace's native services.
Systemd documents truncated dash-prefix drop-ins in
https://github.com/systemd/systemd/blob/main/man/systemd.unit.xml.
The protected recovery inventory records the drop-in directory and both
original directory/file identities. Successful matched fixture teardown removes
only that exact unchanged one-file directory and reloads the manager; failures
retain it alongside their evidence and require identity-matched later cleanup.
Production launch policy and unrelated services are unchanged.

Every provisioned handler has an explicit catalogue argument requiring actual
memory.max=1073741824 and memory.swap.max=0 from its own kernel cgroup before
logging calls or touching workflow resources. Missing or ineffective settings
fail the workflow rather than allowing acceptance under assumed controller
limits. These are per-service limits, not a claim of a shared aggregate native
memory budget or a diagnosis of previous host freezes.

Terminal session 66013 exits zero: formatting/file-size checks, stable/MSRV
executor and CLI all-target Clippy, and three ordinary helper/diagnostic tests
pass; five provisioned cases are explicitly ignored in this ordinary run.
workflow-memory-limits-check.log records the controller's separately asserted
one-GiB/zero-swap scope. Actual native limit assertions and phase-inventory race
diagnosis remain unexecuted at this source, as do complete changed-source host,
MSRV native and hosted platform acceptance. Plan 0022 remains 3/7 and task 9401
in progress; these compilation/helper results do not promote native acceptance.


## Native limit catalogue mismatch and correction

Frozen f096d504c86123c4c83ef45e7885c4a3e2894787 stable producer session 95516
terminated with exit one (native test exit 101): the first workflow stalled
before native launch, so none of the ten scenarios or kernel-limit assertions
passed. Protected records contain original allocation/claim but no launch or
entry; querying its original service finds no loaded unit/drop-ins. This is not
evidence that systemd applied or rejected the new limits. The root approved
catalogue included the new memory-check argument while the operator-generated
catalogue omitted it, changing the handler fingerprint and preventing approved
launch; the operator now includes the same argument only for validated native
fixtures, preserving ordinary helper contracts.

Corrected session 38491 passes stable/MSRV executor/CLI all-target Clippy,
formatting/file size and the three ordinary helper tests with five explicit
native skips; workflow-memory-limits-catalogue-check.log retains the asserted
controller limits. Actual native acceptance remains pending. The failed stage
/usr/libexec/fsm-workflow-fe3a0ea1f47dc50a7280b03e41745ffc and namespace
69a130488c43ca430f4053bb05cf1c05 remain retained, including their identity-tracked
runtime drop-in; archival and matched retirement must include that drop-in
before any fresh producer. The failed report/log remain in
local-native-workflow-f096d50-stable and task 9401 stays in progress.


## Corrected native launch and retained limit diagnostics

The failed f096d50 fixture was archived before guarded retirement, including
its original runtime drop-in; its sole prepared domain was unbound and empty,
so removal matched prepared-1.json rather than assuming a binding existed.
The installed authority was removed only by original device/inode/digest.

Frozen cc974a3bc3563eed1284d19f79e421a40a1338f9 producer session 66204 ended
with exit one (native test 101) after 2.02 seconds: the first workflow reached
a rejected outcome, with no external operation logged. Its original native
service journal records handler exit 101. Unlike the prior catalogue mismatch,
a service actually launched; this failure does not identify the failed handler
assertion and no scenario or race acceptance is claimed. The namespace was
normally retired by the existing matched Fixture cleanup after settlement,
while stage /usr/libexec/fsm-workflow-33e0d73f6799528506bb1c28acbd9013,
resource/home and identity-tracked runtime drop-in remain retained in the
protected inventory for namespace d20b5d58b48184bfe206cac1c49c8554.

A tiny independent transient unit under that exact namespace reports the
original DropInPaths and manager MemoryMax=1073741824, MemorySwapMax=0;
a second bounded two-second probe with the handler's DynamicUser,
ProtectControlGroups/ProtectHome/ProtectProc/RestrictNamespaces, no capabilities
and no delegation reads those same values from its exact kernel cgroup path.
These probes prove the configuration mechanism, not the failed actual handler's
observations. The next fixture preprovisions a root-owned writable diagnostic
file and the actual handler records kernel cgroup path and memory/swap read
results before asserting limits; no operation log or workflow effect is added
by diagnostic observation.

Terminal session 87415 passes stable/MSRV executor/CLI all-target Clippy,
format/file-size checks and three ordinary helper tests (five provisioned skips),
recorded in workflow-memory-observation-check.log under verified controller
limits. Actual handler diagnosis, limit guard sensitivity, provisioned race,
full host/native and platform acceptance remain pending; task 9401 and plan
0022 completion stay unchanged.


## Actual cgroup access refusal and privileged test preflight

The remaining cc974a3 stage/resource/home/drop-in were archived under its
retained-fixture directory before exact identity/content-matched retirement;
its namespace and original cgroups had already been retired by Fixture cleanup,
so no authority-domain closure was inferred or generated from their absence.
Frozen 9bab4bf7307ce4e962f6ae262685e05f25ed39f2 producer session 43358 failed
with test exit 101 before any original scenario passed. The actual retained
handler observation names its original service cgroup and reports PermissionDenied
(code 13) for both memory.max and memory.swap.max. Original prepared cgroups
are root-only; the independent manager-created probe did not reproduce that
permission boundary, so its success could not prove unprivileged handler reads.

The fixture now uses a namespace-specific ExecStartPre privileged test checker
before the original DynamicUser gate/handler, leaving their access restrictions
unchanged. The checker matches the actual current cgroup device/inode and service
name against protected prepared-<allocation>.json, asserts the kernel memory
and swap limits, and writes an exclusive root-protected fixture-only receipt.
Original workflow verification independently requires that receipt's complete
domain and expected values for every claimed allocation. It is diagnostic
resource evidence, never a native closure or settlement receipt; production
launch policy and published handler contracts are unchanged. The runtime
inventory includes exact configuration bytes for matched later retirement.

Terminal session 43318 passes stable/MSRV executor/CLI all-target Clippy,
formatting/file size and three ordinary helper tests with five provisioned skips;
workflow-memory-preflight-check.log records verified controller limits.
Actual preflight execution and guard sensitivity are pending. The failed 9bab4bf
stage /usr/libexec/fsm-workflow-70b2e5d6b51a740bbfa29fd2c9941746 and protected
resource/home/drop-in inventory for namespace 900133141f382ae4a34d25c5bf8e5cbf
remain retained for matched archive/cleanup before a new producer. Task 9401 and
plan 0022 completion remain unchanged; full native race and host/platform proof
are still required.


## Native preflight execution and canonical receipt correction

The exact 9bab4bf remaining fixture resources, stage, runtime drop-in and actual
permission-denied diagnostic were archived before identity/content-matched
teardown and installed-authority removal. Frozen
90eeef79663eea45838574dc4c7896e909f53d70 producer session 92096 ended with exit
one (native test 101), retaining the staged evidence: the original discovered
workflow test passed, with its retained log showing one passed, zero failed,
zero ignored in 4.18 seconds. The outer native verifier then refused its first
memory receipt as noncanonical, before any case marker was accepted; the
complete ten-scenario producer is therefore unaccepted. The workflow executed
through the privileged preflight but receipt bytes were not retained after
Fixture teardown, so this result is not independent archived per-domain limit
proof and does not validate the competing-owner race.

The fixture checker now serializes its restricted integer/string domain record
with sorted keys and compact separators required by the authority canonical
reader; no reader guard was weakened. Root supervision additionally archives
bounded original memory-receipt bytes into protected staging before verifying
each fixture, so a later verifier refusal cannot erase them during normal
matched namespace cleanup. Missing receipts remain a verification failure for
claimed domains, and these test receipts grant no closure authority.

Terminal session 19273 passes stable/MSRV executor/CLI all-target Clippy,
formatting/file-size checks and three ordinary helper tests with five native
skips; workflow-memory-receipt-check.log retains verified controller limits.
The failed /usr/libexec/fsm-workflow-9bd8cfad3c407b8b17f22b92d970c6c3 stage and
its resource/home/drop-in inventory for namespace 4ae92016a963bdce73596109d44b99f1
remain retained; protected inspection confirms the namespace was already
retired by Fixture cleanup. Corrected native receipt verification, sensitivity,
full race and changed-source host/platform acceptance remain pending, with
9401 in progress and plan 0022 still 3/7.


## Verified native memory receipts and uncertain-cleanup race phase

The prior 90eeef7 failed fixture was archived before exact resource/home/stage,
configuration/identity-matched drop-in and installed-authority retirement.
Frozen 82bbde440fcffca1f8a8035658eb81029194f39c stable producer session 32225
ended with exit one (native test 101) after 41.89 seconds: all nine original
scenarios passed their original assertions and root native verification,
including per-claimed-domain kernel memory receipts; the race failed its
mandatory competing-owner stopped assertion. No complete race or MSRV native
acceptance is claimed. The retained workflow log SHA-256 matches its report;
independent protected staging inspection finds 44 archived canonical receipts,
each with memory_max=1073741824 and memory_swap_max=0. These cover the nine
accepted scenarios, not a substitute for the failed race's final verdict.

The race's authenticated last version-two observation reports admission closed,
draining, complete inventory, helpers retired, writer released, no unresolved
local run IDs, and one unclaimed reservation classified uncertain_cleanup;
all other nine preparation buckets are zero. Protected original metadata has
eight allocations, seven bound claimed domains with closure material, and an
unbound allocation two with neither closing nor closed record. Read-only
kernel inspection confirms allocation two's original prepared device/inode
(29,5381215), populated zero; manager inspection reports not-found/inactive,
no PID/job/control-group. These facts do not authorize dropping the reservation
or manufacture an original domain closure. The next diagnosis must recover the
actual cleanup transport/refusal reason while preserving root identity and
revocation guards; preparation-phase uncertainty is no longer the sole missing
observation.

The failed stage /usr/libexec/fsm-workflow-d8b35d34ccc5d7d0bf18b470ad42d42b and
namespace afdc129fd292adafcd2cdef226eeb982, resources, original authority and
identity-tracked memory drop-in remain retained. Evidence lives in
local-native-workflow-82bbde4-stable/workflow.json and workflow.log and protected
staging. Full changed-source host gate, native MSRV, memory-guard sensitivity,
complete race and remaining ownership/crash/platform proof are still required;
9401 remains in progress and plan 0022 remains 3/7.


## Bounded original cleanup failure logging

The retained uncertain-cleanup transition previously discarded its actual
transport/refusal string. Native admissions now retain at most one diagnostic
line when that transition occurs and drain it once through both owned and
paired lifecycle polling, without journal/native I/O in the getter, additional
writer acquisition, a changed cleanup deadline, or release of any reservation.
The line is capped at 1024 UTF-8 bytes including its fixed
native-prepared-cleanup-uncertain prefix, normalizes controls to spaces and
omits additional simultaneous failures rather than growing a queue. It carries
transport/refusal diagnostics, never handler output or closure authorization.
The four capability documents move with this additive logging behavior; public
Rust inventory, control wire versions and persistent formats are unchanged.

Terminal session 32923 passes stable/MSRV executor/CLI all-target Clippy,
formatting/file-size checks and seven admission/control tests on each toolchain;
native-cleanup-diagnostic-check.log records the asserted one-GiB/zero-swap scope.
The new control covers exact 1024-byte and limit-plus-one inputs, UTF-8 boundary,
control normalization and one-time draining. This is helper/accounting proof;
actual driver failure emission and production-facing budget sensitivity still
require the provisioned control, and no race acceptance is inferred.
The original failed 82bbde4 namespace/stage and matched memory configuration
remain retained pending archive/teardown before that run; task 9401 and plan
0022 completion remain unchanged.


## Race variability and preserved broker refusal text

The original 82bbde4 failed race was archived with namespace/resource/home/stage,
original socket/cgroup/manager inventories and matched runtime configuration
before identity-guarded test teardown; no successful closure verdict was added.
Frozen 7d6b0fbe02bc9f1949e4a21d02199f64a1a2af5f stable producer session 12108
ended with exit one (native test 101), again accepting all nine original
scenarios but failing the race. This time it stalled after first acknowledged
settlement, with a second claimed effect and no external check_identity call,
before reaching the competing-owner drain observation. The retained original
second-domain records have binding and exec-status, but no launch/handoff;
all three original manager units are inactive with no PID/job. This is a new
observed failure window, not proof that earlier uncertain cleanup was fixed.

Review finds prepared-cleanup response validation previously replaced any
broker refusal text with a generic original-domain mismatch. It now preserves
a string reason only from the exact closed format/ok=false/result envelope,
capped at 1024 characters, before existing one-line 1024-byte diagnostic
formatting; successful responses still require complete original-domain
matching. The helper control pins exact refusal text and preserves malformed,
changed-identity and extended-response refusals. No deadline, closure guard,
reservation release or native authority eligibility was changed.
Terminal session 44592 passes stable/MSRV executor/CLI all-target Clippy,
formatting/file size, seven admission/control tests and the prepared-cleanup
refusal control on both toolchains; native-cleanup-refusal-check.log records
asserted controller limits. Actual failure emission and full native acceptance
remain pending.

The failed stage /usr/libexec/fsm-workflow-e4c52d395991f98314acb542dd6f26b2,
namespace 243b5c034fe924af35bbaca06f1afc6c and its original memory configuration
remain retained; evidence is local-native-workflow-7d6b0fb-stable/workflow.json
and workflow.log. Next diagnosis must inspect the original second-claim
execution failure and cleanup reasons, rather than infer native launch from a
host request log. Task 9401 stays in progress and plan 0022 remains 3/7.


## Retained execution observation errors

Review of the actual second-claim failure path finds NativeOwners::observe
ignores errors returned by NativeExecution::observe, though the latter already
retains transport refusal text in its ExecError. The owner now retains one
bounded native-execution-uncertain diagnostic on this error path and drains it
through the existing owned/paired lifecycle diagnostic channel, after any
pending cleanup diagnostic. The shared formatter bounds each line to 1024
UTF-8 bytes including its fixed prefix and replaces controls with spaces;
total pending diagnostic retention is two lines/2048 bytes. It changes no
ownership, native I/O, launch eligibility, deadline or retry behavior, and
never treats the diagnostic as original domain evidence. All four capability
documents record the changed aggregate budget.

Terminal session 66960 passes stable/MSRV executor/CLI all-target Clippy,
format/file-size checks, seven admission/control tests, the prepared-cleanup
refusal control and two owner provenance/cancellation controls on each
toolchain; native-execution-diagnostic-check.log retains asserted one-GiB,
zero-swap controller limits. These are preliminary compilation/helper proofs;
actual owned/paired failure logging and guard sensitivity remain pending.
The original failed 7d6b0fb stage/namespace and its second durable claim remain
retained, requiring original evidence-based reconciliation and guarded archival
before a new producer. Plan 0022 is still 3/7 and task 9401 remains in progress;
full race and changed-source host/native/platform acceptance are unproven.


## Original prelaunch reconciliation and actual authority contention

Before retiring the failed 7d6b0fb fixture, its original installed authority was
verified against protected device/inode/digest, original binding and cgroup
identity, absence of live original staged/authority processes, zero population
and no launch/handoff/entry. Namespace evidence was archived before invoking
only complete-close for allocation two through that original authority. It
returned success and produced a closure receipt matching the complete domain,
run ID and original journal-claim hash; receipt/revocation/closed bytes are
archived under original-prelaunch-reconciliation. Journal ownership remained
retained until test archival, with no manufactured settlement or race verdict.
The remaining original fixture and runtime configuration were then fully
archived before identity-matched teardown and installation removal.

Frozen df6ad430109991435cfec3790f079eae429ee983 producer session 36915 ended
with exit one (native test 101), accepting all nine original scenarios before
the race's mandatory stopped assertion failed. Actual last observation again
reports exactly one uncertain_cleanup reservation with complete inventory,
retired helpers, released writer and no unresolved local claims. The log digest
matches the frozen report. Normal lifecycle lines are on race-stdout, rather
than race-stderr: protected extraction identifies the actual bounded line
`native-prepared-cleanup-uncertain prepared cleanup refused: authority busy`.
It also records an execution uncertainty with generic unsuccessful broker
completion, which does not identify its root refusal. This proves the cleanup
diagnostic is wired through actual paired production polling; a root refusal
reason is now observed rather than inferred from a missing closure marker.

Prepared discard now retries only authority busy before any native mutation,
charging contention and original retirement to one existing two-second cleanup
budget instead of immediately stranding a known unbound domain. All original
identity, no-submission, revocation, manager and population guards remain;
other failures and exhaustion retain uncertainty, with no host deadline renewal.
Terminal session 94815 passes stable/MSRV executor/CLI all-target Clippy,
formatting/file size, seven admission/control tests and prepared-cleanup refusal
control on both toolchains; native-cleanup-contention-check.log records verified
controller limits. Actual corrected-root contention/sensitivity, full race,
current-source complete host/native and platform acceptance remain pending.

The failed stage /usr/libexec/fsm-workflow-c3a117d0f4bc31f2fcc3a8d5bcb62e99 and
namespace 14d8d0b46fee501c283d2e736bf0be75 with original runtime configuration
remain retained for guarded archive/teardown; report/log are in
local-native-workflow-df6ad43-stable. Task 9401 remains in progress, plan 0022
3/7, and plans 20–23 remain incomplete.


## Corrected contention run and failure-time competitor observation

The df6ad43 failed fixture was fully archived before identity-matched retirement
of original namespace/resources/home/stage, runtime configuration and installed
authority. Frozen 32e7cdf7d99d62acda40bf78873dfa379013433c stable producer session
55114 ended with exit one (native test 101), accepting all nine original
scenarios but failing the race before its final stop assertion. Actual journal
inspection shows the first claim stopped, acknowledged and its outcome event
applied; the workflow stalls at check_identity with no second claim, despite an
unbound prepared allocation two. The external calls contain only
check_prerequisite. Protected extraction finds no execution/cleanup diagnostic
line in either the retained workflow log or competitor stdout, so this run does
not prove contention retry success or isolate the retained preparation phase.
Its workflow log digest independently matches the source-frozen report.

The original race loop now obtains one authenticated competitor observation
only when its existing thirty-second workflow assertion fails, before the
competitor's exact child guard retires it. Observation has its own bounded
250-millisecond diagnostic timeout and returns null on unavailable inventory;
it cannot renew the workflow or driver deadlines or promote unavailable facts
to zero inventory. Original completion, external effects and stopped assertions
remain intact. This supplies actual phase evidence for early race stalls that
previously reached teardown without a control snapshot.

Terminal session 86987 passes stable/MSRV executor/CLI all-target Clippy,
format/file-size checks and three ordinary helper tests with five native skips;
workflow-stalled-inventory-check.log retains asserted one-GiB/zero-swap controller
limits. Actual failure-time observation, corrected contention sensitivity,
complete race and changed-source host/native/platform proof remain pending.
The failed stage /usr/libexec/fsm-workflow-d6dd6ca63ca907c912f54255307d18af,
namespace 7d303327335eab38c99567501a6311c0 and original runtime configuration
remain retained for guarded archive/teardown; report/log are under
local-native-workflow-32e7cdf-stable. Task 9401 remains in progress and plan
0022 stays 3/7; plans 20–23 remain incomplete.


## Completed race scenario and unused-allocation verifier correction

The exact 32e7cdf failed fixture was archived before matched resource, native
cgroup, manager, runtime drop-in and installation teardown. Frozen
ae89aee79d95193b59e8bfa0114f772855051216 stable producer session 84683 ended
with exit one (native test 101), accepting all nine original scenarios and
reaching successful race scenario completion before root verification. The root
verifier rejected last_allocation=8 versus its historical expected=7; its counter
assertion ran before independent claimed-domain verification, so this is not
complete native race acceptance. The original scenario reached completed
workflow and successful competing-owner stopped assertions, demonstrating
progress beyond earlier drain/stall windows without establishing repeatability.

The counter assumption conflated published handler claims with prepared
allocations: competing owners may prepare an extra original domain which must
be authenticated and retired unused. Ordinary scenarios retain exact counter
assertions. The race still requires exactly seven claimed/stopped/acknowledged
records and original per-claim closure/store/memory proofs, unique claimed
allocations within the original counter, and a bounded complete allocation
inventory. Every additional allocation must match protected original prepared
identity and complete closing/closed records, have no binding/submission/entry,
exec-status or preflight execution material, no original cgroup, and an unloaded
original unit with no queued job. No absent domain is promoted to cleanup
without matching durable original revocation and closed evidence. Original
unused prepared/closing/closed bytes are archived to protected staging before
verification can trigger normal fixture teardown; root refusal remains fatal.

Terminal session 22206 passes stable/MSRV executor/CLI all-target Clippy,
format/file-size checks and three ordinary helper tests with five native skips;
workflow-unused-domains-check.log retains verified one-GiB/zero-swap controller
limits. Actual corrected unused-domain verification and sensitivity, repeated
stable/MSRV native race execution and complete host/platform acceptance remain
pending. The failed /usr/libexec/fsm-workflow-0c35f95795dcdb3070a0a405d122cf39 stage
and its original protected inventory remain retained for guarded archival and
teardown before a new producer. Task 9401 stays in progress; plan 0022 is 3/7.


## Accepted ten-scenario stable and MSRV native workflow checkpoint

The exact ae89aee remaining failed resources/stage/runtime configuration were
archived before identity-matched teardown; protected inspection confirmed its
namespace had already been retired by Fixture cleanup, without deriving any
new closure from absence. Frozen 568ebf6 source then passed the complete
provisioned producer on stable (terminal session 29935, exit zero) and Rust
1.89.0 (terminal session 83776, exit zero). Each executed all ten scenarios,
including the actual standalone/embedded live-tree exclusion interval, original
workflow effects/journal/history, competing-owner stopped report, per-claimed
original closure/store/memory verification and complete unused-domain cleanup
verification. Both reports have clean exact source and no retained installation
or staging fields. Independent verification matches complete log SHA-256, one
exact root marker per case, all five groups totaling ten scenarios, terminal
one-passed/zero-failed/zero-ignored fixture result and actual MSRV rustc identity.
Evidence is local-native-workflow-568ebf6-stable and
local-native-workflow-568ebf6-msrv, with their workflow.json/workflow.log and
verified controller logs workflow-568ebf6-check.log and
workflow-568ebf6-msrv-check.log. Controllers remain serial one-GiB/zero-swap;
original native services separately verify their kernel limits before entry.

This accepts that concrete race scenario on both toolchains, not completed
ownership integration or general concurrency reliability. Earlier stalled
prepublication/prelaunch windows remain historical observed failures; two green
runs do not establish that every such window is fixed. Deterministic contention
and new verifier guard sensitivity, standalone/standalone races, complete crash
windows, remaining host routes, bounded shutdown/reconciliation, current-source
full host and 81-case native gates, hosted macOS/Windows and live-model evidence
remain required. Task 9401 stays in progress, plan 0022 remains 3/7 and plans
20–23 remain incomplete; gate_released remains false.

## Full stable host gate at 57052bd

Frozen 57052bdcf7e00e72abf4b740a9077aef3851200c completed the full stable
host gate in terminal session 70719 with exit zero: formatting, source size,
debug and release workspace tests, workspace all-target Clippy with denied
warnings, documentation with denied warnings, zero-dependency verification and
embed acceptance each returned zero; GATE_FAILED_STAGES is zero.
The retained native-race-full-stable-gate.log has SHA-256
2195b114576356ca1da454483400a99447cee4e134f26dac38c5705dc221ac9c
under the dedicated fsm-plan-native-matrix-20261005 task cache.
The controller used one build worker and one test worker in a verified
one-GiB memory scope with memory.swap.max=0; observed swap usage was zero,
with no OOM or OOM-kill events. The tracked worktree remained clean throughout;
the unrelated user-owned untracked workflow was excluded from edits and review.
The committed 568ebf6..57052bd review range passes git diff --check.

This closes the current-source stable host gate obligation for this checkpoint,
but ordinary workspace tests do not execute provisioned ignored native cases;
the separate ten-scenario stable/MSRV producer evidence above retains its own
scope. Current-source full 81-case native coverage, deterministic contention
and verifier sensitivity, standalone/standalone races, crash windows and the
remaining host, shutdown, recovery and platform obligations remain pending.
Task 9401 remains in progress, plan 0022 remains 3/7 and gate_released is false.

## Prepared cleanup contention fixture

The existing provisioned empty_domain_preparation case now additionally enters
production discard_prepared with a real protected authority lock held: expiry
must report the original two-second cleanup deadline and preserve the recorded
domain, original cgroup identity and absence of revocation, binding and closure.
A second call must remain pending while the lock is held, then close only that
original domain after release, with the matching durable closed record.
This adds no production behavior or public surface and preserves the existing
native case inventory. Stable and Rust 1.89 all-target executor/CLI Clippy and
the existing seven admission/control and one cleanup-refusal controls pass in
terminal session 83188; the initial check failed on module-path imports,
corrected before rerunning. Logs prepared-contention-compilation-check.log and
prepared-contention-compilation-corrected-check.log retain both outcomes.
Actual privileged execution and retry/deadline guard sensitivity remain pending;
compilation and portable controls do not prove those native assertions.
The earlier full host gate predates this test-only addition, so affected full
test gates require renewal before final review; task 9401 remains in progress.

Frozen dea71a7 then passed actual provisioned empty_domain_preparation on
stable and Rust 1.89.0 in terminal session 67420, exit zero, each with one
passed, zero failed and zero ignored; runtimes were 2.28 and 2.27 seconds.
The controller independently verified its kernel one-GiB/zero-swap limits,
installed each exact built authority with digest/device/inode verification and
removed only that matching installation after confirming no retained namespace.
Evidence is prepared-contention-native-corrected.log and its retained
prepared-contention-native-check.py controller in the dedicated task cache.
An earlier direct attempt in session 25406 failed before domain allocation
because the required installed authority was absent; protected readback found
no retained namespace, and that failure remains in prepared-contention-native-stable.log.
The successful runs execute both real-lock deadline retention and release-to-
matching-closure assertions, but guard-neutralization sensitivity remains pending;
they do not close broader race, shutdown, recovery or full native coverage.
