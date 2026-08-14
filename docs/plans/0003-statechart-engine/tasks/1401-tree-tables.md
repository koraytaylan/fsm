---
id: tree-tables
title: "Tree Tables"
workstream: "0014"
kind: task
depends_on:
  - expression-binding
gated: false
touches:
  - crates/fsm-core/src/tree.rs
  - crates/fsm-core/src/machine.rs
  - crates/fsm-core/tests/tree_build.rs
status: planned
merged_as: ""
---
# Tree Tables

The hierarchy's index structures — names, parent, depth, children, initial-child, and node-kind tables built by one document-order walk — plus the chain iterator and the instance-state type; every later hierarchy operation reads these tables, so their expected contents for the reference machine are pinned in a table-driven test first.

**Steps:**

1. Author `crates/fsm-core/tests/tree_build.rs` first: table-driven expectations for the `case_review` tree — every node's index, parent, depth, kind, and initial child exactly as listed in architecture's expected-tables block — plus chain order from each leaf, and build-time cases for a deeper hand-built tree.
2. Implement `Tree { names, parent, depth, children, initial_child, kind, index }`, `NodeKind { Leaf, Compound, History(HistoryKind) }`, and `build(states) -> Tree` in `crates/fsm-core/src/tree.rs` per the architecture walk: a stack-based document-order traversal pushing `(node, parent_index)`, assigning indices as visited, depths as parent-depth+1 (top level = 1), and resolving `initial_child` by name lookup after all children are indexed.
3. Implement `chain(&self, leaf) -> impl Iterator<Item = u16>` (leaf → top level, innermost first; the implicit unnamed root is not a node and never appears).
4. Extend `crates/fsm-core/src/machine.rs` with `Status { Running, Completed, Cancelled }` and `InstanceState { status, leaf, ctx, history: BTreeMap<String, String>, pending: Vec<String> }`.

- **Done when:** the `case_review` table expectations and chain orders pass exactly under `cargo test -p fsm-core --test tree_build`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
