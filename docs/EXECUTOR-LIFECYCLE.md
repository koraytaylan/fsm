# Executor lifecycle feasibility

## Decision status

Root-only `begin-close` now validates the actual prepared domain under the
authority lock and durably publishes its closing marker before removing
entry and pending grants and syncing the directory. Exact replay is allowed;
changed identities or unexpected grant ownership/types refuse. This remains
admission revocation, without manager fencing, termination or closure receipts,
and preserves durable ownership even after instance cancellation.

The protected `gate` entry verifier now accepts only a canonical native route,
requires an unprivileged identity, reads a root-owned immutable entry grant,
and checks authority/cgroup device and inode, boot and its own exact cgroup
membership before replacing itself with the granted absolute command.
It rejects caller-supplied argv and malformed grants and rechecks the grant
before exec. Exact routed cgroup membership precedes a bounded five-second
wait for grant publication, allowing the launcher to derive the enrolled
DynamicUser group before authorizing entry. Only an absent grant permits
waiting; closing/closed markers refuse during the wait and before exec.
The launcher must still publish that grant only after independent
current-claim verification, and closure must fence/kill the enrolled gate to
cover the final check-to-exec race. The root-only `authorize` operation now
publishes grants after matching the protected binding and revalidating current
ownership under the authority lock. It rejects closing/closed allocations and
group zero, and exclusively publishes a synced root-owned, group-readable
0440 file. The future broker must derive the trusted isolated group itself;
this privileged command does not establish broker authorization. Systemd
launch, bounded native I/O and closure integration remain unimplemented.

The in-development `fsm-containment-authority` binary adds root-only store
registration and immutable claim binding under the protected persistent
authority namespace. Registration verifies the store read-only and creates
a fresh generation exclusively. Binding independently checks the registered
store identity and durable claim, current runnable/pending ownership, boot,
authority/cgroup identities and a protected prepared record before writing a
private create-once, fsynced binding. Its input format is closed and bounded;
unsupported runtimes and unprivileged callers refuse before request I/O.
These commands perform no handler launch and publish no closure receipt;
the protected entry, broker and runner integration
remain unfinished, so this binary is not an accepted contained runtime.

The production `prepare` operation now creates an empty protected cgroup
after durably recording its allocation intent and counter advance, then
publishes its actual native identity as a prepared record. It verifies all
prior intents, rejects unresolved preparation, rollback, copied authority or
boot changes, and refuses unknown domains in its generation. Cgroup names
include namespace, generation and allocation. Each generation currently
allows 4096 lifetime allocations; directory scans are limited to 32768 entries.
It neither launches a gate/handler nor enables execution admission. Five
opt-in real cgroup cases cover empty preparation, unknown-domain refusal,
counter rollback, persisted incomplete intent and genuine durable claim
binding; the native matrix must execute them on writable provisioned Linux,
not count ignored portable cases or local read-only cgroups as native proof.

Task 9303 now shares a bounded accumulator and Linux nonblocking socket
capture between process output and MCP stderr in the existing runner: each
poll drains at most 64 KiB per stream, retains at most 4 KiB, hashes at most
1 MiB and creates no capture spool or reader thread. Final draining is bounded
and an open descendant-held peer leaves an incomplete capture without a
whole-stream digest. This transport does not authenticate a claim or prove
domain closure; the production native authority remains implementation work,
and existing direct-child results
must not be used as contained settlement evidence.

The Linux MCP protocol transport now owns independent socket cancellation
controls for both read and write directions and tracks the worker join handle.
Natural conversation results wait for an observed join; cancellation retains
unfinished workers and refuses further launches until they join. The retained
peer unit check covers actual blocked reads and large request writes without
closing either peer. `Drop` cancels best-effort and never waits on an unfinished
thread. This is worker cleanup infrastructure; production claim authentication,
domain closure and explicit uncertain-cleanup reporting remain required.

Plan 0022 task `lifecycle-containment-feasibility` is **complete** at reviewed
source `399ed6ed5636118151ffb7ad94140538f865d1a7`; this releases its native
prerequisite, not a production containment capability.
The user delegated the runtime/platform decision explicitly on 2026-10-04:
“i explicitly allow you to act on my behalf on this”. Acting under that
authority, the implementation decision is a provisioned Linux/systemd
containment backend for the initial contained executor. Safe Rust, zero
third-party crates and MSRV 1.89 remain binding. The new contained executor
will initially require this Linux runtime; macOS and Windows must explicitly
refuse that capability until separately proved backends exist. Existing
portable core/store/CLI Rust coverage remains required. This is an explicit
supported-runtime change, not evidence that existing macOS/Windows execution
already satisfies containment.

The selected trust boundary is the system systemd manager, protected cgroups
and a separately authenticated, privileged local supervisor. Handler code
runs under a different unprivileged identity without cgroup delegation,
privilege acquisition or namespace escape. The supervisor must persist
non-reusable domain identities and closure tombstones; unit names or PIDs
alone are insufficient. Installation, resource-access policy, failure
detection and unsupported-runtime refusal are implementation requirements.
The administrator and kernel are trusted; loss of supervisor authority or
identity evidence leaves claims unresolved. No arbitrary root command API
may be exposed to protocol clients.

Authorization and the passing native matrix are recorded; terminal CI run
`37245431479` passes both native jobs, all six portable gates and zero
dependencies on that exact source, with full local stable gates also passing.
The coordinator releases the native-proof gate for downstream implementation.
Historical checkpoints below retain their original unreleased verdicts rather
than relabeling incomplete evidence. The executor currently
provides direct-child cleanup, not durable process-tree containment. Plans
0020–0023 must not claim the stronger guarantee from the evidence below.

## Reproducible negative probe

`crates/fsm-execute/tests/lifecycle_platform.rs` re-executes its own native
test binary to create a root and descendant. File barriers synchronize
startup; a challenge written only after root termination requires a fresh
response from the descendant. The descendant inherits the root's stdout.
The parent checks that the pipe has not reached EOF after reaping the root.
Both normal root exit and `Child::kill` leave the descendant alive.

Cleanup uses a cooperative stop marker and joins the reader after EOF; a
ten-second fixture watchdog bounds orphan lifetime even on assertion failure.
That marker is test cleanup, **not** a proposed production containment proof.
There are no shell commands, third-party crates, unsafe blocks or OS exclusions
in the probe. Passing these negative tests proves a limitation, not the
positive native matrix required to release the gate.

Run with:

```sh
CARGO_TARGET_DIR=/tmp/fsm-plans-target cargo test -p fsm-execute --test lifecycle_platform
CARGO_TARGET_DIR=/tmp/fsm-plans-target cargo +stable test -p fsm-execute --test lifecycle_platform
```

Observed host: Linux x86_64, kernel `7.0.0-31-generic`.
MSRV: `rustc 1.89.0 (29483883e 2025-08-04)`; stable installed on this host:
`rustc 1.98.1 (48a229cea 2026-09-01)`.
MSRV negative probes passed (two behavioral tests and one fixture entry).
The same probes passed on installed stable 1.98.1.
Native macOS and Windows execution has not been performed here.

## API and facility inventory

The installed Rust 1.89 HTML documentation was inspected, rather than assuming
that current web documentation describes the pinned toolchain:

| Platform | Available surface | Missing proof |
| --- | --- | --- |
| Linux/macOS | Safe `CommandExt::process_group`; `Child::kill` and `wait`; `pre_exec` requires unsafe | A group is not a permanently closed, non-reusable containment identity; std supplies no durable domain supervisor or safe tree-closure API |
| Windows | `creation_flags`, `spawn_with_attributes`, `Child::kill` and `wait` | Creation flags alone do not establish Job Object ownership, admission, termination or restart inspection |
| Linux cgroup v2 | Kernel hierarchy, inherited membership, populated state and group termination facilities | Provisioned delegation, protected admission, durable identity and pre-execution enrollment still need implementation and proof |

The current host has cgroup v2 mounted, but `/sys/fs/cgroup` is not writable
by this session. `/proc/self/cgroup` identifies a login-session scope, not
a dedicated, authorized executor containment boundary. No system cgroups
were changed. Neither the presence of cgroup files nor permission to write
a PID after spawn proves atomic enrollment before external action.

Primary references:
[Unix CommandExt](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html),
[Windows CommandExt](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html),
and [kernel cgroup v2 documentation](https://docs.kernel.org/admin-guide/cgroup-v2.html).
The web Rust pages describe a newer toolchain; pinned API claims above come
from the installed 1.89 documentation. Backend feasibility remains unproven.

## Concrete decision options

1. **Keep the charter and all shipped platforms.** Require a provisioned,
   OS-managed containment service with an authenticated safe protocol.
   Approve the runtime installation and trust boundary separately. Its native
   proofs must include service/helper death and escaped descendants; a
   cooperative subprocess wrapper does not qualify. No suitable service has
   yet been demonstrated across all three OS families.
2. **Approve a narrowly scoped native backend exception.** Permit audited
   platform bindings/dependencies and, where necessary, unsafe code confined
   to the containment boundary. Retain core purity and Rust 1.89. Specify
   Linux protected cgroups, Windows Job Objects and a separately proved macOS
   mechanism. This authorizes investigation, not a claim that these proposals
   already meet the architecture. macOS remains a feasibility risk.
3. **Approve a supported-platform/runtime change.** Initially ship the new
   contained executor only in an explicitly provisioned Linux environment,
   keeping other executor platforms unsupported for this new guarantee.
   Document the compatibility and release consequences; do not silently
   replace native coverage with cross-compilation.

Every option still requires the full applicable native tests for atomic
enrollment, descendants, spawn during closure, retained pipes, supervisor
death, unrelated-process survival, identity reuse, signal notification and
uncertain-state refusal. The delegated decision above selects path 3 with
a provisioned OS-managed
backend, preserving the Rust charter. This note does not release the proof
gate for production ownership, launch or journal changes.

## Review record

Initial self-review: the probe intentionally demonstrates the existing gap;
it neither detects a supported production backend nor tests all acceptance
rows. Backend implementation and complete Linux positive proof remain
outstanding; native unsupported-capability refusal must also be tested on
Windows/macOS. The delegated
authorization above resolves the decision requirement, not the proof
requirement. Task completion and landing OIDs must remain unset until those requirements are met.

## First protected host experiment

The actual host, outside the command sandbox, has systemd 259.5 and a working
noninteractive administrative provisioner. Its systemd user manager delegates
cgroups to the caller; that same-user domain is not selected because handlers
could otherwise have cgroup migration authority. The command sandbox's
read-only cgroup mount does not establish the host manager's capability.

A uniquely named transient system service with DynamicUser, no delegation,
read-only cgroups, no new privileges and restricted namespaces ran a root
and descendant under UID 61659 in the same system cgroup. The root exited,
the descendant answered a fresh file challenge, and the unit remained active.
Stopping only that unit killed the descendant and produced pipe EOF. All
probe resources were temporary and cleaned up. This is partial positive
Linux feasibility evidence, not atomic authorization, immutable identity,
supervisor-death or full lifecycle acceptance. The first attempt failed
before execution because task-private /tmp was invisible to the host manager;
the successful probe used a separately provisioned, temporary /run directory.

The installed manuals and upstream definitions document
[service lifetime](https://github.com/systemd/systemd/blob/main/man/systemd.service.xml)
and [execution isolation](https://github.com/systemd/systemd/blob/main/man/systemd.exec.xml).
The selected production supervisor and tombstone protocol still need native
proof; systemd configuration resemblance does not complete task 9301.

## Reproducible protected Rust probes

The native fixture records its cgroup and UID at entry, before spawning or
external fixture work. The systemd probe driver installs a root-owned copy
of the compiled test binary into a uniquely named temporary /run directory;
only that directory is writable by its isolated handler identity. The driver
runs normal root exit, SIGKILL of the root, a descendant in a different
process group, and uncatchable launcher death. Each checks a fresh descendant
challenge, matching native cgroup membership, refused migration, survival of
an unrelated process, cgroup removal and independently bounded pipe EOF.
A killed service may be failed after closure; that status is neither success
nor proof of a live tree. Closure checks the actual domain and pipe evidence.

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/systemd_probe.py --toolchain 1.89.0 --report /tmp/fsm-systemd-native-msrv.json
python3 crates/fsm-execute/tests/lifecycle_platform/systemd_probe.py --toolchain stable --report /tmp/fsm-systemd-native-stable.json
```

Both toolchains passed these four cases on Linux 7.0.0-31-generic with systemd
259.5. The reports bind the native executable digest, source commit/dirty
flag, kernel, Rust and systemd versions, boot identity and observed unit
invocation identity. They explicitly declare partial feasibility and
`gate_released: false`; they are not consumer-installed candidate evidence.
Atomic journal-bound authorization, spawn during closure, permanent identity
and tombstones, privileged supervisor death/recovery, loss of authority,
signal notification and uncertain-state replacement refusal remain unproved.

The runtime restriction changes supported CLI execution behavior when the
new runner ships. Per [API-POLICY.md](API-POLICY.md), it requires a breaking
minor release from the current 0.3 line, an explicit runtime installation and
upgrade guide, and release notes; no version/tag is changed by these probes.
Historical hashes and journal bytes remain unchanged. The future claim-format
migration still needs its own version bump and backward-reader refusal.

## Frozen review and retained evidence

Reviewed range:
`69ad2aa3b12080407314c8be86bac741ac4cbc68..dd9726081b86e06b41d0c08295c06fac71b8d35f`.
The decision/probes landed in `3f397f5`; review repair `dd97260` enables
collection of failed transient probe units. Clean-checkout reports are
`/tmp/fsm-systemd-final-msrv.json` and `/tmp/fsm-systemd-final-stable.json`.
Both identify exact source `dd97260`, dirty false, four passing native cases
and gate unreleased. No task-owned probe units remain after either run.
Review separated failed service outcomes from empty-domain proof, checked
real process-group separation, bounded inherited-pipe waits, and rejected
optimized Python before any provisioning. These checks do not validate a
future privileged supervisor or its authorization protocol.

Formatting, all-targets Clippy, warning-free docs, zero-dependency, embedding
and full stable workspace debug/release gates pass on Linux. The existing
MSRV native negative probes and all 45 acceptance-harness self-tests pass.
macOS/Windows native execution remains unperformed. No candidate acceptance,
full lifecycle gate, task completion or integration is claimed.

## Shutdown race and native freeze probes

The probe inventory now has six positive cases. In `spawn-stop`, a real
systemd stop job first reaches deactivating while its initial SIGCONT leaves
the tree alive. Only then does an external barrier authorize two new native
fixture descendants. Both record the original cgroup before final killing;
closure still requires removal of that domain and bounded inherited-pipe EOF.
This tests containment during the stop window, not a production admission
fence or a claim that graceful deactivation immediately prevents forks.

In `frozen-stop`, the privileged probe writes the owned domain's cgroup.freeze
and waits for kernel events showing frozen=1 and populated=1. It then arms
the fixture fork, verifies no new leaf marker exists, and writes cgroup.kill
without thawing. Domain removal, absent leaf markers, pipe EOF and survival
of an unrelated process are required. This demonstrates the native primitive
needed for final closure; supervisor authorization and immutable identity
remain separate unimplemented proof obligations.

Every root and descendant also records all real/effective/saved/filesystem
UIDs, requiring one non-root identity distinct from the operator, zero
effective capabilities and NoNewPrivs=1. Configuration property names alone
do not satisfy these assertions.

`--neutralize-final-kill` runs a separately labelled negative native probe
with SendSIGKILL=no. Its stop times out with the tree still present, so the
observer returns nonzero and a failed report. Emergency test cleanup kills
only that uniquely created fixture domain; it cannot change the failed
observation into a pass. The report retains the failure-time unit state and
kernel domain events, and verifies unrelated-process survival through cleanup.
Neither this negative test nor the six positives releases the full gate.

### Frozen shutdown-race review

Range `3be7292..8b2dd037c0f43b2c4b983c44253bb09f75256649` is reviewed.
Review required late-descendant privilege checks, retained failure-time kernel
events before emergency cleanup, initialized freeze observations defensively,
and kept graceful deactivation distinct from the final admission fence.
Exact clean-checkout reports are `/tmp/fsm-systemd-race-clean-msrv.json`
and `/tmp/fsm-systemd-race-clean-stable.json` (six positives each), plus
`/tmp/fsm-systemd-race-clean-negative-msrv.json` and
`/tmp/fsm-systemd-race-clean-negative-stable.json` (one expected failure each).
All bind source `8b2dd03`, dirty false and gate unreleased. Disabled killing
leaves an observed populated domain; bounded cleanup succeeds and no
probe units remain. Formatting, all-targets Clippy, warning-free docs,
zero-dependency, embedding and full stable workspace debug/release gates
pass on Linux. Existing MSRV negative probes and all 45 Python harness tests
pass. These results still do not prove a production supervisor, immutable
identity, journal-bound authorization, graceful signal notification or
uncertain-state replacement refusal.

## Protected identity and controller-death prototype

`lifecycle_platform/identity_root.rs` supplies a private, root-only native
fixture, invoked by `identity_probe.py`. It is not a shipped privilege broker,
production execution path, journal claim or authenticated client protocol.
Its scope is one separately provisioned, protected, unique /run namespace.
Absent authority is refused rather than automatically initialized. Missing
registry data and unprivileged invocation cause no handler launch.

The prototype fsyncs a monotonic counter before creating an empty cgroup,
then records its boot identity and inode before returning a handle. systemd
adopts the existing empty domain without replacing that inode. Operations
require the complete counter/inode/boot tuple in the caller's handle and
compare it with protected authority; a counter or PID alone cannot select
another tree. Unknown native state prevents cleanup and successor allocation.
Closed records remain as tombstones, and a same-name new unit is classified
as an alias rather than killed using the old handle. Counter rollback cannot
overwrite a historical identity or move behind the active identity.

A separate privileged controller unit launches the fixture, then publishes
an explicit barrier after its systemd utility has returned. The test waits
for real handler readiness before killing the controller with SIGKILL. The
handler domain survives; a fresh helper acquires the released OS file lock,
reads the preserved armed record and closes only the matching native domain.
Unrelated-process survival is checked through cleanup. This proves recovery
at the stated barrier, not death before utility handoff or queued launches.

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/identity_probe.py --toolchain 1.89.0 --report /tmp/fsm-native-identity-msrv.json
python3 crates/fsm-execute/tests/lifecycle_platform/identity_probe.py --toolchain stable --report /tmp/fsm-native-identity-stable.json
```

Ten cases currently pass at both toolchains: empty preallocation, unprivileged
refusal, missing authority, controller death, active successor refusal,
unknown inode, closed/stale and prior-boot handle refusal, same-name alias
refusal, counter rollback and successor non-reuse. The root fixture's action
errors are test-harness failures, not a production error API. Records are
synchronized within the current boot's /run environment; no cross-reboot
registry persistence or reset-reconciliation claim is made.

Still required before the native gate releases: a bounded authenticated
privilege protocol, cancellation of queued/delayed launches, death at every
handoff and closure window, lost backend authority, signal notification and
usable reconciliation or explicit refusal of uncertain states. Existing
positive containment, freeze/kill and pipe probes remain required alongside
this prototype. Binding native authorization to actual production journal
claims and handler contracts belongs to gated downstream tasks 9302/9303;
those tasks remain pending rather than becoming a circular prerequisite for
native feasibility. No production authorization is claimed by this prototype.

### Frozen identity-prototype review

Range `5f3b5ce..8f57dc34f039c60ce4560558ea0f26266b0883ec` is reviewed.
Repairs require full supplied handles, prevent counter rollback behind active
or historical identities, refuse missing authority rather than initializing
it, preserve the full identity on idempotent close, and verify intended
refusal diagnostics with unchanged protected counter/active/run bytes.
A misplaced idempotent-close assertion was corrected before frozen review.

Exact clean reports are `/tmp/fsm-native-identity-clean-msrv.json` and
`/tmp/fsm-native-identity-clean-stable.json` (ten identity cases each), plus
`/tmp/fsm-native-identity-clean-containment-msrv.json` and
`/tmp/fsm-native-identity-clean-containment-stable.json` (six existing native
containment cases each). All identify source `8f57dc3`, dirty false and gate
unreleased. No task-owned identity/controller/containment units remain.
Formatting, all-targets Clippy, warning-free docs, zero-dependency, embedding
and full stable Linux workspace debug/release gates pass. MSRV negative
probes and all 45 Python harness self-tests pass. macOS/Windows native
execution remains unperformed. No production broker, migration, journal
claim, complete task-9301 proof or integration completion is claimed.

### Private privilege protocol and entry gate preparation

The native fixture now includes a private Unix-socket broker with a fixed
`privilege/1` protocol. Provisioning fixes its operator UID and task namespace;
the root-owned parent and operator-owned mode-0600 socket enforce local
access. Handler DynamicUsers cannot connect. Requests retain at most 256 bytes
and only admit allocate, inspect, launch and close with complete identities.
There is one active connection, a 300ms frame deadline plus at most one 250ms
read timeout, and a 250ms write timeout. The kernel listen backlog is not a
production admission budget. Lock acquisition is bounded; utility completion
and reap polling have deadlines and preserve unresolved ownership on failure.
The finite probe broker lifetime is not a production service lifecycle.

Root-owned grants publish armed, closing and closed phases. Before handler
fixture work, the unprivileged entry gate checks the grant's full counter,
inode and boot tuple, armed phase, and actual kernel cgroup enrollment.
Closure revokes the grant before freeze/kill. Native probes require refusal
of a copied armed grant in another domain and of a captured submission
replayed after durable closure, without protected-record mutation.

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/broker_probe.py --toolchain 1.89.0 --report /tmp/fsm-native-broker-msrv.json
python3 crates/fsm-execute/tests/lifecycle_platform/broker_probe.py --toolchain stable --report /tmp/fsm-native-broker-stable.json
```

This remains private feasibility preparation. It does not bind grants to
journal claims or handler contracts, implement stale-socket restart, cancel
all manager jobs, or prove every queued-job/crash/authority-loss window.
A replay submitted after closure is narrower than a queued manager job
surviving controller death. Production budgets, autonomous reconciliation,
cross-boot authority and signal handling remain pending; task 9301 and the
integration gate are unreleased.

Frozen review range `f545be7..f95b4e3fd046fb3caeeea5a1c6540b69f03eea8e`
is complete for this private prototype. Exact clean reports
`/tmp/fsm-native-broker-clean-msrv.json` and
`/tmp/fsm-native-broker-clean-stable.json` pass nine broker cases each;
`clean-identity-*` and `clean-containment-*` companions pass ten identity and
six containment cases per toolchain. The two
`/tmp/fsm-native-broker-neutralized-{msrv,stable}.json` reports retain the
expected native-enrollment failure after removing only the entry gate;
they are negative controls with dirty source and do not claim backend success.
The isolated checkout is restored, and no task-owned units remain. All required
stable Linux host gates pass, as do final MSRV/stable lifecycle tests and 45
Python harness self-tests. macOS/Windows native execution remains unperformed.
The reviewed changes do not affect public APIs, journal formats or historical
hashes; prior-version migration evidence is not claimed for these private
probe records. The full task-9301 gate remains unreleased.

### Pending launch and closure window preparation

The private native fixture can hold a trusted `ExecStartPre` barrier inside
its preallocated domain. The window probe observes the kernel inode and a
real manager start job (`activating`, `start-pre`, nonzero job), then kills
the launch controller. A fresh helper observes the armed identity, closes
the domain, requires the job to disappear and releases the barrier; no
handler or descendant ready marker may have appeared. This covers a manager
job accepted before controller death, beyond the earlier post-closure replay.

Two separate closure-death barriers cover revoked grants before private
closing publication, and native removal plus a protected closure receipt
before the final tombstone. Recovery from the latter requires a matching
counter/inode/boot receipt and revoked grant; missing or corrupt evidence
refuses cleanup without mutating the private record. An unrelated same-name
replacement survives refusal even when the genuine closure receipt exists.
The receipt is written only after actual native cleanup and domain removal.
There is still an unproved interruption before receipt persistence: missing
native state without a receipt remains unknown and cannot authorize reuse.
These are private /run probe records, not a production journal format.

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/window_probe.py --toolchain 1.89.0 --report /tmp/fsm-native-window-msrv.json
python3 crates/fsm-execute/tests/lifecycle_platform/window_probe.py --toolchain stable --report /tmp/fsm-native-window-stable.json
```

Eleven named cases cover pending start, controller death, pending-job close,
revocation death/recovery, finalization death, missing/mismatched receipt,
alias refusal, finalization recovery and strictly monotonic succession.
Full journal/contract authorization, all other crash windows, signals,
authority loss, restart and cross-boot proof remain pending. Task 9301 and
integration remain unreleased; this work does not start downstream tasks.

Frozen window review `81aab9a..68895897f576e821c698a2e54f96d9ce15d52102`
is complete. The review added direct observation of the same manager job
after controller death and changed finalization recovery to close a live
handler tree. That stronger case exposed prior-DynamicUser file permissions;
only the task-owned fixture work directory is rebuilt after verified closure.
Exact clean reports `/tmp/fsm-native-window-clean-msrv.json` and
`/tmp/fsm-native-window-clean-stable.json` pass eleven window cases each.
The `clean-broker-*`, `clean-identity-*` and `clean-containment-*` companions
pass nine, ten and six cases per toolchain, all from source `6889589` with
dirty false and the gate unreleased. Removing only the receipt inode guard
makes the receipt-mismatch case fail at both toolchains; separately labelled
`/tmp/fsm-native-window-neutralized-{msrv,stable}.json` reports retain those
expected failures with dirty source, never a passing backend verdict. The
isolated checkout is restored and no task-owned units remain. Full stable
Linux debug/release workspace, formatting/file-size, all-targets Clippy,
warning-free docs, zero-dependency and embedding gates pass, as do final
MSRV/stable lifecycle tests and all 45 Python harness tests. Native
macOS/Windows remain unexecuted. No public journal/API/hash bytes change;
prior-format migration is not claimed for these private probe records.

### Socket publication and signal notification preparation

The private broker checks the Linux process creation mask before binding its
socket: group and other write bits must be excluded. Applying mode 0600 after
binding cannot retroactively reject a connection queued during a permissive
creation window. A native UMask=0000 case must fail with the intended mask
diagnostic before socket/ready publication or counter mutation. Probe broker
units explicitly use UMask=0077; protected kernel state is inspected as root.

A fixed `lease:<full-identity>` request launches through the existing admission
path and keeps the Unix connection open. EOF, a client byte, connection error,
failed reply delivery or the finite probe watchdog asks the surviving broker
to close the domain through the ordinary full-identity closure path. The
notification marker is diagnostic only; persisted closure and actual domain
removal remain the authority. The probe is sequential with one held lease
and a five-second safety watchdog, not a production multi-resource budget.

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/signal_probe.py --toolchain 1.89.0 --report /tmp/fsm-native-signal-msrv.json
python3 crates/fsm-execute/tests/lifecycle_platform/signal_probe.py --toolchain stable --report /tmp/fsm-native-signal-stable.json
```

Separate real SIGTERM and SIGKILL cases kill only the operator client's main
process. Its exec'd descendant remains alive in a separate native domain;
EOF must still reach the surviving broker and verified handler-domain closure
must finish within the probe's two-second bound. This tests exec-closed socket
inheritance and prevents the five-second watchdog from masquerading as signal
notification. An unrelated process also survives. SIGTERM uses its native
default termination behavior here: this proves a death notification primitive,
not production graceful task joining or settlement. Broker death, stale-socket
restart, lost authority, remaining crash windows and production shutdown still
need evidence; task 9301 and integration remain unreleased.

Frozen socket/signal review `776f02a..ce3d163` is complete. Exact clean
reports `/tmp/fsm-native-signal-clean-{msrv,stable}.json` pass two signal
cases per toolchain; their `clean-broker-*`, `clean-window-*`,
`clean-identity-*` and `clean-containment-*` companions pass ten, eleven,
ten and six. Every positive identifies source `ce3d163`, dirty false and
gate unreleased. Separately removing the creation-mask guard and ignoring
EOF makes the intended mask and signal cases fail at both toolchains.
`/tmp/fsm-native-signal-negative-{mask,eof}-{msrv,stable}.json` retain those
expected failures with dirty source, never a passing capability verdict.
The isolated checkout is restored and no task-owned units remain. Required
stable Linux debug/release workspace, formatting/file-size, all-targets
Clippy, warning-free docs, zero-dependency and embedding gates pass; final
MSRV/stable lifecycle and all 45 Python harness tests pass. Native
macOS/Windows remain unexecuted. These private fixtures change no public
journal/API/hash bytes; no migration evidence or task completion is claimed.

### Private supervisor endpoint epochs and restart

The broker now holds a lifetime exclusive authority lock and burns a
provisioned monotonic broker epoch before binding `control-<epoch>.sock`.
A root-owned, synchronized read-only route records the epoch, boot and
operator UID. Native clients validate that route and current boot rather
than accepting a caller-supplied socket path. A restarted broker never binds,
connects to or reclaims an older endpoint. Missing epoch authority, rollback
behind the published route, operator mismatch, malformed routes and route
inspection errors refuse startup. The lock is held independently of the
short-lived native run writer lock. These remain private probe formats.

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/restart_probe.py --toolchain 1.89.0 --report /tmp/fsm-native-restart-msrv.json
python3 crates/fsm-execute/tests/lifecycle_platform/restart_probe.py --toolchain stable --report /tmp/fsm-native-restart-stable.json
```

Nine cases prove exclusive supervisor ownership, uncatchable broker death
with a live handler domain and unchanged claim, a new endpoint after restart,
refusal of successor allocation while that claim remains armed, survival of
an unrelated listener installed at the old endpoint, matched-domain cleanup,
missing-authority refusal, counter rollback refusal and strictly monotonic
succession. Administrator alias injection is separate from recovery and does
not become cleanup permission. Stale sockets are intentionally preserved;
production cleanup/storage budgets are still required. Directory replacement,
other authority loss and remaining handoff/closure windows remain pending,
as do production journal binding, hosting and shutdown. The full gate remains
unreleased; these helpers do not initialize missing authority.

Frozen restart review `7824c4b..75dceda` is complete. The initial clean MSRV
report `/tmp/fsm-native-restart-clean-msrv.json` at `5f5125a` records a real
permission failure between rename and chmod; it is not a passing result.
Repair `75dceda` applies public permissions before fsync and atomic rename
for both routes and grants. A reader acknowledges every one of 64 complete
publications, giving ten named restart/publication cases per toolchain.
Exact final reports `/tmp/fsm-native-restart-final-{msrv,stable}.json` pass
those ten cases; their `final-signal-*`, `final-broker-*`, `final-window-*`,
`final-identity-*` and `final-containment-*` companions pass two, ten, eleven,
ten and six. All positives identify `75dceda`, dirty false and gate unreleased.
Separately removing lifetime authority, rollback and readable publication
ordering fails the intended native case at both toolchains; retained
`/tmp/fsm-native-restart-negative-{lock,rollback,publication}-{msrv,stable}.json`
reports are expected failures with dirty source, never passing capability
verdicts. The isolated checkout is restored and no task-owned units remain.
All required stable Linux host gates, final MSRV/stable lifecycle tests and
45 Python harness tests pass. Native macOS/Windows remain unexecuted. These
private formats change no public journal/API/hash bytes, and no prior-format
migration or integration completion is claimed.

### Authority-directory replacement refusal

The private route format is now `endpoint/2`: it additionally records the
protected authority directory's device and inode. Startup compares this
published lineage before opening its lifetime lock, and the live broker
checks the captured directory identity, owner and private permissions before
operations and lease cleanup. A copied counter and lock file cannot substitute
for the original directory. Read failures and changed identities refuse work.
These checks cover observed replacement at operation boundaries; they do not
claim protection against a malicious root administrator racing filesystem
changes during an operation or replacing the entire namespace and registry.

The restart probe adds two named cases. It moves the original data directory,
copies every valid byte into a new protected directory, and requires both the
live broker and a new startup to reject the copied authority. Counter, epoch
and claim bytes remain unchanged; no new endpoint is published, and the
original handler domain remains populated with its recorded inode. Restoring
the original directory makes matched inspection and cleanup available again.
The private probe format rejects old endpoint/1 routes; no production format
migration is claimed. Other privilege/facility loss, whole-namespace loss,
storage budgets and remaining windows still need evidence. The native gate
remains unreleased.

Frozen directory review `0fbff45..9c4abdd` passes twelve restart cases at
MSRV and stable in `/tmp/fsm-native-directory-clean-{msrv,stable}.json`;
both reports identify `9c4abdd`, dirty false and gate unreleased. Separately
neutralizing the live and startup lineage checks fails the intended case on
both toolchains; `directory-negative-{live,startup}-{msrv,stable}` reports
retain expected failures, and the isolated checkout is restored. Required
stable host gates and 45 Python harness tests pass. Existing canonical stable
fixtures additionally pass two signal, ten broker, eleven window and ten
identity cases, recorded as `directory-regression-*-stable` with explicit
prebuilt provenance. These regressions do not claim a new MSRV rerun or a
new containment-suite run. High host swap usage prevented additional builds
under the repository memory rule.

### Facility-loss preparation

`facility_probe.py` masks `/run/systemd/private` and the system bus socket
inside a uniquely owned broker service's mount namespace. The host manager
continues running. Manager submission fails within the deadline before any
handler entry, leaving the original armed, empty domain and durable claim.
Successor allocation refuses without changing its counter. Native cgroup
freeze/kill/removal and protected grant revocation still establish matched
closure even with manager access unavailable; this is not absence-based
recovery. A second owned broker mounts cgroups read-only. Allocation refuses
before handler execution, retaining the previous active identity and burning
successive counters rather than reusing a failed allocation identity.

Five preparation cases pass using the already built canonical stable fixture
at `9c4abdd`; `/tmp/fsm-native-facility-preparation.json` is an exploratory
result, not frozen source-bound acceptance. No host manager or unrelated unit
is stopped. The unrelated process survives cleanup. The probe's standard
entry point builds and records exact source/toolchain evidence using the
same native reporter as the identity suite:

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/facility_probe.py --toolchain 1.89.0 --report /tmp/fsm-native-facility-msrv.json
python3 crates/fsm-execute/tests/lifecycle_platform/facility_probe.py --toolchain stable --report /tmp/fsm-native-facility-stable.json
```

This tests loss of access to a running manager, not manager/kernel death or
loss of all administrative authority. Whole-namespace loss, storage budgets,
remaining crash windows and production integration remain pending; the
native prerequisite is unreleased.

### Reproducible complete native inventory

`native_matrix.py` runs the seven native suites sequentially at a selected
compiler, requires the exact named inventories, complete passing cases,
unrelated-process survival, clean identical source and matching compiler,
and then requires the disabled-final-kill control to fail with an observed
populated domain. It bounds report reads and driver deadlines, retains each
source-bound report and its digest, and produces a final matrix summary only
if all requirements pass. Existing directories cannot be reused as evidence.
It never releases the task gate automatically or counts unsupported facilities
as successful skips.

```sh
python3 crates/fsm-execute/tests/lifecycle_platform/native_matrix.py --toolchain 1.89.0 --report-dir /path/to/new/msrv-evidence
python3 crates/fsm-execute/tests/lifecycle_platform/native_matrix.py --toolchain stable --report-dir /path/to/new/stable-evidence
```

The CI `native-containment` job runs this inventory on provisioned Ubuntu at
both compilers and uploads reports even on failure. The existing six-leg
portable Rust matrix remains intact. CI execution and exact frozen aggregate
review are required before claiming these jobs pass. The separate frozen
facility review at `809978f` already passes five cases per compiler in
`/tmp/fsm-native-facility-clean-{msrv,stable}.json`, with dirty false and gate
unreleased. This does not prove host manager/kernel death, production claim
binding or integration completion.

The portable lifecycle test now exercises all six private native fixture
entry modes on non-Linux hosts, requires a nonzero exit with the precise
unsupported-platform diagnostic, and requires the work directory to remain
empty. The pre-entry barrier also refuses outside Linux before writing a
marker. These assertions cover fixture capability refusal, not a production
CLI capability that has not yet been implemented. Real macOS/Windows CI
execution remains required to establish their outcome.

### Frozen aggregate review and actual CI

Local frozen matrix source `3f9f0d1` passes 56 native cases and one required
negative per compiler. Candidate CI initially rejected the job-level runner
context before starting jobs (run `37241901509`); `bac926e` moves runner paths
into the execution step. Run `37241946083` then exposed two real portability
issues: unreachable fixture returns after non-Linux panics, and Python 3.12
raising a permission error where Python 3.14 hid a protected-path stat failure.
`f1ccee1` restricts returns to Linux and proves protected counter absence
through the administrative provisioner. The failed artifacts are retained;
permission failure is not evidence of absence.

Actual native CI at `f1ccee1` and `6852cf3` passes the full 56-case inventory
and disabled-kill control on Ubuntu kernel `6.17.0-1022-azure`, systemd
`255.4-1ubuntu8.17`, Rust `1.89.0` and stable `1.99.0`. This is additional native
host evidence beyond the local kernel 7.0/systemd 259 environment. Reports
are uploaded with their exact source, fixture hashes, compiler and case
observations. These runs are on the separate review branch; remote develop
and releases are not updated.

- [Initial rejected workflow](https://github.com/koraytaylan/fsm/actions/runs/37241901509)
- [First executed candidate and retained failures](https://github.com/koraytaylan/fsm/actions/runs/37241946083)
- [Repaired native candidate](https://github.com/koraytaylan/fsm/actions/runs/37242072483)
- [Unsupported-host refusal candidate](https://github.com/koraytaylan/fsm/actions/runs/37242284024)

The last two runs' portable six-leg jobs are still active at this checkpoint.
Native successes do not establish their result or a final task landing.
Production journal, hosting, shutdown and admission requirements remain owned
by the downstream tasks rather than becoming a circular feasibility gate.

### Surviving namespace membership after authority loss

Review found that directory lineage alone does not cover losing both private
records and the published route, or losing the active pointer in place.
Before startup burns an endpoint epoch, and before allocation burns a run
counter, the private prototype now inventories at most 4096 immediate native
system-slice entries. Every matching task namespace domain must have its
protected full record, active pointer, monotonic counter and current boot/inode
identity. Unknown, aliased, unreadable or over-budget inventory refuses
without creating a handler or adopting an unknown tree. This is native domain
inspection, not a PID/process-tree scan, and it never kills an unknown group.

The restart probe independently removes the active pointer while retaining
its directory, then moves all private authority and the route aside and
provisions fresh zero counters. Both paths must refuse despite the surviving
original domain. Counters remain unchanged and the live domain retains its
recorded inode. Restoring original authority restores matched inspection;
subsequent broker-death recovery and closure remain part of the same suite.
The probe records the next possible empty-domain name for negative-control
failure cleanup, preventing a deliberately disabled guard from leaving an
untracked fixture cgroup. Exact frozen review and both toolchain controls are
required before claiming this repair passes; whole-environment reset and
production recovery remain downstream work.

### Interruptible native stdio independently of tree death

Safe Rust 1.89 supplies `UnixStream::pair`, safe `OwnedFd` conversion into
`Stdio`, cloned cancellation handles and `shutdown(Read)`. The Linux native
Rust test `pipe_cancel::socket_read_cancellation_joins_with_surviving_descendant`
passes on MSRV and stable using real socket-backed child stdout. It kills
only the direct root, observes the worker blocked on its own socket through
its thread syscall diagnostic, and retains an additional peer outside the
child tree. The independent cancellation call must return the reader result
and join within 250 ms. A fresh filesystem challenge after cancellation
requires the surviving descendant to answer; reader cancellation is therefore
not tree-closure evidence. TID is diagnostic synchronization only.

Cooperative fixture cleanup occurs after the candidate result is captured,
waits for the descendant to observe its stop marker, and cannot turn a
cancellation timeout into a pass. The child receives null protocol stdin.
The native matrix now requires this exact one-test result at its selected
compiler before running the seven native suites and retains its diagnostic
digest. No production MCP reader is changed by this proof. The production
runner must use the demonstrated interruptible transport and keep ownership
unresolved until native tree closure; plain anonymous-pipe EOF is insufficient.
Frozen aggregate review and cancellation neutralization remain required.

### Verified matrix and signal decision

Frozen source `46d0db24df7c5f108cdf8631259a3b81c4023a01` passes all
61 cases at MSRV and stable on the local Linux host and in both native jobs
of [CI run 37244150329](https://github.com/koraytaylan/fsm/actions/runs/37244150329).
Downloaded matrix artifacts have that exact clean source, and every suite,
I/O log and disabled-final-kill report matches its recorded SHA-256 digest.
Removing only the candidate read shutdown makes the bounded reader test fail
on both compilers. Separately disabling startup namespace inspection,
allocation namespace inspection or canonical domain-name validation fails
the corresponding native restart case on both compilers. Negative controls
retain dirty-source status and never release the gate.

The initial Linux backend uses an explicit project decision under the
delegated authority recorded above: ordinary signal termination does not
promise graceful drain. SIGTERM and SIGKILL terminate the executor; the
protected supervisor observes lease EOF and closes the native domain, while
durable ownership must survive until closure and settlement are verified.
Task 9402 must provide the separately requested, independently woken local
drain/abort control and bounded report. It must not label native SIGTERM
default termination as drain, invoke settlement from a signal handler, or
clear ownership merely because the executor exited. This decision avoids
assuming a safe standard-library signal callback that the API inventory did
not establish; safe Rust, zero dependencies and MSRV remain unchanged.

For the excluded macOS and Windows contained capability, neither Unix signal
tests nor cross-compilation establish native lifecycle support. Windows
console events and forced termination need separate safe notification and
Job Object enrollment/closure proof before that capability can be enabled;
macOS likewise requires a separately proved containment authority. Existing
portable behavior remains covered by the six-leg CI gate. Both macOS legs
of earlier run `37242284024` passed, while its Windows legs failed held-lock
test snapshots; `fedfc47` repairs the snapshots without changing production
locking. Current portable CI and the local stable host gate remain active,
so task 9301 and its implementation gate remain unreleased at this checkpoint.

### Provisioning boundary for the selected backend

The proved primitive requires the system manager and a privileged supervisor,
not a delegated user-manager service. The provisioner must establish protected
cgroup-v2 authority with readable membership/populated state and writable
`cgroup.freeze` and `cgroup.kill`, plus system-manager access for transient
service enrollment. Capability detection must refuse missing or inaccessible
facilities before business code runs; the facility suite proves those refusal
paths and separates manager access from matched cgroup closure. Tested host
versions above are evidence, not a blanket minimum-version compatibility claim.

The supervisor's namespace and counters belong to root; its private records
exclude group and other access. Public routes and grants must be complete,
durably published root-owned records, with creation permissions applied before
publication. The operator is a distinct non-root UID authorized by the local
socket boundary; endpoint epochs and authority-directory device/inode identity
prevent adopting a replacement directory or reusing an old socket. A new
supervisor must inspect surviving namespace domains before burning an epoch;
missing identity evidence refuses recovery rather than adopting or killing an
unknown tree. The fixed prototype protocol accepts no client-supplied root
command, shell fragment or arbitrary privileged path.

Actual enrollment uses an unprivileged DynamicUser with
`ProtectControlGroups=yes`, `Delegate=no`, `NoNewPrivileges=yes`, an empty
capability bounding set and namespace restriction. These protect membership
and the authority boundary; they do not establish that remote effects stop
when local processes die. Production installation and handler resource-access
policy remain task 9303 requirements: the operator must provision the required
resources for the separate handler identity, and protocol clients must not
broaden those permissions or choose privileged execution. The test-only
provisioner and fixed fixture are not an installed production service.

Production task 9303 must preserve the proved freeze/kill/empty verification,
immutable identities, entry authorization and independently cancellable I/O,
while replacing fixture authorization with task 9302's durable claim and
immutable handler fingerprint. Installation cannot enable direct-child
fallback, reset counters to repair lost authority, or reinterpret an unresolved
claim under a changed handler. Its production capability refusal and resource
policy require their own integration tests before that task can complete.
