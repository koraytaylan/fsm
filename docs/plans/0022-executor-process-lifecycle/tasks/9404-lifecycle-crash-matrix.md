---
id: lifecycle-crash-matrix
title: "Lifecycle Crash Matrix"
workstream: "0094"
kind: task
depends_on:
  - uncertain-run-reconciliation
gated: false
touches:
  - crates/fsm-cli/Cargo.toml
  - crates/fsm-cli/tests/executor_lifecycle_crash.rs
  - crates/fsm-cli/tests/executor_lifecycle_crash/
  - crates/fsm-execute/tests/lifecycle_platform.rs
  - crates/fsm-execute/tests/lifecycle_platform/
  - .github/workflows/ci.yml
  - .github/workflows/develop-snapshot.yml
  - docs/EXECUTOR-LIFECYCLE.md
  - docs/RELEASE.md
status: in_progress
merged_as: ""
---
# Lifecycle Crash Matrix

The lifecycle promise needs an independent observer of real processes across
native crash boundaries, not only a runner state-machine assertion.

**Steps:**

1. Build a portable fixture executable with process and MCP modes that can
   spawn descendants, retain pipes, outlive the root, emit external markers
   and synchronize at explicit barriers without shell interpretation.
2. Drive standalone and embedded binaries through claim, authorization,
   spawn, candidate-result, domain-close, stopped-record, ack and event
   cutpoints, killing the executor and any supervisor independently.
3. Restart immediately while the prior tree is alive and use an independent
   observer to assert no overlapping marker interval; verify eventual
   sequential recovery when native closure succeeds and refusal otherwise.
4. Add the lifecycle matrix to native Linux/macOS/Windows stable/MSRV CI with
   bounded runtime and retained diagnostic artifacts, pinning which native
   actions represent Ctrl-C, ordinary termination and forced kill.
5. Record frozen-commit evidence, prerequisite/backend versions and residual
   remote-effect limitations in the lifecycle/release documentation, leaving
   unexecuted platforms and unresolved gate decisions visibly incomplete.

**Tests:**

- `cargo test -p fsm-cli --test executor_lifecycle_crash` covers both handler
  kinds and both execution hosts at every documented crash cutpoint, including
  concurrent executors, writer contention, spawning descendants and partial
  durable writes; journal verification succeeds after each supported recovery.
- Retained-pipe and noisy-output fixtures prove bounded stop duration and
  repeated-run resource use; uncooperative descendants cannot authorize a
  replacement simply because the direct child has exited.
- Removing only claim-before-start, identity matching or verified-closure
  enforcement causes named production-facing cases to fail.
- Native PID/domain reuse and unrelated-process sentinels prove cleanup does
  not target another tree; helper death, missing backend and environment-reset
  cases pin uncertainty versus proved recovery.
- Full debug/release workspace tests, formatting, file-size checks,
  all-target clippy, rustdoc warnings, zero dependencies, embed acceptance and
  the native stable/MSRV matrix pass on the reviewed frozen range; no native
  termination axis is replaced by a mock or cross-compilation result.

- **Done when:** the frozen implementation range has passing native lifecycle CI evidence for every supported platform, handler kind, execution host and declared termination/crash boundary, with independent proof of no overlapping local run and explicit unresolved-state refusal.
