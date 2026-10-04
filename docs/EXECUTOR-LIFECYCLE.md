# Executor lifecycle feasibility

## Decision status

Plan 0022 task `lifecycle-containment-feasibility` is **not complete**.
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

Authorization is now recorded, but the native-proof gate remains unreleased.
No downstream ownership, launch or journal implementation is justified until
the applicable native matrix passes. The executor currently
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
