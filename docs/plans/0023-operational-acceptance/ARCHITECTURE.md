# Architecture — Plan 0023

> Keep the client independent, make each claim falsifiable, retain the run.

## Implementer orientation

1. Read `../README.md`, `CONTRIBUTING.md`, `docs/RELEASE.md` and the task's
   declared footprint before editing; the task's **Tests:** are its acceptance
   inventory, and all applicable repository gates still apply.
2. Preserve the independent boundary: Python standard-library clients invoke
   an installed `fsm` or speak JSON-RPC; they do not import Rust implementation
   helpers or derive expected outcomes from the production analyzer.
3. Register after 0020–0022; inspect their integrated contracts rather than
   assuming a proposal here outranks the final specification.
4. Keep the Rust workspace dependency-free and forbid unsafe code; Python
   tooling remains outside the runtime, as the current acceptance suite does.
5. Distinguish a harness test from an executed candidate run, and a compiled
   platform branch from native execution; neither substitutes for the other.

## 0000 — Starting evidence

At `67ad5e3`, `acceptance/acceptance.sh` builds a Podman image and invokes
`acceptance.suite.run`; `FSM_BIN` and `FSM_REPO` already let the Python client
address an installed binary and source fixtures. The runner prints assertions
but has no durable candidate manifest; it returns success if no scenario
fails, even when a scenario is skipped. `docs/RELEASE.md` requires live-model
and Desktop checks but does not retain a structured candidate report.

`.github/workflows/ci.yml` runs the stable/MSRV Rust gate on three OS families;
`release.yml` adds release and fuzz proofs. Those configured jobs are not
evidence that a particular candidate has passed. The prior local debug/release
verification is useful baseline evidence, not a substitute for this plan's
native acceptance or duration runs.

## 0095 — Evidence and installed scenarios

### Evidence report (task `9501`)

Introduce `fsm.acceptance/1` in the acceptance tooling, separate from engine
formats. Include source commit, dirty-state flag, candidate binary SHA-256,
version output, OS/architecture, toolchain where known, transport, run profile,
start/end times, scenario revision, selected and required scenarios, assertions
executed, per-scenario verdict, failures, skips with reasons and artifact
digests. Preserve raw diagnostic output beside the report; secrets never
belong in fixture data. A filtered developer run reports its filter and cannot
satisfy a full release manifest. Zero scenarios, zero-assertion scenarios,
interrupted runs and missing required scenarios cannot be reported as passed.

Use a temporary report plus atomic replacement on successful report emission;
retain a distinct incomplete marker on interruption. A harness must still
return nonzero when a scenario fails, even if it fails to write the report.
Schema and verdict tests use synthetic reports and stub commands, so testing
the reporter does not require a full build or a live model.

### Observable outcomes (task `9502`)

Extend the suite with generic fixture operations: validate prerequisites,
temporarily suspend a resource, process a bounded set of items, then restore
it, with injected failure before mutation, during work and during restoration.
Retain the machine-authored failure and compensation behavior rather than
teaching the client to repair a broken production outcome.

For each transport and handler kind:

| Claim | Independent observation |
|---|---|
| Progress does not need polling | After one trigger, send no progress requests; a fixture marker and subscription output show terminal progress, then one final read verifies state and journal |
| A busy handler does not stop MCP | A separate request completes within the documented service bound while a fixture waits on an explicit barrier |
| Invalid contracts cannot run | A deliberately incompatible machine/table is refused before a fixture's external marker changes, with the exact diagnostic and unchanged journal where the contract promises no mutation |
| An intentional manual effect stays visible | The declared manual path is reported as such, with no fabricated automatic-completion claim |
| Termination cannot overlap runs | Independent per-resource concurrency instrumentation never exceeds one across the supported stop/crash/restart sequence |
| Recovery remains auditable | Exactly one ack and intended advance per effect, retries consistent with policy, terminal compensation state, successful verify/replay |

An HTTP disconnect closes a session, not the shared host; stdio EOF follows
the host shutdown contract from 0020/0022. A slow or disconnected observer must
not prevent execution. Include contended read-only and degraded negative
controls: fixture invocation count stays zero, and capability discovery reports
what is actually available. Change-feed reads used to observe progress are not
allowed to become a hidden executor trigger.

## 0096 — Duration and native execution

### Workload and bounds (task `9601`)

Add a seeded operational harness which launches the installed binary and
mixes successes, retries, deadline expiry, manual pauses, cancellation,
compensation, concurrent sessions, client disconnection and bounded output
floods; exercise both standalone execution and the shared embedded host.
Reuse the hand-written acceptance fixtures, not the scheduler as an oracle.
Exercise sealed-store reopen and repeated executor replacement during the
long run, using an explicit finite seed sequence and an independent event log.

Two committed profiles are required: **smoke**, at least two minutes and 100
completed fixture cycles, and **sustained**, at least eight hours and 10,000
completed cycles per supported contained-executor OS family on stable. Both duration and count floors apply;
an early success, a stalled loop or a run that merely sleeps cannot satisfy
them. A maximum wall time and a no-progress watchdog produce incomplete/fail
outcomes, never a silent pass. Expensive profiles are opt-in locally and run
serially; do not turn the ordinary unit suite into an eight-hour job.

Measure completed-work latency, control-request latency, scheduler lag,
active children, surviving descendants, handles/file descriptors, capture
bytes and RSS where available. Correctness bounds are exact: no missing
accepted work, invalid journal, overlapping resource mutation, cap violation,
unsettled expected compensation or leaked child after the contract's bound.
Resource and latency ceilings use a committed calibration for a named host,
fixed workload and retained-store size, with numeric tolerances selected
before the candidate run; report unavailable metrics and block any claim that
depends on them. Journal growth from retained records is accounted separately
from leaked resources; warm-up and quiescent samples compare equivalent live
state. No hardcoded universal throughput promise is introduced.

### Native evidence (task `9602`)

Keep existing six-leg Rust coverage and native installed-binary baseline smoke
runs on Linux, macOS and Windows at stable and MSRV; `FSM_BIN` must address
the artifact built on that leg. The explicitly authorized initial contained
runtime is provisioned Linux/systemd (see `docs/EXECUTOR-LIFECYCLE.md`). Run
the full new executor matrix on native Linux at both toolchains; macOS and
Windows additionally prove unsupported-capability refusal before launch,
rather than claiming containment or treating missing facilities as passes.
Podman acceptance remains a separate Linux consumer-install proof. Sustained
contained-executor runs initially execute natively on provisioned Linux at
stable through a separately bounded workflow, selected by an
immutable candidate SHA; no secrets or scheduled cost are enabled merely by
writing this plan. Timeouts, cancellation and artifact-upload failures make
the relevant evidence incomplete.

The lifecycle tests must use the platform's actual shutdown and hard-kill
mechanisms specified in 0022, not relabel a graceful API call as a signal.
Unsupported containment is a failed prerequisite, not a passing skip. Each
artifact identifies the tested executable and full scenario manifest.

## 0097 — Live-model protocol and release closure

### Uncoached authoring (task `9701`)

Retain the existing case-review brief and add two generic briefs: an automated
workflow with compensation and a deliberate incompatible-handler recovery.
The model receives only the natural-language brief, the MCP connection and
published product documentation/resources; it is not given a completed machine,
private implementation knowledge or corrective coaching. Operator installation
of a handler table is recorded setup, not model-authored arbitrary execution.

Run three fresh sessions for each brief, nine total, with the model version,
host version, candidate digest, starting store, tool transcript and scored
outcomes retained. Predeclare a limit of 40 tool calls and 15 minutes per
session; exceeding either fails that attempt. Every session must produce the
expected machine behavior, discover the execution contract, correct the
deliberate invalid draft when applicable, preserve the resource on failure,
and accurately explain the final state. Any safety failure blocks acceptance;
all nine must pass the frozen rubric. Report every attempted session, including
coached or aborted attempts, and rerun the full set after relevant fixes.

Keep the Desktop connect/list/configuration smoke check distinct from model
authoring and protocol conformance. Genuine human review remains explicit;
credentials or an unavailable interactive host leave the task gated, never
converted into a mock pass. Artifacts can be stored in access-controlled CI
or a review bundle with stable references and digests; public copies must be
redacted without erasing the evidence needed to judge the result.

### Candidate closure (tasks `9702`, `9703`)

One evidence index binds all claims to the candidate code SHA and artifact
digests; documentation-only evidence commits are identified separately, so
retaining a report does not pretend the executable changed. Automated and
manual validators reject stale revisions, missing native axes, missing
required scenarios, failed/unknown verdicts or unresolved review findings.
Existing format/clippy/debug/release/rustdoc, dependency, fuzz, library-consumer
and release checks from `docs/RELEASE.md` remain required; the new index
references them rather than copying a stale command list.

The review closure maps each of the five concerns to its implementation plan,
required scenario IDs, reports and a reviewer conclusion. Unit-test volume,
plan files and a green harness self-test are not accepted as the corresponding
operational run. The release preflight runs before tagging; the tag pipeline
rechecks immutable automated evidence before publication and fails closed on
missing evidence. Final status changes are coordinator-owned after review.
