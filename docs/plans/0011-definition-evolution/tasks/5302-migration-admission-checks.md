---
id: migration-admission-checks
title: "Migration Admission Checks"
workstream: "0053"
kind: task
depends_on:
  - supersedes-declaration
gated: false
touches:
  - crates/fsm-core/src/migrate/mod.rs
  - crates/fsm-core/src/migrate/validate.rs
  - crates/fsm-core/src/lib.rs
  - crates/fsm-store/src/store/lifecycle.rs
  - crates/fsm-store/tests/migration_admission.rs
  - crates/fsm-core/src/lib.rs
  - crates/fsm-store/tests/migration_admission.rs
  - crates/fsm-cli/tests/naive_caller/*.rs
status: done
merged_as: ""
---
# Migration Admission Checks

An operator should learn their mapping is wrong when they write it, not when they try to move a live workflow with it — so every check that needs both definitions runs at `define_machine`, before a single instance is at risk.

**Steps:**

1. Create `crates/fsm-core/src/migrate/mod.rs` declaring `pub mod validate;` and, for later tasks, `pub mod apply; pub mod carryover; pub mod preview;` as empty module stubs. Add `pub mod migrate;` to `crates/fsm-core/src/lib.rs`.
2. In `crates/fsm-core/src/migrate/validate.rs`, implement `pub fn validate_supersedes(old: &CompiledMachine, t_old: &Tree, new: &CompiledMachine, t_new: &Tree) -> Vec<Finding>` covering the eight catalogue-dependent rules from architecture §0053:
   - `def/supersedes_unknown_state` — a `states` key is not a state of the old machine, or a value is not a state of the new one;
   - `def/supersedes_target_not_leaf` — a value names a compound or a history pseudostate, when an active configuration only ever holds leaves;
   - `def/supersedes_target_terminal` — a value names a `terminal` or `final` state, which would complete an instance by migrating it;
   - `def/supersedes_region` — the two machines disagree on shape (one sequential, one parallel) or on their region-name set, because region topology is not mappable and this plan does not pretend otherwise;
   - `def/supersedes_ctx_unknown` — a `context` key is not a variable of the new machine, or an expression names a variable the old machine does not declare;
   - `def/supersedes_ctx_type` — an expression's type differs from the new variable's declared type, scale included;
   - `def/supersedes_slot` — an invoke slot in the old machine has no counterpart in the new one.
3. Type-check each `context` expression with the **old** machine's declared context in scope and the **new** machine's variable as the assignment target, reusing the existing `def/assign_type`-style exact-type machinery rather than a second comparison.
4. In `crates/fsm-store/src/store/lifecycle.rs::define_machine_on`, resolve the superseded machine from the catalogue and run `validate_supersedes`. A machine the store does not hold is `def/supersedes_unknown_machine` — refuse the definition rather than accepting it and failing later, because a definition that cannot be checked cannot be trusted.
5. Report every finding through the existing `Finding` → `ErrorObj::from_findings` path so `machine_create`, `fsm validate`, and the MCP surface all surface them by the routes they already use.
6. Leave the run-time refusals (`req/migrate_*`) to `5401`. Admission answers "is this mapping coherent", not "can this particular instance move".

**Tests:**

- `crates/fsm-store/tests/migration_admission.rs`: a well-formed pair defines cleanly.
- Defining a machine whose `supersedes.machine` is not in the store reports `def/supersedes_unknown_machine` and writes **no** record.
- A `states` key naming an absent old state, and a value naming an absent new state, each report `def/supersedes_unknown_state`.
- A value naming a compound reports `def/supersedes_target_not_leaf`; a value naming a history pseudostate reports the same.
- A value naming a terminal state and one naming a `final` state each report `def/supersedes_target_terminal`.
- A sequential machine superseding a parallel one reports `def/supersedes_region`; so does a pair whose region names differ.
- A `context` expression naming an absent old variable reports `def/supersedes_ctx_unknown`; a decimal-scale mismatch reports `def/supersedes_ctx_type`.
- An old machine with an invoke slot absent from the new machine reports `def/supersedes_slot`.
- Findings are reported in a stable order across two runs, and a definition without `supersedes` produces byte-identical findings to the pre-change behaviour.

- **Done when:** `cargo test -p fsm-store --test migration_admission` covers all eight rules plus the accepted pair, an unresolvable supersede is refused at definition time with no record written, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.

**Landed:** `crates/fsm-core/src/migrate/` with `validate.rs` implementing all eight catalogue-dependent rules in a stable order (region shape, then the state mapping in document order, then the context mapping, then slots) and empty stubs for the three modules later tasks fill; the admission hook in `define_machine_on` that resolves the superseded machine from the same catalogue the invoke rules use and refuses `def/supersedes_unknown_machine` when it is absent; and `migration_admission.rs` covering every rule plus the accepted pair, each asserting that a refused definition writes no record.

**Corrections.** (1) An expression naming an unknown variable surfaces as `expr/unknown_var` from the type checker, not `expr/unknown_field`; both are mapped onto `def/supersedes_ctx_unknown`, because the operator's mistake is in the mapping and should read as one finding rather than as an expression bug they did not write. (2) The plan's step 3 says to reuse the `def/assign_type` machinery; the reusable half is `typecheck` against a `Scope`, which is what this does — the comparison itself is one exact-equality check, and a second code would have been a second vocabulary for the same mistake.
