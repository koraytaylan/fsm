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
status: done
merged_as: ""
---
# Expression Binding

Compilation binds every guard, action, emit argument, and invariant through the expression pipeline with the correct scope, enforces exact assignment typing (scale included — the machine-level "no implicit rounding" gate), and produces the indexed `CompiledMachine` the engine executes.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/machines/binding/` (one minimal machine per violation, named after its expected code) and `crates/fsm-core/tests/compile_machine.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `compile(spec) -> Result<CompiledMachine, Vec<Finding>>` in `crates/fsm-core/src/spec.rs`: typecheck guards and transition blocks with `ctx` + the transition's event fields, entry/exit blocks and invariants with `ctx` only, and emit args against declared effect field types; enforce `def/assign_type` and `def/dup_set` per block.
3. Define `CompiledMachine { machine_id, spec, canonical, transitions_by, compiled_exprs }` in `crates/fsm-core/src/machine.rs` with document-ordered `transitions_by: BTreeMap<(String, String), Vec<usize>>`.

**Tests:**

- One binding fixture per code, each asserted by `compile_machine.rs`: `def/assign_type` (a `set` of a `decimal(2)` target from a `decimal(3)` expression — scale mismatch is a type error); `def/dup_set` (two sets of one target within one block); `expr/evt_in_block` (an `evt` reference in an entry block); `expr/evt_in_invariant`; an emit argument whose type fails its declared effect field → `expr/type_mismatch` with the pointer into the emit; a guard referencing a field of a *different* event than the transition's own `on` → `expr/unknown_field` (guards see only their transition's payload).
- Scope-correct acceptance: a machine whose guard uses `evt.*`, whose entry block uses `ctx.*` only, and whose invariant uses `ctx.*` only compiles cleanly.
- `case_review.json` compiles, and its index is asserted as the exact map: `(intake, docs_ok) → [0]`; `(docs_review, docs_ok) → [1]`; `(risk_review, scored) → [2, 3]` (document order preserved — the guarded transition first); `(in_review, note_added) → [4]`; `(in_review, withdraw) → [5]`; `(in_review, suspend) → [6]`; `(suspended, resume) → [7]`; no other keys.
- Compiled-expression bookkeeping, inline: every compiled expression retains its verbatim source (spans render against it) and its inferred type; the reference machine's guard `evt.score >= 700` carries type `bool`.

- **Done when:** every binding fixture yields exactly its named code and `case_review.json` compiles with the exact transition index map under `cargo test -p fsm-core --test compile_machine`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
