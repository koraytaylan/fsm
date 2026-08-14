---
id: machine-spec-parse
title: "Machine Spec Parse"
workstream: "0012"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/src/lib.rs
  - crates/fsm-core/src/spec.rs
  - crates/fsm-core/src/tree.rs
  - crates/fsm-core/src/machine.rs
  - crates/fsm-core/src/step.rs
  - crates/fsm-core/src/trace.rs
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/src/simulate.rs
  - crates/fsm-core/src/hashes.rs
  - crates/fsm-core/tests/spec_parse.rs
  - "crates/fsm-core/tests/fixtures/machines/**"
status: planned
merged_as: ""
---
# Machine Spec Parse

The `fsm.machine/1` JSON format — recursive state tree, flat transition array, entry/exit blocks, history pseudo-children — parses into a typed model with JSON-Pointer error paths; as the plan's first task it also wires all engine modules into `lib.rs` so no later task touches it again.

**Steps:**

1. Add `pub mod tree; pub mod spec; pub mod machine; pub mod step; pub mod trace; pub mod analyze; pub mod simulate; pub mod hashes;` to `crates/fsm-core/src/lib.rs` and create the corresponding empty stub files.
2. Commit the reference fixture `crates/fsm-core/tests/fixtures/machines/case_review.json` first, verbatim from architecture (compound `in_review` with entry/exit blocks and a deep-history child, ancestor-sourced and internal transitions, an enforced invariant), plus small malformed variants for each shape-error case.
3. Implement the typed model (`MachineSpec`, `StateNode`, `Block`, `TransitionSpec`, `InvariantSpec`, …) and `parse_machine(v: &Value) -> Result<MachineSpec, Vec<Finding>>` in `crates/fsm-core/src/spec.rs` with `def/unknown_key`, `def/shape`, and `req/number_token` errors carrying JSON-Pointer paths.
4. Add `crates/fsm-core/tests/spec_parse.rs` asserting the reference fixture parses into the expected model shape and every malformed variant yields its expected code and path.

- **Done when:** `case_review.json` parses and every malformed fixture yields its expected `def/*` or `req/*` code and JSON-Pointer path under `cargo test -p fsm-core --test spec_parse`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
