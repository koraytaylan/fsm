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

**Acceptance review at source `31e0c6318cb45bcc5f76468d6586e172356e466a`:**

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
