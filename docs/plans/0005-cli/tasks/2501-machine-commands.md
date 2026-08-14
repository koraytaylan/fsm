---
id: machine-commands
title: "Machine Commands"
workstream: "0025"
kind: task
depends_on:
  - output-frame
gated: false
touches:
  - crates/fsm-cli/src/cli/machine.rs
status: planned
merged_as: ""
---
# Machine Commands

Definition authoring and inspection over the store: idempotent content-addressed add, listing, the stored canonical spec, and the full static analysis — findings, enterable-set reachability, the leaf-by-event completeness matrix, and ancestor-shadowing warnings.

**Steps:**

1. Fill `crates/fsm-cli/src/cli/machine.rs::SPECS` with `machine add <spec.json|-> [--if-exists return|error]` over `Store::define_machine` — default `return` (identical spec succeeds with `created: false`), printing machine id, created flag, and warnings.
2. Add `machine ls [--name-contains S]` (id, name, version, state/event counts, instance counts) and `machine show <machine>` (stored canonical spec plus summary) over `Store::resolve_machine`.
3. Add `machine analyze <machine>` rendering findings with severities, reachability, the completeness matrix with `handled@<level>` annotations, and ancestor-shadowing warnings.
4. Add inline unit tests over a temp store: add-twice idempotency (`created: false`), `--if-exists error` strictness, ambiguity error listing versions, and analyze output for a machine with one unenterable state.

- **Done when:** inline machine-command tests prove idempotent add, strict mode, ambiguity listing, and analyze rendering, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
