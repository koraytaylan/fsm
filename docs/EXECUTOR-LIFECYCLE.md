# Executor lifecycle feasibility

## Decision status

Plan 0022 task `lifecycle-containment-feasibility` is **not complete**.
No backend or charter exception has been accepted. The executor currently
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
uncertain-state refusal. No production ownership, launch or journal changes
are authorized by this feasibility note alone.

## Review record

Initial self-review: the probe intentionally demonstrates the existing gap;
it neither detects a supported production backend nor tests all acceptance
rows. Positive containment probes, native Windows/macOS evidence, backend
selection and separate authorization remain outstanding. Task completion and
landing OIDs must remain unset until those requirements are met.
