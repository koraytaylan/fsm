# Installed operational acceptance

Run the opt-in `operational-smoke` GitHub workflow on an immutable develop
commit, first in `calibrate` mode and then in `validate` mode after committing
the measured host budgets in `acceptance/profiles/operational.json`.
Its inputs are the candidate SHA, mode and seed; seed 123 is the default.
The runner identity is `github-ubuntu-24.04-x86_64-stable`.
Calibration observations alone do not establish a candidate resource pass.

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
Successful fixture retirement removes only matched task-owned native paths;
the final helper removal checks its original device, inode and digest.
Uncertain cleanup retains its authority and diagnostics on the disposable VM.
The sustained profile needs a runner whose uninterrupted job allowance exceeds
eight hours; the hosted smoke workflow does not supply sustained evidence.
