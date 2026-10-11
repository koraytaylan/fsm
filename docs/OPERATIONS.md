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

## Native candidate axes

Dispatch `operational-acceptance` with `scope=smoke` and an exact committed
candidate SHA to run six native installed baselines (Linux, macOS and Windows,
each on stable and Rust 1.89.0) plus the complete Linux executor inventory at
both toolchains; the workflow serializes these eight jobs and verifies their
original artifacts together. Each baseline preserves all fifteen original
scenarios; macOS and Windows prove unsupported containment before handler entry.
The two executor jobs execute all 37 scenarios, including actual native signals,
hard kills, hardware cuts, descendant closure and original-claim recovery.
The existing six-leg Rust gate retains every required step and adds installed
baseline proof after those steps; this opt-in smoke workflow does not run or
replace that expensive gate. Podman's consumer installation remains a separately
invoked `installed-consumer-check` run for the same candidate.
The consumer image explicitly sets `RUSTUP_TOOLCHAIN` from its `RUST_VERSION`
build argument: the pinned toolchain file remains a receipt input without
silently changing the compiler requested for that consumer installation.

`acceptance/run-native.py` checks a clean immutable checkout, explicitly selected
toolchain, native OS and the original consumer build receipt; Linux installation
and workloads execute under observed 1 GiB memory and zero-swap cgroup limits
with one Cargo worker. Original evidence is copied to
`$TMPDIR/native-check/evidence`, including failure diagnostics and native fixture
retirement. The `native.json` receipt binds the original file inventory and
digests; it is controlled-run evidence, not an independent attestation.
Verification requires a genuinely successful CI job as well as the matching
candidate, executable, source receipt, complete inventory and cleanup:

```sh
python3 acceptance/run-native.py --candidate "$candidate" \
  --bundle "$original_axis" --job-conclusion "$actual_ci_conclusion"
```

Cancelled, unfinished or failed jobs, calibration-only observations, missing
artifacts, unknown axes, duplicate/missing matrix legs and changed source or
binary identities cannot establish a native pass; an eight-axis smoke verdict
explicitly leaves sustained operation and candidate completion outstanding.

## Provisioning the sustained runner

[GitHub's documented hosted-job limit](https://docs.github.com/en/actions/reference/limits)
is six hours, below the required eight-hour duration; self-hosted jobs permit
longer execution. Provision a fresh, dedicated Ubuntu 24.04 x86-64 VM with
systemd/cgroup v2, Python 3, Git, Rustup, GDB, binutils, a non-root runner account
and passwordless administrative access for the task's containment provisioning.
Use at least 4 GiB RAM and 30 GiB free disk, no competing build, one ephemeral
repository runner registered with `--ephemeral` and the
`fsm-operational-disposable` label, and an external VM termination deadline
of twelve hours; dispose of the VM after its one job even on cancellation or
uncertain native cleanup. Do not label an ordinary development machine as this
disposable runner. Provisioning must complete before setting repository variable
`FSM_OPERATIONAL_RUNNER_READY=true`; without that explicit prerequisite, the
sustained workflow fails before queuing its expensive job.

Dispatch `scope=sustained`, a named `host`, exact candidate SHA and seed first
with `mode=calibrate`; retain the original observations, commit numeric budgets
for that host and sustained workload, then dispatch a separate `mode=validate`
run. A missing or stale sustained calibration refuses before compilation or
workload dispatch. Each sustained job first reruns the complete installed native
signal/hard-kill inventory for that same candidate, then performs actual mixed
work until both eight hours and 10,000 completed cycles hold; count alone,
elapsed time alone and idle padding cannot pass. The duration producer is bounded
to 10 hours 15 minutes including installation, and the complete job to 11 hours.
The maximum duration/count and progress watchdog still classify insufficient
throughput as incomplete; runner capacity does not waive either floor.

There is no schedule, cloud provisioning or credential installation in this
workflow; the operator chooses the VM provider and authorizes its quoted cost
before provisioning, budgeting up to twelve VM-hours per dispatched calibration
or validation plus disk and artifact storage. Artifacts upload on failures and
successes; a cancelled job or unsuccessful upload remains incomplete even if
its producer had written a passing report. Final candidate acceptance also
requires all other plan evidence, including human sessions and independent review.

## Deployment, shutdown and recovery runbook

This procedure targets the supported owned Linux/systemd runtime on one
physical store with one writer; macOS/Windows provide observation and protocol
coverage while refusing unsupported contained execution.
The [operational review matrix](reviews/operational-readiness.md) identifies
executed historical evidence separately from the final candidate still to be
reviewed. Writing this runbook does not establish that its final-candidate
walkthrough has executed successfully.

### Install and provision

Select one immutable code revision and retain its controlled-build receipt,
CLI and helper SHA-256 digests, actual Rust/native OS identities, operator UID,
handler-table digest and physical data-directory identity before starting work.
Use a non-root execution account, a private task cache, bounded diagnostics
and an explicitly provisioned Linux/systemd host with cgroup v2.
Inspect memory/swap before compilation; compile with one Cargo worker and the
same finite memory/zero-swap limits used for candidate evidence.
The disposable acceptance installation is provided by the native workflows
above; it refuses an existing helper and remains separate from production
provisioning. Never enable its disposable-CI guard on a development machine.

For a production installation, the administrator provides the candidate's
`fsm-containment-authority` binary at `/usr/libexec/fsm-containment-authority`,
Root-owned mode 0711 beneath protected nonsymlink parents, and the protected
`/var/lib/fsm-containment` base. Handler programs/configuration must also satisfy
the native protection policy; preserve the approved bytes rather than allowing
the execution account or model to change them.
The privileged binary is the `fsm-execute` package's separately built
`fsm-containment-authority` target, not a file the model installs or edits.
Retain installation ownership, device/inode and digest evidence; an unknown or
writable existing installation requires refusal of execution and investigation
before replacement. The pre-entry gate requires `fs.suid_dumpable` to be 0 or 2.

As the non-root execution account, select the retained installed `fsm_binary`,
approved `machine_file` and a fresh `data_dir`, then initialize that store once:

```sh
"$fsm_binary" --data-dir "$data_dir" machine add "$machine_file"
```

Register its physical path only after initialization; registration verifies
the store without creating or repairing it.
The administrator chooses a fresh 32-character lowercase hexadecimal namespace,
a canonical positive generation and the non-root operator UID, then uses the
existing authority operations in this order:

```sh
# Administrator operations on the selected provisioned host, with identified inputs.
authority=/usr/libexec/fsm-containment-authority
"$authority" register "$namespace" "$generation" "$data_dir"
"$authority" catalogue "$namespace" "$generation" "$protected_handlers"
"$authority" provision-broker "$namespace" "$generation" "$operator_uid"
```

Keep `serve "$namespace" "$generation"` supervised as Root with umask 0077;
the protected route and socket authorize only the chosen operator UID.
The catalogue source has protected parents and contains the approved handler
table; catalogue publication is immutable after allocation begins.
Retain the actual broker route/configuration/epoch and physical store binding.
Replacing a directory, helper, table or broker configuration is not ordinary
restart and cannot repair an unresolved original claim.

### Check and complete one workflow

For the isolated operational walkthrough, the operator uses
`acceptance/fixtures/executor_workflow.json` and the independently provisioned
success table for its synthetic `supplier` resource; the installed scenario
producers generate and approve that table through
`acceptance.suite.executor_scenarios.workflow_table`.
Its two-item resource starts unsuspended and empty.
This operator walkthrough supplies no model-authoring evidence; scored live
sessions use separate fresh stores and receive their frozen briefs and public
contracts, with no answer definition or private fixture code supplied to the model.

Keep the initialized machine/store and set `handlers_file` to the retained
candidate's actually approved table, then create a private mode-0700
`control_root` owned by its execution account before starting the owner.
Use the identical handler-table bytes for preflight, protected catalogue and
execution. As that non-root account:

```sh
"$fsm_binary" --json --data-dir "$data_dir" execute --check \
  --handlers "$handlers_file" --machine-file "$machine_file"
"$fsm_binary" --json --data-dir "$data_dir" instance new acceptance_workflow \
  --request-id operational-clean-create
# Read instance_id from that original response; keep it as instance_identifier.
"$fsm_binary" --json --data-dir "$data_dir" instance send "$instance_identifier" start \
  --request-id operational-clean-start
"$fsm_binary" --json --data-dir "$data_dir" execute \
  --handlers "$handlers_file" --control-dir "$control_root"
```

Run the executor in its supervised owner terminal/process and retain stdout
and stderr independently without letting a blocked reader retain ownership.
From another terminal, observe the instance and the independent synthetic
resource without supplying outcome events, acknowledgements or progress polls.
The expected journal/event path is `start → validated → suspended → processed
→ restored`, ending at `completed`; original external observations must show
ordered `suspend`, `process:0`, `process:1`, `restore`, with no overlap, exactly
two items and an unsuspended resource.
A terminal instance alone cannot prove that its external resource was restored.

For embedded deployments, use `serve --execute --handlers "$handlers_file"`
through an actual MCP stdio client, or add `--http 127.0.0.1:<selected-port>` for
the owned HTTP host; discover `fsm://executor` and require its actual embedded
mode, `executes_effects` and autonomous progress before authoring automated work.
Those hosts use the execution account's default `$HOME/.cache/fsm/control`
root; HTTP owner lifetime is server-scoped, and deleting a session leaves the
shared owner running. Stdio requires stdin to remain open; EOF starts retirement.
Read-only, contended or degraded observation cannot prove available automation.

### Drain, abort and diagnose uncertainty

Use the original owner's actual control root and physical data directory:

```sh
"$fsm_binary" --json --data-dir "$data_dir" execute stop \
  --control-dir "$control_root" --mode drain --timeout-ms 10000
```

Retain the original report and owner exit; require `phase: stopped`, true
`admission_closed`, `inventory_complete`, `helpers_retired` and `writer_released`,
false `timed_out`, no `unresolved_run_ids` and zero `unclaimed_reservations`.
Observe original process/domain retirement, endpoint retirement and the owner's
separate protocol/diagnostic output facts; successful native cleanup does not
turn failed output drainage into success.
`drain` stops admission and observes already admitted work; a pending later
workflow action or compensation requires a safely admitted successor.
Use the same command with `--mode abort` when admitted work must be revoked;
abort may escalate an earlier drain, but neither a new caller nor a repeated
request renews the owner's first absolute shutdown deadline.

An `exec/inflight_deferred` error, transport timeout, missing endpoint or
uncertain report leaves closure/writer availability unproved.
Retain the journal, original ownership, protected records and external state;
do not clear claims, delete authority files, acknowledge from absence, start a
competing owner or signal arbitrary PIDs to manufacture a successful report.
Use `execute runs` and original diagnostics to identify the retained work.

### Recover a deliberate interruption

Perform the fault walkthrough only on the isolated disposable acceptance host
and resource, using a separate initialized store, new instance and fresh approved
namespace. Provision its held-validation table before allocation and preserve
those exact approved bytes throughout the original run and successor.
Hold its actual validation handler at the fixture's external
barrier, retain the original `execution_claimed` record, claim hash, domain,
run ID, handler contract, owner PID/birth and live handler/descendant identities,
then deliberately terminate that exact captured owner.
The installed lifecycle scenarios exercise SIGINT, SIGTERM and hard-kill cuts;
signals are crash mechanisms and supply no graceful shutdown or closure proof.
Preserve the interrupted trace's original start without inventing a finish or
result. The full hardware inventory additionally preserves acknowledged success
and accepted events at their original instruction/descriptor boundaries.

Inspect the same physical store read-only:

```sh
"$fsm_binary" --json --data-dir "$data_dir" execute runs
"$fsm_binary" --json --data-dir "$data_dir" journal verify
```

The run inventory explicitly labels native evidence `unverified`; a stopped
journal record, absent PID or refused socket alone cannot authorize replacement.
After the original owner's death is independently observed, retain its exact
run ID from that inventory and reconcile through the original protected authority:

```sh
"$fsm_binary" --json --data-dir "$data_dir" execute reconcile \
  --run-id "$original_run_id" --timeout-ms 10000
```

Reconciliation requires a healthy original writer and matched protected closure,
original claim/hash/domain and retained result; uncertainty refuses settlement
and retains ownership. A known original success or acknowledged handoff must
recover without repeating its handler or acknowledging again.
An interrupted attempt may remain pending for a fresh owner after confirmed
original retirement; retain absent results as absent, rather than assigning
success or a fabricated failure event.
Restart the same supported owner with the identical approved configuration only
after safe original-run retirement permits admission, release the successor's
own fixture barrier and require sequential, nonoverlapping work to restore the
resource and reach the expected terminal state.
Retain original closure before replacement entry and actual original-process
death; a reused PID is not the original process.

Finish with a confirmed owner drain and independent external observations, then:

```sh
"$fsm_binary" --json --data-dir "$data_dir" instance show "$instance_identifier"
"$fsm_binary" --json --data-dir "$data_dir" execute runs
"$fsm_binary" --json --data-dir "$data_dir" journal verify
"$fsm_binary" --json --data-dir "$data_dir" journal replay
```

For a sealed store, provide its retained original archive chain to verification
with the documented `--with-archive` option; a partial-prefix result is not full
verification. Preserve request IDs, event/settlement ledgers, original closures,
resource history and cleanup before removing only matched task-owned artifacts.
Uncertain cleanup retains its authority and diagnostics; disposable VM retirement
does not convert that incomplete run into a pass.

External effects remain at least once and require domain-specific idempotency
or reconciliation; local containment cannot roll back mutations or cancel work
already delegated to a remote service. Recovery uses original claims and
contracts, not the current handler table as replacement authority.
The independent reviewer must record the actually executed clean-setup and
interrupted-recovery artifacts for the final candidate before this runbook can
establish task-9703 completion.
