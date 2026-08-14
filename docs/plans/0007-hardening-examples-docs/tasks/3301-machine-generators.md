---
id: machine-generators
title: "Machine Generators"
workstream: "0033"
kind: task
depends_on: []
gated: false
touches:
  - crates/fsm-core/tests/proputil.rs
status: planned
merged_as: ""
---
# Machine Generators

Property suites need a stream of well-formed random machines — bounded trees with history, typed events, pooled guards and actions — from a seeded generator whose failures are reproducible by printing the seed.

**Steps:**

1. Implement `crates/fsm-core/tests/proputil.rs` per architecture: `Gen` (xorshift64*), `gen_machine(g, size)` producing definitions that pass spec validation by construction (tree depth ≤ 4, ≤ 10 nodes, optional history pseudostate, 1–3 typed events, pooled guards/sets/emits/invariants), and `gen_events(g, machine, len)` producing type-correct payloads with a tagged low-probability share of deliberately wrong ones.
2. Document the consumption pattern at the top of the file (`#[path = "proputil.rs"] mod proputil;` — the file also compiles as its own empty test target, which is harmless).
3. Add the in-file `generator_sanity` test encoding exactly the inventory under **Tests**.

**Tests:**

- `generator_sanity` over 100 fixed seeds: every generated machine passes full `fsm_core` spec validation (structure and expression binding — the same path `machine_create` uses); every generated event name is declared by its machine.
- Honest tagging both ways: untagged generated payloads pass event validation; every payload tagged deliberately-wrong indeed fails it (the tag never lies in either direction).
- Distribution self-checks over the 100-seed corpus (loose bounds, non-flaky by fixed seeding): at least 30 machines contain a compound state, at least 15 contain a history pseudostate, at least 50 contain a guarded transition, and at least 20 declare an invariant — a degenerate generator fails loudly instead of silently gutting the downstream suites.
- Determinism: the same seed twice yields byte-identical canonical machine bytes and an identical event list.
- Reproducibility plumbing: every assertion path includes the offending seed in its panic message.

- **Done when:** `cargo test -p fsm-core --test proputil` passes `generator_sanity` — validity, honest tagging, distribution bounds, and determinism — over 100 seeds with printed-seed reproduction on failure, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
