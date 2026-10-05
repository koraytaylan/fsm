---
id: contained-handler-runner
title: "Contained Handler Runner"
workstream: "0093"
kind: task
depends_on:
  - durable-execution-claims
gated: false
touches:
  - crates/fsm-core/src/record/execution/ownership.rs
  - crates/fsm-core/src/record/execution/ownership_values.rs
  - crates/fsm-core/tests/execution_ownership.rs
  - crates/fsm-store/tests/execution_native/fixture.rs
  - crates/fsm-execute/tests/lifecycle_platform/evidence_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/systemd_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/native_matrix.py
  - crates/fsm-execute/tests/lifecycle_platform/authority_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/authority_install.py
  - crates/fsm-execute/tests/lifecycle_platform/verify_native_evidence.py
  - crates/fsm-execute/Cargo.toml
  - crates/fsm-execute/src/containment/
  - crates/fsm-store/src/store/execution.rs
  - crates/fsm-store/src/store/execution_crash_tests.rs
  - crates/fsm-store/src/store/execution_evidence.rs
  - crates/fsm-store/src/store/execution_tests.rs
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/src/value_limits.rs
  - crates/fsm-execute/src/config.rs
  - crates/fsm-execute/src/config/identity.rs
  - crates/fsm-execute/tests/handler_identity.rs
  - crates/fsm-cli/tests/http_session.rs
  - crates/fsm-cli/tests/machine_test_regen.rs
  - crates/fsm-execute/src/run.rs
  - crates/fsm-execute/src/run/
  - crates/fsm-execute/src/mcp_client.rs
  - crates/fsm-execute/src/error.rs
  - crates/fsm-execute/tests/lifecycle_runner.rs
  - crates/fsm-execute/tests/lifecycle_runner/
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - docs/EXECUTOR-LIFECYCLE.md
  - docs/EMBEDDING.md
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Contained Handler Runner

The task adopts the separately provisioned privileged authority binary and
its private modules as part of the proved native backend implementation;
adding that footprint does not declare the authority or contained runner
accepted before native integration review.
The native matrix and authority probe are adopted to execute the production
allocator on actual writable Linux cgroups at stable and MSRV.

The task also adopts the two CLI test harness repairs needed to execute the
required local stable gate with serial libtest and conventional commit hooks;
this does not add a production CLI capability or accept the native runner.

The runner reports a settleable result only after the entire owned domain is
closed; a root exit or MCP response cannot release surviving descendants.

**Steps:**

1. Implement the proved backend behind one runner path for process and MCP
   handlers, binding launch authorization and every result to the claimed
   run identity and preventing user code before domain enrollment.
2. Separate candidate outcome, domain closing, verified termination and
   uncertain cleanup; retain ownership when kill, wait or native inspection
   fails, and never convert those failures into a successful termination.
3. Make natural root exit, MCP result, timeout, cancellation and explicit stop
   close remaining descendants and future admission before returning a
   settleable result or releasing a concurrency slot.
4. Bound captured bytes during execution, including disk usage, while draining
   excess output; preserve truthful prefix/digest semantics and bound MCP
   worker, reader and handle cleanup even when descendants retain pipes.
5. Expose explicit cleanup progress for the service; keep `Drop` best-effort,
   update the provisional API inventory and publish the actual native limits.

**Tests:**

- `cargo test -p fsm-execute --test lifecycle_runner` uses real roots and
  grandchildren for both handler kinds, including early root exit, lingering
  MCP server, retained pipes, repeated spawning and uncooperative shutdown.
- Limit/limit-plus-one stream cases prove bounded memory and spool usage;
  noisy children keep draining, exact truncation/digest semantics hold and
  many repeated runs do not leak threads, handles or capture files.
- Injected native termination/inspection errors leave an uncertain result;
  no success, reaped-tree claim or freed capacity is reported prematurely.
- Root enrollment and cleanup tests are load-bearing against the production
  runner, and native stable/MSRV coverage plus public-surface/zero-dependency
  gates pass without falling back to weaker platform implementations.

**Historical acceptance review at source `31e0c6318cb45bcc5f76468d6586e172356e466a`:**

This table predates the interrupted-association deadline correction and does
not accept corrected product source `9f1f175ad91609359699e3a2d670119e8cbb506a`.
At that corrected source, CI `37376949993` independently verifies 81 native
cases on each compiler and passes zero dependencies plus Ubuntu stable's full
gate (`111988370337`, including debug/release workspace tests, all-target
Clippy, documentation, fuzz compilation and decimal regeneration).
The other five portable axes remain live; local frozen host verification and
complete contract reconciliation remain outstanding.

| Requirement | Inspected implementation and evidence | Remaining acceptance |
| --- | --- | --- |
| One claimed process/MCP enrollment path | `containment/runner.rs`, `exec_status.rs`, `enrollment.rs` and `runner_native_tests.rs`; independently verified stable/MSRV artifacts from CI `37365652966` execute the production authority suite through the frozen `lifecycle_runner` target. | Complete frozen-range spec/API review; all six portable gates now pass. |
| Candidate separated from verified closure and uncertainty | `runner.rs` publishes completion only after `closure::complete`, matching `VerifiedClosure`, and owned child/worker retirement; corrupted handoff cases retain the original claim without a receipt or successful result. | Preserve these controls in the final acceptance range. |
| Root exit, response, timeout and cancellation close descendants | Native runner controls independently observe root, child and grandchild membership, then exercise process exit/failure/signal and MCP response/protocol/timeout/cancellation paths. | Production service routing and host capacity integration belong to dependent task 9401 and remain incomplete. |
| Bounded capture and resource retirement | `capture_native_tests.rs` checks both handler kinds at 4096, 4097, hash limit, hash limit plus one, and eight MiB; exact prefixes/digests, bounded result material and repeated FD/thread inventories are verified. | Separate frozen local stable host invocation remains unexecuted; full CI host gates pass. |
| Explicit cleanup progress and best-effort Drop | `NativePreparation`, `NativeRun` and `NativeExecution` expose bounded phase/helper progress; observation does not require a writer and settlement retains completion on refusal; authority `OwnedRun::drop` fences without manufacturing closure. | Compiled portable public-surface and zero-dependency gates pass; aggregate review remains required. |

The independently verified native inventory contains 81 cases per compiler;
this count describes the whole matrix, not 81 runner-specific controls.
The artifacts do not independently compare executable bytes and do not release
the production native gate; task status remains `in_progress`.

The earlier command-level coverage gap was repaired at `0cef6f6`: the named
`cargo test -p fsm-execute --test lifecycle_runner` target now includes claimed
native process/MCP grandchild controls, selected by `authority_probe.py` under
the provisioned Root fixture; an ordinary unprovisioned invocation leaves those
native controls ignored and cannot alone prove this requirement.


- **Done when:** the same production runner passes native process and MCP descendant/pipe/capture tests and returns a settleable result only with matching run identity and proved closed containment, while every uncertain cleanup remains explicit and bounded.

The coverage repair compiles the production authority and its existing native
controls directly into `lifecycle_runner`; the provisioned probe selects that
exact Cargo test artifact while still installing the separate production binary.
Reports identify the test target, and the independent verifier derives the
required target from frozen source; verified provisioned native execution is
the evidence for those ignored controls, not an ordinary portable target pass.

CI `37363278015` at frozen repair `0cef6f6` passes native stable and MSRV;
independent artifact verification confirms `lifecycle_runner` as the fixture
target and all 81 matrix cases per compiler, including the existing claimed
process/MCP descendant controls, so the named-target coverage gap is repaired.
Full portable and frozen host gates and aggregate review remain pending.

Current CI `37365652966` independently verifies the same named-target identity
and all 81 cases at both stable and MSRV after the borrowed handler-input guard
and all four production refusal branches were added; the retained earlier
stable failure at `7bd363a` remains unexplained and is not erased by this pass.
Static cross-checks find the original-completion retention and admission bounds
in SPEC's native host contract, matching provisional API/embedding/release
text and `NativePreparation`/`NativeRun`/`NativeExecution` inventory entries;
these source checks do not replace compiled public-surface or full gate tests.

Aggregate review, candidate-to-completion slice at the frozen source above:
`runner::execute_cancellable` collects process/MCP/timeout/cancellation candidates
without publishing them, cancels the protocol worker, fences admission and
requires `closure::complete` plus a readable `VerifiedClosure` before retiring
owned child and worker handles and publishing completion.
`closure::complete` validates the original binding and handoff, requires revoked
entry and repeated manager retirement with original cgroup absence, retires the
exec-status endpoint, rechecks protected records, then syncs an immutable receipt.
`completion_record::publish` verifies original claim/hash proof, bounds the full
response, syncs its response-hash attestation and verifies `NativeCompletion`
before publishing the protected recoverable response; recovery reads and
verifies that response without launching.
The runner's uncertain process/MCP handoff controls assert no receipt, no
recovered completion and an unresolved journal claim even after fixture-owned
repair and independent closure; successful modes verify original result identity.
No acceptance-blocking finding was identified in this slice, but this is a
limited source review rather than acceptance of the remaining enrollment,
transport, capture, settlement and spec/API portions of the complete range.

Aggregate review, capture/worker slice at the same frozen source:
`NativeCapture` uses the Linux socket implementation of `StreamCapture`, so
native output creates no capture spool file; polls drain at most 64 KiB per
stream and final draining has a fixed budget.
`Capture` retains only the 4096-byte prefix, stops hashing after the one-MiB
work limit and emits a suffix digest only after observed EOF within that limit;
read failure or a retained peer cannot earn complete-stream evidence.
`NativeProtocol` adopts enrolled streams with cloned shutdown controls and
joins only an already-finished thread; collecting a candidate requires that
join, and the authority's completion path separately checks worker retirement.
The native capture control exercises both handler kinds at prefix/hash exact
limits, each limit plus one and eight MiB, asserting exact output/digest,
bounded result material, closed cgroup and unchanged FD/thread inventories
across ten runs; the journal claim remains held after each completion.
Socket unit controls independently keep peers alive during blocked read/write
cancellation and final capture, so peer drop does not substitute for retirement.
No acceptance-blocking finding was identified in this slice; historical
non-Linux direct-child capture is outside the proved native path and does not
provide fallback acceptance for task 9401.

Aggregate review, host transport slice at the same frozen source:
`NativeRequest` validates the fixed route, bounded canonical request and
protected ordinary helper before spawning with a cleared environment;
nonblocking output retains only response limit plus one overflow byte and a
4096-byte diagnostic prefix, draining with separate 64-KiB budgets.
Response decoding requires exact length, canonical closed shape, successful
helper exit and both observed EOFs; deadlines and failures cancel the helper
without proving domain closure, while `Drop` makes only a bounded reap attempt.
`NativeRun` retains the original claim/hash and one overall deadline across
binding and execution; a successful bind advances to a separate later poll,
and only verified original completion changes its phase to closed.
Recovery requests only the original allocation and never transitions through
binding or execution; helper cancellation leaves the run uncertain.
Framing, retained-writer, request-policy and recovery-alias unit controls plus
the provisioned supervisor's read-only recovery and retained-completion cases
support these boundaries; production service admission remains task 9401.
No acceptance-blocking finding was identified in this slice; enrollment and
settlement/spec/API review still remain, together with frozen local gates.

Aggregate review, launch/enrollment/grant slice at the same frozen source:
`launch::begin` holds fresh binding validation through exclusive durable launch
intent and protected handoff publication, refuses previous submission or entry
grants, and reserves envelope limits before submitting the isolated systemd gate.
Incomplete submission revokes admission under that lock and never removes the
launch intent to permit resubmission.
`enrollment::inspect` compares manager invocation/policy, routed membership,
installed executable identity, bounded proc credentials and private descriptors,
then repeats observations and cgroup/installation identity before returning.
`authorize::publish_enrolled` repeats current binding/catalogue validation and
actual gate inspection under the authority lock; only the matched dynamic group
receives an exclusively linked, synced grant.
The gate refuses Root, verifies its route/boot/cgroup/authority identity and
rechecks the protected grant plus closing state before exec; final validation
races depend on closing's native fence rather than pathname checks alone.
Native enrollment controls refuse missing/altered handoff and prove approved
execution and stopped-gate retirement; their passing frozen matrix is evidence
for this production path, not permission for a host to skip durable claiming.
No acceptance-blocking finding was identified in these inspected operations;
private exec-status authentication, settlement and the complete spec/API cross-
check still require review, and frozen local host verification remains pending.

Aggregate review finding at frozen `31e0c63`: private exec-status association
does not enforce its shared deadline on every interrupted I/O retry.
The listener accept loop continues directly on `Interrupted`, bypassing its
bottom-of-loop `remaining(deadline)` check, and the hello read loop checks the
deadline only for `WouldBlock`; repeated interruptions can therefore extend
association beyond SPEC's two-second bound.
This blocks acceptance of the frozen range until both production loops check
the shared deadline for every retry and a production-facing fault control
proves interrupted retries cannot bypass it; the affected native and portable
gates must then be rerun at the corrected source.
The existing green matrix remains evidence for `31e0c63`, not proof that this
previously untested refusal is load-bearing.

Aggregate review, settlement slice at corrected source `9f1f175` (unchanged
from the earlier frozen source): `NativeExecution::settle` requires a healthy
durable writer and original physical-store proof, persists matching stopped
evidence before disposition, and retains completion/capacity on refusal.
`Pipeline::settle_native_stopped` checks current original claim hash and exact
stopped outcome, selects disposition from original policy and current pending
state, and uses existing derived ack/attempt keys or the original run's
interruption key; it consults no changed handler table.
`Store::settle_execution_on` validates a projected ownership/instance transition
and commits one settlement record before publishing projected state; replay
uses the original claim/disposition fingerprint without claiming unused keys.
`advance_native_settled` requires exact acknowledged settlement response and
matching original result before using the retained original contract's advance,
preserving acknowledgement-before-event and existing event request keys.
Native retry/interruption/owned-settlement controls and store abrupt-death,
write/fsync-failure and copied-store controls exercise these boundaries;
corrected-source compiled verdicts remain pending despite unchanged code.
No acceptance-blocking finding was identified in this slice; integrated host
race/crash recovery remains task 9401 rather than evidence supplied by these
primitive-level controls.

Private exec-status review at corrected `9f1f175`: Root obtains a bounded
kernel-random challenge before launch, protects original socket metadata and
revalidates the original sole enrolled gate before and after its exact PID/nonce
hello; private paths are identity-matched and removed before grant publication.
The status descriptor is checked close-on-exec; a positive OS error requires
the exact 12-byte frame and EOF, while partial/excess/malformed frames retain
uncertainty and empty EOF supplies only exec observation, never domain closure.
Native controls independently reject the same-identity wrong-nonce actor,
classify actual missing/non-executable commands through the private stream,
and refuse original-path replacement during retirement.
The deadline finding is corrected by checks before each production accept/read
retry; both injected branches cross the actual shared deadline and assert no
subsequent accept/read, entry grant or stopped record, retaining durable claim.
Both independently verified 81-case native compiler matrices execute these
controls at corrected source; SPEC and API/embedding/release text agree that
formats, hash domains and public APIs are unchanged and fresh gates are required.
No further acceptance-blocking finding was identified in this portion; the
full spec/API review and frozen local verification remain outstanding.

Contract cross-check, typed admission at corrected `9f1f175`:
`Pipeline::claim_native_handler` derives effect identity from the journal,
rejects name mismatch, validates one bounded canonical original contract and
derives both fingerprint and retry from it before publishing ownership.
`identity::checked_material` charges borrowed argv/retry/tool/argument and
both outcome payload/stamp branches before cloning, then charges the complete
normalized envelope before hashing `fsm:handler-contract:1`.
Post-publication checks require current matching ownership/domain/contract and
available original claim hash; duplicate responses cannot recreate consumption.
`start_native` separately requires healthy durable writer, current original
hash, enabled admission, no stopped result and running pending eligibility.
This matches SPEC's typed native admission and API-POLICY's borrowed-input
boundary; exact canonical limit/plus-one and all four production refusal
branches have native evidence at the corrected source.
No additional finding was identified here; this entry covers the claim/admit
contract portion and does not complete the entire frozen-range contract review.

Contract cross-check, result evidence at corrected `9f1f175`:
`NativeCompletion::verify` bounds caller material before canonicalization/hash,
requires closed response/result shapes and exact original claim/hash, decodes
the original contract against its fingerprint/retry/kind, and requires the
derived original receipt route before reading protected closure evidence.
The full response digest under `fsm:native-response:1` must match a separate
immutable Root attestation for that domain/run/claim hash; a valid closure
cannot authenticate substituted candidate or policy material by itself.
`VerifiedClosure::check_store` compares the registered physical directory
device/inode before application; unsupported platform proof readers refuse.
This agrees with SPEC's response-attestation contract and provisional API policy;
native missing/torn/symlink/writable-attestation controls and copied-store
refusal provide targeted evidence, while the full workspace gate remains live.
No additional finding was identified in this portion; registration/allocation,
broker policy and complete range review still require aggregate reconciliation.

Registration/allocation review at corrected `9f1f175`: registration opens the
canonical physical store read-only before exclusive authority publication,
keeps its path private and publishes synced immutable public device/inode
identity; it neither takes the store writer nor creates launch permission.
Preparation validates the installed profile/catalogue/facility, serializes on
the protected authority lock and checks bounded original intent inventory
before publishing intent and advancing the durable counter ahead of cgroup
creation; partial preparation cannot reuse the allocation.
The initial cgroup sample requires protected original identity, bounded empty
and unfrozen events, then repeats directory identity before prepared publication.
Binding requires original authority/prepared/boot/cgroup identities, no closing
marker, fresh current claim/hash and approved catalogue, and repeats physical
store identity after its read-only journal verification.
These operations agree with the native registration/allocation contract and
existing fault/inventory controls; no further finding was identified here.
Production route discovery remains an explicit dependent task 9401 obligation,
and none of these primitives implements it implicitly.

Broker dispatch review at corrected `9f1f175`: bounded canonical closed frames
allow only prepare/bind and allocation-only execute/close/observe/recover;
there is no caller command/path or grant-publication action.
Frame read/write loops charge every retry, including interruption, against a
shared 500-ms deadline, and session admission retains at most eight owned
threads, joining completed sessions before reuse.
Execution observes connection loss, trailing bytes and endpoint guard failure
through cancellation shared with the same claimed runner, then joins that
runner before returning; a response failure leaves journal ownership untouched.
The ordinary helper's lifetime watcher makes owner EOF close its transport,
without treating helper death as native closure or settlement.
Native disconnect/trailing-byte and frame-policy controls support these
dispatch boundaries; no additional finding was identified in this portion.
Endpoint provisioning/leadership checks and the full frozen range still need
their remaining review; host shutdown/reconciliation remain dependent tasks.

Endpoint leadership review at corrected `9f1f175`: provisioning excludes Root
and the reserved dynamic handler identity range from operator access;
leadership retains a protected lock descriptor and repeatedly compares its
pathname identity, broker/authority identity, original boot and configuration.
Epoch startup checks bounded complete history, refuses incomplete publication,
durably burns a new epoch before socket creation and publishes its synced
immutable route only after restrictive bind permissions and operator ownership.
The client derives only that published socket, verifies original authority,
boot, operator and socket identity/access, then repeats route verification
after connect and before dispatch; connection remains in the host-owned helper.
The checked supervised stdin is nonblocking and initial framing is bounded by
the original 500-ms deadline; the owner lifetime watcher survives blocking
connection/response I/O and never mutates journal ownership.
These checks match the provisional endpoint contract and native takeover,
route-replacement, epoch and client-lifetime controls; no additional finding
was identified in this portion, and complete-range acceptance remains pending.

Stop/revocation review at corrected `9f1f175`: closing serializes with authority
admission, validates original prepared/boot/cgroup identity, syncs an irreversible
closing marker before removing both visible and pending grants, then syncs the
directory; repeating a visible marker repeats durability rather than assuming it.
Manager stop revokes a live original group before reading potentially damaged
handoff, then requires original binding, invocation and policy before stop/reset;
an absent group cannot substitute for that original handoff verification.
`stop::fence` preserves manager refusal while independently attempting original
kernel freeze/kill under the lock, rechecking parent/control identity before each
write; neither submission supplies closure proof or journal disposition.
Only the separately reviewed complete-close protocol can publish a receipt.
Native corrupted-handoff, refusal and original-identity termination controls
support these boundaries; no additional finding was identified in this portion.
Manager observation/retirement parsing and complete-range reconciliation remain
to review before aggregate acceptance.

Manager observation review at corrected `9f1f175`: fixed protected systemctl
queries clear the environment and use nonblocking bounded output/diagnostics;
each drain has a fixed iteration budget, including interrupted reads, and
successful observation requires actual helper reap plus both EOFs.
Property parsing rejects missing/extra/duplicate keys; retirement requires
the original unit absent from loaded inventory and no matching queued job,
rejecting malformed or noncanonical job rows rather than guessing retirement.
Failed-unit reset requires original invocation and ExecMainPID after stop and
does not supply closure; original cgroup absence and repeated retirement remain
the complete-close protocol's responsibility.
Exact capture limit/plus-one, retained-peer, expired-query, property/job parser
and provisioned native stop controls support this portion; no additional
finding was identified, and earlier unexplained manager deadline failure
remains retained evidence rather than a reason to relax deadlines.
Complete-range review reconciliation and frozen local gates remain outstanding.

Changed-file reconciliation at corrected `9f1f175` identifies remaining direct
review of core ownership deltas, authority publication/read helpers and dispatch,
catalogue validation, process-root exit projection, NativePreparation and the
historical Runner/MCP-client changes, plus evidence verifier/test harness deltas;
the reviewed slices above do not silently cover those files.
The additional observation/unlaunched-closure slice requires exact protected
bounded populated/frozen events and repeated original domain/phase identity;
population is never a receipt. Unlaunched closure refuses any visible or pending
submission/handoff/manager record, durably revokes, independently proves manager
retirement and original group absence, rechecks original records, then publishes
the same immutable receipt; it cannot resolve an uncertain submitted launch.
No additional finding was identified in this slice; acceptance still requires
the explicitly remaining file review and corrected-source/local gates.

Pure ownership delta review at corrected `9f1f175`: the only production core
changes add nonmutating `settlement_for` selection and admit terminal unclassified
`failed` stopped outcomes for acknowledgement while refusing attempted or
pending interrupted disposition of that outcome.
Selection requires the exact current stopped owner, uses its immutable retry
classes/attempt limit and current pending presence, and mutates no state;
removed effects or cancelled candidates select interruption without reclassifying
failure. Independent table fixtures cover status/class branches, absent effects,
stale runs, unstopped owners, final attempts and terminal-failure round trips.
SPEC's selector and stopped-outcome clauses and the additive provisional API
entry match these deltas; no new I/O, clock or hash domain enters fsm-core.
No additional finding was identified; current workspace compiled verdicts remain
pending and this does not accept the remaining changed-file inventory.

Catalogue/root-exit/preparation review at corrected `9f1f175`: catalogue
publication validates the wrapped byte/depth envelope and only a fresh unused
authority can adopt it; later verification resolves the journal effect and
matches original full fingerprint/retry plus substituted argv before entry.
Root-exit projection requires original invocation/PID and canonical recognized
exit fields, treating root retirement only as a candidate; only the exact
manager deadline refusal at an expired handler deadline becomes timeout.
Other identity/inspection failures remain uncertain.
`NativePreparation` requests only preparation, delivers one parsed original-
route domain after actual helper retirement, and checks its overall deadline
before/after collection; cancellation/refusal keeps uncertainty and never binds
or launches. Metadata alone does not authorize execution or cleanup.
Catalogue mismatch, root-status parser/classification and provisioned
preparation cancellation/delivery controls cover these boundaries; no additional
finding was identified, and remaining authority/helpers, historical runner and
evidence changes still require their direct range review.

Authority helper/dispatch review at corrected `9f1f175`: privileged commands
require observed Root identity, exact argument counts and canonical fixed routes;
client/gate dispatch instead applies their separately reviewed identity checks.
Unsupported target binaries refuse rather than selecting a fallback.
Protected reads use nofollow/nonblocking regular-file checks, size plus one
overflow detection, repeated descriptor identity/mode/owner/length and canonical
JSON; create-once private publication syncs record and parent and leaves partial
material refusing cold replay. Original claim verification requires enabled
admission, exact pending running ownership, no stopped result and the original
record hash from records or the sealed base index before exclusive binding.
The manifest adds only the separately provisioned authority binary, and library
limits remain private; MCP-client changes document socket cancellation without
changing protocol behavior. No additional finding was identified in these deltas;
historical Runner and evidence/test harness range review remain outstanding.

Historical Runner delta review at corrected `9f1f175`: Linux process and MCP
stderr capture now share bounded nonblocking socket accounting, drained from
poll/finished scans; other platforms retain existing file transport with shared
prefix/hash accounting and capture removal on owned drop.
MCP workers retain independent cancellation and observed join, and unjoined
retiring workers exclude another local spawn; candidate collection waits for
worker join. Existing child kill/wait and direct-child result semantics remain
outside the native proof boundary and are not claim-safe host integration.
This is consistent with the provisional documentation and the separately
reviewed shared adapters; native authority does not invoke this spawn path.
Task 9401 must replace production host routing through durable native ownership,
and task 9402 must establish bounded host shutdown rather than inherit the
historical blocking waits. No additional finding in these deltas changes those
already explicit unfinished obligations; evidence/harness review remains.

Evidence verifier and CLI harness review at corrected `9f1f175`: the verifier
loads suite and case inventories from the exact frozen Git commit, bounds each
retained artifact, checks report/log digests, compiler identity, exact passing
case order and the expected failed final-kill control with a live original
domain. Authority evidence requires the production allocator scope while
explicitly denying production backend acceptance; store-bridge evidence requires
physical identity and missing/torn/symlink/writable metadata refusal controls.
Executable digest syntax is checked, but executable bytes are not retained or
independently compared, so `executable_bytes_verified` and `gate_released` remain
false. Fixture installation exclusively publishes a newly built authority and
cleanup requires its original inode/device and digest rather than overwriting
or removing an existing product installation.
The CLI harness changes require successful subprocess exit, preserve the full
1,000-ID uniqueness check despite serial libtest's unterminated label, and use
a Conventional Commit in the disposable Git fixture with every Git exit checked.
No additional finding was identified in these reviewed deltas; this evidence
does not accept host integration or replace the remaining full-range review.

Public surface and contract-fixture delta review at corrected `9f1f175`:
`lib.rs` adds only the private value-limit module, while the public inventory
records the provisional contract, native I/O, preparation, execution, completion
and pipeline entries and their progress variants. The inventory is an API
declaration check, not evidence that public ticks use native ownership.
Handler-identity fixtures include an independently computed process digest,
explicit/default equivalence, each process contract dimension, MCP tool and
arguments, outcome payload and ordered stamps, and host concurrency exclusion.
Recovery controls require original structural material, reject changed or
missing fields, wrong fingerprint and equivalent noncanonical retry material,
and reject excessive nesting before serialization. These tests preserve the
distinction between an immutable contract identity and launch permission.
No additional finding was identified in these deltas; complete documentation
contract reconciliation and integrated-host acceptance remain outstanding.

Contract reconciliation, result/ownership boundary at corrected `9f1f175`:
SPEC's supervised transport, native completion, settlement/recovery, physical
store evidence and Root result-attestation clauses distinguish helper retirement,
tree closure, authenticated candidate and writer-protected disposition.
The reviewed implementation preserves those distinctions: receipt matching alone
does not authenticate output; `completion_record` syncs the separate full-response
attestation and immutable mode before verification/private completed publication;
recovery consults original binding and recorded completion without relaunch or
current catalogue selection. API policy and embedding text identify the new
private response hash domain and retain historical journal/closure bytes and
unreleased production routing. SPEC's two-second association clause includes
every interrupted accept/read retry, matching the corrected implementation.
The lifecycle guide separately retains manager retirement, cgroup absence and
owned transport/worker retirement as required evidence and identifies uncertain
starts and host integration as unfinished. No contradiction was identified in
these inspected contract clauses; this is a scoped reconciliation, with remaining
contract/file coverage and frozen gates still required before task acceptance.

Contract reconciliation, revocation and bounded transport at corrected
`9f1f175`: SPEC's protected entry, begin-close, enrolled authorization,
request-kill, request-stop and complete-close clauses require durable admission
revocation before termination, original native identity and independently proved
manager/job/cgroup retirement before receipt publication. Kernel termination
holds the revocation lock, rechecks original parent identity around nofollow
control opens and submits freeze then kill without claiming closure.
Broker framing bounds request/response material and checks the same deadline
before each read/write retry, including interruptions; its successful transport
does not settle ownership. Entry's five-second wait and exec-status association's
two-second deadline are distinct bounds and neither expiry releases the claim.
These clauses agree with the previously reviewed stopping, closure, enrollment
and transport implementation slices; no additional finding was identified.
Uncertain submission reconciliation and production host cancellation/shutdown
remain dependent tasks, rather than consequences of these primitive controls.

Admission/profile fault-control review at corrected `9f1f175`: the native
admission fixture checks memory/read-only stores, quarantined retained ownership,
a stale run, an actually poisoned writer and removed/cancelled pending work.
Refusals assert exact existing error classes, unchanged records/ownership,
absent binding/launch material and cold read-only replay where applicable;
the quarantined-state control is explicitly an in-memory predicate test rather
than fabricated native migration proof. Profile refusal first verifies the
installed fixture's original device/inode/digest, alters permissions through
that original descriptor, then requires unchanged allocation counter/inventory
before restoring permissions and proving normal preparation. Fixture cleanup
does not clear durable ownership or stand in for a production closure receipt.
These directly inspected assertions support early-refusal coverage without
proving concurrent production host exclusion; no additional finding was
identified, and broker-disconnect fixture review remains to be completed.

Broker-disconnect fixture review at corrected `9f1f175`: six process/MCP
variants kill the actual unprivileged installed helper, close its sole lifetime
endpoint, or kill the unprivileged public-adapter supervisor without sending an
explicit cancellation request. Before that fault, independent barriers verify
original handoff PID and root/child/grandchild cgroup membership and isolated
identity, live population and absence of closure proof. Afterward, a bounded
wait requires readable verified closure, actual group absence and broker worker
retirement while the broker remains alive; lifetime EOF must retire its helper
unsuccessfully and supervisor death must retire the separately observed helper.
Original manager-stop binding/gate and authority identity remain matched,
durable journal ownership remains without a stopped result, and duplicate execute
refuses. The copied test supervisor invokes production public adapters and is
explicitly bounded, but it is not the standalone/embedded production service.
Thus these controls establish helper-owner death cancellation at the primitive
boundary, not task 9401 routing or task 9402 host shutdown acceptance; no
additional finding was identified in the inspected fixture.

Public-adapter supervisor fixture review at corrected `9f1f175`: the positive
process case starts `NativeExecution` with the current writer-held claim/hash,
then an independent unprivileged writer holds the lease through handler completion
and rejects a competing domain claim without changing records/state/head.
Read-only recovery checks identical original candidate, contract, failure class,
stopped outcome and matching proof, actual helper reap plus both EOFs and retained
capacity; attempted settlement on that snapshot refuses and preserves ownership.
Releasing the competing writer must succeed before cold writer reopening, which
still sees the original unresolved claim/hash. Missing completion recovery,
pre-poll cancellation, wrong claim hash and preparation cancellation/single
delivery each have separate refusal/retirement assertions.
The test does not positively settle the retained completion in this supervisor;
its writer barrier proves observation/recovery during contention and competing
claim refusal, not the production service's launch/settlement crash sequence.
No additional finding was identified; this closes direct inspection of the
supervisor fixture while preserving the dependent integrated-host obligations.

Termination/enrollment fixture review at corrected `9f1f175`: administrative
kernel-submission controls independently enroll roots and an inherited descendant,
observe populated closing state, then require root retirement and depopulation
while asserting no closed tombstone; this intentionally proves no full closure.
Production enrollment controls preserve partial intent and prearmed final/pending
entries on refusal, match actual manager/proc gate identity, refuse missing or
changed handoff before grant, and verify Root-owned derived-group mode 0440.
Successful root exit still refuses closure while the manager remains retained.
Matched stop remains distinct from immutable receipt publication and journal stop:
damaged handoff triggers original-domain fencing without fabricated manager
completion, malformed completion and partial receipt refuse closure, replay
preserves receipt bytes/inode, duplicate launch refuses, and the eventual
writer-protected stopped record retains the claim. Fast exit/signal, capture
boundary and timeout cases require readable native proof without consuming
ownership. Repairs remove only deliberately injected fixture faults, not a
production recovery permission. No additional finding was identified in these
directly inspected controls; integrated service races/crashes remain unexecuted.

Allocator fixture/control review at corrected `9f1f175`, registration through
never-launched closure: the Root-only fixture creates an exclusive namespace,
checks physical-store metadata refusal before catalogue adoption, and tracks
original cgroup identities for nonrecursive empty-domain teardown.
Preparation controls verify distinct monotonic allocations without journal claims,
refuse unknown groups, rolled-back counters and persisted incomplete intents,
and explicitly distinguish injected intermediate state from physical power loss.
Residual cleanup refuses missing revocation, replacement identity, live manager
ownership and child groups, leaving the other allocation untouched.
The genuine-claim case observes without changing inventory while the authority
lock is held, then tests pending submission material and partial receipts before
never-launched closure. Cold replay preserves final receipt bytes/inode, rejects
a separate identical pending copy, and accepts only a matching hard-linked pending
inode; original durable ownership remains without a stopped record.
No additional finding was identified in this inspected portion; the remaining
binding helper and broker access test bodies still require direct inspection.

Allocator binding-helper review at corrected `9f1f175`: typed admission rejects
a different effect, invalid retry and oversized/deep argv, MCP arguments and both
outcome payloads with unchanged records/state and no request-key claim; duplicate
valid admission returns the original currently owned claim/hash without append.
Grant controls reject group zero, substituted argv and an unenrolled prepared
domain, refuse catalogue replacement and malformed closing/closed markers, and
verify immutable original grant publication. Cancellation prevents later binding
or authorization without replacing the original binding/grant. A separately
claimed unapproved fingerprint cannot bind or publish entry.
Revocation of a deliberately symlinked pending grant preserves that obstacle,
removes the valid visible grant and retains closing; only explicit test-owned
fault removal allows replay. Kernel-submission controls follow while journal
ownership remains. No additional finding was identified; all allocator fixture
body portions have now been directly inspected, while broker access and final
aggregate reconciliation remain outstanding.

Broker access/settlement fixture review at corrected `9f1f175`: actual installed
binary controls drop supplementary groups and Root identity, deny unrelated and
reserved dynamic identities, refuse out-of-policy actions, verify operator-only
socket/public route modes and exclusive leadership, and preserve epoch authority
on missing history or counter rollback. Process success and timeout execute the
public-adapter contention controls before checking original completion.
Forged output fails independent attestation despite matching closure; missing,
torn, symlinked or writable attestation and torn completed/physical-store metadata
refuse recovery without repairing injected bytes. Missing/malformed catalogue
permits original recovery but refuses preparation without allocation-counter
movement, including broker restart without a catalogue.
Positive stop persists once under the original proof while stale stop and
premature settlement/advance refuse; stopped ownership excludes another claim
and launch. Adopted completion settles once, releases retained capacity only
after durability, and replays exact acknowledgement without append; read-only
replay neither creates an unused key nor accepts a different disposition.
Restarted original-contract outcome advancement checks stale identity, emits the
original event once and preserves idempotent state, without current catalogue
selection. These assertions directly cover primitive settlement/recovery, not
standalone/embedded service crash routing; no additional finding was identified.

Store test delta review at corrected `9f1f175`: terminal protocol failure must
acknowledge once with its original candidate, reject attempted/pending interruption
without changing ownership or retry count, and replay the same execution response
after clearing cached responses. Opaque matching predicates independently reject
changed run, allocation, authority/cgroup identity or original claim hash without
mutating ownership. Current-claim hash lookup rejects changed full identity,
survives stopping and repeated sealed cold opens, and becomes stale after atomic
settlement. A byte-identical copied journal with another physical directory
cannot consume the original proof, leaves its head/claim unchanged and reopens
without a stopped result, while the original directory can stop successfully.
Legacy admission also rejects foreign physical evidence before clock advancement
or request-key mutation. These internal proof fixtures isolate store transition
checks; actual Root publication/file verification is established separately by
the native bridge and broker controls, not by constructing a test-only proof.
No additional finding was identified in these changed tests; they do not replace
production host crash and concurrent-executor acceptance.

Contract reconciliation found a stale API-policy paragraph describing the initial
private result envelope `/1` without identifying its supersession. Corrected
the paragraph to name current `/3`, its original kind/full contract and refusal
of older envelopes, matching SPEC and NativeCompletion; no code, persisted
journal/closure bytes, hash or public signature changes. This documentation
finding is repaired independently of the interrupted-I/O product correction;
compiled evidence remains bound to product `9f1f175`.

Named-target/native-bridge fixture delta review at corrected `9f1f175`:
`lifecycle_runner` includes production authority source and its opt-in native
cases only for supported Linux architectures; ordinary execution of the target
does not unignore the provisioned cases. Portable controls assert unsupported
or unprivileged refusal before request I/O, and the noisy-root test independently
checks empty scratch directories, stream bounds/digest limits and no local
running entries after collection; that test uses historical direct-child Runner
and is not native descendant-closure evidence. The Root bridge fixture adds
exclusive registered physical directory identity metadata before binding and
retains its separate fixture scope. Provisioned artifact verification remains
the evidence that the named target actually executed native controls.
No additional finding was identified in these deltas. The current local memory
check still exceeds the workspace swap threshold, so no intensive frozen host
command was started; remote CI remains the only live gate execution.

## Corrected-source acceptance reconciliation

Product review range is `cf3f6003963d057b7bfdb6d1bc26ea29a15ad0fb` through
`9f1f175ad91609359699e3a2d670119e8cbb506a`; subsequent review records and the
API-policy envelope clarification are documentation only.

| Task obligation | Reconciled evidence | Verdict |
| --- | --- | --- |
| One claimed process/MCP enrollment path | Launch/enrollment/grant, catalogue, private exec-status and native runner slices; exact provisioned `lifecycle_runner` artifact inventory on stable/MSRV. | Source and native controls reviewed; compiled portable matrix not yet complete. |
| Candidate, closure and uncertainty remain separate | Closure publication, manager/stop/revocation, transport and completion/attestation slices; corrupted handoff, partial receipt and retained-ownership assertions. | Reviewed primitive boundary; no claim release from kill, root exit or helper EOF. |
| Exit, answer, timeout and cancellation close descendants | Independently enrolled process/MCP tree controls and helper/supervisor-death controls with verified closure and original claim retained. | Native evidence passes on both compilers; production host routing is task 9401. |
| Bounded capture and owned resource cleanup | Limit/plus-one/hash-limit/noisy capture, worker join, helper streams and repeated native resource assertions. | Reviewed native/portable boundaries; remaining platform verdicts and local frozen invocation still required. |
| Explicit progress, best-effort Drop and public contract | Native preparation/run/execution progress, public inventory, original-contract recovery/settlement and scoped SPEC/API/guide reconciliation. | Reviewed provisional interface; full task acceptance remains withheld. |

The interrupted-association deadline finding has a product correction executed
by both native compiler jobs; the stale API-policy envelope reference has a
documentation correction. No other finding emerged from the recorded slices.
CI `37376949993` passes native stable/MSRV, both Ubuntu gates and zero dependencies;
four macOS/Windows axes remain live. The separate frozen local stable host gate
is unexecuted under the workspace swap rule. Retained artifacts verify source,
compiler, report/log integrity and inventory, not independent executable bytes.
This reconciliation does not mark the task done, release production routing,
or accept dependent shutdown/reconciliation/crash work or plans 20, 21 and 23.

Corrected-source CI `37376949993` subsequently completed successfully: all nine
jobs pass at exact `9f1f175`, including all six portable axes, both independently
verified native matrices and zero dependencies. Each portable/dependency log is
retained in the task cache, with final Windows stable log
`ci-37376949993-windows-stable.log`. This supersedes the live-job observations
above. The separate local frozen stable host gate remains unexecuted under the
workspace swap rule, so status remains `in_progress` and no production gate is
released; dependent integrated-host acceptance remains outside this verdict.

### Local frozen stable gate resumed

At exact product `9f1f175ad91609359699e3a2d670119e8cbb506a`, the isolated
local `cargo +stable test --workspace --no-fail-fast` completed with exit code
zero after swap fell below the workspace threshold. The run used
`CARGO_BUILD_JOBS=1`, `RUST_TEST_THREADS=1` and the dedicated disk-backed task
cache as `TMPDIR`; repeated observations showed approximately 2.1 GiB swap,
65 GiB available RAM and zero current memory pressure. The retained log is
`~/.cache/fsm-plan-native-matrix-20261005/local-9f1f175-stable-debug.log`.
The separate release gate has started serially; clippy, documentation and
remaining explicit host gates are still pending, so this task remains
`in_progress` and production ownership integration remains unaccepted.

The frozen local `cargo +stable test --workspace --release --no-fail-fast`
also completed with exit code zero using the same serial environment and
disk-backed `TMPDIR`; retained evidence is
`~/.cache/fsm-plan-native-matrix-20261005/local-9f1f175-stable-release.log`.
At completion, swap remained approximately 2.1 GiB and available RAM 61 GiB.
The all-targets clippy gate is now running serially; documentation and the
explicit zero-dependency/downstream acceptance commands remain pending.
Task status remains `in_progress` until the full frozen host gate completes.

The frozen local `cargo +stable clippy --workspace --all-targets -- -D warnings`
completed with exit code zero; retained log:
`~/.cache/fsm-plan-native-matrix-20261005/local-9f1f175-stable-clippy.log`.
The warnings-denied rustdoc gate is running next, with explicit zero-dependency
and downstream acceptance commands still pending; status remains `in_progress`.

The remaining frozen local host commands completed with exit code zero:
warnings-denied `cargo +stable doc --workspace --no-deps`, explicit
`cargo +stable test -p fsm-cli --test zero_deps`, and
`cargo +stable test -p fsm-embed-acceptance`. Logs are retained under the
dedicated task cache as `local-9f1f175-stable-doc.log`,
`local-9f1f175-stable-zero-deps.log`, and `local-9f1f175-stable-embed.log`.
Formatting, oversized-file and complete frozen range diff checks were also
rerun successfully in the isolated checkout. All required stable host
commands now pass at the frozen product source; the task acceptance record
still requires final consolidation before changing its status.
