---
id: reachability-and-completeness
title: "Reachability And Completeness"
workstream: "0013"
kind: task
depends_on:
  - expression-binding
  - tree-tables
gated: false
touches:
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/tests/analyze_golden.rs
  - "crates/fsm-core/tests/fixtures/machines/analyze/**"
status: planned
merged_as: ""
---
# Reachability And Completeness

The analyzer's first two claims: an exact enterable-set reachability walk (backed by the lemma that history never extends the reachable set) and the leaf-by-event completeness matrix with chain-level annotations — a plain worklist walk and a plain double loop once the tree tables exist.

**Steps:**

1. Author fixtures first under `crates/fsm-core/tests/fixtures/machines/analyze/`: a machine with an unenterable state (`def/unreachable_state`) and a machine whose history target is only enterable via the owner's-initial-chain modeling; plus `crates/fsm-core/tests/analyze_golden.rs` asserting per-fixture finding sets and the full completeness matrix for `case_review` (every cell spelled out).
2. Implement `Findings`, `Finding { severity, code, message, path, span, hint }`, and the enterable-set walk in `crates/fsm-core/src/analyze.rs` per architecture: seed with the creation entry chain, then repeatedly take any transition whose source is enterable and add its full possible entry set (target path, initial descents, history modeled as the owner's initial chain), until fixed point; unenterable states warn.
3. Implement the completeness matrix: rows = leaves, columns = declared events; each cell `handled@<source_state>` (the innermost chain level declaring a transition for that event) or `unhandled(<policy>)`.
4. Document the reachability lemma as a comment block with its two-sentence proof from architecture.

- **Done when:** the reachability fixtures yield exactly their expected findings and the `case_review` completeness matrix matches its golden cell-for-cell under `cargo test -p fsm-core --test analyze_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
