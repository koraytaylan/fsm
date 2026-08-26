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
status: done
merged_as: ""
---
# Shadowing And Const Fold

The analyzer's remaining claims, each a bounded check transcribed from architecture: two in-source shadowing errors, the provably-dead ancestor-handler warning (its exact two-condition rule is given as a checklist), and the const-folded always-failing-creation check.

**Steps:**

1. Author the shadowing/const-fold fixtures under `crates/fsm-core/tests/fixtures/machines/analyze/` and `crates/fsm-core/tests/shadowing_golden.rs` first, encoding exactly the inventory under **Tests**.
2. Implement the in-source checks in `crates/fsm-core/src/analyze.rs`: within one `(from, on)` group in document order, a guardless or literal-`true` transition preceding later entries is `def/shadowed`; two entries with identical span-stripped guard structures are `def/duplicate_guard`.
3. Implement `def/ancestor_shadowed` exactly per the architecture rule: for an ancestor A's transition on event e, warn iff **for every leaf under A**, walking that leaf's chain strictly below A finds a transition on e that is guardless/`true` or structurally identical to A's guard — one loop over leaves, one loop up each chain, no cleverness.
4. Implement `def/create_always_fails`: evaluate the creation entry chain with the declared initial values (all initializers are literals, so this is ordinary evaluation, not symbolic); report only when it deterministically errors regardless of overrides — conservative by construction, per architecture.

**Tests:**

- In-source shadowing fixtures asserted by `shadowing_golden.rs`: a guardless transition preceding a guarded one on the same `(from, on)` → error `def/shadowed` whose hint names both transition indices; a literal-`true` guard preceding another entry → `def/shadowed`; two entries with structurally identical guards (differing only in whitespace/spans) → error `def/duplicate_guard`; the same two guards differing in one literal → no finding.
- Ancestor-shadowing, all four rule quadrants: (a) every leaf under A masked by guardless inner handlers → warning `def/ancestor_shadowed`; (b) every leaf masked, one via a structurally *identical* guard rather than guardless → warning (the second decidable condition); (c) one leaf keeps the ancestor live (no inner handler on that leaf's chain) → no warning; (d) an inner handler with a *different* guard on every leaf → no warning (masking is not provable).
- Legality baseline: a plain child-first override (child and ancestor both declare the event, child guarded differently) produces zero findings — override is the feature, not a smell.
- Const-fold fixtures: an entry block overflowing on declared inits regardless of overrides → error `def/create_always_fails` carrying the inner span; a machine whose entry only fails under *some* override values → no finding (conservative); a clean machine → no finding.
- `case_review` yields zero findings from this whole task.

- **Done when:** every shadowing and const-fold fixture yields exactly its expected finding set — including the live-leaf and different-guard variants yielding none — under `cargo test -p fsm-core --test shadowing_golden`, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
