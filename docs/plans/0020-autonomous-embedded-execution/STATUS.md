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

### Shared handler-to-owner workflow path — 2026-10-07

The shared MCP method implementation now supports a private hosted entry with
owned store commands and session-side response formatting. Integration cases
create a workflow through that handler, resolve its resource/completion, and
prove quiet native deadline progress with independent read-only inspection and
actual writer release. Admission busy, cancelled retirement and output failure
remain distinct so no committed mutation is mislabeled server-busy. Twenty-one
host cases and CLI Clippy pass on stable/Rust 1.89; ten existing borrowed
protocol suites pass on each toolchain, with detailed evidence and coordinator
footprint adoption in HOST-OWNERSHIP-REVIEW.md. Production stdio entry,
interactive/progress forwarding, complete egress and real-handler proof remain
open; task 8901 stays in progress and completion remains 0/7.

### Owned byte-stream stdio milestone — 2026-10-07

The actual capped byte-framing loop now composes with the owned native host
through a facade that gives the adapter no writer reference. Unix-stream cases
prove quiet deadline progress, EOF writer release, original control deadline
reuse, and writer release while output is held with delivery still false. All
24 host cases and CLI all-target Clippy pass on stable/Rust 1.89 in session
5017; review and manual footprint adoption are recorded in
HOST-OWNERSHIP-REVIEW.md. The combined milestone's broader stable gate will
use a frozen isolated checkout so work can continue independently. Production
process entry, interactive/progress forwarding, complete egress and versioned
discovery remain open; task 8901 is in progress with completion still 0/7.

The combined owned-stdio milestone at 3ff97a74850a9ffab39f5e41da1b908603cd97c4
passes all eight stable host stages in terminal session 84455, using a frozen
tracked-clean cache checkout, verified actual 1 GiB/zero-swap scope and serial
jobs. Review and the exact log hash are in HOST-OWNERSHIP-REVIEW.md. Subsequent
adapter wait retirement is outside that frozen proof; all 27 host cases and
CLI all-target Clippy pass on stable/MSRV in terminal session 63238. Three
actual protocol guard-removal cases fail as intended in session 15951, then
pass after exact source restoration. This keeps stop, session close and failed
output independent of owner response delivery, without claiming writer closure. Production selection
and remaining transport/protocol/native acceptance stay open; completion is 0/7.

### Private stdio question path — 2026-10-07

The integration priority is one working stdio path before wider transport work.
Hosted elicitation now retains the original admission slot and cancellation
control while waiting outside the store owner, then settles through ordinary
idempotent send with target-instance history revalidation. Actual byte-stream
cases cover a quiet deadline during a question, a stale answer followed by a
fresh question, original RPC cancellation, independent stop, and invalid-answer
correction with the same journal key. Portable cases cover full-count resume,
replacement-session refusal, unrelated writes and exact/plus-one answer byte
limits. These are private composition tests, not installed-handler acceptance.

The final focused stable check is queued in session 96804 and must finish before
source edits resume; it waits for unrelated Cargo work and verifies actual
1 GiB memory/zero-swap limits before starting. The earlier 37-case check passed,
but the subsequent request-key fix and explicit Awaiting envelope require this
new result. Broader checks belong at production stdio activation; the frozen
3ff97a7 gate does not cover these changes. Progress forwarding, complete egress,
versioned discovery, HTTP integration and platform/native acceptance remain
open, with no task completion or dependency release.

The corrected private question path passes all 38 host cases and CLI Clippy on
stable and MSRV, plus ten affected compatibility suites on both; four guard
neutralizations fail and restore successfully, as recorded in
HOST-OWNERSHIP-REVIEW.md, with production activation still pending.

### One logical sample per native pass — implementation pending verification

The native owner now supplies a fixed sampled clock to every operation within
a decision pass and within each shutdown-observation pass, rather than letting
journal operations resample the underlying clock. A new actual native-driver
case advances two due instances in one pass and pins one sample and identical
durable timestamps, with original-owner retirement before assertions. Focused
verification is queued in session 36963 behind unrelated Cargo work under the
existing verified 1 GiB/zero-swap serial scope; no pass is claimed yet.
The stdio path remains the integration priority and completion remains 0/7.

### Hosted output and decision-clock integration — pending verification

The coordinator extends the serial task-8901 footprint to the existing output
queue and new bounded encoding module. Private stdio now selects a distinct
64-frame / 32 MiB retained queue and preflights the 16 MiB encoded-frame limit
without allocating a serialized duplicate; any refusal closes output admission
and is visible to idle input/lifecycle checks. Legacy queued/direct helpers
retain their limits and behavior. New cases reach the actual hosted notifier
and include blocked in-flight allocation, exact count/byte acceptance, plus-one
refusals, canonical escapes and bounded recursion. The native decision-clock
case checks two durable deadline records under one sample.

Session 36963 was retired at its verified idle waiter with exit 130, before any
Cargo stage; no result is claimed for that attempt. Combined CLI-library and
all-target Clippy checks on stable/MSRV are queued in session 28645, with
Cargo-idle waits and memory/swap threshold and actual 1 GiB/zero-swap checks
immediately before each stage. Code and acceptance remain provisional until
that job and the next production-activation milestone are verified. Progress
forwarding, long diagnostics, response-before-notification/commit ordering,
production selection/versioned discovery, HTTP, growth sensitivity and native
platform/installed-handler acceptance remain open; no task completion or
dependency release follows from the implementation checkpoint.
