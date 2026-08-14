---
id: typechecker
title: "Typechecker"
workstream: "0008"
kind: task
depends_on:
  - parser
gated: false
touches:
  - crates/fsm-core/src/expr/typeck.rs
  - crates/fsm-core/src/ident.rs
  - crates/fsm-core/tests/expr_typeck.rs
  - crates/fsm-core/tests/fixtures/expr/typeck.jsonl
status: planned
merged_as: ""
---
# Typechecker

Static typing is where "no implicit rounding" and "no mixed-class arithmetic" are actually enforced: every expression is checked at definition time against declared context/event types, with scope flags for invariant and entry/exit contexts and Levenshtein suggestions for unknown identifiers.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/typeck.jsonl` first: lines carrying an inline scope declaration plus source and either the expected type rendering or an expected error code, with at least one line per code (`expr/type_mismatch`, `expr/mixed_class` with its two-fix hint, `expr/scale_cap`, `expr/unknown_var`, `expr/unknown_field`, `expr/unknown_enum`, `expr/unknown_variant`, `expr/unknown_builtin`, `expr/cmp_unordered`, `expr/evt_in_invariant`, `expr/evt_in_block`); plus `crates/fsm-core/tests/expr_typeck.rs` asserting every line.
2. Implement `Ty`, `ScopeKind`, `Scope`, `TypeWarning`, and `typecheck(e, scope) -> Result<(Ty, Vec<TypeWarning>), ExprError>` in `crates/fsm-core/src/expr/typeck.rs` following the architecture typing table exactly, including exact `Dec` scale arithmetic, `Ts`/`Dur` algebra, enum equality-only comparison, `if`-branch unification with exact `Dec` widening, and builtin calls uniformly rejected as `expr/unknown_builtin` until the builtins task lands signatures.
3. Implement `suggest(name, candidates)` (Levenshtein distance ≤ 2) in `crates/fsm-core/src/ident.rs` and wire it into the unknown-identifier hints together with the full legal list.

- **Done when:** every line of `typeck.jsonl` holds under `cargo test -p fsm-core --test expr_typeck`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
