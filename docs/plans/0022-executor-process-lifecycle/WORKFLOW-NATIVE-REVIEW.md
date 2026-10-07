# Provisioned workflow acceptance review

## Required production path

The privileged workflow fixture executes the original CLI integration tests,
including discovered handlers, preflight failure, compensation and cleanup,
standalone/embedded contention, standalone/standalone contention, and borrowed
embedded execution: seven named groups containing twelve scenarios.

The fixture selects Cargo-reported CLI and test executables by identity and
SHA-256, provisions the exact Root authority, and checks the original native
claims and authenticated closure evidence against the cold, verified Store.
An independent external marker proves exclusion of competing live handler
trees; a workflow's terminal state alone does not prove native retirement.

Source owners:

- `crates/fsm-execute/src/containment/workflow_native_tests.rs`
- `crates/fsm-execute/src/containment/workflow_memory_limits.rs`
- `crates/fsm-execute/tests/lifecycle_platform/workflow_probe.py`
- `crates/fsm-cli/tests/mcp_execute_workflow.rs`
- `crates/fsm-cli/tests/workflow_race/`

## Fixture and retention invariants

Native workflow groups require provisioned Linux execution and are explicitly
ignored by ordinary portable test runs on every platform; the privileged
supervisor selects them with `--ignored` and requires exactly one passed child
with zero ignored cases. Unsupported platforms instead test refusal before
handler execution or journal mutation. Stable and MSRV native CI require this
producer after the containment matrix and upload its reports even on failure.

Fixture-owned domains use bounded memory and zero swap; assertions read actual
kernel controls and verify matching receipts. Namespace-scoped limit catalogues
are installed and retired by their original fixture identity. A successful
case verifies all original runs and unused allocations before matched teardown.

Failure retains the original authority, namespaces and staged executable
identities. Root-protected failure observations are exported through a bounded,
read-only reader into user-readable CI artifacts; export errors cannot authorize
cleanup or replace the original failed verdict. These observations do not
constitute closure receipts, and a snapshot taken after owner exit is not an
atomic observation of the preceding failure phase.

A sandbox can display a Root-owned host helper as UID 65534 because its UID map
cannot represent host Root; host metadata, rather than that mapped display,
determines provisioning ownership. Existing helpers are never replaced or
chowned merely because the sandbox reports that mapped UID.

## Defects resolved before acceptance

`71a7aa71` and `0868fd40` bounded binding-authority lock contention before entry,
retaining the original deadline, identity and ownership checks.

`bd16df6d` prevents a transport worker from exiting when a separate reap first
observes retirement before its pending poll has decoded a response; a joined
worker without a response returns explicit uncertainty. Real-child sensitivity
controls fail when either guard is removed and pass after restoration.

`26c68220` waits only for preparation lock contention, under one bounded
monotonic acquisition deadline, then rechecks the original authority identity
before allocation. Genuine privileged controls cover lock release, exhaustion
without mutation, and replacement identity; neutralized guards fail the named
cases and restoration passes.

## Frozen acceptance evidence

At `26c682203df56c498132ab3881b48bd04cdbb46f`, stable and MSRV native jobs passed
both 82-case containment matrices and all twelve production workflow scenarios.
Independent review verified exact source/compiler identities, registered case
inventories, report/log digests and one occurrence of every workflow marker.
The uploaded artifacts do not include executable bytes, so that independent
review does not claim to have rehashed those binaries.

The exact checkpoint is [CI run 37683538976](https://github.com/koraytaylan/fsm/actions/runs/37683538976).
The ownership review and its Linux gate evidence are retained in the task cache
under digest `bc05b865c0725b896ca520c39c8816b2af0258c4f51c855f7c6e1641f3c64b72`;
applicable macOS ownership-test evidence has digest
`f42b2b86b2d021b7e02d6aa982797463d6a16184fa58d70c7974775cab86e675`.

These workflow and containment verdicts do not complete the launch/settlement
crash matrix, full shutdown inventory, HTTP ownership, or operational acceptance.
Task frontmatter and STATUS remain authoritative for completion.

## Historical evidence

The exact prior review is preserved outside the repository in task-cache
`plan-status-archives`, addressed by SHA-256:
`d4452728436bf0be614c3bc205386f2fabf6b88cf8b904a5d5d321cc1e01530f`.
Intermediate sessions, PIDs, logs, failed fixtures and superseded checkpoint
narratives belong to that archive, rather than the current mechanism review.
