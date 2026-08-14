---
id: structural-validation
title: "Structural Validation"
workstream: "0012"
kind: task
depends_on:
  - machine-spec-parse
gated: false
touches:
  - crates/fsm-core/src/spec.rs
  - crates/fsm-core/tests/spec_validate.rs
  - "crates/fsm-core/tests/fixtures/machines/invalid/**"
status: planned
merged_as: ""
---
# Structural Validation

Every structural rule of the machine format — one initial child per compound, one global name namespace, leaf-only terminals, the four history rules, reserved identifiers, rejected reserved keys, and all size limits — lands with its own `def/*` code, pinned by one invalid fixture per rule authored first.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/machines/invalid/` (one minimal machine per rule, named after its expected code) and `crates/fsm-core/tests/spec_validate.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `validate(spec) -> Result<(), Vec<Finding>>` in `crates/fsm-core/src/spec.rs` covering the full architecture table, every finding carrying path and mechanically generated hint.

**Tests:**

- One fixture per structural rule, each asserted to yield exactly its named code (and nothing else) by `spec_validate.rs`: `def/dup_name` (a state and a history pseudostate sharing a name); `def/one_initial` (a compound without `initial`); `def/initial_not_child` (`initial` naming a grandchild); `def/initial_is_history`; `def/unknown_state` (a transition `to` naming nothing); `def/unknown_event`; `def/unknown_effect`; `def/unknown_enum`; `def/terminal_not_leaf` (a terminal compound); `def/terminal_has_transitions` (a terminal leaf as `from`); `def/initial_terminal` (creation chain landing terminal); `def/multiple_history` (two history pseudostates in one compound); `def/from_history`; `def/history_target_from_inside` (a transition inside the owner targeting its history pseudostate); `def/reserved_ident` (a `$`-prefixed event name); `def/not_supported` for a `regions` key and a second fixture for `deadlines`.
- Representative `def/limit_*` fixtures: 257 state nodes; nesting depth 13; 33 sets in one block; a 4,097-byte guard source (routed to the expression limit at compile time is *not* this task — the structural size limit fixture here is the definition-size cap, exercised with a generated >256 KiB document constructed in-test rather than committed).
- Finding quality asserted on three representative fixtures: `path` points at the offending element (exact pointer compared) and `hint` is non-empty and names the violated rule.
- The reference `case_review.json` validates cleanly (empty finding list).
- `spec_validate.rs` mechanics: the fixture directory iterates with filename-encoded expectations; an unknown filename pattern fails the run.

- **Done when:** every invalid fixture yields exactly its named `def/*` code and `case_review.json` passes under `cargo test -p fsm-core --test spec_validate`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
