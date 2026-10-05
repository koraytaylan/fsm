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

**Acceptance review at source `893dd8b`:**

| Requirement | Inspected implementation and evidence | Remaining acceptance |
| --- | --- | --- |
| One claimed process/MCP enrollment path | `containment/runner.rs`, `exec_status.rs`, `enrollment.rs` and `runner_native_tests.rs`; verified native stable/MSRV artifacts from CI `37361472892` include the production authority suite. | Complete frozen-range spec/API review and terminal portable gates. |
| Candidate separated from verified closure and uncertainty | `runner.rs` publishes completion only after `closure::complete`, matching `VerifiedClosure`, and owned child/worker retirement; corrupted handoff cases retain the original claim without a receipt or successful result. | Preserve these controls in the final acceptance range. |
| Root exit, response, timeout and cancellation close descendants | Native runner controls independently observe root, child and grandchild membership, then exercise process exit/failure/signal and MCP response/protocol/timeout/cancellation paths. | Production service routing and host capacity integration belong to dependent task 9401 and remain incomplete. |
| Bounded capture and resource retirement | `capture_native_tests.rs` checks both handler kinds at 4096, 4097, hash limit, hash limit plus one, and eight MiB; exact prefixes/digests, bounded result material and repeated FD/thread inventories are verified. | Full stable host gates remain running or unexecuted at this source. |
| Explicit cleanup progress and best-effort Drop | `NativePreparation`, `NativeRun` and `NativeExecution` expose bounded phase/helper progress; observation does not require a writer and settlement retains completion on refusal; authority `OwnedRun::drop` fences without manufacturing closure. | Final public-surface and zero-dependency verdicts remain required. |

The independently verified native inventory contains 81 cases per compiler;
this count describes the whole matrix, not 81 runner-specific controls.
The artifacts do not independently compare executable bytes and do not release
the production native gate; task status remains `in_progress`.

A command-level coverage gap remains: the named `cargo test -p fsm-execute
--test lifecycle_runner` entry currently exercises protected-entry refusal and
legacy noisy-root capture, not claimed native process/MCP grandchildren.
Those tree controls currently run through `authority_probe.py` selecting the
production authority binary's ignored native cases under the provisioned Root
fixture; green `lifecycle_runner` output alone cannot satisfy the named native
runner requirement, which requires verified named-target native evidence before this task is accepted.


- **Done when:** the same production runner passes native process and MCP descendant/pipe/capture tests and returns a settleable result only with matching run identity and proved closed containment, while every uncertain cleanup remains explicit and bounded.

The coverage repair compiles the production authority and its existing native
controls directly into `lifecycle_runner`; the provisioned probe selects that
exact Cargo test artifact while still installing the separate production binary.
Reports identify the test target, and the independent verifier derives the
required target from frozen source; fresh local compilation and native
stable/MSRV execution remain required before accepting this repair.

CI `37363278015` at frozen repair `0cef6f6` passes native stable and MSRV;
independent artifact verification confirms `lifecycle_runner` as the fixture
target and all 81 matrix cases per compiler, including the existing claimed
process/MCP descendant controls, so the named-target coverage gap is repaired.
Full portable and frozen host gates and aggregate review remain pending.
