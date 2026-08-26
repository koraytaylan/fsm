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
status: done
merged_as: ""
---
# Typechecker

Static typing is where "no implicit rounding" and "no mixed-class arithmetic" are actually enforced: every expression is checked at definition time against declared context/event types, with scope flags for invariant and entry/exit contexts and Levenshtein suggestions for unknown identifiers.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/typeck.jsonl` and `crates/fsm-core/tests/expr_typeck.rs` first, encoding exactly the inventory under **Tests** (each line declares its scope inline and maps a source to either a type rendering or an error code).
2. Implement `Ty`, `ScopeKind`, `Scope`, `TypeWarning`, and `typecheck(e, scope) -> Result<(Ty, Expr, Vec<TypeWarning>), ExprError>` in `crates/fsm-core/src/expr/typeck.rs` following the architecture typing table exactly, including exact `Dec` scale arithmetic, `Ts`/`Dur` algebra, enum equality-only comparison, `if`-branch unification with exact `Dec` widening, and builtin calls uniformly rejected as `expr/unknown_builtin` (listing the seven legal names) until the builtins task lands signatures. The returned `Expr` is annotated with compile-time decimal `if` scales.
3. Implement `suggest(name, candidates)` (Levenshtein distance ≤ 2) in `crates/fsm-core/src/ident.rs` and wire it into the unknown-identifier hints together with the full legal list.

**Tests:**

- The architecture's worked examples as named `typeck.jsonl` lines with hint content asserted: `ctx.total + 1` with `total: decimal(2)` → `expr/mixed_class`, hint containing both fixes (`1.00`-style literal and `dec(1, 2)`); `if evt.express then 2.50 else 1.0` → `decimal(2)` (exact widening); `if evt.express then 2.50 else 1` → `expr/type_mismatch`.
- One line per typing-table row family, expected type rendered exactly: `Int+Int → int`; `Dec(1)+Dec(2) → decimal(2)`; `Ts+Dur → timestamp` and `Dur+Ts → timestamp`; `Ts−Ts → duration`; `Ts−Dur → timestamp`; `Dur−Dur → duration`; `Int×Dec(2) → decimal(2)` and `Dec(2)×Int → decimal(2)`; `Dec(7)×Dec(6)` → `expr/scale_cap`; `Dur×Int → duration`; unary `-` on `Str` → `expr/type_mismatch`; `Str+Str` → `expr/type_mismatch`.
- Comparisons: `1.5 == 1.50` → `bool` (cross-scale Dec comparison legal); `ctx.a < ctx.b` for `Int`/`Dec`/`Ts`/`Dur` scopes → `bool`; `Str < Str` and `Enum < Enum` → `expr/cmp_unordered`; `Enum(Risk) == Enum(Risk)` → `bool`; enums of two different declared types compared → `expr/type_mismatch`; `Bool and Int` → `expr/type_mismatch`; a non-`Bool` `if` condition → `expr/type_mismatch`.
- Scope flags: an `evt` reference under `ScopeKind::Invariant` → `expr/evt_in_invariant`; under `ScopeKind::Block` → `expr/evt_in_block`; the same expression under `Guard` → well-typed.
- Unknown identifiers with suggestion content asserted: `ctx.limit_nam` with declared `limit` → `expr/unknown_var`, hint containing the suggestion `limit` *and* the full legal list; `evt.amonut` → `expr/unknown_field`; `Rsk.low` → `expr/unknown_enum`; `Risk.lo` → `expr/unknown_variant`; any call, e.g. `round(ctx.r, 2, half_even)` → `expr/unknown_builtin` listing all seven names (signatures land in the builtins task).
- Coverage assertion in `expr_typeck.rs`: every code in the list (`expr/type_mismatch`, `expr/mixed_class`, `expr/scale_cap`, `expr/unknown_var`, `expr/unknown_field`, `expr/unknown_enum`, `expr/unknown_variant`, `expr/unknown_builtin`, `expr/cmp_unordered`, `expr/evt_in_invariant`, `expr/evt_in_block`) appears in at least one fixture line — the test fails if a code is unexercised.
- Inline in `ident.rs`: `suggest` returns the distance-1 candidate; returns `None` when the best distance is 3; deterministic tie-break (first of equally distant candidates in iteration order) pinned.

- **Done when:** every line of `typeck.jsonl` holds and the code-coverage assertion passes under `cargo test -p fsm-core --test expr_typeck`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
