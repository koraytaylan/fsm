---
id: composition-spec-and-docs
title: "Composition Spec And Docs"
workstream: "0052"
kind: task
depends_on:
  - composition-chaos-harness
gated: false
touches:
  - docs/SPEC.md
  - docs/RELEASE.md
  - docs/EMBEDDING.md
  - docs/EXAMPLES.md
  - README.md
  - examples/order_lifecycle.json
  - examples/case_review_child.json
  - crates/fsm-cli/tests/spec_appendix.rs
status: planned
merged_as: ""
---
# Composition Spec And Docs

SPEC is the source of truth and every golden in this plan derives from its prose, so composition is not finished until the document is normative about it — including the two rulings a reader will otherwise assume were oversights.

**Steps:**

1. Add a `## Composition` section to `docs/SPEC.md` covering, normatively: the `invoke` declaration and the **MUST** that `machine` is a 64-hex `machine_id` rather than a name, with the reason; the derived child instance id including its exact domain string `fsm:child:1` and truncation; the `with` and `returns` projections and their typing; the two operations, their legality rules, and their records; the cascade and its single documented two-record window with the reconciliation that closes it; and the `$done.invoke.<slot>` payload.
2. Add a `### Signals` subsection stating the single-target **MUST**, the reason (a query-targeted delivery would match a different set on replay, so the store would stop being a function of its journal), the run-time typing rule against the target's declarations, the fire-and-forget rule, and the full outcome vocabulary of `signal_delivered`.
3. Extend `### Record kinds` with `instance_invoked`, `invocation_returned`, and `signal_delivered` and their exact body fields, and `## Format versions` with `fsm.state/3` and `fsm.snapshot/5`. State the migration rule already implemented by `4904`: records carry their own `state_format` and old hashes are never recomputed under a new format.
4. Extend `## Appendix A — Error codes` with the fourteen codes `4801` registered and `## Appendix B — Limits` with `MAX_INVOKES_PER_STATE`, `MAX_INVOKE_DEPTH`, and `MAX_SIGNALS_PER_BLOCK`, each noting it is deliberately absent from the genesis `limits` block.
5. Extend `docs/EMBEDDING.md` with the operator's view: the executor's three new directives, their derived keys, the fact that none of them spawns a subprocess, and how a composed workflow behaves in each of the three run modes.
6. Add the worked example: a `case_review_child.json` invoked by an extended `order_lifecycle.json`, documented in `docs/EXAMPLES.md` with the exact record sequence a full run produces — create, invoke, child events, return, parent advance. If `order_lifecycle.json`'s `machine_id` is pinned by a golden, add a sibling machine instead of editing it and say so in the commit message.
7. Add to `README.md` one guarantee row — *explicit composition: a child exists because a record says so, and its id is derivable from its parent's* — and one honest non-claim: composition is single-store and single-writer, and a signal reaches exactly one instance by design.
8. Add to `docs/RELEASE.md`: a **Manual acceptance** row for driving a parent-and-child workflow through a live MCP host, and a note under the compatibility section that this release moves the state format to `fsm.state/3` and the store to `VERSION` 9 — an operator upgrading needs to know their store will be migrated on first open, and the release notes are where they look.
9. Extend `crates/fsm-cli/tests/spec_appendix.rs` to assert every new record kind named in `record.rs` appears in SPEC's `### Record kinds` table, in both directions, so a record kind can never ship undocumented.

**Tests:**

- `cargo test -p fsm-cli --test spec_appendix` passes with the fourteen codes, three limits, and three record kinds documented, and now covers record kinds in both directions.
- `cargo test -p fsm-cli --test examples` replays the composed example end to end and asserts the exact record sequence.
- A documentation test asserts SPEC contains the domain string `fsm:child:1`, so the id scheme is pinned to prose and cannot drift silently.
- A documentation test asserts SPEC's signals subsection contains the single-target MUST.
- Every `exec/*` code in `fsm-execute`'s `ALL_CODES` appears in `docs/EMBEDDING.md`, extending the existing `executor_doc.rs` assertion to the two new codes.
- The banned-vocabulary scan in `crates/fsm-cli/tests/policy.rs` passes over the new prose and the new example machine.
- `docs/RELEASE.md` names both the composition acceptance pass and the `fsm.state/3` / `VERSION` 9 migration.

- **Done when:** SPEC is normative about invocation, signals, the new records, and both format bumps; EMBEDDING covers the executor's new directives; a composed example replays with its documented record sequence; `cargo test -p fsm-cli --test spec_appendix --test examples --test executor_doc --test policy` passes; and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
