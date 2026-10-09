---
id: autonomous-execution-contract
title: "Autonomous Execution Contract"
workstream: "0090"
kind: task
depends_on:
  - autonomous-http-transport
gated: false
touches:
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/src/mcp/resources.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/tests/mcp_executor.rs
  - crates/fsm-cli/tests/mcp_execute_workflow.rs
  - crates/fsm-cli/tests/workflow_stdio/mod.rs
  - crates/fsm-cli/tests/executor_doc.rs
  - crates/fsm-cli/tests/transport_doc.rs
  - crates/fsm-cli/tests/fixtures/mcp_live/
  - crates/fsm-cli/tests/fixtures/mcp_affordance/
  - crates/fsm-cli/tests/fixtures/transcripts/
  - crates/fsm-cli/tests/fixtures/audit/session.expected
  - docs/SPEC.md
  - docs/API-POLICY.md
  - docs/EMBEDDING.md
  - docs/RELEASE.md
  - README.md
status: done
merged_as: "2bee3850a68eea0133005aed916f2de3288a9c24"
---
# Autonomous Execution Contract

Make discovery and operator instructions describe the autonomous host that
both transports actually run.

**Steps:**

1. Publish `fsm.executor/2` at the existing resource URI with embedded
   `progress: "autonomous"`; retain the other modes, sanitized handler
   contracts, unknown external-executor status, and null handler behavior.
2. Specify the resource format transition and release compatibility in
   SPEC and API-POLICY; explicitly reject an unknown format in the repository's
   discovery-driven client fixture rather than interpreting it as v1.
3. Reconcile initialize instructions, prompts, tool descriptions, README,
   and EMBEDDING so no current instruction requires requests or subscription
   to advance execution, including recovery; keep historical plan records intact.
4. Document host versus session lifetime for stdio and HTTP, admission and
   output bounds, observable overload behavior, clock behavior, and the
   process-lifecycle limits supplied by plan 0022.
5. Refresh affected goldens from SPEC, add release notes naming the behavior
   and wire-format changes, and make the neutral discovery-driven workflow
   fixture submit once then observe autonomous completion on each transport.

**Tests:**

- `cargo test -p fsm-cli --test mcp_executor --test mcp_execute_workflow`:
  both production transports report the new format only with the actual
  mode/progress semantics; handler metadata still omits command and template
  literals, and discovery-derived workflows complete without driver polls.
- An unsupported format is rejected explicitly by the fixture client;
  read-only and degraded discovery never advertise an active executor.
- `cargo test -p fsm-cli --test executor_doc --test transport_doc` and
  affected MCP goldens agree with SPEC, including bounds and session lifetime.
- The complete plan's real-process tests pass against the integrated plan
  0022 lifecycle implementation; record native OS/toolchain axes before release.

- **Done when:** discovery, current user documentation, release notes, and production tests agree on autonomous stdio/HTTP execution and its versioned lifecycle contract, with the stable host gate green and no unrecorded integration prerequisite.

Frozen acceptance:

All five steps pass against the reviewed `32dc8711..2bee3850` range:
truthful versioned discovery, explicit unknown-format refusal, current
operator guidance, bounded transport/host lifetime contracts and quiet
stdio/HTTP workflow execution. Complete CI 37969821437 passes all six
Linux/macOS/Windows stable/MSRV gates, both Linux/systemd native jobs and
zero-dependency acceptance at runtime source `08c059f5`; downstream embed
acceptance passes in debug and release. Native reports verify original
ownership, provisioning, shutdown, recovery and upgrade scenarios.
The documentation-only successor `2bee3850` passes 26 focused guide checks,
14 resource checks and actual stdio resource responses matching the current
SPEC and EMBEDDING bytes; embedded document bytes differ from the gated
binary, with no runtime-source change or executable byte comparison claimed.
Final self-review found no unresolved task-scope defect; no independent
reviewer or plan 0023 release/live-model acceptance is claimed.
Task-cache verdict SHA-256: `bb797aa3d548751284cc59866a45c6ef740a3c81e04dc829cdafc519cdb0b361`.
