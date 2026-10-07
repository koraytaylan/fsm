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
