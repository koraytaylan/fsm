---
id: static-analysis
title: "Static Analysis"
workstream: "0013"
kind: task
depends_on:
  - expression-binding
  - configuration-and-lca
gated: false
touches:
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/tests/analyze_golden.rs
  - "crates/fsm-core/tests/fixtures/machines/analyze/**"
status: planned
merged_as: ""
---
# Static Analysis

The analyzer makes exactly four pinned claims — exact enterable-set reachability (via the lemma that history never extends the reachable set), a leaf-by-event completeness matrix with chain-level annotations, in-source shadowing errors, and provably-dead ancestor-handler warnings — and nothing more; it depends on the tree module for chain and descent computation.

**Steps:**

1. Author `crates/fsm-core/tests/fixtures/machines/analyze/` first: machines exhibiting an unenterable state (`def/unreachable_state`), a guardless transition shadowing later ones (`def/shadowed`), two identical normalized guards (`def/duplicate_guard`), an ancestor handler dead for all its leaves (`def/ancestor_shadowed`) next to a variant where one leaf keeps it live (no warning), a history target whose owner is only reachable via history modeling, and an entry chain that provably fails with declared inits (`def/create_always_fails`); plus `crates/fsm-core/tests/analyze_golden.rs` asserting per-fixture finding sets and the full completeness matrix for `case_review`.
2. Implement `Findings`, `Finding { severity, code, message, path, span, hint }`, and `analyze(m, t)` in `crates/fsm-core/src/analyze.rs` per architecture: enterable-set seeded from the creation chain with history targets modeled as the owner's initial chain, the leaf×event matrix with `handled@<source_state>` cells, both in-source shadowing errors, the two decidable ancestor-shadowing conditions, and the const-folded `def/create_always_fails` check.
3. Document the reachability lemma as a comment block with its two-sentence proof.

- **Done when:** every analyze fixture yields exactly its expected finding set and the `case_review` completeness matrix matches its golden under `cargo test -p fsm-core --test analyze_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
