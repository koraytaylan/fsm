---
id: creation-entry-chain
title: "Creation Entry Chain"
workstream: "0014"
kind: task
depends_on:
  - apply-pipeline
gated: false
touches:
  - crates/fsm-core/src/step.rs
  - crates/fsm-core/tests/create_chain.rs
status: planned
merged_as: ""
---
# Creation Entry Chain

Instance creation runs the root's initial descent through the same block pipeline as transitions — entry blocks, effects, then all invariants — and its failure is a pure, unjournaled outcome, because with no prior instance state the result is a function of the definition and the overrides alone.

**Steps:**

1. Implement `create(m, t, overrides) -> Result<Applied, Rejection>` in `crates/fsm-core/src/step.rs`: validate overrides against declared context types (same value rules as event fields), form ctx₀ from declared inits plus overrides, enter the root's initial descent outer-to-inner reusing the pipeline's block-application helper for each entry block, collect effects under the same global `k` counter, evaluate all invariants on the final context; `history` starts empty (creation exits nothing, so no captures).
2. Make failure `run/create_failed`, wrapping the inner block or invariant error with its full trace, and add the doc comment stating the shell never journals it and no id or seq is consumed (pure function of definition and overrides).
3. Add `crates/fsm-core/tests/create_chain.rs`: over `case_review`, creation lands on `docs_review` with `visits = 1` and the `notify` effect collected; an invalid override (wrong type, over-precision decimal) rejects before any evaluation; a hand-built machine whose entry block overflows under given overrides yields `run/create_failed` with the completed-blocks trace preserved; a rejected creation leaves nothing observable (call twice, second attempt sees identical inputs and produces an identical result).

- **Done when:** the creation tests pass, including the failure-purity double-call case, under `cargo test -p fsm-core --test create_chain`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
