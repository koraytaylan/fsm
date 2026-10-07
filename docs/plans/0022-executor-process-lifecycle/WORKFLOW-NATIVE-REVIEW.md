# Provisioned ordinary workflow fixture review — 2026-10-07

The original four mcp_execute_workflow tests and their nine scenarios retain
their ordering, compensation, external-resource, acknowledgement and verified
journal assertions and original per-scenario deadline. A private test manifest
can supply separately registered physical stores, external resource paths,
the exact staged CLI and a compact private operator home; it does not select
a different executor. Without the manifest the existing test environment and
production CLI remain unchanged, including the currently failing Linux cases.

The root fixture uses the existing strict root-only store transfer and genuine
broker daemon, publishes each immutable catalogue before allocation, and runs
the copied original test binary after dropping supplementary groups, GID and
UID to 65534. The helper binary lives outside ProtectHome in an exclusively
created protected staging directory. Artifact reads reject symlinks, nonfiles
and more than 64 MiB, and authenticate bounded bytes before any executable
publication. The producer packages only debug-stripped exact Cargo artifacts;
the frozen full-gate CLI is 84 MiB unstripped, exceeding that unchanged bound.
Debug assertions and optimization defaults are not changed by packaging.

External resource files are initialized while their directory is root-private,
then made writable for distinct contained DynamicUser allocations; the work
result remains readable by its separate operator observer. Store/control paths
stay private and executable/catalogue paths stay protected. Native final socket
paths are checked against the transport limit. Cleanup checks original resource
and home inodes, then uses existing changed/populated/unknown-domain refusal;
only complete matched fixture teardown retires staged bytes. No namespace or
cgroup absence is promoted into a production closure receipt.

After each original scenario group, independent root readback requires the
exact native allocation and Claimed/Stopped/Settled counts, no unresolved run,
and an original protected closure receipt for every claim and physical store.
These checks reject a legacy route even if it produces the expected external
workflow effects. Four verified-case markers bind the producer's nine-scenario
verdict; missing markers cannot pass. The producer retains authority and staged
bytes on unknown teardown, saves timeout diagnostics with unknown exit status,
and preserves the original TimeoutExpired cause.

Seven permanent mocked producer tests pass; neutralizing namespace, staged-file
or missing-case guards individually fails their cached sensitivity controls,
with restored tests passing. No native process executes in these mocked tests.
Stable all-target CLI/executor Clippy, MSRV compilation, size checks and the
existing real env-cleared helper control pass in native-workflow-fixture-final-
check.log under serial asserted 1 GiB/zero-swap scopes. Earlier check logs record
a six-line size excess and then an ancestor-visibility compile refusal; the
setup is now in its own module and the original root-only ownership assertions
are unchanged. Actual registered production workflow execution remains pending;
compilation and mocked evidence do not promote task 9401 or release its gate.

## First native execution and fixture corrections

Frozen 9aa7c63 terminated with native test exit 101 in 4.67 seconds: the first
ordinary scenario reached its succeeded state but its legacy EffectAcked
history counter read zero versus seven expected calls. SPEC settlement is an
atomic ExecutionSettled/acked record, without a separate EffectAcked append;
the manifest-backed test now counts only durable acked settlements through a
read-only Store and cross-checks the MCP settlement-entry count, while ordinary
legacy counting stays unchanged. Independent root readback also requires every
settlement to be acked; attempted/interrupted dispositions never substitute.

The retirement guard retained exact authority and staged bytes. Subsequent
authoritative inspection found no authority namespace, production-profile
cgroup or owned executable process, with all seven external calls and active
phase; work was missing. An owned DynamicUser service with MemoryMax=64 MiB
and MemorySwapMax=0 reproduced deletion of its newly owned shm file while a
root-owned linked file survived, consistent with systemd's implied RemoveIPC
policy ([upstream documentation](https://raw.githubusercontent.com/systemd/systemd/main/man/systemd.exec.xml)).
The fixture now preprovisions a root-owned empty backing inode and the actual
perform_work handler publishes its work link and writes the real full/partial
result; work remains absent for rejected preflights, and containment policy
is unchanged. Protected staging inventories now retain original resource/home
identities for failed-run recovery. Matched cleanup verified staged helper/CLI
digests, original authority device/inode/digest, no live owned executables and
no production domains before removing only these test artifacts; this is owned
fixture teardown, not native closure inferred from absence or a passed verdict.
Failed evidence remains local-native-workflow-9aa7c63-stable. Stable all-target
Clippy, MSRV compilation, source size, seven producer tests and the real helper
argument control pass after correction (native-workflow-settlement-resource-
final-check.log); a fresh frozen native rerun remains required.

## Exact registered ordinary workflow execution

Frozen 9f744da663add6bb86480df27163492b3e6f5e1e passed all four original
workflow groups and their nine scenarios on stable Rust 1.98.1 (session 88622)
and MSRV 1.89.0 (session 55823), both terminal exit zero. Independent readback
verifies exact clean source, all nine scenario counts, each unique verified-case
marker and the complete log SHA-256 in local-native-workflow-9f744da-stable
and local-native-workflow-9f744da-msrv. Root assertions execute after the actual
ordinary CLI checks and require original native claims, matching closure
receipts, acked settlements and physical store identity for every call; legacy
effects cannot satisfy this verdict. Matched teardown completed without retained
artifact fields. Both reports keep gate_released=false. Controllers/builds ran
serially under asserted 1 GiB and zero swap; root-manager service units are
outside those scopes. This accepts these registered embedded workflow scenarios,
not standalone/signal/blockage races or full ownership integration.
