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
