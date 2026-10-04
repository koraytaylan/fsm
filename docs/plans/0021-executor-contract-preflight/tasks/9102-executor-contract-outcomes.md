---
id: executor-contract-outcomes
title: "Executor Contract Outcome Validation"
workstream: "0091"
kind: task
depends_on:
  - executor-contract-effects
gated: false
touches:
  - crates/fsm-execute/src/contract/mod.rs
  - crates/fsm-execute/src/contract/outcomes.rs
  - crates/fsm-execute/src/contract/effects.rs
  - crates/fsm-execute/src/contract/report.rs
  - crates/fsm-execute/tests/contract_outcomes.rs
  - crates/fsm-execute/tests/fixtures/contract/
  - crates/fsm-execute/tests/fixtures/public_surface.txt
  - docs/SPEC.md
  - docs/EMBEDDING.md
status: planned
merged_as: ""
---
# Executor Contract Outcome Validation

The handler can finish successfully before the store discovers that its configured success event is not a valid input.

**Steps:**

1. Extend the structural report with independently checked `on_ok` and `on_failed` dispositions for each actually emitted automatic effect, preserving an absent outcome as deliberate no-advance behavior and keeping progress observations separate from validity; unused handlers in a shared table do not impose outcomes on the checked machine.
2. Reuse `fsm_core::step::validate_event` for concrete payload rules and externally sendable event names; retain underlying typed causes when reporting unknown/internal/generated events, non-object payloads, missing/extra fields, number tokens, scalar types, decimal scale, or enum members.
3. Model stamps exactly as `Store::send_event_stamp_on`: supplied values win, only absent fields receive a timestamp string, and every filled field uses the same unknown timestamp; do not read a clock or test one favorable timestamp as if it proved the family of possible outputs.
4. Define and test the symbolic compatibility of generated signed-millisecond strings with each existing field parser; accept universally compatible representations, reject provably incompatible ones, and report unknown if a representation cannot be proved from the existing rules, without imposing a new `ts`-only stamping rule on legal handlers.
5. Retain static payload literal behavior including braces, preserve parser behavior for duplicate stamp names, and report unused or no-transition outcome events as uncertain progress rather than invent a structural prohibition.
6. Finish report aggregation rules so a known invalid outcome dominates missing evidence, a runtime guard does not masquerade as an unknown contract, and success/failure findings are stable and bounded under the same report limits.
7. Update the report contract, operator examples of typed fixes, and the provisional public inventory if the public API grows, without changing settle ordering, event enablement, or outcome data mapping.

**Tests:**

- `cargo test -p fsm-execute --test contract_outcomes`: handwritten success/failure matrices cover every event field type, typed string requirements, integer limits, exact decimal scale, enum membership, unknown/extra/missing fields, and reserved/internal event names.
- Absent outcomes, one-sided outcomes, a declared unused event, and payload-dependent guards preserve the compatibility/progress distinction; no test assumes a structurally valid event will necessarily fire a transition.
- Supplied stamped fields remain literal, absent stamped fields are checked symbolically, all timestamp edge values obey the promised accepted families, duplicate stamps follow parser rules, and invalid named stamp fields are diagnosed before execution.
- Braces in an outcome payload remain literal while the same syntax in an actual argv or MCP argument template is handled by the existing template rules.
- Compatible concrete fixtures agree with independently invoked production event validation, including a failing fixture whose expected diagnosis is handwritten; expected report bytes are never regenerated from analyzer behavior to make the test pass.
- Repeated analysis consumes no clock tick and changes no input objects; report limits and deterministic ordering hold when effect and both outcome findings combine.

- **Done when:** the production outcome checker passes the full `contract_outcomes` matrix and independent report goldens, proves only the stamp families and structural properties it reports, preserves deliberate no-advance behavior, and the applicable CONTRIBUTING gate succeeds.
