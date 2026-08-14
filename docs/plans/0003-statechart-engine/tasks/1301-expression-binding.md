---
id: expression-binding
title: "Expression Binding"
workstream: "0013"
kind: task
depends_on:
  - structural-validation
gated: false
touches:
  - crates/fsm-core/src/spec.rs
  - crates/fsm-core/src/machine.rs
  - crates/fsm-core/tests/compile_machine.rs
  - "crates/fsm-core/tests/fixtures/machines/binding/**"
status: planned
merged_as: ""
---
# Expression Binding

Compilation binds every guard, action, emit argument, and invariant through the expression pipeline with the correct scope, enforces exact assignment typing (scale included — the machine-level "no implicit rounding" gate), and produces the indexed `CompiledMachine` the engine executes.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/machines/binding/` first: minimal machines each violating one rule — `def/assign_type` (scale mismatch on a set target), `def/dup_set` within a block, an `evt` reference in an entry block (`expr/evt_in_block`), an `evt` reference in an invariant (`expr/evt_in_invariant`), an emit argument failing its effect field type — named after the expected code; plus `crates/fsm-core/tests/compile_machine.rs`.
2. Implement `compile(spec) -> Result<CompiledMachine, Vec<Finding>>` in `crates/fsm-core/src/spec.rs`: typecheck guards and transition blocks with `ctx` + the transition's event fields, entry/exit blocks and invariants with `ctx` only, and emit args against declared effect field types; enforce `def/assign_type` and `def/dup_set` per block.
3. Define `CompiledMachine { machine_id, spec, canonical, transitions_by, compiled_exprs }` in `crates/fsm-core/src/machine.rs` with document-ordered `transitions_by: BTreeMap<(String, String), Vec<usize>>`.
4. Assert `case_review.json` compiles cleanly and its `transitions_by` index matches the expected (source, event) → indices map.

- **Done when:** every binding fixture yields exactly its named code and `case_review.json` compiles with the expected transition index under `cargo test -p fsm-core --test compile_machine`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
