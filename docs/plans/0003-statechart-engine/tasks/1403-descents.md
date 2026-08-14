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

1. Author `crates/fsm-core/tests/tree_descents.rs` first: initial descent from every `case_review` compound; history descent for `resume_review` with a deep binding to `risk_review`, with no binding (falls back to `in_review`'s initial chain landing on `docs_review`), and — on a hand-built tree with a shallow history — a shallow binding to a compound child followed by that child's initial chain.
2. Implement `initial_descent(&self, from) -> Vec<u16>` in `crates/fsm-core/src/tree.rs`: follow `initial_child` from `from` down to a leaf, collecting each entered node.
3. Implement `history_descent(&self, hist, binding: Option<&str>) -> Vec<u16>` per the architecture rule table: deep bound leaf → the path from the owner down to that leaf, entering each node; shallow bound child → that child, then its initial descent; no binding → the owner's initial descent.

- **Done when:** all descent cases — deep, shallow, and unbound — pass with exact ordered expectations under `cargo test -p fsm-core --test tree_descents`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
