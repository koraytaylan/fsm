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

1. Author `crates/fsm-core/tests/tree_paths.rs` first, encoding exactly the inventory under **Tests**.
2. Implement `proper_lca(&self, a, b) -> Option<u16>` in `crates/fsm-core/src/tree.rs` per the verbatim pseudocode: start from `parent(a)` and `parent(b)` (either may be the root, `None`), walk the deeper side up until depths match, then walk both up until equal — `None` means the implicit root.
3. Implement `exit_set(&self, leaf, dom) -> Vec<u16>` (the chain from the leaf up to, and excluding, `dom` — inner to outer) and `entry_path(&self, dom, target) -> Vec<u16>` (the chain from just below `dom` down to `target` — outer to inner, built by reversing the target's walk-up).

**Tests:**

- The architecture's dom/exit/entry golden table for `case_review`, asserted row by row in order in `tree_paths.rs` (history targets resolve to their owner for dom purposes): `intake → in_review` from `intake` (dom root, exit `[intake]`, entry `[in_review]`); `docs_review → risk_review` (dom `in_review`, exit `[docs_review]`, entry `[risk_review]`); `risk_review → approved` (dom root, exit `[risk_review, in_review]`, entry `[approved]`); `in_review → rejected` on `withdraw` from active leaf `docs_review` (exit `[docs_review, in_review]`, entry `[rejected]`); `in_review → suspended` on `suspend` from `risk_review` (exit `[risk_review, in_review]`, entry `[suspended]`); `suspended → resume_review`⇒owner `in_review` (dom root, exit `[suspended]`, entry `[in_review]` — the descent extension is the next task's concern).
- Hand-built-tree rows: an external self-transition `X → X` (dom = `parent(X)`, exit `[X]`, entry `[X]`); a target that is an ancestor of the source at depth 3 (dom = the ancestor's parent; exit runs up through the ancestor; entry re-enters it); a cross-subtree transition between two depth-3 leaves (dom root, three exits, three entries, order asserted).
- `proper_lca` unit rows: two siblings → their parent; a node and its own parent → the grandparent (`None` when the parent is top-level); two top-level states → `None`; the deeper-side equalization pinned with an asymmetric-depth pair.
- Ordering invariants asserted across every row: `exit_set` is inner→outer (first element is the leaf), `entry_path` is outer→inner (last element is the target), and `dom` appears in neither.

- **Done when:** every row of the dom/exit/entry table and the hand-built LCA rows pass in order under `cargo test -p fsm-core --test tree_paths`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
