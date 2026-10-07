# Initial private execution-host ownership review

Implementation base: `31c579d`; task 8901 remains in progress with no landing
OID or completed acceptance inventory. This is the initial owner/admission
portion of the task, not autonomous execution or production transport routing.

The Store and injected logical clock live in Owner, never in shared state.
Session handles submit owned tool arguments and RPC IDs into one admission-order
queue; ordinary dispatch retains existing validation, read-only refusals,
request IDs and fingerprints. The owner captures result, journal.last_seq and
the appended interval before another command. No session input or transport
write occurs in this primitive; interactive dispatch currently returns the
ordinary unsupported-session error rather than initiating an interaction.

Admission charges the complete retained command, Value storage, String and
array capacities, conservative BTree allocation allowances and envelope
metadata. There are no retained wire copies. Host limits are 32 commands/32 MiB;
original-session limits are 8 commands/16 MiB. RAII reservations stay charged
through dequeue and dispatch, with payload fields destroyed before reservation
retirement. Stop is one coalesced reserved state, closes admission and rejects
queued work; an executing operation completes normally. Owner destruction also
stops admission. Original-session close is a separate atomic control and
response channels belong to exact monotonic generations, so an old RPC ID
cannot route a response to a replacement incarnation.

Seven private harness cases use the actual owner/envelopes and physical Stores:
mixed ordered reads/writes with one writer and complete captured prefixes;
eight concurrent callers with per-session FIFO and exactly eight creations;
exact host/session count limits and saturation-independent stop; exact owned
byte limits, plus-one refusal and charges surviving dequeue; exact host byte
limit-plus-one; closed-generation refusal; lost-response replay and differing
content conflict. Store reopen independently verifies the durable prefix.

Stable and Rust 1.89 all-target CLI Clippy and all seven cases pass in terminal
session 88778 under asserted 1 GiB RAM and zero swap, serial workers and
cache-only scratch/artifacts; formatting and file-size checks pass. Four
sensitivity controls in session 95084 independently neutralized host count,
session count, host bytes and session bytes; every control returned test exit
101 with an assertion failure, then restored byte-identical mailbox source
SHA-256 `cc17105ba4778e1120ee552a0f43b9ae025640163ab2d93131e980dd3c6219c3`.
That aggregate later failed only the unformatted new test; 88778 validates the
corrected healthy source. Logs are retained as execution-host-sensitivity-*.log
under the dedicated task cache. The first focused run (85649) found two test
expectations using incorrect status/error vocabulary; corrections cite SPEC's
completed status and req/request_id_conflict, and subsequent checks pass.

Review limits remain explicit: the private constructors await transport
adapters, hence staged dead-code allowance; executor state and lifecycle
integration are absent; reserved cancellation is absent; interactive and
long-running diagnostic separation is unfinished; reply slots are single-slot
nonblocking channels, but this does not prove the later egress byte budget.
No autonomous discovery record, supported CLI-library API, journal/hash change,
new public error code or production busy mapping is introduced. The full
changed-source stable host gate and native CI are pending; task completion
requires the complete 8901 inventory and later cross-plan lifecycle evidence.

## Frozen initial-owner gate — 2026-10-07

Session 45445 is terminal exit zero at
`b817071829d1cf79f77122a93ac6784bb211ddca`; all eight stable host stages
pass, including full workspace debug/release tests, all-target Clippy and
warning-free docs, zero-dependency and embedding checks. All seven new host
cases occur twice as passing tests in the workspace log. The wrapper verifies
exact HEAD and a clean tracked worktree after every stage and at termination;
independent post-terminal readback agrees. Log
`execution-host-owner-full-stable-gate.log` under the task cache has SHA-256
`13660141645039ecb648cd9e03359bf5308efe9e443243d9d90fa4e1f48aa3be`.
The frozen `31c579d..b817071` range passes diff checks, and mailbox source
matches the sensitivity-restored hash above. Kernel readback of the live gate
PID 2228170 confirmed 1 GiB RAM, zero swap usage and no OOM kills; serial workers
and cache-only artifacts were used. This local primitive gate does not complete
task 8901, implement transport autonomy or replace platform CI/native acceptance.

## Reserved cancellation and coarse-loop repair — 2026-10-07

Implementation base: `66027d3`; task 8901 remains in progress. Admitted/in-flight
control entries are bounded by the same 32 host/8 original-session command
counts and reserved before dispatch. Charges include the parsed RPC-ID copy,
owned capacities and a conservative 12 KiB allowance for envelope/control
metadata, cancellation-set BTree entries and bounded internal numeric keys,
including temporary duplicate keys on repeated cancellation. The internal
request token is not a journal key or client RPC ID. Control retirement is
part of reservation retirement and unknown/retired IDs allocate no future
cancellation state. Stop/close/cancel remain independent of application
capacity; original-generation close marks its coarse-loop flags cancelled.

Cancellation before dispatch suppresses the reply, performs no Store operation
and leaves the caller's original journal key reusable. After dispatch, the
existing shared flag reaches every coarse-loop handler even without a progress
token; single macrosteps retain their noninterruptibility and cancellation
does not revoke an already-created durable workflow. The prior dispatch branch
only passed the flag when progress was live or cancellation was already true,
so an arriving cancellation could be missed by a tokenless call. All four
capability documents move with this correction; the coordinator adopts the
embedding/release guide paths into the task footprint, with no manifest or
dependency adoption required. Production host adapters remain unimplemented.

Eleven private host cases pass on stable and Rust 1.89 in terminal session 85885,
with CLI all-target Clippy, formatting and size checks. New cases cover
cancellation at a full 32-command host queue, no-response/key reuse, original
generation isolation, cancellation injected through the logical clock at an
actual coarse-loop boundary without progress metadata, and exact-budget RPC
control-copy accounting before allocation. Existing nine cancellation, seven
progress and one structured-parity cases pass on both toolchains in terminal
session 15809 (the initial filtered integration invocations ran zero cases;
the subsequent unfiltered invocations provide the actual evidence).

Eight isolated neutralizations in 85885 each return test exit 101 with an
assertion failure: host/session count and byte limits, RPC-copy charging,
pre-dispatch cancellation, generation matching, and coarse-loop flag forwarding.
All modified files are restored byte-identically in finally blocks before
healthy stable/MSRV checks. Restored SHA-256 values:
- mailbox: `73c852da85b58f4cb882bbf0ec4fa16def6b63da510e8437b153ec3831eb36c1`
- owner: `acf6baefc6f7bab37bf53fceae5f318eb598873f52cfd71d9c30d612ea41af17`
- dispatch: `7ae676b58adacede41e9c502d27d2a7e2bfea7d10e3eca42a650d1461dc7a7c8`

Sensitivity logs are execution-host-cancel-sensitivity-*.log under the dedicated
task cache; all runs assert 1 GiB RAM/zero swap and use serial workers. The
full changed-source stable host gate, platform CI, executor-state integration,
interactive/diagnostic separation and production routing remain outstanding;
no task completion or autonomous discovery capability is claimed.

## Complete cancellation stable host gate — 2026-10-07

Frozen `93af8416af59349a17a06ae613e1adc64171691c` passes all eight
CONTRIBUTING stable host stages in terminal session 18040 (exit 0), including
workspace debug/release tests, all-target Clippy, warning-denying Rustdoc,
zero-dependency and embed acceptance. Independent terminal log readback confirms
eight zero stage exits and GATE_FAILED_STAGES=0; tracked sources remain clean
and HEAD matches the frozen revision. Cache log
execution-host-cancellation-full-stable-gate.log has SHA-256
`77f301a23d8fd93403d6ff95cf7798e413bd9fec90582615e4d8e6246afe94ae`.
The wrapper asserts actual kernel 1 GiB RAM/zero-swap limits and serial workers.
Native platform CI and complete transport/lifecycle acceptance remain open.

Following the user's integration direction, the next milestone is one complete
autonomous stdio path using the existing native lifecycle driver, bounded owner
commands and independent scheduling; acceptance must demonstrate quiet-input
execution, a responsive request while a handler is held, and verified shutdown.
Focused checks accompany development, with broader gates at meaningful
integration milestones; no private primitive is treated as shipped autonomy.
HTTP integration and the remaining plans stay in scope.

## Native owner integration toward the working stdio path — 2026-10-07

The Linux private owner now retains the existing OwnedNativeExecutor instead
of placing a writer in a handler thread. Its command path calls the same
apply_command boundary as the writer-only owner. A monotonic Condvar deadline
bounds idle waits independently of injected logical time; native decision
passes happen initially, on the configured interval, or after eight commands.
A separate 50 ms control check prevents a long poll interval from hiding an
independent original lifecycle stop. Host Handle::stop fences original native
admission before rejecting queued applications. Existing lifecycle polling
preserves the original shutdown deadline and report; the returned NativeExit
retains the original driver even on uncertainty. A retirement guard disconnects
queued replies on owner destruction/unwind, without calling transport I/O.
Diagnostic publication uses the existing bounded DiagnosticOutput worker;
dropped diagnostics and output-drain evidence remain available to the adapter.

Independent tests cover quiet scheduling with a fixed logical clock and a due
deadline reaching completed at exactly the supplied timestamp without creating
any session or command. Inspection uses Store::open_read_only and therefore
cannot tick the executor. The idle case proves repeated quiet passes add no
records, an ordinary read still captures the original prefix, only one writer
exists, stop closes native admission and rejects further commands, and actual
verified shutdown permits a second writer to reopen the unchanged journal.
All 13 execution_host cases and CLI all-target Clippy pass on stable in terminal
session 93252 and Rust 1.89 in terminal session 97765, under asserted actual
1 GiB RAM/zero-swap limits and serial workers; formatting and size checks pass.
An initial focused compilation failed because the new test named a nonexistent
Store::verify method; the test now uses authoritative reopening/folding instead.

Review limitations: no production transport constructs this owner; interactive
continuations, egress publication, completion-batch fairness, real process/MCP
held-handler responsiveness, and uncertain-owner transport retention still
require integration proof. No new journal/error/wire/public-library contract
is introduced, and all four capability documents describe this private scope.
The broader changed-source stable host gate is deferred to the working-path
integration milestone at the user's direction; the prior frozen cancellation
gate does not certify this new code. Platform CI remains unexecuted for this
unit, task 8901 remains in progress, and no downstream task is released.

## Store-backed protocol command boundary — 2026-10-07

Owned resource-list/resource-read/completion commands now share the exact tool
mailbox, original-generation cancellation and reservations. Their dispatcher
reuses the existing resources/complete implementations; immutable read results
capture the same complete prefix as admitted writes, with no publication
interval. Original native handler metadata is resolved from the retained
driver's table; adapters receive no writer or transport borrow through this
boundary. Resource URI capacities and completion Value allocations are charged
with the same RPC-copy/envelope accounting before admission.

Fifteen host cases and all-target CLI Clippy pass on stable and Rust 1.89 in
terminal session 87315. The new mixed protocol/tool case proves a resource read,
resource listing and instance-ID completion all see the preceding committed
creation in admission order, and none appends a record. The URI-capacity case
accepts exactly the session byte limit, rejects limit-plus-one and a further
tool command sharing that full budget, and preserves the journal on stop.
The initial mixed case used the wrong template parameter; it now uses the
existing resource registry's fsm://instance/{id} vocabulary, without changing
completion behavior. A private-interface warning was fixed by retaining MCP
visibility for the command type.

The isolated URI-charge neutralization in terminal session 78755 failed the
actual submission assertion with test exit 101, restored source bytes in
finally and passed the healthy rerun; its retained log is execution-host-protocol-read-charge-sensitivity.log
in the task cache; restored mailbox SHA-256 is
`eec54c2d3ae8113691259c14a0b93ec4e3b0ea3a027cadd498de85fb113a3ab6`.
Broad gates remain assigned to the working-path milestone;
production stdio wiring, interaction/progress egress, real held-handler proof
and full lifecycle acceptance remain open, with task 8901 still in progress.

## Shared protocol handler to native owner — 2026-10-07

The existing method handler now shares its implementation between borrowed
compatibility helpers and a private hosted entry. Store-dependent tools and
resource/completion calls submit owned commands; response formatting, client
state and subscriptions remain in the session. Tool arguments move out of
the parsed params object instead of retaining a cloned argument body. Borrowed
helper signatures remain unchanged. The manual coordinator adopts methods.rs
and methods/ into task 8901's footprint for this dispatch-boundary extraction;
no dependency/manifest owner or task landing OID changes.

The actual shared entry now drives initialize, workflow creation, resource
resolution and identifier completion through the owner. A second case submits
creation through that entry to the native owner, stops sending client requests,
advances injected logical time to the exact deadline and independently observes
the durable completed state through a read-only Store. The adapter's separate
clock cannot supply the writer timestamp. Native stop then permits actual
writer reopening. This is a method-entry integration case, not a real binary
stdio/process/MCP fixture or production lifetime proof.

Review caught a material error in the initial hosted wrapper: mapping every
WouldBlock to server-busy could send a false admission refusal after a committed
mutation when output refused delivery. Internal AdmissionBusy and Retired
types now distinguish that condition from output failure and closed admission.
Named cases prove one failed output attempt leaves the committed instance
recoverable, busy refusal leaves the journal unchanged, closed admission ends
the session, and pre-dispatch cancellation suppresses the reply while allowing
the same RPC and journal key to be reused. Initial new tests incorrectly
expected isError=false on successful replies; they now assert the existing
structuredContent instance handle, without changing the wire contract.

All 21 host cases and CLI all-target Clippy pass on stable and Rust 1.89 in
terminal session 22022. Ten existing unfiltered protocol suites (resources,
completion, progress, cancellation, elicitation, structured parity, lifecycle,
affordance goldens, embedded read-only and serve modes) pass on stable in
terminal session 47217 and Rust 1.89 in terminal session 60350; later changes
only tighten the new private hosted error classification. All checks assert
actual 1 GiB RAM/zero-swap kernel limits and use serial workers.

The isolated output/admission neutralization in terminal session 37980 restored
the old over-broad WouldBlock mapping and failed the actual committed-output
case with test exit 101; exact source restoration preceded the healthy rerun.
Restored methods SHA-256 is
`10d973e0da1438bf8d7967ba84ef7ce7315e5ad6c5a2ad0197d27bcb56460a6c`;
the task-cache log is execution-host-output-admission-sensitivity.log. Full changed-source
gates remain assigned to the requested working-path milestone. Production
process entry, interactive continuations, progress forwarding, complete egress
and real held-handler/lifecycle acceptance remain incomplete; no task is done.

## Owned byte-stream stdio composition — 2026-10-07

The new private composition starts the original native owner on its own worker
and reuses the real capped framing/session loop and shared method dispatcher.
SessionStore::Hosted returns no Store borrow and performs no client-driven
executor tick. Input idle stays live; EOF/broken output/control request original
shutdown and reject queued commands without escalating an earlier drain.
The report retains either the returned original driver or a still-live worker,
uses the original absolute deadline for output retirement, and exposes unknown
diagnostic loss as None. No live worker is joined after the deadline.

Three actual Unix-stream cases cover byte-level initialization and creation,
quiet deadline completion observed only through a read-only Store, EOF and
actual writer reopening, independent stop while input remains open, and an
output worker held until after the original deadline. The held-output report
proves writer release while output_drained remains false; it does not infer
interruption/retirement of arbitrary blocking Read/Write objects or real
process/MCP domain closure. All 24 host cases and CLI all-target Clippy pass
on stable and Rust 1.89 in terminal session 5017 under verified actual 1 GiB
RAM/zero-swap limits with serial workers. The initial 23-case run passed tests
but failed Clippy on literal formatting in the new fixture; that was fixed
before 5017. Formatting and source-size checks pass after the exact-wording
mode instruction extraction in c5722fd; this extraction is a separate refactor
commit and introduces no wire or persistence change.

The adapter and native owner share one bounded operator-output worker;
each producer retains its own rejection count and the report combines them,
avoiding synchronous adapter diagnostic writes during shutdown. This final
sharing adjustment is covered by the frozen milestone gate recorded below;
the earlier 5017 focused result does not cover it.

The manual coordinator adopts serve.rs, serve/ and notify/diagnostic_output.rs
for this ownership/dispatch staging milestone; final process-entry selection remains task 9001's obligation
and these shared boundaries are integrated serially, with no parallel mutation
or manifest/dependency change. Task 8901 stays in progress. A frozen isolated
checkout will run the broader eight-stage stable host gate for this combined
integration milestone; further active-worktree changes are not covered by it.
Production selection, interactive/progress forwarding, complete egress,
versioned discovery and real held-handler/native/platform acceptance remain
unfinished, and no autonomous capability or task completion is claimed.

## Admitted adapter wait retirement — 2026-10-07

Review of the working stdio path found that an adapter blocked on a response
could outlive the original stop deadline if the owner did not return from its
current operation. The wait now checks original session close and original
native lifecycle phase every 50 ms, suppresses the retired response and closes
that session without claiming writer completion. A held owner/no-turn fixture
will require the actual shared method caller to return before the owner is
allowed to run, then verify that stop had not already proven writer release,
that final owner shutdown releases it, and that the queued key stayed unclaimed.
A second held-owner case covers original session close while a resource-list
read waits. Both fixtures release the owner before failure assertions, so a
neutralized adapter guard does not strand their waiting callers. Retirement
is checked again after receipt, before returning the original response. A
third held-owner case asynchronously fails the actual queued protocol writer
after command admission and requires a BrokenPipe result before an owner
turn, preserving that initiating I/O failure without a false busy response or
writer-release claim. Both tool and protocol-read callers use this same wait. All 27 host cases and CLI all-target Clippy pass on stable and Rust 1.89
in terminal session 63238, under verified actual 1 GiB/zero-swap limits with
serial workers. The separate eight-stage gate at frozen
3ff97a74850a9ffab39f5e41da1b908603cd97c4 does not cover this follow-up. No task completion or production acceptance follows.

## Frozen owned-stdio milestone gate — 2026-10-07

Terminal session 84455 passed all eight stable host stages (format, source size,
debug and release workspace tests, all-target workspace Clippy, warning-denied
rustdoc, zero-dependency and embedding acceptance) against detached checkout
3ff97a74850a9ffab39f5e41da1b908603cd97c4. Each stage and final exit verified
that exact HEAD and a clean tracked checkout; the active workspace advanced
independently and its follow-up adapter wait changes are outside this proof.
The task-cache log owned-stdio-integration-full-stable-gate.log has SHA-256
`f05187c8930660c4fc4619d275a867ca1eaef4e8d1ded4d05aaa15fb19564d04`.
Actual kernel limits were verified as 1073741824 memory.max and zero
memory.swap.max before Cargo; in-flight readback also observed zero
memory.swap.current and zero OOM/OOM-kill events. Heavy jobs ran serially.
An initial automatic approval rejection prevented overlapping another Cargo
job; verified job exit preceded the successful bounded gate launch.
This is the working byte-stream integration milestone, not production process
selection, authority/platform or real native handler-domain acceptance; those
axes and all remaining plans stay open with no task marked complete.

Terminal sensitivity session 15951 disabled original stop/session retirement,
then separately disabled output-failure detection; each of the three actual
protocol cases executed and failed its assertion with test exit 101. Both
source files were restored byte-for-byte in finally before all three healthy
reruns passed. Restored host/mod.rs SHA-256 is
`f8cbececc071dc38a06b4e2cbe94c1e5c2aba50ac78e417808389432978eadb5`;
methods/store_access.rs is
`99c1d4ac130eaa566c308c85266c93e10ee9267b73988c6cda976476725d54fd`.
The task-cache focused log owned-stdio-control-wait-focused.log has SHA-256
`aa11d59c7257e937fb228078eb70742e62a2fce2cd7b33ce61b68e92f9d7c855`;
owned-stdio-control-wait-sensitivity.log has SHA-256
`9c7df769c78c80e8bb715a9549301db17d63f27fb0fd415d04cc1fb6a39d087a`.
Formatting, source-size and complete diff checks pass. Native macOS/Windows,
authority, installed/live client and held real handler-domain axes were not
executed by these checks; the full changed-source gate belongs to the next
integration milestone. Manual review retains original generation, cancellation
and reservation behavior, performs no Store I/O in the wait control, keeps
BrokenPipe distinct from silent retirement/admission busy, and makes no new
closure claim. Production selection and task completion remain outstanding.

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

### Corrected private elicitation verification — 2026-10-07

Terminal sessions 79389 and 72314 passed all 38 host cases and CLI all-target
Clippy on stable and Rust 1.89; session 56518 passed the ten affected MCP and
stdio compatibility suites on both toolchains. Initial session 96804 failed
only the new invalid-answer assertion, which looked for request_id directly
on error rather than the established error.details object; the assertion was
corrected before these successful checks.

Sensitivity session 89756 disabled target-history guarding, original-session
identity, post-preparation cancellation and original request-key attachment
one at a time. Each actual case executed and failed its assertion with exit
101; exact source bytes were restored in finally and each healthy rerun passed.
Actual kernel limits were checked as 1 GiB memory and zero swap, with one worker
and a Cargo-idle wait before each stage. The cache evidence digests are:

- `hosted-elicitation-corrected-path-check.log`: `760ca55352791fa306821131ab98ea5cebebd7c42ee6ba03d4cf9550c4e01e95`.
- `hosted-elicitation-msrv-path-check.log`: `ea786d85fe3f1e59ebacbe7575d3b4ff5ca455a9480b891a08a919ddce8fc0b7`.
- `hosted-elicitation-sensitivity.log`: `18a26eace0ac68820e891634467ac540d4c104a50d75c3dfe1858b35898e3441`.
- `hosted-elicitation-compatibility.log`: `76d017b699023fafec95f2f54b8e2874846390dd0e8caefab5e890feb170fd5d`.

Manual review confirms that the continuation has no Store borrow or compiled
machine clone, keeps the original reservation/control through settle or drop,
resumes at full count without a new slot, and charges retained question and
answer growth before settlement. The owner performs no client I/O; ordinary
send retains dedup-before-precondition ordering. Generic WouldBlock still
reaches the borrowed timeout check; hosted idle input permits cancellation and
independent stop. Full changed-source gate, progress forwarding, complete
egress, production selection, preparation-growth wiring sensitivity and native
macOS/Windows, installed-handler and live-client acceptance remain outstanding.
No task is complete and no prerequisite is released.

Reservation growth sensitivity session 71126 remained in its Cargo-idle wait
behind an unrelated workspace test and started no Cargo stage; the coordinator
interrupted only its exact original Python process to continue implementation.
Its finally handler restored the original guard and terminal exit was 130;
readback confirms both ordinary growth conditions are active. The two growth
neutralizations and preparation-phase wiring sensitivity are deferred to the
next integration milestone, with no passing evidence claimed for this attempt.
Formatting, source-size and the complete staged diff check pass.

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

Frozen output/clock verification session 94022 ran against clean exact
7f506c4 in a detached task-cache checkout and exited 101 at compilation: the
new escaping fixture used JsonLimits::default() although the established
constructor is the JsonLimits::DEFAULT constant. No test or Clippy pass is
claimed for that source. The fixture is corrected in the active workspace;
new progress-context implementation is outside that frozen verification range.

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

Combined session 64303 compiled the corrected code and passed 117 library
cases, including bounded hosted output and actual stdio progress forwarding;
the remaining clock case failed only its absolute Vec length assertion.
load_records includes the sequence-zero Genesis record. The fixture now
selects records after the captured pre-pass sequence and requires exactly two
DeadlineApplied records at the single sampled timestamp, then verifies the
reopened sequence and journal. It does not change a golden or runtime behavior.
Session 98317 is the corrected combined stable/MSRV verification; its result
remains pending and no all-target Clippy or MSRV pass is inferred from 64303.

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

### Owned response waits and adapter unwind checkpoint

Implementation commits `9ba4b09` and `fafc28f` retain the original native
owner and session while the protocol adapter reads cancellation, ping and EOF,
defers ordinary requests under eight-frame/16 MiB capacity limits, and catches
adapter unwinds before requesting original-control shutdown.

The serial milestone log `hosted-input-wait-corrected-check.log` has SHA-256
`6a4f8636d74411c61a1cea86226e6b3cf53a8de7738c51ff100df9a6604fc96f`.
Stable ran 123 CLI library tests successfully before the adapter-panic fixture
was added; MSRV 1.89.0 ran all 124 successfully, including that fixture.
CLI all-target Clippy passed on both toolchains, but the source changed
between stages, so this is preliminary integration evidence rather than an
exact-HEAD gate; the standalone stable adapter-panic check remains queued.
Each Cargo stage waited for independent Cargo work to retire, checked memory
thresholds, and asserted actual 1 GiB memory and zero-swap cgroup limits.

The ninth deferred frame case reaches hosted method handling while the original
owner is held, observes the busy response before dispatch, preserves the first
deferred request, and verifies the reopened journal contains only the original
creation. The capacity limit case is helper evidence; production byte-limit
wiring and guard-neutralization evidence are still required. No task is done,
no scheduling dependency is released, production stdio is not yet activated,
and native platform and complete egress acceptance remain outstanding.

The encoded-frame boundary fixture now derives its 16 MiB expectation
independently from SPEC and attempts the oversized frame on an empty hosted
queue, asserting zero retained bytes and no sink publication; this closes the
review defect where queue capacity could mask an ineffective encoded-frame
guard, but execution and guard-neutralization proof remain pending.

### Production stdio first working path

`dd6446c` selects the owned host in the actual Linux CLI and publishes
`fsm.executor/2`/`autonomous` only through its hosted executor resource;
borrowed and HTTP discovery retain v1. Matching guidance and instruction
goldens are integrated without releasing any scheduling dependency.
Stable CLI library verification passed all 124 cases on this production
source. The first real-binary case then failed because its empty handler table
was refused before startup, as required by the established config contract.
The fixture now declares a manual effect, retains startup stderr for actionable
failures, and passes on stable: actual discovery is v2/autonomous, creation
returns, stdin remains open with no further protocol traffic, read-only store
inspection observes the deadline reaching completed, EOF exits successfully,
and the original writer reopens. The standalone stable adapter unwind case
also passed. Broader compatibility, MSRV production, full scenario coverage
and the frozen full gate remain outstanding; this is one working path, not
completion of any task or native-installed authority acceptance.

The corrected production milestone ran against committed source `bd36328`
without further Rust changes and terminated successfully (session 40507,
exit 0). On stable and MSRV 1.89.0, all 124 CLI library cases passed, the
actual-binary autonomous stdio case passed, and compatibility suites passed:
embedded_read_only (4), mcp_affordance_golden (5), mcp_executor (3), and
serve_modes (12), followed by CLI all-target Clippy with denied warnings.
The terminal log `production-owned-stdio-corrected-check.log` has SHA-256
`2f93279030feb9a35ab37673486ca0300bf6cce6de67128961caad9b31a6e91c`.
Each Cargo stage asserted actual 1 GiB memory and zero-swap limits, checked
available-memory/swap thresholds and waited for independent Cargo retirement.
This milestone proves one production path on both host toolchains; the frozen
workspace debug/release/docs/full gate, remaining success/retry/compensation/
recovery/notification scenarios and native platform axes are still required.
Task statuses and all unreleased dependencies remain unchanged.

The actual-binary inventory now additionally covers a subscribed instance:
after subscribe acknowledgement, no more requests are sent and an actual
`notifications/resources/updated` frame must arrive with the expected URI,
followed by read-only confirmation of terminal deadline state and EOF cleanup.
The unsubscribed case remains separate, with independent fixture directories
for parallel test execution. A broken-stdout case closes the actual stdout
receiver, retains open stdin, requires an explicit failure exit within the
lifecycle bound and reopens the unchanged empty journal; input EOF cannot
supply its shutdown trigger. These new cases are queued for stable/MSRV
execution and all-target Clippy, not yet passing acceptance evidence.

The initial subscription/output run terminated 101 after stable passed the
quiet subscribed and unsubscribed cases and invalid interval refusal. Broken
stdout caused the expected failure exit, but its diagnostic assertion failed:
the hosted retirement loop waited until the original deadline despite a
permanently broken output queue, leaving the final regular-file diagnostic
worker no delivery window. The loop now finishes once original native owner
retirement and operator drainage are proven and protocol output is broken,
retaining the original failure and false drainage facts; a healthy blocked
sink still uses the original deadline. Exact one-millisecond/24-hour CLI
boundary acceptance cases are also added. The corrected stable/MSRV actual
binary inventory and Clippy checks are running; no successful final result
or additional task completion is claimed at this checkpoint.

The broader working-path gate is prepared on the clean detached cache checkout
`production-stdio-full-gate-source` at exact source
`582c6a6c1f56865697b366670a834061b9e4f271`. The script
`production-stdio-full-stable-gate.sh` covers all eight required stable stages,
checks the frozen commit and tracked cleanliness before/after each stage,
waits for Cargo idle and asserts memory thresholds plus actual 1 GiB/zero-swap
cgroup limits for every stage. It has not been launched: the corrected narrow
actual-binary/MSRV check remains live, waiting for independent Cargo work.
Preparation is not passing gate evidence, and the earlier 3ff97a7 full gate
does not cover this production integration.

The corrected output-retirement run first proved all five real-binary cases
on stable, then stopped on two write_literal fixture lint findings; the
conditional full-gate wrapper therefore terminated without starting a stage.
`c4a0ace` corrects those fixtures without changing protocol bytes. Its focused
check terminated 0 (session 37304): all five actual-binary cases and CLI
all-target Clippy pass on stable and MSRV 1.89.0. This includes quiet subscribed
and unsubscribed deadlines, broken stdout with stdin still open, exact interval
boundaries with EOF cleanup independent of the timer, and invalid intervals
refused before table loading/store creation. The terminal log
`production-quiet-subscription-lint-corrected-check.log` has SHA-256
`d69f7da162e0e85be1338ae968dfca57276b86a5ed920e37f76adc77a1c44969`.
The frozen full-gate checkout and expected source were updated to `c4a0ace`
and the eight-stage stable gate is now launched; it is not yet passing proof.
No task is marked done and native-installed authority, additional effect/
retry/compensation/recovery cases, complete publication ordering and HTTP
remain outstanding.

The frozen full stable gate on c4a0ace stopped at the size stage: serve.rs
had 1007 lines; formatting passed and no workspace test stage ran. Refactor
6cb9b40 moves unchanged panic formatting/installation into serve/panic.rs
and preserves the public formatter re-export, bringing the file below 1000.
Review of that boundary then found that production run_with_mode installs an
aborting panic hook before native startup, so the earlier private catch_unwind
fixture did not prove actual production adapter cleanup. The new adapter-thread
unwind scope permits the installed hook to return only inside that cleanup
boundary, queues a bounded static operator diagnostic, and retains original
session retirement and native-control shutdown. Other threads and legacy
serve retain fatal hook behavior; owner/handler panic containment is not proven.
An actual-binary FSM_MCP_PANIC case now requires a normal failure exit instead
of abort, both bounded/final diagnostics and writer reopening with stdin open.
Stable/MSRV library, actual-binary, legacy lifecycle and all-target Clippy
verification is running; this newer source invalidates the older full-gate
candidate and no new successful result or task completion is claimed.

The owned stdio path now adds a shared atomic publication scope: ordinary
hosted tool calls hold it from submission through request-scope response
admission, elicitation holds it only after the answer before settlement,
and production native decision/shutdown passes hold it through journal return.
The feed checks both before and after loading a read-only prefix and skips
without changing its watermark while a scope is active. It holds no Store or
transport lock and retains no journal records or request-ID registry.
A new fixture calls the real hosted method boundary with its original owner,
observes the committed prefix while the response scope still exists, proves
feed/watermark deferral, releases the scope and verifies actual queued response
before instance/list notifications and the reopened journal. Its execution,
commit-boundary pause, native-pass/elicitation phase coverage and guard
neutralization proof remain pending; this is not complete 8904 acceptance.
The live panic-hook check began before this source addition, so its stages
must be attributed individually rather than treated as one frozen-source gate.

The native publication fixture now pauses after the original driver tick
returns, with the production no-op observer retaining the same scope. It
reads the actual durable deadline record while the pass is held, requires
feed deferral with unchanged watermark, releases the pass and requires one
actual instance notification plus a verified reopened journal. The hook only
pauses; it does not construct a replacement executor, result or journal record.
This adds deterministic post-commit native-phase coverage; execution and guard
neutralization still remain pending, as does fault injection before fsync.

The installed-hook verification terminated successfully (session 55183,
exit 0). Stable and MSRV each passed all six real-binary stdio cases,
including installed-hook adapter cleanup, and all 11 legacy lifecycle cases;
their CLI libraries each ran the pre-publication 124-case inventory.
All-target CLI Clippy passed on both, with the later MSRV stage compiling the
new publication source. The log `production-adapter-panic-hook-check.log`
has SHA-256 `b67a354fdc1c93764e6941aa671b3e167e71f7a17b0b34a647a8e2fcc0f076d6`.
Because source evolved between stages, this does not prove execution of the
new publication fixtures or one frozen complete gate. The full-gate wrapper
initially misclassified native_error test names using a substring check and
stopped before any stage; it now matches actual diagnostic line prefixes.
The exact clean frozen checkout is updated to `2416fe5`, and the full stable
gate is launched behind independent Cargo work (session 80272). Its result,
MSRV publication fixture execution and guard-neutralization remain pending;
no plan task or cross-plan dependency is released.

The frozen `2416fe5` stable gate terminated at workspace debug tests
(session 80272, exit 101): formatting and file-size checks passed, all 126
CLI library cases including both new publication fixtures passed, and the
only failing target was `execute_stop_cli` with two stderr lifecycle failures;
release, Clippy, documentation and later acceptance stages did not execute.
The broken-stderr case waited for impossible operator drainage after original
owner retirement; the stalled-stderr case exposed skipped initialization
warnings in the owned response-wait pump. The correction retains failed
delivery facts, restores bounded warnings once per incoming request and
observes initialization notifications during that wait. Focused stable/MSRV
verification is queued serially under 1 GiB and zero-swap limits (session
76883); no task or dependency is released, and the full gate requires a new
frozen source after the focused checks pass.

The corrected source passes all 13 stable `execute_stop_cli` cases, including
broken and stalled actual stderr with stdin open and physical writer release;
remaining focused stages and the new frozen full gate are still pending.

Focused session 76883 terminated with exit 0 on corrected source `a99ae93`: stable
production stop tests passed 13/13; stable and MSRV each passed CLI libraries
126/126, actual stdio 6/6, lifecycle 11/11 and all-target CLI Clippy.
The terminal `production-stderr-check.log` SHA-256 is `c0a58e284a444cbd0f0aa05bf4fa5536c6961c045d2db9054e3ed6b18ca80822`.
The new frozen full stable gate and MSRV production stop cases remain pending;
these results do not release any task or establish native platform acceptance.

The new frozen-gate wrapper terminated before its eight stable stages
(session 31118, exit 101): MSRV production stop tests passed 12/13, with
actual backpressure and cleanup succeeding but the required initial native
action diagnostic absent. The fixture could flood warnings before the native
owner initial pass, filling the bounded operator queue and rejecting that
diagnostic. It now waits for one actual original-owner tool response before
the flood while retaining the unread stderr pipe, so original-owner readiness
is established without sleeps or fabricated scheduler results. A first retry
failed compilation on a nonexistent JSON variant and executed no acceptance;
the ID comparison is corrected using the existing JSON parser. Corrected
serial stable/MSRV stop and all-target Clippy checks run in session 81218.
No full-gate or task-completion claim follows from the prior narrow results.

Corrected readiness session 81218 terminated successfully (exit 0): stable
and MSRV production stop tests each pass 13/13, including required native
diagnostic retention, and all-target CLI Clippy passes on both. Formatting
and file-size checks pass. The terminal log SHA-256 is `72ab7bc9f5a9b0fc0326870df438196f7eb8ad700fc1411fbbcb3963a35ce74a`.
The fixture preserves unread actual stderr, bounded stop, physical writer
reacquisition and both diagnostic assertions; it adds original-owner readiness
rather than relaxing acceptance. A new exact-source full gate is required.

The full stable gate is now frozen at exact clean source `3314aaa`
(session 63257, log `readiness-full-stable-frozen-gate.log`); the previous
MSRV stop prerequisite is omitted because its corrected-source inventory
already passed in session 81218. Session 63257 is confirmed live and waiting
for independently running Cargo PID 46799 before any stable gate stage,
without concurrent builds or added swap allowance. Its outcome is pending.

An additional original-owner response-wait fixture holds the admitted
command while actual Unix owned input carries pre-initialization requests,
`notifications/initialized` and a ping. It requires the original marker to
change, exactly three successfully drained warnings (ordinary method plus
512/513 multibyte method boundaries), no warning for the post-initialization
ping, writer ownership until retirement and unchanged verified creation
sequence after EOF. This exercises the real hosted method/input-wait seam
without releasing the original owner or fabricating a completion. Execution
and sensitivity are pending; focused stable/MSRV checks (session 17943) wait
for the original full-gate process 50136 to retire before starting Cargo.
The live frozen gate remains on `3314aaa`, so it does not cover this new test.

Frozen source `3314aaa` has now passed the complete stable workspace debug
gate, following formatting and source-size checks, including all 13 real
production stop cases and the two original publication fixtures. Session
63257 remains live and has started release workspace compilation; release,
workspace Clippy, documentation, zero-dependency and downstream embedding
stages are not yet proven. Kernel readback of the actual gate scope verifies
`memory.max=1073741824`, `memory.swap.max=0` and `memory.swap.current=0`;
the queued focused scope likewise uses zero swap. The new initialization
fixture at `c378ad0` and its three isolated sensitivity variants remain
queued behind the original gate/focused processes (sessions 17943/14081),
so this debug pass does not cover them or complete a task or plan.
Read-only installed-helper metadata currently reports mode 0711, owner
nobody, device 2306 and inode 94765497; this is not a protected Root helper
or evidence of native acceptance, and no helper mutation was attempted.

An additional elicitation publication fixture reaches the actual hosted
method with owned byte input and the original owner preparation/resume
commands. While the original question remains unanswered, an unrelated
actual owner-store mutation must publish through the feed. After the actual
answer, the original settlement is applied and its real response is queued
while the same request I/O scope is held; feed publication and watermark
advance must defer until that scope releases. It then requires response
before instance update, actual completed state, durable request identity and
verified reopened journal. Runtime and phase-specific guard sensitivity
remain pending; this adds no new public test API or fabricated completion.

Frozen `3314aaa` full stable session 63257 terminated with exit 0 and all
eight required stages passed: formatting, file size, complete workspace
debug/release, workspace all-target Clippy, warning-denying documentation,
zero dependencies and downstream embedding. Terminal log SHA-256 is
`ac13dea7d3bedae5173bbe814dfd76ec10c80b7ba400a2ed32e07d15578a7ca1`.
It predates the two newer test fixtures. Their first queued compilation
failed on an explicit move of borrowed I/O; the next failed on private Live
struct update, and a subsequent runtime fixture selected instance_send
instead of instance_elicit. The corrected fixture then exposed its own
wrong expectation of a list-changed notification for EventApplied. The
existing membership contract is now explicit in SPEC before correcting
that expectation, retaining response order, watermark, completed state,
request identity and verified prefix assertions. No production feed code
changed. Initialization sensitivity session 14081 stopped on its failed
prerequisite before any mutation, and phase session 45771 failed compilation.
Corrected session 56765 terminates with exit 0: stable/MSRV each pass all
128 CLI library tests and all-target CLI Clippy, including both new cases.
Its terminal log SHA-256 is `730e16d7c60908d059802ae2b3a87f4b22e531bbe70af4a2771e02b29483f36f`.
New-source full acceptance, initialization/publication sensitivity and
installed native acceptance remain open; no task or dependency is released.

Initialization sensitivity session 35634 terminated with exit 0 at frozen
`0dc9782`: individually neutralizing the original notification marker, wait
warning emission and 512-character method cap each makes the actual held-owner
input fixture fail; each mutation is restored and the clean baseline passes.
The aggregate terminal log SHA-256 is `7fda4cf7f3cd71db50041266c0d0d1cc199f21968e73da1e48aae0aaccfa981c`.
This proves those three guards are load-bearing at the real hosted method/input
seam, without claiming publication-phase sensitivity or native acceptance.

### Long-interval admitted observation candidate — 2026-10-07

The original NativeOwner now has a separate 50 ms admitted-work observation
deadline; ordinary scheduler decisions retain their configured interval and
eight-command allowance. Observation uses the original driver, one logical
clock sample and the same publication guard, without new effect admission,
retry or machine-deadline scheduling. After each ordinary decision the owner
publishes lifecycle inventory once, then suppresses repeated scans only when
that original inventory is complete, all helpers retired, no run IDs remain
and unclaimed reservations are exactly Some(0). Unknown inventory remains
observable, and independent stop checks retain their 50 ms bound.

The production-owner long-interval fixture advances logical time after the
first decision and requires one inventory observation, no further idle clock
samples, no deadline journal append, original writer release and successful
reopen/verification. Formatting, file-size and tracked diff checks pass;
compilation and runtime checks are queued after the existing frozen full gate
and publication sensitivity work. This fixture does not prove an installed
handler completion, completion-triggered wake, worker transport isolation or
eight-completion fairness. Tasks and dependencies remain unchanged.

### Retained readiness wakes the next writer decision — 2026-10-07

Review of the long-interval observation candidate found that admission-free
polling can deliver a prepared domain or make bound entry ready, while only
an ordinary owner decision may authorize its next transition. Waiting for
the configured timer after that observation can still strand the original
three-second preparation transport at a 24-hour interval. The original driver
now exposes a read-only retained readiness query, recorded explicitly in the
provisional public-surface inventory; after observation the host schedules
its next ordinary decision immediately when readiness exists. It still
services the mailbox between decisions and samples the logical clock once
per pass. Observation itself remains admission free.

No installed-helper acceptance is claimed: real prepared-domain, bound-entry
and completion-triggered fixtures, eight-completion fairness and worker
isolation remain obligations. The downstream empty-lifecycle fixture checks
that readiness supplies neither work nor closure on empty live/stopped drivers;
focused compilation and public-surface verification are pending.

### Frozen initialization/publication gate and sensitivity terminal evidence

Original session 58152 exited 0 at exact clean frozen `0dc9782`, with all eight
stable stages passing: fmt, size, workspace debug and release, workspace
all-target Clippy, warnings-denied Rustdoc, zero-dependency and downstream
embedding tests. Terminal log `initialization-publication-full-stable-frozen-gate.log`
SHA-256: `2a77d1a99733094e88fc7449cec889c743dc1d7bc61dd7551e8ef8c83f31c962`.

Original sensitivity session 53595 exited 0: neutralizing ordinary SessionIo
publication, native decision publication, post-answer elicitation timing and
the two feed checks as one guard produces the exact expected runtime failures;
the original source is restored clean and all three publication cases pass.
Terminal log `publication-phase-sensitivity-check.log` SHA-256:
`8bfe43036516da5d685eb49ad0603bf62b9f7048c5bd788ee27091032459af1d`. The source remains exact
`0dc9782`; no claims are made for individual post-load feed-race, pre-fsync or
shutdown-phase guards. These gates do not cover later `753a579` observation or
`82c0fc9` readiness scheduling changes, which retain their own focused review
and pending real-handler acceptance obligations.

The retained-readiness unit candidate also adopts caller-owned handoff
metadata into the original runner, observes readiness with the store path
temporarily unavailable, then restores the original directory before an
owner decision. Missing original durable acknowledgement must return
exec/inflight_deferred, park readiness, preserve every journal record and
permit an unchanged reopen. This is a refusal/query fixture, not native
closure, successful handoff delivery or installed-handler acceptance; its
compilation/runtime check remains pending.

### Observation terminal checks and readiness Clippy discrepancy — 2026-10-07

Original observation session 83548 exited 0 at clean frozen `753a579`:
stable and MSRV CLI library 129/129, production stdio 6/6, protocol lifecycle
11/11 and CLI all-target Clippy pass, with formatting and file-size checks.
Terminal `admitted-observation-check.log` SHA-256:
`e6cf1b7420599f7557b28b5e0a411950dcddad93c6dc461d54c64fc85ab125f3`.

Corrected readiness session 99838 exited 101 at frozen `82c0fc9`: stable
owned lifecycle 3/3, public surface 17/17, CLI library 129/129, stdio 6/6 and
protocol lifecycle 11/11 pass, then all-target Clippy reports E0599 for the
readiness method that those runtime targets just compiled. No MSRV or guard
sensitivity was reached. Session 34499 likewise exited 101 at frozen `1c529f6`:
stable retained-readiness unit 1/1, owned lifecycle 3/3 and public surface
17/17 pass, then Clippy reports the same missing-method error. Neither failed
run is recorded as a passing source gate; terminal log SHA-256 values are
`21d88ee4e302edb247d58d128e637018c75f179e7f09cc2a347758050b96de0e` and
`196420758cc830042a46abdd69c4141016e6e4c9fc397309c6f2105b596d5e6a` respectively.

The method exists in both exact clean frozen sources, and direct runtime
targets compile and invoke it; shared build-artifact reuse is suspected, not
established. Original session 72884 now verifies unchanged `1c529f6` serially
in a dedicated readiness-isolated-check-target, retaining actual 1GiB/zero-swap
limits and host memory thresholds, before any conclusion about Clippy or
sensitivity. Existing failed logs are preserved and tasks remain open.
