# Installed operational acceptance

Run the opt-in `operational-smoke` GitHub workflow on an immutable develop
commit, first in `calibrate` mode and then in `validate` mode after committing
the measured host budgets in `acceptance/profiles/operational.json`.
Its inputs are the candidate SHA, mode and seed; seed 123 is the default.
The runner identity is `github-ubuntu-24.04-x86_64-stable`.
Calibration observations alone do not establish a candidate resource pass.

The committed smoke/seed-123 calibration comes from
[100 native cycles on candidate f0b68c61](https://github.com/koraytaylan/fsm/actions/runs/38089163514),
covering all 72 case/handler/transport cells in 1,444.95 seconds with 362
original domain closures, nine verified/replayed blocks and successful cleanup.
The profile records the original report and implementing-author replay digests;
this author replay is distinct from the plan's independent final review.
Nonzero measured maxima have at least twice their observed allowance, rounded
up to binary memory/count or time buckets; work latency uses an 80-second
ceiling below the 90-second watchdog. Active capture allowance is twice the
authored 131,072-byte flood although every sampled queue was empty; quiescent
children, descendants and queues remain exactly zero, and capture growth is
zero. Equivalent-host RSS growth allows 16 MiB and descriptor growth allows two.
These budgets apply only to this named host, smoke profile and seed/workload
digest; sustained and other native hosts require their own prior calibration.

The entry point on a provisioned disposable Linux CI runner is:

```sh
python3 acceptance/run-operational.py --candidate "$candidate" \
  --mode calibrate --profile smoke --seed 123 \
  --host github-ubuntu-24.04-x86_64-stable
```

Use an explicit task-owned `TMPDIR` under the operator's cache and a separate
`CARGO_TARGET_DIR`; `/tmp` is refused, including indirect paths.
The entry point refuses a pre-existing containment installation and local
privileged execution; it snapshots committed inputs, installs with
`cargo install --locked`, verifies a source/build receipt, and provisions its
Root-owned helper on the disposable runner.
Builds use one worker and an observed cgroup limit of 1 GiB with zero swap;
the build has a ten-minute upper bound.
CI serializes operational runs and retains all failure artifacts.

The smoke floor is both two minutes of completed work and 100 completed
cycles; the sustained floor is both eight hours and 10,000 completed cycles.
The finite seeded schedule covers twelve cases through process and MCP
handlers on standalone, stdio and HTTP hosts.
One host and store serve each twelve-case block, with explicit retirement,
archive/reopen and replacement within the block; subsequent blocks reuse the
same store and retain prior archives.
Eight shared handler definitions, including the four explicit fault variants,
fit the unchanged 8 KiB protected catalog envelope; per-resource barriers keep
the twelve cases independently observable.
The block's Root broker has a five-minute upper bound for its 43 authored
allocations; smaller existing fixtures keep their two-minute bound.
Client reads and control requests never repair expected autonomous progress.
Standalone mutations retry only a structured retryable `store/lock` response
for at most three seconds with the same request key, including errors emitted
on stderr; other failures and expired contention remain failures.
Manual acknowledgements are explicit operator actions and count as manual work.
No-progress, maximum duration/count, correctness failure, unavailable required
observation and incomplete cleanup produce an incomplete run with nonzero exit.

Each retained cycle binds its authored event/settlement ledger to physical
fixture entries, birth identities, monotonic entry/finish times, external
mutation order and final resource state.
Cancelled and expired work retain their absent physical result and finish;
original native closure and observed process death provide separate proof.
Cancellation leaves its original effect visible in the cancelled instance;
interrupted settlement consumes native ownership without acknowledging that
effect or inventing an outcome event.
The physical output flood is verified for both handler kinds: process results
retain the exact 4 KiB stderr prefix and full digest, while successful MCP
results retain their exact typed tool answer and omit server logs under the
existing result contract; both must reach zero queued capture bytes.
The installed verifier checks each host block and the latest archived prefix;
the installed replay must reproduce the store, and archive manifests must
continue their retained predecessor hashes.

Resource samples retain raw procfs and scoped kernel socket observations.
RSS, descriptors and CPU nanoseconds sum the live owner and original native
cgroup members; CPU is the observed live-process counter, not an estimate of
already retired process CPU.
Child counts include the owner's direct children and all original native
members; descendants count actual fixture-recorded birth identities.
Capture bytes are non-consuming pipe and Unix stream kernel queue observations,
deduplicated by identity; heap allocations are accounted in RSS separately.
Work latency spans dispatch through verified final state and required restart;
scheduler lag measures dispatch to physical entry, and control latency times
the actual out-of-band observation request.

Disk bytes include the retained report/store/archive directory and the active
native fixture's protected records, staged sources and external resource files;
journal bytes include retained archives separately from heap/descriptor growth.
Warmed and quiescent comparisons require the same physical host birth and
handler/transport state; a replacement has a fresh warmed baseline.
Children, descendants and queued capture bytes must be exactly zero at
quiescence even during calibration.
Numeric ceilings and growth tolerances are named-host and profile/seed/source
specific, with an original calibration evidence digest; missing or stale
calibration fails before workload dispatch in validation mode.

Evidence lives under `$TMPDIR/operational-check/evidence`, including the
identified installed binary/helper, build receipt and actual limits,
consumer log, cycle records, original closure records and retirement receipts.
The source receipt includes the root README, contributing guide, pinned
toolchain and CI/release workflows referenced by compile-time documentation
tests; credentials and Git metadata remain outside the staged inputs.
Quiet settlement has a bounded 60-second wait below the 90-second profile
watchdog, allowing the contending pair's eight serialized native operations;
this wait is separate from committed numeric latency budgets. Failures retain
the exact scheduled case, original journal prefix and physical resource rows
before owner retirement, so cleanup progress cannot replace the failed sample.
Successful fixture retirement removes only matched task-owned native paths;
the final helper removal checks its original device, inode and digest.
Uncertain cleanup retains its authority and diagnostics on the disposable VM.
The sustained profile needs a runner whose uninterrupted job allowance exceeds
eight hours; the hosted smoke workflow does not supply sustained evidence.
