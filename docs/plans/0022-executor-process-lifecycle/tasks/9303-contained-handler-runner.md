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
