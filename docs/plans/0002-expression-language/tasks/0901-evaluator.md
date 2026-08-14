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

1. Author `crates/fsm-core/tests/fixtures/expr/eval.jsonl` and `crates/fsm-core/tests/expr_eval.rs` first, encoding exactly the inventory under **Tests** (lines carry bindings plus source mapped to a canonical value or a `run/*` error).
2. Implement `Val` (with `canonical_string`), `Bindings`, `Budget`, `TraceNode`/`TraceOutcome`/`trace_to_value`, and `eval(e, bindings, budget, trace) -> (Result<Val, ExprError>, Option<TraceNode>)` in `crates/fsm-core/src/expr/eval.rs` per architecture: every node visit decrements the budget (`internal/budget` on exhaustion), all integer-class arithmetic is `checked_*`, decimal arithmetic delegates to `crate::decimal`, and skipped subtrees are recorded as `Skipped`.

**Tests:**

- Value lines in `eval.jsonl`: integer arithmetic (`2 + 3 * 4` → `14`); exact decimal alignment (`1.5 + 0.25` → `1.75`); cross-scale decimal comparison (`1.5 == 1.50` → `true`); `Ts + Dur` and `Ts − Ts` over bound timestamps; enum equality against an `EnumLit`; string equality.
- Short-circuit proven observably (the skipped operand *would* error): `false and (9223372036854775807 + 1 > 0)` → `false` with no error and the right subtree traced `Skipped`; the `true or …` mirror; `if false then <overflowing subtree> else 1` → `1` with the then-branch `Skipped`.
- Checked-arithmetic errors with operand strings asserted in `details`: `-(−9223372036854775808)`-shaped negation → `run/overflow`; i64 addition overflow → `run/overflow`; a decimal alignment overflow at the mantissa bound → `run/overflow`.
- Trace-shape golden for one representative expression (`ctx.flag and evt.amount > ctx.limit` with bindings): the full `TraceNode` tree rendered via `trace_to_value` and byte-compared — values at each node, spans present; and the same call with `trace: false` returns `None` for the trace.
- Budget mechanics, inline in `eval.rs`: an expression of hand-counted N nodes evaluates with `Budget::new(N)` and fails with `internal/budget` under `Budget::new(N−1)`; the budget is shared — two sequential evaluations against one budget consume cumulatively.
- `canonical_string`, inline: pinned output for every `Val` variant (`Dec` via `Dec::format`, `Ts`/`Dur` as decimal integer strings, `Bool` as `true`/`false`, `Enum` as `Risk.low`).

- **Done when:** every line of `eval.jsonl` holds, the trace golden matches byte-for-byte, and the inline budget and `canonical_string` tests pass under `cargo test -p fsm-core --test expr_eval` and `cargo test -p fsm-core eval`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
