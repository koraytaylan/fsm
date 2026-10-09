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
status: in_progress
merged_as: ""
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

Focused progress:

- The discovery-driven fixture now accepts only the supported executor resource
  formats before interpreting mode or handler fields, explicitly refusing unknown,
  missing and non-string formats. Its named regression, 28 executor/document
  cases, CLI all-target clippy and format/size/diff checks pass. Current operator
  guidance still contains stale inline-execution and pending HTTP-egress claims;
  reconciliation and final plan integration gates remain outstanding.
- README, execution-mode guidance, HTTP session guidance, SPEC, API-POLICY,
  release notes and the ServeMode Rust documentation now describe supported
  production stdio/HTTP autonomous ownership, server/session lifetimes and
  implemented asynchronous HTTP output. Review removed superseded incomplete
  HTTP ownership clauses; borrowed helpers remain explicitly request-driven.
  A transport-document regression and 29 executor/document cases pass with CLI
  all-target clippy and format/size/diff checks. The complete public-contract
  audit and frozen plan-end integration gates remain outstanding.
- The follow-up audit reconciled duplicated native-owner, hosted-method and
  protocol-read paragraphs across SPEC, API-POLICY, EMBEDDING and RELEASE,
  replacing obsolete production-selection and lifecycle-route claims with the
  implemented transport contracts. Thirty focused discovery/document checks
  and format/size/diff pass; older recovery clauses still require source-backed
  review before the final plan freeze, and full gates have not started.
- Source-backed review confirms guarded production launch in native_owners,
  original-run reconciliation in service/reconciliation and authenticated
  event-only recovery in native_handoffs. Superseded incomplete/provisional
  recovery clauses now retain those implemented identity and authority checks;
  crash-matrix references distinguish the completed plan 0022 range from the
  upcoming plan 0020 integration gate. Thirty focused contract checks pass;
  final documentation consistency and integration gates remain due.
