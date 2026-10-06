---
id: bounded-executor-shutdown
title: "Bounded Executor Shutdown"
workstream: "0094"
kind: task
depends_on:
  - executor-ownership-integration
gated: false
touches:
  - crates/fsm-execute/src/service.rs
  - crates/fsm-execute/src/service/
  - crates/fsm-execute/src/lib.rs
  - crates/fsm-execute/tests/shutdown.rs
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - crates/fsm-cli/src/cli/execute.rs
  - crates/fsm-cli/src/cli/mod.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/lifecycle.rs
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/args.rs
  - crates/fsm-cli/src/main.rs
  - crates/fsm-cli/tests/executor_shutdown.rs
  - docs/EXECUTOR-LIFECYCLE.md
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Bounded Executor Shutdown

An explicit stop closes admission immediately and reports whether cleanup
completed, timed out or remains uncertain.

**Steps:**

1. Implement running/draining/stopping/stopped/uncertain lifecycle states and
   a public control handle/report with validated finite timeout bounds; add
   the minimal independent lifecycle pump to the existing production stdio
   entry so blocked input/output cannot prevent stop or already-admitted
   timeout processing, without launching pending work or scheduling retries
   and machine deadlines before plan 0020.
2. Drain current handlers until the deadline, then close their native domains;
   abort closes immediately, and both preserve pending effects on interrupted
   execution instead of inventing instance cancellation or failure events.
3. Add `fsm execute stop --data-dir <dir> --mode drain|abort --timeout-ms <n>`
   using the proved local control mechanism, with owner-only access, bounded
   request size and exact incarnation binding; control must work while the
   journal writer is unavailable.
4. Connect the approved native ordinary-termination/console notification paths
   and embedded host shutdown to the same lifecycle state machine, documenting
   how repeated signals escalate and how uncatchable kill differs.
5. Return an uncertain report within the deadline when native cleanup or
   journaling cannot finish, retain every unresolved durable claim and stop
   admitting work; keep `Drop` outside the guarantee and update public docs.

**Tests:**

- Standalone and embedded production tests cover explicit drain/abort, empty
  executor, completing and hung trees, zero/maximum/invalid timeout, repeated
  controls and control concurrent with spawn; admission closes before drain.
- Launch the current production embedded stdio binary, admit a long-running
  handler, leave stdin open without more frames, and issue external `execute
  stop`: the handler closes and the writer releases within the lifecycle bound
  without plan 0020 code, polling requests or EOF; a separate quiet-client case
  enforces the admitted handler's timeout but starts no new pending effect.
- Native Ctrl-C and ordinary termination cases use their actual OS mechanisms,
  demonstrate bounded reports or documented signal exits and retain claims
  whenever termination cannot be proved; hard kill is tested as recovery.
- Writer contention, full disk and failed native stop cannot hold the control
  response forever or turn interrupted shutdown into domain cancellation.
- Unauthorized/stale control requests fail, endpoint cleanup is bounded, and
  handlers cannot consume the MCP server's protocol stdin.

- **Done when:** the public API and native standalone/embedded shutdown tests prove bounded drain and abort with immediate admission closure, correct signal integration and durable uncertainty preservation, without inventing machine events or relying on `Drop`.

Owned native lifecycle driver implementation now takes the original durable
writer and native execution components, exposes independently waitable cloned
control, and explicitly polls admission-free original completion plus bounded
fair claim-bound closure; interruption waits for execution helper reap/EOF and
keeps authentic completion policy, and Stopped follows actual writer release.
Focused stable session 97397 passed executor library/downstream writer-control
tests, stable/MSRV all-target Clippy, public API inventory and source-size checks
under asserted 1 GiB RAM/zero-swap limits; see OWNED-DRIVER-REVIEW.md and retained
local-owned-lifecycle-focused-v3.log. Full changed-source stable gate and actual
installed driver/production stdio/endpoint/CLI/signal controls remain pending;
no task is promoted and progress remains 3/7.

Owned driver follow-up corrected admission closure versus explicit stop and
proved the downstream writer-retention regression fails with only the old
predicate restored (mutation session 91927, terminal 0 after expected test
exit 101 and successful restored tests). Focused stable/MSRV checks passed
(session 18750), and actual live-bound process/MCP native driver controls now
compile (11667) while remaining unexecuted here. Known-defective f90871b full
gate 85037 was explicitly stopped and confirmed terminal 143 before mutation;
its partial log is not full acceptance. Corrected-source full stable gate and
production/installed acceptance remain pending, with progress unchanged at 3/7.

Corrected owned-driver runtime f50d95868f8c0bcac5800aa24dbea5b5f7cf3d36
passed the full stable host gate in session 47032, confirmed terminal 0: format,
source size, workspace debug and release tests, workspace all-target Clippy,
warning-denying workspace documentation, zero-dependency and embed acceptance
all passed under asserted 1 GiB RAM and zero swap. The complete retained log is
local-owned-driver-request-fix-stable-gate.log. This resolves the local full-gate
obligation for the writer-release correction and compiled live-bound probes;
installed native execution, portable CI and production stdio/endpoint/CLI/signal
integration remain unexecuted or unfinished, and progress stays 3/7.

Opt-in owned native MCP session composition now integrates one writer owner
and clock, bounded single-reader input shared with elicitation, idle admitted
observation, queued output and nonjoining owned-feed stop admission; native
cleanup/writer facts and actual output delivery are separate, with original
deadline reuse and explicit timeout facts. Focused stable/MSRV session 24844
passed library/downstream owned and borrowed session, elicitation, lifecycle
and public API checks; worker-bound mutation session 24100 confirmed an exact
limit refusal fails when only that guard is disabled and restored tests pass.
See OWNED-SESSION-REVIEW.md and retained logs. This does not change current CLI
selection or supply production/installed tree, endpoint, CLI stop, signals or
paired standalone acceptance; full changed-source stable gate remains pending
and progress stays 3/7 with no task promotion.


Owned-session runtime 3157f1c subsequently passed the full stable host gate
(session 30244, terminal 0; local-owned-stdio-integration-stable-gate.log).
The opt-in actual-driver local control endpoint and client now implement private
exact-incarnation discovery, bounded independent metadata transport, abort
capacity despite waiting drains and replacement-preserving scoped cleanup;
LOCAL-CONTROL-REVIEW.md records focused stable/MSRV and twelve real transport
tests plus five guard mutation failures followed by restored passing tests.
The new transport still needs its full stable gate; CLI stop, production
publication, signals and installed/paired native acceptance remain unfinished,
so this task stays planned and plan progress remains 3/7.


Endpoint runtime 9d2439f passed the full stable host gate in session 47222,
confirmed terminal zero; local-control-endpoint-stable-gate.log is retained.
The execute stop command now reaches that endpoint through the real production
argument dispatcher, validates finite bounds and emits actual report or unknown
transport facts without acquiring the journal writer. CLI-STOP-REVIEW.md records
focused session 44567 and mutation session 89989, both terminal zero after the
expected guard-disabled test failures and restored passing tests. Production
server endpoint publication, installed native trees, signals, paired standalone
and the new CLI runtime's full gate remain pending; no task promotion occurs.


### Complete execute stop stable host gate — 2026-10-06

Session 45893 completed with exit zero against exact CLI runtime
cd0213b3df7aa6a9dac4afa47c1cfd7b98b124fc under verified
MemoryMax=1G and MemorySwapMax=0; execute-stop-stable-gate.log is retained.
Formatting, source size, debug and release workspace tests, all-target
workspace Clippy, warning-free documentation, zero dependencies and embed
acceptance all passed. Subsequent bc6aa3b/f3d2086 ownership reviews changed
only documentation and did not alter the frozen tested runtime.
This proves the local stable host gate for execute stop, not current native
server publication, paired actors, installed trees, signals or portable CI.
Plan progress remains 3/7 with task 9401 in progress and 9402 planned.
