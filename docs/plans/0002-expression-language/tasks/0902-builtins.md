---
id: builtins
title: "Builtins"
workstream: "0009"
kind: task
depends_on:
  - evaluator
gated: false
touches:
  - crates/fsm-core/src/expr/typeck.rs
  - crates/fsm-core/src/expr/eval.rs
  - crates/fsm-core/tests/expr_builtins.rs
  - crates/fsm-core/tests/fixtures/expr/builtins.jsonl
status: planned
merged_as: ""
---
# Builtins

The seven builtins — `min max abs dec round div dur` — land their typing signatures and evaluation together, enforcing that scale arguments are integer literals and mode/unit arguments are literal words, so result types never depend on runtime values; this touches the typechecker and evaluator files sequentially after their owning tasks.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/builtins.jsonl` and `crates/fsm-core/tests/expr_builtins.rs` first, encoding exactly the inventory under **Tests**.
2. Extend `crates/fsm-core/src/expr/typeck.rs` with the seven signatures per the architecture table, resolving `Arg::Word` mode/unit arguments and emitting the `expr/round_widens` warning.
3. Extend `crates/fsm-core/src/expr/eval.rs` with the seven evaluations delegating decimal work to `crate::decimal` and checking `dur` multiplication.

**Tests:**

- Per-builtin lines in `builtins.jsonl` (each: a normal case, an edge case, an error case):
  - `min`/`max`: an `Int` pair; a cross-scale `Dec` pair (`min(1.4, 1.50)` → `1.40` at the widened scale); a `Ts` pair; mixed classes (`min(1, 1.0)`) → `expr/type_mismatch`.
  - `abs`: `Int` and `Dur` normal; `abs` of the minimum i64 → `run/overflow`; type-preservation asserted (`abs(-1.50)` → `decimal(2)`).
  - `dec`: `dec(1, 2)` → `1.00`; `dec(1.50, 4)` → `1.5000`; narrowing `dec(1.5000, 2)` → `expr/scale_narrow` with hint naming `round`; scale literal outside 0..=12 (`dec(ctx.x, 13)`) → `expr/scale_not_literal`.
  - `round`: the worked tie `round(2.345, 2, M)` across all seven modes (values per the decimal table — `half_even` → `2.34`); widening `round(1.5, 4, half_even)` → value `1.5000` *and* warning `expr/round_widens` asserted present; the architecture worked example `round(ctx.rate, ctx.places, half_even)` → `expr/scale_not_literal` with the hint explaining that types cannot depend on runtime values.
  - `div`: `div(1, 3, 4, half_even)` → `0.3333`; an exact division identical in all seven modes; `div(ctx.a, 0.00, 2, down)` → `run/div_zero`; a non-word mode argument (`div(1, 3, 2, ctx.m)`) → `expr/mode_invalid`; an unknown mode word (`half_evenn`) → `expr/mode_invalid` listing the seven legal modes.
  - `dur`: `dur(5, min)` → `300000` ms; `dur(0, d)` → `0`; overflow (`dur` of a near-max `Int` with unit `d`) → `run/overflow`; an unknown unit word → `expr/mode_invalid` listing `ms s min h d`.
- Arity and name errors: `min(ctx.a)` → `expr/arity` naming expected 2 / found 1; `clamp(ctx.a, 1, 2)` → `expr/unknown_builtin` listing the seven names.
- Coverage assertion in `expr_builtins.rs`: every code this task introduces (`expr/scale_narrow`, `expr/round_widens`, `expr/scale_not_literal`, `expr/mode_invalid`, `expr/arity`, `run/div_zero`) appears in at least one fixture line, and all seven builtins appear — the test fails on an unexercised code or builtin.

- **Done when:** every line of `builtins.jsonl` holds and the coverage assertion passes under `cargo test -p fsm-core --test expr_builtins`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
