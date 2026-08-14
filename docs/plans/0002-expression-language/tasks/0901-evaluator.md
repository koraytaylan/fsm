---
id: evaluator
title: "Evaluator"
workstream: "0009"
kind: task
depends_on:
  - typechecker
gated: false
touches:
  - crates/fsm-core/src/expr/eval.rs
  - crates/fsm-core/tests/expr_eval.rs
  - crates/fsm-core/tests/fixtures/expr/eval.jsonl
status: planned
merged_as: ""
---
# Evaluator

Evaluation is strict, checked, budgeted, and traced: left-to-right with short-circuit `and`/`or` and lazy `if`, checked integer and exact decimal arithmetic, a step budget shared across one event's evaluations, and a full sub-expression trace recording values, skipped subtrees, and error operands.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/eval.jsonl` first: bindings plus source mapped to an expected canonical value or a `run/*` error, including short-circuit proofs where the skipped operand would error, `-(i64::MIN)` and decimal-alignment overflow cases, and trace-shape expectations for one representative expression; plus `crates/fsm-core/tests/expr_eval.rs` asserting every line.
2. Implement `Val` (with `canonical_string`), `Bindings`, `Budget`, `TraceNode`/`TraceOutcome`/`trace_to_value`, and `eval(e, bindings, budget, trace) -> (Result<Val, ExprError>, Option<TraceNode>)` in `crates/fsm-core/src/expr/eval.rs` per architecture: every node visit decrements the budget (`internal/budget` on exhaustion), all integer-class arithmetic is `checked_*`, decimal arithmetic delegates to `crate::decimal`, and skipped subtrees are recorded as `Skipped`.
3. Add inline unit tests for the budget mechanics and for `canonical_string` across all `Val` variants.

- **Done when:** every line of `eval.jsonl` holds under `cargo test -p fsm-core --test expr_eval`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
