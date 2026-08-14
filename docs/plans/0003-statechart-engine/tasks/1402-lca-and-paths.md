---
id: lca-and-paths
title: "Lca And Paths"
workstream: "0014"
kind: task
depends_on:
  - tree-tables
gated: false
touches:
  - crates/fsm-core/src/tree.rs
  - crates/fsm-core/tests/tree_paths.rs
status: planned
merged_as: ""
---
# Lca And Paths

Proper-LCA and the exit/entry path computations, transcribed from the architecture's walk-up pseudocode and pinned by the dom/exit/entry table for every `case_review` transition that architecture spells out row by row — implement until the table passes.

**Steps:**

1. Author `crates/fsm-core/tests/tree_paths.rs` first: the complete architecture table of `(source, target) → (dom, exit_set, entry_path)` for `case_review`, in exact order, plus hand-built-tree rows for an external self-transition, a target that is an ancestor of the source, and a cross-subtree target at depth 3.
2. Implement `proper_lca(&self, a, b) -> Option<u16>` in `crates/fsm-core/src/tree.rs` per the verbatim pseudocode: start from `parent(a)` and `parent(b)` (either may be the root, `None`), walk the deeper side up until depths match, then walk both up until equal — `None` means the implicit root.
3. Implement `exit_set(&self, leaf, dom) -> Vec<u16>` (the chain from the leaf up to, and excluding, `dom` — inner to outer) and `entry_path(&self, dom, target) -> Vec<u16>` (the chain from just below `dom` down to `target` — outer to inner, built by reversing the target's walk-up).

- **Done when:** every row of the dom/exit/entry table passes in order under `cargo test -p fsm-core --test tree_paths`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
