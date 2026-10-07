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

## Failure archival rerun at 4ce2aa5

The clean frozen MSRV producer at 4ce2aa5b8bf2ffce80ecb11a8666d91c5e686c68
terminated with native fixture exit 101 and no producer timeout; the first
four groups/nine scenarios passed, while the standalone/embedded race failed
Drain and the two-standalone group was not reached. The last actual observation
reported draining, closed admission, retired helpers, complete inventory,
released writer, no unresolved run IDs and one unclaimed reservation in
uncertain_preparation. These observations do not prove domain retirement or
successful shutdown. The root test completed in 41.84 seconds and its child
race in 9.32 seconds; workflow log SHA-256 is
ec887428b02dd76f6243484b28047cb20a36b520d3adfad3291714b9bfba76f7.
The report and log remain in local-native-workflow-4ce2aa5-msrv beneath the
dedicated task cache; matched staging and authority remain retained pending
guarded archival and retirement.

Failure-only archival now preserves original owner logs before fixture Drop,
with exclusive root-owned files and at most 64 KiB per source. The archived
embedded log records all seven settled/acknowledged effects, while the competing
owner records native-preparing check_identity followed by the generic native
completion refusal. This changes the next diagnostic action: preparation's
observe branch discards its actual error, and preparation/completion response
decoders replace a broker refusal with generic text. Neither the log nor the
inventory establishes authority-lock contention as the cause. Preserve the
actual bounded refusal reason before choosing a production correction; do not
release uncertain allocation ownership based on helper retirement or absence.
Task 9401 remains in progress and plan 0022 remains 3/7.

The residual 67f61c0 MSRV staging, HOME/resource and runtime drop-in were
archived under retained-fixture-complete before identity-matched retirement;
socket identities were recorded separately after a first copy attempt refused
socket contents without deleting anything. Exact staged digests, original
HOME/resource/drop-in identities and configuration bytes, no surviving staged
process, and already-retired original namespace/cgroups were checked before
removing the matched installation; no production closure verdict was created.

Failed workflow supervision now archives the last at most 65,536 bytes of each
of five original observer/owner stdout/stderr files before Fixture Drop can
retire their store. Sources use no-follow/nonblocking opens and must be regular
files; missing optional owners are skipped, and exclusive root-mode-0600
staging copies and their directory are synchronized. At most 327,680 bytes
per fixture are copied, with streaming memory rather than unbounded log reads.
This changes test diagnostics only, not production ownership or acceptance.
Stable/MSRV executor/CLI all-target Clippy and existing admission/cleanup
controls passed in terminal session 93178; the initial authority constant path
compile failure and corrected outcome remain in workflow-failure-diagnostics
check logs. Actual failure-path preservation and a complete MSRV producer
remain pending; the existing recurring second-launch stall is unresolved.


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

Native guard sensitivity completed in terminal session 28529, exit zero:
replacing only prepared cleanup's authority-busy retry with immediate refusal
failed the expected deadline error assertion (authority busy versus prepared
cleanup authority deadline), and extending only its entry deadline from two
to four seconds failed the three-second upper-bound assertion after 4.02 seconds.
Both mutated native tests exited 101, with zero passed and one failed; each
matched authority installation was retired only after clear native state.
The controller restored original closure.rs bytes in finally and verified
git diff --exit-code before rebuilding healthy controls; stable passed in
2.27 seconds and MSRV passed in 2.37 seconds, each one passed/zero failed/
zero ignored. The tracked worktree is clean after restoration.
Evidence prepared-contention-sensitivity-final.log has SHA-256
03690d9d1a3c15ac339ecd3e4159afc6dbfacdc159ee267cfccd2c05a74c6472;
prepared-contention-sensitivity.py preserves the exact mutations and controller.
Two earlier controller prechecks refused ambiguous replacement patterns before
changing source; their logs remain retained. This proves the tested retry and
deadline assertions are load-bearing, not complete concurrency integration or
other native verifier sensitivity; task 9401 remains in progress.

## Two standalone owners: fixture implementation

A sixth provisioned workflow group now exercises two actual standalone
executors against the same original physical store, expanding the producer
inventory from ten to eleven scenarios without dropping an existing group.
Handler discovery occurs with no pending effects; that embedded discovery host
is then retired before a plain MCP observer and the first standalone owner
start. The observer sends begin, the first standalone's real handler publishes
the parent/descendant marker, and a second standalone starts during that live
tree. Existing exact live-process identities, original single claim/unresolved
owner, unchanged external calls and journal checks remain in the race interval;
both standalone owners must subsequently confirm Drain/Stopped and exit zero.
The original full workflow history, seven acknowledged settlements, resource
state, root native closures, unused-domain and kernel memory receipt assertions
remain required. Control roots and diagnostic files are distinct per owner.
Stable/MSRV all-target executor/CLI Clippy and existing admission/cleanup controls
passed in terminal session 60447; actual eleven-scenario execution is pending.
No production capability changed and task 9401 remains in progress.

The first 52f1cb5 stable producer (terminal session 20089, exit one) passed
all original ten scenarios but failed the new standalone pair before handler
entry: the plain MCP observer entered writer mode and held the physical writer,
while first-stdout showed native-preparing check_prerequisite. The exact report
and workflow log remain in local-native-workflow-52f1cb5-stable, with the matched
authority/staging retained for protected archival and teardown; no native race
success is claimed. The fixture correction sends begin through the plain command
host before retiring it, starts the replacement observer while a test-owned
writer is held, explicitly checks read-only startup, then releases that writer
before starting the first standalone owner. This preserves the actual two-owner
race and avoids introducing a third embedded executor. Compilation and native
rerun of this correction remain pending behind guarded retained-fixture cleanup.

The failed 52f1cb5 fixture was archived and identity-matched retired using
retire-failed-workflow-52f1cb5.py before frozen 67f61c0 verification.
Stable session 24778 completed successfully with all six groups and eleven
scenarios, including both original live-tree races; exact-source clean status,
all markers/counts, terminal exit zero, no timeout, no retained installation or
stage fields and log SHA-256 were independently verified. Stable log digest is
2d74092cf6842a3d32a8e94a2008fb807fcebeba8a227726f3515815221960ad.
Stable/MSRV all-target executor/CLI Clippy also passed before that producer.

The matching MSRV producer in session 42460 terminated with exit one:
its first four groups/nine scenarios passed, but the existing standalone/embedded
race stalled in check_identity with inst-run/7/0 pending after its second
native-claimed/native-launched diagnostics; the new standalone pair was not
reached. Its original live-tree exclusion interval had returned, but that is
not whole-scenario success. Actual inventory reported running, incomplete
inventory, zero unclaimed reservations and all preparation counts zero.
The root fixture failed with exit 101 and no producer timeout; protected
inspection found the original namespace already retired by fixture teardown,
while staging, resource/HOME and runtime drop-in remained retained, so no new
production closure was inferred from absence. Evidence remains under
local-native-workflow-67f61c0-msrv, log SHA-256
7cfc8745cd9eef59aec5289bb77b97a0600523ebabfa1c0983801d6abbedab64.
The next review must retain original owner diagnostics before fixture Drop
and investigate the recurring second-launch stall; stable success does not
establish a fixed MSRV race or complete ownership integration.
Task 9401 remains in progress and plan 0022 remains 3/7.

The diagnostic correction preserves closed preparation/completion refusal
reasons with single-line 1024-byte UTF-8 limits and records preparation startup
and poll failures in the existing single pending diagnostic slot; uncertainty,
reservation ownership and shutdown deadlines are unchanged. Stable focused
closed-envelope/sanitization control and executor all-target Clippy passed in
terminal session 11019 under verified memory.max=1073741824 and
memory.swap.max=0; formatting, file-size and tracked diff checks also pass.
This helper control does not prove production failure delivery or diagnose the
original race; actual native rerun, MSRV checks, full host gate and guard
sensitivity remain required, after identity-guarded retained-fixture retirement.

MSRV session 48058 completed with all 19 native-client controls and executor
all-target Clippy passing under verified one GiB/zero-swap limits. Protected
inspection confirmed the failed 4ce2aa5 namespace and original cgroups absent,
with no registered matching units or live staged executable; exact manifest
identities and staged digests were checked before archival and retirement by
retire-failed-workflow-4ce2aa5.py. Original logs, resource/HOME, staging and
drop-in evidence are preserved in that report directory retained-fixture;
socket metadata is recorded separately. The matched installation was removed
with its device, inode and digest guard; absence supplies no closure verdict.

## MSRV eleven-scenario result at 0593350

Terminal producer session 47837 passed all six groups/eleven scenarios on
rustc 1.89.0, including both standalone/embedded and two-standalone live-tree
races, against clean frozen 05933508c2e9facb0b88efed416cc0b8f0b831e2.
Independent checks matched current source, exact scenario counts, all success
flags, exit zero, no timeout, no retained installation/stage fields and complete
log SHA-256 0dc494145a4305ebc611f1370b955e002b9d5a19483a427a2baaa178d071b8eb.
Evidence is local-native-workflow-diagnostics-msrv beneath the task cache.
The live controller scope measured MemoryCurrent=429924352 and
MemorySwapCurrent=0 during the final group; its kernel limits were verified
before dispatch, and native service memory receipt assertions remained enabled.
This is the first verified MSRV eleven-scenario success, not proof that the
intermittent preparation failure is fixed: the change preserves diagnostics
and makes no lock/admission correction. Current stable native rerun, full host
gate, production diagnostic wiring/sensitivity and broader crash/ownership
coverage remain required; task 9401 remains in progress and plan 0022 3/7.

Stable session 92662 also passed all eleven native scenarios at clean frozen
8caa82caf6b01365041ecf840717192ae1524973, using rustc 1.98.1 (48a229cea 2026-09-01).
Both toolchains executor/CLI all-target Clippy, formatting and source-size
checks passed before native dispatch. Independent report/source/count/digest
checks confirm terminal exit zero, no timeout and no retained installation
or staging fields. Stable workflow log SHA-256 is 0d8c4d4fd005141aed6685a0e274464029f92c79efdd5b9eae71976f64849611;
evidence remains in local-native-workflow-diagnostics-stable. Together with
the preceding MSRV result this proves both eleven-scenario executions, while
intermittent failure diagnosis, complete host gate and broader task 9401
requirements remain open; plan status is unchanged.

Preparation decode and public NativeCompletion::verify now have named refusal
controls reaching the production decoder, preserving exact sanitized reasons
while refusing domain/completion delivery; they cover the exact 1024-byte
boundary, ASCII limit-plus-one, a multibyte overflow and extra-field envelope
refusal. Both controls and executor all-target Clippy passed on stable/MSRV
in terminal session 5706 under verified one GiB/zero-swap limits; formatting,
file-size and tracked diff checks pass. These controls prove decoder wiring,
not owner diagnostic delivery, real broker failure reproduction or guard
sensitivity; those broader proofs remain open.

## Complete stable host gate at 1127dca

Terminal session 88327 exited zero against the frozen committed decoder-test
source 1127dca: all eight required stable host stages passed, including debug
and release workspace tests, all-target workspace Clippy, warning-free docs,
format/file-size checks, zero dependencies and embedding acceptance. Independent
log inspection verified exactly eight zero stage exits and zero failed stages;
tracked source remained clean. Complete native-refusal-full-stable-gate.log
SHA-256 is 43e31a10f9a1ba79b7e1b92f8272fb29d2805a020c5f329ea984f8b18a8fae79. The original scope used verified
one GiB RAM/zero-swap limits and sampled zero swap/no OOM throughout.
Ordinary ignored native cases are not established by this host gate; actual
eleven-scenario stable/MSRV evidence above predates only these test additions.
Hosted CI and unsupported-platform axes remain unexecuted for current source;
production failure delivery/sensitivity, legacy entry-point integration and
broader shutdown/recovery/crash requirements remain incomplete.

## Borrowed MCP native selection and workflow fixture

ExecutorLoop::new now selects native shared-tick admission. The borrowed public
session refusal test drives a genuine emitted effect through initialize/send
and forty additional ping ticks, requiring exec/mode, unchanged later records,
retained pending effect and no acknowledgement/claim without authority. The
previous unprovisioned happy-path ack assertion is replaced by this real refusal
contract; happy-path coverage moves to a registered twelfth provisioned workflow
scenario, invoking the public borrowed session helper in the exact staged test
artifact against the same seven-operation immutable catalogue and original
root closure/memory proofs. All eleven prior scenarios remain registered.
Only the helper libtest preamble is excluded from response decoding; helper
EOF exits after explicitly dropping original executor/store, with no shutdown
claim for live work. Stable/MSRV refusal and CLI/executor all-target Clippy pass
in terminal session 71682, with one GiB/zero-swap bounds; a redundant PathBuf
conversion was fixed after initial Clippy. All seven mocked producer retirement
controls pass with the twelve-scenario inventory (the first default-sandbox
attempt could not write its cache directory). Actual provisioned borrowed
execution, selection sensitivity, read-only constructor controls and renewed
full host gate remain pending; task 9401 and plan 0022 status are unchanged.

## Actual twelve-scenario borrowed execution at dd207d4

Stable session 43209 and MSRV session 56712 both exited zero with all seven
groups/twelve scenarios at clean frozen dd207d497f5baf117a47fde8782a20104754ea47.
Independent checks verify exact source, complete markers/counts including the
borrowed workflow, successful root status, no timeout, no retained installation
or staging fields and complete log digests. The new borrowed public session
executes all seven original handler effects and passes the existing durable
settlement/acknowledgement, physical-store, native closure, resource restoration
and kernel memory receipt assertions; both ownership races also pass.
Stable/MSRV reports and logs remain in local-native-workflow-borrowed-* under
the dedicated task cache; stable log SHA-256 is 54cbdbe5f7736d90ff0b1515039c9af328324eadd4272da60981f1d2731c8283
and MSRV log SHA-256 is 5b41f45b260c438ce7f7e5fc75fb8e7332e47ffce333352e1d95fcf62e3d9d32.
Sampled MSRV scope memory.current=343318528 and memory.swap.current=0, with
verified one GiB/zero-swap controller and original service preflight limits.
This proves borrowed happy-path execution; borrowed read-only controls, selector
sensitivity, independent bounded output/EOF stop, HTTP hosting, complete native
matrix and renewed full host gate remain pending. Earlier intermittent
preparation uncertainty is still unexplained; repeated success does not prove
its correction. Task 9401 remains in progress and plan 0022 3/7.

## Borrowed read-only workflow control

The provisioned borrowed scenario now first publishes its real pending begin
effect through an ordinary session without an executor, retains the physical
writer lease, and drives initialize plus three ping requests through a borrowed
read-only public session against the healthy native facility. After observer
EOF, a fresh read-only store must contain exactly the original journal records,
the held writer has no unresolved claim, and the external handler log must be
absent or empty; unexpected log read errors fail the control. Releasing the
writer then resumes the original seven-effect borrowed workflow, whose existing
root allocation count and original closure assertions remain unchanged.
Stable/MSRV CLI all-target Clippy, formatting and file-size checks pass in
terminal session 49403 under verified one GiB/zero-swap limits. Actual native
execution, guard sensitivity and renewed full host gate remain pending; the
held-writer assertion alone is a snapshot, while the fresh journal read proves
durable immutability, and neither identifies which layered guard refused work.
Task 9401 remains in progress and plan 0022 3/7.

## Actual borrowed read-only proof and host-gate finding at 43b7031

Stable session 20399 and MSRV session 98668 both exited zero with all seven
groups/twelve scenarios at clean frozen 43b7031cc9b1b2f536eaa8f2da126d4a774ec4b4.
Independent report and complete-log checks verify exact source, all scenario
markers, root exit zero, no timeout and no retained authority/staging fields.
The borrowed case proves the new read-only journal/handler assertions followed
by the original seven successful writable effects, including exact native
allocation counts and original closure/memory receipts. Cache evidence is
local-native-workflow-readonly-stable and local-native-workflow-readonly-msrv;
stable log SHA-256 is c3598d2155a989dc4dd269f645f27acb9ba1056e6a4425dfb6dafda1ab862430
and MSRV log SHA-256 is bbecd3bf8077780aa431b2769b81de872da60b1924c4d3e99eabe556802aef7f.
The MSRV controller sampled memory.current=95989760 and memory.swap.current=0.

Full stable gate session 91078 found the legacy positive expectation in
embedded_read_only::the_same_handler_starts_when_the_session_owns_a_writer:
the actual native constructor correctly reports exec/mode without authority.
After preserving the failing log, its exact scope invocation was stopped;
terminal exit 143 means the remaining gate stages were not completed and no
aggregate acceptance is claimed. The ordinary test now requires pending-work
observation, exec/mode, unchanged journal, retained effect, no unresolved claim
and no legacy spawn. Actual provisioned positive execution remains above.
All four embedded_read_only tests and CLI all-target Clippy pass on stable/MSRV
in terminal session 98800, with verified one GiB/zero-swap bounds; formatting
and file-size checks pass. Renewed full gate, selector sensitivity, HTTP hosting,
bounded borrowed stop and complete recovery/crash matrix remain pending.

## Borrowed native selector sensitivity at 1513df5

Terminal session 25108 exited zero: changing only ExecutorLoop::new's native
runner selector to Runner::new makes the public borrowed refusal test fail
with exit 101 at its exec/mode assertion, with an actual legacy spawned-handler
diagnostic. A finally block restores exact serve.rs bytes, then the same test
passes on stable/MSRV and tracked source is clean. Complete cache log
borrowed-selection-sensitivity.log SHA-256 is
3a0448952750cd73591d95e868e7b08d8a3dcfa4e95885f374f188086d94c1d6;
restored source SHA-256 is
27e6b24171996b0f333bffe11e3be27b9392b038ce80d8e5cff341f46e87696d.
The scope asserts one GiB/zero-swap limits before mutation. This proves native
selection through the borrowed public entry, not individual read-only guard
sensitivity, bounded stop or current-source full platform acceptance.

## Complete stable host gate at 44f05eb

Terminal session 77644 exited zero at frozen clean
44f05ebaab9df170debe1e1c0c4b2a918ad2ca47. Independent readback verifies exactly
eight zero stage exits and GATE_FAILED_STAGES=0: formatting, file size, debug
and release workspace tests, workspace all-target Clippy, warning-free docs,
zero dependencies and downstream embedding acceptance all pass. Complete cache
log native-borrowed-refusal-corrected-full-stable-gate.log SHA-256 is
2f3fa24f640f4b18618c39b911e5015dda0c9593afb07641f2f808d3ea41ef30.
The exact user scope asserted one GiB/zero-swap limits; sampled swap stayed zero.
This renews ordinary host acceptance after public service and borrowed native
selection, but ignored native cases rely on their separate provisioned evidence.
Older authority/feasibility native fixtures still need service limits before
the complete local native matrix can be rerun; HTTP, bounded borrowed stop,
complete recovery/crash proof and hosted/platform acceptance remain incomplete.

## Common authority-fixture service limits

Ordinary authority fixtures now install the same namespace-specific one GiB,
zero-swap service configuration before native work; workflow staging explicitly
retains its existing separate limit inventory. The public-service admission
axis uses this common ownership instead of installing duplicate configuration.
Common teardown archives actual memory receipts after matching each against
the original prepared domain and retires configuration only after successful
matched fixture cleanup. Inventories use exclusive root-owned directories
fsm-native-service-{namespace} under the explicit task cache. Failed assertions
retain the namespace and configuration rather than invoking cleanup in panic
Drop; missing receipt files do not manufacture native closure evidence.
Stable/MSRV executor all-target Clippy, formatting, file-size and diff checks
pass in terminal session 11012 under verified one GiB/zero-swap user limits.
Actual native execution and a renewed host gate remain pending for this test
change. The 44f05eb full gate above predates it; separate Python feasibility
probes still need their own service limits before the entire matrix is run.
All original tests remain registered; task 9401 stays in progress, plan 0022 3/7.

## First common-limits authority run and conservative retention

Stable session 48165 at frozen 97b1477937cbbe7c83310d5db8735698b8eb0f38 is
terminal exit one: the first five registered cases pass, but the multi-fixture
enrolled_gate_authorization case reaches its 30-second outer harness timeout.
No full native acceptance is claimed. The original helper remains installed
at device 2306/inode 94765497, SHA-256
9118fd5fa85fb9677bc0358ccb2ce182bbdc063eaafaf6dd9b13706e165f68ec.
Failure logs and authority-retained.json remain in
local-common-limits-authority-stable; controller log SHA-256 is
d065e313aefe6fa666a55f3620292e30a10eb3976a394bb3c876b0ba9f6b205c.
Later read-only retention-readback.json verifies 26 original namespace limit
inventories, 20 actual root-owned memory receipts with one GiB/zero swap,
clear original process/unit/group/job/authority inventories and exact retained
helper identity; no closure receipt was created or inferred from absence.
Automatic approval review rejected guarded helper removal because failed-case
instructions require retention; removal remains unexecuted, with explicit
approval requested for that exact original helper after rechecking the guards.

The enrollment harness now receives the same finite 90-second outer allowance
as other multi-fixture cases, reflecting its many sequential limit installation
and retirement operations; original native run/stop deadlines are unchanged.
The producer also retains exact helper identity after any failed or incomplete
case inventory, even when namespace state is clear. All seven mocked retirement
controls pass, and neutralizing only the complete-inventory guard makes the
named clear-state failure and timeout controls fail before exact source
restoration and a healthy rerun. Sensitivity log SHA-256 is
acd974899bd530cb23dad5ef59e18da63b5c7d1c5722dd724a9b0be52e4b33c0;
format/file-size/diff checks pass. Actual enrollment/full authority reruns,
other feasibility service limits and renewed full host acceptance remain open.

## Enrollment rerun with original helper retained

Terminal session 32347 exits zero at clean frozen
b3f316cf4bbf8f26ae33b946b3a755da076ccae3: actual enrolled_gate_authorization
passes in 33.14367136405781 seconds within the revised 90-second outer bound.
This measured duration exceeds the previous 30-second harness allowance;
native run/stop assertions and deadlines remain unchanged. Exact fixture
SHA-256 is c573d9f39eafe56d5f2f3403f6ec1cdc9d2dfc0c7edd6bbbd3165dcbc7c790a5.
Complete log SHA-256 is
765ef66f1c4737ba7270d7b78a1b809771cead1f47ab55177deb32ea645b8d25;
report/log remain under local-retained-enrollment-stable in the task cache.
The controller asserts one GiB/zero swap, source remains clean and original
authority state is clear afterward. Original helper device/inode/digest is
verified before and after, and it remains installed: no installation,
replacement or removal occurred. This proves enrollment with common service
limits, not all twelve authority cases, MSRV or complete matrix acceptance.

## Complete stable authority inventory with retained helper

Terminal session 32476 exits zero at clean frozen
2adb3565f565fee173c204eeaa5635a3daf9e32e: all twelve registered authority cases
pass with common native service limits, including enrollment, capture/status,
broker ownership/recovery, disconnect and process/MCP public service execution.
Every case exits zero without timeout and verifies clear authority state;
all twelve original case-log digests were independently checked against
local-retained-authority-stable/authority.json in the explicit task cache.
Their total measured case duration is 136.6618398865685 seconds, within the
existing 180-second suite allowance; native assertions/deadlines are unchanged.
Fixture SHA-256 is
c573d9f39eafe56d5f2f3403f6ec1cdc9d2dfc0c7edd6bbbd3165dcbc7c790a5;
report SHA-256 is
b8089dc23d5971f2a011323375de52e2435d67b0586ab4a15628e1ae327f25cc;
retained-authority-stable-controller.log SHA-256 is
3141db7b011ed3d48c76fd8c9f016dfbb1a6b8af712f45569c6cdf48c8ed3a45.
The controller verifies actual one-GiB/zero-swap limits, and native services
use the common original-domain preflight. The exact original helper identity
is checked before and after each case and remains installed unchanged;
the driver performs no installation, replacement or removal.
This proves the complete stable authority inventory through the retained-helper
driver, not the official nine-suite matrix or MSRV acceptance. Other feasibility
service limits, renewed host/platform gates, HTTP ownership, bounded borrowed
shutdown and the production crash matrix remain open; task 9401 stays in progress.
This evidence-only update runs diff checks; compilation gates are omitted.

## Remaining feasibility service launch limits

All sixteen literal Python systemd-run launch vectors now request MemoryMax=1G
and MemorySwapMax=0, including broker rejection/replay subprocesses and numeric
alias/controller units. The private Rust identity prototype adds the same two
properties to its handler unit. This closes the configuration gap for services
that systemd launches outside the capped test-controller scope. AST inspection
checks every Python launch vector for both properties; Python syntax, file-size
and diff checks pass. Actual kernel preflight verification for these older
fixtures, compiled checks and renewed native/host gates remain pending, so no
new runtime acceptance or task promotion is claimed.

### Actual kernel preflight wiring

Every Python transient launch now enables the fixture memory guard; contained
Rust prototype handlers use their existing contained marker. The guard runs
before publication, pre-entry, broker, identity or handler operations, checks
the current unified kernel cgroup for exactly 1073741824 memory.max and zero
memory.swap.max, rechecks original device/inode and writes a per-process
observation in the owned fixture directory. Python handler identity checks
match that observation to the original live cgroup and independently reread
both kernel limits; identity root-ready barriers invoke that check too.
The observation is a fixture diagnostic, not an authority closure receipt.
Terminal session 29404 exits zero: stable and MSRV lifecycle_platform Clippy,
formatting and file-size checks pass serially after actual controller kernel
limit assertions. Python AST/syntax checks verify all sixteen guarded launch
vectors. Native execution, guard sensitivity and renewed complete host/platform
gates remain pending; plan/task statuses remain unchanged.

### Native preflight execution and broker umask repair

At clean frozen a8b11e9fa75ac3d5ae20f9fd3c46d11dbeddd84f, terminal session
82950 passes all six systemd cases with independently verified original live
handler kernel limits; report SHA-256 is
79305f78ce332818cb2a102b7491d4f8850a9b49b5c519de96ab293f9a3fe597.
Session 56772 passes all ten identity cases (report SHA-256
3fdf8aeaba8bd7e25d4ef35210d7da161c66c0468de4987f43547a96b37e2f47)
then terminates on the failed broker suite; subsequent suites were not run.
Failed broker report SHA-256 is
62bba88c15e00fed8ba07078a621cae4c1c07ce83711d0991fe6cee9a01445ca.
The original journal identifies PermissionDenied reading memory.max before
handler entry: the broker's restrictive socket umask also makes manually
prepared cgroup controls unreadable to the dynamic handler identity.
The private prototype now sets its own prepared directory to root-owned 0755
and the two memory controls to root-owned 0644, allowing inspection while
preserving write restrictions. No production authority policy changes.
After adding the required PermissionsExt import, terminal session 20046 passes
stable/MSRV target Clippy, formatting and all ten actual broker cases on the
working-tree repair. Its report explicitly records source_dirty=true, so this
is preliminary evidence requiring a frozen rerun; report SHA-256 is
f7fc2bb8567911a19f490fb0c8d32f99fd3e63252c5c6bee61077281b11babbb,
log SHA-256 is
5960da7dee2768abc0ae671476cbf87fe77fbd4280e2baba6538022d550c6d8b.
All evidence remains in the explicit task cache; scopes verify one GiB/zero
swap, the retained authority helper is unchanged, and broader gates stay open.
