---
id: transition-selection
title: "Transition Selection"
workstream: "0014"
kind: task
depends_on:
  - tree-tables
gated: false
touches:
  - crates/fsm-core/src/step.rs
  - crates/fsm-core/tests/select_golden.rs
status: planned
merged_as: ""
---
# Transition Selection

Candidate scan is the determinism heart: transitions collect along the ancestor chain innermost-first in document order, guards evaluate against the pre-transition context and event only, the first true guard wins, and the `run/unhandled` versus `run/not_enabled` distinction tells the caller whether to fix the definition or the payload.

**Steps:**

1. Implement event validation (`validate_event` with `req/event_unknown`, `req/field_missing`, `req/field_unknown`, `req/number_token`, `req/field_type`, `req/field_scale`) and the selection stage of `step()` in `crates/fsm-core/src/step.rs` per architecture: chain-ordered candidate collection from `transitions_by`, guard evaluation under the shared budget with `run/guard_error` aborting loudly (never treat-as-false), first-true-wins, `not_considered` labels for candidates after the winner, and the empty-candidates path honoring `on_unhandled`.
2. Add `crates/fsm-core/tests/select_golden.rs`: table-driven cases over `case_review` and hand-built machines asserting child-first override (a child transition beats an ancestor's for the same event), document order within a source, exact `run/unhandled` vs `run/not_enabled` outcomes, guard-error abort with source/index/span, and per-chain-level trace grouping.

- **Done when:** the selection table tests pass, including the child-first-override and unhandled-versus-not-enabled cases, under `cargo test -p fsm-core --test select_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
