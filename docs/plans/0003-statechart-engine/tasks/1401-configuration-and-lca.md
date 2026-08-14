---
id: configuration-and-lca
title: "Configuration And Lca"
workstream: "0014"
kind: task
depends_on:
  - expression-binding
gated: false
touches:
  - crates/fsm-core/src/tree.rs
  - crates/fsm-core/src/machine.rs
  - crates/fsm-core/tests/tree_ops.rs
status: planned
merged_as: ""
---
# Configuration And Lca

All hierarchy computation — parent/depth tables, proper LCA, chain iteration, exit/entry sets, initial and history descent — lives in one isolated module so it is testable alone, alongside the instance-state type whose history bindings are hashed state.

**Steps:**

1. Implement `Tree` (`names`, `parent`, `depth`, `children`, `initial_child`, `kind`, `index`), `build(states)`, `chain`, `proper_lca`, `exit_set`, `entry_path`, `initial_descent`, and `history_descent` in `crates/fsm-core/src/tree.rs` per architecture, with the implicit unnamed root represented as `None`.
2. Extend `crates/fsm-core/src/machine.rs` with `Status { Running, Completed, Cancelled }` and `InstanceState { status, leaf, ctx, history: BTreeMap<String, String>, pending: Vec<String> }`.
3. Add `crates/fsm-core/tests/tree_ops.rs`: table-driven cases over the `case_review` tree and hand-built deeper trees asserting exact chain order, LCA results including root, exit/entry set contents and ordering for self/ancestor/descendant/cross-subtree targets, initial descent, and all three history-descent branches (deep bound, shallow bound, unbound).

- **Done when:** the tree-operation table tests pass with exact ordered expectations under `cargo test -p fsm-core --test tree_ops`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
