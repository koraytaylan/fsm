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
status: done
merged_as: ""
---
# Tree Tables

The hierarchy's index structures — names, parent, depth, children, initial-child, and node-kind tables built by one document-order walk — plus the chain iterator and the instance-state type; every later hierarchy operation reads these tables, so their expected contents for the reference machine are pinned in a table-driven test first.

**Steps:**

1. Author `crates/fsm-core/tests/tree_build.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `Tree { names, parent, depth, children, initial_child, kind, index }`, `NodeKind { Leaf, Compound, History(HistoryKind) }`, and `build(states) -> Tree` in `crates/fsm-core/src/tree.rs` per the architecture walk: a stack-based document-order traversal pushing `(node, parent_index)`, assigning indices as visited, depths as parent-depth+1 (top level = 1), and resolving `initial_child` by name lookup after all children are indexed.
3. Implement `chain(&self, leaf) -> impl Iterator<Item = u16>` (leaf → top level, innermost first; the implicit unnamed root is not a node and never appears).
4. Extend `crates/fsm-core/src/machine.rs` with `Status { Running, Completed, Cancelled }` and `InstanceState { status, leaf, ctx, history: BTreeMap<String, String>, pending: Vec<String> }`.

**Tests:**

- The architecture's expected-tables block for `case_review`, asserted row by row in `tree_build.rs` — every one of the eight nodes' index, name, parent, depth, kind, and initial-child exactly as printed (idx 0 `intake` top-level leaf … idx 1 `in_review` compound with `initial_child` = idx 3 `docs_review` … idx 2 `resume_review` `History(Deep)` at depth 2 … idx 7 `rejected`); the `index` name→idx map agrees.
- Chain order: `chain(docs_review)` = `[docs_review, in_review]`; `chain(risk_review)` = `[risk_review, in_review]`; `chain(intake)` = `[intake]` — exact sequences, innermost first.
- A hand-built depth-4 tree (compound → compound → compound → leaf): every node's depth and parent asserted; document-order indexing across siblings-with-children pinned (a later top-level sibling gets a higher index than the entire earlier subtree).
- Determinism: two `build` calls over the same spec produce equal tables.
- `machine.rs` types, inline: `InstanceState` constructs with an empty history and empty pending; `Status` equality/serialization form used by later tasks pinned (`running`/`completed`/`cancelled` snake_case names).

- **Done when:** the `case_review` table expectations and chain orders pass exactly under `cargo test -p fsm-core --test tree_build`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
