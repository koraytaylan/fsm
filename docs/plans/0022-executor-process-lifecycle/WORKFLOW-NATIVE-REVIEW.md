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
