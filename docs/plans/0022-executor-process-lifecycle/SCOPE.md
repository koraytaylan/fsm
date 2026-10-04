---
id: 0022
title: "Executor Process Lifecycle"
status: planned
---
# Scope — Plan 0022

> A successor may repeat an external operation, but it must not start a second local handler tree while the predecessor might still be running.

## Why this plan

`Runner::kill` and `Runner::drop` currently kill and wait for direct
`std::process::Child` handles; the module explicitly documents that a
signalled executor leaves children running and a restarted executor starts
fresh ones. A handler's descendants can also outlive a normally exiting
direct child, and an MCP descendant can retain the protocol pipes after its
server exits. The store's writer lock serializes journal writes, not handler
lifetimes: `service::prepare` starts processes before the short writer-open
phase, and a second executor can observe the same pending effect.

The review's lifecycle gap therefore includes clean shutdown, timeout and
cancellation, simultaneous executors, and restart after uncatchable death;
adding a signal handler or a PID file alone does not close it.

## In scope

- **0093 — Containment and durable ownership.** Establish a proved native
  process-containment boundary on every shipped executor platform, journal
  each run's ownership before its first possible external action, and make
  process and MCP handlers share containment, capture and cleanup semantics.
- **0094 — Shutdown and restart.** Admit starts through those durable claims,
  provide bounded drain and abort controls, reconcile interrupted claims
  using native evidence, and prove that an executor restart never overlaps an
  uncertain predecessor's handler tree.
- Keep at-least-once external effects explicit: a terminated process may
  already have submitted remote work, and only the handler's domain can
  supply remote idempotency, reconciliation or compensation.

## The prerequisite that must be resolved

The repository currently supplies no demonstrated native containment backend
that satisfies this target under safe Rust, zero third-party dependencies,
MSRV 1.89, and native Linux/macOS/Windows coverage. Task
`lifecycle-containment-feasibility` is deliberately gated: it must produce
working native evidence under that charter or an explicitly accepted charter
or supported-platform decision before dependent implementation can begin.
Plan authoring does not approve changing those promises.

Persistent uncertainty and refusal to restart are mandatory safety behavior,
but are not a substitute for implementing supported containment and usable
recovery. A plan with an unresolved platform gate is unfinished; neither
direct-child cleanup nor a cooperative helper earns the full lifecycle
claim without proof covering the helper's own death and escaped descendants.

## Out of scope

Distributed execution, high availability, remote-job cancellation guarantees,
exactly-once effects, shell interpretation, and changing statechart semantics
are excluded. Autonomous embedded scheduling belongs to plan 0020; this plan
provides the lifecycle protocol that host must use. Plan 0021 owns machine
and handler contract validation, while plan 0023 owns release-wide operational
and live-model evidence. No machine-specific or service-specific example is
required.

## Completion evidence

All seven tasks must land, and the lifecycle suite must exercise real process
and MCP trees through standalone and embedded production entry points on each
supported native OS at stable and MSRV. Evidence must distinguish explicit
drain, explicit abort, Ctrl-C, termination requests, and uncatchable kill;
prove no overlapping run even across claim/spawn/settle crash windows;
exercise unknown native state without starting work; preserve prior journal
history through migration and sealing; and show bounded shutdown, capture,
worker and descriptor use. Documentation must retain the remote at-least-once
boundary rather than turning local containment into an exactly-once claim.

## Authorized initial runtime decision (2026-10-04)

Under explicit user delegation, the implementation selects provisioned
Linux/systemd for the initial contained executor. The safe-Rust,
zero-dependency, Rust 1.89 charter remains intact. References above to every
supported executor platform mean every platform shipping the new containment
capability: initially Linux. macOS/Windows must reject that capability before
handler execution until their own native backends are proved; their existing
portable Rust and unsupported-capability tests remain required. This changes
the planned runtime/platform matrix explicitly; it does not earn task
completion or substitute a passing skip for native proof. The full Linux
containment matrix and all original ownership/shutdown invariants still apply.
See [the decision record](../../EXECUTOR-LIFECYCLE.md).
