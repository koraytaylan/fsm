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
status: done
merged_as: ""
---
# Creation Entry Chain

Instance creation runs the root's initial descent through the same block pipeline as transitions — entry blocks, effects, then all invariants — and its failure is a pure, unjournaled outcome, because with no prior instance state the result is a function of the definition and the overrides alone.

**Steps:**

1. Author `crates/fsm-core/tests/create_chain.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `create(m, t, overrides) -> Result<Applied, Rejection>` in `crates/fsm-core/src/step.rs`: validate overrides against declared context types (same value rules as event fields), form ctx₀ from declared inits plus overrides, enter the root's initial descent outer-to-inner reusing the pipeline's block-application helper for each entry block, collect effects under the same global `k` counter, evaluate all invariants on the final context; `history` starts empty (creation exits nothing, so no captures).
3. Make failure `run/create_failed`, wrapping the inner block or invariant error with its full trace, and add the doc comment stating the shell never journals it and no id or seq is consumed (pure function of definition and overrides).

**Tests:**

- Entry-chain order (hand-built machine whose root `initial` names a compound): creation enters `[compound, its initial leaf]` outer→inner, runs both entry blocks in that order (counters prove it), collects an entry-block effect with `k = 0`, and `history` is empty.
- `case_review` creation exact expectations (its `initial` is the top-level leaf `intake`, so no entry blocks run): leaf `intake`, `visits = 0`, no effects, empty history, `status = Running`; two `create` calls return structurally equal values (pure).
- Overrides: a valid override (`score = 5`) reflected in the created context; an unknown variable name → rejection with the same code family as event fields (`req/field_unknown`); a wrong-typed override → `req/field_type`; an over-precision decimal override → `req/field_scale`; every override rejection occurs before any entry block runs (no trace blocks present).
- Failure purity: a hand-built machine whose compound entry block overflows under a given override → `run/create_failed` wrapping the inner `run/overflow` with the completed-blocks trace preserved; calling `create` twice with identical inputs returns structurally identical rejections (pure function — the double-call case).
- Enforce invariant at creation: a machine whose invariant fails on the declared inits → `run/create_failed`; the monitor-mode variant creates successfully with the flag in `monitor_flags`.
- Terminal guard: creation never lands terminal (that spec is `def/initial_terminal`, rejected at validation — asserted here as a compile-time expectation, not a runtime branch).

- **Done when:** the creation tests pass — the entry-chain order case, all override validation codes, the failure-purity double-call case, and both invariant modes — under `cargo test -p fsm-core --test create_chain`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
