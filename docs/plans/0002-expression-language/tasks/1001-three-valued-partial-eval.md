---
id: three-valued-partial-eval
title: "Three Valued Partial Eval"
workstream: "0010"
kind: task
depends_on:
  - evaluator
gated: false
touches:
  - crates/fsm-core/src/expr/partial.rs
  - crates/fsm-core/tests/expr_partial.rs
  - crates/fsm-core/tests/fixtures/expr/partial.jsonl
status: done
merged_as: ""
---
# Three Valued Partial Eval

The enabled-events report needs to answer "could this guard pass?" for a live instance whose next event payload is unknown, so guards get a Kleene three-valued evaluation with event fields as Unknown and a deliberately conservative rule that concrete sub-evaluation errors yield Unknown.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/partial.jsonl` and `crates/fsm-core/tests/expr_partial.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `Truth { True, False, Unknown }` and `partial_eval_bool(e, ctx, scope, budget) -> Truth` in `crates/fsm-core/src/expr/partial.rs` per architecture, reusing `eval` for concrete subtrees under the shared budget. `scope` carries declared enums and event types; lazy `if` drops unreachable branches before payload dependence is decided.

**Tests:**

- Kleene truth tables pinned exhaustively inline in `partial.rs`: all nine `and` cells, all nine `or` cells, all three `not` cells — the 21-cell table asserted against the architecture semantics (`False and Unknown = False`, `True or Unknown = True`, `not Unknown = Unknown`, …), so any transcription slip is a named cell.
- Unknown propagation lines in `partial.jsonl`: `evt.amount > 100.00` → `unknown`; arithmetic containing an `evt` operand compared to a constant → `unknown`; `Unknown` flowing through a comparison, an `if` condition, and a nested call argument.
- Pruning lines: `ctx.flag and evt.x > 1` with `flag = false` → `false` (the Unknown side never matters); the `or`-mirror with `flag = true` → `true`.
- Concrete decision lines: a fully-`ctx` guard evaluates exactly (`ctx.limit > 0.00` with a binding → `true`, and with a zero binding → `false`), matching `eval`'s verdict on the same bindings.
- Conservative-error rule (fixture line *and* inline test with the SPEC.md-rationale comment): a ctx-only subtree that overflows → `unknown`, never a panic or error; budget exhaustion inside a concrete sub-evaluation follows the same rule → `unknown`.
- Budget sharing, inline: partial evaluation decrements the same `Budget` as `eval` (a hand-counted expression consumes the expected visits).

- **Done when:** every line of `partial.jsonl` holds and the 21-cell Kleene table passes under `cargo test -p fsm-core --test expr_partial` and `cargo test -p fsm-core partial`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
