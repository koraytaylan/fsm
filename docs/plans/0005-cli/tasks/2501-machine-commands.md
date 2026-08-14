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
4. Write the inline test module encoding exactly the inventory under **Tests** (spec `run` functions over a temp store with capture buffers).

**Tests:**

- Inline in `machine.rs` — idempotent add: adding the reference spec twice prints the same machine id with `created: true` then `created: false`, both exit 0; with `--if-exists error` the second add exits 1 rendering the strictness error and the existing id.
- `ls`: after two adds (one machine, plus a second version of the same name), the listing carries both rows with id, name, and state/event counts; `--name-contains` filters to matching names only.
- Ambiguity surfaced: `machine show <bare name>` with two stored versions exits 1 rendering `req/machine_ambiguous` with both full ids listed; `show` with a unique 12-hex prefix succeeds.
- `show` fidelity: the rendered spec section is byte-identical to the stored canonical spec (the auditor's "what exactly is deployed" view), followed by the summary (initial chain, terminal leaves).
- `analyze`: a hand-built machine with one unenterable state renders the `def/unreachable_state` warning with severity and hint, the completeness matrix section with at least one `handled@<ancestor>` cell, and exit 0 (warnings do not fail the command); a machine with a `def/shadowed` error exits 1.

- **Done when:** inline machine-command tests prove idempotent add, strict mode, ambiguity listing, and analyze rendering, and `cargo test`, `cargo clippy --workspace -- -D warnings`, and `cargo fmt --check` succeed.
