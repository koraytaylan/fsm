---
id: descents
title: "Descents"
workstream: "0014"
kind: task
depends_on:
  - lca-and-paths
gated: false
touches:
  - crates/fsm-core/src/tree.rs
  - crates/fsm-core/tests/tree_descents.rs
status: planned
merged_as: ""
---
# Descents

The two ways an entry path extends downward — initial-child descent into a compound, and history descent per the deep/shallow/unbound rule table — each a short loop over the tables landed by the previous tasks.

**Steps:**

1. Author `crates/fsm-core/tests/tree_descents.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `initial_descent(&self, from) -> Vec<u16>` in `crates/fsm-core/src/tree.rs`: follow `initial_child` from `from` down to a leaf, collecting each entered node.
3. Implement `history_descent(&self, hist, binding: Option<&str>) -> Vec<u16>` per the architecture rule table: deep bound leaf → the path from the owner down to that leaf, entering each node; shallow bound child → that child, then its initial descent; no binding → the owner's initial descent.

**Tests:**

- Initial descent: from `in_review` → `[docs_review]` (one hop to the initial leaf); from a leaf → `[]` (nothing to descend); on a hand-built depth-4 tree, from the top compound → the full three-node chain of initial children, order outer→inner asserted.
- Deep history: `resume_review` bound to `risk_review` → `[risk_review]`; on the hand-built tree, a deep history bound to a depth-4 leaf → the full path from just below the owner down to that leaf, entering each intermediate compound, order asserted.
- Shallow history (hand-built tree with a shallow pseudostate): bound to a compound direct child → `[that child]` followed by that child's initial descent; bound to a leaf direct child → `[that child]` alone.
- Unbound: `resume_review` with no binding → `[docs_review]` (`in_review`'s initial descent — the fallback rule); the hand-built shallow case unbound → the owner's initial descent.
- Consistency invariant asserted across every case: the returned sequence never contains the owner itself (the owner is entered by `entry_path`; descent extends strictly below it), and its last element is always a leaf.

- **Done when:** all descent cases — deep, shallow, and unbound, on both trees — pass with exact ordered expectations under `cargo test -p fsm-core --test tree_descents`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
