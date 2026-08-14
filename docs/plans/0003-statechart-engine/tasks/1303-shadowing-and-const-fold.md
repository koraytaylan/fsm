---
id: shadowing-and-const-fold
title: "Shadowing And Const Fold"
workstream: "0013"
kind: task
depends_on:
  - reachability-and-completeness
gated: false
touches:
  - crates/fsm-core/src/analyze.rs
  - crates/fsm-core/tests/shadowing_golden.rs
  - "crates/fsm-core/tests/fixtures/machines/analyze/**"
status: planned
merged_as: ""
---
# Shadowing And Const Fold

The analyzer's remaining claims, each a bounded check transcribed from architecture: two in-source shadowing errors, the provably-dead ancestor-handler warning (its exact two-condition rule is given as a checklist), and the const-folded always-failing-creation check.

**Steps:**

1. Author fixtures first under `crates/fsm-core/tests/fixtures/machines/analyze/`: a guardless transition shadowing later ones (`def/shadowed`), two identical normalized guards (`def/duplicate_guard`), an ancestor handler dead for all its leaves (`def/ancestor_shadowed`) next to a variant where one leaf keeps it live (no warning), and an entry chain that provably fails with declared inits (`def/create_always_fails`); plus `crates/fsm-core/tests/shadowing_golden.rs` asserting per-fixture finding sets.
2. Implement the in-source checks in `crates/fsm-core/src/analyze.rs`: within one `(from, on)` group in document order, a guardless or literal-`true` transition preceding later entries is `def/shadowed`; two entries with identical span-stripped guard structures are `def/duplicate_guard`.
3. Implement `def/ancestor_shadowed` exactly per the architecture rule: for an ancestor A's transition on event e, warn iff **for every leaf under A**, walking that leaf's chain strictly below A finds a transition on e that is guardless/`true` or structurally identical to A's guard — one loop over leaves, one loop up each chain, no cleverness.
4. Implement `def/create_always_fails`: evaluate the creation entry chain with the declared initial values (all initializers are literals, so this is ordinary evaluation, not symbolic); report only when it deterministically errors regardless of overrides — conservative by construction, per architecture.

- **Done when:** every shadowing and const-fold fixture yields exactly its expected finding set, including the live-leaf variant yielding none, under `cargo test -p fsm-core --test shadowing_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
