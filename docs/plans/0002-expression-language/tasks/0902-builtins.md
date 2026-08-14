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

1. Author `crates/fsm-core/tests/fixtures/expr/builtins.jsonl` first: every builtin across its edge cases — rounding ties per mode, `dec` narrowing rejected as `expr/scale_narrow`, `round`-widens warning `expr/round_widens`, `div` by zero as `run/div_zero` and correct rounding at repeating expansions, `dur` overflow, wrong arity as `expr/arity`, non-literal scale as `expr/scale_not_literal`, bad mode word as `expr/mode_invalid`; plus `crates/fsm-core/tests/expr_builtins.rs` asserting every line.
2. Extend `crates/fsm-core/src/expr/typeck.rs` with the seven signatures per the architecture table, resolving `Arg::Word` mode/unit arguments and emitting the `expr/round_widens` warning.
3. Extend `crates/fsm-core/src/expr/eval.rs` with the seven evaluations delegating decimal work to `crate::decimal` and checking `dur` multiplication.

- **Done when:** every line of `builtins.jsonl` holds under `cargo test -p fsm-core --test expr_builtins`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
