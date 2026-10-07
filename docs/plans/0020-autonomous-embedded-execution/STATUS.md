# Plan 0020 — Autonomous Embedded Execution — In progress

The roll-up row in [../STATUS.md](../STATUS.md) must stay in sync with this
file; task frontmatter is authoritative and registration and integration
evidence are coordinator-owned.

- **Status:** Registered by hand; execution-host-ownership and nonblocking-execution-completions are in progress;
  private Store ownership, bounded admission and reserved cancellation are
  implemented; a private native owner now retains executor state and drives
  quiet decision passes; initial Linux production stdio wiring is implemented
  with v2 discovery and a passing stable/MSRV real-binary quiet-deadline/EOF path;
  selected compatibility and CLI all-target Clippy pass on both; complete
  scenario acceptance, subsequent milestone gates and HTTP remain outstanding.
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
- **Outcome:** the initial production stdio quiet-deadline/EOF path passes on stable and MSRV;
  no task or full autonomous acceptance is claimed.

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

### Hosted stdio progress context — implementation pending verification

The owned stdio path now submits a charged HostedToolContext with the original
metadata and cancellation control to ordinary dispatch. Its constructor
accepts only the private hosted queued notifier, preventing owner-side direct
transport writes. Metadata/progress copies, adapter retention and context
allocation are conservatively charged before dispatch. A byte-stream case
uses journal_verify with a numeric progress token and checks the final report,
unchanged journal and original writer release. Borrowed/direct helper behavior
is unchanged.

Session 28645 was retired at its verified idle waiter with exit 130 before
Cargo; its frozen successor 94022 failed compilation as recorded in the review.
Corrected output/clock code plus this progress integration is queued in session
64303 for stable/MSRV library tests and CLI all-target Clippy, under serial
Cargo-idle waits and immediate memory/swap threshold and actual 1 GiB/zero-swap
checks. No passing result is claimed yet.

Manual review confirms that progress emission uses the original bounded output
and preserves the existing reporter rate/final semantics. Normal response waits
still need input pumping for cancellation and EOF during long calls, and long
read-only diagnostics still need separation from the owner; these are concrete
production-path gaps, alongside response/change-notification ordering, versioned
discovery, real-binary activation and remaining native/platform acceptance.
Completion remains 0/7 with no dependency release or task landing OID.

### Corrected stdio output/clock/progress milestone — 2026-10-07

Terminal session 98317 passed all 118 CLI library cases and CLI all-target
Clippy on both stable and Rust 1.89, covering hosted frame/queue bounds,
original-owner stdio lifetime, question continuation, one logical sample for
two durable deadlines, and numeric progress metadata reaching its final
byte-stream report. The cache log hosted-progress-corrected-check.log has
SHA-256 `4fbf944b0941561f13ea3e211b96120db43bab189ef4f54ca52e200b742108a7`. Each serial stage verified Cargo idle,
available-memory/swap thresholds and actual 1 GiB/zero-swap kernel limits
immediately before starting.

This closes the two fixture findings from 94022 and 64303. It is a focused
private-composition milestone, not the eight-stage changed-source workspace
gate, production binary activation or native macOS/Windows, installed-handler
and live-client acceptance. Normal response-wait cancellation/EOF pumping,
long diagnostics, publication ordering, versioned discovery and transport
activation remain concrete next steps; metadata/preparation-growth and output
guard sensitivity remain required before final acceptance. No task is complete
and no dependency is released.

### Production stdio observation and lifecycle milestone

Five actual-binary cases and CLI all-target Clippy pass on stable/MSRV at
`c4a0ace`: quiet subscribed/unsubscribed deadline progress, broken-output
shutdown with open input, exact interval acceptance/EOF independence, and
invalid interval refusal before loading/opening. The frozen full stable gate
has started on that source; its result is pending, and progress remains 0/7
with full native/effect scenarios, publication ordering and HTTP incomplete.

### Verified observation/readiness milestone and bounded startup seam

Exact clean `1c529f6` passes all eight stable host gate stages in terminal
session 26190, after stable/MSRV focused checks and three restored observation
guard sensitivity cases. Separate exact clean `344d8e2` passes native-client
21/21, public-surface 17/17 and executor all-target Clippy on stable/MSRV in
terminal session 77843, covering the behavior-preserving immutable startup
data seam. Source identities and terminal log hashes are recorded in
HOST-OWNERSHIP-REVIEW.md; the earlier full gate does not cover this later
refactor.

The original stdio owner observes retained work independently of long
scheduler intervals, suppresses repeated empty-inventory scans and wakes an
ordinary decision for retained readiness. Real installed-handler completion,
held-handler responsiveness, transport worker isolation, eight-completion
fairness, long diagnostics and HTTP remain unfinished; the working stdio
path remains the priority, with 0/7 tasks complete and no dependency release.

### Original raw transport worker polling — focused checks verified

Task 8902 starts with actual owned-stdio selection of worker polling: raw
transport socket exchange, reap and final drop move with the original request;
new helper startup reserves a pool slot and transport charge first, transferred
helpers reserve before adoption, and response/retirement requires actual worker
join plus reap and both EOFs. Four actual held-child/storage/capacity fixtures
are added without a public test-access API. Stable/MSRV executor and stdio checks, all-target Clippy and four restored
guard sensitivity cases pass at exact `80f1e5a`, as recorded in
TRANSPORT-WORKER-REVIEW.md; startup, receipt verification, durable-completion accounting,
real process/MCP responsiveness and worker panic/failure cases are unfinished.
Task 8901 remains in progress, no prerequisite is released, and completion
remains 0/7; native helper fixtures do not imply installed-authority acceptance.

### Worker helper startup candidate

New owned transport construction now dispatches protected helper validation,
socket setup and spawn to its reserved worker; absolute deadlines and pending
cancellation follow the original startup. Joined startup refusal is separately
observed and retires only an empty transport, with actual child/EOF facts false
and original execution retained. Six startup/refusal fixtures pass with the executor unit/lifecycle/public API
checks and both-crate all-target Clippy on stable/MSRV at exact `0705bcb` in
terminal session 99171. That source does not cover the proof-reader candidate.
Route discovery and real installed-handler acceptance remain open, with 0/7 complete.

### Original proof-reader candidate

Completion/recovery and shutdown now dispatch original receipt verification to
workers after actual transport response collection, reusing the original slot,
charge and deadline without a Store or journal allocator. Results and transport
retirement inventory require actual proof-worker join; cancellation and expiry
retain uncertainty. Actual-child wiring and timing/cancellation guard fixtures
are added with runtime checks pending; Root-issued success, held real process/MCP
host acceptance, route discovery, completion storage and worker fault phases
remain unfinished, with 0/7 complete and no prerequisite release.

### Proof-reader runtime checkpoint

Exact `635e933` passes stable executor unit (71), owned lifecycle (3), public
surface (17), CLI unit (129), autonomous stdio (6) and MCP lifecycle (11) checks.
Original session 96881 continues behind unrelated Cargo work for Clippy/MSRV;
new guard sensitivity is prepared but unexecuted. The milestone is partial,
not task completion or native Root-authority acceptance; completion remains 0/7.

### Worker unwind candidate

Linux stdio now wraps its existing panic hook with the inventoried native-worker
filter, scoped only to actual internal transport/proof worker bodies; other
panics retain their fatal policy. Five worker/filter fixtures cover ordinary
panic, unknown observations, discarded published responses and forged-name
refusal, with runtime checks pending. Native Root-handler/owner fault acceptance
and the remaining plan obligations are unchanged; completion remains 0/7.

Worker/filter predecessor `7165ef9` passes stable/MSRV executor unit (76),
lifecycle/public API, CLI/stdio/MCP and both-crate all-target Clippy in terminal
session 87416; two additional transferred-helper/post-publication fatal-hook
cases are added and await verification, without releasing a task prerequisite.

### Working stdio path — verified reservation correction

Terminal evidence in TRANSPORT-WORKER-REVIEW resolves the historical pending
proof-reader, additional unwind and completion-entry capacity checkpoints;
none constitutes installed Root-handler acceptance or final review.
The next working-path correction prevents execution startup from reserving a
second slot while its binding transport retains the first, so all 128 occupied
slots no longer refuse an already-bound attempt solely for extra capacity.
Actual predecessor retirement gates reuse; late selection of worker mode for
an unreserved standalone predecessor still dispatches startup off the caller.

Frozen `acf0f57` passes stable/MSRV executor unit tests (82), both-crate
all-target Clippy, formatting/file-size checks, three isolated intended guard
failures and all 60 restored native tests; session 75717 is terminal with exit 0.
These fixtures use actual child retirement and startup refusal without Root
closure or installed helper mutation. Complete derived-completion accounting,
real-handler stdio execution/settlement acceptance, and milestone-wide gates
remain open; tasks 8901/8902 remain in progress and completion remains 0/7.

### Exact original startup deadline — verification waiting

Committed `9b130ce6700090507e1d737f0938f8d1a25f11e2` carries the original
absolute run deadline into binding, recovery and execution startup material
instead of recomputing a later deadline during request preparation.
Worker-entry fixtures compare the received Instant with the original run's
deadline and then retire startup refusal without manufacturing closure proof.
Original verification session 58142 is confirmed live, with its script process
1565135 and idle-wait child 1565141 waiting for unrelated Cargo process 1561491;
no runtime, Clippy or deadline-extension sensitivity pass is claimed yet.
Continue that existing session and frozen worktree rather than restarting it
on an observation timeout; its task-cache log is
`native-original-deadline-check.log`, and its scope enforces 1 GiB/zero swap.
Task status remains unchanged, with no final-review or installed acceptance claim.

### Deadline checkpoint and response-depth correction

Original session 58142 is terminal with exit 0: frozen `9b130ce6` passes
84 executor unit tests and executor/CLI all-target Clippy on stable/MSRV,
formatting/file-size checks, two intentional deadline-extension fixture
failures, and all 62 restored native tests; terminal evidence is recorded in
TRANSPORT-WORKER-REVIEW and supersedes the preceding live-job checkpoint.

Completion storage review then identified unbounded accounting recursion on
caller-built arrays/objects before canonical depth validation. The default
JSON ceiling now gates that recursion, with an exact-depth/one-level-over
public verifier fixture. Initial compilation rejected the misplaced fixture;
its corrected frozen source is `55efd81a`, and session 34198 is confirmed
live waiting for unrelated Cargo before verification, with no pass claimed.
The broad eight-command stable host gate is prepared for this transport
checkpoint and remains unexecuted, pending success of that original depth job.
Derived completion/handoff storage accounting and real-handler stdio
execution/settlement acceptance remain open; completion remains 0/7.

### Response-depth checkpoint and milestone host gate

The corrected frozen `55efd81a` now passes 85 executor unit tests and both-crate
all-target Clippy on stable/MSRV, formatting/file-size checks, the isolated
intended depth-guard failure and all 63 restored native tests; original session
34198 is terminal with exit 0 and hashes are in TRANSPORT-WORKER-REVIEW.
The prepared broad stable host gate has started serially in session 37722
after confirming that success and exact clean frozen source. It is confirmed
live compiling workspace debug tests, with remaining stages and a full-gate
result pending; continue that original handle and preserve its frozen worktree.
The job retains the actual 1 GiB/zero-swap cap and dedicated task cache.
No native acceptance, task completion or prerequisite release is claimed.

The original milestone session 37722 has passed the complete stable debug
workspace suite and doctests on unchanged clean frozen `55efd81a`; release
Cargo process 1673689 is confirmed live compiling the next stage.
Formatting/file-size/debug checks are passed, with the other five host-gate
commands still pending. Continue the same session, log and frozen source;
the full gate and installed/native acceptance remain unproven, and plan
completion stays 0/7.

The same milestone session 37722 has now passed the complete stable release
workspace suite and doctests on exact clean frozen `55efd81a`; it remains
live at the next stage's idle check, waiting for unrelated Cargo before
workspace Clippy. Four host-gate commands are passed and four remain pending:
Clippy, warning-denied documentation, explicit zero-dependency and embed
acceptance commands. No full-gate, native acceptance or task completion is
claimed; preserve the original session and frozen source.

### Stable transport milestone — all host gates passed

Session 37722 is now terminal with exit 0 and
`NATIVE_TRANSPORT_MILESTONE_HOST_GATE_PASSED` on exact clean frozen `55efd81a`.
All eight CONTRIBUTING stable host checks pass, including full debug/release
workspace suites, all-target workspace Clippy, warning-denied documentation
and explicit zero-dependency/embed acceptance; review-range diff checks pass
as well. Terminal evidence and its log hash are in TRANSPORT-WORKER-REVIEW.
This resolves all preceding pending stages for that frozen transport source.

The next working-path obligation remains complete bounded retention through
completion settlement and event delivery, followed by real-handler stdio
acceptance; installed Root-handler success and native macOS/Windows/live-client
axes remain unexecuted. HTTP and other plan obligations remain open, with
tasks 8901/8902 still in progress and completion unchanged at 0/7; no
prerequisite is released by this local host milestone.

### Complete client submission in the quiet stdio path — 2026-10-07

Commit `d1af4bf6` removes direct Store machine preloading from both real-binary
quiet-deadline cases: the client now submits `machine_create` followed by
`instance_create`, and the tests require successful replies before observing
quiet durable completion, asynchronous subscription output and EOF retirement.
This closes a submission-coverage gap without claiming native handler success.
Focused stable/MSRV `autonomous_stdio` tests and target Clippy are queued in
session 30366, script `stdio-client-submission-check.sh`, on frozen `d1af4bf6`
in the existing isolated cache worktree; the live script PID 1799728 was
confirmed while waiting for competing Cargo PID 1799180, so no passing result
is recorded yet and the prior eight-stage milestone remains the last full gate.
The installed containment helper remains owned by `nobody`, so genuine native
handler acceptance remains outstanding; task completion stays 0/7.

### Quiet native workflow acceptance path — 2026-10-07

Commit `c83bbb59` updates the existing production workflow scenarios instead
of adding another substitute driver: Linux embedded discovery requires
`fsm.executor/2` and autonomous progress; after the begin event the client
sends no observation requests until a read-only Store sees terminal completion.
The existing success, preflight-refusal and compensating-failure assertions
still check handler order, external resource state and verified history.
The client then closes stdin, requires successful bounded process exit and
valid remaining JSON-RPC output, and reopens the writer to check zero unresolved
claims and acknowledged handoffs; borrowed helpers retain v1/request-driven
acceptance and other native platforms retain their current contract.

This is a test requirement, not a claim that native scenarios have passed:
Linux cases remain explicitly provisioning-dependent and ignored by ordinary
Cargo runs; the wrongly owned installed helper has not been altered.
Session 95283 (`quiet-native-workflow-check.sh`, live PID 1835723) waits for
session 30366's original script PID 1799728 to retire successfully before
advancing the same isolated source worktree to frozen `c83bbb59`; stable/MSRV
ordinary tests and both integration targets' Clippy are queued, with actual
1 GiB memory and zero-swap limits checked at every build stage.
Both handles were confirmed live while unrelated Cargo PID 1799180 remained
running; no checks have been declared successful and completion stays 0/7.
