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
