# Original transport polling worker review

OwnedNativeExecutor exposes explicit worker polling and NativeOwner selects
it after validating finite bounds. Scoped pool selection restores the previous
thread-local context; standalone defaults remain synchronous. New requests
reserve before helper construction; transferred running requests reserve before
adoption and carry the original child, streams, captured prefix, write offset
and absolute deadline into their worker without restarting the helper.
Each of 128 pool slots charges 16 MiB until both original owner handle and
worker retire; parsed response storage preflights at 2 MiB, using Value storage,
String/array capacities and conservative object-entry allocation.

Only finished workers are joined, cached reap is withheld until join, and
response collection cannot race an unfinished worker. Worker cancellation is
an atomic request; actual helper kill/reap/socket observation remains on that
worker. Failed thread creation retains the original inline request for later
observation rather than dropping its helper in the failed spawn closure.
An unfinished detached worker retains its ticket and grants no retirement.

Four fixtures use an actual barrier-held child and actual Unix streams to
check polling/cancellation before release, actual reap/EOF with a held unjoined
worker, exact 2 MiB parsed storage and one byte over through public polling,
and public-constructor refusal at slot 129 before helper startup. They do not
provide native domain closure or installed process/MCP acceptance. Formatting, file-size and tracked diff checks pass; focused runtime and
Clippy checks and four guard sensitivity cases also pass as recorded below;
thread-failure and panic phases remain unexecuted.

Helper spawn and protected/receipt metadata I/O still execute on the owner;
completion candidates need durable-storage accounting and actual supervised
handler tests before task completion. Task 8902 is now in progress for this
partial integrated boundary; task 8901's prerequisite remains unreleased and
no task or plan completion follows from these implementation changes.

## Frozen verification milestone

Exact source `80f1e5af6810d292f0182d7189b1d138d9afd5b9` was verified in
an isolated detached worktree with a dedicated target directory, one Cargo
job, one test thread, kernel MemoryMax=1 GiB and MemorySwapMax=0; every
stage checked host memory thresholds and waited for unrelated Cargo work.

- Session 8941 returned terminal 0: formatting, file size and all four worker
  tests passed.
- Session 19572 returned terminal 0: stable and MSRV 1.89.0 each passed
  executor unit tests (58), owned lifecycle (3), public surface (17), CLI
  unit tests (129), autonomous stdio (6), MCP lifecycle (11), and executor
  plus CLI all-target Clippy with warnings denied.
- Session 55625 returned terminal 0: independently neutralizing parsed-storage
  refusal, the coupled slot/transport capacity refusal, response join gating
  and retirement join gating each produced the expected single test failure,
  without compilation failure; restoring source passed all four worker tests
  and the worktree was clean at the original source identity.

The earlier complete stable gate at `1c529f6` does not cover this candidate;
no new full-workspace gate or native macOS/Windows execution is claimed.
The next implementation milestone is removing owner-side startup and receipt
verification from the same stdio path, followed by actual held process/MCP
responsiveness and durable settlement acceptance; broader final gates remain
required before task completion or final review.

Terminal logs remain under the dedicated task cache with SHA-256:

- `native-worker-check.log`: `66b9254bdbcf3db12848fc6b20bcff32ef803725368f60e0070c7b80316d8b8a`

- `native-worker-integrated-check.log`: `4632bd78ffb97a5db8e445ef1524292ef9c07e385bc6ba555588f71567554507`

- `native-worker-sensitivity-check.log`: `19547b55d5f97c80e39c0ac52065e011d0b52b4b441e343c9a5da5cc34cde2d7`

## Worker startup candidate

New owned requests now dispatch immutable startup material after reservation;
protected-helper validation, sockets and spawn occur on that worker under the
original absolute deadline. Cancellation during held startup is delivered to
the actual helper returned by that original startup. Standalone startup remains
synchronous. Joined startup refusal reports `not_started` with all actual
reap/EOF bits false; `is_retired` retires only that empty transport. Admission,
recovery and shutdown inventories use this predicate without granting claim
release or native closure. The provisional field changes downstream struct
literals and is inventoried with its public method.

A private per-caller factory exercises public construction with held startup
and an actual child; fixtures also cover empty startup refusal, unjoined
startup refusal, expiry before startup, synchronous standalone refusal and
retained original execution without a completion. They do not replace installed
Root-authority process/MCP acceptance. Candidate runtime, Clippy and new guard
sensitivity are pending; receipt verification, route discovery, completion
storage accounting and worker failure/panic acceptance remain open.

## Original completion and shutdown proof readers — candidate

NativeRun completion/recovery and NativeShutdown receipt verification now
select independent proof workers after actual raw response collection when
owned polling is enabled. They reuse the original transport reservation and
absolute deadline, capture immutable response/claim/hash/route material and
hold no Store or journal allocator. Original current ownership, stopped/ack
ordering and physical-store checks at writer settlement remain on the owner.
Proof results are delivered once after actual worker join; helper retirement
inventory is withheld while that proof worker remains unjoined. Cancellation
or expiry before/after the read refuses delivery and retains uncertainty.
Standalone observation remains synchronous.

Actual-child fixtures cover held completion and recovery readers, cancellation,
refusal without generation or slot replacement, publication before join, and
shutdown polling/retirement. Shutdown's directly retained fixture bypasses
startup routing and reads only an absent authority/receipt route; it proves no
installed shutdown or native closure. Unit-result reader cases additionally
pin pre-read expiry, post-read expiry and post-entry cancellation; unit values
provide no opaque proof. Shared private child fixtures expose no public API.
Runtime and Clippy checks for this candidate remain pending. Actual Root-issued
completion success, real process/MCP host responsiveness, full completion-storage
accounting, metadata route discovery and worker failure/panic phases remain open.

Worker-startup predecessor `0705bcb7f8e7eb0c94606446d05550eb6fe902a7` passed
formatting/file size, executor unit tests (64), owned lifecycle (3), public
surface (17), and executor plus CLI all-target Clippy on stable/MSRV in terminal
session 99171; that source does not cover this later proof-reader candidate.

## Verified runtime checkpoint and continuing job

Exact frozen `635e933e9892d310f1d0ce1b9a5d1f724301bde5` passes stable
executor unit tests (71), owned lifecycle (3), public surface (17), CLI unit
tests (129), autonomous stdio (6) and MCP lifecycle (11), plus formatting and
file-size checks. Original session 96881 remains live, waiting for unrelated
Cargo work before Clippy and the MSRV stages; the job is not terminal and
neither those stages nor the prepared twelve guard-neutralization cases are
claimed passed. Continue that original session and source rather than starting
a replacement on an observation timeout. Its dedicated cache script/log are
`native-proof-check.sh` and `native-proof-check.log`; sensitivity is prepared in
`native-proof-sensitivity-check.sh` and must run after the original job retires.
The mutable proof log has no terminal hash yet. Both verification scopes use
actual kernel MemoryMax=1 GiB/MemorySwapMax=0 with serial Cargo/test workers.

The terminal predecessor startup log `native-worker-startup-check.log`
(source `0705bcb`, session 99171, exit 0) has SHA-256:
88a9e5a123e281bf43abff081f9fd0b5f3db3a1e5ac1eeda6dd19470447e0326.

## Internally scoped native worker unwind — implementation candidate

The CLI process hook previously aborted before a native worker join could
report failure. A new provisional filter wraps an embedding hook without
global installation and suppresses that hook only inside private thread-local
transport/proof worker entry scopes. Installed Linux stdio composes this exact
filter with its existing adapter/fatal hook. Thread names, environment values,
sibling threads and callers cannot arm the permission. Other panics still
reach their original hook. Ordinary std-thread join errors override published
responses/proofs with bounded uncertainty and retain original ownership;
previously published actual helper observations remain usable only after join.

Fatal-hook subprocess cases use the exact public filter used by installed
stdio: real original startup/proof worker panics must return bounded uncertainty
without panic-payload disclosure, while a forged worker name must abort through
the original hook. Core dumps are disabled before re-exec and no artifact path
is selected. A separate actual-child case panics after real reap/EOF and checks
that its published response is discarded. These are worker/filter acceptance,
not installed Root-handler panic or native-domain closure acceptance; candidate
compilation, runtime, Clippy and new guard sensitivity remain pending.

### Additional unwind phases

Fatal-hook re-exec now also covers the transferred original helper after actual
reap/EOF and a proof panic after its result has been published; each must retain
bounded failure rather than deliver the prior response/proof. These new cases
await runtime verification. Predecessor `7165ef9` passed stable/MSRV executor
unit (76), lifecycle (3), public surface (17), CLI unit (129), autonomous stdio
(6), MCP lifecycle (11) and both-crate all-target Clippy in terminal session
87416; it does not cover these later test additions.

## Terminal checkpoints and working-path priority

The previously continuing proof-reader job is now terminal: frozen `635e933`
passed all listed runtime suites and both-crate all-target Clippy on stable
and MSRV, with marker `NATIVE_PROOF_INTEGRATED_PASSED`; its twelve isolated
guard neutralizations each failed the intended fixture, and restoring that
source passed all 49 native tests with marker
`NATIVE_PROOF_GUARD_SENSITIVITY_PASSED`.
The respective terminal log SHA-256 values are
`aa126fee1595dbb648356f75891bb278cf7eca7b382cd011245f2f33e6d004ad`
and `b836864cb16fb912eaa1614a00e53f91c97dd143bceaa8457143409e68349fc9`.

The integrated unwind checkpoint at `7165ef9` is terminal with marker
`NATIVE_UNWIND_INTEGRATED_PASSED` and log SHA-256
`02fad5a8aec4d0f33f8c18ff9b1f0162728751dd1b9de6cc822342a84acabc05`.
Frozen `7a293e7ffa5f9fe248b169542ad29abd9b7d1708` then passed formatting,
file-size checks, all six unwind-filter fixtures, the actual-child
post-retirement panic fixture, and both-crate all-target Clippy on stable
and MSRV, ending with `NATIVE_UNWIND_ADDITIONAL_PHASES_PASSED` and a clean
source worktree; its terminal log SHA-256 is
`7cfa5d259c4c34ab854fc1539be28bf5f59ccdd4106c37adff283f0a8ca640bd`.
These additional checks cover test additions to unchanged production code;
unwind-specific guard sensitivity remains pending.

Following the user's working-path direction, the next implementation milestone
is one complete Linux production stdio execution path, including result
settlement and bounded retention, before expanding HTTP or other plan surfaces.
Use focused checks while developing that path and run the full stable host gate
at the integrated milestone before final review, together with the required
guard sensitivity and platform evidence; earlier checkpoints do not replace
those gates or installed Root-handler acceptance.
All recorded build scopes enforce 1 GiB memory and zero swap with serial
workers, using the dedicated task cache rather than `/tmp`.
Plan 20 remains 0/7 and tasks 8901/8902 remain in progress.

### Load-bearing unwind guards and completion-entry capacity

Frozen `7a293e7` now passes all seven isolated guard-sensitivity cases:
restoring fatal-hook invocation, suppressing unmarked hooks, removing each
startup/transferred/proof entry scope separately, and disabling transport/proof
panic-result replacement each causes its intended single fixture to fail.
Each mutation was restored, after which all 56 native tests passed; original
session 49181 exited 0 with `NATIVE_UNWIND_GUARD_SENSITIVITY_PASSED`.
The terminal log SHA-256 is
`37491371e19c0894c0be75da00a90611e04b6a0be57cc1e532abfde20eaf72ec`.

The next implementation adds the existing retained-response storage preflight
to public NativeCompletion::verify before serialization, cloning and receipt
access, closing the caller-built allocation-capacity bypass of transport
checking; a limit/one-byte-over fixture uses identical encoded content.
Runtime and sensitivity checks for this addition are pending, and accounting
for every derived completion representation remains open.

Frozen `191078b2d96c88b75a65cc7153ec4a81a7298e5c` subsequently passed
formatting/file-size checks, all 79 executor unit tests and executor/CLI
all-target Clippy on stable and MSRV; neutralizing only the completion-entry
charge produced the intended single boundary-fixture failure, and restoring
the source passed that fixture with a clean worktree.
Original session 93273 exited 0 with `NATIVE_COMPLETION_CAPACITY_PASSED`.
Terminal check/sensitivity log SHA-256 values are respectively
`5909de9bb3919b8135b58fa4a438292347b1835289fce96759e6ed4eec9b0fa1`
and `519bbabac7a715ac2c3907f4662e264ad2c4360c05781f6172a2623a894c0553`.
These focused checks do not constitute the full stable host gate, installed
native acceptance, or complete derived-storage accounting; those remain
required at the integrated working-path milestone.

### Sequential attempt reservation reuse — candidate

Inspection found that execution startup reserved a second slot while the
completed binding request still owned the first, refusing bound attempts at
the 128-slot ceiling; sequential execution now reuses the original ticket
after actual predecessor retirement, retaining identity and the run deadline.
The full-pool fixture fills 127 other slots without spawning helpers and
reaches the original NativeRun bound-entry path using actual binding child
retirement and a refused execution-start fixture, supplying no Root closure.
A held-child fixture refuses successor startup before actual retirement.
Runtime and isolated guard-sensitivity verification remain pending; tasks
8901/8902 remain in progress and derived-completion accounting is still open.
