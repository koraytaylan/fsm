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
status: planned
merged_as: ""
---
# Three Valued Partial Eval

The enabled-events report needs to answer "could this guard pass?" for a live instance whose next event payload is unknown, so guards get a Kleene three-valued evaluation with event fields as Unknown and a deliberately conservative rule that concrete sub-evaluation errors yield Unknown.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/expr/partial.jsonl` first: context bindings plus source mapped to `true`/`false`/`unknown`, covering the Kleene tables (`false and unknown = false`, `true or unknown = true`), Unknown propagation through comparisons and arithmetic, fully-concrete subtrees evaluating exactly, and the conservative-error rule; plus `crates/fsm-core/tests/expr_partial.rs` asserting every line.
2. Implement `Truth { True, False, Unknown }` and `partial_eval_bool(e, ctx, budget) -> Truth` in `crates/fsm-core/src/expr/partial.rs` per architecture, reusing `eval` for concrete subtrees under the shared budget.
3. Add inline unit tests pinning that a concrete sub-evaluation error (overflow in a ctx-only subtree) yields Unknown, with a comment pointing at the SPEC.md rationale.

- **Done when:** every line of `partial.jsonl` holds under `cargo test -p fsm-core --test expr_partial`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
