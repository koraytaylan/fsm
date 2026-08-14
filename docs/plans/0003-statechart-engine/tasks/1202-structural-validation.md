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

1. Author `crates/fsm-core/tests/fixtures/machines/invalid/` first: one minimal machine per rule in the architecture table (`def/dup_name`, `def/one_initial`, `def/initial_not_child`, `def/initial_is_history`, `def/unknown_state`, `def/unknown_event`, `def/unknown_effect`, `def/unknown_enum`, `def/terminal_not_leaf`, `def/terminal_has_transitions`, `def/initial_terminal`, `def/multiple_history`, `def/from_history`, `def/history_target_from_inside`, `def/reserved_ident`, `def/not_supported`, and representative `def/limit_*` cases), named after its expected code; plus `crates/fsm-core/tests/spec_validate.rs` asserting each fixture yields exactly its code.
2. Implement `validate(spec) -> Result<(), Vec<Finding>>` in `crates/fsm-core/src/spec.rs` covering the full architecture table, every finding carrying path and mechanically generated hint.
3. Assert the reference `case_review.json` validates cleanly.

- **Done when:** every invalid fixture yields exactly its named `def/*` code and `case_review.json` passes under `cargo test -p fsm-core --test spec_validate`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
