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
status: done
merged_as: ""
---
# Transition Selection

Candidate scan is the determinism heart: transitions collect along the ancestor chain innermost-first in document order, guards evaluate against the pre-transition context and event only, the first true guard wins, and the `run/unhandled` versus `run/not_enabled` distinction tells the caller whether to fix the definition or the payload.

**Steps:**

1. Author `crates/fsm-core/tests/select_golden.rs` first, encoding exactly the inventory under **Tests**.
2. Implement event validation (`validate_event`) and the selection stage of `step()` in `crates/fsm-core/src/step.rs` per architecture: chain-ordered candidate collection from `transitions_by`, guard evaluation under the shared budget with `run/guard_error` aborting loudly (never treat-as-false), first-true-wins, `not_considered` labels for candidates after the winner, and the empty-candidates path honoring `on_unhandled`.

**Tests:**

- `validate_event` cases, one per code: an undeclared event name → `req/event_unknown`; a declared field missing → `req/field_missing`; an extra field → `req/field_unknown`; a raw JSON number where a decimal string belongs → `req/number_token`; a string where an `int` belongs → `req/field_type`; a decimal with more fraction digits than declared → `req/field_scale` (and one fewer-digits case that widens exactly and passes).
- Child-first override (hand-built machine: child and ancestor both declare event `e`): from the child leaf, the child's transition wins — winner `source_state` asserted; from a sibling leaf under the same ancestor without its own handler, the ancestor's fires.
- Document order within one source: two guarded transitions where the first guard is true → first wins and the second is traced `not_considered`; first false, second true → second wins with the first's guard trace showing `false`.
- The distinction, exact codes asserted: `case_review` in `docs_review` receiving `scored` (no handler anywhere on `[docs_review, in_review]`) → `run/unhandled`; a hand-built machine whose only matching transition has a false guard → `run/not_enabled` with the per-chain-level guard trace present.
- `on_unhandled: ignore` (hand-built): the same no-candidate situation yields the `Ignored` outcome instead of a rejection.
- Guard evaluation error: a guard that overflows on the given bindings → `run/guard_error` carrying source state, transition index, and span — never treated as false, later candidates never evaluated.
- Pre-transition-only evaluation pinned: guards see the incoming `ctx` (a guard reading a variable that the would-be transition's own actions modify still sees the old value — constructed case).

- **Done when:** the selection table tests pass — every `req/*` validation code, child-first override, document order, `run/unhandled` vs `run/not_enabled`, ignore mode, and the guard-error abort — under `cargo test -p fsm-core --test select_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
