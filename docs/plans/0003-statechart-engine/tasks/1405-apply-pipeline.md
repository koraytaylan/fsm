---
id: apply-pipeline
title: "Apply Pipeline"
workstream: "0014"
kind: task
depends_on:
  - transition-selection
  - lca-and-paths
  - descents
gated: false
touches:
  - crates/fsm-core/src/step.rs
  - crates/fsm-core/tests/apply_pipeline.rs
status: done
merged_as: ""
---
# Apply Pipeline

The atomic heart of the engine, assembled from already-landed, already-tested pieces (selection, LCA paths, descents): exit blocks inner-to-outer, the transition block (the only one that sees the event), entry blocks outer-to-inner — each block snapshot-internal, context threading block to block, effects under one global counter, history captured from the pre-transition configuration, invariants once on the final context, and any failure anywhere discarding everything.

**Steps:**

1. Author `crates/fsm-core/tests/apply_pipeline.rs` first, encoding exactly the inventory under **Tests**.
2. Complete `step()` in `crates/fsm-core/src/step.rs` per the architecture decision procedure: status gate (`run/instance_completed`, `run/instance_cancelled`), internal-versus-external resolution (absent `to` is internal; external self-transitions exit and re-enter through `dom = parent(from)`), history-target resolution via `history_descent` with the instance binding, the three-stage block pipeline with per-block snapshot application, `run/action_error` naming the failing block with computed-but-discarded values preserved in the trace, history capture (deep = pre leaf, shallow = pre direct child) atomic with the transition, all-invariants evaluation with `run/invariant` on enforce failures and `monitor_flags` otherwise, and `Completed` status on a terminal leaf.

**Tests:**

- The architecture walkthrough end-to-end on `case_review`: `suspend` from `risk_review` → `Applied` with `exited = [risk_review, in_review]`, `entered = [suspended]`, `ctx.notes = 0` (the exit block ran), `history_after = {in_review: risk_review}` (captured from the **pre**-transition leaf), no effects; then `resume` → `entered = [in_review, risk_review]`, `visits` incremented and the `notify` effect emitted (entry blocks re-ran), `score = 0` (the restored leaf's entry ran), configuration restored with `ctx` otherwise persisted.
- Block ordering and threading (hand-built): the staging idiom — a transition set `ctx.x = evt.y` consumed by an entry-block set — yields the entry's read of the staged value; an exit block cannot see the transition block's writes (asserted by ordering: exit runs first on the pre context).
- Per-block snapshot semantics (hand-built): two sets in one block both read the previous block's context (swapping two variables in one block works: `a = b; b = a` exchanges them).
- Effect ordering: emits placed in an exit block, the transition block, and an entry block collect with `k = 0, 1, 2` in pipeline order.
- Atomicity at every failure point, each case asserting the returned rejection code *and* that context, leaf, history bindings, and effects are all bit-identical to the pre-state: a guard error (`run/guard_error`), an exit-block overflow, a transition-block overflow, an entry-block overflow (all `run/action_error` naming their block as `exit(state)` / `transition` / `entry(state)`), and an enforce-invariant failure (`run/invariant`) — the invariant case on a transition that *would* have captured history, proving bindings stay untouched.
- Discarded-value preservation: the entry-block-failure rejection's trace contains the completed exit block's computed values, marked discarded.
- Invariants: an enforce failure lists *every* failing invariant (two failing at once → both named); a monitor-mode failure applies the event and records the flag in `monitor_flags`.
- Internal vs external self: `note_added` (internal) leaves `visits`, leaf, and history untouched while applying its set; a hand-built external self-transition `X → X` re-runs `X`'s entry block (counter increments) with `exited = entered = [X]`.
- Status: `scored` with a passing guard → leaf `approved`, `status_after = Completed`; a further event → `run/instance_completed`; against a `Cancelled` instance → `run/instance_cancelled`.

- **Done when:** the pipeline tests pass — the walkthrough, ordering/threading, effect `k` order, atomicity at all five failure points with discarded-value preservation, monitor collection, internal-versus-external, and both status gates — under `cargo test -p fsm-core --test apply_pipeline`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
