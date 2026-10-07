# Plan 0020 — Autonomous Embedded Execution — In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and registration and integration
evidence are coordinator-owned.

- **Status:** Registered by hand; execution-host-ownership is in progress;
  private Store ownership, bounded admission and reserved cancellation are
  implemented; a private native owner now retains executor state and drives
  quiet decision passes; production transport integration remains outstanding.
- **Goal:** accepted embedded workflows advance without client polling,
  with one writer and responsive, bounded stdio and HTTP sessions.
- **Root cause:** stdio runs an executor tick only after requests and HTTP
  does not retain its embedded executor; moving work into a timer alone
  would leave blocking client and handler paths inside the writer owner.
- **Approach:** build one bounded execution host, separate process work from
  settlement, schedule independently of input, and connect both transports
  with explicit session lifetime and output ordering contracts.
- **Progress:** 0/7 tasks done; 0 blocked; 0 dropped.
- **Integration:** Phase R bound by hand on `develop` to validation base
  `41f9350182b2437b6b97855487df253667ee90d0`, following the recorded manual
  coordinator mode of plans 0019 and 0022; this is not a Makina invocation.
  The committed scope SHA-256 is
  `0c79c9e7f23b0032852ffea32417470bfca894a254702732be7eeea0017c7239`.
  Seven closed task headers, matching IDs/titles/workstreams, ordered steps,
  unique per-task mutation footprints and six dependency edges validate as
  one acyclic local DAG; no manifest owner requires footprint adoption.
  No task has a landing OID or completed acceptance inventory; final
  integration still requires plan 0022's supervised lifecycle behavior.
- **Exceptions:** none recorded.
- **Outcome:** planned; no autonomous execution capability is claimed yet.

Registration makes the dependency-free ungated ownership task ready; it does
not establish autonomous execution or release any cross-plan prerequisite.

### Initial private owner and admission — 2026-10-07

The private `mcp/host/` owner now retains the Store and injected clock, accepts
owned tool/RPC envelopes, applies complete ordinary dispatch operations and
captures immutable results, committed sequence and appended interval. Host and
session count/owned-allocation budgets survive dequeue until retirement; stop
and original-generation close bypass application admission. Seven real Store
command-boundary cases pass on stable and Rust 1.89 with all-target CLI Clippy,
formatting and size checks in terminal session 88778. Four isolated guard
neutralizations each failed their production-envelope assertions and restored
the original mailbox bytes; session 95084 then failed only the new test's
formatting, repaired before 88778. HOST-OWNERSHIP-REVIEW.md records the scope.

This starts task 8901 without completing it: executor state, reserved cancel
control and interaction/diagnostic separation remain unfinished; transports do
not construct this owner yet, and bounded egress and autonomous scheduling are
not established. Full changed-source stable host gate and platform CI remain
pending, merged_as stays empty, and completion remains 0/7.

### Complete initial-owner stable host gate — 2026-10-07

Frozen b817071 passes all eight stable host stages in terminal session 45445,
with all seven private host cases passing in debug and release. Exact-source,
clean-worktree, log-hash and no-swap kernel readbacks are recorded in
HOST-OWNERSHIP-REVIEW.md. This evidence-only update does not repeat unchanged
code gates; cancellation, executor state, interaction and transport integration
remain unfinished, and task 8901 stays in progress at 0/7 tasks complete.

### Reserved request-scoped cancellation — 2026-10-07

Each admitted request now reserves charged control metadata and an RPC-ID copy;
cancellation uses bounded host-local numeric keys and retires with its original
reservation. Host saturation does not block cancellation, and unknown/retired
IDs install no future state. Pre-dispatch cancellation suppresses the reply and
claims no journal key; cancellation during coarse tool loops works without
progress metadata. Original-session close cancels its coarse-loop flags.
Eleven host cases and CLI Clippy pass on stable/Rust 1.89 in terminal session
85885; the existing nine cancellation, seven progress and one structured-parity
cases pass on both toolchains in terminal session 15809. Eight independent
guard/accounting neutralizations in 85885 fail their intended assertions,
restore exact sources and are followed by healthy checks.

The manual coordinator adopts EMBEDDING.md and RELEASE.md into task 8901's
footprint for the shared coarse-cancellation correction's same-commit guide and
release documentation; no dependency/manifest owner changes are introduced.
Full changed-source host gates and production transport integration are pending;
executor state, interaction/diagnostic separation, scheduling and egress remain
unimplemented, with task 8901 still in progress and completion unchanged at 0/7.

### Cancellation gate and working-path milestone — 2026-10-07

All eight stable host stages pass on frozen 93af841 in terminal session 18040;
HOST-OWNERSHIP-REVIEW.md records the independent terminal and log-hash checks.
The next integration milestone is an autonomous stdio path through the existing
native lifecycle driver, with quiet-input progress, held-handler responsiveness
and verified shutdown as end-to-end evidence; broader checks follow meaningful
integration milestones. HTTP and plans 0021–0023 remain in scope; task 8901
stays in progress, merged_as stays empty, and completion remains 0/7.
This evidence-only change omits unchanged code gates and uses diff checks.

### Native owner integration for the stdio path — 2026-10-07

The private native owner retains the existing lifecycle driver and sole writer,
uses the same committing command boundary, runs quiet decision passes, and
fences native admission before queue rejection on stop. Monotonic waits never
supply journal timestamps; bounded diagnostic output and the original driver
are retained through shutdown reporting. An independent read-only observer
proves a due deadline completes without any client command; an idle fixed-clock
case proves no extra append, ordinary reads and actual writer release.
Focused stable checks pass 13 host cases and CLI all-target Clippy in session
93252; the same 13 cases and CLI all-target Clippy pass on Rust 1.89
in terminal session 97765. Formatting and file-size checks also pass. This advances the working stdio integration
milestone without releasing later tasks or claiming real-handler/transport
acceptance; task 8901 remains in progress at 0/7, and the broader changed-source
gate will run at the next integration milestone.

### Store-backed protocol boundary for stdio — 2026-10-07

Resource reads/listing and argument completions now use owned typed commands
through the same ordered, count/byte-bounded mailbox as tools; URI and Value
capacities are charged before admission. Fifteen host cases and CLI all-target
Clippy pass on stable/Rust 1.89 in terminal session 87315; focused sensitivity
and boundary evidence are recorded in HOST-OWNERSHIP-REVIEW.md. Production
stdio routing and client-interaction/progress egress remain the working-path
integration frontier; no task completion or autonomous capability is claimed.
