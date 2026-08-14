---
id: apply-pipeline
title: "Apply Pipeline"
workstream: "0014"
kind: task
depends_on:
  - transition-selection
gated: false
touches:
  - crates/fsm-core/src/step.rs
  - crates/fsm-core/tests/apply_pipeline.rs
status: planned
merged_as: ""
---
# Apply Pipeline

The atomic heart of the engine: exit blocks inner-to-outer, the transition block (the only one that sees the event), entry blocks outer-to-inner — each block snapshot-internal, context threading block to block, effects under one global counter, history captured from the pre-transition configuration, invariants once on the final context, and any failure anywhere discarding everything; plus the creation entry chain whose failure is a pure, unjournaled outcome.

**Steps:**

1. Complete `step()` in `crates/fsm-core/src/step.rs` per the architecture decision procedure: status gate (`run/instance_completed`, `run/instance_cancelled`), internal-versus-external resolution (absent `to` is internal; external self-transitions exit and re-enter through `dom = parent(from)`), history-target resolution via the instance binding, the three-stage block pipeline with per-block snapshot application, `run/action_error` naming the failing block with computed-but-discarded values preserved in the trace, history capture (deep = pre leaf, shallow = pre direct child) atomic with the transition, all-invariants evaluation with `run/invariant` on enforce failures and `monitor_flags` otherwise, and `Completed` status on a terminal leaf.
2. Implement `create(m, t, overrides) -> Result<Applied, Rejection>`: declared inits plus validated overrides, root initial descent running entry blocks per the pipeline, effects, invariants; failure is `run/create_failed` wrapping the inner error and trace, with a doc comment stating the shell never journals it (pure function of definition and overrides).
3. Add `crates/fsm-core/tests/apply_pipeline.rs`: over `case_review` and hand-built machines, assert the exact block ordering and context threading (the staging idiom: a transition set feeding an entry-block read), effect `k` ordering across blocks, atomic rejection leaving context/leaf/history/effects untouched at every failure point (guard error, each block, enforce invariant), monitor flags collecting without blocking, internal transitions leaving `visits` untouched while external self-transitions re-run entry blocks, and creation running the full entry chain.

- **Done when:** the pipeline tests pass, including atomicity at every failure point and the internal-versus-external observable difference, under `cargo test -p fsm-core --test apply_pipeline`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
