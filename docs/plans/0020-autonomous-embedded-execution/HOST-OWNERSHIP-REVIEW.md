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
