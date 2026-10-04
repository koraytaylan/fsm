---
id: lifecycle-containment-feasibility
title: "Lifecycle Containment Feasibility"
workstream: "0093"
kind: task
depends_on: []
gated: true
touches:
  - crates/fsm-execute/tests/lifecycle_platform.rs
  - crates/fsm-execute/tests/lifecycle_platform/
  - docs/EXECUTOR-LIFECYCLE.md
status: planned
merged_as: ""
---
# Lifecycle Containment Feasibility

The implementation gate is native proof, not a design sketch that assumes
process-group, signal or job APIs are safely available under this charter.

**Steps:**

1. Inventory the actual safe APIs available at Rust 1.89 and stable, plus
   required native facilities on Linux, macOS and Windows, against the full
   containment and bounded-cleanup contract in this plan's architecture.
2. Add minimal executable native probes for atomic root enrollment, surviving
   descendants, spawn during stop, retained MCP pipes, clean and uncatchable
   executor/helper death, non-reusable identity and closure verification.
3. Record exact OS/toolchain versions, commands, observable outcomes and
   negative cases in `docs/EXECUTOR-LIFECYCLE.md`; a proof must show that an
   unrelated process survives cleanup and that an escaped or unknown tree
   prevents a replacement start.
4. Select a charter-compatible backend only if the full native matrix passes;
   otherwise describe the precise required supervisor/platform/charter
   decision and leave the gate unreleased until that decision is explicitly
   accepted and the resulting native proof passes.
5. Document graceful-signal notification and hard-kill recovery separately,
   including Windows console and forced-termination mechanisms; a cooperative
   wrapper, PID scan or inherited lock alone is an insufficient result.

**Tests:**

- `cargo test -p fsm-execute --test lifecycle_platform` executes the native
  probes on every supported executor OS at stable and MSRV, without silently
  excluding unsupported mechanisms or treating skipped probes as passes.
- Probe assertions fail when atomic enrollment, descendant containment,
  domain-closure verification or bounded pipe cancellation is neutralized.
- Zero-dependency and no-unsafe checks pass under the existing charter, or
  the accepted charter change and its exact revised gates are recorded before
  any downstream task starts; this task does not approve that change itself.
- Missing native facilities produce a detected unsupported capability before
  handler execution, with no accidental fallback to direct-child semantics.

- **Done when:** a recorded, explicitly accepted backend decision is backed by passing native containment and signal/kill probes on every supported OS and toolchain, and any required charter or platform change has separate recorded authorization rather than an unresolved assumption.
