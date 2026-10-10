---
id: executor-contract-mcp
title: "Executor Contract MCP Draft Check"
workstream: "0092"
kind: task
depends_on:
  - executor-contract-outcomes
  - executor-contract-admission
gated: false
touches:
  - .github/workflows/ci.yml
  - crates/fsm-cli/src/mcp/tools/mod.rs
  - crates/fsm-cli/src/mcp/tools/dispatch.rs
  - crates/fsm-cli/src/mcp/tools/validate.rs
  - crates/fsm-cli/src/mcp/tools/schema_in.rs
  - crates/fsm-cli/src/mcp/tools/schema_out.rs
  - crates/fsm-cli/src/mcp/tools/handlers/mod.rs
  - crates/fsm-cli/src/mcp/tools/handlers/executor.rs
  - crates/fsm-cli/src/mcp/tools/handlers/executor/
  - crates/fsm-cli/src/mcp/mod.rs
  - crates/fsm-cli/src/mcp/methods.rs
  - crates/fsm-cli/src/mcp/serve.rs
  - crates/fsm-cli/src/mcp/host/
  - crates/fsm-cli/src/mcp/executor.rs
  - crates/fsm-cli/src/mcp/descriptions.rs
  - crates/fsm-cli/src/mcp/prompts.rs
  - crates/fsm-cli/src/mcp/resources.rs
  - crates/fsm-cli/tests/executor_contract_mcp.rs
  - crates/fsm-cli/tests/contract_mcp/
  - crates/fsm-execute/src/containment/workflow_native_tests.rs
  - crates/fsm-execute/tests/lifecycle_platform/cli_artifact.py
  - crates/fsm-execute/tests/lifecycle_platform/test_cli_artifact.py
  - crates/fsm-execute/tests/lifecycle_platform/workflow_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/test_workflow_producer.py
  - crates/fsm-cli/tests/tool_schemas.rs
  - crates/fsm-cli/tests/mcp_affordance_golden.rs
  - crates/fsm-cli/tests/mcp_full.rs
  - crates/fsm-cli/tests/fixtures/contract/
  - crates/fsm-cli/tests/fixtures/transcripts/
  - crates/fsm-cli/tests/fixtures/mcp_live/
  - crates/fsm-cli/tests/fixtures/mcp_affordance/
  - docs/SPEC.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Executor Contract MCP Draft Check

A model should be able to repair an executor mismatch before creating the machine that contains it.

**Steps:**

1. Add read-only `executor_check` with closed input/output schemas, exactly one of `spec` or `machine`, and the standard versioned structural report; validation of malformed arguments follows the existing MCP error conventions.
2. Pass an immutable view of the active execution host's loaded table into the tool through the session's host handle, never a global or client-supplied executable configuration; sessions sharing one HTTP host share its table and distinct hosts remain isolated; check a draft without calling machine creation, and resolve stored definitions/catalogues through read-only operations.
3. Report unknown active-execution compatibility in writer-only, read-only, and degraded modes with explicit provenance and independently available draft findings; another process's possible executor is neither an inferred table nor evidence of compatibility.
4. Update discovery guidance, tool descriptions, annotations, prompts, and schemas together so clients discover, check, repair, then create; do not require a client-issued approval token or allow one to bypass service admission when state or configuration changes.
5. Preserve disclosure boundaries in reports, codes, hints, structured content, and logged errors; the tool cannot reveal private argv/MCP literals or accept handler overrides, and no clock, poll, journal append, definition, instance, or subprocess is caused by the check itself.
6. Keep the capability compatible with plan 0020's autonomous service ownership and `fsm.executor/2` discovery with `progress: "autonomous"`; the separate `fsm.executor-check/1` analysis format does not redefine that scheduling enum; independent existing workflow progress may continue, but the check's own operations remain read-only and its result uses the host's actual loaded contract.

**Tests:**

- `cargo test -p fsm-cli --test executor_contract_mcp`: a real stdio client initializes, discovers the tool/resource, checks an unregistered invalid draft, follows the diagnostic to repair it, checks again, then creates it and observes execution through the actual embedded driver.
- The invalid check creates no machine, instance, request-id entry, journal mutation, or marker subprocess; checking an existing machine also has no attributable writes, and a draft-only session without other pending work remains byte-for-byte unchanged.
- Writer/read-only/degraded sessions report unknown without borrowed or invented table metadata; embedded fallback to read-only never retains stale compatibility authority, sessions attached to one host agree on its table, and sessions on distinct hosts with different loaded tables cannot contaminate each other's results.
- Discovery continues to publish `fsm.executor/2` with autonomous embedded progress while draft checks use the separate analysis format, and quiet clients do not prevent execution after a successful check and trigger.
- Skipping a check, reusing a report after configuration change, or creating an unchecked invalid draft never bypasses the shared production admission guard; outcome/manual distinctions match the core report contract.
- Golden schemas reject unknown inputs, both/neither selectors, and any attempted handler/path override; annotations accurately describe the check's read-only behavior, and every documented result satisfies the committed output schema.
- Sentinels in private executable paths, fixed argv, and nested MCP literals appear nowhere in tool results or failures; formatted public reports equal the same independent analyzer fixtures.

- **Done when:** the real MCP draft/repair workflow and every mode, authority, schema, non-disclosure, and bypass case in `executor_contract_mcp` passes, the tool performs no attributable writes, and the applicable CONTRIBUTING gate succeeds.
