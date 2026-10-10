---
id: executor-contract-acceptance
title: "Executor Contract Acceptance"
workstream: "0092"
kind: task
depends_on:
  - executor-contract-admission
  - executor-contract-cli
  - executor-contract-mcp
gated: false
touches:
  - .github/workflows/ci.yml
  - crates/fsm-cli/tests/executor_contract_acceptance.rs
  - crates/fsm-cli/tests/fixtures/contract/
  - crates/fsm-cli/tests/mcp_execute_workflow.rs
  - crates/fsm-cli/tests/executor_contract_mcp.rs
  - crates/fsm-cli/tests/contract_mcp/native.rs
  - crates/fsm-cli/tests/contract_mcp/native/staged.rs
  - crates/fsm-execute/src/containment/workflow_native_tests.rs
  - crates/fsm-execute/tests/lifecycle_platform/workflow_probe.py
  - crates/fsm-execute/tests/lifecycle_platform/test_workflow_producer.py
  - examples/
  - README.md
  - docs/EXAMPLES.md
  - docs/EMBEDDING.md
  - docs/API-POLICY.md
  - docs/RELEASE.md
status: planned
merged_as: ""
---
# Executor Contract Acceptance

Agreement between two wrappers proves little when both merely repeat an analyzer's mistake, so the acceptance cases also observe whether a real external operation occurred.

**Steps:**

1. Commit independently authored machine/table pairs and expected reports covering a generic staged workflow with preconditions, work, and recovery; include an incompatibility in a late recovery path whose presence must prevent the first external operation, plus a repaired pair that completes.
2. Drive those fixtures through offline CLI, stored-machine CLI, MCP draft checking, standalone execution, and embedded execution; normalize only documented transport envelopes, not finding order, status, paths, or contract identity.
3. Use real process and MCP marker handlers as an independent side-effect oracle: an invalid/unknown contract yields zero starts, repair permits ordered work, and a no-outcome handler or explicit manual effect preserves the documented pending/ack behavior instead of being rejected to make the test simpler.
4. Extend the existing workflow acceptance where it shares these production paths rather than duplicating its full scenario matrix; include a changed loaded table and a changed receiving definition to prove a successful preflight report is not a reusable execution bypass.
5. Update the operator example and README path to discover/check/repair/run, explain compatible versus progress-unknown versus unknown-contract, and document the explicit manual-policy upgrade, command exit codes, new report/tool versions, and unaffected persistence formats.
6. Record applicable debug/release, lint, docs, zero-dependency, and native-platform evidence in the release procedure; identify the live-model acceptance evidence as supplied by the overarching acceptance work rather than claiming a canned protocol client is a live model.

**Tests:**

- `cargo test -p fsm-cli --test executor_contract_acceptance` and the affected `mcp_execute_workflow` target exercise the documented commands and actual protocol, with fixtures and expected reports written from SPEC before implementation.
- All surfaces agree on known invalid, unknown evidence, explicit manual, deliberate no-outcome, and compatible repaired fixtures; offline unresolved composition is deliberately unknown where a complete store catalogue supplies proof.
- Both real handler kinds create no marker before invalid admission and create the expected marker/order after repair; disabling only the production admission check fails the side-effect test even if every diagnostic-only test still passes.
- A stale good report cannot admit a changed table or migrated receiving definition; pending effects and journal verification remain correct through refusal, repair, restart, and successful completion.
- Every documented new example parses and runs under the repository's normal acceptance approach, compatibility changes have explicit release-note coverage, and no text describes result mapping, guard satisfiability, or dynamic-recipient validation as implemented.

- **Done when:** the independently specified cross-surface acceptance suite proves both diagnostic parity and the presence/absence of real side effects, the documented repair/upgrade path is executable, and the complete applicable CONTRIBUTING gate passes with unexecuted release environments explicitly recorded.
